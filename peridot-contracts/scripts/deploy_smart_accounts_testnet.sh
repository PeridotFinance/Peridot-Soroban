#!/usr/bin/env bash
set -euo pipefail

NETWORK="${NETWORK:-testnet}"
IDENTITY="${IDENTITY:-dev}"

ADMIN="$(stellar keys address "$IDENTITY")"
echo "Admin: $ADMIN"
export SMART_ACCOUNT_FACTORY_INIT_ADMIN="$ADMIN"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "Building smart-account-basic..."
(cd "$ROOT_DIR/contracts/smart-account-basic" && stellar contract build)
echo "Building smart-account-factory..."
(cd "$ROOT_DIR/contracts/smart-account-factory" && stellar contract build)

BASIC_WASM="$ROOT_DIR/target/wasm32v1-none/release/smart_account_basic.wasm"
FACTORY_WASM="$ROOT_DIR/target/wasm32v1-none/release/smart_account_factory.wasm"

echo "Deploying SmartAccountFactory..."
FACTORY_ID=$(stellar contract deploy \
  --wasm "$FACTORY_WASM" \
  --source-account "$IDENTITY" \
  --network "$NETWORK" \
  --alias peridot_smart_account_factory)
echo "Factory: $FACTORY_ID"

echo "Initializing factory..."
stellar contract invoke --id "$FACTORY_ID" --source-account "$IDENTITY" --network "$NETWORK" -- \
  initialize --admin "$ADMIN"

echo "Installing BasicSmartAccount WASM..."
BASIC_HASH=$(stellar contract install \
  --wasm "$BASIC_WASM" \
  --source-account "$IDENTITY" \
  --network "$NETWORK")
echo "Basic WASM hash: $BASIC_HASH"

echo "Setting Basic WASM hash in factory..."
stellar contract invoke --id "$FACTORY_ID" --source-account "$IDENTITY" --network "$NETWORK" -- \
  set_wasm_hash --admin "$ADMIN" --account_type Basic --hash "$BASIC_HASH"

echo
echo "Next step: create a smart account"
echo "You need a signer public key (ed25519) as BytesN<32> and a salt."
echo "Example:"
echo "stellar contract invoke --id \"$FACTORY_ID\" --source-account \"$IDENTITY\" --network \"$NETWORK\" -- \\"
echo "  create_account --config '{\"account_type\":\"Basic\",\"owner\":\"$ADMIN\",\"signer\":\"<BYTESN_32>\",\"peridottroller\":\"<CTRL_ID>\",\"margin_controller\":\"<MARGIN_ID>\"}' --salt \"<BYTESN_32>\""
