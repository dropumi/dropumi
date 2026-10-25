// SPDX-License-Identifier: MPL-2.0
//! Committed short authentication strings (SAS).
//!
//! The responder commits to a random nonce `N_R` in Noise message 2
//! (`C_R = SHA-256("dropumi/v1/sas-commit" ‖ N_R)`), learns the initiator's
//! nonce `N_I` in message 3, and reveals `N_R` only in its Hello. Both
//! sides then compute
//! `SAS = u64be(SHA-256("dropumi/v1/sas" ‖ h ‖ N_I ‖ N_R)[0..8]) mod 10^6`
//! where `h` is the final Noise handshake hash. A man in the middle runs two
//! handshakes with different `h` and must fix both nonces before seeing the
//! other, so it gets one guess per attempt (success probability 10⁻⁶)
//! instead of being able to grind a matching code.

use std::fmt;

use sha2::{Digest, Sha256};

use crate::error::ProtocolError;

/// Domain-separation label for the commitment.
pub const COMMIT_LABEL: &[u8] = b"dropumi/v1/sas-commit";
/// Domain-separation label for the code.
pub const SAS_LABEL: &[u8] = b"dropumi/v1/sas";

/// `SHA-256("dropumi/v1/sas-commit" ‖ n_r)`.
#[must_use]
pub fn commit(n_r: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(COMMIT_LABEL);
    h.update(n_r);
    h.finalize().into()
}

/// Checks a revealed nonce against its commitment.
///
/// # Errors
/// [`ProtocolError::SasCommitmentMismatch`] on mismatch.
pub fn verify_reveal(commitment: &[u8; 32], n_r: &[u8; 32]) -> Result<(), ProtocolError> {
    if &commit(n_r) == commitment {
        Ok(())
    } else {
        Err(ProtocolError::SasCommitmentMismatch)
    }
}

/// A 6-digit verification code.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SasCode(u32);

impl SasCode {
    /// Derives the code from the handshake hash and both nonces.
    #[must_use]
    pub fn derive(handshake_hash: &[u8; 32], n_i: &[u8; 32], n_r: &[u8; 32]) -> Self {
        let mut h = Sha256::new();
        h.update(SAS_LABEL);
        h.update(handshake_hash);
        h.update(n_i);
        h.update(n_r);
        let d: [u8; 32] = h.finalize().into();
        let mut first = [0u8; 8];
        first.copy_from_slice(d.get(..8).unwrap_or(&[0; 8]));
        let v = u64::from_be_bytes(first) % 1_000_000;
        Self(u32::try_from(v).unwrap_or(0))
    }

    /// Six digits without separators, for example `"123456"`.
    #[must_use]
    pub fn as_digits(&self) -> String {
        format!("{:06}", self.0)
    }
}

impl fmt::Display for SasCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.as_digits();
        write!(
            f,
            "{} {}",
            d.get(..3).unwrap_or_default(),
            d.get(3..).unwrap_or_default()
        )
    }
}

impl fmt::Debug for SasCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SasCode({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected value from an independent oracle:
    /// `python3 -c "import hashlib;print(hashlib.sha256(b'dropumi/v1/sas-commit'+bytes([0x11]*32)).hexdigest())"`
    #[test]
    fn commitment_kat() {
        let c = commit(&[0x11; 32]);
        let hex: String = c.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "90b1ecc25e8d77c89eebb9d017b5dc92534b02bc49cf8afb8b66cc442a34fe6c"
        );
        assert!(verify_reveal(&c, &[0x11; 32]).is_ok());
    }

    /// Expected value from an independent oracle:
    /// `python3 -c "import hashlib;d=hashlib.sha256(b'dropumi/v1/sas'+bytes(range(100,132))+bytes([0x22]*32)+bytes([0x11]*32)).digest();print('%06d'%(int.from_bytes(d[:8],'big')%10**6))"`
    #[test]
    fn sas_kat() {
        let mut h = [0u8; 32];
        for (i, b) in h.iter_mut().enumerate() {
            *b = u8::try_from(100 + i).unwrap();
        }
        let code = SasCode::derive(&h, &[0x22; 32], &[0x11; 32]);
        assert_eq!(code.as_digits(), "575292");
        assert_eq!(code.to_string(), "575 292");
    }

    #[test]
    fn sas_commitment_mismatch_detected() {
        let c = commit(&[1; 32]);
        assert_eq!(
            verify_reveal(&c, &[2; 32]),
            Err(ProtocolError::SasCommitmentMismatch)
        );
    }
}
