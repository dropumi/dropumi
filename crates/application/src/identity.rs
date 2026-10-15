// SPDX-License-Identifier: MPL-2.0
//! This device's identity.

use std::fmt;

use dropumi_domain::device::DeviceName;
use dropumi_domain::identity::{Fingerprint, PublicKey};
use dropumi_protocol::ProtocolError;
use dropumi_protocol::identity::{StaticSecret, fingerprint_of};

/// The local device: static key, derived identity and chosen name.
#[derive(Clone)]
pub struct LocalIdentity {
    secret: StaticSecret,
    public: PublicKey,
    fingerprint: Fingerprint,
    name: DeviceName,
}

impl LocalIdentity {
    /// Derives the public identity from a stored secret.
    ///
    /// # Errors
    /// [`ProtocolError::Handshake`] if the crypto backend is unavailable.
    pub fn new(secret: StaticSecret, name: DeviceName) -> Result<Self, ProtocolError> {
        let public = secret.public_key()?;
        Ok(Self {
            fingerprint: fingerprint_of(&public),
            public,
            secret,
            name,
        })
    }

    /// Static secret (for the handshake only).
    #[must_use]
    pub const fn secret(&self) -> &StaticSecret {
        &self.secret
    }

    /// Public key.
    #[must_use]
    pub const fn public_key(&self) -> &PublicKey {
        &self.public
    }

    /// Fingerprint.
    #[must_use]
    pub const fn fingerprint(&self) -> &Fingerprint {
        &self.fingerprint
    }

    /// Device name.
    #[must_use]
    pub const fn name(&self) -> &DeviceName {
        &self.name
    }

    /// The same identity with another name.
    #[must_use]
    pub fn with_name(mut self, name: DeviceName) -> Self {
        self.name = name;
        self
    }
}

impl fmt::Debug for LocalIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalIdentity")
            .field("fingerprint", &self.fingerprint)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
