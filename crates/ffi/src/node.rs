// SPDX-License-Identifier: MPL-2.0
//! `DropumiNode`: the object the apps hold.
//!
//! Every exported method is synchronous and returns quickly; call them off
//! the main thread, because some read small configuration files. Long work
//! (sending, pairing, receiving) runs on the node's runtime and reports
//! through events; `send`, `pair_with` and `open_pairing_window` return an
//! [`OperationHandle`] at once.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dropumi_application::config::{AdmissionLimits, ReceiveConfig, SendRequest, Timeouts};
use dropumi_application::error::{DiscoveryError, StoreError};
use dropumi_application::events::{AppEvent, EventSink};
use dropumi_application::identity::LocalIdentity;
use dropumi_application::ports::{
    AdHandle, Advertisement, Clock, Discovery, IdentityStore, TrustStore,
};
use dropumi_application::usecases::UseCaseError;
use dropumi_application::usecases::connect::ResolveParams;
use dropumi_application::usecases::pair::{PairDeps, PairOutcome, pair_listen, pair_with};
use dropumi_application::usecases::receive::{
    ReceiveDeps, ReceiveFiles, ReceiverControl, TransferCanceller,
};
use dropumi_application::usecases::send::{SendDeps, SendFiles};
use dropumi_application::usecases::trust;
use dropumi_domain::device::DeviceName;
use dropumi_domain::discovery::{DiscoveredPeer, Target};
use dropumi_domain::error::TransferError;
use dropumi_domain::identity::{Fingerprint, FingerprintPrefix};
use dropumi_domain::policy::{PolicyMode, TransferPolicy};
use dropumi_domain::transfer::TransferId;
use dropumi_infra::clock::SystemClock;
use dropumi_infra::fs::cleanup::{STALE_AFTER, remove_stale_parts};
use dropumi_infra::fs::{LocalFileSource, ReceiveDir};
use dropumi_infra::net::{DEFAULT_PORT, TcpConnector, TcpListenerAdapter};
use dropumi_infra::store::{
    DEVICE_FILE, FileIdentityStore, KEY_FILE, LOCK_FILE, TRUST_FILE, TrustFile,
};
use dropumi_protocol::limits::SUPPORTED_VERSIONS;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::discovery::{PeerTable, PlatformDiscovery, service_ad, single_trust_match};
use crate::error::{DropumiError, ErrorCode, settle};
use crate::mailbox::{EventListener, Mailbox, spawn_dispatcher};
use crate::prompts::{AnswerKind, PromptRegistry, UiInteraction};
use crate::runtime::{Runtime, lock, on_app_thread};
use crate::source::{CompositeSource, SendItem};
use crate::types::{
    AcceptMode, Direction, IdentityInfo, NearbyPeer, NodeConfig, NodeEvent, OfferedFile,
    OperationKind, OperationResult, PairingResult, PeerInfo, Progress, Prompt, ReceiveOptions,
    ReceivePolicy, ReceiverInfo, ReceiverStopReason, SendOptions, SendTarget, TransferResult,
    TrustedDeviceInfo, policy_info,
};

/// Files per send (protocol limit).
const MAX_FILES: usize = 10_000;
/// Addresses tried for one target.
const MAX_ADDRESSES: usize = 8;
/// Longest Everyone window, in minutes.
const MAX_EVERYONE_MINUTES: u32 = 60;
/// Longest pairing window, in minutes.
const MAX_PAIRING_MINUTES: u32 = 10;
/// Finished operations remembered for `result()` and `snapshot()`.
const KEEP_FINISHED_OPS: usize = 64;

/// Version of this foreign API; apps check it at start.
#[uniffi::export]
#[must_use]
pub fn ffi_api_version() -> u32 {
    1
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The node's view of everything at once (after an app screen is recreated
/// or after `ResyncRequired`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NodeSnapshot {
    /// The running receiver.
    pub receiver: Option<ReceiverInfo>,
    /// The receiver is finishing its last transfers and will stop.
    pub receiver_stopping: bool,
    /// Prompts waiting for the user.
    pub prompts: Vec<Prompt>,
    /// Devices seen while browsing.
    pub nearby: Vec<NearbyPeer>,
    /// Operations, running and recently finished.
    pub operations: Vec<OperationInfo>,
    /// Ids of incoming transfers that are running.
    pub incoming_transfers: Vec<String>,
}

/// An operation and its result once it ended.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OperationInfo {
    /// Operation id.
    pub id: u64,
    /// What it does.
    pub kind: OperationKind,
    /// Its result, once it ended.
    pub result: Option<OperationResult>,
}

#[derive(Debug)]
struct Op {
    kind: OperationKind,
    cancel: CancellationToken,
    result: Option<OperationResult>,
    order: u64,
}

#[derive(Debug)]
struct Running {
    generation: u64,
    cancel: CancellationToken,
    control: ReceiverControl,
    canceller: TransferCanceller,
    info: ReceiverInfo,
    options: ReceiveOptions,
    done: watch::Receiver<bool>,
}

#[derive(Debug)]
enum ReceiverState {
    Idle,
    Running(Running),
    Stopping {
        generation: u64,
        done: watch::Receiver<bool>,
    },
}

#[derive(Debug)]
struct State {
    receiver: ReceiverState,
    /// Advertised instance of the receiver or pairing window.
    advertised: Option<String>,
    pairing_window: bool,
    browsing: bool,
    ops: HashMap<u64, Op>,
    generation: u64,
}

/// Discovery for the use cases: sends never need it (nearby devices are
/// reached by their resolved addresses), so it is always unavailable.
#[derive(Debug)]
struct NoDiscovery;

#[derive(Debug)]
struct NoAd;

impl AdHandle for NoAd {
    async fn stop(self) {}
    fn instance(&self) -> &str {
        ""
    }
}

impl Discovery for NoDiscovery {
    type Handle = NoAd;

    async fn advertise(&self, _ad: Advertisement) -> Result<NoAd, DiscoveryError> {
        Err(DiscoveryError::Unavailable("not used by the node".into()))
    }

    async fn browse(&self, _t: Duration) -> Result<Vec<DiscoveredPeer>, DiscoveryError> {
        Err(DiscoveryError::Unavailable("not used by the node".into()))
    }
}

struct Engine {
    runtime: Runtime,
    mailbox: Arc<Mailbox>,
    dispatcher: Mutex<Option<JoinHandle<()>>>,
    dispatcher_thread: Mutex<Option<std::thread::ThreadId>>,
    config_dir: PathBuf,
    receive_dir: Option<PathBuf>,
    identity: FileIdentityStore,
    trust: TrustFile,
    prompts: Arc<PromptRegistry>,
    platform: Option<Arc<dyn PlatformDiscovery>>,
    peers: Arc<PeerTable>,
    incoming: Arc<Mutex<HashSet<String>>>,
    state: Mutex<State>,
    next_op: AtomicU64,
    shut_down: AtomicBool,
    timeouts: Timeouts,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").finish_non_exhaustive()
    }
}

/// The Dropumi node: identity, trust, discovery, receiving and sending.
#[derive(Debug, uniffi::Object)]
pub struct DropumiNode {
    engine: Arc<Engine>,
}

/// A running or finished operation.
#[derive(Debug, uniffi::Object)]
pub struct OperationHandle {
    id: u64,
    kind: OperationKind,
    engine: Weak<Engine>,
}

#[uniffi::export]
impl OperationHandle {
    /// Operation id (events name it).
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// What it does.
    #[must_use]
    pub fn kind(&self) -> OperationKind {
        self.kind
    }

    /// Cancels it; the peer is told. Its result arrives as an event.
    pub fn cancel(&self) {
        if let Some(engine) = self.engine.upgrade()
            && let Some(op) = lock(&engine.state).ops.get(&self.id)
        {
            op.cancel.cancel();
        }
    }

    /// Its result, once it ended.
    #[must_use]
    pub fn result(&self) -> Option<OperationResult> {
        let engine = self.engine.upgrade()?;
        let state = lock(&engine.state);
        state.ops.get(&self.id).and_then(|op| op.result.clone())
    }
}

#[uniffi::export]
impl DropumiNode {
    /// Opens the configuration directory (creating it with mode 0700) and
    /// starts the node's threads. Does not create an identity yet.
    ///
    /// # Errors
    /// Invalid paths, an insecure configuration directory, or a failure to
    /// start a thread.
    #[uniffi::constructor]
    pub fn new(
        config: NodeConfig,
        listener: Arc<dyn EventListener>,
        discovery: Option<Arc<dyn PlatformDiscovery>>,
    ) -> Result<Arc<Self>, DropumiError> {
        Self::with_options(config, listener, discovery, NodeOptions::default())
    }

    /// This device's identity, created on first use.
    ///
    /// # Errors
    /// The identity cannot be read or created.
    pub fn identity(&self) -> Result<IdentityInfo, DropumiError> {
        self.engine.check_open()?;
        Ok(identity_info(&self.engine.identity.load_or_create()?))
    }

    /// Renames this device (sanitized; at most 64 bytes).
    ///
    /// A name saved without a confirmed directory sync is applied: the call
    /// succeeds and a `Warning` event says it may not survive a crash.
    ///
    /// # Errors
    /// `Busy` while receiving or while an operation runs; storage errors.
    pub fn set_device_name(&self, name: String) -> Result<IdentityInfo, DropumiError> {
        self.engine.check_open()?;
        self.engine.check_quiet()?;
        let store = &self.engine.identity;
        let id = self
            .engine
            .applied(store.set_name(&DeviceName::sanitize(&name)), || {
                store.load_or_create()
            })?;
        Ok(identity_info(&id))
    }

    /// Replaces this device's key. Every paired device must pair again.
    ///
    /// A key saved without a confirmed directory sync is applied: the call
    /// succeeds with the new identity and a `Warning` event says it may not
    /// survive a crash.
    ///
    /// # Errors
    /// `Busy` while receiving or while an operation runs; storage errors.
    pub fn reset_identity(&self) -> Result<IdentityInfo, DropumiError> {
        self.engine.check_open()?;
        self.engine.check_quiet()?;
        let store = &self.engine.identity;
        let id = self
            .engine
            .applied(store.reset(), || store.load_or_create())?;
        Ok(identity_info(&id))
    }

    /// Paired devices.
    ///
    /// # Errors
    /// The trust store cannot be read.
    pub fn trusted_devices(&self) -> Result<Vec<TrustedDeviceInfo>, DropumiError> {
        self.engine.check_open()?;
        Ok(trust::list(&self.engine.trust)?
            .iter()
            .map(TrustedDeviceInfo::from)
            .collect())
    }

    /// Trusts a device by its full fingerprint (52 characters), compared
    /// with the other device out of band.
    ///
    /// A device saved without a confirmed directory sync is trusted: the
    /// call succeeds and a `Warning` event says it may not survive a crash.
    ///
    /// # Errors
    /// An invalid fingerprint, or storage errors.
    pub fn trust_fingerprint(
        &self,
        fingerprint: String,
        alias: Option<String>,
    ) -> Result<TrustedDeviceInfo, DropumiError> {
        self.engine.check_open()?;
        let fp = parse_full(&fingerprint)?;
        let store = &self.engine.trust;
        let added = trust::add_manual(
            store,
            &SystemClock,
            fp,
            alias.as_deref().map(DeviceName::sanitize),
        );
        let device = self
            .engine
            .applied(added.map(Some), || store.get(&fp))?
            // Forgotten again by another call before the re-read.
            .ok_or_else(|| DropumiError::NotFound {
                detail: "the device was forgotten again before it could be read back".into(),
            })?;
        Ok(TrustedDeviceInfo::from(&device))
    }

    /// Renames a paired device; returns whether it is paired. Never adds a
    /// device.
    ///
    /// A name saved without a confirmed directory sync is applied: the call
    /// succeeds and a `Warning` event says it may not survive a crash.
    ///
    /// # Errors
    /// An invalid fingerprint, or storage errors.
    pub fn rename_trusted(&self, fingerprint: String, alias: String) -> Result<bool, DropumiError> {
        self.engine.check_open()?;
        let fp = parse_full(&fingerprint)?;
        let store = &self.engine.trust;
        self.engine.applied(
            trust::rename(store, &fp, DeviceName::sanitize(&alias)),
            || Ok(store.get(&fp)?.is_some()),
        )
    }

    /// Forgets a paired device; returns whether it was paired.
    ///
    /// A removal saved without a confirmed directory sync is applied: the
    /// call succeeds and a `Warning` event says it may not survive a crash.
    /// Whether the device was paired is then the state read just before the
    /// removal.
    ///
    /// # Errors
    /// An invalid fingerprint, or storage errors.
    pub fn untrust(&self, fingerprint: String) -> Result<bool, DropumiError> {
        self.engine.check_open()?;
        let fp = parse_full(&fingerprint)?;
        let store = &self.engine.trust;
        // After a removal saved without a confirmed directory sync the store
        // cannot say whether the device existed, so read it first.
        let was_paired = store.get(&fp)?.is_some();
        self.engine
            .applied(trust::remove(store, &fp), || Ok(was_paired))
    }

    /// Starts browsing for nearby devices (`PeerFound`, `PeerLost`). Browse
    /// only while a device list is visible.
    ///
    /// # Errors
    /// `DiscoveryUnavailable` without a discovery adapter or when the
    /// platform refuses.
    pub fn start_discovery(&self) -> Result<(), DropumiError> {
        self.engine.check_open()?;
        let Some(platform) = self.engine.platform.clone() else {
            return Err(DropumiError::DiscoveryUnavailable {
                detail: "no discovery adapter".into(),
            });
        };
        if lock(&self.engine.state).browsing {
            return Ok(());
        }
        let sink = self.engine.peers.sink();
        app_call(move || platform.start_browse(sink))?.map_err(|e| {
            DropumiError::DiscoveryUnavailable {
                detail: e.to_string(),
            }
        })?;
        lock(&self.engine.state).browsing = true;
        Ok(())
    }

    /// Stops browsing and forgets the nearby list.
    pub fn stop_discovery(&self) {
        self.engine.stop_browsing();
    }

    /// Devices seen while browsing.
    #[must_use]
    pub fn nearby_peers(&self) -> Vec<NearbyPeer> {
        self.engine.peers.list()
    }

    /// Starts receiving into the receive directory.
    ///
    /// # Errors
    /// `InvalidState` without a receive directory, `Busy` if a receiver or a
    /// pairing window is running, or binding failures.
    pub fn start_receiving(&self, options: ReceiveOptions) -> Result<ReceiverInfo, DropumiError> {
        self.engine.check_open()?;
        Engine::start_receiver(&self.engine, options)
    }

    /// Stops accepting connections: the listener closes and the
    /// advertisement is withdrawn at once; running transfers finish, then
    /// the receiver stops (`ReceiverStopped`). Use it when the app goes to
    /// the background.
    pub fn stop_accepting(&self) {
        self.engine.stop_receiver(false);
    }

    /// Stops receiving; running transfers are cancelled.
    pub fn stop_receiving(&self) {
        self.engine.stop_receiver(true);
    }

    /// Changes who may send, for sessions that start from now on.
    ///
    /// # Errors
    /// `InvalidState` when not receiving; an invalid Everyone window.
    pub fn set_receive_policy(
        &self,
        policy: ReceivePolicy,
        accept: AcceptMode,
    ) -> Result<ReceiverInfo, DropumiError> {
        self.engine.check_open()?;
        let domain = domain_policy(policy, accept)?;
        let mut state = lock(&self.engine.state);
        let ReceiverState::Running(r) = &mut state.receiver else {
            return Err(DropumiError::state("not receiving"));
        };
        r.control.set_policy(domain);
        r.options.policy = policy;
        r.options.accept = accept;
        let (p, a, until) = policy_info(&domain, now_secs());
        r.info.policy = p;
        r.info.accept = a;
        r.info.everyone_until = until;
        Ok(r.info.clone())
    }

    /// Cancels one incoming transfer; returns whether it was running.
    #[must_use]
    pub fn cancel_incoming(&self, transfer_id: String) -> bool {
        let Some(id) = parse_transfer_id(&transfer_id) else {
            return false;
        };
        match &lock(&self.engine.state).receiver {
            ReceiverState::Running(r) => r.canceller.cancel(&id),
            _ => false,
        }
    }

    /// Cancels every incoming transfer; the receiver keeps running.
    pub fn cancel_all_incoming(&self) {
        if let ReceiverState::Running(r) = &lock(&self.engine.state).receiver {
            r.canceller.cancel_active();
        }
    }

    /// Answers an offer prompt.
    ///
    /// # Errors
    /// `PromptNotFound` if it is not waiting (answered, expired, never
    /// shown); `PromptKindMismatch` for a code prompt.
    pub fn answer_offer(&self, prompt_id: u64, accept: bool) -> Result<(), DropumiError> {
        self.engine
            .prompts
            .answer(prompt_id, AnswerKind::Offer, accept)
    }

    /// Answers a code prompt: whether both screens show the same code.
    ///
    /// # Errors
    /// `PromptNotFound` if it is not waiting; `PromptKindMismatch` for an
    /// offer prompt.
    pub fn answer_sas(&self, prompt_id: u64, confirmed: bool) -> Result<(), DropumiError> {
        self.engine
            .prompts
            .answer(prompt_id, AnswerKind::Code, confirmed)
    }

    /// Prompts waiting now, oldest first.
    #[must_use]
    pub fn pending_prompts(&self) -> Vec<Prompt> {
        self.engine.prompts.waiting()
    }

    /// A page of an offer's files (prompts list the first 200).
    ///
    /// # Errors
    /// `PromptNotFound` or `PromptKindMismatch`.
    pub fn offer_files(
        &self,
        prompt_id: u64,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<OfferedFile>, DropumiError> {
        self.engine.prompts.offer_files(prompt_id, offset, limit)
    }

    /// Sends files.
    ///
    /// # Errors
    /// Invalid targets or items (including Dropumi's own configuration
    /// files), or an unreadable identity.
    pub fn send(
        &self,
        target: SendTarget,
        items: Vec<SendItem>,
        options: SendOptions,
    ) -> Result<Arc<OperationHandle>, DropumiError> {
        self.engine.check_open()?;
        Engine::send(&self.engine, &target, items, options)
    }

    /// Pairs with a device: both users compare a code.
    ///
    /// # Errors
    /// Invalid targets, or an unreadable identity.
    pub fn pair_with(&self, target: SendTarget) -> Result<Arc<OperationHandle>, DropumiError> {
        self.engine.check_open()?;
        Engine::pair(&self.engine, &target)
    }

    /// Lets one device pair with this one, for `minutes` (1–10). A running
    /// receiver pauses (it must be idle) and resumes afterwards.
    ///
    /// # Errors
    /// `Busy` while an incoming transfer runs or another window is open.
    pub fn open_pairing_window(&self, minutes: u32) -> Result<Arc<OperationHandle>, DropumiError> {
        self.engine.check_open()?;
        Engine::pairing_window(&self.engine, minutes)
    }

    /// Everything at once.
    #[must_use]
    pub fn snapshot(&self) -> NodeSnapshot {
        self.engine.snapshot()
    }

    /// Stops everything: operations and transfers are cancelled, prompts
    /// withdrawn, advertising stopped. Waits a few seconds at most. Later
    /// calls fail with `ShutDown`.
    pub fn shutdown(&self) {
        self.engine.shutdown();
    }
}

/// Settings that are not part of the app API (tests on one host).
#[derive(Debug, Clone, Copy, Default)]
pub struct NodeOptions {
    /// Protocol timeouts.
    pub timeouts: Timeouts,
    /// Keep loopback addresses of discovered devices. Off for apps: a
    /// device on the network never has a loopback address.
    pub discovered_loopback: bool,
}

impl DropumiNode {
    /// Like [`DropumiNode::new`], with [`NodeOptions`] (Rust only).
    ///
    /// # Errors
    /// As [`DropumiNode::new`].
    pub fn with_options(
        config: NodeConfig,
        listener: Arc<dyn EventListener>,
        discovery: Option<Arc<dyn PlatformDiscovery>>,
        options: NodeOptions,
    ) -> Result<Arc<Self>, DropumiError> {
        let timeouts = options.timeouts;
        let config_dir = absolute_dir(&config.config_dir, "configuration directory")?;
        let receive_dir = match config.receive_dir.as_deref() {
            Some(d) => {
                let dir = absolute_dir(d, "receive directory")?;
                if !dir.is_dir() {
                    return Err(DropumiError::invalid(
                        "the receive directory does not exist",
                    ));
                }
                Some(dir)
            }
            None => None,
        };
        let identity = FileIdentityStore::open(&config_dir)?;
        let trust_store = TrustFile::open(&config_dir)?;
        let mailbox = Arc::new(Mailbox::default());
        let runtime = Runtime::start()?;
        let dispatcher = spawn_dispatcher(Arc::clone(&mailbox), listener)
            .map_err(|e| DropumiError::internal(format!("cannot start the event thread: {e}")))?;
        let dispatcher_thread = dispatcher.thread().id();
        let peer_trust = trust_store.clone();
        let peers = PeerTable::new(
            Arc::clone(&mailbox),
            Box::new(move || peer_trust.list().unwrap_or_default()),
            options.discovered_loopback,
        );
        let engine = Arc::new(Engine {
            runtime,
            prompts: PromptRegistry::new(Arc::clone(&mailbox), timeouts),
            mailbox,
            dispatcher: Mutex::new(Some(dispatcher)),
            dispatcher_thread: Mutex::new(Some(dispatcher_thread)),
            config_dir,
            receive_dir,
            identity,
            trust: trust_store,
            platform: discovery,
            peers,
            incoming: Arc::default(),
            state: Mutex::new(State {
                receiver: ReceiverState::Idle,
                advertised: None,
                pairing_window: false,
                browsing: false,
                ops: HashMap::new(),
                generation: 0,
            }),
            next_op: AtomicU64::new(1),
            shut_down: AtomicBool::new(false),
            timeouts,
        });
        Ok(Arc::new(Self { engine }))
    }
}

impl Drop for DropumiNode {
    fn drop(&mut self) {
        // Never blocks: cancels and lets the threads end on their own.
        self.engine.release();
    }
}

fn identity_info(id: &LocalIdentity) -> IdentityInfo {
    IdentityInfo {
        name: id.name().as_str().to_owned(),
        fingerprint: id.fingerprint().to_full(),
        fingerprint_display: id.fingerprint().to_display(),
        fingerprint_hint: id.fingerprint().hint(),
    }
}

fn absolute_dir(raw: &str, what: &str) -> Result<PathBuf, DropumiError> {
    let p = PathBuf::from(raw);
    if !p.is_absolute() {
        return Err(DropumiError::invalid(format!(
            "the {what} must be an absolute path"
        )));
    }
    Ok(p)
}

fn parse_full(s: &str) -> Result<Fingerprint, DropumiError> {
    Fingerprint::parse_full(s).map_err(|e| DropumiError::invalid(e.to_string()))
}

fn parse_transfer_id(hex: &str) -> Option<TransferId> {
    let bytes = hex.trim().as_bytes();
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, pair) in bytes.chunks(2).enumerate() {
        let text = std::str::from_utf8(pair).ok()?;
        *out.get_mut(i)? = u8::from_str_radix(text, 16).ok()?;
    }
    Some(TransferId::from_bytes(out))
}

fn domain_policy(
    policy: ReceivePolicy,
    accept: AcceptMode,
) -> Result<TransferPolicy, DropumiError> {
    let mode = match policy {
        ReceivePolicy::TrustedOnly => PolicyMode::TrustedOnly,
        ReceivePolicy::Everyone { minutes } => {
            if minutes == 0 || minutes > MAX_EVERYONE_MINUTES {
                return Err(DropumiError::invalid(
                    "an Everyone window lasts 1 to 60 minutes",
                ));
            }
            PolicyMode::Everyone {
                until: SystemClock
                    .now()
                    .saturating_add(Duration::from_secs(u64::from(minutes) * 60)),
            }
        }
    };
    Ok(TransferPolicy {
        mode,
        accept: accept.to_domain(),
    })
}

/// Runs app code on its own thread and waits for it.
fn app_call<T, F>(f: F) -> Result<T, DropumiError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    on_app_thread(f)
        .blocking_recv()
        .map_err(|_| DropumiError::internal("app callback did not return"))?
}

/// What a pump knows about the events it forwards.
#[derive(Debug, Clone, Copy)]
enum PumpCtx {
    Outgoing(u64),
    Incoming,
    Pairing(u64),
}

impl PumpCtx {
    const fn operation(self) -> Option<u64> {
        match self {
            Self::Outgoing(op) | Self::Pairing(op) => Some(op),
            Self::Incoming => None,
        }
    }
}

/// Maps a core event to an app event (`None`: not shown).
fn map_event(event: AppEvent, ctx: PumpCtx) -> Option<NodeEvent> {
    let operation = ctx.operation();
    Some(match event {
        AppEvent::Connected { peer, .. } => NodeEvent::Connected {
            operation,
            peer: PeerInfo::from(&peer),
        },
        AppEvent::Sas { peer, code } => NodeEvent::SessionCode {
            operation,
            peer: PeerInfo::from(&peer),
            code: code.as_digits(),
        },
        AppEvent::OfferSent {
            transfer_id,
            peer,
            files,
            total_bytes,
        } => NodeEvent::OfferSent {
            operation: operation?,
            transfer_id: transfer_id.to_hex(),
            peer: PeerInfo::from(&peer),
            files,
            total_bytes,
        },
        AppEvent::OfferReceived {
            transfer_id,
            peer,
            files,
            total_bytes,
            ..
        } => NodeEvent::OfferReceived {
            transfer_id: transfer_id.to_hex(),
            peer: PeerInfo::from(&peer),
            files: u32::try_from(files.len()).unwrap_or(u32::MAX),
            total_bytes,
        },
        AppEvent::Accepted { transfer_id } => NodeEvent::TransferAccepted {
            operation,
            transfer_id: transfer_id.to_hex(),
        },
        AppEvent::Progress {
            transfer_id,
            progress,
        } => NodeEvent::TransferProgress {
            operation,
            transfer_id: transfer_id.to_hex(),
            progress: Progress {
                bytes_done: progress.bytes_done(),
                bytes_total: progress.bytes_total(),
                files_done: progress.files_done(),
                files_total: progress.files_total(),
            },
        },
        AppEvent::FileSent {
            transfer_id,
            index,
            status,
        } => NodeEvent::FileSent {
            operation: operation?,
            transfer_id: transfer_id.to_hex(),
            index,
            status: status.into(),
        },
        AppEvent::FileSaved {
            transfer_id,
            index,
            saved_as,
            size,
            blake3,
            ..
        } => NodeEvent::FileSaved {
            transfer_id: transfer_id.to_hex(),
            index,
            saved_as,
            size,
            blake3,
        },
        AppEvent::FileFailed {
            transfer_id,
            index,
            status,
        } => NodeEvent::FileFailed {
            transfer_id: transfer_id.to_hex(),
            index,
            status: status.into(),
        },
        AppEvent::SessionClosed { peer, code } => NodeEvent::SessionClosed {
            peer: peer.as_ref().map(PeerInfo::from),
            code: code.to_owned(),
        },
        AppEvent::PolicyChanged { policy } => {
            let (policy, accept, everyone_until) = policy_info(&policy, now_secs());
            NodeEvent::PolicyChanged {
                policy,
                accept,
                everyone_until,
            }
        }
        AppEvent::Finished(outcome) => NodeEvent::TransferFinished {
            operation,
            direction: match ctx {
                PumpCtx::Incoming => Direction::Incoming,
                _ => Direction::Outgoing,
            },
            result: TransferResult::from(&outcome),
        },
        AppEvent::DiscoveryUnavailable { reason } => NodeEvent::DiscoveryUnavailable { reason },
        AppEvent::Warning { message } => NodeEvent::Warning { message },
        _ => return None,
    })
}

/// Forwards core events to the mailbox until the use case drops its sink.
async fn pump(
    mut rx: mpsc::Receiver<AppEvent>,
    ctx: PumpCtx,
    mailbox: Arc<Mailbox>,
    incoming: Arc<Mutex<HashSet<String>>>,
) {
    while let Some(event) = rx.recv().await {
        if let PumpCtx::Incoming = ctx {
            match &event {
                AppEvent::OfferReceived { transfer_id, .. } => {
                    lock(&incoming).insert(transfer_id.to_hex());
                }
                AppEvent::Finished(outcome) => {
                    if let Some(id) = TransferResult::from(outcome).transfer_id {
                        lock(&incoming).remove(&id);
                    }
                }
                _ => {}
            }
        }
        if let Some(e) = map_event(event, ctx) {
            mailbox.push(e);
        }
    }
}

fn usecase_code(e: &UseCaseError) -> ErrorCode {
    match e {
        UseCaseError::Transfer(t) => ErrorCode::of(t),
        _ => ErrorCode::Internal,
    }
}

impl Engine {
    fn check_open(&self) -> Result<(), DropumiError> {
        if self.shut_down.load(Ordering::SeqCst) {
            return Err(DropumiError::ShutDown);
        }
        Ok(())
    }

    /// Identity changes would not reach running use cases.
    fn check_quiet(&self) -> Result<(), DropumiError> {
        let state = lock(&self.state);
        let running_op = state.ops.values().any(|o| o.result.is_none());
        if !matches!(state.receiver, ReceiverState::Idle) || state.pairing_window || running_op {
            return Err(DropumiError::busy(
                "stop receiving and wait for running operations first",
            ));
        }
        Ok(())
    }

    /// The result of a configuration write ([`settle`]). A change saved
    /// without a confirmed directory sync is applied: the re-read state is
    /// returned and a `Warning` event with fixed text is published.
    fn applied<T>(
        &self,
        written: Result<T, StoreError>,
        reread: impl FnOnce() -> Result<T, StoreError>,
    ) -> Result<T, DropumiError> {
        let (state, warning) = settle(written, reread)?;
        if let Some(message) = warning {
            self.mailbox.push(NodeEvent::Warning {
                message: message.to_owned(),
            });
        }
        Ok(state)
    }

    /// Dropumi's own configuration files, which are never sent.
    fn config_files(&self) -> Vec<PathBuf> {
        [KEY_FILE, DEVICE_FILE, TRUST_FILE, LOCK_FILE]
            .iter()
            .map(|f| self.config_dir.join(f))
            .collect()
    }

    /// An absolute path the user picked, refused if it lies inside the
    /// configuration directory.
    fn checked_path(&self, raw: &str) -> Result<PathBuf, DropumiError> {
        let path = PathBuf::from(raw);
        if !path.is_absolute() {
            return Err(DropumiError::invalid("file paths must be absolute"));
        }
        let canonical = std::fs::canonicalize(&path)
            .map_err(|e| DropumiError::invalid(format!("cannot read a file: {e}")))?;
        let config =
            std::fs::canonicalize(&self.config_dir).unwrap_or_else(|_| self.config_dir.clone());
        if canonical.starts_with(&config) {
            return Err(DropumiError::invalid(
                "refusing to send a Dropumi configuration file",
            ));
        }
        Ok(path)
    }

    /// Candidates and identity pin for a target (see [`SendTarget`]).
    fn resolve_target(
        &self,
        target: &SendTarget,
    ) -> Result<(Vec<Target>, Option<FingerprintPrefix>), DropumiError> {
        match target {
            SendTarget::Nearby { instance } => {
                let peer = self
                    .peers
                    .get(instance)
                    .ok_or_else(|| DropumiError::NotFound {
                        detail: "that device is no longer nearby".into(),
                    })?;
                let targets: Vec<Target> = peer
                    .addrs
                    .iter()
                    .take(MAX_ADDRESSES)
                    .map(|ip| Target::Addr(SocketAddr::new(*ip, peer.port)))
                    .collect();
                // A hint that names exactly one paired device pins that
                // device's full key; the handshake then checks it before
                // anything is sent. Without a match the send is unverified.
                let trusted = trust::list(&self.trust)?;
                let expect = peer
                    .fp_hint
                    .as_deref()
                    .and_then(|h| single_trust_match(h, &trusted))
                    .map(|d| FingerprintPrefix::of(&d.fingerprint, d.fingerprint.to_full().len()));
                Ok((targets, expect))
            }
            SendTarget::Address {
                address,
                expect_fingerprint,
            } => {
                let target =
                    Target::parse(address).map_err(|e| DropumiError::invalid(e.to_string()))?;
                if !matches!(target, Target::Addr(_) | Target::Host { .. }) {
                    return Err(DropumiError::invalid(
                        "enter an address such as 192.168.1.20:47321",
                    ));
                }
                let expect = expect_fingerprint
                    .as_deref()
                    .map(FingerprintPrefix::parse_pin)
                    .transpose()
                    .map_err(|e| DropumiError::invalid(e.to_string()))?;
                Ok((vec![target], expect))
            }
        }
    }

    fn new_op(&self, kind: OperationKind) -> (u64, CancellationToken) {
        let id = self.next_op.fetch_add(1, Ordering::Relaxed);
        let cancel = CancellationToken::new();
        let mut state = lock(&self.state);
        if state.ops.len() > KEEP_FINISHED_OPS {
            let mut finished: Vec<(u64, u64)> = state
                .ops
                .iter()
                .filter(|(_, o)| o.result.is_some())
                .map(|(k, o)| (o.order, *k))
                .collect();
            finished.sort_unstable();
            let excess = state.ops.len().saturating_sub(KEEP_FINISHED_OPS);
            for (_, k) in finished.into_iter().take(excess) {
                state.ops.remove(&k);
            }
        }
        state.ops.insert(
            id,
            Op {
                kind,
                cancel: cancel.clone(),
                result: None,
                order: id,
            },
        );
        (id, cancel)
    }

    fn finish_op(&self, id: u64, result: OperationResult) {
        if let Some(op) = lock(&self.state).ops.get_mut(&id) {
            op.result = Some(result);
        }
    }

    fn handle(self: &Arc<Self>, id: u64, kind: OperationKind) -> Arc<OperationHandle> {
        Arc::new(OperationHandle {
            id,
            kind,
            engine: Arc::downgrade(self),
        })
    }

    fn send(
        self: &Arc<Self>,
        target: &SendTarget,
        items: Vec<SendItem>,
        options: SendOptions,
    ) -> Result<Arc<OperationHandle>, DropumiError> {
        if items.is_empty() || items.len() > MAX_FILES {
            return Err(DropumiError::invalid("send 1 to 10,000 files"));
        }
        let identity = self.identity.load_or_create()?;
        let (targets, expect) = self.resolve_target(target)?;
        let mut source = CompositeSource::new(LocalFileSource::with_denied(self.config_files()));
        let mut paths = Vec::with_capacity(items.len());
        for item in items {
            match item {
                SendItem::Path { path } => paths.push(self.checked_path(&path)?),
                SendItem::Stream { file } => {
                    paths.push(source.add_stream(file).map_err(DropumiError::invalid)?);
                }
            }
        }
        let request = SendRequest {
            targets,
            paths,
            expect,
            allow_unverified: options.allow_unverified,
            throttle_bps: None,
            discover_timeout: Duration::ZERO,
            allow_loopback: true,
        };
        let (op, cancel) = self.new_op(OperationKind::Send);
        let engine = Arc::downgrade(self);
        let (mailbox, prompts, trust, incoming) = (
            Arc::clone(&self.mailbox),
            Arc::clone(&self.prompts),
            self.trust.clone(),
            Arc::clone(&self.incoming),
        );
        let timeouts = self.timeouts;
        self.runtime.handle().spawn(async move {
            let (tx, rx) = mpsc::channel(256);
            let pump = tokio::spawn(pump(
                rx,
                PumpCtx::Outgoing(op),
                Arc::clone(&mailbox),
                incoming,
            ));
            let interaction = UiInteraction::new(prompts, Some(op));
            let outcome = {
                let use_case = SendFiles::new(SendDeps {
                    connector: &TcpConnector,
                    discovery: None::<&NoDiscovery>,
                    source: &source,
                    trust: &trust,
                    interaction: &interaction,
                    clock: &SystemClock,
                    identity: &identity,
                    timeouts,
                    events: EventSink::new(tx),
                });
                use_case.run(request, cancel).await
            };
            let _ = pump.await;
            let result = match &outcome {
                Ok(o) => TransferResult::from(o),
                Err(e) => TransferResult::failed(usecase_code(e)),
            };
            if let Some(engine) = engine.upgrade() {
                engine.finish_op(
                    op,
                    OperationResult::Transfer {
                        result: result.clone(),
                    },
                );
            }
            mailbox.push(NodeEvent::TransferFinished {
                operation: Some(op),
                direction: Direction::Outgoing,
                result,
            });
        });
        Ok(self.handle(op, OperationKind::Send))
    }

    fn pair(self: &Arc<Self>, target: &SendTarget) -> Result<Arc<OperationHandle>, DropumiError> {
        let identity = self.identity.load_or_create()?;
        let (targets, expect) = self.resolve_target(target)?;
        let (op, cancel) = self.new_op(OperationKind::Pair);
        let engine = Arc::downgrade(self);
        let (mailbox, prompts, trust, incoming) = (
            Arc::clone(&self.mailbox),
            Arc::clone(&self.prompts),
            self.trust.clone(),
            Arc::clone(&self.incoming),
        );
        let timeouts = self.timeouts;
        self.runtime.handle().spawn(async move {
            let (tx, rx) = mpsc::channel(256);
            let pump = tokio::spawn(pump(
                rx,
                PumpCtx::Pairing(op),
                Arc::clone(&mailbox),
                incoming,
            ));
            let interaction = UiInteraction::new(prompts, Some(op));
            let outcome = {
                let deps = PairDeps {
                    trust: &trust,
                    interaction: &interaction,
                    clock: &SystemClock,
                    identity: &identity,
                    timeouts,
                    events: EventSink::new(tx),
                };
                pair_with(
                    &TcpConnector,
                    None::<&NoDiscovery>,
                    &deps,
                    ResolveParams {
                        targets: &targets,
                        explicit: expect,
                        discover_timeout: Duration::ZERO,
                        allow_loopback: true,
                    },
                    cancel,
                )
                .await
            };
            let _ = pump.await;
            let result = pairing_result(outcome);
            finish_pairing(&engine, &mailbox, op, result);
        });
        Ok(self.handle(op, OperationKind::Pair))
    }

    fn pairing_window(
        self: &Arc<Self>,
        minutes: u32,
    ) -> Result<Arc<OperationHandle>, DropumiError> {
        if minutes == 0 || minutes > MAX_PAIRING_MINUTES {
            return Err(DropumiError::invalid(
                "a pairing window lasts 1 to 10 minutes",
            ));
        }
        let identity = self.identity.load_or_create()?;
        // Pause a running receiver; it must be idle.
        let paused = {
            let mut state = lock(&self.state);
            if state.pairing_window {
                return Err(DropumiError::busy("a pairing window is already open"));
            }
            match &state.receiver {
                ReceiverState::Stopping { .. } => {
                    return Err(DropumiError::busy("the receiver is stopping"));
                }
                ReceiverState::Running(_) if !lock(&self.incoming).is_empty() => {
                    return Err(DropumiError::busy("a transfer is running"));
                }
                _ => {}
            }
            state.pairing_window = true;
            match std::mem::replace(&mut state.receiver, ReceiverState::Idle) {
                ReceiverState::Running(r) => {
                    r.cancel.cancel();
                    state.receiver = ReceiverState::Stopping {
                        generation: r.generation,
                        done: r.done.clone(),
                    };
                    Some((r.options, r.info.port, r.done))
                }
                other => {
                    state.receiver = other;
                    None
                }
            }
        };
        self.withdraw_advertisement();
        let (op, cancel) = self.new_op(OperationKind::PairingWindow);
        let engine = Arc::downgrade(self);
        let (mailbox, prompts, trust, incoming, platform) = (
            Arc::clone(&self.mailbox),
            Arc::clone(&self.prompts),
            self.trust.clone(),
            Arc::clone(&self.incoming),
            self.platform.clone(),
        );
        let timeouts = self.timeouts;
        let window = Duration::from_secs(u64::from(minutes) * 60);
        self.runtime.handle().spawn(async move {
            let port = match &paused {
                Some((_, port, done)) => {
                    let mut done = done.clone();
                    let _ =
                        tokio::time::timeout(Duration::from_secs(10), done.wait_for(|d| *d)).await;
                    *port
                }
                None => DEFAULT_PORT,
            };
            let result = match TcpListenerAdapter::bind(
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                port,
                false,
            )
            .await
            {
                Err(e) => PairingResult::Failed {
                    error: ErrorCode::of(&TransferError::from(e)),
                },
                Ok(listener) => {
                    let port = listener.port();
                    let advertised = match (&platform, engine.upgrade()) {
                        (Some(p), Some(engine)) => {
                            engine.advertise(Arc::clone(p), &identity, port).await
                        }
                        _ => None,
                    };
                    mailbox.push(NodeEvent::PairingWindowOpened {
                        operation: op,
                        port,
                    });
                    let (tx, rx) = mpsc::channel(256);
                    let pump = tokio::spawn(pump(
                        rx,
                        PumpCtx::Pairing(op),
                        Arc::clone(&mailbox),
                        incoming,
                    ));
                    let interaction = UiInteraction::new(prompts, Some(op));
                    let outcome = {
                        let deps = PairDeps {
                            trust: &trust,
                            interaction: &interaction,
                            clock: &SystemClock,
                            identity: &identity,
                            timeouts,
                            events: EventSink::new(tx),
                        };
                        pair_listen(listener, &deps, window, cancel).await
                    };
                    let _ = pump.await;
                    if advertised.is_some()
                        && let Some(engine) = engine.upgrade()
                    {
                        engine.withdraw_advertisement();
                    }
                    pairing_result(outcome)
                }
            };
            finish_pairing(&engine, &mailbox, op, result);
            let Some(engine) = engine.upgrade() else {
                return;
            };
            lock(&engine.state).pairing_window = false;
            if let Some((options, port, _)) = paused {
                // Resume on the same port the receiver had.
                let options = ReceiveOptions {
                    port: Some(port),
                    ..options
                };
                let restart = Arc::clone(&engine);
                let restarted =
                    on_app_thread(move || Engine::start_receiver(&restart, options)).await;
                if !matches!(restarted, Ok(Ok(Ok(_)))) {
                    mailbox.push(NodeEvent::ReceiverStopped {
                        reason: ReceiverStopReason::Internal,
                    });
                }
            }
        });
        Ok(self.handle(op, OperationKind::PairingWindow))
    }

    /// Registers this device's advertisement through the platform.
    async fn advertise(
        &self,
        platform: Arc<dyn PlatformDiscovery>,
        identity: &LocalIdentity,
        port: u16,
    ) -> Option<String> {
        let ad = service_ad(&Advertisement {
            name: identity.name().clone(),
            fp_hint: identity.fingerprint().hint(),
            port,
            versions: SUPPORTED_VERSIONS.to_vec(),
        });
        // Filter our own records out before the platform reports them.
        self.peers.add_own(&ad.instance);
        let requested = ad.instance.clone();
        let registered = on_app_thread(move || platform.advertise(ad)).await;
        match registered {
            Ok(Ok(Ok(instance))) => {
                self.peers.add_own(&instance);
                lock(&self.state).advertised = Some(instance.clone());
                Some(instance)
            }
            other => {
                self.peers.remove_own(&requested);
                let reason = match other {
                    Ok(Ok(Err(e))) => e.to_string(),
                    _ => "advertising failed".into(),
                };
                self.mailbox.push(NodeEvent::DiscoveryUnavailable {
                    reason: crate::types::display_safe(&reason),
                });
                None
            }
        }
    }

    /// Withdraws the advertisement, if any (on its own thread; not awaited).
    fn withdraw_advertisement(&self) {
        let instance = lock(&self.state).advertised.take();
        if let (Some(instance), Some(platform)) = (instance, self.platform.clone()) {
            self.peers.remove_own(&instance);
            drop(on_app_thread(move || platform.withdraw(instance)));
        }
    }

    fn start_receiver(
        self: &Arc<Self>,
        options: ReceiveOptions,
    ) -> Result<ReceiverInfo, DropumiError> {
        let Some(dir) = self.receive_dir.clone() else {
            return Err(DropumiError::state("this node has no receive directory"));
        };
        let policy = domain_policy(options.policy, options.accept)?;
        let identity = self.identity.load_or_create()?;
        let sink = ReceiveDir::open(&dir).map_err(|e| DropumiError::Transfer {
            code: ErrorCode::Storage,
            detail: e.to_string(),
        })?;
        let removed = remove_stale_parts(sink.path(), STALE_AFTER);
        if removed > 0 {
            tracing::info!(removed, "removed stale partial files");
        }
        let generation = {
            let mut state = lock(&self.state);
            if state.pairing_window {
                return Err(DropumiError::busy("a pairing window is open"));
            }
            match state.receiver {
                ReceiverState::Idle => {}
                ReceiverState::Running(_) => return Err(DropumiError::busy("already receiving")),
                ReceiverState::Stopping { .. } => {
                    return Err(DropumiError::busy("the receiver is stopping"));
                }
            }
            state.generation = state.generation.wrapping_add(1);
            let generation = state.generation;
            // Reserve the slot while binding.
            let (_, done) = watch::channel(false);
            state.receiver = ReceiverState::Stopping { generation, done };
            generation
        };
        let explicit = options.port.is_some();
        let port = options.port.unwrap_or(DEFAULT_PORT);
        let bound = self.runtime.block_on(async move {
            TcpListenerAdapter::bind(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port, explicit).await
        });
        let listener = match bound {
            Ok(Ok(l)) => l,
            Ok(Err(e)) => {
                self.release_slot(generation);
                return Err(DropumiError::Transfer {
                    code: ErrorCode::Internal,
                    detail: e.to_string(),
                });
            }
            Err(e) => {
                self.release_slot(generation);
                return Err(e);
            }
        };
        let port = listener.port();
        let instance = match (&self.platform, options.advertise) {
            (Some(p), true) => {
                let engine = Arc::clone(self);
                let platform = Arc::clone(p);
                let id = identity.clone();
                self.runtime
                    .block_on(async move { engine.advertise(platform, &id, port).await })
                    .ok()
                    .flatten()
            }
            _ => None,
        };
        let (tx, rx) = mpsc::channel(256);
        let receiver = ReceiveFiles::new(
            listener,
            ReceiveDeps {
                sink,
                trust: self.trust.clone(),
                interaction: UiInteraction::new(Arc::clone(&self.prompts), None),
                clock: SystemClock,
                identity,
                timeouts: self.timeouts,
                limits: AdmissionLimits::default(),
                events: EventSink::new(tx),
            },
        );
        let canceller = receiver.canceller();
        let control = receiver.control();
        let cancel = CancellationToken::new();
        let (done_tx, done) = watch::channel(false);
        let (p, a, everyone_until) = policy_info(&policy, now_secs());
        let info = ReceiverInfo {
            port,
            instance,
            policy: p,
            accept: a,
            everyone_until,
        };
        {
            let mut state = lock(&self.state);
            state.receiver = ReceiverState::Running(Running {
                generation,
                cancel: cancel.clone(),
                control,
                canceller,
                info: info.clone(),
                options,
                done,
            });
        }
        self.mailbox
            .push(NodeEvent::ReceiverStarted { info: info.clone() });
        let engine = Arc::downgrade(self);
        let (mailbox, incoming) = (Arc::clone(&self.mailbox), Arc::clone(&self.incoming));
        self.runtime.handle().spawn(async move {
            let pump = tokio::spawn(pump(
                rx,
                PumpCtx::Incoming,
                Arc::clone(&mailbox),
                Arc::clone(&incoming),
            ));
            let summary = receiver
                .run(
                    ReceiveConfig {
                        policy,
                        once: false,
                    },
                    cancel,
                )
                .await;
            let _ = pump.await;
            lock(&incoming).clear();
            let reason = match summary {
                Ok(_) => ReceiverStopReason::Stopped,
                Err(UseCaseError::Transfer(_)) => ReceiverStopReason::ListenerFailed,
                Err(_) => ReceiverStopReason::Internal,
            };
            if let Some(engine) = engine.upgrade() {
                let current = {
                    let mut state = lock(&engine.state);
                    let current = match &state.receiver {
                        ReceiverState::Running(r) => r.generation == generation,
                        ReceiverState::Stopping { generation: g, .. } => *g == generation,
                        ReceiverState::Idle => false,
                    };
                    if current {
                        state.receiver = ReceiverState::Idle;
                    }
                    current
                };
                if current && !lock(&engine.state).pairing_window {
                    engine.withdraw_advertisement();
                }
            }
            mailbox.push(NodeEvent::ReceiverStopped { reason });
            let _ = done_tx.send(true);
        });
        Ok(info)
    }

    /// Frees a reserved receiver slot after a failed start.
    fn release_slot(&self, generation: u64) {
        let mut state = lock(&self.state);
        if let ReceiverState::Stopping { generation: g, .. } = state.receiver
            && g == generation
        {
            state.receiver = ReceiverState::Idle;
        }
    }

    /// Stops the receiver: `cancel` cancels running transfers; otherwise
    /// they finish first.
    fn stop_receiver(&self, cancel: bool) {
        {
            let mut state = lock(&self.state);
            let ReceiverState::Running(r) =
                std::mem::replace(&mut state.receiver, ReceiverState::Idle)
            else {
                return;
            };
            if cancel {
                r.cancel.cancel();
            } else {
                r.control.stop_accepting();
            }
            state.receiver = ReceiverState::Stopping {
                generation: r.generation,
                done: r.done,
            };
        }
        self.withdraw_advertisement();
    }

    fn stop_browsing(&self) {
        let was = std::mem::replace(&mut lock(&self.state).browsing, false);
        if was && let Some(platform) = self.platform.clone() {
            drop(on_app_thread(move || platform.stop_browse()));
        }
        self.peers.clear();
    }

    fn snapshot(&self) -> NodeSnapshot {
        let state = lock(&self.state);
        let (receiver, stopping) = match &state.receiver {
            ReceiverState::Running(r) => (Some(r.info.clone()), false),
            ReceiverState::Stopping { .. } => (None, true),
            ReceiverState::Idle => (None, false),
        };
        let mut operations: Vec<OperationInfo> = state
            .ops
            .iter()
            .map(|(id, o)| OperationInfo {
                id: *id,
                kind: o.kind,
                result: o.result.clone(),
            })
            .collect();
        drop(state);
        operations.sort_by_key(|o| o.id);
        let mut incoming: Vec<String> = lock(&self.incoming).iter().cloned().collect();
        incoming.sort();
        NodeSnapshot {
            receiver,
            receiver_stopping: stopping,
            prompts: self.prompts.waiting(),
            nearby: self.peers.list(),
            operations,
            incoming_transfers: incoming,
        }
    }

    /// Cancels everything without waiting.
    fn cancel_all(&self) {
        let done = {
            let mut state = lock(&self.state);
            for op in state.ops.values() {
                op.cancel.cancel();
            }
            match std::mem::replace(&mut state.receiver, ReceiverState::Idle) {
                ReceiverState::Running(r) => {
                    r.cancel.cancel();
                    Some(r.done)
                }
                ReceiverState::Stopping { done, .. } => Some(done),
                ReceiverState::Idle => None,
            }
        };
        drop(done);
        self.prompts.withdraw_all();
        self.withdraw_advertisement();
        self.stop_browsing();
    }

    fn shutdown(&self) {
        if self.shut_down.swap(true, Ordering::SeqCst) {
            return;
        }
        let done = match &lock(&self.state).receiver {
            ReceiverState::Running(r) => Some(r.done.clone()),
            ReceiverState::Stopping { done, .. } => Some(done.clone()),
            ReceiverState::Idle => None,
        };
        self.cancel_all();
        if let Some(mut done) = done {
            // Let the receiver send its cancels and drain (bounded).
            let _ = self.runtime.block_on(async move {
                let _ = tokio::time::timeout(Duration::from_secs(5), done.wait_for(|d| *d)).await;
            });
        }
        self.runtime.stop(true);
        self.mailbox.close();
        let on_dispatcher = *lock(&self.dispatcher_thread) == Some(std::thread::current().id());
        if let Some(dispatcher) = lock(&self.dispatcher).take()
            && !on_dispatcher
        {
            let _ = dispatcher.join();
        }
    }

    /// The last handle went away: cancel and let threads end by themselves.
    fn release(&self) {
        if self.shut_down.swap(true, Ordering::SeqCst) {
            return;
        }
        self.cancel_all();
        self.runtime.stop(false);
        self.mailbox.close();
    }
}

fn pairing_result(outcome: Result<PairOutcome, UseCaseError>) -> PairingResult {
    match outcome {
        Ok(PairOutcome::Paired(peer)) => PairingResult::Paired {
            peer: PeerInfo::from(&peer),
        },
        Ok(PairOutcome::Failed(e)) => PairingResult::Failed {
            error: ErrorCode::of(&e),
        },
        Ok(PairOutcome::Cancelled) => PairingResult::Cancelled,
        Err(e) => PairingResult::Failed {
            error: usecase_code(&e),
        },
    }
}

fn finish_pairing(engine: &Weak<Engine>, mailbox: &Mailbox, op: u64, result: PairingResult) {
    if let Some(engine) = engine.upgrade() {
        engine.finish_op(
            op,
            OperationResult::Pairing {
                result: result.clone(),
            },
        );
    }
    mailbox.push(NodeEvent::PairingFinished {
        operation: op,
        result,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_ids_parse_from_hex() {
        let id = TransferId::from_bytes([0xab; 16]);
        assert_eq!(parse_transfer_id(&id.to_hex()), Some(id));
        assert_eq!(parse_transfer_id("zz"), None);
        assert_eq!(parse_transfer_id(&"g".repeat(32)), None);
    }

    #[test]
    fn everyone_window_bounds() {
        assert!(domain_policy(ReceivePolicy::Everyone { minutes: 0 }, AcceptMode::Prompt).is_err());
        assert!(
            domain_policy(ReceivePolicy::Everyone { minutes: 61 }, AcceptMode::Prompt).is_err()
        );
        assert!(domain_policy(ReceivePolicy::Everyone { minutes: 10 }, AcceptMode::Prompt).is_ok());
    }
}
