// SPDX-License-Identifier: MPL-2.0
//! The device's static X25519 key and fingerprint derivation.

use std::fmt;

use dropumi_domain::identity::{Fingerprint, PublicKey};
use sha2::{Digest, Sha256};
use snow::params::DHChoice;
use snow::resolvers::{CryptoResolver, DefaultResolver};
use zeroize::Zeroizing;

use crate::error::ProtocolError;

/// Domain-separation label for fingerprints.
pub const FINGERPRINT_LABEL: &[u8] = b"dropumi/v1/device-fingerprint";

/// The device's static X25519 private key. Zeroized on drop; `Debug`
/// never prints it.
#[derive(Clone)]
pub struct StaticSecret(Zeroizing<[u8; 32]>);

impl StaticSecret {
    /// 32 bytes from the OS random number generator.
    ///
    /// # Errors
    /// [`ProtocolError::Random`] if the OS generator fails.
    pub fn generate() -> Result<Self, ProtocolError> {
        let mut bytes = Zeroizing::new([0u8; 32]);
        getrandom::fill(bytes.as_mut()).map_err(|_| ProtocolError::Random)?;
        Ok(Self(bytes))
    }

    /// Wraps stored key bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Key bytes for storage.
    #[must_use]
    pub fn as_storage_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The matching public key, `X25519(secret, 9)`.
    ///
    /// # Errors
    /// [`ProtocolError::Handshake`] if the crypto backend is unavailable.
    pub fn public_key(&self) -> Result<PublicKey, ProtocolError> {
        let mut dh = DefaultResolver
            .resolve_dh(&DHChoice::Curve25519)
            .ok_or(ProtocolError::Handshake)?;
        dh.set(self.0.as_ref());
        let pk: [u8; 32] = dh
            .pubkey()
            .try_into()
            .map_err(|_| ProtocolError::Handshake)?;
        Ok(PublicKey::from_bytes(pk))
    }
}

impl fmt::Debug for StaticSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StaticSecret(..)")
    }
}

/// `SHA-256("dropumi/v1/device-fingerprint" ‖ public_key)`.
#[must_use]
pub fn fingerprint_of(pk: &PublicKey) -> Fingerprint {
    let mut h = Sha256::new();
    h.update(FINGERPRINT_LABEL);
    h.update(pk.as_bytes());
    Fingerprint::from_digest(h.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex32(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
        }
        out
    }

    /// RFC 7748 §6.1 test vector, copied from
    /// <https://www.rfc-editor.org/rfc/rfc7748.txt> (section 6.1).
    #[test]
    fn x25519_rfc7748_vector() {
        let alice = StaticSecret::from_bytes(hex32(
            "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a",
        ));
        let bob = StaticSecret::from_bytes(hex32(
            "5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb",
        ));
        assert_eq!(
            alice.public_key().unwrap().as_bytes(),
            &hex32("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a")
        );
        assert_eq!(
            bob.public_key().unwrap().as_bytes(),
            &hex32("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f")
        );
    }

    /// Expected value from an independent oracle:
    /// `python3 -c "import hashlib,base64;print(base64.b32encode(hashlib.sha256(b'dropumi/v1/device-fingerprint'+bytes(range(32))).digest()).decode().rstrip('='))"`
    #[test]
    fn fingerprint_kat() {
        let mut pk = [0u8; 32];
        for (i, b) in pk.iter_mut().enumerate() {
            *b = u8::try_from(i).unwrap();
        }
        let fp = fingerprint_of(&PublicKey::from_bytes(pk));
        assert_eq!(fp.to_full(), FINGERPRINT_KAT);
    }

    const FINGERPRINT_KAT: &str = "QLARLUQOXKSVO4B6FERSNH3MRDB427ZIYR4JJE2NM4YDJ3KZ7H4Q";

    #[test]
    fn debug_redacts_secret() {
        let s = StaticSecret::from_bytes([0x42; 32]);
        assert_eq!(format!("{s:?}"), "StaticSecret(..)");
    }
}
