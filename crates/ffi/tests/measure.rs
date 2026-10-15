// SPDX-License-Identifier: MPL-2.0
//! Measurements for ADR-0009 (spikes S6 and S7). Opt-in and never part of
//! the default run:
//!
//! ```sh
//! cargo test --release -p dropumi-ffi --test measure --locked -- --ignored --nocapture
//! ```
//!
//! They print numbers; they assert only that the work completed.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dropumi_ffi::{
    AcceptMode, Direction, ForeignFile, NodeEvent, ReceiveOptions, ReceivePolicy, SendItem,
    SendOptions, SendTarget, TransferStatus,
};
use support::*;

fn receive(n: &TestNode) -> u16 {
    n.node
        .start_receiving(ReceiveOptions {
            policy: ReceivePolicy::TrustedOnly,
            accept: AcceptMode::AutoTrusted,
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

fn finished(n: &mut TestNode, direction: Direction) -> TransferStatus {
    match n.events.wait(
        "a finished transfer",
        |e| matches!(e, NodeEvent::TransferFinished { direction: d, .. } if *d == direction),
    ) {
        NodeEvent::TransferFinished { result, .. } => result.status,
        _ => unreachable!(),
    }
}

fn summary(label: &str, mut v: Vec<Duration>) {
    v.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    println!(
        "{label}: n={} min={:.2} ms median={:.2} ms max={:.2} ms",
        v.len(),
        ms(v[0]),
        ms(v[v.len() / 2]),
        ms(v[v.len() - 1])
    );
}

/// S7: from `OperationHandle::cancel` on the sender until each side reports
/// `TransferFinished(Cancelled)`, over loopback, with the sender's document
/// held mid-file. The timer starts only once the sender's reader reports that
/// it is blocked at its hold point (the receiver's `TransferAccepted` alone
/// does not prove that).
#[test]
#[ignore = "measurement"]
fn measure_cancel_latency() {
    let (mut a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b);
    let (mut sender, mut receiver) = (Vec::new(), Vec::new());
    for _ in 0..20 {
        let mut doc = MemDoc::new("held.bin", pattern(400_000, 1));
        doc.hold_after = Some(100_000);
        doc.hold.close();
        let gate = Arc::clone(&doc.hold);
        let held = doc.signal_held();
        let file: Arc<dyn ForeignFile> = Arc::new(doc);
        let op = a
            .node
            .send(
                to(port),
                vec![SendItem::Stream { file }],
                SendOptions {
                    allow_unverified: false,
                },
            )
            .unwrap();
        b.events.wait("accepted", |e| {
            matches!(
                e,
                NodeEvent::TransferAccepted {
                    operation: None,
                    ..
                }
            )
        });
        held.recv_timeout(WAIT)
            .unwrap_or_else(|e| panic!("the reader never reached its hold point: {e}"));
        let start = Instant::now();
        op.cancel();
        assert_eq!(
            finished(&mut a, Direction::Outgoing),
            TransferStatus::Cancelled
        );
        sender.push(start.elapsed());
        assert_eq!(
            finished(&mut b, Direction::Incoming),
            TransferStatus::Cancelled
        );
        receiver.push(start.elapsed());
        gate.open();
    }
    summary("S7 cancel latency, sender sees Cancelled", sender);
    summary("S7 cancel latency, receiver sees Cancelled", receiver);
}

/// S6 (Rust side): events and batches delivered to a listener for a
/// transfer of many small files.
#[test]
#[ignore = "measurement"]
fn measure_event_delivery() {
    let (mut a, mut b) = (node(), node());
    pair_manually(&a, &b);
    let port = receive(&b);
    let files: Vec<SendItem> = (0..2_000)
        .map(|i| {
            let p = write_file(&a.inbox, &format!("f{i:04}.txt"), b"x");
            SendItem::Path {
                path: p.to_string_lossy().into_owned(),
            }
        })
        .collect();
    let start = Instant::now();
    a.node
        .send(
            to(port),
            files,
            SendOptions {
                allow_unverified: false,
            },
        )
        .unwrap();
    assert_eq!(
        finished(&mut a, Direction::Outgoing),
        TransferStatus::Completed
    );
    let elapsed = start.elapsed();
    assert_eq!(
        finished(&mut b, Direction::Incoming),
        TransferStatus::Completed
    );
    let sender_events = a.events.drain().len();
    let receiver_events = b.events.drain().len();
    println!(
        "S6 2,000-file transfer: {:.0} ms; events delivered: sender {sender_events}, receiver {receiver_events}",
        elapsed.as_secs_f64() * 1000.0
    );
}
