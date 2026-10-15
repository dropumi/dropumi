// SPDX-License-Identifier: MPL-2.0
//! `Noise_XX_25519_ChaChaPoly_SHA256` with committed-SAS payloads, as a
//! typestate API.
//!
//! ```text
//! I→R msg1  -> e               payload empty (exactly 32 bytes)
//! R→I msg2  <- e, ee, s, es    payload {1: sas_commit}
//!           [initiator verifies the responder's static key here]
//! I→R msg3  -> s, se           payload {1: sas_nonce}
//! ```
//!
//! The initiator cannot produce message 3 without first holding an
//! [`InitiatorVerify`], so its own identity is revealed only after the
//! caller has checked the responder's key. All randomness used for the SAS
//! is passed in; ephemeral keys come from the OS generator inside the
//! Noise library.

use std::fmt;
use std::sync::Arc;

use dropumi_domain::identity::{Fingerprint, PublicKey};
use minicbor::Encoder;
use snow::{Builder, HandshakeState};

use crate::codec::Reader;
use crate::error::ProtocolError;
use crate::identity::{StaticSecret, fingerprint_of};
use crate::limits::{MAX_HANDSHAKE_MSG, MAX_HANDSHAKE_PAYLOAD, MSG1_LEN, NOISE_PARAMS};
use crate::record::{Opener, Sealer};
use crate::sas::{self, SasCode};
use crate::version::Prologue;

fn builder<'a>(
    secret: &'a StaticSecret,
    prologue: &'a Prologue,
) -> Result<Builder<'a>, ProtocolError> {
    let params = NOISE_PARAMS.parse().map_err(|_| ProtocolError::Handshake)?;
    Builder::new(params)
        .local_private_key(secret.as_storage_bytes())
        .and_then(|b| b.prologue(prologue.as_bytes()))
        .map_err(|_| ProtocolError::Handshake)
}

/// Encodes `{1: bytes32}`.
fn encode_payload(value: &[u8; 32]) -> Result<Vec<u8>, ProtocolError> {
    let mut out = Vec::with_capacity(40);
    Encoder::new(&mut out)
        .map(1)
        .and_then(|e| e.u64(1))
        .and_then(|e| e.bytes(value))
        .map_err(|_| ProtocolError::Encode)?;
    Ok(out)
}

/// Decodes `{1: bytes32, ...}` (unknown keys skipped).
fn decode_payload(payload: &[u8]) -> Result<[u8; 32], ProtocolError> {
    if payload.len() > MAX_HANDSHAKE_PAYLOAD {
        return Err(ProtocolError::LimitExceeded("handshake payload"));
    }
    let mut r = Reader::new(payload);
    let mut m = r.map(1)?;
    let mut value = None;
    while let Some(k) = m.next_key(&mut r)? {
        match k {
            1 => value = Some(r.bytes32()?),
            _ => r.skip_value(m.value_depth())?,
        }
    }
    r.finish()?;
    value.ok_or(ProtocolError::Malformed("missing handshake payload value"))
}

fn remote_static(hs: &HandshakeState) -> Result<PublicKey, ProtocolError> {
    let rs: [u8; 32] = hs
        .get_remote_static()
        .ok_or(ProtocolError::Handshake)?
        .try_into()
        .map_err(|_| ProtocolError::Handshake)?;
    Ok(PublicKey::from_bytes(rs))
}

fn handshake_hash(hs: &HandshakeState) -> Result<[u8; 32], ProtocolError> {
    hs.get_handshake_hash()
        .try_into()
        .map_err(|_| ProtocolError::Handshake)
}

/// Entry point for the side that opened the connection.
#[derive(Debug)]
pub struct Initiator;

impl Initiator {
    /// Starts the handshake and returns message 1.
    ///
    /// # Errors
    /// [`ProtocolError::Handshake`] if the Noise library fails.
    pub fn start(
        secret: &StaticSecret,
        prologue: &Prologue,
        n_i: [u8; 32],
    ) -> Result<(InitiatorAwaitMsg2, Vec<u8>), ProtocolError> {
        let mut hs = builder(secret, prologue)?
            .build_initiator()
            .map_err(|_| ProtocolError::Handshake)?;
        // The Noise library needs headroom beyond the 32-byte result.
        let mut msg1 = vec![0u8; MAX_HANDSHAKE_MSG];
        let n = hs
            .write_message(&[], &mut msg1)
            .map_err(|_| ProtocolError::Handshake)?;
        if n != MSG1_LEN {
            return Err(ProtocolError::Handshake);
        }
        msg1.truncate(n);
        Ok((InitiatorAwaitMsg2 { hs, n_i }, msg1))
    }
}

/// Initiator waiting for message 2.
pub struct InitiatorAwaitMsg2 {
    hs: HandshakeState,
    n_i: [u8; 32],
}

impl InitiatorAwaitMsg2 {
    /// Reads message 2 and learns the responder's static key.
    ///
    /// # Errors
    /// [`ProtocolError::Handshake`] for tampered messages or a wrong
    /// prologue; payload decoding errors otherwise.
    pub fn read_msg2(mut self, msg2: &[u8]) -> Result<InitiatorVerify, ProtocolError> {
        if msg2.len() > MAX_HANDSHAKE_MSG {
            return Err(ProtocolError::LimitExceeded("handshake message"));
        }
        let mut payload = vec![0u8; MAX_HANDSHAKE_MSG];
        let n = self
            .hs
            .read_message(msg2, &mut payload)
            .map_err(|_| ProtocolError::Handshake)?;
        let commitment = decode_payload(payload.get(..n).unwrap_or_default())?;
        let remote = remote_static(&self.hs)?;
        Ok(InitiatorVerify {
            hs: self.hs,
            n_i: self.n_i,
            commitment,
            remote,
        })
    }
}

impl fmt::Debug for InitiatorAwaitMsg2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InitiatorAwaitMsg2")
    }
}

/// Initiator holding the authenticated responder key, before revealing its
/// own identity.
pub struct InitiatorVerify {
    hs: HandshakeState,
    n_i: [u8; 32],
    commitment: [u8; 32],
    remote: PublicKey,
}

impl InitiatorVerify {
    /// The responder's authenticated static key.
    #[must_use]
    pub const fn remote_static(&self) -> PublicKey {
        self.remote
    }

    /// Fingerprint of the responder's key.
    #[must_use]
    pub fn remote_fingerprint(&self) -> Fingerprint {
        fingerprint_of(&self.remote)
    }

    /// The responder's SAS commitment.
    #[must_use]
    pub const fn commitment(&self) -> [u8; 32] {
        self.commitment
    }

    /// Writes message 3 (revealing this device's identity). Call only after
    /// the responder's key has been checked.
    ///
    /// # Errors
    /// [`ProtocolError::Handshake`] if the Noise library fails.
    pub fn proceed(mut self) -> Result<(InitiatorDone, Vec<u8>), ProtocolError> {
        let payload = encode_payload(&self.n_i)?;
        let mut msg3 = vec![0u8; MAX_HANDSHAKE_MSG];
        let n = self
            .hs
            .write_message(&payload, &mut msg3)
            .map_err(|_| ProtocolError::Handshake)?;
        msg3.truncate(n);
        let hash = handshake_hash(&self.hs)?;
        let ts = self
            .hs
            .into_stateless_transport_mode()
            .map_err(|_| ProtocolError::Handshake)?;
        let ts = Arc::new(ts);
        Ok((
            InitiatorDone {
                outcome: HandshakeOutcome {
                    remote_static: self.remote,
                    remote_fingerprint: fingerprint_of(&self.remote),
                    handshake_hash: hash,
                    n_i: self.n_i,
                    responder_nonce: ResponderNonce::Committed(self.commitment),
                    sealer: Sealer::new(Arc::clone(&ts)),
                    opener: Opener::new(ts),
                },
            },
            msg3,
        ))
    }
}

impl fmt::Debug for InitiatorVerify {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InitiatorVerify")
            .field("remote", &self.remote_fingerprint())
            .finish_non_exhaustive()
    }
}

/// Initiator after message 3.
#[derive(Debug)]
pub struct InitiatorDone {
    outcome: HandshakeOutcome,
}

impl InitiatorDone {
    /// Session keys and SAS material.
    #[must_use]
    pub fn finish(self) -> HandshakeOutcome {
        self.outcome
    }
}

/// Entry point for the side that accepted the connection.
#[derive(Debug)]
pub struct Responder;

impl Responder {
    /// Prepares to read message 1.
    ///
    /// # Errors
    /// [`ProtocolError::Handshake`] if the Noise library fails.
    pub fn start(
        secret: &StaticSecret,
        prologue: &Prologue,
        n_r: [u8; 32],
    ) -> Result<ResponderAwaitMsg1, ProtocolError> {
        let hs = builder(secret, prologue)?
            .build_responder()
            .map_err(|_| ProtocolError::Handshake)?;
        Ok(ResponderAwaitMsg1 { hs, n_r })
    }
}

/// Responder waiting for message 1.
pub struct ResponderAwaitMsg1 {
    hs: HandshakeState,
    n_r: [u8; 32],
}

impl ResponderAwaitMsg1 {
    /// Reads message 1 (exactly 32 bytes, empty payload) and returns
    /// message 2.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for a wrong length,
    /// [`ProtocolError::Handshake`] otherwise.
    pub fn read_msg1(
        mut self,
        msg1: &[u8],
    ) -> Result<(ResponderAwaitMsg3, Vec<u8>), ProtocolError> {
        if msg1.len() != MSG1_LEN {
            return Err(ProtocolError::Malformed(
                "message 1 must be exactly 32 bytes",
            ));
        }
        let mut scratch = [0u8; MAX_HANDSHAKE_MSG];
        let n = self
            .hs
            .read_message(msg1, &mut scratch)
            .map_err(|_| ProtocolError::Handshake)?;
        if n != 0 {
            return Err(ProtocolError::Malformed("message 1 payload must be empty"));
        }
        let payload = encode_payload(&sas::commit(&self.n_r))?;
        let mut msg2 = vec![0u8; MAX_HANDSHAKE_MSG];
        let n = self
            .hs
            .write_message(&payload, &mut msg2)
            .map_err(|_| ProtocolError::Handshake)?;
        msg2.truncate(n);
        Ok((
            ResponderAwaitMsg3 {
                hs: self.hs,
                n_r: self.n_r,
            },
            msg2,
        ))
    }
}

impl fmt::Debug for ResponderAwaitMsg1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ResponderAwaitMsg1")
    }
}

/// Responder waiting for message 3.
pub struct ResponderAwaitMsg3 {
    hs: HandshakeState,
    n_r: [u8; 32],
}

impl ResponderAwaitMsg3 {
    /// Reads message 3 and learns the initiator's static key.
    ///
    /// # Errors
    /// [`ProtocolError::Handshake`] for tampered or replayed messages.
    pub fn read_msg3(mut self, msg3: &[u8]) -> Result<ResponderDone, ProtocolError> {
        if msg3.len() > MAX_HANDSHAKE_MSG {
            return Err(ProtocolError::LimitExceeded("handshake message"));
        }
        let mut payload = vec![0u8; MAX_HANDSHAKE_MSG];
        let n = self
            .hs
            .read_message(msg3, &mut payload)
            .map_err(|_| ProtocolError::Handshake)?;
        let n_i = decode_payload(payload.get(..n).unwrap_or_default())?;
        let remote = remote_static(&self.hs)?;
        let hash = handshake_hash(&self.hs)?;
        let ts = Arc::new(
            self.hs
                .into_stateless_transport_mode()
                .map_err(|_| ProtocolError::Handshake)?,
        );
        Ok(ResponderDone {
            outcome: HandshakeOutcome {
                remote_static: remote,
                remote_fingerprint: fingerprint_of(&remote),
                handshake_hash: hash,
                n_i,
                responder_nonce: ResponderNonce::Known(self.n_r),
                sealer: Sealer::new(Arc::clone(&ts)),
                opener: Opener::new(ts),
            },
        })
    }
}

impl fmt::Debug for ResponderAwaitMsg3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ResponderAwaitMsg3")
    }
}

/// Responder after message 3.
#[derive(Debug)]
pub struct ResponderDone {
    outcome: HandshakeOutcome,
}

impl ResponderDone {
    /// Session keys and SAS material.
    #[must_use]
    pub fn finish(self) -> HandshakeOutcome {
        self.outcome
    }
}

/// What a side knows about the responder's SAS nonce.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResponderNonce {
    /// Initiator: only the commitment, until the Hello reveal.
    Committed([u8; 32]),
    /// Responder: the nonce itself (to reveal in its Hello).
    Known([u8; 32]),
}

impl fmt::Debug for ResponderNonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Committed(_) => f.write_str("Committed(..)"),
            Self::Known(_) => f.write_str("Known(..)"),
        }
    }
}

/// A completed handshake.
#[derive(Debug)]
pub struct HandshakeOutcome {
    /// The peer's authenticated static key.
    pub remote_static: PublicKey,
    /// Fingerprint of the peer's key.
    pub remote_fingerprint: Fingerprint,
    /// Final Noise handshake hash `h` (channel binding).
    pub handshake_hash: [u8; 32],
    /// The initiator's SAS nonce.
    pub n_i: [u8; 32],
    /// The responder's SAS nonce or its commitment.
    pub responder_nonce: ResponderNonce,
    /// Outgoing record encryption.
    pub sealer: Sealer,
    /// Incoming record decryption.
    pub opener: Opener,
}

impl HandshakeOutcome {
    /// Responder: the nonce to reveal in Hello.
    #[must_use]
    pub const fn sas_reveal(&self) -> Option<[u8; 32]> {
        match self.responder_nonce {
            ResponderNonce::Known(n) => Some(n),
            ResponderNonce::Committed(_) => None,
        }
    }

    /// Responder: the SAS code. Initiator: the code after checking the
    /// revealed nonce against the commitment.
    ///
    /// # Errors
    /// [`ProtocolError::SasCommitmentMismatch`] if the reveal does not match;
    /// [`ProtocolError::Violation`] if an initiator has no reveal.
    pub fn sas(&self, reveal: Option<&[u8; 32]>) -> Result<SasCode, ProtocolError> {
        match (self.responder_nonce, reveal) {
            (ResponderNonce::Known(n_r), _) => {
                Ok(SasCode::derive(&self.handshake_hash, &self.n_i, &n_r))
            }
            (ResponderNonce::Committed(c), Some(n_r)) => {
                sas::verify_reveal(&c, n_r)?;
                Ok(SasCode::derive(&self.handshake_hash, &self.n_i, n_r))
            }
            (ResponderNonce::Committed(_), None) => {
                Err(ProtocolError::Violation("missing SAS reveal"))
            }
        }
    }
}
