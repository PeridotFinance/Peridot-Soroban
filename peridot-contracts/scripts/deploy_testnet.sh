#!/usr/bin/env bash
set -euo pipefail

# Testnet deployment for Peridot lending components
# Prereqs:
# - stellar-cli configured with a funded identity on testnet
#   e.g. stellar keys generate --global dev --network testnet --fund
# - Build WASMs first with the target admin baked into init guards:
#     ADMIN=$(stellar keys public-key "${IDENTITY:-dev}")
#     INIT_ADMIN=$ADMIN bash scripts/build_wasm.sh

IDENTITY=${IDENTITY:-dev}
NETWORK="--network testnet"

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)

WASM_CONTROLLER="$ROOT_DIR/target/wasm32v1-none/release/simple_peridottroller.optimized.wasm"
WASM_VAULT="$ROOT_DIR/target/wasm32v1-none/release/receipt_vault.optimized.wasm"
WASM_JRM="$ROOT_DIR/target/wasm32v1-none/release/jump_rate_model.optimized.wasm"
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
  initialize --name "Mock USDT" --symbol USDT --decimals 7

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

echo "Enable static-rate borrowing mode on both vaults..."
stellar contract invoke \
  --id "$VA_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  enable_static_rates --admin "$ADMIN"
stellar contract invoke \
  --id "$VB_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  enable_static_rates --admin "$ADMIN"
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

echo "Set market CF..."
CF_A=${CF_A:-700000}
CF_B=${CF_B:-900000}
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_market_cf --market "$VA_ID" --cf_scaled "$CF_A"
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_market_cf --market "$VB_ID" --cf_scaled "$CF_B"

echo "Done. Controller=$CTRL_ID VA=$VA_ID VB=$VB_ID JRM=$JRM_ID USDT=$USDT_ID"
