#!/usr/bin/env bash
set -euo pipefail

# Verify deployment on testnet by reading controller + markets config
# Required env:
#   CTRL_ID   - controller contract id
#   VA_ID     - market A id (vault)
#   VB_ID     - market B id (vault)
# Optional:
#   IDENTITY  - soroban identity (defaults to myadmin)

IDENTITY=${IDENTITY:-myadmin}
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
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- get_market_cf --market "$m"
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
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- is_deposit_paused --market "$m"
  echo -n "borrow_paused="
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- is_borrow_paused --market "$m"
  echo -n "redeem_paused="
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- is_redeem_paused --market "$m"
  echo -n "liquidation_paused="
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- is_liquidation_paused --market "$m"
}

echo -n "controller_admin="
soroban contract invoke $NETWORK --id "$CTRL_ID" -- get_admin
echo -n "controller_oracle="
soroban contract invoke $NETWORK --id "$CTRL_ID" -- get_oracle || true

for M in "$VA_ID" "$VB_ID"; do
  echo "--- Market $M ---"
  echo -n "collateral_factor="; read_cf "$M"
  read_pauses "$M"
done

echo "Vault stats:"
for M in "$VA_ID" "$VB_ID"; do
  echo "--- Vault $M ---"
  echo -n "vault_admin="; soroban contract invoke $NETWORK --id "$M" -- get_admin
  echo -n "exchange_rate="; soroban contract invoke $NETWORK --id "$M" -- get_exchange_rate
  echo -n "total_deposited="; soroban contract invoke $NETWORK --id "$M" -- get_total_deposited
  echo -n "total_ptokens="; soroban contract invoke $NETWORK --id "$M" -- get_total_ptokens
  echo -n "total_borrowed="; soroban contract invoke $NETWORK --id "$M" -- get_total_borrowed
  echo -n "total_reserves="; soroban contract invoke $NETWORK --id "$M" -- get_total_reserves
  echo -n "total_admin_fees="; soroban contract invoke $NETWORK --id "$M" -- get_total_admin_fees
done

echo "Verification complete."


