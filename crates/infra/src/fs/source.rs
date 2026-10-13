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
