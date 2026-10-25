# Device test run: <YYYY-MM-DD>-<nn>

Copy this file to `docs/device-tests/runs/<YYYY-MM-DD>-<nn>.md`, fill it in during the session, and commit it with
`docs: record device test run <YYYY-MM-DD>-<nn>`. Only passed scenarios change a label in
[`docs/testing.md`](../testing.md); a failed one is recorded here, with what happened.

**Privacy.** Record models and OS builds only. Do not record:
- device names (keep the default `Dropumi-XXXX` names);
- Wi-Fi names, IP addresses or UDIDs;
- user names or home paths;
- screenshots that show any of these.

## Setup

| Item | Value |
|---|---|
| Commit tested | `<git rev-parse --short HEAD>` |
| Build | Debug or Release; Rust core profile from `apps/apple/DropumiKit/Artifacts/core-stamp.txt` |
| Devices | for example: iPhone 15, iOS 27.0.1 (build …); iPad Air (5th generation), iPadOS 27.0.1; Mac arm64, macOS 27.0.1; Android phone model, Android version |
| Network | for example: home Wi-Fi, 5 GHz, no client isolation; both devices on the same network |
| Test files | `bash scripts/device-tests/make-test-files.sh <dir> [MiB]`; checked with `shasum -a 256 -c SHA256SUMS` on the receiver (copy the received files to a Mac if needed) |

## Results

Use one label per row:
- `Manual device test passed`;
- `Failed` (describe what happened);
- `Not tested`.

The core set (DISC-1, PAIR-1, TX-1, TX-3, CAN-1) runs on every pair. For each pair, write which device sent; for PAIR rows, which device tapped Pair.

| ID | Scenario | Pair (sender → receiver) | Result | Notes |
|---|---|---|---|---|
| DISC-1 | Each side sees the other in Nearby (Bonjour ↔ NSD ↔ `mdns-sd`) | | | |
| DISC-2 | A device never lists itself | | | |
| DISC-3 | Off: the device is not listed and connections are refused | | | |
| DISC-4 | The advertised host name is `dropumi-…`, not the device's name (`dns-sd -B _dropumi._tcp`, then `dns-sd -L <instance> _dropumi._tcp local.` on the Mac) | | | |
| PAIR-1 | Both devices open Pair a device and show "Ready to pair" with a countdown; one taps Pair next to the other; codes match on both screens; both show "Paired with “…”" and list each other as paired | | | |
| PAIR-2 | One side taps "Codes don't match": that side reads "You said the codes don't match"; the other reads "“…” didn't confirm the code" or "The other device didn't confirm the code"; nothing is stored on either side | | | |
| PAIR-3 | Not ready: one device taps Pair next to a device that is open but not on Pair a device (receiving on). The first reads "“…” isn't waiting to pair" (never "Only paired devices") and is ready again; the other shows "A device was refused" (at most once a minute), and its "Pair a device" opens the pairing screen | | | |
| PAIR-4 | An Android phone by address: the phone shows its address on Pair a device (in the current Android app, after Wait for a device); on the Apple device, "Not listed? Pair by address", type it; codes match on both; both list each other as paired | | | |
| PAIR-5 | Leaving resumes receiving: with receiving on, open Pair a device ("Receiving files is paused…"), then leave by Stop, Back or (Mac) closing the window. The status no longer reads "Ready to pair", a paired device can send again, and "Receiving could not start" never appears | | | |
| TX-1 | Paired transfer with a prompt; SHA-256 equal | | | |
| TX-2 | Auto-accept on: no prompt; off: prompt | | | |
| TX-3 | Everyone for 10 minutes, unverified sender: both show the same code; no pairing is created | | | |
| TX-4 | Paired only: an unpaired sender is refused | | | |
| TX-5 | Decline | | | |
| TX-6 | A file of at least 2 GiB; note the time from Transfers and the method | | | |
| TX-7 | 300 small files (`many/`) | | | |
| TX-8 | Unicode, right-to-left, duplicate and 0-byte names; nothing overwritten | | | |
| CAN-1 | The sender cancels; no partial file is left on the receiver | | | |
| CAN-2 | The receiver cancels; no partial file is left | | | |
| NET-1 | The sender's Wi-Fi goes off during a transfer | | | |
| NET-2 | The receiver's Wi-Fi goes off during a transfer | | | |
| NET-3 | The access point reboots | | | |
| NET-4 | Client isolation: Connect by address works | | | |
| NET-5 | Android on Wi-Fi without internet while mobile data is on | | | |
| BG-1 | iOS: go to the Home Screen during a transfer (background completion) | | | |
| BG-2 | iOS idle in the background is not listed by other devices | | | |
| BG-3 | Mac with the window closed: the menu bar receives and notifies; Quit stops advertising | | | |
| BG-4 | Android sends, then Home and lock for 5 minutes | | | |
| BG-5 | Android receives in the background with the screen locked | | | |
| SHARE-1 | iOS Photos → Dropumi → open Dropumi → send (a small photo and a 1 GB video) | | | |
| SHARE-2 | iOS Files: Share → Dropumi, or open a file in Dropumi | | | |
| SHARE-3 | Android: share one file and several files | | | |
| SHARE-4 | Mac: drag files onto the window | | | |
| PERM-1 | Local Network denied on iOS and Mac: the app says so; turn it back on and discovery works | | | |
| PERM-2 | Android notifications denied | | | |
| SPOOF-1 | A device renamed to a paired device's name shows "Unverified device … claims …" and never as the paired device | | | |
| KILL-1 | Force-quit the receiver during a transfer: the sender reports a lost connection; the stale `.part` file is gone at the next start | | | |
| UI-1 | VoiceOver or TalkBack, the largest text size, keyboard only | | | |
| UI-2 | iPad: layout, drag from Files | | | |
| UI-3 | Android rotation and tablet layout | | | |
| UI-4 | Mac: open at login; a chosen receive folder survives a relaunch | | | |
| MAC-UI | Mac UI state tests (`xcodebuild test -scheme Dropumi-macOS`), after approving UI automation | | | |
| MAC-BONJOUR | `DROPUMI_TEST_MDNS=1 bash scripts/apple/interop.sh apple_macos_bonjour`, answering the Local Network question | | | |

## Findings

- …
