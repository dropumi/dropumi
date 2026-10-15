// SPDX-License-Identifier: MPL-2.0
//! Establishing an authenticated session: preamble, Noise XX, Hellos.
//!
//! The initiator verifies the responder's static key against the target
//! expectation and its trust policy *before* sending message 3, so a wrong
//! or untrusted receiver never learns who is calling or what is offered.

use std::time::Instant;

use dropumi_domain::connection::PeerExpectation;
use dropumi_domain::device::{DeviceKind, DeviceName};
use dropumi_domain::error::{Phase, TransferError};
use dropumi_domain::identity::{DeviceIdentity, Fingerprint, PublicKey};
use dropumi_domain::peer::{Intent, Peer, PeerCapabilities, TrustLevel};
use dropumi_protocol::ProtocolError;
use dropumi_protocol::frame::Frame;
use dropumi_protocol::handshake::{HandshakeOutcome, Initiator, Responder};
use dropumi_protocol::limits::{HASH_ALG_BLAKE3, MSG1_LEN, PREAMBLE_MAX, SUPPORTED_VERSIONS};
use dropumi_protocol::message::{Decoded, ErrorCode, Hello, Message, Role, decode};
use dropumi_protocol::random;
use dropumi_protocol::record::{RecordPhase, frame_handshake, parse_len};
use dropumi_protocol::sas::SasCode;
use dropumi_protocol::version::{
    PREAMBLE_HEADER_LEN, Preamble, PreambleReply, Prologue, REPLY_HEADER_LEN, select_version,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf, split};
use tokio::time::timeout;

use super::io::{RecordReader, RecordWriter, RecvError};
use crate::config::Timeouts;
use crate::error::map_io;
use crate::events::PeerView;
use crate::identity::LocalIdentity;
use crate::ports::TrustStore;

/// Software identifier sent in Hello.
pub const SOFTWARE: &str = concat!("dropumi/", env!("CARGO_PKG_VERSION"));

/// An authenticated, encrypted session.
#[derive(Debug)]
pub struct EstablishedSession<S> {
    /// The peer.
    pub peer: Peer,
    /// How to present the peer.
    pub view: PeerView,
    /// Session intent.
    pub intent: Intent,
    /// Verification code for this session.
    pub sas: SasCode,
    /// Preamble-to-Hello latency in microseconds.
    pub handshake_us: u64,
    /// Incoming records.
    pub reader: RecordReader<ReadHalf<S>>,
    /// Outgoing records.
    pub writer: RecordWriter<WriteHalf<S>>,
}

/// Initiator options.
#[derive(Debug, Clone, Copy)]
pub struct InitiateParams<'a> {
    /// Why we connect.
    pub intent: Intent,
    /// Identity the responder's key must satisfy.
    pub expectation: &'a PeerExpectation,
    /// Allow an untrusted responder (after SAS confirmation by the caller).
    pub allow_unverified: bool,
    /// Timeouts.
    pub timeouts: &'a Timeouts,
}

/// What the responder learned about the initiator before deciding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerInfo {
    /// Authenticated fingerprint.
    pub fingerprint: Fingerprint,
    /// Authenticated key.
    pub public_key: PublicKey,
}

/// The responder's decision after the initiator's Hello.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponderDecision {
    /// Continue.
    Allow {
        /// Whether the initiator is trusted.
        verified: bool,
    },
    /// Refuse with `NOT_AUTHORIZED`.
    Deny,
    /// Refuse with `BUSY`.
    Busy,
}

fn local_hello(local: &LocalIdentity, intent: Option<Intent>, reveal: Option<[u8; 32]>) -> Hello {
    Hello {
        device_name: local.name().as_str().to_owned(),
        device_kind: DeviceKind::Unknown,
        software: Some(SOFTWARE.to_owned()),
        features: Vec::new(),
        hash_algs: vec![HASH_ALG_BLAKE3],
        intent,
        sas_reveal: reveal,
    }
}

fn capabilities(version: u16, hello: &Hello) -> PeerCapabilities {
    PeerCapabilities {
        protocol_version: version,
        // Protocol v1 defines no optional features: the intersection is empty.
        features: Vec::new(),
        hash_algs: hello.hash_algs.clone(),
    }
}

fn peer_view<T: TrustStore>(trust: &T, peer: &Peer) -> PeerView {
    let alias = trust
        .get(&peer.identity.fingerprint)
        .ok()
        .flatten()
        .and_then(|d| d.alias);
    PeerView {
        fingerprint: peer.identity.fingerprint,
        public_key: peer.identity.public_key,
        trusted: peer.trust.is_trusted(),
        alias,
        claimed_name: peer.identity.name.clone(),
    }
}

async fn read_exact<S: AsyncRead + Unpin>(s: &mut S, buf: &mut [u8]) -> Result<(), TransferError> {
    s.read_exact(buf)
        .await
        .map(drop)
        .map_err(|e| map_io(&e, Phase::Handshake))
}

async fn write_all<S: AsyncWrite + Unpin>(s: &mut S, buf: &[u8]) -> Result<(), TransferError> {
    s.write_all(buf)
        .await
        .map_err(|e| map_io(&e, Phase::Handshake))?;
    s.flush().await.map_err(|e| map_io(&e, Phase::Handshake))
}

async fn read_handshake_usg<S: AsyncRead + Unpin>(s: &mut S) -> Result<Vec<u8>, TransferError> {
    let mut prefix = [0u8; 2];
    read_exact(s, &mut prefix).await?;
    let len = parse_len(prefix, RecordPhase::Handshake)?;
    let mut msg = vec![0u8; len];
    read_exact(s, &mut msg).await?;
    Ok(msg)
}

fn trust_level<T: TrustStore>(trust: &T, fp: &Fingerprint) -> Result<TrustLevel, TransferError> {
    Ok(if trust.get(fp)?.is_some() {
        TrustLevel::Trusted
    } else {
        TrustLevel::Unverified
    })
}

fn hello_from(frame: Result<Frame<'_>, RecvError>, from: Role) -> Result<Hello, TransferError> {
    let Frame::Control(body) = frame? else {
        return Err(TransferError::ProtocolViolation("expected Hello"));
    };
    match decode(body, from)? {
        Decoded::Message(Message::Hello(h)) => Ok(h),
        Decoded::Message(Message::Error { code, .. }) => Err(match code {
            ErrorCode::NotAuthorized => TransferError::NotAuthorized,
            ErrorCode::Busy => TransferError::Busy,
            other => TransferError::Remote {
                code: u16::try_from(other.code()).unwrap_or(u16::MAX),
            },
        }),
        _ => Err(TransferError::ProtocolViolation("expected Hello")),
    }
}

/// Opens a session as the initiator.
///
/// # Errors
/// `IncompatibleVersion`, `Busy`, `HandshakeFailed`,
/// `PeerIdentityMismatch` and `UnverifiedPeer` (both before msg3),
/// `NotAuthorized`, connection errors, or `Timeout(Handshake)`.
pub async fn initiate<S, T>(
    stream: S,
    local: &LocalIdentity,
    params: InitiateParams<'_>,
    trust: &T,
) -> Result<EstablishedSession<S>, TransferError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    T: TrustStore,
{
    let started = Instant::now();
    let limit = params.timeouts.handshake;
    match timeout(limit, initiate_inner(stream, local, params, trust, started)).await {
        Ok(r) => r,
        Err(_) => Err(TransferError::Timeout(Phase::Handshake)),
    }
}

async fn initiate_inner<S, T>(
    mut stream: S,
    local: &LocalIdentity,
    params: InitiateParams<'_>,
    trust: &T,
    started: Instant,
) -> Result<EstablishedSession<S>, TransferError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    T: TrustStore,
{
    // Version preamble.
    let preamble = Preamble::new(SUPPORTED_VERSIONS)?.encode();
    write_all(&mut stream, &preamble).await?;
    let mut header = [0u8; REPLY_HEADER_LEN];
    read_exact(&mut stream, &mut header).await?;
    let mut reply = header.to_vec();
    let version = match PreambleReply::decode_header(&header)? {
        0 => {
            let mut body = [0u8; 2];
            read_exact(&mut stream, &mut body).await?;
            reply.extend_from_slice(&body);
            match PreambleReply::decode_ok(&body)? {
                PreambleReply::Ok(v) if SUPPORTED_VERSIONS.contains(&v) => v,
                _ => {
                    return Err(TransferError::ProtocolViolation(
                        "unsupported version selected",
                    ));
                }
            }
        }
        1 => {
            let mut count = [0u8; 1];
            read_exact(&mut stream, &mut count).await?;
            let mut body = vec![0u8; PreambleReply::no_common_body_len(count[0])?];
            read_exact(&mut stream, &mut body).await?;
            let PreambleReply::NoCommonVersion(peer) = PreambleReply::decode_no_common(&body)?
            else {
                return Err(TransferError::ProtocolViolation("bad preamble reply"));
            };
            return Err(TransferError::IncompatibleVersion { peer });
        }
        _ => return Err(TransferError::Busy),
    };
    let prologue = Prologue::new(&preamble, &reply)?;

    // Noise XX: msg1, msg2.
    let (state, msg1) = Initiator::start(local.secret(), &prologue, random::sas_nonce()?)?;
    write_all(&mut stream, &frame_handshake(&msg1)?).await?;
    let msg2 = read_handshake_usg(&mut stream).await?;
    let verify = state
        .read_msg2(&msg2)
        .map_err(|e| ProtocolError::to_transfer_error(&e))?;

    // Verify the responder before revealing ourselves.
    let fp = verify.remote_fingerprint();
    if !params.expectation.matches(&fp) {
        return Err(TransferError::PeerIdentityMismatch);
    }
    let trust_level = trust_level(trust, &fp)?;
    if params.intent == Intent::Transfer && !trust_level.is_trusted() && !params.allow_unverified {
        return Err(TransferError::UnverifiedPeer);
    }
    let remote_static = verify.remote_static();
    let (done, msg3) = verify.proceed()?;
    write_all(&mut stream, &frame_handshake(&msg3)?).await?;
    let outcome: HandshakeOutcome = done.finish();
    let (sas_hash, sas_n_i, sas_nonce) =
        (outcome.handshake_hash, outcome.n_i, outcome.responder_nonce);

    // Transport phase: Hellos.
    let (r, w) = split(stream);
    let mut reader = RecordReader::new(r, outcome.opener, params.timeouts.idle);
    let mut writer = RecordWriter::new(w, outcome.sealer, params.timeouts.write_stall);
    writer
        .send(&Message::Hello(local_hello(
            local,
            Some(params.intent),
            None,
        )))
        .await?;
    let hello = hello_from(
        reader
            .recv_within(params.timeouts.handshake, Phase::Handshake)
            .await,
        Role::Responder,
    )?;
    let reveal = hello
        .sas_reveal
        .ok_or(TransferError::ProtocolViolation("missing SAS reveal"))?;
    let sas = match outcome_sas(&sas_hash, &sas_n_i, &sas_nonce, &reveal) {
        Ok(code) => code,
        Err(_) => {
            let _ = writer
                .send(&Message::error(ErrorCode::ProtocolViolation))
                .await;
            writer.shutdown().await;
            return Err(TransferError::HandshakeFailed);
        }
    };
    let peer = Peer {
        identity: DeviceIdentity {
            public_key: remote_static,
            fingerprint: fp,
            name: DeviceName::sanitize(&hello.device_name),
        },
        trust: trust_level,
        capabilities: capabilities(version, &hello),
    };
    let view = peer_view(trust, &peer);
    Ok(EstablishedSession {
        peer,
        view,
        intent: params.intent,
        sas,
        handshake_us: elapsed_us(started),
        reader,
        writer,
    })
}

fn outcome_sas(
    h: &[u8; 32],
    n_i: &[u8; 32],
    responder_nonce: &dropumi_protocol::handshake::ResponderNonce,
    reveal: &[u8; 32],
) -> Result<SasCode, ProtocolError> {
    use dropumi_protocol::handshake::ResponderNonce;
    match responder_nonce {
        ResponderNonce::Committed(c) => {
            dropumi_protocol::sas::verify_reveal(c, reveal)?;
            Ok(SasCode::derive(h, n_i, reveal))
        }
        ResponderNonce::Known(n_r) => Ok(SasCode::derive(h, n_i, n_r)),
    }
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Responder options.
#[derive(Debug, Clone, Copy)]
pub struct RespondParams<'a> {
    /// Refuse at the preamble because all session slots are in use.
    pub busy: bool,
    /// Timeouts.
    pub timeouts: &'a Timeouts,
}

/// Accepts a session as the responder.
///
/// `decide` sees the authenticated initiator and its intent before anything
/// about this device beyond its key is sent.
///
/// # Errors
/// `HandshakeFailed` (including an initiator that hung up before msg3),
/// `IncompatibleVersion`, `Busy`, `NotAuthorized`, connection errors, or
/// `Timeout(Handshake)`.
pub async fn respond<S, T, D>(
    stream: S,
    local: &LocalIdentity,
    params: RespondParams<'_>,
    trust: &T,
    decide: D,
) -> Result<EstablishedSession<S>, TransferError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    T: TrustStore,
    D: FnOnce(&PeerInfo, Intent) -> ResponderDecision + Send,
{
    let started = Instant::now();
    match timeout(
        params.timeouts.handshake,
        respond_inner(stream, local, params, trust, decide, started),
    )
    .await
    {
        Ok(r) => r,
        Err(_) => Err(TransferError::Timeout(Phase::Handshake)),
    }
}

async fn respond_inner<S, T, D>(
    mut stream: S,
    local: &LocalIdentity,
    params: RespondParams<'_>,
    trust: &T,
    decide: D,
    started: Instant,
) -> Result<EstablishedSession<S>, TransferError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    T: TrustStore,
    D: FnOnce(&PeerInfo, Intent) -> ResponderDecision + Send,
{
    // Preamble; anything invalid is dropped silently.
    let mut header = [0u8; PREAMBLE_HEADER_LEN];
    read_exact(&mut stream, &mut header).await?;
    let body_len = Preamble::decode_header(&header)?;
    let mut preamble = Vec::with_capacity(PREAMBLE_MAX);
    preamble.extend_from_slice(&header);
    let mut body = vec![0u8; body_len];
    read_exact(&mut stream, &mut body).await?;
    let offered = Preamble::decode_body(&body)?;
    preamble.extend_from_slice(&body);
    if params.busy {
        let _ = write_all(&mut stream, &PreambleReply::Busy.encode()).await;
        return Err(TransferError::Busy);
    }
    let Some(version) = select_version(SUPPORTED_VERSIONS, offered.versions()) else {
        let _ = write_all(
            &mut stream,
            &PreambleReply::NoCommonVersion(SUPPORTED_VERSIONS.to_vec()).encode(),
        )
        .await;
        return Err(TransferError::IncompatibleVersion {
            peer: offered.versions().to_vec(),
        });
    };
    let reply = PreambleReply::Ok(version).encode();
    write_all(&mut stream, &reply).await?;
    let prologue = Prologue::new(&preamble, &reply)?;

    // Noise XX. A peer that hangs up before msg3 failed the handshake.
    let hs_err = |e: TransferError| match e {
        TransferError::ConnectionLost => TransferError::HandshakeFailed,
        other => other,
    };
    let state = Responder::start(local.secret(), &prologue, random::sas_nonce()?)?;
    let msg1 = read_handshake_usg(&mut stream).await.map_err(hs_err)?;
    if msg1.len() != MSG1_LEN {
        return Err(TransferError::HandshakeFailed);
    }
    let (state, msg2) = state
        .read_msg1(&msg1)
        .map_err(|_| TransferError::HandshakeFailed)?;
    write_all(&mut stream, &frame_handshake(&msg2)?).await?;
    let msg3 = read_handshake_usg(&mut stream).await.map_err(hs_err)?;
    let outcome = state
        .read_msg3(&msg3)
        .map_err(|_| TransferError::HandshakeFailed)?
        .finish();
    let info = PeerInfo {
        fingerprint: outcome.remote_fingerprint,
        public_key: outcome.remote_static,
    };
    let reveal = outcome.sas_reveal().ok_or(TransferError::Internal(
        "responder without SAS nonce".into(),
    ))?;
    let sas = outcome.sas(None)?;

    let (r, w) = split(stream);
    let mut reader = RecordReader::new(r, outcome.opener, params.timeouts.idle);
    let mut writer = RecordWriter::new(w, outcome.sealer, params.timeouts.write_stall);
    let hello = hello_from(
        reader
            .recv_within(params.timeouts.handshake, Phase::Handshake)
            .await,
        Role::Initiator,
    )?;
    let intent = hello
        .intent
        .ok_or(TransferError::ProtocolViolation("missing intent"))?;
    let verified = match decide(&info, intent) {
        ResponderDecision::Allow { verified } => verified,
        ResponderDecision::Deny => {
            let _ = writer.send(&Message::error(ErrorCode::NotAuthorized)).await;
            writer.shutdown().await;
            reader.drain(params.timeouts.drain).await;
            return Err(TransferError::NotAuthorized);
        }
        ResponderDecision::Busy => {
            let _ = writer.send(&Message::error(ErrorCode::Busy)).await;
            writer.shutdown().await;
            reader.drain(params.timeouts.drain).await;
            return Err(TransferError::Busy);
        }
    };
    writer
        .send(&Message::Hello(local_hello(local, None, Some(reveal))))
        .await?;
    let peer = Peer {
        identity: DeviceIdentity {
            public_key: info.public_key,
            fingerprint: info.fingerprint,
            name: DeviceName::sanitize(&hello.device_name),
        },
        trust: if verified {
            TrustLevel::Trusted
        } else {
            TrustLevel::Unverified
        },
        capabilities: capabilities(version, &hello),
    };
    let view = peer_view(trust, &peer);
    Ok(EstablishedSession {
        peer,
        view,
        intent,
        sas,
        handshake_us: elapsed_us(started),
        reader,
        writer,
    })
}
