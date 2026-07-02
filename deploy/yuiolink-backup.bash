#!/usr/bin/env bash
#
# yuiolink-backup -- take a consistent snapshot of the YuioLink SQLite database
# and rotate old snapshots. Run nightly by yuiolink-backup.timer, and by
# yuiolink-update before every install (so a bad migration is recoverable).
#
# Uses sqlite3's `.backup`, which snapshots safely under WAL even mid-write --
# a plain `cp` of the db file can capture a torn state.
set -euo pipefail
IFS=$'\n\t'
shopt -s nullglob

DB="/var/lib/yuiolink/yuiolink.db"
DIR="/var/lib/yuiolink/backups"
KEEP=7

usage() {
    cat <<'EOF'
Usage: yuiolink-backup [--keep N] [-h|--help]

Snapshot the YuioLink database to /var/lib/yuiolink/backups/ and keep the
newest N snapshots (default 7). Exits 0 with a note when no database exists.
EOF
}

while (( $# )); do
    case "$1" in
        --keep) shift; KEEP="${1:?--keep needs a number}" ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; usage >&2; exit 1 ;;
    esac
    shift
done
[[ "${KEEP}" =~ ^[0-9]+$ ]] || { printf 'error: --keep must be a whole number\n' >&2; exit 1; }

command -v sqlite3 >/dev/null 2>&1 || { printf 'error: sqlite3 is required\n' >&2; exit 1; }

if [[ ! -e "${DB}" ]]; then
    printf 'no database at %s -- nothing to back up\n' "${DB}"
    exit 0
fi

install -d -o yuiolink -g yuiolink -m 0750 -- "${DIR}"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out="${DIR}/yuiolink-${stamp}.db"

sqlite3 "${DB}" ".backup '${out}'"
chown yuiolink:yuiolink -- "${out}"
chmod 0640 -- "${out}"
printf 'backed up to %s\n' "${out}"

# Rotate: newest KEEP snapshots survive.
# shellcheck disable=SC2012 -- our own yuiolink-<UTC-stamp>.db names: no spaces/newlines
ls -1t -- "${DIR}"/yuiolink-*.db 2>/dev/null | tail -n +$(( KEEP + 1 )) \
    | while IFS= read -r old; do
        rm -f -- "${old}"
        printf 'pruned %s\n' "${old}"
    done
