// SPDX-License-Identifier: MPL-2.0
//! Secure channel establishment: identity checks before msg3, tampering,
//! and pre-authentication bounds.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

mod support;

use std::net::{IpAddr, Ipv4Addr};

use dropumi_application::channel::{
    InitiateParams, RespondParams, ResponderDecision, initiate, respond,
};
use dropumi_application::config::Timeouts;
use dropumi_application::usecases::connect::{ResolveParams, resolve};
use dropumi_domain::connection::PeerExpectation;
use dropumi_domain::device::DeviceName;
use dropumi_domain::discovery::{DiscoveredPeer, Target};
use dropumi_domain::error::TransferError;
use dropumi_domain::identity::{Fingerprint, FingerprintPrefix};
use dropumi_domain::peer::Intent;
use support::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

async fn run_pair(
    i_trust: &MemTrust,
    r_trust: &MemTrust,
    expectation: &PeerExpectation,
    allow_unverified: bool,
) -> (Result<(), TransferError>, Result<(), TransferError>) {
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let (a, b) = duplex(PIPE);
    let t = Timeouts::default();
    let i = initiate(
        a,
        &alice,
        InitiateParams {
            intent: Intent::Transfer,
            expectation,
            allow_unverified,
            timeouts: &t,
        },
        i_trust,
    );
    let r = respond(
        b,
        &bob,
        RespondParams {
            busy: false,
            timeouts: &t,
        },
        r_trust,
        |_, _| ResponderDecision::Allow { verified: true },
    );
    let (i, r) = tokio::join!(i, r);
    (i.map(drop), r.map(drop))
}

#[tokio::test]
async fn roundtrip_same_sas_and_views() {
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let (a, b) = duplex(PIPE);
    let t = Timeouts::default();
    let exp = PeerExpectation::default();
    let i_trust = MemTrust::with(&[(&bob, "Bobby")]);
    let r_trust = MemTrust::with(&[(&alice, "Ally")]);
    let (i, r) = tokio::join!(
        initiate(
            a,
            &alice,
            InitiateParams {
                intent: Intent::Transfer,
                expectation: &exp,
                allow_unverified: false,
                timeouts: &t
            },
            &i_trust
        ),
        respond(
            b,
            &bob,
            RespondParams {
                busy: false,
                timeouts: &t
            },
            &r_trust,
            |_, _| ResponderDecision::Allow { verified: true }
        )
    );
    let (i, r) = (i.unwrap(), r.unwrap());
    assert_eq!(i.sas, r.sas);
    assert_eq!(i.view.fingerprint, *bob.fingerprint());
    assert_eq!(i.view.alias.as_ref().unwrap().as_str(), "Bobby");
    assert_eq!(i.view.claimed_name.as_str(), "Bob");
    assert!(i.view.trusted);
    assert_eq!(r.view.fingerprint, *alice.fingerprint());
    assert_eq!(r.intent, Intent::Transfer);
}

#[tokio::test]
async fn sender_aborts_before_msg3_for_unverified() {
    let (i, r) = run_pair(
        &MemTrust::default(),
        &MemTrust::default(),
        &PeerExpectation::default(),
        false,
    )
    .await;
    assert_eq!(i.unwrap_err(), TransferError::UnverifiedPeer);
    // The responder only saw the initiator hang up before msg3.
    assert_eq!(r.unwrap_err(), TransferError::HandshakeFailed);
}

#[tokio::test]
async fn pin_mismatch_aborts_before_msg3() {
    let bob = identity(2, "Bob");
    let exp = PeerExpectation {
        exact: Some(Fingerprint::from_digest([0xEE; 32])),
        prefixes: vec![],
    };
    let (i, r) = run_pair(
        &MemTrust::with(&[(&bob, "Bob")]),
        &MemTrust::default(),
        &exp,
        true,
    )
    .await;
    assert_eq!(i.unwrap_err(), TransferError::PeerIdentityMismatch);
    assert_eq!(r.unwrap_err(), TransferError::HandshakeFailed);
}

#[tokio::test]
async fn fp_prefix_unique_trust_pins_full_key() {
    // A key that matches the 8-character prefix but not the pinned full
    // fingerprint is rejected: the exact pin is enforced, not just the prefix.
    let bob = identity(2, "Bob");
    let mut lookalike = *bob.fingerprint().as_bytes();
    lookalike[31] ^= 0x01;
    let exp = PeerExpectation {
        exact: Some(Fingerprint::from_digest(lookalike)),
        prefixes: vec![FingerprintPrefix::of(bob.fingerprint(), 8)],
    };
    let (i, _r) = run_pair(
        &MemTrust::with(&[(&bob, "Bob")]),
        &MemTrust::default(),
        &exp,
        true,
    )
    .await;
    assert_eq!(i.unwrap_err(), TransferError::PeerIdentityMismatch);
}

#[tokio::test]
async fn name_target_binds_fp_hint() {
    // The user asks for "Alpha" (trusted A). Discovery says Alpha has A's
    // hint, but the device that answers is the also-trusted B.
    let a = identity(3, "Alpha");
    let b = identity(4, "Beta");
    let sender = identity(1, "Sender");
    let trust = MemTrust::with(&[(&a, "Alpha"), (&b, "Beta")]);
    let discovery = StaticDiscovery(vec![DiscoveredPeer {
        instance: "x".into(),
        name: DeviceName::sanitize("Alpha"),
        fp_hint: Some(a.fingerprint().hint()),
        addrs: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        port: 9100,
        versions: vec![1],
    }]);
    let net = Net::new();
    let resolution = resolve(
        &net.connector(),
        Some(&discovery),
        &trust,
        ResolveParams {
            targets: &[Target::Name("Alpha".into())],
            explicit: None,
            discover_timeout: std::time::Duration::from_millis(1),
            allow_loopback: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(resolution.expectation.exact, Some(*a.fingerprint()));
    let (s, r) = duplex(PIPE);
    let t = Timeouts::default();
    let empty = MemTrust::default();
    let (i, resp) = tokio::join!(
        initiate(
            s,
            &sender,
            InitiateParams {
                intent: Intent::Transfer,
                expectation: &resolution.expectation,
                allow_unverified: false,
                timeouts: &t
            },
            &trust
        ),
        respond(
            r,
            &b,
            RespondParams {
                busy: false,
                timeouts: &t
            },
            &empty,
            |_, _| ResponderDecision::Allow { verified: true }
        )
    );
    assert_eq!(i.unwrap_err(), TransferError::PeerIdentityMismatch);
    assert_eq!(resp.unwrap_err(), TransferError::HandshakeFailed);
}

#[tokio::test]
async fn preamble_tamper_breaks_handshake() {
    // A proxy rewrites the version list to [3, 1]. The responder still picks
    // 1, but the prologue differs, so the handshake fails: no silent downgrade.
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let (i_side, mut p_left) = duplex(PIPE);
    let (mut p_right, r_side) = duplex(PIPE);
    let proxy = tokio::spawn(async move {
        let mut pre = [0u8; 8];
        p_left.read_exact(&mut pre).await.unwrap();
        assert_eq!(&pre, b"DRPM\x01\x01\x00\x01");
        p_right
            .write_all(b"DRPM\x01\x02\x00\x03\x00\x01")
            .await
            .unwrap();
        let (mut lr, mut lw) = tokio::io::split(p_left);
        let (mut rr, mut rw) = tokio::io::split(p_right);
        let _ = tokio::join!(
            async {
                let _ = tokio::io::copy(&mut lr, &mut rw).await;
                let _ = rw.shutdown().await;
            },
            async {
                let _ = tokio::io::copy(&mut rr, &mut lw).await;
                let _ = lw.shutdown().await;
            }
        );
    });
    let t = Timeouts::default();
    let exp = PeerExpectation::default();
    let trust = MemTrust::default();
    let (i, r) = tokio::join!(
        initiate(
            i_side,
            &alice,
            InitiateParams {
                intent: Intent::Pair,
                expectation: &exp,
                allow_unverified: true,
                timeouts: &t
            },
            &trust
        ),
        respond(
            r_side,
            &bob,
            RespondParams {
                busy: false,
                timeouts: &t
            },
            &trust,
            |_, _| ResponderDecision::Allow { verified: false }
        )
    );
    assert_eq!(i.unwrap_err(), TransferError::HandshakeFailed);
    assert!(r.is_err());
    proxy.abort();
}

#[tokio::test]
async fn oversized_length_rejected_preauth() {
    let bob = identity(2, "Bob");
    let (mut client, server) = duplex(PIPE);
    let t = Timeouts::default();
    let trust = MemTrust::default();
    let r = tokio::spawn(async move {
        respond(
            server,
            &bob,
            RespondParams {
                busy: false,
                timeouts: &t,
            },
            &trust,
            |_, _| ResponderDecision::Allow { verified: false },
        )
        .await
        .map(drop)
    });
    client.write_all(b"DRPM\x01\x01\x00\x01").await.unwrap();
    let mut reply = [0u8; 8];
    client.read_exact(&mut reply).await.unwrap();
    assert_eq!(&reply, b"DRPM\x01\x00\x00\x01");
    // Claim a 65,535-byte handshake message: rejected before reading it.
    client.write_all(&[0xFF, 0xFF]).await.unwrap();
    let err = r.await.unwrap().unwrap_err();
    assert!(matches!(err, TransferError::LimitExceeded(_)), "{err:?}");
}

#[tokio::test]
async fn busy_and_incompatible_preamble_replies() {
    let alice = identity(1, "Alice");
    let bob = identity(2, "Bob");
    let t = Timeouts::default();
    let trust = MemTrust::default();
    let exp = PeerExpectation::default();
    let (a, b) = duplex(PIPE);
    let (i, r) = tokio::join!(
        initiate(
            a,
            &alice,
            InitiateParams {
                intent: Intent::Transfer,
                expectation: &exp,
                allow_unverified: true,
                timeouts: &t
            },
            &trust
        ),
        respond(
            b,
            &bob,
            RespondParams {
                busy: true,
                timeouts: &t
            },
            &trust,
            |_, _| ResponderDecision::Allow { verified: false }
        )
    );
    assert_eq!(i.unwrap_err(), TransferError::Busy);
    assert_eq!(r.unwrap_err(), TransferError::Busy);

    // A raw initiator offering only version 99.
    let (mut client, server) = duplex(PIPE);
    let r = tokio::spawn(async move {
        respond(
            server,
            &bob,
            RespondParams {
                busy: false,
                timeouts: &Timeouts::default(),
            },
            &MemTrust::default(),
            |_, _| ResponderDecision::Allow { verified: false },
        )
        .await
        .map(drop)
    });
    client.write_all(b"DRPM\x01\x01\x00\x63").await.unwrap();
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).await.unwrap();
    assert_eq!(reply, b"DRPM\x01\x01\x01\x00\x01");
    assert_eq!(
        r.await.unwrap().unwrap_err(),
        TransferError::IncompatibleVersion { peer: vec![99] }
    );
}
