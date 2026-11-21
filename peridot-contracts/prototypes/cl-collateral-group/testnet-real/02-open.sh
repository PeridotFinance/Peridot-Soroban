#!/usr/bin/env bash
# usage: 02-open.sh <BOR1|BOR2> [amount_raw]
# Owner opens staged units that are INVESTED into the real pool position [-20,20):
# quote -> stage_open (owner) -> convert -> mint -> 3 keeper observations on consecutive ledgers +
# stage_invest within 3 ledgers (tn.mjs invest; CLI signs) -> finalise. Then enter both markets.
source "$(dirname "$0")/lib.sh"
K=$1; AMOUNT=${2:-10000000000}; ID="$(echo "$K" | tr A-Z a-z)"; OWNER=${!K}; SRC="clgr-${ID}"
# Attempt suffix: BOR1 attempt 1 was opened on an invalid quote (reserve 0) and cancelled + withdrawn.
J="${K}_${ATT:-2}"; v="OPEN_ID_$J"
if [[ -z "${!v:-}" ]]; then
  q=$(view "$VAULT" stage_quote_open --amount "$AMOUNT" --tolerance_bps 20 --reserve_margin_bps 5000)
  echo "quote $q"; [[ "$(echo "$q" | python3 -c 'import json,sys;print(json.load(sys.stdin)["valid"])')" == "True" ]] || { echo "quote not valid; refusing to open"; exit 1; }
  jlog "{\"t\":\"$(now)\",\"kind\":\"view\",\"label\":\"stage_quote_open $K\",\"result\":$q}"
  f() { python3 -c "import json,sys;print(json.loads(sys.argv[1])['$1'])" "$q"; }
  out=$(inv "stage_open $K" "$RECEIPT" "$SRC" stage_open --owner "$OWNER" --amount "$AMOUNT" --swap_in "$(f swap_in)" --min_swap "$(f min_swap)" --min_shares "$(f min_shares)" --reserve "$(f reserve)" --deadline "$(( $(date +%s) + 3000 ))")
  echo "$out"; put "$v" "$(echo "$out" | grep -oE 'result=[0-9]+' | cut -d= -f2)"
fi
OID=${!v}
once "CONVERT_$J" "stage_convert $J" "$RECEIPT" clg-admin stage_convert --id "$OID"
once "MINT_$J" "stage_mint $J" "$RECEIPT" clg-admin stage_mint --id "$OID"
if [[ -z "$(eval echo \${DONE_INVEST_$J:-})" ]]; then
  res=$(node "$HERE/tn.mjs" invest "$STATE" "$OID") || { echo "invest failed: $res"; exit 1; }
  echo "$res"
  for h in $(echo "$res" | python3 -c 'import json,sys;[print(x["hash"]+"|"+x["step"]+"|"+str(x["ledger"])) for x in json.load(sys.stdin)]'); do
    IFS='|' read -r hh st lg <<<"$h"; jlog "{\"t\":\"$(now)\",\"kind\":\"invoke\",\"label\":\"$st $K (ledger $lg)\",\"ok\":true,\"tx\":\"$hh\",\"source\":\"clg-admin\"}"
  done
  put "DONE_INVEST_$J" 1
fi
once "FINALISE_$J" "stage_finalise $J" "$RECEIPT" clg-admin stage_finalise --id "$OID"
echo "units $K: $(view "$RECEIPT" stage_units --owner "$OWNER")  vault position L: $(view "$VAULT" get_position_snapshot 2>/dev/null || true)"
once "ENTER_RECEIPT_$K" "enter receipt market $K" "$CTRL" "$SRC" enter_market --user "$OWNER" --market "$RECEIPT"
once "ENTER_USDC_$K" "enter usdc market $K" "$CTRL" "$SRC" enter_market --user "$OWNER" --market "$USDCM"
