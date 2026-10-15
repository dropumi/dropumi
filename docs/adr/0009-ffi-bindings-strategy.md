# ADR-0009: FFI and bindings strategy

- **Status:** Accepted
- **Date:** 2026-10-13 (proposed); 2026-10-14 (accepted after the milestone 2 spike)
- **Related:** ADR-0001, ADR-0008, ADR-0010, ADR-0014

## Context

- ADR-0008 chooses native UIs on top of the shared Rust core: Swift/SwiftUI for iPhone, iPad and Mac, and Kotlin/Jetpack Compose for Android. The specification asks to reuse the core through an appropriate FFI or binding strategy and to evaluate the options rather than assume one is best.
- The core is async (tokio in the application layer), uses cancellation tokens and streams progress events (ADR-0001). Rust futures cannot simply be dropped from a foreign language to cancel them, so cancellation across the boundary needs an explicit mechanism.
- The workspace forbids `unsafe` code (ADR-0001). Generated FFI scaffolding needs `unsafe` internally; how this interacts with the workspace lint was unknown.
- The iOS Share Extension does not load the core (milestone 2 decision D1: it copies the shared items into an App Group inbox and the app sends them), so no "lite" runtime profile is needed.
- `uniffi` 0.32.2 is licensed `MPL-2.0` (checked with `cargo info` on 2026-10-13).

## Decision

**Tool.** UniFFI 0.32.2 (`MPL-2.0`), pinned exactly (`=0.32.2`) in `[workspace.dependencies]`, generating Swift and Kotlin bindings in library mode.

**Two crates.**
- `dropumi-ffi` (`crates/ffi`) is the only crate that depends on `uniffi` (with `default-features = false`). It is a composition root like `dropumi-cli`: it wires the use cases to the infra adapters (`dropumi-infra` without `mdns`) and to app callbacks. The architecture script keeps `uniffi*` and `dropumi-ffi` out of every other crate (the CLI uses `dropumi-ffi` as a dev-dependency only, for interop tests).
- `dropumi-ffi-bindgen` (`crates/ffi-bindgen`) holds the generator binaries (`uniffi-bindgen`, `uniffi-bindgen-swift`), so the library never depends on the generator.

**Unsafe code.** `dropumi-ffi` inherits the workspace lints unchanged (`unsafe_code = "forbid"`). The FFI scaffolding that UniFFI's procedural macros generate (`#[unsafe(no_mangle)]` exports, `unsafe impl`s, raw handle conversion) is not attributed to this crate by rustc, so it compiles under `forbid` (spike S1); hand-written `unsafe` in the crate is still rejected. That generated code is UniFFI's, reviewed as a pinned dependency.

**API shape: use cases, never ports.**
- One object, `DropumiNode`, exposes identity, trust (list, add by full fingerprint, rename, remove), discovery (start, stop, list), receiving (start, `stop_accepting`, stop, change policy, cancel by transfer id), prompts (answer by prompt id, list, page through offered files), sending, pairing, a pairing window, `snapshot()` and `shutdown()`.
- **Every exported call is synchronous** and returns quickly; nothing is exported as `async`. Long work runs on the node's own runtime and reports through events. `send`, `pair_with` and `open_pairing_window` return an `OperationHandle` with `cancel()` and `result()`. Cancellation is explicit; no foreign future is ever dropped to cancel Rust work.
- Application ports are not exported. Where a platform supplies an adapter, the facade defines its own small callback interface: `PlatformDiscovery` (Bonjour, NSD), `ForeignFile` and `ForeignReader` (documents the app reads, such as Android `content://` URIs), and `EventListener`.
- Errors cross the boundary as `DropumiError`, with `ErrorCode` mirroring `TransferError::code()` one to one. A `detail` field is for local display only (it may name local files).

**Threads.** One tokio `current_thread` runtime on a dedicated thread (`dropumi-rt`), the CLI's model, with a bounded blocking pool. A second thread (`dropumi-events`) delivers events. App code is never called on the runtime thread or while a Rust lock is held: each call into the app runs on a short-lived thread of its own, and documents are read on a reader thread per file, so a stuck app callback cannot stall transfers. A call made from inside the runtime is refused instead of deadlocking.

**Events.** A bounded mailbox that never blocks the core:
- critical events (prompts, offers, results, receiver state) are kept; if more than 4,096 pile up (an app that stopped reading), the queue is replaced by `ResyncRequired` and the app reloads `snapshot()`;
- progress per transfer and peer changes per instance are coalesced, but never overtake a lifecycle event for the same transfer or peer;
- soft events (per-file results, warnings, closed sessions) are capped at 1,024, oldest dropped first, counted in `EventsDropped`;
- batches hold at most 256 events; critical events go out at once, other batches at most every 100 ms.

**Prompts.** Ported from the CLI's control registry with one change: a prompt is registered before `PromptOpened` is published. Prompt ids are never reused; answers name a prompt id and are never queued; a late or wrong-kind answer is refused; when the core stops waiting, `PromptClosed { Withdrawn }` is published. Nothing in the facade answers on its own; `AutoTrusted` (verified peers only) remains the core's only automatic accept.

**Discovery.** Rust owns every rule: random instance names (`txt::random_label`), building and parsing TXT (`txt::build`, `txt::parse`, invalid UTF-8 values treated as empty), dropping IPv6 and loopback addresses, filtering out its own advertisement, and capping the list at 256 devices. A send to a nearby device dials that device's resolved addresses and pins the paired device's full key when the hint matches exactly one trust entry, so a copied hint can neither redirect nor block a send.

**Files.** Path items go through `LocalFileSource::with_denied`, which refuses Dropumi's configuration files however they are named (directly, symlink, hard link); `send` also refuses paths inside the configuration directory. Stream items carry their metadata (read when the send starts) and are read by the app on a reader thread.

**Packaging.**
- Bindings are generated by `scripts/ffi/bindings.sh` and committed (`linguist-generated`); CI fails if they drift.
- `scripts/ffi/build-apple.sh` builds `DropumiFFI.xcframework` (iOS device, iOS simulator, macOS; arm64 only), consumed by the local Swift package `apps/apple/DropumiKit`.
- `scripts/ffi/build-android.sh` builds `libdropumi_ffi.so` with the NDK's clang and 16 KB page alignment; `scripts/ffi/check-16k.sh` verifies it. Kotlin uses JNA 5.19.1; UniFFI is configured to use JNA's cleaner, so one generated file serves Android and the host JVM.
- Two API names were chosen for the generators: `ForeignFile::open_reader` (UniFFI's Swift generator escapes the keyword `open` inside the C header too) and `ForeignReader::close_document` (generated Kotlin objects already have `close()`); error fields are `detail`, not `message` (which clashes with `Throwable.message` in Kotlin).

## Spike results (2026-10-14, macOS 27.0.1 arm64, Rust 1.99.0, Xcode 27.0, local runs)

| ID | Check | Result |
|---|---|---|
| S1 | Unsafe policy | Passed: the scaffolding compiles under the inherited `forbid`; `clippy -D warnings` is clean; exported symbols such as `uniffi_dropumi_ffi_fn_func_ffi_api_version` are present (`nm -gU`). No lint exception was needed. |
| S2 | MSRV | Passed: `cargo +1.89.0 check --workspace --all-targets --all-features --locked`. |
| S3 | Supply chain and architecture | Passed: `cargo deny --locked check` (advisories, bans, licenses, sources ok) and `scripts/ci/check-architecture.sh`. UniFFI pulls `anyhow` transitively (allowed; Dropumi code does not use it) and a second `getrandom` line (0.4) through `tempfile`. |
| S4 | Foreign async | Not applicable: no exported function is `async`. |
| S5 | Size | `libdropumi_ffi.so`, Android arm64-v8a, release, stripped: 2,085,640 bytes. JNA 5.19.1 `libjnidispatch.so` (arm64-v8a): 176,520 bytes. iOS `libdropumi_ffi.a` (release, before linking): 33,971,608 bytes; the linked app-size impact is measured with the app (milestone 2b). |
| S6 | Event cost | Partly measured: a 2,000-file loopback transfer (`measure_event_delivery`, release) took 8.1 s and delivered 2,008 events to the sender and 2,039 to the receiver; the time is dominated by the core's per-file fsync and acknowledgement, not by event delivery. Per-event JNA or Swift callback cost was not isolated. |
| S7 | Cancel latency | Remeasured 2026-10-15 (macOS 27.0.1 arm64, Rust 1.93.0, local run) with `cargo test --release -p dropumi-ffi --test measure --locked -- --ignored --nocapture`. `measure_cancel_latency` (loopback, 20 runs) now starts the timer only once the sender's reader reports that it is blocked at its hold point (100,000 of 400,000 bytes); the earlier run started it at the receiver's `TransferAccepted`, when the reader was not yet blocked (20 of 20 runs in a check of that timing). From `OperationHandle::cancel()` to `TransferFinished(Cancelled)` on the sender: min 0.18 ms, median 0.55 ms, max 0.83 ms; on the receiver: min 0.18 ms, median 0.55 ms, max 0.83 ms. |
| S8 | Thread safety | Passed: `stress::concurrent_calls_during_transfers` runs 8 threads × 10,000 mixed node calls while three prompted transfers complete; no panic, deadlock or lost answer. |
| S9 | Share Extension memory | Not applicable: the extension does not load the core (decision D1). |
| S10 | Generated code compiles | Passed: the Swift bindings build in Swift 6 language mode (Swift 6.4) and pass `swift test`; the Kotlin bindings compile with AGP 9.4.1 and pass the JVM tests. |

## Implementation

- Code: `crates/ffi/src/` (`node.rs`, `mailbox.rs`, `prompts.rs`, `discovery.rs`, `source.rs`, `runtime.rs`, `types.rs`, `error.rs`), `crates/ffi/uniffi.toml`, `crates/ffi-bindgen/`, `scripts/ffi/`, `apps/apple/DropumiKit/`, `apps/android/core-bindings/`.
- Core changes made for the facade (each test-first): cancel one incoming transfer by id and reject a reused active id with BUSY (`receive.rs`), back off on accept errors, change a running receiver's policy and stop accepting gracefully (`ReceiverControl`), rename a trusted device atomically (`TrustStore::rename`), move the random instance label out of the `mdns` module, and refuse configuration files in `LocalFileSource`.
- Tests: `crates/ffi/src/*` unit tests (mailbox, prompts, discovery, errors); `crates/ffi/tests/{node,discovery,stress}.rs` (two real nodes over loopback); `crates/cli/tests/e2e/ffi_interop.rs` (the facade against the real CLI); `apps/apple/DropumiKit/Tests` (Swift); `apps/android/core-bindings/src/test` (Kotlin, host JVM); `crates/ffi/tests/measure.rs` (opt-in measurements).
- Evidence: the facade, CLI interop, Swift and Kotlin suites: Automated test passed (macOS 27.0.1 arm64, local run). Android ABIs and the iOS slices: Build verified only (local). CI: `ffi (apple)` and `ffi (android)`, Not tested (pending CI).

## Consequences

### Positive

- One binding stack for both platforms; UniFFI is MPL-2.0 like Dropumi.
- The core crates stay FFI-free, and the whole workspace keeps `forbid(unsafe_code)`.
- Coarse use-case calls keep the apps thin; protocol rules, discovery rules and file safety stay in Rust.
- A slow, stuck or failing app cannot stall transfers or receivers.

### Negative

- UniFFI is pre-1.0; a minor release can break generated code, so upgrades need deliberate work (the committed bindings make every change visible).
- The generated `unsafe` scaffolding is trusted as part of a pinned dependency rather than checked by the crate's own lint.
- Synchronous calls read small configuration files; apps must call them off their main thread.
- Per-event callback cost across JNA and Swift was not isolated (S6); batching bounds the number of calls by design.

## Alternatives considered

- **swift-bridge:** Swift only and pre-1.0; Kotlin would need a second binding stack (UNVERIFIED).
- **cbindgen with hand-written Swift/Kotlin wrappers:** maximum control, manual memory and async handling; remains the fallback if UniFFI stops fitting.
- **Diplomat:** at the reviewed version it lacked a Swift backend (UNVERIFIED).
- **Hand-written JNI:** Kotlin only and `unsafe`-heavy boilerplate.
- **Exporting async functions:** would tie cancellation to foreign future semantics and require a foreign-side executor contract; synchronous calls plus operation handles were enough.
- **Exporting application ports directly:** couples app code to internal trait signatures.
