// SPDX-License-Identifier: MPL-2.0
//! ManageTrustedDevices.

use dropumi_domain::device::DeviceName;
use dropumi_domain::identity::Fingerprint;
use dropumi_domain::trust::{PairingMethod, TrustedDevice};

use crate::error::StoreError;
use crate::ports::{Clock, TrustStore};

/// Lists trusted devices.
///
/// # Errors
/// [`StoreError`] from the trust store.
pub fn list<T: TrustStore>(trust: &T) -> Result<Vec<TrustedDevice>, StoreError> {
    trust.list()
}

/// Trusts a device by its full fingerprint, entered by the user after
/// comparing it out of band.
///
/// # Errors
/// [`StoreError`] from the trust store.
pub fn add_manual<T: TrustStore, K: Clock>(
    trust: &T,
    clock: &K,
    fingerprint: Fingerprint,
    alias: Option<DeviceName>,
) -> Result<TrustedDevice, StoreError> {
    let existing = trust.get(&fingerprint)?;
    let device = TrustedDevice {
        fingerprint,
        public_key: existing.as_ref().and_then(|d| d.public_key),
        alias: alias.or_else(|| existing.as_ref().and_then(|d| d.alias.clone())),
        method: existing
            .as_ref()
            .map_or(PairingMethod::Manual, |d| d.method),
        added_at: existing
            .as_ref()
            .map_or_else(|| clock.now(), |d| d.added_at),
        last_seen: existing.and_then(|d| d.last_seen),
    };
    trust.upsert(device.clone())?;
    Ok(device)
}

/// Removes a trusted device; returns whether it existed.
///
/// # Errors
/// [`StoreError`] from the trust store.
pub fn remove<T: TrustStore>(trust: &T, fingerprint: &Fingerprint) -> Result<bool, StoreError> {
    trust.remove(fingerprint)
}
