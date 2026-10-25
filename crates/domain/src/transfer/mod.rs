// SPDX-License-Identifier: MPL-2.0
//! Transfers: requests, progress, outcomes and the session state machine.

mod progress;
mod request;
mod session;
mod status;

pub use progress::{ProgressError, TransferProgress};
pub use request::{FileEntry, RequestError, TransferId, TransferRequest};
pub use session::{Direction, InvalidTransition, TransferEvent, TransferSession};
pub use status::{AbortReason, CancelReason, FileOutcome, Party, RejectReason, TransferStatus};
