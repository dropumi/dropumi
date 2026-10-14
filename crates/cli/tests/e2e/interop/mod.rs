// SPDX-License-Identifier: MPL-2.0
//! The apps' real cores against the real CLI (D11). Opt-in, one module per
//! platform family.

#[cfg(target_os = "macos")]
mod apple;
