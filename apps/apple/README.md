# Dropumi for iPhone, iPad and Mac

SwiftUI apps over the Rust core (ADR-0009, ADR-0012). Pre-alpha and unaudited.
What has actually been tested is listed in [`docs/testing.md`](../../docs/testing.md).
Nothing here is "supported".

| Target | What it is |
|---|---|
| `Dropumi-iOS` | The iPhone and iPad app |
| `Dropumi-iOS-ShareExtension` | "Dropumi" in the share sheet. It copies the shared files for the app and never loads the Rust core or uses the network (D1). |
| `Dropumi-macOS` | The Mac app, with a menu-bar receiver |

`DropumiKit/` is a local Swift package:

| Module | Contents |
|---|---|
| `DropumiFFI` | The Rust core (`DropumiFFI.xcframework`) |
| `DropumiBindings` | Generated bindings |
| `DropumiCore` | Paths, Bonjour, the core client |
| `DropumiAppState` | UI-free state |
| `DropumiDesign` | Tokens, fonts and components |
| `DropumiShare` | The share inbox |
| `DropumiTestSupport` | A fake core for tests |

The Xcode project is generated from `project.yml` and is not committed.

## Build

You need Xcode 27, xcodegen 2.46.0 and Rust with the Apple targets (see [`docs/development.md`](../../docs/development.md)).

```sh
bash scripts/ffi/build-apple.sh --profile dev     # the Rust core (all three slices)
(cd apps/apple && xcodegen generate)
open apps/apple/Dropumi.xcodeproj
```

A build phase checks that the core was built from the current Rust sources. If it wasn't, the build fails and tells you to run `build-apple.sh` again. The build never runs cargo itself.

## Install on your own devices

1. Signing. Copy `Config/Signing.local.xcconfig.example` to `Config/Signing.local.xcconfig` (gitignored) and set your team ID. Then run `xcodegen generate` again.
   - Xcode's automatic signing registers the App IDs:
     - `io.github.dropumi.Dropumi`;
     - `io.github.dropumi.Dropumi.share`;
     - the App Group `group.io.github.dropumi`.
2. iPhone or iPad:
   - In Xcode, pick the `Dropumi-iOS` scheme and your device, and press Run.
   - From the command line:
     ```sh
     xcodebuild build -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-iOS \
       -destination 'generic/platform=iOS' -derivedDataPath apps/apple/build -allowProvisioningUpdates
     xcrun devicectl list devices
     xcrun devicectl device install app --device <device id> \
       apps/apple/build/Build/Products/Debug-iphoneos/Dropumi.app
     ```
   - The first launch on a device needs Settings › General › VPN & Device Management › trust your developer certificate.
3. Mac:
   - Pick the `Dropumi-macOS` scheme and press Run.
   - Or build it and open `apps/apple/build/Build/Products/Debug/Dropumi.app`.

## First run

- Onboarding asks for a device name. The suggestion is `Dropumi-XXXX`, never your device's name.
- It then says who may send:
  - Paired only (the default);
  - Everyone for 10 minutes;
  - no one.
- Receiving starts at the end of onboarding. At that point the system asks for Local Network access. Choose Allow, or Dropumi cannot see other devices.
- Each user's Mac and iPhone are separate Dropumi devices with separate keys. Pair them once (Settings › Pair a device) by comparing the six-digit code.

## Where things are

| | iPhone and iPad | Mac |
|---|---|---|
| Identity and paired devices | `Library/Application Support/Dropumi` in the app container, excluded from backups (ADR-0014) | `Application Support/Dropumi` in the sandbox container, excluded from backups |
| Received files | Files › On My iPhone/iPad › Dropumi › Received | `~/Downloads/Dropumi`, or a folder you choose in Settings |
| Receiving | Only while Dropumi is open. In the background it stops accepting, and running transfers may finish. | Also while the window is closed, from the menu bar. "Quit Dropumi completely" stops it. |

## Tests

```sh
swift test --package-path apps/apple/DropumiKit
xcodebuild test -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-iOS \
  -destination 'platform=iOS Simulator,name=iPhone 17,OS=27.0' -derivedDataPath apps/apple/build
xcodebuild test -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-macOS \
  -destination 'platform=macOS,arch=arm64' -derivedDataPath apps/apple/build
```

- `DropumiTests` and `DropumiMacTests` run inside the app with temporary folders, never your real identity.
- `DropumiUITests` and `DropumiMacUITests` run the app on a fake core. They are labelled "UI state test (fake core)", and they say nothing about the network or OS permissions.
- Real devices, Bonjour and Local Network permission are covered only by the manual device matrix in `docs/device-tests/`.
