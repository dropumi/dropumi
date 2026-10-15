// SPDX-License-Identifier: MPL-2.0
//! Handshake correctness, MitM and replay resistance.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use dropumi_protocol::ProtocolError;
use dropumi_protocol::handshake::{HandshakeOutcome, Initiator, Responder};
use dropumi_protocol::identity::{StaticSecret, fingerprint_of};
use dropumi_protocol::version::Prologue;

fn prologue() -> Prologue {
    Prologue::new(b"DRPM\x01\x01\x00\x01", b"DRPM\x01\x00\x00\x01").unwrap()
}

fn secret(b: u8) -> StaticSecret {
    StaticSecret::from_bytes([b; 32])
}

/// Runs a full handshake and returns (initiator, responder) outcomes.
fn run(i_secret: &StaticSecret, r_secret: &StaticSecret) -> (HandshakeOutcome, HandshakeOutcome) {
    let p = prologue();
    let (i, m1) = Initiator::start(i_secret, &p, [0xA1; 32]).unwrap();
    let (r, m2) = Responder::start(r_secret, &p, [0xB2; 32])
        .unwrap()
        .read_msg1(&m1)
        .unwrap();
    let verify = i.read_msg2(&m2).unwrap();
    let (i_done, m3) = verify.proceed().unwrap();
    let r_done = r.read_msg3(&m3).unwrap();
    (i_done.finish(), r_done.finish())
}

#[test]
fn xx_happy_path_same_hash_and_sas() {
    let (i, r) = run(&secret(1), &secret(2));
    assert_eq!(i.handshake_hash, r.handshake_hash);
    let reveal = r.sas_reveal().unwrap();
    assert_eq!(reveal, [0xB2; 32]);
    let sas_i = i.sas(Some(&reveal)).unwrap();
    let sas_r = r.sas(None).unwrap();
    assert_eq!(sas_i, sas_r);
    assert_eq!(i.n_i, r.n_i);
}

#[test]
fn derived_public_matches_peer_view() {
    let si = secret(7);
    let sr = secret(9);
    let (i, r) = run(&si, &sr);
    assert_eq!(r.remote_static, si.public_key().unwrap());
    assert_eq!(i.remote_static, sr.public_key().unwrap());
    assert_eq!(
        i.remote_fingerprint,
        fingerprint_of(&sr.public_key().unwrap())
    );
}

#[test]
fn initiator_learns_responder_before_msg3() {
    let p = prologue();
    let sr = secret(5);
    let (i, m1) = Initiator::start(&secret(4), &p, [1; 32]).unwrap();
    let (_r, m2) = Responder::start(&sr, &p, [2; 32])
        .unwrap()
        .read_msg1(&m1)
        .unwrap();
    let verify = i.read_msg2(&m2).unwrap();
    assert_eq!(verify.remote_static(), sr.public_key().unwrap());
    assert_eq!(verify.commitment(), dropumi_protocol::sas::commit(&[2; 32]));
}

#[test]
fn prologue_mismatch_fails_at_initiator_msg2() {
    let (i, m1) = Initiator::start(&secret(1), &prologue(), [1; 32]).unwrap();
    let other = Prologue::new(b"DRPM\x01\x01\x00\x02", b"DRPM\x01\x00\x00\x01").unwrap();
    let (_r, m2) = Responder::start(&secret(2), &other, [2; 32])
        .unwrap()
        .read_msg1(&m1)
        .unwrap();
    assert_eq!(i.read_msg2(&m2).unwrap_err(), ProtocolError::Handshake);
}

#[test]
fn msg1_with_payload_rejected() {
    let r = Responder::start(&secret(2), &prologue(), [2; 32]).unwrap();
    assert!(matches!(
        r.read_msg1(&[0u8; 33]),
        Err(ProtocolError::Malformed(_))
    ));
    let r = Responder::start(&secret(2), &prologue(), [2; 32]).unwrap();
    assert!(r.read_msg1(&[0u8; 31]).is_err());
}

#[test]
fn handshake_msg_over_cap_rejected() {
    let (i, _m1) = Initiator::start(&secret(1), &prologue(), [1; 32]).unwrap();
    assert!(matches!(
        i.read_msg2(&[0u8; 513]),
        Err(ProtocolError::LimitExceeded(_))
    ));
}

#[test]
fn tampered_msg2_fails() {
    for pos in [0usize, 40, 90] {
        let (i, m1) = Initiator::start(&secret(1), &prologue(), [1; 32]).unwrap();
        let (_r, mut m2) = Responder::start(&secret(2), &prologue(), [2; 32])
            .unwrap()
            .read_msg1(&m1)
            .unwrap();
        let p = pos.min(m2.len() - 1);
        m2[p] ^= 0x01;
        assert!(i.read_msg2(&m2).is_err(), "flip at {p}");
    }
}

#[test]
fn tampered_msg3_fails() {
    let p = prologue();
    let (i, m1) = Initiator::start(&secret(1), &p, [1; 32]).unwrap();
    let (r, m2) = Responder::start(&secret(2), &p, [2; 32])
        .unwrap()
        .read_msg1(&m1)
        .unwrap();
    let (_i, mut m3) = i.read_msg2(&m2).unwrap().proceed().unwrap();
    let last = m3.len() - 1;
    m3[last] ^= 0x80;
    assert_eq!(r.read_msg3(&m3).unwrap_err(), ProtocolError::Handshake);
}

#[test]
fn replayed_msg1_msg3_fail_at_new_responder() {
    let p = prologue();
    let sr = secret(2);
    // Record an honest session's initiator messages.
    let (i, m1) = Initiator::start(&secret(1), &p, [1; 32]).unwrap();
    let (r, m2) = Responder::start(&sr, &p, [2; 32])
        .unwrap()
        .read_msg1(&m1)
        .unwrap();
    let (_i, m3) = i.read_msg2(&m2).unwrap().proceed().unwrap();
    r.read_msg3(&m3).unwrap();
    // Replay both to a fresh responder (fresh ephemeral key).
    let (r2, _m2b) = Responder::start(&sr, &p, [3; 32])
        .unwrap()
        .read_msg1(&m1)
        .unwrap();
    assert_eq!(r2.read_msg3(&m3).unwrap_err(), ProtocolError::Handshake);
}

#[test]
fn mitm_split_handshakes_sas_independent() {
    // A relay with its own key terminates both legs and forwards nothing it
    // cannot compute. Both legs succeed, but the two codes differ.
    let p = prologue();
    let alice = secret(1);
    let bob = secret(2);
    let mallory = secret(3);
    let (a, a1) = Initiator::start(&alice, &p, [0x11; 32]).unwrap();
    let (m_resp, m2_to_a) = Responder::start(&mallory, &p, [0x22; 32])
        .unwrap()
        .read_msg1(&a1)
        .unwrap();
    let (m_init, m1_to_b) = Initiator::start(&mallory, &p, [0x33; 32]).unwrap();
    let (b, b2) = Responder::start(&bob, &p, [0x44; 32])
        .unwrap()
        .read_msg1(&m1_to_b)
        .unwrap();
    let a_verify = a.read_msg2(&m2_to_a).unwrap();
    assert_ne!(
        a_verify.remote_static(),
        bob.public_key().unwrap(),
        "pinned trust would stop here"
    );
    let (a_done, a3) = a_verify.proceed().unwrap();
    let m_leg_a = m_resp.read_msg3(&a3).unwrap().finish();
    let (m_done, m3_to_b) = m_init.read_msg2(&b2).unwrap().proceed().unwrap();
    let b_out = b.read_msg3(&m3_to_b).unwrap().finish();
    let a_out = a_done.finish();
    let m_leg_b = m_done.finish();
    let sas_a = a_out.sas(m_leg_a.sas_reveal().as_ref()).unwrap();
    let sas_b = b_out.sas(None).unwrap();
    assert_eq!(sas_a, m_leg_a.sas(None).unwrap());
    assert_eq!(sas_b, m_leg_b.sas(b_out.sas_reveal().as_ref()).unwrap());
    assert_ne!(sas_a, sas_b, "independent handshakes yield different codes");
    assert_ne!(a_out.handshake_hash, b_out.handshake_hash);
}

#[test]
fn sas_reveal_mismatch_detected() {
    let (i, _r) = run(&secret(1), &secret(2));
    assert_eq!(
        i.sas(Some(&[0xFF; 32])).unwrap_err(),
        ProtocolError::SasCommitmentMismatch
    );
    assert!(i.sas(None).is_err());
}
