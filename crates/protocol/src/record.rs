// SPDX-License-Identifier: MPL-2.0
//! Length-prefixed records and the transport cipher states.
//!
//! Every record on the wire is `u16be length ‖ Noise message`. Lengths are
//! validated before any allocation. Transport nonces are implicit (one
//! counter per direction, starting at 0, never sent). Any decryption failure
//! poisons the receiving direction: the session must end.

use std::fmt;
use std::sync::Arc;

use snow::StatelessTransportState;

use crate::error::ProtocolError;
use crate::limits::{MAX_HANDSHAKE_MSG, MAX_PLAINTEXT, MIN_TRANSPORT_MSG, NOISE_MAX_MSG, TAG_LEN};

/// Size of the record length prefix.
pub const LEN_PREFIX: usize = 2;
/// Buffer size that fits any record including its prefix.
pub const MAX_RECORD: usize = LEN_PREFIX + NOISE_MAX_MSG;

/// Which limits apply to a record length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordPhase {
    /// Handshake messages: 1..=512 bytes.
    Handshake,
    /// Transport messages: 17..=65,535 bytes.
    Transport,
}

/// Validates a length prefix before anything is allocated.
///
/// # Errors
/// [`ProtocolError::LimitExceeded`] or [`ProtocolError::Malformed`] for
/// lengths outside the phase's bounds.
pub fn parse_len(prefix: [u8; 2], phase: RecordPhase) -> Result<usize, ProtocolError> {
    let len = usize::from(u16::from_be_bytes(prefix));
    match phase {
        RecordPhase::Handshake if len == 0 => {
            Err(ProtocolError::Malformed("empty handshake message"))
        }
        RecordPhase::Handshake if len > MAX_HANDSHAKE_MSG => {
            Err(ProtocolError::LimitExceeded("handshake message"))
        }
        RecordPhase::Transport if len < MIN_TRANSPORT_MSG => {
            Err(ProtocolError::Malformed("transport record too short"))
        }
        _ => Ok(len),
    }
}

/// `u16be len ‖ msg` for a handshake message.
///
/// # Errors
/// [`ProtocolError::LimitExceeded`] above 512 bytes.
pub fn frame_handshake(msg: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if msg.is_empty() || msg.len() > MAX_HANDSHAKE_MSG {
        return Err(ProtocolError::LimitExceeded("handshake message"));
    }
    let len = u16::try_from(msg.len()).map_err(|_| ProtocolError::LimitExceeded("handshake"))?;
    let mut out = Vec::with_capacity(msg.len().saturating_add(LEN_PREFIX));
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(msg);
    Ok(out)
}

/// Encrypts outgoing records with an explicit, monotonically increasing
/// nonce. Single owner: the send half of a session.
pub struct Sealer {
    ts: Arc<StatelessTransportState>,
    nonce: u64,
}

impl Sealer {
    pub(crate) fn new(ts: Arc<StatelessTransportState>) -> Self {
        Self { ts, nonce: 0 }
    }

    /// Encrypts `plaintext` (1..=65,519 bytes) into `out` as
    /// `u16be len ‖ ciphertext` and returns the record length.
    /// `out` must hold at least `plaintext.len() + 18` bytes.
    ///
    /// # Errors
    /// [`ProtocolError::NonceExhausted`] when the nonce space is used up,
    /// [`ProtocolError::LimitExceeded`] for oversized input.
    pub fn seal(&mut self, plaintext: &[u8], out: &mut [u8]) -> Result<usize, ProtocolError> {
        if plaintext.is_empty() || plaintext.len() > MAX_PLAINTEXT {
            return Err(ProtocolError::LimitExceeded("record plaintext"));
        }
        // Nonce 2^64 − 1 is reserved by the Noise specification.
        if self.nonce == u64::MAX {
            return Err(ProtocolError::NonceExhausted);
        }
        let ct_len = plaintext.len().saturating_add(TAG_LEN);
        let total = ct_len.saturating_add(LEN_PREFIX);
        let (prefix, body) = out
            .get_mut(..total)
            .ok_or(ProtocolError::LimitExceeded("record buffer"))?
            .split_at_mut(LEN_PREFIX);
        let n = self
            .ts
            .write_message(self.nonce, plaintext, body)
            .map_err(|_| ProtocolError::Encode)?;
        if n != ct_len {
            return Err(ProtocolError::Encode);
        }
        let len = u16::try_from(n).map_err(|_| ProtocolError::LimitExceeded("record"))?;
        prefix.copy_from_slice(&len.to_be_bytes());
        self.nonce = self
            .nonce
            .checked_add(1)
            .ok_or(ProtocolError::NonceExhausted)?;
        Ok(total)
    }

    /// Number of records sealed so far.
    #[must_use]
    pub const fn sent(&self) -> u64 {
        self.nonce
    }

    #[cfg(test)]
    pub(crate) fn set_nonce_for_test(&mut self, nonce: u64) {
        self.nonce = nonce;
    }
}

impl fmt::Debug for Sealer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sealer")
            .field("nonce", &self.nonce)
            .finish()
    }
}

/// Decrypts incoming records in order. Any failure poisons it.
pub struct Opener {
    ts: Arc<StatelessTransportState>,
    nonce: u64,
    poisoned: bool,
}

impl Opener {
    pub(crate) fn new(ts: Arc<StatelessTransportState>) -> Self {
        Self {
            ts,
            nonce: 0,
            poisoned: false,
        }
    }

    /// Decrypts one record body (without the length prefix) into `out` and
    /// returns the plaintext.
    ///
    /// # Errors
    /// [`ProtocolError::Decrypt`] for tampered, replayed, reordered or
    /// truncated records and for every call after a failure.
    pub fn open<'o>(
        &mut self,
        ciphertext: &[u8],
        out: &'o mut [u8],
    ) -> Result<&'o [u8], ProtocolError> {
        if self.poisoned {
            return Err(ProtocolError::Decrypt);
        }
        let result = self.open_inner(ciphertext, out);
        match result {
            Ok(n) => out.get(..n).ok_or(ProtocolError::Decrypt),
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }

    fn open_inner(&mut self, ciphertext: &[u8], out: &mut [u8]) -> Result<usize, ProtocolError> {
        if ciphertext.len() < MIN_TRANSPORT_MSG || ciphertext.len() > NOISE_MAX_MSG {
            return Err(ProtocolError::Decrypt);
        }
        if self.nonce == u64::MAX {
            return Err(ProtocolError::NonceExhausted);
        }
        let n = self
            .ts
            .read_message(self.nonce, ciphertext, out)
            .map_err(|_| ProtocolError::Decrypt)?;
        self.nonce = self
            .nonce
            .checked_add(1)
            .ok_or(ProtocolError::NonceExhausted)?;
        Ok(n)
    }

    /// Whether a failure has poisoned this direction.
    #[must_use]
    pub const fn is_poisoned(&self) -> bool {
        self.poisoned
    }
}

impl fmt::Debug for Opener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Opener")
            .field("nonce", &self.nonce)
            .field("poisoned", &self.poisoned)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handshake::{HandshakeOutcome, Initiator, Responder};
    use crate::identity::StaticSecret;
    use crate::version::Prologue;

    fn session_pair() -> (HandshakeOutcome, HandshakeOutcome) {
        let p = Prologue::new(b"p", b"r").unwrap();
        let (i, m1) = Initiator::start(&StaticSecret::from_bytes([1; 32]), &p, [3; 32]).unwrap();
        let (r, m2) = Responder::start(&StaticSecret::from_bytes([2; 32]), &p, [4; 32])
            .unwrap()
            .read_msg1(&m1)
            .unwrap();
        let (i, m3) = i.read_msg2(&m2).unwrap().proceed().unwrap();
        (i.finish(), r.read_msg3(&m3).unwrap().finish())
    }

    #[test]
    fn nonce_exhaustion_errors() {
        let (mut i, _r) = session_pair();
        let mut out = vec![0u8; MAX_RECORD];
        i.sealer.set_nonce_for_test(u64::MAX - 1);
        assert!(i.sealer.seal(b"x", &mut out).is_ok());
        assert_eq!(
            i.sealer.seal(b"x", &mut out),
            Err(ProtocolError::NonceExhausted)
        );
    }

    #[test]
    fn len_prefix_bounds() {
        assert!(parse_len([0, 0], RecordPhase::Handshake).is_err());
        assert_eq!(parse_len([0, 32], RecordPhase::Handshake).unwrap(), 32);
        assert!(
            parse_len([2, 1], RecordPhase::Handshake).is_err(),
            "513 > 512"
        );
        assert!(parse_len([0, 16], RecordPhase::Transport).is_err());
        assert_eq!(parse_len([0, 17], RecordPhase::Transport).unwrap(), 17);
        assert_eq!(
            parse_len([0xff, 0xff], RecordPhase::Transport).unwrap(),
            65_535
        );
        assert!(frame_handshake(&[0; 513]).is_err());
        assert_eq!(frame_handshake(&[7; 3]).unwrap(), vec![0, 3, 7, 7, 7]);
    }
}
