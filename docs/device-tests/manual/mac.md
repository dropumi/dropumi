# Manual device testing: Mac app

Read the [common page](README.md) first: privacy, test files, the CLI as a test peer, and the session plan.

The Mac app is the `Dropumi-macOS` target. Test it on the Mac (macOS 27.0.1, arm64). In this section the partners are the iPhone app, the `dropumi` CLI and, for the core set only, the Android phone (see "Pair: Mac ↔ Android phone" in the checklist). The CLI runs on the same Mac, so CLI ↔ Mac app is not a LAN pair. Write "CLI (same Mac)" in the run file's Pair column.

Run every command block from the APPLE worktree. After a logout (UI-4) or in a new Terminal window, `cd "<APPLE worktree>"` first. The blocks below that use relative paths start with this `cd`.

## Run it

**Prerequisites**

- Build only from the APPLE worktree (`dropumi-apple`, branch `feat/native-apps-apple`). The Android worktree holds an older copy of the Apple app.
- You need these tools:
  - Xcode 27 (the recorded run used Xcode 27.1);
  - xcodegen 2.46.0;
  - Rust stable (MSRV 1.89) with the Apple targets;
  - `python3`, `git` and `shasum`, which the core script calls.
- Check the tools. The repo docs do not include the last four commands.

```sh
cd "<APPLE worktree>"
sw_vers
xcodebuild -version
swift --version
xcodegen --version
rustc --version
rustup target list --installed
```

- Add the Rust targets. `aarch64-apple-darwin` is the Mac's own target. The core script always builds the simulator slice too, so you need `aarch64-apple-ios-sim` even though you never use a simulator.

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
```

- xcodegen must be 2.46.0. `brew install xcodegen` may install a different version. Below is the CI install, changed to use a temp folder. The repo does not document this local version.

```sh
XG="$(mktemp -d)"
curl -fsSL -o "$XG/xcodegen.zip" \
  https://github.com/yonaskolb/XcodeGen/releases/download/2.46.0/xcodegen.zip
echo "4d9e34b62172d645eed6457cac13fc222569974098ef4ee9c3368bedf0196806  $XG/xcodegen.zip" \
  | shasum -a 256 -c -
unzip -q "$XG/xcodegen.zip" -d "$XG/xcodegen"
export PATH="$XG/xcodegen/xcodegen/bin:$PATH"
echo "$XG"
```

- `export PATH` lasts only for this shell. CI adds the folder to the PATH for the whole job; you have to do it in every new shell. Write down the `echo "$XG"` output locally, not in the run file. MAC-BONJOUR runs `xcodegen generate` under `set -euo pipefail`, so it stops with "xcodegen: command not found" if the PATH is not set. In a new shell, run `export PATH="<noted XG>/xcodegen/xcodegen/bin:$PATH"` again. If that folder is gone, repeat the block above. Whether a temp folder survives a logout is not confirmed: check and note what you see.

**Signing**

- Create the local signing file and set your personal team ID (find it in Xcode › Settings › Accounts). A local copy may already exist. `-n` keeps an existing copy. Check that it holds the personal team, not an employer team. The file is gitignored: never commit it.

```sh
cd "<APPLE worktree>"
cp -n apps/apple/Config/Signing.local.xcconfig.example apps/apple/Config/Signing.local.xcconfig
open -e apps/apple/Config/Signing.local.xcconfig
```

- In the file that opens, find the line `DEVELOPMENT_TEAM = YOURTEAMID`. Change `YOURTEAMID` to your personal team ID and save. Never paste the file.
- Signing is automatic. The Mac build registers the App ID `io.github.dropumi.Dropumi`. Only the iOS targets use the share extension App ID `io.github.dropumi.Dropumi.share` and the App Group `group.io.github.dropumi`. The Mac app has no App Group and no share extension.
- The Mac app is sandboxed and uses the hardened runtime. It is not notarized.

**Build the Rust core (Release profile)**

```sh
cd "<APPLE worktree>"
bash scripts/ffi/build-apple.sh --profile release
cat apps/apple/DropumiKit/Artifacts/core-stamp.txt
```

- The stamp must show `profile release` and the slices `aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin`.
- The Xcode build never runs cargo.
- A pre-build step, "Check the Rust core", stops the build when the core is missing or out of date. Example: "error: the Rust core is older than the Rust sources; …".
- A Release app built with a dev core only gets a warning, so check the stamp yourself.

**Generate the Xcode project**

```sh
cd "<APPLE worktree>"
(cd apps/apple && xcodegen generate)
```

Run it again after any change to `Signing.local.xcconfig`.

**Release build**

The repo documents only the Debug command. This is the same command with `-configuration Release`. No recorded session has run it yet.

```sh
cd "<APPLE worktree>"
xcodebuild build -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-macOS -configuration Release \
  -destination 'platform=macOS,arch=arm64' -derivedDataPath apps/apple/build -allowProvisioningUpdates
```

- The app is built to `apps/apple/build/Build/Products/Release/Dropumi.app`.
- Release builds leave out the fake core and the UI-test scenarios. REL-1 (extra) checks this.
- Xcode's Run button builds into Xcode's own DerivedData, so you can end up with two copies of the app. Always launch by path.
- For UI-4 (open at login), copy the app to `/Applications` first, because the worktree is on an external volume. This step is an inference, not from the repo. The copy has the same bundle ID. Remove it at the end (Full reset, step 6).

```sh
cd "<APPLE worktree>"
ditto apps/apple/build/Build/Products/Release/Dropumi.app /Applications/Dropumi.app
```

**Launch**

```sh
cd "<APPLE worktree>"
open apps/apple/build/Build/Products/Release/Dropumi.app
```

- The main window may not appear. This was seen under XCUITest; for a normal launch it is part of UI-4. If no window appears, open it from the menu bar icon › "Open Dropumi", or open the app again.
- The Dock icon shows only while the window is open. With the window closed, Dropumi stays in the menu bar and keeps receiving.
- If the core cannot start, the window shows "Dropumi could not start" with "Try again". The menu bar panel shows the same.

**First launch**

1. The welcome screen says "Just drop it nearby." and ends with "Dropumi is a pre-release and has not had a security audit yet.". Press "Continue".
2. "Name this device": the field is prefilled with `Dropumi-XXXX`. Keep that default name (privacy rule) and press "Continue".
3. "Find devices on your network": "macOS may ask whether Dropumi may find devices on your local network. Choose Allow, or Dropumi can't see other devices." Press "Continue".
4. "Who can send to this device?": the choices are "Paired devices only" (the default), "Everyone, for 10 minutes" and "No one for now". Press "Start".

- VoiceOver reads the page dots as "Step N of 4".
- After "Start", macOS asks for Local Network access, with the text "Dropumi finds nearby devices and sends files to them directly over your local network. Nothing goes through the internet." Choose Allow. Answer this before any test: the first Mac Bonjour run failed until access was allowed.
- macOS asks for notification permission only once, the first time you close the window. Allow it, because BG-3 needs it. You then see "Dropumi is still receiving" / "Find it in the menu bar. Choose Quit Dropumi completely there to stop." If you never close the window, macOS never asks.
- "Everyone" is never restored at launch: the app falls back to Paired only. "Auto-accept paired devices" is off by default.

**Where things are**

The container paths below come from the bundle ID. They are an inference, not documented in the repo.

| What | Where |
|---|---|
| Identity and paired devices (`identity.key`, `device.json`, `trusted_devices.json`, `.lock`) | `~/Library/Containers/io.github.dropumi.Dropumi/Data/Library/Application Support/Dropumi/` |
| History | `~/Library/Containers/io.github.dropumi.Dropumi/Data/Library/Application Support/history.json` |
| Settings | `~/Library/Containers/io.github.dropumi.Dropumi/Data/Library/Preferences/io.github.dropumi.Dropumi.plist` |
| Received files | `~/Downloads/Dropumi`, or the folder chosen under Settings › "Save received files to" › "Change…" |

- Partial files are hidden: `.dropumi-<12 letters and digits>.part` in the receive folder. List them with `ls -A`.
- Settings › "Save received files to" › "Show" opens the folder in Finder. No button goes back to `~/Downloads/Dropumi`; see UI-4.
- The Settings subtitle shortens the path against your real home folder, not the sandbox container. The default folder is the real Downloads folder, with symlinks resolved. So expect "~/Downloads/Dropumi", or "~/…" for a chosen folder under your home folder. A folder on another volume shows a full `/Volumes/…` path, which is expected. A full `/Users/…` path is a defect: mark the row Failed. Keep the path out of screenshots in every case.
- To empty the receive folder between tests, including hidden files (inference):

```sh
find ~/Downloads/Dropumi -mindepth 1 -delete
```

**Reset to a fresh state**

The repo documents no reset procedure. Steps 1 to 6 are inferred.

- **Partial reset in the app:** Settings › This device › "Reset identity…" › "Reset identity".
  - The Mac gets a new key and fingerprint, and every paired device must pair again.
  - Onboarding, settings, history and received files stay.
- **Full reset:**
  1. Turn Settings › "Open at login" off in the copy that turned it on. For UI-4 that is the `/Applications` copy. "Open at login" acts on the app bundle that is running.
  2. Menu bar › "Quit Dropumi completely".
  3. Clear the settings through `defaults`, then delete the container and the received files. The `defaults delete` comes first because macOS may cache the settings after a plain file delete (inference):

```sh
pgrep -x Dropumi || echo "Dropumi is not running"
defaults delete ~/Library/Containers/io.github.dropumi.Dropumi/Data/Library/Preferences/io.github.dropumi.Dropumi
rm -rf ~/Library/Containers/io.github.dropumi.Dropumi
ls -A ~/Downloads/Dropumi
rm -rf ~/Downloads/Dropumi
```

  macOS may ask whether Terminal may access another app's data, or refuse unless Terminal has Full Disk Access. You can delete the folder in Finder instead. After the next launch, check that the welcome screen ("Just drop it nearby.") appears. If it does not, record that.

  4. To reset only the settings, run this while the app is quit:

```sh
defaults delete ~/Library/Containers/io.github.dropumi.Dropumi/Data/Library/Preferences/io.github.dropumi.Dropumi
```

  5. Set Local Network (System Settings › Privacy & Security › Local Network) and notifications (System Settings › Notifications › Dropumi) as the next test needs. Whether macOS asks again after a reset is not confirmed: check and note what you see.
  6. Delete old `.app` copies so that Launch Services and the login items cannot open a stale one. They are in `apps/apple/build`, in Xcode's DerivedData and in `/Applications`. Then check System Settings › General › Login Items: Dropumi must no longer be listed.

```sh
rm -rf /Applications/Dropumi.app
```

- The Debug, Release, `/Applications` and Xcode-run copies use the same bundle ID. So they probably share one container, with the same identity and settings (inference).

**Logs**

- The app writes no logs of its own. The Swift code has no `os.Logger`, `NSLog` or `print`, and the Rust core's `tracing` events are not collected anywhere in the app.
- Your evidence is the UI: banners, the Transfers screen and its history.
- History rows in Transfers › "Earlier":
  - The title is the first file name, plus "and N more" when there are more files.
  - The subtitle is "Sent to ‹name› · ‹time›" or "Received from ‹name› · ‹time›". When a transfer did not complete, a second line follows with a short headline, for example "Connection lost", "Only paired devices", "You declined" or "Cancelled by the other device".
  - A failed transfer shows no banner and no longer message text.
- For system messages only (inference), use Console.app filtered on the process `Dropumi`, or:

```sh
log stream --predicate 'process == "Dropumi"'
```

Never paste lines that show addresses, paths, fingerprints or device names.

**The CLI as a partner (on the same Mac)**

Run all CLI steps in one Terminal window, so the variables stay set. The CLI config is a throwaway folder. Never use `~/Library/Application Support/dropumi`.

```sh
cd "<APPLE worktree>"
cargo build --release --locked -p dropumi-cli
DROP="$PWD/target/release/dropumi"
export DROPUMI_CONFIG_DIR="$(mktemp -d)"
RX="$(mktemp -d)"
ls -d "$FILES" "$UNPAIRED"
"$DROP" identity show
```

- `$FILES` and `$UNPAIRED` come from the common part. Run its setup in this same Terminal window first. If either is not set, `ls -d` fails.
- `$FILES` is the common test file set, made with 2048. It holds `small.txt`, `empty.bin`, `photo-like.bin`, `large-2048MiB.bin`, `Résumé – naïve 日本語.txt`, `invoice<U+202E>txt.exe`, `many/` (300 files) and `SHA256SUMS`. The script fills the files with new random content each time it runs, so check received files only against `$FILES/SHA256SUMS`.
- `$UNPAIRED` is a CLI config that is never paired with anything. Use it for every "unpaired CLI" step (DISC-3, PAIR-2, TX-3, TX-4). Do not use `--config-dir "$(mktemp -d)"`, which makes a new identity each time. Never pair with `$UNPAIRED`.
- To add test files in the Mac app, open the folder in Finder and drag the files in:

```sh
open "$FILES"
```

  Or, in the file picker, press ⌘⇧G and paste the output of `echo "$FILES"`.
- The Mac app's address is under Settings › This device › ADDRESS, shown as `<Mac's Wi-Fi address>:<port>`. That list has one line for each active IPv4 interface, and it does not name the interface. Use the Wi-Fi one. On the Mac, `ipconfig getifaddr en0` is the address the repo's interop script takes as Wi-Fi. Never record it.
- The Mac app usually holds port 47321. A CLI `receive` or `pair --listen` then falls back to a free port. Read the port from the CLI's `Listening on <addr> as "<name>"` line.

**Checking SHA-256 on the Mac**

The receiver saves files flat (the files from `many/` land at the top level) and removes U+202E from names. So a plain `shasum -a 256 -c SHA256SUMS` reports missing files. This adjusted check is suggested but has not been run yet: check that it works and note the result. It runs in a subshell, so your shell stays in the worktree.

```sh
(cd ~/Downloads/Dropumi && sed -e 's#  \./many/#  ./#' -e $'s/\xe2\x80\xae//' "$FILES/SHA256SUMS" \
  | shasum -a 256 -c - 2>/dev/null | grep -c ': OK$')
```

Files you did not send count as missing. Compare the number of `OK` lines with the number of files you sent. Empty the receive folder before each test, or duplicates get ` (1)` names.

**Before the session**

- The Mac and the iPhone are on the same Wi-Fi network. Record the band and whether client isolation is on.
- The iPhone is unlocked, with Dropumi in front. iOS receives only while the app is open.
- Local Network access is allowed on both devices.
- Check the macOS firewall under System Settings › Network › Firewall. If it is on, allow Dropumi, and the `dropumi` binary when macOS asks. Record whether the firewall was on and whether a prompt appeared. The repo says a host firewall may block or ask about incoming connections to `receive` / `pair --listen` and about mDNS, and that this is UNVERIFIED and Not tested.
- For the NET rows, unplug the iPhone's cable so that Wi-Fi is the only path (inference).

**TEMPLATE rows that are not in this checklist**

| Rows | Where they run | The Mac app's part |
|---|---|---|
| BG-1, SHARE-1, SHARE-2 | iPhone section | Receiver: the iPhone sends to the Mac app. Accept on the Mac and check what the iPhone section asks for. |
| SHARE-3, BG-4 | Android section | Receiver when that section uses the Mac app. Follow that section. |
| NET-5, BG-5, PERM-2, UI-3, BAK-1, OVL-1, NSD-1, EXP-1, INST-1 | Android only | None. Not in scope here. NET-5 and NSD-1 use the CLI or `dns-sd` on the Mac, not the Mac app. |
| BG-2 | iOS only | None. Not in scope here. |
| UI-2 | iPad only | None. Not in scope here. |

## Checklist

Labels:
- Manual rows get one result: `Manual device test passed`, `Failed` (say what happened) or `Not tested`.
- The automated rows get `Automated test passed`, `Failed` or `Not tested`, as run 2026-10-18-01 did. These are MAC-UI, MAC-BONJOUR and the automated half of PROMPT-1, which runs inside MAC-UI.
- Never write "supported".

Suggested order:
1. The rows that need the Mac and the iPhone unpaired: DISC-1 to DISC-4, TX-4, TX-3 and PAIR-2.
2. PAIR-1, then the paired rows, including every row that uses the CLI (SPOOF-1 and TRUST-1 (extra) too).
3. The Mac ↔ Android phone rows.
4. UI-4. The logout clears the shell variables, so finish the CLI rows first.
5. MAC-UI and MAC-BONJOUR, with the Release app quit.
6. Clean up (at the end of this section).

To unpair, open Settings › "Paired devices" › the row's "…" › "Remove…" › "Remove" on each side.

- [ ] **DISC-1** Each side sees the other in Nearby. Partners: the iPhone, then the CLI (same Mac). Steps: 1) Open Nearby on the Mac and on the iPhone, both on "Paired devices only". Apps browse only while Nearby, Send or Pairing is on screen. 2) Wait for each list. 3) Browse from the CLI, then start a CLI receiver and watch the Mac's Nearby. Stop the receiver with Ctrl-C.
  ```sh
  "$DROP" discover --timeout-ms 5000
  "$DROP" receive --dir "$RX"
  ```
  **Expect:**
  - The Mac's list (headed "Send to") shows the iPhone's name with "Not paired", and the subtitle counts devices, for example "1 device found · works without internet".
  - The iPhone lists the Mac.
  - `discover` prints a line `<name>  fp <8-char hint>  <ip>:<port>` for the Mac app.
  - While the CLI receives, the Mac lists the CLI's name.
  - Nearby never says "Paired".

  **Record:** each direction separately (Mac sees iPhone, iPhone sees Mac, Mac sees CLI, CLI sees Mac), marking the CLI ones "same Mac", and roughly how long each took. Never paste `discover` output: it shows addresses and hints.

- [ ] **DISC-2** A device never lists itself. Partner: the iPhone. Steps: 1) Note the Mac's own name: it is at the bottom of the Mac sidebar, or on the Settings card. 2) With the iPhone listed, check Nearby, the chips on the Send screen and Settings › "Pair a device". **Expect:** the Mac's own name appears in none of them. **Record:** the result and which screens you checked.

- [ ] **DISC-3** Off: the device is not listed and connections are refused. Partners: the iPhone and the CLI. Steps: 1) Stop every CLI `receive` and `pair` (Ctrl-C) before you choose Off, and start none until step 5. With the Mac app Off, its port is free. A CLI started now could take that port and answer in place of the Mac app. Then note the Wi-Fi line under Settings › This device › ADDRESS. 2) Choose Settings › "Who can send to this device" › "Off". 3) Watch the iPhone's Nearby. 4) Try the CLI, using the port from step 1. The `$UNPAIRED` config keeps it unpaired. 5) Click "Turn on for paired devices".
  ```sh
  "$DROP" discover --timeout-ms 5000
  "$DROP" --config-dir "$UNPAIRED" send "$FILES/small.txt" --to <Mac's Wi-Fi address>:<port> --allow-unverified; echo "exit $?"
  ```
  **Expect:**
  - The status pill reads "Not receiving".
  - Nearby shows "Not receiving" / "Other devices can't find or send to this one. You can still send." with the button "Turn on for paired devices".
  - The Settings caption reads "Not receiving. Other devices can't find or send to this one.", and This device › ADDRESS reads "Not receiving right now.".
  - The iPhone stops listing the Mac, and `discover` does not list it.
  - `send` fails with a network error and exit 5.
  - After step 5, the pill reads "Paired devices only".

  **Record:** how long the iPhone took to drop the Mac, and the CLI's error line and exit code (redact the address).

- [ ] **DISC-4** The advertised host name is `dropumi-…`, not the device's name. Partner: none; this uses Terminal on the Mac. Steps: 1) With the Mac app on "Paired devices only" (and the iPhone app in front, to check it too), browse. Stop with Ctrl-C once the list settles. 2) Resolve each instance and find the Mac app by its `n=` value. 3) Compare with the Mac's own host name. That last command is not in the repo docs; only compare, do not record the name. 4) Optional: switch the Mac to "Off", back to "Paired only", and browse again.
  ```sh
  dns-sd -B _dropumi._tcp
  dns-sd -L <instance> _dropumi._tcp local.
  scutil --get LocalHostName
  ```
  **Expect:**
  - The instance is `dropumi-<8 hex>`.
  - `dns-sd -L` reaches the host `dropumi-<8 hex>.local.`.
  - The TXT keys are only `v`, `n` and `fp`, and the device name appears only in `n=`.
  - The Mac's host name appears nowhere.
  - After step 4 the label is new, because labels are random for each advertisement.

  **Record:** pass or fail for the Mac app and for the iPhone app. Never paste `fp=` values, ports or addresses.

- [ ] **PAIR-1** The codes match on both screens, and both devices list each other as paired. Partners: the iPhone, then the CLI. Pair with each partner in both directions, so that each side starts once. Steps: 1) On the Mac, choose Settings › "Pair a device" › "Wait for a device". 2) On the iPhone, choose Nearby "…" › "Pair a device" › "Pair" next to the Mac's name. 3) Compare the codes and press "Codes match" on both devices. 4) Open "Paired devices" on both. 5) For the CLI, run the first command block. On the Mac, choose Settings › "Pair a device" › "Pair" next to the CLI's name. Answer `y` in Terminal only if the codes match. Then run `trust list`. 6) Other direction with the iPhone: remove the pairing on both apps. On the iPhone, choose Nearby "…" › "Pair a device" › "Wait for a device". On the Mac, choose Settings › "Pair a device" › "Pair" next to the iPhone's name. Compare the codes and press "Codes match" on both. 7) Other direction with the CLI: on the Mac, remove the CLI under "Paired devices". In Terminal, remove the Mac using the full fingerprint from the Mac's Settings › This device › FINGERPRINT (the CLI ignores the dashes). On the Mac, choose Settings › "Pair a device" › "Wait for a device". Then run `pair --to` with the Mac app's name (second block). Answer `y` only if the codes match, and click "Codes match" on the Mac.
  ```sh
  "$DROP" pair --listen; echo "exit $?"
  "$DROP" trust list
  ```
  ```sh
  "$DROP" trust remove <Mac FP52>
  "$DROP" pair --to "<the Mac app's Dropumi-XXXX name>"; echo "exit $?"
  "$DROP" trust list
  ```
  **Expect:**
  - The waiting side shows "Waiting for a device · 5:00", counting down.
  - Both devices show the code prompt. The devices are not paired yet, so the other device appears as untrusted:
    - "Pair with this device?", then "Unverified device" as the title;
    - the other device's name only as claims “‹name›”, then "Not paired";
    - the fingerprint;
    - the code, for example "123 456";
    - "Check that the other device shows the same code. Only then tap Codes match.";
    - "Expires in m:ss", counting down from about 2:00.
  - The other device's name is never the title of this prompt.
  - "Codes match" stays dimmed for 600 ms after the prompt appears.
  - Both end on "Paired" / "‹alias› can now send to this device, and you can send to it without a code." with "Done".
  - "Paired devices" lists the alias with "Paired with a code · XXXX-XXXX-XXXX-XXXX-XXXX-XXXX". Those 24 characters match the start of the other device's Settings › This device › FINGERPRINT.
  - The CLI prints `Paired: Trusted "<alias>" (claims "<name>") · XXXX-…` and exits 0, and `trust list` shows the Mac app with `(sas)`.
  - The Mac's Nearby now shows the iPhone as "Looks like ‹alias›", never as "Paired".
  - Step 6: the iPhone shows "Waiting for a device · 5:00", the Mac shows the same code prompt as above, and both end paired again.
  - Step 7: the CLI asks `Pair with Unverified device (claims "<name>") · fp XXXX-…?`, then `Does the other device show NNN NNN? [y/N]`. Then it prints the same `Paired:` line and exits 0. `trust list` shows the Mac app with `(sas)` again, and the Mac's "Paired devices" lists the CLI again with "Paired with a code · …".

  **Record:** which side waited in each run, every result, and whether the fingerprints matched (yes or no; never the fingerprint itself).

- [ ] **PAIR-2** One side presses "Codes don't match", and nothing is stored on either side. Partners: the iPhone, then the CLI. Do this while the devices are unpaired. Steps: 1) Start pairing as in PAIR-1. 2) On the Mac, click "Codes don't match". 3) Run `pair --listen` with the `$UNPAIRED` config, and pair with it from the Mac. Answer `n` in Terminal. 4) Check "Paired devices" on both apps.
  ```sh
  "$DROP" --config-dir "$UNPAIRED" pair --listen; echo "exit $?"
  ```
  **Expect:**
  - Both apps show "Pairing declined" / "The other device did not confirm the code. Nothing was saved.". The Mac shows this even though it declined; that wording is a known issue.
  - The other side's open code prompt is withdrawn.
  - "Paired devices" gains no row.
  - The CLI fails with `pairing_rejected` and exit 6.

  **Record:** which side declined, what each side showed, and the paired lists afterwards.

- [ ] **TX-1** A paired transfer with a prompt; the SHA-256 hashes are equal. Partners: the paired iPhone in both directions, then the paired CLI (same Mac) in both directions. Auto-accept is off on all of them. Steps (iPhone): 1) On the Mac's Nearby, click "Send" on the iPhone's row. 2) Click "Add files" and pick `small.txt` and `photo-like.bin` from `$FILES`. 3) Click "Send to ‹name›" and accept on the iPhone. 4) On the iPhone, send the two received files back to the Mac (they are in Files › On My iPhone › Dropumi › Received). 5) Accept on the Mac. 6) Check the hashes and the quarantine flag.
  ```sh
  (cd ~/Downloads/Dropumi && sed -e 's#  \./many/#  ./#' -e $'s/\xe2\x80\xae//' "$FILES/SHA256SUMS" \
    | shasum -a 256 -c - 2>/dev/null | grep -c ': OK$')
  xattr -l ~/Downloads/Dropumi/small.txt
  ```
  Steps (CLI): 7) Empty the receive folder. Send the two files from the paired CLI and accept on the Mac. Run the hash check above again. 8) Empty `$RX` and start a CLI receiver. On the Mac's Nearby, click "Send" on the CLI's row and add the same two files from `$FILES`. Click "Send to ‹name›" and answer `y` in Terminal. Then check `$RX`.
  ```sh
  "$DROP" send "$FILES/small.txt" "$FILES/photo-like.bin" --to <Mac's Wi-Fi address>:<port>; echo "exit $?"
  find "$RX" -mindepth 1 -delete
  "$DROP" receive --dir "$RX" --once; echo "exit $?"
  (cd "$RX" && sed -e 's#  \./many/#  ./#' -e $'s/\xe2\x80\xae//' "$FILES/SHA256SUMS" \
    | shasum -a 256 -c - 2>/dev/null | grep -c ': OK$')
  ```
  **Expect:**
  - The Send hint reads "Dropumi checks that this is your paired device before sending.".
  - Transfers shows "Sending to ‹alias›", then "Waiting for the other person to accept…", then "‹done› of ‹total›" with a rate.
  - The Mac's sheet shows:
    - "Incoming files", the alias and "Paired";
    - "wants to send you 2 files", the file list and "Total";
    - "Expires in …", starting near 1:59;
    - no code and no fingerprint.
  - "Accept" stays dimmed for 600 ms.
  - History shows a row with "Received from ‹alias› · ‹time›", the "Done" icon and "Show in Finder". The sender's row reads "Sent to ‹alias› · ‹time›".
  - Each hash check prints 2. A match after the iPhone round trip covers both directions.
  - The CLI `send` and `receive --once` both exit 0.

  **Record:** the times, each hash result, both CLI exit codes, and whether `xattr -l` lists `com.apple.quarantine` (quarantine is Not tested so far). Write the CLI results in their own rows, with the Pair column "CLI (same Mac)".

- [ ] **TX-2** With auto-accept on there is no prompt; with it off there is a prompt. Partner: the paired iPhone. Steps: 1) Turn on the Mac's Settings › "Auto-accept paired devices". 2) Send `small.txt` from the iPhone. 3) Turn the setting off. 4) Send again. **Expect:** in step 2 no sheet appears and history shows "Received from ‹alias›". In step 4 the "Incoming files" sheet appears. **Record:** both results.

- [ ] **TX-3** Everyone for 10 minutes, with an unverified sender: both sides show the same code, and no pairing is created. Partners: the unpaired iPhone, then the CLI with the `$UNPAIRED` config. Steps: 1) On the Mac, choose Settings › "Everyone · 10 min". 2) On the iPhone, click "Send" on the Mac's row (it shows "Not paired"), add a file and send. 3) On the iPhone's "Check the code" screen, press "Codes match". 4) On the Mac, compare the codes and click "Accept". 5) Send from the CLI with the `$UNPAIRED` config (first command), answer `y`, and accept on the Mac. 6) Check "Paired devices" on both apps. 7) Put the iPhone on Everyone and send from the Mac. 8) Mac → CLI: empty `$RX` and start a CLI receiver with the `$UNPAIRED` config on Everyone (second and third commands). On the Mac's Nearby, click "Send" on that CLI's row ("Not paired"), add `small.txt` and click "Send to ‹name›". Compare the codes, click "Codes match" on the Mac, then answer `y` in Terminal. Then run `trust list` and check the Mac's "Paired devices".
  ```sh
  "$DROP" --config-dir "$UNPAIRED" send "$FILES/small.txt" --to <Mac's Wi-Fi address>:<port> --allow-unverified; echo "exit $?"
  find "$RX" -mindepth 1 -delete
  "$DROP" --config-dir "$UNPAIRED" receive --dir "$RX" --policy everyone --once; echo "exit $?"
  "$DROP" --config-dir "$UNPAIRED" trust list
  ```
  **Expect:**
  - The countdown is broken, and this is a confirmed code defect:
    - The core sends the end of the Everyone window in Unix seconds. The Swift code treats it as milliseconds and divides it by 1000, so the time left is always clamped to 0.
    - With the real core, the pill, the device card at the bottom of the sidebar and the menu bar panel read "Everyone · 0:00".
    - The Settings caption reads "Anyone nearby can ask to send for 0:00. You compare a code before accepting.".
    - The UI tests missed it because the fake core sends no end time.
  - The Mac's sheet appears only after the iPhone presses "Codes match". It shows:
    - "Unverified device", `claims “‹name›”` and "Not paired";
    - the fingerprint;
    - "Compare this code with the sender's screen", the code, and "Accept only if both screens show the same code.".
  - The code is the same on both sides; the CLI shows it as `Verification code: NNN NNN`.
  - History reads "Received from Unverified device".
  - The CLI prints `Done: 1 file(s), …` and exits 0.
  - Neither app gains a paired device.
  - In step 7, the Mac first shows "Check the code" with "This device isn't paired. Check that its screen shows the same code before anything is sent.".
  - In step 8:
    - The Mac's Send hint reads "This device isn't paired. You'll both see a code to compare before anything is sent.".
    - The Mac shows "Check the code" with "This device isn't paired. Check that its screen shows the same code before anything is sent.".
    - Terminal shows the same code in `Verification code: NNN NNN`. After you click "Codes match", Terminal asks `Accept 1 file(s) from Unverified device (claims "<name>") · fp XXXX-… · SAS NNN NNN? [y/N]`.
    - `receive --once` exits 0 and `$RX` holds `small.txt`.
    - `trust list` prints `No trusted devices.`, and the Mac's "Paired devices" gains no row.
  - After 10 minutes the Mac shows the banner "Back to paired devices only" / "The 10 minutes for everyone ended.". This banner comes from the core's own timer, so it still tests the 10-minute end.

  **Record:**
  - Whether the codes matched, for each direction.
  - The countdown sub-check as Failed, naming the defect ("everyoneUntil is in seconds and the app divides it by 1000"), with the exact text you saw.
  - When the banner appeared.

- [ ] **TX-4** Paired only: an unpaired sender is refused. Partners: the CLI with the `$UNPAIRED` config, then the unpaired iPhone. Steps: 1) Set the Mac to "Paired only" and dismiss any banner with ×; the refusal notice never replaces another banner. 2) Send from the CLI. `--allow-unverified` lets the CLI reach the Mac's check; without it, the CLI stops itself with `unverified_peer`. 3) Wait more than 60 seconds, dismiss the banner, then send from the unpaired iPhone.
  ```sh
  "$DROP" --config-dir "$UNPAIRED" send "$FILES/small.txt" --to <Mac's Wi-Fi address>:<port> --allow-unverified; echo "exit $?"
  ```
  **Expect:**
  - No sheet appears, and nothing arrives in `~/Downloads/Dropumi`.
  - The Mac shows "A device was refused" / "A device on this network tried to connect and was turned away. To pair with it, choose Wait for a device under Pair a device first.", with no action button. It shows at most once every 60 seconds.
  - The CLI fails with `not_authorized` and exit 6.
  - The iPhone's Transfers › "Earlier" gets a row titled with the file name. Its subtitle is "Sent to ‹name› · ‹time›", and its second line is "Only paired devices". Which name appears there (the Mac's name from Nearby, or "Unverified device") is not confirmed in code: check and note what you see.
  - The other notice, "A device that isn't paired tried to send" / "Pair with it, or let everyone send for 10 minutes." with "Let everyone send for 10 minutes", is not expected: the current core refuses before it knows who is sending. If it appears, record it.

  **Record:** the banner text you saw, the CLI exit code, the iPhone's history row, and whether the iPhone ever showed a code (not expected).

- [ ] **TX-5** Decline. Partners: the paired iPhone, then the CLI (paired from PAIR-1). Steps: 1) Send `small.txt` from the iPhone and click "Decline" on the Mac. 2) Send again and press Escape. 3) Send from the CLI and click "Decline".
  ```sh
  "$DROP" send "$FILES/small.txt" --to <Mac's Wi-Fi address>:<port>; echo "exit $?"
  ```
  **Expect:**
  - The sheet closes and nothing new arrives in `~/Downloads/Dropumi`.
  - Mac history shows "You declined"; the iPhone's history shows "Declined".
  - Escape works like "Decline".
  - The CLI exits 3.

  **Record:** all three results and the CLI exit code.

- [ ] **TX-6** A file of at least 2 GiB; note the time and the method. Partner: the paired iPhone, or the CLI (same Mac). Steps: 1) Send `$FILES/large-2048MiB.bin` from the Mac to the iPhone (that counts as TX-6 for the iPhone). 2) Send it back to the Mac. Start a stopwatch when you click Accept. 3) Watch the Transfers card. 4) Stop the stopwatch when the history entry appears. 5) Compare the hashes. As an alternative sender, use the CLI below.
  ```sh
  "$DROP" send "$FILES/large-2048MiB.bin" --to <Mac's Wi-Fi address>:<port>; echo "exit $?"
  (cd ~/Downloads/Dropumi && shasum -a 256 large-2048MiB.bin); grep large "$FILES/SHA256SUMS"
  ```
  **Expect:**
  - The card shows "‹done› of ‹total›" and a line like "42% · 28 MB/s · 20 s left" ("3 min left" when more than a minute remains).
  - History shows "Received from ‹alias›" with "Done".
  - The two hashes are equal.
  - The CLI prints `Done: 1 file(s), … in X s`.

  **Record:**
  - The elapsed time and how you measured it. The app shows only the finish time, so use a stopwatch, or the CLI's `in X s`.
  - The method: iPhone → Mac over Wi-Fi (with the band), or CLI on the same Mac.
  - The hash result.

- [ ] **TX-7** 300 small files (`many/`). Partner: the paired CLI (same Mac). Steps: 1) Empty the receive folder. 2) Send the files. 3) Accept on the Mac. 4) Count the files and check the hashes. 5) Read the history row in Transfers › "Earlier". 6) Optional: repeat with the window closed and Finder active, and read the "Files received" notification.
  ```sh
  "$DROP" send "$FILES"/many/* --to <Mac's Wi-Fi address>:<port>; echo "exit $?"
  ls ~/Downloads/Dropumi | grep -c '^file-'
  (cd ~/Downloads/Dropumi && sed -e 's#  \./many/#  ./#' "$FILES/SHA256SUMS" | grep ' \./file-' \
    | shasum -a 256 -c - | grep -c ': OK$')
  ```
  **Expect:**
  - The sheet reads "wants to send you 300 files" and lists files, then "and 100 more" (the prompt carries at most 200 names), then "Total".
  - The running card counts 300 files.
  - All 300 files land flat in `~/Downloads/Dropumi`.
  - Both counts are 300, and the CLI exits 0.
  - The correct history title is "file-001.bin and 299 more". The app copies only the prompt's 200 names into the record, so the code predicts "file-001.bin and 199 more". If you see that, mark this sub-check Failed and name the defect (the history uses the prompt's capped list of names).
  - Step 6: the correct text is "All 300 files were saved and checked."; "All 200 files…" is the same defect.

  **Record:** the counts, the time, the CLI's `Done` line, the exact history title and, if you ran step 6, the notification's count.

- [ ] **TX-8** Unicode, right-to-left, duplicate and 0-byte names; nothing is overwritten. Partner: the paired CLI (same Mac). The iPhone cannot be the sender here: it already lost the right-to-left character when it received the file. Steps: 1) Empty the receive folder. 2) Send the files below and accept. 3) Note the hashes. 4) Send the same files again and accept. 5) List the folder and compare the hashes.
  ```sh
  "$DROP" send "$FILES/empty.bin" "$FILES/Résumé – naïve 日本語.txt" "$FILES"/invoice*txt.exe --to <Mac's Wi-Fi address>:<port>
  (cd ~/Downloads/Dropumi && ls -lAn)
  ls ~/Downloads/Dropumi | grep -c $'\xe2\x80\xae'
  (cd ~/Downloads/Dropumi && shasum -a 256 *)
  ```
  **Expect:**
  - The first send saves `empty.bin` (0 bytes), `Résumé – naïve 日本語.txt` and `invoicetxt.exe`, without the U+202E.
  - The second send adds ` (1)` copies, such as `empty (1).bin`. The first copies keep their hashes.
  - The `grep -c` prints 0.
  - The core strips U+202E from every offered name before the prompt, and the app keeps those names for the card and the history. So the "Incoming files" list shows "invoicetxt.exe", with no reordering. So do the Transfers card and the history row wherever they show that name. The history title shows only the first name plus "and N more". Any visual reversal is Failed.

  **Record:** the saved names (never with paths), and whether the first copies' hashes stayed the same.

- [ ] **CAN-1** The sender cancels; no partial file is left on the receiver. Partners: the paired iPhone, then the paired CLI (same Mac). Steps: A) Send `large-2048MiB.bin` from the iPhone, accept on the Mac, then tap × on the iPhone partway through. Then list the receive folder. B) Send it from the Mac to the iPhone, then click × ("Cancel") on the Mac's Transfers card partway through. C) Send `$FILES/large-2048MiB.bin` from the CLI, accept on the Mac, and press Ctrl-C once in Terminal partway through. List the receive folder. D) Empty `$RX` and start `receive --once`. Send the large file from the Mac to the CLI, answer `y`, and click × on the Mac's Transfers card partway through. List `$RX`.
  ```sh
  ls -A ~/Downloads/Dropumi
  "$DROP" send "$FILES/large-2048MiB.bin" --to <Mac's Wi-Fi address>:<port>; echo "exit $?"
  find "$RX" -mindepth 1 -delete
  "$DROP" receive --dir "$RX" --once; echo "exit $?"
  ls -A "$RX"
  ```
  **Expect:**
  - In A, Mac history shows "Cancelled by the other device", and there is no partial file and no `.dropumi-*.part` file.
  - In B, the Mac cancels at once, with no confirmation, and shows "Cancelled by you". The iPhone shows "Cancelled by the other device" and keeps no partial file; check it as in the iPhone section.
  - In C, the CLI exits 130 (Interrupted). On a cancel, the send code sends a cancel to the peer, so Mac history shows "Cancelled by the other device". There is no partial file or `.part` file in `~/Downloads/Dropumi`.
  - In D, Mac history shows "Cancelled by you". `receive --once` exits with the transfer's code, 4 (Cancelled), and `$RX` holds no partial file or `.dropumi-*.part` file.

  **Record:** each result, both CLI exit codes, and each `ls` result. Write the CLI results in their own rows, with the Pair column "CLI (same Mac)".

- [ ] **CAN-2** The receiver cancels; no partial file is left. Partners: the paired iPhone, then the CLI. Steps: 1) Send the large file from the iPhone, accept, and click × on the Mac's Transfers card partway through. 2) Repeat, cancelling with × in the menu bar panel instead. 3) Repeat with the CLI as the sender.
  ```sh
  "$DROP" send "$FILES/large-2048MiB.bin" --to <Mac's Wi-Fi address>:<port>; echo "exit $?"
  ls -A ~/Downloads/Dropumi
  ```
  **Expect:**
  - Mac history shows "Cancelled by you", and the iPhone shows "Cancelled by the other device".
  - The CLI exits 4.
  - No partial file and no `.part` file are left.

  **Record:** all three results.

- [ ] **NET-1** The sender's Wi-Fi goes off during a transfer. Partner: the paired iPhone as the receiver, with its cable unplugged. Steps: 1) Send `large-2048MiB.bin` from the Mac and accept on the iPhone. 2) Partway through, turn the Mac's Wi-Fi off. 3) Wait up to about 2 minutes. The core gives up after 30 s with no data, or 120 s when writes stall. 4) Turn Wi-Fi back on and send `small.txt`.
  **Expect:**
  - Both sides end with a failure and neither shows success. Each side gets a Transfers › "Earlier" row with the Failed icon. Its subtitle is "Sent to ‹alias› · ‹time›" on the Mac and "Received from ‹alias› · ‹time›" on the iPhone. The second line is a short headline such as "Connection lost" or "No answer". Which headline each side shows is not confirmed: note it. There is no banner and no longer message text.
  - No partial file is left on the iPhone.
  - After Wi-Fi returns, the devices list each other again and the small send works.
  - If Nearby shows "Local network access is off" while Wi-Fi is off, it is the false positive described in PERM-1. Note it; it is not a permission problem.

  **Record:** each side's second line and how long it took.

- [ ] **NET-2** The receiver's Wi-Fi goes off during a transfer. Partner: the paired iPhone as the sender, with its cable unplugged. Steps: 1) Send `large-2048MiB.bin` from the iPhone and accept on the Mac. 2) Partway through, turn the Mac's Wi-Fi off. 3) Wait up to about 2 minutes. 4) Turn Wi-Fi back on and list the receive folder.
  ```sh
  ls -A ~/Downloads/Dropumi
  ```
  **Expect:** both sides get a history row with the Failed icon and a second-line headline (which one is not confirmed: note it). No partial file or `.part` file is left on the Mac. **Record:** each side's second line, any banner the Mac showed (including the PERM-1 false positive), and the `ls` result.

- [ ] **NET-3** The access point reboots. Partner: the paired iPhone. Steps: 1) Send `large-2048MiB.bin` from the iPhone and accept on the Mac. 2) Reboot the access point partway through. 3) When the network is back, check Nearby and This device › ADDRESS, then send `small.txt`.
  **Expect:**
  - The transfer fails on both sides and neither shows success.
  - No partial file is left on the Mac.
  - Afterwards both devices list each other again (the addresses may change) and the small send works.

  **Record:** each side's history second line, the recovery time and any address change (yes or no only).

- [ ] **NET-4** Client isolation: Connect by address works. Partner: the paired iPhone. This needs a network that filters multicast or isolates clients; if you have none, record Not tested. Steps: 1) Put both devices on that network. Nearby should stay on "Looking for devices…". 2) Read the iPhone's Settings › This device › ADDRESS. If it lists several lines, use the one on the Wi-Fi network. The screen does not name the interface: note which line you used, never the address. 3) On the Mac, choose Nearby "…" › "Connect by address". Type `<address>:<port>`, and optionally at least 24 characters of the iPhone's fingerprint. 4) Click "Choose files", then "Send files". 5) Repeat the other way, from the iPhone to the Mac's Wi-Fi address.
  **Expect:**
  - The fingerprint hint reads:
    - "Without a fingerprint, you compare a code with the other device first." when the field is empty;
    - "Type at least 24 characters of the fingerprint." with 1 to 23 characters;
    - "Dropumi checks this fingerprint before anything is sent." with 24 or more.
  - "Send files" is enabled only when the address contains `:`, files are chosen, and the fingerprint field is empty or has at least 24 characters.
  - The iPhone is paired, so no code appears on either side, even with an empty fingerprint field. The sender asks for a code only when the receiver is not paired. A code appears on both sides only for an unpaired receiver that is on Everyone. An unpaired receiver on Paired only refuses, as in TX-4.
  - The send completes and the Mac switches to Transfers.
  - If the network blocks all traffic between clients, this fails too: record the network's setting.

  **Record:** the network type, whether discovery failed, whether any code appeared, and the result in both directions.

- [ ] **BG-3** With the window closed, the menu bar receives and notifies, and Quit stops advertising. Partner: the paired iPhone. Notifications are posted only while Dropumi is not the active app. Steps:
  1) In a second Terminal window, keep this running: `dns-sd -B _dropumi._tcp`.
  2) Close the Dropumi window.
  3) Click Finder, so that Dropumi is not the active app.
  4) Send `small.txt` from the iPhone. When the notification appears, hover over it or expand it (Options) and check its actions.
  5) Click the notification, or the menu bar › "Someone wants to send you files", and click "Accept".
  6) Turn "Auto-accept paired devices" on, close the window, click Finder, and send `large-2048MiB.bin` from the iPhone. Open the menu bar panel while the file arrives. After you check the panel row, close the panel and click Finder before the transfer finishes.
  7) Choose menu bar › "Quit Dropumi completely".
  8) Run `"$DROP" discover --timeout-ms 5000` and look at the iPhone's Nearby.

  **Expect:**
  - Step 2: the Dock icon goes away and the menu bar icon stays. On the first close only, macOS asks for notification permission, then shows "Dropumi is still receiving".
  - Step 4:
    - The notification reads "Incoming files" / "‹alias› wants to send you 1 files.". "1 files" is a known plural issue.
    - The notification has no Accept and no Decline action. Clicking it only opens Dropumi.
    - The window does not come to the front by itself.
    - The panel shows "Someone wants to send you files".
  - Step 5: the window opens with the sheet, and the notification goes away when the prompt closes.
  - Step 6:
    - No prompt appears.
    - The panel shows a row with the alias, a progress bar and × ("Cancel").
    - At the end the notification "Files received" / "All 1 files were saved and checked." appears.
  - Step 7:
    - The menu bar icon goes away.
    - `dns-sd -B` prints a `Rmv` line for the Mac app's instance; shutdown is bounded at about 5 s.
    - Neither `discover` nor the iPhone lists the Mac any more.

  **Record:** each notification's text, whether the offer notification had any action besides opening Dropumi, how long `Rmv` took, and any step that differed. If no "Files received" notification appeared, record whether the panel was still open or Dropumi was active at the end. Turn auto-accept off afterwards.

- [ ] **SHARE-4** Drag files onto the window. Partner: the paired iPhone. Steps: 1) Run `open "$FILES"`. 2) Drag `small.txt` and `photo-like.bin` onto Nearby's "Drop files here to send". 3) Pick the iPhone's chip, click "Send to ‹name›", and accept on the iPhone. 4) Drag a file onto the Send screen itself. 5) Drag the `many` folder onto the drop zone. 6) Drag a file onto the Dock icon.
  **Expect:**
  - Step 2: Send opens with both files and the header "2 files · ‹size› ready to go".
  - Step 3: the send completes.
  - Step 4: the file is added.
  - Step 5: folders are not filtered out, and what happens next is not confirmed in code: check and note.
  - Step 6: probably refused, because the Mac app declares no document types. Not confirmed: check and note.

  **Record:** the result of each step.

- [ ] **PERM-1 (Mac half)** With Local Network denied, the app says so; after turning it back on, discovery works. Partner: the iPhone, in front (it advertises). Keep the Mac's Wi-Fi on for the whole row. Steps: 1) Turn Dropumi off under System Settings › Privacy & Security › Local Network. 2) Open Nearby on the Mac. If nothing changes, quit completely and relaunch; whether a relaunch is needed is not confirmed. 3) Click "Open Settings". 4) Turn Dropumi back on. 5) Go back to Nearby.
  **Expect:**
  - Step 2: Nearby shows "Local network access is off" / "Dropumi needs it to find and reach nearby devices. Turn on Local Network for Dropumi in Settings." with "Open Settings", and the iPhone is not listed.
  - Step 3: System Settings opens at Privacy & Security › Local Network.
  - Step 5: the card goes away once a device is found, and the iPhone is listed.
  - The app shows this card whenever a discovery problem's text contains "local network". That covers the Bonjour policy denial. It also covers the advertising error raised when receiving starts with no IPv4 address ("no IPv4 address on a local network"). So the card is a false positive when it appears with Wi-Fi off, for example in the NET rows: record that case as the false positive, not as a pass.
  - If, with Wi-Fi on and access denied, you see "Nearby devices can't be found" plus a detail, or only "Looking for devices…", record the exact text as Failed.

  **Record:** whether a relaunch was needed, the texts you saw, and whether the iPhone could still see the Mac or send to it while access was denied.

- [ ] **SPOOF-1** A device renamed to a paired device's name shows as "Unverified device … claims …" and never as the paired device. Partners: the iPhone (paired with the Mac) and the CLI as the spoofer, with its own throwaway config. Use the iPhone's name exactly as its row title in the Mac's Nearby list. Pairing stored that name as the alias. If someone renamed the alias, either rename it back, or expect the subtitle "Looks like ‹alias›" to differ from the titles. Steps:
  1) Start the spoofer with the first three commands below.
  2) With the iPhone app in front, look at the Mac's Nearby.
  3) On the Mac, click "Send" on the spoofer's row, add `small.txt` and click Send. Read the "Check the code" screen, then click "Codes don't match".
  4) Stop the spoofer with Ctrl-C. Set the Mac to "Everyone · 10 min". Send from the spoofer, answer `y`, read the Mac's sheet and click "Decline".
  5) Repeat step 4 with the window closed and Finder active.
  6) Set the Mac to "Paired only" and send from the spoofer again.
  7) Remove the spoofer's config (last command).

  ```sh
  SPOOF="$(mktemp -d)"
  "$DROP" --config-dir "$SPOOF" identity set-name "<the iPhone's row title in the Mac's Nearby>"
  "$DROP" --config-dir "$SPOOF" receive --dir "$RX" --policy everyone
  "$DROP" --config-dir "$SPOOF" send "$FILES/small.txt" --to <Mac's Wi-Fi address>:<port> --allow-unverified
  rm -rf "$SPOOF"
  ```
  **Expect:**
  - Step 2: two rows with the same title. The real iPhone shows "Looks like ‹alias›"; the spoofer shows "Not paired". Neither says "Paired".
  - Step 3:
    - The Send hint reads "This device isn't paired. You'll both see a code to compare before anything is sent.".
    - "Check the code" shows "Unverified device", `claims “‹the iPhone's name›”`, "Not paired" and a different fingerprint.
    - Afterwards history shows "Codes did not match" and nothing was sent.
  - Step 4:
    - The sheet shows "Unverified device", `claims “‹the iPhone's name›”`, "Not paired", the fingerprint and the code.
    - The alias is never the title, and "Paired" never appears.
  - Step 5: the notification reads "An unverified device wants to send you 1 files.".
  - Step 6: the send is refused with "A device was refused", subject to the 60-second limit.

  **Record:** what each screen showed. Never record names or fingerprints.

- [ ] **KILL-1** Force-quit the receiver during a transfer: the sender reports a lost connection, and the stale `.part` file is gone at the next start. Partner: the paired iPhone, or the CLI. Steps: 1) Send `large-2048MiB.bin` from the iPhone and accept on the Mac. 2) Partway through, force-quit with Apple menu › Force Quit… › Dropumi. `pkill -9 -x Dropumi` should also work, but that is an inference. 3) List the receive folder. 4) Wait at least 15 minutes after the kill, then relaunch the app. 5) List the folder again.
  ```sh
  (cd ~/Downloads/Dropumi && ls -lAn)
  ```
  **Expect:**
  - The iPhone's Transfers › "Earlier" gets a row with the Failed icon, with the subtitle "Sent to ‹alias› · ‹time›". The second line is "Connection lost", or another headline such as "No answer". There is no banner and no longer message text.
  - A CLI sender fails with `connection_lost` and exit 5.
  - Step 3 may show a hidden `.dropumi-<12 letters and digits>.part` file.
  - Step 5 no longer shows it. The core removes stale `.part` files when receiving starts, but only once they are older than 15 minutes, so a sooner relaunch keeps the file.

  **Record:** the sender's second line, whether a `.part` file existed and its size, the minutes between the kill and the relaunch, and the result.

- [ ] **UI-1** VoiceOver, the largest text size, and keyboard only. Partner: the iPhone, which sends the offers.
  - **VoiceOver steps:**
    1) Turn VoiceOver on with ⌘F5.
    2) Move through the sidebar ("Nearby", "Transfers", "Settings"), the Nearby rows ("Send to ‹name›") and the menu bar icon ("Dropumi").
    3) Focus Nearby's "…" button ("More") and press VO-Shift-M (Open Menu).
    4) Under Settings › "Paired devices", focus a row's "…" ("Actions") and press VO-Shift-M.
    5) Have the iPhone send an offer, and read the sheet. Also read a code prompt, from pairing or TX-3.
    6) In Transfers, read × ("Cancel") and the history icons.
  - **Text size steps:**
    7) Set the largest text size the Mac offers, under System Settings › Accessibility › Display. Whether the Mac app follows it is not confirmed in code: check and note. Check Nearby, Settings and the offer sheet.
  - **Keyboard steps:**
    8) Turn on System Settings › Keyboard › "Keyboard navigation".
    9) Try ⌘1, ⌘2, ⌘3, ⌘O and ⌘, and Tab through each screen.
    10) Have the iPhone send offer 1. On its sheet, press Return, then Escape.
    11) Have the iPhone send offer 2. Wait until "Accept" is no longer dimmed, Tab to it and press Space.

  **Expect:**
  - Step 3 opens a menu with "Choose files to send", "Connect by address" and "Pair a device". This was fixed in 45e6baa; before that, VoiceOver could not open the menu.
  - Step 4 offers "Rename…" and "Remove…".
  - VoiceOver reads the code digit by digit, like "Verification code: 1 2 3, 4 5 6".
  - ⌘1, ⌘2 and ⌘3 open Nearby, Transfers and Settings.
  - ⌘O ("Send Files…") opens Send and then the file picker; ⌘, opens Settings.
  - Step 10: Return does nothing (there is no default button). Escape declines offer 1 and the sheet closes. On a code prompt, Escape presses "Codes don't match".
  - Step 11: Space on "Accept" accepts offer 2.

  **Record:** anything unlabelled, unreachable or clipped, separately for VoiceOver, text size and keyboard.

- [ ] **UI-4** Mac: open at login, and a chosen receive folder survives a relaunch. Partner: the paired iPhone. Steps:
  0) The logout ends every Terminal shell, so you lose `XG`, `DROP`, `DROPUMI_CONFIG_DIR` (the CLI config paired in PAIR-1), `RX`, `FILES` and `UNPAIRED`. Before you log out, print them and write the values down locally, not in the run file. They are temp paths.
  ```sh
  echo "$XG" "$DROPUMI_CONFIG_DIR" "$RX" "$FILES" "$UNPAIRED"
  mkdir ~/Desktop/dropumi-ui4
  ```
  1) Launch the copy in `/Applications`. Turn Settings › "Open at login" on and check that it stays on; the toggle shows the real state and can flip back.
  2) Log out and log in.
  3) Check the menu bar icon and panel. Note whether a window appears.
  4) Have the iPhone send `small.txt`, and accept through the panel › "Someone wants to send you files".
  5) Choose Settings › "Save received files to" › "Change…", pick the empty folder `~/Desktop/dropumi-ui4` (under your home folder, not on another volume), then click "Use this folder".
  6) Quit completely, relaunch, and open Settings.
  7) Have the iPhone send a file, then click "Show in Finder".
  8) Clean up. In the `/Applications` copy, the one that turned it on, turn "Open at login" off. Quit completely and check System Settings › General › Login Items: Dropumi must be gone. Remove the saved folder choice and the folder (inference):

  ```sh
  defaults delete ~/Library/Containers/io.github.dropumi.Dropumi/Data/Library/Preferences/io.github.dropumi.Dropumi receiveFolderBookmark
  rm -rf ~/Desktop/dropumi-ui4
  ```
  9) Before any later CLI or build step, open Terminal and set the variables again from your notes. Whether temp folders survive a logout is not confirmed. If one is gone, make it again; a new CLI config folder means the CLI must pair again (PAIR-1).

  ```sh
  cd "<APPLE worktree>"
  export PATH="<noted XG>/xcodegen/xcodegen/bin:$PATH"; xcodegen --version
  DROP="$PWD/target/release/dropumi"
  export DROPUMI_CONFIG_DIR="<noted path>"; RX="<noted path>"; FILES="<noted path>"; UNPAIRED="<noted path>"
  ls -d "$DROPUMI_CONFIG_DIR" "$RX" "$FILES" "$UNPAIRED"
  ```
  **Expect:**
  - Step 3: the icon is there, and the panel shows the device name, "Paired devices only" and the control "Paired only" | "Everyone" | "Off". It does not stay on "Starting…".
  - Step 4: the file is received.
  - Step 5:
    - The app reloads briefly: running transfers are cancelled and Everyone falls back to Paired only.
    - Settings shows the new folder as "~/Desktop/dropumi-ui4".
  - Step 6: Settings still shows the chosen folder.
  - Step 7: the file lands there and Finder shows it.
  - Step 8: Login Items no longer lists Dropumi.

  **Record:**
  - Pass or fail for open at login, and separately for the folder.
  - Whether a window appeared at login.
  - Whether the Settings path started with `~` (expected) or showed a full path, which is Failed. Write only "~" or "full path", never the path itself.
  - Whether Login Items was clean after step 8.

- [ ] **MAC-UI** The Mac UI state tests. They are automated: you only start them and approve once. Steps: 1) Quit the Release app completely. This avoids two running copies with one bundle ID (inference). 2) Run the first command block. 3) Approve UI automation the first time macOS asks. 4) Do not touch the mouse or keyboard while the tests run: they move the pointer and open their windows on the built-in display. They run a Debug build on the fake core, with temporary folders and their own settings, and include the PROMPT-1 tests and accessibility audits. 5) After MAC-UI and MAC-BONJOUR, list the result bundles and delete them without opening them. Earlier sessions may have left bundles there too.
  ```sh
  cd "<APPLE worktree>"
  xcodebuild test -project apps/apple/Dropumi.xcodeproj -scheme Dropumi-macOS \
    -destination 'platform=macOS,arch=arm64' -derivedDataPath apps/apple/build
  ```
  ```sh
  cd "<APPLE worktree>"
  ls apps/apple/build/Logs/Test/
  find apps/apple/build/Logs/Test -maxdepth 1 -name '*.xcresult' -exec rm -rf {} +
  ```
  **Expect:** the summary lines report no failures. The last recorded run had "15 tests, with 0 failures" for the UI tests and "7 tests in 3 suites passed" for the hosted tests. xcodebuild writes `Test-Dropumi-macOS-<date>.xcresult` bundles to `apps/apple/build/Logs/Test/`. **Record:** `Automated test passed`, `Failed` or `Not tested`, with the pasted summary lines and the commit. Never share or commit the `.xcresult` bundles, because they record the main display.

- [ ] **MAC-BONJOUR** The CLI and the Mac app find each other over Bonjour. Automated. Steps: 1) Quit the Release app. 2) Check that `xcodegen --version` prints 2.46.0. The script always runs `xcodegen generate`. If the version is wrong or the command is missing, run `export PATH="<noted XG>/xcodegen/xcodegen/bin:$PATH"` again, or repeat the install block. 3) Run the command below. 4) Choose Allow if macOS asks about Local Network access. 5) Delete the new `.xcresult` bundles as in MAC-UI step 5. This test also runs xcodebuild with `-derivedDataPath build` from `apps/apple`.
  ```sh
  cd "<APPLE worktree>"
  xcodegen --version
  DROPUMI_TEST_MDNS=1 bash scripts/apple/interop.sh apple_macos_bonjour
  ```
  **Expect:** a line that starts with `test result: ok. 2 passed;`. If the Rust core is out of date, the script rebuilds it with `--profile dev`. In that case, run `bash scripts/ffi/build-apple.sh --profile release` before the next Release build. **Record:** `Automated test passed`, `Failed` or `Not tested`, with the pasted `test result` line and the commit.

- [ ] **PROMPT-1 (extra)** A press held from before Accept is enabled must not answer. This is not a TEMPLATE row. Sources: commits f642ef7 and 94a293f; `apps/apple/Tests/DropumiMacUITests/MacUIStateTests.swift` (automated on the fake core only, inside MAC-UI); `apps/apple/Dropumi/Shared/App/PromptPresenter.swift`. Partner: the paired iPhone, with auto-accept off on the Mac. Steps:
  1) Have the iPhone send `small.txt`. Note where "Accept" appears on the sheet, then click "Decline".
  2) Rest the pointer on that spot.
  3) Have the iPhone send again. As soon as the sheet appears, press and hold the trackpad or mouse button on "Accept" for about 2 seconds, well past the 600 ms delay. Then release.
  4) If the sheet is still open, click "Accept" normally.
  5) Repeat steps 2 to 4 on a code prompt ("Codes match"), during pairing or TX-3.
  6) Send once more and press Return repeatedly as the sheet appears.

  **Expect:**
  - Step 3: the release does not answer. The sheet stays, "Expires in m:ss" keeps counting, the Mac's Transfers card reads "Waiting for your answer…", and the iPhone keeps waiting.
  - Step 4: a normal click after the delay accepts.
  - Step 5: "Codes match" behaves the same way.
  - Step 6: Return never answers.

  **Record:** whether your press started while the button was still dimmed (by eye), and the result for "Accept" and for "Codes match". A single accidental answer means Failed. The automated half takes its label from MAC-UI.

- [ ] **REL-1 (extra)** A Release build contains no fake-core or UI-test-scenario symbol. This is not a TEMPLATE row. Source: threat model G36 ("a Release build contains no `FakeCoreClient` or `UITestScenario` symbol (`nm`, PR evidence)"). Partner: none. Steps: 1) After the Release build, run the commands below.
  ```sh
  cd "<APPLE worktree>"
  nm apps/apple/build/Build/Products/Release/Dropumi.app/Contents/MacOS/Dropumi | wc -l
  nm apps/apple/build/Build/Products/Release/Dropumi.app/Contents/MacOS/Dropumi | grep -c -e FakeCoreClient -e UITestScenario
  ```
  **Expect:** the first count is above 0, so `nm` found symbols. The second count is 0. **Record:** both counts and the commit.

- [ ] **EXPIRE-1 (extra)** An unanswered offer expires without an answer. This is not a TEMPLATE row. Sources: threat model G34 ("at zero it reads "Expired""); `PromptPresenter.swift` (Accept is disabled at zero; the prompt goes away only when the core closes it). Partner: the paired iPhone, with auto-accept off on the Mac. Steps: 1) Have the iPhone send `small.txt`. 2) Do not touch the sheet for more than 2 minutes. 3) Watch the countdown, the "Accept" button and when the sheet closes. 4) Read both history rows.
  **Expect:**
  - "Expires in m:ss" counts down from about 2:00.
  - At zero the text reads "Expired" and "Accept" is disabled. Nothing is answered.
  - The sheet stays until the core closes it. The core's own 2-minute limit can close it at about the same moment, so whether "Expired" is visible at all is not confirmed in code: check and note what you see.
  - The history second line on each side is not confirmed in code ("Request expired" is the app's text for a timed-out request): check and note what you see.

  **Record:** whether "Expired" appeared, whether Accept was disabled, when the sheet closed, and both history lines.

- [ ] **TRUST-1 (extra)** Trust by full fingerprint, and a fingerprint pin that does not match. This is not a TEMPLATE row. Sources: AGENTS.md §7 (trust comes only from SAS pairing or a manual add with the full fingerprint); `apps/apple/Dropumi/Shared/Screens/TrustedDevicesScreen.swift` ("Add by fingerprint", "Added by fingerprint"); `ThisDeviceScreen.swift` (the full fingerprint); `apps/apple/Dropumi/Shared/Screens/ConnectByAddressScreen.swift`; `docs/cli.md` (`trust`, identity binding). Partner: the CLI (same Mac) with its own throwaway config, never paired with the Mac. Steps:
  1) Make the throwaway identity and show it (first block).
  2) On the Mac, choose Settings › "Paired devices" › "Add by fingerprint". Type the CLI's `Full:` value in "Fingerprint (52 characters)", and its `Name:` value (a default `Dropumi-XXXX` name) in "Name (optional)". Watch "Add" before you type the last character. Click "Add".
  3) Read the Mac's full fingerprint under Settings › This device › FINGERPRINT. Add it on the CLI (second block).
  4) Empty `$RX` and start the CLI receiver (third block). Read the port from `Listening on <addr> as "<name>"`.
  5) On the Mac, choose Nearby "…" › "Connect by address". Type `<Mac's Wi-Fi address>:<port>` with the CLI's port. In the fingerprint field, type the first 24 characters of the CLI's `Fingerprint:` line. Click "Choose files" and pick `small.txt` from `$FILES`. Click "Send files" and answer `y` in Terminal. Check the hash (fourth block).
  6) Empty `$RX` and start the receiver again (third block). Repeat step 5, but change one letter of the fingerprint to another letter (A to Z). When the Mac has finished, stop the receiver with Ctrl-C and run `ls -A "$RX"`.
  7) Optional, the other way: send from the CLI to the Mac app with the first 24 characters of the Mac's fingerprint, and accept on the Mac. Then send again with one letter changed (fifth block).
  8) Clean up: on the Mac, remove this CLI under "Paired devices" ("…" › "Remove…" › "Remove"). Then remove its config (last block).
  ```sh
  MANUAL="$(mktemp -d)"
  "$DROP" --config-dir "$MANUAL" identity show
  ```
  ```sh
  "$DROP" --config-dir "$MANUAL" trust add <Mac FP52>
  ```
  ```sh
  find "$RX" -mindepth 1 -delete
  "$DROP" --config-dir "$MANUAL" receive --dir "$RX" --once; echo "exit $?"
  ```
  ```sh
  (cd "$RX" && shasum -a 256 small.txt); grep 'small.txt' "$FILES/SHA256SUMS"
  ```
  ```sh
  "$DROP" --config-dir "$MANUAL" send "$FILES/small.txt" --to <Mac's Wi-Fi address>:<port> --expect-fingerprint <first 24 characters of the Mac's fingerprint>; echo "exit $?"
  ```
  ```sh
  rm -rf "$MANUAL"
  ```
  **Expect:**
  - Step 2: "Add" stays dimmed until the field holds exactly 52 characters (dashes and spaces do not count). "Paired devices" then lists the name with "Added by fingerprint · XXXX-XXXX-XXXX-XXXX-XXXX-XXXX". Those 24 characters match the CLI's `Fingerprint:` line.
  - Step 3: the printed list shows the Mac with `(manual)`.
  - Step 5:
    - The hint reads "Dropumi checks this fingerprint before anything is sent.".
    - No code appears on either side. The Mac shows no "Check the code", and the Terminal prompt starts with `Accept 1 file(s) from Trusted`.
    - `receive --once` exits 0, and the two hashes are equal.
  - Step 6:
    - The send fails before anything is sent. Terminal shows no `Accept` prompt, and `$RX` stays empty.
    - The Mac's Transfers › "Earlier" row has the Failed icon. Its second line should be "Not the device you paired with", the app's headline for `peer_identity_mismatch` (not confirmed in code: check and note what you see).
    - `receive --once` keeps running, because a session that ends before an offer does not count.
  - Step 7:
    - With the right 24 characters, the Mac's sheet looks as in TX-1 (the name and "Paired", no code), and the CLI exits 0 after you accept.
    - With one letter changed, the CLI fails with `peer_identity_mismatch` and exit 6. No sheet appears on the Mac, and nothing new arrives in `~/Downloads/Dropumi`.

  **Record:** each step's result, the CLI exit codes, the hash result and the Mac's history second line for step 6. Never record fingerprints or names.

- [ ] **IDLE-1 (extra, optional)** Idle memory and CPU of the Mac app, with the window open and with it closed. This is not a TEMPLATE row. Source: ADR-0012 "Not measured: Idle memory and CPU of the Mac app with the window open and closed" (ADR-0008's M1 and M5). Partner: none. Steps: 1) Launch the Release app with the window open and leave it idle for 60 seconds. 2) Run the command below, wait 60 seconds, and run it again. 3) Close the window and repeat steps 1 and 2. This uses `ps` in the same way as `scripts/bench/loopback.sh` does for the CLI. The repo has no documented method for the app. Activity Monitor is an alternative: say which you used.
  ```sh
  ps -o rss=,time= -p "$(pgrep -x Dropumi)"
  ```
  **Expect:** no expected value; this is a measurement. **Record:** the RSS in KiB and the CPU time used over the 60 seconds, for the window open and closed. Label them "measured on this Mac (macOS 27.0.1, arm64), Release build, method: ps" with the commit.

### Pair: Mac ↔ Android phone (core set)

The core set (DISC-1, PAIR-1, TX-1, TX-3, CAN-1) runs on every pair, so it also runs between the Mac app and the Android phone. Use the Android section for the phone's own screens and for checking files on the phone. Keep Dropumi in front on the phone, because Android receives only while the app is open. The Android app does not advertise: it lists other devices, but other devices do not list it (ADR-0013 D5). So the Mac sends to the phone with Nearby "…" › "Connect by address". Use the address under the phone's Settings › This device. In the Pair column, write "Mac → Android phone" or "Android phone → Mac".

- [ ] **DISC-1** (Mac ↔ Android phone) One direction only. Steps: 1) Open Nearby on both, with the Mac on "Paired devices only". 2) Wait for each list. **Expect:** the phone lists the Mac. The Mac never lists the phone; that is expected today. **Record:** "Android sees Mac" as the result, and a note that the Mac not listing the phone is expected (ADR-0013 D5). If the Mac does list the phone, record that as a finding.

- [ ] **TX-3** (Mac ↔ Android phone) Everyone, unverified sender, while they are unpaired. Steps: 1) Set the Mac to "Everyone · 10 min". Send `small.txt` from the phone to the Mac from the phone's list, compare the codes, and click "Accept" on the Mac. 2) Put the phone on Everyone. On the Mac, choose "Connect by address", type `<phone's address>:<port>` with an empty fingerprint field, and send `small.txt`. Compare the codes on both. **Expect:** the Mac's sheet in step 1 shows the same items as in TX-3: "Unverified device", `claims “‹name›”`, "Not paired", the fingerprint and the code. In step 2 the Mac shows "Check the code" before anything is sent. The codes match on both sides, and neither device gains a paired device. The countdown defect from TX-3 shows here too. **Record:** both directions, and whether the codes matched.

- [ ] **PAIR-1** (Mac ↔ Android phone) Pair in both directions, so that each side starts once. Steps: 1) On the Mac, choose Settings › "Pair a device" › "Wait for a device". 2) On the phone, start pairing with the Mac from its Settings › Pair a device. 3) Compare the codes and confirm on both. 4) Other direction: remove the pairing on both sides (on the phone, as the Android section describes). 5) On the phone, choose Settings › Pair a device › "Wait for a device". The phone shows "Waiting for a device · m:ss". Below "Other devices can't see this one nearby. On the other device, choose Connect by address and type:", it shows an address. The caption may instead read "If it doesn't show up there, choose Connect by address on the other device and type:". 6) On the Mac, choose Nearby "…" › "Connect by address". Type that address, leave the fingerprint field empty, and click "Pair with this address". Compare the codes and confirm on both. **Expect:** in both directions, the Mac's code prompt is as in PAIR-1: "Pair with this device?", "Unverified device", claims “‹name›”, "Not paired", the fingerprint, the code and the countdown. Both end paired each time, and the Mac's "Paired devices" lists the phone with "Paired with a code · …". **Record:** which side waited in each run, both results, and whether the fingerprints matched (yes or no).

- [ ] **TX-1** (Mac ↔ Android phone) Paired, with a prompt; SHA-256 equal. Steps: 1) Empty the Mac's receive folder. Send `small.txt` and `photo-like.bin` from the phone, accept on the Mac, and run the Mac hash check (it should print 2). 2) On the Mac, use "Connect by address" with the phone's address and send the same two files from `$FILES`. Accept on the phone, and check the hashes as the Android section describes. **Expect:** the Mac's sheet shows "Incoming files", the alias and "Paired", with no code. The Mac → phone send shows no code, because the phone is paired. The hashes match in both directions. **Record:** both directions, the hash results and the times.

- [ ] **CAN-1** (Mac ↔ Android phone) The sender cancels. Steps: 1) Send `large-2048MiB.bin` from the phone, accept on the Mac, and cancel on the phone partway through. List the Mac's receive folder with `ls -A ~/Downloads/Dropumi`. 2) Send it from the Mac to the phone by address and click × on the Mac's Transfers card partway through. **Expect:** in step 1, Mac history shows "Cancelled by the other device", with no partial file and no `.dropumi-*.part` file. In step 2, the Mac shows "Cancelled by you". For the phone's side, see the Android section. **Record:** both results and the `ls` result.

### Clean up

After the last row of this section, remove the CLI from the Mac's "Paired devices". Then remove the CLI folders this section made. Run this in a shell where the variables are set (see UI-4 step 9). Check the paths with `echo` first.

```sh
echo "$DROPUMI_CONFIG_DIR" "$RX"
rm -rf "$DROPUMI_CONFIG_DIR" "$RX"
```

`$FILES` and `$UNPAIRED` belong to the common part, and other sections use them. Remove them only when the whole session is over:

```sh
rm -rf "$FILES" "$UNPAIRED"
```
