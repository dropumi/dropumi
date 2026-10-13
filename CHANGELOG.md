# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

- Dropumi has no releases yet. Releases before 1.0 will be numbered `0.y.z`. The workspace crates are at `0.1.0` and are not published (`publish = false`).
- The wire-protocol version (currently draft v1, see [`docs/protocol/v1.md`](docs/protocol/v1.md)) is versioned separately from the application version.

## [Unreleased]

### Added

Milestone 1 (Phase 0 + Phase 1) adds the secure LAN transfer core, a draft of protocol v1 and the `dropumi` CLI. This is pre-alpha, unaudited software. There are no desktop or mobile apps, no FFI bindings and no releases yet.

#### Assessment and decisions

- **Feasibility assessment** (`docs/feasibility.md`): capability, interoperability and share-integration matrices with cited sources, and a risk register.
- **Architecture decision records** in `docs/adr/`:
  - ADR-0001 architecture and workspace layout;
  - ADR-0002 secure channel (Noise XX over TCP);
  - ADR-0003 device identity and key storage;
  - ADR-0004 pairing, trust and transfer policy;
  - ADR-0005 wire protocol v1 (draft protocol);
  - ADR-0006 integrity and receiver filesystem safety;
  - ADR-0007 LAN discovery via mDNS;
  - ADR-0008 UI framework strategy (provisional; no UI is implemented);
  - ADR-0009 FFI and bindings strategy (proposed; no FFI is implemented);
  - ADR-0010 dependency, license and CI supply-chain policy.

#### Workspace

- A Cargo workspace with five crates: `dropumi-domain`, `dropumi-protocol`, `dropumi-application`, `dropumi-infra` and `dropumi-cli`, which provides the `dropumi` binary.
  - Edition 2024, MSRV 1.89 (`rust-version` in `Cargo.toml`).
  - `unsafe` code is forbidden in every crate.
  - `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, `dbg!` and `mem::forget` are denied outside tests; the protocol crate also denies unchecked indexing and arithmetic and truncating casts.
- An architecture guard, `scripts/ci/check-architecture.sh`. It checks per-crate deny-lists so that dependencies point inward, and fails if `ring` appears in the workspace dependency graph.

#### Domain model (`dropumi-domain`)

- Public keys and fingerprints: 52 base32 characters in full, a 24-character display form and an 8-character discovery hint.
- Device names, peers, discovery targets, ranked connection candidates and peer expectations.
- Transfer policy: `TrustedOnly` by default, or a time-limited `Everyone` mode; unverified peers are never auto-accepted. Trusted devices.
- File-name sanitization (`SafeFileName`) and stripping of control, bidirectional and invisible characters (`text::strip_unsafe`).
- Transfer requests, progress, the transfer state machine and a single error taxonomy (`TransferError`).

#### Protocol v1 draft (`dropumi-protocol`, sans-IO)

- A `DRPM` version preamble, bound into the Noise prologue.
- `Noise_XX_25519_ChaChaPoly_SHA256`, implemented with `snow` 0.10 using its pure-Rust backends (no `ring`).
- Transport records (2-byte length prefix) with implicit per-direction nonce counters, never sent on the wire and owned explicitly by `Sealer`/`Opener`; CONTROL and DATA frames, with at most 65,518 B of DATA per record.
- Strict CBOR control messages (`minicbor`) with bounded decoding, and fixed, path-free `Error.detail` phrases.
- A 6-digit short authentication string (SAS) committed inside the handshake, used for pairing and for unverified peers.
- BLAKE3 integrity checking per file.
- Sender and receiver session state machines, limited to 10,000 files and 4 MiB of file metadata per transfer.

#### Application layer (`dropumi-application`)

- Nine ports (`Connector`, `Listener`, `Discovery`, `FileSource`, `FileSink`, `IdentityStore`, `TrustStore`, `Interaction`, `Clock`) and the helper traits `AdHandle` and `IncomingFile`.
- Secure channel establishment: preamble, Noise XX handshake, Hello exchange and committed SAS. The initiator checks the responder's key against the expected peer and the trust policy before it sends handshake message 3.
- Cancel-safe encrypted record reading with an idle timeout, and record writing with a write-stall timeout (120 s by default).
- Use cases:
  - resolving targets and dialing ranked candidates;
  - sending files (`SendFiles`);
  - receiving files (`ReceiveFiles`) with admission limits (16 unauthenticated connections in total and 4 per IP address, 4 sessions, 1 session per device) and cancellation of active transfers;
  - pairing (`pair_with`, `pair_listen`);
  - listing, adding and removing trusted devices;
  - showing, resetting and renaming the device identity;
  - discovery and advertisement.

#### Infrastructure adapters (`dropumi-infra`)

- TCP transport. `receive` and `pair --listen` listen on port 47321 by default and fall back to a free port when it is busy and no `--port` was given.
- A receive directory: received data goes to a `create_new` temporary `.dropumi-*.part` file and is BLAKE3-hashed while it is written; only a file whose hash matches is fsynced and finalized without overwriting anything (`name (1).ext` on collisions, symlinks never followed). Windows retries transient access-denied and sharing-violation errors; Unix has a placeholder fallback for filesystems without no-replace renames. Received files are created with mode 0600 on Unix.
- Removal of stale `.dropumi-*.part` files older than 15 minutes when a receiver starts.
- Identity, device-name and trust stores with atomic writes and a cross-process lock. On Unix, the files are created 0600 and a new configuration directory 0700, and owner and mode checks fail with exit code 9. On Windows, a warning is shown when the configuration directory is outside `%LOCALAPPDATA%`.
- mDNS advertisement and browsing (`_dropumi._tcp`, IPv4, TXT hints `v`, `n` and `fp`) with `mdns-sd`, behind the `mdns` cargo feature, which is on by default.

#### `dropumi` CLI (`dropumi-cli`)

- Commands: `identity show|set-name|reset`, `trust list|add|remove`, `pair --listen|--to`, `receive`, `send` and `discover`.
- Targets: an address, `host:port`, a fingerprint prefix or a device name; `--expect-fingerprint` pins a fingerprint and `--allow-unverified` permits an untrusted receiver after the 6-digit code is confirmed.
- Human output, NDJSON events with `--json`, and scripted control with `--control-stdin` (`accept`, `reject`, `confirm`, `deny`, `cancel`, `quit`; a decision answers only a prompt that is waiting, and offer decisions can name a `transfer_id`).
- Logging to stderr with `-v`/`-q` or `DROPUMI_LOG`.
- Typed exit codes (0 to 10, and 130 for Ctrl-C), documented in `docs/cli.md`.

#### Tests

- Unit and property (`proptest`) tests for the domain and protocol crates, including known-answer tests for the RFC 7748 X25519 vector, the fingerprint, the SAS commitment and the SAS.
- Application tests over in-memory streams with paused time.
- Infrastructure tests for finalization, permissions, stores, TCP and TXT parsing.
- Two-process end-to-end, failure and adversarial security suites on loopback (`crates/cli/tests/e2e`), and an opt-in mDNS end-to-end test (`DROPUMI_TEST_MDNS=1`).

#### CI and supply chain

- GitHub Actions workflow `ci.yml` with these jobs:
  - `fmt` (including the SPDX header check), `clippy` (all features and no default features), `docs`;
  - `test` on `ubuntu-latest`, `macos-latest` and `windows-latest`;
  - `msrv` and `architecture`;
  - `build (aarch64-linux-android)` and `build (aarch64-apple-ios)`, which build the core library crates only;
  - `cargo-deny (advisories)` and `cargo-deny (bans licenses sources)`;
  - `zizmor` and `actionlint`;
  - `mdns-e2e (experimental)`, which is allowed to fail.
- `codeql.yml` (CodeQL for Actions and Rust) and `audit.yml` (a daily advisory scan).
- `deny.toml` (license allow-list, bans on `openssl`, `openssl-sys`, `native-tls` and `ring`, crates.io only) and Dependabot configuration for Cargo and GitHub Actions.

#### Documentation and community files

- Protocol v1 draft, threat model and discovery documents (`docs/protocol/`).
- `ARCHITECTURE.md`.
- CLI, development, testing and dependency guides (`docs/`).
- The loopback benchmark script (`scripts/bench/loopback.sh`) and the benchmark method and results (`docs/benchmarks/`).
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md` (Contributor Covenant 3.0), `SECURITY.md`, `AGENTS.md` and `CLAUDE.md`.
- Issue forms and a pull request template.

[Unreleased]: https://github.com/dropumi/dropumi/commits/main
