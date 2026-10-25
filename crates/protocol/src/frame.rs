// SPDX-License-Identifier: MPL-2.0
//! Plaintext frames carried inside transport records.
//!
//! Byte 0 is the frame type: `0x01` CONTROL (exactly one CBOR message) or
//! `0x02` DATA (1..=65,518 bytes of file content). One record carries
//! exactly one frame; there is no fragmentation.

use crate::error::ProtocolError;
use crate::limits::MAX_FRAME_BODY;

/// Frame type byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    /// One CBOR control message.
    Control = 1,
    /// File content.
    Data = 2,
}

/// A decoded frame borrowing the record plaintext.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame<'a> {
    /// CBOR control message bytes.
    Control(&'a [u8]),
    /// File content bytes.
    Data(&'a [u8]),
}

impl<'a> Frame<'a> {
    /// Decodes a record plaintext.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for an empty plaintext, an empty body or
    /// an unknown type; [`ProtocolError::LimitExceeded`] for a body over
    /// 65,518 bytes.
    pub fn decode(plaintext: &'a [u8]) -> Result<Self, ProtocolError> {
        let (&ty, body) = plaintext
            .split_first()
            .ok_or(ProtocolError::Malformed("empty frame"))?;
        if body.is_empty() {
            return Err(ProtocolError::Malformed("empty frame body"));
        }
        if body.len() > MAX_FRAME_BODY {
            return Err(ProtocolError::LimitExceeded("frame body"));
        }
        match ty {
            1 => Ok(Self::Control(body)),
            2 => Ok(Self::Data(body)),
            _ => Err(ProtocolError::Malformed("unknown frame type")),
        }
    }

    /// Appends the frame encoding (type byte plus body) to `out`.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for an empty body and
    /// [`ProtocolError::LimitExceeded`] for a body over 65,518 bytes.
    pub fn encode_into(&self, out: &mut Vec<u8>) -> Result<(), ProtocolError> {
        let (ty, body) = match self {
            Self::Control(b) => (FrameType::Control, *b),
            Self::Data(b) => (FrameType::Data, *b),
        };
        if body.is_empty() {
            return Err(ProtocolError::Malformed("empty frame body"));
        }
        if body.len() > MAX_FRAME_BODY {
            return Err(ProtocolError::LimitExceeded("frame body"));
        }
        out.push(ty as u8);
        out.extend_from_slice(body);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip_and_bounds() {
        let mut out = Vec::new();
        Frame::Data(b"abc").encode_into(&mut out).unwrap();
        assert_eq!(out, b"\x02abc");
        assert_eq!(Frame::decode(&out).unwrap(), Frame::Data(b"abc"));
        assert!(Frame::decode(&[]).is_err());
        assert!(Frame::decode(&[2]).is_err());
        assert!(Frame::decode(&[3, 0]).is_err());
        let max = vec![0u8; MAX_FRAME_BODY];
        let mut out = Vec::new();
        Frame::Data(&max).encode_into(&mut out).unwrap();
        assert_eq!(out.len(), MAX_FRAME_BODY + 1);
        let big = vec![0u8; MAX_FRAME_BODY + 1];
        assert!(Frame::Data(&big).encode_into(&mut Vec::new()).is_err());
        assert!(Frame::Control(&[]).encode_into(&mut Vec::new()).is_err());
    }
}
