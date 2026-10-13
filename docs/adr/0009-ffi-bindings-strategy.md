# ADR-0009: FFI and bindings strategy

- **Status:** Proposed
- **Date:** 2026-10-13 (context updated with milestone 1 facts on 2026-10-13)
- **Related:** ADR-0001, ADR-0008, ADR-0010

## Context

- ADR-0008 chooses native mobile UIs (Swift/SwiftUI with a Share Extension, Kotlin/Jetpack Compose) on top of the shared Rust core. The specification asks to reuse the core through an appropriate FFI or binding strategy and to evaluate UniFFI and other bridging options rather than assume one is best.
- The core is async (tokio in the application layer), uses cancellation tokens and streams progress events (ADR-0001). Rust futures cannot simply be dropped from a foreign language to cancel them, so cancellation across the boundary needs an explicit mechanism.
- An iOS Share Extension runs in its own process with tight resource limits, so the Rust core must be usable there with a small runtime and streaming I/O.
- The workspace forbids `unsafe` code (ADR-0001). Generated FFI scaffolding typically needs `unsafe` internally; how this interacts with the workspace lint requires investigation.
- **No FFI is built in milestone 1.** This record states the intended direction so that Phase 1 code does not paint the project into a corner.
- What exists today: the core library crates (domain, protocol, application, infra without `mdns`) build for `aarch64-apple-ios` (Xcode 27 SDK) and `aarch64-linux-android` (NDK 27.1 clang) on the maintainer's machine; status: Build verified only (core library crates; no app, not run). Cancellation is already token-based (`CancellationToken`, and `TransferCanceller` on the receiver), and progress is already an event stream (`AppEvent`), which is the shape a facade needs.
- `uniffi` 0.32.2 is licensed `MPL-2.0` (checked with `cargo info` on 2026-10-13). Facts about the alternatives below come from Phase 0 research notes of the same date and were not re-checked for this record: treat them as UNVERIFIED until the Phase 3 spike.

## Decision (proposed)

**Tool.** UniFFI 0.32.2 (`MPL-2.0`), generating Swift (iOS/iPadOS, and macOS if a native desktop UI is chosen) and Kotlin (Android) bindings. One exact version is pinned across the workspace and the binding generator.

**One facade crate.** All bindings live in a single future crate, `dropumi-ffi`. No other crate depends on `uniffi`; domain, protocol, application and infra stay FFI-free.

**API shape: use cases, never ports.**
- The facade exposes coarse-grained use cases: start and stop receiving and discoverability, discover nearby devices, send files, accept or decline an offer, pair and confirm a SAS, manage trusted devices, cancel by transfer id, query progress.
- Application ports are **not** exported directly. Where a platform must supply an adapter (for example native discovery via NWBrowser or NsdManager, Apple peer-to-peer links via Network framework `includePeerToPeer` rather than the deprecated Multipeer Connectivity, notifications, or file access through file descriptors or URLs), the facade defines its own small callback interfaces and adapts them to the internal ports, so port signatures can evolve without breaking bindings.
- Async functions map to foreign futures (Swift `async`, Kotlin `suspend`).
- Cancellation is explicit: long-running operations return a handle with a `cancel()` method backed by the core's cancellation token. Dropping a foreign future is not relied on.
- Events (progress, offers, SAS prompts, completion) are delivered through callback interfaces; progress is batched (no more than about 10 updates per second).
- Errors cross the boundary as flat enums derived from the stable `TransferError` codes; panics are converted to errors.

**Share-extension profile.** A "lite" configuration for extension processes: tokio `current_thread` runtime, bounded buffers, streaming from file URLs, never whole files in memory.

**Status change.** This record moves to Accepted (or is superseded) when Phase 3 starts, after a verification spike measures: the binary-size delta of the FFI layer, the overhead of 10,000 progress callbacks, memory of the Rust core inside an iOS Share Extension process, cancellation latency, and thread safety of exported objects. If the spike fails, re-evaluate `cbindgen` with thin hand-written Swift and Kotlin wrappers.

## Consequences

### Positive

- One binding stack for both mobile platforms; UniFFI itself is MPL-2.0, the same license as Dropumi.
- The core crates stay free of FFI concerns and keep `forbid(unsafe_code)`.
- Coarse use-case APIs keep foreign UIs thin and keep transfer logic in Rust, matching ADR-0008.
- Explicit cancellation handles avoid relying on future-drop semantics across languages.

### Negative

- UniFFI is pre-1.0; minor releases can contain breaking changes, so upgrades need deliberate work.
- The facade crate will likely need a scoped exception to the workspace `unsafe_code` lint for generated code (requires investigation).
- A C# or C++ desktop UI on Windows would need a different binding route (third-party UniFFI backends or a C ABI); not covered here.
- Every exported type is a compatibility surface for the apps; the facade must be versioned carefully.

## Alternatives considered

- **swift-bridge:** Swift only and pre-1.0; Kotlin would need a second binding stack (UNVERIFIED).
- **cbindgen with hand-written Swift/Kotlin wrappers:** maximum control, manual memory and async handling; kept as the fallback and for a possible C ABI.
- **Diplomat:** at the reviewed version it lacked a Swift backend (UNVERIFIED).
- **Hand-written JNI:** Kotlin only and `unsafe`-heavy boilerplate.
- **Exporting application ports directly:** couples app code to internal trait signatures.
- **Building FFI in milestone 1:** no app consumes it yet; the spike belongs to Phase 3.
