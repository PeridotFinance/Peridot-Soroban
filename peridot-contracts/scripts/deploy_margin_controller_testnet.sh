#!/usr/bin/env bash
set -euo pipefail

# Deploy MarginController + SwapAdapter on testnet.
#
# Build these WASMs with the target admin baked into the init guard, for example:
#   ADMIN=$(stellar keys public-key "${IDENTITY:-dev}")
#   SWAP_ADAPTER_INIT_ADMIN=$ADMIN MARGIN_CONTROLLER_INIT_ADMIN=$ADMIN bash scripts/build_wasm.sh
#
# Optional env:
#   IDENTITY (default: dev)
#   PERIDOTTROLLER (default: existing testnet SimplePeridottroller)
#   AQUARIUS_ROUTER (default: Aquarius router contract)
#   AQUARIUS_POOL_ID, AQUARIUS_POOL (default: current XLM/mock-USDT testnet pool)
#   USDT_TOKEN, XLM_TOKEN (default: existing testnet tokens)
#   USDT_VAULT, XLM_VAULT (default: existing testnet receipt vaults)
#   WIRE_VAULTS (default: true; set false to skip set_margin_controller)

IDENTITY=${IDENTITY:-dev}
NETWORK="--network testnet"

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)

WASM_SWAP="$ROOT_DIR/target/wasm32v1-none/release/swap_adapter.optimized.wasm"
WASM_MARGIN="$ROOT_DIR/target/wasm32v1-none/release/margin_controller.optimized.wasm"

ADMIN=$(stellar keys public-key "$IDENTITY")

PERIDOTTROLLER=${PERIDOTTROLLER:-CDMXPWG55776NECXQMWNBXMEQXZUAWA2AJBCQS7SU7SA64XHMO3KB3O6}
AQUARIUS_ROUTER=${AQUARIUS_ROUTER:-CBCFTQSPDBAIZ6R6PJQKSQWKNKWH2QIV3I4J72SHWBIK3ADRRAM5A6GD}
AQUARIUS_POOL_ID=${AQUARIUS_POOL_ID:-9ac7a9cde23ac2ada11105eeaa42e43c2ea8332ca0aa8f41f58d7160274d718e}
AQUARIUS_POOL=${AQUARIUS_POOL:-CCMNSENXDBNJSY72BDIPH5CCXLLHBKZ4LXTRKDLKZN4UI2NJFQLWTLD6}

USDT_TOKEN=${USDT_TOKEN:-CDPXNHHVSLX3HFAHV7XOISM23MZH36WSXTO45RNDOBIDFZBGTSOVD4OY}
XLM_TOKEN=${XLM_TOKEN:-CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC}

USDT_VAULT=${USDT_VAULT:-CCEW6NSPCV7XUEQV75ZMII5HK5DGXK5JP2QOTGLV4UFLDBPEKRGO4Y4B}
XLM_VAULT=${XLM_VAULT:-CB32OVY4AADCHQT3DLKJYW5QVTWY5MOX7BBNZFT3SDHZ5HPSDDEA2THJ}

MAX_LEVERAGE=${MAX_LEVERAGE:-5}
WIRE_VAULTS=${WIRE_VAULTS:-true}


echo "Deploying SwapAdapter..."
SWAP_ID=$(stellar contract deploy --wasm "$WASM_SWAP" --source-account "$IDENTITY" $NETWORK)
echo "SwapAdapter: $SWAP_ID"

echo "Initialize SwapAdapter..."
stellar contract invoke --id "$SWAP_ID" --source-account "$IDENTITY" $NETWORK -- \
  initialize --admin "$ADMIN" --router "$AQUARIUS_ROUTER"

echo "Deploying MarginController..."
MARGIN_ID=$(stellar contract deploy --wasm "$WASM_MARGIN" --source-account "$IDENTITY" $NETWORK)
echo "MarginController: $MARGIN_ID"

echo "Allowlist MarginController on SwapAdapter (required before MarginController.initialize)..."
stellar contract invoke --id "$SWAP_ID" --source-account "$IDENTITY" $NETWORK -- \
  set_pool_allowed --admin "$ADMIN" --pool "$MARGIN_ID" --allowed true

echo "Initialize MarginController..."
stellar contract invoke --id "$MARGIN_ID" --source-account "$IDENTITY" $NETWORK -- \
  initialize --admin "$ADMIN" --peridottroller "$PERIDOTTROLLER" --swap_adapter "$SWAP_ID" \
  --max_leverage "$MAX_LEVERAGE"

echo "Set markets..."
stellar contract invoke --id "$MARGIN_ID" --source-account "$IDENTITY" $NETWORK -- \
  set_market --admin "$ADMIN" --asset "$USDT_TOKEN" --vault "$USDT_VAULT"
stellar contract invoke --id "$MARGIN_ID" --source-account "$IDENTITY" $NETWORK -- \
  set_market --admin "$ADMIN" --asset "$XLM_TOKEN" --vault "$XLM_VAULT"

echo "Configure Aquarius pool binding on SwapAdapter..."
stellar contract invoke --id "$SWAP_ID" --source-account "$IDENTITY" $NETWORK -- \
  set_pool_allowed --admin "$ADMIN" --pool "$AQUARIUS_POOL" --allowed true
stellar contract invoke --id "$SWAP_ID" --source-account "$IDENTITY" $NETWORK -- \
  set_pool_binding --admin "$ADMIN" --pool_id "$AQUARIUS_POOL_ID" --pool "$AQUARIUS_POOL" \
  --allowed true

if [[ "$WIRE_VAULTS" == "true" ]]; then
  echo "Wire existing vaults to MarginController..."
  stellar contract invoke --id "$USDT_VAULT" --source-account "$IDENTITY" $NETWORK -- \
    set_margin_controller --admin "$ADMIN" --margin_controller "\"$MARGIN_ID\""
  stellar contract invoke --id "$XLM_VAULT" --source-account "$IDENTITY" $NETWORK -- \
    set_margin_controller --admin "$ADMIN" --margin_controller "\"$MARGIN_ID\""
fi

echo "Done."
echo "SwapAdapter=$SWAP_ID"
echo "MarginController=$MARGIN_ID"
