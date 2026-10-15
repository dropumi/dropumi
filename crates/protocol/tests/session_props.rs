// SPDX-License-Identifier: MPL-2.0
//! Model-based properties for the session machines: any valid sender trace
//! is accepted end to end, and arbitrary input never panics either machine.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use dropumi_domain::transfer::{AbortReason, FileEntry, TransferId, TransferRequest};
use dropumi_protocol::frame::Frame;
use dropumi_protocol::message::{AckStatus, Message, encode};
use dropumi_protocol::session::{ReceiverEvent, ReceiverMachine, SenderEvent, SenderMachine};
use proptest::prelude::*;

fn enc(m: &Message) -> Vec<u8> {
    let mut b = Vec::new();
    encode(m, &mut b).unwrap();
    b
}

#[derive(Debug, Clone)]
struct FilePlan {
    content: Vec<u8>,
    chunk: usize,
    abort: bool,
}

fn file_plan() -> impl Strategy<Value = FilePlan> {
    (
        proptest::collection::vec(any::<u8>(), 0..3000),
        1usize..700,
        proptest::bool::weighted(0.15),
    )
        .prop_map(|(content, chunk, abort)| FilePlan {
            content,
            chunk,
            abort,
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn valid_sender_trace_accepted(plans in proptest::collection::vec(file_plan(), 1..8)) {
        let files: Vec<FileEntry> = plans
            .iter()
            .enumerate()
            .map(|(i, p)| FileEntry {
                name: format!("file-{i}.bin"),
                size: p.content.len() as u64,
                media_type: None,
                modified: None,
            })
            .collect();
        let request = TransferRequest::new(TransferId::from_bytes([1; 16]), files).unwrap();
        let mut s = SenderMachine::new(request.clone());
        let mut r = ReceiverMachine::new();
        let mut ready = None;
        for m in s.offer().unwrap() {
            if let ReceiverEvent::OfferReady(req) = r.on_frame(Frame::Control(&enc(&m))).unwrap() {
                ready = Some(req);
            }
        }
        prop_assert_eq!(ready.as_ref(), Some(&request));
        let accept = enc(&r.accept().unwrap());
        prop_assert_eq!(s.on_control(&accept).unwrap(), SenderEvent::Accepted);

        let mut stored = 0u32;
        for (i, plan) in plans.iter().enumerate() {
            let i = u32::try_from(i).unwrap();
            if plan.abort {
                let abort = enc(&s.abort_file(AbortReason::SourceChanged).unwrap());
                prop_assert_eq!(
                    r.on_frame(Frame::Control(&abort)).unwrap(),
                    ReceiverEvent::FileAborted { index: i }
                );
                let ack = enc(&r.ack(i, AckStatus::Aborted).unwrap());
                s.on_control(&ack).unwrap();
                continue;
            }
            let start = enc(&s.start_file(i).unwrap());
            prop_assert_eq!(
                r.on_frame(Frame::Control(&start)).unwrap(),
                ReceiverEvent::FileStarted { index: i, size: plan.content.len() as u64 }
            );
            let mut received = Vec::new();
            for chunk in plan.content.chunks(plan.chunk) {
                s.data(chunk).unwrap();
                match r.on_frame(Frame::Data(chunk)).unwrap() {
                    ReceiverEvent::Data(d) => received.extend_from_slice(d),
                    other => prop_assert!(false, "unexpected {other:?}"),
                }
            }
            prop_assert_eq!(&received, &plan.content);
            let end = enc(&s.end_file().unwrap());
            let expected = *blake3::hash(&plan.content).as_bytes();
            prop_assert_eq!(
                r.on_frame(Frame::Control(&end)).unwrap(),
                ReceiverEvent::FileFinished { index: i, hash_ok: true, digest: expected }
            );
            let ack = enc(&r.ack(i, AckStatus::Stored).unwrap());
            s.on_control(&ack).unwrap();
            stored += 1;
        }
        let complete = enc(&s.complete().unwrap());
        prop_assert_eq!(
            r.on_frame(Frame::Control(&complete)).unwrap(),
            ReceiverEvent::CompleteRequested
        );
        let done = enc(&r.complete_ack().unwrap());
        prop_assert_eq!(s.on_control(&done).unwrap(), SenderEvent::Completed { stored });
        prop_assert!(s.is_done());
        prop_assert!(r.is_done());
    }

    #[test]
    fn random_inputs_never_panic(
        frames in proptest::collection::vec(
            (any::<bool>(), proptest::collection::vec(any::<u8>(), 1..200)),
            0..40
        )
    ) {
        let request = TransferRequest::new(
            TransferId::from_bytes([2; 16]),
            vec![FileEntry { name: "a".into(), size: 5, media_type: None, modified: None }],
        )
        .unwrap();
        let mut r = ReceiverMachine::new();
        let mut s = SenderMachine::new(request);
        let _ = s.offer();
        for (control, bytes) in &frames {
            let frame = if *control { Frame::Control(bytes) } else { Frame::Data(bytes) };
            let _ = r.on_frame(frame);
            let _ = s.on_control(bytes);
            let _ = r.accept();
            let _ = r.ack(0, AckStatus::Stored);
            let _ = r.complete_ack();
        }
    }
}
