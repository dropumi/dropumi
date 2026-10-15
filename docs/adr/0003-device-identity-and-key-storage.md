# ADR-0003: Device identity and key storage

- **Status:** Accepted
- **Date:** 2026-10-13 (reconciled with the milestone 1 code on 2026-10-14)
- **Related:** ADR-0002, ADR-0004, ADR-0007, [`../protocol/threat-model.md`](../protocol/threat-model.md)

## Context

- Peers need a stable identity they can pin, compare by eye, and type by hand. Device names and discovery records are never identity (specification §11).
- The key that authenticates the Noise channel (ADR-0002) is the natural identity: a separate signing key would have no use in Phase 1.
- Milestone 1 runs headless: in CI, over SSH and in tests that start two independent processes on one machine. OS keychains prompt or are missing in those settings (for example, headless Linux without a Secret Service).
- A local unprivileged user must not be able to read the key or inject entries into the trust store.

## Decision

**Key.** One X25519 static key per installation, which is both the Noise static key and the device identity.
- 32 bytes from `getrandom::fill` (getrandom 0.3, the same major version snow uses).
- The public key is derived through snow's `DefaultResolver` (`resolve_dh(Curve25519)` → `set` → `pubkey()`), so there is no direct `curve25519-dalek` dependency. A unit test checks the RFC 7748 §6.1 vectors.
- In memory the secret is `protocol::identity::StaticSecret(Zeroizing<[u8; 32]>)`; its `Debug` output is `StaticSecret(..)`.

**Fingerprint.** `fp = SHA-256("dropumi/v1/device-fingerprint" ‖ public_key)`, 32 bytes, always written in RFC 4648 base32, upper case, without padding.

| Form | Content | Used for |
|---|---|---|
| Full | All 32 bytes, 52 characters | `trust add` / `trust remove`, NDJSON `fingerprint`, the trust-store key |
| Display | First 24 characters (120 bits) as `XXXX-XXXX-XXXX-XXXX-XXXX-XXXX` | Human output and visual comparison; NDJSON `fingerprint_display` |
| mDNS hint | First 8 characters (40 bits) | TXT `fp` key; a selection hint only, never authentication (ADR-0007) |

- Parsing is case-insensitive and ignores `-` and spaces. A full fingerprint must normalize to exactly 52 characters.
- A `FingerprintPrefix` has 8 to 52 characters; it counts as a pin (`is_pin()`) at 24 or more characters. `--expect-fingerprint` requires at least 24 characters.

**Device name.** Display-only and untrusted. The default is a random `Dropumi-XXXX` (4 upper-case hex digits from 2 random bytes; if none of them is 0, 1, 8 or 9, the last one is replaced by one of those, so a default name is never read as a fingerprint prefix and always works as a `--to` name), never the OS hostname. `identity set-name` changes it. `DeviceName::sanitize` strips unsafe characters (`text::strip_unsafe`, ADR-0006), applies NFC, trims, truncates to 64 bytes on a character boundary, and turns an empty result into `Unnamed device`.

**Config directory.** Resolution order: `--config-dir` > `DROPUMI_CONFIG_DIR` > `dirs::config_local_dir()/dropumi` (LocalAppData on Windows, so the key does not roam). A directory Dropumi creates gets mode 0700 on Unix; an existing directory is never chmod'ed, only checked.

| File | Content | Protection |
|---|---|---|
| `identity.key` | 8-byte magic `DRPMKEY1` + 32-byte secret (exactly 40 bytes; anything else is reported as corrupt) | 0600. First creation: a 0600 temp file, fsync, then `persist_noclobber` (and a directory fsync on Unix), so concurrent first runs end with one identity. `identity reset` replaces it atomically under the lock. |
| `device.json` | `{"version":1,"name":"…"}` | 0600, atomic replace |
| `trusted_devices.json` | `{"version":1,"devices":[…]}` (ADR-0004) | 0600, atomic replace, re-read from disk on every lookup |
| `.lock` | Empty lock file | 0600; std `File::lock` (exclusive) around every read-modify-write and around identity creation |

Atomic replace means: a 0600 temp file in the same directory, `sync_all`, rename over the target (Windows: retried on errors 5 and 32), then a directory fsync on Unix. A failed directory fsync is reported as an error even though the new file may already be in place, because its durability was not confirmed.

**Permission checks** run whenever a command opens the identity or trust store (`identity`, `trust`, `pair`, `receive`, `send`), and again on every read of a store file. `discover` does not read the config directory.
- **Unix:** every checked path must be owned by the effective UID (`rustix::process::geteuid`) and must not be a symbolic link. The config directory, `device.json` and `trusted_devices.json` must have no `0o022` bits (no group or other write). `identity.key` must have no `0o077` bits (no group or other access).
- Any violation is `StoreError::InsecurePermissions`. The CLI reports it locally with a `chmod 700` or `chmod 600` hint and exits with code **9** (local storage or configuration).
- **Windows:** ACLs are not inspected. Dropumi relies on the per-user ACL of `%LOCALAPPDATA%`; if the config directory is outside `dirs::data_local_dir()`, a `warning` event is emitted. An explicit `--config-dir` inherits its parent folder's ACL; this is a documented risk.

**Reset, rotation and revocation.** `identity reset --yes` generates a new key, which is a new identity: peers then see an unverified device and must pair again. Without `--yes` the command refuses with a usage error (exit 2). The device name is kept. v1 has no in-band rotation and no revocation; peers remove old entries by hand.

**Later phases.** OS keychain adapters (Phase 2) and mobile secure storage (iOS Keychain, an Android Keystore-wrapped key) arrive behind the same `IdentityStore` port. Whether Android Keystore can hold X25519 keys natively is UNVERIFIED.

## Implementation

- Code: [`crates/protocol/src/identity.rs`](../../crates/protocol/src/identity.rs) (key, fingerprint), [`crates/domain/src/identity.rs`](../../crates/domain/src/identity.rs) (text forms, prefixes), [`crates/domain/src/device.rs`](../../crates/domain/src/device.rs) (names), [`crates/infra/src/store/`](../../crates/infra/src/store/) (`FileIdentityStore`, `TrustFile`, atomic writes, lock), [`crates/infra/src/perms.rs`](../../crates/infra/src/perms.rs), [`crates/infra/src/paths.rs`](../../crates/infra/src/paths.rs), [`crates/cli/src/commands/mod.rs`](../../crates/cli/src/commands/mod.rs) (exit code 9).
- Tests (all passed in the local run on macOS 27 arm64; CI pending): `fingerprint_kat`, `x25519_rfc7748_vector`, `debug_redacts_secret`, `display_and_parse_roundtrip`, `parse_rejects_bad_input`, `identity_created_0600_in_0700`, `identity_rejects_group_readable`, `identity_bad_magic_rejected`, `concurrent_create_single_identity`, `reset_replaces_key`, `trust_store_rejects_group_writable`, `concurrent_writers_serialized`, `config_dir_rejects_world_writable`, `config_dir_outside_localappdata_detected`.
- The Unix permission checks were exercised only on macOS in the local run. The Windows warning path has a unit test for the path comparison only; Windows itself is Not tested (pending CI).

## Consequences

### Positive

- One key, one identity: what the user pins is exactly what the handshake authenticates.
- Works headless, in CI and in parallel test processes, with no prompts.
- On Unix, another local user cannot read the key or plant trust entries without the check failing.
- One alphabet (base32) for full, display and hint forms; prefixes compose naturally.

### Negative

- The key is a plaintext 0600 file: it is exposed to malware running as the same user, to backups and cloud sync, and to anyone with access to an unencrypted disk.
- snow 0.10.0 keeps its own copies of the private key in plain `[u8; 32]` fields: its source has no `zeroize` and no `Drop` impl that wipes them (checked in the published crate source). Only Dropumi's own copy is zeroized.
- Windows ACLs are not checked; only a warning is shown outside LocalAppData.
- No rotation: a lost or reset key breaks every existing pairing.
- The 52-character full fingerprint is long to type for manual trust.

## Alternatives considered

- **OS keychain now:** prompts or is unavailable on headless Linux and CI; deferred to Phase 2 behind the same port.
- **Ed25519 identity (converted or with a binding signature):** no Phase 1 use for signatures, and it would add a second curve library major version.
- **Direct `curve25519-dalek` dependency:** snow's resolver already derives the public key.
- **Hex fingerprints:** longer for the same entropy; base32 is case-insensitive and avoids ambiguous characters.
- **OS hostname as the default name:** leaks a personal identifier to the network.
- **Roaming AppData on Windows:** would copy the private key to other machines.
