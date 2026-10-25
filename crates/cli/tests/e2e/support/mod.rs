// SPDX-License-Identifier: MPL-2.0
//! Shared e2e harness.

#![allow(dead_code)]

pub mod files;
pub mod proc;
pub mod proxy;
pub mod raw_peer;

pub use files::*;
pub use proc::*;

use std::time::Duration;

/// Deadline for a whole process.
pub const EXIT: Duration = Duration::from_secs(60);
