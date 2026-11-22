#!/usr/bin/env bash
# TESTNET ONLY. Temporary home for the two CL Testnet liquidator bots (stacks R1 and R2) on the existing Droplet
# `peridot-cashliq`, next to (and isolated from) the Mainnet cash liquidator. Run as root on the Droplet; driven from
# the Mac by tnbot-ops.sh. Removed again by teardown-tnbot.sh after the Testnet upgrade test (22 Nov 2026).
#
#   bootstrap-tnbot.sh install            user tnbot, code, stellar CLI, configs, units (NOT started)
#   bootstrap-tnbot.sh put-key <r1|r2> <alias>   key TOML on stdin -> 0600 tnbot file (never printed)
#   bootstrap-tnbot.sh put-data <r1|r2> <journal.jsonl|state.json>   bot data snapshot on stdin
#   bootstrap-tnbot.sh check              isolation + sandboxed identity checks (reads no secret)
#   bootstrap-tnbot.sh start | status
#
# Never touches peridot-cash-liquidator / peridot-cash-pause-watcher / peridot-cl-liquidator, their users, keys, env,
# data, /usr/local/bin/node or the firewall. Signs nothing itself; the bots sign Testnet transactions only.
set -euo pipefail
umask 077

STAGE=${STAGE:-/root/tnbot-stage}
BASE=/opt/tnbot
APP=$BASE/app
BIN=$BASE/bin
USER_=tnbot
NODE=/usr/local/bin/node
NODE_DIR=/opt/node-v22.23.5
TESTNET='Test SDF Network ; September 2015'
SRC_SHA256=${SRC_SHA256:?SRC_SHA256 required}
CLI_SHA256=e18d5a7629102e1ccc07241acbcbebfc05b1c02476ce7d3204ba2d7418be5c0c   # stellar-cli 26.1.0 x86_64-unknown-linux-gnu (GitHub asset digest)
CLI_TGZ=$STAGE/stellar-cli-26.1.0-x86_64-unknown-linux-gnu.tar.gz
SRC_TGZ=$STAGE/cl-liquidator.tgz

# The ONLY identities this host may hold: Testnet keys of the soak bots, pinned to their public keys.
declare -A PUB=(
  [clgr-liq]=GAT4GS37YNFLG6BNPGEYZ6KUPNXTBPPDGRSIFNAEV6OZFURCB4VGRMTC
  [clgr-hb]=GDTLD3UO5ZAGC77ONHDDWL7YGZMB3N73GNWIGPIYCHHG3QXGEQFITAIS
  [clgr-liq2]=GBAMPWZDSK2JLI4F3MAS6ZYATY3QJSSINE26LRPFIO46X62XIC6IHFBK
  [clgr-hb2]=GDQEIGLQZ2LEHTO6EO7WTKSRG7RNACB26QAQBBSSHHLCCEU7SOU2ZTWG
)
declare -A BOT_OF=([clgr-liq]=r1 [clgr-hb]=r1 [clgr-liq2]=r2 [clgr-hb2]=r2)
# Mainnet role keys that must never appear here (admin Ledger, cash liquidator, cash pause key, investment keeper).
FORBIDDEN="GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G GBSK3FYCGNYZLHHPSYRJNE53NZXDFL6ED24KLT3GUUETLFKDAUD7PQX3 GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4"
# Mainnet secrets on this host the units must never see (also kept unreadable by DAC: other owners, 0600/0700).
INACCESSIBLE="-/mnt/cashliq -/mnt/pausewatch -/etc/peridot-cash-liquidator -/etc/peridot-cl-liquidator -/var/lib/cashliq -/var/lib/clliq -/opt/peridot-cash-liquidator -/opt/peridot-cl-liquidator"

die() { echo "REFUSED: $*" >&2; exit 1; }
[ "$(id -u)" -eq 0 ] || die "run as root"
[ "$(hostname)" = peridot-cashliq ] || [ "${ALLOW_OTHER_HOST:-0}" = 1 ] || die "expected host peridot-cashliq"

unit() { echo "peridot-tn-clbot-$1"; }
bot_dir() { echo "$BASE/$1"; }
other() { [ "$1" = r1 ] && echo r2 || echo r1; }

# One list of sandbox properties, used verbatim in the unit AND in the pre-start systemd-run checks.
props() {  # $1 bot
  local b=$1 d; d=$(bot_dir "$1")
  cat <<EOF
User=$USER_
Group=$USER_
UMask=0077
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectProc=invisible
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectKernelLogs=yes
ProtectControlGroups=yes
ProtectClock=yes
ProtectHostname=yes
RestrictSUIDSGID=yes
RestrictNamespaces=yes
RestrictRealtime=yes
LockPersonality=yes
CapabilityBoundingSet=
AmbientCapabilities=
ReadWritePaths=$d/data
InaccessiblePaths=$INACCESSIBLE -$(bot_dir "$(other "$b")")
MemoryMax=200M
MemorySwapMax=0
CPUQuota=25%
TasksMax=64
Environment=HOME=$d/data/home XDG_DATA_HOME=$d/data/home/xdg-data XDG_CACHE_HOME=$d/data/home/xdg-cache XDG_CONFIG_HOME=$d/data/home/xdg-config
Environment=STELLAR_CONFIG_HOME=$d/stellar STELLAR_SDK_DIR=$APP/current PATH=$BIN:/usr/local/bin:/usr/bin:/bin
EOF
}

aliases_of() { local b=$1 a; for a in "${!BOT_OF[@]}"; do [ "${BOT_OF[$a]}" = "$b" ] && echo "$a"; done | sort; }

check_config() {  # $1 bot: the config must pin Testnet and name exactly this bot's aliases/accounts
  local b=$1 f; f=$(bot_dir "$1")/config.json
  [ -f "$f" ] || die "$f missing"
  "$NODE" --input-type=module -e '
    import fs from "node:fs";
    const [file, bot, T, liqA, liqP, hbA, hbP] = process.argv.slice(1);
    const c = JSON.parse(fs.readFileSync(file, "utf8")), bad = m => { console.error("REFUSED: " + file + ": " + m); process.exit(1); };
    if (c.networkPassphrase !== T) bad("networkPassphrase is not the exact Testnet passphrase");
    if (/Public Global/i.test(fs.readFileSync(file, "utf8"))) bad("contains the Mainnet passphrase");
    for (const u of [c.rpcUrl, ...(c.heartbeatRpcUrls ?? [])]) if (!/^https:\/\/[^/]*testnet[^/]*\//.test(u + "/")) bad("RPC not a testnet host: " + u);
    if (c.executorIdentity !== liqA || c.liquidator !== liqP) bad("liquidator alias/account mismatch");
    if (c.heartbeatIdentity !== hbA || c.heartbeatAccount !== hbP) bad("heartbeat alias/account mismatch");
    if (c.dataDir !== "/opt/tnbot/" + bot + "/data") bad("dataDir must be /opt/tnbot/" + bot + "/data");
    for (const k of Object.keys(c)) if (/secret|seed|private|mnemonic|KeyFile/i.test(k)) bad("key-like field " + k);
  ' "$f" "$b" "$TESTNET" "$(liq_alias "$b")" "${PUB[$(liq_alias "$b")]}" "$(hb_alias "$b")" "${PUB[$(hb_alias "$b")]}"
}
liq_alias() { [ "$1" = r1 ] && echo clgr-liq || echo clgr-liq2; }
hb_alias() { [ "$1" = r1 ] && echo clgr-hb || echo clgr-hb2; }

cmd_install() {
  . /etc/os-release; [ "${ID:-}" = ubuntu ] && [ "${VERSION_ID:-}" = 24.04 ] || die "Ubuntu 24.04 expected"
  [ "$(uname -m)" = x86_64 ] || die "x86_64 expected"
  [ -x "$NODE" ] && [[ "$("$NODE" --version)" =~ ^v22\. ]] || die "$NODE (v22) missing; this script does not install or relink the shared node"
  [ -x "$NODE_DIR/bin/npm" ] || die "$NODE_DIR/bin/npm missing"
  # Data must be on the root disk, not a DO volume.
  [ "$(df --output=target /opt | tail -1)" = / ] || die "/opt is not on the root filesystem"
  for f in "$SRC_TGZ" "$CLI_TGZ" "$STAGE/r1.json" "$STAGE/r2.json"; do [ -f "$f" ] || die "$f missing (run tnbot-ops.sh install from the Mac)"; done
  echo "$SRC_SHA256  $SRC_TGZ" | sha256sum -c --quiet - || die "bot tarball checksum mismatch"
  echo "$CLI_SHA256  $CLI_TGZ" | sha256sum -c --quiet - || die "stellar CLI tarball checksum mismatch"

  # User: system account, no login shell, no password, no privileged group, home /opt/tnbot (root-owned).
  id "$USER_" >/dev/null 2>&1 || useradd --system --user-group --no-create-home --home-dir "$BASE" --shell /usr/sbin/nologin "$USER_"
  [ "$(getent passwd "$USER_" | cut -d: -f6,7)" = "$BASE:/usr/sbin/nologin" ] || die "tnbot passwd entry unexpected"
  [ "$(id -nG "$USER_")" = "$USER_" ] || die "tnbot must be in its own group only (has: $(id -nG "$USER_"))"
  passwd -l "$USER_" >/dev/null 2>&1 || true
  install -d -m 750 -o root -g "$USER_" "$BASE"
  install -d -m 755 -o root -g root "$APP" "$APP/releases" "$BIN"

  # stellar CLI (same version as the Mac: 26.1.0), root-owned, read-only for tnbot.
  if [ ! -x "$BIN/stellar" ]; then
    local t; t=$(mktemp -d); tar -xzf "$CLI_TGZ" -C "$t" --no-same-owner stellar
    install -m 755 -o root -g root "$t/stellar" "$BIN/stellar"; rm -rf "$t"
  fi
  if ldd "$BIN/stellar" | grep -q 'not found'; then ldd "$BIN/stellar" | grep 'not found' >&2; die "stellar CLI needs missing shared libraries (not installing packages)"; fi
  "$BIN/stellar" --version | head -1 | grep -q '^stellar 26.1.0 ' || die "stellar CLI version mismatch"

  # Bot code: git archive of a pushed commit, checksum-pinned, deps from its lockfile with scripts disabled.
  local rel=$APP/releases/${SRC_SHA256:0:16}
  if [ ! -d "$rel" ]; then
    rm -rf "$rel.tmp"; mkdir -p "$rel.tmp"; tar -xzf "$SRC_TGZ" -C "$rel.tmp" --strip-components=1 --no-same-owner
    [ -f "$rel.tmp/package-lock.json" ] && [ -f "$rel.tmp/src/main.mjs" ] && [ -f "$rel.tmp/src/heartbeat.mjs" ] || die "tarball is not bots/cl-liquidator"
    [ ! -e "$rel.tmp/node_modules" ] || die "tarball must not contain node_modules"
    ! ls "$rel.tmp"/config/*.local.json >/dev/null 2>&1 || die "tarball must not contain local configs"
    (cd "$rel.tmp" && PATH="$NODE_DIR/bin:$PATH" npm ci --omit=dev --ignore-scripts --no-audit --no-fund --cache "$APP/.npm-cache")
    rm -rf "$APP/.npm-cache"
    (cd "$rel.tmp" && "$NODE" --test >"$APP/host-tests.log" 2>&1) || { tail -30 "$APP/host-tests.log" >&2; die "bot tests failed on this host"; }
    grep -E '(tests|pass|fail) [0-9]+$' "$APP/host-tests.log" || true
    chown -R root:root "$rel.tmp"; chmod -R go-w "$rel.tmp"; chmod -R a+rX "$rel.tmp"
    mv "$rel.tmp" "$rel"
  fi
  ln -sfn "$rel" "$APP/current"

  local b
  for b in r1 r2; do
    local d; d=$(bot_dir $b)
    install -d -m 750 -o root -g "$USER_" "$d"
    install -d -m 700 -o "$USER_" -g "$USER_" "$d/stellar" "$d/stellar/identity" "$d/data" "$d/data/home"
    install -m 640 -o root -g "$USER_" "$STAGE/$b.json" "$d/config.json"
    check_config $b
    [ -e "$d/data/soak.log" ] || install -m 600 -o "$USER_" -g "$USER_" /dev/null "$d/data/soak.log"
    { echo "[Unit]"
      echo "Description=Peridot CL Testnet liquidator bot $b (TESTNET ONLY, temporary until the 22 Nov upgrade test)"
      echo "Wants=network-online.target"
      echo "After=network-online.target"
      echo "StartLimitIntervalSec=600"
      echo "StartLimitBurst=10"
      echo
      echo "[Service]"
      echo "Type=simple"
      props $b
      echo "WorkingDirectory=$APP/current"
      echo "ExecStart=$NODE src/main.mjs --config $d/config.json --watch --live"
      echo "Restart=on-failure"
      echo "RestartSec=15"
      echo "StandardOutput=append:$d/data/soak.log"
      echo "StandardError=append:$d/data/soak.log"
      echo
      echo "[Install]"
      echo "WantedBy=multi-user.target"
    } > "/etc/systemd/system/$(unit $b).service"
    chmod 644 "/etc/systemd/system/$(unit $b).service"
  done
  systemctl daemon-reload
  for b in r1 r2; do systemd-analyze verify "/etc/systemd/system/$(unit $b).service" 2>&1 | grep -v -E 'peridot-(cash|cl)-|snap' || true; done
  echo "installed (units NOT started). Next: put-key x4, put-data, check, start."
}

cmd_put_key() {  # $1 bot $2 alias; TOML on stdin
  local b=${1:-} a=${2:-}
  [[ "$b" =~ ^r[12]$ ]] || die "bot must be r1 or r2"
  [ -n "${PUB[$a]:-}" ] || die "identity '$a' is not an allowlisted Testnet soak identity"
  [ "${BOT_OF[$a]}" = "$b" ] || die "identity '$a' does not belong to $b"
  local dir; dir=$(bot_dir "$b")/stellar/identity
  [ -d "$dir" ] || die "run install first"
  [ ! -e "$dir/$a.toml" ] || { cat >/dev/null; echo "kept existing $a (not overwritten)"; return 0; }
  local tmp; tmp=$(mktemp "$dir/.in.XXXXXX")
  head -c 4096 > "$tmp"
  # Shape only, nothing printed: exactly one non-empty line, a seed phrase or an S... secret.
  if [ "$(grep -cv '^[[:space:]]*$' "$tmp")" != 1 ] || ! grep -qE '^(seed_phrase = "[a-z ]+"|secret_key = "S[A-Z2-7]{55}")$' "$tmp"; then
    rm -f "$tmp"; die "$a: unexpected identity file shape (content not shown)"
  fi
  chown "$USER_:$USER_" "$tmp"; chmod 600 "$tmp"; mv "$tmp" "$dir/$a.toml"
  local got; got=$(runuser -u "$USER_" -- env -i HOME="$(bot_dir "$b")/data/home" XDG_DATA_HOME="$(bot_dir "$b")/data/home/xdg-data" STELLAR_CONFIG_HOME="$(bot_dir "$b")/stellar" "$BIN/stellar" keys address "$a" 2>/dev/null | tail -1)
  local f; for f in $FORBIDDEN; do [ "$got" != "$f" ] || { rm -f "$dir/$a.toml"; die "$a resolves to a Mainnet role key: removed"; }; done
  [ "$got" = "${PUB[$a]}" ] || { rm -f "$dir/$a.toml"; die "$a resolves to $got, expected ${PUB[$a]}: removed"; }
  echo "key $a -> $b ok (public $got)"
}

cmd_put_data() {  # $1 bot $2 file; content on stdin. Snapshot of the Mac journal/state so the bot keeps its history.
  local b=${1:-} f=${2:-}
  [[ "$b" =~ ^r[12]$ ]] && [[ "$f" =~ ^(journal\.jsonl|state\.json)$ ]] || die "usage: put-data <r1|r2> <journal.jsonl|state.json>"
  local d; d=$(bot_dir "$b")/data
  systemctl is-active --quiet "$(unit "$b")" && die "$(unit "$b") is running"
  local tmp; tmp=$(mktemp "$d/.in.XXXXXX"); cat > "$tmp"
  if grep -qE '"(secret|seed_phrase|secret_key)"|\bS[A-Z2-7]{55}\b' "$tmp"; then rm -f "$tmp"; die "$f looks like it contains key material"; fi
  chown "$USER_:$USER_" "$tmp"; chmod 600 "$tmp"; mv "$tmp" "$d/$f"
  echo "data $b/$f: $(wc -l < "$d/$f") lines"
}

sandbox() {  # $1 bot, rest: command (run with the unit's own properties)
  local b=$1; shift
  local -a p=(); local l
  while IFS= read -r l; do p+=(-p "$l"); done < <(props "$b")
  systemd-run --quiet --wait --pipe --collect "${p[@]}" "$@"
}

cmd_check() {
  local fail=0 b a f
  echo "== user"; getent passwd "$USER_"; id "$USER_"
  [ -z "$(grep -rls "\b$USER_\b" /etc/sudoers /etc/sudoers.d 2>/dev/null)" ] || { echo "FAIL: tnbot named in sudoers"; fail=1; }
  echo "== Mainnet secrets: owner/mode (metadata only), and tnbot must not be able to read them (test -r, no open)"
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    if runuser -u "$USER_" -- test -r "$f"; then echo "FAIL readable by tnbot: $f"; fail=1; fi
  done < <(find /mnt/cashliq /etc/peridot-cash-liquidator /etc/peridot-cl-liquidator /root /var/lib/cashliq /var/lib/clliq -xdev -type f 2>/dev/null)
  for f in /mnt/cashliq/cash-liquidator/keys/liquidator.secret /mnt/cashliq/cl-liquidator/keys/cl-liquidator.secret /mnt/cashliq/cl-liquidator/keys/cl-hb-main.secret; do
    [ -e "$f" ] || continue
    echo "  $(stat -c '%a %U:%G' "$f") $f; parent $(stat -c '%a %U' "$(dirname "$f")"); tnbot test -r: $(runuser -u "$USER_" -- test -r "$f" && echo READABLE || echo denied)"
  done
  for b in r1 r2; do
    echo "== sandbox $b (unit properties)"
    check_config $b
    for a in $(aliases_of $b); do
      local got; got=$(sandbox $b "$BIN/stellar" keys address "$a" 2>/dev/null | tail -1)
      [ "$got" = "${PUB[$a]}" ] && echo "  $a = $got" || { echo "FAIL $a resolves to '$got'"; fail=1; }
    done
    for f in /mnt/cashliq /etc/peridot-cash-liquidator /etc/peridot-cl-liquidator "$(bot_dir "$(other $b)")/stellar" /root; do
      if sandbox $b /usr/bin/test -r "$f" 2>/dev/null; then [ -e "$f" ] && { echo "FAIL sandbox $b can read $f"; fail=1; }; else echo "  $f: inaccessible"; fi
    done
    sandbox $b /usr/bin/test -w "$APP/current" && { echo "FAIL code writable"; fail=1; } || echo "  code: read-only"
    sandbox $b /usr/bin/test -w "$(bot_dir $b)/data" && echo "  data dir: writable" || { echo "FAIL data dir not writable"; fail=1; }
    # One read-only Testnet simulation through the CLI inside the sandbox (writes only its spec cache under data/home).
    local rc; rc=$(sandbox $b "$NODE" --input-type=module -e '
      import fs from "node:fs"; import {execFileSync} from "node:child_process";
      const c = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
      const out = execFileSync("stellar", ["contract","invoke","--id",c.contracts.receipt,"--source-account",c.executorIdentity,"--rpc-url",c.rpcUrl,"--network-passphrase",c.networkPassphrase,"--send","no","--","stage2_status"], {encoding: "utf8", stdio: ["ignore","pipe","ignore"]});
      console.log(out.trim().length > 2 ? "ok" : "empty");' "$(bot_dir $b)/config.json" 2>&1 | tail -1)
    [ "$rc" = ok ] && echo "  Testnet read via CLI: ok" || { echo "FAIL Testnet read via CLI: $rc"; fail=1; }
  done
  [ $fail = 0 ] && echo "CHECK PASSED" || die "CHECK FAILED"
}

cmd_start() {
  local b
  for b in r1 r2; do systemctl enable --now "$(unit $b)"; done
  sleep 20; cmd_status
}

cmd_status() {
  local b
  for b in r1 r2; do
    echo "== $(unit $b)"
    systemctl show -p ActiveState -p SubState -p ActiveEnterTimestamp -p NRestarts -p MemoryCurrent -p MemoryPeak -p CPUUsageNSec "$(unit $b)"
    tail -n 3 "$(bot_dir $b)/data/soak.log" | cut -c1-300
  done
}

case ${1:-} in
  install) cmd_install;;
  put-key) cmd_put_key "${2:-}" "${3:-}";;
  put-data) cmd_put_data "${2:-}" "${3:-}";;
  check) cmd_check;;
  start) cmd_start;;
  status) cmd_status;;
  *) echo "usage: $0 install|put-key <bot> <alias>|put-data <bot> <file>|check|start|status" >&2; exit 2;;
esac
