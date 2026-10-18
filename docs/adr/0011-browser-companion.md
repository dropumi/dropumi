# ADR-0011: Browser companion

- **Status:** Proposed
- **Date:** 2026-10-14; research results and the maintainer's direction added 2026-10-18
- **Related:** ADR-0002, ADR-0004, ADR-0005, ADR-0010, [`../feasibility.md`](../feasibility.md) §6, [`../../poc/web/README.md`](../../poc/web/README.md)

## Context

- People want to send to and from a browser that has no Dropumi app (a borrowed laptop, a work machine). The feasibility memo (§6) assessed this and postponed general availability until a secure transport is demonstrated.
- **Core finding** (feasibility §6): a page delivered over plain HTTP from the LAN can be replaced by an active attacker, after which every in-page protection runs attacker-chosen code. Any design must first make sure the page itself is authentic.
- Browsers only treat `https`, `localhost` and loopback origins as secure contexts; `crypto.subtle`, Service Workers and the Web Share API need one [F85]–[F87]. `RTCPeerConnection` is not marked secure-context-only on MDN [F88]; real behaviour on each browser is UNVERIFIED.
- Dropumi's crypto policy allows only `snow` with its pure-Rust backends, `sha2`, `blake3` and `getrandom` (AGENTS.md §7); `ring`, `openssl` and `native-tls` are banned (ADR-0010). Any TLS or DTLS stack on the app side must fit that policy.
- Protocol v1 already reserves Hello key 8 for a pairing token (`docs/protocol/v1.md`).

## Options

| Option | Page integrity | Transport security | Fit with policy | Assessment |
|---|---|---|---|---|
| **A.** The app serves an HTTPS page with a per-session self-signed certificate; its fingerprint is shown as a QR code | Only if the user really compares the certificate, which browsers do not let a page or a QR code pin; the click-through warning looks the same as an attacker's | TLS to the app | Needs a TLS server stack in the app; rustls needs `ring` or `aws-lc` (banned or unreviewed) | Rejected for now |
| **B.** The page comes from Dropumi's official HTTPS origin (GitHub Pages). It talks to the app over a WebRTC DataChannel used only as a transport, and runs protocol v1 inside it (Noise XX, `dropumi-protocol` compiled to wasm). Signaling uses a QR code or a short code. | HTTPS from a public origin; the page is a secure context | Noise XX end to end. The QR code carries the app's full fingerprint, which the page pins before msg3; the browser's key is per session and never trusted, so the app always shows the SAS and asks | Same protocol and crypto as the apps; the app side needs a WebRTC stack without `ring` or `openssl` (candidates: str0m, webrtc-rs, the platform's libwebrtc; all UNVERIFIED) | **Preferred** |
| **C.** Postpone | — | — | — | The current state |

## Decision (proposed)

- Pursue **option B**; do not ship any browser path before its gates pass.
- **Gates** for building it (next milestone):
  1. a WebRTC stack for the app side that passes ADR-0010 (licenses, bans, size);
  2. the connectivity matrix from the proof of concept below: LAN DataChannels between Chrome, Firefox, Safari (macOS and iOS) and Android Chrome;
  3. the maintainers' decision on the one-time internet fetch of the page, or on requiring the page to be cached for offline use;
  4. threat-model rows for the browser path, each with its test.
- **Proof of concept, part 1** (this milestone, `poc/web/`): a plain JavaScript page, no build tools and no new dependencies, that connects two `RTCPeerConnection`s by copy-and-paste SDP, with no STUN or TURN servers. It reports the candidate types used, whether the page is a secure context, and DataChannel throughput. It is labelled "research prototype", excluded from the Cargo workspace, and never deployed or linked from the website.
- **Proof of concept, part 2** (after the maintainers approve the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` and the `getrandom` `wasm_js` feature): `dropumi-protocol` compiled to wasm, known-answer tests equal to native, and a Noise XX plus SAS handshake between two pages over part 1's DataChannel.

## Direction (2026-10-18)

- The maintainer chose to investigate **a page served by the app itself, local only**, as research. Option B's page
  from a public origin is set aside: it conflicts with the master spec's "no public web hosting dependency" and
  "assets bundled with or served by the native application". Nothing ships, and this ADR stays Proposed.
- The measurements below change no conclusion of feasibility §6: a page the app serves over plain HTTP can still be
  replaced by an active attacker on the LAN, and the self-signed HTTPS option is sound only if a person really
  compares the certificate, which the tester could not do.

## Implementation

`poc/web/` holds the research tools (research prototype, never deployed): `probe.html` (secure context, APIs,
throughput), `index.html` (a WebRTC DataChannel with no STUN or TURN), `serve.py` (stands in for "the app serves the
page": HTTP or HTTPS with a throwaway self-signed certificate, signalling relay) and `run-matrix.py` (opens the pages
on the real browsers and collects their Result blocks, which contain no addresses).

**Measured on 2026-10-18** (one run each; the Mac, an iPhone 15 and an Android phone on one Wi-Fi network, band and
isolation not recorded; the page served by the Mac):
- Browsers: Safari 27.0.1, Chrome 155.0.8059.85 and Brave 155.1.97.59 on macOS 27.0.1 arm64; Safari on iOS 27.0.1
  (iPhone 15); Chrome 155.0.8059.39 on Android 16 (Galaxy A54). Firefox: Not tested.

| ID | Setup | Result | Status |
|---|---|---|---|
| WEB-1 | `http://localhost:8000` on the Mac | A secure context on all three Mac browsers: `crypto.subtle` (with SHA-256 and X25519), `randomUUID`, Service Worker, Web Share, WebAssembly, WebSocket and `RTCPeerConnection` present. `showOpenFilePicker` only in Chrome; streaming `fetch` uploads not in Safari | Automated test passed (research page, Mac, local run) |
| WEB-2 | `http://<Mac's LAN address>:8000` | **Not a secure context** on all five browsers. Missing: `crypto.subtle`, `randomUUID`, Service Worker, Web Share, the async Clipboard API. Present: `getRandomValues`, WebAssembly, WebSocket, and `RTCPeerConnection` with a working `createOffer` | Automated test passed (research page; Mac, iPhone and Android browsers; LAN) |
| WEB-2 speed | 64 MiB down and up over HTTP | iPhone Safari: 14.4 MiB/s down, 18.8 MiB/s up. Android Chrome: 14.3 down, 19.8 up. The Mac's own browsers reached the Mac's address without the Wi-Fi radio (1.9 to 3.8 GiB/s down), so their numbers are not network numbers | Measured once each; not a benchmark |
| WEB-3, key pinned | `https://<Mac's LAN address>:8443`, throwaway self-signed certificate; Chrome and Brave started with its key pinned (`--ignore-certificate-errors-spki-list`), standing for a tester who checked the fingerprint | A secure context, with the same APIs as WEB-1 | Automated test passed (research page; Chrome and Brave on the Mac; LAN) |
| WEB-3, by hand | The same page, opened by the maintainer on the Mac | Chrome and Brave: `net::ERR_CERT_AUTHORITY_INVALID`; Advanced, then Proceed, loads the page. Safari: let the tester through after asking for the macOS account password (on macOS that password confirms a change to certificate trust settings; afterwards no certificate or trust setting for it was found in the keychain). **The tester could not find the certificate fingerprint to compare it** in any of the three browsers. Whether the page was then a secure context, and whether the browser remembers the exception: not recorded | Manual device test: the click-through passed; the fingerprint check failed |
| WEB-3 on phones | iPhone Safari, Android Chrome | — | Not tested |
| WEB-4 | `index.html` over WEB-2 (not a secure context); two pages, offer and answer relayed by `serve.py`, as the app could; 64 MiB over the DataChannel | Mac Chrome → Mac Safari: connected, 10.4 MiB/s (same machine). Mac Chrome → iPhone Safari: connected, 8.4 MiB/s. Mac Chrome → Android Chrome: connected, 2.3 MiB/s (the first attempt got no answer while the phone's screen was off). iPhone Safari → Android Chrome: connected, 2.5 MiB/s | Automated test passed (research page, LAN, four pairs) |
| WEB-4 details | Candidate types of the selected pair; the same over WEB-3 | Not recorded | Not tested |

What the measurements say:
- A page served over plain HTTP on the LAN can still open a DataChannel and run WebAssembly with
  `getRandomValues`, so protocol v1 compiled to wasm could run in it. That does not make it safe: the page itself
  is not authentic (the core finding).
- With HTTPS and a pinned key, the page is a secure context. But browsers give a page and a QR code no way to pin a
  key, and the tester could not find the fingerprint by hand, so option A's security property does not hold in
  practice.
- Before anything ships, a page would need an authenticity check a person can actually do (or an opt-in mode whose
  label says it is not protected against an active attacker, feasibility §6), plus gates 1 and 4 above.

## Consequences

### Positive

- The page is authentic (HTTPS from a fixed origin), and the transport is the same audited protocol as the apps.
- The browser never becomes trusted; every browser transfer shows a code and asks.

### Negative

- Loading the page needs the internet once, unless it is cached; this conflicts with "no internet dependency" and needs the maintainers' decision.
- A WebRTC stack on the app side is a large dependency to review.
- Signaling by QR code needs a camera on one side, or a typed code.

## Alternatives considered

- **Plain HTTP page from the app:** an active LAN attacker replaces the page; never secure (feasibility §6).
- **Public-CA certificate for a name that resolves to LAN addresses:** needs a public service and internet; against the project's rules.
- **Installing a local CA on the visitor's device:** too invasive; a long-lived trust anchor.
