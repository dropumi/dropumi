# ADR-0013: Android app (phone and tablet) in Kotlin and Jetpack Compose

- **Status:** Proposed
- **Date:** 2026-10-15
- **Related:** ADR-0003, ADR-0004, ADR-0006, ADR-0007, ADR-0008, ADR-0009, ADR-0012, ADR-0014, [`../protocol/discovery.md`](../protocol/discovery.md), [`../protocol/threat-model.md`](../protocol/threat-model.md)

## Context

- ADR-0008 chose native mobile UIs and named Kotlin with Jetpack Compose for Android. ADR-0012 built the Apple apps on the same core; Android is the other half of milestone 2.
- The core is reached through `dropumi-ffi` and its generated Kotlin bindings in `apps/android/core-bindings` (ADR-0009). The bindings call the Rust library through JNA.
- The identity stays a key file in app-private storage that is excluded from backups (ADR-0014).
- Android adds problems the Apple apps did not have:
  - shared files arrive as `content://` URIs from other apps, so the file-source guard (threat-model G27) needs an Android counterpart;
  - NSD (`NsdManager`) has no public way to choose the SRV host, so advertising may publish the system host name (threat-model R21);
  - other apps can draw over Dropumi's window (threat-model R23);
  - the OS stops background apps, and foreground services have per-level rules;
  - the minimum API level decides which platform features exist.
- Everything that depends on an Android API level, a module version, an emulator image or a device is UNVERIFIED until a spike or a test records it. Every API 26-34 row is "Not tested" until an image or a device run proves it.

## Decision

### D1. Compose and modules

One Kotlin and Jetpack Compose code base for phones and tablets, under `apps/android`, with AGP 9's built-in Kotlin. Why Compose: ADR-0008 already chose it; screens only render state, so the Apple `AppModel` ports almost one to one; one code base covers phone and tablet with a width check; the semantics tree gives deterministic UI state tests and accessibility checks.

| Module | Package | Contents |
|---|---|---|
| `:core-bindings` | `io.github.dropumi.ffi` | The generated bindings and JNA (unchanged) |
| `:core` | `io.github.dropumi.core` | The core client, the NSD adapter, content-URI files and `UriGuard`, app paths, local addresses, received-file export |
| `:appstate` | `io.github.dropumi.appstate` | `AppModel` and the pure state the screens render; no Android framework calls in main code |
| `:testsupport` | `io.github.dropumi.testsupport` | The fake core, fixtures, UI scenarios, and the `AppModel` JVM tests |
| `:app` | `io.github.dropumi` | The application, activities, services, design system, screens and notifications |
| `:overlayhelper` | `io.github.dropumi.overlayhelper` | Test only: draws an overlay window for the R23 tests (D8). Never shipped. |

- Material 3 is used for its components only; dynamic colour is off and the design.md tokens are mapped onto it.
- Not used: navigation-compose, Hilt or Dagger, Room or DataStore, core-splashscreen, Accompanist, Coil, Material icons, Firebase, Play Services, and any analytics or crash SDK.
- Icons are Lucide icons converted to VectorDrawables, under the ISC licence. The font is the same Plus Jakarta Sans variable file as on Apple, under the OFL.
- The Compose compiler plugin version must equal AGP's bundled Kotlin version. Dependabot ignores it; it is bumped by hand together with AGP (`docs/dependencies.md`).
- The merged manifest is network-silent: the EmojiCompat initializer (which could request a downloadable font) and the exported `ProfileInstallReceiver` are removed.

### D2. minSdk 26, targetSdk 36, and the API matrix

- **minSdk 26** (Android 8.0), targetSdk and compileSdk 36. ABIs `arm64-v8a` and `x86_64` only (`abiFilters`), so JNA's 32-bit `libjnidispatch` never makes the APK installable on a device without `libdropumi_ffi`. `.so` files are stored uncompressed and page-aligned (`useLegacyPackaging = false`), for 16 KB pages.
- Every version-dependent behaviour has an explicit branch, and lint's `NewApi` is an error:

| Area | API 26-28 | API 29-30 | API 31-32 | API 33 | API 34 | API 35-36 |
|---|---|---|---|---|---|---|
| Export to shared storage | SAF | MediaStore `Downloads` | same | same | same | same |
| Runtime notification permission | none | none | none | `POST_NOTIFICATIONS` | same | same |
| FGS type | attribute ignored | attribute exists | background start banned | same | type required | `dataSync` capped at 6 h per 24 h |
| Dark theme | Light or Dark in the app (no system setting) | follows the system by default | same | same | same | same |
| Overlay defence (R23) | `FLAG_WINDOW_IS_OBSCURED` | plus `FLAG_WINDOW_IS_PARTIALLY_OBSCURED` | plus `setHideOverlayWindows` and the system's untrusted-touch block | same | same | same |
| Backup | `fullBackupContent` | same | `dataExtractionRules` | same | same | same |
| NSD | system `mdnsd`, serial resolve | same | same | Connectivity module, Executor overloads | `registerServiceInfoCallback` | same |
| Predictive back | Back key | same | same | opt-in | opt-in | default on API 36 |

- `ACCESS_LOCAL_NETWORK` (targetSdk 37) is deferred. The migration (an in-context request, a denial path that `AppModel` matches by a shared constant, and its UI scenario) is a later change.
- The core's first-launch and receive paths use `renameat2` through rustix's raw syscall, which a per-ABI seccomp allowlist could kill. Spike S-AN3 decided minSdk 26 (see "Spike results").

### D3. Config and backup

- The configuration folder is `noBackupFilesDir/dropumi`. The app creates only the parent; the core creates the folder 0700 and runs the ADR-0003 owner and mode checks. App data (history) lives in `noBackupFilesDir/app`; settings in SharedPreferences.
- `android:allowBackup="false"`, `dataExtractionRules` (31+) and `fullBackupContent` (26-30) exclude every domain from cloud backup and device transfer.
- Nothing in the app writes event contents, URIs, display names, paths, peer names, fingerprints or exception text to logcat. One `SafeLog` takes only a fixed event enum, and R8 removes `android.util.Log` calls from release and `qa` builds. `DropumiException.message` is never logged; it is mapped to the error catalog.
- The device name is prefilled only from the core's `Dropumi-XXXX`. The app never reads or suggests the system device name, the model, Bluetooth names or account names.

### D4. Received files stay app-private

- The core writes only to `filesDir/received`, which the app creates with mode 0700 and checks with `Os.stat`; files are finalized 0600 with no overwrite. Shared or FUSE storage is never given to the core.
- Files leave only through an explicit per-transfer user action:
  - **Open** and **Share** through a non-exported FileProvider whose only path is `received/`;
  - **Save to Downloads**: MediaStore `Download/Dropumi/` with `IS_PENDING` on API 29+, and the Storage Access Framework (the user picks the destination) on API 26-28. No storage permission is requested. Save checks the file's identity and size, counts the copied bytes, and never overwrites; it never deletes the internal copy.
- Saving is disclosed: "Saved files can be opened by other apps that can see your Downloads."

### D5. NSD discovery with the R21 gate

- A `PlatformDiscovery` adapter over `NsdManager` only moves bytes; Rust keeps every rule (instance names, TXT, IPv4 only, self-filter, caps).
- **Advertising is gated on observed behaviour.** An allowlist keyed on (API level, Connectivity module version) lists the pairs observed to publish a host label that is random per advertisement; it is **closed by default**, and closed when the module version cannot be read. When it is closed, the app passes `ReceiveOptions.advertise = false`, so the phone is browse-only: it sees other devices, and peers reach it after pairing or by address. The UI says so.
- **Rename rule.** Registration does not wait for NSD's answer. If NSD registers a different name later, the adapter unregisters it at once and, until the unregister completes, drops browse results with that name. This is the only adapter-side filter, and a deliberate exception to "Rust owns every rule".
- **Bounds.** At most 64 queued resolves (below 34) or 64 active service-info callbacks (34+), FIFO and deduplicated; `FAILURE_ALREADY_ACTIVE` is retried at most three times.
- **Threading.** All adapter state lives on one `dropumi-nsd` thread; every callback hops to it first; a new listener object is used per browse session and per registration; callbacks from an older session are ignored.
- **No stack traces reach Rust.** Every callback body catches `Throwable`. Fallible methods rethrow `CallbackException.Failed` with a fixed phrase; infallible ones return fixed values.
- `LocalAddresses` reads only Wi-Fi and Ethernet networks (never a network that also has cellular transport), without loopback or link-local addresses. Process network binding is not used.

### D6. `UriGuard` and the share target

- **`UriGuard`** is the Android counterpart of G27. For every source (share sheet, document picker, interop):
  1. the scheme must be `content`;
  2. the authority, normalized (the part after the last `@`, without a `:port`, case-insensitive, empty refused), must not be one of Dropumi's own declared providers;
  3. the file descriptor that is actually read must not match (`st_dev`, `st_ino`) any entry of the configuration folder or of `noBackupFilesDir/app`, whatever its name (including lock and temporary files), and `/proc/self/fd/<n>` must not resolve under either folder.
  It fails closed. Steps 1-2 run before any query or open; the whole guard runs again on open and on staging. A URI is never used as a file name; the fallback name is `shared-file`.
- **Unknown-size documents** are staged to `noBackupFilesDir/outgoing/<uuid>/` under a sanitized single-component name, with a size cap, then sent as paths so the Rust guards apply too.
- **`ShareActivity`** (exported, no UI) is the only share entry. It hands accepted URIs to the main activity through an in-process hand-off, never through an intent. `MainActivity` never reads share items from its own intent, so another app cannot inject a draft by starting it.
- A Rust-side `ForeignFile` file-identity hook is a follow-up on the FFI branch.

### D7. Foreground services

- **Transfer mode (`TransferService`, `dataSync`):** started only while Dropumi is visible (Send, Accept, or leaving the app with a transfer running), with a progress notification; it stops at the last transfer and is `START_NOT_STICKY`. If the OS refuses a start, the transfer goes on and the card says it may pause. User-initiated data transfer jobs (34+) are not used.
- **Receiver mode (`ReceiverService`, `connectedDevice`, milestone 2c-2):** opt-in and off by default. It refuses to start without visible notifications, stops when its notification or channel is blocked, and **dismissing its notification stops it**. It is never restarted by the OS and never started at boot.
- Leaving the app stops accepting (`ProcessLifecycleOwner`), except while receiver mode runs; configuration changes do not.
- Every notification is private on the lock screen, with a generic public version that names no peer and no file. Offer notifications (2c-2) have no actions.
- The app never calls `DropumiNode.shutdown()` in production.

### D8. Prompts

- One prompt at a time, drawn full screen inside `MainActivity`'s window (never a separate `Dialog` window). Only the core closes it. There is no default action and no initial focus.
- Accept and "Codes match" arm 600 ms after the prompt appears, and re-arm when the activity resumes and when the window regains focus. `AppModel` enforces the arming too and ignores an early Accept.
- Back (including predictive back, once completed) and Escape decline. Back is ignored while an answer is in flight.
- Peer strings are shown literally: no HTML, no links, never a format string.
- **Overlays (R23), by level, on protected surfaces only** (a prompt, a trust or policy confirmation, and the Send button): an obscured gesture is dropped on prompts and confirmations and ignored on Send; on 31+ overlay windows are also hidden while a prompt or confirmation shows. Other screens only show a non-blocking notice. API 26-30 is weaker (no hiding, partial detection only from 29); this residual risk is disclosed. Trust and policy confirmations are in-window panels, never `Dialog`, `AlertDialog` or `ModalBottomSheet`.
- No `FLAG_SECURE` (the code is not secret), nothing on the lock screen, and no notification Accept.

### D9. The fake core exists only in the test APK

- `:testsupport` is wired into `:app` only as `testImplementation` and `androidTestImplementation`. No app APK (debug, `qa` or release) contains the fake core.
- The instrumentation runner selects a test Application: the fake core on temporary paths (default), or the real core on `noBackupFilesDir/interop-<uuid>/` (one runner argument). No intent or exported component can select either.
- CI checks the runtime classpaths, the release and `qa` R8 mappings, and the debug dex.

### D10. Tests, the `qa` build type and interop

- JVM unit tests for `:core`, `:appstate`, `:testsupport` and `:app` never load the native library (a library override makes any load fail); Android adapters sit behind pure-Kotlin seams.
- Compose UI state tests run on the fake core ("UI state test (fake core)"); real-core instrumented tests cover storage, identity, receiving, `UriGuard`, export and services.
- `qa` is a minified build type with the release R8 rules plus keep rules for what the test APK calls. The opt-in CLI interop harness (`crates/cli/tests/e2e/interop/android.rs`, `scripts/android/interop.sh`, `DROPUMI_TEST_ANDROID=1`) runs it.
- The `:overlayhelper` APK is exported but guarded by the `DUMP` permission, draws only with a per-run token, and is uninstalled after every run.
- The core library is stamped with a hash of the Rust sources it was built from; the build refuses a stale one and never runs cargo itself.

### D11. Labels

Results use the project's labels with the OS version, API level, ABI and environment (emulator image or device). An x86_64 emulator result never stands for arm64-v8a, and one API level never stands for another.

### D12. Non-goals

Play Store release work; start at boot; Wi-Fi Direct, Wi-Fi Aware, LocalOnlyHotspot and BLE (so no `NEARBY_WIFI_DEVICES` and no Bluetooth permission); a Keystore-backed identity (later, behind `IdentityStore`); targetSdk 37 and `ACCESS_LOCAL_NETWORK`; the NSD system picker; camera and photo pickers; armeabi-v7a and x86; changes to the wire protocol or the Rust core; user-initiated data transfer jobs.

### D13. Spikes

| ID | Question | Decides |
|---|---|---|
| S-AN3 | Does the real core's receive path (and identity creation) survive seccomp on arm64 API 26 and 28? | minSdk 26 |
| S-AN1 | Does instrumentation status reach the host live? | the interop harness transport |
| S-AN2 | What SRV host label does NSD publish per (API, module)? Browsing without a multicast lock; Wi-Fi without internet (NET-5) | the R21 gate allowlist; the multicast-lock permission |
| S-AN5 | FGS prerequisites on 34+, background-start refusals, Doze | service permissions and fallbacks |
| S-AN6, S-AN7 | API 26/28 platform sweep; the 16 KB-page runtime | per-level rows in `docs/testing.md` |
| S-AN4 | Can CI run an emulator? | whether an emulator CI job exists |

### D14. Sideload policy

Builds are debug, `qa` and locally signed release APKs, installed with `adb install`. Google's developer verification for sideloaded apps and Play Protect warnings may affect handing an APK to a tester; this is recorded per device (device matrix row INST-1). Play Store release work is out of scope.

## Spike results

| ID | Result |
|---|---|
| S-AN3 (2026-10-15, macOS 27 arm64 host, scratch instrumented test, not committed) | arm64-v8a emulators API 26, 28 and 36 (Android 8, 9 and 16). The config lock first failed with "lock() not supported" (std has no `File::lock` on Android); the core now uses `flock` there (ADR-0003). After that fix, identity creation, trust writes and two receives of the same name passed, with no `SIGSYS`: **minSdk 26 stands.** `Os.link` inside the app's data returned `EACCES` on all three images, so the hard-link `UriGuard` tests are "not runnable on this image (link EACCES)" there and fail with that message instead of skipping; their no-link variants run everywhere. A committed instrumented test replaces the scratch one. |
| S-AN1, S-AN2, S-AN4, S-AN5, S-AN6, S-AN7 | Not run yet. |

## Implementation

Not implemented yet; this record is proposed together with milestone 2c. The implementation section is written when the record is accepted, with the code paths, tests and evidence labels.

## Consequences

### Positive

- One UI code base for phones and tablets, with the same consent rules as the Apple apps.
- Received files never become visible to other apps without a user action, and no storage permission is needed on any level.
- A shared URI cannot make Dropumi send its own identity, trust store or history, whatever the URI claims.
- No fake-core code can reach a user's device.
- The app sends no traffic beyond the LAN peer and mDNS.

### Negative

- Where the R21 gate is closed, the phone is browse-only: it is not listed on other devices until an observed module version opens the gate.
- Overlay protection on API 26-30 is weaker, and accessibility-granted apps stay out of scope.
- Two export paths (MediaStore and SAF), and a picker step on API 26-28.
- Receiving needs Dropumi open unless the user turns on receiver mode, which shows a persistent notification.
- The Compose compiler plugin is coupled to AGP's bundled Kotlin and must be bumped by hand.
- About fifteen AndroidX artifacts; Gradle dependency verification is a follow-up.
- Play policy for `dataSync` and `connectedDevice`: Requires investigation (it blocks a store release, not this milestone).

## Alternatives considered

- **Android Views with Material Components:** about the same size, more glue code (RecyclerView, Fragments), and no semantics tree for state tests.
- **A cross-platform UI (Flutter, React Native, Compose Multiplatform for iOS):** ADR-0008 and ADR-0012 chose native UIs; a second bridge to the Rust core.
- **`WRITE_EXTERNAL_STORAGE` on API 26-28:** grants read and write access to all of shared storage; SAF lets the user choose the destination without a permission.
- **Receiving straight into shared storage:** any app with storage access could read files before the user decides; the core's no-overwrite guarantees do not hold on FUSE.
- **Always advertising with NSD:** may publish the system host name, which discovery.md forbids.
- **`resolveContentProvider` to detect Dropumi's own URIs:** unreliable; the fd-identity check is the real guard.
- **User-initiated data transfer jobs for transfers:** they do not exist before API 34, so a service would be needed anyway, and whether a job keeps the in-process Rust runtime's transfers alive is UNVERIFIED.
- **The fake core in debug builds (`debugImplementation`):** a debug APK on a device could run on fake data; keeping it in the test APK makes that impossible by construction.
- **A higher minSdk:** not needed once S-AN3 passed on arm64 API 26 and 28.
