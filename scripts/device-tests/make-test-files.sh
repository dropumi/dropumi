#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Makes the files for the manual device matrix (docs/device-tests/TEMPLATE.md)
# and a SHA256SUMS file to check them on the receiving device.
#
# Usage: bash scripts/device-tests/make-test-files.sh <empty output dir> [large MiB, default 64]
#   TX-6 needs a file of at least 2 GiB: pass 2048.
set -euo pipefail

out="${1:?usage: make-test-files.sh <empty output dir> [large MiB]}"
large_mib="${2:-64}"
mkdir -p "${out}"
if [[ -n "$(ls -A "${out}")" ]]; then
  echo "error: ${out} is not empty" >&2
  exit 1
fi
cd "${out}"

printf 'Dropumi device test\n' > small.txt
: > empty.bin                                             # 0 bytes (TX-8)
head -c 3145735 /dev/urandom > photo-like.bin             # 3 MiB + 7 B
head -c $((large_mib * 1024 * 1024)) /dev/urandom > "large-${large_mib}MiB.bin"
printf 'unicode\n' > "Résumé – naïve 日本語.txt"           # TX-8: Unicode name
printf 'rtl\n' > "$(printf 'invoice\xe2\x80\xaetxt.exe')"  # TX-8: right-to-left override, shown stripped
mkdir many
for i in $(seq -w 1 300); do                              # TX-7: 300 small files
  head -c 1024 /dev/urandom > "many/file-${i}.bin"
done
find . -type f ! -name SHA256SUMS -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 > SHA256SUMS
echo "made $(($(wc -l < SHA256SUMS))) files in ${out}; check them on the receiver with: shasum -a 256 -c SHA256SUMS"
