// SPDX-License-Identifier: MPL-2.0
//! End-to-end tests: two independent `dropumi` processes on loopback.
//!
//! Required tests use manual addresses (`--no-mdns`); the mDNS test is
//! opt-in (`DROPUMI_TEST_MDNS=1`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::cast_possible_truncation
)]

mod support;

// Unix only: it makes the directory sync fail with directory modes.
#[cfg(unix)]
mod config;
mod discovery;
mod failure;
// The app facade ships for Apple platforms and Android only.
#[cfg(unix)]
mod ffi_interop;
// The apps against the CLI (opt-in: DROPUMI_TEST_APPLE=1).
mod interop;
mod pairing;
mod policy;
mod security;
mod transfer;
