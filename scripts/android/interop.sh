#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Android interop tests (plan §8 D): the app's real core, inside the
# instrumented AndroidInteropTest of the minified qa build, against the real
# CLI, on one device or emulator reached through adb. Runs the opt-in
# `android_*` tests of crates/cli/tests/e2e/interop/android.rs one at a time.
#
# 1. Builds the Rust core for Android if it is missing or older than the
#    sources (scripts/ffi/build-android.sh, dev profile; both ABIs, because
#    the app build refuses a core that lacks one), then the qa app and its
#    test APK (-Pdropumi.testBuildType=qa). Skip with --no-build.
# 2. Installs both with `adb install -r`.
# 3. Runs DROPUMI_TEST_ANDROID=1 cargo test … -- --ignored --test-threads=1
#    android_ (or the given filter).
#
# The android_nsd_* tests also need DROPUMI_TEST_MDNS=1: it is set only when
# the device is a real one (not an emulator), on the host's Wi-Fi.
# An exit trap uninstalls the qa app and its test APK, so the test identity
# they hold (under the app's noBackupFilesDir/interop-<uuid>/) never stays on
# the device.
#
# Usage: bash scripts/android/interop.sh [--no-build] [test name filter, default android_]
#   ANDROID_SERIAL picks the device when several are connected.
# Gradle needs JDK 17 (JAVA_HOME).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"
build=yes
filter="android_"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build) build=no; shift ;;
    -*) echo "usage: $0 [--no-build] [test name filter]" >&2; exit 2 ;;
    *) filter="$1"; shift ;;
  esac
done

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
  [[ "${count}" == 1 ]] || { echo "error: ${count} devices connected; set ANDROID_SERIAL" >&2; exit 2; }
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

if [[ "${build}" == yes ]]; then
  if ! bash scripts/android/check-core-artifacts.sh --abis arm64-v8a,x86_64 --build-type debug >/dev/null 2>&1; then
    bash scripts/ffi/build-android.sh --abis arm64-v8a,x86_64 --profile dev
  fi
  "${root}/apps/android/gradlew" -p "${root}/apps/android" --quiet -Pdropumi.testBuildType=qa \
    :app:assembleQa :app:assembleQaAndroidTest
fi
for apk in "${app_apk}" "${test_apk}"; do
  [[ -f "${apk}" ]] || { echo "error: ${apk##*/} is missing; run without --no-build" >&2; exit 2; }
  "${adb}" install -r -t "${apk}" >/dev/null
done

sdk_int="$("${adb}" shell getprop ro.build.version.sdk | tr -d '\r')"
abi="$("${adb}" shell getprop ro.product.cpu.abi | tr -d '\r')"
qemu="$("${adb}" shell getprop ro.kernel.qemu | tr -d '\r')$("${adb}" shell getprop ro.boot.qemu | tr -d '\r')"
if [[ "${qemu}" == *1* ]]; then
  kind="emulator"
  unset DROPUMI_TEST_MDNS
else
  kind="device"
  export DROPUMI_TEST_MDNS=1
fi
echo "${kind} ${ANDROID_SERIAL}: API ${sdk_int}, ${abi}"

DROPUMI_TEST_ANDROID=1 cargo test -p dropumi-cli --test e2e --locked -- --ignored --test-threads=1 "${filter}"
