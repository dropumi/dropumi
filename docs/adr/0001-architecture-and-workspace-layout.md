# ADR-0001: Architecture and workspace layout

- **Status:** Accepted
- **Date:** 2026-10-13 (reconciled with the milestone 1 code on 2026-10-14)
- **Related:** ADR-0002, ADR-0005, ADR-0009, ADR-0010, [`ARCHITECTURE.md`](../../ARCHITECTURE.md)

## Context

- The master specification asks for Clean Architecture with Hexagonal Architecture (ports and adapters): dependencies point inward, the domain imports no infrastructure, traits sit at meaningful boundaries, constructor-based dependency injection, no global mutable state, explicit errors, no unchecked `unwrap`/`expect` on runtime paths, and discovery kept separate from transport.
- It sketches a conceptual layout with many crates (`domain`, `application`, `protocol`, `discovery`, `transport`, `security`, `platform`, `testing`) but also says: prefer the smallest maintainable workspace first and split crates only when module boundaries and dependency ownership justify it.
- Milestone 1 delivers a Rust core and a CLI, not apps. The protocol must later be reusable from mobile apps (ADR-0009), from hostile test peers, from fuzzers and possibly from a WebAssembly web companion. That argues for a protocol core that performs no I/O.
- The trust and identity stores need std `File::lock` (stable since Rust 1.89) for cross-process locking, and `proptest` 1.12.0 declares `rust-version = "1.88"`.

## Decision

**Workspace** (`Cargo.toml`). One Cargo workspace: `resolver = "3"`, `members = ["crates/*"]`, `edition = "2024"`, `rust-version = "1.89"`, `license = "MPL-2.0"`, `publish = false`, version `0.1.0`. There is no `rust-toolchain.toml`; CI tests the latest stable and the `msrv` job checks the `rust-version` it reads from the manifest.

**Five crates, dependencies pointing inward:**

```
dropumi-domain  <-  dropumi-protocol  <-  dropumi-application  <-  dropumi-infra  <-  dropumi-cli (bin "dropumi")
```

| Crate | Owns | Direct external dependencies | Denied by the architecture guard (any depth) |
|---|---|---|---|
| `dropumi-domain` (`crates/domain`) | Pure model: keys and fingerprints, device names, peers, targets, connection candidates and `PeerExpectation`, transfer policy, trusted devices, `SafeFileName`, `text::strip_unsafe`, transfer state machines, `TransferError`. No I/O, clock, tokio, serde or crypto. | `data-encoding`, `thiserror`, `unicode-normalization` | tokio, tokio-util, snow, sha2, getrandom, minicbor, blake3, mdns-sd, tempfile, serde, serde_json, dirs, clap, tracing, tracing-subscriber, rustix, every other `dropumi-*` crate |
| `dropumi-protocol` (`crates/protocol`) | Sans-IO protocol: limits, version preamble and prologue, records, frames, strict CBOR codec, messages, static identity, Noise XX handshake, committed SAS, sender and receiver session machines. Bytes in, bytes out. | `blake3`, `getrandom`, `minicbor`, `sha2`, `snow`, `thiserror`, `zeroize` | tokio, tokio-util, mdns-sd, tempfile, serde, serde_json, dirs, clap, tracing, tracing-subscriber, application, infra, cli |
| `dropumi-application` (`crates/application`) | Ports, the secure-channel driver (`channel::{establish, io}`), use cases, events, timeouts and admission limits. | `thiserror`, `tokio`, `tokio-util`, `tracing` | mdns-sd, tempfile, dirs, serde, serde_json, clap, tracing-subscriber, rustix, infra, cli |
| `dropumi-infra` (`crates/infra`) | Adapters: TCP, receive directory and finalization, file source, cleanup, identity/device/trust stores, config paths and permission checks, TXT parsing, mDNS (feature `mdns`), system clock. | `dirs`, `serde`, `serde_json`, `tempfile`, `thiserror`, `tokio`, `tracing`, `zeroize`, `rustix` (Unix), `mdns-sd` (optional) | not checked by the script; `dropumi-cli` has no library target, so nothing can depend on it |
| `dropumi-cli` (`crates/cli`) | Composition root and the `dropumi` binary: clap arguments, human and NDJSON output, `--control-stdin`, interaction modes, exit codes, end-to-end tests. | `clap`, `serde_json`, `tokio`, `tokio-util`, `tracing`, `tracing-subscriber` | (top of the graph) |

**Ports.** `dropumi_application::ports` defines nine ports plus two helper traits. Async methods are native `fn … -> impl Future<Output = …> + Send` (no `async-trait`, no `dyn`); use cases are generic over ports and receive them through constructors or dependency structs.

| Port | Kind | Real adapter | Test double (`crates/application/tests/support/mod.rs`) |
|---|---|---|---|
| `Connector` | async | `TcpConnector` | `FakeConnector` over in-memory duplex streams |
| `Listener` | async | `TcpListenerAdapter` | `FakeListener` |
| `Discovery` (helper `AdHandle` = `Discovery::Handle`) | async | `MdnsDiscovery` / `MdnsAd` (feature `mdns`); `NoDiscovery` in CLI builds without it | `StaticDiscovery` |
| `FileSource` | async | `LocalFileSource` | `MemSource` |
| `FileSink` (helper `IncomingFile` = `FileSink::File`: `write`, `commit`, async `discard`) | async | `ReceiveDir` / `IncomingFsFile` | `MemSink` / `MemFile`, with injectable write failures |
| `IdentityStore` | sync | `FileIdentityStore` (`identity.key` + `device.json`) | `MemIdentity` |
| `TrustStore` | sync | `TrustFile` | `MemTrust` |
| `Interaction` (offer decisions, SAS confirmation) | async | `CliInteraction`: control channel, terminal prompt, or never (rejects and denies) | `Scripted` |
| `Clock` | sync | `SystemClock` | `ManualClock` |

**Use cases** (`crates/application/src/usecases/`).

| Specification use case | Code |
|---|---|
| ConnectToPeer | `connect::{resolve, dial}` |
| SendFiles, CancelTransfer (sender), QueryTransferProgress | `send::SendFiles` |
| ReceiveFiles, AcceptTransfer, RejectTransfer, CancelTransfer (receiver) | `receive::ReceiveFiles` with `receive::TransferCanceller` and `admission::Admission` |
| PairDevice | `pair::{pair_with, pair_listen}` |
| DiscoverNearbyDevices, ManageDiscoverability | `discovery::{discover, advertise}` |
| ManageTrustedDevices | `trust::{list, add_manual, remove}` |
| Device identity | `identity::{show, reset, set_name}` |
| ConfigureBackgroundReceiver | Deferred to the app phases |

- Accept and reject go through the `Interaction` port. Cancellation uses a `CancellationToken`; on the receiver, the control command `cancel` cancels active transfers with `Cancel USER` and stopping the receiver cancels them with `Cancel SHUTDOWN`.
- Progress and every other observable step are `AppEvent`s sent through an `EventSink` (a bounded tokio `mpsc` channel; the CLI uses capacity 256).

**Runtime.** tokio is used only in application, infra and cli. The CLI builds a `current_thread` runtime with `enable_all()` and `max_blocking_threads(8)`; blocking filesystem work runs in `spawn_blocking`. Domain and protocol never depend on tokio.

**Errors.** `thiserror` only; no `anyhow`. Public error enums are `#[non_exhaustive]`. `domain::error::TransferError::code()` is the single taxonomy behind NDJSON `code` fields and CLI exit codes (`crates/cli/src/exit.rs`). Protocol failures map to wire codes with `ProtocolError::wire_code()` and to `TransferError` with `ProtocolError::to_transfer_error()`.

**Lint policy** (workspace `[lints]`; every crate sets `[lints] workspace = true`).
- Rust: `unsafe_code = "forbid"`; `missing_debug_implementations` and `rust_2018_idioms` warn.
- Clippy: `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `dbg_macro` and `mem_forget` are denied; `print_stdout`, `print_stderr`, `exit` and `cast_possible_truncation` warn. CI runs clippy with `-D warnings`, so warnings fail the build. `clippy.toml` allows `unwrap`, `expect`, `panic` and indexing in tests.
- The protocol crate parses untrusted input and additionally has `#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects, clippy::cast_possible_truncation)]` in `crates/protocol/src/lib.rs` (indexing and arithmetic are allowed in its unit tests).
- Profiles: `release` uses `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`, and keeps `panic = "unwind"` on purpose, because temp-file cleanup relies on `Drop`. `dev` uses `debug = "line-tables-only"` and builds dependencies with `opt-level = 3` so that crypto and hashing stay fast in tests.

**Enforcement.** `scripts/ci/check-architecture.sh` runs `cargo tree -e normal --all-features --target all --locked` for domain, protocol and application, and fails if any crate in that package's deny-list (table above) appears at any depth. It also fails if `ring` appears anywhere in the workspace's normal dependency graph (ADR-0002). Exit status: 0 clean, 1 violation, 2 graph could not be read. It runs as the CI job `architecture`.

**Deferred spec crates.** The specification's `discovery`, `transport`, `security`, `platform` and `testing` crates, and the `apps/` and `platform/` directories, are not created until a second implementation exists (for example BLE or native Bonjour discovery). Until then:

| Spec crate | Lives in |
|---|---|
| discovery | `domain::discovery`, `application::usecases::discovery`, `infra::{mdns, txt}` |
| transport | `application::channel`, `infra::net` |
| security | `protocol::{identity, handshake, sas, record}`, `infra::perms` |
| testing | each crate's `tests/`, `crates/application/tests/support/`, `crates/cli/tests/e2e/support/` |

## Implementation

- Workspace, lints and profiles: [`Cargo.toml`](../../Cargo.toml), [`clippy.toml`](../../clippy.toml), [`crates/protocol/src/lib.rs`](../../crates/protocol/src/lib.rs).
- Ports and use cases: [`crates/application/src/ports.rs`](../../crates/application/src/ports.rs), [`crates/application/src/usecases/`](../../crates/application/src/usecases/), [`crates/application/src/channel/`](../../crates/application/src/channel/).
- Composition root: [`crates/cli/src/main.rs`](../../crates/cli/src/main.rs) (runtime) and [`crates/cli/src/commands/mod.rs`](../../crates/cli/src/commands/mod.rs).
- Guard: [`scripts/ci/check-architecture.sh`](../../scripts/ci/check-architecture.sh).
- Evidence: the architecture check passed and all 224 workspace tests passed (1 opt-in mDNS test ignored) on the maintainer's machine (macOS 27 arm64, Rust 1.99.0). CI has not run yet.

## Consequences

### Positive

- The dependency direction is checked by the compiler (crate graph) and by a CI script, not only by convention.
- The sans-IO protocol crate is tested without sockets, driven by hostile raw peers in the e2e suite, and can be fuzzed or wrapped by an FFI facade later.
- Mobile cross-builds compile the core library crates without the CLI and without mDNS (`--exclude dropumi-cli --no-default-features`).
- One error taxonomy keeps JSON output and exit codes consistent for future UIs and bindings; protocol failures map to wire codes in one place (`ProtocolError::wire_code()`).
- Measured on the maintainer's machine with `scripts/bench/loopback.sh` (macOS 27, Apple M4 Pro): the stripped release binary is 2,819,248 bytes, and an idle `receive` process used 3,744 KiB RSS after 10 s and 0.00 s CPU over 60 s.

### Negative

- `dropumi-infra` bundles several unrelated adapters; it will need splitting when platform adapters arrive.
- Generic, monomorphised use cases may cost compile time and binary size compared with `dyn` ports (not measured).
- MSRV 1.89 excludes older toolchains; a dependency that raises its MSRV forces a bump and an update of this record.
- Moving modules into the deferred spec crates later is a refactor that touches import paths.
- `dropumi-protocol` calls the OS random number generator itself (`protocol::random`, and snow's `use-getrandom` for ephemeral keys). SAS nonces are passed into the handshake API, but the crate is not fully deterministic without a seeded source.

## Alternatives considered

- **Single crate:** no compile-time enforcement of the dependency direction.
- **The spec's full crate list now:** crates without a second implementation add boundaries with nothing to separate.
- **`async-trait` or `dyn` ports:** native `impl Future + Send` is enough and avoids boxing.
- **Multi-thread tokio runtime by default:** no measured need; larger idle footprint.
- **`anyhow` in the CLI:** a typed CLI error drives exit codes without downcasting.
- **`rust-toolchain.toml` pin:** CI should track stable; the MSRV is declared in the manifest instead.
- **MSRV 1.88:** std `File::lock` needs 1.89.
