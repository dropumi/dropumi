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
# Images that refuse hard links. DocumentGuardTest#uriGuardRefusesHardLinkToIdentityKey
# and #stagingRunsUriGuard first make a hard link to the identity key inside
# the app's data. Where the image refuses link() there (EACCES on the
# arm64-v8a API 26, 28 and 36 emulators, spike S-AN3), they fail on purpose
# with "not runnable on this image (link <errno>)"; they are never skipped,
# and their no-link variants run on every image. The script prints each such
# failure by name with that label and does not count it against the exit
# status, but only when the failure is that assertion, raised by the tests'
# link step (the stack is checked). Any other failure, of these two tests or
# of any other, fails the run. On an image that allows the link, both tests
# run in full and must pass.
#
# Usage: bash scripts/android/ui-tests.sh [--no-build] [--class SPEC]
#          [--exclude-local-or-device]
#   ANDROID_SERIAL picks the device when several are connected.
#   --class SPEC               only these tests (am instrument -e class)
#   --exclude-local-or-device  leave out @LocalOrDevice tests (foreground
#                              services, NSD), as a CI emulator job would
#                              (there is none; ADR-0013 S-AN4)
# Gradle needs JDK 17 (JAVA_HOME). Exit status: 0 when both passes report
# OK, or when their only failures are the link-step failures above; 1
# otherwise. The full output of each pass is kept in a temporary folder
# whose path is printed.
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

# Prints one line per failed test in a pass log ($1): "link<TAB>label<TAB>test"
# for a failure of the two hard-link tests' link step (see the header), and
# "other<TAB><TAB>test" for any other failure. A failure's status keys come
# in any order (it differs per API level) before its
# INSTRUMENTATION_STATUS_CODE: -2 line; only the first three stack lines
# are read.
classify_failures() {
  awk '
    function reset() { cls = ""; tst = ""; s1 = ""; s2 = ""; s3 = ""; in_stack = 0; n = 0 }
    function trim(s) { sub(/^[ \t]+/, "", s); return s }
    BEGIN { reset(); g = "io.github.dropumi.files.DocumentGuardTest" }
    /^INSTRUMENTATION_STATUS: [a-z_]+=/ {
      rest = substr($0, 25); eq = index(rest, "=")
      key = substr(rest, 1, eq - 1); val = substr(rest, eq + 1)
      in_stack = 0
      if (key == "class") cls = val
      else if (key == "test") tst = val
      else if (key == "stack") { s1 = val; in_stack = 1; n = 1 }
      next
    }
    /^INSTRUMENTATION_STATUS_CODE: / {
      if (substr($0, 30) == "-2") {
        name = cls "#" tst
        link = (name == g "#uriGuardRefusesHardLinkToIdentityKey" || name == g "#stagingRunsUriGuard") &&
          s1 ~ /^java\.lang\.AssertionError: not runnable on this image \(link E[A-Z]+\)$/ &&
          index(s2, "at org.junit.Assert.fail(") == 1 &&
          index(s3, "at " g ".linkToIdentityKey(") == 1
        if (link) print "link\t" substr(s1, 27) "\t" name
        else print "other\t\t" name
      }
      reset()
      next
    }
    in_stack && n < 3 { n++; if (n == 2) s2 = trim($0); else s3 = trim($0) }
  ' "$1"
}

# Runs one pass ($1: log name, $2: label, then am instrument arguments);
# prints its overlay records, its result line and its failures.
run_pass() {
  local log="${logs}/$1.txt"
  local name="$2"
  shift 2
  local args=(-w -r "$@")
  [[ -z "${class}" ]] || args+=(-e class "${class}")
  "${adb}" shell am instrument "${args[@]}" "${runner}" | tr -d '\r' > "${log}"
  sed -n 's/^INSTRUMENTATION_STATUS: dropumi_overlay=/  overlay: /p' "${log}"
  local result failures link other broken=no
  result="$(grep -E '^(OK \(|Tests run:|FAILURES!!!)' "${log}" | tr '\n' ' ' || true)"
  failures="$(classify_failures "${log}")"
  link="$(grep -c '^link' <<< "${failures}" || true)"
  other="$(grep -c '^other' <<< "${failures}" || true)"
  if grep -qE '^INSTRUMENTATION_FAILED|Process crashed' "${log}"; then broken=yes; fi
  if [[ "${broken}" == no ]] && grep -q '^OK (' "${log}" && ! grep -q '^FAILURES!!!' "${log}"; then
    echo "${name}: ${result}"
  elif [[ "${broken}" == no && "${other}" == 0 && "${link}" -gt 0 ]] \
    && grep -qE "^Tests run: [0-9]+,  Failures: ${link}\$" "${log}"; then
    echo "${name}: passed except ${link} not runnable on this image: ${result}"
  else
    echo "${name}: FAILED ${result}"
    { grep -E '^INSTRUMENTATION_FAILED|Process crashed' "${log}" || true; } | sed 's/^/  /'
    status=1
  fi
  awk -F '\t' '$1 == "link" { print "  " $2 ": " $3 } $1 == "other" { print "  failed: " $3 }' <<< "${failures}"
}

sdk_int="$("${adb}" shell getprop ro.build.version.sdk | tr -d '\r')"
abi="$("${adb}" shell getprop ro.product.cpu.abi | tr -d '\r')"
echo "device ${ANDROID_SERIAL}: API ${sdk_int}, ${abi}"
run_pass pass1 "pass 1 (fake core and real-core tests)" -e overlay_token "${token}" -e notAnnotation "${exclude}"
run_pass pass2 "pass 2 (interop Application)" -e dropumi_app interop -e annotation io.github.dropumi.InteropApp \
  -e notAnnotation "${interop_exclude}"
echo "logs: ${logs}"
exit "${status}"
