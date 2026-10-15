#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Generates the Swift and Kotlin bindings of dropumi-ffi with UniFFI (library
# mode, from a host build) into the app trees. The generated sources are
# committed, so a change to the FFI surface shows up in review (ADR-0009).
#
#   apps/apple/DropumiKit/Sources/DropumiBindings/DropumiBindings.swift
#   apps/android/core-bindings/src/main/kotlin/io/github/dropumi/ffi/dropumi_ffi.kt
#
# The C header and module map are not committed: build-apple.sh generates
# them into the XCFramework.
#
# Usage: bash scripts/ffi/bindings.sh [--check]
#   --check  regenerate into a temporary directory and fail if the committed
#            files differ (CI).

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

check=false
if [[ "${1:-}" == "--check" ]]; then
  check=true
elif [[ $# -gt 0 ]]; then
  echo "usage: $0 [--check]" >&2
  exit 2
fi

swift_dir="apps/apple/DropumiKit/Sources/DropumiBindings"
kotlin_dir="apps/android/core-bindings/src/main/kotlin/io/github/dropumi/ffi"
swift_file="DropumiBindings.swift"
kotlin_file="dropumi_ffi.kt"

cargo build --quiet --locked -p dropumi-ffi --lib
cargo build --quiet --locked -p dropumi-ffi-bindgen
case "$(uname -s)" in
  Darwin) lib="target/debug/libdropumi_ffi.dylib" ;;
  *) lib="target/debug/libdropumi_ffi.so" ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

target/debug/uniffi-bindgen-swift --swift-sources "${lib}" "${tmp}/swift"
target/debug/uniffi-bindgen generate --library "${lib}" --language kotlin \
  --no-format --out-dir "${tmp}/kotlin" >/dev/null

generated_swift="${tmp}/swift/${swift_file}"
generated_kotlin="${tmp}/kotlin/io/github/dropumi/ffi/${kotlin_file}"
for f in "${generated_swift}" "${generated_kotlin}"; do
  if [[ ! -f "${f}" ]]; then
    echo "error: the generator did not produce ${f##*/}" >&2
    exit 1
  fi
done

if [[ "${check}" == true ]]; then
  status=0
  diff -u "${swift_dir}/${swift_file}" "${generated_swift}" || status=1
  diff -u "${kotlin_dir}/${kotlin_file}" "${generated_kotlin}" || status=1
  if [[ "${status}" -ne 0 ]]; then
    echo "error: the committed bindings are out of date; run scripts/ffi/bindings.sh" >&2
    exit 1
  fi
  echo "bindings are up to date"
  exit 0
fi

mkdir -p "${swift_dir}" "${kotlin_dir}"
cp "${generated_swift}" "${swift_dir}/${swift_file}"
cp "${generated_kotlin}" "${kotlin_dir}/${kotlin_file}"
echo "wrote ${swift_dir}/${swift_file}"
echo "wrote ${kotlin_dir}/${kotlin_file}"
