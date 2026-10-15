// SPDX-License-Identifier: MPL-2.0
//! Pairing between two processes.

use std::net::SocketAddr;

use dropumi_domain::peer::Intent;
use dropumi_domain::transfer::CancelReason;
use dropumi_protocol::message::Message;

use crate::support::raw_peer::{RawIdentity, RawSession};
use crate::support::*;

fn listen(dev: &Device) -> (Proc, u16) {
    let mut p = Proc::spawn(
        "pair-listen",
        dev.config(),
        &args(&[
            "pair",
            "--listen",
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--no-mdns",
        ]),
    );
    let port = p.wait_event("listening")["port"].as_u64().unwrap() as u16;
    (p, port)
}

#[test]
fn sas_both_confirm_mutual_trust_then_trusted_transfer() {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    let (mut l, port) = listen(&b);
    let mut i = Proc::spawn(
        "pair-to",
        a.config(),
        &args(&["pair", "--to", &format!("127.0.0.1:{port}")]),
    );
    let code_i = i.wait_event("sas")["code"].as_str().unwrap().to_owned();
    let code_l = l.wait_event("sas")["code"].as_str().unwrap().to_owned();
    assert_eq!(code_i, code_l);
    assert_eq!(code_i.len(), 6);
    i.cmd("confirm");
    l.cmd("confirm");
    assert_eq!(i.wait_exit(EXIT), 0, "{}", i.stderr());
    assert_eq!(l.wait_exit(EXIT), 0, "{}", l.stderr());
    assert_eq!(a.trusted(), vec![b.fingerprint.clone()]);
    assert_eq!(b.trusted(), vec![a.fingerprint.clone()]);

    // Now a TrustedOnly transfer works without any prompt.
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("after-pairing.txt"), b"paired");
    let (mut rx, rport) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&a, std::slice::from_ref(&f), rport, &[]);
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    assert_eq!(rx.wait_exit(EXIT), 0);
    assert_eq!(sha256(&f), sha256(&dst.path().join("after-pairing.txt")));
}

#[test]
fn sas_deny_stores_nothing() {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    let (mut l, port) = listen(&b);
    let mut i = Proc::spawn(
        "pair-to",
        a.config(),
        &args(&["pair", "--to", &format!("127.0.0.1:{port}")]),
    );
    i.wait_event("sas");
    l.wait_event("sas");
    i.cmd("confirm");
    l.cmd("deny");
    assert_eq!(i.wait_exit(EXIT), 6);
    assert_eq!(l.wait_exit(EXIT), 6);
    assert_eq!(i.wait_event("failed")["code"], "pairing_rejected");
    assert!(a.trusted().is_empty());
    assert!(b.trusted().is_empty());
}

/// The listener's prompt ends with the peer's cancel; nobody answers it.
/// Regression (Windows CI): the listener used to wait for its user, write
/// its answer to the closed peer, and lose the peer's unread "no" to the
/// reset ("connection lost", exit 5).
#[test]
fn cancel_during_sas_exits_4_and_stores_nothing() {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    let (mut l, port) = listen(&b);
    let mut i = Proc::spawn(
        "pair-to",
        a.config(),
        &args(&["pair", "--to", &format!("127.0.0.1:{port}")]),
    );
    i.wait_event("sas");
    l.wait_event("sas");
    i.cmd("cancel");
    assert_eq!(i.wait_exit(EXIT), 4, "{}", i.stderr());
    assert_eq!(i.wait_event("cancelled")["by"], "local");
    assert_eq!(l.wait_exit(EXIT), 6, "{}", l.stderr());
    assert_eq!(l.wait_event("failed")["code"], "pairing_rejected");
    assert!(a.trusted().is_empty());
    assert!(b.trusted().is_empty());
}

#[test]
fn name_target_with_no_mdns_explains_why() {
    let a = Device::new("Alice");
    let mut i = Proc::spawn(
        "pair-to",
        a.config(),
        &args(&["pair", "--to", "Office laptop 9", "--no-mdns"]),
    );
    assert_eq!(i.wait_exit(EXIT), 2);
    let out = format!("{:?}\n{}", i.seen, i.stderr());
    assert!(out.contains("--no-mdns disables discovery"), "{out}");
    assert!(a.trusted().is_empty());
}

/// While the code prompt is open, ignorable messages are skipped, and the
/// peer's early "yes" ends their run (§8.4): ten before it and ten after it
/// stay within the limit of 16 in a row, so the Cancel that follows is
/// reached (without the reset, the 17th ignorable would end the session).
#[test]
fn ignorable_messages_around_an_early_yes_are_skipped() {
    let b = Device::new("Bob");
    let raw = RawIdentity::new(0x46);
    let (mut l, port) = listen(&b);
    let (mut s, _hello) =
        RawSession::connect(SocketAddr::from(([127, 0, 0, 1], port)), &raw, Intent::Pair).unwrap();
    // [200, {}], an ignorable message type.
    let ignorable = [0x82, 0x18, 0xC8, 0xA0];
    for _ in 0..10 {
        s.send_control_bytes(&ignorable);
    }
    s.send(&Message::PairResult { confirmed: true });
    for _ in 0..10 {
        s.send_control_bytes(&ignorable);
    }
    s.send(&Message::Cancel {
        reason: CancelReason::User,
    });
    // Nobody answers the listener's prompt: only the peer's messages end it.
    let (replies, closed) = s.until_closed();
    assert!(closed, "the listener closed the session");
    assert!(replies.is_empty(), "a Cancel gets no reply: {replies:?}");
    drop(s);
    assert_eq!(l.wait_exit(EXIT), 6, "{}", l.stderr());
    assert_eq!(l.wait_event("failed")["code"], "pairing_rejected");
    assert!(b.trusted().is_empty());
}
