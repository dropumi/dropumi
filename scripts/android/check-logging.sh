#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# The logcat rule (threat model R24; ADR-0013 D3): nothing in the app's
# main code writes to logcat, so no URI, display name, path, peer string,
# fingerprint or exception text can reach it.
#
# 1. Fails on android.util.Log (and EventLog, LogPrinter), Log., EventLog.,
#    printStackTrace, dumpStack, System.out, System.err, print/println and
#    java.util.logging in the main sources of :core-bindings (the generated
#    bindings ship in the app too), :core, :appstate and :app. There is no
#    exception: the app has no logging helper. (The plan allowed one
#    SafeLog.kt taking only a fixed event enum; none exists, so none is
#    allowed. Adding one means adding it here, by name, in a reviewed
#    change.) A module without sources fails: a check of nothing is not a
#    pass.
# 2. With --r8-config FILE (repeatable; the configuration.txt R8 writes
#    next to mapping.txt), checks that each merged R8 configuration has the
#    -assumenosideeffects rule for android.util.Log with the v, d, i, w, e,
#    wtf and println methods, as apps/android/app/proguard-rules.pro asks
#    for release and qa, so R8 removes those calls from library code too.
#
# Usage: bash scripts/android/check-logging.sh [--r8-config FILE]...
# Exit status: 0 when every check passes, 1 otherwise.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
configs=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --r8-config)
      [[ $# -ge 2 ]] || { echo "usage: $0 [--r8-config FILE]..." >&2; exit 2; }
      configs+=("$2"); shift 2 ;;
    *) echo "usage: $0 [--r8-config FILE]..." >&2; exit 2 ;;
  esac
done

status=0
pattern='android\.util\.(Log|EventLog|LogPrinter)\b|\b(Event)?Log\.|printStackTrace|dumpStack|System\.(out|err)\b|\bprintln\b|\bprint\(|java\.util\.logging'
for module in core-bindings core appstate app; do
  dir="${root}/apps/android/${module}/src/main"
  # A missing or unreadable folder is reported below, not by set -e.
  count="$( { find "${dir}" \( -name '*.kt' -o -name '*.java' \) -type f 2>/dev/null || true; } | wc -l | tr -d ' ')"
  if [[ "${count}" -eq 0 ]]; then
    echo "FAIL :${module}: no Kotlin or Java sources under apps/android/${module}/src/main"
    status=1
    continue
  fi
  # grep: 0 = a match, 1 = no match, anything else = an error (never a pass).
  rc=0
  hits="$(grep -rnE --include='*.kt' --include='*.java' "${pattern}" "${dir}")" || rc=$?
  if [[ "${rc}" -eq 1 ]]; then
    echo "ok   :${module}: no logcat output in ${count} source files"
  elif [[ "${rc}" -eq 0 ]]; then
    printf '%s\n' "${hits}" | while IFS= read -r hit; do echo "FAIL ${hit#"${root}/"}"; done
    status=1
  else
    echo "FAIL :${module}: grep failed (exit ${rc})"
    status=1
  fi
done

for config in ${configs[@]+"${configs[@]}"}; do
  name="${config#"${root}/"}"
  if [[ ! -f "${config}" ]]; then
    echo "FAIL ${name}: no such file; build the minified APK first"
    status=1
    continue
  fi
  # The rule's body, from its opening line to the closing brace.
  body="$(awk '/^-assumenosideeffects class android\.util\.Log *\{/ { on = 1; next } on && /^\}/ { exit } on' "${config}")"
  missing=""
  for method in v d i w e wtf println; do
    grep -qE "^[[:space:]]*public static int ${method}\(\.\.\.\);" <<< "${body}" || missing="${missing} ${method}"
  done
  if [[ -z "${body}" ]]; then
    echo "FAIL ${name}: no -assumenosideeffects rule for android.util.Log, or an empty one"
    status=1
  elif [[ -n "${missing}" ]]; then
    echo "FAIL ${name}: the android.util.Log rule does not list:${missing}"
    status=1
  else
    echo "ok   ${name}: R8 removes android.util.Log calls (v d i w e wtf println)"
  fi
done
exit "${status}"
