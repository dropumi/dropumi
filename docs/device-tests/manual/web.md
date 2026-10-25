# Manual device testing: Web research pages

Read the [common page](README.md) first: privacy, test files, the CLI as a test peer, and the session plan.

Dropumi has no browser feature, so there is no browser companion to test. These pages are research for ADR-0011, which has the status Proposed ("Nothing ships, and this ADR stays Proposed"). Every page shows the banner "Research prototype. Not a Dropumi feature, not secure, not deployed". The rows below measure what a page served on the LAN can do in each browser. They are research measurements, not Dropumi results. They never change a label in `docs/testing.md`. The "Browser companion" label reads "Not tested (no browser path exists; research measurements only)" in WEB `docs/testing.md:225`. In APPLE `docs/testing.md:236` and ANDROID `docs/testing.md:248` it reads `Not tested` ("Proposal and research prototype only").

- **IDs:** WEB-1 to WEB-4 are ADR-0011's row IDs. No TEMPLATE row covers the web pages, so these are not TEMPLATE rows. Checks that are not ADR-0011 rows are marked "extra".
- **Where to record:** record the results with the web research, in ADR-0011's measured table on the WEB branch (`docs/web-local-page-research`). That table is at WEB `docs/adr/0011-browser-companion.md:50-64`. Both pages label their Result "copy into ADR-0011". The earlier run file keeps browser research out of it (APPLE `docs/device-tests/runs/2026-10-18-01.md:4-5`: "recorded with their own work"). If you also note the session in a run file, use a separate "Web research (ADR-0011)" part and never TEMPLATE rows.
- **Labels:** use `Manual device test passed`, `Failed` (say what happened) or `Not tested`. Rows that come from `run-matrix.py` are recorded as `Automated test passed` or `Failed`, as ADR-0011 does. The pass rules are under "Reading the summary".
- **WEB-3:** give each part its own label, as ADR-0011 does: "the click-through passed; the fingerprint check failed".
- **Commit tested:** record the WEB worktree's commit. It must include a1e943b, because before that fix `--https --cert --key` served plain HTTP and run-matrix's WEB-3 rows could not load.

```sh
git -C "<WEB worktree>" rev-parse --short HEAD
git -C "<WEB worktree>" merge-base --is-ancestor a1e943b HEAD && echo "includes a1e943b"
```

## Run it

Only the WEB worktree has `serve.py`, `probe.html` and `run-matrix.py`. Run everything from that worktree's root. The copies of `poc/web/` in the APPLE and ANDROID worktrees are older and hold only `README.md` and `index.html`.

Do not use the `python3 -m http.server` command in `poc/web/README.md`. That README is out of date. `http.server` has no `/result`, `/signal`, `/upload` or `/download`, so the speed test, auto mode and result saving all fail.

In every URL and command below, `<mac-wifi-ip>` is the Mac's Wi-Fi address. It is the address serve.py uses, from `ipconfig getifaddr en0`. Never write the real value into anything you record.

**Before you start**
- Put the Mac, the iPhone and the Android phone on the same Wi-Fi network, with VPN off.
- Record the network: the band, whether all devices share one subnet, and whether client isolation is on or off (README `:39`).
- **Versions:** record each OS version and browser version by hand. Do not take them from `userAgent`. In the earlier run's `results.jsonl`, `userAgent` showed the wrong OS version on every device:
  - the iPhone on iOS 27.0.1 reported `iPhone OS 18_7`;
  - the arm64 Mac on macOS 27.0.1 reported `Intel Mac OS X 10_15_7`;
  - Android Chrome reported `Android 10; K`.
- **Safari's version** does appear in `userAgent` (`Version/27.0.1`).
- **Chrome and Brave** both reported `Chrome/155.0.0.0`, the same string. Read their full versions in the browser instead, for example at `chrome://version` and `brave://version`. These pages are not in the repo.
- **Telling Chrome from Brave:** on WEB-1, `brands` read `Google Chrome 155` or `Brave 155`. On WEB-2 (plain http), `brands` was `null` in both, so only `"brave"` tells them apart.
- Make one results folder for the session. Keep it outside `/tmp`, because a restart wiped `/tmp`, and outside the repository.
- serve.py `--https` and every `run-matrix.py` run need the `openssl` command.

Go to the WEB worktree, which is the checkout of `docs/web-local-page-research`, and make the results folder. Paste the commands without comments: zsh does not treat `#` as a comment when you type commands at the prompt.

```sh
cd "<WEB worktree>"
mkdir -p "<results folder>"
```

**Start serve.py.** Use one terminal for each row you are running.

| Row | Command | Open |
|---|---|---|
| WEB-1 (localhost) | `python3 poc/web/serve.py --bind 127.0.0.1 --results "<results folder>"` | `http://localhost:8000/` (probe), `http://localhost:8000/index.html` (WebRTC) |
| WEB-2 (LAN, http) | `python3 poc/web/serve.py --results "<results folder>"` | `http://<mac-wifi-ip>:8000/`, `http://<mac-wifi-ip>:8000/index.html` |
| WEB-3 (LAN, self-signed https) | `python3 poc/web/serve.py --https --results "<results folder>"` | `https://<mac-wifi-ip>:8443/`, `https://<mac-wifi-ip>:8443/index.html` |

- **Address:** without `--bind`, serve.py listens on the en0 address. If en0 has no address, it exits with `error: no Wi-Fi address on en0; pass --bind`.
- **Ports:** `--port` sets the port. The default is 8000, or 8443 with `--https`. The WEB-2 server does not answer on `localhost`. WEB-1 and WEB-2 can both run on port 8000 at the same time.
- **WEB-1 address:** type `localhost`, not `127.0.0.1`. With `127.0.0.1`, the Result labels the page `<lan-host>`.
- **The WEB-3 certificate:**
  - It is a throwaway EC P-256 certificate (`CN=dropumi-research`), valid for one day. It names only the IP address (`subjectAltName=IP:<address>`). Open it by IP, not by a `.local` name.
  - Each start makes a new certificate with a new fingerprint. Keep one WEB-3 server running for the whole session. If the Mac's address changes, restart it.
  - `--https --cert <pem> --key <pem>` reuses an existing certificate. Without `--https`, both flags are ignored and the server is plain HTTP on port 8000. With `--https` and only one of the two flags, serve.py makes a new certificate.
  - A certificate you pass in should name the Mac's current Wi-Fi address, as serve.py's own certificate does. serve.py does not check this.
  - Ctrl-C deletes only a certificate folder that serve.py made itself. Files passed with `--cert` and `--key` stay where they are.
- **Saving results:**
  - With `--results`, every Result the pages post is added as one line to `<results folder>/results.jsonl`.
  - Without `--results`, serve.py refuses the post with a 400, and the pages do not show it.

**What serve.py prints.** On stdout, in this order:
1. Only with `--https`: `certificate SHA256 Fingerprint=AA:BB:…`. This is the SHA-256 of the certificate as colon-separated hex. The case of "SHA256" depends on the openssl build. This is the value you compare in the browser.
2. `serving <scheme>://<host>:<port>/ (probe) and /index.html (WebRTC); Ctrl-C stops`
3. `research prototype: not secure, not a Dropumi feature; never paste this log`

After that, the access log on stderr prints one line per request with the client's address.

If the log shows `Bad request syntax \x16\x03\x01`, a browser spoke TLS to a plain-HTTP port. Check the scheme and the port. Also check that the server was started with `--https`: `--cert` and `--key` without it give plain HTTP on 8000. Before a1e943b, `--https --cert --key` also served plain HTTP. That commit fixed it.

**The macOS firewall question.** Nothing in `poc/web/` covers it. The firewall row in `docs/development.md` is at `:292` in WEB and APPLE and at `:367` in the ANDROID working tree. That row is about the CLI's listener (`receive` / `pair --listen`, port 47321) and says only that firewall behaviour is UNVERIFIED and Not tested. Nothing covers serve.py.
- serve.py is a Python process that listens on the Wi-Fi address.
- If the macOS Application Firewall is on, macOS may ask whether Python may accept incoming connections (not confirmed in code: check and note what you see).
- If the connection is blocked, the phones cannot load the pages.
- Accepting inbound TCP does not need macOS Local Network privacy (`docs/feasibility.md:67`, macOS column).

To check the firewall state, use this macOS command (it is not in the repo):

```sh
/usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate
```

Record the firewall state, whether a prompt appeared, what it said, and what you answered.

**The Local Network permission (Mac browsers).** The same feasibility cell says "outgoing connections do" need the Local Network privilege. `docs/development.md:293` (ANDROID `:368`) says macOS 15 and later gate local network access behind System Settings > Privacy & Security > Local Network.
- These are outgoing connections:
  - Mac Safari, Chrome or Brave loading `http(s)://<mac-wifi-ip>` (WEB-2, WEB-3, WEB-4);
  - Mac Chrome or Safari opening a DataChannel to a phone (WEB-4).
- These browsers may ask for this permission or need it (not confirmed in code: check and note what you see). It is also not confirmed whether it applies to an address on the same Mac.
- In the earlier run, all three Mac browsers loaded the page at the Mac's address (ADR-0011 WEB-2).
- A denied permission can look like a firewall or client-isolation failure.
- Before WEB-2, record whether Google Chrome, Brave Browser and Safari are listed in that pane and whether each is allowed. During WEB-2 and WEB-4, record any prompt and your answer.

**Open the pages by hand**
- **Mac:** type the URL in Safari, Chrome or Brave.
- **iPhone Safari and Android Chrome:** type the URL on the phone. Keep both phones unlocked with the screen on.
- **Tags:** add `?tag=<row>-<where>-<browser>-manual`, for example `?tag=WEB-2-iphone-safari-manual`.
  - A tag can use letters, digits and hyphens, up to 64 characters.
  - Never put a device name in a tag.
- **Optional, from the Mac:** these are the commands `run-matrix.py` uses to open a phone's browser.
  - The `adb` URL needs the inner single quotes, because the phone's shell would otherwise cut a URL at `&`.
  - The wake key never unlocks the phone's PIN.
  - Replace only `<UDID>`, `<serial>` and `<mac-wifi-ip>`. Leave the quotes as they are.

```sh
xcrun devicectl device process launch --device <UDID> --payload-url 'http://<mac-wifi-ip>:8000/?tag=WEB-2-iphone-safari-manual' com.apple.mobilesafari
adb -s <serial> shell input keyevent KEYCODE_WAKEUP
adb -s <serial> shell am start -a android.intent.action.VIEW -d "'http://<mac-wifi-ip>:8000/?tag=WEB-2-android-chrome-manual'" -p com.android.chrome
```

**Automated run (run-matrix.py).** It covers:
- WEB-1 on Mac Safari, Chrome and Brave;
- WEB-2 on the same three browsers, plus iPhone Safari and Android Chrome;
- WEB-3 with the key pinned, on Mac Chrome and Brave only. This stands for a tester who checked the fingerprint;
- WEB-4 for four pairs over http on port 8000, 64 MiB each, with serve.py relaying the offer and the answer.

It does not cover the WEB-3 click-through, the Files section, copy and paste, any Mac Safari offer, or anything on Firefox.

What it needs:
- **Google Chrome and Brave**, at exactly `/Applications/Google Chrome.app` and `/Applications/Brave Browser.app`.
  - If Brave is missing, the script stops with a Python traceback at the first Brave probe.
  - Chrome is needed for its Chrome probes and for the three pairs that start on Mac Chrome.
  - To skip Brave, pass `--only WEB-1-mac-safari,WEB-1-mac-chrome,WEB-2-mac-safari,WEB-2-mac-chrome,WEB-2-iphone,WEB-2-android,WEB-3-mac-chrome,WEB-4`.
- **Commands on PATH:**
  - `openssl` (always; it makes a certificate on every run);
  - `swift` (Xcode);
  - `xcrun devicectl` (for `--iphone`);
  - `adb` (for `--android`).
- **A Wi-Fi address on en0.** Without one, the script stops with a Python traceback, not with serve.py's message.
- **The iPhone's UDID:** get it from `xcrun devicectl list devices` (the iPhone must be paired with the Mac).
- **The Android serial:** get it from `adb devices`. The phone counts only when its state reads `device`, which needs USB debugging authorized on the phone. Never paste either listing: they show device names and serials.

Before you run it:
- Stop every serve.py you started by hand. run-matrix.py starts its own three servers on ports 8000 (127.0.0.1 and the Wi-Fi address) and 8443.
- Make sure the Mac's built-in display is active. All Mac windows open there. Without it, the script exits with `error: no built-in display; this runner only opens windows there`. The same message appears if `swift` cannot run.
- macOS asks once for permission to control Safari. Allow it.
- Keep both phones unlocked with the screen on. In the earlier run, the first WEB-4 attempt got no answer while the Android phone's screen was off.

```sh
python3 poc/web/run-matrix.py --results "<results folder>" --iphone <UDID> --android <serial>
python3 poc/web/run-matrix.py --results "<results folder>" --iphone <UDID> --android <serial> --only WEB-3,WEB-4
```

- **`--results`** is required.
- **`--only`** takes comma-separated prefixes of `<row>-<where>-<browser>`, for example `WEB-2-android` or `WEB-3-mac-brave`. For pairs, use `WEB-4-<pair>`. The pair names are `macchromemacsafari`, `macchromeiphone`, `macchromeandroid` and `iphoneandroid`.
- **Waiting:** each probe waits 90 s for a record with `speed`, and each pair waits 150 s.
- **Clean-up:** it deletes the throwaway Chrome and Brave profiles and its key folder, and closes its Safari windows. It does not close the phone tabs, so close those by hand.
- **The certificate:** it makes its own certificate and never prints a fingerprint.
- **Phone launch failures are silent.** The `devicectl` and `adb` calls do not stop the script when they fail. A failed launch shows only as `no result`.

**If it prints `error: serve.py did not start:`** (with nothing after the colon), a port is usually in use, often by a serve.py you started by hand. The servers it had already started keep running, and its key folder stays behind. To recover:
1. List leftover servers with `pgrep -fl poc/web/serve.py`. Do not paste the output: it shows paths.
2. Stop them with `pkill -f poc/web/serve.py`.
3. Delete the leftover key folders with `rm -rf "$TMPDIR"/dropumi-matrix-*`.
4. Run it again.

**Reading the summary.** The summary has these columns:
- Probe rows: `row | where browser | result | isSecureContext | subtle | speed`.
- Pair rows: `WEB-4 | offer -> answer | result | connectionState | selectedPair | {sentBytes, seconds, mibPerSecond, errors}`.

The third column reads `result` whenever a matching record arrived, even for a failed pair. In the earlier run, a pair with `errors` and no `sentBytes` also showed `result`. Judge each row by the other columns:
- **Probe rows pass** when all of these hold:
  - `isSecureContext` and `subtle` are `True True` for WEB-1 and for the pinned WEB-3 rows, and `False False` for WEB-2;
  - `speed` shows `downloadBytes` and `uploadBytes` of `67108864`;
  - `speed` has no `error` key.
- **Pinned WEB-3 rows** also need the full capabilities checked in `results.jsonl`. ADR-0011 `:60` expects "A secure context, with the same APIs as WEB-1".
- **`no result in 90 s`** means that no record with a speed result arrived. Look in `results.jsonl` for the row's tag, which ends in the run's time stamp:
  - a record with `capabilities` but no `speed` means the page loaded but the speed test did not finish;
  - no record at all means the page never loaded or never posted.
- **Pair rows pass** when `connectionState` is `connected`, `sentBytes` is `67108864` and `errors` is `None`.
- **`no result in 150 s`** means that the offer page posted no record with `sentBytes`, `errors` or a `failed` state.

```sh
grep '"tag": "WEB-2-android-chrome-' "<results folder>/results.jsonl"
```

**Paste only the summary rows printed at the end.** Never paste a traceback or an error line. They can show the Mac's address (for example in the `openssl` command), a serial, and absolute paths of the checkout and `$TMPDIR`. The earlier run's first log was a traceback that showed the phone's serial and the Mac's address. Before you paste a summary row whose `errors` is not `None`, read the error text, as described under "Never paste".

**Stop the server.** Press Ctrl-C in each serve.py terminal. With `--https` and no `--cert`/`--key`, this also deletes the temporary certificate folder. run-matrix.py stops its own servers, unless it failed to start them (see above).

**Never paste:**
- **serve.py's output.** The `serving` line has the Mac's address, and the access log has every client's address. The `certificate` line has no address, but you only need to record "matched", "did not match" or "not found".
- **SDP offers or answers** from `index.html`. They contain local addresses. In the earlier run, Android Chrome's candidates did not hide their addresses behind `.local` names. Delete the SDP wherever you moved it.
- **Identifiers:** device names, serials, UDIDs or the Mac's address, whether in tags, in `--only` or in anything else you paste. This includes the `devicectl` and `adb devices` listings, the `pgrep` output and tracebacks.

You may paste Result blocks, `results.jsonl` lines and the run-matrix summary rows.
- Result blocks replace the host with `<lan-host>`.
- Candidates keep only the protocol, the type and a `.local` flag.
- Result blocks do include `userAgent`.
- **Check the `errors` text first.** `index.html` copies the browser's exception message into `errors` as it is, shows it in the Result and posts it. The page does not control that text. Whether a browser's SDP error can quote an SDP line is not confirmed. Before you paste a WEB-4 Result, a `results.jsonl` line or a summary row that has `errors`, read the error text. If it contains an address, a `.local` name or an SDP line, replace it with a short description.

## Checklist

**Probe procedure** (used by WEB-1, WEB-2 and WEB-3):
1. Open the probe URL with a tag. The capabilities run on load, show in the Result, and are posted to `results.jsonl`.
2. Press **Measure upload and download**. "Megabytes" defaults to 64 (1 to 512). The speed is measured to the Mac, not between devices.
3. Files section (optional):
   - Pick files with the file picker.
   - On the Mac, drop files on "Drop files here".
   - Press **Save a test file (Blob download)** and check whether `dropumi-probe.txt` arrived in Downloads.
   - Press **Share a test file**.
   - These controls post nothing. Their fields (`filePicker`, `dragAndDrop`, `blobDownload`, `share`) are added to the Result. They are posted only if you press **Run again** or **Measure upload and download** afterwards. To keep them, use **Copy the result**.
4. Press **Copy the result**. Outside a secure context it may read "Copy failed: select the text by hand". In that case, select the Result text by hand.

**What the automated run measured for WEB-1 and WEB-2** (2026-10-18, one run each):
- **WEB-1** (all three Mac browsers):
  - `"page": "http://localhost"` and `capabilities.isSecureContext: true`.
  - `true`: `subtle`, `subtleSha256`, `subtleX25519`, `randomUUID`, `serviceWorker`, `webShare`, `webAssembly`, `webSocket` and `rtcPeerConnection`.
  - `showOpenFilePicker` was true only in Chrome. Streaming `fetch` uploads (`fetchStreamingUpload`) were not available in Safari.
- **WEB-2** (all five browsers):
  - `"page": "http://<lan-host>"` and `isSecureContext: false`.
  - `false`: `subtle`, `randomUUID`, `serviceWorker`, `webShare` and `clipboardApi`.
  - `true`: `getRandomValues`, `webAssembly`, `webSocket`, `rtcPeerConnection` and `rtcOffer`.
- **Speed, WEB-2:** the Mac's own browsers reached the Mac's address without the Wi-Fi radio (1.9 to 3.8 GiB/s down, ADR-0011 `:59`), so their numbers are not network numbers.
- **Speed, WEB-1:** these are loopback speeds. ADR-0011 did not publish them, and they are higher still. The earlier `results.jsonl` shows Mac Chrome at 6666.7 MiB/s down and 803 MiB/s up. They are not network numbers either.

- [ ] **WEB-1 (ADR-0011)** Mac Safari on localhost. Steps: 1) Start the WEB-1 server. 2) Run the probe procedure at `http://localhost:8000/?tag=WEB-1-mac-safari-manual`. **Expect:** the WEB-1 values above; `showOpenFilePicker` not `true`; `fetchStreamingUpload` not `true`; `"brands": null`. **Record:** label, Safari version, any capability that differs, speed (marked "loopback, not a network number"), Files results.
- [ ] **WEB-1 (ADR-0011)** Mac Chrome on localhost. Steps: 1) WEB-1 server running. 2) Run the probe procedure at `http://localhost:8000/?tag=WEB-1-mac-chrome-manual`. **Expect:** the WEB-1 values above; `showOpenFilePicker: true`; `"brave": false`; `brands` contains `Google Chrome 155`. **Record:** label, Chrome version (from `chrome://version`), any capability that differs, speed (loopback, not a network number), Files results.
- [ ] **WEB-1 (ADR-0011)** Mac Brave on localhost. Steps: 1) WEB-1 server running. 2) Run the probe procedure at `http://localhost:8000/?tag=WEB-1-mac-brave-manual`. **Expect:** the WEB-1 values above; `"brave": true`; `brands` contains `Brave 155`; `showOpenFilePicker` not `true`. **Record:** label, Brave version (from `brave://version`), any capability that differs, speed (loopback, not a network number), Files results.
- [ ] **WEB-2 (ADR-0011)** Mac Safari over the LAN. Steps: 1) Start the WEB-2 server. 2) Run the probe procedure at `http://<mac-wifi-ip>:8000/?tag=WEB-2-mac-safari-manual`. **Expect:** the WEB-2 values above. `share` should be an error name because `webShare` is false (not measured: check and note what you see). **Record:** label, any capability that differs, speed (not a network number), Files results, what the copy button did, Safari's Local Network entry and any prompt.
- [ ] **WEB-2 (ADR-0011)** Mac Chrome over the LAN. Steps: 1) WEB-2 server running. 2) Run the probe procedure at `http://<mac-wifi-ip>:8000/?tag=WEB-2-mac-chrome-manual`. **Expect:** the WEB-2 values above; `"brands": null`; `"brave": false`. **Record:** label, any capability that differs, speed (not a network number), Files results, what the copy button did, Chrome's Local Network entry and any prompt.
- [ ] **WEB-2 (ADR-0011)** Mac Brave over the LAN. Steps: 1) WEB-2 server running. 2) Run the probe procedure at `http://<mac-wifi-ip>:8000/?tag=WEB-2-mac-brave-manual`. **Expect:** the WEB-2 values above; `"brands": null`; `"brave": true`. **Record:** label, any capability that differs, speed (not a network number), Files results, what the copy button did, Brave's Local Network entry and any prompt.
- [ ] **WEB-2 (ADR-0011)** iPhone Safari over the LAN. Steps: 1) WEB-2 server running; iPhone unlocked. 2) Run the probe procedure at `http://<mac-wifi-ip>:8000/?tag=WEB-2-iphone-safari-manual`. Skip drag and drop. **Expect:** the WEB-2 values above. The one earlier measurement was 14.4 MiB/s down and 18.8 MiB/s up. That is not a benchmark, so do not pass or fail on speed. **Record:** label, iOS version read from the phone (not from `userAgent`), Safari version, both speeds, Files results, whether the page loaded at all (if not, note the firewall answer and isolation).
- [ ] **WEB-2 (ADR-0011)** Android Chrome over the LAN. Steps: 1) WEB-2 server running; phone unlocked with the screen on. 2) Run the probe procedure at `http://<mac-wifi-ip>:8000/?tag=WEB-2-android-chrome-manual`. Skip drag and drop. **Expect:** the WEB-2 values above. The one earlier measurement was 14.3 MiB/s down and 19.8 MiB/s up, which is not a benchmark. **Record:** label, Android version and Chrome version read from the phone (not from `userAgent`), both speeds, Files results, whether the page loaded.

**WEB-3 by hand: the common steps.** Start the WEB-3 server once and leave it running for every WEB-3 item, including the restart checks. Note the `certificate … Fingerprint=…` line privately. For each browser:
1. Open `https://<mac-wifi-ip>:8443/?tag=WEB-3-<where>-<browser>-manual`.
2. Write down the warning text or code. The warning page, its Proceed link and the address bar may show the Mac's address. In anything you write down, replace the address with `<lan-host>`. Take no screenshots that show the address bar or the Proceed link.
3. Before you continue, try to find the certificate's SHA-256 fingerprint. Where to look (not in the repo, and UNVERIFIED on these versions):
   - Chrome and Brave: the "Not secure" chip, then the certificate viewer, then the SHA-256 fingerprint of the *Certificate*.
   - Mac Safari: **Show Details** on the warning page, then view the certificate, then Details, then Fingerprints.
   - Compare the value with serve.py's line, ignoring colons, spaces and case.
   - Do not use the *public key* SHA-256. That is a different number.
4. Continue past the warning. Count the clicks and note any password prompt.
5. When the probe loads, read `capabilities.isSecureContext` and `subtle`. Then press **Copy the result**.
6. Reload the page and note whether the warning comes back.
7. Open the same URL in a new tab and note whether the warning comes back.
8. Quit the browser fully, relaunch it with the same profile, and open the URL again. Use the same serve.py run. Note whether the warning comes back.
9. **Clean up.** Close the tab, and remove what the click-through left behind (see each item).

For Mac Chrome, use a throwaway profile, so earlier exceptions do not interfere. Run the first two lines of the block below to start Chrome. Step 8: quit the browser, then run the same command again in this terminal, so it reuses the same `$profile`. Run the last line (`rm -rf "$profile"`) only at clean-up. If you will run "WEB-4 over WEB-3" with this browser, do it before you delete the profile. For Brave, use `"/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"` the same way.

```sh
profile="$(mktemp -d)"
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --user-data-dir="$profile" --no-first-run "https://<mac-wifi-ip>:8443/?tag=WEB-3-mac-chrome-manual"
rm -rf "$profile"
```

- [ ] **WEB-3 (ADR-0011)** Mac Chrome, click-through. Steps: 1) Start Chrome with the throwaway profile above. 2) Follow WEB-3 steps 2 to 8. 3) Clean up by deleting the profile (`rm -rf "$profile"`). **Expect:** `net::ERR_CERT_AUTHORITY_INVALID`; **Advanced**, then **Proceed**, loads the page (measured once by hand). The earlier tester could not find the fingerprint. `isSecureContext` after the click-through, and whether the exception is remembered, were never recorded. Those are what this item measures. **Record:** label for each part (click-through; fingerprint found and matched / found and did not match / not found); warning code; clicks; `isSecureContext`; `subtle`; remembered after reload, new tab and relaunch (yes or no each).
- [ ] **WEB-3 (ADR-0011)** Mac Brave, click-through. Steps: 1) Start Brave with a throwaway profile, as above. 2) Follow WEB-3 steps 2 to 8. 3) Delete the profile. **Expect:** the same warning and clicks as Chrome (measured once by hand). The fingerprint, `isSecureContext` and remembering were not recorded. **Record:** the same fields as Mac Chrome.
- [ ] **WEB-3 (ADR-0011)** Mac Safari, click-through. Steps: 1) Follow WEB-3 steps 1 to 8 in Safari. 2) Afterwards, open Keychain Access and search for `dropumi-research`. If you find a certificate or a trust setting for it, delete it (path UNVERIFIED on this macOS version). **Expect:** Safari asked for the macOS account password and then let the tester through. The exact button names are not documented. The earlier run found no certificate or trust setting in the keychain afterwards. The fingerprint, `isSecureContext` and remembering were not recorded. **Record:** the same fields as Mac Chrome, plus:
  - the warning wording, with the address replaced by `<lan-host>`;
  - the button names you saw;
  - whether the password prompt appeared;
  - what Keychain Access showed, and whether you deleted anything.
- [ ] **WEB-3 (ADR-0011)** iPhone Safari, click-through. Steps: 1) Keep the iPhone unlocked. 2) Follow WEB-3 steps 1 to 8. For step 8, close Safari from the app switcher and reopen it. 3) Clean up: clear the site's exception, for example with Settings > Apps > Safari > Clear History and Website Data (path not in the repo and UNVERIFIED on iOS 27.0.1). **Expect:** this was never tested and no steps are documented (not confirmed in code: check and note what you see). **Record:** the same fields as Mac Chrome, plus:
  - the warning wording (address replaced);
  - the buttons you used;
  - where, if anywhere, you found the SHA-256;
  - what you cleared, and whether the warning returned afterwards.
- [ ] **WEB-3 (ADR-0011)** Android Chrome, click-through. Steps: 1) Keep the phone unlocked with the screen on. 2) Follow WEB-3 steps 1 to 8. For step 8, close Chrome from recent apps and reopen it. 3) Clean up: clear the site's data or exception in Chrome's settings (path not in the repo and UNVERIFIED). **Expect:** this was never tested and no steps are documented (not confirmed in code: check and note what you see). **Record:** the same fields as iPhone Safari.

**WEB-4 by hand: the common steps** (manual copy and paste; A makes the offer, B answers):
1. Keep the WEB-2 server running.
2. A opens `http://<mac-wifi-ip>:8000/index.html?tag=WEB-4-<pair>-offer-manual`.
3. B opens `http://<mac-wifi-ip>:8000/index.html?tag=WEB-4-<pair>-answer-manual`.
4. On A, press **Create offer**. Wait until the offer box fills, then press **Copy offer**. Move the text to B over a local path only. The SDP holds local addresses, and README `:26` suggests "a note or a chat with yourself", which uploads them to a third party. Do not do that. Use instead:
   - the Mac's own clipboard for a Mac-only pair;
   - a local path for Mac ↔ iPhone, such as AirDrop or Universal Clipboard (not in the repo).
   If a pair has no local path, use auto mode (below), and record that the pair used auto mode, not copy and paste.
5. On B, paste it into "Paste device A's offer here". Press **Create answer**, then **Copy answer**, and move the answer back to A the same way.
6. On A, paste it into "Paste device B's answer here" and press **Connect**.
7. Watch `connectionState` in the Result. When the channel opens, **Send** is enabled on both pages. Press **Send** on A only. B's Send button is enabled too: do not press it. "Megabytes to send" defaults to 64.
8. Press **Copy result** on both A and B. Then delete the SDP texts wherever you put them.

On this http page, the copy buttons may read "Select and copy by hand". In that case, select the text and copy it yourself. Errors appear in red as `Error: <message>`.

**Auto mode** (serve.py relays the SDP, as run-matrix.py does):
- B opens `…/index.html?auto=answer&room=<R>&tag=<tag>-answer`.
- A opens `…/index.html?auto=offer&room=<R>&mb=64&tag=<tag>-offer`.
- `<R>` is 1 to 64 letters and digits. Use a new room for every run, and load both pages from the same server, because the server keeps each room's offer and answer in memory.
- Open B first, then open A within about 90 s. run-matrix.py uses this order. Each page polls for the other side's SDP for about 90 s.
- After the answer arrives, A waits up to 60 s for the channel to open, then sends.
- Failure texts:
  - `Error: no answer arrived from the other page` (offer side);
  - `Error: no offer arrived from the other page` (answer side);
  - `Error: the channel did not open within 60 s`;
  - `Error: the server refused the offer: <status>` (or `the answer`).
- After any failure, start again with a new room.

**What to record for each WEB-4 pair:**
- the label;
- `connectionState`;
- `selectedPair.local` and `selectedPair.remote` (`type`, `protocol`, `mdnsName`), on both A and B;
- the candidate types in `localCandidates` and `remoteCandidates`, and whether `mdnsName` is true;
- A's `sentBytes`, `seconds` and `mibPerSecond`;
- B's `receivedBytes`;
- any `errors` (read the text before pasting);
- whether copy and paste or auto mode was used;
- for a Mac browser: any Local Network prompt and your answer.

**Reference from the earlier automated run.** That run relayed the SDP through serve.py; it did not use copy and paste. ADR-0011 did not transcribe `selectedPair` (`:64` "Not recorded"), but the run-matrix summary and `results.jsonl` kept it.
- **Selected pair:** on all four pairs, every `selectedPair` side had `protocol: udp` and `mdnsName: false`. Local was always `host`, and remote was `host` or `prflx`.
- **Candidates:**
  - Mac Chrome, Mac Safari and iPhone Safari listed only host/udp candidates with `.local` names (`mdnsName: true`).
  - Android Chrome listed host/udp candidates without `.local` names (`mdnsName: false`).
  - No `srflx` or `relay` candidates appeared.
- **Copy and paste:** the copy-and-paste path was never recorded.

The pages use no STUN or TURN. A `failed` state probably means that no host candidate pair worked, for example because of client isolation or mDNS name resolution. That reading is an inference, not documented.

- [ ] **WEB-4 (ADR-0011)** Mac Chrome to Mac Safari, by hand. Steps: 1) Follow the WEB-4 steps with A = Mac Chrome and B = Mac Safari (pair `macchromemacsafari`), moving the SDP with the Mac's clipboard. **Expect:** `connectionState: "connected"`; `sentBytes: 67108864`. The relayed run connected at 10.4 MiB/s on the same machine. A's `selectedPair` was local host/udp and remote prflx/udp. **Record:** the WEB-4 fields above.
- [ ] **WEB-4 (ADR-0011)** Mac Chrome to iPhone Safari, by hand. Steps: 1) Follow the WEB-4 steps with A = Mac Chrome and B = iPhone Safari (pair `macchromeiphone`). **Expect:** `connected`; `sentBytes: 67108864`. The relayed run connected at 8.4 MiB/s. A's `selectedPair` was local host/udp and remote prflx/udp. **Record:** the WEB-4 fields above, and how you moved the SDP.
- [ ] **WEB-4 (ADR-0011)** Mac Chrome to Android Chrome, by hand. Steps: 1) Keep the phone's screen on and unlocked. 2) Follow the WEB-4 steps with A = Mac Chrome and B = Android Chrome (pair `macchromeandroid`). Use auto mode if you have no local way to move the SDP. **Expect:** `connected`; `sentBytes: 67108864`. The relayed run connected at 2.3 MiB/s. Its first attempt got no answer while the phone's screen was off. A's `selectedPair` was host/udp on both sides. **Record:** the WEB-4 fields above.
- [ ] **WEB-4 (ADR-0011)** iPhone Safari to Android Chrome, by hand. Steps: 1) Keep both phones unlocked. 2) Follow the WEB-4 steps with A = iPhone Safari and B = Android Chrome (pair `iphoneandroid`). Use auto mode if you have no local way to move the SDP. **Expect:** `connected`; `sentBytes: 67108864`. The relayed run connected at 2.5 MiB/s. A's `selectedPair` was host/udp on both sides. **Record:** the WEB-4 fields above.
- [ ] **WEB-4 (extra, ADR-0011 gate 2, `:28`)** Mac Safari to iPhone Safari. Steps: 1) Follow the WEB-4 steps with A = Mac Safari and B = iPhone Safari (pair `macsafariiphone`, tag `WEB-4-macsafariiphone-offer-manual` / `-answer-manual`). **Expect:** never measured; Mac Safari never made an offer in the earlier run (not confirmed in code: check and note what you see). **Record:** the WEB-4 fields above, and how you moved the SDP. If you skip it, record `Not tested`.
- [ ] **WEB-4 (extra, ADR-0011 gate 2, `:28`)** Mac Safari to Android Chrome. Steps: 1) Keep the phone unlocked with the screen on. 2) Follow the WEB-4 steps with A = Mac Safari and B = Android Chrome (pair `macsafariandroid`). Use auto mode if you have no local way to move the SDP. **Expect:** never measured (not confirmed in code: check and note what you see). **Record:** the WEB-4 fields above. If you skip it, record `Not tested`.
- [ ] **WEB-4 on localhost (extra, `poc/web/README.md:9` and `:21`)** Mac Chrome to Mac Safari in a secure context. Steps: 1) Start the WEB-1 server. 2) Follow the WEB-4 steps with A = Mac Chrome at `http://localhost:8000/index.html?tag=WEB-4local-macchromemacsafari-offer-manual` and B = Mac Safari at `http://localhost:8000/index.html?tag=WEB-4local-macchromemacsafari-answer-manual`. Move the SDP with the Mac's clipboard. **Expect:** never measured on `index.html`. On WEB-1, the probe was a secure context in both browsers, so `secureContext: true` is likely (not confirmed in code: check and note what you see). **Record:** the WEB-4 fields above, plus `secureContext` and `origin` at the top of the Result. Compare `mibPerSecond` with the http `macchromemacsafari` pair to answer README's question "does it matter whether the page is a secure context?".
- [ ] **WEB-4 over WEB-3 (extra, ADR-0011 "WEB-4 details", `:64`)** DataChannel on the https page. Steps:
  1) Keep the WEB-3 server running. Run at least Mac Chrome → Mac Safari (pair `macchromemacsafari`). Run the other three pairs (`macchromeiphone`, `macchromeandroid`, `iphoneandroid`) only where the WEB-3 click-through passed on both browsers.
  2) For each pair, first do the WEB-3 click-through on both browsers, in the same browser profile you use here. For Mac Chrome, use the throwaway profile before you delete it.
  3) Follow the WEB-4 steps at `https://<mac-wifi-ip>:8443/index.html?tag=WEB-4https-<pair>-offer-manual` (A) and `…/index.html?tag=WEB-4https-<pair>-answer-manual` (B). Copy and paste works, and so does auto mode, with the same tags plus `auto=…&room=<R>` as described above.
  4) If the certificate warning appears again on `/index.html`, click through again and note it.

  **Expect:** Not tested before. The Environment box and `secureContext` should show whatever the click-through gave (not confirmed: check and note what you see). **Record:** one label per pair; the WEB-4 fields above; the `secureContext` value at the top of the Result; whether the warning appeared again on `/index.html`.
- [ ] **FIREFOX (extra, ADR-0011 "Firefox: Not tested", `:53`)** Firefox on the Mac, if installed. Steps: 1) Run the probe procedure for WEB-1 and WEB-2. 2) Follow the WEB-3 steps, then clean up. 3) Run one WEB-4 pair by hand with Mac Chrome. run-matrix.py has no Firefox support. **Expect:** never measured; record what you see. **Record:** Firefox version and the same fields as the matching rows above. If Firefox is not installed, record `Not tested`.
