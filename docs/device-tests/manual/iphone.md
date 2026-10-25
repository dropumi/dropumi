# Manual device testing: iPhone app

Read the [common page](README.md) first: privacy, test files, the CLI as a test peer, and the session plan.

This section covers the iPhone app (`Dropumi-iOS`, bundle ID `io.github.dropumi.Dropumi`) and its share extension. It is tested on the iPhone 15 only. Never use a simulator.

- **Build from the APPLE worktree only.** That is `dropumi-apple`, on branch `feat/native-apps-apple`. The ANDROID worktree holds an older copy of the Apple app.
- Run every command below from the root of the APPLE worktree, unless a step says otherwise.
- **Current status.** In run 2026-10-18-01, only automated tests passed on the iPhone: the hosted tests, the UI state tests, and 5 core-to-CLI interop tests over Wi-Fi. Every iPhone matrix row is still Not tested.
- A step marked "(iOS)" is a general iOS menu path or behaviour. The repo does not document it.
- **Pasting commands.** Paste the code blocks exactly as shown. They have no `#` comments on purpose. The Mac's shell is zsh, and an interactive zsh without `setopt interactivecomments` does not treat a pasted `#` as a comment.

## Run it

### 1. What you need
- **Tools.** You need:
  - Xcode 27.1 with the iOS SDK;
  - xcodegen 2.46.0;
  - Rust stable with the Apple targets (the recorded run used 1.99.0; the MSRV is 1.89);
  - `python3`, `git` and `shasum`, which the build script calls.
- **Check the tools.** `xcodegen --version` should print 2.46.0.
  ```sh
  sw_vers
  xcodebuild -version
  xcrun --sdk iphoneos --show-sdk-version
  swift --version
  xcodegen --version
  rustup target list --installed
  ```
- **Install the Rust targets.** `build-apple.sh --slices all` always builds the simulator slice. So you need the `-sim` target even though no simulator is used. `aarch64-apple-darwin` is the Mac's own target.
  ```sh
  rustup target add aarch64-apple-ios aarch64-apple-ios-sim
  ```
- **xcodegen 2.46.0.** `brew install xcodegen` may install a different version. These are the same steps CI uses, with the same folder layout (`.github/workflows/ci.yml`, step "Install xcodegen 2.46.0").
  ```sh
  XG="$(mktemp -d)"
  curl -fsSL -o "$XG/xcodegen.zip" https://github.com/yonaskolb/XcodeGen/releases/download/2.46.0/xcodegen.zip
  echo "4d9e34b62172d645eed6457cac13fc222569974098ef4ee9c3368bedf0196806  $XG/xcodegen.zip" | shasum -a 256 -c -
  unzip -q "$XG/xcodegen.zip" -d "$XG/xcodegen"
  export PATH="$XG/xcodegen/xcodegen/bin:$PATH"
  xcodegen --version
  ```
  - The `PATH` change lasts only for this Terminal tab.
  - Run §3 in this same tab.
- **The iPhone:**
  - Developer Mode is on: Settings › Privacy & Security › Developer Mode (iOS).
  - It is connected to the Mac and trusts it.
  - Keep it **unlocked** for the whole session. A locked phone stopped one earlier run with "Timed out while enabling automation mode".
  - During long transfers, set Settings › Display & Brightness › Auto-Lock to Never (iOS). Locking sends Dropumi to the background, where it stops accepting connections.
- **Network.** All devices are on the same Wi-Fi.

### 2. Signing (once)
```sh
[ -f apps/apple/Config/Signing.local.xcconfig ] || cp apps/apple/Config/Signing.local.xcconfig.example apps/apple/Config/Signing.local.xcconfig
```
- This copies the example only when there is no local copy yet. A local copy that holds your team may already exist, and this command never overwrites it.
- Edit `DEVELOPMENT_TEAM` only if it still reads `YOURTEAMID`. Set it to `DEVELOPMENT_TEAM = <team ID>`, using your personal team (Xcode › Settings › Accounts).
- Never commit this file. It is gitignored.
- Automatic signing registers:
  - the App IDs `io.github.dropumi.Dropumi` and `io.github.dropumi.Dropumi.share`;
  - the App Group `group.io.github.dropumi`.

### 3. Build the Rust core (release profile) and generate the project
```sh
bash scripts/ffi/build-apple.sh --profile release
cat apps/apple/DropumiKit/Artifacts/core-stamp.txt
(cd apps/apple && xcodegen generate)
```
- `core-stamp.txt` must contain the line `profile release` and the line `slices aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin`.
- Re-run `xcodegen generate` after any change to `Signing.local.xcconfig`.
- **Stale core.** The build phase "Check the Rust core" fails if the core is stale, with "error: the Rust core is older than the Rust sources; run: bash scripts/ffi/build-apple.sh --profile dev".
  - **Ignore its `--profile dev` hint.** Run `bash scripts/ffi/build-apple.sh --profile release`.
  - Check that `core-stamp.txt` says `profile release` before you build.
  - The errors for a missing core and for a missing slice give the same dev hint. Ignore it there too.
- **Wrong core.** A build warning "Release build with a dev-profile Rust core" means the wrong core. In a Release build this is only a warning, so the build does not stop. Rebuild the core with `--profile release`.
- The Xcode build never runs cargo itself.

### 4. Release build and install
The repo documents only the Debug command. This Release version adds `-configuration Release` to it, and the app then lands in `Release-iphoneos`.
```sh
xcrun devicectl list devices
xcodebuild build -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-iOS -configuration Release \
  -destination 'generic/platform=iOS' -derivedDataPath apps/apple/build -allowProvisioningUpdates
xcrun devicectl device install app --device <UDID> \
  apps/apple/build/Build/Products/Release-iphoneos/Dropumi.app
```
- Find `<UDID>` in the output of `devicectl list devices`. Never record it.
- **If signing fails** because the iPhone is not registered with your team:
  - Open the project in Xcode once (`open apps/apple/Dropumi.xcodeproj`). Pick the `Dropumi-iOS` scheme and the iPhone, and press Run (`apps/apple/README.md`). This makes a Debug build. Then run the Release command above again.
  - A command-line alternative is not in the repo: add `-allowProvisioningDeviceRegistration` next to `-allowProvisioningUpdates`, with `-destination 'platform=iOS,id=<UDID>'`. Check the flag with `xcodebuild -help`.
- **Earlier test runs** (`xcodebuild test`, `scripts/apple/interop.sh`) installed the Debug app and a UI-test runner app. For a fresh session, delete both from the phone first (not in the repo).
- **Release builds** leave out the fake core and the UI-test scenarios (`Config/Release.xcconfig`). REL-1 (iOS) below checks this on the binary you install.

### 5. Trust and launch
- **First launch.** Go to Settings › General › VPN & Device Management and trust your developer certificate.
- **Launch.** Tap the Dropumi icon. The command-line launch is not in the repo. Check its syntax with `xcrun devicectl device process launch --help`.
  ```sh
  xcrun devicectl device process launch --device <UDID> io.github.dropumi.Dropumi
  ```

### 6. First launch: onboarding and the Local Network prompt
1. **Welcome.** It shows "Just drop it nearby." and "Dropumi is a pre-release and has not had a security audit yet." Tap "Continue".
2. **"Name this device".** The field is prefilled with `Dropumi-XXXX`. **Keep that default name** (privacy rule). Tap "Continue".
3. **"Find devices on your network".** It says "Next, iOS asks whether Dropumi may find devices on your local network. Choose Allow, or Dropumi can't see other devices." Tap "Continue".
4. **"Who can send to this device?".** Keep "Paired devices only" (the default), then tap "Start".
   - Receiving starts now, and iOS asks for Local Network access. The purpose text reads: "Dropumi finds nearby devices and sends files to them directly over your local network. Nothing goes through the internet."
   - Choose **Allow**. Earlier runs failed until Local Network was allowed.

Notes:
- The iPhone app asks for no notification permission. Only the Mac app requests one.
- "Everyone" is never restored at launch. It comes back as Paired only.
- "Auto-accept paired devices" is off by default.

### 7. The CLI partner on the Mac (throwaway config)
If the common CLI setup is done, tab 1 is that terminal: skip `cargo build`, `export DROPUMI_CONFIG_DIR` and `make-test-files.sh`. Tab 2 uses `$UNPAIRED`, tab 3 uses `$SPOOF`.

**Tab 1.** Run this in the worktree root:
```sh
cargo build --release --locked -p dropumi-cli
DROP="$PWD/target/release/dropumi"
export DROPUMI_CONFIG_DIR="$(mktemp -d)"
RX="$(mktemp -d)"
FILES="$(mktemp -d)"; bash scripts/device-tests/make-test-files.sh "$FILES" 2048
echo "$FILES"
ipconfig getifaddr en0
```
- `DROPUMI_CONFIG_DIR` gives this tab its own throwaway identity. It is never the real configuration.
- `RX` is the receive folder.
- `FILES` holds the test files and `SHA256SUMS`. Keep the path that `echo "$FILES"` prints for this session. Never record it.
- `ipconfig getifaddr en0` prints `<Mac's Wi-Fi address>`. Never record it.

**Tabs.** Use three tabs, each with its own identity:
- **Tab 1 (paired).** The block above. PAIR-1 (I↔C) pairs this identity with the iPhone. Use it for:
  - DISC-1 (I↔C) step 1 and PAIR-1 (I↔C);
  - TX-1, TX-2, TX-5, PROMPT-2, TX-6, TX-7 and TX-8;
  - CAN-1, CAN-2, NET-4 and BG-1;
  - PROMPT-1 and KILL-1.
- **Tab 2 (unpaired).** It is never paired. Use it for DISC-1 (I↔C) step 3, DISC-3, TX-4, TX-3 (C↔I), PAIR-2 (C half) and BG-1 (receive) step 4.
- **Tab 3 (spoof).** Use it for SPOOF-1 only.

**New tab.** Shell variables do not carry over to a new tab, and a new tab opens in ~. Run this in each new tab:
```sh
cd "<APPLE worktree root>"
export DROPUMI_CONFIG_DIR="$(mktemp -d)"
DROP="$PWD/target/release/dropumi"
FILES="<the path tab 1 printed>"
RX="$(mktemp -d)"
```
- **Always set `DROPUMI_CONFIG_DIR`.** Without it, the CLI uses the real configuration directory (`docs/cli.md`, "Configuration directory").
- **Reuse tab 1's `FILES` path.** Never run `make-test-files.sh` again. It refuses a non-empty folder. A new run also makes new random files, whose hashes do not match the files already on the iPhone.

**Fresh RX.** Run `RX="$(mktemp -d)"` before each receive whose result you check. The steps below say when.

**Ports.**
- The Mac app and the CLI both default to port 47321. Whichever starts second falls back to a free port.
- Read the CLI's port from its line `Listening on <addr> as "<name>"`.
- An explicit `--port` turns the fallback off. NET-4 uses one.

**Local Network on the Mac.** Exempting the CLI from macOS Local Network is Not tested on a real LAN. If the iPhone does not list the CLI, note it and use addresses.

### 8. Where things are on the iPhone
- **Received files.** Files › On My iPhone › Dropumi › Received. This is `Documents/Received` in the app container.
  - Settings › "Received files" ("In Files: On My iPhone › Dropumi › Received") opens that folder.
  - In Transfers › "Earlier", the folder button "Show in Files" opens the first file that a transfer saved.
- **How files are saved.**
  - All files land flat in Received.
  - Received is never emptied. A repeated name gets the next free number: `name (1).ext`, `name (2).ext`, and so on.
  - Control, bidi and invisible characters are removed from names.
- **Partial files.** They are hidden `.dropumi-<12 alphanumerics>.part` files. When receiving starts, a partial file is deleted, but only if it is older than 15 minutes.
- **Identity and paired devices.** They are in `Library/Application Support/Dropumi` in the container, which is excluded from backups. History is in `Library/Application Support/history.json`. There is no Keychain storage.
- **Receiving runs only while Dropumi is in front.** In the background it stops accepting, and running transfers may finish.
- **Address.** Settings › the device card › This device › ADDRESS shows one line `‹IPv4›:‹port›` for each interface that is up. The lines have no interface name, and cellular or VPN addresses can appear too.
  - Use the line that matches Settings › Wi-Fi › (i) › IP Address (iOS).
  - Or turn Cellular Data off for the session (iOS).
  - The default port is 47321.

### 9. Getting received files to the Mac for SHA-256
The repo documents only "copy the received files to a Mac if needed". Neither copy method below is confirmed in the repo.
- **Finder.** Select the iPhone › Files › Dropumi, then drag `Received` into a `mktemp -d` folder.
- **devicectl.** Check the flags with `xcrun devicectl device copy from --help`.
  - `info files` should also list the hidden `.part` files that Files and Finder hide. Use it for CAN and KILL.
  ```sh
  GOT="$(mktemp -d)"
  xcrun devicectl device info files --device <UDID> --domain-type appDataContainer \
    --domain-identifier io.github.dropumi.Dropumi
  xcrun devicectl device copy from --device <UDID> --domain-type appDataContainer \
    --domain-identifier io.github.dropumi.Dropumi --source Documents/Received --destination "$GOT"
  ```

**Hash the file the current test saved, not an older copy.** Earlier checks leave copies with the same name, and a matching older copy can give a false pass.
- **iPhone as receiver.** Before the test, note the names that are already in Received. The new file is the name that was not there before, for example `small (2).txt`. The history row's "Show in Files" button also opens the first file that the transfer saved.
- **CLI as receiver.** Use a fresh `$RX` and the names from the CLI's `Saved <name>` lines.
- **Mac app as receiver.** Use the new names in `~/Downloads/Dropumi`. `ls -lt ~/Downloads/Dropumi` lists the newest first.
- **Or empty Received in Files first** (iOS). Then receive the files that §10 needs again.

A plain `shasum -a 256 -c SHA256SUMS` reports misses, for three reasons:
- the receiver saves files flat, so the `./many/…` paths do not exist;
- the right-to-left override character is stripped from the file name;
- a repeated name becomes `name (N).ext`.

Compare by hash instead. These commands have not been run yet.
```sh
grep ' ./small.txt$' "$FILES/SHA256SUMS"
find "$GOT" -name '<saved name>' -exec shasum -a 256 {} +
diff <(grep ' ./many/' "$FILES/SHA256SUMS" | cut -d' ' -f1 | sort) \
     <(find "$GOT" -type f -name 'file-*.bin' -exec shasum -a 256 {} + | cut -d' ' -f1 | sort) && echo "300 hashes equal"
```
- Replace `<saved name>` with the name the current test saved.
- The `diff` expects exactly one set of `file-NNN.bin` in Received.

### 10. Getting test files onto the iPhone (for the send tests)
- **Receive first.** Run the receive half of TX-1, TX-6, TX-7 and TX-8 from the CLI. The test files then sit in Received.
- **Send from Received.** Use Send › "Add files" › On My iPhone › Dropumi › Received.
  - Picked files are read in place, not copied (`FileImport.swift`).
  - This is also how the **2 GiB file** gets onto the phone for TX-6, with no second copy.
- **Alternative** (not in the repo): Finder › the iPhone › Files › drag files onto Dropumi.
- **SHARE-1 and PHOTOS-1** need a small photo and a video of about 1 GB in Photos. Record the video with Camera and check its size in Photos (iOS).

### 11. Logs
- The app has no logging of its own. The core's `tracing` events go nowhere in the app.
- What you can use:
  - the in-app banners;
  - Transfers › "Earlier";
  - Console.app with the iPhone selected, filtered to the process `Dropumi` (not in the repo).
- Never paste lines that show names, addresses or fingerprints.
- Never share an `.xcresult` bundle.

### 12. Reset
- **Identity only.** Settings › the device card › This device › "Reset identity…" › "Reset this device's identity?" › "Reset identity".
  - Expect the banner "New identity" / "Devices you paired with must pair again."
  - From code reading, a reset keeps the settings, the history and the received files.
- **Everything.** None of this is in the repo.
  1. Copy Received off the phone first (§9).
  2. Long-press the icon › Remove App › Delete App (iOS). Or run `xcrun devicectl device uninstall app --device <UDID> io.github.dropumi.Dropumi` (check the syntax with `--help`).
  3. Reinstall (§4) and go through onboarding again. Note whether the Local Network prompt appears again.
- **Partners.** Remove the iPhone from them too:
  - Mac app: Settings › "Paired devices" › "…" › "Remove…" › "Remove".
  - Android: the same steps, on the Android phone.
  - CLI: open a new tab with the new-tab block (§7). This makes a new identity.
- **History only.** Transfers › "Clear". It asks for no confirmation.

### 13. Recording
- **Build row:** "Release; Rust core profile release (core-stamp.txt)", plus the output of `git rev-parse --short HEAD`.
- **Device:** "iPhone 15, iOS 27.0.1 (build …)".
- **Network row:** for example "home Wi-Fi, 5 GHz, client isolation: no; all devices on one network". Record the band (2.4 or 5 GHz) and whether client isolation is on. Never record the Wi-Fi name. The last run left band and isolation unrecorded.
- **Test files row:** "`bash scripts/device-tests/make-test-files.sh <dir> 2048`; compared by hash as in §9".
- **No screenshots** of:
  - This device, because it shows addresses;
  - any screen that shows names you set;
  - Connect by address;
  - pairing by address, which shows "Asking ‹address›…";
  - Transfers or history rows for transfers started by address. They can show the typed address: "Sending to ‹address›" during the handshake, or "Sent to ‹address›" if the connection never completed.
- Before any Transfers screenshot, clear history (Transfers › "Clear").
- **Labels:** Manual device test passed, Failed (say what happened), or Not tested.

## Checklist

**Partners:**
- **M** is the Mac app.
- **A** is the Android app. It only browses: it lists the iPhone, but the iPhone never lists it. To reach it, use Nearby › "…" › "Connect by address" with the address that the Android app shows under Settings › This device.
- **C** is the CLI on the Mac (§7). Each check names its tab.

**Defaults for every check:**
- The iPhone is unlocked and Dropumi is in front.
- "Paired devices only" is selected.
- Auto-accept is off.

**Reading history (Transfers › "Earlier").** Each row's title is the file name. The Mac app's Transfers screen uses the same code (`Dropumi/Shared/Screens/TransfersScreen.swift`).
- **Success row.** A green check icon and one line, "Sent to ‹alias› · ‹time›" or "Received from ‹alias› · ‹time›". The word "Done" is not shown on screen; VoiceOver reads the icon as "Done".
- **Any other outcome.** A second line names the outcome, for example "You declined". The icon is a grey ✕, or a red ! when only some files failed.

**Suggested order:**
1. DISC rows.
2. TX-4.
3. TX-3 (all pairs).
4. PAIR-2: first I↔M, then the C half in tab 2.
5. PAIR-1 (all pairs and both directions; I↔C in tab 1).
6. The rest.

Until PAIR-1, nothing is paired. If M and the iPhone are still paired from an earlier session, remove the pairing on both sides first: Settings › "Paired devices" › "…" › "Remove…" › "Remove".

**Receive commands** use `<iPhone address>:<port>` from §8.

### Discovery
- [ ] **DISC-1 (I↔M)** Each side sees the other. Steps: 1) Open Nearby on the iPhone and in the Mac app. **Expect:**
  - The iPhone lists the Mac's `Dropumi-XXXX` under "Tap a device to send".
  - The subtitle is "Not paired", or "Looks like ‹alias›" once paired. It is never "Paired".
  - The header reads "1 device found · works without internet".
  - The Mac lists the iPhone under "Send to".

  **Record:** "I lists M" and "M lists I" separately.
- [ ] **DISC-1 (I↔A)** Steps: 1) Open Nearby on both phones. **Expect:**
  - The Android app lists the iPhone.
  - The iPhone does **not** list the Android phone. That is expected today, because the Android app does not advertise (R21 gate closed).

  **Record:** Both directions. If the iPhone does list the Android phone, record it as a finding.
- [ ] **DISC-1 (I↔C)** Steps: 1) In tab 1, run `"$DROP" receive --dir "$RX"`. 2) Open Nearby on the iPhone. 3) In tab 2, run `"$DROP" discover --timeout-ms 5000`. **Expect:**
  - The iPhone lists the CLI's name.
  - `discover` prints one line `<name>  fp <8-char hint>  <ip>:<port>` for the iPhone's name.

  **Record:** Yes or no for each direction. Do not paste the `discover` output, because it contains addresses and hints.
- [ ] **DISC-2** A device never lists itself. Steps: 1) With at least one other device listed, compare the iPhone's Nearby list with its own name in Settings. **Expect:** The iPhone's own `Dropumi-XXXX` never appears in its Nearby list. **Record:** Result.
- [ ] **DISC-3** Off: the device is not listed and connections are refused. Steps: 1) Note `<iPhone address>:<port>` from This device. 2) Settings › "Who can send to this device" › "Off". 3) Check the Mac's Nearby, the Android app's Nearby, and `"$DROP" discover --timeout-ms 5000` in tab 2. 4) In tab 2, run `"$DROP" send "$FILES/small.txt" --to <iPhone address>:<port> --allow-unverified`. 5) On Nearby, tap "Turn on for paired devices". **Expect:**
  - The pill reads "Not receiving".
  - Nearby shows "Not receiving" / "Other devices can't find or send to this one. You can still send." with "Turn on for paired devices".
  - This device › ADDRESS reads "Not receiving right now."
  - No observer lists the iPhone.
  - `send` fails with exit 5 (connection refused; check the exact error).
  - After step 5, the pill reads "Paired devices only".

  **Record:** The exit code (`echo $?`) and which observers you checked.
- [ ] **DISC-4** The advertised host name is `dropumi-…`. Steps: 1) Quit the Mac app and stop the CLI, or tell the instances apart by their TXT `n`. 2) With Dropumi in front on the iPhone, run `dns-sd -B _dropumi._tcp` on the Mac, then `dns-sd -L <instance> _dropumi._tcp local.`. Stop each with Ctrl-C. **Expect:**
  - The instance is `dropumi-<8 hex>`, and the SRV host is `dropumi-<8 hex>.local.` with the same label.
  - The only TXT keys are `v`, `n` and `fp`.
  - The iPhone's own (iOS) name appears nowhere.
  - Optional: send Dropumi to the background and bring it back. The label should change, because each advertisement gets a new one.

  **Record:** Yes or no for each point. Do not paste the output.

### Pairing
- [ ] **PAIR-2** "Codes don't match" stores nothing. Run it before PAIR-1, while nothing is paired. Steps: 1) I↔M: the iPhone waits ("Pair a device" › "Wait for a device"). On the Mac, choose "Pair a device" › the iPhone's row › "Pair". Tap "Codes don't match" on the iPhone. 2) I↔C, in tab 2: stop any `receive` in tab 1 (Ctrl-C), so that the iPhone lists only one CLI. Run `"$DROP" pair --listen`. On the iPhone, choose "Pair a device" › the CLI's row › "Pair", and answer `n` at the CLI. 3) In tab 2, run `"$DROP" trust list`. **Expect:**
  - Both apps show "Pairing declined" / "The other device did not confirm the code. Nothing was saved."
    - The iPhone shows this too, even though it declined.
    - Code reading marks that wording as a known issue.
  - The Mac's open code prompt goes away.
  - Each "Paired devices" list stays empty.
  - The CLI exits 6 (`pairing_rejected`).
  - `trust list` prints `No trusted devices.` (tab 2 has never paired).

  **Record:** Both pairs.
- [ ] **PAIR-1 (I↔M)** Codes match and both list each other. Steps: 1) On the iPhone, Settings › "Pair a device" › "Wait for a device". 2) On the Mac, Settings › "Pair a device" › the iPhone's row › "Pair". 3) Compare the codes, then tap "Codes match" on both. **Expect:**
  - The iPhone shows "Waiting for a device · 5:00", counting down. Below it: "On the other device, choose Pair a device, then this one:" with its name.
  - Both show "Pair with this device?" with a fingerprint and the same code, in the form `123 456`.
  - Both then show "Paired" / "‹alias› can now send to this device, and you can send to it without a code." with "Done".
  - On both sides, Settings › "Paired devices" lists the other device as "Paired with a code · XXXX-…".

  **Record:** Which side waited.
- [ ] **PAIR-1 (I↔M, the Mac waits)** The other direction. Run it after PAIR-1 (I↔M). Steps: 1) Remove the pairing on both sides: Settings › "Paired devices" › "…" › "Remove…" › "Remove". 2) On the Mac, Settings › "Pair a device" › "Wait for a device". 3) On the iPhone, Settings › "Pair a device" › the Mac's row › "Pair". 4) Compare the codes, then tap "Codes match" on both. **Expect:**
  - The Mac shows the waiting screen. The iPhone shows "Asking ‹name›…".
  - Both show "Pair with this device?" with the same code, then "Paired".
  - On both sides, "Paired devices" lists the other device as "Paired with a code · XXXX-…".
  - The two devices end up paired again, which SPOOF-1 needs.

  **Record:** "The Mac waited", and the result.
- [ ] **PAIR-1 (I↔A)** Steps: 1) The iPhone waits, as above. 2) On the Android phone, start pairing from the iPhone's row. Alternative: the Android phone waits, and the iPhone uses Nearby › "…" › "Connect by address" › the address on Android's Pairing screen › "Pair with this address". 3) Codes match on both. **Expect:** The same as I↔M. **Record:** Which side waited.
- [ ] **PAIR-1 (I↔C)** Run it in tab 1. This is the pairing that the later tab-1 checks need. Steps: 1) Run `"$DROP" pair --listen`. It opens a 5-minute window and takes one connection. 2) On the iPhone, "Pair a device" › the CLI's row › "Pair". The iPhone shows "Asking ‹name›…". 3) Compare the iPhone's "Pair with this device?" code with the CLI's `Does the other device show NNN NNN? [y/N]`. Tap "Codes match" and type `y`. **Expect:**
  - The iPhone shows "Paired".
  - The CLI prints `Paired: Trusted "<alias>" (claims "<name>") · XXXX-…` and exits 0.
  - `"$DROP" trust list` shows one line ending in `(sas)`.

  **Record:** The exit code. Do not paste fingerprints.
- [ ] **PAIR-1 (I↔C, the iPhone waits)** The other direction. Run it in tab 1, after PAIR-1 (I↔C). Steps:
  1) Run `"$DROP" --json trust list`. Its `fingerprint` field is the iPhone's full 52-character fingerprint (`<FP52>`). `trust remove` refuses a shorter prefix. Never record it.
  2) Run `"$DROP" trust remove <FP52>`.
  3) On the iPhone, remove the CLI: Settings › "Paired devices" › "…" › "Remove…" › "Remove".
  4) On the iPhone, Settings › "Pair a device" › "Wait for a device".
  5) In tab 1, run `"$DROP" pair --to "<iPhone's Dropumi-XXXX name>"`. If the name does not resolve, use `<iPhone address>:<port>` (§8). From code reading, the wait screen advertises on the port that receiving used.
  6) Compare the codes. Tap "Codes match" on the iPhone and type `y` at the CLI's `Does the other device show NNN NNN? [y/N]`.

  **Expect:**
  - After step 3, neither side lists the other as paired.
  - The iPhone shows "Paired".
  - The CLI prints `Paired: Trusted "<alias>" (claims "<name>") · XXXX-…` and exits 0.
  - `"$DROP" trust list` again shows one line ending in `(sas)`. Tab 1 is paired again for the later checks.

  **Record:** "The iPhone waited", whether you used the name or the address, and the exit code. Do not paste fingerprints.

### Transfers
- [ ] **TX-1 (C↔I)** Paired transfer with a prompt; SHA-256 equal. Run it in tab 1, after PAIR-1 (I↔C). Steps:
  1) Note the names in Received (§9). Run `"$DROP" send "$FILES/small.txt" "$FILES/photo-like.bin" --to <iPhone address>:<port>`.
  2) The iPhone shows "Incoming files" with the alias, "Paired", no code and no fingerprint. Then it shows "wants to send you 2 files", both names with their sizes, "Total", and "Expires in 1:59" counting down. Tap "Accept".
  3) Copy Received to the Mac (§9). Hash the two names that are new.
  4) For the other direction, run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"`. On the iPhone, open Nearby and tap "Send" on the CLI row ("Looks like ‹alias›"). Choose "Add files", pick both files from Received, then tap "Send to ‹name›".
  5) At the CLI's `[y/N]`, type `y`.

  **Expect:**
  - Accept is disabled for a moment, then enabled.
  - The CLI prints `Done: 2 file(s), … in X s` and exits 0.
  - The iPhone's history shows a success row, "Received from ‹alias› · ‹time›".
  - The send hint reads "Dropumi checks that this is your paired device before sending."
  - The iPhone first shows "Waiting for the other person to accept…", then a success row, "Sent to ‹alias› · ‹time›".
  - The CLI prints `Saved small.txt` and `Saved photo-like.bin`.
  - All hashes equal the lines in `SHA256SUMS`.

  **Record:** Each direction separately, with the hash result and the saved names.
- [ ] **TX-1 (M↔I)** Steps: 1) Note the names in Received. On the Mac, Nearby › the iPhone's row › send `small.txt` and `photo-like.bin` from `$FILES`. Accept on the iPhone, then hash the new names (§9). 2) Note the names in `~/Downloads/Dropumi`. The iPhone sends the same files from Received to the Mac's row. Accept on the Mac's sheet. **Expect:**
  - The iPhone prompt is the same as in TX-1 (C↔I).
  - Both histories show success rows.
  - `shasum -a 256 ~/Downloads/Dropumi/<saved name>`, run for each new name, matches `SHA256SUMS`.

  **Record:** Each direction, with the hash result.
- [ ] **TX-1 (A↔I)** Steps: 1) The Android app sends both files to the iPhone. Accept, then hash the new names (§9). 2) On the iPhone, Nearby › "…" › "Connect by address" › the Android address. Optionally type at least 24 fingerprint characters; the hint then reads "Dropumi checks this fingerprint before anything is sent.". Then "Choose files" › "Send files". **Expect:**
  - The iPhone shows no "Check the code" prompt, because the Android phone is paired (code reading of `send.rs`). Note it if one appears.
  - The iPhone's history shows a success row, "Sent to ‹alias› · ‹time›".
  - Check the hashes on the Android side (Android section, EXP-1).

  **Record:** Each direction.
- [ ] **TX-2** Auto-accept on: no prompt; off: prompt. Run it in tab 1. Steps: 1) On the iPhone, turn Settings › "Auto-accept paired devices" on. 2) Run `"$DROP" send "$FILES/small.txt" --to <iPhone address>:<port>`. 3) Turn the toggle off and send again. **Expect:**
  - With the toggle on, no prompt appears. The transfer shows in Transfers, then as a success row, and the CLI exits 0.
  - With the toggle off, "Incoming files" appears.
  - Received already holds `small.txt` from earlier checks, so each send saves the next free name, `small (N).txt`. Earlier copies stay unchanged.

  **Record:** Both halves, and the N of each saved copy.
- [ ] **TX-3 (C↔I)** Everyone, with an unverified sender: the same code, and no pairing. Run it in tab 2. Steps:
  1) On the iPhone, choose Settings › "Everyone · 10 min".
  2) Run `"$DROP" send "$FILES/small.txt" --to <iPhone address>:<port> --allow-unverified`. The CLI prints `Verification code: NNN NNN  — check that the other device shows the same code.` and asks `Does the other device show NNN NNN? [y/N]`. The iPhone shows its code only after you answer here (code order in `send.rs`), so type `y`.
  3) On the iPhone, compare the code with the CLI's, then tap "Accept".
  4) For the other direction, run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX" --policy everyone`. On the iPhone, send `small.txt` to the CLI's row ("Not paired"). Tap "Codes match", then answer `y` at the CLI.

  **Expect:**
  - The Settings caption reads "Anyone nearby can ask to send for m:ss. You compare a code before accepting. Only while Dropumi is open." The pill reads "Everyone · m:ss".
  - The iPhone prompt shows "Unverified device", `claims “‹CLI name›”`, "Not paired", a 24-character fingerprint, and "Compare this code with the sender's screen" with the same code.
  - The CLI exits 0.
  - In the other direction:
    - The send hint reads "This device isn't paired. You'll both see a code to compare before anything is sent."
    - The iPhone shows "Check the code" with "This device isn't paired. Check that its screen shows the same code before anything is sent."
    - Its code equals the CLI's code.
  - Afterwards, "Paired devices" is unchanged, and `"$DROP" trust list` prints `No trusted devices.`
  - If the iPhone stays in front, the banner "Back to paired devices only" / "The 10 minutes for everyone ended." appears after 10 minutes.

  **Record:**
  - Both directions.
  - What the pill and the caption show. Code reading predicts "Everyone · 0:00" because of a seconds/milliseconds mix-up (`Common.swift:203-206`).
- [ ] **TX-3 (M↔I)** M and the iPhone must not be paired. In the suggested order they are not paired yet. If they are, remove the pairing on both sides ("Paired devices" › "…" › "Remove…"). Then pair them again with PAIR-1 (I↔M) before TX-1 (M↔I) and SPOOF-1. Steps: 1) Set the iPhone to Everyone; the Mac sends. Compare the codes and accept. 2) Set the Mac to Everyone; the iPhone sends. Tap "Codes match", then compare on the Mac's sheet and accept. **Expect:** The same code on both screens, the same strings as in TX-3 (C↔I), and no new paired device on either side. **Record:** Each direction.
- [ ] **TX-3 (A↔I)** Steps: 1) Set the iPhone to Everyone; the Android app sends to it. 2) Set the Android app to Everyone. The iPhone uses "Connect by address" with no fingerprint ("Without a fingerprint, you compare a code with the other device first.") › "Send files" › "Codes match". **Expect:** The same code on both phones, and no pairing created. **Record:** Each direction.
- [ ] **TX-4** Paired only: an unpaired sender is refused. Run it in tab 2. Steps: 0) Dismiss any banner on the iPhone with ×. If a device was refused in the last 60 s, wait 60 s. 1) Check that the iPhone is on "Paired devices only". 2) Run `"$DROP" send "$FILES/small.txt" --to <iPhone address>:<port> --allow-unverified`. 3) Optional: the iPhone sends to an unpaired Mac that is on Paired only. **Expect:**
  - The iPhone shows no prompt.
  - It shows the banner "A device was refused" / "A device on this network tried to connect and was turned away. To pair with it, choose Wait for a device under Pair a device first."
    - Banners never close by themselves.
    - The app skips this banner while another banner shows, and within 60 s of the last refusal notice.
    - "No banner" is a failure only if step 0 was done.
  - The CLI exits 6. `not_authorized` is expected; check the exact code.
  - In the optional step, the iPhone's history shows the second line "Only paired devices".

  **Record:** The exit code and the banner text.
- [ ] **TX-5** Decline. Run it in tab 1. Steps: 1) Send `small.txt` from tab 1, and tap "Decline" on the iPhone. 2) Run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"`. The iPhone sends to it; answer `n`. 3) Run `ls -A "$RX"`. **Expect:**
  - The CLI exits 3 (rejected).
  - The iPhone's history shows the second line "You declined", and nothing is saved.
  - In the other direction, the iPhone's history shows "Declined", and `ls -A "$RX"` prints nothing.

  **Record:** Both directions.
- [ ] **PROMPT-2 (extra)** A prompt that runs out. Source: `docs/adr/0012-apple-apps.md` (consent rules) and `PromptPresenter.swift`. Run it in tab 1. Steps: 1) With auto-accept off, run `"$DROP" send "$FILES/small.txt" --to <iPhone address>:<port>`. 2) Do not answer on the iPhone. Watch the prompt for more than 2 minutes. 3) Run `echo $?` when the CLI ends. **Expect:**
  - "Expires in …" counts down to "Expired".
  - At zero, Accept is disabled. Decline stays enabled.
  - The app never answers by itself. The prompt goes away only when the core closes it.
  - From code reading, the iPhone's history then shows the second line "Request expired".

  **Record:** How long after "Expired" the prompt closed, the history line, and the CLI's exit code.
- [ ] **TX-6** A file of at least 2 GiB. Run it in tab 1. Steps:
  1) Check the free space (Settings › General › iPhone Storage). It must be more than 2 GiB, or the transfer ends with "Not enough space".
  2) Run `"$DROP" send "$FILES/large-2048MiB.bin" --to <iPhone address>:<port>` and tap "Accept".
  3) Copy the new file to the Mac (§9) and hash it.
  4) Run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"`. Send `large-2048MiB.bin` from Received to the CLI (or to M).

  **Expect:**
  - The Transfers card shows "‹done› of ‹total›" and "NN% · NN MB/s · … left".
  - The CLI prints `Done: 1 file(s), … in X s` and exits 0.
  - Both histories show success rows.
  - The hashes equal the `large-2048MiB.bin` line in `SHA256SUMS`.

  **Record:**
  - The time, from the CLI's `Done … in X s` line or from the start and end clock times in Transfers.
  - The MB/s shown.
  - The method (Nearby or address) and the band (2.4 or 5 GHz).
  - The direction.
- [ ] **TX-7** 300 small files. Run it in tab 1. Steps:
  1) Run `"$DROP" send "$FILES"/many/* --to <iPhone address>:<port>`. Passing `many/` as a directory is refused with exit 2.
  2) Tap "Accept".
  3) Compare with the `diff` in §9.
  4) Run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"`. Send the 300 files from Received to it.
     - It is not confirmed in code whether the Files picker can select all 300 at once. Note how you selected them.

  **Expect:**
  - The prompt says "wants to send you 300 files". It lists 200 rows, then "and 100 more", then "Total".
  - 300 `file-NNN.bin` files arrive in Received, and "300 hashes equal" prints.
  - After the receive (steps 1-2), the correct history title is "file-001.bin and 299 more". "file-001.bin and 199 more" is the capped-names defect: record Failed. Code reading predicts the defect. The app keeps only the prompt's 200 names for the history (`DropumiKit/Sources/DropumiAppState/AppModel.swift:649-652,659-662`), and the history title is the first name plus the count of the other names (`Dropumi/Shared/Screens/TransfersScreen.swift:203-206`).
  - After the send (step 4), code reading keeps all 300 picked names (`AppModel.swift:494`), so the title should end in "and 299 more". Which name comes first is not confirmed in code: note it.
  - The CLI lists 20 names, then `  … and 280 more`.

  **Record:** The time, the hash result, both directions, and the history title of each direction.
- [ ] **TX-8** Unicode, right-to-left, duplicate and 0-byte names; nothing overwritten. Run it in tab 1. Steps: 1) Run `"$DROP" send "$FILES"/empty.bin "$FILES"/R*.txt "$FILES"/invoice* --to <iPhone address>:<port>` twice, accepting both times. 2) Copy Received to the Mac and hash it. 3) Optional: run `"$DROP" send "$FILES"/invoice* --to <iPhone address>:<port>` once more and accept, so that the Transfers card and the history title show that name. **Expect** (if Received held none of these names before):
  - The first send saves `empty.bin` (0 bytes), `Résumé – naïve 日本語.txt` and `invoicetxt.exe`. The right-to-left override is removed.
  - The second send saves `empty (1).bin`, `Résumé – naïve 日本語 (1).txt` and `invoicetxt (1).exe`.
  - The first copies are unchanged, and their hashes match `SHA256SUMS`.
  - The "Incoming files" list, the Transfers card and the history show "invoicetxt.exe" with no reordering. The core sanitizes offered names before the prompt (`crates/ffi/src/prompts.rs:202`; test `prompt_ids_never_reused_and_names_are_sanitized`). Any visual reversal is Failed.
    - In steps 1 and 2, the card and the history title show only the first name ("empty.bin and 2 more"). Step 3 shows "invoicetxt.exe" there and saves `invoicetxt (2).exe`.

  **Record:** The saved names, typed by hand, and the hash result.

### Cancelling
- [ ] **CAN-1 (C↔I)** The sender cancels; no partial file is left. Run it in tab 1. Steps: 1) The CLI sends `large-2048MiB.bin`, and the iPhone accepts. At about 30%, press Ctrl-C in the CLI. 2) List the container (`devicectl device info files`, §9). 3) Run `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"`. The iPhone sends the large file to it. At about 30%, tap × on the iPhone's Transfers card; there is no confirmation. 4) Run `ls -la "$RX"`. **Expect:**
  - The CLI exits 130.
  - The iPhone's history shows "Cancelled by the other device". Not confirmed: Ctrl-C may show as "Connection lost". Note what you see.
  - No `.dropumi-*.part` file and no partial `large-2048MiB.bin` remain in Received.
  - In the other direction, the iPhone's history shows "Cancelled by you", and `$RX` holds no `.part` file and no partial file.

  **Record:** Both directions.
- [ ] **CAN-1 (M↔I)** and **CAN-1 (A↔I)** Steps: The same as CAN-1 (C↔I), cancelling with × on the sending app. **Expect:**
  - The receiver shows "Cancelled by the other device".
  - No `.part` file is left in Received, in `ls -A ~/Downloads/Dropumi` on the Mac, or on the Android side.

  **Record:** Each pair and direction.
- [ ] **CAN-2** The receiver cancels; no partial file is left. Steps: 1) Tab 1 sends the large file, and the iPhone accepts. Tap × on the iPhone. 2) List the container. **Expect:** The iPhone's history shows "Cancelled by you", the CLI exits 4, and Received holds no `.part` file. **Record:** Result. Optionally, the iPhone sends to the Mac and the Mac cancels. The iPhone should then show "Cancelled by the other device".

### Network
- [ ] **NET-1** The sender's Wi-Fi goes off. Steps: 1) The iPhone sends `large-2048MiB.bin` to M (or A). 2) At about 30%, turn Wi-Fi off from Control Center. Do not open Settings, because that sends Dropumi to the background. **Expect:**
  - Both sides end with an error, never a success row. "Connection lost" / "The connection dropped. Check that both devices are on the same network." is likely.
  - No partial file is left on the receiver.
  - Not confirmed in code: how long each side takes. While streaming, the idle timeout is 30 s and the write-stall timeout is 120 s.

  **Record:** Each side's message and how long it took.
- [ ] **NET-2** The receiver's Wi-Fi goes off. Steps: 1) M (or A) sends the large file to the iPhone. 2) At about 30%, turn the iPhone's Wi-Fi off from Control Center. 3) Turn it back on and list the container. **Expect:** Both sides show an error, never a success row, and Received holds no `.part` file. **Record:** The messages and the times.
- [ ] **NET-3** The access point reboots. Steps: 1) Start sending the large file from the iPhone to M. 2) Reboot the access point. 3) When it is back, check Nearby and send `small.txt` again. **Expect:** The first transfer fails on both sides, with no success row. After recovery, Nearby lists the devices again, and the new transfer shows a success row. **Record:** The messages and the time to recover.
- [ ] **NET-4** Client isolation: Connect by address works. This needs a network where Nearby lists nothing; without one, record Not tested. Run it in tab 1, which PAIR-1 (I↔C) paired with the iPhone. Steps:
  1) Quit the Mac app: its menu bar › "Quit Dropumi completely". An explicit `--port` turns off the busy-port fallback, so with the Mac app on 47321 the CLI's bind can fail (exit 5). Alternative: keep the Mac app running, use `--port 0` in step 3, and use the port from the `Listening on` line in step 4.
  2) Run `"$DROP" send "$FILES/small.txt" --to <iPhone address>:<port>`. Accept on the iPhone.
  3) Run `RX="$(mktemp -d)"`, then `"$DROP" receive --dir "$RX" --bind "$(ipconfig getifaddr en0)" --port 47321 --no-mdns`. It prints `Listening on <addr> as "<name>"`.
  4) On the iPhone, Nearby › "…" › "Connect by address" › `<Mac's Wi-Fi address>:47321`. Optionally type at least 24 characters from the CLI's `Fingerprint:` line. Then "Choose files" › "Send files". Answer `y` at the CLI.

  **Expect:**
  - Both transfers succeed.
    - The CLI `send` exits 0, and the CLI `receive` prints `Saved <name>`.
    - The iPhone shows success rows.
    - The hashes match (§9).
  - The devices are paired, so no code check appears.
  - **Unpaired variant** (optional, in tab 2):
    - Add `--allow-unverified` to step 2, with the iPhone on Everyone. Add `--policy everyone` to step 3.
    - Only then does the iPhone show "Check the code" for its send. Connect by address always allows an unverified receiver.
    - Without these flags, the CLI's `send` is refused (`unverified_peer`, exit 6). The CLI's `receive` refuses the iPhone, which then shows "Only paired devices".

  **Record:** The network type (no names) and both directions.

### Background
- [ ] **BG-1 (send)** iOS: Home Screen while the iPhone sends. Steps: 1) The iPhone sends `large-2048MiB.bin` to M, or to tab 1 after `RX="$(mktemp -d)"; "$DROP" receive --dir "$RX"`. While it runs, Transfers shows "Keep Dropumi open until transfers finish. If you leave, iOS may stop them." 2) At about 10%, swipe up to the Home Screen and wait about 1 minute. 3) Return to Dropumi. **Expect** (from code only: spike S-A3, never run on a device):
  - Dropumi requests a background task titled "Dropumi transfer" / "Finishing files with a nearby device" as soon as a transfer starts.
  - Not confirmed in code: what iOS shows for that task. Note it.
  - One of three outcomes:
    - a) The transfer completes, with success rows on both sides.
    - b) iOS ends the task. The app cancels cleanly, and the receiver shows "Cancelled by the other device", not a timeout.
    - c) iOS refuses the task or never grants it. The code then does not ask again (`BackgroundTransfers.swift`). Dropumi is suspended and the transfer stalls. The receiver ends with a timeout or a lost connection, not "Cancelled by the other device": on M, a second line such as "No answer" or "Connection lost"; on C, an error line (note it). How long this takes is not confirmed in code. The idle timeout is 30 s, and the write-stall timeout is 120 s.

  **Record:** Which outcome, how long it took, whether any system UI for the task appeared (no UI may mean that iOS refused the request), and any banner shown on return.
- [ ] **BG-1 (receive)** iOS: Home Screen while the iPhone receives. Steps: 1) In tab 1, run `"$DROP" send "$FILES/large-2048MiB.bin" --to <iPhone address>:<port>`, or have M send the file. Accept on the iPhone. 2) At about 10%, go to the Home Screen and wait about 1 minute. 3) Return to Dropumi. 4) After the transfer has ended, check the pill, and run `"$DROP" discover --timeout-ms 5000` in tab 2. **Expect** (from code reading only):
  - The background task is requested for incoming transfers too. The app requests it whenever any transfer runs.
  - The outcome is a), b) or c), as in BG-1 (send). Note what the sender reports, and the CLI's exit code.
  - If the transfer is still running when you return, code reading predicts the banner "Receiving could not start" / "not receiving". The app tries to change the policy of a receiver that is stopping.
  - After the transfer ends, receiving should start again by itself. The pill then reads "Paired devices only", and `discover` lists the iPhone.

  **Record:** The outcome, any banner text, and whether receiving came back without tapping anything.
- [ ] **BG-2** iOS idle in the background is not listed. Steps: 1) With Dropumi in front, confirm that M, A and `"$DROP" discover --timeout-ms 5000` (tab 2) list the iPhone. 2) Go to the Home Screen and check again. 3) Bring Dropumi back. **Expect:** While Dropumi is in the background, no observer lists the iPhone. When it is back in front, it is listed again. **Record:**
  - Each observer.
  - Extra check, from code reading: with Everyone on, returning from the background starts a fresh 10-minute window. Note the countdown.
  - Also note that no "Back to paired devices only" banner appears for a window that ran out in the background.

### Share and Open in
- [ ] **SHARE-1** Photos › Dropumi › open Dropumi › send. Steps:
  1) In Photos, select a small photo and the video of about 1 GB, then Share › "Dropumi".
  2) Tap "Done".
  3) Open Dropumi.
  4) Choose the Mac, then tap "Send to ‹name›". Accept on the Mac.

  **Expect:**
  - The extension shows "Preparing X of N…", then "Ready in Dropumi" and "2 items will be waiting. Open Dropumi to choose a device.".
  - Dropumi opens Send with "2 files · ‹size› ready to go".
  - Both histories show success rows, and the files are in `~/Downloads/Dropumi`.
  - On failure, the extension shows a message such as "Some items could not be copied, so nothing was handed to Dropumi. Try again, or share fewer items."

  **Record:** The video size, the time, and the received file names and sizes. How Photos names the exported files is not confirmed.
- [ ] **PHOTOS-1 (extra)** Send › "Add photos" (Photos import). Source: `docs/testing.md` lists Photos import as Not tested; `SendScreen.swift`. Steps: 1) Nearby › tap "Send" on M's row. 2) Tap "Add photos", then pick the small photo and the video of about 1 GB in the picker (iOS). 3) Tap "Send to ‹name›", and accept on the Mac. **Expect:**
  - "Preparing 2 items…" while the photos are copied.
  - Then the header "2 files · ‹size› ready to go".
  - Both histories show success rows.
  - If a photo cannot be added, the banner "A photo could not be added" / "Try again, or add it from Files." appears.

  **Record:** The names and sizes received on M, the time, and any banner. How photos are named is not confirmed in code (check and note what you see).
- [ ] **SHARE-2** Files: Share › Dropumi, or open in Dropumi. Steps: 1) In Files, long-press `small.txt` › Share › "Dropumi" › "Done". Open Dropumi and send to M. 2) Repeat with the share sheet's option that opens the file in the Dropumi app. **Expect:**
  - Each path opens Send with the file.
  - The transfer shows a success row.
  - The hash matches `SHA256SUMS`.
  - Not confirmed in code: which Dropumi entries the share sheet lists. Note them.

  **Record:** Both paths.

### Permissions, spoofing, force quit
- [ ] **PERM-1 (iOS half)** Local Network denied. Steps:
  1) Settings › Privacy & Security › Local Network › turn Dropumi off (iOS). Check that Wi-Fi has an address (Settings › Wi-Fi › (i), iOS).
  2) Open Dropumi › Nearby.
  3) Tap "Open Settings".
  4) Turn Local Network on and return to Dropumi.
  5) If nothing is listed within about 10 s, switch to Transfers and back to Nearby.

  **Expect:**
  - Nearby shows "Local network access is off" / "Dropumi needs it to find and reach nearby devices. Turn on Local Network for Dropumi in Settings." with "Open Settings".
  - When the card appears:
    - The app shows it for any discovery error whose reason mentions "local network". That covers a Bonjour denial, and also an advertisement that found no IPv4 address. So the card can appear without a denial, for example when Wi-Fi has no address.
    - Otherwise, note what shows instead: "Nearby devices can't be found" with a detail, or only "Looking for devices…".
  - "Open Settings" opens iOS Settings at Dropumi's page.
  - After the permission is back on, devices appear again, and the card clears when the first one is found.
    - From code reading, a failed browse is not restarted while Nearby stays open.
    - Leaving Nearby and opening it again restarts the browse, so step 5 may be needed.

  **Record:** The exact card shown, whether Wi-Fi had an address, whether step 5 was needed, and whether iOS relaunched Dropumi when the permission changed. How a relaunch looks is not confirmed in code (check and note what you see).
- [ ] **SPOOF-1** A spoofed name never shows as the paired device. This needs the iPhone paired with M (PAIR-1). Steps:
  1) Open tab 3 with the new-tab block (§7). Run `"$DROP" identity set-name "<the Mac app's Dropumi-XXXX name>"`, then `"$DROP" receive --dir "$RX" --policy everyone`.
  2) Open Nearby on the iPhone.
  3) Set the iPhone to Everyone.
  4) In tab 3, stop `receive`, then run `"$DROP" send "$FILES/small.txt" --to <iPhone address>:<port> --allow-unverified`. Answer `y`.
  5) Decline on the iPhone.

  **Expect:**
  - Nearby shows the real Mac as "Looks like ‹alias›" and the CLI as "Not paired". Neither row ever says "Paired".
  - The prompt shows "Unverified device", `claims “‹name›”`, "Not paired", the fingerprint and a code. It never shows the alias as the title, and never "Paired".

  **Record:** Result.
- [ ] **KILL-1** Force-quit the receiver during a transfer. Steps:
  1) Tab 1 (or M) sends `large-2048MiB.bin`, and the iPhone accepts.
  2) At about 30%, open the app switcher and swipe Dropumi away.
  3) List the container (§9).
  4) Wait **more than 15 minutes**, then open Dropumi. It must not be set to Off.
  5) List the container again.

  **Expect:**
  - The sender reports a lost connection. The CLI exits 5, probably with `connection_lost`; note the exact code.
  - After the kill, a `.dropumi-<12 alphanumerics>.part` file may be in Received. After the restart it is gone, and no partial file remains.
  - A restart within 15 minutes leaves the `.part` file in place by design (`crates/infra/src/fs/cleanup.rs`). That is not a failure.

  **Record:** The sender's message and exit code, and whether a `.part` file was there after the kill and after the restart.

### Accessibility and layout
- [ ] **UI-1 (iPhone)** VoiceOver, the largest text size, keyboard only. Steps:
  1) Turn VoiceOver on (iOS: Settings › Accessibility › VoiceOver). Walk through Nearby, Send, Transfers and Settings › "Paired devices", and receive one offer.
  2) Set the largest text size (iOS: Settings › Accessibility › Display & Text Size › Larger Text, slider at the maximum). Repeat step 1.
  3) With a hardware keyboard and Full Keyboard Access (iOS), answer a prompt. Without a keyboard, mark this part Not tested.
  4) Optional: if you reinstall (§12), go through onboarding with VoiceOver on.

  **Expect:**
  - VoiceOver reads:
    - "More" for the "…" menu on Nearby;
    - "Actions" for the "…" menu on each "Paired devices" row;
    - "Send to ‹name›" for each row's button;
    - "Remove ‹name›" for a file;
    - "Cancel" for a transfer;
    - "Dismiss" for a banner.
  - It reads the code as "Verification code: 1 2 3, 4 5 6" (the digits as shown).
  - The radar is not read.
  - In step 4, the onboarding dots read "Step N of 4". Without step 4, record this part as Not tested.
  - At the largest size, the prompt content scrolls with no clipping, and its two buttons stack at the bottom.
  - With a keyboard, Escape means "Decline" or "Codes don't match", and Return does nothing.

  **Record:** Each screen that clipped or read wrongly.
- [ ] **UI-2** iPad: layout, drag from Files. **Not in scope:** the maintainer's set has no iPad. **Record:** Not tested ("Not in scope (no iPad)"), unless an iPad is available.
- [ ] **PROMPT-1 (extra)** A press held from before the button arms must not answer. Sources: commit f642ef7 and `apps/apple/Tests/DropumiUITests/UIStateTests.swift:150` (fake core only). Steps:
  1) With auto-accept off, send `small.txt` from tab 1.
  2) The moment "Incoming files" appears, press "Accept" within its first 600 ms. Hold for about 2 s, then release.
  3) Then tap "Accept" normally.
  4) Repeat with "Codes match" during PAIR-1.

  **Expect:** The held press does not answer, and the prompt stays. A normal tap after the button arms does answer. **Record:**
  - How many tries landed inside the 600 ms window. This is hard to hit by hand, and Release builds have no longer arming delay.
  - The result.
  - Not confirmed in code: what happens to a touch that started before the prompt appeared. Note what you see.

### Release build
- [ ] **REL-1 (iOS, extra, optional)** The installed Release build has no fake core and no UI-test scenarios. Sources: threat model G36 ("a Release build contains no `FakeCoreClient` or `UITestScenario` symbol (`nm`, PR evidence)") and `apps/apple/Config/Release.xcconfig`. Run it on the Mac, in the APPLE worktree root, after the §4 build. Steps:
  ```sh
  nm apps/apple/build/Build/Products/Release-iphoneos/Dropumi.app/Dropumi | wc -l
  nm apps/apple/build/Build/Products/Release-iphoneos/Dropumi.app/Dropumi | grep -c -e FakeCoreClient -e UITestScenario
  ```
  **Expect:**
  - The first command prints a number above 0, so the binary has a symbol table. If it prints 0, the second check proves nothing: record Not tested and note it.
  - The second command prints `0`. `grep -c` exits 1 when it counts nothing; that is expected.

  **Record:** Both numbers, and the commit from `git rev-parse --short HEAD`.

### Rows this section does not run
These TEMPLATE rows are owned by other sections or are out of scope. Do not record them under the iPhone.

| Rows | The iPhone's part | Where to record |
|---|---|---|
| BG-3, UI-4 | Partner, if the Mac section uses the iPhone: it sends to the Mac, which receives. | Mac section |
| SHARE-4 | Partner, if the Mac section uses the iPhone: it receives the files that the Mac sends after they are dropped on its window. | Mac section |
| PERM-1 (Mac half), MAC-UI, MAC-BONJOUR | None (Mac only; MAC-UI and MAC-BONJOUR are automated). | Mac section |
| NET-5, BG-4, BG-5, SHARE-3, PERM-2, UI-3, BAK-1, OVL-1, NSD-1, EXP-1, INST-1 | None (Android only). EXP-1 checks the files that the iPhone sent in TX-1 (A↔I). | Android section. If no Android phone is tested in the session, record Not tested with the note "Not in scope (Apple run)", as run 2026-10-18-01 did. |
| UI-2 | None (iPad only). | Not tested, "Not in scope (no iPad)" (UI-2 above) |
