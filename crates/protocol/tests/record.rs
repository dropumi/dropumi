// SPDX-License-Identifier: MPL-2.0
//! Record layer: integrity, ordering, replay and poisoning.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use dropumi_protocol::ProtocolError;
use dropumi_protocol::handshake::{HandshakeOutcome, Initiator, Responder};
use dropumi_protocol::identity::StaticSecret;
use dropumi_protocol::limits::MAX_PLAINTEXT;
use dropumi_protocol::record::{LEN_PREFIX, MAX_RECORD};
use dropumi_protocol::version::Prologue;

fn pair() -> (HandshakeOutcome, HandshakeOutcome) {
    let p = Prologue::new(b"a", b"b").unwrap();
    let (i, m1) = Initiator::start(&StaticSecret::from_bytes([1; 32]), &p, [5; 32]).unwrap();
    let (r, m2) = Responder::start(&StaticSecret::from_bytes([2; 32]), &p, [6; 32])
        .unwrap()
        .read_msg1(&m1)
        .unwrap();
    let (i, m3) = i.read_msg2(&m2).unwrap().proceed().unwrap();
    (i.finish(), r.read_msg3(&m3).unwrap().finish())
}

/// Seals and returns the record body (without the length prefix).
fn seal(o: &mut HandshakeOutcome, plain: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; MAX_RECORD];
    let n = o.sealer.seal(plain, &mut out).unwrap();
    let len = u16::from_be_bytes([out[0], out[1]]) as usize;
    assert_eq!(len + LEN_PREFIX, n);
    out[LEN_PREFIX..n].to_vec()
}

fn open(o: &mut HandshakeOutcome, ct: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    let mut out = vec![0u8; MAX_RECORD];
    o.opener.open(ct, &mut out).map(<[u8]>::to_vec)
}

#[test]
fn roundtrip_both_directions_and_max_size() {
    let (mut i, mut r) = pair();
    let big = vec![0x5A; MAX_PLAINTEXT];
    for msg in [&b"hello"[..], &big] {
        let ct = seal(&mut i, msg);
        assert_eq!(open(&mut r, &ct).unwrap(), msg);
        let ct = seal(&mut r, msg);
        assert_eq!(open(&mut i, &ct).unwrap(), msg);
    }
    let mut out = vec![0u8; MAX_RECORD];
    assert!(
        i.sealer
            .seal(&vec![0; MAX_PLAINTEXT + 1], &mut out)
            .is_err()
    );
    assert!(i.sealer.seal(&[], &mut out).is_err());
}

#[test]
fn record_bitflip_fails() {
    let (mut i, mut r) = pair();
    let mut ct = seal(&mut i, b"payload");
    ct[3] ^= 0x04;
    assert_eq!(open(&mut r, &ct), Err(ProtocolError::Decrypt));
}

#[test]
fn record_replay_fails() {
    let (mut i, mut r) = pair();
    let ct = seal(&mut i, b"one");
    assert!(open(&mut r, &ct).is_ok());
    assert_eq!(open(&mut r, &ct), Err(ProtocolError::Decrypt));
}

#[test]
fn record_reorder_fails() {
    let (mut i, mut r) = pair();
    let first = seal(&mut i, b"first");
    let second = seal(&mut i, b"second");
    assert_eq!(open(&mut r, &second), Err(ProtocolError::Decrypt));
    // Poisoned: even the correct next record now fails.
    assert_eq!(open(&mut r, &first), Err(ProtocolError::Decrypt));
}

#[test]
fn directions_use_independent_keys() {
    let (mut i, _r) = pair();
    let ct = seal(&mut i, b"reflect me");
    // Reflecting the initiator's own record back to it must fail.
    assert_eq!(open(&mut i, &ct), Err(ProtocolError::Decrypt));
}

#[test]
fn opener_poisoned_after_failure() {
    let (mut i, mut r) = pair();
    assert!(open(&mut r, &[0u8; 20]).is_err());
    assert!(r.opener.is_poisoned());
    let ct = seal(&mut i, b"valid");
    assert_eq!(open(&mut r, &ct), Err(ProtocolError::Decrypt));
}

#[test]
fn truncated_and_short_records_fail() {
    let (mut i, mut r) = pair();
    let ct = seal(&mut i, b"abcdef");
    assert!(open(&mut r, &ct[..ct.len() - 1]).is_err());
    let (_i2, mut r2) = pair();
    assert!(open(&mut r2, &[0u8; 16]).is_err());
}
