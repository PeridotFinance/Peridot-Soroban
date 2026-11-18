#!/usr/bin/env bash
# Step 2 (TESTNET): owner opens 800 tPYUSD of staged units, then borrows 300 tUSDC against them.
# Open journey: quote -> stage_open -> convert -> mint -> 3 keeper observations -> invest -> finalise.
# If the pool fixture cannot satisfy the investment step, the minted command is cancelled instead,
# which leaves cash-backed units with the owner (StgV1 semantics); the report says which happened.
source "$(dirname "$0")/lib.sh"
AMOUNT=${AMOUNT:-8000000000}
if [[ -z "${OPEN_ID:-}" ]]; then
  q=$(view "$VAULT" stage_quote_open --amount "$AMOUNT" --tolerance_bps 20 --reserve_margin_bps 5000)
  echo "quote $q"; jlog "{\"t\":\"$(now)\",\"kind\":\"view\",\"label\":\"stage_quote_open\",\"result\":$q}"
  f() { python3 -c "import json,sys;print(json.loads(sys.argv[1])['$1'])" "$q"; }
  dl=$(( $(date +%s) + 3000 ))
  out=$(inv "stage_open" "$RECEIPT" clg-admin stage_open --owner "$ADM" --amount "$AMOUNT" --swap_in "$(f swap_in)" --min_swap "$(f min_swap)" --min_shares "$(f min_shares)" --reserve "$(f reserve)" --deadline "$dl")
  echo "$out"; id=$(echo "$out" | grep -oE 'result=[0-9]+' | cut -d= -f2); put OPEN_ID "$id"
fi
once CONVERT "stage_convert" "$RECEIPT" clg-admin stage_convert --id "$OPEN_ID"
once MINT "stage_mint" "$RECEIPT" clg-admin stage_mint --id "$OPEN_ID"
if [[ -z "${DONE_INVEST:-}${DONE_CANCEL_MINTED:-}" ]]; then
  for i in 1 2 3; do inv "keeper observation $i" "$VAULT" clg-admin stage_checkpoint_investment --keeper "$ADM" || true; sleep 7; done
  if inv "stage_invest" "$RECEIPT" clg-admin stage_invest --id "$OPEN_ID"; then
    put DONE_INVEST 1
    once FINALISE "stage_finalise" "$RECEIPT" clg-admin stage_finalise --id "$OPEN_ID"
  else
    echo "investment step refused by the pool fixture; cancelling the minted command (cash-backed units)"
    once CANCEL_MINTED "stage_cancel (minted, cash-backed units)" "$RECEIPT" clg-admin stage_cancel --id "$OPEN_ID"
  fi
fi
echo "units: $(view "$RECEIPT" stage_units --owner "$ADM")"
once ENTER_RECEIPT "enter receipt market" "$CTRL" clg-admin enter_market --user "$ADM" --market "$RECEIPT"
once ENTER_USDC "enter usdc market" "$CTRL" clg-admin enter_market --user "$ADM" --market "$USDCM"
if [[ -z "${DONE_BORROW:-}" ]]; then
  inv "L1a checkpoint_nav (borrow)" "$RECEIPT" clg-admin checkpoint_nav
  inv "borrow 300 tUSDC" "$USDCM" clg-admin borrow --user "$ADM" --amount 3000000000
  put DONE_BORROW 1
fi
echo "status: $(view "$RECEIPT" stage2_status)"
echo "liquidity: $(view "$CTRL" account_liquidity --user "$ADM")"
