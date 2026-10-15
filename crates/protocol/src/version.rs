// SPDX-License-Identifier: MPL-2.0
//! Cleartext version preamble, version selection and the Noise prologue.
//!
//! ```text
//! I→R Preamble       "DRPM" 0x01 n:u8(1..=8) n×u16be (strictly descending)
//! R→I PreambleReply  "DRPM" 0x01 status:u8
//!                    0 OK + selected:u16be
//!                    1 NO_COMMON_VERSION + n:u8 + n×u16be (then close)
//!                    2 BUSY (then close)
//! ```
//!
//! Both sides bind the exact preamble and reply bytes into the Noise
//! prologue, so tampering makes the handshake fail: an attacker can force a
//! denial of service but never a downgrade.

use crate::error::ProtocolError;
use crate::limits::MAX_VERSIONS;

/// Preamble magic.
pub const MAGIC: [u8; 4] = *b"DRPM";
/// Preamble format version.
pub const PREAMBLE_FORMAT: u8 = 1;
/// Length of the fixed preamble header (magic, format, count).
pub const PREAMBLE_HEADER_LEN: usize = 6;
/// Length of the fixed reply header (magic, format, status).
pub const REPLY_HEADER_LEN: usize = 6;
/// Prologue label.
pub const PROLOGUE_LABEL: &[u8] = b"DROPUMI-PROLOGUE";

/// The initiator's version offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preamble {
    versions: Vec<u16>,
}

impl Preamble {
    /// Builds a preamble.
    ///
    /// # Errors
    /// [`ProtocolError::BadPreamble`] unless there are 1..=8 strictly
    /// descending versions.
    pub fn new(versions: &[u16]) -> Result<Self, ProtocolError> {
        validate_versions(versions)?;
        Ok(Self {
            versions: versions.to_vec(),
        })
    }

    /// Offered versions, highest first.
    #[must_use]
    pub fn versions(&self) -> &[u16] {
        &self.versions
    }

    /// Wire encoding.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(PREAMBLE_HEADER_LEN.saturating_add(2 * MAX_VERSIONS));
        out.extend_from_slice(&MAGIC);
        out.push(PREAMBLE_FORMAT);
        out.push(u8::try_from(self.versions.len()).unwrap_or(0));
        for v in &self.versions {
            out.extend_from_slice(&v.to_be_bytes());
        }
        out
    }

    /// Validates the fixed header and returns the number of body bytes
    /// (2 × count) to read next.
    ///
    /// # Errors
    /// [`ProtocolError::BadPreamble`] for a bad magic, format or count.
    pub fn decode_header(header: &[u8; PREAMBLE_HEADER_LEN]) -> Result<usize, ProtocolError> {
        let [m0, m1, m2, m3, fmt, count] = *header;
        if [m0, m1, m2, m3] != MAGIC || fmt != PREAMBLE_FORMAT {
            return Err(ProtocolError::BadPreamble);
        }
        let count = usize::from(count);
        if count == 0 || count > MAX_VERSIONS {
            return Err(ProtocolError::BadPreamble);
        }
        Ok(count.saturating_mul(2))
    }

    /// Decodes the body that follows a valid header.
    ///
    /// # Errors
    /// [`ProtocolError::BadPreamble`] for an odd length or unordered versions.
    pub fn decode_body(body: &[u8]) -> Result<Self, ProtocolError> {
        let versions = decode_u16s(body)?;
        validate_versions(&versions)?;
        Ok(Self { versions })
    }
}

/// The responder's answer to a preamble.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreambleReply {
    /// Version selected; the handshake follows.
    Ok(u16),
    /// No common version; the responder lists its versions and closes.
    NoCommonVersion(Vec<u16>),
    /// The responder is at capacity and closes.
    Busy,
}

impl PreambleReply {
    /// Wire encoding.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        out.extend_from_slice(&MAGIC);
        out.push(PREAMBLE_FORMAT);
        match self {
            Self::Ok(v) => {
                out.push(0);
                out.extend_from_slice(&v.to_be_bytes());
            }
            Self::NoCommonVersion(vs) => {
                out.push(1);
                let n = vs.len().min(MAX_VERSIONS);
                out.push(u8::try_from(n).unwrap_or(0));
                for v in vs.iter().take(n) {
                    out.extend_from_slice(&v.to_be_bytes());
                }
            }
            Self::Busy => out.push(2),
        }
        out
    }

    /// Validates the reply header and returns its status byte.
    ///
    /// # Errors
    /// [`ProtocolError::BadPreamble`] for a bad magic, format or status.
    pub fn decode_header(header: &[u8; REPLY_HEADER_LEN]) -> Result<u8, ProtocolError> {
        let [m0, m1, m2, m3, fmt, status] = *header;
        if [m0, m1, m2, m3] != MAGIC || fmt != PREAMBLE_FORMAT || status > 2 {
            return Err(ProtocolError::BadPreamble);
        }
        Ok(status)
    }

    /// Decodes an OK body (2 bytes).
    ///
    /// # Errors
    /// [`ProtocolError::BadPreamble`] for a wrong length.
    pub fn decode_ok(body: &[u8]) -> Result<Self, ProtocolError> {
        match body {
            [a, b] => Ok(Self::Ok(u16::from_be_bytes([*a, *b]))),
            _ => Err(ProtocolError::BadPreamble),
        }
    }

    /// Validates the version count of a NO_COMMON_VERSION body and returns
    /// the number of bytes to read.
    ///
    /// # Errors
    /// [`ProtocolError::BadPreamble`] for a count above 8.
    pub fn no_common_body_len(count: u8) -> Result<usize, ProtocolError> {
        let n = usize::from(count);
        if n > MAX_VERSIONS {
            return Err(ProtocolError::BadPreamble);
        }
        Ok(n.saturating_mul(2))
    }

    /// Decodes the versions listed in a NO_COMMON_VERSION body.
    ///
    /// # Errors
    /// [`ProtocolError::BadPreamble`] for an odd length.
    pub fn decode_no_common(body: &[u8]) -> Result<Self, ProtocolError> {
        Ok(Self::NoCommonVersion(decode_u16s(body)?))
    }
}

fn decode_u16s(body: &[u8]) -> Result<Vec<u16>, ProtocolError> {
    let (pairs, rest) = body.as_chunks::<2>();
    if !rest.is_empty() || pairs.len() > MAX_VERSIONS {
        return Err(ProtocolError::BadPreamble);
    }
    Ok(pairs.iter().map(|p| u16::from_be_bytes(*p)).collect())
}

fn validate_versions(versions: &[u16]) -> Result<(), ProtocolError> {
    if versions.is_empty() || versions.len() > MAX_VERSIONS {
        return Err(ProtocolError::BadPreamble);
    }
    if versions.windows(2).any(|w| matches!(w, [a, b] if a <= b)) {
        return Err(ProtocolError::BadPreamble);
    }
    Ok(())
}

/// The highest version in both lists.
#[must_use]
pub fn select_version(ours: &[u16], theirs: &[u16]) -> Option<u16> {
    ours.iter().copied().filter(|v| theirs.contains(v)).max()
}

/// The Noise prologue: `"DROPUMI-PROLOGUE" ‖ u8 len(P) ‖ P ‖ u8 len(R) ‖ R`
/// over the exact preamble (`P`) and reply (`R`) bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prologue(Vec<u8>);

impl Prologue {
    /// Builds the prologue from the bytes sent and received.
    ///
    /// # Errors
    /// [`ProtocolError::BadPreamble`] if either part exceeds 255 bytes.
    pub fn new(preamble: &[u8], reply: &[u8]) -> Result<Self, ProtocolError> {
        let p_len = u8::try_from(preamble.len()).map_err(|_| ProtocolError::BadPreamble)?;
        let r_len = u8::try_from(reply.len()).map_err(|_| ProtocolError::BadPreamble)?;
        let mut v = Vec::with_capacity(64);
        v.extend_from_slice(PROLOGUE_LABEL);
        v.push(p_len);
        v.extend_from_slice(preamble);
        v.push(r_len);
        v.extend_from_slice(reply);
        Ok(Self(v))
    }

    /// Prologue bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preamble_roundtrip() {
        let p = Preamble::new(&[3, 1]).unwrap();
        let bytes = p.encode();
        assert_eq!(&bytes[..6], b"DRPM\x01\x02");
        let header: [u8; 6] = bytes[..6].try_into().unwrap();
        let body_len = Preamble::decode_header(&header).unwrap();
        assert_eq!(Preamble::decode_body(&bytes[6..6 + body_len]).unwrap(), p);
        for r in [
            PreambleReply::Ok(1),
            PreambleReply::NoCommonVersion(vec![2, 1]),
            PreambleReply::Busy,
        ] {
            let b = r.encode();
            let status = PreambleReply::decode_header(&b[..6].try_into().unwrap()).unwrap();
            let decoded = match status {
                0 => PreambleReply::decode_ok(&b[6..]).unwrap(),
                1 => {
                    let n = PreambleReply::no_common_body_len(b[6]).unwrap();
                    PreambleReply::decode_no_common(&b[7..7 + n]).unwrap()
                }
                _ => PreambleReply::Busy,
            };
            assert_eq!(decoded, r);
        }
    }

    #[test]
    fn preamble_rejects_bad_magic_format_count_order() {
        assert!(Preamble::decode_header(b"XRPM\x01\x01").is_err());
        assert!(Preamble::decode_header(b"DRPM\x02\x01").is_err());
        assert!(Preamble::decode_header(b"DRPM\x01\x00").is_err());
        assert!(Preamble::decode_header(b"DRPM\x01\x09").is_err());
        assert!(Preamble::decode_body(&[0, 1, 0, 2]).is_err(), "ascending");
        assert!(Preamble::decode_body(&[0, 1, 0, 1]).is_err(), "duplicate");
        assert!(Preamble::decode_body(&[0, 1, 0]).is_err(), "odd");
        assert!(Preamble::new(&[]).is_err());
        assert!(PreambleReply::decode_header(b"DRPM\x01\x03").is_err());
        assert!(PreambleReply::no_common_body_len(9).is_err());
        assert!(PreambleReply::decode_ok(&[0]).is_err());
    }

    #[test]
    fn select_highest_common() {
        assert_eq!(select_version(&[3, 2, 1], &[2, 1]), Some(2));
        assert_eq!(select_version(&[1], &[2]), None);
        assert_eq!(select_version(&[1], &[5, 1]), Some(1));
    }

    #[test]
    fn prologue_layout() {
        let p = Prologue::new(b"ab", b"xyz").unwrap();
        assert_eq!(p.as_bytes(), b"DROPUMI-PROLOGUE\x02ab\x03xyz");
    }
}
