#!/usr/bin/env bash
# TESTNET ONLY. Removes everything bootstrap-tnbot.sh created on `peridot-cashliq`: both units, user/group tnbot,
# /opt/tnbot (code, stellar CLI, configs, Testnet identity files, data) and the staging dir /root/tnbot-stage, then
# verifies that nothing remains. Fetch the bot data first (tnbot-ops.sh fetch). Run as root on the Droplet.
# Does not touch peridot-cash-liquidator / peridot-cash-pause-watcher / peridot-cl-liquidator, their users or data.
# Residue by design: journald messages of the two units, /var/log/auth.log lines from useradd/userdel.
set -euo pipefail
[ "$(id -u)" -eq 0 ] || { echo "run as root" >&2; exit 1; }
[ "$(hostname)" = peridot-cashliq ] || [ "${ALLOW_OTHER_HOST:-0}" = 1 ] || { echo "expected host peridot-cashliq" >&2; exit 1; }
UNITS="peridot-tn-clbot-r1 peridot-tn-clbot-r2"
uid=$(id -u tnbot 2>/dev/null || true)
gid=$(getent group tnbot | cut -d: -f3 || true)

for u in $UNITS; do
  systemctl disable --now "$u.service" 2>/dev/null || true
  rm -f "/etc/systemd/system/$u.service"
  rm -rf "/etc/systemd/system/$u.service.d"
done
systemctl daemon-reload
for u in $UNITS; do systemctl reset-failed "$u.service" 2>/dev/null || true; done

[ -n "$uid" ] && pkill -KILL -u tnbot 2>/dev/null || true
# Identity files first (shred is best effort on ext4/SSD), then the whole tree.
find /opt/tnbot -path '*/stellar/identity/*' -type f -exec shred -u {} + 2>/dev/null || true
rm -rf /opt/tnbot
if [ -d /root/tnbot-stage ]; then find /root/tnbot-stage -type f -exec shred -u {} + 2>/dev/null || true; rm -rf /root/tnbot-stage; fi
if id tnbot >/dev/null 2>&1; then userdel tnbot; fi
getent group tnbot >/dev/null && groupdel tnbot || true

# ---- verify ----
fail=0
for u in $UNITS; do
  if systemctl list-unit-files "$u.service" --no-legend 2>/dev/null | grep -q .; then echo "FAIL unit file still known: $u"; fail=1; fi
  if [ "$(systemctl show -p LoadState --value "$u.service" 2>/dev/null)" != not-found ]; then echo "FAIL $u LoadState $(systemctl show -p LoadState --value "$u.service")"; fail=1; fi
done
id tnbot >/dev/null 2>&1 && { echo "FAIL user tnbot exists"; fail=1; }
getent group tnbot >/dev/null && { echo "FAIL group tnbot exists"; fail=1; }
[ -e /opt/tnbot ] && { echo "FAIL /opt/tnbot exists"; fail=1; }
[ -e /root/tnbot-stage ] && { echo "FAIL /root/tnbot-stage exists"; fail=1; }
if [ -n "$uid" ]; then
  left=$(find / /mnt/cashliq -xdev \( -uid "$uid" -o -gid "${gid:-$uid}" \) -not -path '/proc/*' 2>/dev/null | head -20)
  [ -z "$left" ] || { echo "FAIL files still owned by old tnbot uid $uid:"; echo "$left"; fail=1; }
  pgrep -u "$uid" >/dev/null && { echo "FAIL processes of uid $uid"; fail=1; }
fi
left=$(find / -xdev -name '*tnbot*' -not -path '/proc/*' -not -path '/var/log/journal/*' 2>/dev/null | head -20)
[ -z "$left" ] || { echo "FAIL paths named *tnbot*:"; echo "$left"; fail=1; }
for k in clgr-liq clgr-liq2 clgr-hb clgr-hb2; do
  left=$(find / -xdev -name "$k.toml" -not -path '/proc/*' 2>/dev/null | head -5)
  [ -z "$left" ] || { echo "FAIL identity file left: $left"; fail=1; }
done
for u in peridot-cash-liquidator peridot-cash-pause-watcher peridot-cl-liquidator; do
  systemctl show -p LoadState -p ActiveState -p ActiveEnterTimestamp -p NRestarts "$u" 2>/dev/null | tr '\n' ' '; echo " <- $u (untouched)"
done
[ $fail = 0 ] && echo "TEARDOWN VERIFIED: no tnbot unit, user, group, files, keys or processes remain" || { echo "TEARDOWN INCOMPLETE" >&2; exit 1; }
