# ADR-0011: Browser companion

- **Status:** Proposed
- **Date:** 2026-10-14
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

## Implementation

- `poc/web/` (part 1): status Not tested. Results are recorded here per browser, OS version and network, using the labels of AGENTS.md §10.

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
