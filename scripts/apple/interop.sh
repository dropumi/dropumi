#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Apple interop tests (D11): the app's real core, inside an app-hosted
# XCTest on macOS and on the iOS simulator, against the real CLI over
# loopback. Builds what is missing, then runs the opt-in `apple_*` tests
# one at a time (they share the simulator).
#
# Usage: bash scripts/apple/interop.sh [test name filter, default apple_]
# Environment: DROPUMI_TEST_APPLE_SIMULATOR (default "iPhone 17");
# DROPUMI_TEST_MDNS=1 also runs the macOS Bonjour tests, which show the
# Local Network question for Dropumi the first time (choose Allow).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"
filter="${1:-apple_}"
simulator="${DROPUMI_TEST_APPLE_SIMULATOR:-iPhone 17}"

if ! bash scripts/apple/check-core-artifacts.sh >/dev/null; then
  bash scripts/ffi/build-apple.sh --profile dev
fi
(cd apps/apple && xcodegen generate --quiet)

for target in "Dropumi-macOS|platform=macOS,arch=arm64" "Dropumi-iOS|platform=iOS Simulator,name=${simulator}"; do
  xcodebuild build-for-testing -quiet \
    -project apps/apple/Dropumi.xcodeproj -scheme "${target%%|*}" -destination "${target#*|}" \
    -derivedDataPath apps/apple/build
done

DROPUMI_TEST_APPLE=1 cargo test -p dropumi-cli --test e2e --locked -- --ignored --test-threads=1 "${filter}"
