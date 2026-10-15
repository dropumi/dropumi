// SPDX-License-Identifier: MPL-2.0
//! Property tests: decoders never panic on hostile input, and every valid
//! message round-trips.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use dropumi_domain::device::DeviceKind;
use dropumi_domain::peer::Intent;
use dropumi_domain::transfer::{AbortReason, CancelReason, FileEntry, RejectReason, TransferId};
use dropumi_protocol::frame::Frame;
use dropumi_protocol::message::{
    AckStatus, Decoded, ErrorCode, FileHash, Hello, Message, Role, decode, encode,
};
use dropumi_protocol::version::{Preamble, PreambleReply};
use proptest::prelude::*;

fn try_all_decoders(bytes: &[u8]) {
    let _ = decode(bytes, Role::Initiator);
    let _ = decode(bytes, Role::Responder);
    let _ = Frame::decode(bytes);
    if let Ok(h) = <[u8; 6]>::try_from(bytes.get(..6).unwrap_or_default()) {
        let _ = Preamble::decode_header(&h);
        let _ = PreambleReply::decode_header(&h);
    }
    let _ = Preamble::decode_body(bytes);
    let _ = PreambleReply::decode_ok(bytes);
    let _ = PreambleReply::decode_no_common(bytes);
}

fn file_entry() -> impl Strategy<Value = FileEntry> {
    (
        "[a-zA-Z0-9 ._()\u{e9}\u{6771}-]{1,40}".prop_filter("valid wire name", |s| {
            dropumi_domain::filename::validate_wire_name(s).is_ok()
        }),
        0u64..=i64::MAX as u64,
        proptest::option::of("[a-z]{1,10}/[a-z0-9.+-]{1,20}"),
        proptest::option::of(any::<i64>()),
    )
        .prop_map(|(name, size, media_type, modified)| FileEntry {
            name,
            size,
            media_type,
            modified,
        })
}

fn transfer_id() -> impl Strategy<Value = TransferId> {
    any::<[u8; 16]>().prop_map(TransferId::from_bytes)
}

fn message() -> impl Strategy<Value = (Message, Role)> {
    let hello_i = (
        "[a-zA-Z0-9 '\u{e9}]{1,40}",
        0u64..8,
        proptest::option::of("[a-z0-9./]{1,30}"),
        proptest::collection::vec(any::<u16>(), 0..5),
        any::<bool>(),
    )
        .prop_map(|(name, kind, software, features, pair)| {
            (
                Message::Hello(Hello {
                    device_name: name,
                    device_kind: DeviceKind::from_code(kind),
                    software,
                    features,
                    hash_algs: vec![1],
                    intent: Some(if pair { Intent::Pair } else { Intent::Transfer }),
                    sas_reveal: None,
                }),
                Role::Initiator,
            )
        });
    let hello_r = ("[a-zA-Z0-9 ]{1,40}", any::<[u8; 32]>()).prop_map(|(name, reveal)| {
        (
            Message::Hello(Hello {
                device_name: name,
                device_kind: DeviceKind::Unknown,
                software: None,
                features: vec![],
                hash_algs: vec![1, 2],
                intent: None,
                sas_reveal: Some(reveal),
            }),
            Role::Responder,
        )
    });
    prop_oneof![
        hello_i,
        hello_r,
        any::<bool>().prop_map(|c| (Message::PairResult { confirmed: c }, Role::Initiator)),
        (1u64..20).prop_map(|c| (Message::error(ErrorCode::from_code(c)), Role::Responder)),
        (1u64..8).prop_map(|c| (
            Message::Cancel {
                reason: CancelReason::from_code(c)
            },
            Role::Initiator
        )),
        (transfer_id(), 1u32..=10_000, 0u64..=i64::MAX as u64).prop_map(|(id, n, t)| (
            Message::Offer {
                transfer_id: id,
                file_count: n,
                total_size: t
            },
            Role::Initiator
        )),
        (
            transfer_id(),
            0u32..10_000,
            proptest::collection::vec(file_entry(), 1..20)
        )
            .prop_map(|(id, first, files)| (
                Message::OfferFiles {
                    transfer_id: id,
                    first_index: first,
                    files
                },
                Role::Initiator
            )),
        transfer_id().prop_map(|id| (Message::Accept { transfer_id: id }, Role::Responder)),
        (transfer_id(), 1u64..9).prop_map(|(id, r)| (
            Message::Reject {
                transfer_id: id,
                reason: RejectReason::from_code(r)
            },
            Role::Responder
        )),
        (0u32..10_000).prop_map(|i| (
            Message::FileStart {
                index: i,
                offset: 0
            },
            Role::Initiator
        )),
        (0u32..10_000, any::<[u8; 32]>()).prop_map(|(i, d)| (
            Message::FileEnd {
                index: i,
                hash: FileHash::Blake3(d)
            },
            Role::Initiator
        )),
        (0u32..10_000, 1u64..4).prop_map(|(i, r)| (
            Message::FileAbort {
                index: i,
                reason: AbortReason::from_code(r)
            },
            Role::Initiator
        )),
        (0u32..10_000, 0u64..4).prop_map(|(i, s)| (
            Message::FileAck {
                index: i,
                status: AckStatus::from_code(s).unwrap()
            },
            Role::Responder
        )),
        transfer_id().prop_map(|id| (Message::Complete { transfer_id: id }, Role::Initiator)),
        (transfer_id(), 0u32..=10_000).prop_map(|(id, n)| (
            Message::CompleteAck {
                transfer_id: id,
                stored_count: n
            },
            Role::Responder
        )),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    #[test]
    fn decode_arbitrary_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        try_all_decoders(&bytes);
    }

    #[test]
    fn mutated_valid_never_panics(
        (msg, _from) in message(),
        flips in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..8),
        cut in any::<prop::sample::Index>(),
    ) {
        let mut buf = Vec::new();
        encode(&msg, &mut buf).unwrap();
        let mut mutated = buf.clone();
        for (idx, byte) in &flips {
            let i = idx.index(mutated.len());
            mutated[i] ^= byte | 1;
        }
        try_all_decoders(&mutated);
        let truncated = &buf[..cut.index(buf.len())];
        prop_assert!(decode(truncated, Role::Initiator).is_err());
        prop_assert!(decode(truncated, Role::Responder).is_err());
    }

    #[test]
    fn message_roundtrip((msg, from) in message()) {
        let mut buf = Vec::new();
        encode(&msg, &mut buf).unwrap();
        prop_assert_eq!(decode(&buf, from).unwrap(), Decoded::Message(msg));
    }
}
