// SPDX-License-Identifier: MPL-2.0
//! Domain-level size limits shared by every layer.

/// Maximum number of files in one transfer.
pub const MAX_FILES_PER_TRANSFER: usize = 10_000;
/// Maximum UTF-8 length of a file name on the wire.
pub const MAX_WIRE_NAME_BYTES: usize = 1024;
/// Maximum UTF-8 length of a sanitized local file name.
pub const MAX_SAFE_NAME_BYTES: usize = 255;
/// Maximum length of a sanitized name in UTF-16 code units after
/// canonical decomposition (HFS+ stores names this way).
pub const MAX_SAFE_NAME_UTF16: usize = 255;
/// Maximum UTF-8 length of a sanitized, displayed device name.
pub const MAX_DEVICE_NAME_DISPLAY_BYTES: usize = 64;
/// Maximum size of one file and of a whole transfer (2^63 − 1 bytes).
pub const MAX_FILE_SIZE: u64 = i64::MAX as u64;
/// Maximum extension length (including the dot) kept when truncating names.
pub const MAX_KEPT_EXTENSION_BYTES: usize = 16;
