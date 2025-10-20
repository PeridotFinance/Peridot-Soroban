#!/usr/bin/env bash
set -euo pipefail

# Prereqs: soroban-cli installed, local sandbox running (soroban rpc serve),
# and the WASM artifacts built via scripts/build_wasm.sh

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
ART_DIR="$ROOT_DIR/target/wasm32-unknown-unknown/release"

SOROBAN_BIN=$(command -v soroban || true)
if [[ -z "${SOROBAN_BIN}" ]]; then
  echo "soroban CLI not found in PATH" >&2
  exit 1
fi

WASM_CONTROLLER="$ART_DIR/simple-peridottroller.wasm"
WASM_VAULT="$ART_DIR/receipt-vault.wasm"
WASM_JRM="$ART_DIR/jump-rate-model.wasm"
WASM_PERI="$ART_DIR/peridot-token.wasm"

NETWORK="--network sandbox"

echo "Deploying SimplePeridottroller..."
CTRL_ID=$("$SOROBAN_BIN" contract deploy $NETWORK --wasm "$WASM_CONTROLLER")
echo "Controller: $CTRL_ID"

echo "Initializing controller..."
ADMIN=$("$SOROBAN_BIN" keys address alice)
"$SOROBAN_BIN" contract invoke $NETWORK --id "$CTRL_ID" -- \
  initialize --admin "$ADMIN"

echo "Deploying JumpRateModel..."
JRM_ID=$("$SOROBAN_BIN" contract deploy $NETWORK --wasm "$WASM_JRM")
echo "JRM: $JRM_ID"

echo "Configuring JRM (base=2%, mult=18%, jump=400%, kink=80%)..."
"$SOROBAN_BIN" contract invoke $NETWORK --id "$JRM_ID" -- \
  initialize --base 20000 --multiplier 180000 --jump 4000000 --kink 800000 --admin "$CTRL_ID"

echo "Deploying Peridot Token..."
PERI_ID=$("$SOROBAN_BIN" contract deploy $NETWORK --wasm "$WASM_PERI")
echo "PERI: $PERI_ID"

echo "Initialize Peridot Token (admin=controller)..."
"$SOROBAN_BIN" contract invoke $NETWORK --id "$PERI_ID" -- \
  initialize --name Peridot --symbol P --decimals 6 --admin "$CTRL_ID"

echo "Point controller to PERI..."
"$SOROBAN_BIN" contract invoke $NETWORK --id "$CTRL_ID" -- \
  set_peridot_token --token "$PERI_ID"

echo "Deploying two ReceiptVault markets..."
VA_ID=$("$SOROBAN_BIN" contract deploy $NETWORK --wasm "$WASM_VAULT")
VB_ID=$("$SOROBAN_BIN" contract deploy $NETWORK --wasm "$WASM_VAULT")
echo "VA: $VA_ID"
echo "VB: $VB_ID"

echo "Initialize vaults (0% rates, admin=alice). Replace token addrs accordingly."
TOKEN_A=$("$SOROBAN_BIN" keys address bob) # placeholder asset address
TOKEN_B=$("$SOROBAN_BIN" keys address carol)
"$SOROBAN_BIN" contract invoke $NETWORK --id "$VA_ID" -- \
  initialize --token "$TOKEN_A" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"
"$SOROBAN_BIN" contract invoke $NETWORK --id "$VB_ID" -- \
  initialize --token "$TOKEN_B" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADMIN"

echo "Configure flash loan fee (2%) on both vaults..."
"$SOROBAN_BIN" contract invoke $NETWORK --id "$VA_ID" -- set_flash_loan_fee --fee_scaled 20000
"$SOROBAN_BIN" contract invoke $NETWORK --id "$VB_ID" -- set_flash_loan_fee --fee_scaled 20000

echo "Wire controller + markets..."
"$SOROBAN_BIN" contract invoke $NETWORK --id "$VA_ID" -- set_peridottroller --peridottroller "$CTRL_ID"
"$SOROBAN_BIN" contract invoke $NETWORK --id "$VB_ID" -- set_peridottroller --peridottroller "$CTRL_ID"

"$SOROBAN_BIN" contract invoke $NETWORK --id "$CTRL_ID" -- add_market --market "$VA_ID"
"$SOROBAN_BIN" contract invoke $NETWORK --id "$CTRL_ID" -- add_market --market "$VB_ID"

echo "Set market CF and reward speeds..."
"$SOROBAN_BIN" contract invoke $NETWORK --id "$CTRL_ID" -- set_market_cf --market "$VB_ID" --cf_scaled 1000000
"$SOROBAN_BIN" contract invoke $NETWORK --id "$CTRL_ID" -- set_supply_speed --market "$VA_ID" --speed_per_sec 5
"$SOROBAN_BIN" contract invoke $NETWORK --id "$CTRL_ID" -- set_borrow_speed --market "$VA_ID" --speed_per_sec 3

echo "Done. Controller=$CTRL_ID VA=$VA_ID VB=$VB_ID JRM=$JRM_ID PERI=$PERI_ID"
