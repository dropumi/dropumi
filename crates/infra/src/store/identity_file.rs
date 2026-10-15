// SPDX-License-Identifier: MPL-2.0
//! The identity key file and the device name file.

use std::io::Write;
use std::path::{Path, PathBuf};

use dropumi_application::error::StoreError;
use dropumi_application::identity::LocalIdentity;
use dropumi_application::ports::IdentityStore;
use dropumi_domain::device::DeviceName;
use dropumi_protocol::identity::StaticSecret;
use dropumi_protocol::random;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{ConfigLock, sync_dir, write_atomic};
use crate::paths::{ensure_private_dir, io_err};
use crate::perms::{
    NO_GROUP_OTHER_ACCESS, NO_GROUP_OTHER_WRITE, check_config_dir, check_private_file,
};

/// Identity key file name.
pub const KEY_FILE: &str = "identity.key";
/// Device file name.
pub const DEVICE_FILE: &str = "device.json";
/// Key file magic.
pub const KEY_MAGIC: &[u8; 8] = b"DRPMKEY1";
const KEY_FILE_LEN: usize = 8 + 32;

#[derive(Debug, Serialize, Deserialize)]
struct DeviceJson {
    version: u32,
    name: String,
}

/// Identity and device name stored as files in the configuration directory.
#[derive(Debug, Clone)]
pub struct FileIdentityStore {
    dir: PathBuf,
}

impl FileIdentityStore {
    /// A store in `dir` (created privately if missing).
    ///
    /// # Errors
    /// [`StoreError`] if the directory cannot be created or is insecure.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        ensure_private_dir(dir)?;
        check_config_dir(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    fn key_path(&self) -> PathBuf {
        self.dir.join(KEY_FILE)
    }

    fn load_key(&self) -> Result<Option<StaticSecret>, StoreError> {
        let path = self.key_path();
        let bytes = match std::fs::read(&path) {
            Ok(b) => Zeroizing::new(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_err(&path, &e)),
        };
        check_private_file(&path, NO_GROUP_OTHER_ACCESS)?;
        let corrupt = |reason: &str| StoreError::Corrupt {
            path: path.display().to_string(),
            reason: reason.into(),
        };
        if bytes.len() != KEY_FILE_LEN {
            return Err(corrupt("wrong length"));
        }
        let (magic, key) = bytes.split_at(KEY_MAGIC.len());
        if magic != KEY_MAGIC {
            return Err(corrupt("bad magic"));
        }
        let key: [u8; 32] = key.try_into().map_err(|_| corrupt("wrong length"))?;
        Ok(Some(StaticSecret::from_bytes(key)))
    }

    /// Creates the key unless another process created it first, then
    /// fsyncs the directory.
    ///
    /// A failed directory fsync is reported after the key file is in
    /// place: a later load uses that key.
    fn create_key(&self) -> Result<StaticSecret, StoreError> {
        let secret = StaticSecret::generate().map_err(|e| StoreError::Io(e.to_string()))?;
        let path = self.key_path();
        let mut content = Zeroizing::new(Vec::with_capacity(KEY_FILE_LEN));
        content.extend_from_slice(KEY_MAGIC);
        content.extend_from_slice(secret.as_storage_bytes());
        let mut tmp = tempfile::Builder::new()
            .prefix(".dropumi-key-")
            .tempfile_in(&self.dir)
            .map_err(|e| io_err(&self.dir, &e))?;
        tmp.write_all(&content).map_err(|e| io_err(&path, &e))?;
        tmp.as_file().sync_all().map_err(|e| io_err(&path, &e))?;
        match tmp.into_temp_path().persist_noclobber(&path) {
            Ok(()) => {
                sync_dir(&self.dir)?;
                Ok(secret)
            }
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => self
                .load_key()?
                .ok_or_else(|| StoreError::Io("identity key disappeared".into())),
            Err(e) => Err(io_err(&path, &e.error)),
        }
    }

    fn load_name(&self) -> Result<Option<DeviceName>, StoreError> {
        let path = self.dir.join(DEVICE_FILE);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_err(&path, &e)),
        };
        check_private_file(&path, NO_GROUP_OTHER_WRITE)?;
        let doc: DeviceJson = serde_json::from_slice(&bytes).map_err(|e| StoreError::Corrupt {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
        if doc.version != 1 {
            return Err(StoreError::Corrupt {
                path: path.display().to_string(),
                reason: format!("unsupported version {}", doc.version),
            });
        }
        Ok(Some(DeviceName::sanitize(&doc.name)))
    }

    fn save_name(&self, name: &DeviceName) -> Result<(), StoreError> {
        let doc = DeviceJson {
            version: 1,
            name: name.as_str().to_owned(),
        };
        let json = serde_json::to_vec_pretty(&doc).map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&self.dir, DEVICE_FILE, &json)
    }

    fn default_name() -> DeviceName {
        let mut seed = [0u8; 2];
        let _ = random::fill(&mut seed);
        DeviceName::random_default(seed)
    }

    fn identity(secret: StaticSecret, name: DeviceName) -> Result<LocalIdentity, StoreError> {
        LocalIdentity::new(secret, name).map_err(|e| StoreError::Io(e.to_string()))
    }
}

impl IdentityStore for FileIdentityStore {
    fn load_or_create(&self) -> Result<LocalIdentity, StoreError> {
        check_config_dir(&self.dir)?;
        if let (Some(secret), Some(name)) = (self.load_key()?, self.load_name()?) {
            return Self::identity(secret, name);
        }
        let _lock = ConfigLock::acquire(&self.dir)?;
        let secret = match self.load_key()? {
            Some(s) => s,
            None => self.create_key()?,
        };
        let name = match self.load_name()? {
            Some(n) => n,
            None => {
                let n = Self::default_name();
                self.save_name(&n)?;
                n
            }
        };
        Self::identity(secret, name)
    }

    fn reset(&self) -> Result<LocalIdentity, StoreError> {
        check_config_dir(&self.dir)?;
        let _lock = ConfigLock::acquire(&self.dir)?;
        let secret = StaticSecret::generate().map_err(|e| StoreError::Io(e.to_string()))?;
        let mut content = Zeroizing::new(Vec::with_capacity(KEY_FILE_LEN));
        content.extend_from_slice(KEY_MAGIC);
        content.extend_from_slice(secret.as_storage_bytes());
        write_atomic(&self.dir, KEY_FILE, &content)?;
        let name = match self.load_name()? {
            Some(n) => n,
            None => {
                let n = Self::default_name();
                self.save_name(&n)?;
                n
            }
        };
        Self::identity(secret, name)
    }

    fn set_name(&self, name: &DeviceName) -> Result<LocalIdentity, StoreError> {
        let current = self.load_or_create()?;
        let _lock = ConfigLock::acquire(&self.dir)?;
        self.save_name(name)?;
        Ok(current.with_name(name.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn identity_created_0600_in_0700() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("cfg");
        let store = FileIdentityStore::open(&dir).unwrap();
        let id = store.load_or_create().unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join(KEY_FILE)), 0o600);
        assert_eq!(mode(&dir.join(DEVICE_FILE)), 0o600);
        assert!(id.name().as_str().starts_with("Dropumi-"));
        // Stable across loads.
        let again = store.load_or_create().unwrap();
        assert_eq!(again.fingerprint(), id.fingerprint());
    }

    #[cfg(unix)]
    #[test]
    fn identity_rejects_group_readable() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let store = FileIdentityStore::open(base.path()).unwrap();
        store.load_or_create().unwrap();
        std::fs::set_permissions(
            base.path().join(KEY_FILE),
            std::fs::Permissions::from_mode(0o640),
        )
        .unwrap();
        assert!(matches!(
            store.load_or_create(),
            Err(StoreError::InsecurePermissions { .. })
        ));
    }

    /// Creating the key syncs the directory and reports a failed sync
    /// (write and search, no read), after the key file is in place.
    #[cfg(unix)]
    #[test]
    fn key_dir_sync_failure_reported() {
        use std::os::unix::fs::PermissionsExt;
        if rustix::process::geteuid().is_root() {
            // Skipped: root bypasses directory permissions, so the open
            // cannot be made to fail this way.
            return;
        }
        let base = tempfile::tempdir().unwrap();
        let store = FileIdentityStore::open(base.path()).unwrap();
        store.load_or_create().unwrap();
        // Keep device.json, so only the key is written.
        std::fs::remove_file(base.path().join(KEY_FILE)).unwrap();
        std::fs::set_permissions(base.path(), std::fs::Permissions::from_mode(0o300)).unwrap();
        let result = store.load_or_create();
        std::fs::set_permissions(base.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(result, Err(StoreError::Io(_))), "{result:?}");
        assert!(base.path().join(KEY_FILE).exists());
    }

    #[test]
    fn identity_bad_magic_rejected() {
        let base = tempfile::tempdir().unwrap();
        let store = FileIdentityStore::open(base.path()).unwrap();
        store.load_or_create().unwrap();
        let mut bytes = std::fs::read(base.path().join(KEY_FILE)).unwrap();
        bytes[0] = b'X';
        write_atomic(base.path(), KEY_FILE, &bytes).unwrap();
        assert!(matches!(
            store.load_or_create(),
            Err(StoreError::Corrupt { .. })
        ));
    }

    #[test]
    fn concurrent_create_single_identity() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().to_path_buf();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let d = dir.clone();
                std::thread::spawn(move || {
                    *FileIdentityStore::open(&d)
                        .unwrap()
                        .load_or_create()
                        .unwrap()
                        .fingerprint()
                })
            })
            .collect();
        let fps: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(fps.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn reset_replaces_key() {
        let base = tempfile::tempdir().unwrap();
        let store = FileIdentityStore::open(base.path()).unwrap();
        let a = store.load_or_create().unwrap();
        let b = store.reset().unwrap();
        assert_ne!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.name(), b.name());
        assert_eq!(
            store.load_or_create().unwrap().fingerprint(),
            b.fingerprint()
        );
        let renamed = store.set_name(&DeviceName::sanitize("Desk")).unwrap();
        assert_eq!(renamed.name().as_str(), "Desk");
        assert_eq!(store.load_or_create().unwrap().name().as_str(), "Desk");
    }
}
