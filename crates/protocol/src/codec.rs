// SPDX-License-Identifier: MPL-2.0
//! Strict CBOR reading.
//!
//! Accepted subset: definite-length arrays, maps, byte and text strings;
//! unsigned and negative integers that fit in 64 bits; booleans. Rejected:
//! indefinite lengths, tags, floats, null, undefined, other simple values,
//! reserved encodings and trailing bytes. Map keys are unsigned integers in
//! strictly ascending order (so duplicates are rejected), maps have at most
//! 32 entries and nesting is at most 4 levels. Declared lengths never cause
//! allocation: strings are borrowed and array counts are checked before use.

use minicbor::Decoder;
use minicbor::data::Type;

use crate::error::ProtocolError;
use crate::limits::{MAX_DEPTH, MAX_MAP_ENTRIES};

/// Maximum elements in an array skipped as an unknown value.
const MAX_SKIPPED_ARRAY: u64 = 65_535;

fn malformed(_: minicbor::decode::Error) -> ProtocolError {
    ProtocolError::Malformed("invalid CBOR")
}

/// A strict CBOR reader over one complete buffer.
#[derive(Debug)]
pub struct Reader<'b> {
    d: Decoder<'b>,
    len: usize,
}

impl<'b> Reader<'b> {
    /// Reads from `buf`.
    #[must_use]
    pub fn new(buf: &'b [u8]) -> Self {
        Self {
            d: Decoder::new(buf),
            len: buf.len(),
        }
    }

    /// Fails unless every byte was consumed.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for trailing bytes.
    pub fn finish(&self) -> Result<(), ProtocolError> {
        if self.d.position() == self.len {
            Ok(())
        } else {
            Err(ProtocolError::Malformed("trailing bytes"))
        }
    }

    fn datatype(&self) -> Result<Type, ProtocolError> {
        self.d.datatype().map_err(malformed)
    }

    /// An unsigned integer.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for any other item.
    pub fn uint(&mut self) -> Result<u64, ProtocolError> {
        match self.datatype()? {
            Type::U8 | Type::U16 | Type::U32 | Type::U64 => self.d.u64().map_err(malformed),
            _ => Err(ProtocolError::Malformed("expected unsigned integer")),
        }
    }

    /// An unsigned integer no larger than `max`.
    ///
    /// # Errors
    /// [`ProtocolError::LimitExceeded`] above `max`.
    pub fn uint_max(&mut self, max: u64, what: &'static str) -> Result<u64, ProtocolError> {
        let v = self.uint()?;
        if v > max {
            return Err(ProtocolError::LimitExceeded(what));
        }
        Ok(v)
    }

    /// A `u16`.
    ///
    /// # Errors
    /// [`ProtocolError::LimitExceeded`] above `u16::MAX`.
    pub fn u16(&mut self, what: &'static str) -> Result<u16, ProtocolError> {
        let v = self.uint_max(u64::from(u16::MAX), what)?;
        u16::try_from(v).map_err(|_| ProtocolError::LimitExceeded(what))
    }

    /// A `u32` no larger than `max`.
    ///
    /// # Errors
    /// [`ProtocolError::LimitExceeded`] above `max`.
    pub fn u32_max(&mut self, max: u32, what: &'static str) -> Result<u32, ProtocolError> {
        let v = self.uint_max(u64::from(max), what)?;
        u32::try_from(v).map_err(|_| ProtocolError::LimitExceeded(what))
    }

    /// A signed integer that fits in `i64`.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for any other item.
    pub fn int(&mut self) -> Result<i64, ProtocolError> {
        match self.datatype()? {
            Type::U8 | Type::U16 | Type::U32 | Type::U64 => {
                let v = self.d.u64().map_err(malformed)?;
                i64::try_from(v).map_err(|_| ProtocolError::Malformed("integer out of range"))
            }
            Type::I8 | Type::I16 | Type::I32 | Type::I64 => self.d.i64().map_err(malformed),
            _ => Err(ProtocolError::Malformed("expected integer")),
        }
    }

    /// A boolean.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for any other item.
    pub fn bool(&mut self) -> Result<bool, ProtocolError> {
        match self.datatype()? {
            Type::Bool => self.d.bool().map_err(malformed),
            _ => Err(ProtocolError::Malformed("expected bool")),
        }
    }

    /// A definite UTF-8 text string of at most `max` bytes.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for other items or invalid UTF-8;
    /// [`ProtocolError::LimitExceeded`] above `max`.
    pub fn text(&mut self, max: usize, what: &'static str) -> Result<&'b str, ProtocolError> {
        match self.datatype()? {
            Type::String => {
                let s = self.d.str().map_err(malformed)?;
                if s.len() > max {
                    return Err(ProtocolError::LimitExceeded(what));
                }
                Ok(s)
            }
            _ => Err(ProtocolError::Malformed("expected text string")),
        }
    }

    /// A definite byte string of exactly `n` bytes.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for other items or lengths.
    pub fn bytes_exact(&mut self, n: usize) -> Result<&'b [u8], ProtocolError> {
        match self.datatype()? {
            Type::Bytes => {
                let b = self.d.bytes().map_err(malformed)?;
                if b.len() != n {
                    return Err(ProtocolError::Malformed("wrong byte string length"));
                }
                Ok(b)
            }
            _ => Err(ProtocolError::Malformed("expected byte string")),
        }
    }

    /// A byte string of exactly 32 bytes.
    ///
    /// # Errors
    /// See [`Reader::bytes_exact`].
    pub fn bytes32(&mut self) -> Result<[u8; 32], ProtocolError> {
        let b = self.bytes_exact(32)?;
        b.try_into()
            .map_err(|_| ProtocolError::Malformed("wrong byte string length"))
    }

    /// A definite array header with at most `max` elements.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for other items or indefinite arrays;
    /// [`ProtocolError::LimitExceeded`] above `max`.
    pub fn array_len(&mut self, max: u64, what: &'static str) -> Result<usize, ProtocolError> {
        match self.datatype()? {
            Type::Array => {
                let n = self
                    .d
                    .array()
                    .map_err(malformed)?
                    .ok_or(ProtocolError::Malformed("indefinite array"))?;
                if n > max {
                    return Err(ProtocolError::LimitExceeded(what));
                }
                usize::try_from(n).map_err(|_| ProtocolError::LimitExceeded(what))
            }
            _ => Err(ProtocolError::Malformed("expected definite array")),
        }
    }

    /// A definite map header with at most 32 entries, returned as a reader
    /// that enforces ascending unsigned keys. `depth` is the nesting level
    /// of the map itself (the message array is level 1).
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for other items or indefinite maps;
    /// [`ProtocolError::LimitExceeded`] for too many entries or levels.
    pub fn map(&mut self, depth: usize) -> Result<MapReader, ProtocolError> {
        if depth > MAX_DEPTH {
            return Err(ProtocolError::LimitExceeded("nesting depth"));
        }
        match self.datatype()? {
            Type::Map => {
                let n = self
                    .d
                    .map()
                    .map_err(malformed)?
                    .ok_or(ProtocolError::Malformed("indefinite map"))?;
                if n > MAX_MAP_ENTRIES as u64 {
                    return Err(ProtocolError::LimitExceeded("map entries"));
                }
                Ok(MapReader {
                    remaining: usize::try_from(n)
                        .map_err(|_| ProtocolError::LimitExceeded("map entries"))?,
                    last_key: None,
                    depth,
                })
            }
            _ => Err(ProtocolError::Malformed("expected definite map")),
        }
    }

    /// Skips one value nested at level `depth`, enforcing every strictness
    /// rule.
    ///
    /// # Errors
    /// As for the typed readers; containers beyond level 4 fail with
    /// [`ProtocolError::LimitExceeded`].
    pub fn skip_value(&mut self, depth: usize) -> Result<(), ProtocolError> {
        match self.datatype()? {
            Type::Bool => self.bool().map(drop),
            Type::U8 | Type::U16 | Type::U32 | Type::U64 => self.uint().map(drop),
            Type::I8 | Type::I16 | Type::I32 | Type::I64 => self.int().map(drop),
            Type::Bytes => self.d.bytes().map(drop).map_err(malformed),
            Type::String => self.d.str().map(drop).map_err(malformed),
            Type::Array => {
                if depth > MAX_DEPTH {
                    return Err(ProtocolError::LimitExceeded("nesting depth"));
                }
                let n = self.array_len(MAX_SKIPPED_ARRAY, "array length")?;
                for _ in 0..n {
                    self.skip_value(depth.saturating_add(1))?;
                }
                Ok(())
            }
            Type::Map => {
                let mut m = self.map(depth)?;
                while m.next_key(self)?.is_some() {
                    self.skip_value(depth.saturating_add(1))?;
                }
                Ok(())
            }
            _ => Err(ProtocolError::Malformed("forbidden CBOR item")),
        }
    }
}

/// Iterates the keys of a definite map in strictly ascending order.
#[derive(Debug)]
pub struct MapReader {
    remaining: usize,
    last_key: Option<u64>,
    depth: usize,
}

impl MapReader {
    /// Reads the next key, or `None` when the map is exhausted. The caller
    /// must read or skip the value before asking for the next key.
    ///
    /// # Errors
    /// [`ProtocolError::Malformed`] for non-integer, duplicate or unsorted
    /// keys.
    pub fn next_key(&mut self, r: &mut Reader<'_>) -> Result<Option<u64>, ProtocolError> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining = self.remaining.saturating_sub(1);
        let key = r.uint()?;
        if self.last_key.is_some_and(|last| key <= last) {
            return Err(ProtocolError::Malformed("map keys not strictly ascending"));
        }
        self.last_key = Some(key);
        Ok(Some(key))
    }

    /// Nesting level of values in this map.
    #[must_use]
    pub const fn value_depth(&self) -> usize {
        self.depth.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip_all(buf: &[u8]) -> Result<(), ProtocolError> {
        let mut r = Reader::new(buf);
        r.skip_value(1)?;
        r.finish()
    }

    #[test]
    fn rejects_indefinite() {
        assert!(skip_all(&[0x9f, 0x01, 0xff]).is_err());
        assert!(skip_all(&[0xbf, 0x01, 0x01, 0xff]).is_err());
        assert!(skip_all(&[0x5f, 0x41, 0x00, 0xff]).is_err());
        assert!(skip_all(&[0x7f, 0x61, 0x61, 0xff]).is_err());
    }

    #[test]
    fn rejects_duplicate_or_unsorted_keys() {
        assert!(skip_all(&[0xa2, 0x01, 0x00, 0x01, 0x00]).is_err());
        assert!(skip_all(&[0xa2, 0x02, 0x00, 0x01, 0x00]).is_err());
        assert!(skip_all(&[0xa2, 0x01, 0x00, 0x02, 0x00]).is_ok());
        assert!(skip_all(&[0xa1, 0x61, 0x61, 0x00]).is_err(), "text key");
    }

    #[test]
    fn rejects_tags_floats_undefined() {
        assert!(skip_all(&[0xc1, 0x00]).is_err());
        assert!(skip_all(&[0xf9, 0x3c, 0x00]).is_err());
        assert!(skip_all(&[0xfb, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(skip_all(&[0xf7]).is_err());
        assert!(skip_all(&[0xf6]).is_err());
        assert!(skip_all(&[0xe0]).is_err());
        assert!(skip_all(&[0x1c]).is_err(), "reserved additional info");
        assert!(skip_all(&[0xf5]).is_ok());
        assert!(
            skip_all(&[0x3b, 0x80, 0, 0, 0, 0, 0, 0, 0]).is_err(),
            "below i64::MIN"
        );
    }

    #[test]
    fn rejects_depth_over_4() {
        // [[[[0]]]] is level 4 at the innermost array: allowed.
        assert!(skip_all(&[0x81, 0x81, 0x81, 0x81, 0x00]).is_ok());
        assert!(skip_all(&[0x81, 0x81, 0x81, 0x81, 0x81, 0x00]).is_err());
    }

    #[test]
    fn rejects_trailing_bytes() {
        assert!(skip_all(&[0x00, 0x00]).is_err());
        assert!(skip_all(&[0x00]).is_ok());
    }

    #[test]
    fn enforces_string_and_array_limits() {
        let mut r = Reader::new(&[0x63, b'a', b'b', b'c']);
        assert!(r.text(2, "t").is_err());
        let mut r = Reader::new(&[0x63, b'a', b'b', b'c']);
        assert_eq!(r.text(3, "t").unwrap(), "abc");
        let mut r = Reader::new(&[0x62, 0xff, 0xfe]);
        assert!(r.text(10, "t").is_err(), "invalid UTF-8");
        let mut r = Reader::new(&[0x83, 0, 0, 0]);
        assert!(r.array_len(2, "a").is_err());
        // A huge declared length must not allocate or panic.
        let mut r = Reader::new(&[0x5b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        assert!(r.bytes_exact(32).is_err());
        let mut r = Reader::new(&[0x9b, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff]);
        assert!(r.array_len(10_000, "a").is_err());
        assert!(skip_all(&[0xb8, 0x21]).is_err(), "33 map entries");
    }
}
