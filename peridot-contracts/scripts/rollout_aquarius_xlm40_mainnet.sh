#!/usr/bin/env bash
# XLM-only width-policy fix. Read-only unless PREFLIGHT_ONLY=false and the
# matching CONFIRM_MAINNET is supplied. Never targets the stablecoin strategies.
set -euo pipefail
ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"
MODE=${MODE:-propose}
PREFLIGHT_ONLY=${PREFLIGHT_ONLY:-true}
CONFIRM_MAINNET=${CONFIRM_MAINNET:-}
RESUME_PAUSED=${RESUME_PAUSED:-NO}
NETWORK=mainnet-gateway
IDENTITY=peridot-mainnet
INCLUSION_FEE=100000
ADMIN=GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL
CONTROLLER=CCZKDMAP23ZFL55RVITKSW4LAONQABGPSK2Y77RS64GPHOVDDFF5ENGC
MARKET=CBRJTPI3327YPP57KGIZIU4Z6APBUN5F6LJ2Q3MPKCISUQJLAQFFZECZ
STRATEGY=CB3WLG4QITFRELACDR74N63VEPICMQ35QW3DSAMF4KCFOITKOJSHH6RW
POOL=CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F
ORACLE=CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN
OLD_HASH=daf96e874faf7e7d63884609e490b16526d37eb2c49507a8523cabe873ce0d05
NEW_HASH=00a1e9097339cbd1ee194a7a7f938d8d72918a7c95f89520c23c5ea2d4b8162e
RECEIPT_HASH=016c4baed7298f4835c49cae27857232ec9de9422bf77dcb55cc97d3664b05b6
STRATEGY_WASM=target/wasm32v1-none/release/aquarius_lp_vault.xlm40.optimized.wasm
PENDING_HASH_KEY=AAAAEAAAAAEAAAABAAAADwAAABJQZW5kaW5nVXBncmFkZUhhc2gAAA==
PENDING_ETA_KEY=AAAAEAAAAAEAAAABAAAADwAAABFQZW5kaW5nVXBncmFkZUV0YQAAAA==

fail() { echo "ERROR: $*" >&2; exit 1; }
expect() {
  [[ "$2" == "$3" || "$2" == "\"$3\"" ]] || fail "$1: expected=$3 actual=$2"
}
view() {
  local id=$1; shift
  stellar contract invoke --no-cache --id "$id" --source-account "$IDENTITY" \
    --network "$NETWORK" --send no -- "$@"
}
invoke() {
  local id=$1; shift
  stellar contract invoke --no-cache --inclusion-fee "$INCLUSION_FEE" --id "$id" \
    --source-account "$IDENTITY" --network "$NETWORK" -- "$@"
}
contract_hash() { stellar contract info hash --no-cache --id "$1" --network "$NETWORK"; }
read_key() {
  stellar contract read --no-cache --id "$STRATEGY" --network "$NETWORK" \
    --output json --key-xdr "$1"
}
pending_eta() {
  read_key "$PENDING_ETA_KEY" | jq -er '[.. | objects | select(has("u64")) | .u64 | tonumber] |
    if length == 1 and .[0] > 0 then .[0] else error("invalid upgrade ETA") end'
}
width() { jq -er 'if type == "array" and length == 2 then .[1] - .[0] else error("invalid ticks") end'; }
pause_market() {
  invoke "$CONTROLLER" set_pause_deposit --market "$MARKET" --paused "$1"
  invoke "$CONTROLLER" set_pause_redeem --market "$MARKET" --paused "$1"
  invoke "$CONTROLLER" set_pause_borrow --market "$MARKET" --paused true
}

[[ "$MODE" == propose || "$MODE" == execute ]] || fail "MODE must be propose or execute"
[[ "$PREFLIGHT_ONLY" == true || "$PREFLIGHT_ONLY" == false ]] || fail "invalid PREFLIGHT_ONLY"
[[ "$RESUME_PAUSED" == YES || "$RESUME_PAUSED" == NO ]] || fail "invalid RESUME_PAUSED"
expect signer "$(stellar keys public-key "$IDENTITY")" "$ADMIN"
expect artifact "$(stellar contract info hash --wasm "$STRATEGY_WASM")" "$NEW_HASH"
expect controller_admin "$(view "$CONTROLLER" get_admin)" "$ADMIN"
expect receipt_hash "$(contract_hash "$MARKET")" "$RECEIPT_HASH"
live_hash=$(contract_hash "$STRATEGY")
[[ "$live_hash" == "$OLD_HASH" || ( "$MODE" == execute && "$live_hash" == "$NEW_HASH" ) ]] || fail "unexpected strategy binary"
expect market_admin "$(view "$MARKET" get_admin)" "$ADMIN"
expect strategy_admin "$(view "$STRATEGY" get_admin)" "$ADMIN"
expect market_strategy "$(view "$MARKET" get_boosted_vault)" "$STRATEGY"
expect receipt_binding "$(view "$STRATEGY" get_receipt_vault)" "$MARKET"
expect pool "$(view "$STRATEGY" get_pool)" "$POOL"
expect oracle "$(view "$STRATEGY" get_config | jq -er '.oracle')" "$ORACLE"
expect spacing "$(view "$POOL" get_tick_spacing)" 20
expect collateral_factor "$(view "$CONTROLLER" get_market_cf --market "$MARKET")" 0
expect borrows "$(view "$MARKET" get_total_borrowed)" 0
deposit_paused=$(view "$CONTROLLER" is_deposit_paused --market "$MARKET")
redeem_paused=$(view "$CONTROLLER" is_redeem_paused --market "$MARKET")
[[ "$deposit_paused" == "$redeem_paused" ]] || fail "mixed XLM pause state"
[[ "$deposit_paused" == false || ( "$MODE" == execute && "$deposit_paused" == true && "$RESUME_PAUSED" == YES ) ]] || fail "inspect paused XLM market before resuming"
ticks=$(view "$STRATEGY" get_ticks)
if [[ "$live_hash" == "$OLD_HASH" ]]; then
  expect old_width "$(width <<<"$ticks")" 400
fi

# Never overwrite a different proposal or reset an existing matching timelock.
pending=false
if result=$(read_key "$PENDING_HASH_KEY" 2>&1); then
  [[ "$result" == *"$NEW_HASH"* ]] || fail "different pending upgrade: $result"
  pending=true
elif [[ "$result" != *"no matching contract data entries were found"* ]]; then
  fail "cannot establish pending upgrade state: $result"
fi
if [[ "$MODE" == propose ]]; then
  if [[ "$pending" == true ]]; then
    echo "Matching XLM proposal already staged; ETA=$(pending_eta). No timelock reset."
    exit 0
  fi
  if [[ "$PREFLIGHT_ONLY" == true ]]; then
    echo "XLM proposal preflight passed. No transactions submitted."
    exit 0
  fi
  [[ "$CONFIRM_MAINNET" == PROPOSE_XLM40 ]] || fail "set CONFIRM_MAINNET=PROPOSE_XLM40"
  expect uploaded_hash "$(stellar contract upload --no-cache --inclusion-fee "$INCLUSION_FEE" \
    --wasm "$STRATEGY_WASM" --source-account "$IDENTITY" --network "$NETWORK")" "$NEW_HASH"
  invoke "$STRATEGY" propose_upgrade_wasm --admin_addr "$ADMIN" --new_wasm_hash "$NEW_HASH"
  echo "XLM-only 24-hour proposal confirmed; ETA=$(pending_eta). Position/policy unchanged."
  exit 0
fi

if [[ "$live_hash" == "$OLD_HASH" ]]; then
  [[ "$pending" == true ]] || fail "XLM upgrade not staged"
  eta=$(pending_eta)
  (( $(date +%s) >= eta + 30 )) || fail "timelock not mature; required=$((eta + 30))"
  view "$STRATEGY" upgrade_wasm --admin_addr "$ADMIN" --new_wasm_hash "$NEW_HASH" >/dev/null
elif [[ "$pending" == true ]]; then
  fail "unexpected pending proposal on already upgraded strategy"
fi
if [[ "$PREFLIGHT_ONLY" == true ]]; then
  echo "XLM execution preflight passed. No transactions submitted."
  exit 0
fi
[[ "$CONFIRM_MAINNET" == MIGRATE_XLM40 ]] || fail "set CONFIRM_MAINNET=MIGRATE_XLM40"
pause_market true
expect deposit_pause "$(view "$CONTROLLER" is_deposit_paused --market "$MARKET")" true
expect redeem_pause "$(view "$CONTROLLER" is_redeem_paused --market "$MARKET")" true
expect borrow_pause "$(view "$CONTROLLER" is_borrow_paused --market "$MARKET")" true
# Snapshot only AFTER stopping user mutations. Failures below intentionally leave
# XLM deposits/redemptions paused for inspection; no automatic unsafe unpause.
ptokens=$(view "$MARKET" get_total_ptokens)
deposited=$(view "$MARKET" get_total_deposited)
shares=$(view "$STRATEGY" total_supply)
if [[ "$live_hash" == "$OLD_HASH" ]]; then
  invoke "$STRATEGY" upgrade_wasm --admin_addr "$ADMIN" --new_wasm_hash "$NEW_HASH"
fi
expect upgraded_hash "$(contract_hash "$STRATEGY")" "$NEW_HASH"
invoke "$STRATEGY" set_range_policy --admin_addr "$ADMIN" --half_width_ticks 40 \
  --rebalance_margin_ticks 20 --rebalance_cooldown 3600 --max_rebalance_divergence_bps 100 --enabled true
invoke "$STRATEGY" refresh_nav_root >/dev/null
if [[ "$(view "$STRATEGY" get_ticks | width)" != 80 ]]; then
  # A still-active cooldown must stop the rollout, not falsely report success.
  expect eligible "$(view "$STRATEGY" needs_rebalance)" true
  view "$STRATEGY" rebalance --caller "$ADMIN" >/dev/null
  expect rebalanced "$(invoke "$STRATEGY" rebalance --caller "$ADMIN")" true
fi
expect new_width "$(view "$STRATEGY" get_ticks | width)" 80
view "$STRATEGY" get_range_policy | jq -e '.enabled == true and .half_width_ticks == 40 and
  .rebalance_margin_ticks == 20 and (.rebalance_cooldown | tonumber) == 3600 and
  .max_rebalance_divergence_bps == 100' >/dev/null
liquidity=$(view "$STRATEGY" get_position_liquidity | jq -er 'tonumber | select(. > 0)')
pool_position=$(view "$POOL" get_user_position_snapshot --user "$STRATEGY")
expect pool_liquidity "$(jq -er '.raw_liquidity | tonumber' <<<"$pool_position")" "$liquidity"
expect one_range "$(jq -er '.ranges | length' <<<"$pool_position")" 1
expect pool_width "$(jq -er '.ranges[0] | .tick_upper - .tick_lower' <<<"$pool_position")" 80
invoke "$STRATEGY" refresh_nav_root >/dev/null
invoke "$MARKET" refresh_boosted_underlying >/dev/null
expect ptoken_supply "$(view "$MARKET" get_total_ptokens)" "$ptokens"
expect deposited "$(view "$MARKET" get_total_deposited)" "$deposited"
expect strategy_shares "$(view "$STRATEGY" total_supply)" "$shares"
expect final_borrows "$(view "$MARKET" get_total_borrowed)" 0
pause_market false
expect deposit_reopened "$(view "$CONTROLLER" is_deposit_paused --market "$MARKET")" false
expect redeem_reopened "$(view "$CONTROLLER" is_redeem_paused --market "$MARKET")" false
expect borrow_retained "$(view "$CONTROLLER" is_borrow_paused --market "$MARKET")" true
echo "XLM migration verified: 80-tick total width, 20-tick margin, one-hour cooldown. Stablecoin markets untouched."
