// SPDX-License-Identifier: MPL-2.0
//! Removal of stale partial files left by a crash.

use std::path::Path;
use std::time::{Duration, SystemTime};

use dropumi_domain::filename::TEMP_PREFIX;

use super::receive_dir::{TEMP_RANDOM_LEN, TEMP_SUFFIX};

/// Partial files older than this are considered abandoned.
pub const STALE_AFTER: Duration = Duration::from_secs(15 * 60);

/// Whether `name` is exactly a Dropumi temp-file name:
/// `.dropumi-` + 12 alphanumeric characters + `.part`.
#[must_use]
pub fn is_temp_name(name: &str) -> bool {
    name.strip_prefix(TEMP_PREFIX)
        .and_then(|rest| rest.strip_suffix(TEMP_SUFFIX))
        .is_some_and(|mid| {
            mid.len() == TEMP_RANDOM_LEN && mid.chars().all(|c| c.is_ascii_alphanumeric())
        })
}

/// Deletes regular files in `dir` that match the temp-file pattern exactly
/// and were last modified more than `max_age` ago. Returns how many were
/// removed. Symlinks and everything else are left alone.
#[must_use]
pub fn remove_stale_parts(dir: &Path, max_age: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_temp_name(name) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !meta.file_type().is_file() {
            continue;
        }
        let old = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .is_some_and(|age| age > max_age);
        if old && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_cleanup_pattern_and_age_only() {
        let d = tempfile::tempdir().unwrap();
        let old = d.path().join(".dropumi-abcDEF123456.part");
        let fresh = d.path().join(".dropumi-zzzzzzzzzzzz.part");
        let notes = d.path().join(".dropumi-notes.txt");
        let user = d.path().join("photo.part");
        for p in [&old, &fresh, &notes, &user] {
            std::fs::write(p, b"x").unwrap();
        }
        let past = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(past)
            .unwrap();
        std::fs::File::options()
            .write(true)
            .open(&notes)
            .unwrap()
            .set_modified(past)
            .unwrap();
        assert_eq!(remove_stale_parts(d.path(), STALE_AFTER), 1);
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(notes.exists());
        assert!(user.exists());
        assert!(is_temp_name(".dropumi-0123456789ab.part"));
        assert!(!is_temp_name(".dropumi-0123456789a.part"));
        assert!(!is_temp_name(".dropumi-01234567890-.part"));
    }
}
