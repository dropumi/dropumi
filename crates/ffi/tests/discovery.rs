// SPDX-License-Identifier: MPL-2.0
//! Discovery through an app adapter: advertising, the nearby list, and
//! sending to a nearby device. A shared in-memory "LAN" resolves every
//! advertisement at 127.0.0.1.
//!
//! Unix only: the facade ships for Apple platforms and Android.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dropumi_ffi::{
    AcceptMode, Direction, ErrorCode, NodeEvent, Prompt, ReceiveOptions, ReceivePolicy, SendItem,
    SendOptions, SendTarget, ServiceAd, TransferStatus,
};
use support::*;

fn advertised_receiver(n: &TestNode, policy: ReceivePolicy) -> (u16, String) {
    let info = n
        .node
        .start_receiving(ReceiveOptions {
            policy,
            accept: AcceptMode::AutoTrusted,
            port: Some(0),
            advertise: true,
        })
        .unwrap();
    (info.port, info.instance.expect("advertising"))
}

fn on_lan(lan: &Arc<Lan>) -> TestNode {
    node_with(Setup {
        discovery: Some(lan.adapter()),
        ..Setup::default()
    })
}

fn finished(n: &mut TestNode) -> dropumi_ffi::TransferResult {
    match n.events.wait("a finished send", |e| {
        matches!(
            e,
            NodeEvent::TransferFinished {
                direction: Direction::Outgoing,
                ..
            }
        )
    }) {
        NodeEvent::TransferFinished { result, .. } => result,
        _ => unreachable!(),
    }
}

fn found(n: &mut TestNode, instance: &str) -> dropumi_ffi::NearbyPeer {
    let want = instance.to_owned();
    match n.events.wait(
        "a nearby device",
        |e| matches!(e, NodeEvent::PeerFound { peer } if peer.instance == want),
    ) {
        NodeEvent::PeerFound { peer } => peer,
        _ => unreachable!(),
    }
}

fn send_file(a: &TestNode, instance: &str, allow_unverified: bool) {
    let file = write_file(&a.inbox, "hello.txt", b"hello nearby");
    a.node
        .send(
            SendTarget::Nearby {
                instance: instance.into(),
            },
            vec![SendItem::Path {
                path: file.to_string_lossy().into_owned(),
            }],
            SendOptions { allow_unverified },
        )
        .unwrap();
}

#[test]
fn advertising_publishes_txt_and_withdraws_on_stop_accepting() {
    let lan = Lan::new();
    let adapter = lan.adapter();
    let b = node_with(Setup {
        discovery: Some(Arc::clone(&adapter)),
        ..Setup::default()
    });
    let id = b.node.set_device_name("Studio Mac".into()).unwrap();
    let (port, instance) = advertised_receiver(&b, ReceivePolicy::TrustedOnly);
    let ad: ServiceAd = adapter.advertised.lock().unwrap()[0].clone();
    assert_eq!(ad.instance, instance);
    assert_eq!(ad.service_type, "_dropumi._tcp");
    assert_eq!(ad.port, port);
    assert!(ad.instance.starts_with("dropumi-"));
    assert!(
        !ad.instance.contains("Studio"),
        "the name is never in the instance"
    );
    let txt: Vec<(String, String)> = ad
        .txt
        .iter()
        .map(|e| (e.key.clone(), String::from_utf8(e.value.clone()).unwrap()))
        .collect();
    assert_eq!(
        txt,
        vec![
            ("v".into(), "1".into()),
            ("n".into(), "Studio Mac".into()),
            ("fp".into(), id.fingerprint_hint),
        ]
    );

    b.node.stop_accepting();
    // Withdrawal runs on its own thread: poll (20 ms, bounded).
    let deadline = Instant::now() + Duration::from_secs(10);
    while !adapter.withdrawn.lock().unwrap().contains(&instance) {
        assert!(Instant::now() < deadline, "advertisement not withdrawn");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn nearby_send_pins_full_key_when_hint_matches_one_trust_entry() {
    let lan = Lan::new();
    let (mut a, b) = (on_lan(&lan), on_lan(&lan));
    pair_manually(&a, &b);
    let (_, instance) = advertised_receiver(&b, ReceivePolicy::TrustedOnly);
    a.node.start_discovery().unwrap();
    let peer = found(&mut a, &instance);
    assert_eq!(
        peer.hint_alias.as_deref(),
        Some("B"),
        "shown as 'looks like B'"
    );
    send_file(&a, &instance, false);
    assert_eq!(finished(&mut a).status, TransferStatus::Completed);
    assert!(
        !a.events
            .drain()
            .iter()
            .any(|e| matches!(e, NodeEvent::PromptOpened { .. })),
        "a pinned, paired receiver needs no code"
    );
}

#[test]
fn copied_fp_hint_does_not_redirect_send() {
    let lan = Lan::new();
    let (mut a, b) = (on_lan(&lan), on_lan(&lan));
    let mut impostor = node();
    pair_manually(&a, &b);
    let hint = b.node.identity().unwrap().fingerprint_hint;
    // The impostor accepts anything and advertises the paired device's hint.
    let port = impostor
        .node
        .start_receiving(ReceiveOptions {
            policy: ReceivePolicy::Everyone { minutes: 10 },
            accept: AcceptMode::AutoTrusted,
            port: Some(0),
            advertise: false,
        })
        .unwrap()
        .port;
    lan.publish(ServiceAd {
        service_type: "_dropumi._tcp".into(),
        instance: "dropumi-badc0ffe".into(),
        port,
        txt: txt(&[("v", "1"), ("n", "B"), ("fp", &hint)]),
    });
    a.node.start_discovery().unwrap();
    let peer = found(&mut a, "dropumi-badc0ffe");
    assert_eq!(peer.hint_alias.as_deref(), Some("B"));
    // Even with unverified receivers allowed, the pinned key decides.
    send_file(&a, "dropumi-badc0ffe", true);
    let r = finished(&mut a);
    assert_eq!(r.status, TransferStatus::Failed);
    assert_eq!(r.error, Some(ErrorCode::PeerIdentityMismatch));
    assert!(
        !impostor
            .events
            .drain()
            .iter()
            .any(|e| matches!(e, NodeEvent::OfferReceived { .. }))
    );
}

#[test]
fn copied_fp_hint_does_not_block_send() {
    let lan = Lan::new();
    let (mut a, b) = (on_lan(&lan), on_lan(&lan));
    pair_manually(&a, &b);
    let (_, instance) = advertised_receiver(&b, ReceivePolicy::TrustedOnly);
    let hint = b.node.identity().unwrap().fingerprint_hint;
    lan.publish(ServiceAd {
        service_type: "_dropumi._tcp".into(),
        instance: "dropumi-0badf00d".into(),
        port: 9,
        txt: txt(&[("v", "1"), ("n", "B"), ("fp", &hint)]),
    });
    a.node.start_discovery().unwrap();
    found(&mut a, &instance);
    send_file(&a, &instance, false);
    assert_eq!(finished(&mut a).status, TransferStatus::Completed);
}

#[test]
fn unpaired_nearby_device_needs_a_code() {
    let lan = Lan::new();
    let (mut a, b) = (on_lan(&lan), on_lan(&lan));
    let (_, instance) = advertised_receiver(&b, ReceivePolicy::Everyone { minutes: 10 });
    a.node.start_discovery().unwrap();
    assert_eq!(found(&mut a, &instance).hint_alias, None);
    send_file(&a, &instance, false);
    let r = finished(&mut a);
    assert_eq!(r.error, Some(ErrorCode::UnverifiedPeer));
    send_file(&a, &instance, true);
    assert!(matches!(a.events.prompt(), Prompt::Code { .. }));
}

#[test]
fn own_advertisement_not_listed() {
    let lan = Lan::new();
    let mut b = on_lan(&lan);
    let other = on_lan(&lan);
    let (_, own) = advertised_receiver(&b, ReceivePolicy::TrustedOnly);
    let (_, theirs) = advertised_receiver(&other, ReceivePolicy::TrustedOnly);
    b.node.start_discovery().unwrap();
    found(&mut b, &theirs);
    let listed: Vec<String> = b
        .node
        .nearby_peers()
        .into_iter()
        .map(|p| p.instance)
        .collect();
    assert_eq!(listed, vec![theirs]);
    assert!(!listed.contains(&own));
}

#[test]
fn discovery_unavailable_without_adapter() {
    let a = node();
    assert!(matches!(
        a.node.start_discovery(),
        Err(dropumi_ffi::DropumiError::DiscoveryUnavailable { .. })
    ));
    assert!(matches!(
        a.node.send(
            SendTarget::Nearby {
                instance: "dropumi-00000000".into()
            },
            vec![SendItem::Path {
                path: "/tmp/x".into()
            }],
            SendOptions {
                allow_unverified: false
            },
        ),
        Err(dropumi_ffi::DropumiError::NotFound { .. })
    ));
}
