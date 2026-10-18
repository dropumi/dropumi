# Manual device testing

How to run Dropumi on each platform and test it by hand on real devices: the Mac app, the iPhone app, the Android app,
and the browser research pages. Automated tests are in [`docs/testing.md`](../../testing.md). Results go into a run
file copied from [`TEMPLATE.md`](../TEMPLATE.md).

| Page | What it covers |
|---|---|
| This page | Before you start (devices, branches, privacy, recording), test files, the CLI as a test peer, and the session plan (pairs P1 to P6 and their order) |
| [Mac app](mac.md) | Build, sign, launch and reset the Mac app; the Mac checklist |
| [iPhone app](iphone.md) | Build, install, launch and reset the iPhone app; getting files on and off the phone; the iPhone checklist |
| [Android app](android.md) | Build, install, launch and reset the Android app; the Android checklist |
| [Web research pages](web.md) | Run the ADR-0011 research pages and record the manual web rows. Dropumi has no browser feature |

Read this page first. Every checklist item has steps, the result to expect (with the app's exact text) and what to
record:
- items marked "(extra)" are not rows of `TEMPLATE.md`, and each names its source;
- "(not confirmed in code: check and note what you see)" marks a detail the code did not settle;
- run each row with the labels `Manual device test passed`, `Failed` (with what happened) or `Not tested`.

This guide was written on 2026-10-18 against these branches:
- `feat/native-apps-apple` at `19a6d50`;
- `feat/native-apps-android` at `06b5348`, with its uncommitted doc edits;
- `docs/web-local-page-research` at `5bd0671`.

The facts were collected from the code by reading, and each section was checked against the code, but strings and
commands can drift. When an app shows something else, record what it shows.

## Before you start

This guide covers the **manual** device tests that the maintainer runs by hand on real devices. Automated tests and their current labels are in `docs/testing.md`, and this guide does not repeat them. Never use simulators or emulators for any row here.

`<APPLE worktree>`, `<ANDROID worktree>` and `<WEB worktree>` stand for the three worktree folders. Their paths contain spaces, so always put them in quotes. Every section uses these three names. If you see `<APPLE worktree root>` or `<Android worktree>`, they mean the same folders.

### Terminal on the Mac

- The Mac's Terminal runs zsh. In an interactive zsh, `#` does not start a comment unless `interactivecomments` is set, and macOS does not set it. So a pasted line with a trailing `# comment` fails or does something else.
- Code blocks in this guide should have no `#` comments. The explanations are in the text under each block. If a line in a block ends in ` # …`, do not paste that part. Or run this in that window first:
  ```sh
  setopt interactivecomments
  ```
- **Review check for this guide.** On a saved copy of the guide, this command prints every line inside an `sh` block that has a space before `#`:
  ```sh
  awk '/^ *```sh/ {f=1; next} /^ *```/ {f=0} f && /[ ]#/ {print FNR": "$0}' "<guide file>"
  ```
  Expect no output, or only lines where that `#` is inside quotes. Remove comments after `adb shell` too, even though the phone's shell treats them as comments.

### Devices, and the branch each one builds from

| Device | OS and setup | What runs on it | Build from |
|---|---|---|---|
| Mac | macOS 27.0.1, arm64, Xcode 27.1, with an external display | Mac app, the `dropumi` CLI, Terminal (`dns-sd`), and Safari, Chrome and Brave for the web pages | Mac app: `feat/native-apps-apple` (PR #7), in `<APPLE worktree>`. CLI: any worktree |
| iPhone 15 | iOS 27.0.1, Developer Mode on, paired with the Mac | iPhone app, Safari | `feat/native-apps-apple` (PR #7) |
| Samsung Galaxy A54 | Android 16 (One UI), arm64-v8a, secure lock screen (PIN) | Android app (debug build), Chrome | `feat/native-apps-android` (no PR yet), in `<ANDROID worktree>` |
| Browsers | as above | Web research pages only | `docs/web-local-page-research` (PR #11), in `<WEB worktree>` |

- `<ANDROID worktree>` also holds an **older** copy of the Apple app. Build the Apple apps only from `<APPLE worktree>`.
- These are byte-identical in `<APPLE worktree>` and `<ANDROID worktree>`:
  - `crates/cli/src/` and the crates it links (`domain`, `protocol`, `application`, `infra`);
  - `docs/cli.md`, `Cargo.lock`;
  - `scripts/device-tests/`.

  Build the CLI once. This guide builds it in `<APPLE worktree>`.
- `<ANDROID worktree>` has uncommitted doc edits. Its `docs/device-tests/TEMPLATE.md` has 45 rows. It adds BAK-1, OVL-1, NSD-1, EXP-1 and INST-1, and it extends NET-5. The Apple copy has 40 rows. Use the 45-row copy.
- The web pages are research for ADR-0011. They are **not** a Dropumi feature and have no template row.
  - Record their results in ADR-0011, as `poc/web/README.md` says.
  - Copy only the **Result** blocks, `results.jsonl` lines and the `run-matrix.py` summary rows. By design, these hold no addresses. Read any `errors` text first, and redact anything in it that names an address or a path.
  - Never paste the SDP texts, because they contain local addresses. Never paste the access log that `serve.py` prints, because it contains the clients' addresses.

### One Wi-Fi network

- Put every device on the same Wi-Fi network. The exceptions are:
  - NET-1 and NET-2 (Wi-Fi turned off);
  - NET-3 (the access point reboots);
  - NET-4 (a network with client isolation on);
  - NET-5 (the Android phone and the Mac on a Wi-Fi network without internet).
- Record the band and whether client isolation is on (TEMPLATE › Setup › Network). Never record the network's name.
- `ipconfig getifaddr en0` prints the Mac's Wi-Fi address (`scripts/apple/interop.sh` reads `en0` too). Use it when you type addresses. Never write it down.
- The Android app is **browse-only on every device**. Its R21 allowlist is empty, so it does not advertise. It lists the other devices, but no other device lists it. To reach it, use its address from Settings › This device › Address. Type it in an app's Connect by address, or pass `--to <address>:<port>` to the CLI.
- The Android app receives only while it is open. When you leave the app, it stops accepting new transfers. The iPhone app also receives only while it is open.

### Privacy

- Record device models and OS builds only. Keep the default `Dropumi-XXXX` device names. Never record any of these:
  - Wi-Fi names or IP addresses;
  - UDIDs or serials;
  - user names or home paths;
  - screenshots that show any of the above.
- Never paste fingerprints of real devices, host names or MAC addresses.
- **Redact all pasted `dropumi` output**, human and `--json`. Replace fingerprints with `XXXX-…` and addresses with `<address>:<port>`.
  - In human output, the `Connected:`, `Paired:`, `Accept … file(s) from …`, `Verification code` and `trust list` lines show the other device's display fingerprint.
  - `Fingerprint:` and `Full:` show the CLI's own fingerprint.
  - `Listening on …` shows the Mac's address when `--bind` is used.
  - In `--json` output, `peer` objects carry full fingerprints, `connecting` carries the address that was dialled, and `failed.message` may contain local paths.
- Apply the same redactions to pasted `dropumi discover`, `dns-sd -B`, `dns-sd -L` and `adb logcat` output.
- **Paths:**
  - Replace `$HOME` with `~`.
  - The worktrees are on an external volume, not under `$HOME`. Replace their paths with `<APPLE worktree>`, `<ANDROID worktree>` or `<WEB worktree>`, and any other `/Volumes/...` path with a placeholder.
  - Build tools (cargo, Gradle, xcodebuild) print absolute paths. So do errors such as `receive directory unavailable: <path>`.
- **Serials and UDIDs:**
  - `scripts/android/ui-tests.sh` and `scripts/android/interop.sh` both print `device <serial>: API …`. Replace the serial with `<serial>`.
  - Never paste `adb devices` or `xcrun devicectl list devices` output. Between them, they show the serial, the device name and the UDID.
- **Owner columns:** never paste `ls -l`, `ls -la` or `ls -lt` output from the Mac, because it shows the macOS user name. This guide uses `ls -1A`, or `ls -lAn` when you need sizes (numeric IDs only).
- **About and version pages:** iOS Settings › General › About shows the device name and serial. Android's About phone shows the device name and IMEI. `chrome://version` and `brave://version` show "Profile Path" and "Executable Path" with the home folder. Copy only the version line, by hand. Never screenshot these pages.
- Never share an `.xcresult` bundle. Mac UI tests record the **main** display into it, even while the app's windows are on the built-in display. With an external display in use, that recording can show anything on that display.
- The CLI always uses throwaway folders made with `mktemp -d`. Never use the real `~/Library/Application Support/dropumi`.
- **Account data:**
  - Run the Android instrumented tests, `bmgr` and OVL-1 only on a phone that holds no account data you care about. OVL-1 runs `scripts/android/ui-tests.sh --class io.github.dropumi.prompt.OverlayWindowTest`, which installs test APKs and runs instrumented tests on the phone. `docs/development.md` says: "never run them, or `bmgr` backup commands, on a device signed in to an account whose data you care about".
  - If the A54 holds such data, record OVL-1 as `Not tested`.

### Where each device shows its fingerprint

| Device | Where | What it shows |
|---|---|---|
| CLI | `"$DROP" identity show` | `Fingerprint:` is the 24-character display form. `Full:` has all 52 characters |
| Mac app, iPhone app | Settings › the card at the top with the device's name ("Nearby devices see this name") › This device › "Fingerprint" | All 52 characters, in 13 groups of 4 |
| Android app | Settings › the card at the top with the device's name › This device › "Fingerprint" | All 52 characters, in 13 groups of 4 |

- Each app's Settings › Paired devices shows only the 24-character display form. So do the CLI's `Fingerprint:` line and `trust list`.
- Compare fingerprints on screen. Never write them down or paste them.

### How results are recorded

1. **Choose the branch for the run file.** The docs do not say which branch takes a run that covers both apps, so ask the maintainer (not confirmed in code: check and note what you see).
   - Name the file by date. `2026-10-18-01` already exists on `feat/native-apps-apple` (commit `233b64f`), so a second session today is `2026-10-18-02`.
   - Always copy the 45-row template from `<ANDROID worktree>`.
   - **On `feat/native-apps-apple`:**
     ```sh
     cp "<ANDROID worktree>/docs/device-tests/TEMPLATE.md" "<APPLE worktree>/docs/device-tests/runs/<YYYY-MM-DD>-<nn>.md"
     ```
     Its `runs/README.md` already has the `| Run | Devices | What ran |` table.
   - **On `feat/native-apps-android`:**
     ```sh
     cp "<ANDROID worktree>/docs/device-tests/TEMPLATE.md" "<ANDROID worktree>/docs/device-tests/runs/<YYYY-MM-DD>-<nn>.md"
     ```
     - Its `runs/README.md` has no table yet. It still says "No runs are recorded yet.".
     - Replace that sentence with the header `| Run | Devices | What ran |`, then `|---|---|---|`, then your row.
     - The 2026-10-18-01 row exists only on `feat/native-apps-apple`, so this README will need a merge when both branches land.
2. **Fill in Setup.** Three branches are in play, so give each result the commit it ran on.
   ```sh
   git -C "<APPLE worktree>" rev-parse --short=12 HEAD
   git -C "<ANDROID worktree>" rev-parse --short=12 HEAD
   git -C "<WEB worktree>" rev-parse --short=12 HEAD
   git -C "<APPLE worktree>" status --short -- apps/apple crates scripts
   git -C "<ANDROID worktree>" status --short -- apps/android crates scripts
   cat "<APPLE worktree>/apps/apple/DropumiKit/Artifacts/core-stamp.txt"
   cat "<ANDROID worktree>/apps/android/core-bindings/build/rust/core-stamp.txt"
   sw_vers
   xcodebuild -version
   adb shell getprop ro.product.model
   adb shell getprop ro.build.version.release
   adb shell getprop ro.build.version.sdk
   adb shell getprop ro.product.cpu.abi
   adb shell getprop ro.build.version.security_patch
   adb shell pm list packages --apex-only --show-versioncode | grep -i tethering
   ```
   - Commits have 12 characters. The Android app's build shows the same 12-character commit in Settings, next to the version (`apps/android/app/build.gradle.kts`). Check that it matches.
   - When this guide was written, the Android `status` showed ` M apps/android/app/src/test/kotlin/io/github/dropumi/ui/design/TokensTest.kt` and `?? apps/android/README.md`. If that is still all it shows, record "dirty: unit test and docs only". Otherwise, record what changed.
   - Each `core-stamp.txt` has a `profile` line. Record "Debug or Release" and each core profile once, here.
   - The repo scripts read `ro.build.version.sdk` and `ro.product.cpu.abi`. No repo script reads `ro.product.model`, `ro.build.version.release` or `ro.build.version.security_patch` (not confirmed in code: check and note what you see).
   - For the iPhone's iOS build, read Settings › General › About › iOS Version. Copy only that line, by hand. Do not use `devicectl` device-info output, which shows the UDID and serial.
3. **Give each row one label:**
   - `Manual device test passed`;
   - `Failed`, saying what happened;
   - `Not tested`.

   Rules for the label:
   - A TEMPLATE row is `Failed` if any of its Expect items fails. Name that item in Notes.
   - If part of a row could not run (no keyboard, no tablet), write "Not tested: <part>" in Notes, and label the row by what ran.
   - MAC-UI and MAC-BONJOUR are automated rows. The 2026-10-18-01 run labelled them `Automated test passed`.
   - Never write "supported".

   **The Pair column** (TEMPLATE: "Pair (sender → receiver)") has one format in every section. Write the pair number, then sender → receiver, using M = Mac app, I = iPhone app, A = Android app and C = CLI. Examples: `P3 C → M (same Mac)`, `P6 A → C`, `P1 I → M`. If a row ran both ways, write both: `P1 I → M; P1 M → I`. Use this format even where an app section words the pair differently.
4. **Paste real output only,** with the redactions above.
5. **Add a row** (Run | Devices | What ran) to `docs/device-tests/runs/README.md` on the branch you chose in step 1.
6. **Update `docs/testing.md` from passed rows only.** Failed rows stay in the run file.
   - Apple-app labels go in `<APPLE worktree>/docs/testing.md`, which has the Apple device rows.
   - Android-app labels go in `<ANDROID worktree>/docs/testing.md`. Only that copy has the Android rows.
   - P4 and P5 involve both apps. Ask the maintainer where those labels go (not confirmed in code: check and note what you see).
7. **Commit on a feature branch,** never on `main`. Stage explicit paths. Run this from the worktree of the branch you chose:
   ```sh
   git add docs/device-tests/runs/<YYYY-MM-DD>-<nn>.md docs/device-tests/runs/README.md
   git commit -m "docs: record device test run <YYYY-MM-DD>-<nn>"
   ```

### Build and install the apps

**One build per platform for the whole session:**
- Release Apple apps.
- A debug Android APK.
- `--profile release` Rust cores for both apps, because TX-6 measures time.

Build each app once, here. If an app section's "Run it" builds again, skip that build. Where a section says `--profile dev`, use `--profile release` instead. Do not rebuild a core during the session. If you must (for example, for the Android section's TX-6 rebuild), add a second Setup line with the new `core-stamp.txt`, and say which rows ran on which build.

**Tools** (`docs/development.md`, `apps/apple/README.md`, `apps/android/README.md`):
- **Apple:** Xcode 27, xcodegen 2.46.0, and Rust with the Apple targets. Install xcodegen 2.46.0 as CI does: see the "Install xcodegen 2.46.0" step in `.github/workflows/ci.yml`, which checks the zip's SHA-256.
- **Android:** JDK 17; the Android SDK (platform 36, build-tools 36.0.0, NDK 27.1.12297006); and Rust with the Android targets.

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
rustup target add aarch64-linux-android x86_64-linux-android
```

**Android, from `<ANDROID worktree>`:**

```sh
cd "<ANDROID worktree>"
export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
export ANDROID_HOME="<Android SDK folder>"
bash scripts/ffi/build-android.sh --abis arm64-v8a,x86_64 --profile release
./apps/android/gradlew -p apps/android :app:assembleDebug
```

- Gradle finds the SDK through `ANDROID_HOME` or through `sdk.dir` in the untracked `apps/android/local.properties`. If that file sets `sdk.dir`, skip the `export ANDROID_HOME` line. Inside the quotes, write the SDK folder with `$HOME`, not `~`, because `~` is not expanded inside quotes.
- Always pass both ABIs. The script builds only `arm64-v8a` by default, and the app build refuses a core that lacks `x86_64`.
- Install only after PRE-4 has run, and record INST-1 while you install:
  ```sh
  adb install apps/android/app/build/outputs/apk/debug/app-debug.apk
  ```

**Apple, from `<APPLE worktree>`:**

```sh
cd "<APPLE worktree>"
bash scripts/ffi/build-apple.sh --profile release
cp -n apps/apple/Config/Signing.local.xcconfig.example apps/apple/Config/Signing.local.xcconfig
grep -c YOURTEAMID apps/apple/Config/Signing.local.xcconfig
(cd apps/apple && xcodegen generate)
```

- **Signing.** `cp -n` never overwrites an existing `Signing.local.xcconfig`, and one already exists in `<APPLE worktree>`.
  - If `grep -c` prints `1`, the file still holds the placeholder. Set `DEVELOPMENT_TEAM = <team ID>` in it, then run `xcodegen generate` again.
  - If it prints `0`, open the file and check that `DEVELOPMENT_TEAM` is your personal team.
  - The file is gitignored. Never commit it or paste it.
- **iPhone:**
  ```sh
  xcodebuild build -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-iOS -configuration Release -destination 'generic/platform=iOS' -derivedDataPath apps/apple/build -allowProvisioningUpdates
  xcrun devicectl list devices
  xcrun devicectl device install app --device <UDID> apps/apple/build/Build/Products/Release-iphoneos/Dropumi.app
  ```
  - Run `devicectl list devices` only to read the UDID. Never paste its output.
  - The first launch needs Settings › General › VPN & Device Management › trust your developer certificate.
- **Mac:**
  ```sh
  xcodebuild build -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-macOS -configuration Release -destination 'platform=macOS,arch=arm64' -derivedDataPath apps/apple/build
  open apps/apple/build/Build/Products/Release/Dropumi.app
  ```
  If signing fails, add `-allowProvisioningUpdates` (not confirmed in code: check and note what you see).
- `apps/apple/README.md` shows only the Debug commands (`Debug-iphoneos`, `Debug`). The `Release` paths above follow Xcode's usual layout (not confirmed in code: check and note what you see). Release builds leave out the fake core (`Config/Release.xcconfig`).

### Pre-flight checks

Run them in this order. PRE-4 must come before the Android install.

- [ ] **PRE-4 (extra; `docs/development.md`, `scripts/android/*.sh`)** No earlier Dropumi app on the phone. Steps: 1) `adb shell pm list packages | grep dropumi`. 2) Uninstall everything it lists, for example `adb uninstall io.github.dropumi.debug`, `adb uninstall io.github.dropumi.debug.test`, `adb uninstall io.github.dropumi.overlayhelper`, `adb uninstall io.github.dropumi.qa.test` and `adb uninstall io.github.dropumi.qa`. `ui-tests.sh` leaves the debug app and its test APK installed, and an old debug app keeps its identity, pairings and history. **Expect:** the `grep` prints nothing. **Record:** what you had to remove.
- [ ] **INST-1** Install the Android app. Steps: 1) Run the `adb install` above. 2) Watch the phone. **Expect:** the install completes. Note any Play Protect or developer-verification warning (not confirmed in code: check and note what you see). **Record:** that it was a first install, and any warning with its exact text.
- [ ] **PRE-7 (extra; this guide, "Test files")** Enough free space. Steps: 1) On the Mac, run `df -h ~`. 2) On the iPhone, open Settings › General › iPhone Storage. 3) On the Android phone, open its storage settings (not confirmed in code: check and note what you see). **Expect:** about 10 GiB free on the Mac, 5 GiB on the iPhone and 7 GiB on the Android phone. These are estimates, not measured. Each device keeps several 2 GiB copies, and a full disk gives "Not enough space" or `insufficient_space` failures that look like real defects. **Record:** enough or not, per device. No screenshots.
- [ ] **PRE-1 (extra; TEMPLATE › Setup)** Builds recorded. Steps: 1) Build and install the three apps as described above. 2) Run the commands in "How results are recorded", step 2. **Expect:** each `core-stamp.txt` has the line `profile release`. **Record:** the 12-character commits, the `status` result, "Release" (Apple) and "debug" (Android), each core profile, and the device models and OS builds.
- [ ] **PRE-2 (extra; run 2026-10-18-01 Finding 4)** Local Network allowed on the iPhone and the Mac app. Steps: 1) Finish onboarding on both. Receiving starts at the end of onboarding, and the system asks then. 2) Choose Allow. 3) If onboarding does not appear because the app was onboarded before, check Local Network for Dropumi in the system settings instead. On the Mac, that is System Settings › Privacy & Security › Local Network. On the iPhone, it is Settings › Privacy & Security › Local Network (not confirmed in code: check and note what you see). **Expect:** Nearby does not show "Local network access is off". **Record:** whether the system question appeared on each device.
- [ ] **PRE-6 (extra; app settings persist)** Each app starts clean. Steps: 1) On each app, open Settings › Paired devices. 2) If it lists anything, tap "…" on each entry, then "Remove…", then "Remove". 3) In Settings, check that "Auto-accept paired devices" is off. 4) Check that "Who can send to this device" is "Paired only". **Expect:** "No paired devices", auto-accept off, "Paired only". **Record:** anything you had to change.
- [ ] **PRE-3 (extra; run 2026-10-18-01 Finding 7)** Phones stay unlocked. Steps: 1) Unlock the iPhone and keep Dropumi in front. 2) Unlock the Android phone and keep Dropumi in front. **Expect:** both apps can show Nearby. **Record:** nothing, unless a phone locked during a row. If it did, note it in that row.
- [ ] **PRE-5 (extra; `apps/apple/README.md`)** Mac receive folder checked. Steps: 1) `ls -1A ~/Downloads/Dropumi`. 2) If it holds files you care about, choose another folder in the Mac app: Settings › "Save received files to" › "Change…". UI-4 checks this choice later. **Expect:** an empty receive folder. **Record:** default folder or chosen folder (no path).

## Test files

Make **one** set for the whole session, here. The script is the same in `<APPLE worktree>` and `<ANDROID worktree>`.

```sh
cd "<APPLE worktree>"
FILES="$(mktemp -d)"
bash scripts/device-tests/make-test-files.sh "$FILES" 2048
```

- **Expect:** the last line is `made 306 files in <dir>; check them on the receiver with: shasum -a 256 -c SHA256SUMS`.
- The folder must be empty. Otherwise the script stops with `error: <dir> is not empty`.
- Without the second argument, the large file is 64 MiB. TX-6 needs at least 2 GiB, so pass `2048`.
- The script writes new random bytes on every run. Files from one set never match another set's `SHA256SUMS`. If an app section makes its own `FILES` or `BIG` set, skip that step and use this `FILES`. The 64 MiB file is not needed, because every row works with this set.
- `FILES` exists only in the terminal that set it. Run `echo "$FILES"` and set the same value in any other terminal. Do not paste the path into the run file.
- `SHA256SUMS` lives in the same folder. Never send it.

| File | What it is | Rows |
|---|---|---|
| `small.txt` | `Dropumi device test` plus a newline (20 B) | TX-1, TX-3 |
| `empty.bin` | 0 B | TX-8 |
| `photo-like.bin` | 3,145,735 B of random data (3 MiB + 7 B) | TX-1 |
| `large-2048MiB.bin` | 2 GiB of random data | TX-6, CAN-1, CAN-2, KILL-1, NET-1, NET-2 |
| `Résumé – naïve 日本語.txt` | Unicode name | TX-8 |
| `invoice<U+202E>txt.exe` | right-to-left override in the name | TX-8 |
| `many/file-001.bin` … `many/file-300.bin` | 300 files of 1,024 B of random data | TX-7 |
| `SHA256SUMS` | 306 lines, with paths such as `./small.txt` and `./many/file-001.bin`. Never sent | all |

**SHARE-1 media.** SHARE-1 needs a small photo and a 1 GB video in iOS Photos, and the test-file set has neither.
- Record a throwaway photo and a throwaway video of about 1 GB on the iPhone. They must show nothing private (for example, a plain wall).
- Use them only for SHARE-1, and delete them afterwards.
- The repo gives no way to get a reference hash of a Photos original (not confirmed in code: check and note what you see).

### After each 2 GiB row

Once the row is recorded and its hashes are checked, delete the received copies of `large-2048MiB.bin`:
- **Mac** (the CLI folder and the Mac app's folder; use the folder chosen in PRE-5 if you changed it):
  ```sh
  find ~/Downloads/Dropumi "$RX" -name 'large-2048MiB*' -delete
  ```
  If you pulled files off a phone into `"$D"`, remove that folder too: `rm -rf -- "${D:?}"`.
- **iPhone:** in Files › On My iPhone › Dropumi › Received, delete the `large-2048MiB` copies.
- **Android:** in Transfers, use "Delete" on that entry ("Delete these files?" › "Delete"). Do **not** use "Clear".

### Why a plain `shasum -a 256 -c SHA256SUMS` reports failures

This follows from the receive rules in `docs/cli.md`. It was not run.

- A folder cannot be sent (`send` refuses directories), so send `"$FILES"/many/*`. Receivers keep only the last part of the path, so the 300 files land flat, and the `./many/…` paths do not exist.
- Receivers strip U+202E from names, so the right-to-left file is saved as `invoicetxt.exe`.
- A reused receive folder gives `name (1).ext`, `name (2).ext`, and so on. Nothing is overwritten.
- Files you did not send show as missing.

### Sending all 306 files at once (CLI as sender)

```sh
(cd "$FILES" && "$DROP" send small.txt empty.bin photo-like.bin large-2048MiB.bin Résumé* invoice* many/* --to <address>:<port>)
```

Do not use `"$FILES"/*`. It includes the `many` folder, which makes the whole send fail with exit 2. It also includes `SHA256SUMS`.

### Check on the Mac (CLI folder, Mac app folder, or files copied off a phone)

None of these were run while writing this guide (not confirmed in code: check and note what you see). Run them in the received folder:

```sh
cd "<received folder>"
```

To check only the files you sent, by name:

```sh
grep -F -e './small.txt' -e './photo-like.bin' "$FILES/SHA256SUMS" | shasum -a 256 -c -
```

To check all 306 files, after sending everything into one empty folder:

```sh
sed -e 's#  \./many/#  ./#' -e $'s/\xe2\x80\xae//' "$FILES/SHA256SUMS" | shasum -a 256 -c -
```

To check only the 300 files of TX-7:

```sh
sed -n 's#  \./many/#  ./#p' "$FILES/SHA256SUMS" | shasum -a 256 -c -
```

To check by content when names changed (duplicates). Expect no output:

```sh
comm -13 <(cut -d' ' -f1 "$FILES/SHA256SUMS" | sort -u) \
         <(find . -type f ! -name '.DS_Store' ! -name '.dropumi-*' -exec shasum -a 256 {} + | cut -d' ' -f1 | sort -u)
```

To find partial files left behind (CAN-1, CAN-2, KILL-1):

```sh
ls -1A | grep '^\.dropumi-' || echo "no partial files"
```

- **Expect:** one `<name>: OK` line per file, and no `FAILED`.
- The Mac app saves to `~/Downloads/Dropumi`, or to the folder chosen in Settings.

### iPhone as receiver

- Received files are in Files › On My iPhone › Dropumi › Received.
- The repo gives no command to copy them to the Mac. Use one of these routes:
  1. **Send them back to the CLI.**
     - Give the CLI an empty receive folder first: `rm -rf -- "${RX:?}" && RX="$(mktemp -d)"`.
     - Start `receive`. In the iPhone app, choose the files from On My iPhone › Dropumi › Received and send them.
     - Then run the Mac checks in `$RX`. A pass shows that the round trip kept every byte. (Not confirmed in code: check and note what you see.)
  2. **Finder.** Select the iPhone, open Files › Dropumi, and drag `Received` into a `mktemp -d` folder. The app's `Info.plist` sets `UIFileSharingEnabled`, but this route was not run (not confirmed in code: check and note what you see).
  3. **devicectl.** Check the flags with `xcrun devicectl device copy from --help` first. (Not confirmed in code: check and note what you see.)
     ```sh
     xcrun devicectl device copy from --device <UDID> --domain-type appDataContainer \
       --domain-identifier io.github.dropumi.Dropumi --source Documents/Received --destination <empty mktemp -d folder>
     ```
- To **send** test files from the iPhone, first receive them from the CLI or the Mac app. Then pick them from Received.

### Android as receiver

- Received files stay app-private in `files/received` until you Save, Share or Open them.
- The debug build should allow `run-as`. None of these commands are in the repo (not confirmed in code: check and note what you see).
  ```sh
  adb shell run-as io.github.dropumi.debug ls -la files/received
  adb exec-out run-as io.github.dropumi.debug cat 'files/received/small.txt' | shasum -a 256
  adb exec-out "run-as io.github.dropumi.debug cat 'files/received/Résumé – naïve 日本語.txt'" | shasum -a 256
  grep -F './small.txt' "$FILES/SHA256SUMS"
  ```
  Compare each hash with the line that the last command prints.
- **EXP-1 (saving).** On a Transfers history entry, tap "Save to Downloads". On Android 8 and 9, the button reads "Save to…".
  - On Android 10 and later, the copies go to `Download/Dropumi/` (`DOWNLOADS_FOLDER` in `ReceivedExport.kt`).
  - Save checks the file's identity and size and counts the copied bytes. It does not hash. The SHA-256 check on the Mac is the content check.
  - These commands are not in the repo (not confirmed in code: check and note what you see). After the second command, run the Mac checks above.
    ```sh
    adb shell sha256sum /sdcard/Download/Dropumi/small.txt
    D="$(mktemp -d)"; adb pull /sdcard/Download/Dropumi "$D"; cd "$D/Dropumi"
    ```
- **Do not use Transfers › "Clear"** to tidy up. It deletes every received file ("Files you received are deleted from Dropumi too."). To remove one transfer's files, use "Delete" on that entry.

### Test files on the Android phone (for sending from it)

Copy the set once, as the Android section does:

```sh
adb push "$FILES" /sdcard/Download/dropumi-test-files
```

- This copies all 306 files **and** `SHA256SUMS`. Never pick `SHA256SUMS` to send.
- The phone needs more than 2 GiB free for the copy (PRE-7).
- Keep test files out of `Download/Dropumi/`, which EXP-1 uses for the copies it saves.
- In the app, pick files from Download › dropumi-test-files (not confirmed in code: check and note what you see).

## The CLI as a test peer

### Build

```sh
cd "<APPLE worktree>"
cargo build --release --locked -p dropumi-cli
```

- The binary is `<APPLE worktree>/target/release/dropumi`. Each worktree has its own `target/`. Neither worktree has a release CLI yet.
- Build it only here. The CLI sources are identical in both worktrees, so do not build a second CLI in `<ANDROID worktree>`.
- The `mdns` feature is on by default. A `--no-default-features` build cannot run `discover`, and its name or fingerprint targets exit 2.

### Throwaway identities and a receive folder

```sh
export DROPUMI_CONFIG_DIR="$(mktemp -d)"
UNPAIRED="$(mktemp -d)"
SPOOF="$(mktemp -d)"
RX="$(mktemp -d)"
DROP="<APPLE worktree>/target/release/dropumi"
"$DROP" --version
"$DROP" identity show
```

- `DROPUMI_CONFIG_DIR` holds the main CLI identity for the whole session. This is the CLI that the apps pair with.
- `UNPAIRED` holds a second identity that is never paired. Use it for every unpaired or unverified run: `"$DROP" --config-dir "$UNPAIRED" <command>`. It replaces the iPhone section's tab 2, the Android section's unpaired tab and the Mac section's `--config-dir "$(mktemp -d)"` runs.
- `SPOOF` is only for SPOOF-1 (the iPhone section's tab 3). See below.
- `RX` is the receive folder. It must already exist, because the CLI never creates it.
- `DROP` is an absolute path, so it works from any folder.
- **App sections.** Each app section has its own CLI setup block. If you did this part, skip those blocks and reuse `DROP`, `DROPUMI_CONFIG_DIR`, `UNPAIRED`, `SPOOF`, `RX` and `FILES`. Otherwise the session ends with several 2 GiB file sets and several CLI identities paired with the apps.

**Expect** from `identity show`:
```
Name:        Dropumi-XXXX
Fingerprint: XXXX-XXXX-XXXX-XXXX-XXXX-XXXX
Full:        <FP52>
```

- Do not pass `--config-dir "$(mktemp -d)"` on each command. That makes a new identity every time.
- A second terminal needs the same `DROPUMI_CONFIG_DIR`, `UNPAIRED`, `SPOOF`, `RX`, `DROP` and `FILES`. `echo` each one and set the same values there. Do not paste them into the run file.
- The CLI locks its config files only while it changes them, so `trust list` works while `receive` runs.
- Keep the default `Dropumi-XXXX` names. Rename only for SPOOF-1, and only the `SPOOF` identity:
  ```sh
  "$DROP" --config-dir "$SPOOF" identity set-name "<the paired device's Dropumi-XXXX name>"
  "$DROP" --config-dir "$SPOOF" receive --dir "$RX" --port 0
  ```
  The last command advertises the spoofed name, so the apps can list it.
- **Fingerprint-prefix names.** The CLI reads some names as a fingerprint prefix instead: names made only of letters, the digits 2-7, spaces and hyphens, with 8 to 52 letters and digits (spaces and hyphens do not count).
  - `abc-defg` has only 7 letters, so it stays a name.
  - `Study lap` has 8 letters, so it becomes a prefix.
  - Default names always contain a 0, 1, 8 or 9, so they are never affected.
- **Missing receive folder.** If `--dir` points at a missing folder, `receive` exits 9 with `Error: receive directory unavailable: <path>: No such file or directory (os error 2)`. Redact the path.
- **Never leave out `--dir`.** Without `--dir` and `DROPUMI_RECEIVE_DIR`, the CLI saves into the real `~/Downloads`.

### Receive, so the phones can reach the Mac

Run this to read the Mac's Wi-Fi address. Never record it.

```sh
ipconfig getifaddr en0
```

Then start receiving:

```sh
"$DROP" receive --dir "$RX"
```

**Expect** (on stdout):
```
Listening on 0.0.0.0:<port> as "Dropumi-XXXX"
Fingerprint: XXXX-XXXX-XXXX-XXXX-XXXX-XXXX  (full: <FP52>)
Policy: trusted-only
```

- **Port.** The default is 47321. The Mac app also uses 47321 by default, so whichever starts first gets it, and the other falls back to a free port. Read the port from the `Listening on` line.
  - `--port 0` always picks a free port.
  - An explicit port that is busy fails with exit 5.
- **mDNS.** The CLI advertises by default, so the iPhone, Mac and Android apps can list it in Nearby. If mDNS cannot start, you get `Discovery unavailable (…); manual addresses still work.`
- **By address.** On a phone, use Connect by address and type `<Mac's Wi-Fi address>:<port>`. Then choose "Send files" or "Pair with this address".
- **Firewall and Local Network.** How the macOS firewall treats the CLI, and whether Terminal is exempt from Local Network, are Not tested. Write down any prompt you see.
- **Pairing.** `receive` refuses pairing sessions (`not_authorized`). Use `pair --listen` to pair (see "Pair in both directions").

| For | Command |
|---|---|
| Default: paired senders only, asks every time | `"$DROP" receive --dir "$RX"` |
| TX-3: unverified senders for 10 minutes, shows a code | `"$DROP" receive --dir "$RX" --policy everyone` |
| TX-2: paired senders without a prompt | `"$DROP" receive --dir "$RX" --accept trusted` |
| An unpaired receiver | `"$DROP" --config-dir "$UNPAIRED" receive --dir "$RX" --port 0` |
| Address only, no mDNS | `"$DROP" receive --dir "$RX" --bind "$(ipconfig getifaddr en0)" --port 0 --no-mdns` |
| Scripted answers (see below) | `"$DROP" --json --control-stdin receive --dir "$RX" --port 0 --no-mdns --once` |

- In the "Address only" row, use `--port 0` and read the port from the `Listening on` line. Do not pin 47321 there.
  - With the Mac app running, a pinned 47321 either fails with exit 5, or binds next to the app's wildcard listener and takes connections meant for the Mac app (not verified on the device).
  - That `Listening on` line shows the Mac's address, so redact it.

**Answering prompts in the terminal.** Prompts go to stderr. Only `y` or `yes` means yes. Anything else, or end of input, means no. For TX-5, answer `n`.

- Offer from an unverified device: `Accept N file(s) from Unverified device (claims "<name>") · fp XXXX-… · SAS NNN NNN? [y/N]`
- Code for an untrusted peer: `Verification code: NNN NNN  — check that the other device shows the same code.`
- A trusted peer shows `Session code: NNN NNN` instead.
- Each stored file prints `Saved <name>`.
- An offer nobody answers is rejected after 120 s.

### Send from the CLI

| For | Command |
|---|---|
| Send to a paired phone by address | `"$DROP" send "$FILES/small.txt" "$FILES/photo-like.bin" --to <phone address>:<port>` |
| TX-3: receiver in Everyone mode | `"$DROP" send "$FILES/small.txt" --to <phone address>:<port> --allow-unverified` |
| An unpaired sender | `"$DROP" --config-dir "$UNPAIRED" send "$FILES/small.txt" --to <phone address>:<port> --allow-unverified` |
| Name over mDNS (iPhone or Mac app only) | `"$DROP" send "$FILES/small.txt" --to "Dropumi-XXXX"` |
| TX-7: never pass the folder itself | `"$DROP" send "$FILES"/many/* --to <phone address>:<port>` |

Record the exit code after each send:

```sh
echo $?
```

- **The phone's address** is under the app's Settings › This device › Address.
  - "Not receiving right now." means the app is not receiving.
  - On Android, "No local network address right now. Connect to Wi-Fi." means the phone has no Wi-Fi address.
- **The Android app** is never found by name or fingerprint, because it does not advertise. Always use `--to <address>:<port>` for it.
- **Unpaired receivers.** Without `--allow-unverified`, sending to an unpaired receiver fails with `unverified_peer` (exit 6). With the flag, the CLI asks `Send to <peer line>?` and then `Does the other device show NNN NNN? [y/N]`.
- **Results.** Success is `Done: N file(s), <size> in X s` with exit 0. It is printed only after the receiver confirms that every file was stored. `Finished with failures: …` means exit 8.

### Pair in both directions

**The app starts and the CLI waits:**

1. Stop `receive` with Ctrl-C.
   - Otherwise, `pair --listen` takes another port and advertises a second entry with the same name and its own random instance.
   - The app then lists both entries, and the `receive` one refuses pairing with `not_authorized`.
2. Start the pairing window:
   ```sh
   "$DROP" pair --listen
   ```
   - The window lasts 5 minutes.
   - It accepts **one** attempt and then closes, whatever the result.
   - If nothing arrives, it ends with `timeout` (exit 5).
3. On the app, open Settings › Pair a device. Under "Pair with a nearby device", tap "Pair" next to the CLI's name. Or use Connect by address with `<Mac's Wi-Fi address>:<port>` (the port from `pair`'s own `Listening on` line), then "Pair with this address".
4. Afterwards, restart `receive` and read its port again.

**The CLI starts and the app waits:**

1. On the app, open Settings › Pair a device and tap "Wait for a device". The app shows "Waiting for a device · m:ss" (five minutes). Then it shows "On the other device, choose Pair a device, then this one:" and its name.
2. **Android** also shows `<address>:<port>` under "Other devices can't see this one nearby. On the other device, choose Connect by address and type:". Run:
   ```sh
   "$DROP" pair --to <address>:<port>
   ```
3. **The Apple apps** show only the name. `pair --to` browses for 3 s. Run:
   ```sh
   "$DROP" pair --to "Dropumi-XXXX"
   ```

**Expect:**
- The CLI shows `Pair with <peer line>?`, then `Does the other device show NNN NNN? [y/N]`.
- Answer `y` only if the app shows the same code.
- When both sides confirm, the CLI prints `Paired: Trusted "<alias>" (claims "<name>") · XXXX-…`.

**Other endings:**
- If you answer `n` (PAIR-2): `pairing_rejected`, exit 6, and nothing is stored.
- If the other device disconnects: `connection_lost`, exit 5.
- If you press Ctrl-C: `Pairing cancelled; nothing was stored.`

### Trust

| Command | Expect |
|---|---|
| `"$DROP" trust list` | One line per device, `XXXX-…  <alias>  (sas)`, or `No trusted devices.` |
| `"$DROP" --json trust list` | The `fingerprint` field holds all 52 characters |
| `"$DROP" trust remove <FP52>` | `Removed.` or `That device was not trusted.` |
| `"$DROP" trust add <FP52> --name "<alias>"` | Adds the device, or renames one that is already trusted, and prints the updated list |

- `add` and `remove` need all 52 characters. Otherwise they fail with exit 2.
- See "Where each device shows its fingerprint" for each app's full fingerprint.

### Discover

```sh
"$DROP" discover --timeout-ms 5000
```

- Each device is one line: `<name>  fp <8-character hint>  <ip>:<port>`. If nothing is found, you get `No devices found.` and exit 0.
- The Android app is never listed.
- Redact the output before pasting it.

### Cancelling with `--json` and `--control-stdin`

- **Plain terminal mode.** A line typed while no prompt is waiting is thrown away. So typing `{"cmd":"cancel"}` into a plain `receive` or `send` does nothing.
- **`--json`** writes one event per line on stdout, each with a `t_ms` value.
  - Prompts, codes and failures still go to stderr.
  - The `completed` event has `elapsed_ms`, which is useful for TX-6.
- **`--control-stdin`** takes every decision as a JSON line that you type. Offers are no longer asked as y/N:
  - `{"cmd":"accept"}` or `{"cmd":"reject"}`, after the `offer_received` event;
  - `{"cmd":"confirm"}` or `{"cmd":"deny"}`, after the `sas` event.
- **CAN-2 with the CLI as receiver.**
  1. Start the receiver:
     ```sh
     "$DROP" --json --control-stdin receive --dir "$RX"
     ```
  2. After `offer_received`, type `{"cmd":"accept"}`.
  3. During the transfer, type `{"cmd":"cancel"}`. The receiver cancels the running transfers and keeps listening.
- **CAN-1 with the CLI as sender.** The receiver must already be paired.
  1. Start the send:
     ```sh
     "$DROP" --control-stdin send "$FILES/large-2048MiB.bin" --to <address>:<port>
     ```
  2. During the transfer, type `{"cmd":"cancel"}`. Expect exit 4.
- Ctrl-C also cancels, with exit 130.
- `{"cmd":"quit"}` stops `receive` with exit 0.

### Ctrl-C and exit codes

- The first Ctrl-C cancels, and the process exits **130**. A second Ctrl-C exits at once.
- `receive` waits at most 4 s for open sessions, plus up to 2 s to withdraw mDNS.
- A later `receive` removes `.dropumi-*.part` files only when they are older than 15 minutes. For the CAN and KILL rows, run this right after the cancel:
  ```sh
  ls -1A "$RX"
  ```

| Exit | Meaning |
|---|---|
| 0 | Success (also `receive` stopped with `quit`) |
| 1 | Internal error |
| 2 | Usage error: a target that cannot be resolved, a folder given as a file, or mDNS unavailable |
| 3 | Rejected by the receiver |
| 4 | Cancelled |
| 5 | Network: connection lost or refused, timeout, busy, or a bind failure |
| 6 | Auth or trust: `unverified_peer`, `pairing_rejected`, `not_authorized`, `peer_identity_mismatch`, … |
| 7 | Incompatible protocol version |
| 8 | Integrity failure, or completed with failures |
| 9 | Local storage or config, including a missing receive folder |
| 10 | The peer broke the protocol |
| 130 | Ctrl-C |

### Clean up

```sh
rm -rf -- "${DROPUMI_CONFIG_DIR:?}" "${UNPAIRED:?}" "${SPOOF:?}" "${RX:?}"
rm -rf -- "${FILES:?}"
adb shell rm -rf /sdcard/Download/dropumi-test-files
unset DROPUMI_CONFIG_DIR UNPAIRED SPOOF RX FILES
```

- Remove `FILES` and the phone's `dropumi-test-files` copy only at the end of the session. The `adb shell rm` line is not in the repo.
- Also delete the received test files: the Mac app's folder, the iPhone's Received folder, Android Transfers › "Delete" on each entry, and any `"$D"` folder pulled from a phone.
- The CLI keys are now gone. In each app, open Settings › Paired devices and remove **every** CLI entry: tap "…", choose "Remove…", then "Remove". Only the main CLI should be listed. If the `UNPAIRED` or `SPOOF` identity appears there, remove it too, and record it as a finding.

## Session plan

### The pairs

M = Mac app, I = iPhone app, A = Android app, C = CLI on the Mac.

| Pair | Devices | How they find each other |
|---|---|---|
| P1 | I ↔ M | Nearby, both ways |
| P2 | I ↔ C | The iPhone lists the CLI. `"$DROP" discover` lists the iPhone. The CLI sends by name or address |
| P3 | M ↔ C | Same Mac, not a LAN pair. The Mac app lists the CLI, and `discover` lists the Mac app. Whichever starts first gets port 47321 |
| P4 | A ↔ M | Android lists the Mac. The Mac does not list Android: use Connect by address with Android's address |
| P5 | A ↔ I | As P4 |
| P6 | A ↔ C | Android lists the CLI. The CLI uses `--to <Android address>:<port>` |

### The core set on every pair, both directions

The core set is DISC-1, PAIR-1, TX-1, TX-3 and CAN-1. Run it on every pair, in the order below. TX-3 comes before PAIR-1, so no unpairing is needed. The two devices of a pair are not paired with each other when the pair starts, although each may still list peers from earlier pairs.

1. **DISC-1.** Record each direction in Notes.
   - On P4, P5 and P6, Android should list the other device. The other device is expected **not** to list Android, because Android is browse-only (ADR-0013 D5).
   - The template does not say how to label this case. This guide's rule:
     - label DISC-1 `Manual device test passed` when Android lists the other device and the other device does not list Android;
     - label it `Failed` otherwise;
     - note the rule in Findings, so the maintainer can change it.
   - This rule applies to every section's DISC-1 row on an Android pair: the Mac section's Mac ↔ Android, the iPhone section's I↔A and the Android section's DISC-1. It applies even where a section words it differently.
   - The app takes no MulticastLock and does not request `CHANGE_WIFI_MULTICAST_STATE`. Note whether Android lists the other device anyway. This answers an open point of ADR-0013 S-AN2.
   - Before you label DISC-1 `Failed` for a device that is missing, work through "If nothing is listed".
2. **TX-3.** Do this before pairing, both ways.
   - **Put the receiver in Everyone mode:**
     - On an app: Settings › "Who can send to this device" › "Everyone · 10 min".
     - On Android, a panel then asks "Let everyone send for 10 minutes?". Tap "Let everyone send".
     - The status pill then shows "Everyone · " and a countdown. Record the countdown under EVERY-1, not under TX-3.
     - On the CLI, use `--policy everyone`.
   - **The unpaired device sends.** The CLI uses `--allow-unverified`.
   - **Check that no pairing was created:**
     - The TX-3 peer is not in `"$DROP" trust list` (compare the display fingerprint).
     - It is not in the app's Settings › Paired devices.
     - `No trusted devices.` and "No paired devices" appear only while that device has no pairings at all, which is during the first pair it takes part in.
   - Label TX-3 by the codes and the missing pairing only.
   - TX-4 and PAIR-2 also need unpaired devices, so do them now if this pair runs them.
3. **PAIR-1.** One side starts. Then remove the pairing on both sides, and pair again with the other side starting.
4. **TX-1.** Both directions, with a prompt. Auto-accept is off (PRE-6). Check SHA-256 as described in "Test files".
5. **CAN-1.** Both directions. The sender cancels during `large-2048MiB.bin`. Then check the receiver for `.dropumi-*.part` files.

### Between pairs

Do this before each pair, and after any row that changed a setting (TX-2, TX-3, SPOOF-1, DISC-3 and the NET rows).

1. On every app:
   - "Who can send to this device" is "Paired only" (the pill reads "Paired devices only");
   - "Auto-accept paired devices" is off;
   - no Everyone window is running;
   - the app is not on "Off".
2. Stop every CLI `receive` and `pair` with Ctrl-C, and wait for the shell prompt to come back.
3. Replace the CLI's receive folder and empty the Mac app's folder. Use the folder chosen in PRE-5 if you changed it, and set the new `RX` in any other terminal:
   ```sh
   rm -rf -- "${RX:?}" && RX="$(mktemp -d)"
   find ~/Downloads/Dropumi -mindepth 1 -delete
   ```
4. On the iPhone, note the names already in Files › On My iPhone › Dropumi › Received, so that new files are easy to tell apart.
5. On Android, open Dropumi on Nearby and wait until the pill no longer reads "Starting…". Do not use Transfers › "Clear" (see "Android as receiver").
6. Read each phone's Settings › This device › Address again. It can change, especially after the NET rows.
7. After NET-4 and NET-5, put every device back on the shared Wi-Fi.

### If nothing is listed

Work through this before you label DISC-1 `Failed`:

1. The other device is receiving and not on "Off". The iPhone or Android app is open in front. The Mac app was not quit ("Quit Dropumi completely" stops it).
2. The observing app is on Nearby, Send or Pairing. The apps browse only while one of these screens is open.
3. Local Network is allowed for the Mac app and the iPhone app (PRE-2), and for Terminal if macOS lists it.
4. On the Mac, check who advertises. Run this for about 10 s, then press Ctrl-C:
   ```sh
   dns-sd -B _dropumi._tcp
   ```
   Then run:
   ```sh
   "$DROP" discover --timeout-ms 5000
   ```
   Redact both outputs before pasting them.
5. Check the network: every device on the same Wi-Fi, VPN off, no guest or isolated network, and the Mac firewall not set to block all incoming connections.
6. Leave Nearby and open it again. The Apple apps do not restart a failed browse while Nearby stays open.
7. Remember that no device ever lists Android. This is by design.
8. If the device is still missing, use Connect by address, and record in DISC-1 what you tried.

### Other rows: where they run

| Row | Run on |
|---|---|
| DISC-2 | Each app with another device listed: M and I in P1, A in P4 |
| DISC-3 | Each app as the Off device (M, I, A), with the other apps and the CLI observing. Before choosing "Off", note the app's address:port from Settings › This device › Address. After Off, that screen shows "Not receiving right now.". Then `"$DROP" send "$FILES/small.txt" --to <that address>:<port>` should fail with `connection_lost` (exit 5), and `discover` should no longer list M or I |
| DISC-4 | Mac Terminal against the Mac app and the foreground iPhone app (during P1). CLI optional |
| NSD-1 | Android with the Mac Terminal (during P4) |
| PAIR-2 | One app↔app pair (P1) and one app↔CLI pair (P2) |
| TX-2, TX-4, TX-5 | Each receiver: M and I in P1, C in P2, A in P6 |
| TX-6, TX-7, TX-8, CAN-2 | Each receiver at least once. M: TX-7, TX-8 and the CLI part of TX-6 in P3 (the Mac section uses the CLI as sender there; its TX-8 says the iPhone cannot be the sender), and CAN-2 plus the rest of TX-6 in P1. I in P1, C in P2, A in P6 |
| KILL-1 | Each app as receiver; the CLI is optional. After the force-quit, check that the `.part` file exists. Relaunch the receiver at once: the file is expected to remain, because it is younger than 15 minutes. Then relaunch it again after the file is more than 15 minutes old: expect it to be gone. The apps clean up with the CLI's 15-minute rule (`STALE_AFTER` in `crates/infra/src/fs/cleanup.rs`) each time receiving starts. For the iPhone, how to see the `.part` file is not confirmed in code (check and note what you see) |
| NET-1, NET-2 | App↔app pairs P1, P4 and P5, plus P6 for the Android section's rows (C ↔ A) |
| NET-3 | Any pair (the access point reboots) |
| NET-4 | Any pair, on a network with client isolation on. Record its band and isolation setting. If you have no such network, record `Not tested` |
| NET-5 | P6 only. The Android phone and the Mac both join a Wi-Fi network without internet, with mobile data on for the phone. Record separately: the address This device shows (Wi-Fi or not), the app sending to the CLI, and the CLI sending to the app |
| BG-1 | I sends to M (P1) |
| BG-2 | I idle in the background, observed from M (Nearby), A (Nearby) and C (`discover`) |
| BG-3 | M receives from I. After Quit, check with `dns-sd -B _dropumi._tcp` or `discover` |
| BG-4 | A sends to C (P6), as in the Android section (`receive --accept trusted`) |
| BG-5 | A receives. Also run the Android section's LOCK-1 and LOCK-2 (extra rows) and BG-5 step 2 |
| SHARE-1, SHARE-2 | I sends to M (SHARE-1 uses the throwaway photo and video) |
| SHARE-3 | A sends to C (P6), as in the Android section's step 5 |
| SHARE-4 | M sends to I (P1). The Mac section's partner is the paired iPhone |
| PERM-1 | I and M |
| PERM-2 | A |
| SPOOF-1 | Each app as observer. The CLI is the spoofer, with the `SPOOF` identity |
| UI-1, UI-4, EXP-1, INST-1 | Single device |
| UI-3 | Rotation on the A54. There is no Android tablet, so write "Not tested: tablet layout (no Android tablet)" in Notes, and label the row by the rotation result |
| OVL-1 | A, through `scripts/android/ui-tests.sh --class io.github.dropumi.prompt.OverlayWindowTest` (see Privacy) |
| UI-2, BAK-1 | Not tested: no iPad, and BAK-1 needs two throwaway Android phones |
| MAC-UI, MAC-BONJOUR | Automated, re-run on the commit you test (step 10) |

### Extra checks

- [ ] **QUAR-1 (extra; ADR-0012, threat-model R9)** Quarantine flag on received files. Steps: 1) After any TX row with the Mac app as receiver, `cd` into the Mac app's receive folder and run `xattr -l small.txt` (or the name of a file it received). 2) In `"$RX"`, run the same command on a file the CLI received. **Expect:** ADR-0012 says the Mac app receives with `LSFileQuarantineEnabled`, so its file should carry `com.apple.quarantine`. R9 says the CLI does not quarantine, so its file should not carry it (not confirmed on a device: check and note what you see). **Record:** present or absent for each, with no path.
- [ ] **EVERY-1 (extra; `Common.swift`, `ScreenText.kt`, `crates/ffi/src/types.rs`)** Everyone countdown, one result per app (M, I, A). Steps: 1) On the app, open Settings › "Who can send to this device" › "Everyone · 10 min". On Android, tap "Let everyone send". 2) Watch the status pill for a minute. **Expect:** the pill reads "Everyone · m:ss" and counts down from about 10:00. From code reading, both the Apple apps and Android read the core's end time as milliseconds, although the core gives it in Unix seconds. So the pill is predicted to read "Everyone · 0:00" for the whole window. This has not been seen on a device: check and note what you see. **Record:** what the pill showed on each app. Label it `Failed` if it did not count down. Record it here even where an app section puts the countdown under TX-3.
- **Lock-screen checks** are in the Android section: LOCK-1 (no prompt over the lock screen), LOCK-2 and BG-5 step 2. This part has no lock-screen row of its own.
- **EXPIRE-1** is the offer prompt that nobody answers. The Mac section calls it EXPIRE-1. The iPhone section's PROMPT-2 is the same check, so record it as EXPIRE-1. The Android section has no EXPIRE-1 row, and only its LOCK-1 step "Unlocked after 120 s" touches this. Record Android's EXPIRE-1 from that step, or as `Not tested`.

### Order for the session

1. **Setup.**
   - Build the CLI. Run PRE-4, then build and install the apps.
   - Record INST-1 during the `adb install`.
   - Run PRE-7. Then make the test files, push them to the Android phone, and run PRE-1, PRE-2, PRE-6, PRE-3 and PRE-5.
   - Set up the CLI with `DROPUMI_CONFIG_DIR`, `UNPAIRED`, `SPOOF` and `RX`, and keep them for the whole session. Pairing is per pair of devices, so P2, P3 and P6 each start with that pair unpaired.
   - Before each of the following pairs, run "Between pairs".
2. **P3 (M ↔ C).** Core set and QUAR-1. With the CLI sending to the Mac app, also run TX-7, TX-8 and the CLI part of TX-6. Only the Mac is involved, so the CLI commands and any firewall prompt are settled before the phones join.
3. **P1 (I ↔ M).** Core set, plus:
   - DISC-2 (M, I), DISC-4 and PAIR-2;
   - TX-2, TX-4, TX-5 and CAN-2 with M and with I as receivers, the rest of TX-6 for M, and TX-6, TX-7 and TX-8 with I as receiver;
   - SHARE-1, SHARE-2, SHARE-4 (M sends to I) and BG-1;
   - EVERY-1 and EXPIRE-1 for M and I.
4. **P2 (I ↔ C).** Core set, plus PAIR-2 (app↔CLI), and TX-2, TX-4, TX-5, TX-6, TX-7, TX-8 and CAN-2 with the CLI as receiver.
5. **P4 (A ↔ M).** Core set, plus DISC-2 (A), NSD-1 and EXP-1.
6. **P6 (A ↔ C).** Core set, plus:
   - TX-2, TX-4, TX-5, TX-6, TX-7, TX-8 and CAN-2 with Android as receiver;
   - SHARE-3 and BG-4 (A sends to C);
   - EVERY-1 for A.
7. **P5 (A ↔ I).** Core set and BG-2.
8. **Rows that need every device.** DISC-3 and SPOOF-1.
9. **Disruptive rows.** These change state that later rows depend on. Run "Between pairs" after them.
   - **Network rows:**
     - NET-1 and NET-2 (P1, P4 and P5, plus the Android section's rows on P6), and NET-3;
     - NET-4, on the client-isolated network;
     - NET-5, with the phone and the Mac on the Wi-Fi network without internet. Afterwards, reconnect both to the shared Wi-Fi, and check that Android's This device shows a Wi-Fi address again.
   - **Other rows:**
     - KILL-1 (allow for the 15-minute wait);
     - PERM-1 and PERM-2 (deny, then restore);
     - BG-3 (Quit the Mac app);
     - BG-5, with the Android section's LOCK-1 and LOCK-2;
     - UI-1, UI-3 and UI-4.
10. **MAC-UI and MAC-BONJOUR.**
    - These runs launch Dropumi under XCTest and build into `apps/apple/build`, the same derived-data folder as the Release app. Quit the Mac app first. Launching it under test is expected to end a running instance (not confirmed in code: check and note what you see).
    - Approve UI automation and Local Network if asked. Run from `<APPLE worktree>`:
      ```sh
      cd "<APPLE worktree>"
      xcodebuild test -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-macOS -destination 'platform=macOS,arch=arm64' -derivedDataPath apps/apple/build
      DROPUMI_TEST_MDNS=1 bash scripts/apple/interop.sh apple_macos_bonjour
      ```
    - Then delete the result bundles: they hold a recording of the main display.
      ```sh
      cd "<APPLE worktree>"
      find apps/apple/build/Logs/Test -maxdepth 1 -name '*.xcresult' -exec rm -rf {} +
      ```
    - If you need the Mac app again afterwards, relaunch the Release app with the `open` command from "Build and install the apps".
11. **OVL-1, then the optional Android harnesses.** Run them last.
    - Pass 1 runs on a fake-core test Application. Pass 2 runs the app's live graph on temporary folders (`ui-tests.sh` header).
    - The script installs with `install -r -t`, so the installed debug app keeps its data. But unless you pass `--no-build`, it rebuilds the APK from the current tree first.
    - Afterwards, check Settings › Paired devices. If the commit in Settings changed, record it.
    ```sh
    cd "<ANDROID worktree>"
    export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
    bash scripts/android/ui-tests.sh --class io.github.dropumi.prompt.OverlayWindowTest
    ```
    Record the `overlay:` lines it prints, with the serial redacted.
12. **Clean up** the CLI and the test files as described above. Remove any test APKs a script left behind (see PRE-4).

### What run 2026-10-18-01 already covered

That run was on `feat/native-apps-apple`. It recorded automated tests only. **Every manual row stayed `Not tested`**, except MAC-UI and MAC-BONJOUR.

| Covered automatically | Commit | Result |
|---|---|---|
| iPhone 15 hosted tests | `0a2b82d` | 6 tests passed |
| iPhone 15 UI state tests (fake core) with the accessibility audit | `0a2b82d` | 15 tests, 0 failures |
| iPhone 15 core against the CLI over Wi-Fi: code-checked transfers both ways, pairing, Bonjour both ways | `1ec2655` | 5 passed |
| Mac hosted tests | `0a2b82d` | 7 tests passed |
| MAC-UI (fake core) | `0a2b82d` | 15 tests, 0 failures |
| Mac core against the CLI on loopback | `94a293f` | 3 passed |
| MAC-BONJOUR | `94a293f` | 2 passed |
| Package tests | `0a2b82d` | passed |

**Partial evidence that does not change any label:**
- DISC-1 was covered only between the iPhone's core and the CLI, not on the Nearby screen.
- DISC-4: the interop test checked the host name, but `dns-sd -L` was not run.
- UI-1: only the audits ran. VoiceOver itself was not used.

**Android.** No device run is recorded.
- The evidence covers emulators only: API 26, 28 and 36.1, on 2026-10-16.
- The only claims for this phone are in commit messages, not in a run file:
  - `785f811` names the Galaxy A54 (One UI, Android 16): OverlayProtectionTest, "all seven tests pass on that device".
  - `06b5348` names no device. It changes how `ui-tests.sh` reports `LockScreenTest#promptNeverShowsOverTheLockScreen` on any device with a PIN, pattern or password: "not runnable on this device (secure lock screen)". It is not a result from the A54.
- So the lock-screen behaviour still needs the Android section's LOCK-1 and LOCK-2 hand checks on the PIN-locked phone. Threat-model G39 lists real devices as Not tested.

**Still needs hands:**
- every manual row, on all six pairs;
- the full Android harnesses on the real phone, with a person present, and only if the phone holds no account data you care about. Both print the serial, so redact it.
  ```sh
  cd "<ANDROID worktree>"
  export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
  bash scripts/android/ui-tests.sh
  ANDROID_SERIAL=<serial> bash scripts/android/interop.sh
  ```
  - `ui-tests.sh` runs the whole suite, not just OVL-1.
  - Run `interop.sh` with the phone on the Mac's Wi-Fi. Its transfer tests go through `adb reverse` and `adb forward` on `127.0.0.1`. Only the `android_nsd_*` tests use Wi-Fi.
- re-running MAC-UI and MAC-BONJOUR on the commit you test (step 10).
