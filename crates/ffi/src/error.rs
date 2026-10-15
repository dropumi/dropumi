// SPDX-License-Identifier: MPL-2.0
//! Errors returned to the apps, and errors thrown by app code.
//!
//! A `detail` is for local display only. It may name local files, so apps
//! never log it, persist it or send it anywhere. (Fields are not called
//! `message`: that would clash with `Throwable.message` in Kotlin.)

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
    #[error("invalid input: {detail}")]
    InvalidInput {
        /// What is wrong (local display only).
        detail: String,
    },
    /// Nothing matches.
    #[error("not found: {detail}")]
    NotFound {
        /// What is missing (local display only).
        detail: String,
    },
    /// The node is doing something that prevents this call now.
    #[error("busy: {detail}")]
    Busy {
        /// Why (local display only).
        detail: String,
    },
    /// The call does not fit the current state.
    #[error("invalid state: {detail}")]
    InvalidState {
        /// Why (local display only).
        detail: String,
    },
    /// No discovery adapter is available; manual addresses still work.
    #[error("discovery unavailable: {detail}")]
    DiscoveryUnavailable {
        /// Why (local display only).
        detail: String,
    },
    /// The local configuration cannot be used. Nothing was changed, except
    /// when the detail starts with "the change was saved, but it could not
    /// be read back": then a configuration change is in place.
    #[error("configuration problem: {detail}")]
    Config {
        /// The kind of problem.
        problem: ConfigProblem,
        /// Details, which may name a local file (local display only).
        detail: String,
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
    #[error("transfer error: {detail}")]
    Transfer {
        /// Stable code.
        code: ErrorCode,
        /// Details (local display only).
        detail: String,
    },
    /// An internal failure.
    #[error("internal error: {detail}")]
    Internal {
        /// Details (local display only).
        detail: String,
    },
}

impl DropumiError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            detail: message.into(),
        }
    }

    pub(crate) fn busy(message: impl Into<String>) -> Self {
        Self::Busy {
            detail: message.into(),
        }
    }

    pub(crate) fn state(message: impl Into<String>) -> Self {
        Self::InvalidState {
            detail: message.into(),
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            detail: message.into(),
        }
    }
}

fn config_problem(e: &StoreError) -> ConfigProblem {
    match e {
        StoreError::InsecurePermissions { .. } => ConfigProblem::InsecurePermissions,
        StoreError::Corrupt { .. } => ConfigProblem::Corrupt,
        _ => ConfigProblem::Io,
    }
}

impl From<StoreError> for DropumiError {
    fn from(e: StoreError) -> Self {
        Self::Config {
            problem: config_problem(&e),
            detail: e.to_string(),
        }
    }
}

/// The warning a node call publishes when its configuration change is in
/// place but the configuration directory could not be synced (fixed text:
/// no path, nothing local).
pub(crate) const SAVED_NOT_DURABLE: &str = "the change was saved, but the configuration \
    directory could not be synced, so it may not survive a crash or power loss";

/// The start of the `Config` error detail for a change that was saved but
/// whose new state could not be read back (the CLI uses the same phrase).
pub(crate) const SAVED_UNREADABLE: &str = "the change was saved, but it could not be read back";

/// Settles the result of a configuration write for a node call.
///
/// - `Ok(state)`: applied; the state is returned, with no warning.
/// - [`StoreError::NotDurable`]: the change is in place, only the directory
///   sync failed. `warn` gets [`SAVED_NOT_DURABLE`] first, for the caller to
///   publish as a warning (the log names only the error's fixed kind); then
///   `reread` loads the applied state, which is returned. A failed re-read
///   is a `Config` error whose detail starts with [`SAVED_UNREADABLE`], so it
///   never reads as "nothing changed", and the warning is already out.
/// - Any other error: nothing was changed; a plain `Config` error.
pub(crate) fn settle<T>(
    written: Result<T, StoreError>,
    warn: impl FnOnce(&'static str),
    reread: impl FnOnce() -> Result<T, StoreError>,
) -> Result<T, DropumiError> {
    match written {
        Ok(state) => Ok(state),
        Err(e @ StoreError::NotDurable(_)) => {
            // Only the fixed kind is logged, never the path.
            tracing::warn!(
                kind = e.kind(),
                "configuration change saved, but the configuration directory could not be synced"
            );
            warn(SAVED_NOT_DURABLE);
            reread().map_err(|e| DropumiError::Config {
                problem: config_problem(&e),
                detail: format!("{SAVED_UNREADABLE}: {e}"),
            })
        }
        Err(e) => Err(e.into()),
    }
}

impl From<TransferError> for DropumiError {
    fn from(e: TransferError) -> Self {
        Self::Transfer {
            code: ErrorCode::of(&e),
            detail: e.to_string(),
        }
    }
}

impl From<UseCaseError> for DropumiError {
    fn from(e: UseCaseError) -> Self {
        match e {
            UseCaseError::Usage(m) => Self::InvalidInput { detail: m },
            UseCaseError::Resolve(r) => Self::NotFound {
                detail: r.to_string(),
            },
            UseCaseError::DiscoveryUnavailable(m) => Self::DiscoveryUnavailable { detail: m },
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
    #[error("{detail}")]
    Failed {
        /// What failed.
        detail: String,
    },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for CallbackError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed { detail: e.reason }
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

    const PATH: &str = "/home/someone/.config/dropumi/device.json";

    /// Regression: a write saved without a confirmed directory sync is
    /// applied. The call publishes the fixed, path-free warning first, then
    /// re-reads the state and succeeds with it instead of failing.
    #[test]
    fn not_durable_write_warns_before_rereading() {
        let log = std::cell::RefCell::new(Vec::new());
        let state = settle(
            Err::<&str, _>(StoreError::NotDurable(format!("{PATH}: I/O error"))),
            |warning| log.borrow_mut().push(warning.to_owned()),
            || {
                log.borrow_mut().push("re-read".to_owned());
                Ok("re-read state")
            },
        )
        .unwrap();
        assert_eq!(state, "re-read state");
        assert_eq!(*log.borrow(), [SAVED_NOT_DURABLE, "re-read"]);
        assert!(!SAVED_NOT_DURABLE.contains('/'), "{SAVED_NOT_DURABLE}");
    }

    /// Regression: when the re-read after a not-durable write fails, the
    /// warning has still been published, and the error says the change was
    /// saved (a plain `Config` error would mean nothing changed).
    #[test]
    fn failed_reread_after_not_durable_write_keeps_the_warning() {
        let mut warnings = Vec::new();
        let settled = settle(
            Err::<&str, _>(StoreError::NotDurable(PATH.into())),
            |warning| warnings.push(warning),
            || {
                Err(StoreError::Corrupt {
                    path: PATH.into(),
                    reason: "bad JSON".into(),
                })
            },
        );
        assert_eq!(warnings, [SAVED_NOT_DURABLE]);
        let Err(DropumiError::Config {
            problem: ConfigProblem::Corrupt,
            detail,
        }) = settled
        else {
            panic!("expected a Config error, got {settled:?}")
        };
        assert!(
            detail.starts_with("the change was saved, but it could not be read back: "),
            "{detail}"
        );
    }

    /// A durable write needs no re-read and no warning.
    #[test]
    fn durable_write_is_applied_without_a_warning() {
        let settled = settle(
            Ok("written"),
            |warning| panic!("no warning after a durable write: {warning}"),
            || -> Result<&str, StoreError> { panic!("no re-read after a durable write") },
        );
        assert_eq!(settled.unwrap(), "written");
    }

    /// Every other store error means nothing changed: a plain `Config`
    /// error, with no warning and no re-read.
    #[test]
    fn store_failure_is_a_config_error() {
        for (e, expected) in [
            (StoreError::Io(PATH.into()), ConfigProblem::Io),
            (
                StoreError::InsecurePermissions {
                    path: PATH.into(),
                    hint: "chmod 600".into(),
                },
                ConfigProblem::InsecurePermissions,
            ),
            (
                StoreError::Corrupt {
                    path: PATH.into(),
                    reason: "bad JSON".into(),
                },
                ConfigProblem::Corrupt,
            ),
        ] {
            let settled = settle(
                Err::<&str, _>(e),
                |warning| panic!("no warning after a failed write: {warning}"),
                || -> Result<&str, StoreError> { panic!("no re-read after a failed write") },
            );
            assert!(
                matches!(
                    &settled,
                    Err(DropumiError::Config { problem, detail })
                        if *problem == expected && !detail.contains("saved")
                ),
                "{settled:?}"
            );
        }
    }
}
