// SPDX-License-Identifier: MPL-2.0
//! Atomic file replacement.

use std::io::Write;
use std::path::Path;
use std::time::Duration;

use dropumi_application::error::StoreError;

use crate::paths::io_err;

/// Writes `data` to `<dir>/<name>` atomically: a 0600 temp file in the same
/// directory, fsync, rename over the target, then a directory fsync (Unix).
///
/// # Errors
/// [`StoreError::Io`] on a failure before the rename (the temp file is
/// removed and the target is unchanged). [`StoreError::NotDurable`] if the
/// directory fsync fails after the rename: the new content is in place,
/// but its durability was not confirmed.
pub fn write_atomic(dir: &Path, name: &str, data: &[u8]) -> Result<(), StoreError> {
    let target = dir.join(name);
    let mut tmp = tempfile::Builder::new()
        .prefix(".dropumi-cfg-")
        .tempfile_in(dir)
        .map_err(|e| io_err(dir, &e))?;
    tmp.write_all(data).map_err(|e| io_err(&target, &e))?;
    tmp.as_file().sync_all().map_err(|e| io_err(&target, &e))?;
    let mut tmp = tmp.into_temp_path();
    let mut attempts = 0;
    loop {
        match tmp.persist(&target) {
            Ok(()) => break,
            Err(e)
                if cfg!(windows)
                    && attempts < 5
                    && matches!(e.error.raw_os_error(), Some(5 | 32)) =>
            {
                attempts += 1;
                tmp = e.path;
                std::thread::sleep(Duration::from_millis(20 << attempts));
            }
            Err(e) => return Err(io_err(&target, &e.error)),
        }
    }
    sync_dir_after_commit(dir)
}

/// Fsyncs `dir` so a rename or a new entry in it is durable (Unix). Call
/// it only after the commit point (the rename or link that made the new
/// content visible): a failure means the change is in place.
/// Windows has no portable directory fsync; NTFS journals directory
/// metadata.
///
/// # Errors
/// [`StoreError::NotDurable`] if the directory cannot be opened or synced.
pub(crate) fn sync_dir_after_commit(dir: &Path) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|e| StoreError::NotDurable(format!("{}: {e}", dir.display())))?;
    }
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_replace_no_temp_left() {
        let d = tempfile::tempdir().unwrap();
        write_atomic(d.path(), "a.json", b"one").unwrap();
        write_atomic(d.path(), "a.json", b"two").unwrap();
        assert_eq!(std::fs::read(d.path().join("a.json")).unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(d.path().join("a.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    /// A directory that cannot be opened for the fsync (write and search,
    /// no read) is reported after the rename as `NotDurable`: the new
    /// content is in place.
    #[cfg(unix)]
    #[test]
    fn dir_sync_failure_reported() {
        use std::os::unix::fs::PermissionsExt;
        if rustix::process::geteuid().is_root() {
            // Skipped: root bypasses directory permissions, so the open
            // cannot be made to fail this way.
            return;
        }
        let d = tempfile::tempdir().unwrap();
        write_atomic(d.path(), "a.json", b"one").unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o300)).unwrap();
        let result = write_atomic(d.path(), "a.json", b"two");
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            matches!(result, Err(StoreError::NotDurable(_))),
            "{result:?}"
        );
        // The error comes after the rename: the new content is in place.
        assert_eq!(std::fs::read(d.path().join("a.json")).unwrap(), b"two");
    }

    /// A failure before the rename (here: the temp file cannot be created
    /// in a read-only directory) stays `Io`, and nothing changes.
    #[cfg(unix)]
    #[test]
    fn failure_before_rename_is_io() {
        use std::os::unix::fs::PermissionsExt;
        if rustix::process::geteuid().is_root() {
            // Skipped: root bypasses directory permissions.
            return;
        }
        let d = tempfile::tempdir().unwrap();
        write_atomic(d.path(), "a.json", b"one").unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = write_atomic(d.path(), "a.json", b"two");
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(result, Err(StoreError::Io(_))), "{result:?}");
        assert_eq!(std::fs::read(d.path().join("a.json")).unwrap(), b"one");
    }
}
