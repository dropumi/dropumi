// SPDX-License-Identifier: MPL-2.0
//! Device identity management.

use dropumi_domain::device::DeviceName;

use crate::error::StoreError;
use crate::identity::LocalIdentity;
use crate::ports::IdentityStore;

/// The current identity (created on first use).
///
/// # Errors
/// [`StoreError`] from the identity store.
pub fn show<S: IdentityStore>(store: &S) -> Result<LocalIdentity, StoreError> {
    store.load_or_create()
}

/// Replaces the key: other devices must pair again.
///
/// # Errors
/// [`StoreError`] from the identity store.
pub fn reset<S: IdentityStore>(store: &S) -> Result<LocalIdentity, StoreError> {
    store.reset()
}

/// Sets the device name (sanitized).
///
/// # Errors
/// [`StoreError`] from the identity store.
pub fn set_name<S: IdentityStore>(store: &S, raw: &str) -> Result<LocalIdentity, StoreError> {
    store.set_name(&DeviceName::sanitize(raw))
}
