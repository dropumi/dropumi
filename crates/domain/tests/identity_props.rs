// SPDX-License-Identifier: MPL-2.0
//! Property tests: fingerprint, key and target parsing never panic and
//! round-trip.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use dropumi_domain::discovery::Target;
use dropumi_domain::identity::{
    FINGERPRINT_FULL_LEN, FINGERPRINT_PREFIX_MIN, Fingerprint, FingerprintPrefix, PublicKey,
    looks_like_fingerprint,
};
use proptest::prelude::*;

/// Strings near the parsers' grammar: base32, hex, separators, ports.
fn near_grammar() -> impl Strategy<Value = String> {
    prop_oneof![
        "[A-Za-z2-7 -]{0,70}",
        "[ \t\n]{0,2}[A-Za-z2-7]{8,20}[ \t\n]{0,2}",
        "[0-9a-fA-F]{0,70}",
        "[A-Za-z0-9.:\\[\\]-]{0,40}",
        ".*",
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn fingerprint_forms_round_trip(bytes in any::<[u8; 32]>()) {
        let fp = Fingerprint::from_digest(bytes);
        let full = fp.to_full();
        prop_assert_eq!(full.len(), FINGERPRINT_FULL_LEN);
        prop_assert_eq!(Fingerprint::parse_full(&full).unwrap(), fp);
        prop_assert_eq!(Fingerprint::parse_full(&full.to_ascii_lowercase()).unwrap(), fp);
        for shown in [fp.to_display(), fp.hint(), full.clone()] {
            prop_assert!(FingerprintPrefix::parse(&shown).unwrap().matches(&fp));
        }
        let key = PublicKey::from_bytes(bytes);
        prop_assert_eq!(PublicKey::from_hex(&key.to_hex()).unwrap(), key);
    }

    #[test]
    fn parsers_never_panic(s in near_grammar()) {
        let _ = Fingerprint::parse_full(&s);
        let _ = FingerprintPrefix::parse_pin(&s);
        let _ = PublicKey::from_hex(&s);
        if let Ok(p) = FingerprintPrefix::parse(&s) {
            let n = p.as_str().len();
            prop_assert!((FINGERPRINT_PREFIX_MIN..=FINGERPRINT_FULL_LEN).contains(&n));
            prop_assert!(
                p.as_str()
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c))
            );
        }
    }

    #[test]
    fn target_classification_is_consistent(s in near_grammar()) {
        match Target::parse(&s) {
            Ok(Target::Fingerprint(p)) => {
                // Classification works on the trimmed input.
                prop_assert!(looks_like_fingerprint(s.trim()));
                prop_assert_eq!(FingerprintPrefix::parse(s.trim()).unwrap(), p);
            }
            Ok(Target::Name(n)) => {
                prop_assert_eq!(n.as_str(), s.trim());
                prop_assert!(
                    FingerprintPrefix::parse(&n).is_err() || !looks_like_fingerprint(&n)
                );
            }
            Ok(Target::Addr(a)) => prop_assert_ne!(a.port(), 0),
            Ok(Target::Host { port, .. }) => prop_assert_ne!(port, 0),
            Err(_) => {}
        }
    }
}
