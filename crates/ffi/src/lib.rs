// SPDX-License-Identifier: MPL-2.0
//! Dropumi's foreign-function interface for the native apps.
//!
//! [UniFFI](https://mozilla.github.io/uniffi-rs/) generates the Swift and
//! Kotlin bindings from this crate (ADR-0009). The crate is a composition
//! root, like the CLI: it wires the core's use cases to the infra adapters
//! and to two kinds of app code, the platform's discovery service
//! ([`PlatformDiscovery`]) and documents the app reads ([`ForeignFile`]).
//!
//! The crate keeps the workspace's `unsafe_code = "forbid"`. The FFI
//! scaffolding that UniFFI's procedural macros generate does contain
//! `unsafe` code (exported `extern "C"` functions), which rustc does not
//! attribute to this crate; it is UniFFI's code, pinned to one version.

uniffi::setup_scaffolding!();

mod discovery;
mod error;
mod mailbox;
mod node;
mod prompts;
mod runtime;
mod source;
mod types;

pub use discovery::{BrowseSink, PlatformDiscovery, SERVICE_TYPE, ServiceAd, TxtEntry};
pub use error::{CallbackError, ConfigProblem, DropumiError, ErrorCode};
pub use mailbox::EventListener;
pub use node::{
    DropumiNode, NodeOptions, NodeSnapshot, OperationHandle, OperationInfo, ffi_api_version,
};
pub use source::{ForeignFile, ForeignReader, SendItem};
pub use types::*;
