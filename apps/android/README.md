# Dropumi for Android phones and tablets

A Kotlin and Jetpack Compose app over the Rust core (ADR-0009, ADR-0013). Pre-alpha and unaudited.
What has actually been tested is listed in [`docs/testing.md`](../../docs/testing.md).
Nothing here is "supported".

minSdk 26 (Android 8.0), targetSdk and compileSdk 36. The APKs hold `arm64-v8a` and `x86_64` only.

| Module | What it is |
|---|---|
| `:app` | The app: `MainActivity` (every screen and the prompt, in one window), `ShareActivity` (the share target, no UI), the `dataSync` `TransferService`, notifications and the design system |
| `:core` | `LiveCoreClient` over the Rust core, the NSD discovery adapter, `content://` files behind `UriGuard`, `AppPaths`, saving received files |
| `:appstate` | `AppModel`: all UI state, without Android framework calls, so its tests run on the host JVM |
| `:core-bindings` | The generated Kotlin bindings and the Rust core (`libdropumi_ffi.so`) |
| `:testsupport` | A fake core for tests. No app APK contains it. |
| `:overlayhelper` | A test-only APK that draws an overlay window for the overlay tests. Never shipped. |

## Build

You need JDK 17, the Android SDK (platform 36, build-tools 36.0.0, NDK 27.1.12297006) and Rust with the Android
targets (see [`docs/development.md`](../../docs/development.md#the-android-app)).

```sh
bash scripts/ffi/build-android.sh --abis arm64-v8a,x86_64 --profile dev   # the Rust core
./apps/android/gradlew -p apps/android :app:assembleDebug
```

The build checks that the core was built from the current Rust sources. If it wasn't, the build fails and tells you to
run `build-android.sh` again. The build never runs cargo itself.

## Install on your own device

There is no store release (ADR-0013 D14). Install with `adb`:

```sh
adb install -r apps/android/app/build/outputs/apk/debug/app-debug.apk   # io.github.dropumi.debug
```

- A release APK (`:app:assembleRelease`, `io.github.dropumi`) is signed only if you create
  `apps/android/keystore.properties` (gitignored); without it the APK is unsigned and cannot be installed.
- Build the release core (`--profile release`) for an APK you measure or hand out.

## First run

- Onboarding asks for a device name. The suggestion is `Dropumi-XXXX`, never your device's name.
- It then asks who may send:
  - paired devices only (the default);
  - everyone, for 10 minutes;
  - no one.
- Each phone, tablet and computer is a separate Dropumi device with its own key. Pair them once by comparing a
  six-digit code, or add one by its full fingerprint. To pair:
  1. open Pair a device on both devices (Settings › Pair a device, or Nearby);
  2. on one of them, tap Pair next to the other device;
  3. check that both screens show the same code, then tap Codes match on both.

  A device that does not list the other one uses "Not listed? Pair by address" and types an address the other
  device's Pair a device screen shows. A note or a result card says how each attempt ended. When this phone turns a
  device away, its notice offers Pair a device.
- **Other devices do not list this one.** Android's NSD may publish the device's host name, so the app advertises only
  on Android versions observed to publish a random one, and none has been observed yet (ADR-0013 D5). The app still
  lists other devices. Other devices reach it only by typing its address, shown under Settings › This device (in the
  `dropumi` CLI: `--to <address>`). A paired device stores no address, so this holds after pairing too; pairing only
  means no code to compare.
- Notifications are asked for only when they are needed, from the Transfers screen (Android 13 and later).

## Where things are

| | |
|---|---|
| Identity and paired devices | `noBackupFilesDir/dropumi` in the app's private storage, excluded from backups and device transfer (ADR-0014) |
| Received files | App-private storage (`filesDir/received`) until you Save (to Downloads on Android 10 and later, to a folder you pick on Android 8 and 9), Share or Open them. Saving never overwrites. |
| Receiving | Only while Dropumi is open. Leaving it stops accepting new connections; transfers already running finish, with a progress notification. |

## Tests

```sh
cargo build --locked -p dropumi-ffi --lib     # the host library the binding tests load
./apps/android/gradlew -p apps/android :core-bindings:testDebugUnitTest :core:testDebugUnitTest \
  :appstate:testDebugUnitTest :testsupport:testDebugUnitTest :app:testDebugUnitTest
bash scripts/android/ui-tests.sh              # instrumented tests on one emulator or device
bash scripts/android/interop.sh               # the app's core against the CLI (opt-in)
bash scripts/devices/interop.sh android       # the app against the Mac app or the iPhone app (opt-in, macOS host)
```

- The JVM tests run on the host. They never load the Android framework, and only the binding tests load the Rust core.
- The UI state tests run the app on a fake core. They are labelled "UI state test (fake core)", and they say nothing
  about the network or OS permissions.
- The real-core instrumented tests and the interop harness use temporary folders inside the app, never your real
  identity. Never run them on a device signed in to an account whose data you care about.
- The device-to-device tests (`devices_android_mac_pair_and_send`, `devices_iphone_android_pair_and_send`) run the
  qa app on a real phone (`ANDROID_SERIAL`) against the Mac app or the iPhone app over Wi-Fi
  (`DROPUMI_TEST_DEVICES=1`, plus `DROPUMI_TEST_APPLE_DEVICE` and `DROPUMI_TEST_LAN_ADDR` for the iPhone). The
  phone's side is the `agent` scenario of `AndroidInteropTest`; the script uninstalls the qa app and its test APK on
  exit.
- Real devices: one phone so far (Android 16, arm64-v8a), with the automated tests only: see the runs
  [2026-10-25-01](../../docs/device-tests/runs/2026-10-25-01.md) and
  [2026-10-26-01](../../docs/device-tests/runs/2026-10-26-01.md). Backups, device transfer and the manual device matrix in
  `docs/device-tests/` have not been run.
