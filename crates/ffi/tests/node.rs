// SPDX-License-Identifier: MPL-2.0
//! Two real nodes over loopback (127.0.0.1, ephemeral ports): transfers,
//! codes, pairing, cancelling and the facade's security guards.
//!
//! Unix only: the facade ships for Apple platforms and Android.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

mod support;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use dropumi_ffi::{
    AcceptMode, CancelledBy, CodePurpose, Direction, DropumiError, ErrorCode, ForeignFile,
    NodeEvent, OperationResult, PairingResult, Prompt, PromptCloseReason, ReceiveOptions,
    ReceivePolicy, ReceiverStopReason, SendItem, SendOptions, SendTarget, TransferStatus,
};
use support::*;

fn receive(n: &TestNode, policy: ReceivePolicy, accept: AcceptMode) -> u16 {
    n.node
        .start_receiving(ReceiveOptions {
            policy,
            accept,
            port: Some(0),
            advertise: false,
        })
        .unwrap()
        .port
}

fn to(port: u16) -> SendTarget {
    SendTarget::Address {
        address: format!("127.0.0.1:{port}"),
        expect_fingerprint: None,
    }
}

fn path_item(p: &std::path::Path) -> SendItem {
    SendItem::Path {
        path: p.to_string_lossy().into_owned(),
    }
}

fn verified() -> SendOptions {
    SendOptions {
        allow_unverified: false,
    }
}

/// The result of the next finished transfer in `direction`.
fn finished(n: &mut TestNode, direction: Direction) -> dropumi_ffi::TransferResult {
    match n.events.wait(
        "a finished transfer",
        |e| matches!(e, NodeEvent::TransferFinished { direction: d, .. } if *d == direction),
    ) {
        NodeEvent::TransferFinished { result, .. } => result,
        _ => unreachable!(),
    }
}

fn pairing_finished(n: &mut TestNode) -> PairingResult {
    match n.events.wait("pairing to end", |e| {
        matches!(e, NodeEvent::PairingFinished { .. })
    }) {
        NodeEvent::PairingFinished { result, .. } => result,
        _ => unreachable!(),
    }
}

#[test]
fn trusted_send_auto_accept() {
    let (mut a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let data = pattern(700_000, 3);
    let file = write_file(&a.inbox, "photo one.jpg", &data);
    let op = a
        .node
        .send(to(port), vec![path_item(&file)], verified())
        .unwrap();

    let sent = finished(&mut a, Direction::Outgoing);
    assert_eq!(sent.status, TransferStatus::Completed, "{sent:?}");
    let got = finished(&mut b, Direction::Incoming);
    assert_eq!(got.status, TransferStatus::Completed);
    assert_eq!(got.bytes, 700_000);
    assert_eq!(sha256_file(&b.inbox.join("photo one.jpg")), sha256(&data));
    assert!(matches!(
        op.result(),
        Some(OperationResult::Transfer { result }) if result.status == TransferStatus::Completed
    ));
    // A trusted transfer asks nobody.
    assert!(
        !a.events
            .drain()
            .iter()
            .any(|e| matches!(e, NodeEvent::PromptOpened { .. }))
    );
    assert!(
        !b.events
            .drain()
            .iter()
            .any(|e| matches!(e, NodeEvent::PromptOpened { .. }))
    );
    a.node.shutdown();
    b.node.shutdown();
}

#[test]
fn everyone_offer_prompt_shows_code_and_creates_no_trust() {
    let (mut a, mut b) = (node(), node());
    let port = receive(
        &b,
        ReceivePolicy::Everyone { minutes: 10 },
        AcceptMode::AutoTrusted,
    );
    let file = write_file(&a.inbox, "notes.txt", b"hello from an unpaired device");
    a.node
        .send(
            to(port),
            vec![path_item(&file)],
            SendOptions {
                allow_unverified: true,
            },
        )
        .unwrap();

    // The sender compares the code first (unverified receiver).
    let Prompt::Code {
        id,
        code: sender_code,
        purpose,
        ..
    } = a.events.prompt()
    else {
        panic!("expected a code prompt");
    };
    assert_eq!(purpose, CodePurpose::UnverifiedReceiver);
    a.node.answer_sas(id, true).unwrap();
    // The receiver is asked, with the same code, even in AutoTrusted mode.
    let Prompt::Offer {
        id,
        code: Some(receiver_code),
        peer,
        files,
        ..
    } = b.events.prompt()
    else {
        panic!("expected an offer with a code");
    };
    assert!(!peer.trusted);
    assert_eq!(receiver_code, sender_code);
    assert_eq!(files[0].name, "notes.txt");
    b.node.answer_offer(id, true).unwrap();

    assert_eq!(
        finished(&mut a, Direction::Outgoing).status,
        TransferStatus::Completed
    );
    assert_eq!(
        finished(&mut b, Direction::Incoming).status,
        TransferStatus::Completed
    );
    assert!(a.node.trusted_devices().unwrap().is_empty());
    assert!(b.node.trusted_devices().unwrap().is_empty());
}

#[test]
fn unverified_sender_refused_in_trusted_only() {
    let (mut a, mut b) = (node(), node());
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::Prompt);
    let file = write_file(&a.inbox, "x.txt", b"x");
    a.node
        .send(
            to(port),
            vec![path_item(&file)],
            SendOptions {
                allow_unverified: true,
            },
        )
        .unwrap();
    // The receiver denies the session during the handshake, before any code
    // is shown or any offer is made.
    let r = finished(&mut a, Direction::Outgoing);
    assert_eq!(r.status, TransferStatus::Failed);
    assert_eq!(r.error, Some(ErrorCode::NotAuthorized));
    b.events.wait(
        "the denied session",
        |e| matches!(e, NodeEvent::SessionClosed { code, .. } if code == "not_authorized"),
    );
    assert!(
        !b.events
            .seen
            .iter()
            .any(|e| matches!(e, NodeEvent::PromptOpened { .. }))
    );
    assert!(stored(&b.inbox).is_empty());
}

#[test]
fn decline_rejects_and_stores_nothing() {
    let (mut a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::Prompt);
    let file = write_file(&a.inbox, "x.txt", b"x");
    a.node
        .send(to(port), vec![path_item(&file)], verified())
        .unwrap();
    let Prompt::Offer { id, code, .. } = b.events.prompt() else {
        panic!("expected an offer");
    };
    assert_eq!(code, None, "paired senders show no code");
    b.node.answer_offer(id, false).unwrap();
    let r = finished(&mut a, Direction::Outgoing);
    assert_eq!(r.status, TransferStatus::Rejected);
    assert!(stored(&b.inbox).is_empty());
}

#[test]
fn pairing_both_confirm_mutual_trust() {
    let (mut a, mut b) = (node(), node());
    let window = b.node.open_pairing_window(1).unwrap();
    let port = match b.events.wait("the window", |e| {
        matches!(e, NodeEvent::PairingWindowOpened { .. })
    }) {
        NodeEvent::PairingWindowOpened { port, operation } => {
            assert_eq!(operation, window.id());
            port
        }
        _ => unreachable!(),
    };
    a.node.pair_with(to(port)).unwrap();
    let (
        Prompt::Code {
            id: ia,
            code: ca,
            purpose,
            ..
        },
        Prompt::Code {
            id: ib, code: cb, ..
        },
    ) = (a.events.prompt(), b.events.prompt())
    else {
        panic!("expected code prompts");
    };
    assert_eq!(purpose, CodePurpose::Pairing);
    assert_eq!(ca, cb);
    a.node.answer_sas(ia, true).unwrap();
    b.node.answer_sas(ib, true).unwrap();
    assert!(matches!(
        pairing_finished(&mut a),
        PairingResult::Paired { .. }
    ));
    assert!(matches!(
        pairing_finished(&mut b),
        PairingResult::Paired { .. }
    ));
    let (ida, idb) = (a.node.identity().unwrap(), b.node.identity().unwrap());
    assert_eq!(
        a.node.trusted_devices().unwrap()[0].fingerprint,
        idb.fingerprint
    );
    assert_eq!(
        b.node.trusted_devices().unwrap()[0].fingerprint,
        ida.fingerprint
    );
}

#[test]
fn pairing_one_denies_stores_nothing() {
    let (mut a, mut b) = (node(), node());
    b.node.open_pairing_window(1).unwrap();
    let NodeEvent::PairingWindowOpened { port, .. } = b.events.wait("the window", |e| {
        matches!(e, NodeEvent::PairingWindowOpened { .. })
    }) else {
        unreachable!()
    };
    a.node.pair_with(to(port)).unwrap();
    let (Prompt::Code { id: ia, .. }, Prompt::Code { id: ib, .. }) =
        (a.events.prompt(), b.events.prompt())
    else {
        panic!("expected code prompts");
    };
    a.node.answer_sas(ia, true).unwrap();
    b.node.answer_sas(ib, false).unwrap();
    assert!(matches!(
        pairing_finished(&mut a),
        PairingResult::Failed { .. }
    ));
    assert!(matches!(
        pairing_finished(&mut b),
        PairingResult::Failed { .. }
    ));
    assert!(a.node.trusted_devices().unwrap().is_empty());
    assert!(b.node.trusted_devices().unwrap().is_empty());
}

/// The other device's "no" withdraws this device's code prompt at once:
/// nobody has to answer it, a late answer is refused, nothing is stored.
#[test]
fn pairing_peer_no_withdraws_the_code_prompt() {
    let (mut a, mut b) = (node(), node());
    b.node.open_pairing_window(1).unwrap();
    let NodeEvent::PairingWindowOpened { port, .. } = b.events.wait("the window", |e| {
        matches!(e, NodeEvent::PairingWindowOpened { .. })
    }) else {
        unreachable!()
    };
    a.node.pair_with(to(port)).unwrap();
    let (Prompt::Code { id: ia, .. }, Prompt::Code { id: ib, .. }) =
        (a.events.prompt(), b.events.prompt())
    else {
        panic!("expected code prompts");
    };
    a.node.answer_sas(ia, false).unwrap();
    let NodeEvent::PromptClosed { reason, .. } = b.events.wait(
        "b's prompt to close",
        |e| matches!(e, NodeEvent::PromptClosed { id, .. } if *id == ib),
    ) else {
        unreachable!()
    };
    assert_eq!(reason, PromptCloseReason::Withdrawn);
    assert!(matches!(
        pairing_finished(&mut b),
        PairingResult::Failed {
            error: ErrorCode::PairingRejected
        }
    ));
    assert!(matches!(
        pairing_finished(&mut a),
        PairingResult::Failed {
            error: ErrorCode::PairingRejected
        }
    ));
    assert!(matches!(
        b.node.answer_sas(ib, true),
        Err(DropumiError::PromptNotFound)
    ));
    assert!(a.node.trusted_devices().unwrap().is_empty());
    assert!(b.node.trusted_devices().unwrap().is_empty());
}

#[test]
fn pairing_window_suspends_and_resumes_receiver() {
    let (mut a, mut b) = (node(), node());
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let window = b.node.open_pairing_window(1).unwrap();
    b.events.wait("the receiver to pause", |e| {
        matches!(e, NodeEvent::ReceiverStopped { .. })
    });
    b.events.wait("the window", |e| {
        matches!(e, NodeEvent::PairingWindowOpened { .. })
    });
    window.cancel();
    assert!(matches!(pairing_finished(&mut b), PairingResult::Cancelled));
    let NodeEvent::ReceiverStarted { info } = b.events.wait("the receiver to resume", |e| {
        matches!(e, NodeEvent::ReceiverStarted { .. })
    }) else {
        unreachable!()
    };
    assert_eq!(info.port, port);
    // And it receives again.
    pair_manually(&a, &b);
    let file = write_file(&a.inbox, "after.txt", b"after the window");
    a.node
        .send(to(port), vec![path_item(&file)], verified())
        .unwrap();
    assert_eq!(
        finished(&mut a, Direction::Outgoing).status,
        TransferStatus::Completed
    );
}

/// A trusted transfer of a document held after 100,000 bytes.
fn held_transfer(a: &TestNode, port: u16) -> (Arc<dropumi_ffi::OperationHandle>, Arc<Gate>) {
    let mut doc = MemDoc::new("held.bin", pattern(400_000, 9));
    doc.hold_after = Some(100_000);
    doc.hold.close();
    let gate = Arc::clone(&doc.hold);
    let file: Arc<dyn ForeignFile> = Arc::new(doc);
    let op = a
        .node
        .send(to(port), vec![SendItem::Stream { file }], verified())
        .unwrap();
    (op, gate)
}

fn incoming_id(b: &mut TestNode) -> String {
    // Accepted: the transfer is running (its document is held mid-file).
    match b.events.wait("the receiver to accept", |e| {
        matches!(
            e,
            NodeEvent::TransferAccepted {
                operation: None,
                ..
            }
        )
    }) {
        NodeEvent::TransferAccepted { transfer_id, .. } => transfer_id,
        _ => unreachable!(),
    }
}

#[test]
fn sender_cancel_seen_by_both() {
    let (mut a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let (op, gate) = held_transfer(&a, port);
    incoming_id(&mut b);
    op.cancel();
    let r = finished(&mut a, Direction::Outgoing);
    assert_eq!(
        (r.status, r.cancelled_by),
        (TransferStatus::Cancelled, Some(CancelledBy::Local))
    );
    let r = finished(&mut b, Direction::Incoming);
    assert_eq!(
        (r.status, r.cancelled_by),
        (TransferStatus::Cancelled, Some(CancelledBy::Peer))
    );
    gate.open();
    assert!(stored(&b.inbox).is_empty(), "no partial file is kept");
}

#[test]
fn receiver_cancel_by_id() {
    let (mut a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let (_op, gate) = held_transfer(&a, port);
    let id = incoming_id(&mut b);
    assert!(b.node.cancel_incoming(id.clone()));
    let r = finished(&mut a, Direction::Outgoing);
    assert_eq!(
        (r.status, r.cancelled_by),
        (TransferStatus::Cancelled, Some(CancelledBy::Peer))
    );
    assert_eq!(
        finished(&mut b, Direction::Incoming).cancelled_by,
        Some(CancelledBy::Local)
    );
    assert!(!b.node.cancel_incoming(id), "no longer running");
    gate.open();
}

#[test]
fn stop_accepting_keeps_active_transfer() {
    let (mut a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let (_op, gate) = held_transfer(&a, port);
    incoming_id(&mut b);
    b.node.stop_accepting();
    assert!(b.node.snapshot().receiver_stopping);
    gate.open();
    assert_eq!(
        finished(&mut a, Direction::Outgoing).status,
        TransferStatus::Completed
    );
    let NodeEvent::ReceiverStopped { reason } = b.events.wait("the receiver to stop", |e| {
        matches!(e, NodeEvent::ReceiverStopped { .. })
    }) else {
        unreachable!()
    };
    assert_eq!(reason, ReceiverStopReason::Stopped);
    // New connections are refused now.
    let file = write_file(&a.inbox, "late.txt", b"late");
    a.node
        .send(to(port), vec![path_item(&file)], verified())
        .unwrap();
    let r = finished(&mut a, Direction::Outgoing);
    assert_eq!(r.status, TransferStatus::Failed);
}

#[test]
fn stream_items_are_sent_and_closed() {
    let (mut a, b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let data = pattern(1_000_000, 5);
    let doc = MemDoc::new("/content/document/r\u{e9}sum\u{e9}.pdf", data.clone());
    let closed = Arc::clone(&doc.closed);
    let file: Arc<dyn ForeignFile> = Arc::new(doc);
    a.node
        .send(to(port), vec![SendItem::Stream { file }], verified())
        .unwrap();
    assert_eq!(
        finished(&mut a, Direction::Outgoing).status,
        TransferStatus::Completed
    );
    // Only the last name component is offered.
    assert_eq!(
        sha256_file(&b.inbox.join("r\u{e9}sum\u{e9}.pdf")),
        sha256(&data)
    );
    assert!(closed.load(Ordering::SeqCst));
}

#[test]
fn foreign_size_mismatch_fails_the_file() {
    let (mut a, b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let mut doc = MemDoc::new("short.bin", pattern(1_000, 1));
    doc.announced = Some(5_000);
    let file: Arc<dyn ForeignFile> = Arc::new(doc);
    a.node
        .send(to(port), vec![SendItem::Stream { file }], verified())
        .unwrap();
    let r = finished(&mut a, Direction::Outgoing);
    assert_ne!(r.status, TransferStatus::Completed, "{r:?}");
    assert!(!stored(&b.inbox).iter().any(|n| n == "short.bin"));
}

#[test]
fn panicking_foreign_reader_contained() {
    let (mut a, b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let mut doc = MemDoc::new("boom.bin", pattern(10, 1));
    doc.panic_on_read = true;
    let file: Arc<dyn ForeignFile> = Arc::new(doc);
    a.node
        .send(to(port), vec![SendItem::Stream { file }], verified())
        .unwrap();
    let r = finished(&mut a, Direction::Outgoing);
    assert_ne!(r.status, TransferStatus::Completed);
    // The node still works.
    assert!(a.node.identity().is_ok());
}

#[test]
fn send_refuses_paths_inside_config_dir() {
    let (a, b) = (node(), node());
    pair_manually(&a, &b);
    let key = a.config.join("identity.key");
    assert!(key.exists());
    let err = a
        .node
        .send(to(9), vec![path_item(&key)], verified())
        .unwrap_err();
    assert!(matches!(err, DropumiError::InvalidInput { .. }), "{err:?}");
    // A symlink elsewhere that points at the key is refused too.
    let link = a.inbox.join("innocent.jpg");
    std::os::unix::fs::symlink(&key, &link).unwrap();
    let err = a
        .node
        .send(to(9), vec![path_item(&link)], verified())
        .unwrap_err();
    assert!(matches!(err, DropumiError::InvalidInput { .. }), "{err:?}");
    let relative = a.node.send(
        to(9),
        vec![SendItem::Path {
            path: "x.txt".into(),
        }],
        verified(),
    );
    assert!(matches!(relative, Err(DropumiError::InvalidInput { .. })));
}

#[test]
fn hard_link_to_identity_key_never_offered() {
    let (mut a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let link = a.inbox.join("holiday.jpg");
    std::fs::hard_link(a.config.join("identity.key"), &link).unwrap();
    a.node
        .send(to(port), vec![path_item(&link)], verified())
        .unwrap();
    let r = finished(&mut a, Direction::Outgoing);
    assert_eq!(r.status, TransferStatus::Failed);
    // Nothing reached the receiver, not even the offer.
    assert!(
        !b.events
            .drain()
            .iter()
            .any(|e| matches!(e, NodeEvent::OfferReceived { .. }))
    );
    assert!(
        !a.events
            .drain()
            .iter()
            .any(|e| matches!(e, NodeEvent::OfferSent { .. }))
    );
}

#[test]
fn events_never_contain_local_paths() {
    let (mut a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let file = write_file(&a.inbox, "doc.txt", b"text");
    a.node
        .send(to(port), vec![path_item(&file)], verified())
        .unwrap();
    finished(&mut a, Direction::Outgoing);
    finished(&mut b, Direction::Incoming);
    let rendered = format!("{:?} {:?}", a.events.drain(), b.events.drain());
    for dir in [&a.inbox, &b.inbox, &a.config, &b.config] {
        let d = dir.to_string_lossy();
        assert!(
            !rendered.contains(d.as_ref()),
            "event names {d}: {rendered}"
        );
    }
}

#[test]
fn identity_mutation_busy_while_receiving() {
    let b = node();
    receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::Prompt);
    assert!(matches!(
        b.node.set_device_name("New name".into()),
        Err(DropumiError::Busy { .. })
    ));
    assert!(matches!(
        b.node.reset_identity(),
        Err(DropumiError::Busy { .. })
    ));
    b.node.stop_receiving();
}

/// Regression: a configuration change saved without a confirmed directory
/// sync (`StoreError::NotDurable`) is applied. Each call returns the
/// applied state and publishes a warning (fixed text, no path) instead of
/// failing.
#[test]
fn change_saved_without_dir_sync_is_applied_with_warning() {
    use std::os::unix::fs::PermissionsExt;

    let mut n = node();
    let before = n.node.identity().unwrap();
    let fp = dropumi_domain::identity::Fingerprint::from_digest([7; 32]).to_full();
    let mode = |m| std::fs::set_permissions(&n.config, std::fs::Permissions::from_mode(m));
    // Write and search but no read: files can still be replaced, but the
    // directory cannot be opened for its fsync.
    mode(0o300).unwrap();
    if std::fs::read_dir(&n.config).is_ok() {
        // Skipped: directory permissions are bypassed (root).
        mode(0o700).unwrap();
        return;
    }
    let renamed = n.node.set_device_name("Desk".into());
    let trusted = n.node.trust_fingerprint(fp.clone(), Some("Laptop".into()));
    let alias_changed = n.node.rename_trusted(fp.clone(), "Studio".into());
    let listed = n.node.trusted_devices();
    let removed = n.node.untrust(fp.clone());
    let removed_again = n.node.untrust(fp.clone());
    let reset = n.node.reset_identity();
    mode(0o700).unwrap();

    assert_eq!(renamed.unwrap().name, "Desk");
    let trusted = trusted.unwrap();
    assert_eq!(
        (trusted.fingerprint.as_str(), trusted.alias.as_deref()),
        (fp.as_str(), Some("Laptop"))
    );
    assert!(alias_changed.unwrap());
    assert_eq!(listed.unwrap()[0].alias.as_deref(), Some("Studio"));
    assert!(removed.unwrap(), "the device was paired before the call");
    assert!(!removed_again.unwrap(), "the device was no longer paired");
    let reset = reset.unwrap();
    assert_ne!(reset.fingerprint, before.fingerprint);
    // The applied state is what the store holds.
    let now = n.node.identity().unwrap();
    assert_eq!(
        (now.name.as_str(), &now.fingerprint),
        ("Desk", &reset.fingerprint)
    );
    assert!(n.node.trusted_devices().unwrap().is_empty());

    // One warning per change, with a fixed message that names no path.
    let config = n.config.to_string_lossy().into_owned();
    for _ in 0..6 {
        match n
            .events
            .wait("a warning", |e| matches!(e, NodeEvent::Warning { .. }))
        {
            NodeEvent::Warning { message } => {
                assert!(message.contains("could not be synced"), "{message}");
                assert!(!message.contains(&config), "{message}");
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn answers_bind_to_prompt_ids() {
    let (a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::Prompt);
    let file = write_file(&a.inbox, "x.txt", b"x");
    a.node
        .send(to(port), vec![path_item(&file)], verified())
        .unwrap();
    let Prompt::Offer { id, .. } = b.events.prompt() else {
        panic!("expected an offer");
    };
    assert_eq!(b.node.pending_prompts().len(), 1);
    assert!(matches!(
        b.node.answer_sas(id, true),
        Err(DropumiError::PromptKindMismatch)
    ));
    assert!(matches!(
        b.node.answer_offer(id + 1000, true),
        Err(DropumiError::PromptNotFound)
    ));
    b.node.answer_offer(id, true).unwrap();
    assert!(matches!(
        b.node.answer_offer(id, true),
        Err(DropumiError::PromptNotFound)
    ));
}

#[test]
fn shutdown_idempotent_and_calls_after_fail() {
    let b = node();
    receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::Prompt);
    let started = Instant::now();
    b.node.shutdown();
    b.node.shutdown();
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(matches!(b.node.identity(), Err(DropumiError::ShutDown)));
    assert!(matches!(
        b.node.start_receiving(ReceiveOptions {
            policy: ReceivePolicy::TrustedOnly,
            accept: AcceptMode::Prompt,
            port: Some(0),
            advertise: false,
        }),
        Err(DropumiError::ShutDown)
    ));
}

#[test]
fn drop_without_shutdown_closes_the_listener() {
    let port = {
        let b = node();
        receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::Prompt)
    };
    // The runtime stops in the background; poll (20 ms, bounded) until the
    // port refuses connections.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            break;
        }
        assert!(Instant::now() < deadline, "listener still open after drop");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn listener_errors_contained() {
    let mut b = node_with(Setup {
        listener_failures: 3,
        ..Setup::default()
    });
    let a = node();
    pair_manually(&a, &b);
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::Prompt);
    let file = write_file(&a.inbox, "x.txt", b"x");
    a.node
        .send(to(port), vec![path_item(&file)], verified())
        .unwrap();
    // Deliveries failed and stopped, but the prompt is still reachable.
    let deadline = Instant::now() + Duration::from_secs(30);
    let id = loop {
        if let Some(p) = b.node.pending_prompts().first() {
            break p.id_of();
        }
        assert!(Instant::now() < deadline, "no prompt");
        std::thread::sleep(Duration::from_millis(20));
    };
    b.node.answer_offer(id, true).unwrap();
    assert!(
        b.events.drain().is_empty(),
        "delivery stopped after three failures"
    );
}

#[test]
fn slow_listener_does_not_stall_a_transfer() {
    let (mut a, b) = (node(), node());
    pair_manually(&a, &b);
    // The receiver's app never returns from its event callback.
    b.gate.close();
    let port = receive(&b, ReceivePolicy::TrustedOnly, AcceptMode::AutoTrusted);
    let data = pattern(2_000_000, 7);
    let file = write_file(&a.inbox, "big.bin", &data);
    a.node
        .send(to(port), vec![path_item(&file)], verified())
        .unwrap();
    assert_eq!(
        finished(&mut a, Direction::Outgoing).status,
        TransferStatus::Completed
    );
    assert_eq!(sha256_file(&b.inbox.join("big.bin")), sha256(&data));
    b.gate.open();
}

#[test]
fn listener_may_reenter_node() {
    // Answering from the event callback itself (the node's own thread).
    struct Answering {
        node: std::sync::Mutex<Option<Arc<dropumi_ffi::DropumiNode>>>,
        done: std::sync::mpsc::Sender<()>,
    }
    impl dropumi_ffi::EventListener for Answering {
        fn on_events(&self, events: Vec<NodeEvent>) -> Result<(), dropumi_ffi::CallbackError> {
            let node = self.node.lock().unwrap().clone();
            for e in events {
                if let (NodeEvent::PromptOpened { prompt }, Some(n)) = (&e, &node) {
                    n.answer_offer(prompt.id_of(), true).unwrap();
                    let _ = n.snapshot();
                }
                if matches!(e, NodeEvent::TransferFinished { .. }) {
                    let _ = self.done.send(());
                }
            }
            Ok(())
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let inbox = dir.path().join("inbox");
    std::fs::create_dir(&inbox).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let listener = Arc::new(Answering {
        node: std::sync::Mutex::new(None),
        done: tx,
    });
    let b = dropumi_ffi::DropumiNode::new(
        dropumi_ffi::NodeConfig {
            config_dir: dir.path().join("config").to_string_lossy().into_owned(),
            receive_dir: Some(inbox.to_string_lossy().into_owned()),
        },
        Arc::clone(&listener) as Arc<dyn dropumi_ffi::EventListener>,
        None,
    )
    .unwrap();
    *listener.node.lock().unwrap() = Some(Arc::clone(&b));
    let a = node();
    let (ia, ib) = (a.node.identity().unwrap(), b.identity().unwrap());
    a.node.trust_fingerprint(ib.fingerprint, None).unwrap();
    b.trust_fingerprint(ia.fingerprint, None).unwrap();
    let port = b
        .start_receiving(ReceiveOptions {
            policy: ReceivePolicy::TrustedOnly,
            accept: AcceptMode::Prompt,
            port: Some(0),
            advertise: false,
        })
        .unwrap()
        .port;
    let file = write_file(&a.inbox, "x.txt", b"x");
    a.node
        .send(to(port), vec![path_item(&file)], verified())
        .unwrap();
    rx.recv_timeout(WAIT).unwrap();
    *listener.node.lock().unwrap() = None;
    b.shutdown();
}

/// `Prompt::id` for tests (the generated bindings expose it per variant).
trait PromptId {
    fn id_of(&self) -> u64;
}

impl PromptId for Prompt {
    fn id_of(&self) -> u64 {
        match self {
            Prompt::Offer { id, .. } | Prompt::Code { id, .. } => *id,
        }
    }
}
