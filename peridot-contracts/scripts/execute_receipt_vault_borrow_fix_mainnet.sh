#!/usr/bin/env bash
# Execute the timelocked ReceiptVault borrow-footprint upgrade for the existing
# XLM, USDC, and EURC markets. This script never submits a borrow: after a
# successful rollout it only simulates the exact 5,000 EURC borrow that
# previously exceeded Soroban's footprint limit.
#
# Read-only preflight (the default):
#   bash scripts/execute_receipt_vault_borrow_fix_mainnet.sh
#
# Execute after all three timelocks have matured:
#   PREFLIGHT_ONLY=false CONFIRM_MAINNET=UPGRADE \
#     bash scripts/execute_receipt_vault_borrow_fix_mainnet.sh
#
# If an earlier execution stopped after all market operations were paused,
# inspect the failure and then explicitly acknowledge the paused resume:
#   PREFLIGHT_ONLY=false CONFIRM_MAINNET=UPGRADE RESUME_PAUSED=YES \
#     bash scripts/execute_receipt_vault_borrow_fix_mainnet.sh
#
# Safety behavior:
# - Every live address, admin, underlying, strategy binding, strategy output
#   shape, staged hash, staged ETA, and local WASM hash is pinned below.
# - A mutation run starts only from all-unpaused state, unless RESUME_PAUSED=YES
#   and all nine relevant pause flags are already true.
# - Once pausing begins, any failure intentionally leaves the markets paused.
# - The original all-unpaused policy is restored only after every upgrade and
#   post-upgrade invariant succeeds.
# - No borrow transaction is submitted by this script.
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

NETWORK=${NETWORK:-mainnet-gateway}
IDENTITY=${IDENTITY:-peridot-mainnet}
INCLUSION_FEE=${INCLUSION_FEE:-100000}
PREFLIGHT_ONLY=${PREFLIGHT_ONLY:-true}
CONFIRM_MAINNET=${CONFIRM_MAINNET:-}
RESUME_PAUSED=${RESUME_PAUSED:-NO}
ETA_SAFETY_SECONDS=${ETA_SAFETY_SECONDS:-30}
BORROWER=${BORROWER:-GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL}
BORROW_AMOUNT=${BORROW_AMOUNT:-50000000000} # 5,000 EURC at Stellar's 1e7 scale

EXPECTED_ADMIN=GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL
CONTROLLER=CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX
OLD_WASM_HASH=df06cfa8174d5523bb8b067a6179af7d669bcb30fa2ce34d811b5b85788f6f00
TARGET_WASM_HASH=5f35bc16b3262feb27fb77080fc007d3461a536bd62afd3b5580688ab4b004e1
TARGET_WASM=${TARGET_WASM:-target/wasm32v1-none/release/receipt_vault.optimized.wasm}

LABELS=(XLM USDC EURC)
MARKETS=(
  CBU4Y7CJFOUZZE3QBOXTKM54UTUYW3SDJWTNMDGJBNCR5HS5UCEKV3BE
  CBVUJJIJTRJNOORPPCVH72DP7YDCOMDHI6WYKP3WOFVEPSCVP3TBXHIN
  CD3WN3PLW63HFZXE56OTRLMBV46WG54TFPGRL4RDQ43HQTTWVB4RPO3G
)
TOKENS=(
  CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
  CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75
  CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV
)
STRATEGIES=(
  CCB2AR5X3KP4WQKE7HNSUSDS7SHFMC2WPVSZ2ZXJ6DHXOKHFFKOZE6GK
  CAB4JOLSCNELJVDQKZLVGHKWJCLXFDBZZMITJAFL4GBGTHIKWO47PYFH
  CBP2R5KYAWJCOCVDTSNTEVL3O6JBTWOOH7SZOX7DX5DLGVZCAMLBDZM3
)
UPGRADE_ETAS=(1787994284 1787994289 1787994294)
MAX_UPGRADE_ETA=1787994294

# DataKey::PendingUpgradeHash and DataKey::PendingUpgradeEta encoded as ScVal.
PENDING_HASH_KEY_XDR=AAAAEAAAAAEAAAABAAAADwAAABJQZW5kaW5nVXBncmFkZUhhc2gAAA==
PENDING_ETA_KEY_XDR=AAAAEAAAAAEAAAABAAAADwAAABFQZW5kaW5nVXBncmFkZUV0YQAAAA==

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

view() {
  local contract_id=$1
  shift
  stellar contract invoke \
    --no-cache --id "$contract_id" --source-account "$IDENTITY" \
    --network "$NETWORK" --send no -- "$@"
}

invoke() {
  local contract_id=$1
  shift
  stellar contract invoke \
    --no-cache --inclusion-fee "$INCLUSION_FEE" \
    --id "$contract_id" --source-account "$IDENTITY" \
    --network "$NETWORK" -- "$@"
}

contract_hash() {
  stellar contract info hash \
    --no-cache --id "$1" --network "$NETWORK"
}

read_persistent_key() {
  stellar contract read \
    --no-cache --id "$1" --network "$NETWORK" --output json \
    --key-xdr "$2"
}

expect_value() {
  local label=$1
  local actual=$2
  local expected=$3
  if [[ "$actual" != "$expected" && "$actual" != "\"$expected\"" ]]; then
    fail "$label mismatch: expected=$expected actual=$actual"
  fi
}

get_pause_state() {
  local function_name=$1
  local market=$2
  view "$CONTROLLER" "$function_name" --market "$market"
}

verify_all_pauses() {
  local expected=$1
  local i market state
  for i in 0 1 2; do
    market=${MARKETS[$i]}
    state=$(get_pause_state is_deposit_paused "$market")
    expect_value "${LABELS[$i]} deposit pause" "$state" "$expected"
    state=$(get_pause_state is_redeem_paused "$market")
    expect_value "${LABELS[$i]} redeem pause" "$state" "$expected"
    state=$(get_pause_state is_borrow_paused "$market")
    expect_value "${LABELS[$i]} borrow pause" "$state" "$expected"
  done
}

all_pauses_equal() {
  local expected=$1
  local i market state
  for i in 0 1 2; do
    market=${MARKETS[$i]}
    state=$(get_pause_state is_deposit_paused "$market")
    [[ "$state" == "$expected" || "$state" == "\"$expected\"" ]] || return 1
    state=$(get_pause_state is_redeem_paused "$market")
    [[ "$state" == "$expected" || "$state" == "\"$expected\"" ]] || return 1
    state=$(get_pause_state is_borrow_paused "$market")
    [[ "$state" == "$expected" || "$state" == "\"$expected\"" ]] || return 1
  done
}

pause_all_market_operations() {
  local paused=$1
  local i market
  for i in 0 1 2; do
    market=${MARKETS[$i]}
    invoke "$CONTROLLER" set_pause_deposit --market "$market" --paused "$paused"
    invoke "$CONTROLLER" set_pause_redeem --market "$market" --paused "$paused"
    invoke "$CONTROLLER" set_pause_borrow --market "$market" --paused "$paused"
  done
}

snapshot_accounting() {
  local i market
  SNAPSHOT_DEPOSITED=()
  SNAPSHOT_PTOKENS=()
  SNAPSHOT_BORROWED=()
  SNAPSHOT_RESERVES=()
  SNAPSHOT_RATES=()
  for i in 0 1 2; do
    market=${MARKETS[$i]}
    SNAPSHOT_DEPOSITED[$i]=$(view "$market" get_total_deposited)
    SNAPSHOT_PTOKENS[$i]=$(view "$market" get_total_ptokens)
    SNAPSHOT_BORROWED[$i]=$(view "$market" get_total_borrowed)
    SNAPSHOT_RESERVES[$i]=$(view "$market" get_total_reserves)
    SNAPSHOT_RATES[$i]=$(view "$market" get_exchange_rate)
    echo "    ${LABELS[$i]} deposited=${SNAPSHOT_DEPOSITED[$i]} ptokens=${SNAPSHOT_PTOKENS[$i]} borrowed=${SNAPSHOT_BORROWED[$i]} reserves=${SNAPSHOT_RESERVES[$i]} exchange_rate=${SNAPSHOT_RATES[$i]}"
  done
}

verify_accounting_unchanged() {
  local i market actual
  for i in 0 1 2; do
    market=${MARKETS[$i]}
    actual=$(view "$market" get_total_deposited)
    expect_value "${LABELS[$i]} total deposited" "$actual" "${SNAPSHOT_DEPOSITED[$i]}"
    actual=$(view "$market" get_total_ptokens)
    expect_value "${LABELS[$i]} total pTokens" "$actual" "${SNAPSHOT_PTOKENS[$i]}"
    actual=$(view "$market" get_total_borrowed)
    expect_value "${LABELS[$i]} total borrowed" "$actual" "${SNAPSHOT_BORROWED[$i]}"
    actual=$(view "$market" get_total_reserves)
    expect_value "${LABELS[$i]} total reserves" "$actual" "${SNAPSHOT_RESERVES[$i]}"
    actual=$(view "$market" get_exchange_rate)
    expect_value "${LABELS[$i]} exchange rate" "$actual" "${SNAPSHOT_RATES[$i]}"
  done
}

if [[ "$PREFLIGHT_ONLY" != "true" && "$PREFLIGHT_ONLY" != "false" ]]; then
  fail "PREFLIGHT_ONLY must be true or false"
fi

if [[ ! "$ETA_SAFETY_SECONDS" =~ ^[0-9]+$ ]]; then
  fail "ETA_SAFETY_SECONDS must be a non-negative integer"
fi

if [[ ! -f "$TARGET_WASM" ]]; then
  fail "missing pinned upgrade artifact: $TARGET_WASM"
fi

echo "==> Verifying signer and pinned artifact"
SIGNER=$(stellar keys public-key "$IDENTITY")
expect_value "signer" "$SIGNER" "$EXPECTED_ADMIN"
LOCAL_HASH=$(stellar contract info hash --wasm "$TARGET_WASM")
expect_value "local optimized ReceiptVault hash" "$LOCAL_HASH" "$TARGET_WASM_HASH"
CONTROLLER_ADMIN=$(view "$CONTROLLER" get_admin)
expect_value "controller admin" "$CONTROLLER_ADMIN" "$EXPECTED_ADMIN"

echo "==> Verifying live market bindings and staged upgrades"
LIVE_HASHES=()
for i in 0 1 2; do
  label=${LABELS[$i]}
  market=${MARKETS[$i]}
  token=${TOKENS[$i]}
  strategy=${STRATEGIES[$i]}
  eta=${UPGRADE_ETAS[$i]}

  live_hash=$(contract_hash "$market")
  LIVE_HASHES[$i]=$live_hash
  if [[ "$live_hash" != "$OLD_WASM_HASH" && "$live_hash" != "$TARGET_WASM_HASH" ]]; then
    fail "$label market has an unexpected WASM hash: $live_hash"
  fi

  actual=$(view "$market" get_admin)
  expect_value "$label vault admin" "$actual" "$EXPECTED_ADMIN"
  actual=$(view "$market" get_underlying_token)
  expect_value "$label underlying token" "$actual" "$token"
  actual=$(view "$market" get_boosted_vault)
  expect_value "$label boosted vault" "$actual" "$strategy"

  quote=$(view "$strategy" get_asset_amounts_per_shares --vault_shares 0)
  if [[ "$quote" != '["0"]' && "$quote" != '[0]' ]]; then
    fail "$label strategy no longer reports an exact one-asset zero-share quote: $quote"
  fi

  if [[ "$live_hash" == "$OLD_WASM_HASH" ]]; then
    pending_hash=$(read_persistent_key "$market" "$PENDING_HASH_KEY_XDR")
    case "$pending_hash" in
      *"$TARGET_WASM_HASH"*) ;;
      *) fail "$label pending upgrade hash is not the pinned target" ;;
    esac
    pending_eta=$(read_persistent_key "$market" "$PENDING_ETA_KEY_XDR")
    case "$pending_eta" in
      *"$eta"*) ;;
      *) fail "$label pending upgrade ETA is not the pinned value $eta" ;;
    esac
  fi

  echo "    $label market=$market wasm=$live_hash eta=$eta"
done

echo "==> Verifying current pause policy"
if all_pauses_equal false; then
  PAUSE_MODE=unpaused
  echo "    all XLM/USDC/EURC deposit, redeem, and borrow flags are false"
elif all_pauses_equal true; then
  PAUSE_MODE=paused
  echo "    all XLM/USDC/EURC deposit, redeem, and borrow flags are true"
else
  fail "pause state is mixed; inspect and repair it before continuing"
fi

READY_AT=$((MAX_UPGRADE_ETA + ETA_SAFETY_SECONDS))
NOW=$(date +%s)

if [[ "$PREFLIGHT_ONLY" == "true" ]]; then
  if (( NOW < READY_AT )); then
    echo "==> Read-only preflight passed; mutation remains gated until unix $READY_AT"
  else
    echo "==> Read-only preflight passed; all timelocks plus the safety margin have elapsed"
  fi
  exit 0
fi

if [[ "$CONFIRM_MAINNET" != "UPGRADE" ]]; then
  fail "set CONFIRM_MAINNET=UPGRADE for the state-changing rollout"
fi

if (( NOW < READY_AT )); then
  fail "timelock safety gate has not elapsed: now=$NOW required=$READY_AT"
fi

if [[ "$PAUSE_MODE" == "paused" && "$RESUME_PAUSED" != "YES" ]]; then
  fail "markets are already paused; set RESUME_PAUSED=YES only after inspecting the prior attempt"
fi

echo "==> Capturing pre-upgrade accounting"
snapshot_accounting

if [[ "$PAUSE_MODE" == "unpaused" ]]; then
  echo "==> Pausing deposit, redeem, and borrow on all three markets"
  pause_all_market_operations true
else
  echo "==> Resuming from the explicitly acknowledged all-paused state"
fi

# From here onward, any failure intentionally leaves all markets paused.
verify_all_pauses true

echo "==> Executing and verifying ReceiptVault upgrades"
for i in 0 1 2; do
  label=${LABELS[$i]}
  market=${MARKETS[$i]}
  strategy=${STRATEGIES[$i]}
  live_hash=$(contract_hash "$market")
  if [[ "$live_hash" == "$OLD_WASM_HASH" ]]; then
    invoke "$market" upgrade_wasm --new_wasm_hash "$TARGET_WASM_HASH"
  elif [[ "$live_hash" == "$TARGET_WASM_HASH" ]]; then
    echo "    $label already runs the target hash; skipping upgrade_wasm"
  else
    fail "$label market changed to an unexpected WASM hash: $live_hash"
  fi

  live_hash=$(contract_hash "$market")
  expect_value "$label post-upgrade WASM hash" "$live_hash" "$TARGET_WASM_HASH"
  invoke "$market" set_boosted_asset_count \
    --admin "$EXPECTED_ADMIN" --boosted_vault "$strategy" --asset_count 1
  actual=$(view "$market" get_boosted_asset_count)
  expect_value "$label boosted asset count" "$actual" 1
done

echo "==> Verifying upgraded bindings and unchanged accounting"
for i in 0 1 2; do
  label=${LABELS[$i]}
  market=${MARKETS[$i]}
  actual=$(view "$market" get_admin)
  expect_value "$label vault admin" "$actual" "$EXPECTED_ADMIN"
  actual=$(view "$market" get_underlying_token)
  expect_value "$label underlying token" "$actual" "${TOKENS[$i]}"
  actual=$(view "$market" get_boosted_vault)
  expect_value "$label boosted vault" "$actual" "${STRATEGIES[$i]}"
done
verify_accounting_unchanged

echo "==> Restoring the original all-unpaused policy"
pause_all_market_operations false
verify_all_pauses false

echo "==> Simulating (not submitting) the exact 5,000 EURC borrow"
view "${MARKETS[2]}" borrow --user "$BORROWER" --amount "$BORROW_AMOUNT"

cat <<SUMMARY

ReceiptVault Mainnet upgrade complete.
  target hash: $TARGET_WASM_HASH
  markets: XLM, USDC, EURC
  pause policy: deposit=false redeem=false borrow=false
  boosted asset counts: 1, 1, 1
  borrow check: simulated only; no EURC borrow transaction was submitted
SUMMARY
