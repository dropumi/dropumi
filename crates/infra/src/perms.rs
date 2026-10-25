// SPDX-License-Identifier: MPL-2.0
//! Permission checks for configuration files.
//!
//! On Unix, the configuration directory and every file in it must belong to
//! the current user and must not be writable by group or others (another
//! local user could otherwise inject trust); the identity key must not be
//! readable by anyone else. On Windows, ACLs are not inspected: the default
//! `%LOCALAPPDATA%` location is protected by the profile's ACL, and a
//! warning is shown when the configuration lives elsewhere.

use std::path::Path;

use dropumi_application::error::StoreError;

/// Bits that must not be set on the config directory and data files.
pub const NO_GROUP_OTHER_WRITE: u32 = 0o022;
/// Bits that must not be set on the identity key.
pub const NO_GROUP_OTHER_ACCESS: u32 = 0o077;

/// Checks the configuration directory.
///
/// # Errors
/// [`StoreError::InsecurePermissions`] for a foreign owner or group/other
/// write access.
pub fn check_config_dir(path: &Path) -> Result<(), StoreError> {
    check(path, NO_GROUP_OTHER_WRITE, "chmod 700")
}

/// Checks a private file against `forbidden` mode bits.
///
/// # Errors
/// [`StoreError::InsecurePermissions`] for a foreign owner or forbidden bits.
pub fn check_private_file(path: &Path, forbidden: u32) -> Result<(), StoreError> {
    check(path, forbidden, "chmod 600")
}

#[cfg(unix)]
fn check(path: &Path, forbidden: u32, fix: &str) -> Result<(), StoreError> {
    use std::os::unix::fs::MetadataExt;
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(crate::paths::io_err(path, &e)),
    };
    let euid = rustix::process::geteuid().as_raw();
    if meta.uid() != euid {
        return Err(StoreError::InsecurePermissions {
            path: path.display().to_string(),
            hint: "owned by another user; use a directory you own".into(),
        });
    }
    if meta.file_type().is_symlink() {
        return Err(StoreError::InsecurePermissions {
            path: path.display().to_string(),
            hint: "is a symbolic link; use a regular file or directory".into(),
        });
    }
    if meta.mode() & forbidden != 0 {
        return Err(StoreError::InsecurePermissions {
            path: path.display().to_string(),
            hint: format!("{fix} '{}'", path.display()),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn check(_path: &Path, _forbidden: u32, _fix: &str) -> Result<(), StoreError> {
    Ok(())
}

/// Whether `candidate` lies inside `base` (both canonicalized by the caller).
#[must_use]
pub fn is_within(base: &Path, candidate: &Path) -> bool {
    candidate.starts_with(base)
}

/// A warning when the configuration directory is not under the per-user
/// local application data directory (Windows only; elsewhere `None`).
#[must_use]
pub fn config_dir_warning(config: &Path) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let base = dirs::data_local_dir()?;
    let base = base.canonicalize().unwrap_or(base);
    let cfg = config
        .canonicalize()
        .unwrap_or_else(|_| config.to_path_buf());
    (!is_within(&base, &cfg)).then(|| {
        format!(
            "configuration directory {} is outside your local application data; \
             its access control is inherited from the parent folder",
            cfg.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn config_dir_outside_localappdata_detected() {
        let base = PathBuf::from("/users/a/appdata/local");
        assert!(is_within(&base, &base.join("dropumi")));
        assert!(!is_within(&base, Path::new("/tmp/dropumi")));
        assert!(!is_within(&base, Path::new("/users/a/appdata/localx")));
    }

    #[cfg(unix)]
    #[test]
    fn config_dir_rejects_world_writable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        check_config_dir(dir.path()).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(matches!(
            check_config_dir(dir.path()),
            Err(StoreError::InsecurePermissions { .. })
        ));
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        check_config_dir(dir.path()).unwrap();
    }
}
