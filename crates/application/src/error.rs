// SPDX-License-Identifier: MPL-2.0
//! Port error types and their mapping into [`TransferError`].
//!
//! Local detail (paths, OS messages) stays in `Storage`/`Internal` variants
//! for local display; it is never put on the wire.

use std::io;

use dropumi_domain::error::{Phase, TransferError};

/// Maps an I/O error from a peer connection.
///
/// `UnexpectedEof`, `ConnectionReset`, `ConnectionAborted` and `BrokenPipe`
/// all mean the connection is gone; `TimedOut` becomes a timeout in `phase`.
#[must_use]
pub fn map_io(e: &io::Error, phase: Phase) -> TransferError {
    match e.kind() {
        io::ErrorKind::UnexpectedEof
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::NotConnected => TransferError::ConnectionLost,
        io::ErrorKind::TimedOut => TransferError::Timeout(phase),
        _ => TransferError::Internal(format!("I/O error: {e}")),
    }
}

/// Connecting or listening failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TransportError {
    /// The peer refused the connection.
    #[error("connection refused")]
    Refused,
    /// No route or host unreachable.
    #[error("host unreachable")]
    Unreachable,
    /// Connect timed out.
    #[error("connect timed out")]
    Timeout,
    /// Host name resolution failed.
    #[error("could not resolve host")]
    Resolve,
    /// Binding the listener failed.
    #[error("could not listen: {0}")]
    Bind(String),
    /// Other I/O failure.
    #[error("network I/O error: {0}")]
    Io(#[from] io::Error),
}

impl From<TransportError> for TransferError {
    fn from(e: TransportError) -> Self {
        match e {
            TransportError::Refused | TransportError::Unreachable => Self::ConnectionLost,
            TransportError::Timeout => Self::Timeout(Phase::Connect),
            TransportError::Resolve => Self::Internal("could not resolve host".into()),
            TransportError::Bind(m) => Self::Internal(format!("could not listen: {m}")),
            TransportError::Io(e) => map_io(&e, Phase::Connect),
        }
    }
}

/// Writing received files failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StorageError {
    /// The disk is full.
    #[error("not enough storage space")]
    Full,
    /// The receive directory is missing or not a directory.
    #[error("receive directory unavailable: {0}")]
    Directory(String),
    /// Other failure (local detail only).
    #[error("storage error: {0}")]
    Other(String),
}

impl StorageError {
    /// Classifies an I/O error.
    #[must_use]
    pub fn from_io(e: &io::Error) -> Self {
        if e.kind() == io::ErrorKind::StorageFull {
            Self::Full
        } else {
            Self::Other(e.to_string())
        }
    }
}

impl From<StorageError> for TransferError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::Full => Self::InsufficientSpace,
            StorageError::Directory(m) | StorageError::Other(m) => Self::Storage(m),
        }
    }
}

/// Reading a source file failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SourceError {
    /// The path is not a regular file.
    #[error("not a regular file: {0}")]
    NotAFile(String),
    /// The file changed while being sent.
    #[error("source file changed")]
    Changed,
    /// The file could not be read.
    #[error("cannot read {path}: {message}")]
    Unreadable {
        /// Path as given.
        path: String,
        /// OS message.
        message: String,
    },
}

impl From<SourceError> for TransferError {
    fn from(e: SourceError) -> Self {
        match e {
            SourceError::Changed => Self::SourceChanged,
            other => Self::Storage(other.to_string()),
        }
    }
}

/// Reading or writing local configuration failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// A file or directory has unsafe permissions.
    #[error("insecure permissions on {path}: {hint}")]
    InsecurePermissions {
        /// Offending path.
        path: String,
        /// How to fix it.
        hint: String,
    },
    /// A file is corrupt.
    #[error("corrupt configuration file {path}: {reason}")]
    Corrupt {
        /// Offending path.
        path: String,
        /// What is wrong.
        reason: String,
    },
    /// Other I/O failure.
    #[error("configuration I/O error: {0}")]
    Io(String),
}

impl StoreError {
    /// A fixed, path-free description for `tracing` logs. Logs never carry
    /// local paths; the `Display` text (which names the file) is only for
    /// local error messages.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InsecurePermissions { .. } => "insecure permissions",
            Self::Corrupt { .. } => "corrupt file",
            Self::Io(_) => "I/O error",
        }
    }
}

impl From<StoreError> for TransferError {
    fn from(e: StoreError) -> Self {
        Self::Storage(e.to_string())
    }
}

/// Discovery is unavailable or failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DiscoveryError {
    /// The mechanism cannot run here (no multicast, built without mDNS…).
    #[error("discovery unavailable: {0}")]
    Unavailable(String),
    /// Other failure.
    #[error("discovery error: {0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_error_kind_has_no_path() {
        let path = "/home/someone/.config/dropumi/trusted_devices.json";
        for e in [
            StoreError::InsecurePermissions {
                path: path.into(),
                hint: "chmod 600".into(),
            },
            StoreError::Corrupt {
                path: path.into(),
                reason: "bad JSON".into(),
            },
            StoreError::Io(format!("{path}: permission denied")),
        ] {
            assert!(e.to_string().contains(path), "Display keeps the path");
            assert!(!e.kind().contains(path), "{}", e.kind());
        }
        // The log labels are fixed phrases.
        assert_eq!(
            StoreError::Corrupt {
                path: path.into(),
                reason: String::new()
            }
            .kind(),
            "corrupt file"
        );
        assert_eq!(StoreError::Io(path.into()).kind(), "I/O error");
    }

    #[test]
    fn io_kinds_map_to_connection_lost() {
        for kind in [
            io::ErrorKind::UnexpectedEof,
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::BrokenPipe,
        ] {
            assert_eq!(
                map_io(&io::Error::from(kind), Phase::Idle),
                TransferError::ConnectionLost,
                "{kind:?}"
            );
        }
        assert_eq!(
            map_io(&io::Error::from(io::ErrorKind::TimedOut), Phase::Write),
            TransferError::Timeout(Phase::Write)
        );
        assert_eq!(
            TransferError::from(StorageError::from_io(&io::Error::from(
                io::ErrorKind::StorageFull
            ))),
            TransferError::InsufficientSpace
        );
    }
}
