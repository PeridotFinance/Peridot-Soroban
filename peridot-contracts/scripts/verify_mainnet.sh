#!/usr/bin/env bash
set -euo pipefail

# Verify Peridot core lending deployment on Stellar mainnet.
# Required env:
#   CTRL_ID   - SimplePeridottroller contract id
#   VA_ID     - XLM ReceiptVault id
#   VB_ID     - USDC ReceiptVault id
#   VC_ID     - EURC ReceiptVault id
# Optional:
#   IDENTITY  - Stellar identity used as source account (defaults to peridot-mainnet)
#   NETWORK_NAME - Stellar CLI network name (defaults to mainnet)

IDENTITY=${IDENTITY:-peridot-mainnet}
NETWORK_NAME=${NETWORK_NAME:-mainnet}
NETWORK="--network ${NETWORK_NAME}"

if [[ -z "${CTRL_ID:-}" || -z "${VA_ID:-}" || -z "${VB_ID:-}" || -z "${VC_ID:-}" ]]; then
  echo "Please set CTRL_ID, VA_ID, VB_ID, VC_ID environment variables." >&2
  exit 1
fi

inv() {
  stellar contract invoke \
    --source-account "$IDENTITY" \
    $NETWORK \
    "$@"
}

verify_market() {
  local label=$1
  local vault=$2
  local expected_token=$3
  local expected_cf=$4

  echo "--- $label market: $vault ---"
  echo -n "vault_admin="
  inv --id "$vault" -- get_admin

  echo -n "underlying="
  local underlying
  underlying=$(inv --id "$vault" -- get_underlying_token)
  echo "$underlying"
  if [[ "$underlying" != "\"$expected_token\"" && "$underlying" != "$expected_token" ]]; then
    echo "ERROR: $label underlying mismatch. expected=$expected_token got=$underlying" >&2
    exit 1
  fi

  echo -n "collateral_factor="
  local cf
  cf=$(inv --id "$CTRL_ID" -- get_market_cf --market "$vault")
  echo "$cf"
  if [[ "$cf" != "\"$expected_cf\"" && "$cf" != "$expected_cf" ]]; then
    echo "ERROR: $label CF mismatch. expected=$expected_cf got=$cf" >&2
    exit 1
  fi

  echo -n "deposit_paused="
  inv --id "$CTRL_ID" -- is_deposit_paused --market "$vault"
  echo -n "borrow_paused="
  inv --id "$CTRL_ID" -- is_borrow_paused --market "$vault"
  echo -n "redeem_paused="
  inv --id "$CTRL_ID" -- is_redeem_paused --market "$vault"
  echo -n "liquidation_paused="
  inv --id "$CTRL_ID" -- is_liquidation_paused --market "$vault"

  echo -n "exchange_rate="
  inv --id "$vault" -- get_exchange_rate
  echo -n "total_deposited="
  inv --id "$vault" -- get_total_deposited
  echo -n "total_ptokens="
  inv --id "$vault" -- get_total_ptokens
  echo -n "total_borrowed="
  inv --id "$vault" -- get_total_borrowed
  echo -n "total_reserves="
  inv --id "$vault" -- get_total_reserves
  echo -n "total_admin_fees="
  inv --id "$vault" -- get_total_admin_fees

  echo -n "oracle_price="
  local oracle_price
  oracle_price=$(inv --id "$CTRL_ID" -- get_price_usd --token "$expected_token")
  echo "$oracle_price"
  if [[ "$oracle_price" == "null" ]]; then
    echo "ERROR: $label oracle price unavailable for $expected_token" >&2
    exit 1
  fi
}

XLM_TOKEN=${XLM_TOKEN:-CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA}
USDC_TOKEN=${USDC_TOKEN:-CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75}
EURC_TOKEN=${EURC_TOKEN:-CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV}
CF_XLM=${CF_XLM:-700000}
CF_USDC=${CF_USDC:-900000}
CF_EURC=${CF_EURC:-900000}


echo "Identity: $IDENTITY"
echo "Network : $NETWORK_NAME"
echo "Controller: $CTRL_ID"
echo "Markets: XLM=$VA_ID USDC=$VB_ID EURC=$VC_ID"

echo -n "controller_admin="
inv --id "$CTRL_ID" -- get_admin

echo -n "controller_oracle="
inv --id "$CTRL_ID" -- get_oracle

verify_market "XLM" "$VA_ID" "$XLM_TOKEN" "$CF_XLM"
verify_market "USDC" "$VB_ID" "$USDC_TOKEN" "$CF_USDC"
verify_market "EURC" "$VC_ID" "$EURC_TOKEN" "$CF_EURC"

echo "Mainnet verification complete."
