// SPDX-License-Identifier: MPL-2.0
//! PairDevice: mutual trust after both users confirm the same code.
//!
//! Both sides show the committed SAS, each sends `PairResult` when its user
//! answers, and trust is stored only if both confirmed. Nothing is stored on
//! any other path (no trust on first use).

use std::time::Duration;

use dropumi_domain::error::{Phase, TransferError};
use dropumi_domain::peer::Intent;
use dropumi_domain::trust::{PairingMethod, TrustedDevice};
use dropumi_protocol::frame::Frame;
use dropumi_protocol::limits::MAX_CONSECUTIVE_IGNORABLE;
use dropumi_protocol::message::{Decoded, Message, Role, decode};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::UseCaseError;
use super::connect::{ResolveParams, dial, resolve};
use super::send::remote_error;
use crate::channel::{
    EstablishedSession, InitiateParams, RecvError, RespondParams, ResponderDecision, initiate,
    respond,
};
use crate::config::Timeouts;
use crate::events::{AppEvent, EventSink, PeerView};
use crate::identity::LocalIdentity;
use crate::ports::{
    Clock, Connector, Discovery, Interaction, Listener, SasPrompt, SasPurpose, TrustStore,
};

/// How long past the code timeout the peer is still read while the local
/// prompt is open (the prompt's own timeout ends it first).
const PROMPT_READ_GRACE: Duration = Duration::from_secs(5);

/// How pairing ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairOutcome {
    /// Both users confirmed; the peer is now trusted.
    Paired(PeerView),
    /// Pairing failed; nothing was stored.
    Failed(TransferError),
    /// The local user cancelled; nothing was stored.
    Cancelled,
}

/// Shared ports for pairing.
#[derive(Debug)]
pub struct PairDeps<'a, T, I, K> {
    /// Trust store.
    pub trust: &'a T,
    /// User interaction.
    pub interaction: &'a I,
    /// Clock.
    pub clock: &'a K,
    /// This device.
    pub identity: &'a LocalIdentity,
    /// Timeouts.
    pub timeouts: Timeouts,
    /// Event output.
    pub events: EventSink,
}

/// Pairs with a target (initiator side).
///
/// # Errors
/// [`UseCaseError`] for invalid targets or configuration failures.
pub async fn pair_with<C, D, T, I, K>(
    connector: &C,
    discovery: Option<&D>,
    deps: &PairDeps<'_, T, I, K>,
    params: ResolveParams<'_>,
    cancel: CancellationToken,
) -> Result<PairOutcome, UseCaseError>
where
    C: Connector,
    D: Discovery,
    T: TrustStore,
    I: Interaction,
    K: Clock,
{
    let resolution = resolve(connector, discovery, deps.trust, params).await?;
    let (stream, _) = match dial(
        connector,
        &resolution.candidates,
        deps.timeouts.connect,
        &deps.events,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => return Ok(PairOutcome::Failed(e)),
    };
    let session = match initiate(
        stream,
        deps.identity,
        InitiateParams {
            intent: Intent::Pair,
            expectation: &resolution.expectation,
            allow_unverified: true,
            timeouts: &deps.timeouts,
        },
        deps.trust,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => return Ok(PairOutcome::Failed(e)),
    };
    Ok(exchange(session, deps, Role::Responder, cancel).await)
}

/// Waits up to `window` for one pairing attempt (responder side). The
/// window closes after any attempt.
///
/// # Errors
/// [`UseCaseError::Transfer`] if accepting fails.
pub async fn pair_listen<L, T, I, K>(
    mut listener: L,
    deps: &PairDeps<'_, T, I, K>,
    window: Duration,
    cancel: CancellationToken,
) -> Result<PairOutcome, UseCaseError>
where
    L: Listener,
    T: TrustStore,
    I: Interaction,
    K: Clock,
{
    let accepted = tokio::select! {
        biased;
        () = cancel.cancelled() => return Ok(PairOutcome::Cancelled),
        r = timeout(window, listener.accept()) => r,
    };
    let (stream, _addr) = match accepted {
        Err(_) => return Ok(PairOutcome::Failed(TransferError::Timeout(Phase::Sas))),
        Ok(Err(e)) => return Err(UseCaseError::Transfer(e.into())),
        Ok(Ok(s)) => s,
    };
    let trust = deps.trust;
    let session = match respond(
        stream,
        deps.identity,
        RespondParams {
            busy: false,
            timeouts: &deps.timeouts,
        },
        deps.trust,
        |info, intent| match intent {
            Intent::Pair => ResponderDecision::Allow {
                verified: trust.get(&info.fingerprint).ok().flatten().is_some(),
            },
            Intent::Transfer => ResponderDecision::Deny,
        },
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            deps.events
                .emit(AppEvent::SessionClosed {
                    peer: None,
                    code: e.code(),
                })
                .await;
            return Ok(PairOutcome::Failed(e));
        }
    };
    Ok(exchange(session, deps, Role::Initiator, cancel).await)
}

async fn exchange<S, T, I, K>(
    session: EstablishedSession<S>,
    deps: &PairDeps<'_, T, I, K>,
    peer_role: Role,
    cancel: CancellationToken,
) -> PairOutcome
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    T: TrustStore,
    I: Interaction,
    K: Clock,
{
    let EstablishedSession {
        peer,
        view,
        sas,
        mut reader,
        mut writer,
        handshake_us,
        ..
    } = session;
    deps.events
        .emit(AppEvent::Connected {
            peer: view.clone(),
            handshake_us,
        })
        .await;
    deps.events
        .emit(AppEvent::Sas {
            peer: view.clone(),
            code: sas,
        })
        .await;
    let prompt = SasPrompt {
        peer: view.clone(),
        code: sas,
        purpose: SasPurpose::Pairing,
    };
    // While the local user decides, the peer is read too: its answer is
    // kept, and its "no", Cancel, Error or departure ends the prompt at once
    // (the prompt is dropped, which withdraws it). Otherwise this side would
    // write its answer to a peer that is gone, and on Windows the reset that
    // follows discards the peer's unread "no".
    let mut ignored = 0usize;
    let mut remote: Option<bool> = None;
    let asked = {
        let confirm = timeout(deps.timeouts.sas, deps.interaction.confirm_sas(&prompt));
        tokio::pin!(confirm);
        let deadline =
            tokio::time::Instant::now() + deps.timeouts.sas.saturating_add(PROMPT_READ_GRACE);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            tokio::select! {
                biased;
                () = cancel.cancelled() => break Asked::Cancelled,
                r = &mut confirm => break Asked::Answered(r.unwrap_or(false)),
                f = reader.recv_within(left, Phase::Sas) => match peer_step(f, peer_role, &mut ignored) {
                    PeerStep::Ignored => {}
                    PeerStep::Answer(confirmed) if remote.is_none() => {
                        remote = Some(confirmed);
                        if !confirmed {
                            break Asked::PeerSaidNo;
                        }
                        // A known message ends a run of ignorable ones (§8.4).
                        ignored = 0;
                    }
                    PeerStep::Answer(_) => {
                        break Asked::PeerFailed(TransferError::ProtocolViolation("second PairResult"));
                    }
                    PeerStep::Cancel => break Asked::PeerCancelled,
                    PeerStep::Failed(e) => break Asked::PeerFailed(e),
                },
            }
        }
    };
    let local = match asked {
        Asked::Answered(local) => local,
        Asked::Cancelled => {
            let _ = writer.send(&Message::PairResult { confirmed: false }).await;
            writer.shutdown().await;
            reader.drain(deps.timeouts.drain).await;
            return PairOutcome::Cancelled;
        }
        Asked::PeerSaidNo => {
            // The withdrawn prompt counts as this side's "no".
            let _ = writer.send(&Message::PairResult { confirmed: false }).await;
            writer.shutdown().await;
            reader.drain(deps.timeouts.drain).await;
            return PairOutcome::Failed(TransferError::PairingRejected);
        }
        Asked::PeerCancelled => {
            // A Cancel gets no reply (§16.1); it still counts as "no".
            writer.shutdown().await;
            reader.drain(deps.timeouts.drain).await;
            return PairOutcome::Failed(TransferError::PairingRejected);
        }
        Asked::PeerFailed(e) => {
            writer.shutdown().await;
            reader.drain(deps.timeouts.drain).await;
            return PairOutcome::Failed(e);
        }
    };
    if let Err(e) = writer.send(&Message::PairResult { confirmed: local }).await {
        return PairOutcome::Failed(e);
    }
    // Wait for the peer's answer unless it came during the prompt: ignorable
    // messages are skipped (bounded), and the local user can still cancel.
    let deadline = tokio::time::Instant::now() + deps.timeouts.sas;
    let remote = loop {
        if let Some(r) = remote {
            break Ok(r);
        }
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        let frame = tokio::select! {
            biased;
            () = cancel.cancelled() => break Err(None),
            f = reader.recv_within(left, Phase::Sas) => f,
        };
        match peer_step(frame, peer_role, &mut ignored) {
            PeerStep::Ignored => {}
            PeerStep::Answer(r) => remote = Some(r),
            PeerStep::Cancel => remote = Some(false),
            PeerStep::Failed(e) => break Err(Some(e)),
        }
    };
    writer.shutdown().await;
    reader.drain(deps.timeouts.drain).await;
    let remote = match remote {
        Ok(r) => r,
        Err(None) => return PairOutcome::Cancelled,
        Err(Some(e)) => return PairOutcome::Failed(e),
    };
    if !(local && remote) {
        return PairOutcome::Failed(TransferError::PairingRejected);
    }
    let device = TrustedDevice {
        fingerprint: peer.identity.fingerprint,
        public_key: Some(peer.identity.public_key),
        alias: Some(view.claimed_name.clone()),
        method: PairingMethod::Sas,
        added_at: deps.clock.now(),
        last_seen: Some(deps.clock.now()),
    };
    if let Err(e) = deps.trust.upsert(device) {
        return PairOutcome::Failed(e.into());
    }
    let paired = PeerView {
        trusted: true,
        alias: Some(view.claimed_name.clone()),
        ..view
    };
    deps.events
        .emit(AppEvent::Paired {
            peer: paired.clone(),
        })
        .await;
    PairOutcome::Paired(paired)
}

/// How the local code prompt ended.
enum Asked {
    /// The local user answered (a timeout is a "no").
    Answered(bool),
    /// The local user cancelled.
    Cancelled,
    /// The peer said "no" before the local user answered.
    PeerSaidNo,
    /// The peer cancelled before the local user answered.
    PeerCancelled,
    /// The peer sent an error, broke the protocol or left.
    PeerFailed(TransferError),
}

/// One message from the peer during pairing.
enum PeerStep {
    /// An ignorable message, within the limit.
    Ignored,
    /// The peer's PairResult.
    Answer(bool),
    /// The peer cancelled (counts as "no").
    Cancel,
    /// An error, a violation or the end of the session.
    Failed(TransferError),
}

fn peer_step(
    frame: Result<Frame<'_>, RecvError>,
    peer_role: Role,
    ignored: &mut usize,
) -> PeerStep {
    match frame {
        Ok(Frame::Control(body)) => match decode(body, peer_role) {
            Ok(Decoded::Message(Message::PairResult { confirmed })) => PeerStep::Answer(confirmed),
            Ok(Decoded::Message(Message::Error { code, .. })) => {
                PeerStep::Failed(remote_error(code))
            }
            Ok(Decoded::Message(Message::Cancel { .. })) => PeerStep::Cancel,
            Ok(Decoded::Ignored(_)) if *ignored < MAX_CONSECUTIVE_IGNORABLE => {
                *ignored = ignored.saturating_add(1);
                PeerStep::Ignored
            }
            Ok(Decoded::Ignored(_)) => PeerStep::Failed(TransferError::LimitExceeded(
                "consecutive ignorable messages",
            )),
            Ok(_) => PeerStep::Failed(TransferError::ProtocolViolation("expected PairResult")),
            Err(p) => PeerStep::Failed(p.to_transfer_error()),
        },
        Ok(Frame::Data(_)) => {
            PeerStep::Failed(TransferError::ProtocolViolation("data during pairing"))
        }
        Err(e) => PeerStep::Failed(e.into()),
    }
}
