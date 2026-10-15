// SPDX-License-Identifier: MPL-2.0
//! Trusted devices.

use crate::device::DeviceName;
use crate::identity::{Fingerprint, FingerprintPrefix, PublicKey};
use crate::time::Timestamp;

/// How trust in a device was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PairingMethod {
    /// Both users confirmed matching short authentication strings.
    Sas,
    /// The user entered the full fingerprint manually.
    Manual,
}

impl PairingMethod {
    /// Stable storage name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sas => "sas",
            Self::Manual => "manual",
        }
    }

    /// Parses a storage name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sas" => Some(Self::Sas),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// A device whose key the local user trusts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedDevice {
    /// Fingerprint of the trusted key (the identity).
    pub fingerprint: Fingerprint,
    /// The full public key when known (SAS pairing records it).
    pub public_key: Option<PublicKey>,
    /// Local alias chosen or confirmed by the user. Displayed before any
    /// name the peer claims.
    pub alias: Option<DeviceName>,
    /// How trust was established.
    pub method: PairingMethod,
    /// When the device was added.
    pub added_at: Timestamp,
    /// When the device last completed a session.
    pub last_seen: Option<Timestamp>,
}

/// Trusted devices whose fingerprint starts with `prefix`.
#[must_use]
pub fn trusted_matching<'a>(
    trusted: &'a [TrustedDevice],
    prefix: &FingerprintPrefix,
) -> Vec<&'a TrustedDevice> {
    trusted
        .iter()
        .filter(|d| prefix.matches(&d.fingerprint))
        .collect()
}
