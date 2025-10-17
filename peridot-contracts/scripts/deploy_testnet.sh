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

WASM_CONTROLLER="$ROOT_DIR/target/wasm32v1-none/release/simple_peridottroller.wasm"
WASM_VAULT="$ROOT_DIR/target/wasm32v1-none/release/receipt_vault.wasm"
WASM_JRM="$ROOT_DIR/target/wasm32v1-none/release/jump_rate_model.wasm"
WASM_PERI="$ROOT_DIR/target/wasm32v1-none/release/peridot_token.wasm"

echo "Using identity: $IDENTITY (testnet)"
ADMIN=$(stellar keys address "$IDENTITY" $NETWORK)
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
  initialize --base 20000 --multiplier 180000 --jump 4000000 --kink 800000

echo "Deploying Peridot Token..."
PERI_ID=$(stellar contract deploy \
  --wasm "$WASM_PERI" \
  --source-account "$IDENTITY" \
  $NETWORK)
echo "PERI: $PERI_ID"

echo "Initialize Peridot Token (admin=controller)..."
stellar contract invoke \
  --id "$PERI_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  initialize --name Peridot --symbol P --decimals 6 --admin "$CTRL_ID"

echo "Point controller to PERI..."
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  set_peridot_token --token "$PERI_ID"

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

# TODO: Replace TOKEN_A and TOKEN_B with real asset contract addresses on testnet
TOKEN_A=${TOKEN_A:-GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA} # placeholder
TOKEN_B=${TOKEN_B:-GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB} # placeholder

echo "Initialize vaults (0% rates, admin=$ADMIN)."
stellar contract invoke \
  --id "$VA_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  initialize --token "$TOKEN_A" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"
stellar contract invoke \
  --id "$VB_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  initialize --token "$TOKEN_B" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"

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

