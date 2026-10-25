// SPDX-License-Identifier: MPL-2.0
//! Wall-clock timestamps supplied by a clock port.

use std::time::Duration;

/// Seconds since the Unix epoch. The domain never reads the clock itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Adds a duration, saturating at the maximum.
    #[must_use]
    pub fn saturating_add(self, d: Duration) -> Self {
        Self(self.0.saturating_add(d.as_secs()))
    }

    /// Seconds since the Unix epoch.
    #[must_use]
    pub const fn as_secs(self) -> u64 {
        self.0
    }
}
