// SPDX-License-Identifier: MPL-2.0
//! Property tests: mDNS TXT records are untrusted peer input; parsing never
//! panics and every kept value is bounded and well-formed.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use dropumi_domain::identity::FINGERPRINT_HINT_LEN;
use dropumi_domain::limits::MAX_DEVICE_NAME_DISPLAY_BYTES;
use dropumi_domain::text::is_unsafe_char;
use dropumi_infra::txt;
use proptest::prelude::*;

fn key() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("n".to_owned()),
        Just("fp".to_owned()),
        Just("v".to_owned()),
        ".{0,4}"
    ]
}

fn value() -> impl Strategy<Value = String> {
    prop_oneof!["[A-Z2-7]{8}", "[0-9, ]{0,40}", ".{0,255}"]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn txt_parse_is_bounded(props in prop::collection::vec((key(), value()), 0..8)) {
        let pairs: Vec<(&str, &str)> = props.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let hints = txt::parse(pairs);
        let name = hints.name.as_str();
        prop_assert!(!name.is_empty() && name.len() <= MAX_DEVICE_NAME_DISPLAY_BYTES);
        prop_assert!(!name.chars().any(is_unsafe_char));
        if let Some(fp) = &hints.fp_hint {
            prop_assert_eq!(fp.len(), FINGERPRINT_HINT_LEN);
            prop_assert!(fp.chars().all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c)));
        }
        let longest_v = props.iter().filter(|(k, _)| k == "v").map(|(_, v)| v.len()).max().unwrap_or(0);
        prop_assert!(hints.versions.len() <= longest_v / 2 + 1);
    }
}
