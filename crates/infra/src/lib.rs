// SPDX-License-Identifier: MPL-2.0
//! Dropumi infrastructure adapters.
//!
//! Real implementations of the application ports: TCP ([`net`]), the
//! filesystem ([`fs`]), configuration stores ([`store`]), permissions
//! ([`perms`]), the clock ([`clock`]) and DNS-SD TXT hints ([`txt`]).

pub mod clock;
pub mod fs;
pub mod net;
pub mod paths;
pub mod perms;
pub mod store;
pub mod txt;
