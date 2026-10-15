// SPDX-License-Identifier: MPL-2.0
//! Ports: the interfaces use cases need from the outside world.
//!
//! Each port has a real adapter in `dropumi-infra` and an in-memory double
//! in the tests. Async methods return `impl Future + Send`; use cases are
//! generic over ports (no `dyn`).

use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, SystemTime};

use dropumi_domain::device::DeviceName;
use dropumi_domain::discovery::DiscoveredPeer;
use dropumi_domain::filename::SafeFileName;
use dropumi_domain::identity::Fingerprint;
use dropumi_domain::time::Timestamp;
use dropumi_domain::transfer::{FileEntry, RejectReason, TransferId};
use dropumi_domain::trust::TrustedDevice;
use dropumi_protocol::sas::SasCode;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::{DiscoveryError, SourceError, StorageError, StoreError, TransportError};
use crate::events::PeerView;
use crate::identity::LocalIdentity;

/// Opens outgoing connections.
pub trait Connector: Send + Sync {
    /// Connection type.
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// Connects to `addr` within `timeout`.
    fn connect(
        &self,
        addr: SocketAddr,
        timeout: Duration,
    ) -> impl Future<Output = Result<Self::Stream, TransportError>> + Send;

    /// Resolves a host name.
    fn resolve(
        &self,
        host: &str,
        port: u16,
    ) -> impl Future<Output = Result<Vec<SocketAddr>, TransportError>> + Send;
}

/// Accepts incoming connections.
pub trait Listener: Send {
    /// Connection type.
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// Waits for the next connection.
    fn accept(
        &mut self,
    ) -> impl Future<Output = Result<(Self::Stream, SocketAddr), TransportError>> + Send;

    /// Addresses the listener is bound to.
    fn local_addrs(&self) -> Vec<SocketAddr>;
}

/// What a receiver advertises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advertisement {
    /// Device name.
    pub name: DeviceName,
    /// 8-character fingerprint hint.
    pub fp_hint: String,
    /// TCP port.
    pub port: u16,
    /// Supported protocol versions.
    pub versions: Vec<u16>,
}

/// A running advertisement.
pub trait AdHandle: Send {
    /// Stops advertising (sends goodbye packets where supported).
    fn stop(self) -> impl Future<Output = ()> + Send;
    /// Instance name.
    fn instance(&self) -> &str;
}

/// Finds nearby devices and advertises this one.
pub trait Discovery: Send + Sync {
    /// Advertisement handle.
    type Handle: AdHandle;

    /// Starts advertising.
    fn advertise(
        &self,
        ad: Advertisement,
    ) -> impl Future<Output = Result<Self::Handle, DiscoveryError>> + Send;

    /// Returns the peers seen during the last `timeout`. An adapter that
    /// already watches the network may answer at once from what it has seen
    /// for at least that long, instead of browsing again.
    fn browse(
        &self,
        timeout: Duration,
    ) -> impl Future<Output = Result<Vec<DiscoveredPeer>, DiscoveryError>> + Send;
}

/// Metadata of a file to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMeta {
    /// Wire name derived from the local file name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Modification time (Unix seconds), if available. This is the value
    /// sent on the wire.
    pub modified: Option<i64>,
    /// Local only, never sent on the wire: the modification time at the
    /// file system's full resolution, if available. A change of the file is
    /// detected by its size or this time, so an edit within the same second
    /// is detected where the file system stores sub-second times. On a
    /// file system that stores whole seconds, a same-size edit within the
    /// same second can still go undetected; the receiver's BLAKE3 check
    /// still verifies the bytes that were sent.
    pub local_modified: Option<SystemTime>,
}

/// Reads files to send.
///
/// Use cases treat the `path` they are given as an opaque key: they never
/// open, list or display it themselves, and only pass it back to the same
/// source. An adapter may therefore key entries that are not files on disk
/// (for example a document handed over by the operating system).
pub trait FileSource: Send + Sync {
    /// Reader type.
    type Reader: AsyncRead + Unpin + Send;

    /// Inspects a path; regular files only.
    fn meta(&self, path: &Path) -> impl Future<Output = Result<SourceMeta, SourceError>> + Send;

    /// Opens a file whose metadata must still match.
    fn open(
        &self,
        path: &Path,
        meta: &SourceMeta,
    ) -> impl Future<Output = Result<Self::Reader, SourceError>> + Send;

    /// Whether the file still matches its metadata after sending.
    fn unchanged(&self, path: &Path, meta: &SourceMeta) -> impl Future<Output = bool> + Send;
}

/// A received file that has been finalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedFile {
    /// Final file name inside the receive directory (never a path).
    pub saved_as: String,
}

/// A file being received into a temporary location.
pub trait IncomingFile: Send {
    /// Appends bytes.
    fn write(&mut self, buf: &[u8]) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Syncs and finalizes without overwriting anything.
    fn commit(self) -> impl Future<Output = Result<SavedFile, StorageError>> + Send;

    /// Deletes the temporary file. Must be called on every failure path;
    /// dropping without it is only a best-effort fallback.
    fn discard(self) -> impl Future<Output = ()> + Send;
}

/// Where received files go.
pub trait FileSink: Send + Sync {
    /// Incoming file type.
    type File: IncomingFile;

    /// Starts receiving a file.
    fn begin(
        &self,
        name: &SafeFileName,
        size: u64,
    ) -> impl Future<Output = Result<Self::File, StorageError>> + Send;

    /// Makes new directory entries durable.
    fn sync_dir(&self) -> impl Future<Output = Result<(), StorageError>> + Send;
}

/// Stores this device's identity.
///
/// A write method that returns [`StoreError::NotDurable`] has applied the
/// change (only its durability is unconfirmed); every other error means
/// nothing was changed.
pub trait IdentityStore: Send + Sync {
    /// Loads the identity, creating one on first use. A new identity whose
    /// directory sync fails is still returned (the files are in place);
    /// the adapter logs a warning.
    ///
    /// # Errors
    /// [`StoreError`] for I/O, corruption or unsafe permissions.
    fn load_or_create(&self) -> Result<LocalIdentity, StoreError>;

    /// Replaces the key with a new one.
    ///
    /// # Errors
    /// [`StoreError`] for I/O failures; [`StoreError::NotDurable`] if the
    /// new key is in place but the directory sync failed.
    fn reset(&self) -> Result<LocalIdentity, StoreError>;

    /// Changes the device name.
    ///
    /// # Errors
    /// [`StoreError`] for I/O failures; [`StoreError::NotDurable`] if the
    /// new name is in place but the directory sync failed.
    fn set_name(&self, name: &DeviceName) -> Result<LocalIdentity, StoreError>;
}

/// Stores trusted devices.
///
/// A write method that returns [`StoreError::NotDurable`] has applied the
/// change (only its durability is unconfirmed); every other error means
/// nothing was changed.
pub trait TrustStore: Send + Sync {
    /// Looks up a fingerprint.
    ///
    /// # Errors
    /// [`StoreError`] for I/O, corruption or unsafe permissions.
    fn get(&self, fp: &Fingerprint) -> Result<Option<TrustedDevice>, StoreError>;

    /// Lists every trusted device.
    ///
    /// # Errors
    /// [`StoreError`] for I/O, corruption or unsafe permissions.
    fn list(&self) -> Result<Vec<TrustedDevice>, StoreError>;

    /// Adds or replaces a trusted device.
    ///
    /// # Errors
    /// [`StoreError`] for I/O failures; [`StoreError::NotDurable`] if the
    /// entry is stored but the directory sync failed.
    fn upsert(&self, device: TrustedDevice) -> Result<(), StoreError>;

    /// Adds or replaces the entry for `fp` with `merge(current entry)`, in
    /// one atomic step: the current entry is read under the same lock as
    /// the write, so a concurrent change (a rename, a last-seen update, a
    /// pairing) is never overwritten with stale data. Returns the stored
    /// entry.
    ///
    /// `merge` runs while the store's lock is held: it must not call back
    /// into this store's write methods (`upsert`, `upsert_with`, `remove`,
    /// `touch`), which take the same lock and would deadlock. It receives
    /// the current entry, so it has no reason to read the store either.
    ///
    /// # Errors
    /// [`StoreError`] for I/O, corruption or unsafe permissions, or if the
    /// merged entry has a fingerprint other than `fp` (nothing is changed);
    /// [`StoreError::NotDurable`] if the entry is stored but the directory
    /// sync failed.
    fn upsert_with<F: FnOnce(Option<&TrustedDevice>) -> TrustedDevice>(
        &self,
        fp: &Fingerprint,
        merge: F,
    ) -> Result<TrustedDevice, StoreError>;

    /// Removes a device; returns whether it existed.
    ///
    /// # Errors
    /// [`StoreError`] for I/O failures; [`StoreError::NotDurable`] if the
    /// device is removed but the directory sync failed (whether it existed
    /// is then not reported).
    fn remove(&self, fp: &Fingerprint) -> Result<bool, StoreError>;

    /// Records a completed session.
    ///
    /// # Errors
    /// [`StoreError`] for I/O failures; [`StoreError::NotDurable`] if the
    /// time is stored but the directory sync failed.
    fn touch(&self, fp: &Fingerprint, at: Timestamp) -> Result<(), StoreError>;

    /// Changes the local alias of a trusted device in one locked step and
    /// returns whether the device is trusted. A device that is not trusted
    /// is never added, so a rename that races a removal cannot bring the
    /// device back. The fingerprint, key, method and dates are unchanged.
    ///
    /// # Errors
    /// [`StoreError`] for I/O failures.
    fn rename(&self, fp: &Fingerprint, alias: DeviceName) -> Result<bool, StoreError>;
}

/// An offer awaiting the receiving user's decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingOffer {
    /// Transfer id (lets an interface bind its answer to this offer).
    pub transfer_id: TransferId,
    /// The sender.
    pub peer: PeerView,
    /// Verification code (unverified senders only).
    pub sas: Option<SasCode>,
    /// Offered files.
    pub files: Vec<FileEntry>,
    /// Total bytes.
    pub total_bytes: u64,
}

/// The receiving user's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Accept the offer.
    Accept,
    /// Reject the offer.
    Reject(RejectReason),
}

/// A verification code awaiting confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SasPrompt {
    /// The peer.
    pub peer: PeerView,
    /// The code both screens must show.
    pub code: SasCode,
    /// Why the code is shown.
    pub purpose: SasPurpose,
}

/// Why a verification code is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SasPurpose {
    /// Pairing devices.
    Pairing,
    /// Sending to an unverified receiver.
    UnverifiedReceiver,
}

/// Asks the local user.
pub trait Interaction: Send + Sync {
    /// Accept or reject an offer.
    fn decide_offer(&self, offer: &IncomingOffer) -> impl Future<Output = Decision> + Send;

    /// Whether the user confirmed that both screens show the same code.
    fn confirm_sas(&self, prompt: &SasPrompt) -> impl Future<Output = bool> + Send;
}

/// Wall-clock time.
pub trait Clock: Send + Sync {
    /// Now.
    fn now(&self) -> Timestamp;
}
