// SPDX-License-Identifier: MPL-2.0
//! Device names and kinds.

use std::fmt;

use unicode_normalization::UnicodeNormalization;

use crate::limits::MAX_DEVICE_NAME_DISPLAY_BYTES;
use crate::text::{strip_unsafe, truncate_bytes};

/// Placeholder used when a sanitized name is empty.
pub const UNNAMED_DEVICE: &str = "Unnamed device";

/// A sanitized, display-safe device name (at most 64 UTF-8 bytes).
///
/// Device names are self-declared and never proof of identity.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DeviceName(String);

impl DeviceName {
    /// Sanitizes untrusted input: NFC normalization, removal of control,
    /// separator and format characters, trimming, and truncation to 64 bytes
    /// on a character boundary. Empty results become `"Unnamed device"`.
    #[must_use]
    pub fn sanitize(raw: &str) -> Self {
        let stripped = strip_unsafe(raw);
        let normalized: String = stripped.nfc().collect();
        let trimmed = normalized.trim();
        let truncated = truncate_bytes(trimmed, MAX_DEVICE_NAME_DISPLAY_BYTES).trim_end();
        if truncated.is_empty() {
            Self(UNNAMED_DEVICE.to_owned())
        } else {
            Self(truncated.to_owned())
        }
    }

    /// A default name such as `Dropumi-3FA9`, derived from random bytes.
    ///
    /// The four hex digits always include a 0, 1, 8 or 9 (none of which is
    /// a base32 character), so a default name is never read as a
    /// fingerprint prefix and always works as a `--to` name target.
    #[must_use]
    pub fn random_default(seed: [u8; 2]) -> Self {
        let [hi, lo] = seed;
        let mut suffix = format!("{hi:02X}{lo:02X}");
        if !suffix.contains(['0', '1', '8', '9']) {
            let digit = ['0', '1', '8', '9']
                .get(usize::from(lo & 0b11))
                .copied()
                .unwrap_or('9');
            suffix.pop();
            suffix.push(digit);
        }
        Self(format!("Dropumi-{suffix}"))
    }

    /// The sanitized name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DeviceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceName({:?})", self.0)
    }
}

impl fmt::Display for DeviceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Self-declared device form factor (informational only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum DeviceKind {
    /// Unknown or not declared.
    #[default]
    Unknown,
    /// Desktop computer.
    Desktop,
    /// Laptop computer.
    Laptop,
    /// Phone.
    Phone,
    /// Tablet.
    Tablet,
    /// Headless device (for example the CLI on a server).
    Headless,
}

impl DeviceKind {
    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Unknown => 0,
            Self::Desktop => 1,
            Self::Laptop => 2,
            Self::Phone => 3,
            Self::Tablet => 4,
            Self::Headless => 5,
        }
    }

    /// Decodes a wire code; unknown values map to [`DeviceKind::Unknown`].
    #[must_use]
    pub const fn from_code(code: u64) -> Self {
        match code {
            1 => Self::Desktop,
            2 => Self::Laptop,
            3 => Self::Phone,
            4 => Self::Tablet,
            5 => Self::Headless,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_and_truncates() {
        assert_eq!(
            DeviceName::sanitize("  Alice's Mac  ").as_str(),
            "Alice's Mac"
        );
        assert_eq!(
            DeviceName::sanitize("evil\u{202E}gpj.exe").as_str(),
            "evilgpj.exe"
        );
        assert_eq!(
            DeviceName::sanitize("a\u{1B}[2Jb\u{2028}c").as_str(),
            "a[2Jbc"
        );
        assert_eq!(
            DeviceName::sanitize("\u{200B}\u{FEFF} ").as_str(),
            UNNAMED_DEVICE
        );
        assert_eq!(DeviceName::sanitize("").as_str(), UNNAMED_DEVICE);
        // NFD input is normalized to NFC.
        assert_eq!(DeviceName::sanitize("Cafe\u{301}").as_str(), "Caf\u{E9}");
        let long = "東".repeat(40);
        let n = DeviceName::sanitize(&long);
        assert!(n.as_str().len() <= MAX_DEVICE_NAME_DISPLAY_BYTES);
        assert_eq!(n.as_str().len(), 63);
        assert_eq!(
            DeviceName::random_default([0x3F, 0xA9]).as_str(),
            "Dropumi-3FA9"
        );
    }

    #[test]
    fn default_names_are_always_name_targets() {
        // Every possible default name is classified as a device name, never
        // as a fingerprint prefix (which would make `--to <name>` fail).
        for hi in 0..=u8::MAX {
            for lo in 0..=u8::MAX {
                let name = DeviceName::random_default([hi, lo]);
                let s = name.as_str();
                let suffix = s.strip_prefix("Dropumi-").unwrap();
                assert_eq!(suffix.len(), 4, "{s}");
                assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()), "{s}");
                assert_eq!(
                    crate::discovery::Target::parse(s),
                    Ok(crate::discovery::Target::Name(s.to_owned())),
                    "{s}"
                );
            }
        }
        // Unchanged when the random digits already include 0, 1, 8 or 9;
        // otherwise the last digit is replaced by one of them.
        assert_eq!(
            DeviceName::random_default([0x1A, 0x2B]).as_str(),
            "Dropumi-1A2B"
        );
        assert_eq!(
            DeviceName::random_default([0xAB, 0x23]).as_str(),
            "Dropumi-AB29"
        );
    }

    #[test]
    fn kind_codes_roundtrip() {
        for k in [
            DeviceKind::Unknown,
            DeviceKind::Desktop,
            DeviceKind::Laptop,
            DeviceKind::Phone,
            DeviceKind::Tablet,
            DeviceKind::Headless,
        ] {
            assert_eq!(DeviceKind::from_code(k.code()), k);
        }
        assert_eq!(DeviceKind::from_code(99), DeviceKind::Unknown);
    }
}
