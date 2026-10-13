// SPDX-License-Identifier: MPL-2.0
//! Dropumi infrastructure adapters.
//!
//! Real implementations of the application ports: TCP ([`net`]), the
//! filesystem ([`fs`]), configuration stores ([`store`]), permissions
//! ([`perms`]), the clock ([`clock`]), DNS-SD TXT hints ([`txt`]) and,
//! with the `mdns` feature, mDNS discovery (`mdns`).

pub mod clock;
pub mod fs;
#[cfg(feature = "mdns")]
pub mod mdns;
pub mod net;
pub mod paths;
pub mod perms;
pub mod store;
pub mod txt;
