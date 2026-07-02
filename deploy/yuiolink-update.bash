#!/usr/bin/env bash
#
# yuiolink-update -- pull the latest yuiolink-server release binary from GitHub,
# verify its SHA-256, install it, and restart the service, rolling back if the new
# binary fails its health check.
#
# Pull-based and on-demand: run it with `systemctl start yuiolink-update` (or
# directly as root). A public repo needs no auth. Installs nothing if the latest
# release is already in place.
set -euo pipefail
IFS=$'\n\t'
shopt -s nullglob

REPO="jooize/YuioLink"
ASSET="yuiolink-server-linux-amd64"
DEST="/usr/local/bin/yuiolink-server"
STATE="/var/lib/yuiolink/installed-release"
SERVICE="yuiolink"
# /healthz touches the database, so a failed migration reads as unhealthy --
# probing / would only prove the process is up.
HEALTH_URL="http://127.0.0.1:8080/healthz"
BACKUP="/usr/local/bin/yuiolink-backup"

DRY_RUN=0
FORCE=0

usage() {
    cat <<'EOF'
Usage: yuiolink-update [--force] [--dry-run] [-h|--help]

Fetch the latest YuioLink release from GitHub, verify its SHA-256, install it, and
restart the service -- rolling back to the previous binary if the new one fails its
health check. Does nothing if the latest release is already installed.

  --force     Reinstall even when the latest tag is already installed.
  --dry-run   Print the actions that would run; change nothing.
  -h, --help  Show this help and exit.

Exit codes: 0 ok / up to date, 1 input or fetch error, 3 verification or
rollback failure.
EOF
}

while (( $# )); do
    case "$1" in
        --force) FORCE=1 ;;
        --dry-run) DRY_RUN=1 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; usage >&2; exit 1 ;;
    esac
    shift
done

log() { printf '%s\n' "$*"; }
die() { printf 'error: %s\n' "$1" >&2; exit "${2:-1}"; }
run() {
    if (( DRY_RUN )); then
        printf 'DRY-RUN: '; printf '%q ' "$@"; printf '\n'
    else
        "$@"
    fi
}

command -v python3 >/dev/null 2>&1 || die "python3 is required to parse the GitHub API response" 1

# --- resolve the latest release tag (public repo, unauthenticated) -------------
body="$(mktemp)"
work="$(mktemp -d)"
trap 'rm -f -- "${body}"; rm -rf -- "${work}"' EXIT

api="https://api.github.com/repos/${REPO}/releases/latest"
code="$(curl -sL -o "${body}" -w '%{http_code}' --max-time 20 "${api}" || true)"
if [[ "${code}" == "404" ]]; then
    log "No release published yet -- nothing to install."
    exit 0
fi
[[ "${code}" == "200" ]] || die "GitHub API returned HTTP ${code} for ${api}" 1

latest="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["tag_name"])' < "${body}")" \
    || die "could not parse the latest release tag" 1
[[ -n "${latest}" ]] || die "the latest release has an empty tag" 1

installed="$(cat -- "${STATE}" 2>/dev/null || echo "none")"
if [[ "${latest}" == "${installed}" && "${FORCE}" -eq 0 ]]; then
    log "Already on ${installed} -- up to date."
    exit 0
fi
log "Updating ${SERVICE}: ${installed} -> ${latest}"

# --- download the asset + its checksum, then verify (fail closed) --------------
base="https://github.com/${REPO}/releases/download/${latest}"
curl -fsSL --max-time 120 -o "${work}/${ASSET}" "${base}/${ASSET}" \
    || die "failed to download ${ASSET}" 1
curl -fsSL --max-time 30 -o "${work}/${ASSET}.sha256" "${base}/${ASSET}.sha256" \
    || die "failed to download ${ASSET}.sha256" 1

if ! ( cd "${work}" && sha256sum -c "${ASSET}.sha256" ) >/dev/null 2>&1; then
    die "SHA-256 verification FAILED -- refusing to install" 3
fi
log "SHA-256 verified."

# --- back up, swap, restart, health-check, roll back on failure ----------------
# Snapshot the database first: migrations run forward-only on startup, so the
# binary rollback below cannot undo a schema change. Fail closed -- no snapshot,
# no update (unless there is no database yet, which yuiolink-backup treats as ok).
if [[ -x "${BACKUP}" ]]; then
    run "${BACKUP}" || die "pre-update database backup failed -- refusing to update" 3
else
    log "warning: ${BACKUP} not installed; updating without a database snapshot"
fi

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
backup="${DEST}.${stamp}.bak"
if [[ -e "${DEST}" ]]; then
    run cp -a -- "${DEST}" "${backup}"
fi
# Keep only the newest three binary backups; they otherwise accumulate forever.
# shellcheck disable=SC2012 -- our own <dest>.<UTC-stamp>.bak names: no spaces/newlines
ls -1t -- "${DEST}".*.bak 2>/dev/null | tail -n +4 | while IFS= read -r old; do
    run rm -f -- "${old}"
done
run install -m0755 -o root -g root -- "${work}/${ASSET}" "${DEST}"
run systemctl restart "${SERVICE}"

if (( DRY_RUN )); then
    log "DRY-RUN: would health-check ${HEALTH_URL} and record ${latest} in ${STATE}"
    exit 0
fi

ok=0
for _ in 1 2 3 4 5; do
    if curl -fsS -o /dev/null --max-time 5 "${HEALTH_URL}"; then ok=1; break; fi
    sleep 1
done

if (( ok )); then
    printf '%s\n' "${latest}" > "${STATE}"
    log "Updated to ${latest} and healthy."
else
    printf '%s\n' "health check failed -- rolling back to the previous binary" >&2
    if [[ -e "${backup}" ]]; then
        install -m0755 -- "${backup}" "${DEST}"
        systemctl restart "${SERVICE}"
    fi
    printf '%s\n' "note: the database was NOT rolled back; if the new release migrated the" >&2
    printf '%s\n' "schema, restore the newest snapshot from /var/lib/yuiolink/backups/" >&2
    die "rolled back after a failed health check" 3
fi
