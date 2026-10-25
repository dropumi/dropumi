# Testing

How Dropumi is tested, how to run the tests, and exactly what has been verified where. Commands that mirror CI are in
[`development.md`](development.md).

## Current status

| Item | Result |
|---|---|
| Latest recorded run | macOS 27 arm64 (Apple M4 Pro), Rust 1.99.0, maintainer machine, 2026-10-14, on the milestone 2a code (`dropumi-ffi` and the core changes it needed) |
| `cargo test --workspace --all-features --locked` | **309 passed, 0 failed, 3 ignored** (the opt-in mDNS test and the two opt-in FFI measurements). After the pairing fix found by the first Windows CI run (2026-10-15, macOS 27.0.1 arm64, Rust 1.93.0): **319 passed, 0 failed, 3 ignored**. After merging the core fixes (manual trust merges inside the store lock, Ctrl-C during discovery and name resolution, configuration directory syncs and `StoreError::NotDurable`, same-second source edits; same day, machine and toolchain): **334 passed, 0 failed, 3 ignored**; after the facade treated such a change as applied, with a warning: **338 passed, 0 failed, 3 ignored**; after the manual-trust and rename race test: **339 passed, 0 failed, 3 ignored**; after merging the core follow-ups (an identity reset checks `device.json` before it writes the key, a rejected trust merge writes nothing, the CLI warns before it re-reads and reports a saved change it cannot read back as saved, the chmod regression test for source files and the `config` e2e test; same day, machine and toolchain): **349 passed, 0 failed, 3 ignored**; after the facade published its warning before the re-read and reported a saved change that cannot be read back as saved: **350 passed, 0 failed, 3 ignored**; on the Apple app branch the same suite also lists the 8 opt-in `apple_*` interop tests as ignored (run by `scripts/apple/interop.sh`): **350 passed, 0 failed, 11 ignored** (2026-10-15, after merging the review fixes); with the Android config lock (2026-10-15): **352 passed, 0 failed, 11 ignored** (the 8 opt-in `apple_*` tests among the ignored) on this branch; with the 5 opt-in `apple_ios_device_*` tests (2026-10-18, macOS 27.0.1 arm64, Rust 1.99.1): **352 passed, 0 failed, 16 ignored** (the 13 opt-in `apple_*` tests among the ignored); on the Android app branch, with the 7 opt-in `android_*` interop tests (2026-10-16, macOS 27.0.1 arm64, Rust 1.93.0): **352 passed, 0 failed, 18 ignored** (e2e: 44 passed, 16 ignored, 34.24 s) |
| Two-process e2e suite | 42 passed, 1 ignored (32.03 s), including the 5 facade-against-CLI interop tests; after the pairing fix 43 passed, 1 ignored (32.06 s); after merging the core fixes 43 passed, 1 ignored (32.07 s); after merging the core follow-ups, with the `config` module, 44 passed, 1 ignored (32.04 s), and again 44 passed, 1 ignored (32.12 s) in the 350-test run; 44 passed, 14 ignored (32.32 s) on 2026-10-18, the 13 opt-in `apple_*` tests among the ignored. Milestone 1's 37 tests were green 3 times in a row (31.73 s, 31.64 s and 31.61 s) |
| Swift binding tests (`swift test --package-path apps/apple/DropumiKit`) | **6 passed** (Swift 6.4, Xcode 27.0, macOS slice of the XCFramework). Again 6 passed on 2026-10-15 (macOS 27.0.1 arm64, same Swift and Xcode, `build-apple.sh --profile dev --slices ci`) after the bindings were regenerated for the facade's documentation of changes saved without a confirmed directory sync. Again 6 passed later that day (same machine, Swift and Xcode, XCFramework rebuilt with `build-apple.sh --profile dev --slices ci`) after the bindings were regenerated for the documentation of a saved change that cannot be read back |
| Kotlin binding tests (`./apps/android/gradlew -p apps/android :core-bindings:testDebugUnitTest`) | **6 passed, 0 failed** (AGP 9.4.1, Gradle 9.8.1, Temurin 21, host JVM on macOS 27 arm64). Again 6 passed, 0 failed on 2026-10-15 (Temurin 21.0.10, host JVM on macOS 27.0.1 arm64) with the regenerated bindings. Again 6 passed, 0 failed later that day with the bindings regenerated for the documentation of a saved change that cannot be read back (Gradle 9.8.1, `jvmToolchain(17)`, Gradle daemon on Homebrew OpenJDK 17.0.20.1, host JVM on macOS 27.0.1 arm64). Again 6 passed, 0 failed on 2026-10-16 on the Android app branch (same Gradle and JDK, test tasks forced to run) |
| Android app JVM tests (`:core`, `:appstate`, `:testsupport`, `:app` `testDebugUnitTest`, milestone 2c-1) | **219 passed, 0 failed, 0 skipped**: `:core` 48, `:appstate` 21, `:testsupport` 108 (`AppModel` on the fake core), `:app` 42 (2026-10-16, Gradle 9.8.1, AGP 9.4.1, Homebrew OpenJDK 17.0.20.1, host JVM on macOS 27.0.1 arm64; test tasks forced to run with `cleanTestDebugUnitTest`; counts from the JUnit XML reports). These never load the Rust library (`NativeLibraryGuardTest` in each module). After the one-screen pairing (2026-10-26, commit `5ae2825`, macOS 27.0.1 arm64), with `:core-bindings`: **all passed, 0 failed**: `:core-bindings` 6, `:core` 48, `:appstate` 44, `:testsupport` 158, `:app` 48 |
| Android app build and audits (`ffi (android)` steps, run locally) | Passed on 2026-10-16 (macOS 27.0.1 arm64): the release Rust core for `arm64-v8a` and `x86_64` and its stamp check, 16 KB alignment of `jniLibs` and of the release APK (and the check's failure on an empty folder), the debug, release and `qa` APKs and the debug test APK, the overlay helper, `:app:lintRelease`, `check-manifest.sh release qa debug`, `check-logging.sh` with the release and `qa` R8 configurations, the APK native-library list, and the fake-core check (no `:testsupport` on the release, `qa` and debug runtime classpaths, none in the release and `qa` R8 mappings, none in the debug APK's 14 dex files; positive controls: the unit-test classpath, and 660 fake-core lines in the test APK). Again on 2026-10-26 (commit `5ae2825`): the app build, `:app:lintRelease` (0 errors, 12 dependency and version warnings), `check-manifest.sh release qa debug`, `check-logging.sh` and the `qa` interop build passed |
| Android app on emulators (2026-10-16, macOS 27 arm64 host) | Instrumented tests (`scripts/android/ui-tests.sh`) on the arm64-v8a `google_apis` emulators API 26 (Android 8.0) and API 28 (Android 9) and the arm64-v8a Play `Tablet` emulator API 36.1 (Android 16): passed, except the two hard-link `UriGuard` tests, not runnable on these images (`link()` returns `EACCES`). The app's core against the CLI (`scripts/android/interop.sh`) on API 36.1 and API 26: `android_cli_sends_to_app_after_code` passed (commit `2d8631a`, ADR-0013 S-AN1), and so did `android_app_sends_to_cli_after_code` and `android_unpaired_sender_refused` (threat model G39); the `android_nsd_*` tests print a skip line on emulators. Per-image test counts are not recorded in this file. Details per goal: threat model G39 to G45 |
| Android app on a real phone (2026-10-25 and 2026-10-26, runs [2026-10-25-01](device-tests/runs/2026-10-25-01.md) and [2026-10-26-01](device-tests/runs/2026-10-26-01.md); no emulator) | Samsung Galaxy A54 (SM-A546E), Android 16, arm64-v8a, on the Mac's Wi-Fi network, commit `5ae2825`. `scripts/android/ui-tests.sh`: pass 1 (fake core and real-core tests) `Tests run: 102,  Failures: 3`, the 3 not runnable on this phone (`DocumentGuardTest` `stagingRunsUriGuard` and `uriGuardRefusesHardLinkToIdentityKey`: link `EACCES`; `LockScreenTest promptNeverShowsOverTheLockScreen`: secure lock screen); the 102 include 14 new pairing-screen UI state tests (fake core). Pass 2 (interop Application): `OK (1 test)`. `scripts/android/interop.sh` with `DROPUMI_TEST_MDNS=1`: **6 of 7 passed**; `android_nsd_app_finds_cli_and_sends` failed (the app listed no device before the timeout; it also failed on 2026-10-18 and 2026-10-25; intermittent, cause not established) |
| Device to device (`scripts/devices/interop.sh`, opt-in) | 2026-10-25 (commit `a90a075`): `devices_iphone_mac_pair_and_send`, `devices_android_mac_pair_and_send` and `devices_iphone_android_pair_and_send` passed, `test result: ok. 1 passed; 0 failed` each. 2026-10-26 (commit `5ae2825`, the new Android pairing): `devices_android_mac_pair_and_send` and `devices_iphone_android_pair_and_send` passed. Each pairs the two apps (the same code on both, each trusts the other), then sends two files each way with no code, SHA-256 equal on the receiving device. iPhone 15 (iOS 27.0.1), Mac arm64 (macOS 27.0.1), Galaxy A54 (Android 16, arm64-v8a), one Wi-Fi network |
| Opt-in mDNS e2e (`DROPUMI_TEST_MDNS=1`) | Passed on loopback in a separate local run (8.37 s) |
| Scripted terminal-mode smoke test | Passed on macOS 27 arm64 and in the Debian 12 aarch64 container (as root). A script (not in the repository) ran two `dropumi` processes, each on its own pseudo-terminal, and typed the answers: pairing with matching codes; a `y` typed before the first prompt did not answer it (the `n` typed at the prompt rejected the offer); a trusted transfer with equal SHA-256; `receive --accept trusted` without a prompt; Ctrl-C (exit 130); and a second Ctrl-C exiting at once while a silent connection held the graceful shutdown. A second script showed stray input answering a prompt in 5 of 5 tries before the terminal flush (macOS only: one run on the code without the flush) and in 0 of 5 after it (macOS and the container), both for a line typed before the first prompt and for a partial line typed before a later prompt |
| Linux, full workspace suite (milestone 1 code) | **225 passed, 0 failed, 1 ignored** in a Debian 12 (bookworm) aarch64 container (Docker on the same machine, Rust 1.99.0, non-root user, `--no-fail-fast`), including the Linux-only `transfer::bounded_memory_independent_of_size` |
| Windows target | Milestone 1 code: for `x86_64-pc-windows-gnu`, with MinGW in the Linux container image, `cargo clippy --workspace --all-targets -D warnings` (both feature sets) clean and `cargo build --workspace --all-targets --all-features` compiled and linked. Milestone 2a: `cargo xwin check -p dropumi-ffi -p dropumi-cli --all-targets --target x86_64-pc-windows-msvc` passed (the FFI's integration tests are Unix only). Nothing was run |
| Apple app package tests (`swift test --package-path apps/apple/DropumiKit`, milestone 2b) | **48 passed** (`ShareInboxTests` 8, `BindingsTests` 6, `AppModelTests` 34; macOS 27.0.1 arm64, Xcode 27.0, 2026-10-15, after the This device fingerprint and background-launch fixes and the handling of changes saved without a confirmed directory sync: a core `Warning` shows a banner or is added to the one showing, and a rename, reset or manual pairing that the core saved but could not read back is reloaded and reported as done; on the old code 5 of these 7 tests fail). New: the full fingerprint is grouped in fours and gives the original back; a model sent to the background before `start()` does not start receiving until it is active again; a background change while the receiver is starting stops accepting once it is up |
| Apple app, hosted and UI state tests (`xcodebuild test -scheme Dropumi-iOS`, iPhone 17 simulator, iOS 27.0) | **5 hosted tests passed, 9 UI state tests passed** (2026-10-15; UI state tests use the fake core, and 4 of them run the accessibility audit); the interop suite is skipped outside its harness. New: the hosted `backgroundDuringLoadKeepsTheModelInTheBackground` (the app went to the background while the core was opening) and the UI state test (fake core) `testThisDeviceShowsTheFullFingerprint` |
| Apple app, hosted tests on macOS (`xcodebuild test -scheme Dropumi-macOS -only-testing:DropumiMacTests`) | **6 passed** (2026-10-15), including `MenuBarReceiverTests`: the app delegate opens the core with no window and feeds its offers to the notifications (also after the core reopens); offers, closed prompts and "Files received" are announced with no view, and offers are withdrawn when the model is replaced or the app quits; a failed start is not shown as "Starting…". The notifications are checked at the point where the app hands them to Notification Center (a test sink); what macOS displays is Not tested. The macOS UI state tests did not run: UI automation needs the user's approval on the machine |
| Apple app against the CLI (`scripts/apple/interop.sh`, opt-in) | **6 passed** on loopback (2026-10-15, again after the pairing fix with the rebuilt core): transfers both ways after comparing a code, and pairing, each on the iOS 27.0 simulator and on macOS 27. The two macOS Bonjour tests were not run (they ask the macOS Local Network question) |
| Apple app on real devices (2026-10-18, run [2026-10-18-01](device-tests/runs/2026-10-18-01.md); no simulator) | iPhone 15, iOS 27.0.1: **6 hosted tests and 15 UI state tests passed**, and the app's core against the CLI over Wi-Fi **5 passed** (`apple_ios_device_*`: transfers both ways after comparing a code, pairing, and Bonjour both ways). Mac arm64, macOS 27.0.1, Xcode 27.1: **7 hosted tests and 15 UI state tests passed** (windows on the built-in display), the 3 loopback interop tests passed, and **MAC-BONJOUR 2 passed**. Package tests: **52 passed**. Defects found and fixed: no notice when the core refused a device it could not name; the Mac's "…" menus offered VoiceOver no Open Menu action. The manual device matrix was not run |
| GitHub Actions CI | **Not run yet.** The maintainer pushes the branch; every runner row below stays "Not tested (pending CI)" until its job is green. |

Automated tests run both peers on one machine over loopback (`127.0.0.1`), except these opt-in tests:
- `apple_ios_device_*`: the app's core on an iPhone and the CLI on a Mac, over Wi-Fi;
- `android_*`: the app's core on an emulator or a phone and the CLI on the host, through `adb reverse` and
  `adb forward` on `127.0.0.1`; the `android_nsd_*` tests use the phone's Wi-Fi for discovery;
- `devices_*`: two apps on two devices (iPhone, Mac, Android phone), over Wi-Fi; only the Android phone's control
  channel goes through adb.

No manual test between two real devices has been recorded.

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

# Apple apps on a real iPhone or iPad (unlocked; signing as in apps/apple/README.md)
xcodebuild test -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-iOS \
  -destination 'platform=iOS,id=<UDID>' -derivedDataPath apps/apple/build -allowProvisioningUpdates
DROPUMI_TEST_MDNS=1 DROPUMI_TEST_APPLE_DEVICE=<UDID> bash scripts/apple/interop.sh apple_ios_device_   # over Wi-Fi
# Android app (see apps/android/README.md and development.md; JDK 17)
cargo build --locked -p dropumi-ffi --lib && ./apps/android/gradlew -p apps/android :core-bindings:testDebugUnitTest \
  :core:testDebugUnitTest :appstate:testDebugUnitTest :testsupport:testDebugUnitTest :app:testDebugUnitTest
bash scripts/android/ui-tests.sh                                      # instrumented tests, one emulator or device
bash scripts/android/interop.sh                                       # the app's core against the CLI (opt-in)

# Device to device: two real apps pair and send both ways over Wi-Fi (opt-in, macOS host; see development.md)
DROPUMI_TEST_APPLE_DEVICE=<UDID> bash scripts/devices/interop.sh      # all devices_* tests
bash scripts/devices/interop.sh devices_android_mac_pair_and_send     # one test (ANDROID_SERIAL picks the phone)
```

The Apple app tests are labelled by what they exercise:
- *hosted tests* run inside the app with temporary folders;
- *UI state tests (fake core)* show every screen on a scripted core and say nothing about the network;
- *interop tests* run the app's real core, through the Swift layer, against the CLI: on loopback, or over Wi-Fi
  with a real iPhone or iPad (`apple_ios_device_*`).

The Android app tests are labelled the same way:
- *JVM tests* (`android-jvm` in the threat model) run on the host JVM: the core client, the NSD adapter on a fake NSD,
  `UriGuard`'s policy, staging, export rules and `AppModel` on the fake core. Only `:core-bindings` loads the Rust
  library (the host build);
- *instrumented tests* (`android-emu`) run on an emulator or device through `scripts/android/ui-tests.sh`: UI state tests
  on the fake core, the overlay tests (with a real overlay window drawn by the test-only helper APK), and tests on the
  real core with temporary folders inside the app;
- *interop tests* run the app's real core, inside the minified `qa` build, against the CLI through `adb reverse` and
  `adb forward` on `127.0.0.1` (`scripts/android/interop.sh`).

*Device-to-device tests* (`devices_*`, `scripts/devices/interop.sh`) run two real apps against each other: each runs
its real core inside its interop test, in the scenario `agent` (`AppleInteropTests` on the iPhone or the Mac,
`AndroidInteropTest` in the Android `qa` build), steered by `crates/cli/tests/e2e/interop/devices.rs`. They say
nothing about the apps' UI.

An emulator result is not a device result, an x86_64 image never stands for arm64-v8a, and one API level never stands
for another.

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
| `dropumi-application` | `src/lib.rs` (unit) | I/O error mapping, admission limits (one session per fingerprint, whatever the number of session slots), path-free store-error log text, the merge rule of a manual trust add (a new entry; a paired entry keeps its key, method, dates and alias; an explicit alias wins) | 5 | 0 |
| `dropumi-application` | `tests/channel.rs` | Handshake over in-memory streams: pinning before msg3, busy and incompatible replies, oversized pre-auth input | 8 | 0 |
| `dropumi-application` | `tests/failures.rs` | Cancellation on both sides (synchronized on bytes written, not on sleeps), consent timeout, absolute offer deadline, idle timeout, write stall, storage errors not leaked, `discard` on every failure path, candidate order, CompleteAck deadline restarted by each FileAck and still firing without one, trust-store errors (lookup and last-seen update) logged without paths, cancellation while browsing (`discover` and `send` to a name both stop at once, `discover` without reporting peers) | 15 | 0 |
| `dropumi-application` | `tests/pairing.rs` | Both confirm, one denies, window closes, cancel while waiting for the peer's result; while the code prompt is open, the peer's "no", Cancel, Error, departure or second PairResult ends it at once, an early "yes" is kept, and an early "yes" followed by a local "no" or timeout stores nothing; a manual add of a paired device merges inside the store's write, without a separate lookup; a trust entry saved without a confirmed directory sync still completes the pairing, after a warning | 14 | 0 |
| `dropumi-application` | `tests/policy.rs` | Trusted-only refuses unknown senders, pair intent refused by `receive`, receiver declines, Everyone expiry, unverified offers always prompt, unverified senders must confirm the SAS | 6 | 0 |
| `dropumi-application` | `tests/receiver_control.rs` | Cancel one incoming transfer by id; a reused active transfer id rejected BUSY (also concurrently); accept errors back off and stop after 8; a policy change applies to new sessions only; `stop_accepting` lets running transfers finish and refuses new connections | 14 | 0 |
| `dropumi-application` | `tests/transfer.rs` | Round trip, collisions, progress, source changed, write failure | 5 | 0 |
| `dropumi-infra` | `src/lib.rs` (unit) | Finalization without overwrite, symlinks, 0600/0700 modes, permission checks, stores and locking (including an atomic trust rename that never re-creates a removed device; a manual trust add racing a last-seen update or a rename keeps that change; a failed directory fsync after a config write, key creation or identity reset is reported as saved but not durable, while a failure before the rename stays an I/O error and changes nothing; a first run goes on with an identity saved without a confirmed directory sync; an identity reset with a corrupt or group-writable `device.json` fails and keeps the old key; a rejected trust merge writes nothing and is never reported as saved), a same-size source edit within the same second (size and full-resolution modification time; the test skips itself where the file system stores whole seconds), a chmod of a source file is not a source change, stale cleanup, collision probing bounded by a per-name hint and a probe cap, TCP fallback, TXT parsing and random instance labels, a file source that refuses configuration files through symlinks and hard links; the Android config lock (`flock`) excludes a second holder and is the same lock as std `File::lock` | 57 | 0 |
| `dropumi-infra` | `tests/txt_props.rs` | proptest: untrusted mDNS TXT records never panic the parser; kept values are bounded and well-formed | 1 | 0 |
| `dropumi-cli` | `src/main.rs` (unit) | Argument matrix, exit-code table, control parsing and decisions bound to the waiting prompt, terminal prompts that ignore earlier lines, `terminal_safe`, NDJSON escaping, peer display, how a configuration write is reported (saved without a confirmed directory sync: the warning, then the re-read, exit 0; a saved change that cannot be read back: a failure that says it was saved, the warning still printed; any other store error: a failure); proptest: `terminal_safe` output is inert for any input | 18 | 0 |
| `dropumi-cli` | `tests/e2e/main.rs` | Two real `dropumi` processes on loopback, configuration commands against a real configuration directory, and the app facade against a real `dropumi` process | 44 | 1 |
| `dropumi-ffi` | `src/lib.rs` (unit) | Event mailbox (coalescing, ordering, caps, resync, batching), prompts (registered before published, 1,000-iteration race, late and wrong-kind answers), discovery table (TXT rules, IPv6/loopback, cap, self-filter, "looks like" hints), error codes, how a configuration write is settled (saved without a confirmed directory sync: a fixed, path-free warning, then the re-read; a failed re-read after it: a `Config` error that says the change was saved, the warning still published; any other store error: a plain `Config` error) | 30 | 0 |
| `dropumi-ffi` | `tests/node.rs` | Two real nodes on loopback: trusted send, Everyone with codes, decline, pairing both ways, a code prompt withdrawn when the other device says no, the pairing window pausing and resuming the receiver, cancel by either side and by id, `stop_accepting`, app-read documents, size mismatch and panicking readers, configuration files refused, no paths in events, busy identity changes, answers bound to prompt ids, shutdown and drop, listener failures, a stuck listener, re-entrant listeners, identity and trust changes saved without a confirmed directory sync (applied, with one warning each) | 25 | 0 |
| `dropumi-ffi` | `tests/discovery.rs` | Advertising and withdrawal, sending to a listed device with a pinned key, copied hints that can neither redirect nor block a send, unpaired devices need a code, own advertisement filtered | 7 | 0 |
| `dropumi-ffi` | `tests/stress.rs` | 8 threads × 10,000 mixed calls while prompted transfers run | 1 | 0 |
| `dropumi-ffi` | `tests/measure.rs` | Opt-in measurements (cancel latency, event volume) | 0 | 2 |
| **Total** | | | **352** | **3** |

Per crate: domain 37, protocol 65, application 67, infra 58, cli 62, ffi 63 (plus the ignored mDNS test and the two
ignored measurements).

End-to-end tests by module (`crates/cli/tests/e2e/`):

| Module | Tests | What they prove |
|---|---|---|
| `transfer` | 6 | Single file (SHA-256 equal); Unicode names, an empty file and duplicate names; 64 MiB streaming; 300 small files; existing files never overwritten; Everyone mode with matching codes on both sides |
| `policy` | 9 | Trusted-only refuses an unknown sender; receiver declines; `--accept never`; consent timeout; sender refuses an unverified receiver; `--expect-fingerprint` mismatch; sending by IP reports the actual authenticated peer and its local alias; a late `accept` never approves a later offer and a decision for another `transfer_id` is ignored; Ctrl-C (SIGINT) exits 130 (Unix) |
| `pairing` | 5 | Both confirm: mutual trust, then a trusted transfer; one denies: nothing stored; cancel during the code comparison: exit 4, and the other side's prompt ends with exit 6 without an answer, nothing stored; a name target with `--no-mdns` explains why it cannot be found (exit 2); ignorable messages around an early "yes" are skipped within the limit, and the peer's Cancel ends the prompt without a reply |
| `failure` | 7 | Cancel from either side; kill either side (`connection_lost`, partial files removed); connection refused (exit 5); a dead candidate is skipped; stale `.part` files cleaned at start |
| `security` | 11 | Wire capture has no plaintext (file content, file name, both device names, both static public keys); incompatible version; preamble tamper; recorded-session replay; ciphertext bit flip; malformed traffic matrix; wrong file hash; MitM relay refused by pinned trust; traversal and reserved names forged on the wire; an ignorable message during the consent prompt is skipped; a peer Error before the offer is reported with its own code |
| `discovery` | 0 (+1 ignored) | mDNS discovery and send by fingerprint prefix (opt-in) |
| `ffi_interop` | 5 | The app facade (`dropumi-ffi`, in the test process) against a real `dropumi` process: a trusted send from the app; a CLI send accepted at the app's prompt; pairing with matching codes on both sides; an app cancel reported by the CLI; an unverified CLI sender whose code the app shows (Unix only) |
| `interop` | 1 (+26 ignored) | The apps' real cores against a real `dropumi` process, opt-in: 16 `apple_*` tests (`apple.rs`, `scripts/apple/interop.sh`) and 7 `android_*` tests (`android.rs`, `scripts/android/interop.sh`); two apps against each other, opt-in: 3 `devices_*` tests (`devices.rs`, `scripts/devices/interop.sh`), see [Opt-in variables](#opt-in-variables). The one test that always runs is `devices.rs`'s `file_pattern_hash_is_the_agents` (the driver's file pattern and its SHA-256) |
| `config` | 1 (Unix) | One process per command: with the configuration directory at 0300 (no directory sync possible), `identity reset --yes`, `trust add` and `trust remove` each exit 0 with the not-durable `warning` before their result; reset prints the new fingerprint, remove reports `existed: null`, and the changes are in place afterwards (skipped where a 0300 directory can still be opened, for example as root) |

### Platform-conditional tests

The counts above are from macOS. Some tests compile only on certain targets:

| Test | Compiled on | Status |
|---|---|---|
| `fs::receive_dir::tests::{received_file_mode_0600, symlink_at_final_name_not_followed, dangling_symlink_not_replaced}`, `fs::finalize::tests::{placeholder_fallback_never_overwrites, placeholder_removed_when_second_persist_fails}`, `store::identity_file::tests::{identity_created_0600_in_0700, identity_rejects_group_readable}`, `store::trust_file::tests::trust_store_rejects_group_writable`, `perms::tests::config_dir_rejects_world_writable`, `store::atomic::tests::{dir_sync_failure_reported, failure_before_rename_is_io}`, `store::identity_file::tests::{key_dir_sync_failure_reported, first_run_without_dir_sync_loads_the_new_identity}` (13 tests; the last four skip themselves when run as root, which bypasses directory permissions) | Unix | Ran on macOS 27 (the first 9 included in the 309; the last four in the 334 of the 2026-10-15 run on macOS 27.0.1, not as root) |
| `store::identity_file::tests::{reset_with_insecure_device_file_keeps_the_key, reset_without_dir_sync_rotates_the_key}`, `store::trust_file::tests::rejected_merge_is_never_not_durable`, `fs::source::tests::metadata_only_change_is_not_a_source_change` (4 tests; `reset_without_dir_sync_rotates_the_key` and `rejected_merge_is_never_not_durable` skip themselves when run as root) | Unix | Ran on macOS 27.0.1 (included in the 349 of 2026-10-15, not as root) |
| `config::not_durable_changes_are_applied_with_a_warning` (e2e; skips itself where a 0300 directory can still be opened, for example as root) | Unix | Ran on macOS 27.0.1 (included in the 349, not as root; on the core branch, a separate run with the skip branch made to panic passed, so the skip was not taken) |
| `policy::ctrl_c_exits_130` (e2e; sends SIGINT with `kill`) | Unix | Ran on macOS 27 (included in the 309) |
| `crates/ffi/tests/{node,discovery,stress,measure}.rs`, `fs::source::tests::*` (infra) except `same_size_subsecond_edit_detected` and `metadata_only_change_is_not_a_source_change` (listed above), and the `ffi_interop` e2e module | Unix (the facade ships for Apple platforms and Android) | Ran on macOS 27 (included in the 309; `node change_saved_without_dir_sync_is_applied_with_warning`, which skips itself when run as root, in the 338 of the 2026-10-15 run on macOS 27.0.1, not as root) |
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
| `PendingDiscovery` | `Discovery` | A browse that never completes; notifies the test when it starts (cancellation tests) |
| `MemSource`, `StallingReader` | `FileSource` | In-memory files; a reader that stops after N bytes |
| `MemSink`, `MemFile` | `FileSink`, `IncomingFile` | Fault injection (fail after N bytes with a chosen message, block writes); counts begin, commit, discard and directory syncs |
| `MemTrust`, `MemIdentity` | `TrustStore`, `IdentityStore` | `fail_*` switches make a lookup or a last-seen update fail; `upsert_not_durable` makes `upsert` store the entry, then report `NotDurable`; `upsert_with` merges under the mutex and never consults `fail_get` |
| `Scripted` | `Interaction` | Fixed decisions, never answers (consent and SAS timeouts), or answers the code after a delay (paused time) |
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
| `DROPUMI_TEST_APPLE=1` | Enables the `interop::apple::apple_*` tests (also need `--ignored`; run one at a time). They drive the app-hosted XCTest `AppleInteropTests` with `xcodebuild test-without-building`; `scripts/apple/interop.sh` builds the bundles first, only for the destinations of the selected tests (a filter such as `apple_macos_` never builds for a simulator). With `DROPUMI_TEST_MDNS=1` as well, the `apple_macos_bonjour_*` tests run too; they ask the macOS Local Network question for Dropumi the first time. | Off (the tests are `#[ignore]`) |
| `DROPUMI_TEST_APPLE_SIMULATOR=<name>` | Simulator for the iOS interop tests. | `iPhone 17` |
| `DROPUMI_TEST_APPLE_DEVICE=<UDID>` | Enables the `apple_ios_device_*` tests on that iPhone or iPad (unlocked, on the same Wi-Fi as the Mac): the CLI listens on the Mac's Wi-Fi address and the app reaches it over the network. The first run asks for Local Network access on the device. Also names the iPhone for the `devices_iphone_*` tests. | Unset (the device tests do not run) |
| `DROPUMI_TEST_LAN_ADDR=<IPv4>` | The Mac's address for the `apple_ios_device_*` and `devices_iphone_*` tests (the iPhone dials it). | `en0`'s address (`scripts/apple/interop.sh`, `scripts/devices/interop.sh`) |
| `DROPUMI_TEST_DEVICES=1` | Enables the `interop::devices::devices_*` tests (also need `--ignored`; run one at a time; macOS host): two real apps pair and send both ways over Wi-Fi. `devices_iphone_mac_pair_and_send` needs `DROPUMI_TEST_APPLE_DEVICE` and `DROPUMI_TEST_LAN_ADDR`; `devices_android_mac_pair_and_send` needs `ANDROID_SERIAL` (or one connected phone); `devices_iphone_android_pair_and_send` needs all three. `scripts/devices/interop.sh` builds and installs the apps first and uninstalls the Android APKs on exit. | Off (the tests are `#[ignore]`) |
| `DROPUMI_TEST_ANDROID=1` | Enables the `interop::android::android_*` tests (also need `--ignored`; run one at a time). Each one starts a scenario of the instrumented `AndroidInteropTest` in the installed `qa` build with `adb shell am instrument` and reads its status lines as they arrive; `scripts/android/interop.sh` builds and installs the `qa` app and its test APK first, and uninstalls both on exit. `ANDROID_SERIAL` picks the device. With `DROPUMI_TEST_MDNS=1` as well, the two `android_nsd_*` tests run too; they need a real device on the host's Wi-Fi, and the script sets that variable only when the device is not an emulator. | Off (the tests are `#[ignore]`) |
| `--ignored` on `dropumi-ffi --test measure` | Runs the ADR-0009 measurements (cancel latency, events of a 2,000-file transfer) and prints the numbers. Run with `--release --nocapture`. | Off (the tests are `#[ignore]`) |

## Time budgets

| Budget ([`AGENTS.md`](../AGENTS.md) §8) | Limit | Latest recorded |
|---|---|---|
| Unit and application tests together | < 30 s | Every non-e2e target reported at most 1.57 s (sum 4.60 s, including the FFI's loopback tests; compile time excluded; 2026-10-14 run). After merging the core fixes: at most 3.60 s (`dropumi-infra` unit tests), sum 6.81 s (2026-10-15 run); in the 338-test run: at most 3.67 s (the same target), sum 6.95 s; in the 339-test run, with a third 40-round trust-store race test: at most 5.37 s (the same target), sum 8.51 s; in the 349-test run after merging the core follow-ups: at most 5.51 s (the same target), sum 8.70 s; in the 350-test run: at most 5.21 s (the same target), sum 8.23 s |
| End-to-end suite per OS | < 4 min | 32.03 s on macOS 27 arm64 with the interop tests (1 run), 32.07 s after merging the core fixes (1 run), 32.04 s with the `config` module after merging the core follow-ups (1 run), 32.12 s in the 350-test run (1 run); milestone 1: 31.61 to 31.73 s (3 runs), 30.52 s in the Debian 12 aarch64 container (1 run); other OSes not measured (pending CI) |
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
| Android aarch64 (`aarch64-linux-android`) | Core library crates built locally with NDK 27.1 clang; CI job `build (aarch64-linux-android)` pending. `cargo clippy -p dropumi-infra --all-targets --target aarch64-linux-android -D warnings` (both feature sets) clean. A scratch instrumented test (not committed) on the arm64-v8a emulators API 26, 28 and 36 (Android 8, 9 and 16, macOS host): the config lock first failed with "lock() not supported" (std has no `File::lock` on Android); with the `flock` path, identity creation, trust writes and two receives of the same name (`photo.jpg`, `photo (1).jpg`, bytes equal) passed, with no `SIGSYS` | Build verified only (core library crates); the scratch emulator run is a spike, not a committed test |
| iOS aarch64 (`aarch64-apple-ios`) | Core library crates built locally with the Xcode 27 SDK; CI job `build (aarch64-apple-ios)` pending | Build verified only (core library crates alone; the app's results are in the iPhone rows below) |
| mDNS on a GitHub runner (`mdns-e2e (experimental)`, `ubuntu-latest`, `continue-on-error`) | Opt-in mDNS e2e | Not tested (pending CI, experimental job) |
| macOS 27 arm64, same machine | `dropumi-ffi` unit and loopback tests (57), the facade-against-CLI e2e tests (5), the stress test | Automated test passed (FFI, loopback, local run) |
| macOS 27 arm64, same machine, Xcode 27.0, Swift 6.4 | Swift binding tests against the macOS slice of the XCFramework (`swift test`, 6 tests) | Automated test passed (Swift bindings, loopback, local run) |
| macOS 27 arm64, same machine, Temurin 21, AGP 9.4.1 | Kotlin binding tests on the host JVM (6 tests) | Automated test passed (Kotlin bindings on the host JVM, loopback, local run) |
| iOS device and iOS simulator (`aarch64-apple-ios`, `aarch64-apple-ios-sim`) | `DropumiFFI.xcframework` slices built with `scripts/ffi/build-apple.sh` (dev and release) | Build verified only (the FFI library alone; inside the app, see the iPhone and simulator rows below) |
| Android arm64-v8a | `libdropumi_ffi.so` built with `scripts/ffi/build-android.sh` (NDK 27.1, API 26); `scripts/ffi/check-16k.sh` passed (all 4 LOAD segments aligned to 0x4000) | Build verified only (the FFI library alone; inside the app, see the Android rows below) |
| Android x86_64 | Not built locally; CI job `ffi (android)` | Not tested (pending CI) |
| `ffi (apple)`, `ffi (android)` (GitHub runners `macos-latest`, `ubuntu-latest`) | The CI jobs above | Not tested (pending CI) |
| Real multi-device Wi-Fi: iPhone 15 (iOS 27.0.1) and a Mac (macOS 27.0.1 arm64) on one network | The app's core on the iPhone against the CLI on the Mac, `apple_ios_device_*` (5) | Automated test passed (iPhone 15 and Mac over Wi-Fi, local run, 2026-10-18); the manual device matrix is Not tested |
| iOS 27.0 simulator (iPhone 17), Xcode 27.0, same machine | Apple app: hosted tests (5), UI state tests (9, 4 with the accessibility audit), interop with the CLI (3) | Automated test passed (iOS simulator, loopback, local run); UI state tests use the fake core |
| iOS 27.0 simulator (iPad Air 11-inch (M4)) | Apple app launched on the fake core; the wide two-column Nearby layout checked by screenshot | Manual simulator check only (no automated iPad tests) |
| macOS 27.0.1 arm64, same machine | Apple Mac app: hosted tests (6), interop with the CLI (3) | Automated test passed (macOS app, loopback, local run) |
| macOS 27.0.1 arm64, same machine, Xcode 27.1 | Apple Mac app: hosted tests (7), UI state tests (15, with the accessibility audit; windows on the built-in display) | Automated test passed (macOS app, local run, 2026-10-18); UI state tests use the fake core |
| Apple app Bonjour (sandboxed Mac app; iPhone and iPad devices) | Opt-in `apple_macos_bonjour_*` (2) on macOS 27.0.1 arm64 and `apple_ios_device_bonjour_*` (2) on an iPhone 15 (iOS 27.0.1) over Wi-Fi, each against the CLI; device matrix DISC-1 to DISC-4 | Automated test passed (Mac app and iPhone 15 with the CLI, local run, 2026-10-18); the device matrix and iPad are Not tested. On the iOS 27.0 simulator the app's DNS-SD requests are never answered ([`discovery.md`](protocol/discovery.md) §12) |
| iPhone 15 (iPhone15,4), iOS 27.0.1, Xcode 27.1 | Apple app: hosted tests (6), UI state tests (15, with the accessibility audit) | Automated test passed (real iPhone, local run, 2026-10-18); UI state tests use the fake core |
| iPhone 15, iPad Air (5th generation), the Mac on a LAN | Device matrix | Not tested (run 2026-10-18-01 recorded automated tests only) |
| Android 8.0 (API 26) and 9 (API 28) `google_apis` emulators, Android 16 (API 36.1) Play `Tablet` emulator, all arm64-v8a, macOS 27 arm64 host | Android app (2c-1): instrumented tests (UI state tests on the fake core, overlay tests, real-core tests); interop with the CLI on API 36.1 and API 26 | Automated test passed (emulators, local run, 2026-10-16); the two hard-link `UriGuard` tests are not runnable on these images; UI state tests use the fake core |
| Samsung Galaxy A54 (SM-A546E), Android 16, arm64-v8a, macOS 27.0.1 arm64 host | Android app (2c-1): instrumented tests (102 in pass 1, 1 in pass 2), interop with the CLI over adb (7, with `DROPUMI_TEST_MDNS=1`) | Automated test passed (real phone, local run, 2026-10-26), except 3 instrumented tests not runnable on this phone and `android_nsd_app_finds_cli_and_sends`, which failed (intermittent, open) |
| Device to device over Wi-Fi: iPhone 15 (iOS 27.0.1), Mac arm64 (macOS 27.0.1), Galaxy A54 (Android 16, arm64-v8a) on one network | `devices_iphone_mac_pair_and_send`, `devices_android_mac_pair_and_send`, `devices_iphone_android_pair_and_send` | Automated test passed (real devices, local run, 2026-10-25; the two Android pairs again on 2026-10-26 with the new Android pairing) |
| Galaxy A54 and the Apple devices | Manual device matrix (`docs/device-tests/manual/android.md`) | Not tested |
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
| Real multi-device LAN or Wi-Fi | Opt-in automated tests only: `apple_ios_device_*` (an iPhone and the CLI), `android_nsd_*` (an Android phone and the CLI) and `devices_*` (two apps), each over one Wi-Fi network, on the dates above. The manual device matrix has not been run; other networks and client isolation are Not tested. |
| cargo-fuzz targets and golden handshake vectors | Deferred. Parsers, sanitizers and session machines have proptest coverage (`codec_props`, `session_props`, `filename_props`, `name_props`, `identity_props`, `txt_props`, and `output::tests::terminal_safe_output_is_inert` in the CLI) and the KATs pin fingerprint, commitment, SAS and X25519; there is no cargo-fuzz target and no recorded full-handshake transcript. |
| Bounded-memory test | Linux only (reads `/proc/<pid>/status`). Passed in a Debian 12 aarch64 container; pending CI on `ubuntu-latest`. It checks the receiver's peak only. The macOS evidence is the measured peak RSS from the loopback benchmark ([`ARCHITECTURE.md`](../ARCHITECTURE.md#7-memory)). |
| Windows finalization retries | The two Windows-only finalize tests above compile for `x86_64-pc-windows-gnu` but have not run (pending CI). Real antivirus or indexer locks are Not tested. |
| Windows temp-file deletion retry (`remove_with_retry`, errors 5 and 32) | No test: it needs a real sharing violation on Windows. |
| Windows: input typed before a terminal prompt | Not flushed on Windows (flushing the console input buffer needs an unsafe system call), so a line typed before a prompt can answer it there. On Unix the terminal is flushed before every prompt. Not covered by a test in the repository; the Unix behaviour was checked by the scripted runs on macOS 27 arm64 and in the Debian 12 aarch64 container. |
| Pairing with ignorable messages while waiting for the peer's PairResult | No test: the e2e raw peer cannot pair yet. |
| A second Ctrl-C exits at once | No test in the repository; checked by the scripted terminal-mode runs (`e2e policy::ctrl_c_exits_130` covers one Ctrl-C). |
| Ctrl-C during `dropumi discover` or while `send` browses for a name target | No e2e test: the process gives no signal that the browse has started, and mDNS e2e tests are opt-in. The use cases are covered by `failures::cancel_stops_discover_without_reporting_peers` and `failures::cancel_while_resolving_a_name_stops_send`. A manual run on macOS 27.0.1 arm64 (SIGINT one second into `discover --timeout-ms 600000` and `send --to Bob --discover-timeout-ms 600000`) exited 130 at once, with no output from `discover` and a `cancelled` event from `send --json`. |
| The sender's CompleteAck deadline is not restarted by ignorable messages | No test: the in-memory receiver cannot send ignorable messages. The code restarts the deadline only when the number of acknowledged files grows, and ignorable messages are capped at 16 in a row. |
| exFAT / vfat placeholder fallback | Tested only with injected errors, not on a real exFAT or vfat volume. |
| MitM relay showing different SAS codes end to end (planned tests `channel::mitm_relay_sas_differs`, `security::mitm_relay_sas_codes_differ`) | Not implemented (planned). Covered at the protocol level by `mitm_split_handshakes_sas_independent`, and the pinned-trust case by `security::mitm_relay_rejected_by_pinned_trust`. |
| Pre-auth slots recover after silent connections (planned test `security::silent_preauth_slots_recover`) | Not implemented (planned). Admission limits are covered by `usecases::admission::tests::admission_limits_global_per_ip_and_session` and `usecases::admission::tests::one_session_per_fingerprint_regardless_of_session_slots`; the 10 s handshake deadline releases silent connections. |
| Release-only R8 problems in what the Android interop run calls | The CLI interop harness runs the minified `qa` build. It uses the release R8 rules plus keep rules (`apps/android/app/src/androidTest/proguard-qa.pro`) for the classes and members the interop test code references from the app: 89 classes from `:app`, `:appstate`, `:core`, the Kotlin standard library, kotlinx.coroutines and androidx.tracing, with renaming allowed. In `qa`, R8 never removes, inlines, merges or re-signs those members, though it may in release, so a release-only R8 problem in them cannot show there. Everything else gets the release treatment, including the ServiceLoader rewriting that loads kotlinx.coroutines' Main dispatcher. Classes in the dex on 2026-10-16: release 3,510, `qa` 3,568 (5,347 with the earlier rules, which kept all of Kotlin, kotlinx.coroutines, `:core` and `:appstate`). See ADR-0013 D10. |
| Performance regressions in CI | Not automated. Benchmarks run by hand with `scripts/bench/loopback.sh`. |
| FFI on a device | Apple: the core ran inside the app on the iOS 27.0 simulator and on macOS (interop tests); no iPhone or iPad yet. Android: inside the app on the arm64-v8a emulators API 26, 28 and 36.1 and on one Android 16 arm64-v8a phone (instrumented and interop tests, rows above). |
| Apple app on devices | Not tested: install on iPhone 15 and iPad Air (5th generation) and the device matrix are next. Share Extension, Photos import, background completion (`BGContinuedProcessingTask`), the Mac menu-bar receiver, notifications, open at login, the receive-folder bookmark and quarantine flags are all Not tested. |
| Per-event callback cost across JNA and Swift (ADR-0009 S6) | Not isolated: the opt-in measurement counts the events of a 2,000-file transfer, whose time is dominated by per-file fsync. |
| Host name published by native advertising (Bonjour, NSD) | Apple: the adapter registers a proxy with a random host; checked with `dns-sd` and from an unsandboxed process on the Mac, not yet from the app on a device (`apple_macos_bonjour_cli_finds_app_at_random_host`, DISC-4). Android: Not tested ([`discovery.md`](protocol/discovery.md) §7, §12). |
