// SPDX-License-Identifier: MPL-2.0
//! The trusted-devices file.

use std::path::{Path, PathBuf};

use dropumi_application::error::StoreError;
use dropumi_application::ports::TrustStore;
use dropumi_domain::device::DeviceName;
use dropumi_domain::identity::{Fingerprint, PublicKey};
use dropumi_domain::time::Timestamp;
use dropumi_domain::trust::{PairingMethod, TrustedDevice};
use serde::{Deserialize, Serialize};

use super::{ConfigLock, write_atomic};
use crate::paths::{ensure_private_dir, io_err};
use crate::perms::{NO_GROUP_OTHER_WRITE, check_config_dir, check_private_file};

/// Trust file name.
pub const TRUST_FILE: &str = "trusted_devices.json";

#[derive(Debug, Serialize, Deserialize)]
struct TrustJson {
    version: u32,
    devices: Vec<DeviceJson>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeviceJson {
    fingerprint: String,
    public_key: Option<String>,
    alias: Option<String>,
    method: String,
    added_at: u64,
    last_seen: Option<u64>,
}

/// Trusted devices stored as JSON. Every read goes to disk so concurrent
/// processes see each other's changes; every change is a locked
/// read-modify-write with an atomic replace.
#[derive(Debug, Clone)]
pub struct TrustFile {
    dir: PathBuf,
}

impl TrustFile {
    /// A trust store in `dir` (created privately if missing).
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

    fn path(&self) -> PathBuf {
        self.dir.join(TRUST_FILE)
    }

    fn read(&self) -> Result<Vec<TrustedDevice>, StoreError> {
        check_config_dir(&self.dir)?;
        let path = self.path();
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_err(&path, &e)),
        };
        check_private_file(&path, NO_GROUP_OTHER_WRITE)?;
        let corrupt = |reason: String| StoreError::Corrupt {
            path: path.display().to_string(),
            reason,
        };
        let doc: TrustJson = serde_json::from_slice(&bytes).map_err(|e| corrupt(e.to_string()))?;
        if doc.version != 1 {
            return Err(corrupt(format!("unsupported version {}", doc.version)));
        }
        doc.devices
            .into_iter()
            .map(|d| {
                let fingerprint =
                    Fingerprint::parse_full(&d.fingerprint).map_err(|e| corrupt(e.to_string()))?;
                let public_key = d
                    .public_key
                    .as_deref()
                    .map(PublicKey::from_hex)
                    .transpose()
                    .map_err(|e| corrupt(e.to_string()))?;
                let method = PairingMethod::parse(&d.method)
                    .ok_or_else(|| corrupt(format!("unknown method {}", d.method)))?;
                Ok(TrustedDevice {
                    fingerprint,
                    public_key,
                    alias: d.alias.as_deref().map(DeviceName::sanitize),
                    method,
                    added_at: Timestamp(d.added_at),
                    last_seen: d.last_seen.map(Timestamp),
                })
            })
            .collect()
    }

    fn write(&self, devices: &[TrustedDevice]) -> Result<(), StoreError> {
        let doc = TrustJson {
            version: 1,
            devices: devices
                .iter()
                .map(|d| DeviceJson {
                    fingerprint: d.fingerprint.to_full(),
                    public_key: d.public_key.map(|k| k.to_hex()),
                    alias: d.alias.as_ref().map(|a| a.as_str().to_owned()),
                    method: d.method.as_str().to_owned(),
                    added_at: d.added_at.as_secs(),
                    last_seen: d.last_seen.map(Timestamp::as_secs),
                })
                .collect(),
        };
        let json = serde_json::to_vec_pretty(&doc).map_err(|e| StoreError::Io(e.to_string()))?;
        write_atomic(&self.dir, TRUST_FILE, &json)
    }

    fn modify<R>(&self, f: impl FnOnce(&mut Vec<TrustedDevice>) -> R) -> Result<R, StoreError> {
        let _lock = ConfigLock::acquire(&self.dir)?;
        let mut devices = self.read()?;
        let r = f(&mut devices);
        self.write(&devices)?;
        Ok(r)
    }
}

impl TrustStore for TrustFile {
    fn get(&self, fp: &Fingerprint) -> Result<Option<TrustedDevice>, StoreError> {
        Ok(self.read()?.into_iter().find(|d| &d.fingerprint == fp))
    }

    fn list(&self) -> Result<Vec<TrustedDevice>, StoreError> {
        self.read()
    }

    fn upsert(&self, device: TrustedDevice) -> Result<(), StoreError> {
        self.modify(|v| {
            v.retain(|d| d.fingerprint != device.fingerprint);
            v.push(device);
        })
    }

    fn upsert_with<F: FnOnce(Option<&TrustedDevice>) -> TrustedDevice>(
        &self,
        fp: &Fingerprint,
        merge: F,
    ) -> Result<TrustedDevice, StoreError> {
        self.modify(|v| {
            let device = merge(v.iter().find(|d| &d.fingerprint == fp));
            if &device.fingerprint != fp {
                // Checked before any change: the rewrite keeps the content.
                return Err(StoreError::Io(
                    "merged trust entry has a different fingerprint".into(),
                ));
            }
            v.retain(|d| &d.fingerprint != fp);
            v.push(device.clone());
            Ok(device)
        })?
    }

    fn remove(&self, fp: &Fingerprint) -> Result<bool, StoreError> {
        self.modify(|v| {
            let before = v.len();
            v.retain(|d| &d.fingerprint != fp);
            v.len() != before
        })
    }

    fn touch(&self, fp: &Fingerprint, at: Timestamp) -> Result<(), StoreError> {
        self.modify(|v| {
            for d in v.iter_mut().filter(|d| &d.fingerprint == fp) {
                d.last_seen = Some(at);
            }
        })
    }

    fn rename(&self, fp: &Fingerprint, alias: DeviceName) -> Result<bool, StoreError> {
        let _lock = ConfigLock::acquire(&self.dir)?;
        let mut devices = self.read()?;
        let Some(device) = devices.iter_mut().find(|d| &d.fingerprint == fp) else {
            return Ok(false);
        };
        device.alias = Some(alias);
        self.write(&devices)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(b: u8, alias: &str) -> TrustedDevice {
        TrustedDevice {
            fingerprint: Fingerprint::from_digest([b; 32]),
            public_key: Some(PublicKey::from_bytes([b; 32])),
            alias: Some(DeviceName::sanitize(alias)),
            method: PairingMethod::Sas,
            added_at: Timestamp(10),
            last_seen: None,
        }
    }

    #[test]
    fn trust_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let t = TrustFile::open(d.path()).unwrap();
        assert!(t.list().unwrap().is_empty());
        t.upsert(device(1, "One")).unwrap();
        t.upsert(device(2, "Two")).unwrap();
        t.upsert(device(1, "Uno")).unwrap();
        let list = t.list().unwrap();
        assert_eq!(list.len(), 2);
        let one = t.get(&Fingerprint::from_digest([1; 32])).unwrap().unwrap();
        assert_eq!(one.alias.unwrap().as_str(), "Uno");
        t.touch(&Fingerprint::from_digest([2; 32]), Timestamp(99))
            .unwrap();
        assert_eq!(
            t.get(&Fingerprint::from_digest([2; 32]))
                .unwrap()
                .unwrap()
                .last_seen,
            Some(Timestamp(99))
        );
        assert!(t.remove(&Fingerprint::from_digest([1; 32])).unwrap());
        assert!(!t.remove(&Fingerprint::from_digest([1; 32])).unwrap());
        assert_eq!(t.list().unwrap().len(), 1);
    }

    #[test]
    fn concurrent_writers_serialized() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().to_path_buf();
        let handles: Vec<_> = (0..8u8)
            .map(|i| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    TrustFile::open(&dir)
                        .unwrap()
                        .upsert(device(i + 1, &format!("D{i}")))
                        .unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(TrustFile::open(&dir).unwrap().list().unwrap().len(), 8);
    }

    #[cfg(unix)]
    #[test]
    fn trust_store_rejects_group_writable() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let t = TrustFile::open(d.path()).unwrap();
        t.upsert(device(1, "One")).unwrap();
        std::fs::set_permissions(
            d.path().join(TRUST_FILE),
            std::fs::Permissions::from_mode(0o620),
        )
        .unwrap();
        assert!(matches!(
            t.list(),
            Err(StoreError::InsecurePermissions { .. })
        ));
    }

    /// A manual add that races a session's last-seen update never writes
    /// back an entry read before that update.
    #[test]
    fn manual_trust_racing_touch_keeps_last_seen() {
        use dropumi_application::usecases::trust::add_manual;

        use crate::clock::SystemClock;

        let d = tempfile::tempdir().unwrap();
        let dir = d.path().to_path_buf();
        let fp = Fingerprint::from_digest([1; 32]);
        for round in 0..40 {
            let t = TrustFile::open(&dir).unwrap();
            t.upsert(device(1, "One")).unwrap();
            let start = std::sync::Arc::new(std::sync::Barrier::new(2));
            let adder = {
                let (dir, start) = (dir.clone(), std::sync::Arc::clone(&start));
                std::thread::spawn(move || {
                    let t = TrustFile::open(&dir).unwrap();
                    start.wait();
                    for _ in 0..5 {
                        add_manual(&t, &SystemClock, fp, None).unwrap();
                    }
                })
            };
            let toucher = {
                let (dir, start) = (dir.clone(), std::sync::Arc::clone(&start));
                std::thread::spawn(move || {
                    let t = TrustFile::open(&dir).unwrap();
                    start.wait();
                    t.touch(&fp, Timestamp(99)).unwrap();
                })
            };
            adder.join().unwrap();
            toucher.join().unwrap();
            let stored = t.get(&fp).unwrap().unwrap();
            assert_eq!(
                stored.last_seen,
                Some(Timestamp(99)),
                "round {round}: a manual add dropped a concurrent last-seen update"
            );
            assert_eq!(stored.method, PairingMethod::Sas);
            assert_eq!(stored.alias.unwrap().as_str(), "One");
        }
    }

    /// A manual add without an alias that races a rename never writes back
    /// the alias read before that rename.
    #[test]
    fn manual_trust_racing_rename_keeps_new_alias() {
        use dropumi_application::usecases::trust::add_manual;

        use crate::clock::SystemClock;

        let d = tempfile::tempdir().unwrap();
        let dir = d.path().to_path_buf();
        let fp = Fingerprint::from_digest([1; 32]);
        for round in 0..40 {
            let t = TrustFile::open(&dir).unwrap();
            t.upsert(device(1, "One")).unwrap();
            let start = std::sync::Arc::new(std::sync::Barrier::new(2));
            let adder = {
                let (dir, start) = (dir.clone(), std::sync::Arc::clone(&start));
                std::thread::spawn(move || {
                    let t = TrustFile::open(&dir).unwrap();
                    start.wait();
                    for _ in 0..5 {
                        add_manual(&t, &SystemClock, fp, None).unwrap();
                    }
                })
            };
            let renamer = {
                let (dir, start) = (dir.clone(), std::sync::Arc::clone(&start));
                std::thread::spawn(move || {
                    let t = TrustFile::open(&dir).unwrap();
                    start.wait();
                    assert!(t.rename(&fp, DeviceName::sanitize("Renamed")).unwrap());
                })
            };
            adder.join().unwrap();
            renamer.join().unwrap();
            let stored = t.get(&fp).unwrap().unwrap();
            assert_eq!(
                stored.alias.unwrap().as_str(),
                "Renamed",
                "round {round}: a manual add dropped a concurrent rename"
            );
            assert_eq!(stored.method, PairingMethod::Sas);
        }
    }

    #[test]
    fn rename_keeps_fingerprint_and_method() {
        let d = tempfile::tempdir().unwrap();
        let t = TrustFile::open(d.path()).unwrap();
        let fp = Fingerprint::from_digest([1; 32]);
        t.upsert(device(1, "One")).unwrap();
        t.touch(&fp, Timestamp(42)).unwrap();
        assert!(t.rename(&fp, DeviceName::sanitize("Studio Mac")).unwrap());
        let renamed = t.get(&fp).unwrap().unwrap();
        assert_eq!(renamed.alias.unwrap().as_str(), "Studio Mac");
        assert_eq!(renamed.fingerprint, fp);
        assert_eq!(renamed.public_key, Some(PublicKey::from_bytes([1; 32])));
        assert_eq!(renamed.method, PairingMethod::Sas);
        assert_eq!(renamed.added_at, Timestamp(10));
        assert_eq!(renamed.last_seen, Some(Timestamp(42)));
    }

    #[test]
    fn rename_unknown_returns_false_and_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let t = TrustFile::open(d.path()).unwrap();
        let fp = Fingerprint::from_digest([3; 32]);
        assert!(!t.rename(&fp, DeviceName::sanitize("Ghost")).unwrap());
        assert!(!d.path().join(TRUST_FILE).exists());
        t.upsert(device(1, "One")).unwrap();
        assert!(!t.rename(&fp, DeviceName::sanitize("Ghost")).unwrap());
        assert_eq!(t.list().unwrap().len(), 1);
        assert!(t.get(&fp).unwrap().is_none());
    }

    #[test]
    fn rename_racing_remove_never_resurrects() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().to_path_buf();
        let fp = Fingerprint::from_digest([1; 32]);
        for round in 0..40 {
            let t = TrustFile::open(&dir).unwrap();
            t.upsert(device(1, "One")).unwrap();
            let start = std::sync::Arc::new(std::sync::Barrier::new(2));
            let renamer = {
                let (dir, start) = (dir.clone(), std::sync::Arc::clone(&start));
                std::thread::spawn(move || {
                    let t = TrustFile::open(&dir).unwrap();
                    start.wait();
                    for _ in 0..5 {
                        t.rename(&fp, DeviceName::sanitize("Renamed")).unwrap();
                    }
                })
            };
            let remover = {
                let (dir, start) = (dir.clone(), std::sync::Arc::clone(&start));
                std::thread::spawn(move || {
                    let t = TrustFile::open(&dir).unwrap();
                    start.wait();
                    assert!(t.remove(&fp).unwrap());
                })
            };
            renamer.join().unwrap();
            remover.join().unwrap();
            assert!(
                t.get(&fp).unwrap().is_none(),
                "round {round}: a rename brought back a removed device"
            );
        }
    }

    #[test]
    fn corrupt_file_reported() {
        let d = tempfile::tempdir().unwrap();
        let t = TrustFile::open(d.path()).unwrap();
        write_atomic(d.path(), TRUST_FILE, b"{not json").unwrap();
        assert!(matches!(t.list(), Err(StoreError::Corrupt { .. })));
    }
}
