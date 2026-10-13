// SPDX-License-Identifier: MPL-2.0
//! Timeouts, cancellation, stalls and cleanup.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dropumi_application::events::{AppEvent, TransferOutcome};
use dropumi_application::usecases::connect::dial;
use dropumi_domain::connection::ConnectionCandidate;
use dropumi_domain::error::{Phase, TransferError};
use dropumi_domain::policy::AcceptMode;
use dropumi_domain::transfer::{CancelReason, Party, RejectReason};
use support::*;
use tokio_util::sync::CancellationToken;

fn bytes(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

#[tokio::test(start_paused = true)]
async fn consent_timeout_sends_reject_timeout() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.policy.accept = AcceptMode::Prompt;
    setup.interaction = Scripted::with(None, None);
    let rx = setup.start(&net, addr(9400));
    let source = MemSource::default();
    let paths = vec![source.add("/c.txt", b"c".to_vec())];
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, _) = tx
        .send(&net, to(9400), paths, false, None, CancellationToken::new())
        .await;
    assert!(
        matches!(
            out.as_ref().unwrap(),
            TransferOutcome::Rejected {
                reason: RejectReason::Timeout,
                ..
            }
        ),
        "{out:?}"
    );
    let _ = finish(rx).await;
}

#[tokio::test(start_paused = true)]
async fn idle_timeout_fails_session() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let rx = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]))
        .start(&net, addr(9401));
    let source = MemSource::default();
    let p = source.add("/stall.bin", bytes(200_000));
    source.stall_after(&p, 70_000);
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let send = tokio::spawn(async move {
        tx.send(
            &net,
            to(9401),
            vec![p],
            false,
            None,
            CancellationToken::new(),
        )
        .await
    });
    let (summary, _, sink) = finish(rx).await;
    let summary = format!("{:?}", summary.unwrap());
    assert!(summary.contains("Timeout(Idle)"), "{summary}");
    let (begun, committed, discarded) = sink.counts();
    assert_eq!(
        (begun, committed, discarded),
        (1, 0, 1),
        "partial file discarded"
    );
    send.abort();
}

#[tokio::test]
async fn sender_cancel_mid_file_discards_partial_keeps_acked() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let rx = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]))
        .start(&net, addr(9402));
    let source = MemSource::default();
    let first = source.add("/first.txt", b"first".to_vec());
    let second = source.add("/second.bin", bytes(500_000));
    source.stall_after(&second, 100_000);
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    let send = tokio::spawn(async move {
        tx.send(&net, to(9402), vec![first, second], false, None, c2)
            .await
    });
    // Cancel once the first file is stored and the second has reached its
    // stall point (all of "first" plus 100,000 bytes of "second" written).
    tokio::time::timeout(Duration::from_secs(5), rx.sink.wait_written(5 + 100_000))
        .await
        .unwrap();
    assert_eq!(
        rx.sink.counts().1,
        1,
        "first file committed before the second"
    );
    cancel.cancel();
    // The sender races its (stalled) read against the cancel token, so it
    // must stop promptly; a hang fails here instead of in the receiver.
    let (out, _) = tokio::time::timeout(Duration::from_secs(5), send)
        .await
        .expect("the sender ignored the cancel")
        .unwrap();
    assert!(
        matches!(
            out.as_ref().unwrap(),
            TransferOutcome::Cancelled {
                by: Party::Local,
                reason: CancelReason::User,
                ..
            }
        ),
        "{out:?}"
    );
    let (summary, _, sink) = finish(rx).await;
    let summary = format!("{:?}", summary.unwrap());
    // The receiver got Cancel USER from the peer.
    assert!(
        summary.contains("Cancelled") && summary.contains("Peer") && summary.contains("User"),
        "{summary}"
    );
    let stored = sink.stored();
    assert_eq!(
        stored.get("first.txt").map(Vec::as_slice),
        Some(&b"first"[..])
    );
    assert!(!stored.contains_key("second.bin"));
    // Two files begun, the first stored, the partial second discarded.
    assert_eq!(sink.counts(), (2, 1, 1));
}

#[tokio::test]
async fn receiver_cancel_mid_file() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let rx = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]))
        .start(&net, addr(9403));
    let source = MemSource::default();
    let p = source.add("/big.bin", bytes(300_000));
    source.stall_after(&p, 100_000);
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let send = tokio::spawn(async move {
        tx.send(
            &net,
            to(9403),
            vec![p],
            false,
            None,
            CancellationToken::new(),
        )
        .await
    });
    // Cancel mid-file: once the stalled file's first 100,000 bytes arrived.
    tokio::time::timeout(Duration::from_secs(5), rx.sink.wait_written(100_000))
        .await
        .unwrap();
    rx.cancel.cancel();
    let (_, _, sink) = finish(rx).await;
    let (out, _) = tokio::time::timeout(Duration::from_secs(5), send)
        .await
        .unwrap()
        .unwrap();
    let out = out.unwrap();
    assert!(
        matches!(
            out,
            TransferOutcome::Cancelled {
                by: Party::Peer,
                // Stopping the receiver cancels with SHUTDOWN.
                reason: CancelReason::Shutdown,
                ..
            } | TransferOutcome::Failed {
                error: TransferError::ConnectionLost,
                ..
            }
        ),
        "{out:?}"
    );
    let (begun, committed, discarded) = sink.counts();
    assert_eq!((begun, committed, discarded), (1, 0, 1));
}

#[tokio::test(start_paused = true)]
async fn stalled_receiver_times_out_sender() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.sink.block_writes = true;
    let rx = setup.start(&net, addr(9404));
    let source = MemSource::default();
    let p = source.add("/flood.bin", bytes(2_000_000));
    let mut tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    tx.connector_buffer = 16 * 1024;
    let (out, _) = tx
        .send(
            &net,
            to(9404),
            vec![p],
            false,
            None,
            CancellationToken::new(),
        )
        .await;
    assert!(
        matches!(
            out.as_ref().unwrap(),
            TransferOutcome::Failed {
                error: TransferError::Timeout(Phase::Write),
                ..
            }
        ),
        "{out:?}"
    );
    rx.cancel.cancel();
    rx.task.abort();
}

#[tokio::test(start_paused = true)]
async fn complete_ack_deadline_restarts_on_each_file_ack() {
    // Every commit takes 25 s (a slow disk), so the last FileAck and the
    // CompleteAck arrive about 100 s after the data, beyond the 60 s
    // CompleteAck deadline. The deadline restarts with each FileAck, so the
    // transfer completes instead of timing out.
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.sink.commit_delay = Some(Duration::from_secs(25));
    let rx = setup.start(&net, addr(9410));
    let source = MemSource::default();
    let paths: Vec<_> = (0..4)
        .map(|i| source.add(&format!("/slow-{i}.txt"), bytes(1_000)))
        .collect();
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let started = tokio::time::Instant::now();
    let (out, _) = tx
        .send(&net, to(9410), paths, false, None, CancellationToken::new())
        .await;
    assert!(
        matches!(
            out.as_ref().unwrap(),
            TransferOutcome::Completed {
                files_stored: 4,
                ..
            }
        ),
        "{out:?}"
    );
    assert!(started.elapsed() >= Duration::from_secs(100));
    let (_, _, sink) = finish(rx).await;
    assert_eq!(sink.counts(), (4, 4, 0));
}

#[tokio::test]
async fn storage_error_detail_not_leaked() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.sink.fail_write_after = Some(0);
    setup.sink.fail_message = "EACCES /home/someone/secret/path".into();
    let rx = setup.start(&net, addr(9405));
    let source = MemSource::default();
    let p = source.add("/x.bin", bytes(10));
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, events) = tx
        .send(
            &net,
            to(9405),
            vec![p],
            false,
            None,
            CancellationToken::new(),
        )
        .await;
    let rendered = format!("{out:?} {events:?}");
    assert!(!rendered.contains("secret/path"), "{rendered}");
    assert!(rendered.contains("Storage"));
    let _ = finish(rx).await;
}

#[tokio::test]
async fn discard_called_on_every_failure_path() {
    // Peer disappears mid-file (EOF): the receiver discards the partial file.
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let rx = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]))
        .start(&net, addr(9406));
    let source = MemSource::default();
    let p = source.add("/eof.bin", bytes(300_000));
    source.stall_after(&p, 100_000);
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let send = tokio::spawn(async move {
        tx.send(
            &net,
            to(9406),
            vec![p],
            false,
            None,
            CancellationToken::new(),
        )
        .await
    });
    // Kill the sender mid-file: once the first 100,000 bytes arrived.
    tokio::time::timeout(Duration::from_secs(5), rx.sink.wait_written(100_000))
        .await
        .unwrap();
    send.abort();
    let (summary, events, sink) = finish(rx).await;
    assert!(format!("{:?}", summary.unwrap()).contains("ConnectionLost"));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AppEvent::Finished(TransferOutcome::Failed { .. })))
    );
    let (begun, committed, discarded) = sink.counts();
    assert_eq!((begun, committed, discarded), (1, 0, 1));
}

#[tokio::test]
async fn connect_tries_candidates_in_rank_order() {
    let net = Net::new();
    let _listener = net.listen(addr(9408));
    let (sink, mut erx) = events();
    let candidates = vec![
        ConnectionCandidate::manual(addr(9407)),
        ConnectionCandidate::manual(addr(9408)),
    ];
    let (_stream, at) = dial(&net.connector(), &candidates, Duration::from_secs(1), &sink)
        .await
        .unwrap();
    assert_eq!(at, addr(9408));
    let order: Vec<_> = drain(&mut erx)
        .into_iter()
        .filter_map(|e| match e {
            AppEvent::Connecting { addr } => Some(addr),
            _ => None,
        })
        .collect();
    assert_eq!(order, vec![addr(9407), addr(9408)]);
    let err = dial(
        &net.connector(),
        &[ConnectionCandidate::manual(addr(9499))],
        Duration::from_secs(1),
        &sink,
    )
    .await
    .err()
    .unwrap();
    assert_eq!(err, TransferError::ConnectionLost);
}

#[tokio::test(start_paused = true)]
async fn offer_deadline_is_absolute() {
    use dropumi_application::channel::{InitiateParams, initiate};
    use dropumi_application::config::Timeouts;
    use dropumi_application::ports::Connector;
    use dropumi_domain::connection::PeerExpectation;
    use dropumi_domain::peer::Intent;
    use dropumi_domain::transfer::{FileEntry, TransferId};
    use dropumi_protocol::message::Message;

    // A trusted peer trickles an offer one batch every 20 s. The 30 s offer
    // limit covers the whole offer, so the receiver gives up at 30 s.
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.once = false;
    let rx = setup.start(&net, addr(9410));
    let stream = net
        .connector()
        .connect(addr(9410), Duration::from_secs(1))
        .await
        .unwrap();
    let t = Timeouts::default();
    let exp = PeerExpectation::default();
    let trust = MemTrust::with(&[(&bob, "Bob")]);
    let mut s = initiate(
        stream,
        &alice,
        InitiateParams {
            intent: Intent::Transfer,
            expectation: &exp,
            allow_unverified: false,
            timeouts: &t,
        },
        &trust,
    )
    .await
    .unwrap();
    let id = TransferId::from_bytes([1; 16]);
    s.writer
        .send(&Message::Offer {
            transfer_id: id,
            file_count: 3,
            total_size: 3,
        })
        .await
        .unwrap();
    for i in 0..2u32 {
        tokio::time::sleep(Duration::from_secs(20)).await;
        let _ = s
            .writer
            .send(&Message::OfferFiles {
                transfer_id: id,
                first_index: i,
                files: vec![FileEntry {
                    name: format!("f{i}"),
                    size: 1,
                    media_type: None,
                    modified: None,
                }],
            })
            .await;
    }
    assert!(
        s.reader.recv(Phase::Idle).await.is_err(),
        "receiver closed the session"
    );
    rx.cancel.cancel();
    let (_, events, _) = finish(rx).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            AppEvent::SessionClosed {
                code: "timeout",
                ..
            }
        )),
        "{events:?}"
    );
}

/// Records the fields of every `tracing` event on this thread.
struct CaptureLogs(Arc<Mutex<Vec<String>>>);

impl tracing::Subscriber for CaptureLogs {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        struct Fields(String);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
                self.0.push_str(&format!(" {}={v:?}", f.name()));
            }
        }
        let mut fields = Fields(String::new());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

#[tokio::test]
async fn trust_store_errors_are_logged_without_paths() {
    // The current-thread test runtime runs every task on this thread, so a
    // thread-local subscriber sees the receiver's logs.
    let logs = Arc::new(Mutex::new(Vec::new()));
    let _guard = tracing::subscriber::set_default(CaptureLogs(Arc::clone(&logs)));
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let trust = MemTrust::with(&[(&alice, "Alice")]);
    trust
        .fail_get
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut setup = ReceiverSetup::new(bob.clone(), trust);
    setup.once = false;
    let rx = setup.start(&net, addr(9409));
    let source = MemSource::default();
    let paths = vec![source.add("/x", b"x".to_vec())];
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, _) = tx
        .send(&net, to(9409), paths, false, None, CancellationToken::new())
        .await;
    assert!(
        matches!(
            out.unwrap(),
            TransferOutcome::Failed {
                error: TransferError::NotAuthorized,
                ..
            }
        ),
        "an unreadable trust store denies the session"
    );
    rx.cancel.cancel();
    let _ = finish(rx).await;
    let logs = logs.lock().unwrap().join("\n");
    assert!(logs.contains("trust store unavailable"), "{logs}");
    assert!(logs.contains("insecure permissions"), "{logs}");
    assert!(!logs.contains(BROKEN_TRUST_PATH), "{logs}");
    assert!(!logs.contains("someone"), "{logs}");
}

#[tokio::test]
async fn last_seen_failures_are_logged_without_paths() {
    // Both sides record last_seen after a trusted transfer; when that fails
    // the transfer still succeeds and the debug log carries no path.
    let logs = Arc::new(Mutex::new(Vec::new()));
    let _guard = tracing::subscriber::set_default(CaptureLogs(Arc::clone(&logs)));
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let rx_trust = MemTrust::with(&[(&alice, "Alice")]);
    rx_trust
        .fail_touch
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let rx = ReceiverSetup::new(bob.clone(), rx_trust).start(&net, addr(9411));
    let source = MemSource::default();
    let paths = vec![source.add("/x", b"x".to_vec())];
    let tx_trust = MemTrust::with(&[(&bob, "Bob")]);
    tx_trust
        .fail_touch
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let tx = SenderSetup::new(alice, tx_trust, source);
    let (out, _) = tx
        .send(&net, to(9411), paths, false, None, CancellationToken::new())
        .await;
    assert!(
        matches!(out.as_ref().unwrap(), TransferOutcome::Completed { .. }),
        "{out:?}"
    );
    let (summary, _, _) = finish(rx).await;
    assert!(format!("{:?}", summary.unwrap()).contains("Completed"));
    let logs = logs.lock().unwrap();
    let last_seen: Vec<_> = logs
        .iter()
        .filter(|l| l.contains("could not record last_seen"))
        .collect();
    assert_eq!(last_seen.len(), 2, "sender and receiver: {logs:?}");
    assert!(
        last_seen.iter().all(|l| l.contains("insecure permissions")),
        "{logs:?}"
    );
    assert!(
        !logs.iter().any(|l| l.contains(BROKEN_TRUST_PATH)),
        "{logs:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn complete_ack_deadline_still_fires() {
    // A single commit that takes longer than the CompleteAck deadline (60 s)
    // with no acknowledgement in between: the sender gives up at 60 s.
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.sink.commit_delay = Some(Duration::from_secs(90));
    let rx = setup.start(&net, addr(9412));
    let source = MemSource::default();
    let paths = vec![source.add("/stuck.bin", bytes(1_000))];
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let started = tokio::time::Instant::now();
    let (out, _) = tx
        .send(&net, to(9412), paths, false, None, CancellationToken::new())
        .await;
    assert!(
        matches!(
            out.as_ref().unwrap(),
            TransferOutcome::Failed {
                error: TransferError::Timeout(Phase::CompleteAck),
                ..
            }
        ),
        "{out:?}"
    );
    // 60 s deadline, then the 2 s drain after sending Error TIMEOUT (the
    // receiver's commit would only finish at 90 s).
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_secs(60) && waited <= Duration::from_secs(63),
        "{waited:?}"
    );
    rx.cancel.cancel();
    let _ = finish(rx).await;
}
