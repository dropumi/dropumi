# ADR-0007: LAN discovery via mDNS

- **Status:** Accepted
- **Date:** 2026-10-13 (reconciled with the milestone 1 code on 2026-10-13)
- **Related:** ADR-0001, ADR-0003, ADR-0004, [`../protocol/discovery.md`](../protocol/discovery.md), [`../feasibility.md`](../feasibility.md)

## Context

- Mode A of the specification (existing local network) needs devices to find each other without servers or accounts. mDNS/DNS-SD (RFC 6762, RFC 6763) is the standard mechanism on LANs.
- The specification forbids treating any unauthenticated advertisement or device name as proof of identity, asks to keep discovery separate from transport, to minimise idle work, and to use feature flags so unused platform code is not compiled.
- Phase 1 is a CLI on desktop operating systems. Mobile apps will use native discovery APIs later (on iOS, raw multicast needs the Apple-approved multicast entitlement), so the Rust mDNS adapter must be optional.
- Discovery must never be the only way to connect: multicast can be blocked by networks, firewalls or CI runners.
- Multicast on GitHub-hosted runners is uncertain. actions/runner-images#10924 is an open request to grant the Local Network permission on macOS 15 runners; a comment dated 2026-09-30 reports an IPv4 mDNS send failure on a `macos-26` arm64 runner, with no cause diagnosed (sources in [`feasibility.md`](../feasibility.md)). Whether `macos-latest` and `ubuntu-latest` deliver same-host multicast is UNVERIFIED.
- macOS Local Network privacy applies from macOS 15 and exempts command-line tools run from Terminal or SSH (TN3179). TN3179 does not mention loopback, so "loopback traffic is exempt" is an inference and UNVERIFIED.

## Decision

**Library and feature flag.** `mdns-sd` 0.21.5 (`Apache-2.0 OR MIT`) with `default-features = false, features = ["async"]`: pure Rust, browses and responds, no system daemon. It is optional behind the cargo feature `mdns`, on by default:
- `dropumi-infra`: `default = ["mdns"]`, `mdns = ["dep:mdns-sd"]`.
- `dropumi-cli`: `default = ["mdns"]`, `mdns = ["dropumi-infra/mdns"]`.
- Built without it: `discover` and name or fingerprint `--to` targets fail with a usage error (exit 2, "this build has no mDNS discovery"); `receive` and `pair --listen` simply do not advertise; `--mdns-loopback` has no effect; address targets keep working. The Android and iOS cross-builds of the core crates use `--no-default-features`.

**Service and naming.** Service type `_dropumi._tcp.local.`. Each advertisement uses a random instance name `dropumi-<8 lowercase hex>` and the same random label as host name (`dropumi-<8 hex>.local.`); the OS hostname is never announced.

**TXT record: hints only** (`crates/infra/src/txt.rs`).

| Key | Value written | Parsing |
|---|---|---|
| `v` | Protocol versions, comma-separated (`1`) | Comma-separated `u16`s; invalid entries skipped |
| `n` | Device name, truncated to 64 bytes | `DeviceName::sanitize` (ADR-0003, ADR-0006); missing → `Unnamed device` |
| `fp` | First 8 base32 characters of the fingerprint | Must be exactly 8 characters from `A-Z2-7`, otherwise the hint is dropped and the peer is still listed |

- Unknown keys are ignored.
- **Hint-only rule:** TXT values and names are never identity. Identity always comes from the static key authenticated in the Noise handshake (ADR-0002). The `fp` hint only selects candidates and seeds the target expectation that is verified before msg3 (ADR-0004).

**Interfaces and lifecycle** (`crates/infra/src/mdns.rs`).
- IPv4 only in v1: the daemon disables IPv6 interfaces, `browse` collects only IPv4 addresses, and discovered IPv6 candidates are dropped by the ranking. The IPv4 loopback interface is disabled unless the hidden `--mdns-loopback` flag is given (`receive`, `pair`, `send`, `discover`; for single-machine tests and benchmarks).
- One `ServiceDaemon` per `MdnsDiscovery` value, created lazily on first use (one per CLI process).
- `browse` reads `mdns-sd`'s own bounded event channel with `recv_async` until the deadline, keeps the last resolved record per instance, removes instances reported as removed, then calls `stop_browse`.
- Stopping an advertisement (`AdHandle::stop`) unregisters the service, which sends goodbye packets, and waits at most 1 s. Dropping `MdnsDiscovery` shuts the daemon down and waits at most 1 s.
- A failure to start or register becomes `DiscoveryError::Unavailable`. For `receive` and `pair --listen` the CLI emits `discovery_unavailable` and continues with manual addressing; for a name or fingerprint target the command fails with exit 2.

**Idle behaviour.** `receive` and `pair --listen` advertise but never browse. `discover` (`--timeout-ms`, default 3,000 ms), `send` with a name or fingerprint target (`--discover-timeout-ms`, default 3,000 ms) and `pair --to` (fixed 3 s) browse once and then stop. There is no continuous scanning.

**Manual addressing always works** (Mode D):
- `--to IPv4:port | [IPv6]:port | host:port`; `send` accepts `--to` up to 8 times (addresses only), tried in the given order.
- Candidates are ranked: manual (user order) > loopback (only with `--mdns-loopback`) > private IPv4 > other IPv4 > 169.254/16, and tried sequentially with a 3 s connect timeout each (`--connect-timeout-ms`).
- `receive` reports its listening addresses, port and full fingerprint. The default port is 47321; only when that default is busy and no `--port` was given does it fall back to an ephemeral port, which mDNS then advertises.

**CI.** Every required test uses loopback TCP with `--no-mdns` and `--to 127.0.0.1:<port>`. The real two-process mDNS test `discovery::mdns_discover_and_send_by_fingerprint` is `#[ignore]`d, runs only with `DROPUMI_TEST_MDNS=1` (with `--mdns-loopback`), and executes in the non-required job `mdns-e2e (experimental)` (`ubuntu-latest`, `continue-on-error: true`).

## Implementation

- Code: [`crates/infra/src/mdns.rs`](../../crates/infra/src/mdns.rs), [`crates/infra/src/txt.rs`](../../crates/infra/src/txt.rs), [`crates/infra/src/net.rs`](../../crates/infra/src/net.rs) (default port and fallback), [`crates/domain/src/discovery.rs`](../../crates/domain/src/discovery.rs) (targets), [`crates/domain/src/connection.rs`](../../crates/domain/src/connection.rs) (`rank_candidates`), [`crates/application/src/usecases/discovery.rs`](../../crates/application/src/usecases/discovery.rs), [`crates/application/src/usecases/connect.rs`](../../crates/application/src/usecases/connect.rs), [`crates/cli/src/commands/mod.rs`](../../crates/cli/src/commands/mod.rs) (`NoDiscovery` without the feature).
- Status:

| Check | Result |
|---|---|
| Unit tests `txt_parse_valid`, `txt_rejects_bad_fp_hint`, `txt_sanitizes_name_64b`, `rank_candidates_order`, `listener_falls_back_to_ephemeral_when_port_busy` | Automated test passed (local run, macOS 27 arm64) |
| `discovery::mdns_discover_and_send_by_fingerprint` | Automated test passed (opt-in, loopback, local run on macOS 27) |
| Same test on the Ubuntu 24.04 x64 runner (`ubuntu-latest`) | Not tested (pending CI, experimental job); no CI job runs it on macOS or Windows runners |
| Discovery between two devices on a real LAN | Not tested |
| Discovery latency | Not measured (the benchmark found the device within its 5,000 ms browse window but does not report latency) |

## Consequences

### Positive

- Zero-configuration discovery on desktop LANs without installing Avahi or Bonjour.
- Spoofed advertisements cannot impersonate a device: at worst they waste a connection attempt, which then fails authentication or expectation checks.
- Low idle cost: advertising only, browsing on demand.
- Mobile and minimal builds can drop the mDNS code entirely.

### Negative

- **Privacy cost:** `n` and `fp` are stable values broadcast in cleartext to the whole LAN, which allows tracking a device across networks. A rotating private hint is future work (Phase 5).
- After a crash or `SIGKILL` no goodbye is sent, so peers may show a stale entry until the record's TTL expires; connecting to it simply fails.
- A browsing process does not filter out its own advertisement (in milestone 1 no command both advertises and browses).
- IPv6 discovery is deferred.
- Coexistence with Avahi on Linux, Windows Defender Firewall prompts on first listen, and Local Network privacy behaviour on CI runners are UNVERIFIED.
- Real multi-device LAN discovery is not covered by required CI.

## Alternatives considered

- **zeroconf:** wraps Bonjour/Avahi and needs system libraries and daemons.
- **libmdns:** responder only; cannot browse.
- **searchlight:** stale.
- **simple-mdns:** much smaller user base; kept only as a fallback candidate.
- **Native OS APIs (Avahi D-Bus, Windows DNS-SD, Apple NWBrowser, Android NsdManager) now:** each is a separate platform adapter; planned for the app phases behind the same `Discovery` port.
- **Continuous background browsing:** costs idle CPU and battery for no CLI benefit.
- **Announcing the OS hostname:** leaks a personal identifier and can trigger host-name conflicts.
