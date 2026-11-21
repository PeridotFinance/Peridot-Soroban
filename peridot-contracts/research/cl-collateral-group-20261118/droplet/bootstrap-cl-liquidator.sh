#!/usr/bin/env bash
# Peridot CL-collateral-group liquidator (bots/cl-liquidator, HG4): one-time host bootstrap. Run as root over SSH on
# EITHER the existing cash-liquidator Droplet `peridot-cashliq` (second service, its own user, data dir, keys, unit)
# OR a fresh Ubuntu 24.04 x86_64 Droplet with a DO volume mounted. It does NOT create a Droplet, volume, firewall or
# any DigitalOcean resource, and it signs nothing on chain.
#
# What it does:
#   - pinned, checksum-verified Node.js (reuses /opt/node-$NODE_VERSION if present) and the checksum-pinned
#     bots/cl-liquidator tarball (npm ci from its lockfile, host tests must pass)
#   - system user `clliq` (no shell, no sudo/adm/docker); data dirs on the volume (0700): $DATA/{state,logs,keys}
#   - TWO fresh Stellar keys created ON this host as `clliq` (0600 files), printing ONLY public keys:
#       $DATA/keys/cl-liquidator.secret   the liquidator (USDC inventory, L3, APPROVE; trustlines USDC + PYUSD)
#       $DATA/keys/cl-hb-main.secret      the fee-only D3 heartbeat account (receipt.checkpoint_nav only)
#   - /etc/peridot-cl-liquidator/mainnet.json from the release's config/mainnet.example.json with both public keys and
#     paths filled in (never overwritten; no secret inside), and cl-liquidator.env with DRY_RUN=true
#   - systemd unit peridot-cl-liquidator (hardened, DRY RUN `--watch`), logs on the volume, daily logrotate, and a
#     needrestart override so unattended upgrades never restart it (a restart resets nothing on chain but interrupts
#     the soak clock only if the bot errors; see SIGNER-KEEPER-V6.md)
# It never reads, changes or restarts peridot-cash-liquidator / peridot-cash-pause-watcher, their user, keys, env or
# data, and adds a separate needrestart file instead of editing /etc/needrestart/conf.d/peridot.conf.
#
# Operator machine (once per release; the commit must be pushed):
#   git -C .wt-cl-group archive --format=tar.gz --prefix=cl-liquidator/ <commit>:peridot-contracts/bots/cl-liquidator > cl-liquidator-<commit>.tgz
#   shasum -a 256 cl-liquidator-<commit>.tgz                # -> SRC_SHA256 (rebuild once: must be identical)
#   NODE_VERSION / NODE_SHA256: reuse DROPLETS.md (v22.23.5, cdeb8670...) on the cash host
#   scp cl-liquidator-<commit>.tgz lib.sh bootstrap-cl-liquidator.sh root@<droplet>:/root/
# Droplet:
#   NODE_VERSION=v22.23.5 NODE_SHA256=<hex> SRC_TARBALL=/root/cl-liquidator-<commit>.tgz SRC_SHA256=<hex> \
#     [SKIP_PACKAGES=1] bash /root/bootstrap-cl-liquidator.sh
# Optional: DATA_MOUNT (default /mnt/cashliq), ALLOW_ROOT_DISK=1, RPC_URL, START=0 (install but do not start).
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"

DATA_MOUNT=${DATA_MOUNT:-/mnt/cashliq}
RPC_URL=${RPC_URL:-https://soroban-rpc.mainnet.stellar.gateway.fm}
UNIT=peridot-cl-liquidator
preflight
if [ "${ALLOW_ROOT_DISK:-0}" != 1 ]; then mountpoint -q "$DATA_MOUNT" || die "$DATA_MOUNT is not a mounted volume (or ALLOW_ROOT_DISK=1)"; fi

install_packages
install_node
install_code
make_user clliq /var/lib/clliq
id cashliq >/dev/null 2>&1 && [ "$(id -u clliq)" != "$(id -u cashliq)" ] || true

DATA=$DATA_MOUNT/cl-liquidator
[ "$DATA" != "$DATA_MOUNT/cash-liquidator" ] || die "refusing the cash liquidator data directory"
install -d -m 700 -o clliq -g clliq "$DATA" "$DATA/state" "$DATA/logs" "$DATA/keys"
new_log clliq "$DATA/logs/cl-liquidator.log"
LIQ_KEY=$DATA/keys/cl-liquidator.secret HB_KEY=$DATA/keys/cl-hb-main.secret
LIQ_PUB=$(ensure_key clliq "$LIQ_KEY"); HB_PUB=$(ensure_key clliq "$HB_KEY")
# Second heartbeat RPC (Alchemy; its URL carries the API key): an EMPTY 0600 placeholder owned by clliq. The operator
# fills it on this host with `read -s` + printf (SIGNER-KEEPER-V6.md §3); the bot refuses live mode while it is empty.
RPC_FILE=$DATA/keys/rpc-alchemy.url
[ -e "$RPC_FILE" ] || install -m 600 -o clliq -g clliq /dev/null "$RPC_FILE"
[[ "$LIQ_PUB" =~ ^G[A-Z2-7]{55}$ && "$HB_PUB" =~ ^G[A-Z2-7]{55}$ ]] || die "key creation did not return public keys"
[ "$LIQ_PUB" != "$HB_PUB" ] || die "liquidator and heartbeat keys must differ"
for forbidden in GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G \
  GBSK3FYCGNYZLHHPSYRJNE53NZXDFL6ED24KLT3GUUETLFKDAUD7PQX3 GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4; do
  [ "$LIQ_PUB" != "$forbidden" ] && [ "$HB_PUB" != "$forbidden" ] || die "a new key equals a reserved role key"
done

ETC=/etc/peridot-cl-liquidator
install -d -m 750 -o root -g clliq "$ETC"
/usr/local/bin/node --input-type=module -e '
  import fs from "node:fs";
  const [example, liq, hb, data, rpc] = process.argv.slice(1);
  const c = JSON.parse(fs.readFileSync(example, "utf8"));
  Object.assign(c, {rpcUrl: rpc, liquidator: liq, heartbeatAccount: hb, executorKeyFile: data + "/keys/cl-liquidator.secret",
    heartbeatKeyFile: data + "/keys/cl-hb-main.secret", heartbeatRpcUrlFiles: [data + "/keys/rpc-alchemy.url"], dataDir: data + "/state"});
  process.stdout.write(JSON.stringify(c, null, 2) + "\n");' \
  "$APP_ROOT/current/config/mainnet.example.json" "$LIQ_PUB" "$HB_PUB" "$DATA" "$RPC_URL" | write_once "$ETC/mainnet.json" root:clliq 0640
grep -q "\"liquidator\": \"$LIQ_PUB\"" "$ETC/mainnet.json" && grep -q "\"heartbeatAccount\": \"$HB_PUB\"" "$ETC/mainnet.json" || die "existing $ETC/mainnet.json names different keys"
write_once "$ETC/cl-liquidator.env" root:clliq 0640 <<EOF2
# No secret in this file. Live needs BOTH DRY_RUN=false here AND --live in ExecStart (the live.conf drop-in).
DRY_RUN=true
EOF2

cat >/etc/systemd/system/$UNIT.service <<EOF2
[Unit]
Description=Peridot CL collateral group liquidator (Mainnet; dry run unless the live.conf drop-in is installed)
Wants=network-online.target
After=network-online.target
RequiresMountsFor=$DATA_MOUNT
StartLimitIntervalSec=600
StartLimitBurst=10

[Service]
Type=simple
User=clliq
Group=clliq
EnvironmentFile=$ETC/cl-liquidator.env
WorkingDirectory=$APP_ROOT/current
ExecStart=/usr/local/bin/node src/main.mjs --config $ETC/mainnet.json --watch
Restart=on-failure
RestartSec=30
StandardOutput=append:$DATA/logs/cl-liquidator.log
StandardError=append:$DATA/logs/cl-liquidator.log
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
ReadOnlyPaths=$DATA/keys $ETC
InaccessiblePaths=-$DATA_MOUNT/cash-liquidator -/etc/peridot-cash-liquidator
MemoryMax=450M

[Install]
WantedBy=multi-user.target
EOF2

cat >/etc/logrotate.d/$UNIT <<EOF2
$DATA/logs/cl-liquidator.log {
  su clliq clliq
  daily
  rotate 60
  compress
  delaycompress
  missingok
  notifempty
  copytruncate
}
EOF2

# needrestart: never restart this unit automatically (separate file; the cash file is left untouched).
if [ -d /etc/needrestart/conf.d ]; then
  write_once /etc/needrestart/conf.d/peridot-cl.conf root:root 0644 <<'EOF2'
$nrconf{override_rc}{qr(^peridot-cl-liquidator\.service$)} = 0;
EOF2
fi

systemctl daemon-reload
if [ "${START:-1}" = 1 ]; then
  systemctl enable --now $UNIT.service
  sleep 5
  systemctl is-active --quiet $UNIT.service || echo "NOTE: not active yet (expected before the contracts exist / the accounts are funded): tail -n 50 $DATA/logs/cl-liquidator.log" >&2
else
  systemctl enable $UNIT.service
fi
echo "CL liquidator public key: $LIQ_PUB"
echo "CL heartbeat public key:  $HB_PUB"
echo "DRY_RUN=true. Config $ETC/mainnet.json, logs $DATA/logs/cl-liquidator.log. Next: clg-mainnet.json liquidator/heartbeat (HG6), G43 funding, trustlines."
