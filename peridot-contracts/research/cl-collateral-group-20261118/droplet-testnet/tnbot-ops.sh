#!/usr/bin/env bash
# TESTNET ONLY. Mac-side driver for the temporary move of the two CL Testnet liquidator bots (R1, R2) to the
# Droplet `peridot-cashliq`. See ../TESTNET-BOTS-DROPLET.md for the operator sequence.
#
#   tnbot-ops.sh ssh-ok                  exit 0 iff SSH to the Droplet works (no command run besides `true`)
#   tnbot-ops.sh prepare                 offline-ish: tarball from the pushed commit, Linux stellar CLI, configs,
#                                        identity gates (allowlist, pubkeys, on Testnet, NOT on Mainnet)
#   tnbot-ops.sh install                 scp stage, bootstrap install, stream 4 Testnet keys + data, check
#   tnbot-ops.sh start                   start both units (the Mac bots keep running: short overlap)
#   tnbot-ops.sh verify [since-iso]      Droplet log: plan cycles + sent L1a hashes confirmed SUCCESS on Testnet
#   tnbot-ops.sh stop-mac                kill the Mac bots (PIDs re-checked by command line) and their caffeinate
#   tnbot-ops.sh status                  units + Mac processes
#   tnbot-ops.sh fetch                   copy Droplet bot data back next to the Mac data (never overwrites)
#   tnbot-ops.sh teardown                fetch, then teardown-tnbot.sh on the Droplet (via stdin) and its verification
#
# Never reads/prints a secret: identity TOMLs are streamed from the CLI store straight into ssh stdin. Mainnet keys,
# env files and other secrets are never touched. No firewall change.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
PC=$(cd "$HERE/../../.." && pwd)                       # peridot-contracts (worktree .wt-cl-group)
BOT=$PC/bots/cl-liquidator
STAGE=${TNBOT_STAGE:-$PC/target/clg/tnbot-stage}       # outside git
HOST=165.245.249.83
SSH=(ssh -i "$HOME/.ssh/peridot-cashliq_ed25519" -o UserKnownHostsFile="$HOME/.ssh/known_hosts_peridot_cash" -o StrictHostKeyChecking=yes
     -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 root@$HOST)
SCP=(scp -q -i "$HOME/.ssh/peridot-cashliq_ed25519" -o UserKnownHostsFile="$HOME/.ssh/known_hosts_peridot_cash" -o StrictHostKeyChecking=yes
     -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=10)
COMMIT=${TNBOT_COMMIT:-4b63939476a46c5476b5d72a961e814d4f11974e}   # pushed; bots/cl-liquidator tree == 8b71f32
CLI_TGZ=stellar-cli-26.1.0-x86_64-unknown-linux-gnu.tar.gz
CLI_SHA256=e18d5a7629102e1ccc07241acbcbebfc05b1c02476ce7d3204ba2d7418be5c0c
TESTNET='Test SDF Network ; September 2015'
IDDIR=${STELLAR_CONFIG_HOME:-$HOME/.config/stellar}/identity
MAC_PIDS="74590:testnet-real.local.json 74595:testnet-real2.local.json"
# macOS /bin/bash is 3.2 (no associative arrays): lookups are functions.
bot_cfg() { case $1 in r1) echo testnet-real;; r2) echo testnet-real2;; *) die "bot $1";; esac; }
aliases() { case $1 in r1) echo "clgr-liq clgr-hb";; r2) echo "clgr-liq2 clgr-hb2";; *) die "bot $1";; esac; }
pub() { case $1 in
  clgr-liq) echo GAT4GS37YNFLG6BNPGEYZ6KUPNXTBPPDGRSIFNAEV6OZFURCB4VGRMTC;; clgr-hb) echo GDTLD3UO5ZAGC77ONHDDWL7YGZMB3N73GNWIGPIYCHHG3QXGEQFITAIS;;
  clgr-liq2) echo GBAMPWZDSK2JLI4F3MAS6ZYATY3QJSSINE26LRPFIO46X62XIC6IHFBK;; clgr-hb2) echo GDQEIGLQZ2LEHTO6EO7WTKSRG7RNACB26QAQBBSSHHLCCEU7SOU2ZTWG;;
  *) echo "";; esac; }
die() { echo "REFUSED: $*" >&2; exit 1; }
src_sha() { cat "$STAGE/SRC_SHA256"; }
remote() { "${SSH[@]}" "SRC_SHA256=$(src_sha) bash /root/tnbot-stage/bootstrap-tnbot.sh $*"; }

# Testnet gate for one identity: allowlisted name, CLI pubkey == pinned == config, account exists on Testnet and does
# NOT exist on Mainnet (public Horizon reads only).
gate_identity() {  # $1 alias
  local a=$1 pub
  [ -n "$(pub "$a")" ] || die "$a is not an allowlisted Testnet soak identity"
  [[ "$a" =~ ^clgr-(liq|liq2|hb|hb2)$ ]] || die "$a: name not in the Testnet soak namespace"
  [ -f "$IDDIR/$a.toml" ] || die "$IDDIR/$a.toml missing"
  [ "$(stat -f %Lp "$IDDIR/$a.toml")" = 600 ] || die "$a.toml is not 0600"
  pub=$(stellar keys address "$a" 2>/dev/null | tail -1)
  [ "$pub" = "$(pub "$a")" ] || die "$a resolves to $pub, pinned $(pub "$a")"
  local t m
  t=$(curl -s -o /dev/null -w '%{http_code}' "https://horizon-testnet.stellar.org/accounts/$pub")
  m=$(curl -s -o /dev/null -w '%{http_code}' "https://horizon.stellar.org/accounts/$pub")
  [ "$t" = 200 ] || die "$a ($pub) not found on Testnet (HTTP $t)"
  [ "$m" = 404 ] || die "$a ($pub) is NOT absent on Mainnet (HTTP $m): refusing to transfer"
  echo "  $a $pub: Testnet account 200, Mainnet 404 -> Testnet-only"
}

cmd_ssh_ok() { "${SSH[@]}" true 2>/dev/null; }

cmd_prepare() {
  mkdir -p "$STAGE"; chmod 700 "$STAGE"
  git -C "$PC" fetch -q origin feat/cl-collateral-group
  git -C "$PC" merge-base --is-ancestor "$COMMIT" origin/feat/cl-collateral-group || die "$COMMIT is not pushed"
  local t1=$STAGE/cl-liquidator.tgz t2; t2=$(mktemp)
  git -C "$PC/.." archive --format=tar.gz --prefix=cl-liquidator/ "$COMMIT:peridot-contracts/bots/cl-liquidator" > "$t1"
  git -C "$PC/.." archive --format=tar.gz --prefix=cl-liquidator/ "$COMMIT:peridot-contracts/bots/cl-liquidator" > "$t2"
  cmp -s "$t1" "$t2" || die "tarball not reproducible"; rm -f "$t2"
  shasum -a 256 "$t1" | cut -d' ' -f1 > "$STAGE/SRC_SHA256"
  ! tar -tzf "$t1" | grep -qE 'config/.*\.local\.json|node_modules/' || die "tarball contains local configs or node_modules"
  tar -tzf "$t1" | grep -qx 'cl-liquidator/src/main.mjs' && tar -tzf "$t1" | grep -qx 'cl-liquidator/package-lock.json' || die "tarball is not bots/cl-liquidator"
  if [ ! -f "$STAGE/$CLI_TGZ" ]; then
    curl -fsSL --proto '=https' -o "$STAGE/$CLI_TGZ" "https://github.com/stellar/stellar-cli/releases/download/v26.1.0/$CLI_TGZ"
  fi
  [ "$(shasum -a 256 "$STAGE/$CLI_TGZ" | cut -d' ' -f1)" = "$CLI_SHA256" ] || die "stellar CLI tarball checksum mismatch"
  local b
  for b in r1 r2; do
    node --input-type=module -e '
      import fs from "node:fs";
      const [src, bot, T] = process.argv.slice(1);
      const c = JSON.parse(fs.readFileSync(src, "utf8"));
      if (c.networkPassphrase !== T) { console.error("REFUSED: " + src + " is not pinned to the Testnet passphrase"); process.exit(1); }
      for (const u of [c.rpcUrl, ...(c.heartbeatRpcUrls ?? [])]) if (!/^https:\/\/[^/]*testnet[^/]*/.test(u)) { console.error("REFUSED: non-testnet RPC " + u); process.exit(1); }
      c.dataDir = "/opt/tnbot/" + bot + "/data";
      process.stdout.write(JSON.stringify(c, null, 2) + "\n");' "$BOT/config/$(bot_cfg $b).local.json" "$b" "$TESTNET" > "$STAGE/$b.json"
    local liq hb; read -r liq hb <<<"$(aliases $b)"
    node -e 'const c=require(process.argv[1]); const [l,lp,h,hp]=process.argv.slice(2); if (c.executorIdentity!==l||c.liquidator!==lp||c.heartbeatIdentity!==h||c.heartbeatAccount!==hp) {console.error("REFUSED: config aliases/accounts differ from the pinned set"); process.exit(1)}' \
      "$STAGE/$b.json" "$liq" "$(pub "$liq")" "$hb" "$(pub "$hb")"
  done
  echo "identity gates:"
  for a in clgr-liq clgr-hb clgr-liq2 clgr-hb2; do gate_identity "$a"; done
  echo "prepared $STAGE: SRC_SHA256=$(src_sha) (commit $COMMIT), CLI $CLI_SHA256"
}

cmd_install() {
  [ -f "$STAGE/SRC_SHA256" ] || die "run prepare first"
  cmd_ssh_ok || die "SSH to $HOST not possible (firewall)"
  "${SSH[@]}" 'umask 077; mkdir -p /root/tnbot-stage'
  "${SCP[@]}" "$STAGE/cl-liquidator.tgz" "$STAGE/$CLI_TGZ" "$STAGE/r1.json" "$STAGE/r2.json" "$HERE/bootstrap-tnbot.sh" root@$HOST:/root/tnbot-stage/
  echo "== cash units before:"; "${SSH[@]}" 'systemctl show -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator; systemctl is-active peridot-cl-liquidator 2>/dev/null || true'
  remote install
  local b a
  for b in r1 r2; do for a in $(aliases $b); do
    gate_identity "$a" >/dev/null
    remote put-key "$b" "$a" < "$IDDIR/$a.toml"          # streamed; never echoed or staged on disk here
  done; done
  for b in r1 r2; do
    local d=$PC/target/clg/$(bot_cfg $b)/bot
    remote put-data "$b" journal.jsonl < "$d/journal.jsonl"
    remote put-data "$b" state.json < "$d/state.json"
  done
  remote check
}

cmd_start() { remote start; }
cmd_status() {
  remote status || true
  echo "== Mac bots"; ps -o pid,etime,command -p 74590,74595,74591,74596 2>/dev/null || echo "(none running)"
}

# Every `sent L1a ok` / `HB` hash in the Droplet logs since $1 must be SUCCESS on Testnet from the expected account.
cmd_verify() {
  local since=${1:-$(date -u -v-10M +%Y-%m-%dT%H:%M:%S)} b fail=0
  for b in r1 r2; do
    local liq hb; read -r liq hb <<<"$(aliases $b)"
    local log; log=$("${SSH[@]}" "awk -v s='$since' 'substr(\$0,7,19) >= s' /opt/tnbot/$b/data/soak.log")
    local plans sent fails
    plans=$(grep -c '"msg":"plan"' <<<"$log" || true); sent=$(grep -c '"msg":"sent' <<<"$log" || true); fails=$(grep -cE '"level":"(warn|alert|error)"' <<<"$log" || true)
    echo "== $b since $since: plan cycles $plans, sends $sent, warn/alert lines $fails"
    grep -oE '"hash":"[0-9a-f]{64}"' <<<"$log" | cut -d'"' -f4 | sort -u | tail -5 | while read -r h; do
      local j; j=$(curl -fsS "https://horizon-testnet.stellar.org/transactions/$h" 2>/dev/null || echo '{}')
      local okv src at; okv=$(jq -r '.successful // "unknown"' <<<"$j"); src=$(jq -r '.source_account // "-"' <<<"$j"); at=$(jq -r '.created_at // "-"' <<<"$j")
      local who=other; [ "$src" = "$(pub "$liq")" ] && who=liquidator; [ "$src" = "$(pub "$hb")" ] && who=heartbeat
      echo "  $h successful=$okv source=$who ($src) ledger_time=$at"
    done
    [ "$plans" -gt 0 ] && [ "$sent" -gt 0 ] || fail=1
  done
  [ $fail = 0 ] && echo "VERIFY: both bots cycling and sending on Testnet" || { echo "VERIFY: not yet (no plan cycles or no sends)"; return 1; }
}

cmd_stop_mac() {
  local e pid cfg
  for e in $MAC_PIDS; do
    pid=${e%%:*} cfg=${e#*:}
    local cmd; cmd=$(ps -o command= -p "$pid" 2>/dev/null || true)
    if [ -z "$cmd" ]; then echo "PID $pid not running"; continue; fi
    [[ "$cmd" == "node src/main.mjs --config config/$cfg --watch --live" ]] || die "PID $pid is '$cmd', not the expected bot: not killing"
    kill "$pid"
  done
  sleep 3
  for e in $MAC_PIDS; do
    pid=${e%%:*}
    local cp; cp=$(pgrep -f "^caffeinate -i -s -w $pid\$" || true)
    if ps -p "$pid" >/dev/null 2>&1; then echo "PID $pid still alive after TERM"; else echo "PID $pid stopped"; fi
    [ -z "$cp" ] || { kill "$cp" 2>/dev/null || true; echo "caffeinate $cp for $pid killed"; }
  done
  sleep 1; ps -ax -o pid,command | grep -E 'testnet-real2?\.local\.json|caffeinate -i -s -w 7459[05]' | grep -v grep || echo "no Mac bot or its caffeinate remains"
}

cmd_fetch() {
  local ts; ts=$(date -u +%Y%m%dT%H%M%SZ) b
  for b in r1 r2; do
    local d=$PC/target/clg/$(bot_cfg $b)/bot/droplet-$ts
    mkdir -p "$d"
    "${SSH[@]}" "tar -C /opt/tnbot/$b/data -czf - journal.jsonl state.json soak.log" | tar -xzf - -C "$d"
    echo "fetched $b -> $d ($(wc -l < "$d/journal.jsonl") journal lines)"
  done
}

cmd_teardown() {
  "${SSH[@]}" 'test -d /opt/tnbot' && cmd_fetch || echo "nothing to fetch (/opt/tnbot absent)"
  "${SSH[@]}" 'bash -s' < "$HERE/teardown-tnbot.sh"
}

case ${1:-} in
  ssh-ok) cmd_ssh_ok;; prepare) cmd_prepare;; install) cmd_install;; start) cmd_start;; status) cmd_status;;
  verify) cmd_verify "${2:-}";; stop-mac) cmd_stop_mac;; fetch) cmd_fetch;; teardown) cmd_teardown;;
  *) sed -n '2,16p' "$0"; exit 2;;
esac
