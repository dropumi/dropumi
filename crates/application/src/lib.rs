// SPDX-License-Identifier: MPL-2.0
//! Dropumi application layer: ports and use cases.
//!
//! Use cases depend only on the domain, the protocol and the [`ports`];
//! adapters live in `dropumi-infra` and are injected through constructors.

pub mod channel;
pub mod config;
pub mod error;
pub mod events;
pub mod identity;
pub mod ports;
pub mod usecases;
