#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Apple interop tests (D11): the app's real core, inside an app-hosted
# XCTest on macOS and on the iOS simulator, against the real CLI over
# loopback. Builds what is missing, then runs the opt-in `apple_*` tests
# one at a time (they share the simulator).
#
# Only the destinations of the selected tests are built: a filter that
# selects only `apple_macos_*` tests never builds for, or needs, a
# simulator.
#
# Usage: bash scripts/apple/interop.sh [--build-only] [test name filter, default apple_]
#   --build-only builds for the selected tests' destinations and stops
#   (scripts/devices/interop.sh uses it).
# Environment: DROPUMI_TEST_APPLE_SIMULATOR (default "iPhone 17"; used only
# by the `apple_ios_sim_*` tests); DROPUMI_TEST_MDNS=1 also runs the
# Bonjour tests, which show the Local Network question for Dropumi the
# first time (choose Allow). The `apple_ios_device_*` tests run on a real,
# unlocked iPhone or iPad over Wi-Fi: DROPUMI_TEST_APPLE_DEVICE is its UDID,
# and DROPUMI_TEST_LAN_ADDR this Mac's address (default: en0's).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"
build_only=no
if [[ "${1:-}" == --build-only ]]; then
  build_only=yes
  shift
fi
filter="${1:-apple_}"
simulator="${DROPUMI_TEST_APPLE_SIMULATOR:-iPhone 17}"

# The tests this filter selects, with cargo's own matching.
selected="$(cargo test -p dropumi-cli --test e2e --locked -- --list --ignored "${filter}" |
  sed -n 's/^interop::apple::\(apple_[a-z_]*\): test$/\1/p')"
targets=()
if grep -q '^apple_macos_' <<< "${selected}"; then
  targets+=("Dropumi-macOS|platform=macOS,arch=arm64")
fi
if grep -q '^apple_ios_sim_' <<< "${selected}"; then
  targets+=("Dropumi-iOS|platform=iOS Simulator,name=${simulator}")
fi
if grep -q '^apple_ios_device_' <<< "${selected}"; then
  if [[ -z "${DROPUMI_TEST_APPLE_DEVICE:-}" ]]; then
    echo "error: the apple_ios_device_* tests need DROPUMI_TEST_APPLE_DEVICE (the device's UDID;" \
      "see: xcrun devicectl list devices)" >&2
    exit 2
  fi
  # The CLI binds this Mac's Wi-Fi address and the app dials it.
  if [[ -z "${DROPUMI_TEST_LAN_ADDR:-}" ]]; then
    DROPUMI_TEST_LAN_ADDR="$(ipconfig getifaddr en0 || true)"
    [[ -n "${DROPUMI_TEST_LAN_ADDR}" ]] || { echo "error: no Wi-Fi address on en0; set DROPUMI_TEST_LAN_ADDR" >&2; exit 2; }
    export DROPUMI_TEST_LAN_ADDR
  fi
  targets+=("Dropumi-iOS|platform=iOS,id=${DROPUMI_TEST_APPLE_DEVICE}")
fi
if [[ ${#targets[@]} -eq 0 ]]; then
  echo "error: no apple_* interop test matches '${filter}'" >&2
  exit 2
fi
echo "selected tests: $(tr '\n' ' ' <<< "${selected}")"

if ! bash scripts/apple/check-core-artifacts.sh >/dev/null; then
  bash scripts/ffi/build-apple.sh --profile dev
fi
(cd apps/apple && xcodegen generate --quiet)

for target in "${targets[@]}"; do
  echo "building ${target%%|*} for ${target#*|}"
  xcodebuild build-for-testing -quiet \
    -project apps/apple/Dropumi.xcodeproj -scheme "${target%%|*}" -destination "${target#*|}" \
    -derivedDataPath apps/apple/build
done
[[ "${build_only}" == no ]] || exit 0

DROPUMI_TEST_APPLE=1 cargo test -p dropumi-cli --test e2e --locked -- --ignored --test-threads=1 "${filter}"
