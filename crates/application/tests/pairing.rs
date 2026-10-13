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

use dropumi_application::usecases::connect::ResolveParams;
use dropumi_application::usecases::pair::{PairDeps, PairOutcome, pair_listen, pair_with};
use dropumi_domain::error::TransferError;
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
    let net = Net::new();
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let listener = net.listen(addr(9300));
    let a_trust = MemTrust::default();
    let b_trust = MemTrust::default();
    let a_ui = Scripted::with(None, Some(initiator_confirms));
    let b_ui = Scripted::with(None, Some(listener_confirms));
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
            CancellationToken::new(),
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
