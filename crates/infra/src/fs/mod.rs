// SPDX-License-Identifier: MPL-2.0
//! Filesystem adapters: sources, the receive directory and cleanup.

pub mod cleanup;
pub mod finalize;
pub mod receive_dir;
pub mod source;

pub use receive_dir::{IncomingFsFile, ReceiveDir};
pub use source::LocalFileSource;
