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
        file.lock().map_err(|e| io_err(&path, &e))?;
        Ok(Self { file })
    }
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
