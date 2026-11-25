# Source this in zsh, in its OWN terminal:  source margin-run.sh
# Pooled margin v2 Mainnet ceremony (CEREMONY.md). Same flow as the CL group clg-run.sh.
#   step <name> prepare <stepId>          fresh live-Mainnet simulation -> Ledger sign -> verify -> send -> require SUCCESS
#   kstep <name> emergency <switch> ...   the same for an admin kill switch / K-step / rollback call
#   hstep <name> emergency pause-opens guardian   guardian (pause host key): prepare only, prints the host commands
#   hsend <name>                          after the host signed: verify the signed file, send, require SUCCESS
# The signer and its HD path come from the prepared JSON (admin = Ledger index 0). Stops at the first problem and sends
# nothing after it. No keys are read here; the Ledger (or the pause host) signs. A step with a SUCCESS send file is skipped.
T=/Users/joshua/Peridot/Peridot-Soroban/.wt-margin-assess/peridot-contracts/research/pooled-margin-20261124/ceremony
U=/Users/joshua/Peridot/margin-v2-launch-20261125
RPC=https://soroban-rpc.mainnet.stellar.gateway.fm
NET="Public Global Stellar Network ; September 2015"
mkdir -p "$U"

_send() {
  local n=$1
  ( cd "$T" && node margin-ceremony.cjs verify-signed "$U/$n.unsigned.xdr" "$U/$n.signed.xdr" ) | grep -q '"ok": true' \
    || { echo "VERIFY FAILED $n"; return 1; }
  stellar tx send --rpc-url "$RPC" --network-passphrase "$NET" "$U/$n.signed.xdr" > "$U/$n.send.json" 2> "$U/$n.send.err"
  if grep -q '"status": "SUCCESS"' "$U/$n.send.json"; then
    echo "OK $n $(grep -o '"tx_hash": "[0-9a-f]*"' "$U/$n.send.json")"
  else
    echo "SEND FAILED $n"; tail -3 "$U/$n.send.err"; return 1
  fi
}
_sign_send() {
  local n=$1 hd=$2
  local hdarg=(); [ "$hd" != "0" ] && hdarg=(--hd-path "$hd")
  echo "   Approve on the Ledger now (Stellar app, account index $hd)."
  stellar tx sign --sign-with-ledger "${hdarg[@]}" --rpc-url "$RPC" --network-passphrase "$NET" \
    "$U/$n.unsigned.xdr" > "$U/$n.signed.xdr" || { echo "SIGN FAILED $n"; return 1; }
  _send "$n"
}
_fresh() {
  local n=$1
  if [ -e "$U/$n.send.json" ] && grep -q '"status": "SUCCESS"' "$U/$n.send.json"; then
    echo "SKIP $n (already SUCCESS)"; return 2
  fi
  local f
  for f in unsigned.xdr unsigned.json prep.out signed.xdr send.json send.err; do
    [ -e "$U/$n.$f" ] && mv "$U/$n.$f" "$U/$n.$(date +%H%M%S)-stale.$f"
  done
  return 0
}
_prep() {
  local n=$1; shift
  _fresh "$n"; local r=$?; [ $r -eq 2 ] && return 2
  ( cd "$T" && node margin-ceremony.cjs "$@" --out "$U/$n.unsigned.xdr" > "$U/$n.prep.out" ) \
    || { echo "PREPARE FAILED $n"; cat "$U/$n.prep.out"; return 1; }
  echo "== $n: $(grep -o '"n": "[A-Z]*[0-9]*"' "$U/$n.unsigned.json") $(grep -o '"call": "[^"]*"' "$U/$n.unsigned.json") $(grep -o '"maxFeeXlm": [0-9.]*' "$U/$n.unsigned.json")"
}
_hd() { node -e 'const h=JSON.parse(require("fs").readFileSync(process.argv[1])).hdPath;console.log(h==null?"none":h)' "$U/$1.unsigned.json"; }

step() {
  local n=$1; shift
  _prep "$n" "$@"; local r=$?; [ $r -eq 2 ] && return 0; [ $r -ne 0 ] && return 1
  local hd; hd=$(_hd "$n")
  [ "$hd" = "none" ] && { echo "REFUSED $n: not a Ledger signer (use hstep/hsend)"; return 1; }
  _sign_send "$n" "$hd"
}
kstep() { step "$@"; }
hstep() {
  local n=$1; shift
  _prep "$n" "$@"; local r=$?; [ $r -eq 2 ] && return 0; [ $r -ne 0 ] && return 1
  echo "   Guardian transaction: copy $U/$n.unsigned.xdr to the pause host, sign it THERE with the pause key, copy the"
  echo "   signed XDR back to $U/$n.signed.xdr, then run:  hsend $n"
}
hsend() { _send "$1"; }

rb() { ( cd "$T" && node margin-ceremony.cjs readback "$1" ); }
gate() { ( cd "$T" && node margin-ceremony.cjs gate "$1" ); }
mc() { ( cd "$T" && node margin-ceremony.cjs "$@" ); }
check() {
  ( cd "$T" && node margin-ceremony.cjs list ) | node -e '
    const j=JSON.parse(require("fs").readFileSync(0,"utf8"));
    console.log("ledger", j.ledger, "margin", j.margin);
    for (const s of j.steps) console.log((s.n||"--").padEnd(4), String(s.done).padEnd(5), s.id);'
}
