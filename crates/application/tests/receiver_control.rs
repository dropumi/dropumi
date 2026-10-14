// SPDX-License-Identifier: MPL-2.0
//! Controlling a running receiver: cancelling one transfer by id and
//! rejecting a second offer that reuses an active transfer id.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use dropumi_application::channel::{EstablishedSession, InitiateParams, initiate};
use dropumi_application::config::Timeouts;
use dropumi_application::events::{AppEvent, TransferOutcome};
use dropumi_application::identity::LocalIdentity;
use dropumi_application::ports::Connector;
use dropumi_application::usecases::UseCaseError;
use dropumi_application::usecases::receive::TransferCanceller;
use dropumi_domain::connection::PeerExpectation;
use dropumi_domain::error::{Phase, TransferError};
use dropumi_domain::identity::Fingerprint;
use dropumi_domain::peer::Intent;
use dropumi_domain::policy::AcceptMode;
use dropumi_domain::transfer::{
    CancelReason, FileEntry, Party, RejectReason, TransferId, TransferRequest,
};
use dropumi_protocol::frame::Frame;
use dropumi_protocol::session::{SenderEvent, SenderMachine};
use support::*;
use tokio::io::DuplexStream;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const WAIT: Duration = Duration::from_secs(5);

fn bytes(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

type SendResult = (Result<TransferOutcome, UseCaseError>, Vec<AppEvent>);

/// Starts a trusted send of one 300,000-byte file that stalls after its
/// first 100,000 bytes, so the transfer stays active until cancelled.
fn spawn_stalled_send(
    net: &Arc<Net>,
    port: u16,
    me: &LocalIdentity,
    receiver: &LocalIdentity,
    name: &str,
) -> JoinHandle<SendResult> {
    let source = MemSource::default();
    let p: PathBuf = source.add(name, bytes(300_000));
    source.stall_after(&p, 100_000);
    let tx = SenderSetup::new(me.clone(), MemTrust::with(&[(receiver, "Bob")]), source);
    let net = Arc::clone(net);
    tokio::spawn(async move {
        tx.send(
            &net,
            to(port),
            vec![p],
            false,
            None,
            CancellationToken::new(),
        )
        .await
    })
}

/// Waits for the next `OfferReceived` event.
async fn next_offer(h: &mut ReceiverHandle) -> (Fingerprint, TransferId) {
    loop {
        let event = tokio::time::timeout(WAIT, h.events.recv())
            .await
            .expect("offer within the deadline")
            .expect("receiver still running");
        if let AppEvent::OfferReceived {
            transfer_id, peer, ..
        } = event
        {
            return (peer.fingerprint, transfer_id);
        }
    }
}

async fn outcome(send: JoinHandle<SendResult>) -> TransferOutcome {
    let (out, _) = tokio::time::timeout(WAIT, send).await.unwrap().unwrap();
    out.unwrap()
}

#[tokio::test]
async fn cancel_by_id_cancels_only_that_transfer() {
    let net = Net::new();
    let (alice, bob, carol) = (
        identity(1, "Alice"),
        identity(2, "Bob"),
        identity(3, "Carol"),
    );
    let mut setup = ReceiverSetup::new(
        bob.clone(),
        MemTrust::with(&[(&alice, "Alice"), (&carol, "Carol")]),
    );
    setup.once = false;
    let mut rx = setup.start(&net, addr(9600));
    let a_send = spawn_stalled_send(&net, 9600, &alice, &bob, "/a.bin");
    let c_send = spawn_stalled_send(&net, 9600, &carol, &bob, "/c.bin");
    let first = next_offer(&mut rx).await;
    let second = next_offer(&mut rx).await;
    let alice_id = if first.0 == *alice.fingerprint() {
        first.1
    } else {
        second.1
    };
    // Both transfers are mid-file.
    tokio::time::timeout(WAIT, rx.sink.wait_written(200_000))
        .await
        .unwrap();

    assert!(rx.canceller.cancel(&alice_id));
    let a = outcome(a_send).await;
    assert!(
        matches!(
            a,
            TransferOutcome::Cancelled {
                by: Party::Peer,
                reason: CancelReason::User,
                ..
            }
        ),
        "{a:?}"
    );
    assert!(!c_send.is_finished(), "the other transfer keeps running");

    rx.cancel.cancel();
    let c = outcome(c_send).await;
    assert!(
        !matches!(
            c,
            TransferOutcome::Cancelled {
                reason: CancelReason::User,
                ..
            }
        ),
        "only the named transfer was cancelled by the user: {c:?}"
    );
    let (_, _, sink) = finish(rx).await;
    let (begun, committed, discarded) = sink.counts();
    assert_eq!((begun, committed, discarded), (2, 0, 2));
}

#[tokio::test]
async fn cancel_by_id_unknown_returns_false() {
    let canceller = TransferCanceller::default();
    assert!(!canceller.cancel(&TransferId::from_bytes([9; 16])));
}

#[tokio::test]
async fn cancel_by_id_after_transfer_ends_returns_false() {
    let net = Net::new();
    let (alice, bob) = (identity(1, "Alice"), identity(2, "Bob"));
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.once = false;
    let mut rx = setup.start(&net, addr(9601));
    let source = MemSource::default();
    let p = source.add("/small.txt", b"hello".to_vec());
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, _) = tx
        .send(
            &net,
            to(9601),
            vec![p],
            false,
            None,
            CancellationToken::new(),
        )
        .await;
    let id = match out.unwrap() {
        TransferOutcome::Completed { transfer_id, .. } => transfer_id,
        other => panic!("{other:?}"),
    };
    // Wait for the receiver's own Finished event: its registration is gone.
    loop {
        let e = tokio::time::timeout(WAIT, rx.events.recv())
            .await
            .unwrap()
            .unwrap();
        if matches!(e, AppEvent::Finished(_)) {
            break;
        }
    }
    assert!(!rx.canceller.cancel(&id));
    rx.cancel.cancel();
    let _ = finish(rx).await;
}

#[tokio::test]
async fn cancel_active_still_cancels_all() {
    let net = Net::new();
    let (alice, bob, carol) = (
        identity(1, "Alice"),
        identity(2, "Bob"),
        identity(3, "Carol"),
    );
    let mut setup = ReceiverSetup::new(
        bob.clone(),
        MemTrust::with(&[(&alice, "Alice"), (&carol, "Carol")]),
    );
    setup.once = false;
    let mut rx = setup.start(&net, addr(9602));
    let a_send = spawn_stalled_send(&net, 9602, &alice, &bob, "/a.bin");
    let c_send = spawn_stalled_send(&net, 9602, &carol, &bob, "/c.bin");
    let _ = next_offer(&mut rx).await;
    let _ = next_offer(&mut rx).await;
    tokio::time::timeout(WAIT, rx.sink.wait_written(200_000))
        .await
        .unwrap();

    rx.canceller.cancel_active();
    for send in [a_send, c_send] {
        let o = outcome(send).await;
        assert!(
            matches!(
                o,
                TransferOutcome::Cancelled {
                    by: Party::Peer,
                    reason: CancelReason::User,
                    ..
                }
            ),
            "{o:?}"
        );
    }
    rx.cancel.cancel();
    let _ = finish(rx).await;
}

/// An initiator that sends a one-file offer with a chosen transfer id.
struct RawSender {
    session: EstablishedSession<DuplexStream>,
    machine: SenderMachine,
}

async fn raw_offer(
    net: &Arc<Net>,
    port: u16,
    me: &LocalIdentity,
    receiver: &LocalIdentity,
    id: TransferId,
) -> RawSender {
    let stream = net
        .connector()
        .connect(addr(port), Duration::from_secs(1))
        .await
        .unwrap();
    let trust = MemTrust::with(&[(receiver, "Bob")]);
    let timeouts = Timeouts::default();
    let expectation = PeerExpectation::default();
    let mut session = initiate(
        stream,
        me,
        InitiateParams {
            intent: Intent::Transfer,
            expectation: &expectation,
            allow_unverified: false,
            timeouts: &timeouts,
        },
        &trust,
    )
    .await
    .unwrap();
    let request = TransferRequest::new(
        id,
        vec![FileEntry {
            name: "a.txt".into(),
            size: 5,
            media_type: None,
            modified: None,
        }],
    )
    .unwrap();
    let mut machine = SenderMachine::new(request);
    for m in machine.offer().unwrap() {
        session.writer.send(&m).await.unwrap();
    }
    RawSender { session, machine }
}

/// The receiver's answer to a raw offer.
async fn answer(raw: &mut RawSender) -> SenderEvent {
    loop {
        let frame = tokio::time::timeout(WAIT, raw.session.reader.recv(Phase::Consent))
            .await
            .unwrap()
            .unwrap();
        let Frame::Control(body) = frame else {
            panic!("data before a decision");
        };
        match raw.machine.on_control(body).unwrap() {
            SenderEvent::Ignored => {}
            e => return e,
        }
    }
}

/// A receiver whose user never answers, so offers stay active.
fn waiting_receiver(
    net: &Arc<Net>,
    port: u16,
    me: &LocalIdentity,
    trusted: &[&LocalIdentity],
) -> ReceiverHandle {
    let trust = MemTrust::default();
    for id in trusted {
        trust.add(id, "peer");
    }
    let mut setup = ReceiverSetup::new(me.clone(), trust);
    setup.once = false;
    setup.policy.accept = AcceptMode::Prompt;
    setup.interaction = Scripted::with(None, None);
    setup.start(net, addr(port))
}

#[tokio::test]
async fn duplicate_active_transfer_id_rejected_busy() {
    let net = Net::new();
    let (alice, bob, carol) = (
        identity(1, "Alice"),
        identity(2, "Bob"),
        identity(3, "Carol"),
    );
    let mut rx = waiting_receiver(&net, 9603, &bob, &[&alice, &carol]);
    let id = TransferId::from_bytes([7; 16]);

    let first = raw_offer(&net, 9603, &alice, &bob, id).await;
    let (from, offered) = next_offer(&mut rx).await;
    assert_eq!((from, offered), (*alice.fingerprint(), id));

    let mut second = raw_offer(&net, 9603, &carol, &bob, id).await;
    assert!(matches!(
        answer(&mut second).await,
        SenderEvent::Rejected(RejectReason::Busy)
    ));
    // Closing lets the receiver's drain end at once.
    drop(second);
    // The duplicate never reached the user and was reported as busy.
    assert_eq!(rx.interaction.offers_seen.load(Ordering::SeqCst), 1);
    let closed = loop {
        let e = tokio::time::timeout(WAIT, rx.events.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            !matches!(e, AppEvent::OfferReceived { .. }),
            "no second offer event: {e:?}"
        );
        if let AppEvent::SessionClosed { peer, code } = e {
            break (peer.map(|p| p.fingerprint), code);
        }
    };
    assert_eq!(
        closed,
        (Some(*carol.fingerprint()), TransferError::Busy.code())
    );
    drop(first);
    rx.cancel.cancel();
    let _ = finish(rx).await;
}

#[tokio::test]
async fn concurrent_duplicate_offers_admit_one() {
    let net = Net::new();
    let (alice, bob, carol) = (
        identity(1, "Alice"),
        identity(2, "Bob"),
        identity(3, "Carol"),
    );
    let mut rx = waiting_receiver(&net, 9604, &bob, &[&alice, &carol]);
    let id = TransferId::from_bytes([8; 16]);

    let (mut a, mut c) = tokio::join!(
        raw_offer(&net, 9604, &alice, &bob, id),
        raw_offer(&net, 9604, &carol, &bob, id),
    );
    let _ = next_offer(&mut rx).await;
    // Exactly one offer reaches the user; the other one is rejected BUSY.
    let busy = tokio::select! {
        e = answer(&mut a) => e,
        e = answer(&mut c) => e,
    };
    assert!(matches!(busy, SenderEvent::Rejected(RejectReason::Busy)));
    assert_eq!(rx.interaction.offers_seen.load(Ordering::SeqCst), 1);
    drop((a, c));
    rx.cancel.cancel();
    let _ = finish(rx).await;
}

#[tokio::test(start_paused = true)]
async fn receiver_stops_after_repeated_accept_failures() {
    let net = Net::new();
    let mut setup = ReceiverSetup::new(identity(2, "Bob"), MemTrust::default());
    setup.once = false;
    // An invalidated listening socket: every accept fails at once.
    setup.listener_failures = usize::MAX;
    let rx = setup.start(&net, addr(9605));
    let attempts = Arc::clone(&rx.accept_attempts);
    let (summary, _, _) = tokio::time::timeout(Duration::from_secs(60), finish(rx))
        .await
        .expect("the receiver gives up instead of spinning");
    assert!(
        matches!(summary, Err(UseCaseError::Transfer(_))),
        "{summary:?}"
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 8);
}

#[tokio::test(start_paused = true)]
async fn accept_failures_back_off() {
    let net = Net::new();
    let mut setup = ReceiverSetup::new(identity(2, "Bob"), MemTrust::default());
    setup.once = false;
    setup.listener_failures = usize::MAX;
    let rx = setup.start(&net, addr(9606));
    let attempts = Arc::clone(&rx.accept_attempts);
    let started = tokio::time::Instant::now();
    let _ = finish(rx).await;
    // 10 + 20 + 40 + 80 + 160 + 320 + 640 ms between the eight attempts.
    assert_eq!(attempts.load(Ordering::SeqCst), 8);
    assert!(started.elapsed() >= Duration::from_millis(1_270));
}

#[tokio::test(start_paused = true)]
async fn single_accept_error_is_retried() {
    let net = Net::new();
    let (alice, bob) = (identity(1, "Alice"), identity(2, "Bob"));
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.listener_failures = 1;
    let rx = setup.start(&net, addr(9607));
    let source = MemSource::default();
    let p = source.add("/small.txt", b"hello".to_vec());
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, _) = tx
        .send(
            &net,
            to(9607),
            vec![p],
            false,
            None,
            CancellationToken::new(),
        )
        .await;
    assert!(matches!(out.unwrap(), TransferOutcome::Completed { .. }));
    let attempts = Arc::clone(&rx.accept_attempts);
    let (summary, _, _) = finish(rx).await;
    assert!(summary.is_ok());
    assert!(attempts.load(Ordering::SeqCst) >= 2);
}
