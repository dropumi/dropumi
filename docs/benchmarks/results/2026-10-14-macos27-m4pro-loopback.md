# Loopback benchmark: macOS 27, Apple M4 Pro (2026-10-14)

> **Loopback on one machine (`127.0.0.1`), not a LAN or Wi-Fi measurement.**
> - These numbers come from one real run of [`scripts/bench/loopback.sh`](../../../scripts/bench/loopback.sh).
> - They say nothing about other hardware, other operating systems or real networks.
> - The method and the meaning of each metric are in [`../README.md`](../README.md).

## Environment

| Item | Value |
|---|---|
| OS | macOS 27 (`uname -srm`: Darwin 27.0.0 arm64) |
| CPU | Apple M4 Pro |
| Rust | rustc 1.99.0 |
| Filesystem of the temp directory | APFS |
| Binary | `cargo build --release --locked -p dropumi-cli` (default features, `mdns` included) |
| Code | The code in the commit that adds this file (see the commit note below) |

## Command

```sh
bash scripts/bench/loopback.sh --size-mib 256 --small-count 1000 --small-kib 100 --runs 3 --startup-runs 20 --idle-secs 60 --mdns
```

**Parameter notes**
- **The large file is 256 MiB, not the default 1024 MiB.** Free disk space on the benchmark machine was limited, and the script needs room for the inputs plus one received copy.
- **The other values are the script defaults,** written out in full: 1000 × 100 KiB small files, 3 runs, 20 startup runs and a 60 s idle window.
- **`--mdns` was added** to check discovery on loopback.
- **Output went to the default `bench-out/` directory,** which is not committed.

**Commit note**
- The `Commit` value in the summary below is the hash the branch had when the benchmark ran, before the branch was rebased for review, so that hash does not exist in the published history.
- The measured code is the code in the commit that adds this file, except one comment line in `crates/cli/Cargo.toml`, which changed after the run; otherwise only documentation and this benchmark's files changed.

**Run-to-run variation.** Four earlier runs with the same command on the same machine, on development versions of this branch (not published), gave large-file medians of 396.9, 388.5, 354.6 and 381.5 MiB/s and small-file medians of 20.9, 20.7, 23.3 and 20.7 MiB/s; this run gave 386.1 and 20.3 MiB/s. Treat differences of that size as noise, not as a regression or an improvement.

## Summary

Copied from `bench-out/summary.md`. The only change is that its `#` title line is shown here as bold text; every table row is unchanged.

**Dropumi loopback benchmark**

| Item | Value |
|---|---|
| System | Darwin 27.0.0 arm64 |
| CPU | Apple M4 Pro |
| Rust | rustc 1.99.0 (b940084d7 2026-09-28) |
| Commit | dcd25c2 |
| Filesystem (temp dir) | apfs |
| Parameters | large=256 MiB, small=1000×100 KiB, runs=3, startup runs=20, idle=60s |

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

## Raw results

`bench-out/transfers.tsv`, unchanged (tab-separated):

```tsv
workload	run	bytes	elapsed_ms	throughput_mib_s	handshake_us	sender_peak_rss_kib	sender_cpu_s	receiver_peak_rss_kib	receiver_cpu_s
large	1	268435456	663	386.1	617	4432	0.55	4400	0.54
small	1	102400000	4816	20.3	494	6112	0.42	6736	0.42
large	2	268435456	653	392.0	615	4416	0.55	4384	0.54
small	2	102400000	4791	20.4	475	6096	0.43	6720	0.43
large	3	268435456	666	384.4	611	4416	0.55	4416	0.54
small	3	102400000	4820	20.3	457	6144	0.43	6576	0.44
```

The same rows as a table:

| workload | run | bytes | elapsed_ms | throughput_mib_s | handshake_us | sender_peak_rss_kib | sender_cpu_s | receiver_peak_rss_kib | receiver_cpu_s |
|---|---|---|---|---|---|---|---|---|---|
| large | 1 | 268435456 | 663 | 386.1 | 617 | 4432 | 0.55 | 4400 | 0.54 |
| small | 1 | 102400000 | 4816 | 20.3 | 494 | 6112 | 0.42 | 6736 | 0.42 |
| large | 2 | 268435456 | 653 | 392.0 | 615 | 4416 | 0.55 | 4384 | 0.54 |
| small | 2 | 102400000 | 4791 | 20.4 | 475 | 6096 | 0.43 | 6720 | 0.43 |
| large | 3 | 268435456 | 666 | 384.4 | 611 | 4416 | 0.55 | 4416 | 0.54 |
| small | 3 | 102400000 | 4820 | 20.3 | 457 | 6144 | 0.43 | 6576 | 0.44 |

`bench-out/startup.tsv` holds the 20 startup samples in run order, in ms. Run 1 is the "first run after build"; runs 2 to 20 are the warm samples.

```text
9
8
8
9
8
9
9
10
10
10
9
9
9
8
9
9
8
8
8
8
```

**How to read the medians**
- **Throughput:** `bytes / elapsed_ms`, with 1 MiB = 1,048,576 bytes. Each large run moves 268435456 bytes (256 MiB), and each small run moves 102400000 bytes (1000 × 100 KiB).
- **Handshake latency:** the median of the three large-file runs only (617, 615 and 611 µs). The small-file runs' handshake values are in the raw rows but not in the median.
- **Idle receiver CPU:** `0.00 s` is below the 0.01 s resolution of `ps -o time=` on macOS.
- **Cancel-to-exit latency:** 26 ms is a single sample. It includes up to about 10 ms of command polling and about 5 ms of exit polling by the script.
- **mDNS:** the receiver was found within the 5000 ms browse window. Discovery latency was not measured.

## Observations

These are interpretations of the numbers above, not extra measurements.

1. **Small-file throughput is far below large-file throughput** (20.3 vs 386.1 MiB/s median).
   - *Explanation, still to be confirmed by profiling (this is not a measured breakdown):* per-file overhead dominates small-file transfers on macOS.
   - For every file, the receiver creates a new temp file and hashes the data with BLAKE3 as it writes it.
   - If the hash matches, the receiver calls `sync_all`, which Rust's standard library implements on Apple platforms with `fcntl(F_FULLFSYNC)`, a full flush to stable storage.
   - It then finalizes the file without overwriting anything and acknowledges it. The receive directory itself is synced only once, at the end, before `CompleteAck`.
   - The cost of each step was not measured.
2. **Peak memory stayed small for the 256 MiB file.** The median peak RSS was 4416 KiB for the sender and 4400 KiB for the receiver.
   - This fits the streaming design: files are never buffered whole.
   - Only one file size was benchmarked here, so this run does not show that memory is independent of size. That property is checked by the Linux-only e2e test `bounded_memory_independent_of_size`, which passed in a Debian 12 aarch64 container (local run) and is pending CI on `ubuntu-latest`.
3. **Startup was 9 ms both on the first run and as the warm median.** These include the overhead of the `perl` timestamp helper. The first run was not a true cold start.

## Not measured

- Battery use, mobile devices, Linux, Windows and any other hardware.
- Real LAN or Wi-Fi.
- Comparison with other tools.
- True cold start.
- An idle receiver with mDNS advertising.
- mDNS discovery latency.
- Installer size (there is no installer).
