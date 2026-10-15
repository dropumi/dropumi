// SPDX-License-Identifier: MPL-2.0
//! Pairing: trust is stored only when both users confirm the same code.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

mod support;

use std::time::Duration;

use dropumi_application::channel::{EstablishedSession, InitiateParams, RecvError, initiate};
use dropumi_application::config::Timeouts;
use dropumi_application::ports::Connector;
use dropumi_application::usecases::connect::ResolveParams;
use dropumi_application::usecases::pair::{PairDeps, PairOutcome, pair_listen, pair_with};
use dropumi_domain::connection::PeerExpectation;
use dropumi_domain::error::{Phase, TransferError};
use dropumi_domain::peer::Intent;
use dropumi_domain::transfer::CancelReason;
use dropumi_protocol::frame::Frame;
use dropumi_protocol::message::{Decoded, ErrorCode, Message, Role, decode};
use support::*;
use tokio_util::sync::CancellationToken;

async fn run(
    initiator_confirms: bool,
    listener_confirms: bool,
) -> (
    PairOutcome,
    PairOutcome,
    MemTrust,
    MemTrust,
    Scripted,
    Scripted,
) {
    run_with(
        Scripted::with(None, Some(initiator_confirms)),
        Scripted::with(None, Some(listener_confirms)),
        CancellationToken::new(),
    )
    .await
}

/// Alice (`a_ui`, cancelled by `a_cancel`) pairs with Bob (`b_ui`).
async fn run_with(
    a_ui: Scripted,
    b_ui: Scripted,
    a_cancel: CancellationToken,
) -> (
    PairOutcome,
    PairOutcome,
    MemTrust,
    MemTrust,
    Scripted,
    Scripted,
) {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let listener = net.listen(addr(9300));
    let a_trust = MemTrust::default();
    let b_trust = MemTrust::default();
    let clock = ManualClock::at(5);
    let (ae, _arx) = events();
    let (be, _brx) = events();
    let a_deps = PairDeps {
        trust: &a_trust,
        interaction: &a_ui,
        clock: &clock,
        identity: &alice,
        timeouts: Default::default(),
        events: ae,
    };
    let b_deps = PairDeps {
        trust: &b_trust,
        interaction: &b_ui,
        clock: &clock,
        identity: &bob,
        timeouts: Default::default(),
        events: be,
    };
    let connector = net.connector();
    let targets = to(9300);
    let (a, b) = tokio::join!(
        pair_with::<_, StaticDiscovery, _, _, _>(
            &connector,
            None,
            &a_deps,
            ResolveParams {
                targets: &targets,
                explicit: None,
                discover_timeout: Duration::from_millis(1),
                allow_loopback: true,
            },
            a_cancel,
        ),
        pair_listen(
            listener,
            &b_deps,
            Duration::from_secs(60),
            CancellationToken::new()
        )
    );
    (a.unwrap(), b.unwrap(), a_trust, b_trust, a_ui, b_ui)
}

/// Bob listens with a code prompt nobody answers, while `peer` drives a raw
/// pairing session as Alice.
async fn against_raw_peer<F, Fut>(peer: F) -> (PairOutcome, MemTrust, tokio::time::Duration)
where
    F: FnOnce(EstablishedSession<tokio::io::DuplexStream>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    against_raw_peer_as(Scripted::with(None, None), peer).await
}

/// Bob (`b_ui`) listens while `peer` drives a raw pairing session as Alice.
async fn against_raw_peer_as<F, Fut>(
    b_ui: Scripted,
    peer: F,
) -> (PairOutcome, MemTrust, tokio::time::Duration)
where
    F: FnOnce(EstablishedSession<tokio::io::DuplexStream>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let listener = net.listen(addr(9303));
    let b_trust = MemTrust::default();
    let (be, _brx) = events();
    let b_deps = PairDeps {
        trust: &b_trust,
        interaction: &b_ui,
        clock: &ManualClock::at(5),
        identity: &bob,
        timeouts: Timeouts::default(),
        events: be,
    };
    let start = tokio::time::Instant::now();
    let raw = async {
        let stream = net
            .connector()
            .connect(addr(9303), Duration::from_secs(1))
            .await
            .unwrap();
        let session = initiate(
            stream,
            &alice,
            InitiateParams {
                intent: Intent::Pair,
                expectation: &PeerExpectation::default(),
                allow_unverified: true,
                timeouts: &Timeouts::default(),
            },
            &MemTrust::default(),
        )
        .await
        .unwrap();
        peer(session).await;
    };
    let (b, ()) = tokio::join!(
        pair_listen(
            listener,
            &b_deps,
            Duration::from_secs(60),
            CancellationToken::new()
        ),
        raw
    );
    (b.unwrap(), b_trust, start.elapsed())
}

/// The peer's "no" ends this side's code prompt at once, and both report
/// a rejection (the prompt is withdrawn; this side answers "no").
#[tokio::test(start_paused = true)]
async fn peer_no_ends_the_prompt_at_once() {
    let start = tokio::time::Instant::now();
    let (a, b, a_trust, b_trust, _, _) = run_with(
        Scripted::with(None, Some(false)),
        Scripted::with(None, None),
        CancellationToken::new(),
    )
    .await;
    assert_eq!(a, PairOutcome::Failed(TransferError::PairingRejected));
    assert_eq!(b, PairOutcome::Failed(TransferError::PairingRejected));
    assert!(
        start.elapsed() < Timeouts::default().sas,
        "{:?}",
        start.elapsed()
    );
    assert_eq!(a_trust.len(), 0);
    assert_eq!(b_trust.len(), 0);
}

/// Regression (Windows CI): the peer cancels during the code check. This
/// side used to wait for its own user, then write to a closed peer; on
/// Windows the reset discarded the peer's unread "no" and pairing ended as
/// "connection lost". Now the "no" is read while the prompt is open.
#[tokio::test(start_paused = true)]
async fn peer_cancel_during_the_prompt_ends_it_as_rejected() {
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        trigger.cancel();
    });
    let start = tokio::time::Instant::now();
    let (a, b, a_trust, b_trust, _, _) = run_with(
        Scripted::with(None, None),
        Scripted::with(None, None),
        cancel,
    )
    .await;
    assert_eq!(a, PairOutcome::Cancelled);
    assert_eq!(b, PairOutcome::Failed(TransferError::PairingRejected));
    assert!(
        start.elapsed() < Timeouts::default().sas,
        "{:?}",
        start.elapsed()
    );
    assert_eq!(a_trust.len(), 0);
    assert_eq!(b_trust.len(), 0);
}

/// The peer confirms while this side's user is still deciding: its answer
/// is kept, and trust is stored once this side confirms too.
#[tokio::test(start_paused = true)]
async fn peer_confirms_first_then_this_side_confirms() {
    let (a, b, a_trust, b_trust, _, _) = run_with(
        Scripted::with(None, Some(true)),
        Scripted::confirm_after(Duration::from_secs(10), true),
        CancellationToken::new(),
    )
    .await;
    assert!(matches!(a, PairOutcome::Paired(_)), "{a:?}");
    assert!(matches!(b, PairOutcome::Paired(_)), "{b:?}");
    assert!(a_trust.contains(&identity(2, "Bob")));
    assert!(b_trust.contains(&identity(1, "Alice")));
}

/// A peer that confirmed and then disconnects ends this side's prompt:
/// there is no answer to give anyone, and nothing is stored.
#[tokio::test(start_paused = true)]
async fn peer_leaving_during_the_prompt_ends_it() {
    let (b, b_trust, elapsed) = against_raw_peer(|mut s| async move {
        s.writer
            .send(&Message::PairResult { confirmed: true })
            .await
            .unwrap();
        s.writer.shutdown().await;
    })
    .await;
    assert_eq!(b, PairOutcome::Failed(TransferError::ConnectionLost));
    assert!(elapsed < Timeouts::default().sas, "{elapsed:?}");
    assert_eq!(b_trust.len(), 0);
}

#[tokio::test(start_paused = true)]
async fn peer_cancel_message_during_the_prompt_is_a_no() {
    let (b, b_trust, elapsed) = against_raw_peer(|mut s| async move {
        s.writer
            .send(&Message::Cancel {
                reason: CancelReason::User,
            })
            .await
            .unwrap();
        // A Cancel gets no reply (§16.1): the next thing is the end of stream.
        assert!(matches!(
            s.reader.recv(Phase::Sas).await,
            Err(RecvError::Transport(TransferError::ConnectionLost))
        ));
    })
    .await;
    assert_eq!(b, PairOutcome::Failed(TransferError::PairingRejected));
    assert!(elapsed < Timeouts::default().sas, "{elapsed:?}");
    assert_eq!(b_trust.len(), 0);
}

/// An early "yes" from the peer never stands in for this side's answer:
/// a local "no", or a local timeout, after it stores nothing.
#[tokio::test(start_paused = true)]
async fn early_peer_yes_then_local_no_or_timeout_stores_nothing() {
    for b_ui in [
        Scripted::confirm_after(Duration::from_secs(10), false),
        Scripted::with(None, None),
    ] {
        let (b, b_trust, _) = against_raw_peer_as(b_ui, |mut s| async move {
            s.writer
                .send(&Message::PairResult { confirmed: true })
                .await
                .unwrap();
            // This side's answer is "no" (after its code timeout, 120 s).
            let frame = s
                .reader
                .recv_within(Duration::from_secs(300), Phase::Sas)
                .await
                .unwrap();
            let Frame::Control(body) = frame else {
                panic!("expected a control frame")
            };
            assert!(matches!(
                decode(body, Role::Responder),
                Ok(Decoded::Message(Message::PairResult { confirmed: false }))
            ));
            s.writer.shutdown().await;
        })
        .await;
        assert_eq!(b, PairOutcome::Failed(TransferError::PairingRejected));
        assert_eq!(b_trust.len(), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn peer_error_during_the_prompt_ends_it() {
    let (b, b_trust, elapsed) = against_raw_peer(|mut s| async move {
        s.writer
            .send(&Message::error(ErrorCode::PairingFailed))
            .await
            .unwrap();
        s.writer.shutdown().await;
    })
    .await;
    assert_eq!(b, PairOutcome::Failed(TransferError::PairingRejected));
    assert!(elapsed < Timeouts::default().sas, "{elapsed:?}");
    assert_eq!(b_trust.len(), 0);
}

#[tokio::test(start_paused = true)]
async fn second_pair_result_during_the_prompt_is_a_violation() {
    let (b, b_trust, elapsed) = against_raw_peer(|mut s| async move {
        for _ in 0..2 {
            s.writer
                .send(&Message::PairResult { confirmed: true })
                .await
                .unwrap();
        }
        s.reader.drain(Duration::from_secs(5)).await;
    })
    .await;
    assert!(
        matches!(b, PairOutcome::Failed(TransferError::ProtocolViolation(_))),
        "{b:?}"
    );
    assert!(elapsed < Timeouts::default().sas, "{elapsed:?}");
    assert_eq!(b_trust.len(), 0);
}

#[tokio::test]
async fn both_confirm_store_trust() {
    let (a, b, a_trust, b_trust, a_ui, b_ui) = run(true, true).await;
    assert!(matches!(a, PairOutcome::Paired(ref v) if v.alias.as_ref().unwrap().as_str() == "Bob"));
    assert!(
        matches!(b, PairOutcome::Paired(ref v) if v.alias.as_ref().unwrap().as_str() == "Alice")
    );
    assert!(a_trust.contains(&identity(2, "Bob")));
    assert!(b_trust.contains(&identity(1, "Alice")));
    // Both users were shown the same code.
    assert_eq!(
        *a_ui.sas_seen.lock().unwrap(),
        *b_ui.sas_seen.lock().unwrap()
    );
}

#[tokio::test]
async fn one_denies_stores_nothing() {
    for (i, l) in [(true, false), (false, true), (false, false)] {
        let (a, b, a_trust, b_trust, _, _) = run(i, l).await;
        assert_eq!(a, PairOutcome::Failed(TransferError::PairingRejected));
        assert_eq!(b, PairOutcome::Failed(TransferError::PairingRejected));
        assert_eq!(a_trust.len(), 0);
        assert_eq!(b_trust.len(), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn window_closes_without_attempt() {
    let net = Net::new();
    let bob = identity(2, "Bob");
    let listener = net.listen(addr(9301));
    let trust = MemTrust::default();
    let (be, _brx) = events();
    let deps = PairDeps {
        trust: &trust,
        interaction: &Scripted::accept(),
        clock: &ManualClock::at(1),
        identity: &bob,
        timeouts: Default::default(),
        events: be,
    };
    let out = pair_listen(
        listener,
        &deps,
        Duration::from_secs(300),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(matches!(
        out,
        PairOutcome::Failed(TransferError::Timeout(_))
    ));
    assert_eq!(trust.len(), 0);
}

#[tokio::test(start_paused = true)]
async fn cancel_while_waiting_for_peer_result() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let listener = net.listen(addr(9302));
    let a_trust = MemTrust::default();
    let b_trust = MemTrust::default();
    // Alice confirms at once; Bob never answers.
    let a_ui = Scripted::with(None, Some(true));
    let b_ui = Scripted::with(None, None);
    let clock = ManualClock::at(5);
    let (ae, _arx) = events();
    let (be, _brx) = events();
    let a_deps = PairDeps {
        trust: &a_trust,
        interaction: &a_ui,
        clock: &clock,
        identity: &alice,
        timeouts: Default::default(),
        events: ae,
    };
    let b_deps = PairDeps {
        trust: &b_trust,
        interaction: &b_ui,
        clock: &clock,
        identity: &bob,
        timeouts: Default::default(),
        events: be,
    };
    let connector = net.connector();
    let targets = to(9302);
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        trigger.cancel();
    });
    let (a, b) = tokio::join!(
        pair_with::<_, StaticDiscovery, _, _, _>(
            &connector,
            None,
            &a_deps,
            ResolveParams {
                targets: &targets,
                explicit: None,
                discover_timeout: Duration::from_millis(1),
                allow_loopback: true,
            },
            cancel,
        ),
        pair_listen(
            listener,
            &b_deps,
            Duration::from_secs(60),
            CancellationToken::new()
        )
    );
    assert_eq!(a.unwrap(), PairOutcome::Cancelled);
    assert!(!matches!(b.unwrap(), PairOutcome::Paired(_)));
    assert_eq!(a_trust.len(), 0);
    assert_eq!(b_trust.len(), 0);
}
