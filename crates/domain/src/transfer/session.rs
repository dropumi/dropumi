// SPDX-License-Identifier: MPL-2.0
//! The transfer session state machine.
//!
//! | From | Event | To |
//! |---|---|---|
//! | Preparing | OfferSent / OfferReceived | Offered |
//! | Offered | ConsentRequested | AwaitingConsent |
//! | Offered, AwaitingConsent | Accepted | Accepted |
//! | Offered, AwaitingConsent | Rejected | Rejected |
//! | Accepted, Transferring | FileStarted / FileSettled | Transferring |
//! | Transferring (every file settled) | Finalizing | Finalizing |
//! | Finalizing | Finished | Completed or CompletedWithFailures |
//! | any non-terminal | Cancelled | Cancelled |
//! | any non-terminal | Failed | Failed |
//!
//! Invariants: terminal states absorb (later events are rejected and leave
//! the state unchanged); each file settles once; Completed requires every
//! file Stored; progress never exceeds the totals.

use crate::error::TransferError;
use crate::identity::Fingerprint;
use crate::transfer::progress::TransferProgress;
use crate::transfer::request::TransferRequest;
use crate::transfer::status::{CancelReason, FileOutcome, Party, RejectReason, TransferStatus};

/// Whether this device sends or receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Outgoing transfer.
    Send,
    /// Incoming transfer.
    Receive,
}

/// Inputs to [`TransferSession::apply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
    /// The sender sent the offer.
    OfferSent,
    /// The receiver received the offer.
    OfferReceived,
    /// The receiver is asking its user.
    ConsentRequested,
    /// The offer was accepted.
    Accepted,
    /// The offer was rejected.
    Rejected(RejectReason),
    /// File `index` started.
    FileStarted(u32),
    /// File `index` settled with an outcome.
    FileSettled(u32, FileOutcome),
    /// The sender declared completion.
    Finalizing,
    /// Completion was acknowledged.
    Finished,
    /// A side cancelled.
    Cancelled {
        /// Who cancelled.
        by: Party,
        /// Why.
        reason: CancelReason,
    },
    /// The session failed.
    Failed(TransferError),
}

/// An event that is not valid in the current state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid transfer transition from {from:?} on {event:?}")]
pub struct InvalidTransition {
    /// State before the event.
    pub from: TransferStatus,
    /// Rejected event.
    pub event: TransferEvent,
}

/// One transfer between this device and a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSession {
    direction: Direction,
    peer: Fingerprint,
    request: TransferRequest,
    status: TransferStatus,
    progress: TransferProgress,
    outcomes: Vec<FileOutcome>,
    started: Vec<bool>,
}

impl TransferSession {
    /// A new session in [`TransferStatus::Preparing`].
    #[must_use]
    pub fn new(direction: Direction, peer: Fingerprint, request: TransferRequest) -> Self {
        let n = request.files().len();
        Self {
            direction,
            peer,
            progress: TransferProgress::new(request.total_size(), request.file_count()),
            request,
            status: TransferStatus::Preparing,
            outcomes: vec![FileOutcome::Pending; n],
            started: vec![false; n],
        }
    }

    /// Direction.
    #[must_use]
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    /// Peer fingerprint.
    #[must_use]
    pub const fn peer(&self) -> &Fingerprint {
        &self.peer
    }

    /// The offer.
    #[must_use]
    pub const fn request(&self) -> &TransferRequest {
        &self.request
    }

    /// Current status.
    #[must_use]
    pub const fn status(&self) -> &TransferStatus {
        &self.status
    }

    /// Current progress.
    #[must_use]
    pub const fn progress(&self) -> &TransferProgress {
        &self.progress
    }

    /// Per-file outcomes in offer order.
    #[must_use]
    pub fn outcomes(&self) -> &[FileOutcome] {
        &self.outcomes
    }

    /// Number of stored files.
    #[must_use]
    pub fn stored_count(&self) -> u32 {
        u32::try_from(self.outcomes.iter().filter(|o| o.is_stored()).count()).unwrap_or(u32::MAX)
    }

    /// Records transferred bytes of the current file.
    ///
    /// # Errors
    /// Fails outside `Transferring` or when the byte total would be exceeded.
    pub fn record_bytes(&mut self, n: u64) -> Result<(), crate::transfer::ProgressError> {
        if self.status != TransferStatus::Transferring {
            return Err(crate::transfer::ProgressError);
        }
        self.progress.add_bytes(n)
    }

    /// Applies an event.
    ///
    /// # Errors
    /// Returns [`InvalidTransition`] (state unchanged) for transitions
    /// outside the table.
    pub fn apply(&mut self, event: TransferEvent) -> Result<(), InvalidTransition> {
        use TransferEvent as E;
        use TransferStatus as S;
        let reject = |s: &Self, event: TransferEvent| {
            Err(InvalidTransition {
                from: s.status.clone(),
                event,
            })
        };
        if self.status.is_terminal() {
            return reject(self, event);
        }
        match (&self.status, &event) {
            (S::Preparing, E::OfferSent) if self.direction == Direction::Send => {
                self.status = S::Offered;
            }
            (S::Preparing, E::OfferReceived) if self.direction == Direction::Receive => {
                self.status = S::Offered;
            }
            (S::Offered, E::ConsentRequested) => self.status = S::AwaitingConsent,
            (S::Offered | S::AwaitingConsent, E::Accepted) => self.status = S::Accepted,
            (S::Offered | S::AwaitingConsent, E::Rejected(r)) => self.status = S::Rejected(*r),
            (S::Accepted | S::Transferring, E::FileStarted(i)) => {
                let idx = *i as usize;
                if self.started.get(idx).copied() != Some(false) {
                    return reject(self, event);
                }
                if let Some(s) = self.started.get_mut(idx) {
                    *s = true;
                }
                self.progress.start_file(*i);
                self.status = S::Transferring;
            }
            (S::Accepted | S::Transferring, E::FileSettled(i, outcome)) => {
                let idx = *i as usize;
                if outcome == &FileOutcome::Pending
                    || self.outcomes.get(idx) != Some(&FileOutcome::Pending)
                {
                    return reject(self, event);
                }
                if self.progress.finish_file().is_err() {
                    return reject(self, event);
                }
                if let Some(slot) = self.outcomes.get_mut(idx) {
                    *slot = outcome.clone();
                }
                self.status = S::Transferring;
            }
            (S::Transferring, E::Finalizing) => {
                if self.outcomes.iter().any(|o| !o.is_settled()) {
                    return reject(self, event);
                }
                self.status = S::Finalizing;
            }
            (S::Finalizing, E::Finished) => {
                let failed = u32::try_from(self.outcomes.iter().filter(|o| !o.is_stored()).count())
                    .unwrap_or(u32::MAX);
                self.status = if failed == 0 {
                    S::Completed
                } else {
                    S::CompletedWithFailures { failed }
                };
            }
            (_, E::Cancelled { by, reason }) => {
                self.status = S::Cancelled {
                    by: *by,
                    reason: *reason,
                };
            }
            (_, E::Failed(err)) => self.status = S::Failed(err.clone()),
            _ => return reject(self, event),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::{FileEntry, TransferId};

    fn request(n: usize) -> TransferRequest {
        TransferRequest::new(
            TransferId::from_bytes([1; 16]),
            (0..n)
                .map(|i| FileEntry {
                    name: format!("f{i}"),
                    size: 10,
                    media_type: None,
                    modified: None,
                })
                .collect(),
        )
        .unwrap()
    }

    fn session(dir: Direction, n: usize) -> TransferSession {
        TransferSession::new(dir, Fingerprint::from_digest([9; 32]), request(n))
    }

    fn stored() -> FileOutcome {
        FileOutcome::Stored {
            saved_as: "x".into(),
        }
    }

    fn all_states() -> Vec<TransferSession> {
        // One representative session per status.
        let mut out = Vec::new();
        let base = session(Direction::Send, 2);
        out.push(base.clone());
        let mut s = base.clone();
        s.apply(TransferEvent::OfferSent).unwrap();
        out.push(s.clone());
        let mut c = s.clone();
        c.apply(TransferEvent::ConsentRequested).unwrap();
        out.push(c);
        s.apply(TransferEvent::Accepted).unwrap();
        out.push(s.clone());
        s.apply(TransferEvent::FileStarted(0)).unwrap();
        out.push(s.clone());
        s.apply(TransferEvent::FileSettled(0, stored())).unwrap();
        s.apply(TransferEvent::FileStarted(1)).unwrap();
        s.apply(TransferEvent::FileSettled(1, FileOutcome::Aborted))
            .unwrap();
        s.apply(TransferEvent::Finalizing).unwrap();
        out.push(s.clone());
        let mut done = s.clone();
        done.apply(TransferEvent::Finished).unwrap();
        out.push(done);
        out
    }

    fn events() -> Vec<TransferEvent> {
        vec![
            TransferEvent::OfferSent,
            TransferEvent::OfferReceived,
            TransferEvent::ConsentRequested,
            TransferEvent::Accepted,
            TransferEvent::Rejected(RejectReason::Declined),
            TransferEvent::FileStarted(0),
            TransferEvent::FileSettled(0, stored()),
            TransferEvent::Finalizing,
            TransferEvent::Finished,
            TransferEvent::Cancelled {
                by: Party::Peer,
                reason: CancelReason::User,
            },
            TransferEvent::Failed(TransferError::ConnectionLost),
        ]
    }

    #[test]
    fn transition_table_exhaustive() {
        for s in all_states() {
            for e in events() {
                let mut t = s.clone();
                let before = t.status().clone();
                let r = t.apply(e.clone());
                let expected_ok = match (&before, &e) {
                    (b, _) if b.is_terminal() => false,
                    (TransferStatus::Preparing, TransferEvent::OfferSent) => true,
                    (TransferStatus::Offered, TransferEvent::ConsentRequested) => true,
                    (
                        TransferStatus::Offered | TransferStatus::AwaitingConsent,
                        TransferEvent::Accepted | TransferEvent::Rejected(_),
                    ) => true,
                    (TransferStatus::Accepted, TransferEvent::FileStarted(0)) => true,
                    (
                        TransferStatus::Accepted | TransferStatus::Transferring,
                        TransferEvent::FileSettled(0, _),
                    ) => s.outcomes()[0] == FileOutcome::Pending,
                    (TransferStatus::Finalizing, TransferEvent::Finished) => true,
                    (_, TransferEvent::Cancelled { .. } | TransferEvent::Failed(_)) => true,
                    _ => false,
                };
                assert_eq!(r.is_ok(), expected_ok, "{before:?} on {e:?}");
                if r.is_err() {
                    assert_eq!(t.status(), &before, "rejected events leave state unchanged");
                }
            }
        }
    }

    #[test]
    fn completion_requires_all_stored_and_settled() {
        let mut s = session(Direction::Receive, 2);
        assert!(s.apply(TransferEvent::OfferSent).is_err());
        s.apply(TransferEvent::OfferReceived).unwrap();
        s.apply(TransferEvent::Accepted).unwrap();
        s.apply(TransferEvent::FileStarted(0)).unwrap();
        assert!(s.apply(TransferEvent::FileStarted(0)).is_err());
        s.record_bytes(10).unwrap();
        s.apply(TransferEvent::FileSettled(0, stored())).unwrap();
        assert!(s.apply(TransferEvent::FileSettled(0, stored())).is_err());
        assert!(
            s.apply(TransferEvent::Finalizing).is_err(),
            "file 1 pending"
        );
        assert!(s.apply(TransferEvent::FileStarted(5)).is_err());
        s.apply(TransferEvent::FileSettled(1, FileOutcome::IntegrityFailed))
            .unwrap();
        s.apply(TransferEvent::Finalizing).unwrap();
        s.apply(TransferEvent::Finished).unwrap();
        assert_eq!(
            s.status(),
            &TransferStatus::CompletedWithFailures { failed: 1 }
        );
        assert_eq!(s.stored_count(), 1);
        assert!(
            s.apply(TransferEvent::Failed(TransferError::Integrity))
                .is_err()
        );
    }
}
