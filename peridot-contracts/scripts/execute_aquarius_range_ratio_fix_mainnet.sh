#!/usr/bin/env bash
# Execute the staged Mainnet strategy-only ratio fix and migrate all three live
# positions from full range to actively managed narrow ranges. Default mode is
# read-only. Any mutation failure after pausing deliberately leaves deposits
# and redemptions paused for operator inspection.
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

ADMIN=GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL
CONTROLLER=CCZKDMAP23ZFL55RVITKSW4LAONQABGPSK2Y77RS64GPHOVDDFF5ENGC
RECEIPT_HASH=016c4baed7298f4835c49cae27857232ec9de9422bf77dcb55cc97d3664b05b6
OLD_STRATEGY_HASH=e75289c427666564a837337aa78f63efa6179eb264c0532d4293f81a67c30868
NEW_STRATEGY_HASH=6fc41aa58bbf08869e85d1b05cd9bc57a6d8ee894caa7f3551c26649993a458c
STRATEGY_WASM=${STRATEGY_WASM:-target/wasm32v1-none/release/aquarius_lp_vault.optimized.wasm}

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
# XLM/yXLM gets approximately +/-2%; the two stablecoin settlement vaults get
# approximately +/-1%. All values are raw ticks and the pool spacing is 20.
HALF_WIDTHS=(200 100 100)
MARGINS=(80 40 40)
REBALANCE_COOLDOWN=21600
REBALANCE_DIVERGENCE_BPS=100

# DataKey::PendingUpgradeHash / PendingUpgradeEta contracttype encodings.
PENDING_HASH_KEY_XDR=AAAAEAAAAAEAAAABAAAADwAAABJQZW5kaW5nVXBncmFkZUhhc2gAAA==
PENDING_ETA_KEY_XDR=AAAAEAAAAAEAAAABAAAADwAAABFQZW5kaW5nVXBncmFkZUV0YQAAAA==

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

read_key() {
  stellar contract read --no-cache --id "$1" --network "$NETWORK" \
    --output json --key-xdr "$2"
}

pending_target() {
  local id=$1 target=$2 value
  value=$(read_key "$id" "$PENDING_HASH_KEY_XDR") || return 1
  [[ "$value" == *"$target"* ]]
}

pending_eta() {
  local value
  value=$(read_key "$1" "$PENDING_ETA_KEY_XDR") || return 1
  python3 - "$value" <<'PY'
import re
import sys

values = [int(value) for value in re.findall(r'u64[^0-9]+([0-9]+)', sys.argv[1])]
if len(values) != 1:
    raise SystemExit("could not identify one pending upgrade ETA")
print(values[0])
PY
}

pause_state_matches() {
  local deposit=$1 redeem=$2 borrow=$3 market
  for market in "${MARKETS[@]}"; do
    [[ "$(view "$CONTROLLER" is_deposit_paused --market "$market")" == "$deposit" ]] || return 1
    [[ "$(view "$CONTROLLER" is_redeem_paused --market "$market")" == "$redeem" ]] || return 1
    [[ "$(view "$CONTROLLER" is_borrow_paused --market "$market")" == "$borrow" ]] || return 1
  done
}

set_market_migration_pause() {
  local paused=$1 market
  for market in "${MARKETS[@]}"; do
    invoke "$CONTROLLER" set_pause_deposit --market "$market" --paused "$paused"
    invoke "$CONTROLLER" set_pause_redeem --market "$market" --paused "$paused"
    invoke "$CONTROLLER" set_pause_borrow --market "$market" --paused true
  done
}

[[ "$PREFLIGHT_ONLY" == "true" || "$PREFLIGHT_ONLY" == "false" ]] || \
  fail "PREFLIGHT_ONLY must be true or false"
[[ "$RESUME_PAUSED" == "NO" || "$RESUME_PAUSED" == "YES" ]] || fail "RESUME_PAUSED must be NO or YES"
[[ "$ETA_SAFETY_SECONDS" =~ ^[0-9]+$ ]] || fail "ETA_SAFETY_SECONDS must be non-negative"
command -v python3 >/dev/null 2>&1 || fail "python3 is required"
[[ -f "$STRATEGY_WASM" ]] || fail "pinned strategy artifact is missing"

expect "signer" "$(stellar keys public-key "$IDENTITY")" "$ADMIN"
expect "controller admin" "$(view "$CONTROLLER" get_admin)" "$ADMIN"
expect "strategy artifact" \
  "$(stellar contract info hash --wasm "$STRATEGY_WASM")" "$NEW_STRATEGY_HASH"

echo "==> Verifying binaries, bindings, supply-only policy, and strategy timelocks"
MAX_ETA=0
SNAPSHOT_PTOKENS=()
SNAPSHOT_DEPOSITED=()
SNAPSHOT_STRATEGY_SHARES=()
for i in 0 1 2; do
  label=${LABELS[$i]}
  market=${MARKETS[$i]}
  strategy=${STRATEGIES[$i]}
  strategy_eta=0
  expect "$label ReceiptVault hash" "$(contract_hash "$market")" "$RECEIPT_HASH"
  strategy_hash=$(contract_hash "$strategy")
  [[ "$strategy_hash" == "$OLD_STRATEGY_HASH" || "$strategy_hash" == "$NEW_STRATEGY_HASH" ]] || \
    fail "$label strategy has unexpected hash $strategy_hash"
  expect "$label market admin" "$(view "$market" get_admin)" "$ADMIN"
  expect "$label strategy admin" "$(view "$strategy" get_admin)" "$ADMIN"
  expect "$label market strategy" "$(view "$market" get_boosted_vault)" "$strategy"
  attached=$(view "$strategy" get_receipt_vault)
  [[ "$attached" == *"$market"* ]] || fail "$label reciprocal strategy binding mismatch"
  expect "$label collateral factor" "$(view "$CONTROLLER" get_market_cf --market "$market")" 0
  expect "$label total borrowed" "$(view "$market" get_total_borrowed)" 0

  SNAPSHOT_PTOKENS[$i]=$(view "$market" get_total_ptokens)
  SNAPSHOT_DEPOSITED[$i]=$(view "$market" get_total_deposited)
  SNAPSHOT_STRATEGY_SHARES[$i]=$(view "$strategy" total_supply)

  if [[ "$strategy_hash" == "$OLD_STRATEGY_HASH" ]]; then
    ticks=$(view "$strategy" get_ticks)
    [[ "$ticks" == *"-887"* && "$ticks" == *"887"* ]] || \
      fail "$label old strategy is not in the expected full-range state: $ticks"
    pending_target "$strategy" "$NEW_STRATEGY_HASH" || fail "$label strategy target is not staged"
    strategy_eta=$(pending_eta "$strategy")
    (( strategy_eta > MAX_ETA )) && MAX_ETA=$strategy_eta
  fi
  echo "    $label strategy_hash=$strategy_hash strategy_eta=$strategy_eta"
done

# Controller emergency pauses expire automatically after 72 hours. CF=0 is the
# durable supply-only control; accept an expired borrow breaker in normal mode,
# then renew it before migration and again when restoring availability.
if pause_state_matches false false true || pause_state_matches false false false; then
  PAUSE_MODE=normal
elif pause_state_matches true true true; then
  PAUSE_MODE=paused
else
  fail "isolated-market pause state is mixed or unexpected"
fi

READY_AT=$((MAX_ETA + ETA_SAFETY_SECONDS))
NOW=$(date +%s)
if (( MAX_ETA > 0 && NOW < READY_AT )); then
  fail "upgrade timelocks are not mature: now=$NOW required=$READY_AT"
fi

# Simulate every still-pending upgrade before the first state change. This
# proves the timelock, target, auth tree, and candidate compatibility together.
for strategy in "${STRATEGIES[@]}"; do
  if [[ "$(contract_hash "$strategy")" == "$OLD_STRATEGY_HASH" ]]; then
    view "$strategy" upgrade_wasm --admin_addr "$ADMIN" --new_wasm_hash "$NEW_STRATEGY_HASH" >/dev/null
  fi
done

if [[ "$PREFLIGHT_ONLY" == "true" ]]; then
  echo "==> Read-only preflight passed; all staged strategy timelocks are mature"
  exit 0
fi
[[ "$CONFIRM_MAINNET" == "MIGRATE" ]] || fail "set CONFIRM_MAINNET=MIGRATE for the live rollout"
if [[ "$PAUSE_MODE" == "paused" && "$RESUME_PAUSED" != "YES" ]]; then
  fail "markets are already paused; inspect the prior attempt and set RESUME_PAUSED=YES to resume"
fi

if [[ "$PAUSE_MODE" == "normal" ]]; then
  echo "==> Pausing deposits and redemptions; borrowing remains paused"
  set_market_migration_pause true
fi
pause_state_matches true true true || fail "failed to establish the migration pause"

# From here onward, failure deliberately leaves all three markets paused.
echo "==> Executing the three strategy-only upgrades"
for strategy in "${STRATEGIES[@]}"; do
  if [[ "$(contract_hash "$strategy")" == "$OLD_STRATEGY_HASH" ]]; then
    invoke "$strategy" upgrade_wasm --admin_addr "$ADMIN" --new_wasm_hash "$NEW_STRATEGY_HASH"
  fi
  expect "strategy post-upgrade hash" "$(contract_hash "$strategy")" "$NEW_STRATEGY_HASH"
done

echo "==> Installing policies and migrating the live positions"
for i in 0 1 2; do
  label=${LABELS[$i]}
  market=${MARKETS[$i]}
  strategy=${STRATEGIES[$i]}
  half=${HALF_WIDTHS[$i]}
  margin=${MARGINS[$i]}

  invoke "$strategy" set_range_policy --admin_addr "$ADMIN" \
    --half_width_ticks "$half" --rebalance_margin_ticks "$margin" \
    --rebalance_cooldown "$REBALANCE_COOLDOWN" \
    --max_rebalance_divergence_bps "$REBALANCE_DIVERGENCE_BPS" --enabled true
  invoke "$strategy" refresh_nav_root >/dev/null

  needs=$(view "$strategy" needs_rebalance)
  if [[ "$needs" == "true" || "$needs" == '"true"' ]]; then
    # Exact live-pool simulation: no position is burned until the full call
    # succeeds against the upgraded code and current Mainnet state.
    view "$strategy" rebalance --caller "$ADMIN" >/dev/null
    result=$(invoke "$strategy" rebalance --caller "$ADMIN")
    expect "$label rebalance result" "$result" true
  fi

  expect "$label rebalance settled" "$(view "$strategy" needs_rebalance)" false
  ticks=$(view "$strategy" get_ticks)
  [[ "$ticks" != *"-887"* ]] || fail "$label still reports a full-range position: $ticks"
  amounts=$(view "$strategy" get_position_amounts)
  [[ "$amounts" != "null" ]] || fail "$label position amount snapshot is missing"
  invoke "$strategy" refresh_nav_root >/dev/null
  invoke "$market" refresh_boosted_underlying >/dev/null
  echo "    $label migrated to ticks=$ticks"
done

echo "==> Verifying ownership and accounting invariants"
for i in 0 1 2; do
  expect "${LABELS[$i]} pToken supply" "$(view "${MARKETS[$i]}" get_total_ptokens)" "${SNAPSHOT_PTOKENS[$i]}"
  expect "${LABELS[$i]} total deposited" "$(view "${MARKETS[$i]}" get_total_deposited)" "${SNAPSHOT_DEPOSITED[$i]}"
  expect "${LABELS[$i]} strategy shares" "$(view "${STRATEGIES[$i]}" total_supply)" "${SNAPSHOT_STRATEGY_SHARES[$i]}"
  expect "${LABELS[$i]} total borrowed" "$(view "${MARKETS[$i]}" get_total_borrowed)" 0
done

echo "==> Restoring supply-only availability"
set_market_migration_pause false
pause_state_matches false false true || fail "failed to restore deposit/redeem availability"

cat <<SUMMARY

Mainnet concentrated-range migration complete.
  XLM/yXLM:   approximately +/-2%, 80-tick edge margin
  PYUSD/USDC: approximately +/-1%, 40-tick edge margin (both settlement vaults)
  cooldown:   6 hours
  price guard: 1% against the independent oracle aliases
  collateral factors: 0
  borrowing: emergency pause renewed for 72 hours; CF=0 remains the durable supply-only control

Deploy the reviewed keeper release with RUN_REBALANCE=true only after its
dry-run `needs_rebalance` checks pass against all three upgraded strategies.
SUMMARY
