// SPDX-License-Identifier: MPL-2.0
//! Configuration and receive directories.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use dropumi_application::error::StoreError;

/// Application directory name.
pub const APP_DIR: &str = "dropumi";

/// The configuration directory: an explicit path (from `--config-dir` or
/// `DROPUMI_CONFIG_DIR`), otherwise the OS local config directory
/// (`%LOCALAPPDATA%` on Windows, so keys never roam).
///
/// # Errors
/// [`StoreError::Io`] if no default location exists.
pub fn config_dir(explicit: Option<&Path>) -> Result<PathBuf, StoreError> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    dirs::config_local_dir()
        .map(|d| d.join(APP_DIR))
        .ok_or_else(|| StoreError::Io("no configuration directory for this user".into()))
}

/// Creates `dir` (and parents) if missing. A directory created here is
/// private (0700) on Unix. Existing directories are not modified; their
/// permissions are checked separately.
///
/// # Errors
/// [`StoreError::Io`] on failure.
pub fn ensure_private_dir(dir: &Path) -> Result<(), StoreError> {
    if dir.is_dir() {
        return Ok(());
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(dir)
        .map_err(|e| StoreError::Io(format!("cannot create {}: {e}", dir.display())))
}

/// The default receive directory: the user's Downloads folder, if it
/// exists. It is never created implicitly.
#[must_use]
pub fn default_receive_dir() -> Option<PathBuf> {
    dirs::download_dir().filter(|d| d.is_dir()).or_else(|| {
        dirs::home_dir()
            .map(|h| h.join("Downloads"))
            .filter(|d| d.is_dir())
    })
}

/// Maps an I/O error with its path for local display.
pub(crate) fn io_err(path: &Path, e: &io::Error) -> StoreError {
    StoreError::Io(format!("{}: {e}", path.display()))
}
