# Dropumi LAN discovery (mDNS / DNS-SD)

| | |
|---|---|
| Status | **Draft. Unaudited.** Optional component; manual addressing always works without it. |
| Applies to | Protocol v1 ([`v1.md`](v1.md)), reference implementation `dropumi-infra` (`mdns.rs`, `txt.rs`) and the `dropumi` CLI |
| Document date | 2026-10-14 (reconciled with the milestone 1 code; test status updated to the final local runs) |

The key words "MUST", "MUST NOT", "SHOULD", "SHOULD NOT" and "MAY" are used as defined in [`v1.md` §2.1](v1.md#21-requirements-notation).
Sections marked *(informative)* are not normative. Paragraphs marked *Reference:* describe what the reference
implementation does.

## Contents

1. [Purpose and principles](#1-purpose-and-principles)
2. [Build and runtime switches](#2-build-and-runtime-switches)
3. [Service type and naming](#3-service-type-and-naming)
4. [TXT record](#4-txt-record)
5. [Hint-only rule](#5-hint-only-rule)
6. [Interfaces and address families](#6-interfaces-and-address-families)
7. [Advertising and browsing behaviour](#7-advertising-and-browsing-behaviour)
8. [Resolving targets and choosing addresses](#8-resolving-targets-and-choosing-addresses)
9. [TTL, goodbye and stale entries](#9-ttl-goodbye-and-stale-entries)
10. [Privacy cost](#10-privacy-cost)
11. [Future work: rotating private hint](#11-future-work-rotating-private-hint)
12. [Platform caveats](#12-platform-caveats)
13. [Test status](#13-test-status)
14. [Changelog](#14-changelog)

## 1. Purpose and principles

- Discovery answers one question: "which nearby devices run a Dropumi receiver, and at which addresses and port?"
- Discovery and data transport are separate. A transfer always uses the authenticated protocol of [`v1.md`](v1.md),
  whichever way the address was found.
- **Discovery data is never identity** ([§5](#5-hint-only-rule)). Identity comes only from the Noise handshake.
- Discovery is optional. Manual addressing (`--to IPv4:port`, `--to [IPv6]:port`, `--to host:port`) always works, and
  every required automated test uses it (with `--no-mdns`).
- Discovery must stay cheap: no continuous scanning. Receivers only advertise; only commands that need a device list
  browse, and only for a bounded time.

## 2. Build and runtime switches

| Switch | Effect |
|---|---|
| Cargo feature `mdns` (default on, in `dropumi-infra` and `dropumi-cli`) | Compiles mDNS discovery using the pure-Rust `mdns-sd` crate (0.21.5). |
| Built without `mdns` | `discover`, and name or fingerprint-prefix targets for `send --to` and `pair --to`, exit with code 2 and a "this build has no mDNS discovery" message. `receive` and `pair --listen` run without advertising. Address targets keep working. *(informative)* The Android and iOS cross-builds of the core library crates use this configuration. |
| `--no-mdns` (`receive`, `pair`) | Do not advertise. The receiver is reachable by manual address only. With `pair --to` it also disables browsing, so only address targets work. |
| `--mdns-loopback` (hidden; `receive`, `pair`, `send`, `discover`) | Also use the IPv4 loopback interface, and accept loopback addresses from discovery. Intended for single-machine tests and benchmarks only. Has no effect in a build without `mdns`. |

## 3. Service type and naming

| Item | Value |
|---|---|
| Service type | `_dropumi._tcp.local.` ([RFC 6763]) |
| Service instance name | `dropumi-<8 hex>`: the prefix `dropumi-` followed by 8 random lowercase hexadecimal digits |
| Host name | `dropumi-<8 hex>.local.` |
| SRV port | The TCP port the receiver actually listens on (after any fallback from the default 47321 to an ephemeral port) |
| Address records | IPv4 addresses of the advertising interfaces ([§6](#6-interfaces-and-address-families)) |

- Instance and host labels MUST be generated at random each time a device starts advertising. They MAY share the same
  random value. They MUST NOT contain the OS host name, the user name or the device name.
- *Reference:* one random label (4 random bytes) is generated per advertisement and used for both the instance and the
  host name.
- *(informative)* A random host name also avoids host-name conflicts with the operating system's own mDNS responder.
- The device name is published only in the TXT key `n`, never in the instance name.
- The service name `dropumi` has not been registered with IANA. Registration is optional and can happen later.

## 4. TXT record

### 4.1 Keys

| Key | Value when advertising | Purpose |
|---|---|---|
| `v` | Protocol versions the device implements, as decimal numbers separated by commas, highest first. v1 implementations publish `v=1`. | Lets a browser flag peers without a common version before connecting. The preamble ([`v1.md` §6.2](v1.md#62-preamble)) remains authoritative. |
| `n` | The local device name, already sanitized, at most 64 UTF-8 bytes. | Human-readable label and name targets. |
| `fp` | The first 8 characters of the full fingerprint, upper case ([`v1.md` §5.3](v1.md#53-text-forms)). | Selector for fingerprint-prefix targets and for the peer expectation rule. |

- Keys MUST be published in lower case exactly as shown. No other keys are published.
- *(informative)* The default device name is `Dropumi-XXXX`, where `XXXX` is 4 random upper-case hexadecimal digits,
  of which at least one is 0, 1, 8 or 9 (the reference code replaces the last digit when none is), so a default name is
  never classified as a fingerprint prefix; it is never derived from the host name or the user name. `dropumi identity set-name` changes it.

### 4.2 Parsing rules

A browser MUST parse every TXT record defensively. A record that fails any rule below still lists the peer; only the
affected hint is dropped.

| Item | Rule |
|---|---|
| Duplicate keys | Only the first occurrence of a key is used ([RFC 6763] §6.4). *Reference:* `mdns-sd` 0.21.5 keeps the first occurrence of each key, compared case-insensitively (checked in the crate source). |
| Key matching | *Reference:* the parser recognizes the lower-case keys `v`, `n` and `fp` only; a key in another case is ignored (and, being first, hides a later lower-case duplicate). |
| Unknown keys | Ignored. |
| Key without `=`, or a value that is not valid UTF-8 | Treated as an empty value. |
| `v` | Split on `,`; each element is trimmed and parsed as a decimal integer 0..=65,535; elements that do not parse are skipped. Absent or empty: an empty (unknown) version list. |
| `n` | Device-name sanitization: removal of C0, DEL, C1, U+2028, U+2029 and every General Category Cf character except U+200C ZWNJ and U+200D ZWJ; NFC normalization; trimming; truncation to at most 64 bytes on a character boundary. An empty result, or an absent key, becomes "Unnamed device". |
| `fp` | MUST be exactly 8 characters from `A`–`Z` and `2`–`7`, upper case. Any other value (including lower case) is discarded: the peer is listed without a hint. |

*Reference:* browsing commands never advertise, so a browser does not see its own record; there is no self-filter.

### 4.3 Size

- Each TXT string (`key=value`) is at most 255 bytes by the DNS wire format ([RFC 6763] §6.1). *(informative)*
  `mdns-sd` 0.21.5 rejects longer properties when a service is registered (checked in the crate source).
- With the limits above, the whole TXT record is at most 83 bytes (`v=1`, `n=` plus 64 bytes, `fp=` plus 8 bytes, plus
  one length byte per string), well within the small-record guidance of [RFC 6763] §6.2.

## 5. Hint-only rule

- Everything learned through discovery (instance name, host name, addresses, port, `v`, `n`, `fp`) is
  **unauthenticated**. Any host on the link can publish any of it, including copies of another device's `n` and `fp`.
- Implementations MUST NOT treat discovery data as proof of identity, MUST NOT mark a peer as trusted or verified based
  on it, and MUST NOT display a discovered name as if it were an authenticated identity.
- Discovery data MAY be used only to:
  1. choose which addresses to connect to;
  2. derive the **peer expectation** for name and fingerprint-prefix targets ([`v1.md` §12.6](v1.md#126-peer-expectation-rule)),
     which is then checked against the authenticated static key before msg3;
  3. show a list of nearby devices, clearly labelled as discovered (unverified) names.
- *(informative)* The `fp` hint is 40 bits. It can be copied trivially, and a key whose fingerprint starts with a chosen
  hint can be ground with feasible effort. The hint is only useful because the peer expectation rule turns a hint that
  matches exactly one trust-store entry into a pin on that entry's full fingerprint.

## 6. Interfaces and address families

- **IPv4 only.** IPv6 interfaces are disabled for both advertising and browsing; no AAAA records are published, and
  IPv6 addresses in received records are ignored. (Manual `[IPv6]:port` targets still work.)
- **Loopback** interfaces are disabled unless the hidden `--mdns-loopback` flag is given.
- All other IPv4 interfaces are used. *(informative)* Interface selection otherwise follows the `mdns-sd` defaults.
  `mdns-sd` 0.21.5 re-checks interface addresses every 5 seconds by default (`IP_CHECK_INTERVAL_IN_SECS_DEFAULT`,
  checked in the crate source).
- *(informative)* `mdns-sd` binds UDP port 5353 with `SO_REUSEADDR` and, where the operating system provides it,
  `SO_REUSEPORT` (checked in the crate source), so it can share the port with the operating system's mDNS responder.
- *(informative)* The idle cost of advertising is **not measured**: the loopback benchmark measures its idle receiver
  with `--no-mdns`.

## 7. Advertising and browsing behaviour

| Command | Advertises | Browses |
|---|---|---|
| `dropumi receive` | Yes, unless `--no-mdns` | Never |
| `dropumi pair --listen` | Yes, unless `--no-mdns` | Never |
| `dropumi discover` | No | For `--timeout-ms` (default 3,000 ms), then stops |
| `dropumi send --to <name or fingerprint prefix>` | No | For `--discover-timeout-ms` (default 3,000 ms), then stops |
| `dropumi pair --to <name or fingerprint prefix>` | No | For 3 s (fixed), then stops |
| Any command with only address targets | No | No |

- There is no continuous background scanning. A browse runs only for the duration of a command that needs it.
- If advertising cannot start (for example UDP 5353 cannot be bound), the receiver emits `discovery_unavailable` (with
  the reason) and keeps running; manual addressing still works. If browsing cannot start, `discover`, `send` and
  `pair --to` exit with code 2.
- *Reference:* the adapter uses one lazily created `mdns-sd` daemon per process. A browse collects resolved and
  removed services until its timeout, keeps one entry per full instance name, and reports each discovered peer once,
  sorted by instance name, with its full instance name, sanitized name, `fp` hint, IPv4 addresses (sorted), port and
  version list (NDJSON event `peer_found`).

## 8. Resolving targets and choosing addresses

*Reference:* the CLI classifies a `--to` value in this order ([`docs/cli.md`](../cli.md)):

1. `IPv4:port` or `[IPv6]:port`: an address (port 0 is refused);
2. `host:port`, where the host consists of ASCII letters, digits, `-` and `.`: a host name, resolved by the operating
   system;
3. 8 to 52 base32 characters (`A`–`Z`, `a`–`z`, `2`–`7`) after removing `-` and spaces: a fingerprint prefix;
4. anything else: a device name.

*(informative)* Known limitation: a device name made only of such characters, with at least 8 of them (for example
"Living Room"), is classified as a fingerprint prefix and does not match by name. Address that device by IP address or
host name instead. Default names are never affected (see §4.1).

`send` accepts up to 8 `--to` values, which must then all be addresses or host names; a name or fingerprint target must
be the only target. Once classified:

| Target | Resolution |
|---|---|
| Device name | Browse, then compare the sanitized target with the sanitized `n` of each discovered instance, case-insensitively. Exactly one instance MUST match. No match is "not found" and several matches are "ambiguous"; both exit with code 2, and an ambiguous result lists the candidates. |
| Fingerprint prefix (8 to 52 base32 characters) | Browse and select instances whose `fp` hint equals the first 8 characters of the prefix (normalized to upper case). Exactly one instance MUST match, as for names. If the prefix also matches two or more trust-store entries, the target is ambiguous (exit 2). |
| Address or host name | No discovery. |

- The selected instance yields one connection candidate per IPv4 address, all with the advertised SRV port.
- Candidates are ranked: manual addresses first, in the order given; then loopback (only with `--mdns-loopback`); then
  private IPv4 addresses; then other IPv4 addresses; then link-local `169.254.0.0/16` addresses last. Duplicate
  addresses are tried once.
- Candidates are tried one at a time with a 3 s connect timeout each (`send --connect-timeout-ms` changes it). A
  refused or timed-out candidate (for example a stale record) is skipped and the next one is tried.
- Which address answered never decides identity. The peer expectation derived from the target
  ([`v1.md` §12.6](v1.md#126-peer-expectation-rule)) is checked against the authenticated static key before msg3:
  - a name target contributes the resolved instance's `fp` hint, and a pin on the full fingerprint if that hint matches
    exactly one trust entry; without a hint there is no fingerprint constraint;
  - a fingerprint-prefix target contributes the prefix itself, and a pin on the full fingerprint if the prefix matches
    exactly one trust entry.

## 9. TTL, goodbye and stale entries

- *(informative)* Record lifetimes are the `mdns-sd` 0.21.5 defaults, which follow [RFC 6762]: **120 s** for host-related
  records (SRV, A) and **4,500 s** for other records (PTR, TXT) (checked in the crate source).
- On a graceful stop the advertiser MUST withdraw its records. *Reference:* the adapter unregisters the service
  (waiting at most 1 s), which sends goodbye records with TTL 0 ([RFC 6762] §10.1), then shuts the daemon down (at
  most 1 s).
- After a crash or `SIGKILL` no goodbye is sent. Other devices can keep showing the entry until it expires from their
  caches. Senders treat a refused or timed-out connection as a stale entry and move on to the next candidate
  ([§8](#8-resolving-targets-and-choosing-addresses)).

## 10. Privacy cost

Advertising is a deliberate trade-off between convenience and privacy. Users should know exactly what is broadcast.

**Broadcast in cleartext to every device on the local link while a receiver advertises:**

| Data | Linkability |
|---|---|
| The fact that a Dropumi receiver is running, its IPv4 addresses and TCP port | Presence on this network |
| Instance and host names (`dropumi-<8 hex>`) | Random; change every time the device starts advertising |
| `v` (implemented protocol versions) | Same for every device on the same release; fingerprinting value is low |
| `n` (device name) | **Stable** until the user renames the device |
| `fp` (40-bit fingerprint hint) | **Stable** for the life of the identity key; links the device across networks and restarts with high confidence |

**Not broadcast:** the OS host name, the user name, the full fingerprint and the public key.

- *(informative)* The full public key is not secret in practice: anyone who can connect to a listening receiver can
  learn it from Noise msg2 ([`v1.md` §20](v1.md#20-security-considerations)). Hiding `fp` alone would therefore not
  stop an active observer from recognizing a receiver.
- Users who do not want to be visible can run `receive --no-mdns` (or `pair --listen --no-mdns`) and share the address,
  port and fingerprint that the receiver prints.
- Users should choose a device name that contains no personal information.
- Senders and `discover` do not advertise anything.

## 11. Future work: rotating private hint

*(informative; not part of v1)*

- Replace the stable `fp` hint with a value that only already-trusted devices can recognize, for example a truncated
  MAC over the current time epoch, keyed with a secret established during pairing. Strangers would then see a value
  that changes every epoch and cannot be linked.
- Consider publishing `n` only while Everyone mode or a pairing window is active.
- Either change needs pairing to establish a per-relationship secret and a new TXT key; it would be specified in a later
  revision of this document. It is planned for Phase 5 (advanced discovery).

## 12. Platform caveats

Status values use the project's labels. "UNVERIFIED" marks a statement that nobody has checked.

| Platform | Caveat | Status |
|---|---|---|
| All | Networks that filter multicast or isolate clients (guest Wi-Fi, some enterprise networks) prevent discovery entirely. Use manual `--to host:port`. | Not tested |
| All | Host firewalls can block UDP 5353 or the receiver's TCP port. | Not tested |
| All | Discovery between two real devices on a LAN or Wi-Fi network. | Not tested |
| macOS 27 arm64 (maintainer machine) | Opt-in two-process mDNS test on the loopback interface, run from a terminal. | Automated test passed (opt-in, loopback, local run on macOS 27) |
| macOS | Local Network privacy applies on macOS 15 and later. Apple's TN3179 documents that command-line tools run from Terminal or SSH are exempt ([`feasibility.md`](../feasibility.md) §7); this has not been tested with the `dropumi` CLI on a real LAN. Whether loopback traffic is exempt is UNVERIFIED: TN3179 does not mention loopback. The local loopback run above shows only that this one configuration worked. | Requires investigation |
| macOS | A future app bundle will need `NSLocalNetworkUsageDescription` and an `NSBonjourServices` entry for `_dropumi._tcp` (UNVERIFIED; no app exists yet). | Not applicable |
| macOS 26 arm64 (GitHub-hosted runner) | [actions/runner-images#10924](https://github.com/actions/runner-images/issues/10924) is an open request to grant Local Network permission on macOS 15 runner images. A comment dated 2026-09-30 reports an IPv4 mDNS send failure on a `macos-26` arm64 runner, without attributing it to Local Network privacy. Whether mDNS works on these runners is UNVERIFIED. | Not tested (no CI job: the experimental mDNS job runs on `ubuntu-latest` only) |
| Linux | Coexistence with Avahi is untested. If Avahi is configured to refuse other mDNS stacks, binding may fail, in which case Dropumi reports `discovery_unavailable`. systemd-resolved with multicast DNS enabled is another competing stack. | Requires investigation |
| Ubuntu 24.04 x64 (GitHub-hosted runner) | Whether multicast works on these runners is unknown; it is exercised only by the non-required `mdns-e2e (experimental)` job. | Not tested (pending CI, experimental job) |
| Windows | Windows Defender Firewall may prompt for, or block, the first listening socket. Sharing UDP 5353 with the system DNS client service is untested. Windows also has native DNS-SD APIs (`DnsServiceRegister`, `DnsServiceBrowse`) that a future native adapter could use. | Requires investigation |
| iOS / iPadOS | No app in this milestone. Raw multicast on iOS needs a restricted entitlement (according to the design research), so a future iOS app would use Bonjour through Apple's Network framework in the platform layer instead of `mdns-sd`. | Not applicable |
| Android | No app in this milestone. A future Android app would use the platform's Network Service Discovery (NSD). | Not applicable |

Any future native discovery adapter (Bonjour, NSD, Windows DNS-SD) MUST publish the same service type and TXT keys
and MUST follow the parsing and hint-only rules of this document, so that all implementations interoperate.

## 13. Test status

Test names follow the convention of [`threat-model.md` §1](threat-model.md#1-scope-and-method). All tests listed below
except the opt-in one ran in the local workspace run on macOS 27 arm64 (Rust 1.99.0), where all 224 non-ignored tests
passed; CI has not run yet.

| Layer | Tests | Status |
|---|---|---|
| TXT building and parsing | `infra txt::tests::txt_parse_valid`, `infra txt::tests::txt_rejects_bad_fp_hint`, `infra txt::tests::txt_sanitizes_name_64b` | Automated test passed (local run on macOS 27) |
| Target parsing and resolution | `domain discovery::tests::parse_targets`, `domain discovery::tests::resolve_target_ambiguous` | Automated test passed (local run on macOS 27) |
| Candidate ranking and dialing | `domain connection::tests::rank_candidates_order`, `application/failures connect_tries_candidates_in_rank_order`, `infra net::tests::connect_first_skips_dead_candidate`, `e2e failure::retry_skips_dead_candidate` | Automated test passed (local run on macOS 27) |
| Default-port fallback | `infra net::tests::listener_falls_back_to_ephemeral_when_port_busy` | Automated test passed (local run on macOS 27) |
| Name binding before msg3 (fake discovery) | `domain connection::tests::expectation_rules`, `application/channel name_target_binds_fp_hint`, `application/channel fp_prefix_unique_trust_pins_full_key` | Automated test passed (local run on macOS 27) |
| Real mDNS between two processes | `e2e discovery::mdns_discover_and_send_by_fingerprint`: ignored by default; runs with `DROPUMI_TEST_MDNS=1 cargo test -p dropumi-cli --test e2e --all-features --locked -- --ignored mdns_` and uses `--mdns-loopback` | Automated test passed (opt-in, loopback, local run on macOS 27); Not tested (pending CI, experimental job) on Ubuntu 24.04 x64 (`ubuntu-latest`); no CI job runs it on macOS or Windows runners |
| Discovery between real devices on a LAN | none | Not tested |

- Required CI never depends on multicast: every required end-to-end test uses `--no-mdns` and loopback addresses.
- Results are recorded in [`docs/testing.md`](../testing.md), pass or fail.

## 14. Changelog

| Date | Change |
|---|---|
| 2026-10-13 | Initial draft for protocol v1 (milestone 1). |
| 2026-10-13 | Reconciled with the reference implementation: lower-case `fp` hints are rejected rather than normalized; invalid `v` elements are skipped individually; keys are matched in lower case only; `pair --to` browses for a fixed 3 s; behaviour without the `mdns` feature and on browse failure corrected; removed the browse event queue that does not exist; platform caveats use the status labels; corrected the description of actions/runner-images#10924; real test names and local results. |
| 2026-10-14 | Default names always contain a 0, 1, 8 or 9 (§4.1) and are never classified as fingerprint prefixes; the default-name note in the target rules updated; test status updated to the final local runs. |

[RFC 6762]: https://www.rfc-editor.org/rfc/rfc6762
[RFC 6763]: https://www.rfc-editor.org/rfc/rfc6763
