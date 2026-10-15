// SPDX-License-Identifier: MPL-2.0
//! SendFiles: offer files to one receiver and stream them.
//!
//! Success is reported only after the receiver's `CompleteAck` with every
//! file `STORED` (verified, synced and visible under its final name).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use dropumi_domain::error::{Phase, TransferError};
use dropumi_domain::peer::Intent;
use dropumi_domain::transfer::{
    AbortReason, CancelReason, Direction, FileEntry, FileOutcome, Party, TransferEvent, TransferId,
    TransferRequest, TransferSession,
};
use dropumi_protocol::frame::Frame;
use dropumi_protocol::limits::MAX_FRAME_BODY;
use dropumi_protocol::message::{AckStatus, ErrorCode, Message};
use dropumi_protocol::random;
use dropumi_protocol::session::{SenderEvent, SenderMachine};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{sleep, sleep_until, timeout};
use tokio_util::sync::CancellationToken;

use super::UseCaseError;
use super::connect::{ResolveParams, dial, resolve};
use crate::channel::{InitiateParams, RecordReader, RecordWriter, RecvError, initiate};
use crate::config::{SendRequest, Timeouts};
use crate::events::{AppEvent, EventSink, PeerView, TransferOutcome};
use crate::identity::LocalIdentity;
use crate::ports::{
    Clock, Connector, Discovery, FileSource, Interaction, SasPrompt, SasPurpose, SourceMeta,
    TrustStore,
};

/// How often progress events are emitted at most.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// Inbound traffic forwarded by the reader task.
#[derive(Debug)]
enum Inbound {
    Control(Vec<u8>),
    Failed(RecvError),
}

/// Reads records until the stream ends and forwards control messages.
fn spawn_reader<R>(mut reader: RecordReader<R>) -> (mpsc::Receiver<Inbound>, JoinHandle<()>)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (tx, rx) = mpsc::channel(8);
    let handle = tokio::spawn(async move {
        loop {
            // The sender's liveness comes from write stalls and the decision
            // and completion deadlines, not from receiver chatter.
            let msg = match reader
                .recv_within(Duration::from_secs(86_400), Phase::Idle)
                .await
            {
                Ok(Frame::Control(body)) => Inbound::Control(body.to_vec()),
                Ok(Frame::Data(_)) => Inbound::Failed(RecvError::Protocol(
                    dropumi_protocol::ProtocolError::Violation("data from the receiver"),
                )),
                Err(e) => Inbound::Failed(e),
            };
            let stop = matches!(msg, Inbound::Failed(_));
            if tx.send(msg).await.is_err() || stop {
                break;
            }
        }
    });
    (rx, handle)
}

/// Aborts the reader task when the transfer future ends or is dropped, so
/// the connection never outlives the use case.
#[derive(Debug)]
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Emits progress at most every [`PROGRESS_INTERVAL`].
#[derive(Debug, Default)]
struct ProgressTicker {
    last: Option<Instant>,
}

impl ProgressTicker {
    fn due(&mut self) -> bool {
        let now = Instant::now();
        match self.last {
            Some(t) if now.duration_since(t) < PROGRESS_INTERVAL => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

/// Dependencies of [`SendFiles`].
#[derive(Debug)]
pub struct SendFiles<'a, C, D, F, T, I, K> {
    connector: &'a C,
    discovery: Option<&'a D>,
    source: &'a F,
    trust: &'a T,
    interaction: &'a I,
    clock: &'a K,
    identity: &'a LocalIdentity,
    timeouts: Timeouts,
    events: EventSink,
}

/// Port bundle for [`SendFiles::new`].
#[derive(Debug)]
pub struct SendDeps<'a, C, D, F, T, I, K> {
    /// Outgoing connections.
    pub connector: &'a C,
    /// Discovery (absent when built or run without it).
    pub discovery: Option<&'a D>,
    /// Source files.
    pub source: &'a F,
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

struct Prepared {
    request: TransferRequest,
    paths: Vec<PathBuf>,
    metas: Vec<SourceMeta>,
    offer: Vec<Message>,
    machine: SenderMachine,
}

impl<'a, C, D, F, T, I, K> SendFiles<'a, C, D, F, T, I, K>
where
    C: Connector,
    D: Discovery,
    F: FileSource,
    T: TrustStore,
    I: Interaction,
    K: Clock,
{
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(deps: SendDeps<'a, C, D, F, T, I, K>) -> Self {
        Self {
            connector: deps.connector,
            discovery: deps.discovery,
            source: deps.source,
            trust: deps.trust,
            interaction: deps.interaction,
            clock: deps.clock,
            identity: deps.identity,
            timeouts: deps.timeouts,
            events: deps.events,
        }
    }

    /// Sends `req.paths` to the target. Returns the transfer outcome; only
    /// input, resolution and configuration problems are errors.
    ///
    /// Cancelling `cancel` while a name or fingerprint target is still being
    /// browsed for, or while dialling, returns
    /// [`TransferOutcome::Cancelled`] at once.
    ///
    /// # Errors
    /// [`UseCaseError`] before any connection is attempted.
    pub async fn run(
        &self,
        req: SendRequest,
        cancel: CancellationToken,
    ) -> Result<TransferOutcome, UseCaseError> {
        let prepared = self.prepare(&req.paths).await?;
        let id = prepared.request.id();
        let cancelled = || TransferOutcome::Cancelled {
            transfer_id: Some(id),
            by: Party::Local,
            reason: CancelReason::User,
        };
        // The browse for a name or fingerprint target can last as long as the
        // discover timeout, so it must not delay a Ctrl-C.
        let resolution = tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(cancelled()),
            r = resolve(
                self.connector,
                self.discovery,
                self.trust,
                ResolveParams {
                    targets: &req.targets,
                    explicit: req.expect.clone(),
                    discover_timeout: req.discover_timeout,
                    allow_loopback: req.allow_loopback,
                },
            ) => r?,
        };
        let fail = |error| TransferOutcome::Failed {
            transfer_id: Some(id),
            error,
        };
        let (stream, _addr) = tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(cancelled()),
            r = dial(self.connector, &resolution.candidates, self.timeouts.connect, &self.events) => match r {
                Ok(s) => s,
                Err(e) => return Ok(fail(e)),
            },
        };
        let session = match initiate(
            stream,
            self.identity,
            InitiateParams {
                intent: Intent::Transfer,
                expectation: &resolution.expectation,
                allow_unverified: req.allow_unverified,
                timeouts: &self.timeouts,
            },
            self.trust,
        )
        .await
        {
            Ok(s) => s,
            Err(e) => return Ok(fail(e)),
        };
        self.events
            .emit(AppEvent::Connected {
                peer: session.view.clone(),
                handshake_us: session.handshake_us,
            })
            .await;
        Ok(self
            .transfer(session, prepared, req.throttle_bps, cancel)
            .await)
    }

    async fn prepare(&self, paths: &[PathBuf]) -> Result<Prepared, UseCaseError> {
        if paths.is_empty() {
            return Err(UseCaseError::Usage("no files given".into()));
        }
        let mut metas = Vec::with_capacity(paths.len());
        for p in paths {
            let m = self
                .source
                .meta(p)
                .await
                .map_err(|e| UseCaseError::Usage(e.to_string()))?;
            metas.push(m);
        }
        let entries = metas
            .iter()
            .map(|m| FileEntry {
                name: m.name.clone(),
                size: m.size,
                media_type: None,
                modified: m.modified,
            })
            .collect();
        let id = random::transfer_id().map_err(|e| UseCaseError::Transfer(e.into()))?;
        let request = TransferRequest::new(id, entries)
            .map_err(|e| UseCaseError::Usage(format!("invalid transfer: {e}")))?;
        let mut machine = SenderMachine::new(request.clone());
        let offer = machine
            .offer()
            .map_err(|e| UseCaseError::Usage(format!("cannot offer these files: {e}")))?;
        Ok(Prepared {
            request,
            paths: paths.to_vec(),
            metas,
            offer,
            machine,
        })
    }

    async fn transfer<S>(
        &self,
        session: crate::channel::EstablishedSession<S>,
        prepared: Prepared,
        throttle_bps: Option<u64>,
        cancel: CancellationToken,
    ) -> TransferOutcome
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let crate::channel::EstablishedSession {
            peer,
            view,
            sas,
            reader,
            mut writer,
            ..
        } = session;
        let Prepared {
            request,
            paths,
            metas,
            offer,
            mut machine,
        } = prepared;
        let id = request.id();
        let mut domain =
            TransferSession::new(Direction::Send, peer.identity.fingerprint, request.clone());
        let (mut rx, reader_task) = spawn_reader(reader);
        let reader_task = AbortOnDrop(reader_task);
        let mut run = SendRun {
            id,
            writer: &mut writer,
            rx: &mut rx,
            machine: &mut machine,
            domain: &mut domain,
            events: &self.events,
            timeouts: &self.timeouts,
            ticker: ProgressTicker::default(),
        };

        // The session code is always shown, so a receiver that does not
        // trust this device (Everyone mode) has something to compare with.
        self.events
            .emit(AppEvent::Sas {
                peer: view.clone(),
                code: sas,
            })
            .await;
        // Unverified receiver: the local user must confirm the code first.
        if !peer.trust.is_trusted() {
            let prompt = SasPrompt {
                peer: view.clone(),
                code: sas,
                purpose: SasPurpose::UnverifiedReceiver,
            };
            let confirmed = tokio::select! {
                biased;
                () = cancel.cancelled() => None,
                r = timeout(self.timeouts.sas, self.interaction.confirm_sas(&prompt)) => Some(r.unwrap_or(false)),
            };
            match confirmed {
                Some(true) => {}
                Some(false) => {
                    run.close_with(Message::Cancel {
                        reason: CancelReason::User,
                    })
                    .await;
                    drop(reader_task);
                    return run.fail(TransferError::SasRejected);
                }
                None => {
                    run.close_with(Message::Cancel {
                        reason: CancelReason::User,
                    })
                    .await;
                    drop(reader_task);
                    return run.cancelled(Party::Local, CancelReason::User);
                }
            }
        }

        let outcome = run
            .execute(
                &offer,
                &view,
                &paths,
                &metas,
                self.source,
                throttle_bps,
                &cancel,
            )
            .await;
        if outcome.is_success()
            && peer.trust.is_trusted()
            && let Err(e) = self
                .trust
                .touch(&peer.identity.fingerprint, self.clock.now())
        {
            tracing::debug!(kind = e.kind(), "could not record last_seen");
        }
        drop(reader_task);
        outcome
    }
}

/// One running transfer (sender side).
struct SendRun<'r, W> {
    id: TransferId,
    writer: &'r mut RecordWriter<W>,
    rx: &'r mut mpsc::Receiver<Inbound>,
    machine: &'r mut SenderMachine,
    domain: &'r mut TransferSession,
    events: &'r EventSink,
    timeouts: &'r Timeouts,
    ticker: ProgressTicker,
}

/// What handling inbound traffic decided.
enum Flow {
    Continue,
    Stop(TransferOutcome),
}

impl<W: AsyncWrite + Unpin> SendRun<'_, W> {
    fn fail(&mut self, error: TransferError) -> TransferOutcome {
        let _ = self.domain.apply(TransferEvent::Failed(error.clone()));
        TransferOutcome::Failed {
            transfer_id: Some(self.id),
            error,
        }
    }

    fn cancelled(&mut self, by: Party, reason: CancelReason) -> TransferOutcome {
        let _ = self.domain.apply(TransferEvent::Cancelled { by, reason });
        TransferOutcome::Cancelled {
            transfer_id: Some(self.id),
            by,
            reason,
        }
    }

    /// Sends a final message, closes our direction and drains the peer.
    async fn close_with(&mut self, msg: Message) {
        let _ = self.writer.send(&msg).await;
        self.writer.shutdown().await;
        let drain = self.timeouts.drain;
        let _ = timeout(drain, async { while self.rx.recv().await.is_some() {} }).await;
    }

    async fn emit_progress(&mut self, force: bool) {
        if force || self.ticker.due() {
            self.events
                .emit(AppEvent::Progress {
                    transfer_id: self.id,
                    progress: *self.domain.progress(),
                })
                .await;
        }
    }

    /// Handles one inbound item.
    async fn handle(&mut self, inbound: Inbound) -> Flow {
        let body = match inbound {
            Inbound::Control(b) => b,
            Inbound::Failed(RecvError::Transport(e)) => return Flow::Stop(self.fail(e)),
            Inbound::Failed(RecvError::Protocol(p)) => {
                self.close_with(Message::error(p.wire_code())).await;
                return Flow::Stop(self.fail(p.to_transfer_error()));
            }
        };
        match self.machine.on_control(&body) {
            Ok(SenderEvent::Ignored | SenderEvent::Accepted) => Flow::Continue,
            Ok(SenderEvent::FileAcked { index, status }) => {
                let outcome = match status {
                    AckStatus::Stored => FileOutcome::Stored {
                        saved_as: String::new(),
                    },
                    AckStatus::IntegrityFailed => FileOutcome::IntegrityFailed,
                    AckStatus::WriteFailed => FileOutcome::WriteFailed,
                    AckStatus::Aborted => FileOutcome::Aborted,
                };
                let _ = self
                    .domain
                    .apply(TransferEvent::FileSettled(index, outcome));
                self.events
                    .emit(AppEvent::FileSent {
                        transfer_id: self.id,
                        index,
                        status,
                    })
                    .await;
                Flow::Continue
            }
            Ok(SenderEvent::Rejected(reason)) => {
                let _ = self.domain.apply(TransferEvent::Rejected(reason));
                Flow::Stop(TransferOutcome::Rejected {
                    transfer_id: self.id,
                    reason,
                })
            }
            Ok(SenderEvent::PeerCancelled(reason)) => {
                self.writer.shutdown().await;
                Flow::Stop(self.cancelled(Party::Peer, reason))
            }
            Ok(SenderEvent::PeerError(code)) => {
                self.writer.shutdown().await;
                Flow::Stop(self.fail(remote_error(code)))
            }
            Ok(SenderEvent::Completed { .. }) => Flow::Continue,
            Ok(_) => Flow::Continue,
            Err(p) => {
                self.close_with(Message::error(p.wire_code())).await;
                Flow::Stop(self.fail(p.to_transfer_error()))
            }
        }
    }

    /// Processes everything already queued without waiting.
    async fn poll_inbound(&mut self) -> Option<TransferOutcome> {
        while let Ok(inbound) = self.rx.try_recv() {
            if let Flow::Stop(o) = self.handle(inbound).await {
                return Some(o);
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute<F: FileSource>(
        &mut self,
        offer: &[Message],
        view: &PeerView,
        paths: &[PathBuf],
        metas: &[SourceMeta],
        source: &F,
        throttle_bps: Option<u64>,
        cancel: &CancellationToken,
    ) -> TransferOutcome {
        // Offer.
        for m in offer {
            if let Err(e) = self.writer.send(m).await {
                return self.fail(e);
            }
        }
        let _ = self.domain.apply(TransferEvent::OfferSent);
        let request = self.machine.request().clone();
        self.events
            .emit(AppEvent::OfferSent {
                transfer_id: self.id,
                peer: view.clone(),
                files: request.file_count(),
                total_bytes: request.total_size(),
            })
            .await;

        // Decision.
        let deadline = tokio::time::Instant::now() + self.timeouts.decision;
        loop {
            let inbound = tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    self.close_with(Message::Cancel { reason: CancelReason::User }).await;
                    return self.cancelled(Party::Local, CancelReason::User);
                }
                () = sleep_until(deadline) => {
                    self.close_with(Message::Cancel { reason: CancelReason::Timeout }).await;
                    return self.fail(TransferError::Timeout(Phase::Consent));
                }
                i = self.rx.recv() => i,
            };
            let Some(inbound) = inbound else {
                return self.fail(TransferError::ConnectionLost);
            };
            match self.handle(inbound).await {
                Flow::Stop(o) => return o,
                Flow::Continue => {
                    if self.machine.next_file().is_some() {
                        break;
                    }
                }
            }
        }
        let _ = self.domain.apply(TransferEvent::Accepted);
        self.events
            .emit(AppEvent::Accepted {
                transfer_id: self.id,
            })
            .await;
        self.emit_progress(true).await;

        // Stream every file.
        let started = Instant::now();
        let mut read_buf = vec![0u8; MAX_FRAME_BODY].into_boxed_slice();
        let mut sent_total: u64 = 0;
        for (i, (path, meta)) in paths.iter().zip(metas).enumerate() {
            let index = u32::try_from(i).unwrap_or(u32::MAX);
            if let Some(o) = self.poll_inbound().await {
                return o;
            }
            let mut file = match source.open(path, meta).await {
                Ok(f) => f,
                Err(e) => {
                    let reason = match e {
                        crate::error::SourceError::Changed => AbortReason::SourceChanged,
                        _ => AbortReason::SourceUnreadable,
                    };
                    match self.machine.abort_file(reason) {
                        Ok(m) => {
                            if let Err(e) = self.writer.send(&m).await {
                                return self.fail(e);
                            }
                            continue;
                        }
                        Err(p) => return self.fail(p.to_transfer_error()),
                    }
                }
            };
            let start = match self.machine.start_file(index) {
                Ok(m) => m,
                Err(p) => return self.fail(p.to_transfer_error()),
            };
            if let Err(e) = self.writer.send(&start).await {
                return self.fail(e);
            }
            let _ = self.domain.apply(TransferEvent::FileStarted(index));
            let mut aborted = false;
            while let Some(remaining) = self.machine.remaining().filter(|r| *r > 0) {
                if cancel.is_cancelled() {
                    self.close_with(Message::Cancel {
                        reason: CancelReason::User,
                    })
                    .await;
                    return self.cancelled(Party::Local, CancelReason::User);
                }
                if let Some(o) = self.poll_inbound().await {
                    return o;
                }
                let want = usize::try_from(remaining)
                    .unwrap_or(MAX_FRAME_BODY)
                    .min(MAX_FRAME_BODY);
                let Some(chunk) = read_buf.get_mut(..want) else {
                    return self.fail(TransferError::Internal("buffer".into()));
                };
                // Reads race against cancellation and the receiver's messages,
                // so a slow or stuck source never makes the sender deaf.
                let n: usize = loop {
                    tokio::select! {
                        biased;
                        () = cancel.cancelled() => {
                            self.close_with(Message::Cancel { reason: CancelReason::User }).await;
                            return self.cancelled(Party::Local, CancelReason::User);
                        }
                        inbound = self.rx.recv() => match inbound {
                            Some(i) => {
                                if let Flow::Stop(o) = self.handle(i).await {
                                    return o;
                                }
                            }
                            None => return self.fail(TransferError::ConnectionLost),
                        },
                        // A read error is treated like a shrunken file: abort it.
                        r = file.read(chunk) => break r.unwrap_or_default(),
                    }
                };
                if n == 0 {
                    // The file shrank or became unreadable mid-transfer.
                    match self.machine.abort_file(AbortReason::SourceChanged) {
                        Ok(m) => {
                            if let Err(e) = self.writer.send(&m).await {
                                return self.fail(e);
                            }
                        }
                        Err(p) => return self.fail(p.to_transfer_error()),
                    }
                    aborted = true;
                    break;
                }
                let data = read_buf.get(..n).unwrap_or_default();
                if let Err(p) = self.machine.data(data) {
                    return self.fail(p.to_transfer_error());
                }
                if let Some(dst) = self.writer.data_buf().get_mut(..n) {
                    dst.copy_from_slice(data);
                }
                if let Err(e) = self.writer.send_data(n).await {
                    return self.fail(e);
                }
                let n64 = u64::try_from(n).unwrap_or(u64::MAX);
                let _ = self.domain.record_bytes(n64);
                sent_total = sent_total.saturating_add(n64);
                self.emit_progress(false).await;
                if let Some(bps) = throttle_bps.filter(|b| *b > 0) {
                    let due = Duration::from_secs_f64(sent_total as f64 / bps as f64);
                    let elapsed = started.elapsed();
                    if due > elapsed {
                        sleep(due - elapsed).await;
                    }
                }
            }
            if aborted {
                continue;
            }
            let end = if source.unchanged(path, meta).await {
                self.machine.end_file()
            } else {
                self.machine.abort_file(AbortReason::SourceChanged)
            };
            match end {
                Ok(m) => {
                    if let Err(e) = self.writer.send(&m).await {
                        return self.fail(e);
                    }
                }
                Err(p) => return self.fail(p.to_transfer_error()),
            }
        }

        // Completion: success only after CompleteAck.
        match self.machine.complete() {
            Ok(m) => {
                if let Err(e) = self.writer.send(&m).await {
                    return self.fail(e);
                }
            }
            Err(p) => return self.fail(p.to_transfer_error()),
        }
        // The deadline restarts whenever the receiver acknowledges another
        // file: a slow disk still fsyncing a backlog is progress, not a hang.
        let mut deadline = tokio::time::Instant::now() + self.timeouts.complete_ack;
        let mut acked = self.machine.acks().iter().filter(|a| a.is_some()).count();
        loop {
            if self.machine.is_done() {
                break;
            }
            let now_acked = self.machine.acks().iter().filter(|a| a.is_some()).count();
            if now_acked > acked {
                acked = now_acked;
                deadline = tokio::time::Instant::now() + self.timeouts.complete_ack;
            }
            let inbound = tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    self.close_with(Message::Cancel { reason: CancelReason::User }).await;
                    return self.cancelled(Party::Local, CancelReason::User);
                }
                () = sleep_until(deadline) => {
                    self.close_with(Message::error(ErrorCode::Timeout)).await;
                    return self.fail(TransferError::Timeout(Phase::CompleteAck));
                }
                i = self.rx.recv() => i,
            };
            let Some(inbound) = inbound else {
                return self.fail(TransferError::ConnectionLost);
            };
            if let Flow::Stop(o) = self.handle(inbound).await {
                return o;
            }
        }
        self.writer.shutdown().await;
        let _ = self.domain.apply(TransferEvent::Finalizing);
        let _ = self.domain.apply(TransferEvent::Finished);
        self.emit_progress(true).await;
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let stored = self.machine.stored_count();
        let failed = request.file_count().saturating_sub(stored);
        if failed == 0 {
            TransferOutcome::Completed {
                transfer_id: self.id,
                files_stored: stored,
                bytes: sent_total,
                elapsed_ms,
            }
        } else {
            TransferOutcome::CompletedWithFailures {
                transfer_id: self.id,
                files_stored: stored,
                failed,
                bytes: sent_total,
                elapsed_ms,
            }
        }
    }
}

/// Maps a peer's error code.
#[must_use]
pub fn remote_error(code: ErrorCode) -> TransferError {
    match code {
        ErrorCode::NotAuthorized => TransferError::NotAuthorized,
        ErrorCode::Busy => TransferError::Busy,
        ErrorCode::PairingFailed => TransferError::PairingRejected,
        other => TransferError::Remote {
            code: u16::try_from(other.code()).unwrap_or(u16::MAX),
        },
    }
}
