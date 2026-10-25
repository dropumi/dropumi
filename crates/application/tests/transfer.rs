// SPDX-License-Identifier: MPL-2.0
//! End-to-end transfers through the real channel over in-memory pipes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

mod support;

use dropumi_application::events::{AppEvent, TransferOutcome};
use dropumi_application::usecases::receive::ReceiveSummary;
use dropumi_domain::transfer::{CancelReason, Party};
use dropumi_protocol::message::AckStatus;
use support::*;
use tokio_util::sync::CancellationToken;

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

#[tokio::test]
async fn roundtrip_trusted_auto_accept() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let rx = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]))
        .start(&net, addr(9000));
    let source = MemSource::default();
    let big = pattern(300_000, 7);
    let paths = vec![
        source.add("/a/big.bin", big.clone()),
        source.add("/a/empty.txt", Vec::new()),
        source.add("/b/r\u{e9}sum\u{e9}.txt", b"hello".to_vec()),
    ];
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, events) = tx
        .send(&net, to(9000), paths, false, None, CancellationToken::new())
        .await;
    let out = out.unwrap();
    assert!(
        matches!(
            out,
            TransferOutcome::Completed {
                files_stored: 3,
                bytes: 300_005,
                ..
            }
        ),
        "{out:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEvent::OfferSent { files: 3, .. }))
    );
    // The session code is shown even when the receiver is trusted.
    assert!(events.iter().any(|e| matches!(e, AppEvent::Sas { .. })));
    let acks = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                AppEvent::FileSent {
                    status: AckStatus::Stored,
                    ..
                }
            )
        })
        .count();
    assert_eq!(acks, 3);
    let (summary, revents, sink) = finish(rx).await;
    assert!(matches!(
        summary.unwrap(),
        ReceiveSummary::Once(TransferOutcome::Completed { .. })
    ));
    let stored = sink.stored();
    assert_eq!(stored["big.bin"], big);
    assert_eq!(stored["empty.txt"], Vec::<u8>::new());
    assert_eq!(stored["r\u{e9}sum\u{e9}.txt"], b"hello");
    assert!(
        revents
            .iter()
            .any(|e| matches!(e, AppEvent::FileSaved { blake3, .. } if blake3.len() == 64))
    );
    let (_, _, discarded) = sink.counts();
    assert_eq!(discarded, 0);
}

#[tokio::test]
async fn duplicate_names_get_collision_suffixes() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let rx = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]))
        .start(&net, addr(9001));
    let source = MemSource::default();
    let paths = vec![
        source.add("/src1/dup.txt", b"one".to_vec()),
        source.add("/src2/dup.txt", b"two".to_vec()),
    ];
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, _) = tx
        .send(&net, to(9001), paths, false, None, CancellationToken::new())
        .await;
    assert!(out.unwrap().is_success());
    let (_, _, sink) = finish(rx).await;
    let stored = sink.stored();
    assert_eq!(stored["dup.txt"], b"one");
    assert_eq!(stored["dup (1).txt"], b"two");
}

#[tokio::test]
async fn write_failure_cancels_with_storage() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.sink.fail_write_after = Some(10);
    setup.sink.fail_message = "disk error at /secret/path".into();
    let rx = setup.start(&net, addr(9002));
    let source = MemSource::default();
    let paths = vec![source.add("/f.bin", pattern(100_000, 1))];
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, events) = tx
        .send(&net, to(9002), paths, false, None, CancellationToken::new())
        .await;
    let out = out.unwrap();
    assert!(
        matches!(
            out,
            TransferOutcome::Cancelled {
                by: Party::Peer,
                reason: CancelReason::Storage,
                ..
            }
        ),
        "{out:?}"
    );
    // Local detail never reaches the peer.
    assert!(!format!("{out:?}{events:?}").contains("/secret/path"));
    let (_, revents, sink) = finish(rx).await;
    assert!(
        revents
            .iter()
            .any(|e| matches!(e, AppEvent::Finished(TransferOutcome::Failed { .. })))
    );
    let (begun, committed, discarded) = sink.counts();
    assert_eq!(
        (begun, committed, discarded),
        (1, 0, 1),
        "partial file discarded"
    );
}

#[tokio::test]
async fn source_changed_sends_file_abort() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let rx = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]))
        .start(&net, addr(9003));
    let source = MemSource::default();
    let changed = source.add("/changing.log", pattern(5_000, 3));
    let stable = source.add("/stable.txt", b"stable".to_vec());
    source.mark_changed(&changed);
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, events) = tx
        .send(
            &net,
            to(9003),
            vec![changed, stable],
            false,
            None,
            CancellationToken::new(),
        )
        .await;
    let out = out.unwrap();
    assert!(
        matches!(
            out,
            TransferOutcome::CompletedWithFailures {
                files_stored: 1,
                failed: 1,
                ..
            }
        ),
        "{out:?}"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        AppEvent::FileSent {
            index: 0,
            status: AckStatus::Aborted,
            ..
        }
    )));
    let (_, _, sink) = finish(rx).await;
    let stored = sink.stored();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored["stable.txt"], b"stable");
    let (_, _, discarded) = sink.counts();
    assert_eq!(discarded, 1, "aborted partial file discarded");
}

#[tokio::test]
async fn progress_reaches_total_only_on_completion() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let rx = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]))
        .start(&net, addr(9004));
    let source = MemSource::default();
    let paths = vec![source.add("/p.bin", pattern(400_000, 9))];
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, events) = tx
        .send(&net, to(9004), paths, false, None, CancellationToken::new())
        .await;
    assert!(out.unwrap().is_success());
    let progress: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AppEvent::Progress { progress, .. } => Some(*progress),
            _ => None,
        })
        .collect();
    assert!(progress.len() >= 2);
    let last = progress.last().unwrap();
    assert_eq!(last.bytes_done(), 400_000);
    assert_eq!(last.bytes_total(), 400_000);
    assert!(
        progress
            .windows(2)
            .all(|w| w[0].bytes_done() <= w[1].bytes_done())
    );
    let _ = finish(rx).await;
}
