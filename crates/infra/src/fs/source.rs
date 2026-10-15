// SPDX-License-Identifier: MPL-2.0
//! Local files to send.

use std::path::Path;
use std::time::UNIX_EPOCH;

use dropumi_application::error::SourceError;
use dropumi_application::ports::{FileSource, SourceMeta};
use dropumi_domain::filename::wire_name_from_local;

/// Reads regular files from the local filesystem. Explicitly named
/// symlinks are followed (like `cp`); directories and special files are
/// rejected.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileSource;

fn meta_of(path: &Path, meta: &std::fs::Metadata) -> Result<SourceMeta, SourceError> {
    if !meta.is_file() {
        return Err(SourceError::NotAFile(path.display().to_string()));
    }
    let name = path
        .file_name()
        .map(wire_name_from_local)
        .ok_or_else(|| SourceError::NotAFile(path.display().to_string()))?;
    let local_modified = meta.modified().ok();
    let modified = local_modified
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok());
    Ok(SourceMeta {
        name,
        size: meta.len(),
        modified,
        local_modified,
    })
}

/// Whether `now` still describes the file `expected` was taken from: the
/// same size and the same modification time, compared at full resolution
/// (the wire time has whole seconds only). Where the file system stores
/// whole seconds, a same-size edit within the same second is not seen
/// here; the receiver's BLAKE3 check still verifies the bytes sent.
fn same_file_state(now: &SourceMeta, expected: &SourceMeta) -> bool {
    now.size == expected.size
        && now.modified == expected.modified
        && now.local_modified == expected.local_modified
}

fn unreadable(path: &Path, e: &std::io::Error) -> SourceError {
    SourceError::Unreadable {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

impl FileSource for LocalFileSource {
    type Reader = tokio::fs::File;

    async fn meta(&self, path: &Path) -> Result<SourceMeta, SourceError> {
        let meta = tokio::fs::metadata(path)
            .await
            .map_err(|e| unreadable(path, &e))?;
        meta_of(path, &meta)
    }

    async fn open(
        &self,
        path: &Path,
        expected: &SourceMeta,
    ) -> Result<tokio::fs::File, SourceError> {
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|e| unreadable(path, &e))?;
        let meta = file.metadata().await.map_err(|e| unreadable(path, &e))?;
        let now = meta_of(path, &meta)?;
        if !same_file_state(&now, expected) {
            return Err(SourceError::Changed);
        }
        Ok(file)
    }

    async fn unchanged(&self, path: &Path, expected: &SourceMeta) -> bool {
        match tokio::fs::metadata(path).await {
            Ok(m) => meta_of(path, &m).is_ok_and(|now| same_file_state(&now, expected)),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn set_mtime(path: &Path, at: std::time::SystemTime) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    /// An edit that keeps the size, within the same second as the previous
    /// write, is still a change (the wire time has whole seconds only).
    #[tokio::test]
    async fn same_size_subsecond_edit_detected() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("a.txt");
        let second = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        std::fs::write(&path, b"aaaa").unwrap();
        set_mtime(&path, second + Duration::from_millis(100));
        let stored = std::fs::metadata(&path).unwrap().modified().unwrap();
        if stored.duration_since(UNIX_EPOCH).unwrap().subsec_nanos() == 0 {
            // Skipped: this file system stores whole seconds, so a
            // same-second edit cannot be told apart by its modification
            // time (the documented limitation).
            return;
        }
        let source = LocalFileSource;
        let meta = source.meta(&path).await.unwrap();

        std::fs::write(&path, b"bbbb").unwrap();
        set_mtime(&path, second + Duration::from_millis(600));

        assert!(matches!(
            source.open(&path, &meta).await,
            Err(SourceError::Changed)
        ));
        assert!(!source.unchanged(&path, &meta).await);
    }

    /// Regression: a metadata-only change (here a chmod, which updates the
    /// inode change time but not the bytes, the size or the modification
    /// time) is not a source change.
    #[cfg(unix)]
    #[tokio::test]
    async fn metadata_only_change_is_not_a_source_change() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("a.txt");
        std::fs::write(&path, b"aaaa").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let source = LocalFileSource;
        let meta = source.meta(&path).await.unwrap();

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        assert!(source.open(&path, &meta).await.is_ok());
        assert!(source.unchanged(&path, &meta).await);
    }
}
