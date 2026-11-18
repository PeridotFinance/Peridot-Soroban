#!/usr/bin/env bash
# Peridot pause-only guardian watcher: one-time bootstrap. RECOMMENDED: its own small Ubuntu 24.04 Droplet (x86_64), so a
# compromise of the liquidator host (which holds ~$1.1k of inventory) does not also yield the guardian key. Acceptable
# fallback: the liquidator Droplet with SAME_HOST=1 (separate user `cashpause`, separate 0700 key dir unreadable by
# `cashliq`; root on that host still controls both). It creates no DigitalOcean resource and signs nothing on chain.
#
# The watcher (src/pause-main.mjs) reads Reflector, and on a CONFIRMED depeg/staleness alert builds ONLY
# controller.pause_borrow_g(pauseKey, USDC|XLM|EURC market, true). It starts in DRY_RUN=true and refuses live mode until
# the Ledger ceremony step `controller.set_pause_guardian(<this public key>)` reads back. The key itself COULD also call
# pause_redeem_g / pause_liquidation_g / pause_deposit_g (72 h each) if stolen; revoke with
# `node ceremony.cjs revoke-guardian` (Ledger). Unpause is always the Ledger (`ceremony.cjs unpause-borrow <M>`).
#
# Same inputs as bootstrap-liquidator.sh (NODE_VERSION, NODE_SHA256, SRC_TARBALL, SRC_SHA256). Optional:
#   SAME_HOST=1           share the liquidator host (separate user); default expects a dedicated host
#   LIQUIDATOR_PUBLIC_KEY lets the watcher refuse to run if the two keys are ever equal (public key only)
#   DATA_DIR              default /var/lib/cashpause/data (state is small and may be recreated)
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"

DATA=${DATA_DIR:-/var/lib/cashpause/data}
RPC_URL=${RPC_URL:-https://soroban-rpc.mainnet.stellar.gateway.fm}
preflight
if [ "${SAME_HOST:-0}" != 1 ] && id cashliq >/dev/null 2>&1; then die "user cashliq exists: this looks like the liquidator host (set SAME_HOST=1 to accept a shared host)"; fi

install_packages
install_node
install_code
make_user cashpause /var/lib/cashpause

install -d -m 700 -o cashpause -g cashpause "$DATA" "$DATA/state" "$DATA/logs" "$DATA/keys"
new_log cashpause "$DATA/logs/pause-watcher.log"
KEY=$DATA/keys/pause.secret
PUB=$(ensure_key cashpause "$KEY")
[[ "$PUB" =~ ^G[A-Z2-7]{55}$ ]] || die "key creation did not return a public key"
[ "$PUB" != GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP ] || die "pause key equals the admin"
[ "$PUB" != "${LIQUIDATOR_PUBLIC_KEY:-}" ] || die "pause key equals the liquidator key"

install -d -m 750 -o root -g cashpause /etc/peridot-cash-pause
write_once /etc/peridot-cash-pause/pause.env root:cashpause 0640 <<EOF
# No secret in this file. No liquidator credentials may appear here (the watcher refuses to start if they do).
RPC_URL=$RPC_URL
PAUSE_PUBLIC_KEY=$PUB
PAUSE_KEY_FILE=$KEY
${LIQUIDATOR_PUBLIC_KEY:+LIQUIDATOR_PUBLIC_KEY=$LIQUIDATOR_PUBLIC_KEY}
DRY_RUN=true
POLL_INTERVAL_MS=30000
PAUSE_INCLUSION_FEE_STROOPS=100000
PAUSE_MAX_FEE_STROOPS=5000000
USDC_DEPEG_BPS=50
EURC_VS_EUR_BPS=100
STALE_AFTER_SECONDS=900
PAUSE_CONFIRMATIONS=2
PAUSE_CLEAR_CYCLES=20
PAUSE_STATE_FILE=$DATA/state/pause-state.json
PAUSE_JOURNAL_FILE=$DATA/state/pause-journal.jsonl
EOF
grep -q "^PAUSE_PUBLIC_KEY=$PUB$" /etc/peridot-cash-pause/pause.env || die "existing env file names a different public key"

cat >/etc/systemd/system/peridot-cash-pause-watcher.service <<EOF
[Unit]
Description=Peridot cash-market pause-only watcher (pause_borrow_g only; DRY_RUN per env file)
Wants=network-online.target
After=network-online.target
StartLimitIntervalSec=600
StartLimitBurst=10

[Service]
Type=simple
User=cashpause
Group=cashpause
EnvironmentFile=/etc/peridot-cash-pause/pause.env
WorkingDirectory=/opt/peridot-cash-liquidator/current
ExecStart=/usr/local/bin/node src/pause-main.mjs
Restart=on-failure
RestartSec=30
StandardOutput=append:$DATA/logs/pause-watcher.log
StandardError=append:$DATA/logs/pause-watcher.log
UMask=0077
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=read-only
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
InaccessiblePaths=-/mnt/cashliq -/etc/peridot-cash-liquidator
MemoryMax=300M

[Install]
WantedBy=multi-user.target
EOF

cat >/etc/logrotate.d/peridot-cash-pause-watcher <<EOF
$DATA/logs/pause-watcher.log {
  su cashpause cashpause
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
systemctl enable --now peridot-cash-pause-watcher.service
sleep 5
systemctl is-active --quiet peridot-cash-pause-watcher.service || echo "WARNING: service not active yet; check: tail -n 50 $DATA/logs/pause-watcher.log" >&2
echo "pause key public key: $PUB"
echo "DRY_RUN=true. Fund 10 XLM, put this key in params.pauseGuardian.publicKey, then the Ledger runs controller.set_pause_guardian."
