// SPDX-License-Identifier: MPL-2.0
//! Property tests: sanitized file names are safe for any input.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use dropumi_domain::filename::{SafeFileName, TEMP_PREFIX};
use dropumi_domain::limits::MAX_SAFE_NAME_BYTES;
use dropumi_domain::text::is_unsafe_char;
use proptest::prelude::*;

fn assert_safe(name: &str) {
    assert!(!name.is_empty(), "empty name");
    assert!(
        name.len() <= MAX_SAFE_NAME_BYTES,
        "too long: {}",
        name.len()
    );
    assert!(!name.contains(['/', '\\', '\0']), "separator in {name:?}");
    assert!(
        !name.contains(['<', '>', ':', '"', '|', '?', '*']),
        "windows-reserved char in {name:?}"
    );
    assert!(!name.chars().any(is_unsafe_char), "unsafe char in {name:?}");
    assert!(
        !name.ends_with(['.', ' ']),
        "trailing dot/space in {name:?}"
    );
    assert!(!name.starts_with(' '), "leading space in {name:?}");
    assert!(name != "." && name != "..");
    assert!(
        !name.to_ascii_lowercase().starts_with(TEMP_PREFIX),
        "temp prefix in {name:?}"
    );
    assert!(unicode_normalization::is_nfc(name), "not NFC: {name:?}");
    let utf16: usize = unicode_normalization::UnicodeNormalization::nfd(name)
        .map(char::len_utf16)
        .sum();
    assert!(utf16 <= 255, "decomposed UTF-16 length {utf16}: {name:?}");
    let base = name
        .split('.')
        .next()
        .unwrap()
        .trim_end_matches(' ')
        .to_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
    assert!(!reserved.contains(&base.as_str()), "reserved: {name:?}");
    for p in ["COM", "LPT"] {
        for d in [
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "\u{B9}", "\u{B2}", "\u{B3}",
        ] {
            assert_ne!(base, format!("{p}{d}"), "reserved: {name:?}");
        }
    }
}

fn hostile_string() -> impl Strategy<Value = String> {
    let pieces = prop_oneof![
        any::<char>().prop_map(String::from),
        Just("..".to_owned()),
        Just("/".to_owned()),
        Just("\\".to_owned()),
        Just("CON".to_owned()),
        Just("com1".to_owned()),
        Just(".dropumi-".to_owned()),
        Just(" ".to_owned()),
        Just(".".to_owned()),
        Just("\u{202E}".to_owned()),
        Just("\u{200D}".to_owned()),
        Just("e\u{301}".to_owned()),
        Just("東京".to_owned()),
        Just("<>:\"|?*".to_owned()),
        Just(" ".repeat(250)),
        Just("\u{1D6}".repeat(40)),
        Just("NUL".to_owned()),
    ];
    proptest::collection::vec(pieces, 0..120).prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    #[test]
    fn sanitized_is_safe(raw in hostile_string()) {
        let once = SafeFileName::sanitize(&raw);
        assert_safe(once.as_str());
        // Idempotent: sanitizing a safe name changes nothing.
        let twice = SafeFileName::sanitize(once.as_str());
        prop_assert_eq!(twice.as_str(), once.as_str());
    }

    #[test]
    fn sanitized_is_safe_any_string(raw in ".*") {
        assert_safe(SafeFileName::sanitize(&raw).as_str());
    }

    #[test]
    fn collision_suffix_bounded(raw in hostile_string(), n in 0u32..=9999) {
        let name = SafeFileName::sanitize(&raw).with_collision_suffix(n);
        assert_safe(name.as_str());
        if n > 0 {
            let suffix = format!(" ({n})");
            prop_assert!(name.as_str().contains(&suffix));
        }
    }
}
