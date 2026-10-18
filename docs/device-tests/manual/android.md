# Manual device testing: Android app

Read the [common page](README.md) first: privacy, test files, the CLI as a test peer, and the session plan.

The Android app is in `apps/android` on the `feat/native-apps-android` checkout (the Android worktree). It is pre-alpha and unaudited.

No Android device run has been recorded yet (`docs/device-tests/runs/README.md`: "No runs are recorded yet"). The only evidence from this phone is in two commit messages, `785f811` (overlay test) and `06b5348` (lock-screen test). A result from an emulator never counts for this phone.

Keep these four things in mind for the whole session:

- **The phone only browses.** It lists the Mac app, the iPhone app and `dropumi receive`. None of them lists the phone, on any Android version, because the R21 gate's allowlist is empty (ADR-0013 D5).
  - Every send or pairing request to the phone needs its address as "ip:port". This applies to C→A, M→A and I→A, paired or not. Read the address under Settings › This device › Address, or on the pairing window screen.
  - Pairing only removes the code check. It does not make the phone listed or reachable, because a trust entry stores no address.
  - On the CLI, use `--to <phone address>:<port>`. In the Apple apps, use Connect by address.
- **It receives only while Dropumi is open and on screen.** About 700 ms after the app leaves the screen (Home, lock, screen timeout), it stops accepting new connections. Transfers already running still finish.
- **No prompt over the lock screen.** A prompt never shows over the lock screen, and an incoming offer or code never gets a notification.
- **Arming delay.** "Accept" and "Codes match" become enabled 600 ms after the prompt appears. The 600 ms starts again every time the app resumes or its window gets focus back.

## Run it

Run everything in this section from the Android worktree. The Apple worktree has no `scripts/android/`, no `:app` module and no `crates/cli/tests/e2e/interop/android.rs`.

**Paste rule for every block below.** The blocks have no trailing `#` comments, because zsh does not treat `#` as a comment when you paste at the prompt (`/etc/zshrc` does not set `interactivecomments`). The explanations are in the bullets under each block. Paste the lines as they are.

### What you need

On the Mac:
- **JDK 17.**
  - Gradle runs on JDK 17 or later. The build itself needs an installed JDK 17, because every module declares `jvmToolchain(17)` and `settings.gradle.kts` has no toolchain-download plugin.
  - `docs/development.md:155` says `-v 21`, but every other doc and script uses 17. On this Mac the default `java_home` is 21, so export 17 yourself.
  - The recorded runs used Homebrew OpenJDK 17.0.20.1 (`docs/development.md`). `java_home -v 17` finds only JDKs registered under `/Library/Java/JavaVirtualMachines`. If it finds none, follow the symlink step that brew prints for `openjdk@17` (not in the repo docs).
- **Android SDK.** You need platform 36, build-tools 36.0.0, NDK 27.1.12297006 and platform-tools. You do not need emulator images.
  - `sdkmanager` comes from the "Android SDK Command-line Tools" package. If `~/Library/Android/sdk/cmdline-tools/latest` is missing, install that package from Android Studio's SDK Manager (not in the repo docs).
- **Rust.** You need the Android targets and Rust 1.89 or later. You do not need cargo-ndk, because `build-android.sh` sets the compiler, linker and 16 KB page flags itself.
- **No keystore.** The debug build needs none.

On the phone:
- **USB debugging.** Turn it on (standard Android developer setting) and connect the phone by USB.
- **Same network.** Put the phone on the same Wi-Fi as the Mac.
- **Screen timeout.** A screen timeout locks the phone, and locking stops receiving. Raise the timeout for the session.
- **Auto Blocker (not in the repo docs).** One UI's Auto Blocker may block USB installs. Check it if `adb install` is refused.

```sh
cd "<Android worktree>"
export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
"$JAVA_HOME/bin/java" -version
export ANDROID_HOME=~/Library/Android/sdk
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"
rustup target add aarch64-linux-android x86_64-linux-android
sdkmanager --licenses
sdkmanager --install "platforms;android-36" "build-tools;36.0.0" "ndk;27.1.12297006" "platform-tools"
```

- `java -version` must print version 17.
- The `PATH` line is not in the repo docs.

These exports apply only to the tab you run them in. Run the `JAVA_HOME`, `ANDROID_HOME` and `PATH` lines again in every tab that runs Gradle, `adb` or a script under `scripts/android/` or `scripts/ffi/`.

**How each script finds the SDK, NDK and adb.** Exporting `ANDROID_HOME` as above satisfies all of them.
- **`build-android.sh`.** SDK: `ANDROID_HOME`, then `sdk.dir` in `apps/android/local.properties` (gitignored), then `~/Library/Android/sdk`. NDK: `ANDROID_NDK_HOME`, or else `<sdk>/ndk/27.1.12297006`.
- **`ui-tests.sh` and `interop.sh`.** `adb` on PATH. Otherwise they look in `ANDROID_HOME`, then `ANDROID_SDK_ROOT`, then `~/Library/Android/sdk`. They never read `local.properties`.
- **`check-16k.sh`.** Tools on PATH. Otherwise they look in `ANDROID_HOME` (or `ANDROID_NDK_HOME`), else `~/Library/Android/sdk`. It never reads `ANDROID_SDK_ROOT` or `local.properties`.

### Build the Rust core

```sh
bash scripts/ffi/build-android.sh --abis arm64-v8a,x86_64 --profile dev
```

- **Always pass `--abis arm64-v8a,x86_64`, even though the phone is arm64-v8a.** Every APK variant checks for both ABIs. Without x86_64, every variant fails with "the Rust core has no x86_64 library". The script's own default is `--abis arm64-v8a`.
- **Use `--profile dev` by default.** The script's own default is `--profile release`. A debug build accepts a release-profile core without a warning. Only a release build type with a non-release core prints a warning.
- **Never use a plain `cargo build`.** It produces misaligned `.so` files.
- **Gradle never runs cargo.** After any Rust change, the build stops with: "the Rust core is older than the Rust sources; run: bash scripts/ffi/build-android.sh --abis arm64-v8a,x86_64 --profile dev".
- **TX-6 timing uses a release core.** The full commands are in TX-6.
- **Optional 16 KB check.** Run `bash scripts/ffi/check-16k.sh apps/android/core-bindings/build/rust/jniLibs`. The 16 KB-page runtime itself is Not tested (S-AN7).

### Build the APK (use `debug`)

```sh
./apps/android/gradlew -p apps/android :app:assembleDebug
```

- The APK is `apps/android/app/build/outputs/apk/debug/app-debug.apk` (package `io.github.dropumi.debug`).

| Build type | Package | Use it for manual tests? |
|---|---|---|
| `debug` | `io.github.dropumi.debug` | **Yes.** Signed with the debug key and contains no fake core. It is debuggable, so `run-as` can read received files. |
| `qa` | `io.github.dropumi.qa` | No. Only `interop.sh` uses it, and it is not debuggable. |
| `release` | `io.github.dropumi` | Only if you create `apps/android/keystore.properties`. Without it the APK is unsigned and cannot be installed. |

- **Same label everywhere.** Every variant is labelled "Dropumi". With two installed, the launcher and the share sheet show two "Dropumi" entries. Keep only `debug` installed.
- **First build.** It downloads Gradle 9.8.1 and the dependencies from google() and mavenCentral(), so the Mac needs internet the first time.

### Install (INST-1)

First check the connection, then remove any old debug app and anything the scripts or other builds left behind. An error for a package that is not installed is harmless.

```sh
adb devices -l
export ANDROID_SERIAL=<serial>
adb -s <serial> uninstall io.github.dropumi.debug
adb -s <serial> uninstall io.github.dropumi.debug.test
adb -s <serial> uninstall io.github.dropumi.overlayhelper
adb -s <serial> uninstall io.github.dropumi.qa.test
adb -s <serial> uninstall io.github.dropumi.qa
adb -s <serial> uninstall io.github.dropumi
adb -s <serial> install apps/android/app/build/outputs/apk/debug/app-debug.apk
adb -s <serial> shell pm list packages | grep dropumi
```

- **`adb devices -l`.** The phone must show as "device". If it shows "unauthorized", accept the USB-debugging prompt on the phone.
- **`ANDROID_SERIAL`.** `ui-tests.sh` and `interop.sh` read it.
- **Uninstalling `io.github.dropumi.debug` first** removes an old debug app with its identity, pairings and history (the same reason as PRE-4). That makes INST-1 a first install.
- **No `-r` here.** The uninstall line is what makes this a first install. TX-6 later uses `install -r` on purpose, to keep the app's data.

If the install fails (neither fix is in the repo docs):
- **`INSTALL_FAILED_TEST_ONLY`:** add `-t`. The scripts use `adb install -r -t`.
- **`INSTALL_FAILED_UPDATE_INCOMPATIBLE`:** an old `io.github.dropumi.debug` is still installed, for example one that another machine's debug key signed. Run its uninstall line again; this deletes its data.

**Never install these by hand:**
- the overlay helper (`io.github.dropumi.overlayhelper`, label "Dropumi test overlay");
- the `*.test` APKs;
- the fake core (`:testsupport`), which is never in an app APK anyway.

### Record the setup

```sh
git rev-parse --short=12 HEAD
cat apps/android/core-bindings/build/rust/core-stamp.txt
adb -s <serial> shell getprop ro.build.version.sdk
adb -s <serial> shell getprop ro.product.cpu.abi
adb -s <serial> shell getprop ro.build.version.security_patch
adb -s <serial> shell pm list packages --apex-only --show-versioncode | grep -i tethering
```

- **Commit.** `git rev-parse --short=12 HEAD` is the same command the build uses for the app's commit (`apps/android/app/build.gradle.kts`). The Settings footer shows the same 12-character hash: "Free · No account · No internet needed · Version 0.1.0 (‹commit›)".
- **Core profile.** From `core-stamp.txt`, copy the "profile …" line.
- **Security patch.** The `security_patch` line is not in the repo docs.
- **What to write.** Model, Android version, API level, ABI, security patch, Connectivity module version, build type and core profile.
- **Never write** the serial, Wi-Fi name, addresses or device names.

### Launch and first run

- **Launch.** Tap "Dropumi" in the launcher, or run `adb -s <serial> shell am start -n io.github.dropumi.debug/io.github.dropumi.MainActivity`. The component name keeps the `io.github.dropumi` namespace. This command is not in the repo docs.
- **Loading.** The brand symbol shows on navy. If startup fails, you see "Dropumi could not start", a message and "Try again".
- **Onboarding has 4 pages.** The button reads "Continue", or "Start" on the last page. Receiving starts only when you tap "Start".
  1. "Just drop it nearby."
  2. "Name this device". Keep the suggested `Dropumi-XXXX` name. It is never the phone's own name.
  3. "Find devices on your network".
  4. "Who can send to this device?". The default is "Paired devices only". The other choices are "Everyone, for 10 minutes" and "No one for now".
- **No permission dialog appears at launch or during onboarding.**

### Permissions, and when they appear

- **The only runtime dialog is notifications** (Android 13 and later).
  - It is asked from a card on Transfers after a transfer starts, never over a prompt. The card reads "Allow notifications to see progress when you leave Dropumi", with "Allow" and "Not now".
  - Once you refuse, the next "Allow" opens the app's notification settings instead of the dialog.
  - "Not now" hides the card until the app process restarts. The flag is kept in memory only.
- **Never requested:**
  - Nearby Wi-Fi devices (`NEARBY_WIFI_DEVICES`);
  - Bluetooth;
  - local network (`ACCESS_LOCAL_NETWORK`, deferred to targetSdk 37);
  - multicast;
  - storage;
  - battery-optimisation exemption.
- **The manifest's permission list is an exact allowlist of 8 entries:**
  - `INTERNET`;
  - `ACCESS_NETWORK_STATE`;
  - `HIDE_OVERLAY_WINDOWS`;
  - `FOREGROUND_SERVICE`;
  - `FOREGROUND_SERVICE_DATA_SYNC`;
  - `WAKE_LOCK`;
  - `POST_NOTIFICATIONS`;
  - androidx.core's `io.github.dropumi.debug.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION`. This one is signature level and never shows a dialog (`scripts/android/check-manifest.sh`).
- **Save to Downloads asks for nothing** on API 29 and later.

### The Mac as the test peer (CLI)

Tab 1 (the main CLI identity, which you pair with the phone):

```sh
cd "<Android worktree>"
cargo build --release --locked -p dropumi-cli
DROP="$PWD/target/release/dropumi"
export DROPUMI_CONFIG_DIR="$(mktemp -d)"
FILES="$(mktemp -d)"; bash scripts/device-tests/make-test-files.sh "$FILES" 2048
echo "$FILES"
RX="$(mktemp -d)"
echo "$DROPUMI_CONFIG_DIR" "$RX"
```

- **Worktree.** The CLI sources are the same in both worktrees. Use the Android one.
- **`DROPUMI_CONFIG_DIR`.** A throwaway identity, never the real configuration.
- **`echo "$FILES"`.** Copy this path for the other tabs. Do not record it.
- **`RX`.** Make a new one for each receive test.
- **Check before any `receive`.** The last line must print two temp paths. If one is empty, run its line again. Without `DROPUMI_CONFIG_DIR` the CLI uses the real `~/Library/Application Support/dropumi` (`docs/cli.md`, "Configuration directory").

Then, in the same tab:

```sh
"$DROP" identity show
ipconfig getifaddr en0
"$DROP" receive --dir "$RX"
```

- **`ipconfig getifaddr en0`** prints the Mac's Wi-Fi address. This guide writes it as `<Mac's Wi-Fi address>`. Do not record it.
- **`receive` defaults.** It listens on all interfaces on port 47321 (a free port if that one is busy), advertises, accepts paired devices only, and asks before it accepts.

Every new Terminal tab starts with no `DROP`, `FILES` or `RX`. Run these lines in each new tab, for example for an unpaired identity (TX-3, TX-4, SPOOF-1) or for `discover` (DISC-1):

```sh
cd "<Android worktree>"
DROP="$PWD/target/release/dropumi"
FILES="<the path tab 1 printed>"
export DROPUMI_CONFIG_DIR="$(mktemp -d)"
RX="$(mktemp -d)"
echo "$DROPUMI_CONFIG_DIR" "$RX"
```

- **`DROPUMI_CONFIG_DIR`** here is a new, unpaired identity.
- **Check.** The last line must print two temp paths before any `receive`, `send` or `pair`.

- **Never skip the export.** Without `DROPUMI_CONFIG_DIR` (or `--config-dir`), the CLI uses the real configuration directory (`docs/cli.md`).
- **One listening process per config.** Before `pair --listen`, `pair --to` or a new `receive` on the same config, stop the previous `receive` or `pair` with Ctrl-C and wait for the shell prompt.
  - A second listener on the same identity falls back to a free port and also advertises, so the phone can list two rows with the same name.
  - `receive` refuses pairing sessions and `pair --listen` refuses transfers (`not_authorized`).
- **Always pass `--dir "$RX"` to `receive`.** Without it, files go to the Mac's real Downloads folder.
- **Mac firewall.** The macOS firewall may ask about incoming connections. Note what it asks.
- **Local Network privacy.** If LAN connections from the CLI fail while the phone and Mac are on the same Wi-Fi, check System Settings › Privacy & Security › Local Network. See whether your terminal app is listed and allowed, and note what you see (`docs/development.md`, troubleshooting).
- **The phone's address.** Read it on the phone under Settings › This device › Address as "ip:port". It appears only while receiving. The port defaults to 47321, with a fallback to a free port. This guide writes it as `<phone address>:<port>`.

### Test files for sending from the phone

The repo does not document how to copy files to the phone. One way is standard `adb push` (not in the repo docs):

```sh
adb -s <serial> push "$FILES" /sdcard/Download/dropumi-test-files
```

- **Pick files** with "Add files" or "Choose files to send" (the system picker).
- **Keep test files out of `Download/Dropumi/`.** Save writes there (EXP-1).

### Where received files are, and how to hash them

**Where files go:**
- **Received files** stay app-private in `files/received`: folder 0700, files 0600, never overwritten. They stay there until you Save, Share or Open them.
- **Name collisions.** A second file with the same name becomes `name (1).ext`.
- **Save to Downloads** (on a history row) writes to `Download/Dropumi/`.
  - On the phone this is probably My Files › Download › Dropumi (not confirmed in code: check and note what you see).
  - The copy is checked by size only; the core has already verified BLAKE3.

**Hash commands** (derived from the code, not in the repo docs; `run-as` works only on the debug build):

```sh
adb -s <serial> shell run-as io.github.dropumi.debug ls -la files/received
adb -s <serial> exec-out run-as io.github.dropumi.debug cat "'files/received/<name>'" | shasum -a 256
adb -s <serial> shell sha256sum "'/sdcard/Download/Dropumi/<name>'"
OUT="$(mktemp -d)"; adb -s <serial> pull /sdcard/Download/Dropumi "$OUT"; (cd "$OUT/Dropumi" && shasum -a 256 *)
grep <hash> "$FILES/SHA256SUMS"
```

The double quotes around single quotes keep names with spaces intact on the phone's shell.

**Compare by hash, not by `shasum -c`.** Received names do not match `SHA256SUMS` paths:
- the 300 `many/` files arrive flat, without the `many/` prefix;
- the right-to-left override character is removed from the name.

### What a finished transfer looks like in Transfers

- **Completed transfer.**
  - The title is the first file's name, for example "small.txt and 1 more".
  - The subtitle is "Received from ‹alias› · ‹local short time›" or "Sent to ‹alias› · ‹time›".
  - A check icon shows; TalkBack reads it as "Done".
  - There is no second line. "All files saved and checked" and "Sent and checked" never show on a completed row.
- **Not completed.** The subtitle gets a second line, for example:
  - "You declined" or "Declined";
  - "Cancelled by you" or "Cancelled by the other device";
  - "Request expired";
  - "Only paired devices";
  - "Connection lost" or "No answer".
- **Unpaired peer.** ‹alias› is "Unverified device".

### Reset

```sh
adb -s <serial> uninstall io.github.dropumi.debug
adb -s <serial> shell pm clear io.github.dropumi.debug
adb -s <serial> shell rm -r /sdcard/Download/Dropumi
adb -s <serial> shell pm revoke io.github.dropumi.debug android.permission.POST_NOTIFICATIONS
adb -s <serial> shell pm clear-permission-flags io.github.dropumi.debug android.permission.POST_NOTIFICATIONS user-set user-fixed
```

- **`uninstall`** removes everything.
- **`pm clear`** removes data only: identity, trust, history, received files and settings.
- **`rm -r /sdcard/Download/Dropumi`** removes saved files, which survive both of the lines above.
- **After `pm clear` or an uninstall,** the next launch shows onboarding and a new fingerprint. Nothing is backed up (`allowBackup="false"`).
- **The last two lines** make the notification dialog show again. They are not in the repo docs.
- **In-app reset.** Settings › This device › "Reset identity…" › "Reset identity". The banner reads "New identity" / "Devices you paired with must pair again."
- **On the Mac,** run `"$DROP" trust remove <FP52>`, or start a new `mktemp -d` config.

**Display, font and lock settings.** `ui-tests.sh` changes these and puts back the values it saved at its start. UI-1 also changes the font size on purpose. Before UI-1 and before any harness, record the current values:

```sh
adb -s <serial> shell wm size
adb -s <serial> shell wm density
adb -s <serial> shell settings get system font_scale
adb -s <serial> shell locksettings get-disabled
```

- **`wm size` and `wm density`.** Note any "Override size:" or "Override density:" line.
- **`locksettings get-disabled`.** Expect `false` on a phone with a PIN.

After UI-1, or if a `ui-tests.sh` run was killed before its exit trap ran, put back exactly those values:

```sh
adb -s <serial> shell wm size <old override>
adb -s <serial> shell wm density <old override>
adb -s <serial> shell settings put system font_scale <old value>
adb -s <serial> shell locksettings get-disabled
```

- **No override line before?** Use `adb -s <serial> shell wm size reset` or `adb -s <serial> shell wm density reset` instead.
- **Old font scale was "null"?** Use `adb -s <serial> shell settings delete system font_scale` instead.
- **The last line is a check only.** It must print `false`.

On a phone with a secure lock screen, LockScreenTest stops at its precondition before it runs any `locksettings` command.

### Logs

- **Dropumi writes nothing to logcat.** There is no logging helper, and `scripts/android/check-logging.sh` rejects `Log`, `println`, `printStackTrace` and similar calls.
- **The Rust core logs nowhere.** It emits `tracing` events, but `dropumi-ffi` has no subscriber, so they go nowhere (inference from `Cargo.toml`).
- **Logcat shows only system and framework lines.** For a crash, run `adb -s <serial> logcat -b crash`.
- **Before pasting** logcat or any output, remove the serial, device names and addresses. Write `~` for the home directory.

## Checklist

**Not in scope on Android** (iOS, Mac or iPad rows): BG-1, BG-2, BG-3, SHARE-1, SHARE-2, SHARE-4, PERM-1, UI-2, UI-4, MAC-UI and MAC-BONJOUR. The phone may still be a partner in BG-2, where it observes.

Record each row as **Manual device test passed**, **Failed** (say what happened) or **Not tested**.

**Order.**
- **First:** INST-1.
- **Core set:** DISC-1, PAIR-1, TX-1, TX-3 and CAN-1, on every pair that includes the phone: C↔A, M↔A and I↔A. For each, write which device sent.
- **On each pair, TX-3 before PAIR-1.** TX-3 needs the two devices unpaired.
- **Late:** HIST-1, because its "Clear" step deletes every received file in Dropumi.
- **Last:** KILL-1, OVL-1 and the automated harnesses.

**Two rules for every row.**
- For receive tests, keep the phone unlocked with Dropumi on screen.
- Never write addresses, fingerprints or verification codes in the run file.

**Paired-device menu.** Where the common PRE-6 and Clean up steps say "…", the Android app shows ⋮. TalkBack reads it as "Actions for ‹alias›" (`PairedDevicesScreen.kt:96`).

**Mac and iPhone apps as peers.** They never list the phone, paired or not. For M↔A and I↔A:
- **Pairing.** Either the phone asks from its own list (Pairing › "Pair with a nearby device"), or the phone opens "Wait for a device" and the Apple app uses Connect by address › "Pair with this address" with the address the window shows. PAIR-1 needs both.
- **Sending to the phone.** The Apple app always uses Connect by address with `<phone address>:<port>`, even after pairing.

### Install

- [ ] **INST-1** Install with adb. Steps: 1) Run the install block above. 2) Watch the terminal and the phone. **Expect:** the install completes and "Dropumi" appears in the launcher. Any Play Protect or developer-verification warning is quoted word for word. **Record:** the warning text, or "no warning"; that it was a first install; build type; core profile.

### Discovery

- [ ] **DISC-1** The phone lists the others; the others do not list the phone. Steps:
  1) Mac, tab 1: `"$DROP" receive --dir "$RX"`.
  2) Phone: Dropumi open on Nearby, receiving on "Paired only".
  3) Mac, in a new tab set up as above: `"$DROP" discover --timeout-ms 5000`.
  4) Repeat with the Mac app and then the iPhone app on screen.

  **Expect:**
  - The Nearby header reads "1 device found · works without internet" (the count follows the devices).
  - The CLI's row is titled with its `Dropumi-XXXX` name. Its subtitle is "Not paired" (or "Looks like ‹alias›" once paired), with a "Send" pill.
  - Nearby shows the card "Not listed nearby" / "Other devices can't see this phone on this Android version. They can still send to it after pairing, or by address." with "Show my address", which opens This device.
  - The Settings caption reads "Only paired devices can send. Only while Dropumi is open."
  - `discover` never lists the phone.
  - The Mac and iPhone apps never list the phone.

  **Record:**
  - each direction separately; "Not listed" is the expected result for the phone;
  - whether browsing worked: the app takes no `MulticastLock`, and whether it needs one is an open S-AN2 question.
- [ ] **DISC-2** The phone never lists itself. Steps: 1) With at least one other device listed, look at Nearby and at the "Send to" chips on Send. **Expect:** the phone's own `Dropumi-XXXX` never appears. **Record:** the result. Add the note "the phone does not advertise, so this proves little".
- [ ] **DISC-3** Off refuses connections. Steps:
  1) Note the phone's address.
  2) Settings › "Who can send to this device" › "Off".
  3) Mac: `"$DROP" send "$FILES/small.txt" --to <phone address>:<port> --allow-unverified`.
  4) Tap "Turn on for paired devices" on Nearby.

  **Expect, while Off:**
  - The Nearby pill reads "Not receiving".
  - The problem card reads "Not receiving" / "Other devices can't find or send to this one. You can still send."
  - This device shows "Not receiving right now."
  - The Settings caption reads "Not receiving. Other devices can't find or send to this one."
  - The CLI fails to connect. The expected network failure is exit 5. The message is not confirmed on a device, so note it.
  - Nothing appears on the phone.

  **After step 4:**
  - The pill reads "Paired devices only".
  - The "Not receiving" card is replaced by "Not listed nearby" / "Other devices can't see this phone on this Android version. They can still send to it after pairing, or by address."

  **Record:** the CLI's message and exit code (`echo $?`), without the address. Only the refused connection counts, because the phone is never listed anyway.
- [ ] **DISC-4** Advertised host name. **Not applicable on Android today:** the app does not advertise. Use NSD-1. **Record:** `Not tested`, with the note "Not applicable: the app does not advertise (see NSD-1)".
- [ ] **NSD-1** The phone is not published over NSD. Steps:
  1) Stop every other `_dropumi._tcp` advertiser:
     - press Ctrl-C on every CLI `receive` and `pair --listen`;
     - Mac app: menu bar › "Quit Dropumi completely". Closing the window is not enough, because the menu bar keeps receiving (Apple `README.md`, "Where things are");
     - iPhone app: Settings › "Who can send to this device" › "Off", or send Dropumi to the background.
  2) Phone: Dropumi on screen, "Paired only".
  3) Mac: run `dns-sd -B _dropumi._tcp` for about 10 s, then Ctrl-C.
  4) If any instance appears, run `dns-sd -L <instance> _dropumi._tcp local.`
  5) Phone: Settings › "Pair a device" › "Wait for a device". While "Waiting for a device · m:ss" shows, run `dns-sd -B _dropumi._tcp` again for about 10 s, then Ctrl-C. Then tap "Cancel".
  6) Afterwards, open the Mac app again and set the iPhone app back to "Paired only".

  **Expect:**
  - Steps 2 to 4: no `_dropumi._tcp` instance appears. Nearby shows the "Not listed nearby" card.
  - Step 5: no instance appears either. In the pairing window the core asks the adapter to advertise, and the adapter must refuse while the gate is closed. The window still shows the "ip:port" address and the caption "Other devices can't see this one nearby. On the other device, choose Connect by address and type:".
  - If an instance appears in either step, first check that it is not a leftover from the Mac app, the iPhone app or a CLI: its TXT `n=` is that device's name. If it is the phone's, the R21 gate is not closing. Record Failed, with the step and whether the SRV host is `dropumi-…` or the phone's own name.

  **Record:** the API level, the Connectivity module version and the result of each step. Do not paste the `dns-sd` output, because it contains names and addresses.

### Pairing

- [ ] **PAIR-1 (A asks, C listens)** Codes match. Steps:
  1) Mac, tab 1: stop the DISC-1 `receive` with Ctrl-C and wait for the prompt. Then run `"$DROP" pair --listen`.
  2) Phone: Nearby › "More" › "Pair a device" › under "Pair with a nearby device", tap "Pair" next to the CLI.
  3) Compare the codes.
  4) Answer `y` on the Mac, and tap "Codes match" on the phone.

  **Expect:**
  - The phone shows "Asking ‹name›…" / "A code appears on both devices. Check that they match."
  - Then a full-screen navy prompt "Pair with this device?" appears, with the fingerprint and a code shown as "NNN NNN".
  - The Mac shows `Pair with <peer line>?` / `Does the other device show NNN NNN? [y/N]` with the same code.
  - "Codes match" becomes enabled 600 ms after the prompt appears.
  - The phone then shows "Paired — ‹title›" / "‹title› can now send to this device, and you can send to it without a code." with "Done".
  - The Mac prints `Paired: Trusted "<alias>" (claims "<name>") · XXXX-…`.
  - `"$DROP" trust list` shows the device with `(sas)`.
  - Settings › "Paired devices" shows "Paired with a code · XXXX-…".

  **Record:** which side asked; the labels. No fingerprints.
- [ ] **PAIR-1 (A listens, C asks)** Codes match. Steps:
  1) Remove the earlier pairing on both sides. On the phone: "Paired devices" › ⋮ › "Remove…" › "Remove". On the Mac: `"$DROP" trust remove <FP52>`.
  2) Phone: Settings › "Pair a device" › "Wait for a device".
  3) Mac, tab 1, with no `receive` or `pair` running: `"$DROP" pair --to <the address the pairing window shows>`.
  4) Confirm on both sides.

  **Expect:**
  - Before step 2, the Pairing screen caption reads "For five minutes, a device can ask to pair with this one by typing its address. You both check a code."
  - The phone shows "Waiting for a device · m:ss" (5 minutes). Then it shows "Other devices can't see this one nearby. On the other device, choose Connect by address and type:" with the "ip:port" address.
  - The same codes appear, with the same results as above.

  **Record:** as above.
- [ ] **PAIR-1 (M↔A and I↔A)** Codes match, with each side waiting once. Run TX-3 on this pair first. Do it for the Mac app and for the iPhone app. Steps:
  1) Round 1, the Apple app waits. On the Apple app: Settings › "Pair a device" › "Wait for a device". On the phone: Settings › "Pair a device" › under "Pair with a nearby device", tap "Pair" next to the Apple app. Compare the codes, then tap "Codes match" on both.
  2) Remove the pairing on both sides. On the phone: "Paired devices" › ⋮ › "Remove…" › "Remove". On the Apple app: Settings › "Paired devices" › the "…" menu (VoiceOver: "Actions") › "Remove…" › "Remove".
  3) Round 2, the phone waits. On the phone: Settings › "Pair a device" › "Wait for a device". On the Apple app: Connect by address. Set "Address" to the address the phone's window shows, then tap "Pair with this address". Compare the codes, then tap "Codes match" on both.

  **Expect:**
  - In round 1 the Apple app shows "Waiting for a device · m:ss". In round 2 the phone shows "Waiting for a device · m:ss" with its "ip:port" address.
  - Both screens show the same "NNN NNN" code.
  - The phone shows "Paired — ‹title›". The Apple app shows "Paired" / "‹title› can now send to this device, and you can send to it without a code."
  - Each device lists the other under Settings › "Paired devices". On the phone the row reads "Paired with a code · XXXX-…".

  **Record:** both rounds for each pair, and which device asked in each. No fingerprints.
- [ ] **PAIR-2** "Codes don't match": nothing is stored. Steps:
  1) Start pairing as in PAIR-1 (A asks). Stop any running `pair` first.
  2) On the phone, tap "Codes don't match".
  3) Repeat, this time answering `n` on the Mac.

  **Expect, in both rounds:**
  - The phone shows "Pairing declined" / "The other device did not confirm the code. Nothing was saved." with "Done". The core ends the pairing as rejected, whichever side said no.
  - The CLI ends with `pairing_rejected`, exit 6.
  - "Paired devices" is unchanged.
  - `"$DROP" trust list` prints `No trusted devices.`

  **Record:** both rounds. Also note a UX finding: in round 1, the text blames the other device although the phone declined.

### Transfers

- [ ] **TX-1 (C→A)** Paired transfer with a prompt. Steps:
  1) The phone is paired with the CLI, set to "Paired only", with auto-accept off.
  2) Mac, tab 1: `"$DROP" send "$FILES/small.txt" "$FILES/photo-like.bin" --to <phone address>:<port>`.
  3) On the phone, tap "Accept".
  4) Hash the files with `run-as` and compare them to `SHA256SUMS`.

  **Expect:**
  - The prompt shows the chip "Incoming files" and the alias as title, with a shield and "Paired" and no fingerprint.
  - It reads "wants to send you 2 files", lists the files with a "Total" line, and shows "Expires in m:ss" (the offer lasts 120 s).
  - "Accept" becomes enabled after 600 ms. Tapping it shows "Waiting…".
  - Transfers shows "‹done› of ‹total›" while it runs.
  - The history row is titled "small.txt and 1 more", with the subtitle "Received from ‹alias› · ‹time›" and a check icon.
  - The CLI prints `Done: 2 file(s), … in X.X s` and exits 0.
  - Both hashes match.

  **Record:** the hash result; the CLI's `Done` line.
- [ ] **TX-1 (A→C)** Paired transfer with a prompt. Steps:
  1) Mac, tab 1: stop any running `receive` or `pair`. Then run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"`.
  2) Phone: Nearby › tap the CLI row (subtitle "Looks like ‹alias›") › "Add files" › pick `small.txt` and `photo-like.bin` › "Send to ‹name›".
  3) Answer `y` on the Mac.
  4) Run `cd "$RX" && shasum -a 256 *`.

  **Expect:**
  - Send's caption reads "Dropumi checks that this is your paired device before sending."
  - After Send, the app jumps to Transfers.
  - The CLI prints `Saved <name>` for each file.
  - The history row is titled "‹first file› and 1 more", with the subtitle "Sent to ‹alias› · ‹time›" and a check icon.
  - The hashes match `SHA256SUMS`.

  **Record:** the hash result. Repeat both directions with the Mac app and the iPhone app.
- [ ] **TX-2** Auto-accept. Steps:
  1) Settings › "Auto-accept paired devices" switch on. Confirm "Accept paired devices without asking?" with "Turn on".
  2) The paired CLI (tab 1) sends `small.txt` to `<phone address>:<port>`.
  3) Turn the switch off. This takes effect at once, with no question.
  4) Send again.
  5) Reverse direction. In tab 1, stop any running `receive`, then run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX" --accept trusted`. The phone sends `small.txt`. Press Ctrl-C, then run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX" --accept prompt`. The phone sends again.

  **Expect:**
  - **Switch on:**
    - No prompt appears.
    - The running card's title is "1 file", because an auto-accepted receive shows no names.
    - The history row is titled with the saved name ("small.txt"), with "Received from ‹alias› · ‹time›" and a check icon.
  - **Switch off:** the offer prompt appears.
  - **`--accept trusted`:** the CLI asks nothing.
  - **`--accept prompt`:** the CLI asks.

  **Record:** the four results.
- [ ] **TX-3 (C→A)** Everyone, unverified sender: same code, no pairing. Steps:
  1) Open a new tab and set it up as above (unpaired config).
  2) Phone: Settings › "Everyone · 10 min" › "Let everyone send for 10 minutes?" › "Let everyone send".
  3) Mac, new tab: `"$DROP" send "$FILES/small.txt" --to <phone address>:<port> --allow-unverified`.
  4) Compare the codes, answer `y`, then tap "Accept".

  **Expect:**
  - The prompt title reads "Unverified device", with `claims “Dropumi-XXXX”`.
  - A warning icon and "Not paired" show, with a 24-character fingerprint in 6 groups.
  - The prompt reads "Compare this code with the sender's screen", shows the code "NNN NNN", then "Accept only if both screens show the same code."
  - The CLI prints `Verification code: NNN NNN  — check that the other device shows the same code.` and `Does the other device show NNN NNN? [y/N]`, with the same code.
  - The history row reads "Received from Unverified device · ‹time›".
  - After the transfer, "Paired devices" is unchanged and `"$DROP" trust list` prints `No trusted devices.`

  **Record:** whether the codes were equal; the pill text (see EVERY-1).
- [ ] **TX-3 (A→C)** Unverified receiver: code check. Steps:
  1) Mac, unpaired tab: `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX" --policy everyone`.
  2) Phone: tap the CLI row (subtitle "Not paired"), add a file, then "Send to ‹name›".
  3) Compare the codes, tap "Codes match" and answer `y`.

  **Expect:**
  - Send's caption reads "This device isn't paired. You'll both see a code to compare before anything is sent."
  - A code prompt "Check the code" reads "This device isn't paired. Check that its screen shows the same code before anything is sent.", with "Codes don't match" and "Codes match".
  - The CLI asks `Accept 1 file(s) from Unverified device (claims "<name>") · fp XXXX-… · SAS NNN NNN? [y/N]` with the same code.
  - No pairing is created on either side.

  **Record:** as above.
- [ ] **TX-3 (M→A and I→A)** Everyone, unverified Apple sender: same code, no pairing. Run it before PAIR-1 on this pair, so the two are not paired. Do it for the Mac app and for the iPhone app. Steps:
  1) Phone: Settings › "Everyone · 10 min" › "Let everyone send for 10 minutes?" › "Let everyone send".
  2) Apple app: Connect by address. Set "Address" to `<phone address>:<port>` and leave "Fingerprint (optional)" empty. Tap "Choose files", pick a small file, then tap "Send files".
  3) Compare the codes. Tap "Codes match" on the Apple app, then "Accept" on the phone.

  **Expect:**
  - Under the empty fingerprint, the Apple app reads "Without a fingerprint, you compare a code with the other device first."
  - The Apple app shows a code prompt "Check the code" / "This device isn't paired. Check that its screen shows the same code before anything is sent.", with "Codes don't match" and "Codes match".
  - The phone's prompt is the same as in TX-3 (C→A): "Unverified device", `claims “Dropumi-XXXX”`, a warning icon, "Not paired", a fingerprint and the same "NNN NNN" code.
  - The phone's history row reads "Received from Unverified device · ‹time›".
  - Afterwards, neither device lists the other under Settings › "Paired devices".

  **Record:** which Apple app sent; whether the codes were equal.
- [ ] **TX-4** Paired only: an unpaired sender is refused. Steps:
  1) Before you start, dismiss any banner on the phone (X, "Dismiss"). Wait at least 60 s after any earlier refused connection. The refusal banner is skipped while another banner shows, and within 60 s of the previous refusal notice.
  2) Phone on "Paired only". Mac, unpaired tab: `"$DROP" send "$FILES/small.txt" --to <phone address>:<port> --allow-unverified`.
  3) Reverse. In the unpaired tab, stop any running `receive`, then run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"` (trusted-only by default, and not paired with the phone). The phone sends `small.txt` to it.

  **Expect:**
  - **Step 2:**
    - No prompt appears on the phone.
    - The CLI fails with `not_authorized`, exit 6 (docs/cli.md: "The other device refused this device").
    - The phone shows the banner "A device was refused" / "A device on this network tried to connect and was turned away. To pair with it, choose Wait for a device under Pair a device first."
    - This is the no-peer variant, because the refusal happens during the handshake. The other variant, "A device that isn't paired tried to send" / "Pair with it, or let everyone send for 10 minutes.", is not expected here.
  - **Step 3:**
    - The CLI shows no offer.
    - The phone's history row shows a failure on its second line, probably "Only paired devices" (not confirmed in code: check and note what you see).

  **Record:** the CLI exit code; the banner and history text.
- [ ] **TX-5** Decline. Steps:
  1) The CLI sends to the phone. Tap "Decline".
  2) Repeat. On the prompt, start a back swipe from the screen edge, drag back and release so the gesture is cancelled. Then complete a back swipe. This needs gesture navigation; whether it is on is a One UI setting (not confirmed in code: check and note what you see).
  3) The phone sends to a CLI `receive --dir "$RX"` (new `RX`). Answer `n`.

  **Expect:**
  - **Step 1:** the phone's history second line reads "You declined", and the CLI exits 3 (rejected).
  - **Step 2, cancelled swipe:** the prompt stays, nothing is answered, and the CLI is still waiting.
  - **Step 2, completed swipe:** "You declined", and the CLI exits 3.
  - **Step 3:** the phone's history second line reads "Declined".

  **Record:** each result, including the cancelled swipe. ADR-0013 D8 and G39 cover this; real devices are Not tested.
- [ ] **TX-6** A file of at least 2 GiB. Steps:
  1) Build a release-profile core and reinstall. `install -r` keeps the app's data.
     ```sh
     bash scripts/ffi/build-android.sh --abis arm64-v8a,x86_64 --profile release
     ./apps/android/gradlew -p apps/android :app:assembleDebug
     adb -s <serial> install -r apps/android/app/build/outputs/apk/debug/app-debug.apk
     ```
  2) C→A: `"$DROP" send "$FILES/large-2048MiB.bin" --to <phone address>:<port>`, then accept.
  3) A→C: in tab 1, stop any running `receive`, then run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"`. Send `large-2048MiB.bin` from the phone, and answer `y`.
  4) Hash on both sides.

  **Expect:**
  - The running card shows "‹done› of 2.15 GB".
    - ‹done› is in MB with one decimal until about 1 GB, then in GB with two decimals, trailing zeros dropped. For example "512.3 MB of 2.15 GB", later "1.08 GB of 2.15 GB".
    - A rate line reads "NN% · NN MB/s · N min left".
  - The history rows are titled "large-2048MiB.bin", with "Received from ‹alias› · ‹time›" or "Sent to ‹alias› · ‹time›" and a check icon.
  - The phone shows no duration anywhere. It shows only the finish time.
  - The CLI prints `Done: 1 file(s), 2.0 GiB in X.X s` in both directions: as the sender for C→A, and as `receive` for A→C.
  - The hashes match.

  **Record:** the time from the CLI's `Done` line for each direction, and the method (debug APK, release-profile core, Wi-Fi band), plus the hash result.
- [ ] **TX-7** 300 small files. Steps:
  1) C→A: `"$DROP" send "$FILES"/many/* --to <phone address>:<port>`. Pass `many/*`, because a directory is refused with exit 2.
  2) A→C: in tab 1, start a new `RX` and `receive --dir "$RX"`. On the phone, pick all 300 files of `many` with "Add files". The share sheet takes at most 100 items, so use the picker. Whether the picker has a select-all is not confirmed in code: check and note how you selected.
  3) Check the CLI side (suggested, not run): `cd "$RX" && grep ' ./many/' "$FILES/SHA256SUMS" | sed 's#  \./many/#  ./#' | shasum -a 256 -c -`

  **Expect:**
  - The prompt reads "wants to send you 300 files". The first 200 rows end with "and 100 more". Scrolling to that line loads the other 100, and the line then disappears.
  - Record Failed if "and 100 more" stays, or if fewer than 300 rows are listed.
  - The history rows are titled "file-001.bin and 299 more" (C→A) and "‹first file› and 299 more" (A→C), each with a check icon.
  - `run-as … ls files/received` lists `file-001.bin` to `file-300.bin`, flat.
  - On the CLI side, 300 `OK` lines.

  **Record:** the counts and the hash result for both directions.
- [ ] **TX-8** Unicode, right-to-left, duplicate and 0-byte names. Steps:
  1) C→A: send `"$FILES/empty.bin" "$FILES/Résumé – naïve 日本語.txt" "$FILES"/invoice*txt.exe` twice.
  2) List with `run-as … ls -la files/received`, and hash both copies.
  3) Repeat A→C into a new `RX`.

  **Expect:**
  - Both transfers end as completed: a "‹first file› and 2 more" title, a check icon, and no second line.
  - `empty.bin` is 0 bytes.
  - The Unicode name is kept.
  - The right-to-left override character is removed. Per the core's naming rules the file becomes `invoicetxt.exe`; check the exact name.
  - Second copies get ` (1)` before the extension. Nothing is overwritten, and the first copies' hashes are unchanged.
  - The prompt and the history never show text reversed by the override character.

  **Record:** the names you saw and the hash results.

### Cancel and kill

- [ ] **CAN-1** The sender cancels; no partial file is left. Steps:
  1) A→C: during a 2 GiB send, tap the X ("Cancel") on the running card. Then run `ls -la "$RX"`.
  2) C→A: during a 2 GiB send, press Ctrl-C on the Mac. Then run `adb -s <serial> shell run-as io.github.dropumi.debug ls -la files/received`.
  3) M↔A and I↔A: during a 2 GiB send, the sending side taps the X on its running card (VoiceOver and TalkBack: "Cancel"). If the Apple app sends, it uses Connect by address with `<phone address>:<port>`; if the phone sends, it picks the Apple app on Nearby. Then list the receiver's folder: on the phone, `files/received` as in step 2; on the Mac, `ls -la ~/Downloads/Dropumi` (or the folder chosen in Settings); on the iPhone, Files › On My iPhone › Dropumi › Received.

  **Expect:**
  - **Step 1:** the phone's history second line reads "Cancelled by you". `RX` has neither the file nor a `.dropumi-<12 characters>.part`.
  - **Step 2:** the CLI exits 130. The phone's history second line reads "Cancelled by the other device". There is no partial file and no `.part` in `files/received`.
  - **Step 3:** the sender's history reads "Cancelled by you", and the receiver's reads "Cancelled by the other device". The receiver's folder has no partial file and no `.part`.

  **Record:** which device sent on each pair, the listings and the results.
- [ ] **CAN-2** The receiver cancels; no partial file is left. Steps:
  1) C→A, 2 GiB: accept, then tap the X on the phone.
  2) List `files/received`.
  3) Known-limit check: start another receive, press Home, return to Dropumi, and tap the X. While it runs, look at the Nearby pill and This device.

  **Expect:**
  - **Step 1:** the phone's history second line reads "Cancelled by you", and the CLI exits 4 (cancelled).
  - **Step 2:** no partial file and no `.part`.
  - **Step 3:**
    - The banner "Could not cancel" / "Dropumi could not cancel this transfer. A transfer that was already running when you left the app continues until it ends." appears, and the transfer runs to its end. This is a documented limitation (ADR-0013).
    - Until that receive ends, the Nearby pill reads "Starting…", This device shows "Not receiving right now.", and a new CLI send fails to connect (exit 5 expected).
    - Once the receive ends, the pill returns to "Paired devices only" and This device shows an address again. The port may be the same 47321.

  **Record:** the results; whether step 3 shows the banner and the "Starting…" state.
- [ ] **KILL-1** Force-stop the receiver mid-transfer. Steps:
  1) C→A, 2 GiB, accepted.
  2) Mid-transfer: `adb -s <serial> shell am force-stop io.github.dropumi.debug`.
  3) List `files/received`.
  4) Relaunch at once and list again.
  5) After at least 15 minutes, open Dropumi (or leave and return to it) and list again.

  **Expect:**
  - The CLI reports a lost connection (exit 5 expected; note the message).
  - The core removes `.dropumi-*.part` files only when receiving starts, and only those older than 15 minutes (`STALE_AFTER`, `crates/infra/src/fs/cleanup.rs:12`). So the `.part` may still be there after the immediate relaunch, and must be gone after step 5.
  - After the relaunch, Transfers shows no history entry and no running card for the killed transfer. Running transfers are kept only in memory. Any entry or card for it is a Failed finding.

  **Record:** the three listings (file names only), the CLI line, and what Transfers showed.

### Network

- [ ] **NET-1** The sender's Wi-Fi goes off during a transfer. Steps:
  1) A→C, 2 GiB: turn the phone's Wi-Fi off mid-transfer (adb stays on USB).
  2) C→A: turn the Mac's Wi-Fi off mid-transfer.

  **Expect:**
  - The receiving side fails within about 30 s of the cut. That is the receiver's idle timeout.
  - The sending side fails at once on a socket error. Otherwise it fails after up to 120 s (write stall), or up to 60 s if it was waiting for the receiver's final acknowledgement.
  - The phone's history shows the failure on a second line: "Connection lost" or "No answer" (a timeout).
  - The receiver keeps no partial file (`ls -la "$RX"` or `files/received`).

  **Record:** which failure text appeared; the time to failure on each side; the listings.
- [ ] **NET-2** The receiver's Wi-Fi goes off during a transfer. Steps:
  1) C→A, 2 GiB: turn the phone's Wi-Fi off mid-transfer.
  2) After it fails, list `files/received` and open This device.

  **Expect:**
  - The CLI fails with a network error (exit 5 expected).
  - The phone's history shows a failure.
  - There is no partial file and no `.part`.
  - This device shows "No local network address right now. Connect to Wi-Fi."

  **Record:** both sides' messages and the listing.
- [ ] **NET-3** The access point reboots. Steps:
  1) Start a 2 GiB C↔A transfer, then reboot the access point.
  2) When it is back, open Nearby and send again.

  **Expect:**
  - The transfer fails on both sides, with no partial file on the receiver.
  - Afterwards Nearby lists the CLI again and a new transfer completes.
  - The phone's address may change: read it again.

  **Record:** the failure texts and the time to recover.
- [ ] **NET-4** Client isolation: Connect by address works. Steps:
  1) Use a network that filters multicast (for example a guest network). If you have none, record Not tested.
  2) Set up trust so the test cannot fail for trust reasons. Use either option:
     - the paired CLI identity (tab 1), with `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"`;
     - an unpaired tab, with `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX" --policy everyone`, and the phone on "Everyone · 10 min".

     Connect by address always sends with unverified peers allowed. A default `receive` refuses an unpaired phone, and a phone on "Paired only" refuses an unpaired CLI.
  3) Phone: Nearby › "More" › "Connect by address". Fill in "Address" with `<Mac's Wi-Fi address>:<port>`, then "Choose files" › "Send files".
  4) Mac: stop the `receive` with Ctrl-C. From the same identity, run `"$DROP" send "$FILES/small.txt" --to <phone address>:<port>`. Add `--allow-unverified` if this identity is unpaired.

  **Expect:**
  - Nearby probably stays on "Looking for devices…".
  - With the fingerprint field empty, the caption reads "Without a fingerprint, you compare a code with the other device first."
  - With an unpaired receiver:
    - the phone shows a "Check the code" prompt;
    - the CLI asks `Accept 1 file(s) from Unverified device (claims "<name>") · fp XXXX-… · SAS NNN NNN? [y/N]`.
  - Both directions complete if the network passes traffic between clients.

  **Record:** the network type, which identity you used, and both results.
- [ ] **NET-5** Wi-Fi without internet while mobile data is on. Steps:
  1) Put both the phone and the Mac on a Wi-Fi network with no internet access. Keep mobile data on for the phone.
  2) Note whether One UI stays on that Wi-Fi. It may switch away (not confirmed in code: check and note what you see).
  3) Mac, tab 1: stop any running `receive` with Ctrl-C. Run `ipconfig getifaddr en0` again to get the Mac's new address. Then run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"`.
  4) Open This device › Address.
  5) Send from the phone to the CLI: from Nearby if it lists the CLI, otherwise with Connect by address and `<Mac's Wi-Fi address>:<port>`.
  6) Send from the CLI to `<phone address>:<port>`.

  **Expect:**
  - The address comes from the Wi-Fi network even when the default network is cellular ("A LAN without internet counts too", `LocalAddresses.kt:68`).
  - The app process is not bound to a network, so the phone-to-Mac send may go over mobile data and fail. This row is there to find that out.

  **Record, each separately:**
  - whether an address is shown, and whether it is the Wi-Fi one (do not write it);
  - the phone→CLI result;
  - the CLI→phone result;
  - whether Nearby listed the CLI (this is part of spike S-AN2).

### Background, lock screen and notifications

- [ ] **BG-4** The phone sends, then Home and lock for 5 minutes. Steps:
  1) Allow notifications.
  2) Mac, tab 1: stop any running `receive`, then run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX" --accept trusted`. Send 2 GiB from the phone (A→C).
  3) Press Home, then lock, and wait 5 minutes.
  4) Unlock and open Dropumi.

  **Expect:**
  - The transfer continues under the `dataSync` service. Its notification reads "Sending 1 file" with "‹done› of 2.15 GB".
  - The CLI prints `Saved large-2048MiB.bin`.
  - The history row is titled "large-2048MiB.bin", with "Sent to ‹alias› · ‹time›" and a check icon.
  - The notification goes away when the transfer ends. There is no completion notification.
  - If Android refused the service, the card says "May pause while Dropumi is in the background".

  Doze (S-AN5) is still an open spike.

  **Record:** whether it finished, the time, and One UI's battery settings for Dropumi.
- [ ] **BG-5** The phone receives in the background with the screen locked. Steps:
  1) C→A, 2 GiB: accept, then press Home and lock until the CLI finishes.
  2) With the phone still locked, run `"$DROP" send "$FILES/small.txt" --to <phone address>:<port>`.
  3) Unlock.

  **Expect:**
  - **Step 1:**
    - The accepted receive continues, with the notification "Receiving 1 file".
    - The CLI prints `Done: …` and exits 0.
    - The history row is titled "large-2048MiB.bin", with "Received from ‹alias› · ‹time›" and a check icon.
  - **Step 2:** the new send is refused, because receiver mode is not built. The CLI fails to connect (exit 5 expected; note the message).
  - **Step 3:**
    - No prompt waits.
    - The pill reads "Starting…" while a new receiver starts, then "Paired devices only".
    - If an earlier receive were still running, the pill would stay on "Starting…" and This device on "Not receiving right now." until that receive ends.

  **Record:** both results, and what the pill showed after unlocking.
- [ ] **LOCK-1 (extra)** A prompt never shows over the lock screen. Source: ADR-0013 D8; threat model G39. The automated `LockScreenTest#promptNeverShowsOverTheLockScreen` cannot run on a phone with a PIN. Steps:
  1) The CLI sends to the phone, and the offer prompt is on screen.
  2) Lock with the side key. Check that the lock screen asks for the PIN.
  3) Wait 10 s, then wake the phone without unlocking. Look at the lock screen and its notifications.
  4) Unlock within the 120 s.
  5) Repeat, and stay locked for more than 120 s before unlocking.

  **Expect:**
  - **While locked:**
    - Only the lock screen shows. No Dropumi prompt or Accept shows over it.
    - No notification appears for the offer.
    - The CLI is still waiting.
  - **Unlocked within 120 s:**
    - The same prompt is back. "Decline" is enabled, and "Accept" becomes enabled again after 600 ms.
    - Until that offer's session ends, the Nearby pill reads "Starting…" and This device shows "Not receiving right now.".
    - A new CLI send fails to connect during this time (exit 5 expected).
  - **Unlocked after 120 s:**
    - The prompt shows "Expired" or has closed.
    - The CLI is rejected with reason `timeout` (exit 3 expected).
    - The history second line reads "Request expired" (not confirmed on a device).

  **Record:** what the lock screen showed, and both outcomes.
- [ ] **LOCK-2 (extra)** Lock-screen notification privacy. Source: ADR-0013 D7; `NotificationFactory.kt`, `TransferNotificationText.kt`. Steps:
  1) Notifications allowed. Start a 2 GiB transfer (once sending, once receiving).
  2) Press Home, then lock.
  3) Wake without unlocking, and read the notification.
  4) Unlock, pull down the shade, and tap the notification.

  **Expect:**
  - On the lock screen you see one of two versions:
    - the public version, "Dropumi is transferring files";
    - the private version, "Sending 1 file" or "Receiving 1 file" with "‹done› of 2.15 GB".
  - Which one One UI shows depends on its hide-content setting (not confirmed in code: check and note what you see).
  - Neither version ever shows a peer name or a file name.
  - The notification is silent and has no actions.
  - Tapping it opens Dropumi.

  **Record:** One UI's lock-screen notification setting and the exact lines shown.
- [ ] **REARM-1 (extra)** Re-arming when returning to the app. Source: ADR-0013 (line 109); `PromptArming.kt`. Emulators cover it (`acceptReArmsAfterResume`, `acceptReArmsWhenTheWindowRegainsFocus`). Steps:
  1) The offer prompt is on screen, and "Accept" is enabled.
  2) Pull down the notification shade, then close it.
  3) Press Home, then reopen Dropumi from Recents within 120 s.
  4) Repeat steps 2 and 3 on a code prompt ("Codes match").

  **Expect:**
  - After each return, "Accept" or "Codes match" is disabled briefly, then enabled after 600 ms.
  - "Decline" stays enabled.
  - Nothing is answered by itself.

  **Record:** what you saw for each return.
- [ ] **PROMPT-1 (extra)** A press held from before arming must not answer. Source: Apple commit `f642ef7` (the Apple UI state tests). That commit says "the same bug was found in Compose on Android". The Android code has no held-press test under `androidTest/prompt/`. Steps:
  1) The offer prompt is on screen.
  2) Pull the shade down and up. This re-arms the prompt (REARM-1).
  3) At once, press and hold "Accept" for about 2 s, then release.
  4) Try the same as a prompt first appears.
  5) Positive control: once armed, tap "Accept".
  6) Repeat with "Codes match" on a code prompt.

  **Expect:**
  - **Press held from before arming:** it never answers. The prompt stays, no "Waiting…" shows, and the CLI is still waiting.
  - **Tap after arming:** it answers.

  **Record:** Failed, with exactly what happened, if a held press answered.
- [ ] **EVERY-1 (extra)** Everyone countdown and expiry. Source: `ScreenText.kt:49-52`, `crates/ffi/src/types.rs:185-186`, `AppModel.kt:1279-1286`, `Settings.kt:34`. Steps:
  1) Turn on "Everyone · 10 min".
  2) Watch the Nearby pill and the Settings caption for a minute.
  3) Keep Dropumi on screen for 10 minutes.
  4) Force-stop and relaunch.

  **By design:**
  - The pill reads "Everyone · m:ss", counting down from about 10:00.
  - The caption reads "Anyone nearby can ask to send for m:ss. You compare a code before accepting. Only while Dropumi is open."
  - After 10 minutes the banner reads "Back to paired devices only" / "The 10 minutes for everyone ended."
  - After the relaunch the pill reads "Paired devices only".

  **What the code shows instead:**
  - The core sends the window's end in Unix seconds. The app subtracts the wall clock in milliseconds and clamps the result at 0.
  - So for the whole window the pill reads "Everyone · 0:00", and the caption reads "Anyone nearby can ask to send for 0:00. You compare a code before accepting. Only while Dropumi is open."
  - There is no countdown. TX-3 and SPOOF-1 show the same pill.

  Leaving and returning within the 10 minutes probably starts a fresh 10-minute window. This is derived from the code, not confirmed: note it.

  **Record:** the exact pill and caption text. If they read "0:00", record Failed against the by-design text and cite the units mismatch. Also record whether the banner came after 10 minutes, and the pill after the relaunch.

### Share, save and permissions

- [ ] **SHARE-3** Share one file and several files. Steps:
  1) In My Files or Gallery, select one file › Share › "Dropumi".
  2) Repeat with several files.
  3) Share the same file again.
  4) Share a web link from Chrome.
  5) Finish one send to the CLI and hash the result.

  **Expect:**
  - Dropumi opens on Send with the items. It reads "N files ready to go" (or "N files · X MB ready to go") and "Choose a device", with no device chosen.
  - Nothing is sent until you pick a device and tap "Send to ‹name›".
  - Several items join one draft, and duplicates are dropped.
  - At most 100 items are taken.
  - A shared link with no file does not open Dropumi. This is derived from the code: note it.
  - If two "Dropumi" entries show, another build is still installed.

  **Record:** the source app, the counts and the hash result.
- [ ] **EXP-1** Save to Downloads. Precondition: the disclosure text shows only on the first Save after install or `pm clear`. It is stored per installation. Run this row first after a reset if you want to see it. Steps:
  1) After a one-file receive and a several-file receive, go to Transfers, tap the history row, then "Save to Downloads".
  2) Save the same row again.
  3) Hash with `adb … sha256sum` or `adb pull`.
  4) Open a saved file in another app.
  5) Tap "Open" in Dropumi.
  6) Read Settings › "Received files".

  **Expect:**
  - **First Save after install or `pm clear`:** the banner reads "Saved to Downloads" / "Saved files can be opened by other apps that can see your Downloads."
  - **Every later Save:** the banner reads "Saved to Downloads" with "1 file saved." or "N files saved.", for example "3 files saved.".
  - Files appear in `Download/Dropumi/`.
  - The second save gets a new name, and the first copy is unchanged (same hash).
  - The app-private copy stays, so "Open" still works.
  - Settings › "Received files" always includes "Saved files can be opened by other apps that can see your Downloads."

  **Record:** the names, the hashes, the banner texts, and the app that opened the file.
- [ ] **HIST-1 (extra)** Share, Delete and Clear on a received row. Source: ADR-0013 D4 (lines 76-78); `TransfersScreen.kt`. G45's delete test runs only on the JVM. Run it after EXP-1, on a row whose files you have also saved. Steps:
  1) On a received row, tap "Share", and send the file to another app.
  2) On the same row, tap "Delete" › "Delete these files?" › "Delete". Then run `adb -s <serial> shell run-as io.github.dropumi.debug ls -la files/received` and `adb -s <serial> shell sha256sum "'/sdcard/Download/Dropumi/<name>'"`.
  3) Above the list, tap "Clear" › "Clear the list?" › "Clear and delete". List `files/received` and hash `Download/Dropumi` again.

  **Expect:**
  - **Step 1:** the system share sheet opens with the file. Whether the other app receives it intact is not confirmed in code: check and note what you see.
  - **Step 2:**
    - The confirmation reads "Dropumi's copies of the files from this transfer are deleted. Copies you saved or shared elsewhere stay there."
    - Afterwards `files/received` no longer lists those files.
    - The copy in `Download/Dropumi/` has the same hash as before.
    - Whether the row itself stays is not confirmed in code: check and note what you see.
  - **Step 3:**
    - The confirmation reads "Files you received are deleted from Dropumi too. Files you saved to Downloads or another folder stay there."
    - Afterwards the list is empty, `files/received` lists no received files, and `Download/Dropumi/` is unchanged (same hashes).

  **Record:** the app you shared to, the listings (file names only) and the hash results.
- [ ] **PERM-2** Notifications denied. Steps:
  1) Reset the notification permission (see Reset).
  2) Start a transfer. Transfers shows the card "Allow notifications to see progress when you leave Dropumi".
  3) Tap "Allow" and refuse in the system dialog.
  4) Tap "Allow" again.
  5) With the permission denied, run a 2 GiB transfer and press Home.
  6) Run `adb -s <serial> shell am force-stop io.github.dropumi.debug` and relaunch. Start a new transfer (Send, or Accept an offer), open Transfers, and tap "Not now".
  7) Allow notifications in settings and return.

  **Expect:**
  - **Step 3:** the system dialog appears.
  - **Step 4:** the app's notification settings open instead of the dialog.
  - **Step 5:** the transfer still runs and finishes, and no Dropumi notification appears in the shade.
  - **Step 6:**
    - After the relaunch, the card is not on Transfers until the new transfer starts.
    - "Not now" removes it.
    - It does not come back on later transfers until the next force-stop or relaunch.
  - **Step 7:** the card disappears once notifications are allowed.
  - The dialog never appeared at onboarding or over a prompt.

  **Record:** each step's result.
- [ ] **PERM-NEARBY (extra)** No nearby-devices or other permission is asked. Source: ADR-0013 D12 (line 138) and line 57; `scripts/android/check-manifest.sh`. Steps:
  1) During the whole session, note every system permission dialog.
  2) At the end, open the app's info page › Permissions.
  3) Optional (standard adb, not in the repo docs): `adb -s <serial> shell dumpsys package io.github.dropumi.debug | grep -i permission`.

  **Expect:**
  - The only dialog was notifications. None appeared for Nearby devices, location, Bluetooth, storage or local network, and Save asked nothing.
  - `dumpsys` lists only the 8 allowlisted entries. `io.github.dropumi.debug.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION` is expected; it is signature level and never shows a dialog.
  - How One UI lists permissions is not confirmed in code: check and note what you see.

  **Record:** the dialogs seen and the Permissions page.

### Security UI

- [ ] **SPOOF-1** A renamed device never shows as the paired one. Steps:
  1) The phone is paired with CLI identity A (tab 1), and A's `receive --dir "$RX"` is running.
  2) Open a new tab, set it up as above (config B), and echo `$DROPUMI_CONFIG_DIR`. Run `"$DROP" identity set-name "<A's name>"`, then `"$DROP" receive --dir "$(mktemp -d)"`.
  3) Look at Nearby. On Send, choose B's row and read the caption (do not send).
  4) Stop B's `receive` with Ctrl-C. Put the phone on Everyone. In B's tab, run `"$DROP" send "$FILES/small.txt" --to <phone address>:<port> --allow-unverified`.
  5) Decline.

  **Expect:**
  - Nearby shows two rows with the same name. A's subtitle reads "Looks like ‹alias›"; B's reads "Not paired".
  - Choosing B on Send shows "This device isn't paired…", and Send asks for a code check before sending.
  - B's prompt title reads "Unverified device", with `claims “‹A's name›”`, a warning and "Not paired", plus a fingerprint and a code.
  - B never gets a shield or "Paired".

  **Record:** the row subtitles and the prompt title.
- [ ] **TRUST-1 (extra)** Trust by full fingerprint, and a fingerprint pin. Source: `apps/android/README.md` ("add one by its full fingerprint"); AGENTS.md §7; `PairedDevicesScreen.kt`, `ConnectByAddressScreen.kt`, `ErrorCatalog.kt`. Steps:
  1) Open a new tab and set it up as above (a new identity). Run `"$DROP" identity show` and keep the `Full:` line on screen (do not record it).
  2) Phone: Settings › "Paired devices" › "Add by fingerprint". Type the 52 characters into "Fingerprint (52 characters)". Optionally, focus the field and run `adb -s <serial> shell input text <FP52>` (standard adb, not in the repo docs). Then tap "Add" › "Add this device?" › "Add device".
  3) Mac, same tab: read the phone's full fingerprint under Settings › This device. Run `"$DROP" trust add <phone FP52>`, then `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX" --accept trusted`.
  4) Phone: Nearby › "More" › "Connect by address".
     - "Address": `<Mac's Wi-Fi address>:<port the CLI printed>`.
     - "Fingerprint (optional)": the first 24 characters (or all 52) of the CLI's fingerprint.
     - Then "Choose files" › "Send files".
  5) Repeat step 4 with one letter of the pin changed to a different letter.
  6) Clean up: remove the device on the phone ("Paired devices" › ⋮ › "Remove…" › "Remove"), and on the Mac run `"$DROP" trust remove <phone FP52>`.

  **Expect:**
  - **Step 2:** the new row reads "Added by fingerprint · XXXX-…".
  - **Step 4:**
    - The caption under the pin reads "Dropumi checks this fingerprint before anything is sent."
    - No code is asked on the phone.
    - The CLI prints `Saved <name>`.
    - The history row has a check icon.
  - **Step 5:**
    - The send fails, and the history second line reads "Not the device you paired with".
    - No new file arrives in `$RX`.
    - What the CLI prints is not confirmed in code: check and note what you see.

  **Record:** the row subtitle and each step's result. No fingerprints.

### Accessibility and layout

- [ ] **UI-1** TalkBack, the largest text size, keyboard only. Before you start, record the font scale (see Reset). Steps:
  1) Turn on TalkBack.
  2) Walk Nearby, Send, a prompt (the CLI sends), Settings, Paired devices and Transfers.
  3) Set the largest font size, then repeat on Nearby, Send and a prompt.
  4) With a hardware keyboard (if you have one), answer a prompt with Tab or the arrows, Enter or Space, and Escape.
  5) Put the font scale back to the recorded value.

  **Expect:**
  - **Labels read aloud:**
    - "More", "Send to ‹name›", "Back", "Dismiss", "Cancel";
    - the long-press action "Pair with this device";
    - "Actions for ‹alias›";
    - the switch "Auto-accept paired devices";
    - "Tap for actions" / "Actions shown";
    - "Done" for a completed history row.
  - **On a prompt:**
    - pane titles "Incoming files" / "Pair with this device?" / "Check the code";
    - "Verification code: 1 2 3, 4 5 6" (example digits);
    - "Fingerprint: …";
    - "Waiting…", "Expired" and banners are announced.
  - **Hidden from TalkBack:** the radar, and everything under a prompt.
  - **Large text** (font scale 1.3 and above): button pairs stack, and the prompt puts Decline first. Note any clipped text (not confirmed in code: check and note what you see).
  - **Keyboard:** a prompt has no default focus. Enter or Space acts only on a focused button. Escape declines.

  **Record:** the font scale from `adb -s <serial> shell settings get system font_scale` (standard adb, not in the repo docs), and any label that is missing or wrong. If you have no keyboard, record the keyboard part as Not tested.
- [ ] **UI-3** Rotation and tablet layout. Steps:
  1) Turn auto-rotate on.
  2) Rotate on each of these screens:
     - Nearby;
     - Send, with a device chosen;
     - a prompt;
     - the Pairing waiting screen;
     - an open "Let everyone send for 10 minutes?" confirmation;
     - during a transfer.

  **Expect:**
  - The tab and the screen stack are kept.
  - The device chosen on Send is forgotten: choose it again.
  - The prompt stays, and "Accept" re-arms after 600 ms.
  - Receiving continues (the pill is unchanged).
  - **Derived from the code, not confirmed:**
    - an open confirmation probably closes, with nothing changed;
    - landscape probably switches to the side rail with the wordmark, because the window is 600 dp or wider;
    - the problem card probably still says "phone".

  **Record:** each screen's result. Tablet layout: Not tested (no tablet).

### Overlay, backup and automated harnesses (a person must be present)

> **Warning.**
> - **`ui-tests.sh`** (OVL-1, UITESTS-1) rebuilds and reinstalls `io.github.dropumi.debug` with `adb install -r -t`, which keeps its data. It leaves `io.github.dropumi.debug.test` installed. Uninstall that afterwards with `adb -s <serial> uninstall io.github.dropumi.debug.test`.
> - **`interop.sh`** (INTEROP-1) builds and installs `io.github.dropumi.qa` and `io.github.dropumi.qa.test`. It never touches the debug app, and its exit trap uninstalls both.
> - Check what is left with `adb -s <serial> shell pm list packages | grep dropumi`.
> - The docs say: never run them, or `bmgr`, on a device signed in to an account whose data you care about.
> - Run them after the manual rows. Before you start, record the display, font and lock values (see Reset).
> - In the tab you use, first run the `JAVA_HOME`, `ANDROID_HOME` and `PATH` lines from "What you need". Gradle needs JDK 17.

- [ ] **OVL-1** Another app's overlay over the offer prompt, the Everyone confirmation and Send. There is **no safe manual method in the facts.** The only overlay app is the test-only helper, so never install it by hand. Steps:
  1) Run `ANDROID_SERIAL=<serial> bash scripts/android/ui-tests.sh --class io.github.dropumi.prompt.OverlayWindowTest`.

  **Expect:**
  - The pass lines report OK, and the script prints `overlay:` lines.
  - The exit trap uninstalls `io.github.dropumi.overlayhelper`. Check with `adb -s <serial> shell pm list packages | grep dropumi`.

  `OverlayProtectionTest` checks that the window flags hide other apps' overlays. On this phone the only record is commit `785f811` ("all seven tests pass on that device"). To record it, run the same command with `--class io.github.dropumi.prompt.OverlayProtectionTest`.

  **Record:** the `overlay:` lines and the pass lines. Replace `device <serial>` with `<serial>`.
- [ ] **UITESTS-1 (extra)** The full instrumented suite on the phone. Source: `docs/development.md` (line 234); `scripts/android/ui-tests.sh`. Steps:
  1) Run `ANDROID_SERIAL=<serial> bash scripts/android/ui-tests.sh`.

  **Expect:**
  - The lock-screen test is listed as `not runnable on this device (secure lock screen): io.github.dropumi.prompt.LockScreenTest#promptNeverShowsOverTheLockScreen`, and it is not counted.
  - Its sibling `promptWindowRequestsNoLockScreenFlags` passes.
  - The two `DocumentGuardTest` hard-link tests either run in full and pass, or are listed as `not runnable on this image (link EACCES)` (or EPERM).
  - The exit status is 0.
  - Afterwards, `wm size`, `wm density`, `font_scale` and `locksettings get-disabled` match the values you recorded.

  **Record:** the pass lines, and whether the hard-link tests ran in full.
- [ ] **INTEROP-1 (extra)** The app's core against the CLI: loopback over adb, plus the two NSD tests over Wi-Fi. Source: `docs/development.md` (lines 242-246); `crates/cli/tests/e2e/interop/android.rs`; threat model G39 and G44 ("Real devices: Not tested"). Steps:
  1) The Mac and the phone are on the same Wi-Fi.
  2) Run `ANDROID_SERIAL=<serial> bash scripts/android/interop.sh`.

  **Expect:**
  - All 7 `android_*` tests pass.
  - Five of them talk to the CLI through `adb reverse` and `adb forward` on `127.0.0.1` over USB, not over Wi-Fi.
  - `android_nsd_cli_finds_app_at_random_host` passes on this phone because the app is not advertised (the gate is closed). It asserts that the CLI does not find the app, and it never checks a random SRV host. Do not count it as DISC-4 or NSD-1 host-label evidence.
  - `android_nsd_app_finds_cli_and_sends` is the only Wi-Fi transfer in the suite.
  - The exit trap uninstalls `io.github.dropumi.qa` and `io.github.dropumi.qa.test`.

  **Record:** the pasted `test result:` lines, with `~` for the home directory.
- [ ] **BAK-1** Device-to-device transfer leaves out the identity. This needs two throwaway Android phones and a test account. **Never run it on this phone.** **Record:** `Not tested` (no throwaway devices).
