# Benchmarks

> Dropumi is pre-alpha. Every number published here comes from a real run of the script below, on the machine named in the result file. These are loopback numbers. They are not LAN or Wi-Fi numbers, and they say nothing about other hardware.

## Results index

| Date | Machine | Environment | Parameters | Result |
|---|---|---|---|---|
| 2026-10-14 | Apple M4 Pro, macOS 27 (Darwin 27.0.0 arm64), Rust 1.99.0, APFS | Loopback (`127.0.0.1`), one machine, local run | 256 MiB large file, 1000 × 100 KiB small files, 3 runs, 20 startup runs, 60 s idle window, `--mdns` | [2026-10-14-macos27-m4pro-loopback.md](results/2026-10-14-macos27-m4pro-loopback.md) |

## Running the benchmark

```sh
bash scripts/bench/loopback.sh [--size-mib 1024] [--small-count 1000] [--small-kib 100] \
  [--runs 3] [--startup-runs 20] [--idle-secs 60] [--out bench-out] [--mdns]
```

| Option | Default | Meaning |
|---|---|---|
| `--size-mib N` | `1024` | Size of the single large file, in MiB |
| `--small-count N` | `1000` | Number of small files |
| `--small-kib N` | `100` | Size of each small file, in KiB |
| `--runs N` | `3` | Transfer runs per workload (large and small alternate) |
| `--startup-runs N` | `20` | Startup samples (run 1 is reported separately) |
| `--idle-secs N` | `60` | Length of the idle-receiver CPU window, in seconds |
| `--out DIR` | `bench-out` | Output directory for `startup.tsv`, `transfers.tsv` and `summary.md` (`/bench-out/` is git-ignored) |
| `--mdns` | off | Also check mDNS discovery on loopback |

**Requirements**
- bash on macOS or Linux. There is no Windows variant.
- `cargo`, `jq`, `perl` (with `Time::HiRes`) and `/usr/bin/time`.
- Free disk space for the inputs plus one received copy: the large file, the small-file set, and the larger of the two again.
  - With the defaults that is a little over 2 GiB (derived from the script, not measured).
  - Lower `--size-mib` on a small disk, and say so in the result file.

**What the script does**
1. Builds the release binary with `cargo build --release --locked -p dropumi-cli`, using default features (`mdns` included). The release profile uses thin LTO, `codegen-units = 1` and `strip = "symbols"`.
2. Creates two temporary config directories with `mktemp -d` (mode 0700). It names the identities "Bench sender" and "Bench receiver" and trusts each one from the other with `trust add <full fingerprint>`.
3. Generates random input files from `/dev/urandom` in a temporary directory.
4. For each transfer, starts a fresh receiver and a fresh sender, both wrapped in `/usr/bin/time`:
   - receiver: `--json receive --bind 127.0.0.1 --port 0 --accept trusted --once --no-mdns`, into a fresh directory;
   - sender: `--json send … --to 127.0.0.1:<port>`.
5. Deletes all temporary data on exit. The user's real configuration directory is never touched.

## Metrics

| Metric | How it is measured | What it means |
|---|---|---|
| Stripped release binary | `wc -c target/release/dropumi` | On-disk size of the CLI binary. There is no installer, so installer size is not applicable. |
| Startup, first run | Wall-clock time of `dropumi --config-dir <tmp> identity show`, timed with `perl` `Time::HiRes` millisecond timestamps | The first startup sample after the build. By then the script has already run the binary four times (`identity set-name` and `trust add` for both identities), so the page cache is likely warm. This is **not a true cold start**. |
| Startup, warm (median / p90) | The same, for runs 2 to N | Typical start, load identity, print and exit time. Each timestamp comes from a separate `perl` process, so every sample includes some helper overhead. Treat it as an upper bound. |
| Throughput (large, small) | Bytes ÷ `elapsed_ms` from the sender's `completed` event, in MiB/s (1 MiB = 1,048,576 bytes) | `elapsed_ms` starts when the receiver accepts the offer and ends when the sender has the receiver's `CompleteAck`. It includes reading, BLAKE3 hashing, encryption and the receiver's writes, per-file fsync and finalization, plus one directory fsync. It excludes TCP connect, handshake, offer and consent. |
| Handshake latency | `handshake_us` from the sender's `connected` event, median over the **large-file runs only** | From the start of the version preamble (after TCP connect) until the receiver's Hello has arrived and the SAS reveal has been checked. Covers the preamble exchange, Noise XX messages 1 to 3, the trust check and both Hellos. |
| Peak RSS and CPU time, sender and receiver | `/usr/bin/time -l` (macOS: maximum resident set size in bytes, ÷ 1024) or `-v` (Linux: KiB). CPU is user + sys seconds. | Whole-process figures, including start-up and, for the receiver, time spent waiting. Median per workload. |
| Idle receiver RSS | A receiver started with `--no-mdns`; `ps -o rss=` taken 10 s after it reports `listening` | Memory of a receiver that is listening with nothing to do. Idle with mDNS advertising is not measured. |
| Idle receiver CPU | Difference in `ps -o time=` (macOS, 0.01 s resolution) or `/proc/<pid>/stat` utime + stime (Linux) over `--idle-secs` | CPU used while idle. `0.00 s` means less than the measurement resolution. |
| Cancel-to-exit latency | Sends the large file with the hidden test flag `--throttle-bps 104857600` and `--control-stdin`. After the first `progress` event, a flag is set; a helper loop that checks it every 10 ms writes `{"cmd":"cancel"}` to the sender's stdin. Time runs from setting the flag until both processes have exited (checked every 5 ms). | How quickly a user cancel stops both peers. Includes up to about 10 ms of command polling and about 5 ms of exit polling. |
| mDNS discovery | Only with `--mdns`. Starts a receiver with the hidden test flag `--mdns-loopback`, waits for its `advertising` event, then runs `discover --timeout-ms 5000 --mdns-loopback` | Reports only whether the receiver was found within the 5000 ms window. `discover` emits devices when its browse window ends, so **discovery latency is not measured**. |

**Not measured by this script:**
- battery use, and mobile devices;
- real LAN or Wi-Fi, and other hardware;
- comparison with other tools;
- true cold start;
- an idle receiver with mDNS advertising;
- Windows.

## Honesty rules

- **Only real runs.** Publish only numbers this script produced on a run that actually happened. Copy them unchanged: no re-rounding, no averaging across result files, no extrapolation.
  - An estimate must be labelled "design estimate, not measured".
  - Anything else is "not measured".
- **Loopback is not a network.** Both peers run on one machine over `127.0.0.1`, so there is no radio, no switch and no packet loss. Never present these numbers as LAN or Wi-Fi performance, or as performance on other hardware.
- **Startup is not a cold start.** The first run follows the build, so the binary is probably in the page cache.
- **Discovery latency is not measured.** `discover` reports its results only when its browse window ends.
- **State every deviation.** If an option differs from its default, the result file must give the value and the reason, for example a smaller `--size-mib` on a machine short of disk space.
- **One machine is one data point.** A result file names its OS version, architecture, CPU, Rust version and filesystem. It is never evidence about another platform.
- **CI is not a benchmark.** The only performance-related check in CI is the Linux-only e2e test `bounded_memory_independent_of_size`. It requires the receiver's peak RSS to grow by less than 16 MiB between a 16 MiB and a 256 MiB transfer. Numbers from CI runners are never published as benchmarks.

## Privacy rules

- **Temporary directories only.** The script uses `mktemp -d` for config, inputs and received files, and deletes them on exit. It never reads or writes the user's real config directory.
- **`$HOME` is scrubbed.** The script pipes the summary through `sed "s|$HOME|~|g"`. Check any other output the same way before you paste it.
- **The header is minimal.** It records `uname -srm` (never `uname -a`, which includes the host name), the CPU brand string, `rustc -V`, the short commit hash and the filesystem type.
- **Nothing personal is published.** No user names, host names, absolute home paths, temp paths, keys or fingerprints go into a result file.
- **Raw output stays local.** `bench-out/` is local output and is never committed. Only the reviewed Markdown result file is committed.

## Adding a result

1. Run the script on a quiet machine, on AC power, with no other heavy work running.
2. Create `results/<YYYY-MM-DD>-<os><version>-<cpu>-loopback.md` containing:
   - the exact command line;
   - the summary, copied unchanged;
   - the raw `transfers.tsv` and `startup.tsv` rows;
   - every parameter that differs from its default, and why;
   - the commit the binary was built from.
3. Keep any interpretation in a separate "Observations" section. Label explanations that have not been profiled as hypotheses.
4. Add a row to the results index above.
