// SPDX-License-Identifier: MPL-2.0
//! Cross-process advisory lock for read-modify-write on config files.

use std::fs::{File, OpenOptions};
use std::path::Path;

use dropumi_application::error::StoreError;

use crate::paths::io_err;

/// Lock file name.
pub const LOCK_FILE: &str = ".lock";

/// Holds the exclusive lock until dropped.
#[derive(Debug)]
pub struct ConfigLock {
    file: File,
}

impl ConfigLock {
    /// Acquires the exclusive lock on `<dir>/.lock` (blocking).
    ///
    /// # Errors
    /// [`StoreError::Io`] on failure.
    pub fn acquire(dir: &Path) -> Result<Self, StoreError> {
        let path = dir.join(LOCK_FILE);
        let mut opts = OpenOptions::new();
        opts.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let file = opts.open(&path).map_err(|e| io_err(&path, &e))?;
        lock_exclusive(&file).map_err(|e| io_err(&path, &e))?;
        Ok(Self { file })
    }
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        let _ = unlock(&self.file);
    }
}

/// `File::lock` is `flock(LOCK_EX)` on Linux, but std does not implement it
/// on Android ("lock() not supported"), so Android calls `flock` directly:
/// the same lock, the same semantics.
#[cfg(not(target_os = "android"))]
fn lock_exclusive(file: &File) -> std::io::Result<()> {
    file.lock()
}

#[cfg(not(target_os = "android"))]
fn unlock(file: &File) -> std::io::Result<()> {
    file.unlock()
}

#[cfg(target_os = "android")]
fn lock_exclusive(file: &File) -> std::io::Result<()> {
    flock_exclusive(file)
}

#[cfg(target_os = "android")]
fn unlock(file: &File) -> std::io::Result<()> {
    flock_unlock(file)
}

/// The Android lock, also built for the host tests below.
#[cfg(any(target_os = "android", all(test, unix)))]
fn flock_exclusive(file: &File) -> std::io::Result<()> {
    rustix::fs::flock(file, rustix::fs::FlockOperation::LockExclusive).map_err(Into::into)
}

#[cfg(any(target_os = "android", all(test, unix)))]
fn flock_unlock(file: &File) -> std::io::Result<()> {
    rustix::fs::flock(file, rustix::fs::FlockOperation::Unlock).map_err(Into::into)
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    fn open(path: &Path) -> File {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .unwrap()
    }

    /// The Android path (flock) excludes a second holder, as `File::lock`
    /// does, and releases it on unlock.
    #[test]
    fn flock_lock_excludes_a_second_handle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOCK_FILE);
        let (first, second) = (open(&path), open(&path));
        flock_exclusive(&first).unwrap();
        let busy = rustix::fs::flock(
            &second,
            rustix::fs::FlockOperation::NonBlockingLockExclusive,
        );
        assert_eq!(busy, Err(rustix::io::Errno::WOULDBLOCK));
        flock_unlock(&first).unwrap();
        rustix::fs::flock(
            &second,
            rustix::fs::FlockOperation::NonBlockingLockExclusive,
        )
        .unwrap();
    }

    /// `flock` and `File::lock` are the same lock where std has it, so a
    /// process on either path excludes the other.
    #[cfg(not(target_os = "android"))]
    #[test]
    fn flock_and_std_lock_exclude_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOCK_FILE);
        let (first, second) = (open(&path), open(&path));
        first.lock().unwrap();
        let busy = rustix::fs::flock(
            &second,
            rustix::fs::FlockOperation::NonBlockingLockExclusive,
        );
        assert_eq!(busy, Err(rustix::io::Errno::WOULDBLOCK));
        first.unlock().unwrap();
    }
}
