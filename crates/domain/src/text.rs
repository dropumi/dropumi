// SPDX-License-Identifier: MPL-2.0
//! Removal of characters that can spoof or corrupt displayed text.
//!
//! Every string that comes from a peer (device names, file names, error
//! details) passes through [`strip_unsafe`] before it is displayed, logged or
//! used to build a file name.

use std::borrow::Cow;

mod cf_table;

use cf_table::CF_RANGES;

/// Unicode version of the format-character table used by [`strip_unsafe`].
pub const UNICODE_VERSION: (u8, u8, u8) = cf_table::CF_TABLE_UNICODE_VERSION;

const ZWNJ: char = '\u{200C}';
const ZWJ: char = '\u{200D}';

/// Returns `true` for characters that [`strip_unsafe`] removes:
///
/// - C0 controls, DEL and C1 controls (General_Category=Cc);
/// - U+2028 LINE SEPARATOR (Zl) and U+2029 PARAGRAPH SEPARATOR (Zp);
/// - every format character (Cf) — bidi controls, zero-width space, BOM,
///   soft hyphen, invisible operators, tag characters — except U+200C ZWNJ
///   and U+200D ZWJ, which legitimate scripts and emoji sequences need.
#[must_use]
pub fn is_unsafe_char(c: char) -> bool {
    if c.is_control() {
        return true;
    }
    if c == '\u{2028}' || c == '\u{2029}' {
        return true;
    }
    if c == ZWNJ || c == ZWJ {
        return false;
    }
    is_format_char(c)
}

fn is_format_char(c: char) -> bool {
    let cp = u32::from(c);
    CF_RANGES
        .binary_search_by(|&(lo, hi)| {
            if hi < cp {
                std::cmp::Ordering::Less
            } else if lo > cp {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Removes every character for which [`is_unsafe_char`] is `true`.
///
/// Borrows when nothing needs removing.
#[must_use]
pub fn strip_unsafe(s: &str) -> Cow<'_, str> {
    if s.chars().any(is_unsafe_char) {
        Cow::Owned(s.chars().filter(|&c| !is_unsafe_char(c)).collect())
    } else {
        Cow::Borrowed(s)
    }
}

/// Truncates `s` to at most `max_bytes` UTF-8 bytes on a character boundary.
#[must_use]
pub fn truncate_bytes(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_cc_zl_zp_cf_keeps_zwj_zwnj_vs() {
        let input = "a\u{0}b\u{7}c\u{1B}[31md\u{7F}e\u{85}f\u{2028}g\u{2029}h\u{202E}i\u{200B}j\u{FEFF}k\u{AD}l\u{2066}m\u{E0041}n";
        assert_eq!(strip_unsafe(input), "abc[31mdefghijklmn");
        // ZWJ emoji sequence and VS16 (Mn, not Cf) survive.
        let emoji = "\u{1F469}\u{200D}\u{1F4BB} \u{2764}\u{FE0F} ab\u{200C}cd";
        assert_eq!(strip_unsafe(emoji), emoji);
        assert!(matches!(strip_unsafe("plain"), Cow::Borrowed(_)));
    }

    #[test]
    fn cf_table_sorted_disjoint() {
        for w in CF_RANGES.windows(2) {
            assert!(w[0].0 <= w[0].1);
            assert!(w[0].1 + 1 < w[1].0, "ranges must be sorted and merged");
        }
        assert!(is_format_char('\u{E007F}'));
        assert!(is_format_char('\u{00AD}'));
        assert!(!is_format_char('a'));
        assert!(!is_format_char('\u{FE0F}'));
    }

    #[test]
    fn unicode_version_matches_normalization() {
        assert_eq!(
            cf_table::CF_TABLE_UNICODE_VERSION,
            unicode_normalization::UNICODE_VERSION,
            "regenerate cf_table.rs for the Unicode version used by unicode-normalization"
        );
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate_bytes("héllo", 2), "h");
        assert_eq!(truncate_bytes("héllo", 3), "hé");
        assert_eq!(truncate_bytes("abc", 10), "abc");
        assert_eq!(truncate_bytes("東京", 4), "東");
    }
}
