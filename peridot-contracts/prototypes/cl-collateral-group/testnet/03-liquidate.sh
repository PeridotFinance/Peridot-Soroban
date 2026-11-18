#!/usr/bin/env bash
# Step 3 (TESTNET): a real staged liquidation on the NAV path.
# Shortfall: admin emergency_disable_collateral (a CF cut is timelocked). Then, each its own tx:
# L1a checkpoint_nav, L2 cache_price(USDC), L3 liquidate (liquidator signs), L4 real reward claim
# (fixture accrues tAQUA to the vault first), L5 sync_seized(0) within 60 s of L4.
# Finally the liquidator exits the seized units in kind (prepare_close + withdraw, min_swap 0).
source "$(dirname "$0")/lib.sh"
show() { echo "status=$(view "$RECEIPT" stage2_status)"; echo "units borrower=$(view "$RECEIPT" stage_units --owner "$ADM") liquidator=$(view "$RECEIPT" stage_units --owner "$LIQ")";
  echo "vault beneficial borrower=$(view "$VAULT" stage_beneficial --owner "$ADM") liquidator=$(view "$VAULT" stage_beneficial --owner "$LIQ")";
  echo "liquidity=$(view "$CTRL" account_liquidity --user "$ADM") debt=$(view "$USDCM" get_user_borrow_balance --user "$ADM")"; }
once DISABLE_CF "emergency_disable_collateral (shortfall)" "$CTRL" clg-admin emergency_disable_collateral --market "$RECEIPT" --acknowledge_risk true
show
# The cash market pulls the repayment with transfer_from: the liquidator approves it once.
once LIQ_APPROVE "liquidator approves the USDC market" "$USDC" clg-liq approve --owner "$LIQ" --spender "$USDCM" --amount 20000000000
if [[ -z "${DONE_L3:-}" ]]; then
  inv "L1a checkpoint_nav" "$RECEIPT" clg-liq checkpoint_nav
  inv "L2 cache_price(USDC)" "$CTRL" clg-liq cache_price --token "$USDC"
  inv "L3 liquidate 100 tUSDC" "$CTRL" clg-liq liquidate --borrower "$ADM" --repay_market "$USDCM" --collateral_market "$RECEIPT" --repay_amount 1000000000 --liquidator "$LIQ"
  put DONE_L3 1
fi
echo "pending0=$(view "$RECEIPT" stage2_pending --id 0)"; show
if [[ -z "${DONE_L5_0:-}" ]]; then
  once CREDIT_AQUA_1 "fixture: accrue 5 tAQUA to the vault" "$POOL" clg-admin credit_rewards --user "$VAULT" --aqua 50000000 --gauge 0
  inv "L4 vault.checkpoint_rewards" "$VAULT" clg-liq checkpoint_rewards
  inv "L5 sync_seized(0)" "$RECEIPT" clg-liq sync_seized --id 0
  put DONE_L5_0 1
fi
show
echo "rewards borrower=$(view "$VAULT" reward_account --a "$ADM") liquidator=$(view "$VAULT" reward_account --a "$LIQ")"
if [[ -z "${LIQ_CLOSE_ID:-}" ]]; then
  n=$(view "$RECEIPT" stage_units --owner "$LIQ" | tr -d '"')
  out=$(inv "liquidator prepare_close seized units" "$RECEIPT" clg-liq stage_prepare_close --owner "$LIQ" --shares "$n" --min_swap 0 --deadline $(( $(date +%s) + 3000 )))
  echo "$out"; put LIQ_CLOSE_ID "$(echo "$out" | grep -oE 'result=[0-9]+' | cut -d= -f2)"
fi
once LIQ_WITHDRAW "liquidator in-kind withdraw" "$RECEIPT" clg-liq stage_withdraw --id "$LIQ_CLOSE_ID"
echo "liquidator tPYUSD=$(view "$PYUSD" balance --id "$LIQ") tUSDC=$(view "$USDC" balance --id "$LIQ")"
show
