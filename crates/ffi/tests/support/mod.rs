// SPDX-License-Identifier: MPL-2.0
//! Helpers for the node tests: an event recorder, an in-memory discovery
//! service and app-read documents. Every node uses fresh temporary
//! directories, never a real configuration.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use dropumi_application::config::Timeouts;
use dropumi_ffi::{
    BrowseSink, CallbackError, DropumiNode, EventListener, ForeignFile, ForeignReader, NodeConfig,
    NodeEvent, NodeOptions, PlatformDiscovery, Prompt, ServiceAd, TxtEntry,
};
use sha2::{Digest, Sha256};

/// Deadline for one awaited event.
pub const WAIT: Duration = Duration::from_secs(30);

/// Records every event; tests wait on it with a deadline.
pub struct Recorder {
    tx: Mutex<mpsc::Sender<NodeEvent>>,
    /// Failures to report before succeeding.
    pub fail: AtomicUsize,
    /// Blocks every delivery while set.
    pub gate: Arc<Gate>,
}

impl EventListener for Recorder {
    fn on_events(&self, events: Vec<NodeEvent>) -> Result<(), CallbackError> {
        self.gate.wait_open();
        if take_one(&self.fail) {
            return Err(CallbackError::Failed {
                message: "listener failure".into(),
            });
        }
        let tx = self.tx.lock().unwrap();
        for e in events {
            let _ = tx.send(e);
        }
        Ok(())
    }
}

/// Decrements `n` if it is positive; returns whether it did.
pub fn take_one(n: &AtomicUsize) -> bool {
    let mut left = n.load(Ordering::SeqCst);
    loop {
        if left == 0 {
            return false;
        }
        match n.compare_exchange(left, left - 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return true,
            Err(now) => left = now,
        }
    }
}

/// A gate a callback can wait on (open by default).
#[derive(Default)]
pub struct Gate {
    closed: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    pub fn close(&self) {
        *self.closed.lock().unwrap() = true;
    }

    pub fn open(&self) {
        *self.closed.lock().unwrap() = false;
        self.changed.notify_all();
    }

    pub fn wait_open(&self) {
        let mut closed = self.closed.lock().unwrap();
        while *closed {
            closed = self.changed.wait(closed).unwrap();
        }
    }
}

/// The events a node delivered, read in order.
pub struct Events {
    rx: mpsc::Receiver<NodeEvent>,
    /// Everything read so far.
    pub seen: Vec<NodeEvent>,
}

impl Events {
    /// The next event matching `pred` (earlier events are kept in `seen`).
    pub fn wait(&mut self, what: &str, pred: impl Fn(&NodeEvent) -> bool) -> NodeEvent {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let e = self
                .rx
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("timed out waiting for {what}; seen: {:#?}", self.seen));
            self.seen.push(e.clone());
            if pred(&e) {
                return e;
            }
        }
    }

    /// Everything delivered so far without waiting.
    pub fn drain(&mut self) -> &[NodeEvent] {
        while let Ok(e) = self.rx.try_recv() {
            self.seen.push(e);
        }
        &self.seen
    }

    /// The next prompt.
    pub fn prompt(&mut self) -> Prompt {
        match self.wait("a prompt", |e| matches!(e, NodeEvent::PromptOpened { .. })) {
            NodeEvent::PromptOpened { prompt } => prompt,
            _ => unreachable!(),
        }
    }
}

/// A node in fresh temporary directories.
pub struct TestNode {
    pub node: Arc<DropumiNode>,
    pub events: Events,
    pub config: PathBuf,
    pub inbox: PathBuf,
    pub gate: Arc<Gate>,
    pub listener: Arc<Recorder>,
    _dir: tempfile::TempDir,
}

#[derive(Default)]
pub struct Setup {
    pub discovery: Option<Arc<FakeDiscovery>>,
    pub timeouts: Timeouts,
    pub listener_failures: usize,
}

pub fn node() -> TestNode {
    node_with(Setup::default())
}

pub fn node_with(setup: Setup) -> TestNode {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    let inbox = dir.path().join("inbox");
    std::fs::create_dir(&inbox).unwrap();
    let (tx, rx) = mpsc::channel();
    let gate = Arc::new(Gate::default());
    let listener = Arc::new(Recorder {
        tx: Mutex::new(tx),
        fail: AtomicUsize::new(setup.listener_failures),
        gate: Arc::clone(&gate),
    });
    let discovery: Option<Arc<dyn PlatformDiscovery>> =
        setup.discovery.map(|d| d as Arc<dyn PlatformDiscovery>);
    let node = DropumiNode::with_options(
        NodeConfig {
            config_dir: config.to_string_lossy().into_owned(),
            receive_dir: Some(inbox.to_string_lossy().into_owned()),
        },
        Arc::clone(&listener) as Arc<dyn EventListener>,
        discovery,
        NodeOptions {
            timeouts: setup.timeouts,
            discovered_loopback: true,
        },
    )
    .unwrap();
    TestNode {
        node,
        events: Events {
            rx,
            seen: Vec::new(),
        },
        config,
        inbox,
        gate,
        listener,
        _dir: dir,
    }
}

/// Makes each node trust the other (full fingerprints, as if compared).
pub fn pair_manually(a: &TestNode, b: &TestNode) {
    let (ia, ib) = (a.node.identity().unwrap(), b.node.identity().unwrap());
    a.node
        .trust_fingerprint(ib.fingerprint, Some("B".into()))
        .unwrap();
    b.node
        .trust_fingerprint(ia.fingerprint, Some("A".into()))
        .unwrap();
}

pub fn write_file(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, data).unwrap();
    p
}

pub fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| {
            u8::try_from(i % 251)
                .unwrap()
                .wrapping_mul(31)
                .wrapping_add(seed)
        })
        .collect()
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub fn sha256_file(p: &Path) -> [u8; 32] {
    sha256(&std::fs::read(p).unwrap())
}

/// Files in a directory other than temporary `.part` files.
pub fn stored(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

// ------------------------------------------------------------ documents

/// A document the "app" reads: in memory, optionally held after a number
/// of bytes until released.
pub struct MemDoc {
    pub name: String,
    pub data: Vec<u8>,
    /// Size announced (defaults to the real size).
    pub announced: Option<u64>,
    pub hold_after: Option<usize>,
    pub hold: Arc<Gate>,
    pub reads: Arc<AtomicUsize>,
    pub panic_on_read: bool,
    pub closed: Arc<AtomicBool>,
}

impl MemDoc {
    pub fn new(name: &str, data: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            data,
            announced: None,
            hold_after: None,
            hold: Arc::new(Gate::default()),
            reads: Arc::default(),
            panic_on_read: false,
            closed: Arc::default(),
        }
    }
}

struct MemReader {
    data: Vec<u8>,
    pos: Mutex<usize>,
    hold_after: Option<usize>,
    hold: Arc<Gate>,
    reads: Arc<AtomicUsize>,
    panic_on_read: bool,
    closed: Arc<AtomicBool>,
}

impl ForeignFile for MemDoc {
    fn display_name(&self) -> String {
        self.name.clone()
    }

    fn size(&self) -> Option<u64> {
        Some(self.announced.unwrap_or(self.data.len() as u64))
    }

    fn modified(&self) -> Option<i64> {
        Some(1)
    }

    fn open(&self) -> Result<Arc<dyn ForeignReader>, CallbackError> {
        Ok(Arc::new(MemReader {
            data: self.data.clone(),
            pos: Mutex::new(0),
            hold_after: self.hold_after,
            hold: Arc::clone(&self.hold),
            reads: Arc::clone(&self.reads),
            panic_on_read: self.panic_on_read,
            closed: Arc::clone(&self.closed),
        }))
    }
}

impl ForeignReader for MemReader {
    fn read(&self, max_len: u32) -> Result<Vec<u8>, CallbackError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.panic_on_read {
            panic!("reader exploded");
        }
        let mut pos = self.pos.lock().unwrap();
        if let Some(h) = self.hold_after
            && *pos >= h
        {
            drop(pos);
            self.hold.wait_open();
            pos = self.pos.lock().unwrap();
        }
        let mut end = (*pos + max_len as usize).min(self.data.len());
        if let Some(h) = self.hold_after
            && *pos < h
        {
            end = end.min(h);
        }
        let chunk = self.data[*pos..end].to_vec();
        *pos = end;
        Ok(chunk)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

// ------------------------------------------------------------ discovery

/// An in-memory discovery service shared by several nodes: what one node
/// advertises, every browsing node resolves (at 127.0.0.1).
#[derive(Default)]
pub struct Lan {
    ads: Mutex<Vec<ServiceAd>>,
    sinks: Mutex<Vec<Arc<BrowseSink>>>,
}

impl Lan {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn adapter(self: &Arc<Self>) -> Arc<FakeDiscovery> {
        Arc::new(FakeDiscovery {
            lan: Arc::clone(self),
            advertised: Mutex::default(),
            withdrawn: Mutex::default(),
        })
    }

    /// Publishes an arbitrary record (an impostor, a copied hint, …).
    pub fn publish(&self, ad: ServiceAd) {
        for s in self.sinks.lock().unwrap().iter() {
            s.resolved(
                ad.instance.clone(),
                ad.txt.clone(),
                vec!["127.0.0.1".into()],
                ad.port,
            );
        }
        self.ads.lock().unwrap().push(ad);
    }
}

pub struct FakeDiscovery {
    lan: Arc<Lan>,
    pub advertised: Mutex<Vec<ServiceAd>>,
    pub withdrawn: Mutex<Vec<String>>,
}

impl PlatformDiscovery for FakeDiscovery {
    fn advertise(&self, ad: ServiceAd) -> Result<String, CallbackError> {
        self.advertised.lock().unwrap().push(ad.clone());
        let instance = ad.instance.clone();
        self.lan.publish(ad);
        Ok(instance)
    }

    fn withdraw(&self, instance: String) -> Result<(), CallbackError> {
        self.withdrawn.lock().unwrap().push(instance.clone());
        for s in self.lan.sinks.lock().unwrap().iter() {
            s.lost(instance.clone());
        }
        self.lan
            .ads
            .lock()
            .unwrap()
            .retain(|a| a.instance != instance);
        Ok(())
    }

    fn start_browse(&self, sink: Arc<BrowseSink>) -> Result<(), CallbackError> {
        for ad in self.lan.ads.lock().unwrap().iter() {
            sink.resolved(
                ad.instance.clone(),
                ad.txt.clone(),
                vec!["127.0.0.1".into()],
                ad.port,
            );
        }
        self.lan.sinks.lock().unwrap().push(sink);
        Ok(())
    }

    fn stop_browse(&self) -> Result<(), CallbackError> {
        Ok(())
    }
}

pub fn txt(entries: &[(&str, &str)]) -> Vec<TxtEntry> {
    entries
        .iter()
        .map(|(k, v)| TxtEntry {
            key: (*k).into(),
            value: v.as_bytes().to_vec(),
        })
        .collect()
}
