// SPDX-License-Identifier: MPL-2.0
//! Thread safety: many threads calling the node while transfers run must
//! neither crash, deadlock nor lose an answer (ADR-0009 spike S8).
//!
//! Unix only: the facade ships for Apple platforms and Android.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation
)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dropumi_ffi::{
    AcceptMode, Direction, DropumiError, NodeEvent, Prompt, ReceiveOptions, ReceivePolicy,
    SendItem, SendOptions, SendTarget, TransferStatus,
};
use support::*;

const THREADS: usize = 8;
const CALLS: usize = 10_000;

#[test]
fn concurrent_calls_during_transfers() {
    let (mut a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = b
        .node
        .start_receiving(ReceiveOptions {
            policy: ReceivePolicy::TrustedOnly,
            accept: AcceptMode::Prompt,
            port: Some(0),
            advertise: false,
        })
        .unwrap()
        .port;
    let stop = Arc::new(AtomicBool::new(false));
    let workers: Vec<_> = (0..THREADS)
        .map(|t| {
            let node = Arc::clone(&b.node);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                for i in 0..CALLS {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    match (t + i) % 6 {
                        0 => drop(node.snapshot()),
                        1 => drop(node.pending_prompts()),
                        // Prompt ids far beyond any real one: always refused.
                        2 => assert!(matches!(
                            node.answer_offer(1_000_000 + i as u64, true),
                            Err(DropumiError::PromptNotFound)
                        )),
                        3 => drop(node.cancel_incoming(format!("{:032x}", i))),
                        4 => drop(node.nearby_peers()),
                        _ => drop(node.trusted_devices().unwrap()),
                    }
                }
            })
        })
        .collect();

    // Meanwhile, three transfers are offered and each prompt is answered.
    let started = Instant::now();
    for n in 0..3 {
        let file = write_file(&a.inbox, &format!("f{n}.txt"), &pattern(50_000, n as u8));
        a.node
            .send(
                SendTarget::Address {
                    address: format!("127.0.0.1:{port}"),
                    expect_fingerprint: None,
                },
                vec![SendItem::Path {
                    path: file.to_string_lossy().into_owned(),
                }],
                SendOptions {
                    allow_unverified: false,
                },
            )
            .unwrap();
        let Prompt::Offer { id, .. } = b.events.prompt() else {
            panic!("expected an offer");
        };
        b.node
            .answer_offer(id, true)
            .expect("the answer is not lost");
        let r = match a.events.wait("the send to end", |e| {
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
        };
        assert_eq!(r.status, TransferStatus::Completed);
    }
    for w in workers {
        w.join().expect("no worker panicked");
    }
    stop.store(true, Ordering::Relaxed);
    assert!(started.elapsed() < Duration::from_secs(60), "no deadlock");
    assert_eq!(stored(&b.inbox).len(), 3);
}
