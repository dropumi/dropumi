// SPDX-License-Identifier: MPL-2.0
//! Events reported to the user interface.
//!
//! Use cases never print; they emit [`AppEvent`]s on a bounded channel and
//! the interface renders them. Peer-supplied strings inside events are
//! already sanitized for display, but interfaces must still escape them for
//! their output format.

use std::net::SocketAddr;

use dropumi_domain::device::DeviceName;
use dropumi_domain::discovery::DiscoveredPeer;
use dropumi_domain::error::TransferError;
use dropumi_domain::identity::{Fingerprint, PublicKey};
use dropumi_domain::policy::TransferPolicy;
use dropumi_domain::transfer::{
    CancelReason, FileEntry, Party, RejectReason, TransferId, TransferProgress,
};
use dropumi_protocol::message::AckStatus;
use dropumi_protocol::sas::SasCode;
use tokio::sync::mpsc;

/// How a peer is presented: the local trust alias first, the self-declared
/// name only as a claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerView {
    /// Authenticated fingerprint.
    pub fingerprint: Fingerprint,
    /// Authenticated public key.
    pub public_key: PublicKey,
    /// Whether the key is in the local trust store.
    pub trusted: bool,
    /// Alias from the local trust store (never from the peer).
    pub alias: Option<DeviceName>,
    /// Name the peer claims for itself (sanitized, untrusted).
    pub claimed_name: DeviceName,
}

/// How a transfer ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferOutcome {
    /// Every file stored.
    Completed {
        /// Transfer id.
        transfer_id: TransferId,
        /// Files stored.
        files_stored: u32,
        /// Bytes transferred.
        bytes: u64,
        /// Duration in milliseconds.
        elapsed_ms: u64,
    },
    /// Finished, but some files were not stored.
    CompletedWithFailures {
        /// Transfer id.
        transfer_id: TransferId,
        /// Files stored.
        files_stored: u32,
        /// Files not stored.
        failed: u32,
        /// Bytes transferred.
        bytes: u64,
        /// Duration in milliseconds.
        elapsed_ms: u64,
    },
    /// The receiver rejected the offer.
    Rejected {
        /// Transfer id.
        transfer_id: TransferId,
        /// Reason.
        reason: RejectReason,
    },
    /// A side cancelled.
    Cancelled {
        /// Transfer id, if known.
        transfer_id: Option<TransferId>,
        /// Who cancelled.
        by: Party,
        /// Why.
        reason: CancelReason,
    },
    /// The session failed.
    Failed {
        /// Transfer id, if known.
        transfer_id: Option<TransferId>,
        /// Error.
        error: TransferError,
    },
}

impl TransferOutcome {
    /// `true` only for [`TransferOutcome::Completed`].
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }
}

/// Something the interface should show.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AppEvent {
    /// The receiver is listening.
    Listening {
        /// Bound addresses.
        addrs: Vec<SocketAddr>,
        /// Active policy.
        policy: TransferPolicy,
    },
    /// Discovery advertisement started.
    Advertising {
        /// Instance name.
        instance: String,
    },
    /// Discovery could not start; manual addressing still works.
    DiscoveryUnavailable {
        /// Why.
        reason: String,
    },
    /// A device was discovered.
    PeerFound(DiscoveredPeer),
    /// Dialing a candidate.
    Connecting {
        /// Address.
        addr: SocketAddr,
    },
    /// A secure channel is up.
    Connected {
        /// The peer.
        peer: PeerView,
        /// Preamble-to-Hello latency in microseconds.
        handshake_us: u64,
    },
    /// A verification code to compare (unverified peers and pairing).
    Sas {
        /// The peer.
        peer: PeerView,
        /// The code.
        code: SasCode,
    },
    /// The offer was sent.
    OfferSent {
        /// Transfer id.
        transfer_id: TransferId,
        /// The receiver.
        peer: PeerView,
        /// Number of files.
        files: u32,
        /// Total bytes.
        total_bytes: u64,
    },
    /// An offer arrived.
    OfferReceived {
        /// Transfer id.
        transfer_id: TransferId,
        /// The sender.
        peer: PeerView,
        /// Verification code for unverified senders.
        sas: Option<SasCode>,
        /// Offered files (names are untrusted wire names).
        files: Vec<FileEntry>,
        /// Total bytes.
        total_bytes: u64,
    },
    /// The offer was accepted.
    Accepted {
        /// Transfer id.
        transfer_id: TransferId,
    },
    /// Progress update (throttled).
    Progress {
        /// Transfer id.
        transfer_id: TransferId,
        /// Current progress.
        progress: TransferProgress,
    },
    /// The sender finished sending a file.
    FileSent {
        /// Transfer id.
        transfer_id: TransferId,
        /// File index.
        index: u32,
        /// Receiver's result.
        status: AckStatus,
    },
    /// A received file is stored.
    FileSaved {
        /// Transfer id.
        transfer_id: TransferId,
        /// File index.
        index: u32,
        /// Wire name (untrusted).
        name: String,
        /// Final local file name (sanitized; never a path).
        saved_as: String,
        /// Size in bytes.
        size: u64,
        /// BLAKE3 hex digest.
        blake3: String,
    },
    /// A received file was not stored.
    FileFailed {
        /// Transfer id.
        transfer_id: TransferId,
        /// File index.
        index: u32,
        /// Result.
        status: AckStatus,
    },
    /// Devices paired.
    Paired {
        /// The peer (alias now set).
        peer: PeerView,
    },
    /// A session ended without a transfer outcome (denied, failed handshake…).
    SessionClosed {
        /// The peer, if authenticated.
        peer: Option<PeerView>,
        /// Error code or `"normal"`.
        code: &'static str,
    },
    /// The receive policy changed (for example Everyone mode expired).
    PolicyChanged {
        /// New policy.
        policy: TransferPolicy,
    },
    /// A transfer ended.
    Finished(TransferOutcome),
    /// A non-fatal problem.
    Warning {
        /// Message.
        message: String,
    },
}

/// Sends events without failing the use case when the interface is gone.
#[derive(Debug, Clone)]
pub struct EventSink(mpsc::Sender<AppEvent>);

impl EventSink {
    /// Wraps a channel sender.
    #[must_use]
    pub const fn new(tx: mpsc::Sender<AppEvent>) -> Self {
        Self(tx)
    }

    /// Emits an event (waits if the interface is slow; ignores a closed
    /// channel).
    pub async fn emit(&self, event: AppEvent) {
        if self.0.send(event).await.is_err() {
            tracing::debug!("event receiver closed");
        }
    }
}
