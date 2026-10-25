// SPDX-License-Identifier: MPL-2.0
//! The receive directory: temp files, writes, commits and discards.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dropumi_application::error::StorageError;
use dropumi_application::ports::{FileSink, IncomingFile, SavedFile};
use dropumi_domain::filename::{SafeFileName, TEMP_PREFIX};
use tempfile::TempPath;
use tokio::io::AsyncWriteExt;

use super::finalize::{FinalizeOps, StdFinalizeOps, finalize};

/// Temporary file suffix.
pub const TEMP_SUFFIX: &str = ".part";
/// Random characters in temporary file names.
pub const TEMP_RANDOM_LEN: usize = 12;

/// A directory that receives files.
#[derive(Debug)]
pub struct ReceiveDir<O: FinalizeOps = StdFinalizeOps> {
    dir: PathBuf,
    ops: Arc<O>,
    /// Next collision suffix to try per sanitized name, so duplicate names
    /// do not re-probe every taken name.
    next_suffix: Arc<Mutex<HashMap<String, u32>>>,
}

impl ReceiveDir<StdFinalizeOps> {
    /// Opens an existing directory (it is never created implicitly).
    ///
    /// # Errors
    /// [`StorageError::Directory`] if it is missing or not a directory.
    pub fn open(dir: &Path) -> Result<Self, StorageError> {
        Self::with_ops(dir, StdFinalizeOps)
    }
}

impl<O: FinalizeOps> ReceiveDir<O> {
    /// Opens an existing directory with custom finalization operations.
    ///
    /// # Errors
    /// [`StorageError::Directory`] if it is missing or not a directory.
    pub fn with_ops(dir: &Path, ops: O) -> Result<Self, StorageError> {
        let meta = std::fs::metadata(dir)
            .map_err(|e| StorageError::Directory(format!("{}: {e}", dir.display())))?;
        if !meta.is_dir() {
            return Err(StorageError::Directory(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        let dir = dir
            .canonicalize()
            .map_err(|e| StorageError::Directory(format!("{}: {e}", dir.display())))?;
        Ok(Self {
            dir,
            ops: Arc::new(ops),
            next_suffix: Arc::default(),
        })
    }

    /// The canonical directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.dir
    }
}

/// A file being received.
#[derive(Debug)]
pub struct IncomingFsFile<O: FinalizeOps> {
    // Field order: the handle is dropped before the temp path deletes the file.
    file: Option<tokio::fs::File>,
    temp: Option<TempPath>,
    dir: PathBuf,
    name: SafeFileName,
    ops: Arc<O>,
    next_suffix: Arc<Mutex<HashMap<String, u32>>>,
}

fn blocking_err(e: &tokio::task::JoinError) -> StorageError {
    StorageError::Other(format!("background task failed: {e}"))
}

impl<O: FinalizeOps> IncomingFile for IncomingFsFile<O> {
    async fn write(&mut self, buf: &[u8]) -> Result<(), StorageError> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| StorageError::Other("file already closed".into()))?;
        file.write_all(buf)
            .await
            .map_err(|e| StorageError::from_io(&e))
    }

    async fn commit(mut self) -> Result<SavedFile, StorageError> {
        if let Some(mut file) = self.file.take() {
            file.flush().await.map_err(|e| StorageError::from_io(&e))?;
            file.sync_all()
                .await
                .map_err(|e| StorageError::from_io(&e))?;
            drop(file);
        }
        let temp = self
            .temp
            .take()
            .ok_or_else(|| StorageError::Other("file already finalized".into()))?;
        let (dir, name, ops) = (self.dir.clone(), self.name.clone(), Arc::clone(&self.ops));
        let key = name.as_str().to_owned();
        let start = self
            .next_suffix
            .lock()
            .map_or(0, |m| m.get(&key).copied().unwrap_or(0));
        let (saved, used) =
            tokio::task::spawn_blocking(move || finalize(ops.as_ref(), temp, &dir, &name, start))
                .await
                .map_err(|e| blocking_err(&e))?
                .map_err(|e| StorageError::Other(e.to_string()))?;
        if let (Some(n), Ok(mut m)) = (used, self.next_suffix.lock()) {
            m.insert(key, n.saturating_add(1));
        }
        Ok(SavedFile { saved_as: saved })
    }

    async fn discard(mut self) {
        drop(self.file.take());
        if let Some(temp) = self.temp.take() {
            let result = tokio::task::spawn_blocking(move || remove_with_retry(temp)).await;
            if let Ok(Err(e)) | Err(e) = result.map_err(|e| std::io::Error::other(e.to_string())) {
                tracing::warn!(error = %e, "could not remove a partial file; it will be cleaned up later");
            }
        }
    }
}

/// Deletes a temp file, retrying transient Windows locks (antivirus,
/// indexers: errors 5 and 32) a few times. `remove_file` is called directly
/// so the raw OS error code is visible (`TempPath::close` wraps it).
fn remove_with_retry(temp: TempPath) -> std::io::Result<()> {
    let path = temp.to_path_buf();
    let mut last = None;
    for delay_ms in [0u64, 50, 100, 200] {
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {
                last = None;
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                last = None;
                break;
            }
            Err(e) if cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32)) => last = Some(e),
            Err(e) => {
                last = Some(e);
                break;
            }
        }
    }
    match last {
        None => {
            // Already gone: disarm the temp path so its Drop does nothing.
            let _ = temp.keep();
            Ok(())
        }
        // Dropping the temp path makes one last best-effort attempt.
        Some(e) => Err(e),
    }
}

impl<O: FinalizeOps> FileSink for ReceiveDir<O> {
    type File = IncomingFsFile<O>;

    async fn begin(&self, name: &SafeFileName, _size: u64) -> Result<Self::File, StorageError> {
        let dir = self.dir.clone();
        let (file, temp) = tokio::task::spawn_blocking(move || {
            tempfile::Builder::new()
                .prefix(TEMP_PREFIX)
                .suffix(TEMP_SUFFIX)
                .rand_bytes(TEMP_RANDOM_LEN)
                .tempfile_in(&dir)
                .map(tempfile::NamedTempFile::into_parts)
        })
        .await
        .map_err(|e| blocking_err(&e))?
        .map_err(|e| StorageError::from_io(&e))?;
        Ok(IncomingFsFile {
            file: Some(tokio::fs::File::from_std(file)),
            temp: Some(temp),
            dir: self.dir.clone(),
            name: name.clone(),
            ops: Arc::clone(&self.ops),
            next_suffix: Arc::clone(&self.next_suffix),
        })
    }

    async fn sync_dir(&self) -> Result<(), StorageError> {
        #[cfg(unix)]
        {
            let dir = self.dir.clone();
            tokio::task::spawn_blocking(move || {
                std::fs::File::open(&dir).and_then(|d| d.sync_all())
            })
            .await
            .map_err(|e| blocking_err(&e))?
            .map_err(|e| StorageError::from_io(&e))?;
        }
        // Windows: NTFS journals directory metadata; there is no portable
        // directory fsync.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    async fn receive(rd: &ReceiveDir, name: &str, data: &[u8]) -> String {
        let mut f = rd
            .begin(&SafeFileName::sanitize(name), data.len() as u64)
            .await
            .unwrap();
        f.write(data).await.unwrap();
        f.commit().await.unwrap().saved_as
    }

    #[tokio::test]
    async fn persist_never_overwrites_existing() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("photo.jpg"), b"original").unwrap();
        let rd = ReceiveDir::open(d.path()).unwrap();
        assert_eq!(receive(&rd, "photo.jpg", b"new").await, "photo (1).jpg");
        assert_eq!(receive(&rd, "photo.jpg", b"newer").await, "photo (2).jpg");
        assert_eq!(
            std::fs::read(d.path().join("photo.jpg")).unwrap(),
            b"original"
        );
        assert_eq!(
            std::fs::read(d.path().join("photo (1).jpg")).unwrap(),
            b"new"
        );
        assert_eq!(
            names(d.path()),
            vec!["photo (1).jpg", "photo (2).jpg", "photo.jpg"]
        );
    }

    #[tokio::test]
    async fn case_variant_collision_keeps_original() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("probe-a"), b"x").unwrap();
        let case_insensitive = d.path().join("PROBE-A").exists();
        std::fs::remove_file(d.path().join("probe-a")).unwrap();
        std::fs::write(d.path().join("Photo.jpg"), b"original").unwrap();
        let rd = ReceiveDir::open(d.path()).unwrap();
        let saved = receive(&rd, "photo.jpg", b"new").await;
        if case_insensitive {
            assert_eq!(saved, "photo (1).jpg");
        } else {
            assert_eq!(saved, "photo.jpg");
        }
        assert_eq!(
            std::fs::read(d.path().join("Photo.jpg")).unwrap(),
            b"original"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_at_final_name_not_followed() {
        let d = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("victim.txt");
        std::fs::write(&target, b"victim").unwrap();
        std::os::unix::fs::symlink(&target, d.path().join("evil.txt")).unwrap();
        let rd = ReceiveDir::open(d.path()).unwrap();
        assert_eq!(receive(&rd, "evil.txt", b"payload").await, "evil (1).txt");
        assert_eq!(std::fs::read(&target).unwrap(), b"victim");
        assert!(
            std::fs::symlink_metadata(d.path().join("evil.txt"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dangling_symlink_not_replaced() {
        let d = tempfile::tempdir().unwrap();
        let missing = d.path().join("nowhere");
        std::os::unix::fs::symlink(&missing, d.path().join("x.bin")).unwrap();
        let rd = ReceiveDir::open(d.path()).unwrap();
        assert_eq!(receive(&rd, "x.bin", b"data").await, "x (1).bin");
        assert!(!missing.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn received_file_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let rd = ReceiveDir::open(d.path()).unwrap();
        let saved = receive(&rd, "m.txt", b"m").await;
        let mode = std::fs::metadata(d.path().join(saved))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[tokio::test]
    async fn duplicate_names_probe_from_hint() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[derive(Default)]
        struct Counting {
            inner: StdFinalizeOps,
            calls: AtomicUsize,
        }
        impl FinalizeOps for Counting {
            fn persist_noclobber(
                &self,
                temp: TempPath,
                dest: &Path,
            ) -> Result<(), tempfile::PathPersistError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.inner.persist_noclobber(temp, dest)
            }
            fn persist_overwrite(
                &self,
                temp: TempPath,
                dest: &Path,
            ) -> Result<(), tempfile::PathPersistError> {
                self.inner.persist_overwrite(temp, dest)
            }
            fn create_placeholder(
                &self,
                dest: &Path,
            ) -> std::io::Result<super::super::finalize::FileId> {
                self.inner.create_placeholder(dest)
            }
            fn regular_file_id(
                &self,
                path: &Path,
            ) -> Option<(super::super::finalize::FileId, u64)> {
                self.inner.regular_file_id(path)
            }
            fn exists(&self, path: &Path) -> bool {
                self.inner.exists(path)
            }
            fn remove(&self, path: &Path) -> std::io::Result<()> {
                self.inner.remove(path)
            }
            fn sleep(&self, d: Duration) {
                self.inner.sleep(d);
            }
            fn random_suffix(&self) -> String {
                self.inner.random_suffix()
            }
        }

        let d = tempfile::tempdir().unwrap();
        let rd = ReceiveDir::with_ops(d.path(), Counting::default()).unwrap();
        for _ in 0..100 {
            receive_with(&rd, "same.txt", b"x").await;
        }
        assert_eq!(names(d.path()).len(), 100);
        // Without the hint this would be 100 * 101 / 2 = 5050 attempts.
        assert!(rd.ops.calls.load(Ordering::SeqCst) <= 200);
    }

    async fn receive_with<O: FinalizeOps>(rd: &ReceiveDir<O>, name: &str, data: &[u8]) -> String {
        let mut f = rd
            .begin(&SafeFileName::sanitize(name), data.len() as u64)
            .await
            .unwrap();
        f.write(data).await.unwrap();
        f.commit().await.unwrap().saved_as
    }

    #[tokio::test]
    async fn temp_removed_on_drop() {
        let d = tempfile::tempdir().unwrap();
        let rd = ReceiveDir::open(d.path()).unwrap();
        let mut f = rd.begin(&SafeFileName::sanitize("t.bin"), 3).await.unwrap();
        f.write(b"abc").await.unwrap();
        assert_eq!(names(d.path()).len(), 1);
        assert!(names(d.path())[0].starts_with(".dropumi-"));
        drop(f);
        assert!(names(d.path()).is_empty());
    }

    #[tokio::test]
    async fn discard_removes_temp() {
        let d = tempfile::tempdir().unwrap();
        let rd = ReceiveDir::open(d.path()).unwrap();
        let mut f = rd.begin(&SafeFileName::sanitize("t.bin"), 3).await.unwrap();
        f.write(b"abc").await.unwrap();
        f.discard().await;
        assert!(names(d.path()).is_empty());
        rd.sync_dir().await.unwrap();
    }

    #[test]
    fn dest_must_be_existing_dir() {
        let d = tempfile::tempdir().unwrap();
        assert!(ReceiveDir::open(&d.path().join("missing")).is_err());
        let file = d.path().join("f");
        std::fs::write(&file, b"").unwrap();
        assert!(ReceiveDir::open(&file).is_err());
    }
}
