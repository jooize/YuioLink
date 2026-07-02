#!/usr/bin/env bash
# YuioLink droplet baseline -- paste into a DigitalOcean droplet's "User data"
# field at creation. cloud-init runs it once, as root, on first boot.
#
# It hardens the box and installs Caddy + the yuiolink service scaffold. It does
# NOT build or fetch the app binary -- 512 MB will not compile Rust, so the
# binary is built off-box and copied to /usr/local/bin/yuiolink-server over SSH,
# after which `systemctl start yuiolink` brings it up.
set -euo pipefail
IFS=$'\n\t'

exec > >(tee -a /var/log/yuiolink-init.log) 2>&1
echo "[yuiolink-init] start $(date -u +%Y-%m-%dT%H:%M:%SZ)"

export DEBIAN_FRONTEND=noninteractive

# --- 1. Base tooling --------------------------------------------------------
apt-get update -y
apt-get install -y \
    ufw fail2ban unattended-upgrades \
    curl gnupg openssl ca-certificates sqlite3 \
    debian-keyring debian-archive-keyring apt-transport-https

# --- 2. Firewall FIRST (allow SSH before enabling, or you lock yourself out) -
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# --- 3. SSH: key-only (your key, added at droplet creation, still works) -----
mkdir -p /etc/ssh/sshd_config.d
cat > /etc/ssh/sshd_config.d/10-yuiolink.conf <<'CONF'
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin prohibit-password
CONF
systemctl reload ssh 2>/dev/null || systemctl reload sshd 2>/dev/null || true

# --- 4. Auto-patching + SSH brute-force protection --------------------------
systemctl enable --now unattended-upgrades || true
systemctl enable --now fail2ban || true
apt-get upgrade -y

# A patched kernel/libc only takes effect after a reboot; unattended-upgrades
# leaves /run/reboot-required and otherwise waits forever for a human to act
# on it. Auto-reboot at 04:00 UTC (low-traffic) when one is actually needed --
# yuiolink and caddy are both `enable`d, so they come back up on their own.
cat > /etc/apt/apt.conf.d/52yuiolink-auto-reboot <<'CONF'
Unattended-Upgrade::Automatic-Reboot "true";
Unattended-Upgrade::Automatic-Reboot-WithUsers "true";
Unattended-Upgrade::Automatic-Reboot-Time "04:00";
CONF

# --- 5. Caddy (auto-HTTPS reverse proxy), from its official apt repo ---------
curl -fsSL 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
    | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -fsSL 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
    > /etc/apt/sources.list.d/caddy-stable.list
apt-get update -y
apt-get install -y caddy

# --- 6. Service user, data dir, config --------------------------------------
id -u yuiolink >/dev/null 2>&1 \
    || useradd --system --home /var/lib/yuiolink --shell /usr/sbin/nologin yuiolink
install -d -o yuiolink -g yuiolink -m 0750 /var/lib/yuiolink
install -d -m 0750 /etc/yuiolink

# Persistent HMAC secret for reveal tokens, generated once on the box so it
# survives restarts/redeploys (see config.rs: YUIOLINK_SECRET).
secret="$(openssl rand -hex 32)"
cat > /etc/yuiolink/yuiolink.env <<ENV
YUIOLINK_BIND=127.0.0.1:8080
YUIOLINK_BASE_URL=https://yuio.link/
YUIOLINK_DB=/var/lib/yuiolink/yuiolink.db
YUIOLINK_SECRET=${secret}
ENV
chown root:yuiolink /etc/yuiolink/yuiolink.env
chmod 0640 /etc/yuiolink/yuiolink.env

# --- 7. systemd unit (binary arrives over SSH at the ExecStart path) ---------
cat > /etc/systemd/system/yuiolink.service <<'UNIT'
[Unit]
Description=YuioLink ephemeral link shortener
After=network-online.target
Wants=network-online.target

[Service]
User=yuiolink
Group=yuiolink
EnvironmentFile=/etc/yuiolink/yuiolink.env
ExecStart=/usr/local/bin/yuiolink-server
Restart=on-failure
RestartSec=2

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectControlGroups=true
LockPersonality=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
ReadWritePaths=/var/lib/yuiolink

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable yuiolink   # enabled now; starts once the binary is in place

# --- 8. Caddy site: yuio.link -> the app ------------------------------------
cat > /etc/caddy/Caddyfile <<'CADDY'
# Point yuio.link's A/AAAA records at this droplet; Caddy then auto-provisions
# TLS. Until the yuiolink service is running you will see 502s -- expected.
yuio.link {
	encode zstd gzip
	reverse_proxy 127.0.0.1:8080
}
CADDY
systemctl reload caddy 2>/dev/null || systemctl restart caddy || true

# --- 9. Update + backup tooling (the scripts live in this repo, deploy/) -----
# Pull-based: yuiolink-update fetches the latest GitHub release binary, verifies
# its SHA-256, installs, restarts, and health-checks (rolling back on failure).
# It is a no-op when already on the latest tag, so yuiolink-update.timer polls
# it every 10 minutes for auto-deploy: tag a release and it is live within 10
# minutes, unattended. yuiolink-backup snapshots the SQLite database nightly
# and before every update.
raw="https://raw.githubusercontent.com/jooize/YuioLink/main/deploy"
curl -fsSL "${raw}/yuiolink-update.bash" -o /usr/local/bin/yuiolink-update
curl -fsSL "${raw}/yuiolink-backup.bash" -o /usr/local/bin/yuiolink-backup
chmod 0755 /usr/local/bin/yuiolink-update /usr/local/bin/yuiolink-backup
curl -fsSL "${raw}/yuiolink-update.service" -o /etc/systemd/system/yuiolink-update.service
curl -fsSL "${raw}/yuiolink-update.timer" -o /etc/systemd/system/yuiolink-update.timer
curl -fsSL "${raw}/yuiolink-backup.service" -o /etc/systemd/system/yuiolink-backup.service
curl -fsSL "${raw}/yuiolink-backup.timer" -o /etc/systemd/system/yuiolink-backup.timer
systemctl daemon-reload
systemctl enable --now yuiolink-backup.timer
systemctl enable --now yuiolink-update.timer

# --- 10. Install the current release (leaves the box serving if one exists) --
/usr/local/bin/yuiolink-update \
    || echo "[yuiolink-init] release install failed; run yuiolink-update manually"

echo "[yuiolink-init] done $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "[yuiolink-init] next: point DNS at this IP. Deploys: tag a release and"
echo "[yuiolink-init]       it auto-installs within 10 minutes (yuiolink-update.timer)."
