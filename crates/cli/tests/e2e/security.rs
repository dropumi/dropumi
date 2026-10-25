// SPDX-License-Identifier: MPL-2.0
//! Adversarial tests: eavesdropping, tampering, replay, version mismatch,
//! malformed traffic, hostile names and impersonation.

use std::net::SocketAddr;
use std::time::Duration;

use dropumi_domain::peer::Intent;
use dropumi_domain::transfer::{FileEntry, TransferId};
use dropumi_protocol::message::{ErrorCode, FileHash, Message};

use crate::support::proxy::{Proxy, Tamper, contains};
use crate::support::raw_peer::{
    RawIdentity, RawSession, impostor_responder, no_common_version_responder, preamble_only,
    send_raw_and_wait_close,
};
use crate::support::*;

fn trusted_pair(an: &str, bn: &str) -> (Device, Device) {
    let a = Device::new(an);
    let b = Device::new(bn);
    a.trust(&b, bn);
    b.trust(&a, an);
    (a, b)
}

fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn send_via(dev: &Device, files: &[std::path::PathBuf], addr: SocketAddr) -> Proc {
    let mut a = vec!["send".to_owned()];
    a.extend(files.iter().map(|f| f.display().to_string()));
    a.push("--to".into());
    a.push(addr.to_string());
    Proc::spawn("sender", dev.config(), &a)
}

#[test]
fn wire_capture_contains_no_plaintext() {
    let (a, b) = trusted_pair("AliceCanaryDevice", "BobCanaryDevice");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let content = b"CONTENT-CANARY-7f3a9c CONTENT-CANARY-7f3a9c".repeat(100);
    let f = write(&src.path().join("name-canary-51d2e8.txt"), &content);
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut proxy = Proxy::start(loopback(port), Tamper::None);
    let mut tx = send_via(&a, &[f], proxy.addr);
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    assert_eq!(rx.wait_exit(EXIT), 0);
    proxy.join();
    let wire = proxy.captured();
    assert!(wire.len() > content.len(), "captured the transfer");
    for canary in [
        &b"CONTENT-CANARY-7f3a9c"[..],
        b"name-canary-51d2e8",
        b"AliceCanaryDevice",
        b"BobCanaryDevice",
    ] {
        assert!(
            !contains(&wire, canary),
            "plaintext leaked: {}",
            String::from_utf8_lossy(canary)
        );
    }
    for key in [&a.public_key, &b.public_key] {
        assert!(
            !contains(&wire, &hex_bytes(key)),
            "static public key visible on the wire"
        );
    }
    assert_eq!(
        std::fs::read(dst.path().join("name-canary-51d2e8.txt")).unwrap(),
        content
    );
}

#[test]
fn incompatible_version_raw_peer() {
    let (a, b) = trusted_pair("Alice", "Bob");
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
    // A raw initiator that only speaks version 99.
    let reply = preamble_only(loopback(port), b"DRPM\x01\x01\x00\x63");
    assert_eq!(
        reply, b"DRPM\x01\x01\x01\x00\x01",
        "NO_COMMON_VERSION listing [1]"
    );
    // The receiver stays alive and still serves valid senders.
    let src = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("ok.txt"), b"ok");
    let mut tx = sender(&a, &[f], port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 0);
    rx.cmd("quit");
    rx.wait_exit(EXIT);
    // The CLI sender against a responder that only speaks version 2 exits 7.
    let (addr, h) = no_common_version_responder();
    let f2 = write(&src.path().join("v.txt"), b"v");
    let mut tx = send_via(&a, &[f2], addr);
    assert_eq!(tx.wait_exit(EXIT), 7);
    assert_eq!(tx.wait_event("failed")["code"], "incompatible_version");
    h.join().unwrap();
}

#[test]
fn preamble_tamper_breaks_handshake() {
    let (a, b) = trusted_pair("Alice", "Bob");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("x.txt"), b"x");
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
    // Rewrite the offered versions [1] to [3, 1]: still compatible, but the
    // prologue differs, so the handshake must fail (no silent downgrade).
    let mut proxy = Proxy::start(
        loopback(port),
        Tamper::RewritePrefix(
            b"DRPM\x01\x01\x00\x01".to_vec(),
            b"DRPM\x01\x02\x00\x03\x00\x01".to_vec(),
        ),
    );
    let mut tx = send_via(&a, &[f], proxy.addr);
    assert_eq!(tx.wait_exit(EXIT), 6);
    assert_eq!(tx.wait_event("failed")["code"], "handshake_failed");
    proxy.join();
    assert!(dir_names(dst.path()).is_empty());
    rx.cmd("quit");
    rx.wait_exit(EXIT);
}

#[test]
fn recorded_session_replay_rejected() {
    let (a, b) = trusted_pair("Alice", "Bob");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("once.txt"), b"only once");
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
    let mut proxy = Proxy::start(loopback(port), Tamper::None);
    let mut tx = send_via(&a, &[f], proxy.addr);
    assert_eq!(tx.wait_exit(EXIT), 0);
    proxy.join();
    let recorded = proxy.client_to_server.lock().unwrap().clone();
    std::fs::remove_file(dst.path().join("once.txt")).unwrap();
    rx.wait_event("completed");
    let before = ended(&mut rx);
    // Replay every byte the sender sent to a fresh connection.
    assert!(send_raw_and_wait_close(loopback(port), &recorded));
    rx.wait_count("end of the replayed session", before + 1, is_session_end);
    assert!(dir_names(dst.path()).is_empty(), "replay stored nothing");
    rx.cmd("quit");
    rx.wait_exit(EXIT);
    assert_eq!(rx.all("completed").len(), 1);
}

#[test]
fn ciphertext_bitflip_aborts() {
    let (a, b) = trusted_pair("Alice", "Bob");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write_pattern(&src.path().join("flip.bin"), 2 * 1024 * 1024, 9);
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut proxy = Proxy::start(loopback(port), Tamper::FlipAt(500_000));
    let mut tx = send_via(&a, &[f], proxy.addr);
    let code = tx.wait_exit(EXIT);
    assert!(code == 5 || code == 10, "sender exit {code}");
    assert_ne!(rx.wait_exit(EXIT), 0);
    let failed = rx.wait_event("failed");
    assert!(
        failed["code"] == "integrity" || failed["code"] == "protocol_violation",
        "{failed}"
    );
    proxy.join();
    for _ in 0..100 {
        if !has_part_files(dst.path()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        dir_names(dst.path()).is_empty(),
        "{:?}",
        dir_names(dst.path())
    );
}

fn entry(name: &str, size: u64) -> FileEntry {
    FileEntry {
        name: name.into(),
        size,
        media_type: None,
        modified: None,
    }
}

fn offer(raw: &mut RawSession, id: TransferId, files: Vec<FileEntry>) {
    let total = files.iter().map(|f| f.size).sum();
    raw.send(&Message::Offer {
        transfer_id: id,
        file_count: u32::try_from(files.len()).unwrap(),
        total_size: total,
    });
    raw.send(&Message::OfferFiles {
        transfer_id: id,
        first_index: 0,
        files,
    });
}

/// Counts terminal receiver events (sessions that ended).
/// Whether an event ends a receiver session.
fn is_session_end(e: &serde_json::Value) -> bool {
    matches!(
        e["event"].as_str(),
        Some("session_closed" | "failed" | "completed" | "cancelled" | "rejected")
    )
}

fn ended(rx: &mut Proc) -> usize {
    rx.pump();
    rx.seen.iter().filter(|e| is_session_end(e)).count()
}

#[test]
fn malformed_traffic_matrix() {
    let b = Device::new("Bob");
    let raw = RawIdentity::new(0x42);
    b.trust_fp(&raw.fingerprint, "Raw");
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
    let addr = loopback(port);
    let id = TransferId::from_bytes([5; 16]);

    type Case = Box<dyn Fn(&mut RawSession)>;
    let authenticated: Vec<(&str, Case)> = vec![
        (
            "garbage ciphertext",
            Box::new(|s: &mut RawSession| s.send_garbage_record(100)),
        ),
        (
            "unknown frame type",
            Box::new(|s: &mut RawSession| s.send_plain(&[0x03, 0x00])),
        ),
        (
            "unknown critical type",
            Box::new(|s: &mut RawSession| s.send_control_bytes(&[0x82, 0x05, 0xa0])),
        ),
        (
            "data before accept",
            Box::new(|s: &mut RawSession| s.send_data(b"x")),
        ),
        (
            "too many files",
            Box::new(move |s: &mut RawSession| {
                s.send(&Message::Offer {
                    transfer_id: id,
                    file_count: 10_001,
                    total_size: 0,
                });
            }),
        ),
        (
            "1025-byte name",
            Box::new(move |s: &mut RawSession| offer(s, id, vec![entry(&"n".repeat(1025), 1)])),
        ),
        (
            "dotdot name",
            Box::new(move |s: &mut RawSession| offer(s, id, vec![entry("..", 1)])),
        ),
        (
            "slash name",
            Box::new(move |s: &mut RawSession| offer(s, id, vec![entry("a/b", 1)])),
        ),
        (
            "metadata over 4 MiB",
            Box::new(move |s: &mut RawSession| {
                s.send(&Message::Offer {
                    transfer_id: id,
                    file_count: 10_000,
                    total_size: 0,
                });
                let mut first = 0u32;
                while first < 10_000 {
                    let files: Vec<_> = (0..60)
                        .map(|i| entry(&format!("{:0>1000}", first + i), 0))
                        .collect();
                    s.send(&Message::OfferFiles {
                        transfer_id: id,
                        first_index: first,
                        files,
                    });
                    first += 60;
                }
            }),
        ),
        (
            "out-of-order FileStart",
            Box::new(move |s: &mut RawSession| {
                offer(s, id, vec![entry("a", 1), entry("b", 1)]);
                let _ = s.recv(); // Accept
                s.send(&Message::FileStart {
                    index: 1,
                    offset: 0,
                });
            }),
        ),
        (
            "bytes beyond size",
            Box::new(move |s: &mut RawSession| {
                offer(s, id, vec![entry("a", 1)]);
                let _ = s.recv();
                s.send(&Message::FileStart {
                    index: 0,
                    offset: 0,
                });
                s.send_data(b"xy");
            }),
        ),
        (
            "empty data frame",
            Box::new(move |s: &mut RawSession| {
                offer(s, id, vec![entry("a", 1)]);
                let _ = s.recv();
                s.send(&Message::FileStart {
                    index: 0,
                    offset: 0,
                });
                s.send_plain(&[0x02]);
            }),
        ),
        (
            "indefinite CBOR",
            Box::new(|s: &mut RawSession| s.send_control_bytes(&[0x9f, 0x10, 0xa0, 0xff])),
        ),
        (
            "unsorted keys",
            Box::new(|s: &mut RawSession| {
                s.send_control_bytes(&[
                    0x82, 0x10, 0xa2, 0x02, 0x01, 0x01, 0x50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0,
                ]);
            }),
        ),
        (
            "trailing bytes",
            Box::new(|s: &mut RawSession| {
                let mut b = Vec::new();
                dropumi_protocol::message::encode(
                    &Message::Cancel {
                        reason: dropumi_domain::transfer::CancelReason::User,
                    },
                    &mut b,
                )
                .unwrap();
                b.push(0x00);
                s.send_control_bytes(&b);
            }),
        ),
    ];
    for (name, case) in &authenticated {
        let before = ended(&mut rx);
        let (mut s, _hello) = RawSession::connect(addr, &raw, Intent::Transfer)
            .unwrap_or_else(|e| panic!("{name}: connect failed: {e}"));
        case(&mut s);
        let (_msgs, closed) = s.until_closed();
        assert!(closed, "{name}: receiver kept the session open");
        rx.wait_count(&format!("{name}: session end"), before + 1, is_session_end);
        assert!(
            dir_names(dst.path()).is_empty(),
            "{name}: {:?}",
            dir_names(dst.path())
        );
    }
    // Pre-authentication garbage.
    assert!(
        send_raw_and_wait_close(addr, b"XRPM\x01\x01\x00\x01"),
        "bad magic"
    );
    assert!(
        send_raw_and_wait_close(addr, b"DRPM\x01\x01\x00\x01\xff\xff"),
        "oversized handshake length"
    );
    // The receiver survived everything and still serves a valid sender.
    let a = Device::new("Alice");
    a.trust(&b, "Bob");
    b.trust(&a, "Alice");
    let src = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("valid.txt"), b"still alive");
    let mut tx = sender(&a, &[f], port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    rx.cmd("quit");
    assert_eq!(rx.wait_exit(EXIT), 0);
    assert_eq!(dir_names(dst.path()).len(), 1);
}

#[test]
fn traversal_and_reserved_names() {
    let b = Device::new("Bob");
    let raw = RawIdentity::new(0x43);
    b.trust_fp(&raw.fingerprint, "Raw");
    let parent = tempfile::tempdir().unwrap();
    let dst = parent.path().join("inbox");
    std::fs::create_dir(&dst).unwrap();
    let (mut rx, port) = receiver(&b, &dst, &["--accept", "trusted", "--once"]);
    let names = [
        "CON.txt",
        "a.",
        "a ",
        "invoice\u{202E}fdp.exe",
        ".dropumi-x.part",
        "x\u{2028}y",
        "\u{200B}zero.txt",
    ];
    let files: Vec<_> = names.iter().map(|n| entry(n, 2)).collect();
    let id = TransferId::from_bytes([6; 16]);
    let (mut s, _) = RawSession::connect(loopback(port), &raw, Intent::Transfer).unwrap();
    offer(&mut s, id, files);
    assert!(matches!(s.recv(), Some(Message::Accept { .. })));
    for i in 0..names.len() {
        let index = u32::try_from(i).unwrap();
        s.send(&Message::FileStart { index, offset: 0 });
        s.send_data(b"ok");
        s.send(&Message::FileEnd {
            index,
            hash: FileHash::Blake3(*blake3::hash(b"ok").as_bytes()),
        });
        assert!(matches!(s.recv(), Some(Message::FileAck { .. })));
    }
    s.send(&Message::Complete { transfer_id: id });
    assert!(matches!(
        s.recv(),
        Some(Message::CompleteAck {
            stored_count: 7,
            ..
        })
    ));
    drop(s);
    assert_eq!(rx.wait_exit(EXIT), 0);
    let expected: std::collections::BTreeSet<std::ffi::OsString> = [
        "_CON.txt",
        "a",
        "a (1)",
        "invoicefdp.exe",
        "_.dropumi-x.part",
        "xy",
        "zero.txt",
    ]
    .iter()
    .map(std::ffi::OsString::from)
    .collect();
    assert_eq!(dir_names(&dst), expected);
    assert_eq!(
        dir_names(parent.path()),
        [std::ffi::OsString::from("inbox")].into()
    );
}

#[test]
fn wrong_file_hash_integrity_failed() {
    let b = Device::new("Bob");
    let raw = RawIdentity::new(0x44);
    b.trust_fp(&raw.fingerprint, "Raw");
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let id = TransferId::from_bytes([7; 16]);
    let (mut s, _) = RawSession::connect(loopback(port), &raw, Intent::Transfer).unwrap();
    offer(&mut s, id, vec![entry("bad.bin", 4), entry("good.bin", 4)]);
    assert!(matches!(s.recv(), Some(Message::Accept { .. })));
    s.send(&Message::FileStart {
        index: 0,
        offset: 0,
    });
    s.send_data(b"evil");
    s.send(&Message::FileEnd {
        index: 0,
        hash: FileHash::Blake3([0; 32]),
    });
    assert!(matches!(
        s.recv(),
        Some(Message::FileAck {
            index: 0,
            status: dropumi_protocol::message::AckStatus::IntegrityFailed
        })
    ));
    s.send(&Message::FileStart {
        index: 1,
        offset: 0,
    });
    s.send_data(b"good");
    s.send(&Message::FileEnd {
        index: 1,
        hash: FileHash::Blake3(*blake3::hash(b"good").as_bytes()),
    });
    assert!(matches!(s.recv(), Some(Message::FileAck { index: 1, .. })));
    s.send(&Message::Complete { transfer_id: id });
    assert!(matches!(
        s.recv(),
        Some(Message::CompleteAck {
            stored_count: 1,
            ..
        })
    ));
    drop(s);
    assert_eq!(rx.wait_exit(EXIT), 8);
    assert_eq!(
        dir_names(dst.path()),
        [std::ffi::OsString::from("good.bin")].into()
    );
}

#[test]
fn mitm_relay_rejected_by_pinned_trust() {
    let (a, b) = trusted_pair("Alice", "Bob");
    // The sender believes it talks to Bob, but an impostor with its own key
    // answers: the sender must stop before revealing itself (no msg3).
    let (addr, h) = impostor_responder(RawIdentity::new(0x66));
    let src = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("secret.txt"), b"secret");
    let mut tx = Proc::spawn(
        "sender",
        a.config(),
        &args(&[
            "send",
            &f.display().to_string(),
            "--to",
            &addr.to_string(),
            "--expect-fingerprint",
            &b.fingerprint,
        ]),
    );
    assert_eq!(tx.wait_exit(EXIT), 6);
    assert_eq!(tx.wait_event("failed")["code"], "peer_identity_mismatch");
    assert!(!h.join().unwrap(), "the impostor never received msg3");
    // A TrustedOnly receiver refuses the impostor as an initiator.
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
    let impostor = RawIdentity::new(0x66);
    match RawSession::connect(loopback(port), &impostor, Intent::Transfer) {
        Err(_) => {}
        Ok((mut s, _)) => panic!("impostor accepted: {:?}", s.recv()),
    }
    rx.cmd("quit");
    rx.wait_exit(EXIT);
    assert!(dir_names(dst.path()).is_empty());
}

#[test]
fn ignorable_message_during_consent_is_skipped() {
    // While the receiving user decides, an ignorable message (type 200)
    // arrives. The receiver must skip it and still deliver the decision,
    // not end the session with a protocol violation.
    let b = Device::new("Bob");
    let raw = RawIdentity::new(0x44);
    b.trust_fp(&raw.fingerprint, "Raw");
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "prompt", "--once"]);
    let (mut s, _hello) = RawSession::connect(loopback(port), &raw, Intent::Transfer).unwrap();
    let id = TransferId::from_bytes([7; 16]);
    offer(&mut s, id, vec![entry("ok.txt", 2)]);
    // [200, {}]: queued right behind the offer, so the receiver reads it
    // while its consent prompt is open.
    s.send_control_bytes(&[0x82, 0x18, 0xC8, 0xA0]);
    let offered = rx.wait_event("offer_received");
    rx.cmd_for("accept", offered["transfer_id"].as_str().unwrap());
    match s.recv() {
        Some(Message::Accept { transfer_id }) => assert_eq!(transfer_id, id),
        other => panic!("expected Accept, got {other:?}"),
    }
    drop(s);
    rx.wait_exit(EXIT);
    assert!(
        rx.seen.iter().all(|e| e["code"] != "protocol_violation"),
        "{:?}",
        rx.seen
    );
}

#[test]
fn peer_error_before_offer_reports_its_code() {
    // A peer that answers Hello with Error BUSY ends the session with code
    // "busy", not "cancelled".
    let b = Device::new("Bob");
    let raw = RawIdentity::new(0x45);
    b.trust_fp(&raw.fingerprint, "Raw");
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
    let (mut s, _hello) = RawSession::connect(loopback(port), &raw, Intent::Transfer).unwrap();
    s.send(&Message::error(ErrorCode::Busy));
    let _ = s.until_closed();
    let closed = rx.wait_event("session_closed");
    assert_eq!(closed["code"], "busy", "{closed}");
    rx.cmd("quit");
    rx.wait_exit(EXIT);
}
