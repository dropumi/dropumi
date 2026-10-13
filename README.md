# dropumi
Free, open-source, ultra-lightweight cross-platform local file sharing. No internet required.

[![CI](https://github.com/dropumi/dropumi/actions/workflows/ci.yml/badge.svg)](https://github.com/dropumi/dropumi/actions/workflows/ci.yml)

**Dropumi** — Just drop it nearby.

> Pre-alpha. Protocol and implementation are unaudited. Do not use for sensitive data yet.

## Principles

- **Local-only.** Files go straight from one device to another on the same network. There is no relay and no internet dependency.
- **No accounts.** There is nothing to sign up for. A device is identified by a key it generates itself.
- **No cloud.** No server stores, relays or indexes your files.
- **No telemetry.** No analytics, crash reporting or update checks.
- **Free forever.** No subscriptions, ads or premium tiers.
- **Open source** under the [Mozilla Public License 2.0](LICENSE) (MPL-2.0).

## What works today

Milestone 1 provides the Rust core, protocol v1 (draft) and the `dropumi` command-line tool. The CLI can create a device identity, pair two devices by comparing a 6-digit code, manage trusted devices, and send and receive files over an authenticated, encrypted TCP connection. Receivers can advertise themselves over mDNS, so senders can find them by name or fingerprint as well as by address.

| Platform or environment | Status |
|---|---|
| macOS 27 arm64 (maintainer machine) | Automated test passed (CLI, loopback, local run) |
| Linux aarch64 (Debian 12 container on the maintainer machine) | Automated test passed (CLI, loopback, local run) |
| Ubuntu 24.04 x64 (GitHub runner) | Not tested (pending CI) |
| macOS 26 arm64 (GitHub runner) | Not tested (pending CI) |
| Windows Server 2025 x64 (GitHub runner) | Not tested (pending CI) |
| Windows x86_64 (`x86_64-pc-windows-gnu`, cross-compiled and linked in the Linux container) | Build verified only (not run) |
| Android aarch64 | Build verified only (core library crates; no app, not run) |
| iOS aarch64 | Build verified only (core library crates; no app, not run) |
| Real multi-device LAN | Not tested |
| mDNS discovery, macOS 27 arm64 | Automated test passed (opt-in, loopback, local run on macOS 27) |
| mDNS discovery, Ubuntu 24.04 x64 (GitHub runner) | Not tested (pending CI, experimental job) |
| Desktop and mobile apps | Not implemented |

- "Loopback" means both devices were two `dropumi` processes on one machine, talking over `127.0.0.1`.
- In the local run on macOS 27 (Rust 1.99.0), 224 tests passed, including the two-process end-to-end suite (37 tests, green 3 times in a row); the same suite passed in a Linux aarch64 container (225 tests, including a Linux-only one). The one ignored test is the opt-in mDNS test, which passed when run separately with `DROPUMI_TEST_MDNS=1` (see [`docs/testing.md`](docs/testing.md)).
- Nothing has been tried between two real devices yet. See [`docs/testing.md`](docs/testing.md) and [`docs/feasibility.md`](docs/feasibility.md) §9.

## Quick start

Build with stable Rust. The workspace declares a minimum of Rust 1.89 (`cargo +1.89.0 check` passed locally), and the local test run used 1.99.0.

```sh
cargo build --release --locked -p dropumi-cli
```

The binary is `target/release/dropumi`. The examples below assume it is on your `PATH`.

**1. Check each device's identity.** The first run creates the identity key and a random device name such as `Dropumi-1A2B`.

```sh
dropumi identity show
dropumi identity set-name "Desk PC 1"      # optional; other devices on the network can see this name
```

**2. Pair the two devices once.**

```sh
# Device B: wait for a pairing attempt (default window: 5 minutes)
dropumi pair --listen

# Device A: use B's LAN IP address and the port B printed (default 47321)
dropumi pair --to 192.168.1.20:47321
```

Both devices show a 6-digit verification code. Confirm on each device **only if the two codes match**. Each device then stores the other as trusted.

**3. Receive on device B.** The directory must already exist.

```sh
dropumi receive --dir ./incoming --accept trusted
```

`--accept trusted` accepts offers from paired devices without asking. The default policy, `trusted-only`, refuses everyone else.

**4. Send from device A.**

```sh
dropumi send photo.jpg report.pdf --to 192.168.1.20:47321
```

Other options:
- `--to` also accepts a device name or a fingerprint prefix, found over mDNS. A name made only of letters, digits 2-7, spaces and hyphens, with 8 to 52 such letters and digits (such as "Living Room"), is read as a fingerprint prefix; see [target syntax](docs/cli.md#target-syntax). Default names (`Dropumi-XXXX`) always contain a 0, 1, 8 or 9, so they always work as names.
- `--expect-fingerprint` pins the receiver's key.
- `--json` writes machine-readable NDJSON events.

The full CLI reference (commands, flags, NDJSON events and exit codes) is in [`docs/cli.md`](docs/cli.md).

## Security model

- **Always encrypted and authenticated.** After a short version preamble, everything is protected by Noise XX (`Noise_XX_25519_ChaChaPoly_SHA256`). There is no plaintext mode and no fallback. See [`docs/protocol/v1.md`](docs/protocol/v1.md) and [ADR-0002](docs/adr/0002-secure-channel-noise-xx.md).
- **A device is its key, not its name.** Each device has a static X25519 key, and its 52-character fingerprint is the identity. Device names and mDNS data are only hints. See [ADR-0003](docs/adr/0003-device-identity-and-key-storage.md).
- **Trust is never assumed (no trust on first use).** A device becomes trusted only through SAS pairing (comparing the 6-digit code) or by adding its full fingerprint by hand.
  - Receivers default to `trusted-only`.
  - An unverified peer is never accepted automatically.
  - A sender checks the receiver's key before it reveals its own identity.

  See [ADR-0004](docs/adr/0004-pairing-trust-and-transfer-policy.md).
- **Received files are handled defensively.**
  - Names are sanitized.
  - Data goes to a new `.dropumi-*.part` temp file and is hashed with BLAKE3 as it is written.
  - Only a file whose hash matches is fsynced and finalized. Finalizing never overwrites an existing file and never follows symlinks.
  - On Unix, received files are created with mode 0600.

  See [ADR-0006](docs/adr/0006-integrity-and-receiver-filesystem-safety.md).
- **Peer input is bounded and treated as untrusted.**
  - Every message is size-limited before allocation, and CBOR decoding is strict. An offer may hold at most 10,000 files and 4 MiB of metadata.
  - Peer strings are stripped of control and bidirectional characters before they are shown.
  - Errors sent to a peer are fixed phrases that never include local paths.

  See the [threat model](docs/protocol/threat-model.md) and [ADR-0005](docs/adr/0005-wire-protocol-v1.md).

All decisions are listed in [`docs/adr/`](docs/adr/). To report a vulnerability, see [SECURITY.md](SECURITY.md).

## Privacy

- **No accounts, telemetry, cloud or update checks.** Dropumi connects only to the peer address you give it or that discovery finds. A host name passed as `--to host:port` is resolved by your operating system's resolver.
- **What mDNS advertises.** While `receive` or `pair --listen` is running, the CLI advertises `_dropumi._tcp` on the local network. This advertising is on by default. It sends:
  - a random instance name (`dropumi-` plus 8 hex digits), new on every run;
  - the device's IPv4 addresses and TCP port;
  - `v`: the protocol versions it implements;
  - `n`: the device name (at most 64 bytes);
  - `fp`: an 8-character fingerprint hint.
- **What stays stable.** The device name and the fingerprint hint stay the same across runs, until you rename the device or reset its identity. Other devices on the same network can therefore recognize your device. Choose a device name that contains no personal information.
- **What is never advertised:** the OS host name, the user name, the full fingerprint and the public key.
- **To advertise nothing, use `--no-mdns`** on `receive` or `pair --listen`. Then share the address, port and fingerprint yourself. Senders and `discover` never advertise.
- **Nothing else leaves the local network.** Apart from that host-name lookup, traffic stays local unless you point `--to` at an address outside it.

Details: [`docs/protocol/discovery.md`](docs/protocol/discovery.md) §10.

## Performance

These are medians from one local run of [`scripts/bench/loopback.sh`](scripts/bench/loopback.sh) on 2026-10-14.

- **Machine:** Apple M4 Pro, macOS 27 (Darwin 27.0.0 arm64), Rust 1.99.0, APFS.
- **Parameters:** a 256 MiB large file, 1000 × 100 KiB small files, 3 runs, 20 startup runs, 60 s idle window, `--mdns`.
- **Loopback on one machine (`127.0.0.1`), not a LAN or Wi-Fi measurement.** It says nothing about other hardware.

| Metric | Median of runs |
|---|---|
| Stripped release binary | 2819248 bytes |
| Startup, first run after build (page cache likely warm; not a true cold start) | 9 ms |
| Startup, warm (median / p90) | 9 / 10 ms |
| Large file throughput | 386.1 MiB/s |
| Small files throughput | 20.3 MiB/s |
| Handshake latency (preamble to both Hellos) | 615 µs |
| Sender peak RSS (large / small) | 4416 / 6112 KiB |
| Receiver peak RSS (large / small) | 4400 / 6720 KiB |
| Sender CPU time, user+sys (large / small) | 0.55 / 0.43 s |
| Receiver CPU time, user+sys (large / small) | 0.54 / 0.43 s |
| Idle receiver RSS (after 10 s) | 3744 KiB |
| Idle receiver CPU over 60 s | 0.00 s |
| Cancel-to-exit latency (both processes; includes ~10 ms command polling) | 26 ms |
| mDNS discovery latency | not measured (device found within the 5000 ms browse window; latency is not reported) |

Not measured: battery, mobile devices, real LAN or Wi-Fi, comparison with other tools, true cold start.

The full results, with raw per-run data, are in [`docs/benchmarks/results/2026-10-14-macos27-m4pro-loopback.md`](docs/benchmarks/results/2026-10-14-macos27-m4pro-loopback.md). The method is described in [`docs/benchmarks/`](docs/benchmarks/).

## Documentation

| Document | Contents |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Crates, layers, ports and data flow |
| [docs/cli.md](docs/cli.md) | CLI commands, flags, NDJSON events and exit codes |
| [docs/protocol/v1.md](docs/protocol/v1.md) | Wire protocol v1 (draft) |
| [docs/protocol/threat-model.md](docs/protocol/threat-model.md) | Threat model |
| [docs/feasibility.md](docs/feasibility.md) | Platform feasibility research and current status |
| [docs/adr/](docs/adr/) | Architecture decision records |
| [docs/testing.md](docs/testing.md) | How Dropumi is tested and what has been verified |
| [docs/development.md](docs/development.md) | Building and developing locally |
| [docs/dependencies.md](docs/dependencies.md) | Dependencies and their licenses |
| [docs/benchmarks/](docs/benchmarks/) | Benchmark method and results |

## Maintainers

- [@fahmialfareza](https://github.com/fahmialfareza) (Fahmi Alfareza)
- [@santoadji21](https://github.com/santoadji21) (Ahmad Aji Santoso)

See [CONTRIBUTING.md](CONTRIBUTING.md#maintainers).

## Contributing

Contributions are welcome. Start with these:
- [CONTRIBUTING.md](CONTRIBUTING.md): setup, workflow and the checks to run;
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md): expected behaviour;
- [SECURITY.md](SECURITY.md): reporting vulnerabilities privately (never in a public issue).

## License

Dropumi is licensed under the [Mozilla Public License 2.0](LICENSE) (MPL-2.0).
