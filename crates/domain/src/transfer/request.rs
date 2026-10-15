// SPDX-License-Identifier: MPL-2.0
//! Transfer requests (the offer).

use std::fmt;

use crate::filename::{FileNameError, validate_wire_name};
use crate::limits::{MAX_FILE_SIZE, MAX_FILES_PER_TRANSFER};

/// Random 128-bit transfer identifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransferId([u8; 16]);

impl TransferId {
    /// Wraps random bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Lowercase hex (32 characters).
    #[must_use]
    pub fn to_hex(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.0)
    }
}

impl fmt::Debug for TransferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TransferId({})", self.to_hex())
    }
}

impl fmt::Display for TransferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// One file in an offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Wire name (single component, untrusted on the receiver).
    pub name: String,
    /// Exact size in bytes.
    pub size: u64,
    /// Optional media type hint (untrusted).
    pub media_type: Option<String>,
    /// Optional modification time, seconds since the Unix epoch.
    pub modified: Option<i64>,
}

/// Why a transfer request is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RequestError {
    /// No files.
    #[error("a transfer must contain at least one file")]
    NoFiles,
    /// More than 10,000 files.
    #[error("a transfer may contain at most 10000 files")]
    TooManyFiles,
    /// A file name is invalid.
    #[error("file {index} has an invalid name: {error}")]
    BadName {
        /// Index of the file.
        index: usize,
        /// Validation failure.
        error: FileNameError,
    },
    /// A file or the total exceeds 2^63 − 1 bytes.
    #[error("transfer size exceeds the maximum")]
    TooLarge,
}

/// A validated offer: 1..=10,000 files with valid names and a total size
/// that fits in 63 bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferRequest {
    id: TransferId,
    files: Vec<FileEntry>,
    total_size: u64,
}

impl TransferRequest {
    /// Validates and builds a request.
    ///
    /// # Errors
    /// Returns the first [`RequestError`] rule that fails.
    pub fn new(id: TransferId, files: Vec<FileEntry>) -> Result<Self, RequestError> {
        if files.is_empty() {
            return Err(RequestError::NoFiles);
        }
        if files.len() > MAX_FILES_PER_TRANSFER {
            return Err(RequestError::TooManyFiles);
        }
        let mut total: u64 = 0;
        for (index, f) in files.iter().enumerate() {
            validate_wire_name(&f.name).map_err(|error| RequestError::BadName { index, error })?;
            if f.size > MAX_FILE_SIZE {
                return Err(RequestError::TooLarge);
            }
            total = total
                .checked_add(f.size)
                .filter(|t| *t <= MAX_FILE_SIZE)
                .ok_or(RequestError::TooLarge)?;
        }
        Ok(Self {
            id,
            files,
            total_size: total,
        })
    }

    /// Transfer id.
    #[must_use]
    pub const fn id(&self) -> TransferId {
        self.id
    }

    /// Files in offer order.
    #[must_use]
    pub fn files(&self) -> &[FileEntry] {
        &self.files
    }

    /// Number of files (1..=10,000).
    #[must_use]
    pub fn file_count(&self) -> u32 {
        u32::try_from(self.files.len()).unwrap_or(u32::MAX)
    }

    /// Sum of file sizes.
    #[must_use]
    pub const fn total_size(&self) -> u64 {
        self.total_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, size: u64) -> FileEntry {
        FileEntry {
            name: name.into(),
            size,
            media_type: None,
            modified: None,
        }
    }

    #[test]
    fn request_invariants() {
        let id = TransferId::from_bytes([7; 16]);
        assert_eq!(TransferRequest::new(id, vec![]), Err(RequestError::NoFiles));
        let many = vec![entry("a", 0); MAX_FILES_PER_TRANSFER + 1];
        assert_eq!(
            TransferRequest::new(id, many),
            Err(RequestError::TooManyFiles)
        );
        assert!(TransferRequest::new(id, vec![entry("a", 0); MAX_FILES_PER_TRANSFER]).is_ok());
        assert!(matches!(
            TransferRequest::new(id, vec![entry("ok", 1), entry("../x", 1)]),
            Err(RequestError::BadName { index: 1, .. })
        ));
        assert_eq!(
            TransferRequest::new(id, vec![entry("a", MAX_FILE_SIZE), entry("b", 1)]),
            Err(RequestError::TooLarge)
        );
        assert_eq!(
            TransferRequest::new(id, vec![entry("a", u64::MAX)]),
            Err(RequestError::TooLarge)
        );
        let r = TransferRequest::new(id, vec![entry("a", 3), entry("b", 4)]).unwrap();
        assert_eq!(r.total_size(), 7);
        assert_eq!(r.file_count(), 2);
        assert_eq!(r.id().to_hex(), "07".repeat(16));
    }
}
