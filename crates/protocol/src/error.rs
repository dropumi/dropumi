// SPDX-License-Identifier: MPL-2.0
//! Protocol errors.

use dropumi_domain::error::TransferError;

use crate::message::ErrorCode;

/// Everything that can go wrong while parsing or running protocol v1.
///
/// Details from the Noise library never leave this crate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ProtocolError {
    /// Bytes that do not form a valid structure.
    #[error("malformed: {0}")]
    Malformed(&'static str),
    /// An unknown, non-ignorable message type.
    #[error("unsupported message type {0}")]
    Unsupported(u8),
    /// A valid message in the wrong place.
    #[error("protocol violation: {0}")]
    Violation(&'static str),
    /// A size or count limit was exceeded.
    #[error("limit exceeded: {0}")]
    LimitExceeded(&'static str),
    /// The preamble is invalid.
    #[error("invalid preamble")]
    BadPreamble,
    /// No common protocol version.
    #[error("no common protocol version (peer supports {0:?})")]
    NoCommonVersion(Vec<u16>),
    /// The Noise handshake failed.
    #[error("handshake failed")]
    Handshake,
    /// A transport record failed to decrypt.
    #[error("decryption failed")]
    Decrypt,
    /// The 64-bit nonce space is exhausted.
    #[error("nonce space exhausted")]
    NonceExhausted,
    /// The responder's SAS reveal does not match its commitment.
    #[error("SAS commitment mismatch")]
    SasCommitmentMismatch,
    /// The OS random number generator failed.
    #[error("random number generator failure")]
    Random,
    /// Encoding failed (internal error).
    #[error("encoding failed")]
    Encode,
}

impl ProtocolError {
    /// The wire error code to send for this error, if any.
    #[must_use]
    pub const fn wire_code(&self) -> ErrorCode {
        match self {
            Self::Unsupported(_) => ErrorCode::UnsupportedMessage,
            Self::LimitExceeded(_) => ErrorCode::LimitExceeded,
            Self::Random | Self::Encode | Self::NonceExhausted => ErrorCode::Internal,
            Self::Malformed(_)
            | Self::Violation(_)
            | Self::BadPreamble
            | Self::NoCommonVersion(_)
            | Self::Handshake
            | Self::Decrypt
            | Self::SasCommitmentMismatch => ErrorCode::ProtocolViolation,
        }
    }

    /// The user-facing error.
    #[must_use]
    pub fn to_transfer_error(&self) -> TransferError {
        match self {
            Self::Malformed(m) | Self::Violation(m) => TransferError::ProtocolViolation(m),
            Self::Unsupported(_) => TransferError::ProtocolViolation("unsupported message"),
            Self::LimitExceeded(m) => TransferError::LimitExceeded(m),
            Self::BadPreamble => TransferError::ProtocolViolation("invalid preamble"),
            Self::NoCommonVersion(v) => TransferError::IncompatibleVersion { peer: v.clone() },
            Self::Handshake | Self::SasCommitmentMismatch => TransferError::HandshakeFailed,
            Self::Decrypt => TransferError::Integrity,
            Self::NonceExhausted => TransferError::Internal("nonce space exhausted".into()),
            Self::Random => TransferError::Internal("random number generator failure".into()),
            Self::Encode => TransferError::Internal("message encoding failed".into()),
        }
    }
}

impl From<ProtocolError> for TransferError {
    fn from(e: ProtocolError) -> Self {
        e.to_transfer_error()
    }
}
