# AGENTS.md

Rules for coding agents and human contributors working in this repository. Keep this file under 200 lines; detail lives in [ARCHITECTURE.md](ARCHITECTURE.md), [docs/adr/](docs/adr/), [docs/protocol/](docs/protocol/), [docs/testing.md](docs/testing.md) and [CONTRIBUTING.md](CONTRIBUTING.md). Tool-specific instruction files only import this file and add tool rules; they never contradict it.

## 1. Project snapshot and philosophy
- Dropumi: a free, open-source (MPL-2.0), local-only, privacy-first AirDrop alternative with a Rust core. "Just drop it nearby." Repo: https://github.com/dropumi/dropumi, default branch `main`.
- **Status:** pre-alpha and unaudited. Milestone 1 provides the Rust core, protocol v1 (draft) and the `dropumi` CLI. There are no UIs, no FFI and no mobile apps yet. The current verification status per platform is in [docs/testing.md](docs/testing.md); never upgrade a label without new evidence.
- **Principles:**
  - no accounts, cloud, telemetry or internet dependency;
  - authenticated encryption on every connection;
  - secure defaults;
  - a small, measured footprint;
  - documentation that says exactly what was verified.
- Correctness and security come before feature count. Ship real, working vertical slices, or nothing.

## 2. Non-negotiable git and GitHub rules
- **Never commit to `main`. Never push to `main`. Never force-push.**
- **Never merge a pull request** (no `gh pr merge`, no merge button, no auto-merge). The maintainers review and merge.
- **Never change repository or organization settings:** rulesets, branch protection, Actions policy, secrets, roles or security features. Never bypass protections.
- **Branches:** work on a branch created from an up-to-date `main`. Allowed prefixes: `feat/` `fix/` `refactor/` `docs/` `test/` `perf/` `chore/` `ci/`.
- **Commits:** use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), `type(scope): summary`. Scopes: `domain` `protocol` `security` `core` `infra` `discovery` `cli` `ci` `docs` `deps` `bench`.
- **Pull requests:** one coherent milestone per PR, targeting `main`, with the template filled in using real command output. If you cannot push or open a PR, print the exact commands and state that no PR exists.
- **Staging:** stage explicit paths. Never commit keys, trust stores, benchmark output (`bench-out/`), secrets or personal data (user names, host names, home paths).

## 3. Directory map
| Path | Responsibility |
|---|---|
| `crates/domain/` (`dropumi-domain`) | Pure models and invariants: fingerprints, device names, peers, targets, connection candidates and `PeerExpectation`, transfer policy, trust, `SafeFileName`, `text::strip_unsafe`, limits, transfer state machines, `TransferError`. No I/O. Property tests in `tests/`. |
| `crates/protocol/` (`dropumi-protocol`) | Sans-IO wire protocol: `version` (preamble, prologue), `record` (`Sealer`/`Opener`), `frame`, strict CBOR `codec`, `message`, Noise XX `handshake`, static `identity`, `sas`, `session/{sender,receiver}`, `limits`, `random`. Integration and property tests in `tests/`. |
| `crates/application/` (`dropumi-application`) | `ports.rs`, `channel/{establish,io}.rs` (secure channel, `RecordReader`/`RecordWriter`), `usecases/` (connect, send, receive, admission, pair, trust, identity, discovery), `events.rs`, `config.rs` (timeouts, admission limits). Tests in `tests/` with in-memory fakes in `tests/support/`. |
| `crates/infra/` (`dropumi-infra`) | Adapters: `net` (TCP), `fs/{receive_dir,finalize,source,cleanup}`, `store/{identity_file,trust_file,atomic,lock}`, `perms`, `paths`, `clock`, `txt`, `mdns` (cargo feature `mdns`, on by default). Unit tests in `src/`. |
| `crates/cli/` (`dropumi-cli`, bin `dropumi`) | Composition root: `args`, `commands/`, `output` (human and NDJSON), `control` (`--control-stdin`), `interaction`, `exit` (exit codes). `tests/e2e/` holds the two-process suites and `support/{proc,files,raw_peer,proxy}.rs`. |
| `docs/` | `feasibility.md`, `adr/`, `protocol/{v1,threat-model,discovery}.md`, `benchmarks/`, `cli.md`, `development.md`, `testing.md`, `dependencies.md` |
| `scripts/` | `ci/check-architecture.sh` (dependency deny-lists and `ring` ban), `bench/loopback.sh` (loopback benchmarks) |
| `.github/` | `workflows/{ci,codeql,audit,deploy-site}.yml` (`deploy-site` publishes `website/` to GitHub Pages), `dependabot.yml`, `ISSUE_TEMPLATE/`, `pull_request_template.md` |
| `website/` | Project website (Astro). It has its own `AGENTS.md` for site work; generated lockfiles are exempt from the SPDX header rule. |
| `branding/`, `design.md` | Brand and UI design (no code): logo files (`branding/final/`) and rules (`branding/GUIDELINES.md`), app-icon and store assets, the UI design source (`branding/dropumi-ui.pen`) and exports; `design.md` holds the design tokens and UI rules. |
| Root | `Cargo.toml` (workspace, lints, profiles), `Cargo.lock` (committed), `clippy.toml`, `deny.toml`, `.gitattributes`, `LICENSE`, `README.md`, `ARCHITECTURE.md` and the community files |

## 4. Architecture rules
- **Dependencies point inward:** `domain` ← `protocol` ← `application` ← `infra` ← `cli`. An inner crate never depends on an outer one. `scripts/ci/check-architecture.sh` enforces this with per-crate deny-lists (CI job `architecture`); the lists in the script are authoritative.
- **domain:** no I/O, no clock, no async runtime, no serde, no crypto, no logging.
- **protocol:** sans-IO: no tokio, sockets, files or logging. Callers pass in SAS nonces and transfer IDs; OS randomness comes only from `getrandom` (the `random` and `identity` modules, and `snow` for ephemeral keys).
- **application:** defines the ports and depends only on them, never on concrete adapters. It must not depend on `mdns-sd`, `tempfile`, `dirs`, `serde`, `serde_json`, `clap`, `tracing-subscriber` or `rustix`.
- **infra** implements the ports. **cli** is the composition root and owns user interaction and output.
- **Ports** (`crates/application/src/ports.rs`): nine ports (`Connector`, `Listener`, `Discovery`, `FileSource`, `FileSink`, `IdentityStore`, `TrustStore`, `Interaction`, `Clock`) plus the helper traits `AdHandle` and `IncomingFile`. Async port methods return `impl Future + Send`; use cases are generic over ports. No `dyn` and no async-trait. Use constructor injection. No global mutable state.
- **Discovery is separate from transport.** Names and TXT data are hints, never identity. The identity is the authenticated static key.
- Platform-specific code lives only in adapters, behind `cfg` or cargo features.
- **Significant decisions need an ADR** in `docs/adr/`. Add a crate only when a real boundary justifies it (ADR-0001).
- **Wire-format changes:** update `docs/protocol/v1.md` and the threat model in the same PR.

## 5. Naming
- Crates are `dropumi-<layer>` in `crates/<layer>/`. The binary is `dropumi`. Prose says "Dropumi"; code and commands say `dropumi`.
- Follow the Rust API guidelines: `UpperCamelCase` types, `snake_case` functions and modules, `SCREAMING_SNAKE_CASE` constants.
- Ports are named after a capability (`TrustStore`, `FileSink`). Adapters are named after their technology or storage (`TcpConnector`, `TrustFile`, `FileIdentityStore`, `ReceiveDir`, `MdnsDiscovery`). Use cases are verbs or verb phrases (`SendFiles`, `ReceiveFiles`, `pair_with`, `pair_listen`).
- Error enums are named `<Area>Error` (`ProtocolError`, `StoreError`, `TransferError`). Hash domain-separation labels are `dropumi/v1/<purpose>` (for example `dropumi/v1/device-fingerprint`). Receive temp files are `.dropumi-<12 alphanumerics>.part`.
- Test names state the behaviour: `unverified_never_auto_accepted`.

## 6. Error handling
- Use `thiserror` only (no `anyhow`). Public error enums are `#[non_exhaustive]`.
- **Outside tests, workspace lints deny** `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, `dbg!` and `mem::forget` (`clippy.toml` allows `unwrap`, `expect`, `panic!` and indexing in tests). protocol also denies `indexing_slicing`, `arithmetic_side_effects` and `cast_possible_truncation`; use checked operations.
- `TransferError::code()` is the single local taxonomy behind JSON events and exit codes. Exit codes are listed in `docs/cli.md` and `crates/cli/src/exit.rs`.
- **Nothing local reaches the peer.** A wire `Error` carries a protocol `ErrorCode` and the fixed phrase from `ErrorCode::wire_detail()`. Storage and OS error text stays local.
- Every failure path calls `IncomingFile::discard()`. `Drop` is only a best-effort fallback and never sleeps.
- Never call `std::env::set_var`, not even in tests; tests pass environment through `Command::env`. Library code never calls `std::process::exit`. Nothing may panic across a future FFI boundary.

## 7. Security rules
- `unsafe_code = "forbid"` in every crate.
- **No plaintext mode and no fallback.** Everything after the version preamble is Noise-encrypted.
- **Never weaken a security invariant to make a test or a feature pass.**
- **Bound all peer input before allocating:** records (2-byte length, at most 65,535 B of Noise message), CBOR (definite lengths, sorted unique keys, depth 4, 32 map entries), files per transfer (10,000), offer metadata (4 MiB), names (1,024 B on the wire, 255 B on disk), pre-auth admission (16 total, 4 per IP) and timeouts. The constants live in `crates/{domain,protocol}/src/limits.rs` and `crates/application/src/config.rs`.
- **Unverified peers are never auto-accepted.** The default policy is `TrustedOnly`. There is no TOFU. Trust is created only by SAS pairing or by a manual add with the full fingerprint.
- Name and fingerprint targets pin the expected key, which is checked **before msg3**. Displayed peers show the local alias first; a Hello name is labelled "claims".
- **Receiver filesystem:**
  - sanitize names with `SafeFileName::sanitize`;
  - write to a `create_new` temp file;
  - hash with BLAKE3 while writing; only on a match, fsync and finalize without overwriting;
  - never follow symlinks;
  - never use `std::fs::rename` for finalization.
- Strip control, bidirectional and invisible characters from every peer string you display or log (`text::strip_unsafe`, `terminal_safe`).
- **Never log** keys, nonces or file contents. Local paths may appear in local error messages only: never in `tracing` logs, never on the wire. Logs go to stderr.
- On Unix, config files (`identity.key`, `device.json`, `trusted_devices.json`) and received files are created 0600, and a config directory Dropumi creates is 0700. The owner and mode checks (exit 9) are not optional.
- **Crypto comes only from the vetted crates** (`snow` and its RustCrypto/dalek backends, `sha2`, `blake3`, `getrandom`). Never write your own primitives.
- No network traffic beyond the LAN peer and mDNS: no telemetry, no update checks, no internet calls.
- Tests, scripts and benchmarks use `mktemp -d` config dirs, never the user's real configuration.

## 8. Testing expectations
- Domain and protocol code is developed test-first. Parsers and sanitizers get property tests (`proptest`).
- Application tests use in-memory duplex streams, fakes and paused tokio time. No real sleeps.
- **End-to-end tests:**
  - run two real `dropumi` processes on loopback (`127.0.0.1`, port 0) with `--json --control-stdin` and temp dirs;
  - never synchronize with `sleep`;
  - never add retries to hide flaky tests.
- Every bug fix gets a regression test. Every threat-model row names its test.
- **Opt-in tests:**
  - mDNS tests are `#[ignore]`, named `mdns_*`, and run with `DROPUMI_TEST_MDNS=1` and `--ignored`;
  - the large-file test size is set with `DROPUMI_TEST_LARGE_MIB` (default 64).
- **Time budgets:**
  - unit and application tests together: < 30 s;
  - end-to-end: < 4 min per OS;
  - any single test: < 60 s.

## 9. Performance
- Stream data. Never buffer whole files. Use bounded channels and buffers; DATA frames carry at most 65,518 B.
- Keep the idle receiver cheap. The CLI uses a `current_thread` runtime.
- **Measure before optimizing,** with `scripts/bench/loopback.sh`. Publish only measured numbers, and label estimates "design estimate, not measured".
- Never trade authentication or encryption for speed or binary size. Each dependency costs size and build time.

## 10. Platform integration guidelines
- Native adapters belong only in the platform layer (future adapter crates or `apps/` directories). The core crates stay platform-independent.
- **Use only public, documented OS APIs.**
- **Permissions:** request Local Network, Bluetooth, Nearby Wi-Fi Devices and notification permissions in context, only when needed, with a stated reason.
- Put platform features behind cargo features or `cfg`. Respect OS background-execution and extension limits.
- **Test status labels:** `Automated test passed`, `Manual device test passed`, `Build verified only`, `Not tested`, `Unsupported`.
- **Capability labels:** `Documented capability - not tested`, `Requires investigation`, `Unsupported`, `Not applicable`.
- **Never write "supported".** Always name the OS version, the architecture and the environment. A result from one OS never stands for another.

## 11. Dependency and license policy
- MPL-2.0. The first line of every `.rs` file is `// SPDX-License-Identifier: MPL-2.0` (CI job `fmt` checks line 1). TOML, YAML and shell files start with `# SPDX-License-Identifier: MPL-2.0` (shell scripts on line 2, after the shebang; generated `pnpm-lock.yaml` files are exempt); CI job `fmt` checks this too.
- **New dependencies:**
  - justify them in the PR and in `docs/dependencies.md`;
  - they must pass `cargo deny --locked check`;
  - add them to `[workspace.dependencies]` only in the change that first uses them;
  - `Cargo.lock` is committed, and `--locked` is used everywhere.
- **Banned:** `openssl`, `openssl-sys`, `native-tls`, `ring`. `getrandom` 0.3 and `sha2` 0.10 follow `snow`.
- **No copied code** from other projects (PairDrop is GPL-3.0; nothing from LocalSend either).
- **GitHub Actions:**
  - pin actions by full SHA with a version comment;
  - `permissions: contents: read`;
  - `persist-credentials: false`;
  - no `pull_request_target`;
  - no secrets in PR CI.

## 12. Exact commands
The first block mirrors the CI jobs in `.github/workflows/ci.yml` (job names in comments).
```sh
cargo fmt --all --check                                                          # fmt
git ls-files -z -- '*.rs' | xargs -0 awk 'FNR == 1 && $0 != "// SPDX-License-Identifier: MPL-2.0" { print FILENAME; bad = 1 } END { exit bad }'  # fmt
git ls-files -z -- '*.toml' '*.yml' '*.yaml' '*.sh' ':!*pnpm-lock.yaml' | xargs -0 awk 'FNR == 1 && /^#!/ { next } FNR <= 2 { if ($0 != "# SPDX-License-Identifier: MPL-2.0") { print FILENAME; bad = 1 } nextfile } END { exit bad }'  # fmt
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings    # clippy
cargo clippy --workspace --all-targets --no-default-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked  # docs
cargo test --workspace --all-features --locked                                   # test (<os>)
cargo +1.89.0 check --workspace --all-targets --all-features --locked            # msrv
bash scripts/ci/check-architecture.sh                                            # architecture
cargo deny --locked check                                                        # cargo-deny (both jobs)
cargo build --workspace --exclude dropumi-cli --no-default-features --locked --target aarch64-apple-ios  # build (iOS)
```
Android cross builds need the NDK clang (see `docs/development.md`). CI also runs zizmor (with online audits), actionlint (Docker), CodeQL and the experimental `mdns-e2e` job.
```sh
zizmor --offline .github/                                                        # local workflow audit
cargo test -p dropumi-cli --test e2e --locked                                    # e2e only
DROPUMI_TEST_MDNS=1 cargo test -p dropumi-cli --test e2e --all-features --locked -- --ignored mdns_
```

## 13. Definition of Done (master spec §16)
A feature is not done until:
1. the implementation actually exists;
2. it compiles on the intended target;
3. the appropriate tests pass;
4. failure conditions are handled;
5. the security requirements are met;
6. the performance impact has been considered;
7. the documentation is updated;
8. platform limitations are disclosed;
9. there are no misleading mocked success states;
10. there is reproducible evidence that it works.

## 14. Anti-patterns (never do these)
- Fake implementations or stubs that return success just to satisfy an interface.
- Hiding unfinished work behind successful-looking UI, output or exit codes.
- Claiming platform support that was not tested, or claiming cross-platform verification from one OS.
- Invented benchmark numbers, or estimates presented as measurements.
- Reporting tests as passed without running them, or paraphrasing output instead of pasting it.
- Sleep-based tests, retries to mask flakiness, or disabling tests or lints to get CI green.
- Plaintext fallbacks, auto-accepting unverified peers, or relaxing limits "temporarily".
- Unjustified dependencies, giant mixed PRs, drive-by refactors.
- Logging secrets or personal paths. Touching the user's real config dir in tests.

## 15. Milestone report template
```markdown
## Milestone report: <name>
- Branch / PR: <branch>; PR #<n>, or "no PR exists yet" plus the exact push and `gh pr create` commands
- Implemented: <features, crates, files>
- Actually runs: <what a user can do today, with commands>
- Tested: <exact commands and pasted summary lines>
- Platform status: <OS version, arch, environment: one of the five labels>
- Incomplete: <what remains>
- Known limitations: <...>
- Security concerns: <...>
- Performance: <measured numbers with method, or "not measured">
- Implemented vs compiled vs tested vs production-ready: <be explicit>
- Recommended next step: <...>
```
