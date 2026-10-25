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
- Each user's Mac and iPhone are separate Dropumi devices with separate keys. Pair them once by comparing the six-digit code (see [Pair two devices](#pair-two-devices)).

## Pair two devices

Pairing compares a six-digit code once. After that, the two devices send to each other without a code, and Dropumi checks each time that it is the same device.

1. Open Pair a device on both devices. It is under the Nearby list, in Settings, in Paired devices and, on the Mac, in the menu (Pair a Device…, ⇧⌘P).
   - The screen makes the device ready to pair: "Ready to pair" with the time left (5 minutes), the name other devices see, and Stop.
2. On one of them, tap Pair next to the other device.
3. Both devices show a code. Check that the codes are the same, then tap Codes match on both.
4. Both read "Paired with “…”". Done goes back to where you opened the screen.

- **Only the open screen makes a device ready.** Stop, Back, another tab or closing the window on the Mac ends it. Going to the background ends it too; coming back to the screen makes the device ready again. A code comparison under way is kept.
- **Receiving files is paused** while the device is ready to pair. It resumes when you leave the screen.
- **One attempt per opening.** When the 5 minutes run out, or a device connects without pairing, the screen says so. Tap Start again; it never reopens by itself.
- **Trust needs both people.** It is saved only when both tap Codes match. A device in the list is never shown as "Paired" before that.
- **Each outcome has its own text.** For example, the person who taps Codes don't match reads "You said the codes don't match", and the other device reads that the code was not confirmed. Nothing is saved on either side.
- **Not in the list?** Choose "Not listed? Pair by address" and type the address the other device shows on its Pair a device screen. Android phones are not listed here; pair them this way.
- **With the CLI.** `dropumi pair --listen` is ready like the open screen: tap Pair next to it. `dropumi pair --to <address>` pairs with an address the screen shows.

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
# On a real iPhone or iPad (unlocked, signed as above):
xcodebuild test -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-iOS \
  -destination 'platform=iOS,id=<UDID>' -derivedDataPath apps/apple/build -allowProvisioningUpdates
```

- `DropumiTests` and `DropumiMacTests` run inside the app with temporary folders, never your real identity.
- `DropumiUITests` and `DropumiMacUITests` run the app on a fake core. They are labelled "UI state test (fake core)", and they say nothing about the network or OS permissions.
- The Mac UI tests open their windows on the Mac's built-in display when it has one, so an external display stays free. They need UI automation approved once. Their `.xcresult` bundles contain a recording of the main display: do not share them.
- `scripts/apple/interop.sh` runs the app's real core against the CLI: on loopback (`apple_macos_*`), and over Wi-Fi on a real iPhone or iPad (`apple_ios_device_*`, with `DROPUMI_TEST_APPLE_DEVICE`); see `docs/testing.md`.
- The Nearby screen, Local Network permission prompts and everything a person sees on two devices are covered only by the manual device matrix in `docs/device-tests/`.
