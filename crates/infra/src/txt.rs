// SPDX-License-Identifier: MPL-2.0
//! DNS-SD TXT records: hints only, never identity.
//!
//! Keys: `v` (comma-separated protocol versions), `n` (device name, at most
//! 64 bytes), `fp` (8-character fingerprint hint). Unknown keys are ignored;
//! an invalid `fp` is dropped; `n` is sanitized.

use dropumi_application::ports::Advertisement;
use dropumi_domain::device::DeviceName;
use dropumi_domain::identity::FINGERPRINT_HINT_LEN;
use dropumi_domain::limits::MAX_DEVICE_NAME_DISPLAY_BYTES;
use dropumi_domain::text::truncate_bytes;

/// Builds TXT properties for an advertisement.
#[must_use]
pub fn build(ad: &Advertisement) -> Vec<(String, String)> {
    let versions: Vec<String> = ad.versions.iter().map(u16::to_string).collect();
    vec![
        ("v".into(), versions.join(",")),
        (
            "n".into(),
            truncate_bytes(ad.name.as_str(), MAX_DEVICE_NAME_DISPLAY_BYTES).to_owned(),
        ),
        ("fp".into(), ad.fp_hint.clone()),
    ]
}

/// Parsed TXT hints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxtHints {
    /// Sanitized device name.
    pub name: DeviceName,
    /// Valid fingerprint hint, if present.
    pub fp_hint: Option<String>,
    /// Advertised versions (invalid entries skipped).
    pub versions: Vec<u16>,
}

/// Parses TXT properties.
pub fn parse<'a>(props: impl IntoIterator<Item = (&'a str, &'a str)>) -> TxtHints {
    let mut name = None;
    let mut fp_hint = None;
    let mut versions = Vec::new();
    for (k, v) in props {
        match k {
            "n" => name = Some(DeviceName::sanitize(v)),
            "fp" => {
                if v.len() == FINGERPRINT_HINT_LEN
                    && v.chars()
                        .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c))
                {
                    fp_hint = Some(v.to_owned());
                }
            }
            "v" => versions = v.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
            _ => {}
        }
    }
    TxtHints {
        name: name.unwrap_or_else(|| DeviceName::sanitize("")),
        fp_hint,
        versions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_parse_valid() {
        let ad = Advertisement {
            name: DeviceName::sanitize("Office PC"),
            fp_hint: "ABCDEFG2".into(),
            port: 1,
            versions: vec![1, 2],
        };
        let props = build(&ad);
        let h = parse(props.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        assert_eq!(h.name.as_str(), "Office PC");
        assert_eq!(h.fp_hint.as_deref(), Some("ABCDEFG2"));
        assert_eq!(h.versions, vec![1, 2]);
    }

    #[test]
    fn txt_rejects_bad_fp_hint() {
        for bad in ["abcdefg2", "ABCDEFG1", "ABCDEFG", "ABCDEFGHI", ""] {
            let h = parse([("fp", bad), ("x", "ignored")]);
            assert_eq!(h.fp_hint, None, "{bad}");
        }
    }

    #[test]
    fn txt_sanitizes_name_64b() {
        let long = format!("evil\u{202E}{}", "x".repeat(200));
        let h = parse([("n", long.as_str())]);
        assert!(h.name.as_str().len() <= 64);
        assert!(!h.name.as_str().contains('\u{202E}'));
        assert_eq!(parse([("v", "1,abc, 3")]).versions, vec![1, 3]);
    }
}
