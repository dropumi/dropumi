// SPDX-License-Identifier: MPL-2.0
//! Human-readable and NDJSON output.
//!
//! With `--json`, every line on stdout is one JSON object
//! `{"v":1,"t_ms":…,"event":"…",…}`; logs go to stderr. Every
//! peer-supplied string shown to a human passes through [`terminal_safe`].
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::io::{IsTerminal, Write};
use std::sync::Mutex;
use std::time::Instant;

use dropumi_application::events::{AppEvent, PeerView, TransferOutcome};
use dropumi_application::identity::LocalIdentity;
use dropumi_domain::policy::{PolicyMode, TransferPolicy};
use dropumi_domain::text::strip_unsafe;
use dropumi_domain::transfer::TransferProgress;
use dropumi_domain::trust::TrustedDevice;
use dropumi_protocol::sas::SasCode;
use serde_json::{Map, Value, json};

/// Removes ANSI escape sequences, then every control, separator and format
/// character (see [`strip_unsafe`]).
#[must_use]
pub fn terminal_safe(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    // CSI: parameters until a final byte in @..~.
                    for f in chars.by_ref() {
                        if ('@'..='~').contains(&f) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    // OSC: until BEL or ESC \.
                    while let Some(f) = chars.next() {
                        if f == '\u{7}' {
                            break;
                        }
                        if f == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
            continue;
        }
        out.push(c);
    }
    strip_unsafe(&out).into_owned()
}

/// Policy as a stable string.
#[must_use]
pub fn policy_str(p: &TransferPolicy) -> &'static str {
    match p.mode {
        PolicyMode::Off => "off",
        PolicyMode::TrustedOnly => "trusted-only",
        PolicyMode::Everyone { .. } => "everyone",
    }
}

/// JSON object for a peer.
#[must_use]
pub fn peer_json(p: &PeerView) -> Value {
    json!({
        "fingerprint": p.fingerprint.to_full(),
        "fingerprint_display": p.fingerprint.to_display(),
        "trusted": p.trusted,
        "alias": p.alias.as_ref().map(|a| a.as_str()),
        "claimed_name": p.claimed_name.as_str(),
    })
}

/// One-line human description: the local alias first, the peer's own name
/// only as a claim.
#[must_use]
pub fn peer_line(p: &PeerView, sas: Option<&SasCode>) -> String {
    let claimed = terminal_safe(p.claimed_name.as_str());
    let mut line = match (&p.alias, p.trusted) {
        (Some(alias), true) => format!(
            "Trusted \"{}\" (claims \"{claimed}\") · {}",
            terminal_safe(alias.as_str()),
            p.fingerprint.to_display()
        ),
        (None, true) => format!(
            "Trusted device (claims \"{claimed}\") · {}",
            p.fingerprint.to_display()
        ),
        (_, false) => format!(
            "Unverified device (claims \"{claimed}\") · fp {}",
            p.fingerprint.to_display()
        ),
    };
    if let Some(code) = sas {
        line.push_str(&format!(" · SAS {code}"));
    }
    line
}

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    #[allow(clippy::cast_precision_loss)]
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

/// Writes events to stdout (and human progress to stderr).
#[derive(Debug)]
pub struct Output {
    json: bool,
    start: Instant,
    local: Mutex<Option<(String, String, String)>>,
    progress: Mutex<Option<(Instant, Instant)>>,
    stderr_tty: bool,
}

impl Output {
    /// New output; `json` selects NDJSON.
    #[must_use]
    pub fn new(json: bool) -> Self {
        Self {
            json,
            start: Instant::now(),
            local: Mutex::new(None),
            progress: Mutex::new(None),
            stderr_tty: std::io::stderr().is_terminal(),
        }
    }

    /// Remembers this device's identity for `listening` lines.
    pub fn set_local(&self, id: &LocalIdentity) {
        if let Ok(mut l) = self.local.lock() {
            *l = Some((
                id.fingerprint().to_full(),
                id.fingerprint().to_display(),
                id.name().as_str().to_owned(),
            ));
        }
    }

    /// Writes one NDJSON event (no-op in human mode).
    pub fn event(&self, name: &str, fields: Value) {
        if !self.json {
            return;
        }
        let mut obj = Map::new();
        obj.insert("v".into(), json!(1));
        obj.insert(
            "t_ms".into(),
            json!(u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)),
        );
        obj.insert("event".into(), json!(name));
        if let Value::Object(f) = fields {
            obj.extend(f);
        }
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{}", Value::Object(obj));
        let _ = out.flush();
    }

    /// Writes a human line to stdout (no-op in JSON mode).
    pub fn human(&self, line: &str) {
        if self.json {
            return;
        }
        self.clear_progress();
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }

    /// Writes a human prompt or notice to stderr.
    pub fn notice(&self, line: &str) {
        self.clear_progress();
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "{line}");
        let _ = err.flush();
    }

    fn clear_progress(&self) {
        if let Ok(mut p) = self.progress.lock()
            && p.take().is_some()
            && self.stderr_tty
        {
            let mut err = std::io::stderr().lock();
            let _ = write!(err, "\r\u{1b}[2K");
            let _ = err.flush();
        }
    }

    /// Identity event / lines.
    pub fn identity(&self, id: &LocalIdentity) {
        self.event(
            "identity",
            json!({
                "fingerprint": id.fingerprint().to_full(),
                "fingerprint_display": id.fingerprint().to_display(),
                "public_key": id.public_key().to_hex(),
                "name": id.name().as_str(),
            }),
        );
        self.human(&format!(
            "Name:        {}",
            terminal_safe(id.name().as_str())
        ));
        self.human(&format!("Fingerprint: {}", id.fingerprint().to_display()));
        self.human(&format!("Full:        {}", id.fingerprint().to_full()));
    }

    /// Trusted-devices event / lines.
    pub fn trusted(&self, devices: &[TrustedDevice]) {
        let list: Vec<Value> = devices
            .iter()
            .map(|d| {
                json!({
                    "fingerprint": d.fingerprint.to_full(),
                    "fingerprint_display": d.fingerprint.to_display(),
                    "alias": d.alias.as_ref().map(|a| a.as_str()),
                    "method": d.method.as_str(),
                    "added_at": d.added_at.as_secs(),
                    "last_seen": d.last_seen.map(|t| t.as_secs()),
                })
            })
            .collect();
        self.event("trusted_devices", json!({ "devices": list }));
        if devices.is_empty() {
            self.human("No trusted devices.");
        }
        for d in devices {
            self.human(&format!(
                "{}  {}  ({})",
                d.fingerprint.to_display(),
                d.alias
                    .as_ref()
                    .map_or_else(|| "(no alias)".into(), |a| terminal_safe(a.as_str())),
                d.method.as_str()
            ));
        }
    }

    /// Final outcome event / line.
    pub fn outcome(&self, o: &TransferOutcome) {
        match o {
            TransferOutcome::Completed {
                transfer_id,
                files_stored,
                bytes,
                elapsed_ms,
            } => {
                self.event(
                    "completed",
                    json!({"transfer_id": transfer_id.to_hex(), "status": "completed",
                           "files_stored": files_stored, "failed": 0, "bytes": bytes,
                           "elapsed_ms": elapsed_ms}),
                );
                self.human(&format!(
                    "Done: {files_stored} file(s), {} in {:.1} s",
                    human_bytes(*bytes),
                    *elapsed_ms as f64 / 1000.0
                ));
            }
            TransferOutcome::CompletedWithFailures {
                transfer_id,
                files_stored,
                failed,
                bytes,
                elapsed_ms,
            } => {
                self.event(
                    "completed",
                    json!({"transfer_id": transfer_id.to_hex(), "status": "completed_with_failures",
                           "files_stored": files_stored, "failed": failed, "bytes": bytes,
                           "elapsed_ms": elapsed_ms}),
                );
                self.human(&format!(
                    "Finished with failures: {files_stored} stored, {failed} not stored"
                ));
            }
            TransferOutcome::Rejected {
                transfer_id,
                reason,
            } => {
                self.event(
                    "rejected",
                    json!({"transfer_id": transfer_id.to_hex(), "reason": reason.as_str()}),
                );
                self.human(&format!("Rejected ({})", reason.as_str()));
            }
            TransferOutcome::Cancelled {
                transfer_id,
                by,
                reason,
            } => {
                let by = match by {
                    dropumi_domain::transfer::Party::Local => "local",
                    dropumi_domain::transfer::Party::Peer => "peer",
                };
                self.event(
                    "cancelled",
                    json!({"transfer_id": transfer_id.map(|t| t.to_hex()), "by": by,
                           "reason": reason.as_str()}),
                );
                self.human(&format!("Cancelled by {by} ({})", reason.as_str()));
            }
            TransferOutcome::Failed { transfer_id, error } => {
                self.failed(transfer_id.map(|t| t.to_hex()), error)
            }
        }
    }

    /// Pairing was cancelled locally (control `cancel`, Ctrl-C).
    pub fn pairing_cancelled(&self) {
        self.event(
            "cancelled",
            json!({"transfer_id": null, "by": "local", "reason": "user"}),
        );
        self.human("Pairing cancelled; nothing was stored.");
    }

    /// Failure event / line.
    pub fn failed(
        &self,
        transfer_id: Option<String>,
        error: &dropumi_domain::error::TransferError,
    ) {
        self.event(
            "failed",
            json!({"transfer_id": transfer_id, "code": error.code(),
                   "phase": error.phase().map(|p| p.as_str()), "message": error.to_string()}),
        );
        self.notice(&format!("Failed: {error}"));
    }

    /// A warning.
    pub fn warning(&self, message: &str) {
        self.event("warning", json!({ "message": message }));
        if !self.json {
            self.notice(&format!("Warning: {message}"));
        }
    }

    fn render_progress(&self, p: &TransferProgress) {
        if self.json || !self.stderr_tty {
            return;
        }
        let now = Instant::now();
        let Ok(mut state) = self.progress.lock() else {
            return;
        };
        let (first, last) = state.get_or_insert((now, now - std::time::Duration::from_secs(1)));
        let done = p.bytes_done() == p.bytes_total();
        if !done && now.duration_since(*last).as_millis() < 100 {
            return;
        }
        *last = now;
        let secs = now.duration_since(*first).as_secs_f64().max(0.001);
        #[allow(clippy::cast_precision_loss)]
        let rate = p.bytes_done() as f64 / secs;
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let eta = if rate > 0.0 {
            ((p.bytes_total().saturating_sub(p.bytes_done())) as f64 / rate) as u64
        } else {
            0
        };
        let mut err = std::io::stderr().lock();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let _ = write!(
            err,
            "\r\u{1b}[2K{} / {} ({:.0}%)  {}/s  ETA {}s",
            human_bytes(p.bytes_done()),
            human_bytes(p.bytes_total()),
            p.fraction() * 100.0,
            human_bytes(rate as u64),
            eta
        );
        let _ = err.flush();
    }

    /// Renders an application event.
    pub fn app_event(&self, e: &AppEvent) {
        match e {
            AppEvent::Listening { addrs, policy } => {
                let local = self.local.lock().ok().and_then(|l| l.clone());
                let (fp, fpd, name) = local.unwrap_or_default();
                let port = addrs.first().map_or(0, std::net::SocketAddr::port);
                self.event(
                    "listening",
                    json!({"addrs": addrs.iter().map(ToString::to_string).collect::<Vec<_>>(),
                           "port": port, "fingerprint": fp, "fingerprint_display": fpd,
                           "name": name, "policy": policy_str(policy)}),
                );
                for a in addrs {
                    self.human(&format!("Listening on {a} as \"{}\"", terminal_safe(&name)));
                }
                self.human(&format!("Fingerprint: {fpd}  (full: {fp})"));
                self.human(&format!("Policy: {}", policy_str(policy)));
            }
            AppEvent::Advertising { instance } => {
                self.event("advertising", json!({ "instance": instance }));
            }
            AppEvent::DiscoveryUnavailable { reason } => {
                self.event("discovery_unavailable", json!({ "reason": reason }));
                self.notice(&format!(
                    "Discovery unavailable ({reason}); manual addresses still work."
                ));
            }
            AppEvent::PeerFound(p) => {
                self.event(
                    "peer_found",
                    json!({"instance": p.instance, "name": p.name.as_str(), "fp_hint": p.fp_hint,
                           "addrs": p.addrs.iter().map(ToString::to_string).collect::<Vec<_>>(),
                           "port": p.port, "versions": p.versions}),
                );
                self.human(&format!(
                    "{}  fp {}  {}",
                    terminal_safe(p.name.as_str()),
                    p.fp_hint.as_deref().unwrap_or("????????"),
                    p.addrs
                        .iter()
                        .map(|a| format!("{a}:{}", p.port))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            AppEvent::Connecting { addr } => {
                self.event("connecting", json!({ "addr": addr.to_string() }));
            }
            AppEvent::Connected { peer, handshake_us } => {
                self.event(
                    "connected",
                    json!({"peer": peer_json(peer), "handshake_ms": handshake_us / 1000,
                           "handshake_us": handshake_us}),
                );
                self.human(&format!("Connected: {}", peer_line(peer, None)));
            }
            AppEvent::Sas { peer, code } => {
                self.event(
                    "sas",
                    json!({"code": code.as_digits(), "peer": peer_json(peer)}),
                );
                if peer.trusted {
                    // Shown so an untrusting peer can compare; nothing to do here.
                    self.human(&format!("Session code: {code}"));
                } else {
                    self.notice(&format!(
                        "Verification code: {code}  — check that the other device shows the same code.\n  {}",
                        peer_line(peer, None)
                    ));
                }
            }
            AppEvent::OfferSent {
                transfer_id,
                peer,
                files,
                total_bytes,
            } => {
                self.event(
                    "offer_sent",
                    json!({"transfer_id": transfer_id.to_hex(), "peer": peer_json(peer),
                           "files": files, "total_bytes": total_bytes}),
                );
                self.human(&format!(
                    "Offered {files} file(s), {} — waiting for the receiver…",
                    human_bytes(*total_bytes)
                ));
            }
            AppEvent::OfferReceived {
                transfer_id,
                peer,
                sas,
                files,
                total_bytes,
            } => {
                let list: Vec<Value> = files
                    .iter()
                    .enumerate()
                    .map(|(i, f)| json!({"index": i, "name": f.name, "size": f.size}))
                    .collect();
                self.event(
                    "offer_received",
                    json!({"transfer_id": transfer_id.to_hex(), "peer": peer_json(peer),
                           "sas": sas.map(|s| s.as_digits()), "files": list,
                           "total_bytes": total_bytes}),
                );
                self.human(&format!(
                    "Incoming: {} file(s), {} from {}",
                    files.len(),
                    human_bytes(*total_bytes),
                    peer_line(peer, sas.as_ref())
                ));
                for f in files.iter().take(20) {
                    self.human(&format!(
                        "  {}  ({})",
                        terminal_safe(&f.name),
                        human_bytes(f.size)
                    ));
                }
                if files.len() > 20 {
                    self.human(&format!("  … and {} more", files.len() - 20));
                }
            }
            AppEvent::Accepted { transfer_id } => {
                self.event("accepted", json!({ "transfer_id": transfer_id.to_hex() }));
            }
            AppEvent::Progress {
                transfer_id,
                progress,
            } => {
                self.event(
                    "progress",
                    json!({"transfer_id": transfer_id.to_hex(), "bytes_done": progress.bytes_done(),
                           "bytes_total": progress.bytes_total(), "files_done": progress.files_done(),
                           "files_total": progress.files_total()}),
                );
                self.render_progress(progress);
            }
            AppEvent::FileSent {
                transfer_id,
                index,
                status,
            } => {
                self.event(
                    "file_sent",
                    json!({"transfer_id": transfer_id.to_hex(), "index": index, "status": status.as_str()}),
                );
            }
            AppEvent::FileSaved {
                transfer_id,
                index,
                name,
                saved_as,
                size,
                blake3,
            } => {
                self.event(
                    "file_saved",
                    json!({"transfer_id": transfer_id.to_hex(), "index": index, "name": name,
                           "saved_as": saved_as, "size": size, "blake3": blake3}),
                );
                self.human(&format!("Saved {}", terminal_safe(saved_as)));
            }
            AppEvent::FileFailed {
                transfer_id,
                index,
                status,
            } => {
                self.event(
                    "file_failed",
                    json!({"transfer_id": transfer_id.to_hex(), "index": index, "status": status.as_str()}),
                );
                self.notice(&format!("File {index} not stored ({})", status.as_str()));
            }
            AppEvent::Paired { peer } => {
                self.event("paired", json!({ "peer": peer_json(peer) }));
                self.human(&format!("Paired: {}", peer_line(peer, None)));
            }
            AppEvent::SessionClosed { peer, code } => {
                self.event(
                    "session_closed",
                    json!({"peer": peer.as_ref().map(peer_json), "code": code}),
                );
                if *code != "normal" {
                    self.notice(&format!("Session closed ({code})"));
                }
            }
            AppEvent::PolicyChanged { policy } => {
                self.event("policy_changed", json!({ "policy": policy_str(policy) }));
                self.human(&format!("Policy: {}", policy_str(policy)));
            }
            AppEvent::Finished(o) => self.outcome(o),
            AppEvent::Warning { message } => self.warning(message),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use dropumi_domain::device::DeviceName;
    use dropumi_domain::identity::{Fingerprint, PublicKey};

    use super::*;

    fn view(trusted: bool, alias: Option<&str>, claimed: &str) -> PeerView {
        PeerView {
            fingerprint: Fingerprint::from_digest([7; 32]),
            public_key: PublicKey::from_bytes([1; 32]),
            trusted,
            alias: alias.map(DeviceName::sanitize),
            claimed_name: DeviceName::sanitize(claimed),
        }
    }

    fn hostile_text() -> impl proptest::strategy::Strategy<Value = String> {
        use proptest::prelude::*;
        let piece = prop_oneof![
            any::<char>().prop_map(String::from),
            Just("\u{1b}[".to_owned()),
            Just("\u{1b}]".to_owned()),
            Just("\u{1b}".to_owned()),
            Just("\u{7}".to_owned()),
            Just("\u{202E}".to_owned()),
            Just("\u{200B}".to_owned()),
            "[0-9;]{0,6}[A-Za-z@~]?",
        ];
        prop::collection::vec(piece, 0..16).prop_map(|v| v.concat())
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(2_000))]

        #[test]
        fn terminal_safe_output_is_inert(raw in hostile_text()) {
            let out = terminal_safe(&raw);
            proptest::prop_assert!(!out.contains('\u{1b}'), "escape left in {:?}", out);
            proptest::prop_assert!(
                !out.chars().any(dropumi_domain::text::is_unsafe_char),
                "unsafe char in {:?}",
                out
            );
            proptest::prop_assert_eq!(terminal_safe(&out), out);
        }
    }

    #[test]
    fn terminal_safe_strips_ansi_bidi_and_invisibles() {
        let s = "a\u{1b}[31mred\u{1b}[0m\u{1b}]0;title\u{7}b\u{202E}c\u{2028}d\u{2029}e\u{200B}f\u{FEFF}g\u{AD}h\u{E0041}i";
        assert_eq!(terminal_safe(s), "aredbcdefghi");
        let keep = "\u{1F469}\u{200D}\u{1F4BB} \u{2764}\u{FE0F}";
        assert_eq!(terminal_safe(keep), keep);
    }

    #[test]
    fn ndjson_escapes_peer_strings() {
        let v = peer_json(&view(false, None, "evil\"name\nwith\\stuff"));
        let line = v.to_string();
        assert!(!line.contains('\n'));
        let back: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(back["claimed_name"], "evil\"namewith\\stuff");
    }

    #[test]
    fn human_peer_line_alias_first_claims_label() {
        let line = peer_line(&view(true, Some("Beta"), "Alpha"), None);
        assert!(
            line.starts_with("Trusted \"Beta\" (claims \"Alpha\") · "),
            "{line}"
        );
        let line = peer_line(&view(false, None, "X"), None);
        assert!(
            line.starts_with("Unverified device (claims \"X\") · fp "),
            "{line}"
        );
    }
}
