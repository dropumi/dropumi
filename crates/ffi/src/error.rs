// SPDX-License-Identifier: MPL-2.0
//! Errors returned to the apps, and errors thrown by app code.
//!
//! A `message` is for local display only. It may name local files, so apps
//! never log it, persist it or send it anywhere.

use dropumi_application::error::StoreError;
use dropumi_application::usecases::UseCaseError;
use dropumi_domain::error::TransferError;

/// One stable code per local transfer error (`TransferError::code()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum ErrorCode {
    /// No common protocol version.
    IncompatibleVersion,
    /// The handshake failed.
    HandshakeFailed,
    /// The peer is not the device that was pinned.
    PeerIdentityMismatch,
    /// The peer is not trusted and unverified peers were not allowed.
    UnverifiedPeer,
    /// The peer does not accept files from this device.
    NotAuthorized,
    /// The peer declined pairing.
    PairingRejected,
    /// A verification code was not confirmed.
    SasRejected,
    /// The peer broke the protocol.
    ProtocolViolation,
    /// The peer exceeded a limit.
    LimitExceeded,
    /// A file failed its integrity check.
    Integrity,
    /// Not enough storage space.
    InsufficientSpace,
    /// Storing a file failed.
    Storage,
    /// A file changed while it was being sent.
    SourceChanged,
    /// The connection was lost.
    ConnectionLost,
    /// Something took too long.
    Timeout,
    /// The receiver is busy.
    Busy,
    /// The peer reported an error.
    RemoteError,
    /// An internal error.
    Internal,
}

impl ErrorCode {
    /// Maps a `TransferError::code()` string; unknown codes are `Internal`.
    #[must_use]
    pub fn from_code(code: &str) -> Self {
        match code {
            "incompatible_version" => Self::IncompatibleVersion,
            "handshake_failed" => Self::HandshakeFailed,
            "peer_identity_mismatch" => Self::PeerIdentityMismatch,
            "unverified_peer" => Self::UnverifiedPeer,
            "not_authorized" => Self::NotAuthorized,
            "pairing_rejected" => Self::PairingRejected,
            "sas_rejected" => Self::SasRejected,
            "protocol_violation" => Self::ProtocolViolation,
            "limit_exceeded" => Self::LimitExceeded,
            "integrity" => Self::Integrity,
            "insufficient_space" => Self::InsufficientSpace,
            "storage" => Self::Storage,
            "source_changed" => Self::SourceChanged,
            "connection_lost" => Self::ConnectionLost,
            "timeout" => Self::Timeout,
            "busy" => Self::Busy,
            "remote_error" => Self::RemoteError,
            _ => Self::Internal,
        }
    }

    /// The code of a transfer error.
    #[must_use]
    pub fn of(error: &TransferError) -> Self {
        Self::from_code(error.code())
    }
}

/// What is wrong with the local configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConfigProblem {
    /// A file or directory has unsafe permissions or the wrong owner.
    InsecurePermissions,
    /// A file is corrupt.
    Corrupt,
    /// Reading or writing failed.
    Io,
}

/// An error from a node call.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum DropumiError {
    /// The input is not valid.
    #[error("invalid input: {message}")]
    InvalidInput {
        /// What is wrong (local display only).
        message: String,
    },
    /// Nothing matches.
    #[error("not found: {message}")]
    NotFound {
        /// What is missing (local display only).
        message: String,
    },
    /// The node is doing something that prevents this call now.
    #[error("busy: {message}")]
    Busy {
        /// Why (local display only).
        message: String,
    },
    /// The call does not fit the current state.
    #[error("invalid state: {message}")]
    InvalidState {
        /// Why (local display only).
        message: String,
    },
    /// No discovery adapter is available; manual addresses still work.
    #[error("discovery unavailable: {message}")]
    DiscoveryUnavailable {
        /// Why (local display only).
        message: String,
    },
    /// The local configuration cannot be used.
    #[error("configuration problem: {message}")]
    Config {
        /// The kind of problem.
        problem: ConfigProblem,
        /// Details, which may name a local file (local display only).
        message: String,
    },
    /// No prompt with this id is waiting (answered, expired or never shown).
    #[error("no such prompt")]
    PromptNotFound,
    /// The prompt asks for the other kind of answer.
    #[error("the prompt asks for a different answer")]
    PromptKindMismatch,
    /// The node was shut down.
    #[error("the node is shut down")]
    ShutDown,
    /// A transfer-level failure.
    #[error("transfer error: {message}")]
    Transfer {
        /// Stable code.
        code: ErrorCode,
        /// Details (local display only).
        message: String,
    },
    /// An internal failure.
    #[error("internal error: {message}")]
    Internal {
        /// Details (local display only).
        message: String,
    },
}

impl DropumiError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    pub(crate) fn busy(message: impl Into<String>) -> Self {
        Self::Busy {
            message: message.into(),
        }
    }

    pub(crate) fn state(message: impl Into<String>) -> Self {
        Self::InvalidState {
            message: message.into(),
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

impl From<StoreError> for DropumiError {
    fn from(e: StoreError) -> Self {
        let problem = match &e {
            StoreError::InsecurePermissions { .. } => ConfigProblem::InsecurePermissions,
            StoreError::Corrupt { .. } => ConfigProblem::Corrupt,
            _ => ConfigProblem::Io,
        };
        Self::Config {
            problem,
            message: e.to_string(),
        }
    }
}

impl From<TransferError> for DropumiError {
    fn from(e: TransferError) -> Self {
        Self::Transfer {
            code: ErrorCode::of(&e),
            message: e.to_string(),
        }
    }
}

impl From<UseCaseError> for DropumiError {
    fn from(e: UseCaseError) -> Self {
        match e {
            UseCaseError::Usage(m) => Self::InvalidInput { message: m },
            UseCaseError::Resolve(r) => Self::NotFound {
                message: r.to_string(),
            },
            UseCaseError::DiscoveryUnavailable(m) => Self::DiscoveryUnavailable { message: m },
            UseCaseError::Store(s) => s.into(),
            UseCaseError::Transfer(t) => t.into(),
            other => Self::internal(other.to_string()),
        }
    }
}

/// An exception thrown by app code that Rust called.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CallbackError {
    /// The app reported a failure.
    #[error("{message}")]
    Failed {
        /// What failed.
        message: String,
    },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for CallbackError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed { message: e.reason }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dropumi_domain::error::Phase;

    #[test]
    fn every_transfer_error_code_maps_distinctly() {
        let errors = [
            TransferError::IncompatibleVersion { peer: vec![9] },
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
            TransferError::Storage("x".into()),
            TransferError::SourceChanged,
            TransferError::ConnectionLost,
            TransferError::Timeout(Phase::Idle),
            TransferError::Busy,
            TransferError::Remote { code: 1 },
            TransferError::Internal("x".into()),
        ];
        let codes: Vec<ErrorCode> = errors.iter().map(ErrorCode::of).collect();
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len(), "{codes:?}");
        // Only the internal error maps to Internal.
        assert_eq!(
            codes.iter().filter(|c| **c == ErrorCode::Internal).count(),
            1
        );
        assert_eq!(ErrorCode::from_code("remote_error"), ErrorCode::RemoteError);
        assert_eq!(ErrorCode::from_code("something new"), ErrorCode::Internal);
    }
}
