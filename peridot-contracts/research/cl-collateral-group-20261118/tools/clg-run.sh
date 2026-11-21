# Source this in zsh, in its OWN terminal (it redefines step/check/U of the cash upgrade-run.sh):  source clg-run.sh
# step <name> <clg-ceremony args...> = fresh live-Mainnet simulation -> Ledger sign -> verify -> send -> require SUCCESS.
# The signer and its HD path come from the prepared JSON (admin = path 0, T3 = path 3). Stops at the first problem and
# sends nothing after it. No keys are read; the Ledger signs. A step with a SUCCESS send file is skipped.
# pstep <name> <ledger-owner-call args...> = the same flow with the pilot tool (CCGK/CD6N close only).
T=/Users/joshua/Peridot/Peridot-Soroban/.wt-cl-group/peridot-contracts/research/cl-collateral-group-20261118/tools
P=/Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/prototypes/cl-pyusd-vault-b-pilot-tools
U=/Users/joshua/Peridot/clg-launch-20261122
RPC=https://soroban-rpc.mainnet.stellar.gateway.fm
NET="Public Global Stellar Network ; September 2015"
mkdir -p "$U"
export CLG_RUN_DIR="$U"

_sign_send() {
  local n=$1 tool=$2 hd=$3 okpat=${4:-'"ok": true'}
  local hdarg=(); [ "$hd" != "0" ] && hdarg=(--hd-path "$hd")
  echo "   Approve on the Ledger now (Stellar app, account index $hd)."
  stellar tx sign --sign-with-ledger "${hdarg[@]}" --rpc-url "$RPC" --network-passphrase "$NET" \
    "$U/$n.unsigned.xdr" > "$U/$n.signed.xdr" || { echo "SIGN FAILED $n"; return 1; }
  ( cd "$(dirname "$tool")" && node "$tool" verify-signed "$U/$n.unsigned.xdr" "$U/$n.signed.xdr" ) \
    | grep -q "$okpat" || { echo "VERIFY FAILED $n"; return 1; }
  stellar tx send --rpc-url "$RPC" --network-passphrase "$NET" "$U/$n.signed.xdr" \
    > "$U/$n.send.json" 2> "$U/$n.send.err"
  if grep -q '"status": "SUCCESS"' "$U/$n.send.json"; then
    echo "OK $n $(grep -o '"tx_hash": "[0-9a-f]*"' "$U/$n.send.json")"
  else
    echo "SEND FAILED $n"; tail -3 "$U/$n.send.err"; return 1
  fi
}
_fresh() {
  local n=$1
  if [ -e "$U/$n.send.json" ] && grep -q '"status": "SUCCESS"' "$U/$n.send.json"; then
    echo "SKIP $n (already SUCCESS)"; return 2
  fi
  local f
  for f in unsigned.xdr unsigned.json signed.xdr send.json send.err; do
    [ -e "$U/$n.$f" ] && mv "$U/$n.$f" "$U/$n.$(date +%H%M%S)-stale.$f"
  done
  return 0
}

step() {
  local n=$1; shift
  _fresh "$n"; local r=$?; [ $r -eq 2 ] && return 0
  ( cd "$T" && node clg-ceremony.cjs "$@" --out "$U/$n.unsigned.xdr" > "$U/$n.unsigned.json" ) \
    || { echo "PREPARE FAILED $n"; cat "$U/$n.unsigned.json"; return 1; }
  local hd; hd=$(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1])).hdPath)' "$U/$n.unsigned.json")
  echo "== $n: $(grep -o '"n": "G[0-9]*"' "$U/$n.unsigned.json") $(grep -o '"call": "[^"]*"' "$U/$n.unsigned.json") $(grep -o '"maxFeeXlm": [0-9.]*' "$U/$n.unsigned.json")"
  _sign_send "$n" "$T/clg-ceremony.cjs" "$hd"; local rc=$?
  grep -q '"mandatoryReset"' "$U/$n.unsigned.json" && echo "!! RATE RAISED. Reset within 30 min, whatever happens:  step g56-rate-down prepare rate.down"
  _ratewin
  return $rc
}

# After the first-liquidation raise: print the window (warns at 25 min, OVERDUE at 30) until the 8 % reset lands.
_ratewin() {
  ls "$U"/*rate-up.send.json >/dev/null 2>&1 || return 0
  ( cd "$T" && node clg-ceremony.cjs rate-window 2>/dev/null ) | node -e '
    const j=JSON.parse(require("fs").readFileSync(0,"utf8")); if (!j.raised || j.reset) process.exit(0);
    console.log((j.level==="ok"?"   ":"!! ")+"rate raised "+j.minutes+" min ago ("+j.level+"); reset by "+j.resetBy+": "+j.next);' || true
}
rwin() { ( cd "$T" && node clg-ceremony.cjs rate-window ); }

# Pilot close (CCGK/CD6N) with the existing owner tool; admin account (path 0) only.
pstep() {
  local n=$1; shift
  _fresh "$n"; local r=$?; [ $r -eq 2 ] && return 0
  ( cd "$P" && node ledger-owner-call.cjs "$@" --out "$U/$n.unsigned.xdr" > "$U/$n.prep.json" ) \
    || { echo "PREPARE FAILED $n"; cat "$U/$n.prep.json"; return 1; }
  echo "== $n: $(grep -o '"call": "[^"]*"' "$U/$n.prep.json")"
  _sign_send "$n" "$P/ledger-owner-call.cjs" 0 '"status": "SIGNED_OK"'
}

rb() { ( cd "$T" && node clg-ceremony.cjs readback "$1" ); }
gate() { ( cd "$T" && node clg-ceremony.cjs gate "$1" ); }
clg() { ( cd "$T" && node clg-ceremony.cjs "$@" ); }

# Live read-back of the new stack (read-only).
check() {
  ( cd "$T" && node clg-ceremony.cjs list ) | node -e '
    const j=JSON.parse(require("fs").readFileSync(0,"utf8"));
    console.log("ledger", j.ledger, "frozen", j.frozen);
    for (const s of j.steps) console.log((s.n||"--").padEnd(4), String(s.done).padEnd(5), s.id);'
}
