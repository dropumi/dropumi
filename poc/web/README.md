# Research prototype: LAN WebRTC DataChannel

**Not a Dropumi feature.** This page exists to gather facts for [ADR-0011](../../docs/adr/0011-browser-companion.md) (browser companion), part 1 of its proof of concept. It is not secure, it is never deployed, and nothing links to it.

It answers three questions for each pair of browsers on one network:

1. Can two browsers open a WebRTC DataChannel to each other with **no STUN or TURN server** (local network only)?
2. Which ICE candidate types make it work, and do browsers hide their local addresses behind mDNS `.local` names?
3. How fast is the DataChannel, and does it matter whether the page is a secure context?

The page has no dependencies and no build step. It makes no network requests other than the WebRTC connection between the two browsers.

## How to run it

On the Mac, from the repository root:

```sh
python3 -m http.server 8000 --directory poc/web
```

- **Same computer** (secure context: `localhost`): open `http://localhost:8000` in two tabs or two browsers.
- **Another device on the same Wi-Fi** (not a secure context): open `http://<the Mac's LAN address>:8000` on the phone or tablet.

Then:

1. On device A, press **Create offer** and copy the offer text to device B (for example through a note or a chat with yourself).
2. On device B, paste it, press **Create answer**, and copy the answer back to device A.
3. On device A, paste the answer and press **Connect**.
4. Once the channel is open, press **Send** on device A.

Copy the **Result** block into ADR-0011. It records candidate types, the secure-context flag, the browser and the throughput. It replaces the host name with `<lan-host>` and contains no IP addresses. Do not paste the SDP texts anywhere: they contain local addresses.

## What to record

For each run, record:

- both browsers and versions (from the result);
- the OS version and device class of each side;
- the network: band, same subnet, client isolation on or off, VPN off;
- the outcome, using the labels in AGENTS.md §10.

Until a run is recorded, every combination is `Not tested`.
