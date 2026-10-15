# ADR-0010: Dependency, license and CI supply-chain policy

- **Status:** Accepted
- **Date:** 2026-10-13 (reconciled with the milestone 1 code and configuration on 2026-10-13)
- **Related:** ADR-0001, ADR-0002, ADR-0008, [`../dependencies.md`](../dependencies.md), [`../development.md`](../development.md), [`deny.toml`](../../deny.toml), [`.github/workflows/`](../../.github/workflows/)

## Context

- Dropumi is licensed under MPL-2.0. The specification requires auditing dependency licenses before adding them, preferring mature, maintained, permissively licensed dependencies, documenting third-party attribution, and never copying code from LocalSend, PairDrop or other projects unless their licenses permit it and attribution is given.
- It also requires CI without credentials or paid services; read-only `GITHUB_TOKEN` by default; no secrets for fork PRs; no privileged `pull_request_target` running untrusted code; third-party actions pinned to commit SHAs with a documented update strategy; required status checks that match real job names; and release/signing workflows kept separate from PR CI.
- `minicbor` (ADR-0005) is licensed `BlueOak-1.0.0`, which is outside the most common permissive allow-lists.
- snow 0.10 requires `getrandom` 0.3 and `sha2` 0.10 (ADR-0002); newer majors would duplicate them in the graph.

## Decision

**Project licensing.**
- Every crate declares `license = "MPL-2.0"` and `publish = false` (inherited from `[workspace.package]`).
- Line 1 of every `.rs` file is `// SPDX-License-Identifier: MPL-2.0`. The `fmt` CI job fails unless line 1 of every tracked `.rs` file is exactly that. TOML (including every `Cargo.toml` and `clippy.toml`), YAML and shell files start with `# SPDX-License-Identifier: MPL-2.0` (shell scripts on line 2, after the shebang; generated `pnpm-lock.yaml` files are exempt); a second step of the `fmt` job checks them. Every workflow in `.github/workflows/`, including `deploy-site.yml` for the website, pins its actions by full SHA, keeps `GITHUB_TOKEN` permissions to what each job needs and does not persist checkout credentials; zizmor and actionlint check all of them. The manifests also declare `license = "MPL-2.0"`.
- Contributions are accepted under the project license (inbound = outbound); there is no CLA and no DCO sign-off.
- No code is copied from PairDrop (GPL-3.0) or LocalSend. Their public documentation may be read for comparison only.

**cargo-deny 0.20.2 (`deny.toml`).**
- **License allow-list:** `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `BSD-3-Clause`, `BlueOak-1.0.0` (for minicbor), `BSL-1.0`, `CC0-1.0`, `ISC`, `MIT`, `MPL-2.0`, `Unicode-3.0`, `Zlib`. `confidence-threshold = 0.93`, no exceptions, `unused-allowed-license = "allow"`, and workspace crates are checked too (`[licenses.private] ignore = false`). Any other license (for example GPL, LGPL, `CDLA-Permissive-2.0`, `MIT-0`, `Unlicense` or any `LicenseRef-*`) needs a new ADR before it may be the only license option of a dependency.
- **Bans:** `openssl`, `openssl-sys`, `native-tls` (pure-Rust crypto only) and `ring` (snow is built without it; ADR-0002). Wildcard versions are denied (path dependencies excepted); duplicate or unused `[workspace.dependencies]` entries are denied; multiple versions of one crate and std-replacement crates produce warnings.
- **Advisories:** yanked crates denied; unmaintained crates checked for direct workspace dependencies; unsound crates checked everywhere; the ignore list is empty, and any future entry needs an advisory id, a reason and a tracking issue.
- **Sources:** crates.io only; unknown registries and git sources are denied.
- **Graph:** `x86_64`/`aarch64` Linux (gnu), macOS and Windows (msvc), `aarch64`/`x86_64` Android, `aarch64-apple-ios` and `aarch64-apple-ios-sim`, with all features enabled.

**Licenses actually in the graph** (from `cargo deny list` on the maintainer's machine):

| License | Crates | Note |
|---|---|---|
| MIT | 90 | |
| Apache-2.0 | 75 | |
| MPL-2.0 | 6 | the five `dropumi-*` crates and `option-ext` (via `dirs`) |
| Apache-2.0 WITH LLVM-exception | 3 | `blake3`, `linux-raw-sys`, `rustix` (each also offers other licenses) |
| BSD-3-Clause | 3 | `curve25519-dalek`, `subtle`, `if-addrs` (the last also MIT) |
| CC0-1.0 | 2 | `blake3`, `constant_time_eq` (both also Apache-2.0) |
| BlueOak-1.0.0 | 1 | `minicbor` (only license) |
| Unicode-3.0 | 1 | `unicode-ident` (`(MIT OR Apache-2.0) AND Unicode-3.0`) |
| Zlib | 1 | `tinyvec` (`Zlib OR Apache-2.0 OR MIT`) |
| MIT-0 | 1 | `constant_time_eq` (`CC0-1.0 OR MIT-0 OR Apache-2.0`): only an OR alternative, not on the allow-list |
| Unlicense | 1 | `memchr` (`Unlicense OR MIT`): only an OR alternative, not on the allow-list |

A crate counts once per license in its expression. `BSD-2-Clause`, `BSL-1.0` and `ISC` are allowed but not used today.

**Adding a dependency.** A new direct dependency needs a row in `docs/dependencies.md` (crate, version, license, purpose, justification), must pass `cargo deny --locked check`, and is added to `[workspace.dependencies]` in the commit that first uses it. Prefer small, maintained, pure-Rust crates and disable default features that are not needed. `Cargo.lock` is committed and every command uses `--locked`.

**Pinned majors.** The direct `getrandom` 0.3 and `sha2` 0.10 dependencies follow snow. Dependabot ignores `getrandom >= 0.4.0` and `sha2 >= 0.11.0` through explicit version ranges. `getrandom` 0.4.3 is nevertheless in the graph through `tempfile` 3.27.0, which cargo-deny reports as a duplicate-version warning.

**Third-party notices.** No binaries are released yet. Before the first binary release, generated third-party notices (planned: `cargo-about`) must be bundled, covering the MIT, BSD-3-Clause, Apache-2.0 (including any `NOTICE` files), BlueOak-1.0.0 and MPL-2.0 (`option-ext`) requirements; Zlib, CC0-1.0, MIT-0 and Unlicense appear only as OR alternatives, and the Unicode-3.0 crate (`unicode-ident`) is a build-time dependency of proc macros ([`dependencies.md`](../dependencies.md#third-party-notices)).

**CI hardening** (`ci.yml`, `codeql.yml`, `audit.yml`).
- Workflow-level `permissions: contents: read`; only the CodeQL job adds `security-events: write`. No `pull_request_target`, no repository secrets, no paid services. The `zizmor` job passes the automatic, read-only `github.token` to zizmor for its online audits.
- Every `actions/checkout` sets `persist-credentials: false`.
- Every action is pinned to a full commit SHA with a version comment:
  - `actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1`
  - `Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2`
  - `taiki-e/install-action@1168f6810a3cb30a3514bf5756a3b7f0422c6df8 # v2.87.28`
  - `github/codeql-action/{init,analyze}@24c54180a607b1449ed407dd24f251e4e9147c8d # v4.38.3`
  - actionlint runs from the Docker image `rhysd/actionlint:1.7.12` pinned by its `sha256` digest.
- Rust is installed with the runner's preinstalled `rustup` (`--no-self-update`) rather than a third-party toolchain action. cargo-deny 0.20.2 and zizmor 1.30.1 are installed through install-action with `fallback: none` (checksummed downloads).
- Workflows are linted by zizmor and actionlint. CodeQL runs in advanced setup for `actions` and `rust` (`build-mode: none`, `security-extended`) on pull requests, pushes to `main` and weekly; GitHub's CodeQL default setup must stay off.
- No `rust-toolchain.toml`: CI tracks stable, and the `msrv` job checks the manifest's `rust-version`.
- A daily scheduled run (`audit.yml`, job `scheduled advisories`, 05:23 UTC) checks advisories between pull requests.

**Dependabot.** Ecosystems `cargo` and `github-actions`, weekly on Monday 05:00 UTC, grouped updates, a 7-day cooldown (14 days for cargo semver-major updates), at most 5 open cargo PRs, Conventional Commit prefixes `chore` and `ci`. The actionlint image digest is updated by hand, following `docs/development.md`.

**Required status checks.** Added to the `main` ruleset by a maintainer **only after** they are green on the first pull request, with names exactly matching the jobs: `fmt`, `clippy`, `docs`, `test (ubuntu-latest)`, `test (macos-latest)`, `test (windows-latest)`, `msrv`, `architecture`, `build (aarch64-linux-android)`, `build (aarch64-apple-ios)`, `cargo-deny (advisories)`, `cargo-deny (bans licenses sources)`, `zizmor`, `actionlint`, `CodeQL (actions)`, `CodeQL (rust)`. `mdns-e2e (experimental)` and `scheduled advisories` are never required.

**Releases.** A future release workflow is separate from PR CI and is the only place signing secrets may exist.

## Implementation

- Configuration: [`deny.toml`](../../deny.toml), [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml), [`codeql.yml`](../../.github/workflows/codeql.yml), [`audit.yml`](../../.github/workflows/audit.yml), [`.github/dependabot.yml`](../../.github/dependabot.yml), [`scripts/ci/check-architecture.sh`](../../scripts/ci/check-architecture.sh).
- Status:

| Check | Result |
|---|---|
| `cargo deny --locked check` (advisories, bans, licenses, sources) | Passed locally; only duplicate-version warnings for `cpufeatures`, `getrandom` and `syn` |
| `zizmor --offline .github/` | No findings (local run) |
| actionlint 1.7.12 | No findings (local run) |
| `bash scripts/ci/check-architecture.sh` | Passed (local run) |
| Every CI job on GitHub-hosted runners | Not tested (pending CI): the maintainer pushes the branch; no workflow has run yet |
| Required status checks in the `main` ruleset | Not enabled yet, by design (enable only after the first green run) |

## Consequences

### Positive

- License compatibility and banned crates are enforced mechanically on every pull request.
- Supply-chain changes are reviewable: SHA-pinned actions, checksummed tool downloads, cooldowns and grouped updates.
- Required checks never reference jobs that do not exist or have never passed.
- The policy was written down before the first dependency landed, so later additions follow one process.

### Negative

- SHA pins and the actionlint digest need regular upkeep.
- The allow-list is deliberately broader than today's graph; reviewers still need to read `docs/dependencies.md` for each addition.
- How Dependabot classifies `0.x` to `0.(x+1)` bumps is UNVERIFIED; the explicit version ranges work either way.
- Whether `cargo-about` reproduces Apache `NOTICE` files is UNVERIFIED and must be checked before the first release.
- Holding `getrandom` and `sha2` on older majors depends on snow's release cadence, and the graph already carries two `getrandom` majors.

## Alternatives considered

- **`EmbarkStudios/cargo-deny-action`:** rebuilds a container on every run and downloads cargo-deny without checksum verification.
- **`dtolnay/rust-toolchain`:** flagged by zizmor's `superfluous-actions` audit; `rustup` is already on every runner.
- **Tag-pinned actions:** tags are mutable.
- **cargo-audit only:** covers advisories but not licenses, bans or sources.
- **CodeQL default setup:** not versioned in the repository and conflicts with advanced setup.
- **Requiring checks before the first green run:** would block every PR on checks that never ran.
- **A CLA or DCO sign-off:** inbound = outbound under MPL-2.0 is sufficient for this project.
