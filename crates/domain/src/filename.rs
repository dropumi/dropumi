// SPDX-License-Identifier: MPL-2.0
//! File name validation (wire) and sanitization (local).
//!
//! A receiver never uses a peer-supplied name as a path. It validates the
//! wire name, then derives a single safe path component with
//! [`SafeFileName::sanitize`] that is valid on Windows, macOS and Linux.

use std::ffi::OsStr;
use std::fmt;

use unicode_normalization::UnicodeNormalization;

use crate::limits::{
    MAX_KEPT_EXTENSION_BYTES, MAX_SAFE_NAME_BYTES, MAX_SAFE_NAME_UTF16, MAX_WIRE_NAME_BYTES,
};
use crate::text::{strip_unsafe, truncate_bytes};

/// Fallback name for inputs that sanitize to nothing.
pub const FALLBACK_NAME: &str = "file";
/// Prefix reserved for Dropumi's own temporary files.
pub const TEMP_PREFIX: &str = ".dropumi-";

/// Errors validating a wire file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FileNameError {
    /// Empty name.
    #[error("file name is empty")]
    Empty,
    /// Longer than 1024 UTF-8 bytes.
    #[error("file name is longer than 1024 bytes")]
    TooLong,
    /// `.` or `..`.
    #[error("file name is a dot entry")]
    DotEntry,
    /// Contains `/`, `\` or NUL.
    #[error("file name contains a path separator or NUL")]
    Separator,
}

/// Validates a peer-supplied file name: 1..=1024 UTF-8 bytes, a single
/// component (no `/`, `\` or NUL) and not `.` or `..`.
///
/// # Errors
/// Returns the first rule the name breaks.
pub fn validate_wire_name(name: &str) -> Result<(), FileNameError> {
    if name.is_empty() {
        return Err(FileNameError::Empty);
    }
    if name.len() > MAX_WIRE_NAME_BYTES {
        return Err(FileNameError::TooLong);
    }
    if name == "." || name == ".." {
        return Err(FileNameError::DotEntry);
    }
    if name.contains(['/', '\\', '\0']) {
        return Err(FileNameError::Separator);
    }
    Ok(())
}

/// A file name that is safe to create inside a receive directory on every
/// supported OS: one component, ≤255 UTF-8 bytes, NFC, no control, format or
/// separator characters, no Windows-reserved names or characters, no
/// trailing dots or spaces, and never Dropumi's temp-file prefix.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SafeFileName(String);

impl SafeFileName {
    /// Sanitizes any string. Total: never fails.
    ///
    /// Steps, in order: keep the last `/`- or `\`-separated component;
    /// remove unsafe characters ([`strip_unsafe`]); NFC; replace
    /// `<>:"|?*` with `_`; trim leading spaces and trailing spaces and dots;
    /// prefix `_` to Windows device names; map empty, `.` and `..` to
    /// `file`; prefix `_` to the `.dropumi-` temp prefix; truncate to 255
    /// bytes keeping an extension of at most 16 bytes.
    #[must_use]
    pub fn sanitize(raw: &str) -> Self {
        let last = raw.rsplit(['/', '\\']).next().unwrap_or_default();
        let stripped = strip_unsafe(last);
        let normalized: String = stripped.nfc().collect();
        let replaced: String = normalized
            .chars()
            .map(|c| {
                if matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        Self(secure(&replaced, ""))
    }

    /// The name with a collision suffix: `stem (n).ext` for `n > 0`,
    /// still at most 255 bytes. A leading dot does not start an extension.
    #[must_use]
    pub fn with_collision_suffix(&self, n: u32) -> Self {
        if n == 0 {
            return self.clone();
        }
        Self(secure(&self.0, &format!(" ({n})")))
    }

    /// The name with `-tag` inserted before the extension (for example a
    /// random last-resort name), still safe. Only ASCII alphanumerics of
    /// `tag` are used, and the tag is never truncated away.
    #[must_use]
    pub fn with_tag(&self, tag: &str) -> Self {
        let tag: String = tag
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(32)
            .collect();
        if tag.is_empty() {
            return self.clone();
        }
        Self(secure(&self.0, &format!("-{tag}")))
    }

    /// The sanitized name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SafeFileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SafeFileName({:?})", self.0)
    }
}

impl fmt::Display for SafeFileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Splits `name` into `(stem, extension)`; the extension includes the dot,
/// is at most 16 bytes and never starts at index 0.
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= MAX_KEPT_EXTENSION_BYTES => (
            name.get(..i).unwrap_or(name),
            name.get(i..).unwrap_or_default(),
        ),
        _ => (name, ""),
    }
}

/// Fits `name` plus `suffix` into the length limits, then applies every
/// semantic rule (empty and dot names, Windows device names, the temp-file
/// prefix) to the *result*, repeating until nothing changes. Running the
/// rules after truncation matters: truncation can turn a harmless name
/// such as `NUL` + spaces + `y` back into a device name.
fn secure(name: &str, suffix: &str) -> String {
    let mut current = fit(name, suffix);
    for _ in 0..4 {
        let mut next = current.clone();
        if next.is_empty() || next == "." || next == ".." {
            next = FALLBACK_NAME.to_owned();
        }
        if is_windows_reserved(&next) {
            next.insert(0, '_');
        }
        if starts_with_ignore_ascii_case(&next, TEMP_PREFIX) {
            next.insert(0, '_');
        }
        let refit = fit(&next, "");
        if refit == current {
            return current;
        }
        current = refit;
    }
    current
}

/// Length of the canonical decomposition in UTF-16 code units (the unit
/// HFS+ uses for its 255-unit name limit).
fn decomposed_utf16_len(s: &str) -> usize {
    s.nfd().map(char::len_utf16).sum()
}

/// Builds `stem + suffix + ext`, truncating the stem until the result is
/// at most 255 UTF-8 bytes in NFC *and* at most 255 UTF-16 units when
/// decomposed (HFS+), trimmed of leading spaces and trailing spaces/dots.
fn fit(name: &str, suffix: &str) -> String {
    let (stem, ext) = split_extension(name);
    let mut budget = MAX_SAFE_NAME_BYTES.saturating_sub(suffix.len() + ext.len());
    loop {
        let kept = truncate_bytes(stem, budget);
        // A truncated stem must not end with a space before a suffix.
        let kept_trimmed = if suffix.is_empty() {
            kept
        } else {
            kept.trim_end_matches(' ')
        };
        let joined = format!("{kept_trimmed}{suffix}{ext}");
        let mut out = trim_name(&joined).to_owned();
        if !unicode_normalization::is_nfc(&out) {
            let renormalized: String = out.nfc().collect();
            out = trim_name(truncate_bytes(&renormalized, MAX_SAFE_NAME_BYTES)).to_owned();
        }
        if out.is_empty() {
            return FALLBACK_NAME.to_owned();
        }
        if out.len() <= MAX_SAFE_NAME_BYTES && decomposed_utf16_len(&out) <= MAX_SAFE_NAME_UTF16 {
            return out;
        }
        if kept.is_empty() {
            return FALLBACK_NAME.to_owned();
        }
        // Drop the last character of the stem and try again.
        budget = kept.char_indices().next_back().map_or(0, |(i, _)| i);
    }
}

/// Trims leading spaces and trailing spaces and dots.
fn trim_name(s: &str) -> &str {
    s.trim_start_matches(' ').trim_end_matches([' ', '.'])
}

fn starts_with_ignore_ascii_case(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Windows reserved device names (case-insensitive, compared on the part
/// before the first dot with trailing spaces removed).
fn is_windows_reserved(name: &str) -> bool {
    let base = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    let upper = base.to_uppercase();
    match upper.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" => true,
        _ => {
            let (Some(head), Some(tail)) = (upper.get(..3), upper.get(3..)) else {
                return false;
            };
            (head == "COM" || head == "LPT")
                && matches!(
                    tail,
                    "0" | "1"
                        | "2"
                        | "3"
                        | "4"
                        | "5"
                        | "6"
                        | "7"
                        | "8"
                        | "9"
                        | "\u{B9}"
                        | "\u{B2}"
                        | "\u{B3}"
                )
        }
    }
}

/// The wire name for a local source file: lossy UTF-8 of the final
/// component with `/`, `\` and NUL replaced, at most 1024 bytes, never empty
/// or a dot entry.
#[must_use]
pub fn wire_name_from_local(file_name: &OsStr) -> String {
    let lossy = file_name.to_string_lossy();
    let replaced: String = lossy
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | '\0') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let truncated = truncate_bytes(&replaced, MAX_WIRE_NAME_BYTES);
    if truncated.is_empty() || truncated == "." || truncated == ".." {
        FALLBACK_NAME.to_owned()
    } else {
        truncated.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(raw: &str) -> String {
        SafeFileName::sanitize(raw).as_str().to_owned()
    }

    #[test]
    fn sanitize_fixtures() {
        assert_eq!(s("../../etc/passwd"), "passwd");
        assert_eq!(s("/abs"), "abs");
        assert_eq!(s("C:\\x"), "x");
        assert_eq!(s("\\\\srv\\s\\x"), "x");
        assert_eq!(s("C:"), "C_");
        assert_eq!(s("CON"), "_CON");
        assert_eq!(s("con.txt"), "_con.txt");
        assert_eq!(s("COM\u{B9}"), "_COM\u{B9}");
        assert_eq!(s("LPT0.log"), "_LPT0.log");
        assert_eq!(s("CONOUT$"), "_CONOUT$");
        assert_eq!(s("NUL .txt"), "_NUL .txt");
        assert_eq!(s("COM10"), "COM10");
        assert_eq!(s("console.txt"), "console.txt");
        assert_eq!(s("a."), "a");
        assert_eq!(s("a "), "a");
        assert_eq!(s("  lead"), "lead");
        assert_eq!(s("..."), FALLBACK_NAME);
        assert_eq!(s("."), FALLBACK_NAME);
        assert_eq!(s(".."), FALLBACK_NAME);
        assert_eq!(s(""), FALLBACK_NAME);
        assert_eq!(s("a\u{0}b\u{1F}c\u{7F}d"), "abcd");
        assert_eq!(s("invoice\u{202E}fdp.exe"), "invoicefdp.exe");
        assert_eq!(s("x\u{2028}y"), "xy");
        assert_eq!(s("\u{200B}a"), "a");
        assert_eq!(s("a\u{AD}b"), "ab");
        assert_eq!(s("tag\u{E0041}\u{E007F}s"), "tags");
        assert_eq!(s("a<b>c:d\"e|f?g*h"), "a_b_c_d_e_f_g_h");
        assert_eq!(s("Cafe\u{301}.txt"), "Caf\u{E9}.txt");
        assert_eq!(s(".dropumi-x.part"), "_.dropumi-x.part");
        assert_eq!(s(".DROPUMI-x.part"), "_.DROPUMI-x.part");
        assert_eq!(s(".hidden"), ".hidden");
        assert_eq!(
            s("\u{1F469}\u{200D}\u{1F4BB}.png"),
            "\u{1F469}\u{200D}\u{1F4BB}.png"
        );
        let cjk = "東".repeat(341) + ".txt";
        let out = s(&cjk);
        assert!(out.len() <= MAX_SAFE_NAME_BYTES);
        assert!(out.ends_with(".txt"));
    }

    #[test]
    fn truncation_cannot_resurrect_device_names() {
        let nul = format!("NUL{}y", " ".repeat(300));
        assert_eq!(s(&nul), "_NUL");
        assert_eq!(s(&s(&nul)), "_NUL", "idempotent");
        let con = format!("CON{}y.txt", " ".repeat(300));
        let out = s(&con);
        assert!(out.starts_with("_CON"), "{out:?}");
        let tmp = format!(".dropumi-{}z", " ".repeat(300));
        assert!(s(&tmp).starts_with("_.dropumi-"));
    }

    #[test]
    fn hfs_plus_decomposed_length_bounded() {
        // U+01D6 is 2 UTF-8 bytes but 3 UTF-16 units when decomposed.
        let name: String = std::iter::repeat_n('\u{1D6}', 127).collect();
        let out = s(&name);
        assert!(out.len() <= MAX_SAFE_NAME_BYTES);
        assert!(decomposed_utf16_len(&out) <= MAX_SAFE_NAME_UTF16);
        assert_eq!(out.chars().count(), 85);
    }

    #[test]
    fn tag_survives_long_names() {
        let long = SafeFileName::sanitize(&("a".repeat(300) + ".bin"));
        let tagged = long.with_tag("abcdef012345");
        assert!(tagged.as_str().ends_with("-abcdef012345.bin"));
        assert!(tagged.as_str().len() <= MAX_SAFE_NAME_BYTES);
        assert_eq!(long.with_tag("../"), long);
    }

    #[test]
    fn collision_suffixes() {
        let n = SafeFileName::sanitize("photo.jpg");
        assert_eq!(n.with_collision_suffix(0).as_str(), "photo.jpg");
        assert_eq!(n.with_collision_suffix(1).as_str(), "photo (1).jpg");
        assert_eq!(
            SafeFileName::sanitize("README")
                .with_collision_suffix(2)
                .as_str(),
            "README (2)"
        );
        assert_eq!(
            SafeFileName::sanitize(".bashrc")
                .with_collision_suffix(3)
                .as_str(),
            ".bashrc (3)"
        );
        let long = SafeFileName::sanitize(&("a".repeat(300) + ".tar"));
        let suffixed = long.with_collision_suffix(9999);
        assert!(suffixed.as_str().len() <= MAX_SAFE_NAME_BYTES);
        assert!(suffixed.as_str().ends_with(" (9999).tar"));
    }

    #[test]
    fn wire_name_validation() {
        assert_eq!(validate_wire_name(""), Err(FileNameError::Empty));
        assert_eq!(validate_wire_name("."), Err(FileNameError::DotEntry));
        assert_eq!(validate_wire_name(".."), Err(FileNameError::DotEntry));
        assert_eq!(validate_wire_name("a/b"), Err(FileNameError::Separator));
        assert_eq!(validate_wire_name("a\\b"), Err(FileNameError::Separator));
        assert_eq!(validate_wire_name("a\0b"), Err(FileNameError::Separator));
        assert_eq!(
            validate_wire_name(&"x".repeat(1025)),
            Err(FileNameError::TooLong)
        );
        assert!(validate_wire_name(&"x".repeat(1024)).is_ok());
        assert!(validate_wire_name("CON.txt").is_ok());
        assert_eq!(wire_name_from_local(OsStr::new("a\\b")), "a_b");
        assert_eq!(wire_name_from_local(OsStr::new("")), FALLBACK_NAME);
        assert_eq!(wire_name_from_local(OsStr::new("..")), FALLBACK_NAME);
    }
}
