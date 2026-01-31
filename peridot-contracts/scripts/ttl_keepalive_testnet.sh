#!/usr/bin/env bash
set -euo pipefail

# TTL keepalive for Soroban contracts (testnet)
# Env vars:
#   IDENTITY (default: dev)
#   NETWORK (default: --network testnet)
#   SWAP_ADAPTER_ID
#   MARGIN_CONTROLLER_ID
#   PERIDOT_TOKEN_ID
#   JUMP_RATE_MODEL_ID

IDENTITY=${IDENTITY:-dev}
NETWORK=${NETWORK:---network testnet}

if [[ -z "${SWAP_ADAPTER_ID:-}" || -z "${MARGIN_CONTROLLER_ID:-}" || -z "${PERIDOT_TOKEN_ID:-}" || -z "${JUMP_RATE_MODEL_ID:-}" ]]; then
  echo "Missing one or more required env vars:"
  echo "  SWAP_ADAPTER_ID, MARGIN_CONTROLLER_ID, PERIDOT_TOKEN_ID, JUMP_RATE_MODEL_ID"
  exit 1
fi

echo "TTL keepalive (testnet) using identity: $IDENTITY"

# SwapAdapter explicit bump
stellar contract invoke --id "$SWAP_ADAPTER_ID" --source-account "$IDENTITY" $NETWORK -- \
  bump_ttl >/dev/null

# MarginController: any read to bump TTL
stellar contract invoke --id "$MARGIN_CONTROLLER_ID" --source-account "$IDENTITY" $NETWORK -- \
  get_user_positions --user "$(stellar keys public-key "$IDENTITY")" >/dev/null

# PeridotToken: name() bumps TTL
stellar contract invoke --id "$PERIDOT_TOKEN_ID" --source-account "$IDENTITY" $NETWORK -- \
  name >/dev/null

# JumpRateModel: get_borrow_rate bumps TTL
stellar contract invoke --id "$JUMP_RATE_MODEL_ID" --source-account "$IDENTITY" $NETWORK -- \
  get_borrow_rate --cash 1 --borrows 1 --reserves 0 >/dev/null

echo "TTL keepalive completed."
