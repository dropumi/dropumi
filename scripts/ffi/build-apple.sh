#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Builds DropumiFFI.xcframework for apps/apple/DropumiKit: the dropumi-ffi
# static library for iOS devices, the iOS simulator and macOS (arm64 only),
# with its C header and module map. The output is not committed.
#
#   apps/apple/DropumiKit/Artifacts/DropumiFFI.xcframework
#   apps/apple/DropumiKit/Artifacts/core-stamp.txt   (what it was built from)
#
# Usage: bash scripts/ffi/build-apple.sh [--profile release|dev] [--slices all|ci]
#   --slices ci   simulator and macOS only (no device signing needed)
#
# Needs Xcode and the Rust targets aarch64-apple-ios, aarch64-apple-ios-sim
# and aarch64-apple-darwin.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

profile="release"
slices="all"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile) profile="$2"; shift 2 ;;
    --slices) slices="$2"; shift 2 ;;
    *) echo "usage: $0 [--profile release|dev] [--slices all|ci]" >&2; exit 2 ;;
  esac
done
case "${profile}" in
  release) cargo_profile=(--release); dir="release" ;;
  dev) cargo_profile=(); dir="debug" ;;
  *) echo "error: unknown profile ${profile}" >&2; exit 2 ;;
esac
case "${slices}" in
  all) targets=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin) ;;
  ci) targets=(aarch64-apple-ios-sim aarch64-apple-darwin) ;;
  *) echo "error: unknown slice set ${slices}" >&2; exit 2 ;;
esac

# shellcheck source=scripts/ffi/apple.env
set -a
. scripts/ffi/apple.env
set +a

out="apps/apple/DropumiKit/Artifacts"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

for t in "${targets[@]}"; do
  echo "building dropumi-ffi for ${t} (${profile})"
  cargo build --quiet --locked -p dropumi-ffi --lib --target "${t}" "${cargo_profile[@]}"
done

# Header and module map, generated from the macOS slice. They sit in a
# DropumiFFI/ subdirectory so they never collide with other module maps in
# the build products' include directory.
cargo build --quiet --locked -p dropumi-ffi-bindgen
include="${tmp}/include"
mkdir -p "${include}/DropumiFFI"
target/debug/uniffi-bindgen-swift --headers --modulemap --module-name DropumiFFI \
  --modulemap-filename module.modulemap \
  "target/aarch64-apple-darwin/${dir}/libdropumi_ffi.a" "${include}/DropumiFFI"

args=()
for t in "${targets[@]}"; do
  args+=(-library "target/${t}/${dir}/libdropumi_ffi.a" -headers "${include}")
done
rm -rf "${out}/DropumiFFI.xcframework"
mkdir -p "${out}"
xcodebuild -create-xcframework "${args[@]}" -output "${out}/DropumiFFI.xcframework" >/dev/null

uniffi_version="$(cargo metadata --locked --format-version 1 --offline \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "uniffi"))')"
{
  # Content of the tracked core sources as they are on disk now.
  echo "sources $(bash scripts/ffi/core-sources-hash.sh)"
  echo "profile ${profile}"
  echo "slices ${targets[*]}"
  echo "uniffi ${uniffi_version}"
} > "${out}/core-stamp.txt"
echo "wrote ${out}/DropumiFFI.xcframework"
