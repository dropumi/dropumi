#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Checks Android native libraries for 16 KB page alignment: every LOAD
# segment of every .so must be aligned to at least 0x4000. For an APK, also
# checks the zip alignment of uncompressed libraries (zipalign -P 16).
#
# Usage: bash scripts/ffi/check-16k.sh <file.so | file.apk | directory>...
#
# Needs llvm-readelf (from the NDK) and, for APKs, zipalign (build-tools).
# Both are found in ANDROID_HOME / ANDROID_NDK_HOME or on PATH.

set -euo pipefail

if [[ $# -eq 0 ]]; then
  echo "usage: $0 <file.so | file.apk | directory>..." >&2
  exit 2
fi

find_tool() {
  local name="$1"
  if command -v "${name}" >/dev/null 2>&1; then
    command -v "${name}"
    return
  fi
  local sdk="${ANDROID_HOME:-${HOME}/Library/Android/sdk}"
  local found
  found="$(find "${ANDROID_NDK_HOME:-${sdk}/ndk}" "${sdk}/build-tools" -name "${name}" \( -type f -o -type l \) 2>/dev/null | sort | tail -n 1 || true)"
  if [[ -z "${found}" ]]; then
    echo "error: ${name} not found" >&2
    exit 1
  fi
  echo "${found}"
}

readelf="$(find_tool llvm-readelf)"
status=0

check_so() {
  local so="$1" align bad="" count=0
  # The last column of each LOAD program header is its alignment (hex).
  for align in $("${readelf}" -lW "${so}" | awk '$1 == "LOAD" { print $NF }'); do
    count=$((count + 1))
    if (( align < 16384 )); then
      bad="${bad} ${align}"
    fi
  done
  if [[ "${count}" -eq 0 ]]; then
    echo "FAIL ${so}: no LOAD segments found"
    status=1
  elif [[ -n "${bad}" ]]; then
    echo "FAIL ${so}: LOAD segment alignment${bad} is below 0x4000"
    status=1
  else
    echo "ok   ${so} (${count} LOAD segments)"
  fi
}

for arg in "$@"; do
  if [[ -d "${arg}" ]]; then
    while IFS= read -r so; do check_so "${so}"; done < <(find "${arg}" -name '*.so' | sort)
  elif [[ "${arg}" == *.apk ]]; then
    tmp="$(mktemp -d)"
    unzip -q -o "${arg}" 'lib/*' -d "${tmp}"
    while IFS= read -r so; do check_so "${so}"; done < <(find "${tmp}" -name '*.so' | sort)
    rm -rf "${tmp}"
    zipalign="$(find_tool zipalign)"
    if "${zipalign}" -c -P 16 4 "${arg}" >/dev/null; then
      echo "ok   ${arg}: zip alignment"
    else
      echo "FAIL ${arg}: zip alignment (zipalign -c -P 16 4)"
      status=1
    fi
  else
    check_so "${arg}"
  fi
done
exit "${status}"
