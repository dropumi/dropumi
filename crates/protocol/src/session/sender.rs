// SPDX-License-Identifier: MPL-2.0
//! The initiator's (sender's) transfer machine.

use dropumi_domain::transfer::{AbortReason, CancelReason, RejectReason, TransferRequest};

use super::IgnorableBudget;
use crate::error::ProtocolError;
use crate::limits::{MAX_FRAME_BODY, MAX_OFFER_METADATA};
use crate::message::{
    AckStatus, Decoded, ErrorCode, FileHash, Message, Role, decode, encode, file_entry_encoded_len,
};

/// Conservative upper bound for the `OfferFiles` envelope around entries.
const OFFER_FILES_OVERHEAD: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileState {
    NotStarted,
    Streaming,
    Ended,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Offering,
    AwaitDecision,
    BetweenFiles { next: u32 },
    Streaming { index: u32, sent: u64, size: u64 },
    AwaitCompleteAck,
    Done,
}

/// What an inbound message means for the sender.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SenderEvent {
    /// The receiver accepted the offer.
    Accepted,
    /// The receiver rejected the offer (terminal).
    Rejected(RejectReason),
    /// The receiver reported a file's result.
    FileAcked {
        /// File index.
        index: u32,
        /// Result.
        status: AckStatus,
    },
    /// Everything stored is durable (terminal).
    Completed {
        /// Files stored.
        stored: u32,
    },
    /// The receiver cancelled (terminal).
    PeerCancelled(CancelReason),
    /// The receiver sent an error (terminal).
    PeerError(ErrorCode),
    /// An ignorable message was skipped.
    Ignored,
}

/// Drives one outgoing transfer.
#[derive(Debug)]
pub struct SenderMachine {
    request: TransferRequest,
    state: State,
    files: Vec<FileState>,
    acks: Vec<Option<AckStatus>>,
    hasher: blake3::Hasher,
    ignorable: IgnorableBudget,
}

impl SenderMachine {
    /// A machine for `request`.
    #[must_use]
    pub fn new(request: TransferRequest) -> Self {
        let n = request.files().len();
        Self {
            request,
            state: State::Offering,
            files: vec![FileState::NotStarted; n],
            acks: vec![None; n],
            hasher: blake3::Hasher::new(),
            ignorable: IgnorableBudget::default(),
        }
    }

    /// The offer.
    #[must_use]
    pub const fn request(&self) -> &TransferRequest {
        &self.request
    }

    /// Builds `Offer` plus `OfferFiles` batches; each encodes to at most
    /// 65,518 bytes, and the batches together stay within the receiver's
    /// 4 MiB metadata limit.
    ///
    /// # Errors
    /// [`ProtocolError::LimitExceeded`] if the metadata does not fit;
    /// [`ProtocolError::Violation`] if called twice.
    pub fn offer(&mut self) -> Result<Vec<Message>, ProtocolError> {
        if self.state != State::Offering {
            return Err(ProtocolError::Violation("offer already sent"));
        }
        let id = self.request.id();
        let mut out = vec![Message::Offer {
            transfer_id: id,
            file_count: self.request.file_count(),
            total_size: self.request.total_size(),
        }];
        let mut batch = Vec::new();
        let mut batch_len = OFFER_FILES_OVERHEAD;
        let mut first = 0u32;
        let mut total_metadata = 0usize;
        for (i, entry) in self.request.files().iter().enumerate() {
            let len = file_entry_encoded_len(entry)?;
            if batch_len.saturating_add(len) > MAX_FRAME_BODY && !batch.is_empty() {
                total_metadata = total_metadata.saturating_add(batch_len);
                out.push(Message::OfferFiles {
                    transfer_id: id,
                    first_index: first,
                    files: std::mem::take(&mut batch),
                });
                first = u32::try_from(i).map_err(|_| ProtocolError::LimitExceeded("files"))?;
                batch_len = OFFER_FILES_OVERHEAD;
            }
            batch_len = batch_len.saturating_add(len);
            batch.push(entry.clone());
        }
        total_metadata = total_metadata.saturating_add(batch_len);
        out.push(Message::OfferFiles {
            transfer_id: id,
            first_index: first,
            files: batch,
        });
        if total_metadata > MAX_OFFER_METADATA {
            return Err(ProtocolError::LimitExceeded("offer metadata"));
        }
        // Verify the size bound with the real encoder.
        let mut buf = Vec::new();
        for m in &out {
            buf.clear();
            encode(m, &mut buf)?;
            if buf.len() > MAX_FRAME_BODY {
                return Err(ProtocolError::LimitExceeded("offer batch"));
            }
        }
        self.state = State::AwaitDecision;
        Ok(out)
    }

    /// Decodes and handles one control message from the receiver.
    ///
    /// # Errors
    /// Decoding errors and [`ProtocolError::Violation`] for messages that
    /// are not valid now.
    pub fn on_control(&mut self, body: &[u8]) -> Result<SenderEvent, ProtocolError> {
        match decode(body, Role::Responder)? {
            Decoded::Ignored(_) => {
                self.ignorable.ignored()?;
                Ok(SenderEvent::Ignored)
            }
            Decoded::Message(m) => {
                self.ignorable.reset();
                self.on_message(m)
            }
        }
    }

    /// Handles one decoded message from the receiver.
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] for messages that are not valid now.
    pub fn on_message(&mut self, msg: Message) -> Result<SenderEvent, ProtocolError> {
        let id = self.request.id();
        match msg {
            Message::Cancel { reason } => {
                self.state = State::Done;
                Ok(SenderEvent::PeerCancelled(reason))
            }
            Message::Error { code, .. } => {
                self.state = State::Done;
                Ok(SenderEvent::PeerError(code))
            }
            Message::Accept { transfer_id } if self.state == State::AwaitDecision => {
                if transfer_id != id {
                    return Err(ProtocolError::Violation("accept for another transfer"));
                }
                self.state = State::BetweenFiles { next: 0 };
                Ok(SenderEvent::Accepted)
            }
            Message::Reject {
                transfer_id,
                reason,
            } if self.state == State::AwaitDecision => {
                if transfer_id != id {
                    return Err(ProtocolError::Violation("reject for another transfer"));
                }
                self.state = State::Done;
                Ok(SenderEvent::Rejected(reason))
            }
            Message::FileAck { index, status } if self.accepted() => {
                let i = index as usize;
                let file = self
                    .files
                    .get(i)
                    .copied()
                    .ok_or(ProtocolError::Violation("ack for unknown file"))?;
                let valid = match status {
                    AckStatus::Stored | AckStatus::IntegrityFailed => file == FileState::Ended,
                    AckStatus::Aborted => file == FileState::Aborted,
                    // Writes and aborts cross on the wire: the receiver may report a
                    // write failure for a file the sender has just aborted.
                    AckStatus::WriteFailed => matches!(
                        file,
                        FileState::Streaming | FileState::Ended | FileState::Aborted
                    ),
                };
                let slot = self
                    .acks
                    .get_mut(i)
                    .ok_or(ProtocolError::Violation("ack for unknown file"))?;
                if !valid || slot.is_some() {
                    return Err(ProtocolError::Violation("unexpected file ack"));
                }
                *slot = Some(status);
                Ok(SenderEvent::FileAcked { index, status })
            }
            Message::CompleteAck {
                transfer_id,
                stored_count,
            } if self.state == State::AwaitCompleteAck => {
                if transfer_id != id {
                    return Err(ProtocolError::Violation("completion for another transfer"));
                }
                if self.acks.iter().any(Option::is_none) {
                    return Err(ProtocolError::Violation("completion before every file ack"));
                }
                let stored = self.stored_count();
                if stored != stored_count {
                    return Err(ProtocolError::Violation("stored count mismatch"));
                }
                self.state = State::Done;
                Ok(SenderEvent::Completed { stored })
            }
            _ => Err(ProtocolError::Violation(
                "unexpected message for the sender",
            )),
        }
    }

    const fn accepted(&self) -> bool {
        matches!(
            self.state,
            State::BetweenFiles { .. } | State::Streaming { .. } | State::AwaitCompleteAck
        )
    }

    /// The next file index to start, if the sender is between files.
    #[must_use]
    pub const fn next_file(&self) -> Option<u32> {
        match self.state {
            State::BetweenFiles { next } => Some(next),
            _ => None,
        }
    }

    /// Starts file `index` (must be the next one).
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] when called out of order.
    pub fn start_file(&mut self, index: u32) -> Result<Message, ProtocolError> {
        if self.next_file() != Some(index) {
            return Err(ProtocolError::Violation("file started out of order"));
        }
        let size = self
            .request
            .files()
            .get(index as usize)
            .map(|f| f.size)
            .ok_or(ProtocolError::Violation("no such file"))?;
        if let Some(f) = self.files.get_mut(index as usize) {
            *f = FileState::Streaming;
        }
        self.hasher = blake3::Hasher::new();
        self.state = State::Streaming {
            index,
            sent: 0,
            size,
        };
        Ok(Message::FileStart { index, offset: 0 })
    }

    /// Accounts for and hashes a chunk of the current file.
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] outside a file or past its size.
    pub fn data(&mut self, chunk: &[u8]) -> Result<(), ProtocolError> {
        let State::Streaming { index, sent, size } = self.state else {
            return Err(ProtocolError::Violation("data outside a file"));
        };
        let len = u64::try_from(chunk.len()).map_err(|_| ProtocolError::LimitExceeded("chunk"))?;
        let next = sent
            .checked_add(len)
            .filter(|n| *n <= size)
            .ok_or(ProtocolError::Violation("data beyond declared size"))?;
        self.hasher.update(chunk);
        self.state = State::Streaming {
            index,
            sent: next,
            size,
        };
        Ok(())
    }

    /// Bytes still to send for the current file.
    #[must_use]
    pub const fn remaining(&self) -> Option<u64> {
        match self.state {
            State::Streaming { sent, size, .. } => Some(size.saturating_sub(sent)),
            _ => None,
        }
    }

    /// Ends the current file (all bytes sent) and returns `FileEnd`.
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] if bytes are missing.
    pub fn end_file(&mut self) -> Result<Message, ProtocolError> {
        let State::Streaming { index, sent, size } = self.state else {
            return Err(ProtocolError::Violation("no file in progress"));
        };
        if sent != size {
            return Err(ProtocolError::Violation("file shorter than declared"));
        }
        let hash = FileHash::Blake3(*self.hasher.finalize().as_bytes());
        if let Some(f) = self.files.get_mut(index as usize) {
            *f = FileState::Ended;
        }
        self.state = State::BetweenFiles {
            next: index.saturating_add(1),
        };
        Ok(Message::FileEnd { index, hash })
    }

    /// Abandons the current file, or the next one if none is in progress.
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] when there is no file to abort.
    pub fn abort_file(&mut self, reason: AbortReason) -> Result<Message, ProtocolError> {
        let index = match self.state {
            State::Streaming { index, .. } => index,
            State::BetweenFiles { next } if (next as usize) < self.files.len() => next,
            _ => return Err(ProtocolError::Violation("no file to abort")),
        };
        if let Some(f) = self.files.get_mut(index as usize) {
            *f = FileState::Aborted;
        }
        self.state = State::BetweenFiles {
            next: index.saturating_add(1),
        };
        Ok(Message::FileAbort { index, reason })
    }

    /// Declares the transfer complete once every file was ended or aborted.
    ///
    /// # Errors
    /// [`ProtocolError::Violation`] if files remain.
    pub fn complete(&mut self) -> Result<Message, ProtocolError> {
        match self.state {
            State::BetweenFiles { next } if next as usize == self.files.len() => {
                self.state = State::AwaitCompleteAck;
                Ok(Message::Complete {
                    transfer_id: self.request.id(),
                })
            }
            _ => Err(ProtocolError::Violation("files remain before completion")),
        }
    }

    /// Whether the machine reached a terminal state.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    /// Per-file acknowledgements received so far.
    #[must_use]
    pub fn acks(&self) -> &[Option<AckStatus>] {
        &self.acks
    }

    /// Files the receiver reported as stored.
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
}
