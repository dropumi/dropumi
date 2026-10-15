# Development guide

How to build Dropumi, run the same checks as CI, cross-build the core for mobile targets, maintain the CI pins, and
troubleshoot common problems. Contribution workflow and rules: [`CONTRIBUTING.md`](../CONTRIBUTING.md) and
[`AGENTS.md`](../AGENTS.md). Test details: [`testing.md`](testing.md).

## Toolchains and tools

| Tool | Version | Why |
|---|---|---|
| Rust stable | latest stable (CI installs it with `rustup`); the recorded local run used 1.99.0 | Build, test, clippy, rustfmt, rustdoc |
| Rust MSRV | 1.89 (`rust-version` in [`Cargo.toml`](../Cargo.toml); there is no `rust-toolchain.toml`) | `cargo check` in the `msrv` job. 1.89 is needed for `std::fs::File::lock`. |
| `cargo-deny` | 0.20.2 | Licenses, advisories, bans, sources ([`deny.toml`](../deny.toml)) |
| `zizmor` | 1.30.1 | GitHub Actions security lint |
| `actionlint` | 1.7.12, from a pinned Docker image | Workflow lint (includes shellcheck for `run:` steps) |
| Android NDK | one that provides `aarch64-linux-android26-clang`; the recorded local build used 27.1 | Android cross build (blake3 compiles C for the target) |
| Xcode | with the iOS SDK; the recorded local build used Xcode 27 (Swift 6.4) | iOS cross build; the XCFramework and `swift test` for the FFI; the Apple apps (Xcode 27, iOS 27.0 simulator runtime) |
| `xcodegen` | 2.46.0 (CI downloads the release and checks its SHA-256) | Generates `apps/apple/Dropumi.xcodeproj` from `apps/apple/project.yml` |
| `rsvg-convert` (librsvg) | any | Only to re-render the Mac app icon (`scripts/apple/render-mac-icon.sh`) |
| JDK | 17 or later for Gradle; the recorded run used Temurin 21 | The Kotlin binding tests (`apps/android`) |
| Android SDK | platform 36 and NDK 27.1.12297006 (`ANDROID_HOME`, or `sdk.dir` in the untracked `apps/android/local.properties`) | The Android library and the Kotlin binding tests |
| `jq`, `perl`, `/usr/bin/time` | system | Benchmark script only |

```sh
rustup toolchain install stable --profile minimal -c clippy,rustfmt
rustup toolchain install 1.89.0 --profile minimal
cargo install --locked cargo-deny@0.20.2 zizmor@1.30.1
```

## Build and run

```sh
cargo build --locked                                  # debug build of the workspace
cargo build --release --locked -p dropumi-cli         # target/release/dropumi
cargo run --locked -p dropumi-cli -- --config-dir "$(mktemp -d)" identity show
```

**Always use a temporary configuration directory** (`--config-dir "$(mktemp -d)"` or `DROPUMI_CONFIG_DIR`) when
experimenting, so development never touches a real identity or trust store. `mktemp -d` creates the directory with
mode 0700, which passes the permission checks.

Cargo features: `dropumi-cli` and `dropumi-infra` have one feature, `mdns` (default on). Build without it with
`--no-default-features`; then `discover` and name or fingerprint targets exit 2.

## Checks (mirror of CI)

Every job of [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) and the command to run it locally:

| CI job | Local command |
|---|---|
| `fmt` | `cargo fmt --all --check`, then the SPDX check below |
| `clippy` | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` and `cargo clippy --workspace --all-targets --no-default-features --locked -- -D warnings` |
| `docs` | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked` |
| `test (ubuntu-latest / macos-latest / windows-latest)` | `cargo test --workspace --all-features --locked` |
| `msrv` | `cargo +1.89.0 check --workspace --all-targets --all-features --locked` |
| `architecture` | `bash scripts/ci/check-architecture.sh` |
| `build (aarch64-linux-android)`, `build (aarch64-apple-ios)` | See [cross builds](#cross-builds-android-and-ios) |
| `cargo-deny (advisories)`, `cargo-deny (bans licenses sources)` | `cargo deny --locked check` (all four checks) |
| `zizmor` | `zizmor --offline .` (CI runs `zizmor --format=github .` online, with the job's read-only token) |
| `actionlint` | See [actionlint](#actionlint) |
| `mdns-e2e (experimental)` | `DROPUMI_TEST_MDNS=1 cargo test -p dropumi-cli --test e2e --all-features --locked -- --ignored mdns_` |

Other workflows: [`codeql.yml`](../.github/workflows/codeql.yml) (`CodeQL (actions)`, `CodeQL (rust)`, build mode
none, `security-extended` queries) and [`audit.yml`](../.github/workflows/audit.yml) (`scheduled advisories`, daily
`cargo deny --locked check advisories`). Neither has a local equivalent beyond `cargo deny`.

**SPDX header check.** CI requires the first line of every tracked `.rs` file to be exactly
`// SPDX-License-Identifier: MPL-2.0`. This prints offending files and nothing when all is well:

```sh
git ls-files -z -- '*.rs' | while IFS= read -r -d '' f; do
  [ "$(head -n 1 "$f")" = "// SPDX-License-Identifier: MPL-2.0" ] || echo "$f"
done
```

TOML, YAML and shell files start with `# SPDX-License-Identifier: MPL-2.0` (shell scripts on line 2, after the
shebang; generated `pnpm-lock.yaml` files are exempt; [`AGENTS.md`](../AGENTS.md) §11). The `fmt` job checks them with a second step; locally:

```sh
git ls-files -z -- '*.toml' '*.yml' '*.yaml' '*.sh' ':!*pnpm-lock.yaml' | xargs -0 awk 'FNR == 1 && /^#!/ { next } FNR <= 2 { if ($0 != "# SPDX-License-Identifier: MPL-2.0") { print FILENAME; bad = 1 } nextfile } END { exit bad }'
```

**Architecture guard.** `scripts/ci/check-architecture.sh` exits 0 when clean, 1 on a violation and 2 when it cannot
read the dependency graph. The deny-lists are summarised in [`ARCHITECTURE.md`](../ARCHITECTURE.md#enforcement).

**Fast loops.**

```sh
cargo test -p dropumi-domain --locked
cargo test -p dropumi-protocol --locked
cargo test -p dropumi-application --locked
cargo test -p dropumi-cli --test e2e --locked              # two-process suite only
cargo test -p dropumi-cli --test e2e --locked -- security::   # one e2e module
```

## Cross builds (Android and iOS)

The `build (<target>)` CI jobs build only the core library crates for mobile targets: no CLI, no FFI, no mDNS,
nothing linked into an app or run. The result label is "Build verified only (core library crates; no app, not run)".
The FFI for the apps has its own scripts ([below](#the-ffi-and-the-binding-packages)).

```sh
cargo build --workspace --exclude dropumi-cli --exclude dropumi-ffi --exclude dropumi-ffi-bindgen \
  --no-default-features --locked --target <TARGET>
```

**iOS** (macOS host with Xcode and the iOS SDK):

```sh
rustup target add aarch64-apple-ios
cargo build --workspace --exclude dropumi-cli --exclude dropumi-ffi --exclude dropumi-ffi-bindgen \
  --no-default-features --locked --target aarch64-apple-ios
```

**Android.** `blake3` compiles C code for the target, so Cargo needs the NDK's clang and `llvm-ar`. Replace
`/path/to/android-ndk` with your NDK directory. The prebuilt host directory is `darwin-x86_64` on macOS (also on
Apple silicon) and `linux-x86_64` on Linux.

```sh
rustup target add aarch64-linux-android
export ANDROID_NDK_HOME=/path/to/android-ndk
ndk_bin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"
export CC_aarch64_linux_android="$ndk_bin/aarch64-linux-android26-clang"
export AR_aarch64_linux_android="$ndk_bin/llvm-ar"
cargo build --workspace --exclude dropumi-cli --exclude dropumi-ffi --exclude dropumi-ffi-bindgen \
  --no-default-features --locked --target aarch64-linux-android
```

CI does the same on `ubuntu-latest` with the runner's preinstalled NDK (`ANDROID_NDK_HOME`, API level 26) and prints
the NDK revision in the job log.

## The FFI and the binding packages

`dropumi-ffi` is the facade the apps use ([ADR-0009](adr/0009-ffi-bindings-strategy.md)). Its bindings are generated
and committed; the native libraries are built by scripts and never committed.

```sh
# Regenerate the Swift and Kotlin bindings after changing the FFI (CI fails if they drift).
bash scripts/ffi/bindings.sh            # --check only compares

# Apple: DropumiFFI.xcframework (iOS device, iOS simulator, macOS; arm64), then the Swift binding tests.
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
bash scripts/ffi/build-apple.sh --profile dev          # --profile release for app builds; --slices ci skips the device
swift test --package-path apps/apple/DropumiKit

# Android: libdropumi_ffi.so for the app's jniLibs, with 16 KB pages, then the check.
rustup target add aarch64-linux-android               # and x86_64-linux-android for the emulator
bash scripts/ffi/build-android.sh --abis arm64-v8a    # finds the SDK and NDK 27.1.12297006 itself
bash scripts/ffi/check-16k.sh apps/android/core-bindings/build/rust/jniLibs

# Kotlin binding tests on the host JVM (they load target/debug/libdropumi_ffi).
cargo build --locked -p dropumi-ffi --lib
JAVA_HOME="$(/usr/libexec/java_home -v 21)" ./apps/android/gradlew -p apps/android :core-bindings:testDebugUnitTest
```

- `apps/apple/DropumiKit` declares iOS 17 and macOS 14 as its minimums so CI can test it on older runners
  (`scripts/ffi/apple.env` sets the same for the Rust slices); the apps set their own deployment targets.
- The Gradle wrapper pins Gradle 9.8.1 with a checksum; dependency versions are in
  `apps/android/gradle/libs.versions.toml`.
- Disk: a release build of every slice and ABI, Xcode's and Gradle's caches add several GiB. Rebuildable output is in
  `target/`, `apps/apple/DropumiKit/.build/`, `apps/apple/build/` and `apps/android/**/build/`.

## The Apple apps

```sh
bash scripts/ffi/build-apple.sh --profile dev           # the core; the app build refuses a stale one
(cd apps/apple && xcodegen generate)                     # Dropumi.xcodeproj (not committed)
xcodebuild build -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-iOS \
  -destination 'platform=iOS Simulator,name=iPhone 17,OS=27.0' -derivedDataPath apps/apple/build
xcodebuild build -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-macOS \
  -destination 'platform=macOS,arch=arm64' -derivedDataPath apps/apple/build
```

- **Core check.** A build phase runs `scripts/apple/check-core-artifacts.sh`. It compares the XCFramework's stamp with `scripts/ffi/core-sources-hash.sh`, which hashes the sources of the crates `dropumi-ffi` links. It never runs cargo.
- **Signing.**
  - Signing is automatic. Copy `apps/apple/Config/Signing.local.xcconfig.example` to `Signing.local.xcconfig` (gitignored) and set your team ID.
  - Installing on your own devices is described in [`apps/apple/README.md`](../apps/apple/README.md).
- **UI-test scenarios.**
  - `-dropumi-ui-test -dropumi-scenario <name>` launches a Debug build on the fake core with temporary folders. The names are in `apps/apple/Dropumi/Shared/App/UITestScenarios.swift`.
  - Release builds exclude that file and the fake core (`Config/Release.xcconfig`).
- **The Mac app icon** is rendered from `apps/apple/Design/macos-app-icon.svg`; the iPhone and iPad icons are `branding/mobile/ios`.

## Linux tests and a Windows type check from another OS (Docker)

Code behind `cfg(target_os = "linux")` or `cfg(windows)` does not compile on macOS, so check it before pushing. Run
the full test suite on Linux as a non-root user (as on a CI runner); the build output goes to a host directory you
own (not a shared directory such as `/tmp`), and the source is mounted read-only:

```sh
d="${XDG_CACHE_HOME:-$HOME/.cache}/dropumi-docker"; mkdir -p "$d/target" "$d/cargo"
docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/target \
  -v "$PWD:/repo:ro" -v "$d/cargo:/cargo-home" -v "$d/target:/target" -w /repo \
  rust:1.99-slim-bookworm cargo test --workspace --all-features --locked --no-fail-fast
```

Lint for Linux and for the Windows target. Adding clippy, the Windows target and MinGW needs root, so the source is
mounted read-only and the build output stays in the container:

```sh
docker run --rm -e CARGO_TARGET_DIR=/build -v "$PWD:/repo:ro" -w /repo rust:1.99-slim-bookworm bash -c '
  apt-get update -qq && apt-get install -y -qq gcc-mingw-w64-x86-64 >/dev/null
  rustup component add clippy && rustup target add x86_64-pc-windows-gnu
  for t in "" "--target x86_64-pc-windows-gnu"; do for f in --all-features --no-default-features; do
    cargo clippy --workspace --all-targets $f --locked $t -- -D warnings || exit 1
  done; done
  cargo build --workspace --all-targets --all-features --locked --target x86_64-pc-windows-gnu'
```

The Windows check compiles and links only; Windows behaviour is tested by the `test (windows-latest)` CI job.

## Supply chain

### cargo-deny

```sh
cargo deny --locked check                                # advisories, bans, licenses, sources
cargo deny --locked check advisories                     # what the daily audit runs
cargo deny --locked list                                 # licenses of the dependency graph
```

`multiple-versions` is a warning, not an error. The duplicates it currently reports, and why, are listed in
[`dependencies.md`](dependencies.md#duplicate-versions).

### zizmor

```sh
zizmor --offline .
```

The online audits (for example impostor-commit and known-vulnerable-actions) need a GitHub token and run in CI.

### actionlint

CI runs actionlint from a Docker image pinned by digest. Locally, from the repository root:

```sh
docker run --rm --volume "$PWD:/repo" --workdir /repo \
  rhysd/actionlint:1.7.12@sha256:b1934ee5f1c509618f2508e6eb47ee0d3520686341fec936f3b79331f9315667 -color
```

## Updating pinned actions and images

Every third-party action in `.github/workflows/` is pinned by full commit SHA with a `# vX.Y.Z` comment, and every
checkout sets `persist-credentials: false`. Dependabot proposes action updates weekly
([`.github/dependabot.yml`](../.github/dependabot.yml)). To update a pin by hand:

1. **Resolve the tag to a commit.** Annotated tags point to a tag object, not a commit; pin the commit.

   ```sh
   git ls-remote https://github.com/Swatinem/rust-cache 'refs/tags/v2.9.2' 'refs/tags/v2.9.2^{}'
   ```

   If two lines are printed, the tag is annotated: use the SHA on the `^{}` line. If one line is printed, the tag is
   lightweight and that SHA is the commit.
2. **Replace** every `uses: owner/repo@<sha> # vX.Y.Z` occurrence (the same action may appear in several jobs and
   workflows) and update the comment.
3. **Check:** run `zizmor --offline .` and actionlint locally; CI's online zizmor run also checks for impostor
   commits and tag/comment mismatches.

**actionlint image digest** (not tracked by Dependabot; update by hand):

```sh
docker buildx imagetools inspect rhysd/actionlint:<version>   # the "Digest:" line is the index digest
```

Replace both the tag and the `@sha256:` digest in the `actionlint` job of `ci.yml`, and in the command above.

Tool versions installed by `taiki-e/install-action` (`cargo-deny@0.20.2`, `zizmor@1.30.1`) are pinned in the
workflow files; update them there and in this guide together.

## Benchmarks

```sh
bash scripts/bench/loopback.sh                      # defaults: 1024 MiB file, 1000 x 100 KiB, 3 runs
bash scripts/bench/loopback.sh --size-mib 256 --mdns
```

The script builds the release binary, uses `mktemp -d` configuration directories only, and writes `summary.md`,
`transfers.tsv` and `startup.tsv` to `bench-out/` (ignored by git). It measures loopback on one machine, not a LAN.
Publish only numbers it produced, with the machine, OS, Rust version and parameters next to them.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| Exit 9, "insecure permissions on ..." | The configuration directory or a file in it is writable (or, for `identity.key`, readable) by group or others, owned by another user, or a symlink. Run the `chmod 700` / `chmod 600` command printed in the message, or use a fresh `mktemp -d` directory. |
| Exit 9, "no receive directory" | `receive` never creates directories. Create one and pass `--dir`, or set `DROPUMI_RECEIVE_DIR`. |
| The receiver is not on port 47321 | Without `--port`, a busy default port falls back to a free port. Read `port` from the `listening` event, or pass `--port`. |
| `--to <name>` says "no nearby device matches" | mDNS is blocked (multicast filtering, VPN, firewall, client isolation), the receiver runs with `--no-mdns`, or the name is classified as a fingerprint prefix (see [`cli.md`](cli.md#target-syntax)). Use `--to <ip>:<port>`. |
| "this build has no mDNS discovery" | Built with `--no-default-features`. Use an address target or rebuild with the default features. |
| "--no-mdns disables discovery, so name and fingerprint targets cannot be found" | `pair --to <name or fingerprint>` was combined with `--no-mdns`. Drop `--no-mdns` or use an address target. |
| Firewall prompt or blocked connection | A host firewall may block or ask about incoming TCP connections to `receive` / `pair --listen` (port 47321 by default) and about mDNS (UDP 5353). How each OS firewall reacts to the CLI is UNVERIFIED and Not tested. Allow the binary, or test with `--bind 127.0.0.1` on one machine. |
| macOS: LAN transfers fail while loopback works | macOS 15 and later gate local network access behind the Local Network privacy permission (System Settings > Privacy & Security > Local Network). Apple's TN3179 documents that command-line tools started from Terminal or SSH are exempt ([`feasibility.md`](feasibility.md) §7); this is Not tested with Dropumi on a real LAN, and whether loopback is exempt is UNVERIFIED. Check whether your terminal app is listed and allowed. |
| Testing on one machine | Use loopback: `receive --bind 127.0.0.1 --port 0 --no-mdns`, read the port from `listening`, then `send --to 127.0.0.1:<port>`. mDNS on loopback needs the hidden `--mdns-loopback` testing hook. |
| Android build fails compiling `blake3` | `CC_aarch64_linux_android` / `AR_aarch64_linux_android` are not set to the NDK's clang and `llvm-ar`. |
| A large e2e test needs too much disk | `large_file_streaming` writes 64 MiB by default on each side; lower `DROPUMI_TEST_LARGE_MIB`. |
| `swift test` cannot find `DropumiFFI` or reports a stale library | Run `bash scripts/ffi/build-apple.sh` first; `apps/apple/DropumiKit/Artifacts/core-stamp.txt` records what it was built from. |
| The Kotlin tests fail with `UnsatisfiedLinkError` | Build the host library first (`cargo build --locked -p dropumi-ffi --lib`); the tests load it from `target/debug` (`dropumi.hostLibDir` in `apps/android/gradle.properties`). |
| `check-16k.sh` reports alignment below 0x4000 | The library was built without the 16 KB link flags; build it with `scripts/ffi/build-android.sh`, not plain `cargo build`. |
| `check-16k.sh` reports `no .so files found` | The directory holds no library (a directory without one fails rather than passing unchecked); run `scripts/ffi/build-android.sh` first, or pass the directory given to its `--out`. |

## Building and releasing

- **No releases exist yet.** There are no tags, published binaries or crates (`publish = false`). Build from source:
  `cargo build --release --locked -p dropumi-cli`.
- **Release profile:** `lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`, and `panic = "unwind"` kept on
  purpose (temporary-file cleanup relies on `Drop`). The stripped binary measured 2,819,248 bytes on macOS 27 arm64
  (Rust 1.99.0, loopback benchmark run); other platforms are not measured.
- **Versioning (planned):** Semantic Versioning tags (`vX.Y.Z`) with a matching section in
  [`CHANGELOG.md`](../CHANGELOG.md) (Keep a Changelog format).
- **Release workflow (planned):** a separate, signed release workflow that runs only on tags pushed by maintainers.
  Signing keys and other secrets must never be reachable from pull-request CI, which keeps `contents: read`, no
  secrets and no `pull_request_target`.
- **Third-party notices (planned):** before the first binary release, generate a notices file with `cargo-about` and
  bundle it with every binary, covering the MIT, BSD, Apache, BlueOak and MPL licenses in the graph (see
  [`dependencies.md`](dependencies.md#third-party-notices)).
- **Platform claims:** a release note names the OS version, architecture and environment for every platform and uses
  only the labels in [`testing.md`](testing.md#status-labels).
