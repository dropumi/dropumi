// SPDX-License-Identifier: MPL-2.0
//! Device identity: the static public key and its fingerprint formats.
//!
//! The domain layer does not hash. The protocol crate computes
//! `SHA-256("dropumi/v1/device-fingerprint" ‖ public_key)` and wraps the
//! digest with [`Fingerprint::from_digest`].

use std::fmt;

use data_encoding::BASE32_NOPAD;

use crate::device::DeviceName;

/// Number of base32 characters in the full fingerprint form.
pub const FINGERPRINT_FULL_LEN: usize = 52;
/// Number of base32 characters in the display form (first 15 bytes).
pub const FINGERPRINT_DISPLAY_CHARS: usize = 24;
/// Number of base32 characters in the mDNS hint.
pub const FINGERPRINT_HINT_LEN: usize = 8;
/// Minimum number of characters in a fingerprint prefix.
pub const FINGERPRINT_PREFIX_MIN: usize = 8;
/// Minimum prefix length (120 bits) that pins a peer's identity.
pub const FINGERPRINT_PIN_MIN: usize = 24;

/// A device's X25519 static public key (32 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey([u8; 32]);

impl PublicKey {
    /// Wraps raw key bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Raw key bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex encoding (64 characters).
    #[must_use]
    pub fn to_hex(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.0)
    }

    /// Parses 64 hex characters (either case).
    ///
    /// # Errors
    /// Returns [`IdentityParseError::InvalidPublicKey`] for any other input.
    pub fn from_hex(s: &str) -> Result<Self, IdentityParseError> {
        let bytes = data_encoding::HEXLOWER_PERMISSIVE
            .decode(s.as_bytes())
            .map_err(|_| IdentityParseError::InvalidPublicKey)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| IdentityParseError::InvalidPublicKey)?;
        Ok(Self(arr))
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({})", self.to_hex())
    }
}

/// SHA-256 based device fingerprint (32 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    /// Wraps a digest computed by the protocol layer.
    #[must_use]
    pub const fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// Raw digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Full form: 52 upper-case RFC 4648 base32 characters, no padding.
    #[must_use]
    pub fn to_full(&self) -> String {
        BASE32_NOPAD.encode(&self.0)
    }

    /// Display form: the first 15 bytes as 24 characters in groups of four,
    /// for example `ABCD-EFGH-IJKL-MNOP-QRST-UVWX`.
    #[must_use]
    pub fn to_display(&self) -> String {
        let full = self.to_full();
        let head = full.get(..FINGERPRINT_DISPLAY_CHARS).unwrap_or_default();
        let mut out = String::with_capacity(FINGERPRINT_DISPLAY_CHARS + 5);
        for (i, c) in head.chars().enumerate() {
            if i > 0 && i % 4 == 0 {
                out.push('-');
            }
            out.push(c);
        }
        out
    }

    /// mDNS hint: the first 8 characters of the full form.
    #[must_use]
    pub fn hint(&self) -> String {
        let full = self.to_full();
        full.get(..FINGERPRINT_HINT_LEN)
            .unwrap_or_default()
            .to_owned()
    }

    /// Parses the full form. Case-insensitive; `-` and spaces are ignored.
    ///
    /// # Errors
    /// Returns [`IdentityParseError::InvalidFingerprint`] unless the input
    /// normalizes to exactly 52 canonical base32 characters.
    pub fn parse_full(s: &str) -> Result<Self, IdentityParseError> {
        let norm = normalize_base32(s).ok_or(IdentityParseError::InvalidFingerprint)?;
        if norm.len() != FINGERPRINT_FULL_LEN {
            return Err(IdentityParseError::InvalidFingerprint);
        }
        let bytes = BASE32_NOPAD
            .decode(norm.as_bytes())
            .map_err(|_| IdentityParseError::InvalidFingerprint)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| IdentityParseError::InvalidFingerprint)?;
        Ok(Self(arr))
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({})", self.to_display())
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_display())
    }
}

/// A prefix of a full fingerprint (at least 8 base32 characters).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FingerprintPrefix(String);

impl FingerprintPrefix {
    /// Parses a prefix. Case-insensitive; `-` and spaces are ignored.
    ///
    /// # Errors
    /// Returns [`IdentityParseError::InvalidPrefix`] unless the input
    /// normalizes to 8..=52 base32 characters.
    pub fn parse(s: &str) -> Result<Self, IdentityParseError> {
        let norm = normalize_base32(s).ok_or(IdentityParseError::InvalidPrefix)?;
        if !(FINGERPRINT_PREFIX_MIN..=FINGERPRINT_FULL_LEN).contains(&norm.len()) {
            return Err(IdentityParseError::InvalidPrefix);
        }
        Ok(Self(norm))
    }

    /// Parses a prefix that must be long enough to pin an identity
    /// (at least 24 characters, 120 bits).
    ///
    /// # Errors
    /// Returns [`IdentityParseError::PrefixTooShort`] for shorter prefixes.
    pub fn parse_pin(s: &str) -> Result<Self, IdentityParseError> {
        let p = Self::parse(s)?;
        if p.is_pin() {
            Ok(p)
        } else {
            Err(IdentityParseError::PrefixTooShort)
        }
    }

    /// The exact prefix of `fp`'s full form.
    #[must_use]
    pub fn of(fp: &Fingerprint, len: usize) -> Self {
        let full = fp.to_full();
        let len = len.clamp(FINGERPRINT_PREFIX_MIN, FINGERPRINT_FULL_LEN);
        Self(full.get(..len).unwrap_or_default().to_owned())
    }

    /// Whether `fp`'s full form starts with this prefix.
    #[must_use]
    pub fn matches(&self, fp: &Fingerprint) -> bool {
        fp.to_full().starts_with(&self.0)
    }

    /// Whether this prefix is long enough to pin an identity.
    #[must_use]
    pub fn is_pin(&self) -> bool {
        self.0.len() >= FINGERPRINT_PIN_MIN
    }

    /// Normalized (upper-case, no separators) prefix.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for FingerprintPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FingerprintPrefix({})", self.0)
    }
}

/// Upper-cases, removes `-` and spaces, and checks the base32 alphabet.
fn normalize_base32(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '-' | ' ' => {}
            'a'..='z' | 'A'..='Z' | '2'..='7' => out.push(c.to_ascii_uppercase()),
            _ => return None,
        }
    }
    Some(out)
}

/// Returns `true` if `s` looks like a fingerprint prefix (≥8 base32
/// characters after removing `-` and spaces).
#[must_use]
pub fn looks_like_fingerprint(s: &str) -> bool {
    normalize_base32(s).is_some_and(|n| n.len() >= FINGERPRINT_PREFIX_MIN)
}

/// A device identity as presented by a peer: authenticated key and
/// fingerprint plus an untrusted, sanitized self-declared name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceIdentity {
    /// Authenticated static public key.
    pub public_key: PublicKey,
    /// Fingerprint of `public_key`.
    pub fingerprint: Fingerprint,
    /// Name the device claims for itself (never proof of identity).
    pub name: DeviceName,
}

/// Errors parsing identity material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityParseError {
    /// Not a 52-character base32 fingerprint.
    #[error("invalid fingerprint: expected 52 base32 characters (A-Z, 2-7)")]
    InvalidFingerprint,
    /// Not a fingerprint prefix.
    #[error("invalid fingerprint prefix: expected 8 to 52 base32 characters (A-Z, 2-7)")]
    InvalidPrefix,
    /// Prefix shorter than 24 characters where a pin is required.
    #[error("fingerprint prefix too short: at least 24 characters are required")]
    PrefixTooShort,
    /// Not a 64-character hex public key.
    #[error("invalid public key: expected 64 hex characters")]
    InvalidPublicKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Fingerprint {
        let mut d = [0u8; 32];
        for (i, b) in d.iter_mut().enumerate() {
            *b = u8::try_from(i * 7 + 3).unwrap();
        }
        Fingerprint::from_digest(d)
    }

    #[test]
    fn display_and_parse_roundtrip() {
        let fp = sample();
        let full = fp.to_full();
        assert_eq!(full.len(), FINGERPRINT_FULL_LEN);
        assert!(
            full.chars()
                .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c))
        );
        assert_eq!(Fingerprint::parse_full(&full).unwrap(), fp);
        assert_eq!(Fingerprint::parse_full(&full.to_lowercase()).unwrap(), fp);
        let display = fp.to_display();
        assert_eq!(display.len(), 29);
        assert_eq!(display.matches('-').count(), 5);
        assert_eq!(display.replace('-', ""), full[..24]);
        assert_eq!(fp.hint(), full[..8]);
        let spaced: String = full
            .chars()
            .enumerate()
            .flat_map(|(i, c)| {
                if i % 4 == 0 && i > 0 {
                    vec!['-', c]
                } else {
                    vec![c]
                }
            })
            .collect();
        assert_eq!(Fingerprint::parse_full(&spaced).unwrap(), fp);
    }

    #[test]
    fn parse_rejects_bad_input() {
        let full = sample().to_full();
        assert!(Fingerprint::parse_full("").is_err());
        assert!(Fingerprint::parse_full(&full[..51]).is_err());
        assert!(Fingerprint::parse_full(&format!("{full}A")).is_err());
        assert!(Fingerprint::parse_full(&full.replace('A', "1")).is_err() || !full.contains('A'));
        assert!(Fingerprint::parse_full(&format!("{}0", &full[..51])).is_err());
        // Non-canonical trailing bits are rejected (52 chars encode 260 bits).
        let mut chars: Vec<char> = full.chars().collect();
        let last = chars[51];
        let tweaked = if last == 'A' { 'B' } else { 'A' };
        chars[51] = tweaked;
        let s: String = chars.into_iter().collect();
        if let Ok(fp) = Fingerprint::parse_full(&s) {
            assert_eq!(fp.to_full(), s, "accepted input must be canonical");
        }
        assert!(PublicKey::from_hex("zz").is_err());
        assert!(PublicKey::from_hex(&"ab".repeat(31)).is_err());
        let pk = PublicKey::from_bytes([0xAB; 32]);
        assert_eq!(PublicKey::from_hex(&pk.to_hex()).unwrap(), pk);
        assert_eq!(
            PublicKey::from_hex(&pk.to_hex().to_uppercase()).unwrap(),
            pk
        );
    }

    #[test]
    fn prefix_matching() {
        let fp = sample();
        let full = fp.to_full();
        let p8 = FingerprintPrefix::parse(&full[..8].to_lowercase()).unwrap();
        assert!(p8.matches(&fp));
        assert!(!p8.is_pin());
        let p24 = FingerprintPrefix::parse_pin(&fp.to_display()).unwrap();
        assert!(p24.matches(&fp));
        assert!(p24.is_pin());
        assert_eq!(
            FingerprintPrefix::parse_pin(&full[..23]),
            Err(IdentityParseError::PrefixTooShort)
        );
        assert!(FingerprintPrefix::parse(&full[..7]).is_err());
        assert!(FingerprintPrefix::parse("ABCDEFG1").is_err());
        let other = Fingerprint::from_digest([0xFF; 32]);
        assert!(!p8.matches(&other) || other.to_full().starts_with(&full[..8]));
        assert!(looks_like_fingerprint("abcd-efgh"));
        assert!(!looks_like_fingerprint("Dropumi-AB12"));
        assert!(!looks_like_fingerprint("ABCDEFG"));
        assert_eq!(FingerprintPrefix::of(&fp, 8), p8);
    }
}
