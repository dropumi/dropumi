# ADR-0008: UI framework strategy

- **Status:** Superseded by [ADR-0012](0012-apple-apps.md) (2026-10-15: the Mac app is SwiftUI, shared with iPhone and iPad; Windows and Linux stay undecided under this record's gates G1 and G2)
- **Date:** 2026-10-13 (platform facts reconciled with `feasibility.md` on 2026-10-13; measured receiver baseline updated on 2026-10-14)
- **Related:** ADR-0001, ADR-0009, ADR-0010, [`../feasibility.md`](../feasibility.md)

## Context

- Specification §10 asks for an ADR comparing three options: **A** Tauri 2 with the Rust core, **B** Slint with the Rust core, **C** native UI per platform with a shared Rust core. The choice must rest on measured or documented evidence, not preference, and must not prevent correct native Share Extensions or background receiving.
- Priority order from the specification: (1) correct native integration, (2) security and stability, (3) memory and CPU efficiency, (4) maintainability, (5) application size, (6) development velocity.
- Phase 0 asks for an initial UI approach to be selected. Milestone 1 builds no UI, so **no UI measurements exist yet** and none are claimed here.
- Constraints that apply whatever framework is chosen (details and sources in [`feasibility.md`](../feasibility.md); all "Documented capability - not tested"): the iOS/iPadOS Share Extension is a separate native extension process with tight resource limits; Android sharing and foreground-service integration go through Kotlin/Java platform APIs; desktop share integrations (macOS share extension, Windows share target, Linux file-manager actions) are platform-specific.
- Native radio and discovery APIs that a UI process may have to host (also from `feasibility.md`):
  - Apple-to-Apple peer-to-peer Wi-Fi goes through Network framework `includePeerToPeer`. **Multipeer Connectivity is deprecated** (Apple TN3151) and is not an option.
  - Wi-Fi Aware is available on iOS/iPadOS 26 (and Mac Catalyst 26), not native macOS. It needs the Wi-Fi Aware entitlement, which Apple documents as a capability enabled in Xcode with no request form, plus the `WiFiAwareServices` key. Raw multicast on iOS needs the multicast entitlement, which does require Apple's approval.
  - Windows BLE advertising (`BluetoothLEAdvertisementPublisher`) restricts only the Flags and LocalName sections; manufacturer data and other sections are allowed.
- License facts (checked with `cargo info` on 2026-10-13):

| Crate | Version | License expression | MSRV |
|---|---|---|---|
| `tauri` | 2.12.3 | `Apache-2.0 OR MIT` | 1.90 |
| `slint` | 1.18.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | 1.92 |

## Decision

**Mobile: native UI over the Rust core** (option C).
- iOS/iPadOS: Swift/SwiftUI, including a native Share Extension.
- Android: Kotlin/Jetpack Compose, receiving share intents.
- The Rust core is reached through the bindings strategy in ADR-0009.
- Rationale: priority 1. Share Extensions and background receiving need first-class native integration; with Tauri or Slint on mobile, every critical integration point would still be native code.

**Desktop: provisional default is Tauri 2** (option A, 2.12.3 at the time of writing).
- The UI runs as a **separate process**. Background receiving is done by a **headless Rust receiver**, so the idle footprint never includes a WebView; the UI process starts only when the user opens it.
- Tauri's MSRV (1.90) is above the core workspace MSRV (1.89, ADR-0001); the future desktop app crate will declare its own MSRV.

**Slint is not chosen provisionally.** Its only OSI-approved license option is `GPL-3.0-only`; the other two options are `LicenseRef-Slint-*` licenses that are not OSI-approved. Choosing Slint therefore needs an explicit maintainer licensing decision: either distributing the desktop app under GPL-3.0 terms, or accepting a non-OSI license and its conditions for a project that promises to stay open source. Neither license is on the cargo-deny allow-list today (ADR-0010). This is a recorded reason, not legal advice.

**Fallback: native desktop per OS** (SwiftUI/AppKit on macOS, WinUI 3 on Windows, GTK4 on Linux).

**Phase 2 spike gates.** The provisional default is confirmed or rejected by a time-boxed spike at the start of Phase 2, with an identical prototype per option (tray or menu-bar icon, device list, incoming-request dialog, progress view, notification).

- **G1 - integration feasibility:** tray/menu-bar presence, notifications, Finder/Explorer/Files "send with" integration, launch at login, and screen-reader access, on all three desktop operating systems.
- **G2 - measured budgets:** the maintainers set a threshold for each metric **before** the spike:

| ID | Metric |
|---|---|
| M1 | UI idle resident memory (including WebView or renderer child processes) |
| M2 | UI cold and warm start time |
| M3 | Installer size |
| M4 | Binary size |
| M5 | Idle CPU |
| M6 | CPU during a transfer |
| M7 | Accessibility audit result |

- Failing G1 or G2 means falling back to native desktop UI. The raw spike data is recorded in this ADR (or in a superseding one) and the status changes from "Accepted (provisional)" accordingly. Until then no performance number is claimed for any option.

**Comparison against the specification's priorities** (documented evidence only; everything quantitative waits for the spike):

| Priority | A: Tauri 2 | B: Slint | C: Native |
|---|---|---|---|
| 1 Native integration | Desktop integrations via plugins or native code; to be proven by G1 | Requires investigation (mobile and desktop extension interop) | Full access to platform APIs |
| 2 Security and stability | Rust backend plus system WebView; WebView attack surface requires no remote content and a strict CSP | Requires investigation | Platform toolkits and their update channels |
| 3 Memory and CPU | WebView cost is the specification's stated concern; to be measured (M1, M5, M6) | Small footprint is a hypothesis in the specification; not measured | Not measured |
| 4 Maintainability | One desktop frontend | One desktop frontend | Up to three desktop codebases |
| 5 Size | M3/M4 | M3/M4 | M3/M4 |
| 6 Velocity | Shared web frontend | Declarative `.slint` UI language with Rust integration | Lowest: several toolkits and languages |
| License | `Apache-2.0 OR MIT` | GPL-3.0-only or non-OSI (blocking) | Platform SDKs |

## Implementation

- None in milestone 1. Desktop and mobile apps: Not implemented. The headless receiver this record relies on exists today as `dropumi receive` (ADR-0001); the core library crates build for `aarch64-apple-ios` and `aarch64-linux-android` on the maintainer's machine; status: Build verified only (core library crates; no app, not run).
- Measured baseline for the future headless receiver, from `scripts/bench/loopback.sh` on macOS 27 (Apple M4 Pro): an idle `dropumi receive` used 3,744 KiB RSS after 10 s and 0.00 s CPU over 60 s. No UI framework has been measured (M1-M7 are all not measured).

## Consequences

### Positive

- Mobile share and background integration are never blocked by the UI framework.
- The idle receiver stays a small headless Rust process on every desktop OS, independent of the UI choice.
- The provisional desktop pick has a permissive, MPL-2.0-compatible license.
- The final desktop decision is gated on measured evidence with thresholds fixed in advance.

### Negative

- Several UI codebases (Swift, Kotlin, plus the desktop UI); presentation logic must stay thin, with all state and transfer logic in the Rust core.
- The WebView's memory and CPU cost is unknown until measured; Tauri may fail G2.
- A separate UI process requires a local IPC channel between UI and receiver, which must be authenticated (design deferred to Phase 2).
- The decision may change after the spike, so no desktop work should start before it.

## Alternatives considered

- **Slint 1.18.1 as the provisional desktop UI:** its only OSI option is GPL-3.0-only; needs a maintainer licensing decision.
- **Native desktop per OS now:** strongest integration but the highest cost; kept as the fallback if Tauri fails a gate.
- **Tauri on mobile:** native code would still be needed for every critical integration, plus WebView overhead.
- **Slint on mobile:** mobile maturity and screen-reader support require investigation; native code still needed for extensions.
- **Deciding without a spike:** contradicts the specification's evidence requirement.
