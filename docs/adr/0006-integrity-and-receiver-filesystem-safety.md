# ADR-0006: Integrity and receiver filesystem safety

- **Status:** Accepted
- **Date:** 2026-10-13 (reconciled with the milestone 1 code on 2026-10-14)
- **Related:** ADR-0002, ADR-0005, [`../protocol/v1.md`](../protocol/v1.md), [`../protocol/threat-model.md`](../protocol/threat-model.md)

## Context

- The specification requires protection against corrupted transfers, malicious file names, path traversal and unsafe symlinks; temporary files with atomic finalization; no overwriting of existing files without an explicit policy; and preserving original names while handling collisions safely.
- Progress and completion must never claim more durability than the receiver actually has.
- Platform behaviour that shapes the design:
  - `std::fs::rename` replaces an existing target on POSIX and on Windows, so it must never be used for the final name.
  - `tempfile` 3.27.0 `persist_noclobber` uses `renameat_with(…, RenameFlags::NOREPLACE)` on Linux, Android and Apple targets and falls back to `hard_link` + `unlink` on `EINVAL`/`ENOSYS`; on Windows it calls `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` (both checked in the crate source).
  - exFAT and vfat volumes may reject the no-replace rename or hard links (`ENOTSUP`, `EOPNOTSUPP`, `EPERM`). `ENOTSUP` and `EOPNOTSUPP` have different values on Apple platforms.
  - On Windows, antivirus scanners and indexers can briefly hold files open, producing `ERROR_ACCESS_DENIED` (5) or `ERROR_SHARING_VIOLATION` (32).
  - Peer-supplied text can carry invisible or direction-changing characters (for example U+202E, which makes `invoice<U+202E>fdp.exe` display as `invoiceexe.pdf`).

## Decision

**1. Integrity.** AEAD on every record (ADR-0002) plus a **BLAKE3-256** hash per file (`blake3` 1.8.7, algorithm id 1, required in `Hello.hash_algs`). The sender hashes while reading and the receiver while writing, inside the protocol session machines. `FileEnd` carries `[1, digest]`. On a mismatch the temp file is discarded, the receiver sends `FileAck INTEGRITY_FAILED`, the transfer continues, and the final outcome is "completed with failures" (CLI exit code 8).

**2. Completion semantics.**
- **`FileAck STORED`** means: the byte count matched, the hash matched, the content was `sync_all`'d, and the no-clobber finalization is done, so the final name is visible.
- Durability of the **directory entries** is promised only by **`CompleteAck`**, which the receiver sends after one fsync of the destination directory (Unix). On Windows `sync_dir` is a no-op and Dropumi relies on NTFS metadata journaling (documented assumption, UNVERIFIED).
- The sender reports success only after `CompleteAck` with every file STORED.
- Other `FileAck` statuses: `INTEGRITY_FAILED`; `WRITE_FAILED` (followed by `Cancel STORAGE`); `ABORTED` (the sender sent `FileAbort`).

**3. Name handling** (`crates/domain/src/filename.rs`, `crates/domain/src/text.rs`).
- Wire validation (`validate_wire_name`): 1..=1,024 bytes, not `.` or `..`, no `/`, `\` or NUL; anything else is a protocol violation.
- `text::strip_unsafe` removes C0, DEL and C1 controls, U+2028 (Zl), U+2029 (Zp) and every General_Category=Cf code point **except** U+200C ZWNJ and U+200D ZWJ. That covers bidi controls, U+200B, U+FEFF, U+00AD, U+2060-2064 and tag characters; variation selectors (Mn) are kept. The Cf table is generated from UCD 17.0.0, the same Unicode version as `unicode-normalization` 0.1.25, and the test `unicode_version_matches_normalization` fails if the two drift. The same function sanitizes device names, TXT values and file names; the CLI's `terminal_safe` removes ANSI escape sequences and then applies `strip_unsafe` to peer-supplied strings in human output (test `terminal_safe_strips_ansi_bidi_and_invisibles`).
- `SafeFileName::sanitize` is total (never fails). Steps, in order:
  1. Keep only the last path component (split on `/` and `\`).
  2. `strip_unsafe`.
  3. NFC normalization.
  4. Replace `<>:"|?*` with `_` (on every OS).
  5. Fit the length: shorten the stem on a character boundary, keeping an extension of at most 16 bytes, until the name is at most 255 UTF-8 bytes in NFC and at most 255 UTF-16 code units after canonical decomposition (the HFS+ limit); trim leading spaces and trailing spaces and dots; re-normalize to NFC if truncation broke it; an empty result becomes `file`.
  6. On the fitted result: an empty result, `.` or `..` becomes `file`; prefix `_` to Windows reserved device names (CON, PRN, AUX, NUL, CONIN$, CONOUT$, COM0-9, LPT0-9, COM¹²³, LPT¹²³), compared case-insensitively on the part before the first `.` with trailing spaces removed, on every OS; prefix `_` to names starting with `.dropumi-` (ASCII case-insensitive), protecting the temp-file namespace.
  7. Fit again and repeat step 6 until nothing changes, so truncation cannot turn a name back into a device name or a temp-file name.
- Collisions become `stem (n).ext` for n = 1..=9999, still within the length limits; a leading dot does not start an extension. One finalization probes at most 256 names (`MAX_PROBES_PER_FILE`), starting after the highest suffix the receive directory already used for the same sanitized name. After 256 probes, or past 9999, one final attempt uses `stem-<12 lowercase hex characters>.ext` (`SafeFileName::with_tag`; the tag is never truncated away).

**4. Temp files.** `tempfile::Builder::new().prefix(".dropumi-").suffix(".part").rand_bytes(12).tempfile_in(dir)`: created exclusively (`O_EXCL`), mode 0600 on Unix, in the destination directory. The destination must already exist and be a directory; it is canonicalized once and never created implicitly (the default is the user's existing Downloads folder; otherwise `--dir` or `DROPUMI_RECEIVE_DIR` is required, exit 9).

**5. Finalization** (`infra::fs::finalize`, run in `spawn_blocking`; the file operations sit behind the injectable `FinalizeOps` trait so error paths are tested deterministically):
- `IncomingFile::commit`: flush → `sync_all` → close the handle; then, for each candidate name (at most 256, from the remembered next suffix up to 9999), call `persist_noclobber`:
  - `Ok` → done; `AlreadyExists` → next n.
  - **Windows, raw OS error 5 or 32:** retry after 20, 40, 80, 160 and 320 ms (at most 5 retries). If it still fails and anything exists at the candidate name, treat the name as occupied and try the next n; otherwise fail with a storage error. Whether a directory at the target name produces error 5 is UNVERIFIED.
  - **Unix only, placeholder fallback**, when the error kind is `Unsupported` or the errno is `NOTSUP`, `OPNOTSUPP` or `PERM` (compared as `rustix::io::Errno` values): create the candidate with `create_new` and mode 0600 (`AlreadyExists` → next n) and record its `(dev, ino)`; then replace only that placeholder with the temp file. If that fails, remove the candidate **only if** it is still a regular, zero-length file with the same `(dev, ino)`, then return the error.
  - Any other error → storage error; the temp file is deleted.
- `std::fs::rename` is never called. No existence pre-check chooses the name (the only `exists` check runs after the Windows retries are exhausted), so case-insensitive and normalization collisions surface as `AlreadyExists` and get a suffix. Received files are 0600 on Unix (deliberate).

**6. Symlinks are never followed.** The temp file is created exclusively; no-replace rename, hard link and the Windows move do not follow an existing entry; a symlink or dangling symlink at the final name is never replaced; names are always a single sanitized component.

**7. Cleanup.** `IncomingFile::discard()` is async and called on every cancel and failure path: it closes the handle and deletes the temp file with `remove_file` in `spawn_blocking` (on Windows retried after 50, 100 and 200 ms on errors 5/32). A final failure is logged as a warning without the path and left for stale cleanup. Dropping an unfinished file deletes its temp file as a best-effort, non-sleeping fallback. At `receive` start, only regular files named exactly `.dropumi-` + 12 ASCII alphanumerics + `.part` and last modified more than 15 minutes ago are deleted; symlinks and everything else are left alone.

**8. Error privacy.** Local storage errors stay local; the peer only sees `FileAck WRITE_FAILED`, `Cancel STORAGE` or the static `Error.detail` phrase (ADR-0005).

**9. Sender side** (`infra::fs::source`). Only regular files are sent (an explicitly named symlink is followed, like `cp`; directories and special files are refused). Only the final component goes on the wire (lossy UTF-8, `/`, `\` and NUL replaced with `_`, at most 1,024 bytes). A size or modification-time change while sending yields `FileAbort SOURCE_CHANGED`. The sender compares the size and the modification time at full resolution (local only; the wire carries whole seconds). On file systems that store whole seconds, a same-size edit within the same second can still go undetected; the receiver's BLAKE3 check still verifies the bytes that were sent.

## Implementation

- Code: [`crates/domain/src/filename.rs`](../../crates/domain/src/filename.rs), [`crates/domain/src/text.rs`](../../crates/domain/src/text.rs) and `text/cf_table.rs`, [`crates/protocol/src/session/`](../../crates/protocol/src/session/) (hashing, ack state), [`crates/infra/src/fs/`](../../crates/infra/src/fs/) (`receive_dir.rs`, `finalize.rs`, `cleanup.rs`, `source.rs`), [`crates/application/src/usecases/receive.rs`](../../crates/application/src/usecases/receive.rs) (`discard` on every failure path, `sync_dir` before `CompleteAck`).
- Tests (all passed in the local run on macOS 27 arm64, APFS; CI pending): `sanitize_fixtures`, `sanitized_is_safe`, `sanitized_is_safe_any_string`, `collision_suffix_bounded`, `strips_cc_zl_zp_cf_keeps_zwj_zwnj_vs`, `unicode_version_matches_normalization`, `hash_mismatch_reports_integrity_failed`, `persist_never_overwrites_existing`, `case_variant_collision_keeps_original`, `symlink_at_final_name_not_followed`, `dangling_symlink_not_replaced`, `received_file_mode_0600`, `dest_must_be_existing_dir`, `placeholder_fallback_never_overwrites`, `placeholder_removed_when_second_persist_fails`, `collision_suffix_sequence`, `stale_cleanup_pattern_and_age_only`, `discard_called_on_every_failure_path`, `storage_error_detail_not_leaked`, `source_changed_sends_file_abort`, `filename::tests::truncation_cannot_resurrect_device_names`, `filename::tests::hfs_plus_decomposed_length_bounded`, `filename::tests::tag_survives_long_names`, `fs::finalize::tests::probe_cap_switches_to_random_name`, `fs::receive_dir::tests::duplicate_names_probe_from_hint`, and the e2e tests `security::traversal_and_reserved_names`, `security::wrong_file_hash_integrity_failed`, `transfer::existing_file_never_overwritten`, `failure::stale_parts_cleaned_on_start`, `failure::sender_killed_receiver_cleans_up`.
- The Windows retry and placeholder branches are covered only by injected errors through `FinalizeOps`; real Windows and real exFAT/vfat volumes are Not tested.

## Consequences

### Positive

- Existing files are never overwritten on any code path; collisions keep both files.
- Hostile names (traversal, reserved device names, RTLO, invisible characters, our own temp prefix) are neutralized identically on every OS.
- Progress and acknowledgements never claim durability the receiver does not have.
- Every error path in finalization is testable without special filesystems.

### Negative

- **Security gap:** received files get no macOS quarantine attribute and no Windows Mark-of-the-Web, so opening a received executable may not trigger the Gatekeeper or SmartScreen checks that a download would. This is a Phase 2 priority.
- The placeholder path leaves a time-of-check/time-of-use window on exFAT/vfat between creating the placeholder and replacing it. Behaviour on real exFAT/vfat volumes is UNVERIFIED.
- Per-file `sync_all` (`F_FULLFSYNC` on Apple platforms) costs throughput with many small files. Measured on the maintainer's machine over loopback (`scripts/bench/loopback.sh`, macOS 27, Apple M4 Pro, APFS, median of 3 runs): 1,000 files of 100 KiB at 20.3 MiB/s versus one 256 MiB file at 386.1 MiB/s. How much of that gap is fsync was not isolated.
- No disk-space precheck (Reject reason 4 is reserved); running out of space during a write is the guard.
- 0600 permissions on received files may surprise users who expect their umask defaults.

## Alternatives considered

- **AEAD only, no per-file hash:** misses storage and logic bugs and gives nothing for future resume.
- **SHA-256 per file:** slower than BLAKE3 on devices without SHA extensions; can be added later via `hash_algs`.
- **`std::fs::rename` or `exists()` then rename:** overwrites, or races.
- **Directory fsync before every `FileAck`:** per-file cost; durability is promised once at `CompleteAck` instead.
- **Overwrite-with-prompt policy:** more UI and more risk; collision suffixes keep both files.
- **`unicode-properties` crate for the Cf table:** non-SPDX license string and one more dependency.
- **`fs4` disk-space precheck now:** advisory only (quotas, compression, concurrent writers); deferred.
