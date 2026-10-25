#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Device-to-device interop tests: two real apps (iPhone, Mac app, Android
# phone), each steered by the Rust driver in
# crates/cli/tests/e2e/interop/devices.rs, pair with each other and send
# files both ways over Wi-Fi. Builds what the selected tests need, then runs
# the opt-in `devices_*` tests one at a time.
#
# 1. Apple: `xcodebuild build-for-testing` into apps/apple/build for the Mac
#    app and/or the iPhone (scripts/apple/interop.sh --build-only).
# 2. Android: the qa app and its test APK (scripts/android/interop.sh
#    --build-only), installed with `adb install -r`. An exit trap
#    uninstalls both, so the test identity they hold never stays on the
#    phone.
# 3. Runs DROPUMI_TEST_DEVICES=1 cargo test … -- --ignored --test-threads=1
#    devices_ (or the given filter).
# --no-build skips the builds (the Android APKs are still installed).
#
# Usage: bash scripts/devices/interop.sh [--no-build] [test name filter, default devices_]
# Environment:
#   DROPUMI_TEST_APPLE_DEVICE  the iPhone's UDID (iPhone tests; see:
#                              xcrun devicectl list devices)
#   DROPUMI_TEST_LAN_ADDR      this Mac's Wi-Fi address, which the iPhone's
#                              agent dials (default: en0's)
#   ANDROID_SERIAL             the Android phone (Android tests; default:
#                              the only one connected)
# Both phones must be unlocked, on the same Wi-Fi as this Mac, with Local
# Network access allowed for Dropumi. Gradle needs JDK 17 (JAVA_HOME).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"
build=yes
filter="devices_"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build) build=no; shift ;;
    -*) echo "usage: $0 [--no-build] [test name filter]" >&2; exit 2 ;;
    *) filter="$1"; shift ;;
  esac
done

# The tests this filter selects, with cargo's own matching.
selected="$(cargo test -p dropumi-cli --test e2e --locked -- --list --ignored "${filter}" |
  sed -n 's/^interop::devices::\(devices_[a-z_]*\): test$/\1/p')"
[[ -n "${selected}" ]] || { echo "error: no devices_* test matches '${filter}'" >&2; exit 2; }
echo "selected tests: $(tr '\n' ' ' <<< "${selected}")"
need() { grep -q "^devices_.*$1" <<< "${selected}"; }

if [[ -z "${DROPUMI_TEST_LAN_ADDR:-}" ]]; then
  DROPUMI_TEST_LAN_ADDR="$(ipconfig getifaddr en0 || true)"
  export DROPUMI_TEST_LAN_ADDR
fi
if need iphone; then
  if [[ -z "${DROPUMI_TEST_APPLE_DEVICE:-}" ]]; then
    echo "error: the iPhone tests need DROPUMI_TEST_APPLE_DEVICE (the device's UDID;" \
      "see: xcrun devicectl list devices)" >&2
    exit 2
  fi
  [[ -n "${DROPUMI_TEST_LAN_ADDR}" ]] || { echo "error: no Wi-Fi address on en0; set DROPUMI_TEST_LAN_ADDR" >&2; exit 2; }
fi

if [[ "${build}" == yes ]]; then
  if need mac; then
    bash scripts/apple/interop.sh --build-only apple_macos_
  fi
  if need iphone; then
    bash scripts/apple/interop.sh --build-only apple_ios_device_
  fi
  if need android; then
    bash scripts/android/interop.sh --build-only
  fi
fi

if need android; then
  sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-${HOME}/Library/Android/sdk}}"
  adb="$(command -v adb || true)"
  [[ -n "${adb}" ]] || adb="${sdk}/platform-tools/adb"
  [[ -x "${adb}" ]] || { echo "error: adb not found (set ANDROID_HOME)" >&2; exit 2; }
  # The Rust harness runs the same adb (it looks in PATH first).
  adb_dir="$(dirname "${adb}")"
  export PATH="${adb_dir}:${PATH}"
  if [[ -z "${ANDROID_SERIAL:-}" ]]; then
    devices="$("${adb}" devices | awk 'NR > 1 && $2 == "device" { print $1 }')"
    count="$(printf '%s\n' "${devices}" | grep -c . || true)"
    [[ "${count}" == 1 ]] || { echo "error: ${count} Android devices connected; set ANDROID_SERIAL" >&2; exit 2; }
    export ANDROID_SERIAL="${devices}"
  fi

  app_pkg="io.github.dropumi.qa"
  test_pkg="io.github.dropumi.qa.test"
  app_apk="${root}/apps/android/app/build/outputs/apk/qa/app-qa.apk"
  test_apk="${root}/apps/android/app/build/outputs/apk/androidTest/qa/app-qa-androidTest.apk"
  cleanup() {
    set +e
    "${adb}" uninstall "${test_pkg}" >/dev/null 2>&1
    "${adb}" uninstall "${app_pkg}" >/dev/null 2>&1
    if "${adb}" shell pm path "${app_pkg}" 2>/dev/null | grep -q '^package:'; then
      echo "warning: the qa app is still installed; run: adb uninstall ${app_pkg}" >&2
    fi
  }
  trap cleanup EXIT
  for apk in "${app_apk}" "${test_apk}"; do
    [[ -f "${apk}" ]] || { echo "error: ${apk##*/} is missing; run without --no-build" >&2; exit 2; }
    "${adb}" install -r -t "${apk}" >/dev/null
  done
  sdk_int="$("${adb}" shell getprop ro.build.version.sdk | tr -d '\r')"
  abi="$("${adb}" shell getprop ro.product.cpu.abi | tr -d '\r')"
  echo "android ${ANDROID_SERIAL}: API ${sdk_int}, ${abi}"
fi

DROPUMI_TEST_DEVICES=1 cargo test -p dropumi-cli --test e2e --locked -- --ignored --test-threads=1 "${filter}"
