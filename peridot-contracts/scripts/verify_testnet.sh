#!/usr/bin/env bash
set -euo pipefail

# Verify deployment on testnet by reading controller + markets config
# Required env:
#   CTRL_ID   - controller contract id
#   VA_ID     - market A id (vault)
#   VB_ID     - market B id (vault)
# Optional:
#   IDENTITY  - soroban identity (defaults to dev)

IDENTITY=${IDENTITY:-dev}
NETWORK="--network testnet"

if [[ -z "${CTRL_ID:-}" || -z "${VA_ID:-}" || -z "${VB_ID:-}" ]]; then
  echo "Please set CTRL_ID, VA_ID, VB_ID environment variables." >&2
  exit 1
fi

echo "Identity: $IDENTITY"
echo "Controller: $CTRL_ID"
echo "Markets: $VA_ID, $VB_ID"

read_cf() {
  local m=$1
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  get_market_cf --market "$m"
}

read_speed() {
  local m=$1
  echo -n "supply_speed="
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- get --fn get_supply_speed --market "$m" || true
  echo -n "borrow_speed="
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- get --fn get_borrow_speed --market "$m" || true
}

read_pauses() {
  local m=$1
  echo -n "deposit_paused="
  stellar contract invoke \
    --id "$CTRL_ID" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    is_deposit_paused --market "$m"
  echo -n "borrow_paused="
  stellar contract invoke \
    --id "$CTRL_ID" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    is_borrow_paused --market "$m"
  echo -n "redeem_paused="
  stellar contract invoke \
    --id "$CTRL_ID" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    is_redeem_paused --market "$m"
  echo -n "liquidation_paused="
  stellar contract invoke \
    --id "$CTRL_ID" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    is_liquidation_paused --market "$m"
}

echo -n "controller_admin="
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  get_admin
echo -n "controller_oracle="
stellar contract invoke \
  --id "$CTRL_ID" \
  --source-account "$IDENTITY" \
  $NETWORK \
  -- \
  get_oracle || true

for M in "$VA_ID" "$VB_ID"; do
  echo "--- Market $M ---"
  echo -n "collateral_factor="; read_cf "$M"
  read_pauses "$M"
done

echo "Vault stats:"
for M in "$VA_ID" "$VB_ID"; do
  echo "--- Vault $M ---"
  echo -n "vault_admin="
  stellar contract invoke \
    --id "$M" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    get_admin
  echo -n "exchange_rate="
  stellar contract invoke \
    --id "$M" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    get_exchange_rate
  echo -n "total_deposited="
  stellar contract invoke \
    --id "$M" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    get_total_deposited
  echo -n "total_ptokens="
  stellar contract invoke \
    --id "$M" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    get_total_ptokens
  echo -n "total_borrowed="
  stellar contract invoke \
    --id "$M" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    get_total_borrowed
  echo -n "total_reserves="
  stellar contract invoke \
    --id "$M" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    get_total_reserves
  echo -n "total_admin_fees="
  stellar contract invoke \
    --id "$M" \
    --source-account "$IDENTITY" \
    $NETWORK \
    -- \
    get_total_admin_fees
done

echo "Verification complete."


