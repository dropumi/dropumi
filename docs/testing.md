# Testing

How Dropumi is tested, how to run the tests, and exactly what has been verified where. Commands that mirror CI are in
[`development.md`](development.md).

## Current status

| Item | Result |
|---|---|
| Latest recorded run | macOS 27 arm64 (Apple M4 Pro), Rust 1.99.0, maintainer machine, 2026-10-14, on the milestone 2a code (`dropumi-ffi` and the core changes it needed) |
| `cargo test --workspace --all-features --locked` | **309 passed, 0 failed, 3 ignored** (the opt-in mDNS test and the two opt-in FFI measurements) |
| Two-process e2e suite | 42 passed, 1 ignored (32.03 s), including the 5 facade-against-CLI interop tests. Milestone 1's 37 tests were green 3 times in a row (31.73 s, 31.64 s and 31.61 s) |
| Swift binding tests (`swift test --package-path apps/apple/DropumiKit`) | **6 passed** (Swift 6.4, Xcode 27.0, macOS slice of the XCFramework) |
| Kotlin binding tests (`./apps/android/gradlew -p apps/android :core-bindings:testDebugUnitTest`) | **6 passed, 0 failed** (AGP 9.4.1, Gradle 9.8.1, Temurin 21, host JVM on macOS 27 arm64) |
| Opt-in mDNS e2e (`DROPUMI_TEST_MDNS=1`) | Passed on loopback in a separate local run (8.37 s) |
| Scripted terminal-mode smoke test | Passed on macOS 27 arm64 and in the Debian 12 aarch64 container (as root). A script (not in the repository) ran two `dropumi` processes, each on its own pseudo-terminal, and typed the answers: pairing with matching codes; a `y` typed before the first prompt did not answer it (the `n` typed at the prompt rejected the offer); a trusted transfer with equal SHA-256; `receive --accept trusted` without a prompt; Ctrl-C (exit 130); and a second Ctrl-C exiting at once while a silent connection held the graceful shutdown. A second script showed stray input answering a prompt in 5 of 5 tries before the terminal flush (macOS only: one run on the code without the flush) and in 0 of 5 after it (macOS and the container), both for a line typed before the first prompt and for a partial line typed before a later prompt |
| Linux, full workspace suite (milestone 1 code) | **225 passed, 0 failed, 1 ignored** in a Debian 12 (bookworm) aarch64 container (Docker on the same machine, Rust 1.99.0, non-root user, `--no-fail-fast`), including the Linux-only `transfer::bounded_memory_independent_of_size` |
| Windows target | Milestone 1 code: for `x86_64-pc-windows-gnu`, with MinGW in the Linux container image, `cargo clippy --workspace --all-targets -D warnings` (both feature sets) clean and `cargo build --workspace --all-targets --all-features` compiled and linked. Milestone 2a: `cargo xwin check -p dropumi-ffi -p dropumi-cli --all-targets --target x86_64-pc-windows-msvc` passed (the FFI's integration tests are Unix only). Nothing was run |
| Apple app package tests (`swift test --package-path apps/apple/DropumiKit`, milestone 2b) | **38 passed** (`ShareInboxTests` 8, `BindingsTests` 6, `AppModelTests` 24; macOS 27.0.1 arm64, Xcode 27.0, 2026-10-15) |
| Apple app, hosted and UI state tests (`xcodebuild test -scheme Dropumi-iOS`, iPhone 17 simulator, iOS 27.0) | **3 hosted tests passed, 8 UI state tests passed** (fake core, each with the accessibility audit); the interop suite is skipped outside its harness |
| Apple app, hosted tests on macOS (`xcodebuild test -scheme Dropumi-macOS -only-testing:DropumiMacTests`) | **5 passed** (2026-10-15), including `MenuBarReceiverTests`: the core opens from the app delegate with no window, and offer and "Files received" notifications are fed with no view. The macOS UI state tests did not run: UI automation needs the user's approval on the machine |
| Apple app against the CLI (`scripts/apple/interop.sh`, opt-in) | **6 passed** on loopback: transfers both ways after comparing a code, and pairing, each on the iOS 27.0 simulator and on macOS 27. The two macOS Bonjour tests were not run (they ask the macOS Local Network question) |
| GitHub Actions CI | **Not run yet.** The maintainer pushes the branch; every runner row below stays "Not tested (pending CI)" until its job is green. |

Automated tests run both peers on one machine over loopback (`127.0.0.1`). Nothing has been tested between two real
devices.

## Running the tests

```sh
cargo test --workspace --all-features --locked                       # everything except the opt-in mDNS test
cargo test -p dropumi-cli --test e2e --locked                         # two-process e2e suite only
DROPUMI_TEST_MDNS=1 cargo test -p dropumi-cli --test e2e --all-features --locked -- --ignored mdns_
DROPUMI_TEST_LARGE_MIB=4096 cargo test -p dropumi-cli --test e2e --locked -- transfer::large_file_streaming
cargo test --release -p dropumi-ffi --test measure --locked -- --ignored --nocapture   # FFI measurements (ADR-0009)

# Binding tests (need the XCFramework or the host library; see development.md)
bash scripts/ffi/build-apple.sh --profile dev && swift test --package-path apps/apple/DropumiKit
cargo build --locked -p dropumi-ffi --lib && ./apps/android/gradlew -p apps/android :core-bindings:testDebugUnitTest

# Apple apps (see apps/apple/README.md)
(cd apps/apple && xcodegen generate)
xcodebuild test -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-iOS \
  -destination 'platform=iOS Simulator,name=iPhone 17,OS=27.0' -derivedDataPath apps/apple/build
xcodebuild test -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-macOS \
  -destination 'platform=macOS,arch=arm64' -derivedDataPath apps/apple/build
bash scripts/apple/interop.sh                                         # the app's core against the CLI (opt-in)
```

The Apple app tests are labelled by what they exercise:
- *hosted tests* run inside the app with temporary folders;
- *UI state tests (fake core)* show every screen on a scripted core and say nothing about the network;
- *interop tests* run the app's real core, through the Swift layer, against the CLI on loopback.

The tests never touch the real configuration: every process gets a temporary configuration directory and a temporary
receive directory.

## Test layers and counts

Counts are from the latest recorded run (`test result` lines of `cargo test`). The four library doc-test targets
contain no tests.

| Crate | Target | Kind | Passed | Ignored |
|---|---|---|---|---|
| `dropumi-domain` | `src/lib.rs` (unit) | Fingerprints, targets, candidate ranking, expectation rules, policy matrix, filename sanitizer fixtures (including truncation that cannot recreate reserved names and the HFS+ length bound), invisible-character stripping, default device names (each of the 65,536 random seeds gives a name target), transfer state machine | 28 | 0 |
| `dropumi-domain` | `tests/filename_props.rs` | proptest: sanitized names are safe for any input; collision suffixes stay bounded | 3 | 0 |
| `dropumi-domain` | `tests/name_props.rs` | proptest: device names and stripped text are safe, NFC, bounded and idempotent for any input | 3 | 0 |
| `dropumi-domain` | `tests/identity_props.rs` | proptest: fingerprint and key forms round-trip; fingerprint, key and target parsers never panic and classify consistently | 3 | 0 |
| `dropumi-protocol` | `src/lib.rs` (unit) | Strict CBOR codec, frames, messages, KATs (RFC 7748 X25519, fingerprint, commitment, SAS), record bounds and nonce exhaustion, receiver machine rejections, ignorable-message budget, sender FileAck rules, preamble | 42 | 0 |
| `dropumi-protocol` | `tests/codec_props.rs` | proptest: arbitrary and mutated input never panics; messages round-trip | 3 | 0 |
| `dropumi-protocol` | `tests/handshake.rs` | Noise XX happy path, tampering, replay, prologue mismatch, responder known before msg3, MitM gives independent SAS | 11 | 0 |
| `dropumi-protocol` | `tests/record.rs` | Bit flips, reordering, replay, truncation, poisoning, independent directions | 7 | 0 |
| `dropumi-protocol` | `tests/session_props.rs` | proptest: random input to the receiver machine never panics; valid sender traces are accepted | 2 | 0 |
| `dropumi-application` | `src/lib.rs` (unit) | I/O error mapping, admission limits, path-free store-error log text | 3 | 0 |
| `dropumi-application` | `tests/channel.rs` | Handshake over in-memory streams: pinning before msg3, busy and incompatible replies, oversized pre-auth input | 8 | 0 |
| `dropumi-application` | `tests/failures.rs` | Cancellation on both sides (synchronized on bytes written, not on sleeps), consent timeout, absolute offer deadline, idle timeout, write stall, storage errors not leaked, `discard` on every failure path, candidate order, CompleteAck deadline restarted by each FileAck and still firing without one, trust-store errors (lookup and last-seen update) logged without paths | 13 | 0 |
| `dropumi-application` | `tests/pairing.rs` | Both confirm, one denies, window closes, cancel while waiting for the peer's result | 4 | 0 |
| `dropumi-application` | `tests/policy.rs` | Trusted-only refuses unknown senders, pair intent refused by `receive`, receiver declines, Everyone expiry, unverified offers always prompt, unverified senders must confirm the SAS | 6 | 0 |
| `dropumi-application` | `tests/receiver_control.rs` | Cancel one incoming transfer by id; a reused active transfer id rejected BUSY (also concurrently); accept errors back off and stop after 8; a policy change applies to new sessions only; `stop_accepting` lets running transfers finish and refuses new connections | 14 | 0 |
| `dropumi-application` | `tests/transfer.rs` | Round trip, collisions, progress, source changed, write failure | 5 | 0 |
| `dropumi-infra` | `src/lib.rs` (unit) | Finalization without overwrite, symlinks, 0600/0700 modes, permission checks, stores and locking (including an atomic trust rename that never re-creates a removed device), stale cleanup, collision probing bounded by a per-name hint and a probe cap, TCP fallback, TXT parsing and random instance labels, a file source that refuses configuration files through symlinks and hard links | 41 | 0 |
| `dropumi-infra` | `tests/txt_props.rs` | proptest: untrusted mDNS TXT records never panic the parser; kept values are bounded and well-formed | 1 | 0 |
| `dropumi-cli` | `src/main.rs` (unit) | Argument matrix, exit-code table, control parsing and decisions bound to the waiting prompt, terminal prompts that ignore earlier lines, `terminal_safe`, NDJSON escaping, peer display; proptest: `terminal_safe` output is inert for any input | 13 | 0 |
| `dropumi-cli` | `tests/e2e/main.rs` | Two real `dropumi` processes on loopback, and the app facade against a real `dropumi` process | 42 | 1 |
| `dropumi-ffi` | `src/lib.rs` (unit) | Event mailbox (coalescing, ordering, caps, resync, batching), prompts (registered before published, 1,000-iteration race, late and wrong-kind answers), discovery table (TXT rules, IPv6/loopback, cap, self-filter, "looks like" hints), error codes | 26 | 0 |
| `dropumi-ffi` | `tests/node.rs` | Two real nodes on loopback: trusted send, Everyone with codes, decline, pairing both ways, the pairing window pausing and resuming the receiver, cancel by either side and by id, `stop_accepting`, app-read documents, size mismatch and panicking readers, configuration files refused, no paths in events, busy identity changes, answers bound to prompt ids, shutdown and drop, listener failures, a stuck listener, re-entrant listeners | 23 | 0 |
| `dropumi-ffi` | `tests/discovery.rs` | Advertising and withdrawal, sending to a listed device with a pinned key, copied hints that can neither redirect nor block a send, unpaired devices need a code, own advertisement filtered | 7 | 0 |
| `dropumi-ffi` | `tests/stress.rs` | 8 threads × 10,000 mixed calls while prompted transfers run | 1 | 0 |
| `dropumi-ffi` | `tests/measure.rs` | Opt-in measurements (cancel latency, event volume) | 0 | 2 |
| **Total** | | | **309** | **3** |

Per crate: domain 37, protocol 65, application 53, infra 42, cli 55, ffi 57 (plus the ignored mDNS test and the two
ignored measurements).

End-to-end tests by module (`crates/cli/tests/e2e/`):

| Module | Tests | What they prove |
|---|---|---|
| `transfer` | 6 | Single file (SHA-256 equal); Unicode names, an empty file and duplicate names; 64 MiB streaming; 300 small files; existing files never overwritten; Everyone mode with matching codes on both sides |
| `policy` | 9 | Trusted-only refuses an unknown sender; receiver declines; `--accept never`; consent timeout; sender refuses an unverified receiver; `--expect-fingerprint` mismatch; sending by IP reports the actual authenticated peer and its local alias; a late `accept` never approves a later offer and a decision for another `transfer_id` is ignored; Ctrl-C (SIGINT) exits 130 (Unix) |
| `pairing` | 4 | Both confirm: mutual trust, then a trusted transfer; one denies: nothing stored; cancel during the code comparison: exit 4, nothing stored; a name target with `--no-mdns` explains why it cannot be found (exit 2) |
| `failure` | 7 | Cancel from either side; kill either side (`connection_lost`, partial files removed); connection refused (exit 5); a dead candidate is skipped; stale `.part` files cleaned at start |
| `security` | 11 | Wire capture has no plaintext (file content, file name, both device names, both static public keys); incompatible version; preamble tamper; recorded-session replay; ciphertext bit flip; malformed traffic matrix; wrong file hash; MitM relay refused by pinned trust; traversal and reserved names forged on the wire; an ignorable message during the consent prompt is skipped; a peer Error before the offer is reported with its own code |
| `discovery` | 0 (+1 ignored) | mDNS discovery and send by fingerprint prefix (opt-in) |
| `ffi_interop` | 5 | The app facade (`dropumi-ffi`, in the test process) against a real `dropumi` process: a trusted send from the app; a CLI send accepted at the app's prompt; pairing with matching codes on both sides; an app cancel reported by the CLI; an unverified CLI sender whose code the app shows (Unix only) |

### Platform-conditional tests

The counts above are from macOS. Some tests compile only on certain targets:

| Test | Compiled on | Status |
|---|---|---|
| `fs::receive_dir::tests::{received_file_mode_0600, symlink_at_final_name_not_followed, dangling_symlink_not_replaced}`, `fs::finalize::tests::{placeholder_fallback_never_overwrites, placeholder_removed_when_second_persist_fails}`, `store::identity_file::tests::{identity_created_0600_in_0700, identity_rejects_group_readable}`, `store::trust_file::tests::trust_store_rejects_group_writable`, `perms::tests::config_dir_rejects_world_writable` (9 tests) | Unix | Ran on macOS 27 (included in the 309) |
| `policy::ctrl_c_exits_130` (e2e; sends SIGINT with `kill`) | Unix | Ran on macOS 27 (included in the 309) |
| `crates/ffi/tests/{node,discovery,stress,measure}.rs`, `fs::source::tests::*` (infra), and the `ffi_interop` e2e module | Unix (the facade ships for Apple platforms and Android) | Ran on macOS 27 (included in the 309) |
| `fs::finalize::tests::{transient_access_denied_retried, windows_persistent_denied_fails_without_placeholder}` | Windows | Compiled and linked for `x86_64-pc-windows-gnu`; not run (pending CI) |
| `transfer::bounded_memory_independent_of_size` (e2e; receiver peak `VmHWM` must grow by less than 16 MiB between a 16 MiB and a 256 MiB transfer) | Linux | Passed in the Debian 12 aarch64 container run; pending CI on `ubuntu-latest` |

## Harness design

### Application tests: in-memory doubles

`crates/application/tests/support/mod.rs` provides one double per port. The network is a map of
`tokio::io::duplex` pipes, so the real handshake, record layer and session machines run unchanged; only I/O is
replaced.

| Double | Replaces | Notes |
|---|---|---|
| `Net`, `FakeConnector`, `FakeListener` | `Connector`, `Listener` | Unknown address = connection refused; pipe size configurable (small pipes force write stalls) |
| `StaticDiscovery`, `NoAd` | `Discovery`, `AdHandle` | Fixed list of discovered peers |
| `MemSource`, `StallingReader` | `FileSource` | In-memory files; a reader that stops after N bytes |
| `MemSink`, `MemFile` | `FileSink`, `IncomingFile` | Fault injection (fail after N bytes with a chosen message, block writes); counts begin, commit, discard and directory syncs |
| `MemTrust`, `MemIdentity` | `TrustStore`, `IdentityStore` | |
| `Scripted` | `Interaction` | Fixed decisions, or never answers (consent and SAS timeouts) |
| `ManualClock` | `Clock` | Advanced by hand (Everyone expiry) |

Timeout tests use tokio's paused clock (`#[tokio::test(start_paused = true)]`), so they take no real time. Tests that
act in the middle of a transfer wait on `MemSink::wait_written` (bytes accepted by the sink) instead of sleeping.

### End-to-end tests: two real processes

`crates/cli/tests/e2e/` starts the real `dropumi` binary twice. Required tests use `--bind 127.0.0.1 --port 0
--no-mdns` and manual `--to 127.0.0.1:<port>` targets, so CI never depends on multicast.

| Module (`support/`) | Role |
|---|---|
| `proc.rs` | Spawns `dropumi --config-dir <temp> --json --control-stdin ...` with `DROPUMI_LOG=warn`; collects NDJSON events and stderr on reader threads; `wait_event`, `wait_where`, `cmd` and `cmd_for` (control commands, optionally bound to a `transfer_id`), `wait_exit`, `kill`, `interrupt` (SIGINT, Unix only); `Device` (temporary config dir with a named identity, trust helpers). |
| `files.rs` | Deterministic pseudo-random files; streaming SHA-256 (independent of the BLAKE3 used by the protocol); directory listings by name; `.part` detection. |
| `raw_peer.rs` | Blocking `std::net` peers built directly on `dropumi-protocol`: raw preambles, authenticated raw sessions that send forged names, wrong hashes, garbage records and malformed CBOR; a NO_COMMON_VERSION responder; an impostor responder that records whether the sender ever sent msg3. |
| `proxy.rs` | Transparent TCP proxy that captures both directions and can rewrite a prefix or flip one bit (tampering, replay, plaintext checks). |

Rules:
- **Synchronization is on NDJSON events and process exits, never on fixed delays.** Event waits block on the event
  channel up to a deadline (`wait_event`, `wait_where`, `wait_count`); after an exit, the harness reads stdout until the
  process closes it. Process exits and asynchronous `.part` cleanup are polled every 20 ms up to a deadline.
- **Portability:** compare file names (`saved_as`, directory listings), never full paths. Canonicalization differs
  between OSes (for example `/var` and `/private/var` on macOS).
- **Hostile input comes from the wire.** Reserved names, traversal attempts and bidirectional characters are forged by
  `raw_peer`; hostile files are never created on disk.
- **No retries** hide flaky tests. Deadlines: 30 s per awaited event, 60 s per process exit (longer for
  `large_file_streaming`, scaled by its size).

## Opt-in variables

| Variable | Effect | Default |
|---|---|---|
| `DROPUMI_TEST_MDNS=1` | Enables `discovery::mdns_discover_and_send_by_fingerprint` (also needs `--ignored`). It advertises and browses on loopback interfaces (`--mdns-loopback`), finds the receiver by its fingerprint hint and sends to it. | Off (the test is `#[ignore]`) |
| `DROPUMI_TEST_LARGE_MIB=<N>` | File size for `transfer::large_file_streaming`. | 64 |
| `DROPUMI_TEST_APPLE=1` | Enables the `interop::apple::apple_*` tests (also need `--ignored`; run one at a time). They drive the app-hosted XCTest `AppleInteropTests` with `xcodebuild test-without-building`; `scripts/apple/interop.sh` builds the bundles first. With `DROPUMI_TEST_MDNS=1` as well, the `apple_macos_bonjour_*` tests run too; they ask the macOS Local Network question for Dropumi the first time. | Off (the tests are `#[ignore]`) |
| `DROPUMI_TEST_APPLE_SIMULATOR=<name>` | Simulator for the iOS interop tests. | `iPhone 17` |
| `--ignored` on `dropumi-ffi --test measure` | Runs the ADR-0009 measurements (cancel latency, events of a 2,000-file transfer) and prints the numbers. Run with `--release --nocapture`. | Off (the tests are `#[ignore]`) |

## Time budgets

| Budget ([`AGENTS.md`](../AGENTS.md) §8) | Limit | Latest recorded |
|---|---|---|
| Unit and application tests together | < 30 s | Every non-e2e target reported at most 1.57 s (sum 4.60 s, including the FFI's loopback tests; compile time excluded; 2026-10-14 run) |
| End-to-end suite per OS | < 4 min | 32.03 s on macOS 27 arm64 with the interop tests (1 run); milestone 1: 31.61 to 31.73 s (3 runs), 30.52 s in the Debian 12 aarch64 container (1 run); other OSes not measured (pending CI) |
| Any single test | < 60 s | Not measured individually for the default suite; the e2e harness enforces a 60 s deadline per process. The slowest default test, `security::malformed_traffic_matrix`, took about 30 s alone. An opt-in `DROPUMI_TEST_LARGE_MIB=4096` run of `transfer::large_file_streaming` (debug build) passed in 40.07 s; an earlier 4 GiB run on the same machine took 61.12 s, above this budget, so the 4 GiB size can exceed it. The default size is 64 MiB |

## Status labels

Test status: `Automated test passed`, `Manual device test passed`, `Build verified only`, `Not tested`,
`Unsupported`. Capability research ([`feasibility.md`](feasibility.md)): `Documented capability - not tested`,
`Requires investigation`, `Unsupported`, `Not applicable`. A parenthesis adds the scope (for example "CLI, loopback,
local run"). No platform is ever labelled "supported".

## Platform verification matrix

| Platform or environment | What ran | Status |
|---|---|---|
| macOS 27 arm64 (Apple M4 Pro), Rust 1.99.0, maintainer machine | Full workspace test suite, e2e 3 times | Automated test passed (CLI, loopback, local run) |
| macOS 27 arm64, same machine | Opt-in mDNS e2e on loopback | Automated test passed (opt-in, loopback, local run on macOS 27) |
| Linux aarch64, Debian 12 (bookworm) container, Docker on the maintainer machine, Rust 1.99.0, non-root | Full workspace test suite (225 passed, including the Linux-only bounded-memory test) | Automated test passed (Linux aarch64 container, loopback, local run) |
| macOS 27 arm64, same machine | Scripted terminal-mode smoke test (pseudo-terminals; the script is not in the repository) | Automated test passed (scripted terminal-mode run, loopback, local run) |
| Linux aarch64, Debian 12 container (root), same machine | The same scripted terminal-mode smoke test and the stray-input check | Automated test passed (scripted terminal-mode run, loopback, local run) |
| Ubuntu 24.04 x64 (GitHub runner `ubuntu-latest`) | `test (ubuntu-latest)` | Not tested (pending CI) |
| macOS 26 arm64 (GitHub runner `macos-latest`) | `test (macos-latest)` | Not tested (pending CI) |
| Windows Server 2025 x64 (GitHub runner `windows-latest`) | `test (windows-latest)` | Not tested (pending CI) |
| Windows x86_64 (`x86_64-pc-windows-gnu`, cross-compiled with MinGW in the Linux container) | `cargo clippy --all-targets -D warnings` (both feature sets) and `cargo build --all-targets --all-features` (compiled and linked); nothing run | Build verified only (cross-compiled and linked; not run) |
| MSRV 1.89 (GitHub runner `ubuntu-latest`) | `msrv`: `cargo check` (the same check passed locally with `cargo +1.89.0` on the maintainer machine) | Not tested (pending CI) |
| Android aarch64 (`aarch64-linux-android`) | Core library crates built locally with NDK 27.1 clang; CI job `build (aarch64-linux-android)` pending | Build verified only (core library crates; no app, not run) |
| iOS aarch64 (`aarch64-apple-ios`) | Core library crates built locally with the Xcode 27 SDK; CI job `build (aarch64-apple-ios)` pending | Build verified only (core library crates; no app, not run) |
| mDNS on a GitHub runner (`mdns-e2e (experimental)`, `ubuntu-latest`, `continue-on-error`) | Opt-in mDNS e2e | Not tested (pending CI, experimental job) |
| macOS 27 arm64, same machine | `dropumi-ffi` unit and loopback tests (57), the facade-against-CLI e2e tests (5), the stress test | Automated test passed (FFI, loopback, local run) |
| macOS 27 arm64, same machine, Xcode 27.0, Swift 6.4 | Swift binding tests against the macOS slice of the XCFramework (`swift test`, 6 tests) | Automated test passed (Swift bindings, loopback, local run) |
| macOS 27 arm64, same machine, Temurin 21, AGP 9.4.1 | Kotlin binding tests on the host JVM (6 tests) | Automated test passed (Kotlin bindings on the host JVM, loopback, local run) |
| iOS device and iOS simulator (`aarch64-apple-ios`, `aarch64-apple-ios-sim`) | `DropumiFFI.xcframework` slices built with `scripts/ffi/build-apple.sh` (dev and release) | Build verified only (FFI library; no app, not run) |
| Android arm64-v8a | `libdropumi_ffi.so` built with `scripts/ffi/build-android.sh` (NDK 27.1, API 26); `scripts/ffi/check-16k.sh` passed (all 4 LOAD segments aligned to 0x4000) | Build verified only (FFI library; no app, not run) |
| Android x86_64 | Not built locally; CI job `ffi (android)` | Not tested (pending CI) |
| `ffi (apple)`, `ffi (android)` (GitHub runners `macos-latest`, `ubuntu-latest`) | The CI jobs above | Not tested (pending CI) |
| Real multi-device LAN or Wi-Fi | Nothing | Not tested |
| iOS 27.0 simulator (iPhone 17), Xcode 27.0, same machine | Apple app: hosted tests (3), UI state tests with accessibility audit (8), interop with the CLI (3) | Automated test passed (iOS simulator, loopback, local run); UI state tests use the fake core |
| iOS 27.0 simulator (iPad Air 11-inch (M4)) | Apple app launched on the fake core; the wide two-column Nearby layout checked by screenshot | Manual simulator check only (no automated iPad tests) |
| macOS 27.0.1 arm64, same machine | Apple Mac app: hosted tests (5), interop with the CLI (3) | Automated test passed (macOS app, loopback, local run) |
| macOS 27.0.1 arm64, same machine | Apple Mac app: UI state tests | Not tested (UI automation approval needed) |
| Apple app Bonjour (sandboxed Mac app; iPhone and iPad devices) | Opt-in `apple_macos_bonjour_*`; device matrix DISC-1 to DISC-4 | Not tested. On the iOS 27.0 simulator the app's DNS-SD requests are never answered ([`discovery.md`](protocol/discovery.md) §12) |
| iPhone 15, iPad Air (5th generation), the Mac on a LAN | Device matrix | Not tested |
| Android app | Milestone 2c | Not tested (no app yet) |
| GitHub runner `macos-latest`, job `apple (app)` | Package tests, iOS simulator app tests, unsigned macOS build | Not tested (pending CI; needs Xcode 27, gate G5) |
| Browser companion | Proposal and research prototype only ([ADR-0011](adr/0011-browser-companion.md)) | Not tested |

Other local checks on the same machine: `cargo +1.89.0 check --workspace --all-targets --all-features --locked`
(MSRV) finished; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked` finished with
0 warnings; `cargo deny --locked check` passed (advisories, bans, licenses, sources; only duplicate-version warnings
for `cpufeatures`, `getrandom` and `syn`); `zizmor --offline` and actionlint 1.7.12 (the CI image, with shellcheck 0.11.0) reported no findings;
`scripts/ci/check-architecture.sh` passed; `cargo fmt --all --check` and `cargo clippy --workspace --all-targets
--locked -- -D warnings` (with `--all-features` and with `--no-default-features`) were clean; `cargo clippy -D warnings`
for `dropumi-infra` on `aarch64-linux-android` was clean; in the Linux container, `cargo clippy` for Linux and for
`x86_64-pc-windows-gnu` (both feature sets) was clean and the Windows build of all targets linked; actionlint ran from the CI job's pinned Docker image (with
shellcheck). The macOS checks ran on the final code; the container checks ran on code that differs from it only in one
comment line of `crates/cli/Cargo.toml`.

**Runner images.** `ubuntu-latest`, `macos-latest` and `windows-latest` are moving labels; the versions above are the
images they are expected to map to. After CI runs, record the image name and version from each job log here, and only
for green jobs.

**mDNS on hosted runners is UNVERIFIED.** [actions/runner-images#10924](https://github.com/actions/runner-images/issues/10924)
is an open request to grant the Local Network permission on macOS 15 runners. A comment there from 2026-09-30 reports
an IPv4 mDNS send failure on a `macos-26` arm64 runner, with no Local Network privacy cause diagnosed. For this reason
the experimental job runs on `ubuntu-latest` only and is never a required check. Whether macOS Local Network privacy
applies to loopback traffic is also UNVERIFIED.

## Gaps and explicit deferrals

| Item | Status and reason |
|---|---|
| OS permission-prompt abstractions (spec §14) | Deferred. The CLI triggers no OS permission prompts; Local Network, Bluetooth and nearby-device permission ports arrive with the platform adapters. The key-file and config-directory permission tests stand in for now. |
| Real multi-device LAN or Wi-Fi | Not tested. No device lab in this milestone; all automated tests use loopback on one machine. |
| cargo-fuzz targets and golden handshake vectors | Deferred. Parsers, sanitizers and session machines have proptest coverage (`codec_props`, `session_props`, `filename_props`, `name_props`, `identity_props`, `txt_props`, and `output::tests::terminal_safe_output_is_inert` in the CLI) and the KATs pin fingerprint, commitment, SAS and X25519; there is no cargo-fuzz target and no recorded full-handshake transcript. |
| Bounded-memory test | Linux only (reads `/proc/<pid>/status`). Passed in a Debian 12 aarch64 container; pending CI on `ubuntu-latest`. It checks the receiver's peak only. The macOS evidence is the measured peak RSS from the loopback benchmark ([`ARCHITECTURE.md`](../ARCHITECTURE.md#7-memory)). |
| Windows finalization retries | The two Windows-only finalize tests above compile for `x86_64-pc-windows-gnu` but have not run (pending CI). Real antivirus or indexer locks are Not tested. |
| Windows temp-file deletion retry (`remove_with_retry`, errors 5 and 32) | No test: it needs a real sharing violation on Windows. |
| Windows: input typed before a terminal prompt | Not flushed on Windows (flushing the console input buffer needs an unsafe system call), so a line typed before a prompt can answer it there. On Unix the terminal is flushed before every prompt. Not covered by a test in the repository; the Unix behaviour was checked by the scripted runs on macOS 27 arm64 and in the Debian 12 aarch64 container. |
| Pairing with ignorable messages while waiting for the peer's PairResult | No test: the e2e raw peer cannot pair yet. |
| A second Ctrl-C exits at once | No test in the repository; checked by the scripted terminal-mode runs (`e2e policy::ctrl_c_exits_130` covers one Ctrl-C). |
| The sender's CompleteAck deadline is not restarted by ignorable messages | No test: the in-memory receiver cannot send ignorable messages. The code restarts the deadline only when the number of acknowledged files grows, and ignorable messages are capped at 16 in a row. |
| exFAT / vfat placeholder fallback | Tested only with injected errors, not on a real exFAT or vfat volume. |
| MitM relay showing different SAS codes end to end (planned tests `channel::mitm_relay_sas_differs`, `security::mitm_relay_sas_codes_differ`) | Not implemented (planned). Covered at the protocol level by `mitm_split_handshakes_sas_independent`, and the pinned-trust case by `security::mitm_relay_rejected_by_pinned_trust`. |
| Pre-auth slots recover after silent connections (planned test `security::silent_preauth_slots_recover`) | Not implemented (planned). Admission limits are covered by `usecases::admission::tests::admission_limits_global_per_ip_and_session`; the 10 s handshake deadline releases silent connections. |
| Performance regressions in CI | Not automated. Benchmarks run by hand with `scripts/bench/loopback.sh`. |
| FFI on a device | Apple: the core ran inside the app on the iOS 27.0 simulator and on macOS (interop tests); no iPhone or iPad yet. Android: host JVM only; instrumented tests come with the app. |
| Apple app on devices | Not tested: install on iPhone 15 and iPad Air (5th generation) and the device matrix are next. Share Extension, Photos import, background completion (`BGContinuedProcessingTask`), the Mac menu-bar receiver, notifications, open at login, the receive-folder bookmark and quarantine flags are all Not tested. |
| Per-event callback cost across JNA and Swift (ADR-0009 S6) | Not isolated: the opt-in measurement counts the events of a 2,000-file transfer, whose time is dominated by per-file fsync. |
| Host name published by native advertising (Bonjour, NSD) | Apple: the adapter registers a proxy with a random host; checked with `dns-sd` and from an unsandboxed process on the Mac, not yet from the app on a device (`apple_macos_bonjour_cli_finds_app_at_random_host`, DISC-4). Android: Not tested ([`discovery.md`](protocol/discovery.md) §7, §12). |
