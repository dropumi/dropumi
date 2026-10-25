#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Writes the licence notices of the Rust crates linked into
# libdropumi_ffi.so for Android to the app's assets, where the Licences
# screen shows them (docs/dependencies.md, "Third-party notices").
#
#   apps/android/app/src/main/assets/licenses/rust-crates.txt
#
# Usage: bash scripts/android/rust-notices.sh [--check]
#
# --check only compares and fails if the committed file is out of date.
# Needs cargo-about 0.9 with its CLI (`cargo install --locked cargo-about
# --features cli`), a build tool that is never part of the app. It runs
# offline and locked: licence texts come from the crate packages in the
# local Cargo registry, never from the network.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"
out="apps/android/app/src/main/assets/licenses/rust-crates.txt"
check=0
case "${1:-}" in
  --check) check=1 ;;
  "") ;;
  *) echo "usage: $0 [--check]" >&2; exit 2 ;;
esac

if ! cargo about --version >/dev/null 2>&1; then
  echo "error: cargo-about not found; install it with: cargo install --locked cargo-about --features cli" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
cargo about generate --frozen \
  --config scripts/android/notices/about.toml \
  --manifest-path crates/ffi/Cargo.toml \
  --output-file "${tmp}/raw.txt" \
  scripts/android/notices/rust-crates.hbs
# Some crates ship CRLF licence files; the repository stores LF
# (.gitattributes), so normalize line ends and trailing spaces.
sed -e $'s/\r$//' -e 's/[[:space:]]*$//' "${tmp}/raw.txt" > "${tmp}/rust-crates.txt"

if [[ "${check}" == 1 ]]; then
  if ! cmp -s "${tmp}/rust-crates.txt" "${out}"; then
    echo "error: ${out} is out of date; run: bash scripts/android/rust-notices.sh" >&2
    exit 1
  fi
  echo "${out} is up to date"
else
  mkdir -p "$(dirname "${out}")"
  cp "${tmp}/rust-crates.txt" "${out}"
  echo "wrote ${out}"
fi
