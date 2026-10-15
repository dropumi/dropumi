// SPDX-License-Identifier: MPL-2.0
//! In-memory doubles for every port, used by the application tests.
//!
//! The network is a set of `tokio::io::duplex` pipes, so the real handshake,
//! records and session machines run unchanged.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dropumi_application::error::{
    DiscoveryError, SourceError, StorageError, StoreError, TransportError,
};
use dropumi_application::events::{AppEvent, EventSink};
use dropumi_application::identity::LocalIdentity;
use dropumi_application::ports::{
    AdHandle, Advertisement, Clock, Connector, Decision, Discovery, FileSink, FileSource,
    IdentityStore, IncomingFile, IncomingOffer, Interaction, Listener, SasPrompt, SavedFile,
    SourceMeta, TrustStore,
};
use dropumi_domain::device::DeviceName;
use dropumi_domain::discovery::DiscoveredPeer;
use dropumi_domain::filename::SafeFileName;
use dropumi_domain::identity::Fingerprint;
use dropumi_domain::time::Timestamp;
use dropumi_domain::trust::{PairingMethod, TrustedDevice};
use dropumi_protocol::identity::StaticSecret;
use tokio::io::{AsyncRead, DuplexStream, ReadBuf};
use tokio::sync::mpsc;

pub const PIPE: usize = 256 * 1024;

pub fn identity(seed: u8, name: &str) -> LocalIdentity {
    LocalIdentity::new(
        StaticSecret::from_bytes([seed; 32]),
        DeviceName::sanitize(name),
    )
    .unwrap()
}

pub fn addr(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

// ---------------------------------------------------------------- network

#[derive(Default)]
pub struct Net {
    listeners: Mutex<HashMap<SocketAddr, mpsc::Sender<DuplexStream>>>,
}

impl Net {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn listen(self: &Arc<Self>, at: SocketAddr) -> FakeListener {
        let (tx, rx) = mpsc::channel(32);
        self.listeners.lock().unwrap().insert(at, tx);
        FakeListener { rx, addr: at }
    }

    pub fn connector(self: &Arc<Self>) -> FakeConnector {
        FakeConnector {
            net: Arc::clone(self),
            buffer: PIPE,
        }
    }
}

#[derive(Clone)]
pub struct FakeConnector {
    net: Arc<Net>,
    pub buffer: usize,
}

impl Connector for FakeConnector {
    type Stream = DuplexStream;

    async fn connect(
        &self,
        addr: SocketAddr,
        _t: Duration,
    ) -> Result<DuplexStream, TransportError> {
        let tx = self
            .net
            .listeners
            .lock()
            .unwrap()
            .get(&addr)
            .cloned()
            .ok_or(TransportError::Refused)?;
        let (a, b) = tokio::io::duplex(self.buffer);
        tx.send(b).await.map_err(|_| TransportError::Refused)?;
        Ok(a)
    }

    async fn resolve(&self, _host: &str, port: u16) -> Result<Vec<SocketAddr>, TransportError> {
        Ok(vec![addr(port)])
    }
}

pub struct FakeListener {
    rx: mpsc::Receiver<DuplexStream>,
    addr: SocketAddr,
}

impl Listener for FakeListener {
    type Stream = DuplexStream;

    async fn accept(&mut self) -> Result<(DuplexStream, SocketAddr), TransportError> {
        match self.rx.recv().await {
            Some(s) => Ok((s, addr(1))),
            None => std::future::pending().await,
        }
    }

    fn local_addrs(&self) -> Vec<SocketAddr> {
        vec![self.addr]
    }
}

// --------------------------------------------------------------- discovery

#[derive(Clone, Default)]
pub struct StaticDiscovery(pub Vec<DiscoveredPeer>);

pub struct NoAd;

impl AdHandle for NoAd {
    async fn stop(self) {}
    fn instance(&self) -> &str {
        "test"
    }
}

impl Discovery for StaticDiscovery {
    type Handle = NoAd;

    async fn advertise(&self, _ad: Advertisement) -> Result<NoAd, DiscoveryError> {
        Ok(NoAd)
    }

    async fn browse(&self, _t: Duration) -> Result<Vec<DiscoveredPeer>, DiscoveryError> {
        Ok(self.0.clone())
    }
}

/// Discovery whose browse never completes. `started` is notified when a
/// browse begins, so tests can act while it is pending without sleeping.
#[derive(Clone, Default)]
pub struct PendingDiscovery {
    pub started: Arc<tokio::sync::Notify>,
}

impl Discovery for PendingDiscovery {
    type Handle = NoAd;

    async fn advertise(&self, _ad: Advertisement) -> Result<NoAd, DiscoveryError> {
        Ok(NoAd)
    }

    async fn browse(&self, _t: Duration) -> Result<Vec<DiscoveredPeer>, DiscoveryError> {
        self.started.notify_one();
        std::future::pending().await
    }
}

// ------------------------------------------------------------------ source

/// A reader that yields `limit` bytes and then never completes.
pub struct StallingReader {
    data: Cursor<Vec<u8>>,
    limit: usize,
}

impl AsyncRead for StallingReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let pos = usize::try_from(self.data.position()).unwrap();
        if pos >= self.limit {
            return std::task::Poll::Pending;
        }
        let max = (self.limit - pos).min(buf.remaining());
        let mut tmp = vec![0u8; max];
        let n = std::io::Read::read(&mut self.data, &mut tmp).unwrap();
        buf.put_slice(&tmp[..n]);
        let _ = cx;
        std::task::Poll::Ready(Ok(()))
    }
}

pub enum SourceReader {
    Ready(Cursor<Vec<u8>>),
    Stalling(StallingReader),
}

impl AsyncRead for SourceReader {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Ready(c) => std::pin::Pin::new(c).poll_read(cx, buf),
            Self::Stalling(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

#[derive(Clone, Default)]
pub struct MemSource {
    files: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
    changed: Arc<Mutex<HashSet<PathBuf>>>,
    stall_after: Arc<Mutex<HashMap<PathBuf, usize>>>,
}

impl MemSource {
    pub fn add(&self, path: &str, data: Vec<u8>) -> PathBuf {
        let p = PathBuf::from(path);
        self.files.lock().unwrap().insert(p.clone(), data);
        p
    }

    pub fn mark_changed(&self, path: &Path) {
        self.changed.lock().unwrap().insert(path.to_path_buf());
    }

    pub fn stall_after(&self, path: &Path, bytes: usize) {
        self.stall_after
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), bytes);
    }
}

impl FileSource for MemSource {
    type Reader = SourceReader;

    async fn meta(&self, path: &Path) -> Result<SourceMeta, SourceError> {
        let files = self.files.lock().unwrap();
        let data = files
            .get(path)
            .ok_or_else(|| SourceError::NotAFile(path.display().to_string()))?;
        Ok(SourceMeta {
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            size: data.len() as u64,
            modified: Some(1),
            local_modified: None,
        })
    }

    async fn open(&self, path: &Path, _meta: &SourceMeta) -> Result<SourceReader, SourceError> {
        let data = self.files.lock().unwrap().get(path).cloned().unwrap();
        Ok(match self.stall_after.lock().unwrap().get(path) {
            Some(limit) => SourceReader::Stalling(StallingReader {
                data: Cursor::new(data),
                limit: *limit,
            }),
            None => SourceReader::Ready(Cursor::new(data)),
        })
    }

    async fn unchanged(&self, path: &Path, _meta: &SourceMeta) -> bool {
        !self.changed.lock().unwrap().contains(path)
    }
}

// -------------------------------------------------------------------- sink

#[derive(Default)]
pub struct SinkState {
    pub stored: HashMap<String, Vec<u8>>,
    pub begun: usize,
    pub committed: usize,
    pub discarded: usize,
    pub dir_syncs: usize,
    /// Bytes accepted by `write` across all files.
    pub written: usize,
}

#[derive(Clone, Default)]
pub struct MemSink {
    pub state: Arc<Mutex<SinkState>>,
    /// Fail writes once this many bytes were written to one file.
    pub fail_write_after: Option<usize>,
    /// Error message used for injected failures.
    pub fail_message: String,
    /// Block forever on the first write.
    pub block_writes: bool,
    /// Signalled after every successful write.
    pub wrote: Arc<tokio::sync::Notify>,
    /// Time each commit takes (a slow disk; use with paused time).
    pub commit_delay: Option<Duration>,
}

impl MemSink {
    /// Waits until at least `bytes` were written in total (tests use this
    /// instead of sleeping until a transfer reaches a given point).
    pub async fn wait_written(&self, bytes: usize) {
        loop {
            // Created before the check, so a write in between is not missed.
            let wrote = self.wrote.notified();
            if self.state.lock().unwrap().written >= bytes {
                return;
            }
            wrote.await;
        }
    }

    pub fn stored(&self) -> HashMap<String, Vec<u8>> {
        self.state.lock().unwrap().stored.clone()
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        let s = self.state.lock().unwrap();
        (s.begun, s.committed, s.discarded)
    }
}

pub struct MemFile {
    name: SafeFileName,
    buf: Vec<u8>,
    sink: MemSink,
}

impl IncomingFile for MemFile {
    async fn write(&mut self, data: &[u8]) -> Result<(), StorageError> {
        if self.sink.block_writes {
            std::future::pending::<()>().await;
        }
        if let Some(limit) = self.sink.fail_write_after
            && self.buf.len() + data.len() > limit
        {
            return Err(StorageError::Other(self.sink.fail_message.clone()));
        }
        self.buf.extend_from_slice(data);
        self.sink.state.lock().unwrap().written += data.len();
        self.sink.wrote.notify_waiters();
        Ok(())
    }

    async fn commit(self) -> Result<SavedFile, StorageError> {
        if let Some(delay) = self.sink.commit_delay {
            tokio::time::sleep(delay).await;
        }
        let mut s = self.sink.state.lock().unwrap();
        let mut n = 0;
        let name = loop {
            let candidate = self.name.with_collision_suffix(n).as_str().to_owned();
            if !s.stored.contains_key(&candidate) {
                break candidate;
            }
            n += 1;
        };
        s.stored.insert(name.clone(), self.buf);
        s.committed += 1;
        Ok(SavedFile { saved_as: name })
    }

    async fn discard(self) {
        self.sink.state.lock().unwrap().discarded += 1;
    }
}

impl FileSink for MemSink {
    type File = MemFile;

    async fn begin(&self, name: &SafeFileName, _size: u64) -> Result<MemFile, StorageError> {
        self.state.lock().unwrap().begun += 1;
        Ok(MemFile {
            name: name.clone(),
            buf: Vec::new(),
            sink: self.clone(),
        })
    }

    async fn sync_dir(&self) -> Result<(), StorageError> {
        self.state.lock().unwrap().dir_syncs += 1;
        Ok(())
    }
}

// ------------------------------------------------------------------- trust

/// In-memory trust store. The `fail_*` switches make one call fail the way
/// an insecure trust file would (the error names the file, as the real
/// adapter's does).
#[derive(Clone, Default)]
pub struct MemTrust {
    pub devices: Arc<Mutex<Vec<TrustedDevice>>>,
    /// `get` fails.
    pub fail_get: Arc<AtomicBool>,
    /// `touch` fails.
    pub fail_touch: Arc<AtomicBool>,
    /// `upsert` stores the entry, then reports `NotDurable` (a directory
    /// sync that failed after the commit point).
    pub upsert_not_durable: Arc<AtomicBool>,
}

impl MemTrust {
    pub fn with(devices: &[(&LocalIdentity, &str)]) -> Self {
        let t = Self::default();
        for (id, alias) in devices {
            t.add(id, alias);
        }
        t
    }

    pub fn add(&self, id: &LocalIdentity, alias: &str) {
        self.upsert(TrustedDevice {
            fingerprint: *id.fingerprint(),
            public_key: Some(*id.public_key()),
            alias: Some(DeviceName::sanitize(alias)),
            method: PairingMethod::Manual,
            added_at: Timestamp(1),
            last_seen: None,
        })
        .unwrap();
    }

    pub fn contains(&self, id: &LocalIdentity) -> bool {
        self.devices
            .lock()
            .unwrap()
            .iter()
            .any(|d| &d.fingerprint == id.fingerprint())
    }

    pub fn len(&self) -> usize {
        self.devices.lock().unwrap().len()
    }
}

/// Path named in the errors of a failing [`MemTrust`].
pub const BROKEN_TRUST_PATH: &str = "/home/someone/.config/dropumi/trusted_devices.json";

fn fail_if(switch: &AtomicBool) -> Result<(), StoreError> {
    if switch.load(Ordering::SeqCst) {
        return Err(StoreError::InsecurePermissions {
            path: BROKEN_TRUST_PATH.into(),
            hint: "chmod 600".into(),
        });
    }
    Ok(())
}

impl TrustStore for MemTrust {
    fn get(&self, fp: &Fingerprint) -> Result<Option<TrustedDevice>, StoreError> {
        fail_if(&self.fail_get)?;
        Ok(self
            .devices
            .lock()
            .unwrap()
            .iter()
            .find(|d| &d.fingerprint == fp)
            .cloned())
    }

    fn list(&self) -> Result<Vec<TrustedDevice>, StoreError> {
        Ok(self.devices.lock().unwrap().clone())
    }

    fn upsert(&self, device: TrustedDevice) -> Result<(), StoreError> {
        let mut v = self.devices.lock().unwrap();
        v.retain(|d| d.fingerprint != device.fingerprint);
        v.push(device);
        if self.upsert_not_durable.load(Ordering::SeqCst) {
            return Err(StoreError::NotDurable(format!(
                "{BROKEN_TRUST_PATH}: directory sync failed"
            )));
        }
        Ok(())
    }

    /// Merges under the mutex, like the real store under its lock; never
    /// consults `fail_get`.
    fn upsert_with<F: FnOnce(Option<&TrustedDevice>) -> TrustedDevice>(
        &self,
        fp: &Fingerprint,
        merge: F,
    ) -> Result<TrustedDevice, StoreError> {
        let mut v = self.devices.lock().unwrap();
        let device = merge(v.iter().find(|d| &d.fingerprint == fp));
        if &device.fingerprint != fp {
            return Err(StoreError::Io(
                "merged trust entry has a different fingerprint".into(),
            ));
        }
        v.retain(|d| &d.fingerprint != fp);
        v.push(device.clone());
        Ok(device)
    }

    fn remove(&self, fp: &Fingerprint) -> Result<bool, StoreError> {
        let mut v = self.devices.lock().unwrap();
        let before = v.len();
        v.retain(|d| &d.fingerprint != fp);
        Ok(v.len() != before)
    }

    fn touch(&self, fp: &Fingerprint, at: Timestamp) -> Result<(), StoreError> {
        fail_if(&self.fail_touch)?;
        for d in self.devices.lock().unwrap().iter_mut() {
            if &d.fingerprint == fp {
                d.last_seen = Some(at);
            }
        }
        Ok(())
    }
}

pub struct MemIdentity(pub LocalIdentity);

impl IdentityStore for MemIdentity {
    fn load_or_create(&self) -> Result<LocalIdentity, StoreError> {
        Ok(self.0.clone())
    }
    fn reset(&self) -> Result<LocalIdentity, StoreError> {
        Ok(self.0.clone())
    }
    fn set_name(&self, name: &DeviceName) -> Result<LocalIdentity, StoreError> {
        Ok(self.0.clone().with_name(name.clone()))
    }
}

// ------------------------------------------------------------- interaction

#[derive(Clone)]
pub struct Scripted {
    /// `None` never answers (to test consent timeouts).
    pub decision: Option<Decision>,
    pub confirm: Option<bool>,
    /// How long the user takes to answer the code (paused time).
    pub confirm_delay: Duration,
    pub offers_seen: Arc<AtomicUsize>,
    pub sas_seen: Arc<Mutex<Vec<String>>>,
}

impl Scripted {
    pub fn accept() -> Self {
        Self {
            decision: Some(Decision::Accept),
            confirm: Some(true),
            confirm_delay: Duration::ZERO,
            offers_seen: Arc::default(),
            sas_seen: Arc::default(),
        }
    }

    /// Answers the code with `confirm` after `delay`.
    pub fn confirm_after(delay: Duration, confirm: bool) -> Self {
        Self {
            confirm: Some(confirm),
            confirm_delay: delay,
            ..Self::accept()
        }
    }

    pub fn with(decision: Option<Decision>, confirm: Option<bool>) -> Self {
        Self {
            decision,
            confirm,
            ..Self::accept()
        }
    }
}

impl Interaction for Scripted {
    async fn decide_offer(&self, _offer: &IncomingOffer) -> Decision {
        self.offers_seen.fetch_add(1, Ordering::SeqCst);
        match self.decision {
            Some(d) => d,
            None => std::future::pending().await,
        }
    }

    async fn confirm_sas(&self, prompt: &SasPrompt) -> bool {
        self.sas_seen.lock().unwrap().push(prompt.code.as_digits());
        if !self.confirm_delay.is_zero() {
            tokio::time::sleep(self.confirm_delay).await;
        }
        match self.confirm {
            Some(c) => c,
            None => std::future::pending().await,
        }
    }
}

#[derive(Clone, Default)]
pub struct ManualClock(pub Arc<AtomicU64>);

impl ManualClock {
    pub fn at(secs: u64) -> Self {
        Self(Arc::new(AtomicU64::new(secs)))
    }
    pub fn advance(&self, secs: u64) {
        self.0.fetch_add(secs, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.0.load(Ordering::SeqCst))
    }
}

// ------------------------------------------------------------------ events

pub fn events() -> (EventSink, mpsc::Receiver<AppEvent>) {
    let (tx, rx) = mpsc::channel(10_000);
    (EventSink::new(tx), rx)
}

pub fn drain(rx: &mut mpsc::Receiver<AppEvent>) -> Vec<AppEvent> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

// ----------------------------------------------------------------- harness

use dropumi_application::config::{AdmissionLimits, ReceiveConfig, SendRequest, Timeouts};
use dropumi_application::events::TransferOutcome;
use dropumi_application::usecases::UseCaseError;
use dropumi_application::usecases::receive::{ReceiveDeps, ReceiveFiles, ReceiveSummary};
use dropumi_application::usecases::send::{SendDeps, SendFiles};
use dropumi_domain::discovery::Target;
use dropumi_domain::identity::FingerprintPrefix;
use dropumi_domain::policy::{AcceptMode, PolicyMode, TransferPolicy};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct ReceiverHandle {
    pub task: JoinHandle<Result<ReceiveSummary, UseCaseError>>,
    pub cancel: CancellationToken,
    pub events: mpsc::Receiver<AppEvent>,
    pub sink: MemSink,
    pub trust: MemTrust,
    pub interaction: Scripted,
}

pub struct ReceiverSetup {
    pub identity: LocalIdentity,
    pub trust: MemTrust,
    pub sink: MemSink,
    pub interaction: Scripted,
    pub clock: ManualClock,
    pub policy: TransferPolicy,
    pub once: bool,
    pub timeouts: Timeouts,
}

impl ReceiverSetup {
    pub fn new(identity: LocalIdentity, trust: MemTrust) -> Self {
        Self {
            identity,
            trust,
            sink: MemSink::default(),
            interaction: Scripted::accept(),
            clock: ManualClock::at(1_000),
            policy: TransferPolicy {
                mode: PolicyMode::TrustedOnly,
                accept: AcceptMode::AutoTrusted,
            },
            once: true,
            timeouts: Timeouts::default(),
        }
    }

    pub fn start(self, net: &Arc<Net>, at: SocketAddr) -> ReceiverHandle {
        let listener = net.listen(at);
        let (sink_events, rx) = events();
        let cancel = CancellationToken::new();
        let deps = ReceiveDeps {
            sink: self.sink.clone(),
            trust: self.trust.clone(),
            interaction: self.interaction.clone(),
            clock: self.clock.clone(),
            identity: self.identity.clone(),
            timeouts: self.timeouts,
            limits: AdmissionLimits::default(),
            events: sink_events,
        };
        let use_case = ReceiveFiles::new(listener, deps);
        let cfg = ReceiveConfig {
            policy: self.policy,
            once: self.once,
        };
        let token = cancel.clone();
        let task = tokio::spawn(async move { use_case.run(cfg, token).await });
        ReceiverHandle {
            task,
            cancel,
            events: rx,
            sink: self.sink,
            trust: self.trust,
            interaction: self.interaction,
        }
    }
}

pub struct SenderSetup {
    pub identity: LocalIdentity,
    pub trust: MemTrust,
    pub source: MemSource,
    pub interaction: Scripted,
    pub clock: ManualClock,
    pub timeouts: Timeouts,
    pub discovery: Option<StaticDiscovery>,
    pub connector_buffer: usize,
}

impl SenderSetup {
    pub fn new(identity: LocalIdentity, trust: MemTrust, source: MemSource) -> Self {
        Self {
            identity,
            trust,
            source,
            interaction: Scripted::accept(),
            clock: ManualClock::at(1_000),
            timeouts: Timeouts::default(),
            discovery: None,
            connector_buffer: PIPE,
        }
    }

    pub async fn send(
        &self,
        net: &Arc<Net>,
        targets: Vec<Target>,
        paths: Vec<PathBuf>,
        allow_unverified: bool,
        expect: Option<FingerprintPrefix>,
        cancel: CancellationToken,
    ) -> (Result<TransferOutcome, UseCaseError>, Vec<AppEvent>) {
        let mut connector = net.connector();
        connector.buffer = self.connector_buffer;
        let (sink, mut rx) = events();
        let use_case = SendFiles::new(SendDeps {
            connector: &connector,
            discovery: self.discovery.as_ref(),
            source: &self.source,
            trust: &self.trust,
            interaction: &self.interaction,
            clock: &self.clock,
            identity: &self.identity,
            timeouts: self.timeouts,
            events: sink,
        });
        let req = SendRequest {
            targets,
            paths,
            expect,
            allow_unverified,
            throttle_bps: None,
            discover_timeout: Duration::from_millis(10),
            allow_loopback: true,
        };
        let out = use_case.run(req, cancel).await;
        (out, drain(&mut rx))
    }
}

pub fn to(port: u16) -> Vec<Target> {
    vec![Target::Addr(addr(port))]
}

pub async fn finish(
    mut h: ReceiverHandle,
) -> (Result<ReceiveSummary, UseCaseError>, Vec<AppEvent>, MemSink) {
    let r = h.task.await.unwrap();
    let ev = drain(&mut h.events);
    (r, ev, h.sink)
}
