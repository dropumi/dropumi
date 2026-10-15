// SPDX-License-Identifier: MPL-2.0
//! The system clock.

use std::time::{SystemTime, UNIX_EPOCH};

use dropumi_application::ports::Clock;
use dropumi_domain::time::Timestamp;

/// Wall-clock time from the operating system.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        )
    }
}
