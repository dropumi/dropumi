// SPDX-License-Identifier: MPL-2.0
//! Authorization and consent policy through the real use cases.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

mod support;

use std::sync::atomic::Ordering;

use dropumi_application::events::{AppEvent, TransferOutcome};
use dropumi_application::ports::Decision;
use dropumi_application::usecases::connect::ResolveParams;
use dropumi_application::usecases::pair::{PairDeps, PairOutcome, pair_with};
use dropumi_domain::error::TransferError;
use dropumi_domain::policy::{AcceptMode, PolicyMode, TransferPolicy};
use dropumi_domain::time::Timestamp;
use dropumi_domain::transfer::RejectReason;
use support::*;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn unknown_initiator_denied_trusted_only() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::default());
    setup.once = false;
    let rx = setup.start(&net, addr(9200));
    let source = MemSource::default();
    let paths = vec![source.add("/x", b"x".to_vec())];
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, events) = tx
        .send(&net, to(9200), paths, false, None, CancellationToken::new())
        .await;
    assert!(matches!(
        out.unwrap(),
        TransferOutcome::Failed {
            error: TransferError::NotAuthorized,
            ..
        }
    ));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEvent::OfferSent { .. }))
    );
    rx.cancel.cancel();
    let (_, revents, sink) = finish(rx).await;
    assert!(revents.iter().any(|e| matches!(
        e,
        AppEvent::SessionClosed {
            code: "not_authorized",
            ..
        }
    )));
    assert!(sink.stored().is_empty());
}

#[tokio::test]
async fn pair_intent_denied_in_receive() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.once = false;
    let rx = setup.start(&net, addr(9201));
    let trust = MemTrust::default();
    let (events, _erx) = events();
    let deps = PairDeps {
        trust: &trust,
        interaction: &Scripted::accept(),
        clock: &ManualClock::at(1),
        identity: &alice,
        timeouts: Default::default(),
        events,
    };
    let out = pair_with::<_, StaticDiscovery, _, _, _>(
        &net.connector(),
        None,
        &deps,
        ResolveParams {
            targets: &to(9201),
            explicit: None,
            discover_timeout: std::time::Duration::from_millis(1),
            allow_loopback: true,
        },
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(out, PairOutcome::Failed(TransferError::NotAuthorized));
    assert_eq!(trust.len(), 0);
    rx.cancel.cancel();
    let _ = finish(rx).await;
}

#[tokio::test]
async fn everyone_mode_expires() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::default());
    setup.once = false;
    setup.policy = TransferPolicy {
        mode: PolicyMode::Everyone {
            until: Timestamp(1_000 + 600),
        },
        accept: AcceptMode::Prompt,
    };
    let clock = setup.clock.clone();
    let rx = setup.start(&net, addr(9202));
    let source = MemSource::default();
    let paths = vec![source.add("/e.txt", b"everyone".to_vec())];
    let tx = SenderSetup::new(alice, MemTrust::default(), source);
    // Inside the window: an unverified sender is allowed (after SAS + consent).
    let (out, _) = tx
        .send(
            &net,
            to(9202),
            paths.clone(),
            true,
            None,
            CancellationToken::new(),
        )
        .await;
    assert!(out.unwrap().is_success());
    // After the window: denied.
    clock.advance(601);
    let (out, _) = tx
        .send(&net, to(9202), paths, true, None, CancellationToken::new())
        .await;
    assert!(matches!(
        out.unwrap(),
        TransferOutcome::Failed {
            error: TransferError::NotAuthorized,
            ..
        }
    ));
    rx.cancel.cancel();
    let (_, _, sink) = finish(rx).await;
    assert_eq!(sink.stored().len(), 1);
}

#[tokio::test]
async fn unverified_offer_always_prompts() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::default());
    setup.policy = TransferPolicy {
        mode: PolicyMode::Everyone {
            until: Timestamp(u64::MAX),
        },
        accept: AcceptMode::AutoTrusted,
    };
    setup.interaction = Scripted::with(Some(Decision::Reject(RejectReason::Declined)), Some(true));
    let rx = setup.start(&net, addr(9203));
    let source = MemSource::default();
    let paths = vec![source.add("/u.txt", b"u".to_vec())];
    let tx = SenderSetup::new(alice, MemTrust::default(), source);
    let (out, _) = tx
        .send(&net, to(9203), paths, true, None, CancellationToken::new())
        .await;
    assert!(matches!(
        out.unwrap(),
        TransferOutcome::Rejected {
            reason: RejectReason::Declined,
            ..
        }
    ));
    let offers = rx.interaction.offers_seen.clone();
    let (_, revents, sink) = finish(rx).await;
    assert_eq!(
        offers.load(Ordering::SeqCst),
        1,
        "even with AutoTrusted, unverified offers prompt"
    );
    assert!(
        revents
            .iter()
            .any(|e| matches!(e, AppEvent::OfferReceived { sas: Some(_), .. }))
    );
    assert!(sink.stored().is_empty());
}

#[tokio::test]
async fn unverified_sender_must_confirm_sas() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::default());
    setup.once = false;
    setup.policy = TransferPolicy {
        mode: PolicyMode::Everyone {
            until: Timestamp(u64::MAX),
        },
        accept: AcceptMode::Prompt,
    };
    let rx = setup.start(&net, addr(9204));
    let source = MemSource::default();
    let paths = vec![source.add("/secret-name.txt", b"s".to_vec())];
    let mut tx = SenderSetup::new(alice, MemTrust::default(), source);
    tx.interaction = Scripted::with(Some(Decision::Accept), Some(false));
    let (out, events) = tx
        .send(&net, to(9204), paths, true, None, CancellationToken::new())
        .await;
    assert!(matches!(
        out.unwrap(),
        TransferOutcome::Failed {
            error: TransferError::SasRejected,
            ..
        }
    ));
    assert!(events.iter().any(|e| matches!(e, AppEvent::Sas { .. })));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AppEvent::OfferSent { .. }))
    );
    rx.cancel.cancel();
    let (_, revents, _) = finish(rx).await;
    assert!(
        !revents
            .iter()
            .any(|e| matches!(e, AppEvent::OfferReceived { .. })),
        "file names are never revealed to an unconfirmed receiver"
    );
}

#[tokio::test]
async fn receiver_declines() {
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let mut setup = ReceiverSetup::new(bob.clone(), MemTrust::with(&[(&alice, "Alice")]));
    setup.policy.accept = AcceptMode::Prompt;
    setup.interaction = Scripted::with(Some(Decision::Reject(RejectReason::Declined)), None);
    let rx = setup.start(&net, addr(9205));
    let source = MemSource::default();
    let paths = vec![source.add("/d.txt", b"d".to_vec())];
    let tx = SenderSetup::new(alice, MemTrust::with(&[(&bob, "Bob")]), source);
    let (out, _) = tx
        .send(&net, to(9205), paths, false, None, CancellationToken::new())
        .await;
    assert!(matches!(
        out.unwrap(),
        TransferOutcome::Rejected {
            reason: RejectReason::Declined,
            ..
        }
    ));
    let (_, _, sink) = finish(rx).await;
    assert!(sink.stored().is_empty());
}
