// SPDX-License-Identifier: MPL-2.0
//! Atomic, no-overwrite finalization of received files.
//!
//! A synced temporary file is moved to `name`, `name (1)`, `name (2)`…
//! with an atomic rename that fails instead of replacing anything
//! (`renameat2(RENAME_NOREPLACE)` / `renamex_np(RENAME_EXCL)` on Linux and
//! Apple systems, `MoveFileExW` without `REPLACE_EXISTING` on Windows, via
//! `tempfile`). Symlinks at the destination are never followed or replaced.
//!
//! - **Windows:** antivirus and indexers briefly lock files; access-denied
//!   (5) and sharing-violation (32) errors are retried with backoff.
//! - **Unix filesystems without no-replace rename or hard links**
//!   (exFAT, vfat): an empty placeholder is created with `O_CREAT|O_EXCL`
//!   and then replaced; if that replace fails, the placeholder is removed
//!   only if it is still ours (same device and inode, length 0).
//!
//! `std::fs::rename` (which overwrites) is never used here.

use std::io;
use std::path::Path;
use std::time::Duration;

use dropumi_domain::filename::SafeFileName;
use tempfile::{PathPersistError, TempPath};

/// Highest collision suffix tried before a random name.
pub const MAX_COLLISION_SUFFIX: u32 = 9_999;
/// Most names probed by one finalization before falling back to a random
/// name (bounds the work a peer can cause with duplicate names).
pub const MAX_PROBES_PER_FILE: u32 = 256;
/// Backoff schedule for transient Windows errors.
pub const WINDOWS_RETRY_MS: [u64; 5] = [20, 40, 80, 160, 320];

/// Identity of a placeholder file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileId {
    /// Device number.
    pub dev: u64,
    /// Inode number.
    pub ino: u64,
}

/// File operations used by finalization (injectable for tests).
pub trait FinalizeOps: Send + Sync + 'static {
    /// Moves `temp` to `dest` unless `dest` exists.
    ///
    /// # Errors
    /// The I/O error, returning ownership of the temp path.
    fn persist_noclobber(&self, temp: TempPath, dest: &Path) -> Result<(), PathPersistError>;

    /// Moves `temp` to `dest`, replacing it (used only over our own
    /// placeholder).
    ///
    /// # Errors
    /// The I/O error, returning ownership of the temp path.
    fn persist_overwrite(&self, temp: TempPath, dest: &Path) -> Result<(), PathPersistError>;

    /// Creates an empty file at `dest` that must not exist yet.
    ///
    /// # Errors
    /// `AlreadyExists` if something is there.
    fn create_placeholder(&self, dest: &Path) -> io::Result<FileId>;

    /// Identity and length of a regular file at `path` (not following
    /// symlinks); `None` if absent or not a regular file.
    fn regular_file_id(&self, path: &Path) -> Option<(FileId, u64)>;

    /// Whether anything (file, directory, symlink) exists at `path`.
    fn exists(&self, path: &Path) -> bool;

    /// Removes a file.
    ///
    /// # Errors
    /// The I/O error.
    fn remove(&self, path: &Path) -> io::Result<()>;

    /// Sleeps (blocking).
    fn sleep(&self, d: Duration);

    /// Random suffix for the last-resort name.
    fn random_suffix(&self) -> String;
}

/// The real operations.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdFinalizeOps;

impl FinalizeOps for StdFinalizeOps {
    fn persist_noclobber(&self, temp: TempPath, dest: &Path) -> Result<(), PathPersistError> {
        temp.persist_noclobber(dest)
    }

    fn persist_overwrite(&self, temp: TempPath, dest: &Path) -> Result<(), PathPersistError> {
        temp.persist(dest)
    }

    fn create_placeholder(&self, dest: &Path) -> io::Result<FileId> {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let file = opts.open(dest)?;
        let meta = file.metadata()?;
        Ok(file_id(&meta))
    }

    fn regular_file_id(&self, path: &Path) -> Option<(FileId, u64)> {
        let meta = std::fs::symlink_metadata(path).ok()?;
        meta.file_type()
            .is_file()
            .then(|| (file_id(&meta), meta.len()))
    }

    fn exists(&self, path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }

    fn random_suffix(&self) -> String {
        let mut b = [0u8; 6];
        if dropumi_protocol::random::fill(&mut b).is_err() {
            return "000000000000".into();
        }
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}

#[cfg(unix)]
fn file_id(meta: &std::fs::Metadata) -> FileId {
    use std::os::unix::fs::MetadataExt;
    FileId {
        dev: meta.dev(),
        ino: meta.ino(),
    }
}

#[cfg(not(unix))]
fn file_id(_meta: &std::fs::Metadata) -> FileId {
    FileId { dev: 0, ino: 0 }
}

/// Whether an error means "this filesystem cannot rename without
/// replacing" (Unix only): ENOTSUP, EOPNOTSUPP or EPERM (EPERM comes from
/// hard links on vfat/exFAT).
#[cfg(unix)]
fn needs_placeholder(e: &io::Error) -> bool {
    use rustix::io::Errno;
    // ENOTSUP and EOPNOTSUPP are the same value on Linux but differ on
    // Apple platforms, so compare values rather than match patterns.
    e.kind() == io::ErrorKind::Unsupported
        || Errno::from_io_error(e)
            .is_some_and(|n| n == Errno::NOTSUP || n == Errno::OPNOTSUPP || n == Errno::PERM)
}

#[cfg(not(unix))]
fn needs_placeholder(_e: &io::Error) -> bool {
    false
}

fn is_transient_windows(e: &io::Error) -> bool {
    cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32))
}

/// Why finalization failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FinalizeError {
    /// Every candidate name failed.
    #[error("cannot finalize received file: {0}")]
    Io(io::Error),
}

/// What happened to one candidate name.
enum Attempt {
    Done,
    Taken(TempPath),
    Failed(io::Error),
}

/// Moves `temp` into `dir` under `name` (or a collision-suffixed variant,
/// starting at suffix `start`) without overwriting anything. Returns the
/// final file name and the suffix used (`None` for the random fallback).
///
/// At most [`MAX_PROBES_PER_FILE`] names are probed; after that, or past
/// suffix 9,999, a random `name-<12 hex>.ext` name is used.
///
/// # Errors
/// [`FinalizeError`]; the temporary file is deleted (dropped) on failure.
pub fn finalize<O: FinalizeOps>(
    ops: &O,
    temp: TempPath,
    dir: &Path,
    name: &SafeFileName,
    start: u32,
) -> Result<(String, Option<u32>), FinalizeError> {
    let mut temp = temp;
    let end = start
        .saturating_add(MAX_PROBES_PER_FILE)
        .min(MAX_COLLISION_SUFFIX.saturating_add(1));
    for n in start..end {
        let candidate = name.with_collision_suffix(n);
        let dest = dir.join(candidate.as_str());
        match attempt(ops, temp, &dest) {
            Attempt::Done => return Ok((candidate.as_str().to_owned(), Some(n))),
            Attempt::Taken(t) => temp = t,
            Attempt::Failed(e) => return Err(FinalizeError::Io(e)),
        }
    }
    // Last resort: a random name (the tag is never truncated away).
    let random = name.with_tag(&ops.random_suffix());
    let dest = dir.join(random.as_str());
    match attempt(ops, temp, &dest) {
        Attempt::Done => Ok((random.as_str().to_owned(), None)),
        Attempt::Taken(_) => Err(FinalizeError::Io(io::Error::from(
            io::ErrorKind::AlreadyExists,
        ))),
        Attempt::Failed(e) => Err(FinalizeError::Io(e)),
    }
}

fn attempt<O: FinalizeOps>(ops: &O, temp: TempPath, dest: &Path) -> Attempt {
    let mut temp = temp;
    let mut retries = WINDOWS_RETRY_MS.iter();
    loop {
        let err = match ops.persist_noclobber(temp, dest) {
            Ok(()) => return Attempt::Done,
            Err(PathPersistError { error, path }) => {
                temp = path;
                error
            }
        };
        if err.kind() == io::ErrorKind::AlreadyExists {
            return Attempt::Taken(temp);
        }
        if is_transient_windows(&err) {
            if let Some(ms) = retries.next() {
                ops.sleep(Duration::from_millis(*ms));
                continue;
            }
            // Still denied: a directory or locked file may occupy the name.
            if ops.exists(dest) {
                return Attempt::Taken(temp);
            }
            return Attempt::Failed(err);
        }
        if needs_placeholder(&err) {
            return via_placeholder(ops, temp, dest);
        }
        return Attempt::Failed(err);
    }
}

/// Reserves `dest` with an exclusive empty placeholder, then replaces it.
fn via_placeholder<O: FinalizeOps>(ops: &O, temp: TempPath, dest: &Path) -> Attempt {
    let id = match ops.create_placeholder(dest) {
        Ok(id) => id,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Attempt::Taken(temp),
        Err(e) => return Attempt::Failed(e),
    };
    match ops.persist_overwrite(temp, dest) {
        Ok(()) => Attempt::Done,
        Err(PathPersistError { error, path }) => {
            // Remove the placeholder only if it is still ours and empty.
            if ops.regular_file_id(dest) == Some((id, 0)) {
                let _ = ops.remove(dest);
            }
            drop(path);
            Attempt::Failed(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use super::*;

    /// Scripted operations: queued errors for persist calls, a fake
    /// directory of existing names, and recorded sleeps.
    #[derive(Default)]
    struct ScriptedOps {
        noclobber: Mutex<VecDeque<io::Error>>,
        overwrite: Mutex<VecDeque<io::Error>>,
        existing: Mutex<Vec<PathBuf>>,
        placeholders: Mutex<Vec<PathBuf>>,
        sleeps: Mutex<Vec<u64>>,
    }

    impl FinalizeOps for ScriptedOps {
        fn persist_noclobber(&self, temp: TempPath, dest: &Path) -> Result<(), PathPersistError> {
            if let Some(error) = self.noclobber.lock().unwrap().pop_front() {
                return Err(PathPersistError { error, path: temp });
            }
            if self.existing.lock().unwrap().iter().any(|p| p == dest) {
                return Err(PathPersistError {
                    error: io::Error::from(io::ErrorKind::AlreadyExists),
                    path: temp,
                });
            }
            self.existing.lock().unwrap().push(dest.to_path_buf());
            let _ = temp.keep();
            Ok(())
        }

        fn persist_overwrite(&self, temp: TempPath, dest: &Path) -> Result<(), PathPersistError> {
            if let Some(error) = self.overwrite.lock().unwrap().pop_front() {
                return Err(PathPersistError { error, path: temp });
            }
            self.existing.lock().unwrap().push(dest.to_path_buf());
            let _ = temp.keep();
            Ok(())
        }

        fn create_placeholder(&self, dest: &Path) -> io::Result<FileId> {
            let mut ex = self.existing.lock().unwrap();
            if ex.iter().any(|p| p == dest) {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists));
            }
            ex.push(dest.to_path_buf());
            self.placeholders.lock().unwrap().push(dest.to_path_buf());
            Ok(FileId { dev: 1, ino: 42 })
        }

        fn regular_file_id(&self, path: &Path) -> Option<(FileId, u64)> {
            self.placeholders
                .lock()
                .unwrap()
                .iter()
                .any(|p| p == path)
                .then_some((FileId { dev: 1, ino: 42 }, 0))
        }

        fn exists(&self, path: &Path) -> bool {
            self.existing.lock().unwrap().iter().any(|p| p == path)
        }

        fn remove(&self, path: &Path) -> io::Result<()> {
            self.existing.lock().unwrap().retain(|p| p != path);
            self.placeholders.lock().unwrap().retain(|p| p != path);
            Ok(())
        }

        fn sleep(&self, d: Duration) {
            self.sleeps
                .lock()
                .unwrap()
                .push(u64::try_from(d.as_millis()).unwrap());
        }

        fn random_suffix(&self) -> String {
            "abcdef012345".into()
        }
    }

    fn temp() -> (tempfile::TempDir, TempPath) {
        let dir = tempfile::tempdir().unwrap();
        let t = tempfile::NamedTempFile::new_in(dir.path())
            .unwrap()
            .into_temp_path();
        (dir, t)
    }

    #[test]
    fn collision_suffix_sequence() {
        let ops = ScriptedOps::default();
        let dir = Path::new("/recv");
        ops.existing
            .lock()
            .unwrap()
            .extend([dir.join("photo.jpg"), dir.join("photo (1).jpg")]);
        let (_d, t) = temp();
        let (name, n) = finalize(&ops, t, dir, &SafeFileName::sanitize("photo.jpg"), 0).unwrap();
        assert_eq!(name, "photo (2).jpg");
        assert_eq!(n, Some(2));
    }

    #[cfg(unix)]
    #[test]
    fn placeholder_fallback_never_overwrites() {
        let ops = ScriptedOps::default();
        let dir = Path::new("/exfat");
        ops.existing.lock().unwrap().push(dir.join("a.txt"));
        // Every no-replace rename fails as on exFAT.
        for _ in 0..3 {
            ops.noclobber
                .lock()
                .unwrap()
                .push_back(io::Error::from_raw_os_error(
                    rustix::io::Errno::NOTSUP.raw_os_error(),
                ));
        }
        let (_d, t) = temp();
        let (name, _) = finalize(&ops, t, dir, &SafeFileName::sanitize("a.txt"), 0).unwrap();
        assert_eq!(name, "a (1).txt", "existing a.txt is never replaced");
    }

    #[cfg(unix)]
    #[test]
    fn placeholder_removed_when_second_persist_fails() {
        let ops = ScriptedOps::default();
        let dir = Path::new("/exfat");
        ops.noclobber
            .lock()
            .unwrap()
            .push_back(io::Error::from_raw_os_error(
                rustix::io::Errno::PERM.raw_os_error(),
            ));
        ops.overwrite
            .lock()
            .unwrap()
            .push_back(io::Error::other("device removed"));
        let (_d, t) = temp();
        assert!(finalize(&ops, t, dir, &SafeFileName::sanitize("b.txt"), 0).is_err());
        assert!(
            !ops.exists(&dir.join("b.txt")),
            "our placeholder was removed"
        );
    }

    #[cfg(windows)]
    #[test]
    fn transient_access_denied_retried() {
        let ops = ScriptedOps::default();
        ops.noclobber.lock().unwrap().extend([
            io::Error::from_raw_os_error(32),
            io::Error::from_raw_os_error(5),
        ]);
        let (_d, t) = temp();
        let (name, n) = finalize(
            &ops,
            t,
            Path::new("C:\\r"),
            &SafeFileName::sanitize("c.txt"),
            0,
        )
        .unwrap();
        assert_eq!(name, "c.txt");
        assert_eq!(n, Some(0));
        assert_eq!(*ops.sleeps.lock().unwrap(), vec![20, 40]);
    }

    #[cfg(windows)]
    #[test]
    fn windows_persistent_denied_fails_without_placeholder() {
        let ops = ScriptedOps::default();
        for _ in 0..6 {
            ops.noclobber
                .lock()
                .unwrap()
                .push_back(io::Error::from_raw_os_error(5));
        }
        let (_d, t) = temp();
        assert!(
            finalize(
                &ops,
                t,
                Path::new("C:\\r"),
                &SafeFileName::sanitize("d.txt"),
                0,
            )
            .is_err()
        );
        assert!(ops.placeholders.lock().unwrap().is_empty());
        assert!(!ops.exists(Path::new("C:\\r\\d.txt")));
    }

    #[test]
    fn probe_cap_switches_to_random_name() {
        let ops = ScriptedOps::default();
        let dir = Path::new("/recv");
        let name = SafeFileName::sanitize("photo.jpg");
        ops.existing
            .lock()
            .unwrap()
            .extend((0..400).map(|n| dir.join(name.with_collision_suffix(n).as_str())));
        let (_d, t) = temp();
        let (saved, n) = finalize(&ops, t, dir, &name, 0).unwrap();
        assert_eq!(saved, "photo-abcdef012345.jpg");
        assert_eq!(n, None);
        // Starting from a hint skips the known-taken names.
        let (_d2, t2) = temp();
        let (saved, n) = finalize(&ops, t2, dir, &name, 400).unwrap();
        assert_eq!(saved, "photo (400).jpg");
        assert_eq!(n, Some(400));
    }

    #[test]
    fn other_errors_fail_immediately() {
        let ops = ScriptedOps::default();
        ops.noclobber
            .lock()
            .unwrap()
            .push_back(io::Error::from(io::ErrorKind::StorageFull));
        let (_d, t) = temp();
        assert!(
            finalize(
                &ops,
                t,
                Path::new("/r"),
                &SafeFileName::sanitize("e.txt"),
                0
            )
            .is_err()
        );
    }
}
