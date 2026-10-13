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
/// [`StoreError::Io`] on failure (the temp file is removed).
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
    #[cfg(unix)]
    {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
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
}
