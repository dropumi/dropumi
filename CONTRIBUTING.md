# Contributing to Dropumi

Thank you for helping build Dropumi, a free, open-source, local-only file-sharing tool. *Just drop it nearby.*

Before you start, please note:

- **Dropumi is pre-alpha.** The protocol and the code are unaudited, and many parts are still being designed. Small, focused contributions are the easiest to review.
- Everyone taking part must follow the [Code of Conduct](CODE_OF_CONDUCT.md).
- Security problems are never reported in public. See [SECURITY.md](SECURITY.md).

## Ways to contribute

- **Report bugs** with the [bug report form](https://github.com/dropumi/dropumi/issues/new/choose).
- **Suggest features** with the feature request form. Explain the problem, not only the solution.
- **Ask questions and discuss ideas** in [GitHub Discussions](https://github.com/dropumi/dropumi/discussions).
- **Test on real devices.** Report what you ran, on which OS version and architecture, and the result, using the status labels below.
- **Improve the documentation**: [`docs/`](docs/), the ADRs and the protocol draft.
- **Write tests**, especially adversarial tests and tests on platforms that have none yet.
- **Review pull requests**: careful reading of protocol, security and filesystem code is very valuable.
- **Write code.** For anything bigger than a small fix, open an issue or a discussion first so the design can be agreed before you spend time on it.

## Issue etiquette

- Search existing issues and discussions first.
- Use the issue forms; blank issues are disabled.
- Report one problem per issue.
- **Never attach personal files**, including files you transferred.
- **Never share keys or identity files**: `identity.key`, `trusted_devices.json` or `device.json`.
- **Never include full file-system paths, user names, host names, real device names or fingerprints, or IP or MAC addresses.** Redact logs and screenshots before posting.
- Reproduce problems with throwaway identities, for example `dropumi --config-dir "$(mktemp -d)" ...`, and synthetic test files.
- Security vulnerabilities go through [private vulnerability reporting](SECURITY.md), never through issues.

### Platform status labels

Use only these labels when you describe what you verified:

- `Automated test passed`
- `Manual device test passed`
- `Build verified only`
- `Not tested`
- `Unsupported`

Always name the OS version, the CPU architecture and how you tested. Never call a platform "supported": no platform is supported while Dropumi is pre-alpha.

## Development setup

**You need:**

- [rustup](https://rustup.rs/) with the **stable** toolchain;
- the **MSRV toolchain, Rust 1.89**.

The repository has no `rust-toolchain.toml`. The MSRV is declared as `rust-version` in [`Cargo.toml`](Cargo.toml).

```sh
rustup toolchain install stable --profile minimal -c clippy,rustfmt
rustup toolchain install 1.89.0 --profile minimal
```

**Optional tools** (CI runs them for you):

```sh
cargo install --locked cargo-deny@0.20.2   # licenses, advisories, bans, sources
cargo install --locked zizmor@1.30.1       # GitHub Actions security lint
# actionlint runs from a pinned Docker image; see docs/development.md
```

**Build and try the CLI.** Always use a temporary configuration directory, so that development never touches a real identity:

```sh
cargo build --locked
cargo run --locked -p dropumi-cli -- --config-dir "$(mktemp -d)" identity show
```

The `mdns` cargo feature (mDNS discovery) is on by default; `--no-default-features` builds without it.

[`docs/development.md`](docs/development.md) covers toolchains, cross builds and the CI tooling. [`docs/cli.md`](docs/cli.md) documents every command.

## Workflow

1. **Fork** the repository. Create a branch from an up-to-date `main` with one of these prefixes:

   `feat/`, `fix/`, `refactor/`, `docs/`, `test/`, `perf/`, `chore/`, `ci/`

   For example: `fix/finalize-sharing-violation`.

2. **Commit** in small, coherent steps using [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/): `type(scope): summary`.
   - **Types:** `feat`, `fix`, `refactor`, `docs`, `test`, `perf`, `chore`, `ci`.
   - **Scopes:**

     | Scope | Area |
     |---|---|
     | `domain` | `crates/domain`: models, invariants, policies, file-name safety |
     | `protocol` | `crates/protocol`: preamble, records and frames, CBOR codec, session machines |
     | `security` | handshake, record encryption, identity keys, SAS, pairing and trust rules |
     | `core` | `crates/application`: ports, secure channel, use cases |
     | `infra` | `crates/infra`: TCP, filesystem, stores, permissions |
     | `discovery` | mDNS advertisement and browsing |
     | `cli` | `crates/cli`: the `dropumi` binary and its end-to-end tests |
     | `ffi` | `crates/ffi`, `crates/ffi-bindgen`, `scripts/ffi/` and the binding packages: the app facade and its bindings |
     | `apple` | `apps/apple`: the iOS, iPadOS and macOS app |
     | `android` | `apps/android`: the Android app |
     | `web` | the browser companion and `poc/web/` (ADR-0011) |
     | `ci` | workflows, `deny.toml`, `scripts/ci/` |
     | `docs` | documentation |
     | `deps` | dependency updates |
     | `bench` | benchmarks and `scripts/bench/` |

   - Examples: `feat(cli): add --once to receive`, `fix(infra): retry finalization on sharing violations`, `docs(protocol): clarify OfferFiles batching`.
   - Mark breaking changes with `!`, for example `feat(protocol)!: ...`, or with a `BREAKING CHANGE:` footer.

3. **Run the checks** below before you push.

4. **Open a pull request against `main`.**
   - The PR title must be a Conventional Commit.
   - Fill in every section of the template. Paste the real commands you ran and their results.

5. **Get CI green and address review comments.** Resolve every conversation.

6. **The maintainers review and merge.** Do not merge your own pull request. Automated agents never merge.

Changes to the wire protocol, the cryptography, the trust policy or the dependency direction also need:

- an update to [`docs/protocol/v1.md`](docs/protocol/v1.md) and [`docs/protocol/threat-model.md`](docs/protocol/threat-model.md);
- a new or updated ADR in [`docs/adr/`](docs/adr/).

Discuss such changes in an issue first.

## Checks to run before pushing

These commands mirror the jobs in [`.github/workflows/ci.yml`](.github/workflows/ci.yml). The CI job name is in each comment.

```sh
cargo fmt --all --check                                                            # fmt
git ls-files -z -- '*.rs' | xargs -0 awk 'FNR == 1 && $0 != "// SPDX-License-Identifier: MPL-2.0" { print FILENAME; bad = 1 } END { exit bad }'   # fmt (SPDX header on line 1)
git ls-files -z -- '*.toml' '*.yml' '*.yaml' '*.sh' ':!*pnpm-lock.yaml' | xargs -0 awk 'FNR == 1 && /^#!/ { next } FNR <= 2 { if ($0 != "# SPDX-License-Identifier: MPL-2.0") { print FILENAME; bad = 1 } nextfile } END { exit bad }'   # fmt (TOML, YAML, shell)
# fmt also checks Swift, Kotlin and .kts files (line 2 of Package.swift; generated bindings exempt); see ci.yml
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings      # clippy
cargo clippy --workspace --all-targets --no-default-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked  # docs
cargo test --workspace --all-features --locked                                     # test (<os>)
cargo +1.89.0 check --workspace --all-targets --all-features --locked              # msrv
bash scripts/ci/check-architecture.sh                                              # architecture
cargo deny --locked check                                                          # cargo-deny
```

**CI also runs:**

| Job | What it does |
|---|---|
| `test (ubuntu-latest)`, `test (macos-latest)`, `test (windows-latest)` | The full test suite on each OS |
| `build (aarch64-linux-android)`, `build (aarch64-apple-ios)` | Builds the core library crates only (`--exclude dropumi-cli --exclude dropumi-ffi --exclude dropumi-ffi-bindgen --no-default-features`). Nothing is linked into an app or run. |
| `ffi (apple)` | macOS: the committed bindings are up to date, the XCFramework builds (iOS device, iOS simulator, macOS), and `swift test` runs the Swift binding tests. Not required. |
| `ffi (android)` | Linux: `libdropumi_ffi.so` builds for arm64-v8a and x86_64 with 16 KB pages, and Gradle runs the Kotlin binding tests on the host JVM. Not required. |
| `cargo-deny (advisories)`, `cargo-deny (bans licenses sources)` | The cargo-deny checks, split into two jobs |
| `zizmor`, `actionlint` | Workflow security and syntax checks (locally: `zizmor --offline .github/`; actionlint runs from a pinned Docker image) |
| `mdns-e2e (experimental)` | The opt-in mDNS end-to-end test; allowed to fail and never required |
| `CodeQL (actions)`, `CodeQL (rust)` | CodeQL analysis (`codeql.yml`) |
| `scheduled advisories` | A daily RustSec advisory scan (`audit.yml`); never required |

`cargo test` includes the two-process end-to-end suite (`crates/cli/tests/e2e`). It runs on loopback only, with manual addresses. Opt-in variables are described in [`docs/testing.md`](docs/testing.md):

- `DROPUMI_TEST_MDNS=1` enables the ignored mDNS test: `DROPUMI_TEST_MDNS=1 cargo test -p dropumi-cli --test e2e --all-features --locked -- --ignored mdns_`;
- `DROPUMI_TEST_LARGE_MIB` sets the size of the large-file test (default 64 MiB).

## Coding rules

The full rules are in [AGENTS.md](AGENTS.md) and [ARCHITECTURE.md](ARCHITECTURE.md). The short version:

- **License header.** The first line of every `.rs` file is:

  ```rust
  // SPDX-License-Identifier: MPL-2.0
  ```

  CI fails without it. TOML, YAML and shell files start with `# SPDX-License-Identifier: MPL-2.0` (shell scripts on line 2, after the shebang; generated `pnpm-lock.yaml` files are exempt); CI checks them too.
- **Dependencies point inward:** `dropumi-domain` ← `dropumi-protocol` ← `dropumi-application` ← `dropumi-infra` ← `dropumi-cli`. `scripts/ci/check-architecture.sh` enforces this.
- **Unsafe code is forbidden.** `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, `dbg!` and `mem::forget` are denied outside tests. Use typed errors with `thiserror`.
- **Never weaken a security check to make a test pass.**
- **Tests:**
  - every behaviour change comes with tests, and every bug fix with a regression test;
  - domain and protocol code is developed test-first;
  - tests never use `sleep` for synchronization: they wait for events or exits with a deadline (see [`docs/testing.md`](docs/testing.md#end-to-end-tests-two-real-processes)).
- **Changelog:** update [`CHANGELOG.md`](CHANGELOG.md) under `[Unreleased]` for user-visible changes.

## Dependency policy

Every new dependency is a cost: binary size, build time, audit surface and license obligations.

- **Justify it** in the pull request: why it is needed, what alternatives you considered, and its license. Add a row to [`docs/dependencies.md`](docs/dependencies.md).
- **It must pass `cargo deny --locked check`.** The license allow-list and the bans are in [`deny.toml`](deny.toml).
  - `openssl`, `openssl-sys`, `native-tls` and `ring` are banned.
- **Prefer** small, maintained, pure-Rust crates with permissive licenses.
- **Lockfile:** add the crate to `[workspace.dependencies]` in the same change that first uses it. Commit `Cargo.lock`, and use `--locked` everywhere.
- **Pinned majors:** `getrandom` 0.3 and `sha2` 0.10 follow `snow`. Do not bump them on their own.

## Licensing of contributions

Dropumi is licensed under the [Mozilla Public License 2.0](LICENSE).

- **Inbound = outbound.** By submitting a contribution, you license it under MPL-2.0, as described in [section D.6 of the GitHub Terms of Service](https://docs.github.com/en/site-policy/github-terms/github-terms-of-service#6-contributions-under-repository-license).
- **No CLA, no DCO.** There is no Contributor License Agreement and no Developer Certificate of Origin requirement. A `Signed-off-by` line is welcome but optional.
- **Only submit work you have the right to contribute.**
- **Do not copy code from other projects,** including LocalSend and PairDrop. PairDrop is GPL-3.0, which is not compatible with this project's licensing. If you think third-party code must be included, open an issue first: it needs a compatible license and proper attribution.

## AI-assisted contributions

You may use AI tools. You remain responsible for everything you submit.

- If AI tools produced a significant part of a change (code, tests or documentation), say so in the pull request description.
- Review and understand every line.
- Run the checks yourself.
- Never submit test results, benchmark numbers or platform claims that you did not produce by actually running them.

## Maintainers

- [@fahmialfareza](https://github.com/fahmialfareza) (Fahmi Alfareza)
- [@santoadji21](https://github.com/santoadji21) (Ahmad Aji Santoso)

Maintainers review and merge pull requests. Automated agents never merge.

## Code of Conduct

This project follows the [Contributor Covenant 3.0](CODE_OF_CONDUCT.md). [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) explains how to report a problem.
