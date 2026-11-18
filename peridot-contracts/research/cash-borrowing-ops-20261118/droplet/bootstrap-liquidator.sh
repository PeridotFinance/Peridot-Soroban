#!/usr/bin/env bash
# Peridot cash-market liquidator: one-time host bootstrap for a FRESH Ubuntu 24.04 Droplet (x86_64) with a DO Block Storage
# volume attached and mounted (default /mnt/cashliq). Run as root over SSH. It does NOT create the Droplet, the volume,
# a firewall, or any DigitalOcean resource, and it signs nothing on chain.
#
# What it does:
#   - installs pinned, checksum-verified Node.js and the checksum-pinned liquidator source (npm ci from the lockfile)
#   - creates system user `cashliq` (no shell, no sudo/adm/docker), data dirs on the volume (0700)
#   - creates a FRESH Stellar key ON this host as `cashliq` (0600 file); prints ONLY the public key
#   - writes /etc/peridot-cash-liquidator/liquidator.env with DRY_RUN=true and no secret
#   - installs + starts systemd unit peridot-cash-liquidator (hardened), logs appended on the volume, daily logrotate
# The key has no admin or guardian rights; the bot refuses to start if it ever equals the admin or the PauseGuardian.
#
# Operator machine (once per release):
#   COPYFILE_DISABLE=1 tar --exclude node_modules --exclude data --exclude '._*' -czf cash-liquidator-<commit>.tgz -C peridot-contracts/bots cash-liquidator
#   shasum -a 256 cash-liquidator-<commit>.tgz           # -> SRC_SHA256
#   curl -fsSLO https://nodejs.org/dist/<v22.x.y>/SHASUMS256.txt{,.sig}; gpg --verify SHASUMS256.txt.sig SHASUMS256.txt
#   grep linux-x64.tar.xz SHASUMS256.txt                 # -> NODE_SHA256
#   scp cash-liquidator-<commit>.tgz lib.sh bootstrap-liquidator.sh root@<droplet>:/root/
# Droplet:
#   NODE_VERSION=v22.x.y NODE_SHA256=<hex> SRC_TARBALL=/root/cash-liquidator-<commit>.tgz SRC_SHA256=<hex> \
#     bash /root/bootstrap-liquidator.sh
# Optional: DATA_MOUNT (default /mnt/cashliq), ALLOW_ROOT_DISK=1 (no volume: state is lost with the Droplet), RPC_URL.
#
# After it prints the public key G...: fund 25 XLM (T0, research/cash-borrowing-ops-20261118/funding.md), then as cashliq:
#   systemd-run --uid=cashliq --gid=cashliq --wait --pipe --collect \
#     --property=EnvironmentFile=/etc/peridot-cash-liquidator/liquidator.env \
#     --working-directory=/opt/peridot-cash-liquidator/current /usr/local/bin/node src/trustlines.mjs   # dry run
#   ...same with --setenv=CONFIRM_TRUSTLINES=yes to sign and submit ONCE (check the hash before any retry)
# Never flip DRY_RUN=false before the 48-72 h soak and the T1 inventory funding.
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"

DATA_MOUNT=${DATA_MOUNT:-/mnt/cashliq}
RPC_URL=${RPC_URL:-https://soroban-rpc.mainnet.stellar.gateway.fm}
preflight
if [ "${ALLOW_ROOT_DISK:-0}" != 1 ]; then mountpoint -q "$DATA_MOUNT" || die "$DATA_MOUNT is not a mounted volume (attach + mount the DO volume, or ALLOW_ROOT_DISK=1)"; fi

install_packages
install_node
install_code
make_user cashliq /var/lib/cashliq

DATA=$DATA_MOUNT/cash-liquidator
install -d -m 700 -o cashliq -g cashliq "$DATA" "$DATA/state" "$DATA/logs" "$DATA/keys"
new_log cashliq "$DATA/logs/liquidator.log"
KEY=$DATA/keys/liquidator.secret
PUB=$(ensure_key cashliq "$KEY")
[[ "$PUB" =~ ^G[A-Z2-7]{55}$ ]] || die "key creation did not return a public key"

install -d -m 750 -o root -g cashliq /etc/peridot-cash-liquidator
write_once /etc/peridot-cash-liquidator/liquidator.env root:cashliq 0640 <<EOF
# No secret in this file. The signing key is the 0600 file below, owned by cashliq, created on this host.
RPC_URL=$RPC_URL
LIQUIDATOR_PUBLIC_KEY=$PUB
LIQUIDATOR_KEY_FILE=$KEY
DRY_RUN=true
POLL_INTERVAL_MS=30000
MIN_PROFIT_BPS=200
WITHDRAW_SEIZED=true
INCLUSION_FEE_STROOPS=100000
MAX_FEE_STROOPS=20000000
MIN_FEE_XLM_RAW=200000000
ACCOUNT_RESERVE_XLM_RAW=20000000
REFRESH_INCLUSION_FEE_STROOPS=1000
REFRESH_MAX_FEE_STROOPS=100000
REFRESH_DAILY_BUDGET_STROOPS=30000000
EVENTS_LOOKBACK_LEDGERS=17280
CPU_STOP=97000000
STATE_FILE=$DATA/state/state.json
JOURNAL_FILE=$DATA/state/journal.jsonl
EOF
grep -q "^LIQUIDATOR_PUBLIC_KEY=$PUB$" /etc/peridot-cash-liquidator/liquidator.env || die "existing env file names a different public key"

cat >/etc/systemd/system/peridot-cash-liquidator.service <<EOF
[Unit]
Description=Peridot cash-market liquidator (Mainnet; DRY_RUN per env file)
Wants=network-online.target
After=network-online.target
RequiresMountsFor=$DATA_MOUNT
StartLimitIntervalSec=600
StartLimitBurst=10

[Service]
Type=simple
User=cashliq
Group=cashliq
EnvironmentFile=/etc/peridot-cash-liquidator/liquidator.env
WorkingDirectory=/opt/peridot-cash-liquidator/current
ExecStart=/usr/local/bin/node src/main.mjs
Restart=on-failure
RestartSec=30
StandardOutput=append:$DATA/logs/liquidator.log
StandardError=append:$DATA/logs/liquidator.log
UMask=0077
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictSUIDSGID=yes
LockPersonality=yes
CapabilityBoundingSet=
AmbientCapabilities=
ReadWritePaths=$DATA/state $DATA/logs
ReadOnlyPaths=$DATA/keys
MemoryMax=450M

[Install]
WantedBy=multi-user.target
EOF

cat >/etc/logrotate.d/peridot-cash-liquidator <<EOF
$DATA/logs/liquidator.log {
  su cashliq cashliq
  daily
  rotate 60
  compress
  delaycompress
  missingok
  notifempty
  copytruncate
}
EOF

systemctl daemon-reload
systemctl enable --now peridot-cash-liquidator.service
sleep 5
systemctl is-active --quiet peridot-cash-liquidator.service || echo "WARNING: service not active yet; check: tail -n 50 $DATA/logs/liquidator.log" >&2
echo "liquidator public key: $PUB"
echo "DRY_RUN=true. Logs: $DATA/logs/liquidator.log. Next: fund 25 XLM (T0), add trustlines, set params.liquidator.publicKey."
