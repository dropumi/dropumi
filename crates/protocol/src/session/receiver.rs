// SPDX-License-Identifier: MPL-2.0
//! The responder's (receiver's) transfer machine.
//!
//! ```text
//! AwaitOffer → CollectingOffer → AwaitDecision
//!   → ExpectFileStart(next): FileStart(next, 0) | FileAbort(next) | Complete (iff next == count)
//!   → ReceivingFile(i, remaining): DATA(1..=remaining) | FileEnd(i) (iff remaining == 0) | FileAbort(i)
//!   → Completing → Done
//! any state: Cancel | Error → Closed
//! ```

use dropumi_domain::limits::MAX_FILE_SIZE;
use dropumi_domain::transfer::{
    CancelReason, FileEntry, RejectReason, TransferId, TransferRequest,
};

use super::IgnorableBudget;
use crate::error::ProtocolError;
use crate::frame::Frame;
use crate::limits::MAX_OFFER_METADATA;
use crate::message::{AckStatus, Decoded, ErrorCode, FileHash, Message, Role, decode};

#[derive(Debug)]
enum State {
    AwaitOffer,
    CollectingOffer {
        transfer_id: TransferId,
        file_count: u32,
        total_size: u64,
        files: Vec<FileEntry>,
        metadata: usize,
    },
    AwaitDecision,
    ExpectFileStart {
        next: u32,
    },
    ReceivingFile {
        index: u32,
        remaining: u64,
    },
    Completing,
    Done,
    Closed,
}

/// What an inbound frame means for the receiver.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReceiverEvent<'a> {
    /// Part of the offer arrived; nothing to do yet.
    Pending,
    /// The complete, validated offer.
    OfferReady(TransferRequest),
    /// File `index` starts; `size` bytes follow.
    FileStarted {
        /// File index.
        index: u32,
        /// Declared size.
        size: u64,
    },
    /// File content to write.
    Data(&'a [u8]),
    /// All bytes of `index` arrived; `hash_ok` says whether they match.
    FileFinished {
        /// File index.
        index: u32,
        /// Whether the BLAKE3 hash matched.
        hash_ok: bool,
        /// BLAKE3 digest of the bytes actually received.
        digest: [u8; 32],
    },
    /// The sender abandoned file `index`.
    FileAborted {
        /// File index.
        index: u32,
    },
    /// The sender has nothing more to send.
    CompleteRequested,
    /// The sender cancelled (terminal).
    PeerCancel(CancelReason),
    /// The sender reported an error (terminal).
    PeerError(ErrorCode),
    /// An ignorable message, or a late message after completion.
    Ignored,
}

/// Validates one incoming transfer.
#[derive(Debug)]
pub struct ReceiverMachine {
    state: State,
    request: Option<TransferRequest>,
    hasher: blake3::Hasher,
    pending_ack: Option<u32>,
    acks: Vec<Option<AckStatus>>,
    ignorable: IgnorableBudget,
}

impl Default for ReceiverMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiverMachine {
    /// A machine waiting for an offer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: State::AwaitOffer,
            request: None,
            hasher: blake3::Hasher::new(),
            pending_ack: None,
            acks: Vec::new(),
            ignorable: IgnorableBudget::default(),
        }
    }

    /// The validated offer, once complete.
    #[must_use]
    pub const fn request(&self) -> Option<&TransferRequest> {
        self.request.as_ref()
    }

    /// Handles one decrypted frame from the sender.
    ///
    /// # Errors
    /// Any [`ProtocolError`]; the session must then end with an `Error`.
    pub fn on_frame<'a>(&mut self, frame: Frame<'a>) -> Result<ReceiverEvent<'a>, ProtocolError> {
        match frame {
            Frame::Data(bytes) => self.on_data(bytes),
            Frame::Control(body) => match decode(body, Role::Initiator)? {
                Decoded::Ignored(_) => {
                    self.ignorable.ignored()?;
                    Ok(ReceiverEvent::Ignored)
                }
                Decoded::Message(m) => {
                    self.ignorable.reset();
                    self.on_message(m, body.len())
                }
            },
        }
    }

    fn on_data<'a>(&mut self, bytes: &'a [u8]) -> Result<ReceiverEvent<'a>, ProtocolError> {
        let State::ReceivingFile { index, remaining } = self.state else {
            return Err(ProtocolError::Violation("data outside a file"));
        };
        let len = u64::try_from(bytes.len()).map_err(|_| ProtocolError::LimitExceeded("data"))?;
        if bytes.is_empty() {
            return Err(ProtocolError::Malformed("empty data frame"));
        }
        let remaining = remaining
            .checked_sub(len)
            .ok_or(ProtocolError::Violation("data beyond declared size"))?;
        self.hasher.update(bytes);
        self.state = State::ReceivingFile { index, remaining };
        // Any accepted DATA frame ends a run of ignorable messages.
        self.ignorable.reset();
        Ok(ReceiverEvent::Data(bytes))
    }

    fn on_message<'a>(
        &mut self,
        msg: Message,
        encoded_len: usize,
    ) -> Result<ReceiverEvent<'a>, ProtocolError> {
        match msg {
            Message::Cancel { reason } => {
                if matches!(self.state, State::Done | State::Closed) {
                    return Ok(ReceiverEvent::Ignored);
                }
                self.state = State::Closed;
                return Ok(ReceiverEvent::PeerCancel(reason));
            }
            Message::Error { code, .. } => {
                if matches!(self.state, State::Done | State::Closed) {
                    return Ok(ReceiverEvent::Ignored);
                }
                self.state = State::Closed;
                return Ok(ReceiverEvent::PeerError(code));
            }
            _ => {}
        }
        if self.pending_ack.is_some() {
            return Err(ProtocolError::Violation(
                "message before the file was acknowledged",
            ));
        }
        let state = std::mem::replace(&mut self.state, State::Closed);
        let (next_state, event) = self.transition(state, msg, encoded_len)?;
        self.state = next_state;
        Ok(event)
    }

    fn transition<'a>(
        &mut self,
        state: State,
        msg: Message,
        encoded_len: usize,
    ) -> Result<(State, ReceiverEvent<'a>), ProtocolError> {
        match (state, msg) {
            (
                State::AwaitOffer,
                Message::Offer {
                    transfer_id,
                    file_count,
                    total_size,
                },
            ) => Ok((
                State::CollectingOffer {
                    transfer_id,
                    file_count,
                    total_size,
                    files: Vec::new(),
                    metadata: 0,
                },
                ReceiverEvent::Pending,
            )),
            (
                State::CollectingOffer {
                    transfer_id,
                    file_count,
                    total_size,
                    mut files,
                    metadata,
                },
                Message::OfferFiles {
                    transfer_id: tid,
                    first_index,
                    files: batch,
                },
            ) => {
                if tid != transfer_id {
                    return Err(ProtocolError::Violation("offer batch for another transfer"));
                }
                if first_index as usize != files.len() {
                    return Err(ProtocolError::Violation("offer batch out of sequence"));
                }
                let metadata = metadata.saturating_add(encoded_len);
                if metadata > MAX_OFFER_METADATA {
                    return Err(ProtocolError::LimitExceeded("offer metadata"));
                }
                if files.len().saturating_add(batch.len()) > file_count as usize {
                    return Err(ProtocolError::Violation("more files than offered"));
                }
                files.extend(batch);
                if files.len() < file_count as usize {
                    return Ok((
                        State::CollectingOffer {
                            transfer_id,
                            file_count,
                            total_size,
                            files,
                            metadata,
                        },
                        ReceiverEvent::Pending,
                    ));
                }
                let mut sum: u64 = 0;
                for f in &files {
                    sum = sum
                        .checked_add(f.size)
                        .filter(|s| *s <= MAX_FILE_SIZE)
                        .ok_or(ProtocolError::Violation("total size overflow"))?;
                }
                if sum != total_size {
                    return Err(ProtocolError::Violation("total size mismatch"));
                }
                let request = TransferRequest::new(transfer_id, files)
                    .map_err(|_| ProtocolError::Violation("invalid offer"))?;
                self.acks = vec![None; request.files().len()];
                self.request = Some(request.clone());
                Ok((State::AwaitDecision, ReceiverEvent::OfferReady(request)))
            }
            (State::ExpectFileStart { next }, Message::FileStart { index, offset }) => {
                if index != next {
                    return Err(ProtocolError::Violation("file started out of order"));
                }
                if offset != 0 {
                    return Err(ProtocolError::Violation(
                        "non-zero offset (resume not supported)",
                    ));
                }
                let size = self.file_size(index)?;
                self.hasher = blake3::Hasher::new();
                Ok((
                    State::ReceivingFile {
                        index,
                        remaining: size,
                    },
                    ReceiverEvent::FileStarted { index, size },
                ))
            }
            (State::ExpectFileStart { next }, Message::FileAbort { index, .. })
            | (State::ReceivingFile { index: next, .. }, Message::FileAbort { index, .. }) => {
                if index != next {
                    return Err(ProtocolError::Violation("abort for the wrong file"));
                }
                self.file_size(index)?;
                self.pending_ack = Some(index);
                Ok((
                    State::ExpectFileStart {
                        next: index.saturating_add(1),
                    },
                    ReceiverEvent::FileAborted { index },
                ))
            }
            (State::ReceivingFile { index, remaining }, Message::FileEnd { index: i, hash }) => {
                if i != index {
                    return Err(ProtocolError::Violation("file end for the wrong file"));
                }
                if remaining != 0 {
                    return Err(ProtocolError::Violation("file shorter than declared"));
                }
                let FileHash::Blake3(expected) = hash;
                let actual = *self.hasher.finalize().as_bytes();
                self.pending_ack = Some(index);
                Ok((
                    State::ExpectFileStart {
                        next: index.saturating_add(1),
                    },
                    ReceiverEvent::FileFinished {
                        index,
                        hash_ok: actual == expected,
                        digest: actual,
                    },
                ))
            }
            (State::ExpectFileStart { next }, Message::Complete { transfer_id }) => {
                let request = self
                    .request
                    .as_ref()
                    .ok_or(ProtocolError::Violation("complete without offer"))?;
                if transfer_id != request.id() {
                    return Err(ProtocolError::Violation("complete for another transfer"));
                }
                if next != request.file_count() {
                    return Err(ProtocolError::Violation("complete with files pending"));
                }
                Ok((State::Completing, ReceiverEvent::CompleteRequested))
            }
            _ => Err(ProtocolError::Violation(
                "unexpected message for the receiver",
            )),
        }
    }

    fn file_size(&self, index: u32) -> Result<u64, ProtocolError> {
        self.request
            .as_ref()
            .and_then(|r| r.files().get(index as usize))
            .map(|f| f.size)
            .ok_or(ProtocolError::Violation("no such file"))
    }

    /// Accepts the offer.
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] unless an offer awaits a decision.
    pub fn accept(&mut self) -> Result<Message, ProtocolError> {
        let id = self.decision_id()?;
        self.state = State::ExpectFileStart { next: 0 };
        Ok(Message::Accept { transfer_id: id })
    }

    /// Rejects the offer.
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] unless an offer awaits a decision.
    pub fn reject(&mut self, reason: RejectReason) -> Result<Message, ProtocolError> {
        let id = self.decision_id()?;
        self.state = State::Done;
        Ok(Message::Reject {
            transfer_id: id,
            reason,
        })
    }

    fn decision_id(&self) -> Result<TransferId, ProtocolError> {
        match (&self.state, &self.request) {
            (State::AwaitDecision, Some(r)) => Ok(r.id()),
            _ => Err(ProtocolError::Violation("no offer awaiting a decision")),
        }
    }

    /// Acknowledges the file that just finished or was aborted. While a file
    /// is still arriving, only `WriteFailed` may be reported (the caller
    /// then cancels the transfer).
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] for the wrong file or status.
    pub fn ack(&mut self, index: u32, status: AckStatus) -> Result<Message, ProtocolError> {
        let ok = match (self.pending_ack, &self.state) {
            (Some(p), _) => p == index,
            (None, State::ReceivingFile { index: i, .. }) => {
                *i == index && status == AckStatus::WriteFailed
            }
            _ => false,
        };
        let slot = self
            .acks
            .get_mut(index as usize)
            .ok_or(ProtocolError::Violation("ack for unknown file"))?;
        if !ok || slot.is_some() {
            return Err(ProtocolError::Violation("unexpected local ack"));
        }
        *slot = Some(status);
        if self.pending_ack == Some(index) {
            self.pending_ack = None;
        } else {
            self.state = State::Closed;
        }
        Ok(Message::FileAck { index, status })
    }

    /// Confirms completion after every stored file is durable.
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] unless completion was requested and every
    /// file was acknowledged.
    pub fn complete_ack(&mut self) -> Result<Message, ProtocolError> {
        let request = self
            .request
            .as_ref()
            .ok_or(ProtocolError::Violation("no transfer"))?;
        if !matches!(self.state, State::Completing) || self.acks.iter().any(Option::is_none) {
            return Err(ProtocolError::Violation("not ready to complete"));
        }
        let msg = Message::CompleteAck {
            transfer_id: request.id(),
            stored_count: self.stored_count(),
        };
        self.state = State::Done;
        Ok(msg)
    }

    /// Files acknowledged as stored.
    #[must_use]
    pub fn stored_count(&self) -> u32 {
        u32::try_from(
            self.acks
                .iter()
                .filter(|a| **a == Some(AckStatus::Stored))
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// Per-file acknowledgements sent so far.
    #[must_use]
    pub fn acks(&self) -> &[Option<AckStatus>] {
        &self.acks
    }

    /// Whether the transfer reached `Done` (completed or rejected).
    #[must_use]
    pub const fn is_done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    /// Whether the session was closed by a cancel, error or local failure.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        matches!(self.state, State::Closed)
    }

    /// Whether a file is being received.
    #[must_use]
    pub const fn current_file(&self) -> Option<u32> {
        match self.state {
            State::ReceivingFile { index, .. } => Some(index),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use dropumi_domain::transfer::AbortReason;

    use super::*;
    use crate::limits::MAX_FRAME_BODY;
    use crate::message::encode;
    use crate::session::SenderMachine;

    fn entry(name: &str, size: u64) -> FileEntry {
        FileEntry {
            name: name.into(),
            size,
            media_type: None,
            modified: None,
        }
    }

    fn request(files: Vec<FileEntry>) -> TransferRequest {
        TransferRequest::new(TransferId::from_bytes([3; 16]), files).unwrap()
    }

    fn enc(m: &Message) -> Vec<u8> {
        let mut b = Vec::new();
        encode(m, &mut b).unwrap();
        b
    }

    fn feed(r: &mut ReceiverMachine, m: &Message) -> Result<(), ProtocolError> {
        r.on_frame(Frame::Control(&enc(m))).map(drop)
    }

    /// A receiver that has accepted the offer for `files`.
    fn accepted(files: Vec<FileEntry>) -> (ReceiverMachine, SenderMachine) {
        let mut s = SenderMachine::new(request(files));
        let mut r = ReceiverMachine::new();
        for m in s.offer().unwrap() {
            feed(&mut r, &m).unwrap();
        }
        let accept = r.accept().unwrap();
        s.on_message(accept).unwrap();
        (r, s)
    }

    fn id() -> TransferId {
        TransferId::from_bytes([3; 16])
    }

    #[test]
    fn receiver_rejects_data_before_accept() {
        let mut r = ReceiverMachine::new();
        assert!(r.on_frame(Frame::Data(b"x")).is_err());
        let mut s = SenderMachine::new(request(vec![entry("a", 1)]));
        let mut r = ReceiverMachine::new();
        for m in s.offer().unwrap() {
            feed(&mut r, &m).unwrap();
        }
        assert!(r.on_frame(Frame::Data(b"x")).is_err());
    }

    #[test]
    fn receiver_rejects_data_beyond_size() {
        let (mut r, _s) = accepted(vec![entry("a", 2)]);
        feed(
            &mut r,
            &Message::FileStart {
                index: 0,
                offset: 0,
            },
        )
        .unwrap();
        assert!(r.on_frame(Frame::Data(b"abc")).is_err());
    }

    #[test]
    fn receiver_rejects_short_file_end() {
        let (mut r, _s) = accepted(vec![entry("a", 2)]);
        feed(
            &mut r,
            &Message::FileStart {
                index: 0,
                offset: 0,
            },
        )
        .unwrap();
        r.on_frame(Frame::Data(b"a")).unwrap();
        let end = Message::FileEnd {
            index: 0,
            hash: FileHash::Blake3([0; 32]),
        };
        assert!(feed(&mut r, &end).is_err());
    }

    #[test]
    fn receiver_rejects_out_of_order_index() {
        let (mut r, _s) = accepted(vec![entry("a", 0), entry("b", 0)]);
        assert!(
            feed(
                &mut r,
                &Message::FileStart {
                    index: 1,
                    offset: 0
                }
            )
            .is_err()
        );
        let (mut r, _s) = accepted(vec![entry("a", 0)]);
        assert!(
            feed(
                &mut r,
                &Message::FileStart {
                    index: 0,
                    offset: 5
                }
            )
            .is_err()
        );
    }

    #[test]
    fn receiver_rejects_complete_with_pending() {
        let (mut r, _s) = accepted(vec![entry("a", 0), entry("b", 0)]);
        assert!(feed(&mut r, &Message::Complete { transfer_id: id() }).is_err());
    }

    #[test]
    fn receiver_rejects_count_mismatch() {
        let mut r = ReceiverMachine::new();
        feed(
            &mut r,
            &Message::Offer {
                transfer_id: id(),
                file_count: 1,
                total_size: 2,
            },
        )
        .unwrap();
        let two = Message::OfferFiles {
            transfer_id: id(),
            first_index: 0,
            files: vec![entry("a", 1), entry("b", 1)],
        };
        assert!(feed(&mut r, &two).is_err());
        let mut r = ReceiverMachine::new();
        feed(
            &mut r,
            &Message::Offer {
                transfer_id: id(),
                file_count: 2,
                total_size: 2,
            },
        )
        .unwrap();
        let gap = Message::OfferFiles {
            transfer_id: id(),
            first_index: 1,
            files: vec![entry("a", 1)],
        };
        assert!(feed(&mut r, &gap).is_err());
    }

    #[test]
    fn receiver_rejects_total_mismatch_or_overflow() {
        for (total, sizes) in [
            (5u64, vec![1u64, 1]),
            (u64::MAX >> 1, vec![u64::MAX >> 1, 1]),
        ] {
            let mut r = ReceiverMachine::new();
            feed(
                &mut r,
                &Message::Offer {
                    transfer_id: id(),
                    file_count: 2,
                    total_size: total,
                },
            )
            .unwrap();
            let files = sizes
                .iter()
                .enumerate()
                .map(|(i, s)| entry(&format!("f{i}"), *s))
                .collect();
            let m = Message::OfferFiles {
                transfer_id: id(),
                first_index: 0,
                files,
            };
            assert!(feed(&mut r, &m).is_err());
        }
    }

    #[test]
    fn receiver_rejects_too_many_files_and_metadata_over_limit() {
        let mut r = ReceiverMachine::new();
        let too_many = enc(&Message::Offer {
            transfer_id: id(),
            file_count: 10_001,
            total_size: 0,
        });
        assert!(r.on_frame(Frame::Control(&too_many)).is_err());
        // 4 MiB metadata: many batches of long names.
        let mut r = ReceiverMachine::new();
        feed(
            &mut r,
            &Message::Offer {
                transfer_id: id(),
                file_count: 10_000,
                total_size: 0,
            },
        )
        .unwrap();
        let mut first = 0u32;
        let mut failed = false;
        while first < 10_000 {
            let files: Vec<FileEntry> = (0..60)
                .map(|i| entry(&format!("{:0>1000}", first + i), 0))
                .collect();
            let n = u32::try_from(files.len()).unwrap();
            let m = Message::OfferFiles {
                transfer_id: id(),
                first_index: first,
                files,
            };
            let body = enc(&m);
            assert!(body.len() <= MAX_FRAME_BODY);
            if let Err(e) = r.on_frame(Frame::Control(&body)) {
                assert_eq!(e, ProtocolError::LimitExceeded("offer metadata"));
                failed = true;
                break;
            }
            first += n;
        }
        assert!(failed, "metadata limit must trigger");
    }

    #[test]
    fn receiver_rejects_empty_data_wrong_direction_bad_names_and_flood() {
        let (mut r, _s) = accepted(vec![entry("a", 1)]);
        feed(
            &mut r,
            &Message::FileStart {
                index: 0,
                offset: 0,
            },
        )
        .unwrap();
        assert!(r.on_frame(Frame::Data(b"")).is_err());
        let (mut r, _s) = accepted(vec![entry("a", 1)]);
        assert!(feed(&mut r, &Message::Accept { transfer_id: id() }).is_err());
        // Ignorable flood.
        let ignorable = [0x82, 0x18, 0x80, 0xa0];
        let (mut r, _s) = accepted(vec![entry("a", 1)]);
        for _ in 0..16 {
            assert_eq!(
                r.on_frame(Frame::Control(&ignorable)).unwrap(),
                ReceiverEvent::Ignored
            );
        }
        assert!(r.on_frame(Frame::Control(&ignorable)).is_err());
    }

    #[test]
    fn sender_accepts_write_failed_for_aborted_file() {
        // FileAbort and the receiver's WRITE_FAILED ack cross on the wire.
        let (_r, mut s) = accepted(vec![entry("a", 4)]);
        s.start_file(0).unwrap();
        s.data(b"ab").unwrap();
        s.abort_file(AbortReason::SourceChanged).unwrap();
        assert!(
            s.on_message(Message::FileAck {
                index: 0,
                status: AckStatus::WriteFailed
            })
            .is_ok()
        );
        assert!(
            s.on_message(Message::FileAck {
                index: 0,
                status: AckStatus::WriteFailed
            })
            .is_err(),
            "each file is acked once"
        );
    }

    #[test]
    fn data_resets_ignorable_budget() {
        let (mut r, _s) = accepted(vec![entry("a", 100)]);
        feed(
            &mut r,
            &Message::FileStart {
                index: 0,
                offset: 0,
            },
        )
        .unwrap();
        let ignorable = [0x82, 0x18, 0x80, 0xa0];
        for _ in 0..50 {
            for _ in 0..10 {
                r.on_frame(Frame::Control(&ignorable)).unwrap();
            }
            r.on_frame(Frame::Data(b"xx")).unwrap();
        }
    }

    #[test]
    fn hash_mismatch_reports_integrity_failed() {
        let (mut r, _s) = accepted(vec![entry("a", 3)]);
        feed(
            &mut r,
            &Message::FileStart {
                index: 0,
                offset: 0,
            },
        )
        .unwrap();
        r.on_frame(Frame::Data(b"abc")).unwrap();
        let end = enc(&Message::FileEnd {
            index: 0,
            hash: FileHash::Blake3([0; 32]),
        });
        let ev = r.on_frame(Frame::Control(&end)).unwrap();
        assert!(matches!(
            ev,
            ReceiverEvent::FileFinished {
                index: 0,
                hash_ok: false,
                ..
            }
        ));
        r.ack(0, AckStatus::IntegrityFailed).unwrap();
        feed(&mut r, &Message::Complete { transfer_id: id() }).unwrap();
        let done = r.complete_ack().unwrap();
        assert_eq!(
            done,
            Message::CompleteAck {
                transfer_id: id(),
                stored_count: 0
            }
        );
    }

    #[test]
    fn zero_byte_file_ok() {
        let (mut r, mut s) = accepted(vec![entry("empty", 0)]);
        feed(&mut r, &s.start_file(0).unwrap()).unwrap();
        let end = enc(&s.end_file().unwrap());
        let ev = r.on_frame(Frame::Control(&end)).unwrap();
        assert!(matches!(
            ev,
            ReceiverEvent::FileFinished {
                index: 0,
                hash_ok: true,
                ..
            }
        ));
        let ack = r.ack(0, AckStatus::Stored).unwrap();
        s.on_message(ack).unwrap();
        feed(&mut r, &s.complete().unwrap()).unwrap();
        let ca = r.complete_ack().unwrap();
        assert_eq!(
            s.on_message(ca).unwrap(),
            crate::session::SenderEvent::Completed { stored: 1 }
        );
    }

    #[test]
    fn cancel_after_complete_ack_ignored() {
        let (mut r, mut s) = accepted(vec![entry("e", 0)]);
        feed(&mut r, &s.start_file(0).unwrap()).unwrap();
        feed(&mut r, &s.end_file().unwrap()).unwrap();
        r.ack(0, AckStatus::Stored).unwrap();
        feed(&mut r, &s.complete().unwrap()).unwrap();
        r.complete_ack().unwrap();
        let cancel = enc(&Message::Cancel {
            reason: CancelReason::User,
        });
        assert_eq!(
            r.on_frame(Frame::Control(&cancel)).unwrap(),
            ReceiverEvent::Ignored
        );
        assert!(r.is_done());
    }

    #[test]
    fn abort_and_write_failed_paths() {
        let (mut r, mut s) = accepted(vec![entry("a", 4), entry("b", 4)]);
        feed(&mut r, &s.abort_file(AbortReason::SourceChanged).unwrap()).unwrap();
        let ack = r.ack(0, AckStatus::Aborted).unwrap();
        s.on_message(ack).unwrap();
        feed(&mut r, &s.start_file(1).unwrap()).unwrap();
        s.data(b"ab").unwrap();
        r.on_frame(Frame::Data(b"ab")).unwrap();
        let ack = r.ack(1, AckStatus::WriteFailed).unwrap();
        assert!(r.is_closed());
        s.on_message(ack).unwrap();
        assert!(r.ack(1, AckStatus::Stored).is_err());
    }

    #[test]
    fn offer_batches_fit_one_record() {
        let files: Vec<FileEntry> = (0..2_000)
            .map(|i| entry(&format!("{i:0>200}.bin"), 1))
            .collect();
        let mut s = SenderMachine::new(request(files));
        let msgs = s.offer().unwrap();
        assert!(msgs.len() > 2, "large offers are split into batches");
        let mut r = ReceiverMachine::new();
        let mut ready = false;
        for m in &msgs {
            let body = enc(m);
            assert!(body.len() <= MAX_FRAME_BODY);
            if let ReceiverEvent::OfferReady(req) = r.on_frame(Frame::Control(&body)).unwrap() {
                assert_eq!(req.file_count(), 2_000);
                ready = true;
            }
        }
        assert!(ready);
        // An offer whose metadata cannot fit in 4 MiB is refused locally.
        let huge: Vec<FileEntry> = (0..10_000)
            .map(|i| entry(&format!("{i:0>1000}"), 0))
            .collect();
        let mut s = SenderMachine::new(request(huge));
        assert_eq!(
            s.offer().unwrap_err(),
            ProtocolError::LimitExceeded("offer metadata")
        );
    }
}
