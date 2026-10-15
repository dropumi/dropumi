// SPDX-License-Identifier: MPL-2.0
//! ManageTrustedDevices.

use dropumi_domain::device::DeviceName;
use dropumi_domain::identity::Fingerprint;
use dropumi_domain::time::Timestamp;
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
/// The merge with an existing entry happens inside the store's write
/// ([`TrustStore::upsert_with`]), so a concurrent rename, last-seen update
/// or pairing is never overwritten with data read before it.
///
/// # Errors
/// [`StoreError`] from the trust store.
pub fn add_manual<T: TrustStore, K: Clock>(
    trust: &T,
    clock: &K,
    fingerprint: Fingerprint,
    alias: Option<DeviceName>,
) -> Result<TrustedDevice, StoreError> {
    let now = clock.now();
    trust.upsert_with(&fingerprint, |existing| {
        manual_entry(existing, fingerprint, alias, now)
    })
}

/// The entry a manual add stores. A new device is `Manual`, without a key,
/// added `now`. An existing entry keeps its method, key, dates and alias;
/// an explicit alias replaces the alias.
fn manual_entry(
    existing: Option<&TrustedDevice>,
    fingerprint: Fingerprint,
    alias: Option<DeviceName>,
    now: Timestamp,
) -> TrustedDevice {
    match existing {
        None => TrustedDevice {
            fingerprint,
            public_key: None,
            alias,
            method: PairingMethod::Manual,
            added_at: now,
            last_seen: None,
        },
        Some(d) => TrustedDevice {
            fingerprint,
            public_key: d.public_key,
            alias: alias.or_else(|| d.alias.clone()),
            method: d.method,
            added_at: d.added_at,
            last_seen: d.last_seen,
        },
    }
}

/// Removes a trusted device; returns whether it existed.
///
/// # Errors
/// [`StoreError`] from the trust store.
pub fn remove<T: TrustStore>(trust: &T, fingerprint: &Fingerprint) -> Result<bool, StoreError> {
    trust.remove(fingerprint)
}

/// Renames a trusted device (its local alias); returns whether it was
/// trusted. Never adds a device.
///
/// # Errors
/// [`StoreError`] from the trust store.
pub fn rename<T: TrustStore>(
    trust: &T,
    fingerprint: &Fingerprint,
    alias: DeviceName,
) -> Result<bool, StoreError> {
    trust.rename(fingerprint, alias)
}

#[cfg(test)]
mod tests {
    use dropumi_domain::identity::PublicKey;

    use super::*;

    #[test]
    fn manual_entry_keeps_pairing_data_and_explicit_alias_wins() {
        let fp = Fingerprint::from_digest([7; 32]);
        let now = Timestamp(500);

        let new = manual_entry(None, fp, Some(DeviceName::sanitize("Desk")), now);
        assert_eq!(
            new,
            TrustedDevice {
                fingerprint: fp,
                public_key: None,
                alias: Some(DeviceName::sanitize("Desk")),
                method: PairingMethod::Manual,
                added_at: now,
                last_seen: None,
            }
        );
        assert_eq!(manual_entry(None, fp, None, now).alias, None);

        let paired = TrustedDevice {
            fingerprint: fp,
            public_key: Some(PublicKey::from_bytes([7; 32])),
            alias: Some(DeviceName::sanitize("Laptop")),
            method: PairingMethod::Sas,
            added_at: Timestamp(10),
            last_seen: Some(Timestamp(20)),
        };
        assert_eq!(manual_entry(Some(&paired), fp, None, now), paired);
        assert_eq!(
            manual_entry(Some(&paired), fp, Some(DeviceName::sanitize("Work")), now),
            TrustedDevice {
                alias: Some(DeviceName::sanitize("Work")),
                ..paired.clone()
            }
        );
    }
}
