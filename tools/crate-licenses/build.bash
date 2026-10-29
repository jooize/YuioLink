#!/usr/bin/env bash
# Write server/assets/crates/LICENSE.txt: the license text of every crate linked
# into the release binary, from Cargo.lock, by the cargo-about the flake pins.
# It reads only the crate sources cargo has already fetched, so the same lock
# gives the same bytes. Run it after any change to Cargo.lock.
#
# usage: nix develop --command tools/crate-licenses/build.bash [--check]
#
#   --check  compare with the committed file instead of writing it; exit 1 if
#            it is stale
set -euo pipefail
IFS=$'\n\t'
shopt -s nullglob

usage() {
    sed -n '7,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

check=0
case "${1-}" in
    "") ;;
    --check) check=1 ;;
    -h | --help) usage; exit 0 ;;
    *) usage >&2; exit 1 ;;
esac

here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd -- "${here}/../.." && pwd)"
out="${repo}/server/assets/crates/LICENSE.txt"

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is not on PATH; run this inside nix develop" >&2
    exit 1
fi

about="$(nix build --inputs-from "${repo}" --no-link --print-out-paths nixpkgs#cargo-about)/bin/cargo-about"

work="$(mktemp -d)"
trap 'rm -rf -- "${work}"' EXIT

# --frozen: no network. Crate sources must already be fetched (cargo fetch).
"${about}" generate --frozen \
    -c "${repo}/about.toml" \
    -m "${repo}/server/Cargo.toml" \
    -o "${work}/generated.txt" \
    "${here}/about.hbs"

# A crate shipping no license file gets cargo-about's generic MIT text. Those
# crates must be exactly the ones supplement.txt names holders for, so a new
# such crate, or a stale entry, stops the build instead of passing unnoticed.
awk '
    /^-{79}$/ { crates = ""; listing = 0; next }
    / used by:$/ { listing = 1; next }
    listing && /^  [^ ]/ { split($0, f, " "); crates = crates f[1] " " f[2] "\n"; next }
    listing && /^$/ { listing = 0; next }
    /<year> <copyright holders>/ { printf "%s", crates }
' "${work}/generated.txt" | sort >"${work}/generic"
sed -n 's/^Crate: //p' "${here}/supplement.txt" | sort >"${work}/supplemented"
if ! cmp -s -- "${work}/generic" "${work}/supplemented"; then
    echo "crates under the generic MIT text differ from supplement.txt:" >&2
    diff -- "${work}/supplemented" "${work}/generic" >&2 || true
    exit 1
fi

{
    cat -- "${work}/generated.txt"
    echo
    cat -- "${here}/supplement.txt"
} >"${work}/LICENSE.txt"

if ((check)); then
    if ! cmp -s -- "${work}/LICENSE.txt" "${out}"; then
        echo "${out#"${repo}/"} is stale; run tools/crate-licenses/build.bash" >&2
        exit 1
    fi
    exit 0
fi

mkdir -p -- "$(dirname -- "${out}")"
mv -- "${work}/LICENSE.txt" "${out}"
echo "wrote ${out#"${repo}/"} ($(wc -c <"${out}" | tr -d ' ') bytes)"
