// SPDX-License-Identifier: MPL-2.0
//! Pairing between two processes.

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
    l.cmd("confirm");
    assert_eq!(l.wait_exit(EXIT), 6, "{}", l.stderr());
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
