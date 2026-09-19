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

FAIL=0
fail() { echo "  ✗ $*"; FAIL=$((FAIL+1)); }
ok()   { echo "  ✓ $*"; }

invoke_quiet() {
  stellar contract invoke "$@" 2>/dev/null
}

# --- Wiring assertions (catch ordering bugs we hit during testnet bring-up) ---
echo
echo "=== Wiring assertions ==="

for M in "$VA_ID" "$VB_ID"; do
  echo "Vault $M:"

  vault_token=$(invoke_quiet --id "$M" --source-account "$IDENTITY" $NETWORK -- get_underlying_token)
  if [[ -n "$vault_token" && "$vault_token" != "null" ]]; then
    ok "vault.get_underlying_token=$vault_token"
  else
    fail "vault.get_underlying_token missing"
  fi

  cf=$(invoke_quiet --id "$CTRL_ID" --source-account "$IDENTITY" $NETWORK -- get_market_cf --market "$M")
  if [[ "$cf" =~ ^\"[0-9]+\"$ ]]; then
    ok "controller knows market (cf=$cf)"
  else
    fail "controller doesn't know market $M (add_market not called?)"
  fi

  # Cross-check: controller's view of vault's underlying token. If wiring is
  # wrong the controller will read stale or empty data.
  ctrl_view=$(invoke_quiet --id "$CTRL_ID" --source-account "$IDENTITY" $NETWORK -- \
                get_collateral_excl_usd --user "$M" --exclude_market "$M" 2>/dev/null || echo null)
  if [[ -n "$ctrl_view" && "$ctrl_view" != "null" ]]; then
    ok "controller can read vault state (collateral_excl_usd=$ctrl_view)"
  else
    fail "controller cannot read vault state for $M — set_peridottroller missed?"
  fi
done

# Check PERI treasury balance if set
if [[ -n "${PERI_ID:-}" ]]; then
  echo "PERI treasury:"
  bal=$(invoke_quiet --id "$PERI_ID" --source-account "$IDENTITY" $NETWORK -- balance --who "$CTRL_ID" || echo 0)
  if [[ "$bal" =~ ^\"?[1-9][0-9]*\"?$ ]]; then
    ok "controller PERI balance=$bal (claims will succeed)"
  else
    fail "controller PERI balance=$bal — claims will silently no-op until topped up"
  fi
fi

# Check margin controller wiring if set
if [[ -n "${MARGIN_ID:-}" ]]; then
  echo "Margin controller wiring:"
  for M in "$VA_ID" "$VB_ID"; do
    mc=$(invoke_quiet --id "$M" --source-account "$IDENTITY" $NETWORK -- get_margin_controller || echo null)
    if [[ "$mc" == "\"$MARGIN_ID\"" ]]; then
      ok "vault $M.get_margin_controller == margin controller"
    else
      fail "vault $M.get_margin_controller=$mc, expected \"$MARGIN_ID\""
    fi
  done
  # Margin controller must be allowlisted on swap adapter
  if [[ -n "${SWAP_ID:-}" ]]; then
    allowed=$(invoke_quiet --id "$SWAP_ID" --source-account "$IDENTITY" $NETWORK -- is_pool_allowed --pool "$MARGIN_ID" || echo false)
    if [[ "$allowed" == "true" ]]; then
      ok "swap adapter has margin controller allowlisted"
    else
      fail "swap adapter does NOT allowlist margin controller — initialize will trap"
    fi
  fi
fi

if [[ "$FAIL" -gt 0 ]]; then
  echo
  echo "=== Verification FAILED ($FAIL issue(s)) ==="
  exit 1
fi

echo "Verification complete."


