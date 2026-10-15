// SPDX-License-Identifier: MPL-2.0
//! The user-facing error taxonomy shared by every layer.
//!
//! [`TransferError::code`] is the single stable code used for JSON output,
//! process exit codes and protocol error mapping.

use std::fmt;

/// Which wait timed out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Phase {
    /// TCP connect to one candidate.
    Connect,
    /// Preamble plus handshake before authentication.
    PreAuth,
    /// The handshake.
    Handshake,
    /// Waiting for the offer.
    Offer,
    /// Waiting for the receiving user's decision.
    Consent,
    /// Waiting for SAS confirmation or the peer's pairing result.
    Sas,
    /// No data from the peer.
    Idle,
    /// A write made no progress.
    Write,
    /// Waiting for the completion acknowledgement.
    CompleteAck,
    /// Draining after a cancel or error.
    Drain,
}

impl Phase {
    /// Stable machine-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::PreAuth => "pre_auth",
            Self::Handshake => "handshake",
            Self::Offer => "offer",
            Self::Consent => "consent",
            Self::Sas => "sas",
            Self::Idle => "idle",
            Self::Write => "write",
            Self::CompleteAck => "complete_ack",
            Self::Drain => "drain",
        }
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every way a session can fail.
///
/// `Storage` and `Internal` carry local detail for local display only; it
/// never goes on the wire.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TransferError {
    /// No common protocol version.
    #[error("no common protocol version (peer supports {peer:?})")]
    IncompatibleVersion {
        /// Versions the peer offered.
        peer: Vec<u16>,
    },
    /// The secure handshake failed (tampering, wrong key, bad reveal).
    #[error("secure handshake failed")]
    HandshakeFailed,
    /// The authenticated key does not match the expected device.
    #[error("the device's key does not match the expected fingerprint")]
    PeerIdentityMismatch,
    /// The peer is not trusted and unverified peers were not allowed.
    #[error("the device is not trusted (pair first or pass --allow-unverified)")]
    UnverifiedPeer,
    /// The peer refused to authorize this device.
    #[error("the device refused the connection (not authorized)")]
    NotAuthorized,
    /// Pairing was not confirmed by both users.
    #[error("pairing was not confirmed")]
    PairingRejected,
    /// The local user rejected the short authentication string.
    #[error("the verification code was not confirmed")]
    SasRejected,
    /// The peer broke the protocol.
    #[error("protocol violation: {0}")]
    ProtocolViolation(&'static str),
    /// The peer exceeded a limit.
    #[error("limit exceeded: {0}")]
    LimitExceeded(&'static str),
    /// Content or a record failed integrity checks.
    #[error("integrity check failed")]
    Integrity,
    /// Not enough storage space.
    #[error("not enough storage space")]
    InsufficientSpace,
    /// Local storage failure (detail is local only).
    #[error("storage error: {0}")]
    Storage(String),
    /// A source file changed or became unreadable while sending.
    #[error("a source file changed while sending")]
    SourceChanged,
    /// The connection closed unexpectedly.
    #[error("connection lost")]
    ConnectionLost,
    /// A wait expired.
    #[error("timed out ({0})")]
    Timeout(Phase),
    /// The peer is busy.
    #[error("the device is busy")]
    Busy,
    /// The peer reported an error code.
    #[error("the device reported an error (code {code})")]
    Remote {
        /// Protocol error code.
        code: u16,
    },
    /// A local bug or unexpected condition (detail is local only).
    #[error("internal error: {0}")]
    Internal(String),
}

impl TransferError {
    /// Stable machine-readable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::IncompatibleVersion { .. } => "incompatible_version",
            Self::HandshakeFailed => "handshake_failed",
            Self::PeerIdentityMismatch => "peer_identity_mismatch",
            Self::UnverifiedPeer => "unverified_peer",
            Self::NotAuthorized => "not_authorized",
            Self::PairingRejected => "pairing_rejected",
            Self::SasRejected => "sas_rejected",
            Self::ProtocolViolation(_) => "protocol_violation",
            Self::LimitExceeded(_) => "limit_exceeded",
            Self::Integrity => "integrity",
            Self::InsufficientSpace => "insufficient_space",
            Self::Storage(_) => "storage",
            Self::SourceChanged => "source_changed",
            Self::ConnectionLost => "connection_lost",
            Self::Timeout(_) => "timeout",
            Self::Busy => "busy",
            Self::Remote { .. } => "remote_error",
            Self::Internal(_) => "internal",
        }
    }

    /// The timed-out phase, if any.
    #[must_use]
    pub const fn phase(&self) -> Option<Phase> {
        match self {
            Self::Timeout(p) => Some(*p),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_snake_case() {
        let all = [
            TransferError::IncompatibleVersion { peer: vec![] },
            TransferError::HandshakeFailed,
            TransferError::PeerIdentityMismatch,
            TransferError::UnverifiedPeer,
            TransferError::NotAuthorized,
            TransferError::PairingRejected,
            TransferError::SasRejected,
            TransferError::ProtocolViolation("x"),
            TransferError::LimitExceeded("x"),
            TransferError::Integrity,
            TransferError::InsufficientSpace,
            TransferError::Storage(String::new()),
            TransferError::SourceChanged,
            TransferError::ConnectionLost,
            TransferError::Timeout(Phase::Idle),
            TransferError::Busy,
            TransferError::Remote { code: 1 },
            TransferError::Internal(String::new()),
        ];
        let mut codes: Vec<&str> = all.iter().map(TransferError::code).collect();
        assert!(
            codes
                .iter()
                .all(|c| c.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_'))
        );
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), all.len());
        assert_eq!(
            TransferError::Timeout(Phase::Write).phase(),
            Some(Phase::Write)
        );
    }
}
