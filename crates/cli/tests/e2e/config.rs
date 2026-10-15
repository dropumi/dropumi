// SPDX-License-Identifier: MPL-2.0
//! Configuration commands (`identity`, `trust`) against a real
//! configuration directory (Unix: the test uses directory modes).

use serde_json::Value;

use crate::support::*;

/// Index of the first event named `name`.
fn position(events: &[Value], name: &str) -> usize {
    events
        .iter()
        .position(|e| e["event"] == name)
        .unwrap_or_else(|| panic!("no {name} event: {events:?}"))
}

/// Asserts a `warning` about the unconfirmed directory sync comes before
/// the command's result event `result`, and returns the result event.
fn warned_before(events: &[Value], result: &str) -> Value {
    let warning = position(events, "warning");
    assert!(
        events[warning]["message"]
            .as_str()
            .unwrap()
            .starts_with("the change was saved but could not be confirmed on disk"),
        "{events:?}"
    );
    let at = position(events, result);
    assert!(warning < at, "warning after {result}: {events:?}");
    events[at].clone()
}

/// Puts the configuration directory back to 0700 when dropped, so a failed
/// assertion still leaves a directory the temp-dir cleanup can remove.
struct RestoreMode<'a>(&'a std::path::Path);

impl Drop for RestoreMode<'_> {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(self.0, std::fs::Permissions::from_mode(0o700));
    }
}

/// Changes saved without a confirmed directory sync (the configuration
/// directory is write and search only, so the directory fsync after each
/// write cannot open it) are applied: each command prints the warning,
/// then its normal result, and exits 0. `identity reset` prints the new
/// fingerprint; `trust remove` reports `existed: null`.
#[test]
fn not_durable_changes_are_applied_with_a_warning() {
    use std::os::unix::fs::PermissionsExt;
    let me = Device::new("Me");
    let peer = Device::new("Peer");
    let config = me.config();

    std::fs::set_permissions(config, std::fs::Permissions::from_mode(0o300)).unwrap();
    let restore = RestoreMode(config);
    if std::fs::File::open(config).is_ok() {
        // Skipped: this user (for example root) can open the directory
        // anyway, so its sync cannot be made to fail this way.
        return;
    }

    let (code, events) = run(config, &["identity", "reset", "--yes"]);
    assert_eq!(code, 0, "identity reset: {events:?}");
    let reset = warned_before(&events, "identity");
    let new_fp = reset["fingerprint"].as_str().unwrap().to_owned();
    assert_eq!(new_fp.len(), 52, "{reset}");
    assert_ne!(new_fp, me.fingerprint);

    let (code, events) = run(config, &["trust", "add", &peer.fingerprint, "--name", "X"]);
    assert_eq!(code, 0, "trust add: {events:?}");
    let devices = warned_before(&events, "trusted_devices")["devices"].clone();
    assert_eq!(devices.as_array().unwrap().len(), 1, "{devices}");
    assert_eq!(devices[0]["fingerprint"], peer.fingerprint.as_str());
    assert_eq!(devices[0]["alias"], "X");

    let (code, events) = run(config, &["trust", "remove", &peer.fingerprint]);
    assert_eq!(code, 0, "trust remove: {events:?}");
    let removed = warned_before(&events, "trust_removed");
    assert_eq!(removed["fingerprint"], peer.fingerprint.as_str());
    assert!(removed["existed"].is_null(), "{removed}");

    // With the directory readable again, the changes are all in place.
    drop(restore);
    let (code, events) = run(config, &["identity", "show"]);
    assert_eq!(code, 0, "identity show: {events:?}");
    assert_eq!(events[position(&events, "identity")]["fingerprint"], new_fp);
    assert!(me.trusted().is_empty());
}
