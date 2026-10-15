// SPDX-License-Identifier: MPL-2.0
//! ReceiveFiles: accept sessions, authorize, ask for consent, and store
//! files safely.
//!
//! Every failure path discards the partial file explicitly. A file is
//! reported `STORED` only after its hash matched and the sink committed it
//! without overwriting anything; `CompleteAck` follows the directory sync.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use dropumi_domain::error::{Phase, TransferError};
use dropumi_domain::filename::SafeFileName;
use dropumi_domain::peer::{Intent, TrustLevel};
use dropumi_domain::policy::{
    Authorization, Consent, PolicyMode, TransferPolicy, authorize, consent,
};
use dropumi_domain::transfer::{
    CancelReason, Direction, FileOutcome, Party, RejectReason, TransferEvent, TransferId,
    TransferRequest, TransferSession,
};
use dropumi_protocol::ProtocolError;
use dropumi_protocol::message::{AckStatus, ErrorCode, Message};
use dropumi_protocol::session::{ReceiverEvent, ReceiverMachine};
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::UseCaseError;
use super::admission::{Admission, PreAuthPermit};
use super::send::{PROGRESS_INTERVAL, remote_error};
use crate::channel::{
    EstablishedSession, RecordReader, RecordWriter, RecvError, RespondParams, ResponderDecision,
    respond,
};
use crate::config::{AdmissionLimits, ReceiveConfig, Timeouts};
use crate::error::TransportError;
use crate::events::{AppEvent, EventSink, PeerView, TransferOutcome};
use crate::identity::LocalIdentity;
use crate::ports::{
    Clock, Decision, FileSink, IncomingFile, IncomingOffer, Interaction, Listener, TrustStore,
};

/// Ports shared by every receiving session.
#[derive(Debug)]
pub struct ReceiveDeps<S, T, I, K> {
    /// Where files go.
    pub sink: S,
    /// Trust store.
    pub trust: T,
    /// User interaction.
    pub interaction: I,
    /// Clock.
    pub clock: K,
    /// This device.
    pub identity: LocalIdentity,
    /// Timeouts.
    pub timeouts: Timeouts,
    /// Admission limits.
    pub limits: AdmissionLimits,
    /// Event output.
    pub events: EventSink,
}

/// How the receiver stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiveSummary {
    /// Stopped by the caller (or the listener closed).
    Stopped,
    /// `once` mode: the outcome of the first session that delivered an offer.
    Once(TransferOutcome),
}

/// Cancels running transfers without stopping the receiver: every active
/// one, or one by id. It also keeps active transfer ids unique: an offer
/// that reuses the id of an active transfer is rejected `BUSY`.
#[derive(Debug, Clone, Default)]
pub struct TransferCanceller(Arc<Mutex<CancelState>>);

#[derive(Debug, Default)]
struct CancelState {
    /// Cancelled by `cancel_active`, then replaced, so later sessions are
    /// unaffected.
    all: CancellationToken,
    /// One token per active transfer.
    active: HashMap<TransferId, CancellationToken>,
}

impl TransferCanceller {
    /// Cancels every active transfer (each peer receives `Cancel USER`).
    pub fn cancel_active(&self) {
        let mut state = self.lock();
        state.all.cancel();
        state.all = CancellationToken::new();
    }

    /// Cancels the active transfer `id` (its peer receives `Cancel USER`).
    /// Returns whether such a transfer was active.
    pub fn cancel(&self, id: &TransferId) -> bool {
        match self.lock().active.get(id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    fn current(&self) -> CancellationToken {
        self.lock().all.clone()
    }

    /// Marks `id` active in one step; `None` if a transfer with this id is
    /// already active.
    fn register(&self, id: TransferId) -> Option<ActiveTransfer> {
        let mut state = self.lock();
        if state.active.contains_key(&id) {
            return None;
        }
        let token = CancellationToken::new();
        state.active.insert(id, token.clone());
        Some(ActiveTransfer {
            canceller: self.clone(),
            id,
            token,
        })
    }

    fn lock(&self) -> MutexGuard<'_, CancelState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// An active transfer id; unregistered when dropped.
#[derive(Debug)]
struct ActiveTransfer {
    canceller: TransferCanceller,
    id: TransferId,
    token: CancellationToken,
}

impl Drop for ActiveTransfer {
    fn drop(&mut self) {
        self.canceller.lock().active.remove(&self.id);
    }
}

/// The ways a session can be cancelled locally.
#[derive(Debug, Clone)]
struct SessionCancel {
    /// The receiver is stopping.
    stop: CancellationToken,
    /// The user cancelled every active transfer.
    transfer: CancellationToken,
    /// The user cancelled this transfer (set once its offer arrived).
    this: Option<CancellationToken>,
}

impl SessionCancel {
    async fn cancelled(&self) -> CancelReason {
        let this = async {
            match &self.this {
                Some(token) => token.cancelled().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            () = self.stop.cancelled() => CancelReason::Shutdown,
            () = self.transfer.cancelled() => CancelReason::User,
            () = this => CancelReason::User,
        }
    }
}

/// Why the wait for the user's decision ended early.
enum ConsentInterrupt {
    Cancel(CancelReason),
    Error(ErrorCode),
    Violation(ProtocolError),
    Transport(TransferError),
}

/// What one session produced.
#[derive(Debug)]
struct SessionResult {
    /// Whether the peer delivered an offer.
    reached_offer: bool,
    /// Transfer outcome, if an offer arrived.
    outcome: Option<TransferOutcome>,
}

/// Changes a running receiver: the policy for new sessions, or whether it
/// still accepts connections at all.
#[derive(Debug, Clone)]
pub struct ReceiverControl {
    policy: Arc<watch::Sender<Option<TransferPolicy>>>,
    stop_accepting: CancellationToken,
}

impl ReceiverControl {
    fn new() -> (Self, watch::Receiver<Option<TransferPolicy>>) {
        let (tx, rx) = watch::channel(None);
        let control = Self {
            policy: Arc::new(tx),
            stop_accepting: CancellationToken::new(),
        };
        (control, rx)
    }

    /// Replaces the policy (mode and accept mode) for sessions that connect
    /// from now on. Running sessions keep the authorization and consent
    /// mode they started with. The receiver reports `PolicyChanged`.
    pub fn set_policy(&self, policy: TransferPolicy) {
        self.policy.send_replace(Some(policy));
    }

    /// Stops accepting connections: the listener is closed at once, the
    /// running sessions finish normally (nothing is cancelled), and then the
    /// receiver returns [`ReceiveSummary::Stopped`].
    pub fn stop_accepting(&self) {
        self.stop_accepting.cancel();
    }
}

/// The receive use case.
#[derive(Debug)]
pub struct ReceiveFiles<L, S, T, I, K> {
    listener: L,
    deps: Arc<ReceiveDeps<S, T, I, K>>,
    canceller: TransferCanceller,
    control: ReceiverControl,
    policy_updates: watch::Receiver<Option<TransferPolicy>>,
}

impl<L, S, T, I, K> ReceiveFiles<L, S, T, I, K>
where
    L: Listener,
    S: FileSink + 'static,
    T: TrustStore + 'static,
    I: Interaction + 'static,
    K: Clock + 'static,
{
    /// Builds the use case.
    #[must_use]
    pub fn new(listener: L, deps: ReceiveDeps<S, T, I, K>) -> Self {
        let (control, policy_updates) = ReceiverControl::new();
        Self {
            listener,
            deps: Arc::new(deps),
            canceller: TransferCanceller::default(),
            control,
            policy_updates,
        }
    }

    /// Uses an externally created canceller (so it can be wired up before
    /// the use case exists).
    #[must_use]
    pub fn with_canceller(mut self, canceller: TransferCanceller) -> Self {
        self.canceller = canceller;
        self
    }

    /// A handle that cancels active transfers without stopping the receiver.
    #[must_use]
    pub fn canceller(&self) -> TransferCanceller {
        self.canceller.clone()
    }

    /// A handle that changes the policy or stops accepting connections.
    #[must_use]
    pub fn control(&self) -> ReceiverControl {
        self.control.clone()
    }

    /// Serves sessions until `cancel` fires (or, with `once`, until the
    /// first session that delivered an offer ends).
    ///
    /// # Errors
    /// [`UseCaseError::Transfer`] if accepting connections fails
    /// repeatedly (8 times in a row, with a growing pause between tries).
    pub async fn run(
        self,
        cfg: ReceiveConfig,
        cancel: CancellationToken,
    ) -> Result<ReceiveSummary, UseCaseError> {
        let Self {
            listener,
            deps,
            canceller,
            control,
            mut policy_updates,
        } = self;
        let admission = Admission::new(deps.limits);
        let sessions_cancel = cancel.child_token();
        let mut sessions: JoinSet<SessionResult> = JoinSet::new();
        deps.events
            .emit(AppEvent::Listening {
                addrs: listener.local_addrs(),
                policy: cfg.policy,
            })
            .await;
        // `None` once the receiver stopped accepting.
        let mut listener = Some(listener);
        let mut updates_open = true;
        let mut policy = cfg.policy;
        let mut summary = ReceiveSummary::Stopped;
        // Accept errors are retried with a growing pause; a listener that
        // keeps failing (an invalidated socket, no file descriptors left)
        // ends the receiver instead of spinning.
        let mut accept_failures: u32 = 0;
        let mut accept_paused_until: Option<tokio::time::Instant> = None;
        let mut listener_failed: Option<TransferError> = None;
        loop {
            if listener.is_none() && sessions.is_empty() {
                break;
            }
            let expiry = everyone_expiry(&policy, deps.clock.now().as_secs());
            let resume = accept_paused_until.unwrap_or_else(tokio::time::Instant::now);
            tokio::select! {
                biased;
                () = cancel.cancelled() => break,
                () = control.stop_accepting.cancelled(), if listener.is_some() => {
                    // Dropping the listener closes the socket.
                    listener = None;
                }
                changed = policy_updates.changed(), if updates_open => {
                    if changed.is_err() {
                        updates_open = false;
                        continue;
                    }
                    let update = *policy_updates.borrow_and_update();
                    if let Some(p) = update {
                        policy = p;
                        deps.events.emit(AppEvent::PolicyChanged { policy }).await;
                    }
                }
                Some(joined) = sessions.join_next(), if !sessions.is_empty() => {
                    if let Ok(result) = joined
                        && cfg.once
                        && result.reached_offer
                    {
                        if let Some(o) = result.outcome {
                            summary = ReceiveSummary::Once(o);
                        }
                        break;
                    }
                }
                () = tokio::time::sleep(expiry.unwrap_or(Duration::from_secs(3600))), if expiry.is_some() => {
                    policy = TransferPolicy { mode: PolicyMode::TrustedOnly, ..policy };
                    deps.events.emit(AppEvent::PolicyChanged { policy }).await;
                }
                () = tokio::time::sleep_until(resume), if accept_paused_until.is_some() => {
                    accept_paused_until = None;
                }
                accepted = accept_next(listener.as_mut()),
                    if listener.is_some() && accept_paused_until.is_none() => {
                    let (stream, addr) = match accepted {
                        Ok(a) => {
                            accept_failures = 0;
                            a
                        }
                        Err(e) => {
                            accept_failures = accept_failures.saturating_add(1);
                            tracing::warn!(error = %e, failures = accept_failures, "accept failed");
                            if accept_failures >= MAX_ACCEPT_FAILURES {
                                listener_failed = Some(TransferError::Internal(format!(
                                    "accepting connections failed {accept_failures} times in a row: {e}"
                                )));
                                break;
                            }
                            accept_paused_until =
                                Some(tokio::time::Instant::now() + accept_backoff(accept_failures));
                            continue;
                        }
                    };
                    let Some(permit) = admission.try_preauth(addr.ip()) else {
                        tracing::debug!(%addr, "admission limit reached; dropping connection");
                        continue;
                    };
                    let deps = Arc::clone(&deps);
                    let admission = admission.clone();
                    let token = SessionCancel {
                        stop: sessions_cancel.clone(),
                        transfer: canceller.current(),
                        this: None,
                    };
                    let canceller = canceller.clone();
                    sessions.spawn(async move {
                        serve(stream, deps, policy, permit, admission, token, canceller).await
                    });
                }
            }
        }
        sessions_cancel.cancel();
        let drain = deps.timeouts.drain.saturating_mul(2);
        let _ = timeout(drain, async {
            while sessions.join_next().await.is_some() {}
        })
        .await;
        sessions.abort_all();
        match listener_failed {
            Some(e) => Err(UseCaseError::Transfer(e)),
            None => Ok(summary),
        }
    }
}

/// The next connection, or never once the listener is closed.
async fn accept_next<L: Listener>(
    listener: Option<&mut L>,
) -> Result<(L::Stream, SocketAddr), TransportError> {
    match listener {
        Some(l) => l.accept().await,
        None => std::future::pending().await,
    }
}

/// Consecutive accept errors after which the receiver stops.
const MAX_ACCEPT_FAILURES: u32 = 8;

/// Pause after the `failures`-th consecutive accept error: 10 ms, doubling,
/// at most 1 s.
fn accept_backoff(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(7);
    Duration::from_millis(10)
        .saturating_mul(1 << doublings)
        .min(Duration::from_secs(1))
}

/// Time until an Everyone window expires.
fn everyone_expiry(policy: &TransferPolicy, now: u64) -> Option<Duration> {
    match policy.mode {
        PolicyMode::Everyone { until } => {
            Some(Duration::from_secs(until.as_secs().saturating_sub(now)))
        }
        _ => None,
    }
}

async fn serve<St, S, T, I, K>(
    stream: St,
    deps: Arc<ReceiveDeps<S, T, I, K>>,
    policy: TransferPolicy,
    permit: PreAuthPermit,
    admission: Admission,
    cancel: SessionCancel,
    canceller: TransferCanceller,
) -> SessionResult
where
    St: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: FileSink,
    T: TrustStore,
    I: Interaction,
    K: Clock,
{
    let none = SessionResult {
        reached_offer: false,
        outcome: None,
    };
    let mut session_permit = None;
    let busy = admission.sessions_full();
    let now = deps.clock.now();
    let established = respond(
        stream,
        &deps.identity,
        RespondParams {
            busy,
            timeouts: &deps.timeouts,
        },
        &deps.trust,
        |info, intent| {
            if intent == Intent::Pair {
                return ResponderDecision::Deny;
            }
            let level = match deps.trust.get(&info.fingerprint) {
                Ok(Some(_)) => TrustLevel::Trusted,
                Ok(None) => TrustLevel::Unverified,
                Err(e) => {
                    tracing::warn!(kind = e.kind(), "trust store unavailable; denying");
                    return ResponderDecision::Deny;
                }
            };
            match authorize(&policy, now, level, intent, false) {
                Authorization::Deny(_) => ResponderDecision::Deny,
                Authorization::Allow { verified } => {
                    match admission.try_session(info.fingerprint) {
                        Some(p) => {
                            session_permit = Some(p);
                            ResponderDecision::Allow { verified }
                        }
                        None => ResponderDecision::Busy,
                    }
                }
            }
        },
    )
    .await;
    drop(permit);
    let session = match established {
        Ok(s) => s,
        Err(e) => {
            deps.events
                .emit(AppEvent::SessionClosed {
                    peer: None,
                    code: e.code(),
                })
                .await;
            return none;
        }
    };
    let _session_permit = session_permit;
    deps.events
        .emit(AppEvent::Connected {
            peer: session.view.clone(),
            handshake_us: session.handshake_us,
        })
        .await;
    receive_session(session, &deps, policy, cancel, &canceller).await
}

/// One running transfer (receiver side).
struct RecvRun<'d, St, S: FileSink> {
    reader: RecordReader<ReadHalf<St>>,
    writer: RecordWriter<WriteHalf<St>>,
    machine: ReceiverMachine,
    current: Option<(u32, S::File)>,
    events: &'d EventSink,
    timeouts: &'d Timeouts,
    domain: Option<TransferSession>,
    id: Option<TransferId>,
    last_progress: Option<Instant>,
}

impl<St, S> RecvRun<'_, St, S>
where
    St: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: FileSink,
{
    async fn discard_current(&mut self) {
        if let Some((_, file)) = self.current.take() {
            file.discard().await;
        }
    }

    /// Discards partial data, sends a final message and drains the peer.
    async fn close_with(&mut self, msg: Message) {
        self.discard_current().await;
        let _ = self.writer.send(&msg).await;
        self.writer.shutdown().await;
        self.reader.drain(self.timeouts.drain).await;
    }

    fn fail(&mut self, error: TransferError) -> TransferOutcome {
        if let Some(d) = self.domain.as_mut() {
            let _ = d.apply(TransferEvent::Failed(error.clone()));
        }
        TransferOutcome::Failed {
            transfer_id: self.id,
            error,
        }
    }

    fn cancelled(&mut self, by: Party, reason: CancelReason) -> TransferOutcome {
        if let Some(d) = self.domain.as_mut() {
            let _ = d.apply(TransferEvent::Cancelled { by, reason });
        }
        TransferOutcome::Cancelled {
            transfer_id: self.id,
            by,
            reason,
        }
    }

    async fn progress(&mut self, force: bool) {
        let (Some(d), Some(id)) = (self.domain.as_ref(), self.id) else {
            return;
        };
        let now = Instant::now();
        if force
            || self
                .last_progress
                .is_none_or(|t| now.duration_since(t) >= PROGRESS_INTERVAL)
        {
            self.last_progress = Some(now);
            self.events
                .emit(AppEvent::Progress {
                    transfer_id: id,
                    progress: *d.progress(),
                })
                .await;
        }
    }

    /// Fails the transfer because local storage failed: report the current
    /// file as WRITE_FAILED (if one is open) and cancel with STORAGE.
    async fn storage_failure(
        &mut self,
        index: Option<u32>,
        error: TransferError,
    ) -> TransferOutcome {
        self.discard_current().await;
        if let Some(i) = index
            && let Ok(ack) = self.machine.ack(i, AckStatus::WriteFailed)
        {
            let _ = self.writer.send(&ack).await;
        }
        self.close_with(Message::Cancel {
            reason: CancelReason::Storage,
        })
        .await;
        self.fail(error)
    }
}

async fn receive_session<St, S, T, I, K>(
    session: EstablishedSession<St>,
    deps: &ReceiveDeps<S, T, I, K>,
    policy: TransferPolicy,
    mut cancel: SessionCancel,
    canceller: &TransferCanceller,
) -> SessionResult
where
    St: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: FileSink,
    T: TrustStore,
    I: Interaction,
    K: Clock,
{
    let EstablishedSession {
        peer,
        view,
        sas,
        reader,
        writer,
        ..
    } = session;
    let verified = peer.trust.is_trusted();
    // The session code is always shown, before the offer, so a sender that
    // must confirm it (unverified receiver) has something to compare with.
    deps.events
        .emit(AppEvent::Sas {
            peer: view.clone(),
            code: sas,
        })
        .await;
    let mut run: RecvRun<'_, St, S> = RecvRun {
        reader,
        writer,
        machine: ReceiverMachine::new(),
        current: None,
        events: &deps.events,
        timeouts: &deps.timeouts,
        domain: None,
        id: None,
        last_progress: None,
    };

    // Offer.
    let offer_limit = if verified {
        deps.timeouts.offer_verified
    } else {
        deps.timeouts.offer_unverified
    };
    // One deadline for the whole offer (all batches and ignorable
    // messages), so a peer cannot hold a session slot by trickling frames.
    let offer_deadline = tokio::time::Instant::now() + offer_limit;
    let request = loop {
        let left = offer_deadline.saturating_duration_since(tokio::time::Instant::now());
        let frame = tokio::select! {
            biased;
            reason = cancel.cancelled() => {
                run.close_with(Message::Cancel { reason }).await;
                return closed(deps, &view, "cancelled").await;
            }
            f = run.reader.recv_within(left, Phase::Offer) => f,
        };
        let event = match frame {
            Ok(f) => run.machine.on_frame(f).map_err(RecvError::Protocol),
            Err(e) => Err(e),
        };
        match event {
            Ok(ReceiverEvent::OfferReady(req)) => break req,
            Ok(ReceiverEvent::Pending | ReceiverEvent::Ignored) => {}
            Ok(ReceiverEvent::PeerCancel(_)) => {
                return closed(deps, &view, "cancelled").await;
            }
            Ok(ReceiverEvent::PeerError(code)) => {
                return closed(deps, &view, remote_error(code).code()).await;
            }
            Ok(_) => {
                run.close_with(Message::error(ErrorCode::ProtocolViolation))
                    .await;
                return closed(deps, &view, "protocol_violation").await;
            }
            Err(RecvError::Protocol(p)) => {
                run.close_with(Message::error(p.wire_code())).await;
                return closed(deps, &view, p.to_transfer_error().code()).await;
            }
            Err(RecvError::Transport(e)) => return closed(deps, &view, e.code()).await,
        }
    };
    let id = request.id();
    // Active transfer ids are unique, so a cancel or answer by id is never
    // ambiguous. A reused id is rejected before anyone is asked.
    let Some(active) = canceller.register(id) else {
        if let Ok(m) = run.machine.reject(RejectReason::Busy) {
            let _ = run.writer.send(&m).await;
        }
        run.writer.shutdown().await;
        run.reader.drain(deps.timeouts.drain).await;
        return closed(deps, &view, TransferError::Busy.code()).await;
    };
    cancel.this = Some(active.token.clone());
    run.id = Some(id);
    let mut domain = TransferSession::new(
        Direction::Receive,
        peer.identity.fingerprint,
        request.clone(),
    );
    let _ = domain.apply(TransferEvent::OfferReceived);
    run.domain = Some(domain);
    deps.events
        .emit(AppEvent::OfferReceived {
            transfer_id: id,
            peer: view.clone(),
            sas: (!verified).then_some(sas),
            files: request.files().to_vec(),
            total_bytes: request.total_size(),
        })
        .await;

    // Consent.
    let decision = match consent(verified, policy.accept) {
        Consent::AutoAccept => Decision::Accept,
        Consent::AutoReject => Decision::Reject(RejectReason::Declined),
        Consent::Ask => {
            if let Some(d) = run.domain.as_mut() {
                let _ = d.apply(TransferEvent::ConsentRequested);
            }
            let offer = IncomingOffer {
                transfer_id: id,
                peer: view.clone(),
                sas: (!verified).then_some(sas),
                files: request.files().to_vec(),
                total_bytes: request.total_size(),
            };
            let decision = timeout(deps.timeouts.consent, deps.interaction.decide_offer(&offer));
            tokio::pin!(decision);
            let wait_deadline = tokio::time::Instant::now()
                + deps.timeouts.consent.saturating_add(Duration::from_secs(5));
            let decided = loop {
                let left = wait_deadline.saturating_duration_since(tokio::time::Instant::now());
                let interrupt = tokio::select! {
                    biased;
                    reason = cancel.cancelled() => break Err(reason),
                    d = &mut decision => {
                        break Ok(d.unwrap_or(Decision::Reject(RejectReason::Timeout)));
                    }
                    f = run.reader.recv_within(left, Phase::Consent) => {
                        // While the user decides, only Cancel, Error and
                        // ignorable messages (bounded by the machine) may arrive.
                        match f {
                            Ok(frame) => match run.machine.on_frame(frame) {
                                Ok(ReceiverEvent::Ignored) => None,
                                Ok(ReceiverEvent::PeerCancel(r)) => Some(ConsentInterrupt::Cancel(r)),
                                Ok(ReceiverEvent::PeerError(c)) => Some(ConsentInterrupt::Error(c)),
                                Ok(_) => Some(ConsentInterrupt::Violation(
                                    ProtocolError::Violation("message during consent"),
                                )),
                                Err(p) => Some(ConsentInterrupt::Violation(p)),
                            },
                            Err(e) => Some(ConsentInterrupt::Transport(e.into())),
                        }
                    }
                };
                let Some(interrupt) = interrupt else { continue };
                let outcome = match interrupt {
                    ConsentInterrupt::Cancel(reason) => run.cancelled(Party::Peer, reason),
                    ConsentInterrupt::Error(code) => run.fail(remote_error(code)),
                    ConsentInterrupt::Violation(p) => {
                        run.close_with(Message::error(p.wire_code())).await;
                        run.fail(p.to_transfer_error())
                    }
                    ConsentInterrupt::Transport(e) => run.fail(e),
                };
                return finished(deps, outcome).await;
            };
            match decided {
                Ok(d) => d,
                Err(reason) => {
                    run.close_with(Message::Cancel { reason }).await;
                    let o = run.cancelled(Party::Local, reason);
                    return finished(deps, o).await;
                }
            }
        }
    };
    match decision {
        Decision::Reject(reason) => {
            if let Ok(m) = run.machine.reject(reason) {
                let _ = run.writer.send(&m).await;
            }
            run.writer.shutdown().await;
            run.reader.drain(deps.timeouts.drain).await;
            if let Some(d) = run.domain.as_mut() {
                let _ = d.apply(TransferEvent::Rejected(reason));
            }
            return finished(
                deps,
                TransferOutcome::Rejected {
                    transfer_id: id,
                    reason,
                },
            )
            .await;
        }
        Decision::Accept => match run.machine.accept() {
            Ok(m) => {
                if let Err(e) = run.writer.send(&m).await {
                    let o = run.fail(e);
                    return finished(deps, o).await;
                }
            }
            Err(p) => {
                let o = run.fail(p.to_transfer_error());
                return finished(deps, o).await;
            }
        },
    }
    if let Some(d) = run.domain.as_mut() {
        let _ = d.apply(TransferEvent::Accepted);
    }
    deps.events
        .emit(AppEvent::Accepted { transfer_id: id })
        .await;
    run.progress(true).await;

    let outcome = transfer_loop(&mut run, deps, &request, &cancel).await;
    if outcome.is_success()
        && verified
        && let Err(e) = deps
            .trust
            .touch(&peer.identity.fingerprint, deps.clock.now())
    {
        tracing::debug!(kind = e.kind(), "could not record last_seen");
    }
    finished(deps, outcome).await
}

async fn transfer_loop<St, S, T, I, K>(
    run: &mut RecvRun<'_, St, S>,
    deps: &ReceiveDeps<S, T, I, K>,
    request: &TransferRequest,
    cancel: &SessionCancel,
) -> TransferOutcome
where
    St: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: FileSink,
    T: TrustStore,
    I: Interaction,
    K: Clock,
{
    let started = Instant::now();
    let mut bytes: u64 = 0;
    loop {
        let frame = tokio::select! {
            biased;
            reason = cancel.cancelled() => {
                run.close_with(Message::Cancel { reason }).await;
                return run.cancelled(Party::Local, reason);
            }
            f = run.reader.recv(Phase::Idle) => f,
        };
        let event = match frame {
            Ok(f) => run.machine.on_frame(f),
            Err(RecvError::Transport(e)) => {
                run.discard_current().await;
                return run.fail(e);
            }
            Err(RecvError::Protocol(p)) => {
                run.close_with(Message::error(p.wire_code())).await;
                return run.fail(p.to_transfer_error());
            }
        };
        let event = match event {
            Ok(e) => e,
            Err(p) => {
                run.close_with(Message::error(p.wire_code())).await;
                return run.fail(p.to_transfer_error());
            }
        };
        match event {
            ReceiverEvent::FileStarted { index, size } => {
                let name = request
                    .files()
                    .get(index as usize)
                    .map(|f| f.name.as_str())
                    .unwrap_or_default();
                let safe = SafeFileName::sanitize(name);
                if let Some(d) = run.domain.as_mut() {
                    let _ = d.apply(TransferEvent::FileStarted(index));
                }
                match deps.sink.begin(&safe, size).await {
                    Ok(file) => run.current = Some((index, file)),
                    Err(e) => return run.storage_failure(Some(index), e.into()).await,
                }
            }
            ReceiverEvent::Data(data) => {
                let len = u64::try_from(data.len()).unwrap_or(u64::MAX);
                // The write borrows the reader's buffer; copy-free.
                let result = match run.current.as_mut() {
                    Some((_, file)) => file.write(data).await,
                    None => Err(crate::error::StorageError::Other("no open file".into())),
                };
                if let Err(e) = result {
                    let index = run.current.as_ref().map(|(i, _)| *i);
                    return run.storage_failure(index, e.into()).await;
                }
                bytes = bytes.saturating_add(len);
                if let Some(d) = run.domain.as_mut() {
                    let _ = d.record_bytes(len);
                }
                run.progress(false).await;
            }
            ReceiverEvent::FileFinished {
                index,
                hash_ok,
                digest,
            } => {
                let Some((_, file)) = run.current.take() else {
                    return run.fail(TransferError::Internal("finished file was not open".into()));
                };
                let entry = request.files().get(index as usize);
                if hash_ok {
                    match file.commit().await {
                        Ok(saved) => {
                            if let Err(e) = send_ack(run, index, AckStatus::Stored).await {
                                return run.fail(e);
                            }
                            if let Some(d) = run.domain.as_mut() {
                                let _ = d.apply(TransferEvent::FileSettled(
                                    index,
                                    FileOutcome::Stored {
                                        saved_as: saved.saved_as.clone(),
                                    },
                                ));
                            }
                            deps.events
                                .emit(AppEvent::FileSaved {
                                    transfer_id: request.id(),
                                    index,
                                    name: entry.map(|e| e.name.clone()).unwrap_or_default(),
                                    saved_as: saved.saved_as,
                                    size: entry.map_or(0, |e| e.size),
                                    blake3: hex(&digest),
                                })
                                .await;
                        }
                        Err(e) => {
                            // The pending ack must still be sent before cancelling.
                            if let Ok(ack) = run.machine.ack(index, AckStatus::WriteFailed) {
                                let _ = run.writer.send(&ack).await;
                            }
                            return run.storage_failure(None, e.into()).await;
                        }
                    }
                } else {
                    file.discard().await;
                    if let Err(e) = send_ack(run, index, AckStatus::IntegrityFailed).await {
                        return run.fail(e);
                    }
                    if let Some(d) = run.domain.as_mut() {
                        let _ = d.apply(TransferEvent::FileSettled(
                            index,
                            FileOutcome::IntegrityFailed,
                        ));
                    }
                    deps.events
                        .emit(AppEvent::FileFailed {
                            transfer_id: request.id(),
                            index,
                            status: AckStatus::IntegrityFailed,
                        })
                        .await;
                }
            }
            ReceiverEvent::FileAborted { index } => {
                if run.current.as_ref().is_some_and(|(i, _)| *i == index) {
                    run.discard_current().await;
                }
                if let Err(e) = send_ack(run, index, AckStatus::Aborted).await {
                    return run.fail(e);
                }
                if let Some(d) = run.domain.as_mut() {
                    let _ = d.apply(TransferEvent::FileSettled(index, FileOutcome::Aborted));
                }
                deps.events
                    .emit(AppEvent::FileFailed {
                        transfer_id: request.id(),
                        index,
                        status: AckStatus::Aborted,
                    })
                    .await;
            }
            ReceiverEvent::CompleteRequested => {
                if let Err(e) = deps.sink.sync_dir().await {
                    return run.storage_failure(None, e.into()).await;
                }
                let ack = match run.machine.complete_ack() {
                    Ok(m) => m,
                    Err(p) => {
                        run.close_with(Message::error(p.wire_code())).await;
                        return run.fail(p.to_transfer_error());
                    }
                };
                if let Err(e) = run.writer.send(&ack).await {
                    return run.fail(e);
                }
                // The initiator closes first.
                run.reader.drain(deps.timeouts.drain).await;
                run.writer.shutdown().await;
                if let Some(d) = run.domain.as_mut() {
                    let _ = d.apply(TransferEvent::Finalizing);
                    let _ = d.apply(TransferEvent::Finished);
                }
                run.progress(true).await;
                let stored = run.machine.stored_count();
                let failed = request.file_count().saturating_sub(stored);
                let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                return if failed == 0 {
                    TransferOutcome::Completed {
                        transfer_id: request.id(),
                        files_stored: stored,
                        bytes,
                        elapsed_ms,
                    }
                } else {
                    TransferOutcome::CompletedWithFailures {
                        transfer_id: request.id(),
                        files_stored: stored,
                        failed,
                        bytes,
                        elapsed_ms,
                    }
                };
            }
            ReceiverEvent::PeerCancel(reason) => {
                run.discard_current().await;
                return run.cancelled(Party::Peer, reason);
            }
            ReceiverEvent::PeerError(code) => {
                run.discard_current().await;
                return run.fail(remote_error(code));
            }
            ReceiverEvent::Pending | ReceiverEvent::Ignored => {}
            ReceiverEvent::OfferReady(_) => {
                run.close_with(Message::error(ErrorCode::ProtocolViolation))
                    .await;
                return run.fail(TransferError::ProtocolViolation("second offer"));
            }
            _ => {}
        }
    }
}

async fn send_ack<St, S>(
    run: &mut RecvRun<'_, St, S>,
    index: u32,
    status: AckStatus,
) -> Result<(), TransferError>
where
    St: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: FileSink,
{
    let ack = run.machine.ack(index, status)?;
    run.writer.send(&ack).await
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

async fn closed<S, T, I, K>(
    deps: &ReceiveDeps<S, T, I, K>,
    view: &PeerView,
    code: &'static str,
) -> SessionResult {
    deps.events
        .emit(AppEvent::SessionClosed {
            peer: Some(view.clone()),
            code,
        })
        .await;
    SessionResult {
        reached_offer: false,
        outcome: None,
    }
}

async fn finished<S, T, I, K>(
    deps: &ReceiveDeps<S, T, I, K>,
    outcome: TransferOutcome,
) -> SessionResult {
    deps.events.emit(AppEvent::Finished(outcome.clone())).await;
    SessionResult {
        reached_offer: true,
        outcome: Some(outcome),
    }
}
