// SPDX-License-Identifier: MPL-2.0
//! mDNS discovery (opt-in: `DROPUMI_TEST_MDNS=1`; needs working multicast).

use crate::support::*;

#[test]
#[ignore = "needs multicast; run with DROPUMI_TEST_MDNS=1 and --ignored"]
fn mdns_discover_and_send_by_fingerprint() {
    if std::env::var("DROPUMI_TEST_MDNS").as_deref() != Ok("1") {
        eprintln!("skipped: set DROPUMI_TEST_MDNS=1");
        return;
    }
    let a = Device::new("Alice mDNS");
    let b = Device::new("Bob mDNS");
    a.trust(&b, "Bob");
    b.trust(&a, "Alice");
    let dst = tempfile::tempdir().unwrap();
    let mut rx = Proc::spawn(
        "receiver",
        b.config(),
        &args(&[
            "receive",
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--mdns-loopback",
            "--accept",
            "trusted",
            "--once",
            "--dir",
            &dst.path().display().to_string(),
        ]),
    );
    rx.wait_event("listening");
    rx.wait_event("advertising");
    let (code, events) = run(
        a.config(),
        &["discover", "--timeout-ms", "4000", "--mdns-loopback"],
    );
    assert_eq!(code, 0);
    let hint = &b.fingerprint[..8];
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "peer_found" && e["fp_hint"] == hint),
        "{events:?}"
    );
    let src = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("m.txt"), b"mdns");
    let mut tx = Proc::spawn(
        "sender",
        a.config(),
        &args(&[
            "send",
            &f.display().to_string(),
            "--to",
            hint,
            "--mdns-loopback",
            "--discover-timeout-ms",
            "4000",
        ]),
    );
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    assert_eq!(rx.wait_exit(EXIT), 0);
    assert_eq!(sha256(&f), sha256(&dst.path().join("m.txt")));
}
