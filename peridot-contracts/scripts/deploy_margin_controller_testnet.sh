#!/usr/bin/env bash
set -euo pipefail

# Deploy MarginController + SwapAdapter on testnet
# Optional env:
#   IDENTITY (default: dev)
#   PERIDOTTROLLER (default: existing testnet SimplePeridottroller)
#   AQUARIUS_ROUTER (default: Aquarius router contract)
#   USDT_TOKEN, XLM_TOKEN (default: existing testnet tokens)
#   USDT_VAULT, XLM_VAULT (default: existing testnet receipt vaults)

IDENTITY=${IDENTITY:-dev}
NETWORK="--network testnet"

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)

WASM_SWAP="$ROOT_DIR/target/wasm32v1-none/release/swap_adapter.wasm"
WASM_MARGIN="$ROOT_DIR/target/wasm32v1-none/release/margin_controller.wasm"

ADMIN=$(stellar keys public-key "$IDENTITY")

PERIDOTTROLLER=${PERIDOTTROLLER:-CDKBJC5E44FEZVVETYU2IZZLUVKN2BUH4XOMEMKTYKM4SBSRT5ZR34V3}
AQUARIUS_ROUTER=${AQUARIUS_ROUTER:-CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK}

USDT_TOKEN=${USDT_TOKEN:-CBX3DOZH4HUR3EJS6LAKHXN6RARXKMUT33OUMVVSUW5HCXEIECD4WT75}
XLM_TOKEN=${XLM_TOKEN:-CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC}

USDT_VAULT=${USDT_VAULT:-CDM37TMZO2QQQP6CIMU7E6OIBR6IQMM46P5PCSQ5D7AX6GMEFQX7NTKL}
XLM_VAULT=${XLM_VAULT:-CCPQYPFNAGQPQTMPAEBGNPNSQJ4FAJYPX6WLYBKE5SO5ZONXANCUEYE7}

MAX_LEVERAGE=${MAX_LEVERAGE:-5}
LIQ_BONUS_SCALED=${LIQ_BONUS_SCALED:-50000}


echo "Deploying SwapAdapter..."
SWAP_ID=$(stellar contract deploy --wasm "$WASM_SWAP" --source-account "$IDENTITY" $NETWORK)
echo "SwapAdapter: $SWAP_ID"

echo "Initialize SwapAdapter..."
stellar contract invoke --id "$SWAP_ID" --source-account "$IDENTITY" $NETWORK -- \
  initialize --admin "$ADMIN" --router "$AQUARIUS_ROUTER"

echo "Deploying MarginController..."
MARGIN_ID=$(stellar contract deploy --wasm "$WASM_MARGIN" --source-account "$IDENTITY" $NETWORK)
echo "MarginController: $MARGIN_ID"

echo "Initialize MarginController..."
stellar contract invoke --id "$MARGIN_ID" --source-account "$IDENTITY" $NETWORK -- \
  initialize --admin "$ADMIN" --peridottroller "$PERIDOTTROLLER" --swap_adapter "$SWAP_ID" \
  --max_leverage "$MAX_LEVERAGE" --liquidation_bonus_scaled "$LIQ_BONUS_SCALED"

echo "Set markets..."
stellar contract invoke --id "$MARGIN_ID" --source-account "$IDENTITY" $NETWORK -- \
  set_market --admin "$ADMIN" --asset "$USDT_TOKEN" --vault "$USDT_VAULT"
stellar contract invoke --id "$MARGIN_ID" --source-account "$IDENTITY" $NETWORK -- \
  set_market --admin "$ADMIN" --asset "$XLM_TOKEN" --vault "$XLM_VAULT"

echo "Done."
echo "SwapAdapter=$SWAP_ID"
echo "MarginController=$MARGIN_ID"
