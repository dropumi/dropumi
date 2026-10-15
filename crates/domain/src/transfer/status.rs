// SPDX-License-Identifier: MPL-2.0
//! Transfer status, per-file outcomes and reason codes.

use crate::error::TransferError;

/// Which side did something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Party {
    /// This device.
    Local,
    /// The remote device.
    Peer,
}

/// Why an offer was rejected (wire codes are protocol v1 Reject reasons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RejectReason {
    /// The user declined.
    Declined,
    /// Nobody answered in time.
    Timeout,
    /// Not authorized (reserved).
    NotAuthorized,
    /// Not enough storage (reserved).
    InsufficientSpace,
    /// The offer uses something the receiver does not support.
    Unsupported,
    /// The receiver is busy.
    Busy,
    /// A code this version does not know.
    Unknown(u64),
}

impl RejectReason {
    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Declined => 1,
            Self::Timeout => 2,
            Self::NotAuthorized => 3,
            Self::InsufficientSpace => 4,
            Self::Unsupported => 5,
            Self::Busy => 6,
            Self::Unknown(c) => c,
        }
    }

    /// Decodes a wire code.
    #[must_use]
    pub const fn from_code(code: u64) -> Self {
        match code {
            1 => Self::Declined,
            2 => Self::Timeout,
            3 => Self::NotAuthorized,
            4 => Self::InsufficientSpace,
            5 => Self::Unsupported,
            6 => Self::Busy,
            c => Self::Unknown(c),
        }
    }

    /// Stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declined => "declined",
            Self::Timeout => "timeout",
            Self::NotAuthorized => "not_authorized",
            Self::InsufficientSpace => "insufficient_space",
            Self::Unsupported => "unsupported",
            Self::Busy => "busy",
            Self::Unknown(_) => "unknown",
        }
    }
}

/// Why a transfer was cancelled (wire codes are protocol v1 Cancel reasons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CancelReason {
    /// A user cancelled.
    User,
    /// The application is shutting down.
    Shutdown,
    /// The receiver could not store data.
    Storage,
    /// The sender could not read a source file.
    Source,
    /// A timeout expired.
    Timeout,
    /// A code this version does not know.
    Unknown(u64),
}

impl CancelReason {
    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::User => 1,
            Self::Shutdown => 2,
            Self::Storage => 3,
            Self::Source => 4,
            Self::Timeout => 5,
            Self::Unknown(c) => c,
        }
    }

    /// Decodes a wire code.
    #[must_use]
    pub const fn from_code(code: u64) -> Self {
        match code {
            1 => Self::User,
            2 => Self::Shutdown,
            3 => Self::Storage,
            4 => Self::Source,
            5 => Self::Timeout,
            c => Self::Unknown(c),
        }
    }

    /// Stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Shutdown => "shutdown",
            Self::Storage => "storage",
            Self::Source => "source",
            Self::Timeout => "timeout",
            Self::Unknown(_) => "unknown",
        }
    }
}

/// Why the sender abandoned one file (wire codes are FileAbort reasons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AbortReason {
    /// The source changed while it was being sent.
    SourceChanged,
    /// The source could not be read.
    SourceUnreadable,
    /// A code this version does not know.
    Unknown(u64),
}

impl AbortReason {
    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::SourceChanged => 1,
            Self::SourceUnreadable => 2,
            Self::Unknown(c) => c,
        }
    }

    /// Decodes a wire code.
    #[must_use]
    pub const fn from_code(code: u64) -> Self {
        match code {
            1 => Self::SourceChanged,
            2 => Self::SourceUnreadable,
            c => Self::Unknown(c),
        }
    }
}

/// What happened to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOutcome {
    /// Not settled yet.
    Pending,
    /// Verified, synced and visible under its final name.
    Stored {
        /// Final file name on the receiver (sanitized; never a path).
        saved_as: String,
    },
    /// The content hash did not match; nothing was kept.
    IntegrityFailed,
    /// The receiver could not write the file; nothing was kept.
    WriteFailed,
    /// The sender abandoned the file; nothing was kept.
    Aborted,
}

impl FileOutcome {
    /// `true` for [`FileOutcome::Stored`].
    #[must_use]
    pub const fn is_stored(&self) -> bool {
        matches!(self, Self::Stored { .. })
    }

    /// `true` once settled.
    #[must_use]
    pub const fn is_settled(&self) -> bool {
        !matches!(self, Self::Pending)
    }
}

/// Lifecycle of a transfer session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferStatus {
    /// Sender is preparing the offer, or the receiver has not seen it yet.
    Preparing,
    /// The offer was sent or received.
    Offered,
    /// Waiting for the receiving user's decision.
    AwaitingConsent,
    /// The receiver accepted.
    Accepted,
    /// Files are moving.
    Transferring,
    /// The sender declared the transfer complete; waiting for durability.
    Finalizing,
    /// Every file was stored.
    Completed,
    /// Finished, but some files were not stored.
    CompletedWithFailures {
        /// Number of files not stored.
        failed: u32,
    },
    /// The receiver rejected the offer.
    Rejected(RejectReason),
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

impl TransferStatus {
    /// Terminal states absorb every later event.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::CompletedWithFailures { .. }
                | Self::Rejected(_)
                | Self::Cancelled { .. }
                | Self::Failed(_)
        )
    }
}
