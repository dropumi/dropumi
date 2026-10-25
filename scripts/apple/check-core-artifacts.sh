#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Xcode pre-build check: is the Rust core XCFramework present, and was it
# built from the Rust sources as they are now? It never runs cargo; it
# tells you to run scripts/ffi/build-apple.sh instead.
#
# A missing or stale core fails the build. A dev-profile core in a Release
# build is only a warning (development installs use it on purpose).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
stamp="${root}/apps/apple/DropumiKit/Artifacts/core-stamp.txt"
fix="run: bash scripts/ffi/build-apple.sh --profile dev"

if [[ ! -f "${stamp}" ]]; then
  echo "error: the Rust core (DropumiFFI.xcframework) is missing; ${fix}"
  exit 1
fi

if ! command -v git >/dev/null 2>&1 || ! git -C "${root}" rev-parse --git-dir >/dev/null 2>&1; then
  echo "warning: not a git checkout; cannot check whether the Rust core is up to date"
  exit 0
fi

built="$(sed -n 's/^sources //p' "${stamp}")"
now="$(bash "${root}/scripts/ffi/core-sources-hash.sh")"
if [[ "${built}" != "${now}" ]]; then
  echo "error: the Rust core is older than the Rust sources; ${fix}"
  exit 1
fi

slices="$(sed -n 's/^slices //p' "${stamp}")"
case "${PLATFORM_NAME:-}" in
  iphoneos) need="aarch64-apple-ios" ;;
  iphonesimulator) need="aarch64-apple-ios-sim" ;;
  macosx) need="aarch64-apple-darwin" ;;
  *) need="" ;;
esac
if [[ -n "${need}" && " ${slices} " != *" ${need} "* ]]; then
  echo "error: the Rust core has no ${need} slice; run: bash scripts/ffi/build-apple.sh --profile dev --slices all"
  exit 1
fi

profile="$(sed -n 's/^profile //p' "${stamp}")"
if [[ "${CONFIGURATION:-}" == "Release" && "${profile}" != "release" ]]; then
  echo "warning: Release build with a ${profile}-profile Rust core; for a release core run: bash scripts/ffi/build-apple.sh --profile release"
fi
