// SPDX-License-Identifier: MPL-2.0
//! Dropumi domain layer.
//!
//! Pure business types and rules with no I/O, clock, async runtime,
//! serialization or cryptography dependencies:
//!
//! - [`identity`]: public keys and fingerprint formats;
//! - [`device`], [`peer`], [`trust`]: who is on the other side;
//! - [`discovery`], [`connection`]: targets, candidates and peer expectations;
//! - [`policy`]: authorization and consent rules;
//! - [`filename`], [`text`]: file-name and display-text safety;
//! - [`transfer`]: requests, progress and the session state machine;
//! - [`error`]: the shared error taxonomy.

pub mod connection;
pub mod device;
pub mod discovery;
pub mod error;
pub mod filename;
pub mod identity;
pub mod limits;
pub mod peer;
pub mod policy;
pub mod text;
pub mod time;
pub mod transfer;
pub mod trust;
