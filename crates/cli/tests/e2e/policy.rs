// SPDX-License-Identifier: MPL-2.0
//! Authorization, consent and identity binding.

use crate::support::*;

#[test]
fn trusted_only_rejects_unknown_sender() {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    let c = Device::new("Carol");
    a.trust(&b, "Bob");
    b.trust(&c, "Carol");
    c.trust(&b, "Bob");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("x.txt"), b"x");
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
    let mut tx = sender(&a, std::slice::from_ref(&f), port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 6);
    assert_eq!(tx.wait_event("failed")["code"], "not_authorized");
    assert!(dir_names(dst.path()).is_empty());
    // The receiver keeps serving trusted senders.
    let mut ok = sender(&c, &[f], port, &[]);
    assert_eq!(ok.wait_exit(EXIT), 0, "{}", ok.stderr());
    rx.cmd("quit");
    assert_eq!(rx.wait_exit(EXIT), 0);
    assert_eq!(dir_names(dst.path()).len(), 1);
}

#[test]
fn receiver_declines() {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    a.trust(&b, "Bob");
    b.trust(&a, "Alice");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("x.txt"), b"x");
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "prompt", "--once"]);
    let mut tx = sender(&a, &[f], port, &[]);
    rx.wait_event("offer_received");
    rx.cmd("reject");
    assert_eq!(tx.wait_exit(EXIT), 3);
    assert_eq!(rx.wait_exit(EXIT), 3);
    assert!(dir_names(dst.path()).is_empty());
}

#[test]
fn sender_refuses_unverified_receiver() {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    b.trust(&a, "Alice");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("secret-name.txt"), b"x");
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
    let mut tx = sender(&a, &[f], port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 6);
    assert_eq!(tx.wait_event("failed")["code"], "unverified_peer");
    let closed = rx.wait_event("session_closed");
    assert_eq!(closed["code"], "handshake_failed");
    assert!(!rx.saw("offer_received"), "the sender revealed nothing");
    rx.cmd("quit");
    rx.wait_exit(EXIT);
}

#[test]
fn expect_fingerprint_mismatch() {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    let other = Device::new("Other");
    a.trust(&b, "Bob");
    b.trust(&a, "Alice");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("x.txt"), b"x");
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
    let mut tx = sender(
        &a,
        &[f],
        port,
        &["--expect-fingerprint", &other.fingerprint[..24]],
    );
    assert_eq!(tx.wait_exit(EXIT), 6);
    assert_eq!(tx.wait_event("failed")["code"], "peer_identity_mismatch");
    assert!(!rx.saw("offer_received"));
    rx.cmd("quit");
    rx.wait_exit(EXIT);
}

#[test]
fn send_by_ip_reports_actual_trusted_peer() {
    // The sender trusts A ("Alpha") and B ("Beta"). B renames itself
    // "Alpha". Sending by IP to B shows B's alias first and the claim second.
    let s = Device::new("Sender");
    let a = Device::new("Alpha");
    let b = Device::new("Beta");
    s.trust(&a, "Alpha");
    s.trust(&b, "Beta");
    b.trust(&s, "Sender");
    let (code, _) = run(b.config(), &["identity", "set-name", "Alpha"]);
    assert_eq!(code, 0);
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("x.txt"), b"x");
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&s, &[f], port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    let peer = tx.wait_event("connected")["peer"].clone();
    assert_eq!(peer["fingerprint"], b.fingerprint.as_str());
    assert_eq!(peer["alias"], "Beta");
    assert_eq!(peer["claimed_name"], "Alpha");
    assert_eq!(peer["trusted"], true);
    assert_eq!(tx.wait_event("offer_sent")["peer"]["alias"], "Beta");
    assert_eq!(rx.wait_event("offer_received")["peer"]["alias"], "Sender");
    rx.wait_exit(EXIT);
}

#[test]
fn accept_never_rejects_without_prompt() {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    a.trust(&b, "Bob");
    b.trust(&a, "Alice");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("x.txt"), b"x");
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "never", "--once"]);
    let mut tx = sender(&a, &[f], port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 3);
    assert_eq!(rx.wait_exit(EXIT), 3);
}

#[test]
fn consent_timeout_rejects() {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    a.trust(&b, "Bob");
    b.trust(&a, "Alice");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("x.txt"), b"x");
    let (mut rx, port) = receiver(
        &b,
        dst.path(),
        &[
            "--accept",
            "prompt",
            "--once",
            "--consent-timeout-ms",
            "1000",
        ],
    );
    let mut tx = sender(&a, &[f], port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 3);
    assert_eq!(tx.wait_event("rejected")["reason"], "timeout");
    assert_eq!(rx.wait_exit(EXIT), 3);
}

#[test]
fn stale_accept_never_approves_a_later_offer() {
    // An accept that arrives after its offer is gone must not be kept for
    // the next offer, and a decision for another transfer id is ignored.
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    let c = Device::new("Carol");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("x.txt"), b"x");
    let (mut rx, port) = receiver(&b, dst.path(), &["--policy", "everyone"]);

    let mut first = sender(&a, std::slice::from_ref(&f), port, &["--allow-unverified"]);
    first.wait_event("sas");
    first.cmd("confirm");
    let offer1 = rx.wait_event("offer_received");
    first.cmd("cancel");
    assert_eq!(first.wait_exit(EXIT), 4, "{}", first.stderr());
    rx.wait_event("cancelled");
    rx.cmd("accept"); // too late: nothing is waiting
    let warning = rx.wait_event("warning");
    assert!(
        warning["message"]
            .as_str()
            .unwrap()
            .contains("no offer is waiting"),
        "{warning}"
    );

    let mut second = sender(&c, &[f], port, &["--allow-unverified"]);
    second.wait_event("sas");
    second.cmd("confirm");
    let id1 = offer1["transfer_id"].clone();
    let offer2 = rx.wait_where("second offer", STEP, |v| {
        v["event"] == "offer_received" && v["transfer_id"] != id1
    });
    let id1 = id1.as_str().unwrap();
    rx.cmd_for("accept", id1); // the wrong offer: ignored
    rx.wait_where("mismatch warning", STEP, |v| {
        v["event"] == "warning"
            && v["message"]
                .as_str()
                .is_some_and(|m| m.contains("transfer_id"))
    });
    rx.cmd_for("reject", offer2["transfer_id"].as_str().unwrap());
    assert_eq!(second.wait_exit(EXIT), 3, "{}", second.stderr());
    assert!(dir_names(dst.path()).is_empty(), "nothing was accepted");
    rx.cmd("quit");
    rx.wait_exit(EXIT);
}

#[cfg(unix)]
#[test]
fn ctrl_c_exits_130() {
    let b = Device::new("Bob");
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, _port) = receiver(&b, dst.path(), &[]);
    rx.interrupt();
    assert_eq!(rx.wait_exit(EXIT), 130);
}
