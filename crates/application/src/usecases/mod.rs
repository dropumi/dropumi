// SPDX-License-Identifier: MPL-2.0
//! Use cases.
//!
//! | Use case (spec §9) | Code |
//! |---|---|
//! | DiscoverNearbyDevices, ManageDiscoverability | [`discovery`] |
//! | ConnectToPeer | [`connect`] |
//! | PairDevice | [`pair`] |
//! | SendFiles, CancelTransfer (sender), QueryTransferProgress | [`send`] |
//! | ReceiveFiles, AcceptTransfer, RejectTransfer, CancelTransfer (receiver) | [`receive`] |
//! | ManageTrustedDevices | [`trust`] |
//! | device identity | [`identity`] |
//!
//! Accepting and rejecting go through the [`crate::ports::Interaction`]
//! port, cancellation through a `CancellationToken`, and progress through
//! [`crate::events::AppEvent::Progress`]. ConfigureBackgroundReceiver is
//! out of scope for this milestone.

pub mod admission;
pub mod connect;
pub mod discovery;
pub mod identity;
pub mod pair;
pub mod receive;
pub mod send;
pub mod trust;

use dropumi_domain::discovery::ResolveError;
use dropumi_domain::error::TransferError;

use crate::error::StoreError;

/// Errors that stop a use case before any session outcome exists.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UseCaseError {
    /// Invalid input (bad path, directory, too many files…).
    #[error("{0}")]
    Usage(String),
    /// A target could not be resolved.
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    /// Discovery is needed but unavailable.
    #[error("discovery unavailable: {0}")]
    DiscoveryUnavailable(String),
    /// Local configuration failure.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A failure before a session existed (for example listening).
    #[error(transparent)]
    Transfer(#[from] TransferError),
}
