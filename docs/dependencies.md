# Dependencies

Every direct dependency of the workspace, its license and why it is there. Policy:
[ADR-0010](adr/0010-dependency-license-and-ci-supply-chain-policy.md), enforced by [`deny.toml`](../deny.toml) and
`cargo deny --locked check` in CI.

- Versions are the ones resolved in [`Cargo.lock`](../Cargo.lock); every external dependency is declared once in the
  root `[workspace.dependencies]` and inherited by the crates.
- Licenses are the SPDX expressions from each crate's manifest; they match the `cargo deny list` output of the
  normal dependency graph (dev-dependencies are not part of that list).
- Dropumi itself is MPL-2.0. All seven workspace crates are `publish = false`.
- Dependencies of the apps that Cargo does not see (Swift packages, Gradle) are listed in
  [App-side dependencies](#app-side-dependencies).

## Direct dependencies

| Crate | Version | License | Used by (kind, features) | Purpose and justification |
|---|---|---|---|---|
| `blake3` | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | protocol; cli (dev) | Per-file BLAKE3-256 content hash, computed while data streams (sender and receiver machines). Fast and SIMD-accelerated. Compiles C for some targets, so Android builds need the NDK's clang. The e2e raw peers use it to hash the files they send. |
| `clap` | 4.6.7 | MIT OR Apache-2.0 | cli (`default-features = false`; `std`, `derive`, `help`, `usage`, `error-context`, `env`) | Argument parsing, help text and environment-variable fallbacks. The default `color` and `suggestions` features are not enabled. |
| `data-encoding` | 2.11.1 | MIT | domain | RFC 4648 base32 for fingerprints, hex for keys and transfer ids. Small, no dependencies. |
| `dirs` | 7.0.0 | MIT OR Apache-2.0 | infra | The OS local configuration directory and the Downloads folder. Brings in `dirs-sys` and `option-ext` (MPL-2.0). |
| `getrandom` | 0.3.4 | MIT OR Apache-2.0 | protocol | OS randomness for static keys, SAS nonces and transfer ids. Major version pinned to the one `snow` uses ([pinned majors](#pinned-majors)). |
| `mdns-sd` | 0.21.5 | Apache-2.0 OR MIT | infra (optional, feature `mdns`; `default-features = false`; `async`) | mDNS/DNS-SD advertising and browsing in pure Rust. Optional so that mobile builds and `--no-default-features` builds leave it out. Runs its own daemon thread. |
| `minicbor` | 2.3.0 | BlueOak-1.0.0 | protocol (`default-features = false`; `alloc`) | CBOR encoding and low-level decoding under Dropumi's hand-written strict decoder (definite lengths, ascending keys, depth and entry limits). |
| `proptest` | 1.12.0 | MIT OR Apache-2.0 | domain (dev), protocol (dev), infra (dev), cli (dev) | Property tests for the filename and device-name sanitizers, the terminal sanitizer, fingerprint and target parsing, the mDNS TXT parser, the CBOR codec and the session machines. Not in any shipped binary. |
| `rustix` | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | infra (Unix targets only; `default-features = false`; `std`, `process`, `fs`), cli (Unix targets only; adds `termios`) | Effective user id for the config ownership checks, errno values for the exFAT/vfat finalization fallback, the config lock on Android (`flock`, because std's `File::lock` is not implemented there), and, in the CLI, flushing pending terminal input before each prompt (`tcflush`), without `unsafe` code in Dropumi. Already in the graph through `tempfile`, which enables `fs` too. |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | infra (`derive`) | (De)serializing `device.json` and `trusted_devices.json`. |
| `serde_json` | 1.0.154 | MIT OR Apache-2.0 | infra; cli | JSON configuration files (infra); NDJSON events and control commands (cli). The e2e harness parses NDJSON with it. |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 | protocol; cli (dev), ffi (dev) | SHA-256 for the fingerprint, the SAS commitment and the SAS, each with a `dropumi/v1/...` domain-separation label. The e2e tests use it as an independent file hash. Major version pinned to `snow`'s. |
| `snow` | 0.10.0 | Apache-2.0 OR MIT | protocol (`default-features = false`; `use-curve25519`, `use-chacha20poly1305`, `use-sha2`, `use-getrandom`) | The Noise protocol framework (`Noise_XX_25519_ChaChaPoly_SHA256`). Built with pure-Rust backends only, so `ring` is not in the graph ([ADR-0002](adr/0002-secure-channel-noise-xx.md)). |
| `tempfile` | 3.27.0 | MIT OR Apache-2.0 | infra; cli (dev), ffi (dev) | `create_new` temporary files in the receive directory, `persist_noclobber` (atomic rename that never overwrites), atomic configuration writes. Temporary directories in the e2e and facade tests. |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | domain, protocol, application, infra, ffi | Error enums. The project uses `thiserror` only (no `anyhow`). |
| `tokio` | 1.53.2 | MIT | application (`io-util`, `macros`, `rt`, `sync`, `time`; dev adds `test-util`); infra (`fs`, `io-util`, `net`, `rt`, `sync`, `time`; dev adds `macros`); cli (`io-util`, `macros`, `rt`, `signal`, `sync`, `time`); ffi (`io-util`, `macros`, `net`, `rt`, `sync`, `time`; dev adds `test-util`). `default-features = false` everywhere. | Async runtime (`current_thread`), TCP, file I/O, timers, channels, Ctrl-C handling. `test-util` gives the paused clock for timeout tests. |
| `tokio-util` | 0.7.20 | MIT | application, cli, ffi (`default-features = false`) | `CancellationToken` for cancelling transfers and stopping commands. |
| `tracing` | 0.1.44 | MIT | application, infra, cli, ffi | Structured logs (stderr only). |
| `tracing-subscriber` | 0.3.23 | MIT | cli (`default-features = false`; `fmt`, `std`) | Log formatting to stderr, `-v`/`-q` levels and the `DROPUMI_LOG` filter. |
| `uniffi` | 0.32.2 | MPL-2.0 | ffi (`=0.32.2`, `default-features = false`); ffi-bindgen (`cli`, `cargo-metadata`) | Generates the FFI scaffolding (procedural macros) and the Swift and Kotlin bindings ([ADR-0009](adr/0009-ffi-bindings-strategy.md)). Pinned exactly because it is pre-1.0 and generated code must match the library. The library build pulls `uniffi_core`, `uniffi_macros` and `uniffi_pipeline` (MPL-2.0), `anyhow` (used inside UniFFI; Dropumi code does not use it), `bytes`, `once_cell`, `static_assertions` and, through the macros, `tempfile`; the generator binary also pulls `clap`, `askama`, `goblin`, `cargo_metadata` and `toml`, which never reach a shipped library. Alternatives: swift-bridge (Swift only), cbindgen plus hand-written wrappers (the fallback), Diplomat, hand-written JNI. |
| `unicode-normalization` | 0.1.25 | MIT OR Apache-2.0 | domain | NFC normalization of file and device names. Its Unicode version (17.0.0) must match the format-character table in `domain::text`, which a unit test checks. |
| `zeroize` | 1.9.1 | Apache-2.0 OR MIT | protocol, infra | Wipes secret key bytes from memory (the static key and the key file buffer). |

**Internal crates** (path dependencies in `[workspace.dependencies]`, all MPL-2.0, version 0.1.0): `dropumi-domain`,
`dropumi-protocol`, `dropumi-application`, `dropumi-infra` (declared with `default-features = false`; the CLI's
`mdns` feature enables `dropumi-infra/mdns`; `dropumi-ffi` never enables it), `dropumi-cli`, `dropumi-ffi` (a
dev-dependency of the CLI, for interop tests) and `dropumi-ffi-bindgen` (a build tool nothing depends on). Allowed directions are in
[`ARCHITECTURE.md`](../ARCHITECTURE.md#enforcement).

## Licenses in the dependency graph

From `cargo deny list` (normal dependencies, all features, every target listed in `deny.toml`): 145 crate versions,
of which 7 are the workspace crates (2026-10-14). The generator (`dropumi-ffi-bindgen`) and its tools account for
much of the growth since milestone 1. A crate offered under several licenses is counted under each of them.

| License | Crates | Notes |
|---|---|---|
| MIT | 124 | |
| Apache-2.0 | 99 | |
| MPL-2.0 | 16 | The 7 workspace crates, `option-ext` 0.2.0 (via `dirs` -> `dirs-sys`) and the 8 `uniffi` crates (`uniffi`, `uniffi_core`, `uniffi_macros`, `uniffi_meta`, `uniffi_pipeline`, `uniffi_internal_macros`, `uniffi_bindgen`, `uniffi_udl`) |
| Apache-2.0 WITH LLVM-exception | 3 | `blake3`, `rustix`, `linux-raw-sys` (each also offered under other licenses) |
| BSD-3-Clause | 3 | `curve25519-dalek` 4.1.3 and `subtle` 2.6.1 (via `snow`), `if-addrs` 0.15.0 (via `mdns-sd`, MIT OR BSD-3-Clause) |
| CC0-1.0 | 2 | `blake3`, `constant_time_eq` (alternatives) |
| BlueOak-1.0.0 | 1 | `minicbor` 2.3.0 (only license) |
| MIT-0 | 1 | `constant_time_eq` (alternative) |
| Unicode-3.0 | 1 | `unicode-ident` ((MIT OR Apache-2.0) AND Unicode-3.0), a build-time dependency of the proc-macro crates |
| Unlicense | 1 | `memchr` (alternative to MIT) |
| Zlib | 1 | `tinyvec` (alternative) |

All of these are on the `deny.toml` allow-list. `cargo deny --locked check` passed locally on macOS 27 arm64
(advisories, bans, licenses and sources), with warnings only for the duplicate versions below.

## Duplicate versions

`multiple-versions = "warn"` in `deny.toml`. The local run reported three duplicates. Their origin, from
`Cargo.lock`:

| Crate | Versions | Where each comes from | In the shipped binary? |
|---|---|---|---|
| `cpufeatures` | 0.2.17, 0.3.1 | 0.2.17: `snow`'s RustCrypto stack (`chacha20` 0.9, `poly1305` 0.8, `curve25519-dalek` 4.1, `sha2` 0.10). 0.3.1: `blake3` 1.8.7, and `proptest`'s `chacha20` 0.10 (dev only). | Both (runtime CPU feature detection) |
| `getrandom` | 0.3.4, 0.4.3 | 0.3.4: `snow` 0.10 and `dropumi-protocol`. 0.4.3: `tempfile` 3.27 (`dropumi-infra`, and the `uniffi` macros), and `proptest`'s `rand` 0.10 (dev only). | Both |
| `syn` | 2.0.119, 3.0.9 | 2.0: `curve25519-dalek-derive`, `tracing-attributes`. 3.0: `clap_derive`, `serde_derive`, `thiserror-impl`, `tokio-macros`, and `borsh-derive` via `proptest` (dev only). | No (proc macros run at build time only) |

These are not only dev or proc-macro paths: `cpufeatures` and `getrandom` are duplicated in the runtime graph as well.
The cost (binary size, build time) is not measured. The `getrandom` 0.3 line cannot move until `snow` does
([pinned majors](#pinned-majors)).

## Pinned majors

| Crate | Pinned to | Why | Dependabot |
|---|---|---|---|
| `getrandom` | 0.3 | `snow` 0.10.0 requires `getrandom` ^0.3. A different major in `dropumi-protocol` would add yet another copy. | Ignores `>=0.4.0` |
| `sha2` | 0.10 | `snow` 0.10.0 requires `sha2` ^0.10; Dropumi's fingerprint and SAS hashing share that copy. | Ignores `>=0.11.0` |

Explicit version ranges are used because how Dependabot classifies a 0.x to 0.(x+1) bump is UNVERIFIED. Revisit both
pins when `snow` moves to new majors ([`.github/dependabot.yml`](../.github/dependabot.yml)).

## Banned crates

`deny.toml` bans `openssl`, `openssl-sys`, `native-tls` (pure-Rust cryptography only) and `ring` (`snow` is built
without it). `scripts/ci/check-architecture.sh` also fails if `ring` appears in the workspace graph. `ring` is not in
`Cargo.lock`. Sources are restricted to crates.io; unknown registries and git sources are denied.

## App-side dependencies

Not part of the Cargo graph, so `cargo deny` does not see them. Each is pinned in
`apps/android/gradle/libs.versions.toml` and listed here; Dependabot watches `apps/android` (Gradle). Licences were
checked against each artifact's POM. Not used on Android: navigation-compose, Hilt or Dagger, Room or DataStore,
core-splashscreen, Accompanist, Coil, Material icons, Firebase, Play Services, and any analytics or crash SDK.

| Dependency | Version | License | Where | Purpose and justification |
|---|---|---|---|---|
| JNA (`net.java.dev.jna:jna`, AAR for Android, JAR for host tests) | 5.19.1 | Apache-2.0 OR LGPL-2.1-or-later (taken under Apache-2.0) | `apps/android/core-bindings` | The UniFFI Kotlin bindings call the Rust library through JNA. 5.17.0 and later build `libjnidispatch.so` for 16 KB pages; `scripts/ffi/check-16k.sh` passed on every ABI of the 5.19.1 AAR. `libjnidispatch.so` (arm64-v8a) is 176,520 bytes. |
| Android Gradle Plugin (`com.android.application`, `com.android.library`) | 9.4.1 | Apache-2.0 | `apps/android` (build time) | Builds the app and the Android libraries; its built-in Kotlin (Kotlin Gradle plugin 2.2.10, read with `./gradlew -p apps/android buildEnvironment`) compiles all Kotlin code. |
| Compose compiler Gradle plugin (`org.jetbrains.kotlin.plugin.compose`) | 2.2.10 | Apache-2.0 | `apps/android/app` (build time) | Compiles `@Composable` code. **Coupled to AGP:** its version must equal the Kotlin version AGP bundles, so Dependabot ignores it (`.github/dependabot.yml`) and it is bumped by hand in the same change as AGP, after checking `buildEnvironment`. |
| Jetpack Compose (`androidx.compose:compose-bom`: `ui`, `foundation`, `material3`, `ui-tooling-preview`) | BOM 2026.06.01 (Compose 1.11.4, Material 3 1.4.0) | Apache-2.0 | `apps/android/app` | The UI toolkit (ADR-0013 D1). Material 3 is used for its components only; dynamic colour is off and the design.md tokens are mapped onto it. `ui-tooling-preview` holds only the `@Preview` annotations (no `ui-tooling`, whose debug activity would be exported). Compose brings `androidx.graphics:graphics-path` 1.0.1 (Apache-2.0), which ships a native library, `libandroidx.graphics.path.so`, for both ABIs. BOM 2026.08.00 and later (Compose 1.12; `ui-android` 1.12.0 declares `minCompileSdk=37`) need compileSdk 37; the app compiles against 36 (ADR-0013 D2), so Dependabot ignores those BOMs (`.github/dependabot.yml`). |
| `androidx.activity:activity-compose` | 1.13.0 | Apache-2.0 | `apps/android/app` | `setContent`, `BackHandler` and predictive back, the activity-result APIs, `enableEdgeToEdge`. |
| `androidx.lifecycle:lifecycle-runtime-compose`, `lifecycle-process` | 2.10.0 | Apache-2.0 | `apps/android/app` | `collectAsStateWithLifecycle`; `ProcessLifecycleOwner`, so leaving the app stops accepting without reacting to configuration changes. 2.11.0 needs compileSdk 37 (`lifecycle-runtime-compose-android` 2.11.0 declares `minCompileSdk=37`), so Dependabot ignores `androidx.lifecycle` 2.11.0 and later. |
| `androidx.core:core-ktx` | 1.18.0 | Apache-2.0 | `apps/android/app`, `apps/android/overlayhelper` | `NotificationCompat`, `ServiceCompat`, `IntentCompat`, `ContextCompat`. It declares the signature-level `DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION` in the merged manifest. 1.19 needs compileSdk 37 (`core` 1.19.0 declares `minCompileSdk=37`), so Dependabot ignores 1.19.0 and later. |
| `org.jetbrains.kotlinx:kotlinx-coroutines-android` | 1.11.0 | Apache-2.0 | `apps/android/core` (API), `apps/android/app` | Coroutines, `StateFlow` and the main dispatcher (it was already transitive through Compose; now declared). The Kotlin standard library (Apache-2.0) resolves to 2.2.20. |
| `org.jetbrains.kotlinx:kotlinx-coroutines-test` | 1.11.0 | Apache-2.0 | `apps/android/{core,appstate,testsupport,app}` (tests only) | Virtual time for the JVM tests. Not in any app. |
| AndroidX Test (`androidx.test:runner` 1.7.0, `androidx.test:rules` 1.7.0, `androidx.test.ext:junit` 1.3.0, `androidx.test.uiautomator:uiautomator` 2.4.0, `androidx.test.espresso:espresso-core` 3.7.0) and `androidx.compose.ui:ui-test-junit4` (from the BOM) | see the name | Apache-2.0 | `apps/android/app` (instrumented tests only) | The instrumented tests, the custom runner, key injection and the overlay-helper tests. Only in the test APK. `ui-test-junit4` brings `espresso-core` 3.5.0, which calls `InputManager.getInstance()`, removed on API 36, so every Compose test failed there. 3.7.0 is a new pinned version (test APK only, Apache-2.0): declaring it directly makes Gradle's conflict resolution pick it over the 3.5.0 that `ui-test-android` asks for (`./gradlew :app:dependencyInsight --configuration debugAndroidTestRuntimeClasspath --dependency androidx.test.espresso:espresso-core`: "By conflict resolution: between versions 3.7.0 and 3.5.0"). `ui-test-manifest` is deliberately not used: as a `debugImplementation` it would add an exported activity to debug APKs. |
| Compose accessibility checks (`androidx.compose.ui:ui-test-junit4-accessibility`, from the BOM) and the Accessibility Test Framework it brings (`com.google.android.apps.common.testing.accessibility.framework:accessibility-test-framework`) | 1.11.4; ATF 4.1.1 | Apache-2.0 | `apps/android/app` (instrumented tests only) | `enableAccessibilityChecks()` for the UI state tests (touch targets, contrast, labels on Nearby, the unverified offer, the code screen and Transfers). Only in the test APK; the app shows nothing from it. ATF brings, also only into the test APK: Guava 31.0.1-android with `failureaccess` 1.0.1, `jsr305` 3.0.2 and `j2objc-annotations` 1.3 (Apache-2.0), `protobuf-javalite` 3.19.1 (BSD-3-Clause), jsoup 1.15.1 (MIT), Hamcrest 1.3 (BSD-3-Clause), Material Components 1.2.0-rc01 and older AndroidX view libraries such as `appcompat` 1.1.0 (Apache-2.0), and Guava's `checker-compat-qual` 2.5.5 annotations: its POM lists two licences, GPL-2.0 with the Classpath Exception and MIT, and the Checker Framework's `LICENSE.txt` (tag `checker-framework-2.5.5`) puts its annotations under MIT; the JAR holds only 10 annotation classes (`org.checkerframework.checker.nullness.compatqual`), so it is taken under MIT. Licences read from each POM or its parent POM. |
| Gradle wrapper | 9.8.1 | Apache-2.0 | `apps/android/gradlew`, `gradlew.bat`, `gradle/wrapper/` (committed, generated by Gradle) | Reproducible Gradle version; `distributionSha256Sum` pins the download, and CI validates the wrapper JAR (`gradle/actions/setup-gradle`). |
| JUnit | 4.13.2 | EPL-1.0 | `apps/android/{core-bindings,core,appstate,testsupport,app}` (tests only) | The Kotlin JVM tests and the instrumented tests. Not in any app. |
| Swift packages | none | — | `apps/apple/DropumiKit`, `apps/apple` | The Swift side has no third-party code dependencies. |
| cargo-about | 0.9.2 | MIT OR Apache-2.0 | developer machines (build time; `cargo install --locked cargo-about --features cli`) | Generates the Rust crate notices the Android app bundles (`scripts/android/rust-notices.sh` → `apps/android/app/src/main/assets/licenses/rust-crates.txt`), offline from the local Cargo registry. A build tool, not a crate dependency; never part of an app. |
| Plus Jakarta Sans (variable font, `PlusJakartaSans-Variable.ttf`) | from `google/fonts` (`ofl/plusjakartasans`), SHA-256 `89b3fb38aa0d275d7a731d0d817a4f1622b316b4d7fbdedcf02ee9099ff68bc8` | OFL-1.1 | `apps/apple/DropumiKit/Sources/DropumiDesign/Resources/Fonts/` (with `OFL.txt`), bundled in the apps and the Share Extension; the same file in `apps/android/app/src/main/res/font/plus_jakarta_sans.ttf` (Android resource names must be lower-case), with `OFL.txt` in `apps/android/app/src/main/assets/licenses/` | The typeface design.md specifies. Bundled so the apps never download a font. The OFL allows bundling with software; the font is not sold on its own and keeps its name. On Android each weight sets the font's `wght` axis. |
| Lucide icons (33 icons converted to vector drawables) | `lucide-static` 1.57.0 (npm tarball SHA-256 `afc8602c5231ce64ee227286f93772d0438eddd3b947b645bc14bfd1e49da518`) | ISC; 14 of them (`smartphone`, `monitor`, `upload`, `info`, `link`, `plus`, `x`, `music`, `circle-x`, `circle-alert`, `triangle-alert`, `arrow-left`, `chevron-right`, `ellipsis-vertical`) derive from Feather and are also MIT | `apps/android/app/src/main/res/drawable/lucide_*.xml`, licence in `apps/android/app/src/main/assets/licenses/LUCIDE-ISC.txt` (Lucide's `LICENSE`, verbatim, with the Feather MIT notice) | The icon set design.md specifies. Each file carries its SPDX identifier (`ISC`, or `ISC AND MIT` for the Feather-derived ones) and the icon's name and version. Circles, rectangles and lines were converted to path data; the stroke style is unchanged (2 px, round caps and joins); `arrow-left` and `chevron-right` are mirrored in right-to-left layouts. No icon font and no Material icons. |
| xcodegen | 2.46.0 | MIT | developer machines and the `apple (app)` CI job (build time) | Generates the Xcode project from `apps/apple/project.yml`, so the project file is reviewable YAML instead of a generated `.pbxproj`. CI downloads the release and checks its SHA-256. Never part of an app. |

The UniFFI configuration (`crates/ffi/uniffi.toml`) uses JNA's cleaner instead of Android's `SystemCleaner`, so one
generated Kotlin file serves Android and the host JVM.

**compileSdk 36 coupling.** The app compiles against API 36, and an AAR that declares a higher `minCompileSdk` fails
the build. Compose BOM 2026.08.00 and later (through its Compose 1.12 artifacts), `androidx.lifecycle` 2.11.0 and
later, and `androidx.core:core-ktx` 1.19.0 and later declare `minCompileSdk=37` (read from each AAR's
`aar-metadata.properties`, 2026-10-15). Dependabot ignores those ranges (`.github/dependabot.yml`), so they do not
block the grouped Gradle update; the change that moves compileSdk to 37 removes those entries and takes the updates
together.

## Third-party notices

There are no binary releases yet. Before the first one:

- Every binary distribution must bundle the license texts and copyright notices its dependencies require: MIT, the
  BSD-3-Clause crates (`curve25519-dalek`, `subtle`, and `if-addrs` if its BSD option is taken), Apache-2.0 (including
  any `NOTICE` files), BlueOak-1.0.0 (`minicbor`: the license text or its link must reach every recipient) and
  MPL-2.0 (`option-ext`: recipients must be told where its source code is available).
- The plan is to generate the notices file with `cargo-about` in the release workflow and ship it next to each binary.
  The Android app already bundles one for `libdropumi_ffi.so` (`scripts/android/rust-notices.sh`, cargo-about 0.9.2).
  Its template prints each licence text and the crates that use it; it does not collect Apache `NOTICE` files. None of
  the 89 third-party crates in the Android graph of `dropumi-ffi` ships a file whose name contains "notice" (checked by
  file name in the local registry sources, 2026-10-15), so nothing is missing today; a new dependency needs the same
  check.
- CC0-1.0, MIT-0 and Unlicense options need no notice; crates with an OR expression may be credited under any one of
  their licenses.
- App builds add JNA (Apache-2.0, including its `NOTICE` if any) on Android, and the MPL-2.0 `uniffi` runtime crates
  in every app (recipients must be told where their source code is available). The Android app's Rust notices list each
  crate's repository for that reason.
- The Android app adds AndroidX, Compose, Kotlin, kotlinx, JNA and the other Apache-2.0 libraries of its runtime
  classpath (JetBrains annotations, JSpecify, Guava ListenableFuture); `assets/licenses/android-libraries.txt` lists
  them with the Apache License 2.0 text.
- The Apple apps bundle Plus Jakarta Sans with its `OFL.txt` (the OFL requires the license to travel with the font).
  The Android app bundles the font with `OFL.txt` and the Lucide icons with their licence (ISC, plus MIT for the
  Feather-derived icons) in `assets/licenses/`.
  The Android app shows these notices, the Rust crate notices and Dropumi's own MPL-2.0 notice (with the source
  location) under Settings › Licences. The Apple apps have no in-app licenses screen yet; there the license text is
  inside the app bundle only.

## Adding a dependency

1. Prefer the standard library or an existing dependency. Every new crate costs binary size and build time.
2. Add it to `[workspace.dependencies]` in the same change that first uses it (cargo-deny rejects unused workspace
   dependencies), with `default-features = false` when the defaults are not needed.
3. Add a row to the table above: version, SPDX license, the crates that use it, and why it is needed and which
   alternatives were considered. Repeat the justification in the pull request.
4. Run `cargo deny --locked check` and `bash scripts/ci/check-architecture.sh`. Inner crates have deny-lists; a crate
   that pulls `tokio` into `dropumi-domain`, for example, fails the architecture check.
5. Never copy code from other projects (for example PairDrop or LocalSend); depend on published crates instead.
