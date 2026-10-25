#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Loopback benchmark for the dropumi CLI (macOS and Linux).
#
# Measures, on one machine over 127.0.0.1:
#   - stripped release binary size
#   - startup time (`identity show`): first run and warm median/p90
#   - throughput for one large file and for many small files
#   - handshake latency (preamble to Hello), median
#   - peak RSS and CPU time (user+sys) of sender and receiver per transfer
#   - idle receiver RSS and CPU time over a quiet interval
#   - cancel-to-exit latency during a throttled transfer
#   - mDNS discovery check within a 5 s browse window (only with --mdns;
#     latency is not measured)
#
# This is NOT a LAN or Wi-Fi benchmark and says nothing about other
# hardware. Never paste numbers that this script did not produce.
#
# Usage: bash scripts/bench/loopback.sh [--size-mib 1024] [--small-count 1000]
#          [--small-kib 100] [--runs 3] [--startup-runs 20] [--idle-secs 60]
#          [--out bench-out] [--mdns]
#
# Requires: cargo, jq, perl (Time::HiRes), /usr/bin/time.
# Config and data directories are temporary (mktemp -d) and removed at exit;
# the default configuration directory is never touched.

set -euo pipefail

size_mib=1024
small_count=1000
small_kib=100
runs=3
startup_runs=20
idle_secs=60
out_dir="bench-out"
mdns=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --size-mib) size_mib="$2"; shift 2 ;;
    --small-count) small_count="$2"; shift 2 ;;
    --small-kib) small_kib="$2"; shift 2 ;;
    --runs) runs="$2"; shift 2 ;;
    --startup-runs) startup_runs="$2"; shift 2 ;;
    --idle-secs) idle_secs="$2"; shift 2 ;;
    --out) out_dir="$2"; shift 2 ;;
    --mdns) mdns=1; shift ;;
    -h|--help) sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

for tool in cargo jq perl; do
  command -v "$tool" >/dev/null || { echo "missing tool: $tool" >&2; exit 2; }
done
[[ -x /usr/bin/time ]] || { echo "missing /usr/bin/time" >&2; exit 2; }

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"
mkdir -p "$out_dir"
out_dir="$(cd "$out_dir" && pwd)"
work="$(mktemp -d)"
pids=()
cleanup() {
  for p in "${pids[@]:-}"; do [[ -n "$p" ]] && kill "$p" 2>/dev/null || true; done
  rm -rf "$work"
}
trap cleanup EXIT

os="$(uname -s)"
if [[ "$os" == "Darwin" ]]; then
  time_flag="-l"
  cpu_brand="$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo unknown)"
else
  time_flag="-v"
  cpu_brand="$(awk -F: '/model name/ {print $2; exit}' /proc/cpuinfo 2>/dev/null | sed 's/^ *//' || echo unknown)"
fi

now_ms() { perl -MTime::HiRes=time -e 'printf("%d\n", time()*1000)'; }

# Filesystem type of the mount holding $1.
fs_type() {
  if [[ "$os" == "Darwin" ]]; then
    local mnt
    mnt="$(df "$1" | awk 'NR==2 {print $NF}')"
    mount | awk -v m="$mnt" '$3==m { gsub(/[(,]/, "", $4); print $4; exit }'
  else
    df -T "$1" | awk 'NR==2 {print $2}'
  fi
}

# Peak RSS in KiB and CPU seconds (user+sys) from a /usr/bin/time log.
peak_rss_kib() {
  if [[ "$os" == "Darwin" ]]; then
    awk '/maximum resident set size/ {printf "%d\n", $1/1024}' "$1"
  else
    awk -F: '/Maximum resident set size/ {gsub(/ /,"",$2); print $2}' "$1"
  fi
}
cpu_secs() {
  if [[ "$os" == "Darwin" ]]; then
    awk '/ user / {u=$3} / sys / {s=$5} END {printf "%.2f\n", u+s}' "$1" 2>/dev/null \
      || awk 'NR==1 {print $3+$5}' "$1"
  else
    awk -F: '/User time/ {u=$2} /System time/ {s=$2} END {printf "%.2f\n", u+s}' "$1"
  fi
}

median() { sort -n | awk '{a[NR]=$1} END {if (NR==0) {print "n/a"} else if (NR%2) {print a[(NR+1)/2]} else {print (a[NR/2]+a[NR/2+1])/2}}'; }
p90() { sort -n | awk '{a[NR]=$1} END {if (NR==0) print "n/a"; else {i=int(NR*0.9+0.999); if (i>NR) i=NR; print a[i]}}'; }

echo "building release binary…" >&2
cargo build --release --locked -p dropumi-cli >&2
bin="$repo_root/target/release/dropumi"
bin_size="$(wc -c < "$bin" | tr -d ' ')"

cfg_a="$work/cfg-a"; cfg_b="$work/cfg-b"
mkdir -p "$cfg_a" "$cfg_b"
chmod 700 "$cfg_a" "$cfg_b"
fp_a="$("$bin" --config-dir "$cfg_a" --json identity set-name "Bench sender" | jq -r 'select(.event=="identity").fingerprint')"
fp_b="$("$bin" --config-dir "$cfg_b" --json identity set-name "Bench receiver" | jq -r 'select(.event=="identity").fingerprint')"
"$bin" --config-dir "$cfg_a" trust add "$fp_b" --name receiver >/dev/null
"$bin" --config-dir "$cfg_b" trust add "$fp_a" --name sender >/dev/null

# --- startup -----------------------------------------------------------------
startup_file="$out_dir/startup.tsv"
: > "$startup_file"
for i in $(seq 1 "$startup_runs"); do
  t0="$(now_ms)"
  "$bin" --config-dir "$cfg_a" identity show >/dev/null
  t1="$(now_ms)"
  echo "$((t1 - t0))" >> "$startup_file"
done
startup_first="$(head -n 1 "$startup_file")"
startup_warm_median="$(tail -n +2 "$startup_file" | median)"
startup_warm_p90="$(tail -n +2 "$startup_file" | p90)"

# --- inputs ------------------------------------------------------------------
src="$work/src"; mkdir -p "$src" "$src/small"
echo "generating inputs…" >&2
head -c "$((size_mib * 1024 * 1024))" /dev/urandom > "$src/large.bin"
for i in $(seq 1 "$small_count"); do
  head -c "$((small_kib * 1024))" /dev/urandom > "$src/small/f$(printf '%05d' "$i").bin"
done

# start_receiver <timed|plain> <dest-dir> <log-prefix> [extra args…]:
# sets recv_pid (the time wrapper when timed, dropumi itself when plain) and recv_port
start_receiver() {
  local mode="$1" dest="$2" prefix="$3"; shift 3
  mkdir -p "$dest"
  if [[ "$mode" == timed ]]; then
    /usr/bin/time "$time_flag" "$bin" --config-dir "$cfg_b" --json receive \
      --dir "$dest" --bind 127.0.0.1 --port 0 --accept trusted --once "$@" \
      > "$prefix.recv.ndjson" 2> "$prefix.recv.time" &
  else
    "$bin" --config-dir "$cfg_b" --json receive \
      --dir "$dest" --bind 127.0.0.1 --port 0 --accept trusted --once "$@" \
      > "$prefix.recv.ndjson" 2> "$prefix.recv.err" &
  fi
  recv_pid=$!
  pids+=("$recv_pid")
  recv_port=""
  for _ in $(seq 1 200); do
    recv_port="$(jq -r 'select(.event=="listening").port' "$prefix.recv.ndjson" 2>/dev/null | head -n 1)"
    [[ -n "$recv_port" ]] && break
    sleep 0.05
  done
  [[ -n "$recv_port" ]] || { echo "receiver did not start" >&2; exit 1; }
}

results="$out_dir/transfers.tsv"
printf 'workload\trun\tbytes\telapsed_ms\tthroughput_mib_s\thandshake_us\tsender_peak_rss_kib\tsender_cpu_s\treceiver_peak_rss_kib\treceiver_cpu_s\n' > "$results"

# run_transfer <workload> <run> <files…>
run_transfer() {
  local workload="$1" run="$2"; shift 2
  local prefix="$work/$workload-$run"
  start_receiver timed "$work/dst-$workload-$run" "$prefix" --no-mdns
  /usr/bin/time "$time_flag" "$bin" --config-dir "$cfg_a" --json send "$@" \
    --to "127.0.0.1:$recv_port" > "$prefix.send.ndjson" 2> "$prefix.send.time"
  wait "$recv_pid"
  local bytes elapsed hs
  bytes="$(jq -r 'select(.event=="completed").bytes' "$prefix.send.ndjson")"
  elapsed="$(jq -r 'select(.event=="completed").elapsed_ms' "$prefix.send.ndjson")"
  hs="$(jq -r 'select(.event=="connected").handshake_us' "$prefix.send.ndjson")"
  local mibs
  mibs="$(awk -v b="$bytes" -v e="$elapsed" 'BEGIN { if (e>0) printf "%.1f", (b/1048576)/(e/1000); else print "n/a" }')"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$workload" "$run" "$bytes" "$elapsed" "$mibs" "$hs" \
    "$(peak_rss_kib "$prefix.send.time")" "$(cpu_secs "$prefix.send.time")" \
    "$(peak_rss_kib "$prefix.recv.time")" "$(cpu_secs "$prefix.recv.time")" >> "$results"
  rm -rf "$work/dst-$workload-$run"
}

for r in $(seq 1 "$runs"); do
  echo "large file run $r/$runs…" >&2
  run_transfer large "$r" "$src/large.bin"
  echo "small files run $r/$runs…" >&2
  run_transfer small "$r" "$src"/small/*.bin
done

# --- idle receiver -------------------------------------------------------------
echo "idle receiver for ${idle_secs}s…" >&2
start_receiver plain "$work/dst-idle" "$work/idle" --no-mdns
sleep 10
idle_rss_kib="$(ps -o rss= -p "$recv_pid" | tr -d ' ')"
# CPU seconds (user+sys) of a process.
proc_cpu() {
  if [[ -r "/proc/$1/stat" ]]; then
    awk -v hz="$(getconf CLK_TCK)" '{ printf "%.2f\n", ($14+$15)/hz }' "/proc/$1/stat"
  else
    # macOS: [[hh:]mm:]ss.cc
    ps -o time= -p "$1" | tr -d ' ' | awk -F: '{ s=0; for (i=1; i<=NF; i++) s = s*60 + $i; printf "%.2f\n", s }'
  fi
}
cpu0="$(proc_cpu "$recv_pid")"
sleep "$idle_secs"
cpu1="$(proc_cpu "$recv_pid")"
idle_cpu="$(awk -v a="$cpu0" -v b="$cpu1" 'BEGIN { printf "%.2f", b-a }')"
kill -INT "$recv_pid" 2>/dev/null || true
wait "$recv_pid" 2>/dev/null || true

# --- cancel latency ------------------------------------------------------------
echo "cancel-to-exit latency…" >&2
start_receiver timed "$work/dst-cancel" "$work/cancel" --no-mdns
# The control writer polls for a flag file every 10 ms (bash 3.2 has no coproc).
( while [[ ! -f "$work/cancel.now" ]]; do sleep 0.01; done
  echo '{"cmd":"cancel"}'; sleep 30 ) \
  | "$bin" --config-dir "$cfg_a" --json --control-stdin send "$src/large.bin" \
      --to "127.0.0.1:$recv_port" --throttle-bps 104857600 > "$work/cancel.send.ndjson" 2>/dev/null &
sender_pid=$!
pids+=("$sender_pid")
for _ in $(seq 1 600); do
  jq -e 'select(.event=="progress")' "$work/cancel.send.ndjson" >/dev/null 2>&1 && break
  sleep 0.01
done
t0="$(now_ms)"
touch "$work/cancel.now"
while kill -0 "$sender_pid" 2>/dev/null || kill -0 "$recv_pid" 2>/dev/null; do sleep 0.005; done
t1="$(now_ms)"
cancel_ms="$((t1 - t0))"

# --- discovery latency ---------------------------------------------------------
discovery_ms="not measured (run with --mdns)"
if [[ "$mdns" == 1 ]]; then
  # `discover` reports devices when its browse window ends, so this checks
  # that discovery works within the window; it is not a latency measurement.
  start_receiver plain "$work/dst-mdns" "$work/mdns" --mdns-loopback
  for _ in $(seq 1 100); do
    jq -e 'select(.event=="advertising")' "$work/mdns.recv.ndjson" >/dev/null 2>&1 && break
    sleep 0.05
  done
  t0="$(now_ms)"
  "$bin" --config-dir "$cfg_a" --json discover --timeout-ms 5000 --mdns-loopback > "$work/discover.ndjson"
    if jq -e 'select(.event=="peer_found")' "$work/discover.ndjson" >/dev/null 2>&1; then
    discovery_ms="not measured (device found within the 5000 ms browse window; latency is not reported)"
  else
    discovery_ms="not measured (device not found within 5000 ms)"
  fi
  kill -INT "$recv_pid" 2>/dev/null || true
fi

# --- report --------------------------------------------------------------------
col() { awk -F'\t' -v w="$1" -v c="$2" 'NR>1 && $1==w {print $c}' "$results"; }
summary="$out_dir/summary.md"
{
  echo "# Dropumi loopback benchmark"
  echo
  echo "| Item | Value |"
  echo "|---|---|"
  echo "| System | $(uname -srm) |"
  echo "| CPU | ${cpu_brand} |"
  echo "| Rust | $(rustc -V) |"
  echo "| Commit | $(git rev-parse --short HEAD) |"
    echo "| Filesystem (temp dir) | $(fs_type "$work") |"
  echo "| Parameters | large=${size_mib} MiB, small=${small_count}×${small_kib} KiB, runs=${runs}, startup runs=${startup_runs}, idle=${idle_secs}s |"
  echo
  echo "| Metric | Median of runs |"
  echo "|---|---|"
  echo "| Stripped release binary | ${bin_size} bytes |"
  echo "| Startup, first run after build (page cache likely warm; not a true cold start) | ${startup_first} ms |"
  echo "| Startup, warm (median / p90) | ${startup_warm_median} / ${startup_warm_p90} ms |"
  echo "| Large file throughput | $(col large 5 | median) MiB/s |"
  echo "| Small files throughput | $(col small 5 | median) MiB/s |"
  echo "| Handshake latency (preamble to both Hellos) | $(col large 6 | median) µs |"
  echo "| Sender peak RSS (large / small) | $(col large 7 | median) / $(col small 7 | median) KiB |"
  echo "| Receiver peak RSS (large / small) | $(col large 9 | median) / $(col small 9 | median) KiB |"
  echo "| Sender CPU time, user+sys (large / small) | $(col large 8 | median) / $(col small 8 | median) s |"
  echo "| Receiver CPU time, user+sys (large / small) | $(col large 10 | median) / $(col small 10 | median) s |"
  echo "| Idle receiver RSS (after 10 s) | ${idle_rss_kib} KiB |"
  echo "| Idle receiver CPU over ${idle_secs} s | ${idle_cpu} s |"
  echo "| Cancel-to-exit latency (both processes; includes ~10 ms command polling) | ${cancel_ms} ms |"
  echo "| mDNS discovery latency | ${discovery_ms} |"
  echo
  echo "Not measured: battery, mobile devices, real LAN or Wi-Fi, comparison with other tools, true cold start."
} | sed "s|$HOME|~|g" > "$summary"
sed "s|$HOME|~|g" "$summary"
echo >&2
echo "raw results: $results" | sed "s|$HOME|~|g" >&2
