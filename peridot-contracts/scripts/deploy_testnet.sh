#!/usr/bin/env bash
set -euo pipefail

# Testnet deployment for Peridot lending components
# Prereqs:
# - soroban-cli configured with a funded identity on testnet
#   e.g. soroban config identity generate myadmin
# - Build WASMs first: bash scripts/build_wasm.sh

IDENTITY=${IDENTITY:-myadmin}
NETWORK="--network testnet"

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
ART_DIR="$ROOT_DIR/target/wasm32-unknown-unknown/release"

WASM_CONTROLLER="$ART_DIR/simple-peridottroller.wasm"
WASM_VAULT="$ART_DIR/receipt-vault.wasm"
WASM_JRM="$ART_DIR/jump-rate-model.wasm"
WASM_PERI="$ART_DIR/peridot-token.wasm"

echo "Using identity: $IDENTITY (testnet)"
ADMIN=$(soroban keys address "$IDENTITY" $NETWORK)
echo "Admin address: $ADMIN"

echo "Deploying SimplePeridottroller..."
CTRL_ID=$(soroban contract deploy $NETWORK --wasm "$WASM_CONTROLLER")
echo "Controller: $CTRL_ID"

echo "Initializing controller..."
soroban contract invoke $NETWORK --id "$CTRL_ID" -- initialize --admin "$ADMIN"

echo "Deploying JumpRateModel..."
JRM_ID=$(soroban contract deploy $NETWORK --wasm "$WASM_JRM")
echo "JRM: $JRM_ID"

echo "Configuring JRM (base=2%, mult=18%, jump=400%, kink=80%)..."
soroban contract invoke $NETWORK --id "$JRM_ID" -- \
  initialize --base 20000 --multiplier 180000 --jump 4000000 --kink 800000

echo "Deploying Peridot Token..."
PERI_ID=$(soroban contract deploy $NETWORK --wasm "$WASM_PERI")
echo "PERI: $PERI_ID"

echo "Initialize Peridot Token (admin=controller)..."
soroban contract invoke $NETWORK --id "$PERI_ID" -- \
  initialize --name Peridot --symbol P --decimals 6 --admin "$CTRL_ID"

echo "Point controller to PERI..."
soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_peridot_token --token "$PERI_ID"

echo "Deploying two ReceiptVault markets..."
VA_ID=$(soroban contract deploy $NETWORK --wasm "$WASM_VAULT")
VB_ID=$(soroban contract deploy $NETWORK --wasm "$WASM_VAULT")
echo "VA: $VA_ID"
echo "VB: $VB_ID"

# TODO: Replace TOKEN_A and TOKEN_B with real asset contract addresses on testnet
TOKEN_A=${TOKEN_A:-GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA} # placeholder
TOKEN_B=${TOKEN_B:-GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB} # placeholder

echo "Initialize vaults (0% rates, admin=$ADMIN)."
soroban contract invoke $NETWORK --id "$VA_ID" -- \
  initialize --token "$TOKEN_A" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"
soroban contract invoke $NETWORK --id "$VB_ID" -- \
  initialize --token "$TOKEN_B" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"

FLASH_FEE=${FLASH_FEE:-20000} # default 2%
echo "Configure flash loan fee (${FLASH_FEE}/1e6) on both vaults..."
soroban contract invoke $NETWORK --id "$VA_ID" -- set_flash_loan_fee --fee_scaled "$FLASH_FEE"
soroban contract invoke $NETWORK --id "$VB_ID" -- set_flash_loan_fee --fee_scaled "$FLASH_FEE"

echo "Wire controller + markets..."
soroban contract invoke $NETWORK --id "$VA_ID" -- set_peridottroller --peridottroller "$CTRL_ID"
soroban contract invoke $NETWORK --id "$VB_ID" -- set_peridottroller --peridottroller "$CTRL_ID"

soroban contract invoke $NETWORK --id "$CTRL_ID" -- add_market --market "$VA_ID"
soroban contract invoke $NETWORK --id "$CTRL_ID" -- add_market --market "$VB_ID"

echo "Set market CF and reward speeds..."
soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_market_cf --market "$VB_ID" --cf_scaled 1000000
soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_supply_speed --market "$VA_ID" --speed_per_sec 5
soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_borrow_speed --market "$VA_ID" --speed_per_sec 3

echo "Done. Controller=$CTRL_ID VA=$VA_ID VB=$VB_ID JRM=$JRM_ID PERI=$PERI_ID"

