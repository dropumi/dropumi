# ADR-0014: Interim key storage on the app platforms

- **Status:** Accepted
- **Date:** 2026-10-14
- **Related:** ADR-0003, ADR-0009, ADR-0012 (Apple apps, milestone 2b), ADR-0013 (Android app, milestone 2c), [`../protocol/threat-model.md`](../protocol/threat-model.md)

## Context

- ADR-0003 stores the identity as a 0600 key file with owner and mode checks, and plans OS secure storage (iOS Keychain, an Android Keystore-wrapped key) for later phases "behind the same `IdentityStore` port". It does not say what the first app builds use.
- Milestone 2 ships apps for iPhone, iPad, Mac and Android, on top of `dropumi-ffi` (ADR-0009).
- A key file in app storage is protected by the OS sandbox and by the platform's data-at-rest encryption, but by default it is included in backups. A backup restored onto another device would give two devices the same identity, which pairing and trust cannot tell apart.
- Keychain items need signing entitlements (keychain access groups), which unsigned CI builds and some test setups do not have.
- snow needs the raw 32-byte X25519 secret to run the handshake. A key that never leaves Android Keystore would mean redoing the Noise DH outside snow; whether Keystore holds X25519 keys natively depends on the device (UNVERIFIED).

## Decision

For milestone 2, the apps keep the ADR-0003 key file, unchanged, in app-private storage that is **excluded from backups and device-to-device transfer**:

| Platform | Configuration directory | Exclusion and protection |
|---|---|---|
| iOS, iPadOS | `Library/Application Support/Dropumi` inside the app container (never the App Group) | `isExcludedFromBackup` on the directory; data protection class `completeUntilFirstUserAuthentication` |
| macOS | `Application Support/Dropumi` inside the app's sandbox container | The sandbox; `isExcludedFromBackup` on the directory, which Time Machine honours (reconciled 2026-10-15 with the app's code; the CLI's directory is still backed up) |
| Android | `noBackupFilesDir/dropumi` (never `filesDir` itself, whose mode is 0771) | `android:allowBackup="false"`, plus `dataExtractionRules` (API 31+) and `fullBackupContent` (API 26-30) that exclude every domain from both cloud backup and device transfer |

- The node creates the directory with mode 0700 and runs the ADR-0003 owner and mode checks unchanged, on every platform.
- The secret never crosses the FFI: no exported type or call carries it (ADR-0009).
- The Keychain (iOS and macOS, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, not synchronizable) and Android Keystore adapters come in a later milestone, behind the existing `IdentityStore` port, as ADR-0003 plans.

## Implementation

- `dropumi-ffi` takes the configuration directory from the app (`NodeConfig::config_dir`, absolute) and opens `FileIdentityStore` and `TrustFile` there; it never calls `paths::config_dir(None)`.
- The platform settings in the table (backup exclusion, data protection, manifest attributes) are set by the apps in milestones 2b and 2c and recorded in ADR-0012 and ADR-0013 with their tests.
- Apple apps (2b): `AppPaths.protectConfig` sets `isExcludedFromBackup` on the directory on iOS and macOS (so on the Mac, Time Machine skips it too) and the data protection class on iOS. `TestHostTests configFolderIsExcludedFromBackups` checks the flag and mode 0700: Automated test passed (iOS 27.0 simulator and macOS 27.0.1 arm64, local run). On devices: Not tested.
- Android app (2c-1, ADR-0013 D3): `AppPaths` puts the configuration folder in `noBackupFilesDir/dropumi` and app data (history) in `noBackupFilesDir/app`; the core creates the folder 0700. `configFolderIsPrivateAndExcludedFromBackups` (folder 0700, files 0600, `FLAG_ALLOW_BACKUP` clear) and `backupRulesExcludeEveryDomain` (both rule files exclude every domain, cloud backup and device transfer) are instrumented tests on the real core: Automated test passed (Android 8.0, API 26, and Android 9, API 28, `google_apis` emulators; Android 16, API 36.1, Play `Tablet` emulator; all arm64-v8a, macOS 27 arm64 host, 2026-10-16), and on one Android 16 arm64-v8a phone (2026-10-26, run [2026-10-26-01](../device-tests/runs/2026-10-26-01.md)). The merged-manifest audit (`scripts/android/check-manifest.sh`) checks that the release, `qa` and debug manifests set `allowBackup="false"` and both rule files. A backup or device transfer actually run (`bmgr` with the local transport on a rootable image, or a real device): Not tested.
- Status: the key-file code and checks are the CLI's: Automated test passed (macOS 27.0.1 arm64, local run); on Android, see the line above.

## Consequences

### Positive

- The same code, the same file format and the same checks as the CLI; no new secret-handling code.
- Unsigned CI builds and tests can create and use identities.
- Excluding the directory from backups prevents a restored backup from cloning the identity.

### Negative

- The key is a file readable by any code running as the app's user ID, and by root.
- On macOS, Time Machine still includes the CLI's key; the app's key is excluded.
- Backup exclusion depends on each OS honouring the flags; whether iOS device-to-device migration honours `isExcludedFromBackup` is UNVERIFIED.
- A later move to the Keychain or Keystore needs a migration step.

## Alternatives considered

- **Keychain and Keystore now:** stronger at rest, but needs entitlements that break unsigned CI, and an X25519 key cannot be used by snow from inside Keystore; deferred.
- **App Group container on iOS:** would let the Share Extension read the identity, which it does not need (it never loads the core), and a lock held in a shared container can get a suspended app killed.
- **Default app storage without backup exclusion:** a restored backup would clone the identity.
