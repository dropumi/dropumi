// SPDX-License-Identifier: MPL-2.0
//! Dropumi wire protocol v1 (sans-IO).
//!
//! This crate performs no I/O: callers feed bytes in and get bytes and
//! events out. The normative description is `docs/protocol/v1.md`.
//!
//! - [`version`]: cleartext preamble, version selection and Noise prologue;
//! - [`frame`]: CONTROL / DATA frames inside transport records;
//! - [`codec`] and [`message`]: strict CBOR control messages;
//! - [`identity`], [`handshake`], [`sas`], [`record`]: the static key, the
//!   Noise XX handshake with a committed SAS, and transport records;
//! - [`random`]: OS randomness.
#![deny(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]
#![cfg_attr(test, allow(clippy::indexing_slicing, clippy::arithmetic_side_effects))]

pub mod codec;
pub mod error;
pub mod frame;
pub mod handshake;
pub mod identity;
pub mod limits;
pub mod message;
pub mod random;
pub mod record;
pub mod sas;
pub mod version;

pub use error::ProtocolError;
