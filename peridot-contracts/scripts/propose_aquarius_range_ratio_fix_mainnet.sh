#!/usr/bin/env bash
# Upload and stage the strategy-only Mainnet fix that balances a rebalance
# against the exact concentrated-range deposit ratio. Default mode is read-only.
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

NETWORK=${NETWORK:-mainnet-gateway}
IDENTITY=${IDENTITY:-peridot-mainnet}
INCLUSION_FEE=${INCLUSION_FEE:-100000}
PREFLIGHT_ONLY=${PREFLIGHT_ONLY:-true}
CONFIRM_MAINNET=${CONFIRM_MAINNET:-}

ADMIN=GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL
CONTROLLER=CCZKDMAP23ZFL55RVITKSW4LAONQABGPSK2Y77RS64GPHOVDDFF5ENGC
RECEIPT_HASH=016c4baed7298f4835c49cae27857232ec9de9422bf77dcb55cc97d3664b05b6
OLD_STRATEGY_HASH=e75289c427666564a837337aa78f63efa6179eb264c0532d4293f81a67c30868
NEW_STRATEGY_HASH=4e31c0f4a6b3fe1f765476600edbf8c56dd8342c1ab765acd00ae69d027ebeed
STRATEGY_WASM=${STRATEGY_WASM:-target/wasm32v1-none/release/aquarius_lp_vault.optimized.wasm}
PENDING_HASH_KEY_XDR=AAAAEAAAAAEAAAABAAAADwAAABJQZW5kaW5nVXBncmFkZUhhc2gAAA==

LABELS=(XLM PYUSD USDC)
MARKETS=(
  CBRJTPI3327YPP57KGIZIU4Z6APBUN5F6LJ2Q3MPKCISUQJLAQFFZECZ
  CBNVNCPEW2XXGBEGVMZQXBSODO5V2HMGPT5FFLVLT355SXJMYY53MLMA
  CBIOHQFWKSYTRET3LJV4LTO3ZQWRQMQ7I2SJAZ62IZCQHDST4YO3AZP7
)
STRATEGIES=(
  CB3WLG4QITFRELACDR74N63VEPICMQ35QW3DSAMF4KCFOITKOJSHH6RW
  CANCOWOI6R2FZBDLZKUL6BUZJN3VONPZSUUSWFL3KF3MPG5INAF25EKY
  CAQZ7XPUSOSBI66A4RPSNPEBI2EADBMBVUBSW6R2DYWC64QHDM3HKGIN
)

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

expect() {
  local label=$1 actual=$2 expected=$3
  if [[ "$actual" != "$expected" && "$actual" != "\"$expected\"" ]]; then
    fail "$label mismatch: expected=$expected actual=$actual"
  fi
}

view() {
  local id=$1
  shift
  stellar contract invoke --no-cache --id "$id" --source-account "$IDENTITY" \
    --network "$NETWORK" --send no -- "$@"
}

invoke() {
  local id=$1
  shift
  stellar contract invoke --no-cache --inclusion-fee "$INCLUSION_FEE" \
    --id "$id" --source-account "$IDENTITY" --network "$NETWORK" -- "$@"
}

contract_hash() {
  local attempt hash
  for attempt in 1 2 3 4 5; do
    if hash=$(stellar contract info hash --no-cache --id "$1" --network "$NETWORK"); then
      printf '%s\n' "$hash"
      return 0
    fi
    sleep 2
  done
  return 1
}

assert_pending_compatible() {
  local label=$1 id=$2 target=$3 result
  if result=$(stellar contract read --no-cache --id "$id" --network "$NETWORK" \
    --output json --key-xdr "$PENDING_HASH_KEY_XDR" 2>&1); then
    [[ "$result" == *"$target"* ]] || fail "$label has a different pending upgrade: $result"
  elif [[ "$result" != *"xdr value invalid"* &&
    "$result" != *"not found"* &&
    "$result" != *"no matching contract data entries"* ]]; then
    fail "$label pending-upgrade state is unreadable: $result"
  fi
}

[[ "$PREFLIGHT_ONLY" == "true" || "$PREFLIGHT_ONLY" == "false" ]] || \
  fail "PREFLIGHT_ONLY must be true or false"
[[ -f "$STRATEGY_WASM" ]] || fail "missing strategy artifact: $STRATEGY_WASM"

expect "signer" "$(stellar keys public-key "$IDENTITY")" "$ADMIN"
expect "controller admin" "$(view "$CONTROLLER" get_admin)" "$ADMIN"
expect "AquariusLpVault artifact" \
  "$(stellar contract info hash --wasm "$STRATEGY_WASM")" "$NEW_STRATEGY_HASH"

echo "==> Verifying upgraded markets, exact strategy binaries, and supply-only policy"
for i in 0 1 2; do
  label=${LABELS[$i]}
  market=${MARKETS[$i]}
  strategy=${STRATEGIES[$i]}
  expect "$label ReceiptVault hash" "$(contract_hash "$market")" "$RECEIPT_HASH"
  expect "$label strategy hash" "$(contract_hash "$strategy")" "$OLD_STRATEGY_HASH"
  assert_pending_compatible "$label strategy" "$strategy" "$NEW_STRATEGY_HASH"
  expect "$label market admin" "$(view "$market" get_admin)" "$ADMIN"
  expect "$label strategy admin" "$(view "$strategy" get_admin)" "$ADMIN"
  expect "$label market strategy" "$(view "$market" get_boosted_vault)" "$strategy"
  attached=$(view "$strategy" get_receipt_vault)
  [[ "$attached" == *"$market"* ]] || fail "$label reciprocal strategy binding mismatch: $attached"
  ticks=$(view "$strategy" get_ticks)
  [[ "$ticks" == *"-887"* && "$ticks" == *"887"* ]] || \
    fail "$label position is not the expected unchanged full range: $ticks"
  expect "$label collateral factor" "$(view "$CONTROLLER" get_market_cf --market "$market")" 0
  expect "$label total borrowed" "$(view "$market" get_total_borrowed)" 0
  expect "$label deposit pause" "$(view "$CONTROLLER" is_deposit_paused --market "$market")" false
  expect "$label redeem pause" "$(view "$CONTROLLER" is_redeem_paused --market "$market")" false
  echo "    $label market=$market strategy=$strategy ticks=$ticks"
done

if [[ "$PREFLIGHT_ONLY" == "true" ]]; then
  echo "==> Read-only preflight passed; set PREFLIGHT_ONLY=false CONFIRM_MAINNET=PROPOSE to stage"
  exit 0
fi
[[ "$CONFIRM_MAINNET" == "PROPOSE" ]] || fail "set CONFIRM_MAINNET=PROPOSE to submit Mainnet proposals"

echo "==> Uploading the exact strategy-only artifact"
uploaded=$(stellar contract upload --no-cache --inclusion-fee "$INCLUSION_FEE" \
  --wasm "$STRATEGY_WASM" --source-account "$IDENTITY" --network "$NETWORK")
expect "uploaded strategy hash" "$uploaded" "$NEW_STRATEGY_HASH"

echo "==> Staging three strategy-only 24-hour timelocks"
for strategy in "${STRATEGIES[@]}"; do
  invoke "$strategy" propose_upgrade_wasm --admin_addr "$ADMIN" --new_wasm_hash "$NEW_STRATEGY_HASH"
done

cat <<SUMMARY

All three strategy-only Mainnet upgrades were proposed.
  Current strategy: $OLD_STRATEGY_HASH
  Target strategy:  $NEW_STRATEGY_HASH
  ReceiptVaults:    unchanged at $RECEIPT_HASH

Do not execute before every 24-hour timelock has matured. The recovery executor
proves maturity and exact live state before it pauses or changes anything:
  bash scripts/execute_aquarius_range_ratio_fix_mainnet.sh
SUMMARY
