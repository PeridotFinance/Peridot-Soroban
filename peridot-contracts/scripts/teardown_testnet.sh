#!/usr/bin/env bash
set -euo pipefail

# Testnet teardown: zero reward speeds, reset CF, and pause markets
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

echo "Using identity: $IDENTITY"
echo "Controller: $CTRL_ID"
echo "Markets: $VA_ID, $VB_ID"

pause_market() {
  local mid=$1
  echo "Pausing deposit/borrow/redeem/liquidation on $mid"
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_pause_deposit --market "$mid" --paused true
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_pause_borrow --market "$mid" --paused true
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_pause_redeem --market "$mid" --paused true
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_pause_liquidation --market "$mid" --paused true
}

zero_speeds() {
  local mid=$1
  echo "Zeroing reward speeds on $mid"
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_supply_speed --market "$mid" --speed_per_sec 0
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_borrow_speed --market "$mid" --speed_per_sec 0
}

reset_cf() {
  local mid=$1
  local cf=${2:-500000}
  echo "Resetting collateral factor on $mid to $cf (scaled 1e6)"
  soroban contract invoke $NETWORK --id "$CTRL_ID" -- set_market_cf --market "$mid" --cf_scaled "$cf"
}

for M in "$VA_ID" "$VB_ID"; do
  zero_speeds "$M"
  reset_cf "$M" 500000
  pause_market "$M"
done

echo "Teardown complete."


