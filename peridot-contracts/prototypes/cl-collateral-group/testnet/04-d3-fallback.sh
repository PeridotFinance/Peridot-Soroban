#!/usr/bin/env bash
# Step 4 (TESTNET, real time): D3 fallback. The pool fixture is moved out of range (tick 40). A
# heartbeat checkpoint_nav every ~30 s builds the chain; at ~9:30 the fallback simulation must
# refuse (#2112); after 10:00 L1b -> L2 -> L3 on the fallback -> L4 -> L5.
source "$(dirname "$0")/lib.sh"
SQ=$(python3 -c "print(int((2**96)*(1.0001**20)))")
once POOL_OUT "fixture: pool out of range (tick 40)" "$POOL" clg-admin set_slot0 --sqrt_price_x96 "$SQ" --tick 40
sim "fallback with no chain (expect #2111)" "$RECEIPT" clg-liq checkpoint_fallback
start=$(date +%s)
while :; do
  el=$(( $(date +%s) - start ))
  inv "D3 heartbeat checkpoint_nav t+${el}s" "$RECEIPT" clg-liq checkpoint_nav || true
  echo "chain=$(view "$RECEIPT" stage2_status)"
  if (( el >= 560 && el < 600 )) && [[ -z "${D3_EARLY:-}" ]]; then sim "fallback at t+${el}s (expect #2112)" "$RECEIPT" clg-liq checkpoint_fallback; D3_EARLY=1; fi
  (( el >= 615 )) && break
  sleep 25
done
inv "L1b checkpoint_fallback" "$RECEIPT" clg-liq checkpoint_fallback
inv "L2 cache_price(USDC)" "$CTRL" clg-liq cache_price --token "$USDC"
inv "L3 liquidate on fallback 50 tUSDC" "$CTRL" clg-liq liquidate --borrower "$ADM" --repay_market "$USDCM" --collateral_market "$RECEIPT" --repay_amount 500000000 --liquidator "$LIQ"
once CREDIT_AQUA_2 "fixture: accrue 2 tAQUA to the vault" "$POOL" clg-admin credit_rewards --user "$VAULT" --aqua 20000000 --gauge 0
inv "L4 vault.checkpoint_rewards" "$VAULT" clg-liq checkpoint_rewards
inv "L5 sync_seized(1)" "$RECEIPT" clg-liq sync_seized --id 1
echo "final status=$(view "$RECEIPT" stage2_status) units borrower=$(view "$RECEIPT" stage_units --owner "$ADM") liq=$(view "$RECEIPT" stage_units --owner "$LIQ") ben_liq=$(view "$VAULT" stage_beneficial --owner "$LIQ")"
