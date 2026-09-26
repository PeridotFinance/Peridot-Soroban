#!/usr/bin/env bash
# PYUSD/USDC-only width-policy rollout using the already uploaded XLM candidate.
# Select LABEL=PYUSD or LABEL=USDC explicitly. No WASM upload or XLM mutation.
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
LABEL=${LABEL:?set LABEL=PYUSD or LABEL=USDC}
case "$LABEL" in
  PYUSD)
    MARKET=CBNVNCPEW2XXGBEGVMZQXBSODO5V2HMGPT5FFLVLT355SXJMYY53MLMA
    STRATEGY=CANCOWOI6R2FZBDLZKUL6BUZJN3VONPZSUUSWFL3KF3MPG5INAF25EKY
    UNDERLYING=CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2
    ;;
  USDC)
    MARKET=CBIOHQFWKSYTRET3LJV4LTO3ZQWRQMQ7I2SJAZ62IZCQHDST4YO3AZP7
    STRATEGY=CAQZ7XPUSOSBI66A4RPSNPEBI2EADBMBVUBSW6R2DYWC64QHDM3HKGIN
    UNDERLYING=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75
    ;;
  *) echo "LABEL must be PYUSD or USDC" >&2; exit 1 ;;
esac
POOL=CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX
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
  # Stellar CLI emits CSV rows even with --output json: JSON key, JSON value,
  # ledger metadata. Parse the value column, never regex-match arbitrary text.
  read_key "$PENDING_ETA_KEY" | python3 -c '
import csv, json, sys
rows = list(csv.reader(sys.stdin))
assert len(rows) == 1 and len(rows[0]) >= 2, "invalid contract-data row"
value = json.loads(rows[0][1])
assert set(value) == {"u64"}, "invalid upgrade ETA type"
eta = int(value["u64"])
assert 0 < eta < 2**64, "invalid upgrade ETA"
print(eta)
'
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
# Verify the existing on-chain WASM bytes. Never upload it again.
expect uploaded_hash "$(stellar contract fetch --no-cache --wasm-hash "$NEW_HASH" \
  --network "$NETWORK" | shasum -a 256 | awk '{print $1}')" "$NEW_HASH"
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
expect settlement "$(view "$STRATEGY" get_underlying)" "$UNDERLYING"
expect collateral_factor "$(view "$CONTROLLER" get_market_cf --market "$MARKET")" 0
expect borrows "$(view "$MARKET" get_total_borrowed)" 0
deposit_paused=$(view "$CONTROLLER" is_deposit_paused --market "$MARKET")
redeem_paused=$(view "$CONTROLLER" is_redeem_paused --market "$MARKET")
[[ "$deposit_paused" == "$redeem_paused" ]] || fail "mixed selected-market pause state"
[[ "$deposit_paused" == false || ( "$MODE" == execute && "$deposit_paused" == true && "$RESUME_PAUSED" == YES ) ]] || fail "inspect paused selected market before resuming"
ticks=$(view "$STRATEGY" get_ticks)
if [[ "$live_hash" == "$OLD_HASH" ]]; then
  expect old_width "$(width <<<"$ticks")" 200
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
    eta=$(pending_eta)
    echo "Matching $LABEL proposal already staged; ETA=$eta. No timelock reset."
    exit 0
  fi
  if [[ "$PREFLIGHT_ONLY" == true ]]; then
    echo "${LABEL} proposal preflight passed. No transactions submitted."
    exit 0
  fi
  [[ "$CONFIRM_MAINNET" == PROPOSE_STABLE40 ]] || fail "set CONFIRM_MAINNET=PROPOSE_STABLE40"
  # Cap both resource and inclusion fees for this small proposal (<=0.11 XLM).
  # An unexpectedly expensive simulation must fail rather than spend more.
  stellar contract invoke --no-cache --inclusion-fee "$INCLUSION_FEE" \
    --resource-fee 1000000 --id "$STRATEGY" --source-account "$IDENTITY" \
    --network "$NETWORK" -- propose_upgrade_wasm --admin_addr "$ADMIN" --new_wasm_hash "$NEW_HASH"
  eta=$(pending_eta)
  echo "${LABEL} 24-hour proposal confirmed; ETA=$eta. Position/policy unchanged."
  exit 0
fi

if [[ "$live_hash" == "$OLD_HASH" ]]; then
  [[ "$pending" == true ]] || fail "selected upgrade not staged"
  eta=$(pending_eta)
  (( $(date +%s) >= eta + 30 )) || fail "timelock not mature; required=$((eta + 30))"
  view "$STRATEGY" upgrade_wasm --admin_addr "$ADMIN" --new_wasm_hash "$NEW_HASH" >/dev/null
elif [[ "$pending" == true ]]; then
  fail "unexpected pending proposal on already upgraded strategy"
fi
if [[ "$PREFLIGHT_ONLY" == true ]]; then
  echo "${LABEL} execution preflight passed. No transactions submitted."
  exit 0
fi
[[ "$CONFIRM_MAINNET" == MIGRATE_STABLE40 ]] || fail "set CONFIRM_MAINNET=MIGRATE_STABLE40"
pause_market true
expect deposit_pause "$(view "$CONTROLLER" is_deposit_paused --market "$MARKET")" true
expect redeem_pause "$(view "$CONTROLLER" is_redeem_paused --market "$MARKET")" true
expect borrow_pause "$(view "$CONTROLLER" is_borrow_paused --market "$MARKET")" true
# Snapshot only AFTER stopping user mutations. Failures below intentionally leave
# Selected-market deposits/redemptions paused for inspection; no automatic unsafe unpause.
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
echo "${LABEL} migration verified: 80-tick total width, 20-tick margin, one-hour cooldown. Other strategies, including XLM, untouched."
