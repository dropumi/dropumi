// SPDX-License-Identifier: MPL-2.0
//! Local files to send.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use dropumi_application::error::SourceError;
use dropumi_application::ports::{FileSource, SourceMeta};
use dropumi_domain::filename::wire_name_from_local;

/// Reads regular files from the local filesystem. Explicitly named
/// symlinks are followed (like `cp`); directories and special files are
/// rejected.
#[derive(Debug, Clone, Default)]
pub struct LocalFileSource {
    /// Files that are never sent (see [`LocalFileSource::with_denied`]).
    denied: Vec<PathBuf>,
}

impl LocalFileSource {
    /// A source that refuses the files at `paths`, such as the identity key
    /// and the trust store, however they are named: directly, through a
    /// symlink or through a hard link. Files are compared by device and
    /// inode, both when a file is inspected (so the name never reaches an
    /// offer) and again on the opened handle (so a path swapped in after
    /// inspection is caught). Paths that do not exist are skipped.
    ///
    /// Unix only: elsewhere the list is ignored.
    #[must_use]
    pub fn with_denied(paths: Vec<PathBuf>) -> Self {
        Self { denied: paths }
    }

    async fn check_allowed(
        &self,
        path: &Path,
        meta: &std::fs::Metadata,
    ) -> Result<(), SourceError> {
        for denied in &self.denied {
            if let Ok(d) = tokio::fs::metadata(denied).await
                && same_file(meta, &d)
            {
                return Err(SourceError::Protected(path.display().to_string()));
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(_: &std::fs::Metadata, _: &std::fs::Metadata) -> bool {
    false
}

fn meta_of(path: &Path, meta: &std::fs::Metadata) -> Result<SourceMeta, SourceError> {
    if !meta.is_file() {
        return Err(SourceError::NotAFile(path.display().to_string()));
    }
    let name = path
        .file_name()
        .map(wire_name_from_local)
        .ok_or_else(|| SourceError::NotAFile(path.display().to_string()))?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok());
    Ok(SourceMeta {
        name,
        size: meta.len(),
        modified,
    })
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
        self.check_allowed(path, &meta).await?;
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
        self.check_allowed(path, &meta).await?;
        let now = meta_of(path, &meta)?;
        if now.size != expected.size || now.modified != expected.modified {
            return Err(SourceError::Changed);
        }
        Ok(file)
    }

    async fn unchanged(&self, path: &Path, expected: &SourceMeta) -> bool {
        match tokio::fs::metadata(path).await {
            Ok(m) => meta_of(path, &m)
                .is_ok_and(|now| now.size == expected.size && now.modified == expected.modified),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    struct Fixture {
        _dir: tempfile::TempDir,
        root: std::path::PathBuf,
        key: std::path::PathBuf,
        source: LocalFileSource,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let config = root.join("config");
        std::fs::create_dir(&config).unwrap();
        let key = config.join("identity.key");
        std::fs::write(&key, b"DRPMKEY1 secret").unwrap();
        let source = LocalFileSource::with_denied(vec![key.clone(), config.join("missing.json")]);
        Fixture {
            _dir: dir,
            root,
            key,
            source,
        }
    }

    fn is_protected<T: std::fmt::Debug>(r: &Result<T, SourceError>) -> bool {
        matches!(r, Err(SourceError::Protected(_)))
    }

    #[tokio::test]
    async fn denied_file_refused_directly() {
        let f = fixture();
        assert!(is_protected(&f.source.meta(&f.key).await));
    }

    #[tokio::test]
    async fn denied_file_refused_through_symlink() {
        let f = fixture();
        let link = f.root.join("photo.jpg");
        std::os::unix::fs::symlink(&f.key, &link).unwrap();
        assert!(is_protected(&f.source.meta(&link).await));
    }

    #[tokio::test]
    async fn denied_file_refused_through_hard_link() {
        let f = fixture();
        let link = f.root.join("notes.txt");
        std::fs::hard_link(&f.key, &link).unwrap();
        assert!(is_protected(&f.source.meta(&link).await));
    }

    #[tokio::test]
    async fn denied_file_refused_when_swapped_in_after_inspection() {
        let f = fixture();
        let path = f.root.join("report.txt");
        std::fs::write(&path, b"DRPMKEY1 report").unwrap();
        let meta = f.source.meta(&path).await.unwrap();
        // Replaced by a symlink to the key between inspection and opening.
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&f.key, &path).unwrap();
        assert!(is_protected(&f.source.open(&path, &meta).await));
    }

    #[tokio::test]
    async fn other_files_unaffected() {
        let f = fixture();
        let path = f.root.join("ok.txt");
        std::fs::write(&path, b"fine").unwrap();
        let meta = f.source.meta(&path).await.unwrap();
        assert_eq!((meta.name.as_str(), meta.size), ("ok.txt", 4));
        assert!(f.source.open(&path, &meta).await.is_ok());
        // Without a deny list nothing is refused.
        assert!(LocalFileSource::default().meta(&f.key).await.is_ok());
    }
}
