#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Builds libdropumi_ffi.so for Android with the NDK's clang (the same setup
# as the CI cross-build job and docs/development.md) and copies it into the
# app's jniLibs. 16 KB page alignment is set explicitly; check-16k.sh
# verifies it. The output is not committed.
#
#   apps/android/core-bindings/build/rust/jniLibs/<abi>/libdropumi_ffi.so
#   apps/android/core-bindings/build/rust/core-stamp.txt
#
# The stamp (next to the jniLibs directory) records the hash of the Rust
# sources the libraries were built from (scripts/ffi/core-sources-hash.sh),
# the profile, and each ABI's library hash. scripts/android/check-core-artifacts.sh
# compares it with the sources as they are now; the app build runs that check
# and never runs cargo. Libraries of ABIs not built in this run are removed,
# so every library in the directory matches the stamp.
#
# Usage: bash scripts/ffi/build-android.sh [--abis arm64-v8a[,x86_64]] [--profile release|dev] [--out DIR]
#
# Finds the SDK in ANDROID_HOME, apps/android/local.properties (sdk.dir) or
# ~/Library/Android/sdk, and the NDK in ANDROID_NDK_HOME or
# <sdk>/ndk/27.1.12297006. API level 26 (minSdk).

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

abis="arm64-v8a"
profile="release"
out="apps/android/core-bindings/build/rust/jniLibs"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --abis) abis="$2"; shift 2 ;;
    --profile) profile="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    *) echo "usage: $0 [--abis LIST] [--profile release|dev] [--out DIR]" >&2; exit 2 ;;
  esac
done
case "${profile}" in
  release) cargo_profile=(--release); dir="release" ;;
  dev) cargo_profile=(); dir="debug" ;;
  *) echo "error: unknown profile ${profile}" >&2; exit 2 ;;
esac

api=26
ndk_version="27.1.12297006"
sdk="${ANDROID_HOME:-}"
if [[ -z "${sdk}" && -f apps/android/local.properties ]]; then
  sdk="$(sed -n 's/^sdk\.dir=//p' apps/android/local.properties | head -n 1)"
fi
if [[ -z "${sdk}" && -d "${HOME}/Library/Android/sdk" ]]; then
  sdk="${HOME}/Library/Android/sdk"
fi
ndk="${ANDROID_NDK_HOME:-${sdk:+${sdk}/ndk/${ndk_version}}}"
if [[ -z "${ndk}" || ! -d "${ndk}" ]]; then
  echo "error: Android NDK ${ndk_version} not found; set ANDROID_NDK_HOME or ANDROID_HOME" >&2
  exit 1
fi
case "$(uname -s)" in
  Darwin) host="darwin-x86_64" ;;
  Linux) host="linux-x86_64" ;;
  *) echo "error: unsupported build host $(uname -s)" >&2; exit 1 ;;
esac
bin="${ndk}/toolchains/llvm/prebuilt/${host}/bin"
echo "NDK $(sed -n 's/^Pkg\.Revision *= *//p' "${ndk}/source.properties")"

IFS=',' read -r -a abi_list <<< "${abis}"
for abi in "${abi_list[@]}"; do
  case "${abi}" in
    arm64-v8a | x86_64) ;;
    *) echo "error: unsupported ABI ${abi}" >&2; exit 2 ;;
  esac
done

# A build that fails part-way must not leave an earlier stamp claiming the
# libraries are current.
stamp="$(dirname "${out}")/core-stamp.txt"
rm -f "${stamp}"
# The hash of the sources as they are before cargo runs.
sources="$(bash scripts/ffi/core-sources-hash.sh)"

for abi in "${abi_list[@]}"; do
  case "${abi}" in
    arm64-v8a) triple="aarch64-linux-android" ;;
    x86_64) triple="x86_64-linux-android" ;;
    *) echo "error: unsupported ABI ${abi}" >&2; exit 2 ;;
  esac
  upper="$(echo "${triple}" | tr '[:lower:]-' '[:upper:]_')"
  lower="${triple//-/_}"
  export "CC_${lower}=${bin}/${triple}${api}-clang"
  export "AR_${lower}=${bin}/llvm-ar"
  export "CARGO_TARGET_${upper}_LINKER=${bin}/${triple}${api}-clang"
  export "CARGO_TARGET_${upper}_RUSTFLAGS=-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384"
  echo "building dropumi-ffi for ${abi} (${triple}, ${profile})"
  cargo build --quiet --locked -p dropumi-ffi --lib --target "${triple}" ${cargo_profile[@]+"${cargo_profile[@]}"}
  mkdir -p "${out}/${abi}"
  cp "target/${triple}/${dir}/libdropumi_ffi.so" "${out}/${abi}/"
  echo "wrote ${out}/${abi}/libdropumi_ffi.so"
done

for dir_abi in arm64-v8a x86_64; do
  if [[ " ${abi_list[*]} " != *" ${dir_abi} "* && -e "${out}/${dir_abi}/libdropumi_ffi.so" ]]; then
    rm -f "${out}/${dir_abi}/libdropumi_ffi.so"
    echo "removed ${out}/${dir_abi}/libdropumi_ffi.so (not built in this run)"
  fi
done

{
  echo "sources ${sources}"
  echo "profile ${profile}"
  for abi in "${abi_list[@]}"; do
    echo "abi ${abi} $(shasum -a 256 "${out}/${abi}/libdropumi_ffi.so" | cut -d' ' -f1)"
  done
} > "${stamp}"
echo "wrote ${stamp}"
