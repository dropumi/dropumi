// SPDX-License-Identifier: MPL-2.0
//! The event mailbox between the core and the app.
//!
//! Pushing never waits, so a slow app can never stall a transfer or a
//! receiver. Events fall into three classes:
//!
//! - **critical** (prompts, offers, results, receiver state): kept. If more
//!   than [`CRITICAL_CAP`] pile up (an app that stopped reading), the queue
//!   is replaced by one `ResyncRequired` and the app reloads `snapshot()`.
//! - **coalesced** (progress per transfer, peer changes per instance): a
//!   newer value replaces a queued one, unless a lifecycle event of the same
//!   transfer or peer was queued after it, so progress never overtakes a
//!   file result.
//! - **soft** (per-file results, warnings, closed sessions): at most
//!   [`SOFT_CAP`] are kept; the oldest are dropped first and counted in
//!   `EventsDropped`. Unauthenticated hosts can cause `SessionClosed` in a
//!   loop, so these must stay bounded.
//!
//! Batches hold at most [`MAX_BATCH`] events. Critical events are delivered
//! at once; other batches at most every [`MIN_INTERVAL`] (ten per second).

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::CallbackError;
use crate::runtime::lock;
use crate::types::NodeEvent;

/// Critical events kept before the app must resync.
pub(crate) const CRITICAL_CAP: usize = 4_096;
/// Soft events kept.
pub(crate) const SOFT_CAP: usize = 1_024;
/// Events per delivery.
pub(crate) const MAX_BATCH: usize = 256;
/// Minimum time between deliveries without critical events.
pub(crate) const MIN_INTERVAL: Duration = Duration::from_millis(100);
/// Consecutive listener failures after which delivery stops.
const MAX_LISTENER_FAILURES: u32 = 3;

/// Receives events from the node, in order, on one thread.
#[uniffi::export(with_foreign)]
pub trait EventListener: Send + Sync {
    /// A batch of events. Return quickly: hand the batch to the UI and
    /// return. Calling node methods from here is allowed.
    ///
    /// # Errors
    /// An error (or exception) is counted; after three in a row the node
    /// stops delivering events (prompts stay readable via `snapshot()`).
    fn on_events(&self, events: Vec<NodeEvent>) -> Result<(), CallbackError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Critical,
    Coalesced,
    Soft,
}

/// What an event is about, for coalescing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Key {
    Transfer(Option<u64>, String),
    Peer(String),
}

#[derive(Debug)]
struct Entry {
    event: NodeEvent,
    class: Class,
    key: Option<Key>,
}

fn classify(e: &NodeEvent) -> (Class, Option<Key>) {
    let transfer = |op: &Option<u64>, id: &String| Some(Key::Transfer(*op, id.clone()));
    match e {
        NodeEvent::TransferProgress {
            operation,
            transfer_id,
            ..
        } => (Class::Coalesced, transfer(operation, transfer_id)),
        NodeEvent::PeerFound { peer } => (Class::Coalesced, Some(Key::Peer(peer.instance.clone()))),
        NodeEvent::PeerLost { instance } => (Class::Soft, Some(Key::Peer(instance.clone()))),
        NodeEvent::FileSent {
            operation,
            transfer_id,
            ..
        } => (Class::Soft, transfer(&Some(*operation), transfer_id)),
        NodeEvent::FileSaved { transfer_id, .. } | NodeEvent::FileFailed { transfer_id, .. } => {
            (Class::Soft, transfer(&None, transfer_id))
        }
        NodeEvent::SessionClosed { .. } | NodeEvent::Warning { .. } => (Class::Soft, None),
        NodeEvent::TransferAccepted {
            operation,
            transfer_id,
        }
        | NodeEvent::TransferFinished {
            operation,
            result:
                crate::types::TransferResult {
                    transfer_id: Some(transfer_id),
                    ..
                },
            ..
        } => (Class::Critical, transfer(operation, transfer_id)),
        NodeEvent::OfferSent {
            operation,
            transfer_id,
            ..
        } => (Class::Critical, transfer(&Some(*operation), transfer_id)),
        NodeEvent::OfferReceived { transfer_id, .. } => {
            (Class::Critical, transfer(&None, transfer_id))
        }
        _ => (Class::Critical, None),
    }
}

#[derive(Debug, Default)]
struct State {
    queue: VecDeque<Entry>,
    critical: usize,
    soft: usize,
    dropped: u64,
    resync: bool,
    closed: bool,
    last_delivery: Option<Instant>,
}

/// What the dispatcher should do next.
#[derive(Debug, PartialEq)]
pub(crate) enum Next {
    /// Deliver this batch now.
    Deliver(Vec<NodeEvent>),
    /// Nothing is due before this instant.
    WaitUntil(Instant),
    /// Nothing queued.
    Idle,
    /// Closed and empty.
    Closed,
}

impl State {
    fn push(&mut self, event: NodeEvent) {
        if self.closed {
            return;
        }
        let (class, key) = classify(&event);
        match class {
            Class::Coalesced => {
                if let Some(pos) = key.as_ref().and_then(|k| self.replaceable(k))
                    && let Some(entry) = self.queue.get_mut(pos)
                {
                    entry.event = event;
                    return;
                }
            }
            Class::Soft => {
                if self.soft >= SOFT_CAP
                    && let Some(pos) = self.queue.iter().position(|e| e.class == Class::Soft)
                {
                    self.queue.remove(pos);
                    self.soft = self.soft.saturating_sub(1);
                    self.dropped = self.dropped.saturating_add(1);
                }
                self.soft = self.soft.saturating_add(1);
            }
            Class::Critical => {
                if self.critical >= CRITICAL_CAP {
                    self.queue.clear();
                    self.critical = 0;
                    self.soft = 0;
                    self.resync = true;
                }
                self.critical = self.critical.saturating_add(1);
            }
        }
        self.queue.push_back(Entry { event, class, key });
    }

    /// The queued coalesced entry for `key` that a newer value may replace:
    /// the latest entry with that key, if it is a coalesced one.
    fn replaceable(&self, key: &Key) -> Option<usize> {
        let (pos, entry) = self
            .queue
            .iter()
            .enumerate()
            .rev()
            .find(|(_, e)| e.key.as_ref() == Some(key))?;
        (entry.class == Class::Coalesced).then_some(pos)
    }

    fn next(&mut self, now: Instant) -> Next {
        let extra = usize::from(self.resync) + usize::from(self.dropped > 0);
        if self.queue.is_empty() && extra == 0 {
            return if self.closed {
                Next::Closed
            } else {
                Next::Idle
            };
        }
        let urgent = self.resync || self.critical > 0 || self.closed;
        if !urgent
            && let Some(last) = self.last_delivery
            && let Some(due) = last.checked_add(MIN_INTERVAL)
            && now < due
        {
            return Next::WaitUntil(due);
        }
        let mut batch = Vec::with_capacity(MAX_BATCH.min(self.queue.len().saturating_add(extra)));
        if self.resync {
            self.resync = false;
            batch.push(NodeEvent::ResyncRequired);
        }
        if self.dropped > 0 {
            batch.push(NodeEvent::EventsDropped {
                count: self.dropped,
            });
            self.dropped = 0;
        }
        while batch.len() < MAX_BATCH {
            let Some(entry) = self.queue.pop_front() else {
                break;
            };
            match entry.class {
                Class::Critical => self.critical = self.critical.saturating_sub(1),
                Class::Soft => self.soft = self.soft.saturating_sub(1),
                Class::Coalesced => {}
            }
            batch.push(entry.event);
        }
        self.last_delivery = Some(now);
        Next::Deliver(batch)
    }
}

/// The queue shared by the node (pushing) and the dispatcher (delivering).
#[derive(Debug, Default)]
pub(crate) struct Mailbox {
    state: Mutex<State>,
    ready: Condvar,
}

impl Mailbox {
    /// Queues an event; never waits.
    pub(crate) fn push(&self, event: NodeEvent) {
        lock(&self.state).push(event);
        self.ready.notify_one();
    }

    /// Stops accepting events; the dispatcher delivers what is queued and
    /// ends.
    pub(crate) fn close(&self) {
        lock(&self.state).closed = true;
        self.ready.notify_one();
    }

    /// The next delivery decision at `now`.
    #[cfg(test)]
    pub(crate) fn next(&self, now: Instant) -> Next {
        lock(&self.state).next(now)
    }

    /// Blocks until a batch is due; `None` once closed and empty.
    fn wait_batch(&self) -> Option<Vec<NodeEvent>> {
        let mut state = lock(&self.state);
        loop {
            let now = Instant::now();
            match state.next(now) {
                Next::Deliver(batch) => return Some(batch),
                Next::Closed => return None,
                Next::Idle => {
                    state = self
                        .ready
                        .wait(state)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                Next::WaitUntil(due) => {
                    let left = due.saturating_duration_since(now);
                    state = self
                        .ready
                        .wait_timeout(state, left)
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0;
                }
            }
        }
    }
}

/// Starts the thread that delivers batches to `listener`. It ends after
/// [`Mailbox::close`] once the queue is empty.
pub(crate) fn spawn_dispatcher(
    mailbox: Arc<Mailbox>,
    listener: Arc<dyn EventListener>,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("dropumi-events".into())
        .spawn(move || {
            let mut failures = 0u32;
            while let Some(batch) = mailbox.wait_batch() {
                if failures >= MAX_LISTENER_FAILURES {
                    continue;
                }
                let delivered = catch_unwind(AssertUnwindSafe(|| listener.on_events(batch)));
                if matches!(delivered, Ok(Ok(()))) {
                    failures = 0;
                } else {
                    failures = failures.saturating_add(1);
                    if failures >= MAX_LISTENER_FAILURES {
                        tracing::warn!(
                            "event listener failed three times in a row; delivery stopped"
                        );
                    }
                }
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Direction, NearbyPeer, Progress, PromptCloseReason, TransferResult, TransferStatus,
    };

    fn progress(id: &str, done: u64) -> NodeEvent {
        NodeEvent::TransferProgress {
            operation: None,
            transfer_id: id.into(),
            progress: Progress {
                bytes_done: done,
                bytes_total: 100,
                files_done: 0,
                files_total: 1,
            },
        }
    }

    fn saved(id: &str) -> NodeEvent {
        NodeEvent::FileSaved {
            transfer_id: id.into(),
            index: 0,
            saved_as: "a.txt".into(),
            size: 1,
            blake3: String::new(),
        }
    }

    fn closed(n: u64) -> NodeEvent {
        NodeEvent::PromptClosed {
            id: n,
            reason: PromptCloseReason::Answered,
        }
    }

    fn deliver(state: &mut State) -> Vec<NodeEvent> {
        match state.next(Instant::now() + Duration::from_secs(60)) {
            Next::Deliver(b) => b,
            other => panic!("{other:?}"),
        }
    }

    /// Every remaining batch, with time advancing past each rate limit.
    fn drain_all(state: &mut State) -> Vec<Vec<NodeEvent>> {
        let mut at = Instant::now() + Duration::from_secs(120);
        let mut batches = Vec::new();
        while let Next::Deliver(b) = state.next(at) {
            batches.push(b);
            at += Duration::from_secs(1);
        }
        batches
    }

    #[test]
    fn progress_coalesced_per_transfer() {
        let mut s = State::default();
        s.push(progress("a", 1));
        s.push(progress("b", 1));
        s.push(progress("a", 2));
        s.push(progress("a", 3));
        assert_eq!(deliver(&mut s), vec![progress("a", 3), progress("b", 1)]);
    }

    #[test]
    fn progress_never_overtakes_lifecycle_of_same_transfer() {
        let mut s = State::default();
        s.push(progress("a", 50));
        s.push(saved("a"));
        s.push(progress("a", 100));
        assert_eq!(
            deliver(&mut s),
            vec![progress("a", 50), saved("a"), progress("a", 100)]
        );
    }

    #[test]
    fn critical_events_never_dropped() {
        let mut s = State::default();
        for i in 0..(SOFT_CAP as u64 * 2) {
            s.push(closed(i));
        }
        let all: Vec<NodeEvent> = drain_all(&mut s).into_iter().flatten().collect();
        assert_eq!(all.len(), SOFT_CAP * 2);
        assert!(
            !all.iter()
                .any(|e| matches!(e, NodeEvent::EventsDropped { .. }))
        );
    }

    #[test]
    fn critical_overflow_requests_resync() {
        let mut s = State::default();
        for i in 0..=(CRITICAL_CAP as u64) {
            s.push(closed(i));
        }
        let first = deliver(&mut s);
        assert_eq!(first.first(), Some(&NodeEvent::ResyncRequired));
        // Only what arrived after the overflow is still queued.
        assert_eq!(first.len(), 2);
    }

    #[test]
    fn soft_events_dropped_oldest_with_count() {
        let mut s = State::default();
        for _ in 0..(SOFT_CAP + 10) {
            s.push(NodeEvent::SessionClosed {
                peer: None,
                code: "handshake_failed".into(),
            });
        }
        s.push(saved("last"));
        let all: Vec<NodeEvent> = drain_all(&mut s).into_iter().flatten().collect();
        assert_eq!(all.first(), Some(&NodeEvent::EventsDropped { count: 11 }));
        assert_eq!(all.len() - 1, SOFT_CAP);
        assert_eq!(
            all.last(),
            Some(&saved("last")),
            "the newest soft event is kept"
        );
    }

    #[test]
    fn unauthenticated_session_closed_flood_bounded() {
        let mut s = State::default();
        for _ in 0..100_000 {
            s.push(NodeEvent::SessionClosed {
                peer: None,
                code: "handshake_failed".into(),
            });
        }
        assert!(s.queue.len() <= SOFT_CAP);
    }

    #[test]
    fn batch_policy_flushes_critical_and_rate_limits_progress() {
        let start = Instant::now();
        let mut s = State::default();
        s.push(progress("a", 1));
        assert!(matches!(s.next(start), Next::Deliver(_)));
        // Progress only: not before 100 ms after the last delivery.
        s.push(progress("a", 2));
        assert_eq!(
            s.next(start + Duration::from_millis(10)),
            Next::WaitUntil(start + MIN_INTERVAL)
        );
        // A critical event flushes at once, progress included.
        s.push(NodeEvent::TransferFinished {
            operation: None,
            direction: Direction::Incoming,
            result: TransferResult {
                transfer_id: Some("a".into()),
                ..TransferResult::failed(crate::error::ErrorCode::Internal)
            },
        });
        match s.next(start + Duration::from_millis(20)) {
            Next::Deliver(b) => assert_eq!(b.len(), 2),
            other => panic!("{other:?}"),
        }
        assert_eq!(s.next(start + Duration::from_millis(30)), Next::Idle);
        let _ = TransferStatus::Completed;
    }

    #[test]
    fn batches_hold_at_most_256() {
        let mut s = State::default();
        for i in 0..1_000 {
            s.push(closed(i));
        }
        assert_eq!(deliver(&mut s).len(), MAX_BATCH);
    }

    #[test]
    fn peers_coalesce_until_lost() {
        let mut s = State::default();
        let peer = |name: &str| NodeEvent::PeerFound {
            peer: NearbyPeer {
                instance: "dropumi-00000001".into(),
                name: name.into(),
                fingerprint_hint: None,
                compatible: true,
                hint_alias: None,
            },
        };
        s.push(peer("A"));
        s.push(peer("B"));
        s.push(NodeEvent::PeerLost {
            instance: "dropumi-00000001".into(),
        });
        s.push(peer("C"));
        let b = deliver(&mut s);
        assert_eq!(b.len(), 3);
        assert_eq!(b.first(), Some(&peer("B")));
    }

    #[test]
    fn closed_mailbox_drains_then_ends() {
        let mut s = State::default();
        s.push(progress("a", 1));
        s.closed = true;
        s.push(progress("a", 2));
        assert!(matches!(s.next(Instant::now()), Next::Deliver(b) if b.len() == 1));
        assert_eq!(s.next(Instant::now()), Next::Closed);
    }
}
