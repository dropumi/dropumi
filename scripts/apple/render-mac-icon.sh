#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Renders the macOS app icon PNGs from apps/apple/Design/macos-app-icon.svg
# into the macOS asset catalog. Needs rsvg-convert (librsvg); sips ships
# with macOS. Only needed when the icon artwork changes.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
src="${root}/apps/apple/Design/macos-app-icon.svg"
out="${root}/apps/apple/Dropumi/macOS/macOS.xcassets/AppIcon.appiconset"
mkdir -p "${out}"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
rsvg-convert -w 1024 -h 1024 "${src}" -o "${tmp}/master.png"
for size in 16 32 128 256 512; do
  sips -z "${size}" "${size}" "${tmp}/master.png" --out "${out}/icon_${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "${double}" "${double}" "${tmp}/master.png" --out "${out}/icon_${size}@2x.png" >/dev/null
done
echo "wrote ${out}"
