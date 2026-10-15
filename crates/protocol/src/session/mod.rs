// SPDX-License-Identifier: MPL-2.0
//! Sans-IO transfer session machines.
//!
//! [`SenderMachine`] produces the initiator's messages and validates the
//! receiver's answers; [`ReceiverMachine`] validates everything the
//! initiator sends. Both hash file content (BLAKE3) as it passes through.
//! Neither performs I/O: the application layer moves the bytes.

mod receiver;
mod sender;

pub use receiver::{ReceiverEvent, ReceiverMachine};
pub use sender::{SenderEvent, SenderMachine};

use crate::error::ProtocolError;
use crate::limits::MAX_CONSECUTIVE_IGNORABLE;

/// Counts consecutive ignorable messages.
#[derive(Debug, Default, Clone, Copy)]
struct IgnorableBudget(usize);

impl IgnorableBudget {
    fn ignored(&mut self) -> Result<(), ProtocolError> {
        self.0 = self.0.saturating_add(1);
        if self.0 > MAX_CONSECUTIVE_IGNORABLE {
            return Err(ProtocolError::LimitExceeded(
                "consecutive ignorable messages",
            ));
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.0 = 0;
    }
}
