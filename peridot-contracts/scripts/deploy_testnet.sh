#!/usr/bin/env bash
set -euo pipefail

# Testnet deployment for Peridot lending components
# Prereqs:
# - stellar-cli configured with a funded identity on testnet
#   e.g. stellar keys generate --global dev --network testnet --fund
# - Build WASMs first: bash scripts/build_wasm.sh (produces wasm32v1-none artifacts)

IDENTITY=${IDENTITY:-dev}
NETWORK="--network testnet"

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)

WASM_CONTROLLER="$ROOT_DIR/target/wasm32v1-none/release/simple_peridottroller.optimized.wasm"
WASM_VAULT="$ROOT_DIR/target/wasm32v1-none/release/receipt_vault.optimized.wasm"
WASM_JRM="$ROOT_DIR/target/wasm32v1-none/release/jump_rate_model.optimized.wasm"
WASM_PERI="$ROOT_DIR/target/wasm32v1-none/release/peridot_token.optimized.wasm"
WASM_MOCK="$ROOT_DIR/target/wasm32v1-none/release/mock_token.optimized.wasm"

echo "Using identity: $IDENTITY (testnet)"
ADMIN=$(stellar keys public-key "$IDENTITY")
echo "Admin address: $ADMIN"

echo "Deploying SimplePeridottroller..."
CTRL_ID=$(stellar contract deploy \
  --wasm "$WASM_CONTROLLER" \
  --source-account "$IDENTITY" \
  $NETWORK)
echo "Controller: $CTRL_ID"

echo "Initializing controller..."
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  initialize --admin "$ADMIN"

echo "Deploying JumpRateModel..."
JRM_ID=$(stellar contract deploy \
  --wasm "$WASM_JRM" \
  --source-account "$IDENTITY" \
  $NETWORK)
echo "JRM: $JRM_ID"

echo "Configuring JRM (base=2%, mult=18%, jump=400%, kink=80%)..."
stellar contract invoke \
  --id "$JRM_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  initialize --base 20000 --multiplier 180000 --jump 4000000 --kink 800000 --admin "$ADMIN"

echo "Deploying Peridot Token..."
PERI_ID=$(stellar contract deploy \
  --wasm "$WASM_PERI" \
  --source-account "$IDENTITY" \
  $NETWORK)
echo "PERI: $PERI_ID"

PERI_MAX_SUPPLY=${PERI_MAX_SUPPLY:-1000000000000}
echo "Initialize Peridot Token (admin=$ADMIN, max_supply=$PERI_MAX_SUPPLY)..."
stellar contract invoke \
  --id "$PERI_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  initialize --name Peridot --symbol P --decimals 6 --admin "$ADMIN" --max_supply "$PERI_MAX_SUPPLY"

echo "Point controller to PERI..."
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_peridot_token --token "$PERI_ID"

# Seed the controller's reward treasury. Claim transfers PERI from this balance,
# so the controller does NOT need to be the PERI admin. Anyone holding PERI can
# top up the treasury via standard `transfer`. Top up periodically as emissions deplete it.
PERI_INITIAL_TREASURY=${PERI_INITIAL_TREASURY:-100000000000}
echo "Seed controller PERI treasury (admin mints $PERI_INITIAL_TREASURY directly to controller)..."
stellar contract invoke \
  --id "$PERI_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  mint --to "$CTRL_ID" --amount "$PERI_INITIAL_TREASURY"

echo "Deploying Mock USDT Token..."
USDT_ID=$(stellar contract deploy \
  --wasm "$WASM_MOCK" \
  --source-account "$IDENTITY" \
  $NETWORK)
echo "USDT: $USDT_ID"

echo "Initialize Mock USDT Token..."
stellar contract invoke \
  --id "$USDT_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  initialize --name "Mock USDT" --symbol USDT --decimals 6

echo "Deploying two ReceiptVault markets..."
VA_ID=$(stellar contract deploy \
  --wasm "$WASM_VAULT" \
  --source-account "$IDENTITY" \
  $NETWORK)
VB_ID=$(stellar contract deploy \
  --wasm "$WASM_VAULT" \
  --source-account "$IDENTITY" \
  $NETWORK)
echo "VA: $VA_ID"
echo "VB: $VB_ID"

TOKEN_A=${TOKEN_A:-$(stellar contract id asset --asset native $NETWORK)}
TOKEN_B=${TOKEN_B:-$USDT_ID}

echo "Using TOKEN_A (XLM native): $TOKEN_A"
echo "Using TOKEN_B (USDT mock): $TOKEN_B"

echo "Initialize vaults (0% rates, admin=$ADMIN)."
stellar contract invoke \
  --id "$VA_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  initialize --token_address "$TOKEN_A" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"
stellar contract invoke \
  --id "$VB_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  initialize --token_address "$TOKEN_B" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"

FLASH_FEE=${FLASH_FEE:-20000} # default 2%
echo "Configure flash loan fee (${FLASH_FEE}/1e6) on both vaults..."
stellar contract invoke \
  --id "$VA_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_flash_loan_fee --fee_scaled "$FLASH_FEE"
stellar contract invoke \
  --id "$VB_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_flash_loan_fee --fee_scaled "$FLASH_FEE"

echo "Wire controller + markets..."
# add_market must precede set_peridottroller because the vault's set_peridottroller
# smoke-tests the controller's accrue_user_market, which requires the market to be supported.
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  add_market --market "$VA_ID"
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  add_market --market "$VB_ID"

stellar contract invoke \
  --id "$VA_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_peridottroller --peridottroller "$CTRL_ID"
stellar contract invoke \
  --id "$VB_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_peridottroller --peridottroller "$CTRL_ID"

echo "Set market CF and reward speeds..."
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_market_cf --market "$VB_ID" --cf_scaled 1000000
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_supply_speed --market "$VA_ID" --speed_per_sec 5
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_borrow_speed --market "$VA_ID" --speed_per_sec 3

echo "Done. Controller=$CTRL_ID VA=$VA_ID VB=$VB_ID JRM=$JRM_ID PERI=$PERI_ID"
