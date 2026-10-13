// SPDX-License-Identifier: MPL-2.0
//! Property tests: device names and displayed text are safe for any input.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::borrow::Cow;

use dropumi_domain::device::DeviceName;
use dropumi_domain::limits::MAX_DEVICE_NAME_DISPLAY_BYTES;
use dropumi_domain::text::{is_unsafe_char, strip_unsafe};
use proptest::prelude::*;

/// Mostly hostile input: controls, bidi overrides, zero-width and
/// combining characters, separators and long runs.
fn hostile() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        any::<char>().prop_map(String::from),
        Just("\u{202E}".to_owned()),
        Just("\u{200B}".to_owned()),
        Just("\u{FEFF}".to_owned()),
        Just("\u{2028}".to_owned()),
        Just("\u{1B}[2J".to_owned()),
        Just("e\u{301}".to_owned()),
        Just(" ".repeat(70)),
        Just("東".repeat(30)),
        "[a-zA-Z0-9 -]{0,20}",
    ];
    prop::collection::vec(piece, 0..12).prop_map(|v| v.concat())
}

fn assert_safe_name(name: &str) {
    assert!(!name.is_empty());
    assert!(
        name.len() <= MAX_DEVICE_NAME_DISPLAY_BYTES,
        "{}",
        name.len()
    );
    assert!(!name.chars().any(is_unsafe_char), "unsafe char in {name:?}");
    assert_eq!(name, name.trim(), "untrimmed {name:?}");
    assert!(unicode_normalization::is_nfc(name), "not NFC: {name:?}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn device_name_is_safe_and_idempotent(raw in hostile()) {
        let once = DeviceName::sanitize(&raw);
        assert_safe_name(once.as_str());
        prop_assert_eq!(DeviceName::sanitize(once.as_str()), once);
    }

    #[test]
    fn device_name_from_any_string(raw in ".*") {
        assert_safe_name(DeviceName::sanitize(&raw).as_str());
    }

    #[test]
    fn strip_unsafe_removes_everything_unsafe(raw in hostile()) {
        let out = strip_unsafe(&raw);
        prop_assert!(!out.chars().any(is_unsafe_char));
        // Already-clean text is returned without copying.
        prop_assert!(matches!(strip_unsafe(&out), Cow::Borrowed(s) if s == out.as_ref()));
        if !raw.chars().any(is_unsafe_char) {
            prop_assert!(matches!(out, Cow::Borrowed(_)));
        }
    }
}
