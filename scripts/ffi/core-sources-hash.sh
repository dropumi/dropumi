#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Prints a hash of the tracked files the app libraries are built from: the
# sources and manifests of the crates dropumi-ffi links, its UniFFI config,
# and the workspace manifest and lock file. Tests and the CLI are left out,
# so changing them does not make a built core look stale.
# Used by build-apple.sh (to stamp) and scripts/apple/check-core-artifacts.sh.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${root}"
paths=(Cargo.toml Cargo.lock crates/ffi/uniffi.toml)
for crate in domain protocol application infra ffi; do
  paths+=("crates/${crate}/Cargo.toml" "crates/${crate}/src")
done
git ls-files -z -- "${paths[@]}" | xargs -0 shasum -a 256 | shasum -a 256 | cut -d' ' -f1
