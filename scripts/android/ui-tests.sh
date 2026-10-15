#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Runs the app's instrumented tests on one device or emulator (plan §8 B/C):
# the UI state tests (fake core), the overlay tests and the real-core tests.
#
# 1. Builds the debug app, its test APK and the test-only overlay helper
#    (skip with --no-build), and installs all three with `adb install -r`.
# 2. Writes a random per-run token into the helper's private files (run-as;
#    the helper is a debuggable test build) and passes the same token to the
#    tests as the runner argument `overlay_token`. The helper's service is
#    exported only to the shell (DUMP-guarded) and draws only with it.
# 3. Pass 1: every test except @InteropApp and @CliInterop, on the fake-core
#    test Application. Pass 2: the @InteropApp tests with `-e dropumi_app
#    interop` (the app's live graph on temporary folders). A runner argument
#    applies to a whole run, hence two passes. @CliInterop tests need a CLI
#    and run only in the interop harness.
#
# An exit trap, set before anything is installed, resets the helper's
# SYSTEM_ALERT_WINDOW app-op, uninstalls the helper (it never stays on the
# device) and puts back what the tests change, in case a run dies before
# they do: the display size, density and font scale (the layout and
# large-text tests) and the lock screen setting (LockScreenTest, `locksettings
# set-disabled`). API 26-27 cannot report the lock screen setting: there the
# trap cannot put it back, and it prints a warning when the run failed.
#
# Usage: bash scripts/android/ui-tests.sh [--no-build] [--class SPEC]
#          [--exclude-local-or-device]
#   ANDROID_SERIAL picks the device when several are connected.
#   --class SPEC               only these tests (am instrument -e class)
#   --exclude-local-or-device  leave out @LocalOrDevice tests (foreground
#                              services, NSD), as the CI emulator job does
# Gradle needs JDK 17 (JAVA_HOME). Exit status: 0 when both passes report
# OK, 1 otherwise. The full output of each pass is kept in a temporary
# folder whose path is printed.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
build=yes
class=""
exclude="io.github.dropumi.InteropApp,io.github.dropumi.CliInterop"
interop_exclude="io.github.dropumi.CliInterop"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build) build=no; shift ;;
    --class) class="$2"; shift 2 ;;
    --exclude-local-or-device)
      exclude="${exclude},io.github.dropumi.LocalOrDevice"
      interop_exclude="${interop_exclude},io.github.dropumi.LocalOrDevice"
      shift ;;
    *) echo "usage: $0 [--no-build] [--class SPEC] [--exclude-local-or-device]" >&2; exit 2 ;;
  esac
done

sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-${HOME}/Library/Android/sdk}}"
adb="$(command -v adb || true)"
[[ -n "${adb}" ]] || adb="${sdk}/platform-tools/adb"
[[ -x "${adb}" ]] || { echo "error: adb not found (set ANDROID_HOME)" >&2; exit 2; }

if [[ -z "${ANDROID_SERIAL:-}" ]]; then
  devices="$("${adb}" devices | awk 'NR > 1 && $2 == "device" { print $1 }')"
  count="$(printf '%s\n' "${devices}" | grep -c . || true)"
  [[ "${count}" == 1 ]] || { echo "error: ${count} devices connected; set ANDROID_SERIAL" >&2; exit 2; }
  export ANDROID_SERIAL="${devices}"
fi

app_pkg="io.github.dropumi.debug"
test_pkg="io.github.dropumi.debug.test"
helper="io.github.dropumi.overlayhelper"
runner="${test_pkg}/io.github.dropumi.DropumiTestRunner"
app_apk="${root}/apps/android/app/build/outputs/apk/debug/app-debug.apk"
test_apk="${root}/apps/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"
helper_apk="${root}/apps/android/overlayhelper/build/outputs/apk/debug/overlayhelper-debug.apk"

# What the layout and large-text tests change, to put back on exit.
size_override="$("${adb}" shell wm size | sed -n 's/^Override size: *//p' | tr -d '\r')"
density_override="$("${adb}" shell wm density | sed -n 's/^Override density: *//p' | tr -d '\r')"
font_scale="$("${adb}" shell settings get system font_scale | tr -d '\r')"
# true or false; empty where `locksettings get-disabled` does not exist (API 26-27).
lock_disabled="$("${adb}" shell locksettings get-disabled 2>/dev/null | tr -d '\r' || true)"
case "${lock_disabled}" in true | false) ;; *) lock_disabled="" ;; esac

cleanup() {
  local rc=$?
  set +e
  "${adb}" shell appops set "${helper}" SYSTEM_ALERT_WINDOW default >/dev/null 2>&1
  "${adb}" uninstall "${helper}" >/dev/null 2>&1
  if "${adb}" shell pm path "${helper}" 2>/dev/null | grep -q '^package:'; then
    echo "warning: the overlay helper is still installed; run: adb uninstall ${helper}" >&2
  fi
  if [[ -n "${size_override}" ]]; then "${adb}" shell wm size "${size_override}"; else "${adb}" shell wm size reset; fi
  if [[ -n "${density_override}" ]]; then "${adb}" shell wm density "${density_override}"; else "${adb}" shell wm density reset; fi
  if [[ -n "${font_scale}" && "${font_scale}" != null ]]; then
    "${adb}" shell settings put system font_scale "${font_scale}"
  else
    "${adb}" shell settings delete system font_scale >/dev/null
  fi
  if [[ -n "${lock_disabled}" ]]; then
    "${adb}" shell locksettings set-disabled "${lock_disabled}" >/dev/null 2>&1
    if [[ "$("${adb}" shell locksettings get-disabled 2>/dev/null | tr -d '\r')" != "${lock_disabled}" ]]; then
      echo "warning: the lock screen setting could not be put back; run: adb shell locksettings set-disabled ${lock_disabled}" >&2
    fi
  elif [[ "${rc}" != 0 ]]; then
    echo "warning: this Android version cannot report the lock screen setting, so it was not put back;" \
      "if the run stopped during LockScreenTest, check the device's screen lock" >&2
  fi
}
trap cleanup EXIT

if [[ "${build}" == yes ]]; then
  "${root}/apps/android/gradlew" -p "${root}/apps/android" --quiet \
    :app:assembleDebug :app:assembleDebugAndroidTest :overlayhelper:assembleDebug
fi
for apk in "${app_apk}" "${test_apk}" "${helper_apk}"; do
  [[ -f "${apk}" ]] || { echo "error: ${apk##*/} is missing; run without --no-build" >&2; exit 2; }
  "${adb}" install -r -t "${apk}" >/dev/null
done

token="$(od -An -tx1 -N24 /dev/urandom | tr -d ' \n')"
[[ "${#token}" == 48 ]] || { echo "error: could not make a token" >&2; exit 1; }
printf '%s' "${token}" | "${adb}" shell "run-as ${helper} sh -c 'umask 077; mkdir -p files && cat > files/token'"
if [[ "$("${adb}" shell run-as "${helper}" cat files/token | tr -d '\r')" != "${token}" ]]; then
  echo "error: the token could not be written into the overlay helper's files" >&2
  exit 1
fi

logs="$(mktemp -d)"
status=0

# Runs one pass ($1: log name, $2: label, then am instrument arguments);
# prints its overlay records and its result line.
run_pass() {
  local log="${logs}/$1.txt"
  local name="$2"
  shift 2
  local args=(-w -r "$@")
  [[ -z "${class}" ]] || args+=(-e class "${class}")
  "${adb}" shell am instrument "${args[@]}" "${runner}" | tr -d '\r' > "${log}"
  sed -n 's/^INSTRUMENTATION_STATUS: dropumi_overlay=/  overlay: /p' "${log}"
  local result
  result="$(grep -E '^(OK \(|Tests run:|FAILURES!!!)' "${log}" | tr '\n' ' ' || true)"
  if grep -q '^OK (' "${log}" && ! grep -qE '^FAILURES!!!|^INSTRUMENTATION_FAILED|Process crashed' "${log}"; then
    echo "${name}: ${result}"
  else
    echo "${name}: FAILED ${result}"
    { grep -E '^INSTRUMENTATION_STATUS: (class|test)=|^INSTRUMENTATION_STATUS_CODE: -2|^INSTRUMENTATION_FAILED|Process crashed' "${log}" || true; } \
      | awk '/class=/ { c = $0 } /test=/ { t = $0 } /CODE: -2/ { print "  failed: " c " " t } /FAILED|crashed/ { print "  " $0 }' \
      | sed 's/INSTRUMENTATION_STATUS: //g'
    status=1
  fi
}

sdk_int="$("${adb}" shell getprop ro.build.version.sdk | tr -d '\r')"
abi="$("${adb}" shell getprop ro.product.cpu.abi | tr -d '\r')"
echo "device ${ANDROID_SERIAL}: API ${sdk_int}, ${abi}"
run_pass pass1 "pass 1 (fake core and real-core tests)" -e overlay_token "${token}" -e notAnnotation "${exclude}"
run_pass pass2 "pass 2 (interop Application)" -e dropumi_app interop -e annotation io.github.dropumi.InteropApp \
  -e notAnnotation "${interop_exclude}"
echo "logs: ${logs}"
exit "${status}"
