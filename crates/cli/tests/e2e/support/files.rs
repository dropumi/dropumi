// SPDX-License-Identifier: MPL-2.0
//! Deterministic test files and portable comparisons.
//!
//! Compare file names (never full paths: canonicalization differs, e.g.
//! `/var` vs `/private/var` on macOS) and hash with SHA-256, independently
//! of the BLAKE3 used by the protocol.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Writes `len` deterministic pseudo-random bytes.
pub fn write_pattern(path: &Path, len: u64, seed: u64) -> PathBuf {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut x = seed | 1;
    let mut left = len;
    let mut block = vec![0u8; 64 * 1024];
    while left > 0 {
        for chunk in block.chunks_mut(8) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let bytes = x.to_le_bytes();
            let n = chunk.len();
            chunk.copy_from_slice(&bytes[..n]);
        }
        let n = usize::try_from(left.min(block.len() as u64)).unwrap();
        f.write_all(&block[..n]).unwrap();
        left -= n as u64;
    }
    f.flush().unwrap();
    path.to_path_buf()
}

/// Writes literal content.
pub fn write(path: &Path, content: &[u8]) -> PathBuf {
    std::fs::write(path, content).unwrap();
    path.to_path_buf()
}

/// Streaming SHA-256 hex digest.
pub fn sha256(path: &Path) -> String {
    let mut f = std::fs::File::open(path).unwrap();
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// File names in a directory.
pub fn dir_names(dir: &Path) -> BTreeSet<OsString> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect()
}

/// Whether any Dropumi temp file is left in `dir`.
pub fn has_part_files(dir: &Path) -> bool {
    dir_names(dir)
        .iter()
        .any(|n| n.to_string_lossy().starts_with(".dropumi-"))
}

/// The `saved_as` names reported by a receiver.
pub fn saved_names(events: &[serde_json::Value]) -> BTreeSet<String> {
    events
        .iter()
        .filter(|e| e["event"] == "file_saved")
        .map(|e| e["saved_as"].as_str().unwrap().to_owned())
        .collect()
}
