# Phase 0 feasibility assessment

- **Date:** 2026-10-13
- **Scope:** milestone 1 (Phase 0 + Phase 1) of Dropumi
- **Related:** [ADR-0001](adr/0001-architecture-and-workspace-layout.md), [ADR-0007](adr/0007-lan-discovery-mdns.md), [ADR-0008](adr/0008-ui-framework-strategy.md), [ADR-0009](adr/0009-ffi-bindings-strategy.md), [`protocol/discovery.md`](protocol/discovery.md)

> Dropumi is pre-alpha and unaudited. This document records what platform vendors **document**. It does not say what Dropumi does. No row here is a claim that anything works, and no platform is labelled "supported".

**Method**
- Every matrix cell cites a reference `[Fn]` (see §10). Each one was fetched and read on 2026-10-13.
- If a cell's source could not be fetched and confirmed, the cell reads **Requires investigation (UNVERIFIED)**. Individual facts that could not be confirmed are marked UNVERIFIED inline.
- Developer-forum threads, press releases and news posts are cited as *reports*, not as vendor specifications. Apple Documentation Archive pages may be out of date; this is noted in §10.
- Some cells contain Dropumi's own inference from a source, for example "the source lists no such API". Each of these is labelled as an inference.
- Time-boxed exploration, as master spec §15 requires.

## 1. Status vocabulary

**Capability labels** (§3–§5):

| Label | Meaning |
|---|---|
| Documented capability - not tested | A vendor or specification source documents a public API or mechanism. Dropumi has not tested it on any device. |
| Requires investigation | The documentation is conditional (hardware, driver, packaging, store policy), incomplete, or contradicted by developer reports. Feasibility needs a spike on real devices. With "(UNVERIFIED)", no source could be confirmed at all. |
| Unsupported | A vendor source says it is unavailable, or it is absent from the vendor's own list of options. The basis is stated in the cell. |
| Not applicable | The concept does not exist on that platform. |

**Test labels** (master spec §14; used in §9 and in all platform tables):

| Label | Meaning |
|---|---|
| Automated test passed | An automated test exercised the behaviour on that platform, and the result is linked or recorded. |
| Manual device test passed | A person ran the behaviour on a real device and recorded the result. |
| Build verified only | The code compiled for that target. Nothing was run. This label never implies the platform works. |
| Not tested | No build or test evidence exists. |
| Unsupported | Not possible with public, documented APIs. |

§9 also uses **Not implemented** for features that have no code yet. It is a status, not a test label.

## 2. Summary

1. **Same-LAN (mDNS/DNS-SD plus TCP) is the only data-transfer mode with a documented public-API path on all five platforms** (§3). BLE is documented everywhere too, but only as a discovery hint. Same-LAN is the Phase 1 baseline. Each platform has its own gates:
   - iOS/iPadOS 14+ and macOS 15+ apply Local Network privacy [F1].
   - Android 17 (targetSdk 37) blocks local-network traffic, including *accepting* inbound TCP, unless the app holds `ACCESS_LOCAL_NETWORK` [F30].
   - On iOS, raw multicast needs an Apple-approved entitlement [F7].
2. **No public-API direct Wi-Fi path between iPhone and Android has been demonstrated.**
   - Apple documents two peer-to-peer Wi-Fi technologies for its platforms: Wi-Fi Aware and Apple peer-to-peer Wi-Fi. Wi-Fi Direct is not one of them [F2].
   - Apple peer-to-peer Wi-Fi works only between Apple devices [F2].
   - Developer reports of iOS↔Android Wi-Fi Aware through September 2026 are negative or unvalidated [F94][F95][F96].
   - An offline iPhone↔Android path therefore depends on a hotspot fallback, which is **Requires investigation** (§4).
3. **Quick Share↔AirDrop interoperability is a Google/Samsung system feature, not an API** [F91][F92][F93]. It does not change finding 2 for third-party apps (§4.1).
4. **iOS cannot keep a background listener.**
   - Apple's (archived) networking guidance is to close listening sockets when the app moves to the background [F25].
   - iOS 26 `BGContinuedProcessingTask` can finish user-initiated work in the background, with system progress UI and network access [F23][F24].
   - A Share Extension runs under lower memory limits and may be terminated after it completes its request. Only a background `NSURLSession` upload or download continues after that [F19][F20][F21].
5. **A Windows Share Target needs package identity:** MSIX, or a package with external location signed with a certificate trusted on the target PC [F56][F57]. A Send To shortcut needs no identity [F59][F60].
6. **Linux has no single integration surface.** File-manager actions differ for Nautilus, Dolphin and Thunar [F73]–[F76]. GNOME Shell needs an extension to show StatusNotifierItem tray icons [F77][F78].
7. **Web companion: general availability (GA) is postponed.** A page served from `http://<LAN-IP>` is not a secure context [F85], so `crypto.subtle` is unavailable [F87]. By Dropumi's analysis, no in-page protocol survives an active LAN attacker who can replace the page's code (§6).
8. **BLE advertising formats differ by platform**, so BLE can only ever be a discovery hint:
   - iOS advertises only a local name and service UUIDs. In the background, iOS advertisements are visible only to iOS scanners [F16][F18].
   - Windows forbids setting the Flags and LocalName sections [F54].

## 3. Capability matrix (discovery and connectivity)

| Capability | Windows | Linux | macOS | Android | iOS/iPadOS |
|---|---|---|---|---|---|
| **mDNS/DNS-SD** [F83][F84] | **Documented capability - not tested.** `DnsServiceRegister`/`DnsServiceBrowse`, Windows 10 desktop apps; registration ends with the process [F47][F48]. Or in-process `mdns-sd`, which lists Windows [F66]. Firewall prompts: UNVERIFIED. | **Documented capability - not tested.** Avahi: primary API is D-Bus; ships in most distributions [F65]. Or `mdns-sd` [F66]. Coexistence with Avahi on UDP 5353: UNVERIFIED. | **Documented capability - not tested.** Local Network privacy since macOS 15; receiving multicast requires it. Exempt: Terminal/SSH command-line tools and launchd daemons. Not exempt: launchd agents. No multicast entitlement on macOS [F1]. `mdns-sd` lists macOS [F66]. | **Documented capability - not tested.** `NsdManager` register, discover and resolve [F29]. Raw multicast needs `WifiManager.MulticastLock` [F31]. targetSdk 37 needs `ACCESS_LOCAL_NETWORK`, or the system NSD picker (`FLAG_SHOW_PICKER`) [F30]. | **Documented capability - not tested.** Bonjour with `NSBonjourServices` and `NSLocalNetworkUsageDescription` [F1]. Raw multicast (for example an in-process Rust responder) needs the Apple-approved `com.apple.developer.networking.multicast` entitlement [F1][F7]. |
| **Inbound TCP listener on the LAN** | **Documented capability - not tested.** MSIX apps can declare firewall exceptions with `desktop2:FirewallRules` (Windows 10 1703+) [F49]. Unpackaged first-run firewall behaviour: UNVERIFIED. | **Documented capability - not tested.** Standard `listen(2)` [F67]. Distro firewall defaults: UNVERIFIED. | **Documented capability - not tested.** Listening for and accepting inbound TCP does not need the Local Network privilege; outgoing connections do [F1]. | **Documented capability - not tested.** With targetSdk 37, accepting an incoming TCP connection requires `ACCESS_LOCAL_NETWORK`. Android 16 has opt-in testing only [F30]. | **Documented capability - not tested (foreground only).** Accepting inbound TCP needs no privilege [F1]. Close the listener when backgrounded, because the app may be suspended [F25]. |
| **BLE advertise** | **Documented capability - not tested.** `BluetoothLEAdvertisementPublisher`, best effort. Flags and LocalName sections are restricted; manufacturer data and other sections are allowed [F54]. | **Documented capability - not tested.** BlueZ `org.bluez.LEAdvertisingManager1` over D-Bus. Returns `NotPermitted` when the maximum number of instances is reached [F68]. | **Documented capability - not tested.** `CBPeripheralManager.startAdvertising` (macOS 10.9+). Only local-name and service-UUID keys; best effort [F16]. | **Documented capability - not tested.** `BluetoothLeAdvertiser` (API 21+, up to 31 bytes) [F37]. The adapter returns `null` when Bluetooth is off or the device has no LE advertising [F38]. Needs `BLUETOOTH_ADVERTISE` on Android 12+ [F36]. | **Documented capability - not tested (foreground).** Local name and service UUIDs only, 28 bytes in the foreground. In the background the name is dropped and UUIDs move to an "overflow" area that only iOS devices explicitly scanning can discover [F16][F18]. |
| **BLE scan** | **Documented capability - not tested.** `BluetoothLEAdvertisementWatcher` [F55]. | **Documented capability - not tested.** BlueZ `Adapter1.StartDiscovery` / `SetDiscoveryFilter` [F69]. | **Documented capability - not tested.** Core Bluetooth (macOS 10.10+) [F17]. | **Documented capability - not tested.** Built-in central role [F35]. `BLUETOOTH_SCAN` with `neverForLocation` on Android 12+ [F36]. | **Documented capability - not tested.** In the background, duplicate discoveries are coalesced and scans run less often [F18]. |
| **Wi-Fi Direct (Wi-Fi P2P)** | **Documented capability - not tested.** `Windows.Devices.WiFiDirect`; requires the Proximity capability [F50]. Use from an unpackaged desktop app: Requires investigation (UNVERIFIED). | **Requires investigation.** wpa_supplicant P2P is an optional build component (`CONFIG_P2P`) driven through its control interface [F70]. Driver support and privilege needs: UNVERIFIED. | **Unsupported.** Apple lists only Wi-Fi Aware and Apple peer-to-peer Wi-Fi as its platforms' peer-to-peer Wi-Fi technologies [F2]. | **Documented capability - not tested.** `WifiP2pManager` (API 14+). Needs `NEARBY_WIFI_DEVICES` (13+) or `ACCESS_FINE_LOCATION` (≤12); some calls need Location mode enabled [F32]. | **Unsupported.** Same basis as macOS [F2]. The iOS Wi-Fi API overview also lists no Wi-Fi Direct API [F3]. |
| **Wi-Fi Aware (NAN)** | **Requires investigation (UNVERIFIED).** No public Windows Wi-Fi Aware API was found in the Microsoft documentation searched. | **Requires investigation.** A wpa_supplicant NAN Data Path patch series was posted on 2026-04-01; mac80211 data-path support is "still under review" [F71]. | **Unsupported.** Framework availability is iOS/iPadOS 26 and Mac Catalyst 26, not native macOS. The device list names only iPhone and iPad models [F4]. | **Documented capability - not tested.** API 26+. Check `FEATURE_WIFI_AWARE` and `isAvailable()`; may be unavailable while Wi-Fi Direct, SoftAP or tethering is in use [F33]. Wi-Fi Aware 4.0 pairing needs `isAwarePairingSupported()` (API 34) [F34]. | **Documented capability - not tested.** iOS/iPadOS 26; iPhone 12 and later, plus listed iPad models [F3][F4]. Requires the entitlement (Publish/Subscribe) and the `WiFiAwareServices` key [F5][F6]. Pairing goes through AccessorySetupKit or DeviceDiscoveryUI. Connections work in the foreground and background while the app has runtime [F4]. |
| **Apple peer-to-peer Wi-Fi (AWDL) / Multipeer** | **Not applicable.** The protocol is not documented for third-party use and works only between Apple devices [F2]. | **Not applicable** [F2]. | **Documented capability - not tested (Apple↔Apple only).** Network framework `includePeerToPeer` [F2][F9]. Multipeer Connectivity is deprecated (TN3151: "deprecated in 2026") [F2][F8]. | **Not applicable** [F2]. | **Documented capability - not tested (Apple↔Apple only)** [F2][F9]. Multipeer Connectivity is deprecated [F8]. |
| **Hotspot: create** | **Documented capability - not tested.** Wi-Fi Direct "legacy" mode lets non-Wi-Fi-Direct clients join the advertised access point [F51]. `NetworkOperatorTetheringManager` needs the `wiFiControl` capability [F52]. Unpackaged use: Requires investigation (UNVERIFIED). | **Requires investigation.** `nmcli device wifi hotspot` creates a hotspot "according to Wi-Fi device capabilities" [F72]. AP-mode driver support and polkit rules: UNVERIFIED. | **Requires investigation.** Core WLAN documents only `startIBSSMode`, an ad-hoc computer-to-computer network that may require an administrator password [F15]. Whether phones can join one: UNVERIFIED. | **Documented capability - not tested.** `LocalOnlyHotspot`: no internet access; needs `NEARBY_WIFI_DEVICES` on 13+ [F39]. | **Unsupported.** Inference: Apple's iOS Wi-Fi API overview lists no API for creating a hotspot [F3]. |
| **Hotspot: join from the app** | **Documented capability - not tested.** `WiFiAdapter.ConnectAsync` [F53]. Capability and consent rules for unpackaged apps: UNVERIFIED. | **Documented capability - not tested.** `nmcli device wifi connect` [F72]. | **Documented capability - not tested.** `CWInterface.associate(to:password:)` "may require an administrator password" [F14]. Apple points macOS to Core WLAN [F10][F13]. | **Documented capability - not tested.** `WifiNetworkSpecifier` (Android 10+): user approval for non-specific requests; the joined network provides no internet [F40]. | **Documented capability - not tested.** `NEHotspotConfigurationManager`: the user must authorize. Listed: open, WEP, WPA/WPA2 personal, EAP, Hotspot 2.0 [F10][F11]. `joinOnce` disconnects once the app is in the background for more than 15 s [F12]. |
| **Manual address + fingerprint** (QR scanning deferred; not assessed) | **Documented capability - not tested.** Outbound Winsock `connect` [F64]; the receiver needs the inbound row above [F49]. | **Documented capability - not tested.** Standard sockets [F67]. | **Documented capability - not tested.** Outgoing connections to a local address need the Local Network privilege; Terminal CLIs are exempt [F1]. | **Documented capability - not tested.** With targetSdk 37, an outgoing TCP connection to a local address needs `ACCESS_LOCAL_NETWORK` [F30]. | **Documented capability - not tested.** Outgoing connections to a local address need the Local Network privilege [F1]. |

## 4. Interoperability matrix

| Pair / path | Status | Basis |
|---|---|---|
| Any ↔ any on the same LAN (Dropumi mDNS + TCP, or a manual address) | Documented capability - not tested | Per-platform rows in §3 [F1][F30][F47][F65][F66]. How often AP/client isolation, multicast filtering and VPNs break this: UNVERIFIED. |
| Apple ↔ Apple, Apple peer-to-peer Wi-Fi (Network framework) | Documented capability - not tested | [F2][F9] |
| iOS ↔ iOS, Wi-Fi Aware | Documented capability - not tested | [F4][F5]. A developer reports iPhone↔iPhone working with Apple's sample code [F94]. |
| Android ↔ Android, Wi-Fi Aware | Documented capability - not tested | Device-dependent [F33][F34] |
| Android ↔ Android, Wi-Fi Direct | Documented capability - not tested | [F32] |
| **iOS ↔ Android, Wi-Fi Aware** | **Requires investigation** (public developer evidence is negative) | Evidence: <ul><li>Apple DTS points developers to the *Accessory Design Guidelines*, which require peers to implement Wi-Fi Aware 4.0 [F94].</li><li>Reported failures: Galaxy S23 Ultra not discovered; Galaxy S25 has a "missing DCEA attribute", PASN pairing "authentication rejected", and an NDP request with no reply [F94][F95].</li><li>A September 2026 call for testers calls Android↔Apple "the biggest unknown"; no validation results had been posted when fetched [F96].</li><li>On Android, pairing needs `isAwarePairingSupported()` (API 34) [F34].</li></ul> |
| iOS ↔ Android, Wi-Fi Direct | Unsupported | iOS has no Wi-Fi Direct [F2][F3] |
| iOS ↔ Android, Apple peer-to-peer Wi-Fi | Unsupported | Works only between Apple devices [F2] |
| Android `LocalOnlyHotspot` → iPhone joins via `NEHotspotConfiguration` | Requires investigation | <ul><li>LocalOnlyHotspot: no internet; `NEARBY_WIFI_DEVICES` [F39].</li><li>iOS needs user approval, and WPA3 is not in the listed security types [F10][F11].</li><li>`joinOnce` drops after 15 s in the background [F12].</li><li>The LocalOnlyHotspot security type: UNVERIFIED.</li></ul> |
| Android `LocalOnlyHotspot` → desktop joins | Requires investigation | [F39][F14][F53][F72] |
| Windows/Linux ↔ Android, Wi-Fi Direct | Requires investigation | [F32][F50][F70] |
| macOS ↔ anything, Wi-Fi Aware | Unsupported | [F4] |
| Windows or Linux ↔ anything, Wi-Fi Aware | Requires investigation (UNVERIFIED) | No public Windows API found; the Linux NAN data path is still under review [F71] |
| iOS foreground BLE advertisement → any scanner | Documented capability - not tested | [F16] |
| iOS background BLE advertisement → non-iOS scanner | Unsupported | "discoverable only by an iOS device explicitly scanning for them" [F16][F18] |
| Windows BLE advertisement → any scanner | Documented capability - not tested | [F54] |
| Dropumi ↔ AirDrop | Unsupported | The Apple peer-to-peer Wi-Fi protocol is not documented for third-party use [F2]. No public AirDrop API was found (UNVERIFIED). Reverse engineering is excluded: the spec allows only official documented APIs. |
| Dropumi ↔ Quick Share | Unsupported | The Quick Share announcements describe a system feature and announce no developer API [F91][F92][F93]. No other public Quick Share API was found (UNVERIFIED). |
| Dropumi ↔ LocalSend | Unsupported | Different protocol: LocalSend v2.2 is HTTP(S)/REST with multicast UDP discovery [F98]. It is used for comparison only; no code is reused. |

### 4.1 Google Pixel Quick Share ↔ AirDrop

**What the sources document:**
- **Google, 2025-11-20 [F91]:**
  - Quick Share is interoperable with AirDrop for two-way file sharing, starting with the Pixel 10 family.
  - It works with AirDrop's "Everyone for 10 minutes" mode.
  - The connection is "direct and peer-to-peer". The interoperability layer is written in Rust and was assessed by an independent penetration-testing firm.
- **Pixel 9 series:** expansion reported on 2026-02-23 [F92].
- **Samsung:** AirDrop support in Quick Share for the Galaxy S26 series, rolling out from 2026-03-23; "Share with Apple devices" is on by default [F93].

**What it does not imply:**
- **No third-party API.** All three sources describe a feature of Quick Share itself; none announces a developer API [F91][F92][F93]. Dropumi, like any other third-party app, cannot appear in AirDrop or receive from it this way.
- **Nothing about public APIs.** It does not show that public Wi-Fi Aware (or any other public API) interoperates between iOS and Android for third-party apps. The sources do not name the underlying radio mechanism: UNVERIFIED.
- **No change to the spec rule** that Dropumi uses official documented APIs only. Reimplementing AirDrop from reverse engineering stays out of scope.
- **Product implication:** owners of those phones already have system-level iPhone sharing. Dropumi's differentiation is Windows and Linux, other Android devices, and an open, documented protocol.

**EU Digital Markets Act** [F97]. The Commission's specification decision requires Apple to provide:
- Wi-Fi Aware 4.0 in iOS 26, and Wi-Fi Aware 5.0 at most nine months after that specification is introduced;
- close-range wireless file transfer while the receiving app is open, by 1 June 2026, with all measures in iOS 27 and by the end of 2026;
- background-execution measures for connected-device companion apps.

Whether iOS 27 shipped these features, and whether they apply outside the EU, is UNVERIFIED. Treat this as an upside to re-check, not as a plan dependency.

## 5. Share integration and background receiving

| Capability | Windows | Linux | macOS | Android | iOS/iPadOS |
|---|---|---|---|---|---|
| **System share target** | **Documented capability - not tested.** Share Target requires package identity: MSIX, or a package with external location signed with a certificate trusted on the target PC [F56][F57]. `SupportsAnyFileType` is reserved for general-purpose file movers, including "file transfer apps" [F57]. | **Requires investigation (UNVERIFIED).** No cross-desktop share-target API was identified. Use file-manager actions (next row). | **Documented capability - not tested.** A Share extension appears under the Share button and the Control-click → Share menu [F22]. The guide is archived; current Finder presentation: UNVERIFIED. | **Documented capability - not tested.** `ACTION_SEND` / `ACTION_SEND_MULTIPLE`; files arrive as content URIs in `EXTRA_STREAM` [F41]. | **Documented capability - not tested, with hard limits:** <ul><li>Memory limits are "significantly lower" than for a foreground app, and launch should take well under 1 s [F20].</li><li>After `completeRequest…` the system can terminate the extension. Only a background `NSURLSession` upload or download continues [F19].</li><li>No long-running background tasks [F21].</li><li>Extensions share the container app's Local Network privilege [F1].</li></ul> |
| **File-manager / context-menu fallback** | **Documented capability - not tested.** Send To: a shortcut in the per-user SendTo folder (`FOLDERID_SendTo`); no identity needed [F59][F60]. The Windows 11 modern context menu needs `IExplorerCommand` plus identity [F58]. | **Documented capability - not tested.** <ul><li>Nautilus: nautilus-python `MenuProvider` (Nautilus API 4.0, Nautilus 43+) [F73].</li><li>Dolphin: service-menu `.desktop` files in `kio/servicemenus`; local ones must be executable [F74].</li><li>Thunar: custom actions (thunar-uca) [F75], and Send To `.desktop` files in `$XDG_DATA_DIRS/Thunar/sendto/` [F76].</li></ul> | **Requires investigation (UNVERIFIED).** The Services menu and Finder Sync were not assessed. Finder's Share menu is covered by the row above [F22]. | **Not applicable.** The Sharesheet is the file-manager path [F41]. | **Not applicable.** Photos and Files share through the share sheet and extensions [F22]. |
| **Tray / menu bar** | **Documented capability - not tested.** `Shell_NotifyIcon` notification-area icon [F61]. | **Requires investigation.** StatusNotifierItem over D-Bus needs a host [F77]. GNOME Shell needs the AppIndicator/KStatusNotifierItem extension; its current version targets Shell 45–51 [F78]. KDE behaviour: UNVERIFIED. | **Documented capability - not tested.** `NSStatusItem` [F27]. | **Not applicable.** The equivalent is the persistent foreground-service notification [F42][F45]. | **Not applicable.** The nearest equivalent is the system Live Activity for `BGContinuedProcessingTask` [F23]. |
| **Incoming-transfer notifications** | **Documented capability - not tested.** App notifications through the Windows App SDK [F62]. Package identity unlocks toast notifications for unpackaged apps [F56]. Path without identity: UNVERIFIED. | **Documented capability - not tested.** Desktop Notifications Specification 1.3 over D-Bus [F80]. | **Documented capability - not tested.** User Notifications (macOS 10.14+) [F28]. | **Documented capability - not tested.** `POST_NOTIFICATIONS` runtime permission (13+). If it is denied, foreground-service notices still appear in Task Manager [F45]. | **Documented capability - not tested.** User Notifications (iOS 10+) [F28]. |
| **Background receiver with the UI closed** | **Documented capability - not tested.** An ordinary user process with a notification-area icon [F61]. Packaged background tasks need identity [F56]. | **Documented capability - not tested.** A user process. Sandboxed (Flatpak) apps request background running and autostart through the Background portal [F81]. systemd user units: UNVERIFIED (not fetched). | **Documented capability - not tested.** `SMAppService` LaunchAgent or LoginItem (macOS 13+) [F26]. Launchd agents need the Local Network privilege; an agent not installed through `SMAppService` sets `AssociatedBundleIdentifiers` [F1]. | **Requires investigation.** <ul><li>FGS type `connectedDevice` covers "Interactions with external devices that require a Bluetooth, NFC, IR, USB, or network connection" [F42].</li><li>`dataSync` is capped at 6 h per 24 h on targetSdk 35+ [F44].</li><li>A foreground service cannot be started from the background on targetSdk 31+ [F43].</li><li>Play policy acceptance of an idle receive mode: UNVERIFIED.</li></ul> | **Unsupported for a persistent listener:** close listening sockets in the background [F25]. In-progress, user-initiated transfers: **Documented capability - not tested** via `BGContinuedProcessingTask` (iOS 26). It starts in the foreground on a user action, may use the network, shows a Live Activity, and the user can cancel it [F23][F24]. |
| **Start at login (opt-in)** | **Documented capability - not tested.** The per-user `Run` key [F63]. Packaged `StartupTask` needs identity [F56]. | **Documented capability - not tested.** XDG autostart (draft spec 0.5) [F82]. Background-portal `autostart` for sandboxed apps [F81]. | **Documented capability - not tested.** `SMAppService` LoginItems/LaunchAgents [F26]. The user-approval flow: UNVERIFIED. | **Requires investigation.** On Android 15+, `BOOT_COMPLETED` receivers may not start `dataSync` and several other FGS types [F44]. Whether `connectedDevice` from boot is acceptable: UNVERIFIED. | **Not applicable.** There is no persistent listener [F25]. |

**Linux display servers (Wayland/X11) and desktops.** Status: **Requires investigation**.
- The legacy freedesktop system tray is X11-specific: "an application running on a given X screen" [F79].
- StatusNotifierItem uses D-Bus instead and is designed as its replacement [F77]. Inference: it can work on Wayland wherever a StatusNotifierItem host is running. GNOME Shell needs the extension listed above [F78].
- Notifications [F80] and the Background portal [F81] are D-Bus interfaces, not X11 protocols.
- Phase 2 must produce a per-desktop matrix (GNOME, KDE Plasma, Xfce × Wayland, X11) from real runs.

**Android send side:**
- User-initiated data transfer jobs (Android 14+, `RUN_USER_INITIATED_JOBS`) must be scheduled while the app is visible, and they show a notification [F46].
- Whether they suit LAN peer transfers: **Requires investigation**.

## 6. Web companion assessment (Phase 6)

**Facts**

| Fact | Source |
|---|---|
| Only potentially trustworthy origins are secure contexts: `https`, `wss` or `file` schemes; `127.0.0.0/8` or `::1`; `localhost` and `*.localhost`. Inference: a private LAN IP over `http` is not on that list, so such a page is not a secure context. | [F85] |
| `crypto.subtle` and `crypto.randomUUID()` are secure-context-only; `crypto.getRandomValues()` is not marked secure-context-only. | [F87] |
| Service Workers, the Notifications API, the Web Share API and the Web Crypto API are restricted to secure contexts. | [F86] |
| MDN does not mark `RTCPeerConnection` as secure-context-only. Measured 2026-10-18 on `http://<LAN-IP>` pages (not secure contexts): a DataChannel opened between Safari 27.0.1 and Chrome 155 on macOS 27.0.1, Safari on iOS 27.0.1 and Chrome 155 on Android 16, with no STUN or TURN (ADR-0011, WEB-4). Firefox: Not tested. | [F88]; ADR-0011 |
| Measured 2026-10-18 (ADR-0011, WEB-2): on `http://<LAN-IP>` all five browsers above report no secure context and lack `crypto.subtle`, `randomUUID`, Service Workers and Web Share; `getRandomValues` and WebAssembly work. | ADR-0011 |
| WebRTC endpoints can hide local IPs in ICE candidates behind mDNS `.local` names (IETF draft). | [F90] |
| Chrome's Local Network Access permission prompt launched in Chrome 142. Only secure contexts can request it, and WebRTC connections are not yet gated by it. | [F89] |

**Core finding** (Dropumi analysis, not a vendor statement): if the HTML/JS is delivered over plain HTTP, an active attacker on the LAN can replace it. After that, every in-page protection runs attacker-chosen code: WebAssembly crypto, a DTLS-fingerprint check, a short authentication string shown by the page.

| Option | Status | Security property |
|---|---|---|
| Plain HTTP + session token | Requires investigation [F85][F86] | Not encrypted, and not a secure context. Must never be labelled secure. |
| HTTP page + WebRTC DataChannel, with the DTLS fingerprint passed by the native host | The DataChannel opens on all five browsers tested (ADR-0011, WEB-4); the security property is unchanged [F88][F90] | At best protects against passive eavesdroppers; no protection against an active attacker who replaces the page. |
| Per-session self-signed HTTPS, certificate fingerprint shown in a QR code | Requires investigation [F85]; partly measured (ADR-0011, WEB-3) | Sound only if the user really checks the fingerprint. With the key pinned, Chrome and Brave treat the page as a secure context. By hand, the click-through worked in Safari, Chrome and Brave on macOS, but the tester could not find the certificate fingerprint to compare it. Whether the page is a secure context after a click-through: not recorded. |
| Public-CA certificate for a domain that resolves to LAN IPs | Unsupported (project policy) | Violates the spec's no-public-dependency rule (master spec §6). |
| Installing a local CA on the visitor's device | Unsupported (project policy) | Too invasive; a long-lived trust anchor. |

**Recommendation**
- **General availability is postponed** until a secure transport is demonstrated. The Phase 6 spike tests the WebRTC and self-signed HTTPS options on Chrome, Firefox, Safari (iOS and macOS) and Android Chrome.
- Any interim build must be opt-in, off by default, short-lived and token-scoped. Its label must state exactly what it protects against:
  - WebRTC option: "encrypted against eavesdroppers on this Wi-Fi; not protected against someone actively controlling the network";
  - HTTP option: "not encrypted".

## 7. Phase 1 implications

**Discovery stays a port, with one adapter per platform family.**
- Desktop and CLI use `mdns-sd` behind the cargo feature `mdns` [F66]; see ADR-0007.
- Apple apps should use the system Bonjour APIs, because raw multicast on iOS needs an Apple-approved entitlement [F1][F7].
- Android should use `NsdManager`: its picker avoids the broad permission for outgoing connections [F29][F30].

**TXT records are hints; identity is the Noise static key.**
- mDNS records are unauthenticated, so the fingerprint hint only selects candidates (ADR-0004, ADR-0007).
- Manual `--to host:port` with a pinned fingerprint is always available. It is the fallback for networks that block multicast.

**The transport must accept an externally supplied stream later.**
- On iOS, Wi-Fi Aware sets up the data path and the app transfers data over a Network framework connection [F2].
- On Android, Wi-Fi Direct and Wi-Fi Aware connections are set up through Kotlin/Java platform APIs [F32][F33].
- The sans-IO `dropumi-protocol` crate keeps this possible.

**Dropumi's own authenticated encryption stays mandatory** on every link, including Wi-Fi Aware links that are already encrypted at the Wi-Fi layer [F4].

**Listener lifecycle must be explicit and restartable:**
- iOS closes listeners in the background [F25];
- Android 17 gates inbound connections behind a revocable permission [F30].

**Tests use loopback with manual addresses and `--no-mdns`.**
- TN3179 defines a local network as one on a broadcast-capable interface (Wi-Fi, Ethernet). It does not mention loopback [F1]. "Loopback is exempt" is therefore an **inference (UNVERIFIED)**.
- Command-line tools started from Terminal or SSH are exempt [F1]. Whether processes launched by CI runners are exempt: UNVERIFIED.
- runner-images#10924 is still open. A comment dated 2026-09-30 reports an IPv4 mDNS send failure on a `macos-26` arm64 runner, with the cause undiagnosed [F99]. mDNS e2e therefore stays opt-in and non-required (`mdns-e2e (experimental)`).

**Out of scope this milestone:** BLE, Wi-Fi Direct, Wi-Fi Aware, hotspot, QR, share integrations, background services, all UIs, FFI and the web companion (§9). None of them blocks the LAN baseline.

## 8. Risk register (top 12, most severe first)

| # | Severity | Risk | Mitigation |
|---|---|---|---|
| 1 | Critical | The iOS share-first flow (Photos → Share → Dropumi) may not finish large transfers within the Share Extension's memory and lifetime limits [F19][F20][F21]. | Phase 4 spike. Stream from item-provider file URLs and keep the extension UI open until done. Investigate `BGContinuedProcessingTask` from the containing app [F23]. Document the limits honestly. |
| 2 | Critical | No demonstrated public-API offline path between iPhone and Android [F2][F94][F95][F96]. | Ship LAN first. Phase 5 device tests cover the hotspot fallback and Wi-Fi Aware. No claim before tests pass. |
| 3 | High | Android 17 gates mDNS and inbound TCP behind `ACCESS_LOCAL_NETWORK` [F30]. The FGS type and Play policy for an idle receive mode are uncertain [F42] (UNVERIFIED). | Request the permission in context. Use the NSD picker for outgoing connections. Get an early Play policy review. |
| 4 | High | The web companion cannot be authenticated end to end over `http://<LAN-IP>` [F85][F87]. | GA postponed (§6). |
| 5 | High | Real-world LAN failures: Local Network denial on Apple platforms [F1], firewalls [F49], and AP/client isolation, multicast filtering or VPNs (frequency UNVERIFIED). | Manual address + fingerprint fallback; explicit error states; MSIX firewall rules. |
| 6 | Medium-High | Apple-managed entitlements: the multicast entitlement needs Apple's permission [F7]. | Use Bonjour APIs on Apple platforms; no raw mDNS responder on iOS. |
| 7 | Medium | BLE advertising is asymmetric across platforms, and iOS background advertisements are visible only to iOS [F16][F18][F54]. | BLE is a hint only; scanners accept every format; capabilities are exchanged after connecting. |
| 8 | Medium | Windows package identity needs a signing certificate trusted on the target PC [F56]. Cost and process for a FOSS project: UNVERIFIED. | Decide the signing route before Phase 2. The Send To fallback needs no identity [F60]. |
| 9 | Medium | A macOS background receiver (launchd agent) needs the Local Network privilege [F1]. | Use `SMAppService` and `AssociatedBundleIdentifiers` [F1][F26]. Behaviour of unsigned development builds: UNVERIFIED. |
| 10 | Medium | Linux fragmentation: GNOME needs an extension for tray icons [F78]; three different file-manager mechanisms [F73]–[F76]; the X11-only legacy tray [F79]. | Per-desktop documentation and graceful fallbacks in Phase 2. |
| 11 | Medium | CI evidence gap: mDNS on hosted runners [F99], and the loopback-exemption inference [F1]. | Required CI uses loopback with manual addresses; mDNS e2e is experimental and non-required. |
| 12 | Low-Medium | Platform churn: Multipeer Connectivity deprecated [F2]; Android local-network and FGS rules change with each release [F30][F44]; DMA-driven iOS changes [F97]. | Re-check on every OS release and record changes in ADRs. |

## 9. Implementation status this milestone

These labels describe Dropumi's own code and evidence, not vendor documentation. CI has not run yet: every GitHub-hosted runner row stays **Not tested (pending CI)** until its job is green. Runner image versions are listed in [`testing.md`](testing.md).

**Platforms and targets.**

| Target | What exists | Label | Evidence |
|---|---|---|---|
| macOS 27 arm64 (maintainer development machine, Apple M4 Pro, Rust 1.99.0) | `dropumi` CLI; full workspace test suite, including the two-process e2e suite on loopback (`127.0.0.1`) | **Automated test passed (CLI, loopback, local run)** | Full workspace test run: 224 passed, 0 failed, 1 ignored (the opt-in mDNS test). The e2e suite (37 tests, plus the ignored mDNS test) passed 3 times in a row, 31.61 to 31.73 s per run. The same suite passed in a Debian 12 aarch64 container (225 tests, including the Linux-only bounded-memory test). |
| Linux x86_64 (CI `ubuntu-latest`, Ubuntu 24.04) | Same CLI and test suite | **Not tested (pending CI)** | After a green `test (ubuntu-latest)` job: "Automated test passed (CLI, loopback, CI image)" |
| macOS arm64 (CI `macos-latest`, macOS 26) | Same | **Not tested (pending CI)** | Same, for `test (macos-latest)` |
| Windows x64 (CI `windows-latest`, Windows Server 2025) | Same | **Not tested (pending CI)** | Same, for `test (windows-latest)` |
| Windows x86_64 `x86_64-pc-windows-gnu` (cross-compiled with MinGW in a Debian 12 container on the maintainer machine) | Whole workspace, all targets | **Build verified only (cross-compiled and linked; not run)** | `cargo clippy -D warnings` (both feature sets) clean and `cargo build --all-targets --all-features` linked. Nothing was run. |
| Android `aarch64-linux-android` | Core library crates only (`dropumi-domain`, `-protocol`, `-application`, `-infra`, `--no-default-features`); no CLI, no app | **Build verified only (core library crates; no app, not run)** | Built locally with NDK 27.1 clang. Nothing was linked into an app or run on a device. CI job `build (aarch64-linux-android)`: Not tested (pending CI). |
| iOS `aarch64-apple-ios` | Same | **Build verified only (core library crates; no app, not run)** | Built locally with the Xcode 27 SDK. Nothing was linked into an app or run on a device. CI job `build (aarch64-apple-ios)`: Not tested (pending CI). |
| Real multi-device LAN or Wi-Fi | — | **Not tested** | No device lab in this milestone. The e2e tests above ran both peers on one machine. |

**Features** (any platform):

| Feature | Status |
|---|---|
| Authenticated, encrypted transfer over TCP with a manual address (`--to host:port`), SAS pairing, manual trust | Implemented in the CLI. **Automated test passed (CLI, loopback, local run)** on macOS 27 arm64; GitHub runners: **Not tested (pending CI)**. |
| mDNS discovery (`mdns` cargo feature, on by default in the CLI; `_dropumi._tcp`) | Implemented in the CLI. **Automated test passed (opt-in, loopback, local run on macOS 27)**: `mdns_discover_and_send_by_fingerprint` with `DROPUMI_TEST_MDNS=1`. CI job `mdns-e2e (experimental)` on `ubuntu-latest`: **Not tested (pending CI, experimental job)**. Multicast on a real LAN: **Not tested**. |
| BLE, Wi-Fi Direct, Wi-Fi Aware, Apple peer-to-peer Wi-Fi, hotspot, QR pairing | **Not implemented** |
| Share integrations, tray/menu bar, notifications, background receiver, start at login | **Not implemented** |
| Desktop and mobile UIs, FFI bindings, web companion | **Not implemented** |

The local loopback results do not settle the open questions in §7. Whether loopback is exempt from Local Network privacy remains an inference (UNVERIFIED) [F1], and mDNS on hosted runners remains unverified [F99].

## 10. References

All fetched and read on 2026-10-13.
- Apple developer-documentation pages were read through their DocC JSON data endpoints, which hold the same content as the listed pages.
- The BlueZ, `mdns-sd` and LocalSend documents were read from the raw files on GitHub.
- Pages in the Apple Documentation Archive may be out of date.

**Apple**
- [F1] Apple, "TN3179: Understanding local network privacy". https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy
- [F2] Apple, "TN3151: Choosing the right networking API". https://developer.apple.com/documentation/technotes/tn3151-choosing-the-right-networking-api
- [F3] Apple, "TN3111: iOS Wi-Fi API overview". https://developer.apple.com/documentation/technotes/tn3111-ios-wifi-api-overview
- [F4] Apple, "Wi-Fi Aware" (framework). https://developer.apple.com/documentation/wifiaware
- [F5] Apple, "Adopting Wi-Fi Aware". https://developer.apple.com/documentation/wifiaware/adopting-wi-fi-aware
- [F6] Apple, "com.apple.developer.wifi-aware". https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.wifi-aware
- [F7] Apple, "com.apple.developer.networking.multicast". https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.networking.multicast
- [F8] Apple, "Multipeer Connectivity". https://developer.apple.com/documentation/multipeerconnectivity
- [F9] Apple, "includePeerToPeer" (NWParameters). https://developer.apple.com/documentation/network/nwparameters/includepeertopeer
- [F10] Apple, "Wi-Fi configuration" (NEHotspotConfigurationManager). https://developer.apple.com/documentation/networkextension/wi-fi-configuration
- [F11] Apple, "NEHotspotConfiguration". https://developer.apple.com/documentation/networkextension/nehotspotconfiguration
- [F12] Apple, "joinOnce". https://developer.apple.com/documentation/networkextension/nehotspotconfiguration/joinonce
- [F13] Apple, "Core WLAN". https://developer.apple.com/documentation/corewlan
- [F14] Apple, "associate(to:password:)" (CWInterface). https://developer.apple.com/documentation/corewlan/cwinterface/associate(to:password:)
- [F15] Apple, "startIBSSMode(withSSID:security:channel:password:)" (CWInterface). https://developer.apple.com/documentation/corewlan/cwinterface/startibssmode(withssid:security:channel:password:)
- [F16] Apple, "startAdvertising(_:)" (CBPeripheralManager). https://developer.apple.com/documentation/corebluetooth/cbperipheralmanager/startadvertising(_:)
- [F17] Apple, "Core Bluetooth". https://developer.apple.com/documentation/corebluetooth
- [F18] Apple Documentation Archive, "Core Bluetooth Background Processing for iOS Apps". https://developer.apple.com/library/archive/documentation/NetworkingInternetWeb/Conceptual/CoreBluetooth_concepts/CoreBluetoothBackgroundProcessingForIOSApps/PerformingTasksWhileYourAppIsInTheBackground.html
- [F19] Apple Documentation Archive, "App Extension Programming Guide: Handling Common Scenarios". https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/ExtensionScenarios.html
- [F20] Apple Documentation Archive, "App Extension Programming Guide: Creating an App Extension". https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/ExtensionCreation.html
- [F21] Apple Documentation Archive, "App Extension Programming Guide: Understand How an App Extension Works". https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/ExtensionOverview.html
- [F22] Apple Documentation Archive, "App Extension Programming Guide: Share". https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/Share.html
- [F23] Apple, "Performing long-running tasks on iOS and iPadOS". https://developer.apple.com/documentation/backgroundtasks/performing-long-running-tasks-on-ios-and-ipados
- [F24] Apple, "BGContinuedProcessingTask". https://developer.apple.com/documentation/backgroundtasks/bgcontinuedprocessingtask
- [F25] Apple Documentation Archive, "Technical Note TN2277: Networking and Multitasking" (marked "no longer being updated"). https://developer.apple.com/library/archive/technotes/tn2277/_index.html
- [F26] Apple, "SMAppService". https://developer.apple.com/documentation/servicemanagement/smappservice
- [F27] Apple, "NSStatusItem". https://developer.apple.com/documentation/appkit/nsstatusitem
- [F28] Apple, "User Notifications". https://developer.apple.com/documentation/usernotifications

**Android**
- [F29] Android Developers, "Use network service discovery". https://developer.android.com/develop/connectivity/wifi/use-nsd
- [F30] Android Developers, "Local network permission". https://developer.android.com/privacy-and-security/local-network-permission
- [F31] Android API reference, "WifiManager.MulticastLock". https://developer.android.com/reference/android/net/wifi/WifiManager.MulticastLock
- [F32] Android Developers, "Wi-Fi Direct (peer-to-peer or P2P) overview". https://developer.android.com/develop/connectivity/wifi/wifip2p
- [F33] Android Developers, "Wi-Fi Aware overview". https://developer.android.com/develop/connectivity/wifi/wifi-aware
- [F34] Android API reference, "Characteristics" (`android.net.wifi.aware`). https://developer.android.com/reference/android/net/wifi/aware/Characteristics
- [F35] Android Developers, "Bluetooth Low Energy". https://developer.android.com/develop/connectivity/bluetooth/ble/ble-overview
- [F36] Android Developers, "Bluetooth permissions". https://developer.android.com/develop/connectivity/bluetooth/bt-permissions
- [F37] Android API reference, "BluetoothLeAdvertiser". https://developer.android.com/reference/android/bluetooth/le/BluetoothLeAdvertiser
- [F38] Android API reference, "BluetoothAdapter" (`getBluetoothLeAdvertiser`). https://developer.android.com/reference/android/bluetooth/BluetoothAdapter
- [F39] Android Developers, "Use a local-only Wi-Fi hotspot". https://developer.android.com/develop/connectivity/wifi/localonlyhotspot
- [F40] Android Developers, "Wi-Fi Network Request API for peer-to-peer connectivity". https://developer.android.com/develop/connectivity/wifi/wifi-bootstrap
- [F41] Android Developers, "Receive data from other apps". https://developer.android.com/training/sharing/receive
- [F42] Android Developers, "Foreground service types". https://developer.android.com/develop/background-work/services/fgs/service-types
- [F43] Android Developers, "Restrictions on starting a foreground service from the background". https://developer.android.com/develop/background-work/services/fgs/restrictions-bg-start
- [F44] Android Developers, "Behavior changes: Apps targeting Android 15 or higher". https://developer.android.com/about/versions/15/behavior-changes-15
- [F45] Android Developers, "Notification runtime permission". https://developer.android.com/develop/ui/views/notifications/notification-permission
- [F46] Android Developers, "User-initiated data transfer". https://developer.android.com/develop/background-work/background-tasks/uidt

**Windows**
- [F47] Microsoft Learn, "DnsServiceRegister function". https://learn.microsoft.com/en-us/windows/win32/api/windns/nf-windns-dnsserviceregister
- [F48] Microsoft Learn, "DnsServiceBrowse function". https://learn.microsoft.com/en-us/windows/win32/api/windns/nf-windns-dnsservicebrowse
- [F49] Microsoft Learn, "desktop2:FirewallRules". https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-desktop2-firewallrules
- [F50] Microsoft Learn, "Windows.Devices.WiFiDirect Namespace". https://learn.microsoft.com/en-us/uwp/api/windows.devices.wifidirect
- [F51] Microsoft Learn, "WiFiDirectAdvertisement.LegacySettings Property" and the namespace's `WiFiDirectLegacySettings` entry. https://learn.microsoft.com/en-us/uwp/api/windows.devices.wifidirect.wifidirectadvertisement.legacysettings
- [F52] Microsoft Learn, "NetworkOperatorTetheringManager Class". https://learn.microsoft.com/en-us/uwp/api/windows.networking.networkoperators.networkoperatortetheringmanager
- [F53] Microsoft Learn, "WiFiAdapter.ConnectAsync Method". https://learn.microsoft.com/en-us/uwp/api/windows.devices.wifi.wifiadapter.connectasync
- [F54] Microsoft Learn, "BluetoothLEAdvertisementPublisher Class". https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.advertisement.bluetoothleadvertisementpublisher
- [F55] Microsoft Learn, "BluetoothLEAdvertisementWatcher Class". https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.advertisement.bluetoothleadvertisementwatcher
- [F56] Microsoft Learn, "Grant package identity by packaging with external location manually". https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps
- [F57] Microsoft Learn, "Receive content in your app - integrate Windows Share". https://learn.microsoft.com/en-us/windows/apps/develop/windows-integration/integrate-sharesheet-receive
- [F58] Microsoft Learn, "Add a File Explorer context menu command to a packaged desktop app". https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/integrate-packaged-app-with-file-explorer
- [F59] Microsoft Learn, "KNOWNFOLDERID" (`FOLDERID_SendTo`). https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid
- [F60] The Old New Thing (Microsoft Dev Blogs), "How do I create a shortcut in the Send To menu that runs a program with a command line option?" (2017-04-03). https://devblogs.microsoft.com/oldnewthing/?p=95885
- [F61] Microsoft Learn, "Shell_NotifyIconW function". https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw
- [F62] Microsoft Learn, "Quickstart: Send and Handle App Notifications". https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-quickstart
- [F63] Microsoft Learn, "Run and RunOnce Registry Keys". https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys
- [F64] Microsoft Learn, "connect function (winsock2.h)". https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-connect

**Linux and cross-platform**
- [F65] Avahi project, "avahi - mDNS/DNS-SD". https://avahi.org/
- [F66] `mdns-sd` README (keepsimple1/mdns-sd). https://raw.githubusercontent.com/keepsimple1/mdns-sd/main/README.md
- [F67] Linux man-pages, "listen(2)". https://man7.org/linux/man-pages/man2/listen.2.html
- [F68] BlueZ, "org.bluez.LEAdvertisingManager". https://raw.githubusercontent.com/bluez/bluez/master/doc/org.bluez.LEAdvertisingManager.rst
- [F69] BlueZ, "org.bluez.Adapter". https://raw.githubusercontent.com/bluez/bluez/master/doc/org.bluez.Adapter.rst
- [F70] hostap, "wpa_supplicant and Wi-Fi P2P" (README-P2P). https://w1.fi/cgit/hostap/plain/wpa_supplicant/README-P2P
- [F71] hostap mailing list, "[PATCH 00/71] NAN Data Path and Bootstrapping support" (2026-04-01). https://lists.infradead.org/pipermail/hostap/2026-April/044670.html
- [F72] NetworkManager, "nmcli: NetworkManager Reference Manual". https://networkmanager.dev/docs/api/latest/nmcli.html
- [F73] nautilus-python, "Migrating to Nautilus API 4.0". https://gnome.pages.gitlab.gnome.org/nautilus-python/nautilus-python-migrating-to-4.html
- [F74] KDE Developer, "Creating Dolphin service menus". https://develop.kde.org/docs/apps/dolphin/service-menus/
- [F75] Xfce Docs, "Thunar - Custom Actions". https://docs.xfce.org/xfce/thunar/custom-actions
- [F76] Xfce Docs, "Thunar - Send To". https://docs.xfce.org/xfce/thunar/send-to
- [F77] freedesktop.org, "StatusNotifierItem" specification. https://www.freedesktop.org/wiki/Specifications/StatusNotifierItem/
- [F78] GNOME Shell Extensions, "AppIndicator and KStatusNotifierItem Support". https://extensions.gnome.org/extension/615/appindicator-support/
- [F79] freedesktop.org, "System Tray Protocol Specification" 0.3. https://specifications.freedesktop.org/systemtray-spec/latest/
- [F80] freedesktop.org, "Desktop Notifications Specification" 1.3. https://specifications.freedesktop.org/notification-spec/latest/
- [F81] XDG Desktop Portal, "org.freedesktop.portal.Background". https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Background.html
- [F82] freedesktop.org, "Desktop Application Autostart Specification" 0.5 (draft). https://specifications.freedesktop.org/autostart-spec/latest/
- [F83] IETF, RFC 6762 "Multicast DNS". https://www.rfc-editor.org/rfc/rfc6762.txt
- [F84] IETF, RFC 6763 "DNS-Based Service Discovery". https://www.rfc-editor.org/rfc/rfc6763.txt

**Web platform**
- [F85] MDN, "Secure contexts". https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts
- [F86] MDN, "Features restricted to secure contexts". https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts/features_restricted_to_secure_contexts
- [F87] MDN, "Crypto". https://developer.mozilla.org/en-US/docs/Web/API/Crypto
- [F88] MDN, "RTCPeerConnection". https://developer.mozilla.org/en-US/docs/Web/API/RTCPeerConnection
- [F89] Chrome for Developers, "New permission prompt for Local Network Access" (2025-06-09, updated 2025-09-29). https://developer.chrome.com/blog/local-network-access
- [F90] IETF, draft-ietf-mmusic-mdns-ice-candidates-03, "Using Multicast DNS to protect privacy when exposing ICE candidates". https://datatracker.ietf.org/doc/html/draft-ietf-mmusic-mdns-ice-candidates

**Interoperability reports and policy**
- [F91] Google Security blog, "Android Quick Share Support for AirDrop: A Secure Approach to Cross-Platform File Sharing" (2025-11-20). https://blog.google/security/android-quick-share-support-for-airdrop-security/
- [F92] Privacy Guides, "Google Expands Quick Share's AirDrop Support to Pixel 9 Series" (2026-02-23). https://www.privacyguides.org/news/2026/02/24/google-expands-quick-shares-airdrop-support-to-pixel-9-series/
- [F93] Samsung Mobile Press, "Samsung Brings AirDrop Support to Quick Share with Galaxy S26 Series" (2026-03-23). https://samsungmobilepress.com/articles/samsung-quick-share-airdrop-support-galaxy-s26-series
- [F94] Apple Developer Forums, thread 790195 (Wi-Fi Aware, iPhone ↔ Android; 2025). https://developer.apple.com/forums/thread/790195
- [F95] Apple Developer Forums, thread 801280 (Wi-Fi Aware pairing, iOS 26 ↔ Galaxy S25; 2025). https://developer.apple.com/forums/thread/801280
- [F96] Apple Developer Forums, thread 845528 (call for testers, Wi-Fi Aware library; September 2026). https://developer.apple.com/forums/thread/845528
- [F97] European Commission, "Apple specification proceedings - Interoperability for connected devices and request-based process" (DMA). https://digital-markets-act.ec.europa.eu/developer-portal/interoperability/apple-specification-proceedings-interoperability-connected-devices-and-request-based-process_en
- [F98] LocalSend, "LocalSend Protocol v2.2" (comparison only). https://raw.githubusercontent.com/localsend/protocol/main/README.md
- [F99] GitHub, actions/runner-images issue #10924, "add 'local network access' permission for macOS 15 runners" (open when fetched). https://github.com/actions/runner-images/issues/10924
