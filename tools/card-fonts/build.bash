#!/usr/bin/env bash
# Rebuild server/assets/fonts/noto from the Noto fonts in the nixpkgs this
# repo's flake.lock pins, so the same lock gives the same bytes.
#
# usage: tools/card-fonts/build.bash
set -euo pipefail
IFS=$'\n\t'
shopt -s nullglob

here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd -- "${here}/../.." && pwd)"
out="${repo}/server/assets/fonts/noto"

store() {
    nix build --inputs-from "${repo}" --no-link --print-out-paths "nixpkgs#$1"
}

noto="$(store noto-fonts)/share/fonts/noto"
cjk="$(store noto-fonts-cjk-sans-static)/share/fonts/opentype/noto-cjk"
emoji="$(store noto-fonts-monochrome-emoji)/share/fonts/noto"
fonttools="$(store python3Packages.fonttools)"
python="$(store python3)/bin/python3"

# Build into a fresh directory, then swap it in, so a failed run leaves the
# committed fonts as they were.
staging="$(mktemp -d)"
trap 'rm -rf -- "${staging}"' EXIT

site=("${fonttools}"/lib/python3*/site-packages)
PYTHONPATH="${site[0]}" "${python}" "${here}/build.py" \
    "${noto}" "${cjk}" "${emoji}" "${staging}"

cp -- "${here}/LICENSE.txt" "${staging}/LICENSE.txt"
rm -rf -- "${out}"
mv -- "${staging}" "${out}"
trap - EXIT
du -sh -- "${out}"
