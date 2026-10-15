// SPDX-License-Identifier: MPL-2.0
//! Records and enums the apps see.
//!
//! Every peer-supplied string in these types has already been sanitized by
//! the core (`text::strip_unsafe` and the name rules). Apps still render
//! them verbatim, never as markup.

use dropumi_application::events::{PeerView, TransferOutcome};
use dropumi_domain::policy::{AcceptMode as DomainAccept, PolicyMode, TransferPolicy};
use dropumi_domain::text::strip_unsafe;
use dropumi_domain::transfer::{CancelReason, Party, RejectReason};
use dropumi_domain::trust::{PairingMethod, TrustedDevice};
use dropumi_protocol::message::AckStatus;

use crate::error::ErrorCode;

/// How the node is set up.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NodeConfig {
    /// Absolute path of the configuration directory (identity, trust
    /// store). Dropumi creates it with mode 0700; it must be app-private
    /// and excluded from backups.
    pub config_dir: String,
    /// Absolute path of an existing directory for received files, if this
    /// node receives.
    pub receive_dir: Option<String>,
}

/// This device.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct IdentityInfo {
    /// Device name other devices see.
    pub name: String,
    /// Full fingerprint (52 characters).
    pub fingerprint: String,
    /// Display form (24 characters in groups of four).
    pub fingerprint_display: String,
    /// 8-character hint advertised on the network.
    pub fingerprint_hint: String,
}

/// How trust was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TrustMethod {
    /// Both users confirmed matching codes.
    Code,
    /// The full fingerprint was entered by hand.
    Manual,
}

/// A trusted (paired) device.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TrustedDeviceInfo {
    /// Full fingerprint.
    pub fingerprint: String,
    /// Display form.
    pub fingerprint_display: String,
    /// Local alias, if any.
    pub alias: Option<String>,
    /// How it was trusted.
    pub method: TrustMethod,
    /// When it was added (Unix seconds).
    pub added_at: u64,
    /// When it last completed a session (Unix seconds).
    pub last_seen: Option<u64>,
}

impl From<&TrustedDevice> for TrustedDeviceInfo {
    fn from(d: &TrustedDevice) -> Self {
        Self {
            fingerprint: d.fingerprint.to_full(),
            fingerprint_display: d.fingerprint.to_display(),
            alias: d.alias.as_ref().map(|a| a.as_str().to_owned()),
            method: match d.method {
                PairingMethod::Sas => TrustMethod::Code,
                PairingMethod::Manual => TrustMethod::Manual,
            },
            added_at: d.added_at.as_secs(),
            last_seen: d.last_seen.map(|t| t.as_secs()),
        }
    }
}

/// An authenticated peer. Show `alias` first; `claimed_name` is what the
/// peer says about itself and must be labelled as a claim.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PeerInfo {
    /// Authenticated full fingerprint.
    pub fingerprint: String,
    /// Display form.
    pub fingerprint_display: String,
    /// Whether the key is in the local trust store.
    pub trusted: bool,
    /// Local alias (never from the peer).
    pub alias: Option<String>,
    /// Name the peer claims (sanitized, untrusted).
    pub claimed_name: String,
}

impl From<&PeerView> for PeerInfo {
    fn from(v: &PeerView) -> Self {
        Self {
            fingerprint: v.fingerprint.to_full(),
            fingerprint_display: v.fingerprint.to_display(),
            trusted: v.trusted,
            alias: v.alias.as_ref().map(|a| a.as_str().to_owned()),
            claimed_name: v.claimed_name.as_str().to_owned(),
        }
    }
}

/// A device seen on the network. Everything here is a hint: nothing is
/// verified before connecting.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NearbyPeer {
    /// Discovery instance (random per advertisement); use it to send.
    pub instance: String,
    /// Advertised name (sanitized, unverified).
    pub name: String,
    /// Advertised 8-character fingerprint hint, if valid.
    pub fingerprint_hint: Option<String>,
    /// Whether it speaks a protocol version this device supports.
    pub compatible: bool,
    /// The alias of the one paired device whose fingerprint starts with
    /// the hint. Show it as "looks like", never as verified: the key is
    /// checked only when connecting.
    pub hint_alias: Option<String>,
}

/// Who may send to this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ReceivePolicy {
    /// Paired devices only.
    TrustedOnly,
    /// Any device may ask, for this many minutes (1–60); unverified offers
    /// always ask and show a code.
    Everyone {
        /// Window length.
        minutes: u32,
    },
}

/// Whether offers are accepted without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AcceptMode {
    /// Ask for every offer.
    Prompt,
    /// Accept offers from paired devices automatically; ask for others.
    AutoTrusted,
}

impl AcceptMode {
    pub(crate) const fn to_domain(self) -> DomainAccept {
        match self {
            Self::Prompt => DomainAccept::Prompt,
            Self::AutoTrusted => DomainAccept::AutoTrusted,
        }
    }
}

/// How to receive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ReceiveOptions {
    /// Who may send.
    pub policy: ReceivePolicy,
    /// Whether to ask.
    pub accept: AcceptMode,
    /// TCP port; `None` uses 47321 and falls back to a free port.
    pub port: Option<u16>,
    /// Whether to advertise on the network (needs a discovery adapter).
    pub advertise: bool,
}

/// A running receiver.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReceiverInfo {
    /// The port it listens on (all IPv4 interfaces).
    pub port: u16,
    /// The advertised instance, when advertising.
    pub instance: Option<String>,
    /// Current policy.
    pub policy: ReceivePolicy,
    /// Current accept mode.
    pub accept: AcceptMode,
    /// When an Everyone window ends (Unix seconds).
    pub everyone_until: Option<u64>,
}

/// Where to send.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SendTarget {
    /// A device from the nearby list. Its advertised addresses are used;
    /// if its hint matches exactly one paired device, that device's full
    /// key is required before anything is sent.
    Nearby {
        /// The discovery instance.
        instance: String,
    },
    /// A typed address, `host:port` or `ip:port`.
    Address {
        /// The address.
        address: String,
        /// Optional fingerprint pin (at least 24 characters).
        expect_fingerprint: Option<String>,
    },
}

/// Send options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct SendOptions {
    /// Allow a receiver that is not paired, after both users compare a
    /// code. Without it, only paired receivers are accepted.
    pub allow_unverified: bool,
}

/// Why a verification code is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CodePurpose {
    /// Pairing.
    Pairing,
    /// Sending to a receiver that is not paired.
    UnverifiedReceiver,
}

/// One offered file (name sanitized for display).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OfferedFile {
    /// Display name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
}

/// A question waiting for the local user.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Prompt {
    /// Someone offers files.
    Offer {
        /// Answer with `answer_offer(id, …)`.
        id: u64,
        /// Transfer id.
        transfer_id: String,
        /// The sender.
        peer: PeerInfo,
        /// Code to compare, for senders that are not paired.
        code: Option<String>,
        /// The first files (at most 200; see `offer_files`).
        files: Vec<OfferedFile>,
        /// Number of files.
        file_count: u32,
        /// Total size.
        total_bytes: u64,
        /// When the request expires (Unix milliseconds).
        expires_at_ms: u64,
    },
    /// Both screens must show the same code.
    Code {
        /// Answer with `answer_sas(id, …)`.
        id: u64,
        /// The operation that asks, if any.
        operation: Option<u64>,
        /// The peer.
        peer: PeerInfo,
        /// The six-digit code.
        code: String,
        /// Why it is shown.
        purpose: CodePurpose,
        /// When the request expires (Unix milliseconds).
        expires_at_ms: u64,
    },
}

impl Prompt {
    /// The prompt id.
    #[must_use]
    pub const fn id(&self) -> u64 {
        match self {
            Self::Offer { id, .. } | Self::Code { id, .. } => *id,
        }
    }
}

/// Why a prompt went away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PromptCloseReason {
    /// The user answered.
    Answered,
    /// It expired, the peer cancelled, or the session ended.
    Withdrawn,
}

/// What an operation does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum OperationKind {
    /// Sending files.
    Send,
    /// Pairing with a device.
    Pair,
    /// Waiting for a device to pair.
    PairingWindow,
}

/// A transfer direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Direction {
    /// This device sends.
    Outgoing,
    /// This device receives.
    Incoming,
}

/// A file's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FileStatus {
    /// Stored and checked.
    Stored,
    /// The content did not match; nothing was kept.
    IntegrityFailed,
    /// Writing failed; nothing was kept.
    WriteFailed,
    /// The sender aborted it; nothing was kept.
    Aborted,
}

impl From<AckStatus> for FileStatus {
    fn from(s: AckStatus) -> Self {
        match s {
            AckStatus::Stored => Self::Stored,
            AckStatus::IntegrityFailed => Self::IntegrityFailed,
            AckStatus::WriteFailed => Self::WriteFailed,
            AckStatus::Aborted => Self::Aborted,
        }
    }
}

/// How a transfer ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TransferStatus {
    /// Every file stored and checked. The only success.
    Completed,
    /// Finished, but some files were not stored.
    CompletedWithFailures,
    /// The receiver declined, or nobody answered.
    Rejected,
    /// Someone cancelled.
    Cancelled,
    /// It failed.
    Failed,
}

/// Why an offer was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RejectCause {
    /// The user declined.
    Declined,
    /// Nobody answered in time.
    Timeout,
    /// The receiver was busy.
    Busy,
    /// Another reason.
    Other,
}

/// Who cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CancelledBy {
    /// This device.
    Local,
    /// The other device.
    Peer,
}

/// Why it was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CancelCause {
    /// A user cancelled.
    User,
    /// An app or device shut down.
    Shutdown,
    /// Storing failed.
    Storage,
    /// A source file could not be read.
    Source,
    /// Something took too long.
    Timeout,
    /// Another reason.
    Other,
}

/// The result of a transfer.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TransferResult {
    /// How it ended.
    pub status: TransferStatus,
    /// Transfer id, if one was agreed.
    pub transfer_id: Option<String>,
    /// Files stored.
    pub files_stored: u32,
    /// Files not stored.
    pub files_failed: u32,
    /// Bytes transferred.
    pub bytes: u64,
    /// Duration in milliseconds.
    pub elapsed_ms: u64,
    /// For `Rejected`.
    pub reject_cause: Option<RejectCause>,
    /// For `Cancelled`.
    pub cancelled_by: Option<CancelledBy>,
    /// For `Cancelled`.
    pub cancel_cause: Option<CancelCause>,
    /// For `Failed`.
    pub error: Option<ErrorCode>,
}

impl TransferResult {
    fn base(status: TransferStatus) -> Self {
        Self {
            status,
            transfer_id: None,
            files_stored: 0,
            files_failed: 0,
            bytes: 0,
            elapsed_ms: 0,
            reject_cause: None,
            cancelled_by: None,
            cancel_cause: None,
            error: None,
        }
    }

    /// A failure before any transfer started.
    #[must_use]
    pub fn failed(code: ErrorCode) -> Self {
        Self {
            error: Some(code),
            ..Self::base(TransferStatus::Failed)
        }
    }
}

impl From<&TransferOutcome> for TransferResult {
    fn from(o: &TransferOutcome) -> Self {
        match o {
            TransferOutcome::Completed {
                transfer_id,
                files_stored,
                bytes,
                elapsed_ms,
            } => Self {
                transfer_id: Some(transfer_id.to_hex()),
                files_stored: *files_stored,
                bytes: *bytes,
                elapsed_ms: *elapsed_ms,
                ..Self::base(TransferStatus::Completed)
            },
            TransferOutcome::CompletedWithFailures {
                transfer_id,
                files_stored,
                failed,
                bytes,
                elapsed_ms,
            } => Self {
                transfer_id: Some(transfer_id.to_hex()),
                files_stored: *files_stored,
                files_failed: *failed,
                bytes: *bytes,
                elapsed_ms: *elapsed_ms,
                ..Self::base(TransferStatus::CompletedWithFailures)
            },
            TransferOutcome::Rejected {
                transfer_id,
                reason,
            } => Self {
                transfer_id: Some(transfer_id.to_hex()),
                reject_cause: Some(match reason {
                    RejectReason::Declined => RejectCause::Declined,
                    RejectReason::Timeout => RejectCause::Timeout,
                    RejectReason::Busy => RejectCause::Busy,
                    _ => RejectCause::Other,
                }),
                ..Self::base(TransferStatus::Rejected)
            },
            TransferOutcome::Cancelled {
                transfer_id,
                by,
                reason,
            } => Self {
                transfer_id: transfer_id.map(|t| t.to_hex()),
                cancelled_by: Some(match by {
                    Party::Local => CancelledBy::Local,
                    Party::Peer => CancelledBy::Peer,
                }),
                cancel_cause: Some(match reason {
                    CancelReason::User => CancelCause::User,
                    CancelReason::Shutdown => CancelCause::Shutdown,
                    CancelReason::Storage => CancelCause::Storage,
                    CancelReason::Source => CancelCause::Source,
                    CancelReason::Timeout => CancelCause::Timeout,
                    _ => CancelCause::Other,
                }),
                ..Self::base(TransferStatus::Cancelled)
            },
            TransferOutcome::Failed { transfer_id, error } => Self {
                transfer_id: transfer_id.map(|t| t.to_hex()),
                error: Some(ErrorCode::of(error)),
                ..Self::base(TransferStatus::Failed)
            },
        }
    }
}

/// How pairing ended.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PairingResult {
    /// Both users confirmed; the device is now trusted.
    Paired {
        /// The new trusted device.
        peer: PeerInfo,
    },
    /// Pairing failed; nothing was stored.
    Failed {
        /// Why.
        error: ErrorCode,
    },
    /// Cancelled here; nothing was stored.
    Cancelled,
}

/// The result of an operation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum OperationResult {
    /// A send ended.
    Transfer {
        /// The result.
        result: TransferResult,
    },
    /// Pairing ended.
    Pairing {
        /// The result.
        result: PairingResult,
    },
}

/// Why the receiver stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ReceiverStopReason {
    /// Stopped as asked.
    Stopped,
    /// The listening socket kept failing.
    ListenerFailed,
    /// An internal error.
    Internal,
}

/// Transfer progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Progress {
    /// Bytes done.
    pub bytes_done: u64,
    /// Bytes in total.
    pub bytes_total: u64,
    /// Files done.
    pub files_done: u32,
    /// Files in total.
    pub files_total: u32,
}

/// Something the app should show. Incoming transfers are identified by
/// `transfer_id` (unique among running transfers); outgoing ones by their
/// operation id.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NodeEvent {
    /// The receiver is listening.
    ReceiverStarted {
        /// Its state.
        info: ReceiverInfo,
    },
    /// The receiver stopped.
    ReceiverStopped {
        /// Why.
        reason: ReceiverStopReason,
    },
    /// The receive policy changed (including an Everyone window ending).
    PolicyChanged {
        /// New policy.
        policy: ReceivePolicy,
        /// New accept mode.
        accept: AcceptMode,
        /// When an Everyone window ends (Unix seconds).
        everyone_until: Option<u64>,
    },
    /// Advertising could not start; manual addresses still work.
    DiscoveryUnavailable {
        /// Why (local display only).
        reason: String,
    },
    /// A device appeared or changed.
    PeerFound {
        /// The device.
        peer: NearbyPeer,
    },
    /// A device went away.
    PeerLost {
        /// Its instance.
        instance: String,
    },
    /// A secure channel is up.
    Connected {
        /// The operation, for outgoing work.
        operation: Option<u64>,
        /// The peer.
        peer: PeerInfo,
    },
    /// The session code (shown in every session).
    SessionCode {
        /// The operation, for outgoing work.
        operation: Option<u64>,
        /// The peer.
        peer: PeerInfo,
        /// Six digits.
        code: String,
    },
    /// An offer was sent.
    OfferSent {
        /// The operation.
        operation: u64,
        /// Transfer id.
        transfer_id: String,
        /// The receiver.
        peer: PeerInfo,
        /// Number of files.
        files: u32,
        /// Total size.
        total_bytes: u64,
    },
    /// An offer arrived (a `PromptOpened` follows when the user must answer).
    OfferReceived {
        /// Transfer id.
        transfer_id: String,
        /// The sender.
        peer: PeerInfo,
        /// Number of files.
        files: u32,
        /// Total size.
        total_bytes: u64,
    },
    /// The user must answer.
    PromptOpened {
        /// The question.
        prompt: Prompt,
    },
    /// A prompt went away; later answers are refused.
    PromptClosed {
        /// Prompt id.
        id: u64,
        /// Why.
        reason: PromptCloseReason,
    },
    /// The receiver accepted.
    TransferAccepted {
        /// The operation, for outgoing work.
        operation: Option<u64>,
        /// Transfer id.
        transfer_id: String,
    },
    /// Progress (at most ten per second per transfer).
    TransferProgress {
        /// The operation, for outgoing work.
        operation: Option<u64>,
        /// Transfer id.
        transfer_id: String,
        /// Progress.
        progress: Progress,
    },
    /// The receiver reported a file's result to this sender.
    FileSent {
        /// The operation.
        operation: u64,
        /// Transfer id.
        transfer_id: String,
        /// File index.
        index: u32,
        /// Result.
        status: FileStatus,
    },
    /// A received file is stored and checked.
    FileSaved {
        /// Transfer id.
        transfer_id: String,
        /// File index.
        index: u32,
        /// Final file name in the receive directory (never a path).
        saved_as: String,
        /// Size.
        size: u64,
        /// BLAKE3 digest (hex).
        blake3: String,
    },
    /// A received file was not stored.
    FileFailed {
        /// Transfer id.
        transfer_id: String,
        /// File index.
        index: u32,
        /// Result.
        status: FileStatus,
    },
    /// A transfer ended.
    TransferFinished {
        /// The operation, for outgoing work.
        operation: Option<u64>,
        /// Direction.
        direction: Direction,
        /// The result.
        result: TransferResult,
    },
    /// A pairing window is open on this port.
    PairingWindowOpened {
        /// The operation.
        operation: u64,
        /// Port.
        port: u16,
    },
    /// Pairing ended.
    PairingFinished {
        /// The operation.
        operation: u64,
        /// The result.
        result: PairingResult,
    },
    /// A session ended without a transfer (denied, failed handshake, …).
    SessionClosed {
        /// The peer, if it was authenticated.
        peer: Option<PeerInfo>,
        /// Error code or "normal".
        code: String,
    },
    /// A problem that did not stop anything (local display only).
    Warning {
        /// Message.
        message: String,
    },
    /// Less important events were dropped because the app fell behind.
    EventsDropped {
        /// How many.
        count: u64,
    },
    /// The app fell far behind: reload everything with `snapshot()`.
    ResyncRequired,
}

/// The policy a running receiver applies.
pub(crate) fn policy_info(
    p: &TransferPolicy,
    now: u64,
) -> (ReceivePolicy, AcceptMode, Option<u64>) {
    let accept = match p.accept {
        DomainAccept::AutoTrusted => AcceptMode::AutoTrusted,
        _ => AcceptMode::Prompt,
    };
    match p.mode {
        PolicyMode::Everyone { until } => {
            let left = until.as_secs().saturating_sub(now);
            let minutes = u32::try_from(left.div_ceil(60)).unwrap_or(u32::MAX).max(1);
            (
                ReceivePolicy::Everyone { minutes },
                accept,
                Some(until.as_secs()),
            )
        }
        _ => (ReceivePolicy::TrustedOnly, accept, None),
    }
}

/// A peer string made safe for display (the core already did this; this is
/// defence in depth for strings the facade builds itself).
pub(crate) fn display_safe(s: &str) -> String {
    strip_unsafe(s).into_owned()
}
