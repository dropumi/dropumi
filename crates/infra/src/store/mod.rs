// SPDX-License-Identifier: MPL-2.0
//! File-based configuration stores.
//!
//! Layout of the configuration directory (0700 on Unix):
//!
//! | File | Content | Mode |
//! |---|---|---|
//! | `identity.key` | `DRPMKEY1` + 32-byte X25519 secret | 0600 |
//! | `device.json` | `{"version":1,"name":"…"}` | 0600 |
//! | `trusted_devices.json` | `{"version":1,"devices":[…]}` | 0600 |
//! | `.lock` | advisory lock for read-modify-write | 0600 |
//!
//! Writes are atomic (temp file, fsync, rename, directory fsync). Keychain
//! storage is planned behind the same ports.

mod atomic;
mod identity_file;
mod lock;
mod trust_file;

pub use identity_file::{DEVICE_FILE, FileIdentityStore, KEY_FILE, KEY_MAGIC};
pub use lock::LOCK_FILE;
pub use trust_file::{TRUST_FILE, TrustFile};

pub(crate) use atomic::write_atomic;
pub(crate) use lock::ConfigLock;
