#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Android pre-build check: is the Rust core (libdropumi_ffi.so) present, and
# was it built from the Rust sources as they are now? It never runs cargo; it
# tells you to run scripts/ffi/build-android.sh instead. The :app build runs
# it before every build.
#
# Fails when:
# - the stamp written by build-android.sh is missing;
# - the Rust sources changed since the libraries were built;
# - a library is missing, does not match its stamped hash, or belongs to an
#   ABI the stamp does not list (a library copied in by hand);
# - an ABI given with --abis was not built.
# A dev-profile core with --build-type release is only a warning.
#
# Usage: bash scripts/android/check-core-artifacts.sh [--abis arm64-v8a,x86_64]
#          [--build-type debug|release] [--out DIR]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="${root}/apps/android/core-bindings/build/rust/jniLibs"
require=""
build_type=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --abis) require="$2"; shift 2 ;;
    --build-type) build_type="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    *) echo "usage: $0 [--abis LIST] [--build-type debug|release] [--out DIR]" >&2; exit 2 ;;
  esac
done
stamp="$(dirname "${out}")/core-stamp.txt"
fix="run: bash scripts/ffi/build-android.sh --abis arm64-v8a,x86_64 --profile dev"

if [[ ! -f "${stamp}" ]]; then
  echo "error: the Rust core (libdropumi_ffi.so) is missing or was not stamped; ${fix}" >&2
  exit 1
fi

if ! command -v git >/dev/null 2>&1 || ! git -C "${root}" rev-parse --git-dir >/dev/null 2>&1; then
  echo "warning: not a git checkout; cannot check whether the Rust core is up to date" >&2
  exit 0
fi

built="$(sed -n 's/^sources //p' "${stamp}")"
now="$(bash "${root}/scripts/ffi/core-sources-hash.sh")"
if [[ -z "${built}" || "${built}" != "${now}" ]]; then
  echo "error: the Rust core is older than the Rust sources; ${fix}" >&2
  exit 1
fi

stamped_abis=()
while read -r abi hash; do
  stamped_abis+=("${abi}")
  lib="${out}/${abi}/libdropumi_ffi.so"
  if [[ ! -f "${lib}" ]]; then
    echo "error: the Rust core for ${abi} is missing; ${fix}" >&2
    exit 1
  fi
  if [[ "$(shasum -a 256 "${lib}" | cut -d' ' -f1)" != "${hash}" ]]; then
    echo "error: the Rust core for ${abi} does not match its stamp; ${fix}" >&2
    exit 1
  fi
done < <(sed -n 's/^abi //p' "${stamp}")

if [[ ${#stamped_abis[@]} -eq 0 ]]; then
  echo "error: the Rust core stamp lists no ABI; ${fix}" >&2
  exit 1
fi

# Every library in the directory must be one the stamp vouches for.
while IFS= read -r -d '' lib; do
  abi="$(basename "$(dirname "${lib}")")"
  if [[ " ${stamped_abis[*]} " != *" ${abi} "* ]]; then
    echo "error: ${abi}/libdropumi_ffi.so was not built by the last build-android.sh run; ${fix}" >&2
    exit 1
  fi
done < <(find "${out}" -name 'libdropumi_ffi.so' -print0 2>/dev/null)

if [[ -n "${require}" ]]; then
  IFS=',' read -r -a required <<< "${require}"
  for abi in "${required[@]}"; do
    if [[ " ${stamped_abis[*]} " != *" ${abi} "* ]]; then
      echo "error: the Rust core has no ${abi} library; ${fix}" >&2
      exit 1
    fi
  done
fi

profile="$(sed -n 's/^profile //p' "${stamp}")"
if [[ "${build_type}" == "release" && "${profile}" != "release" ]]; then
  echo "warning: release build with a ${profile}-profile Rust core; for a release core run: bash scripts/ffi/build-android.sh --abis arm64-v8a,x86_64 --profile release" >&2
fi

echo "Rust core up to date (${profile}; ${stamped_abis[*]})"
