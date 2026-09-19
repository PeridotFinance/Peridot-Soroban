#!/usr/bin/env bash
# Final Mainnet activation gate for the predeployed MarginController V3 stack.
# Default mode is read-only. Activation also installs bounded global vault
# borrow caps; these cap direct and margin borrowing together.
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

NETWORK=${NETWORK:-mainnet-gateway}
IDENTITY=${IDENTITY:-peridot-mainnet}
PREFLIGHT_ONLY=${PREFLIGHT_ONLY:-true}
CONFIRM_MAINNET=${CONFIRM_MAINNET:-}
KEEPER_DRY_RUN_CONFIRMED=${KEEPER_DRY_RUN_CONFIRMED:-}
LIQUIDATOR_PUBLIC_KEY=${LIQUIDATOR_PUBLIC_KEY:-}
INCLUSION_FEE=${INCLUSION_FEE:-100000}

ADMIN=GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL
GOVERNANCE_ADMIN=GDM5XP4ACJPXPYP5G2UYGB6WC3G2MUBCYI2ZUCYWVB5CL2JWGCFWNWA3
PERIDOTTROLLER=CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX
MARGIN_ID=CC2MVMWNUSEBJ5ITIKTKTLL3URUPYPGRQB6SWX2HAKEKKXDMYF4EYP5C
SWAP_ID=CCEMGF23WSRRR3C343LQ7SVQWGCNEQ2OA7I5VD7FY7FD4HJAYNQFV7BW
MARGIN_WASM_HASH=1667c93ce21ef31520c4804f077cf0f7b2507323c63ba65df76a2d721984e656
SWAP_WASM_HASH=1bc1266e03cd017700eb6c41afbc0daf04a85f11cf7fcf104c49a3b7eac11062
XLM_VAULT=CBU4Y7CJFOUZZE3QBOXTKM54UTUYW3SDJWTNMDGJBNCR5HS5UCEKV3BE
USDC_VAULT=CBVUJJIJTRJNOORPPCVH72DP7YDCOMDHI6WYKP3WOFVEPSCVP3TBXHIN

# Absolute total principal caps, shared by ordinary and margin borrowing.
# 30,000 XLM and 5,000 USDC bound the initial pilot exposure.
XLM_BORROW_CAP=${XLM_BORROW_CAP:-300000000000}
USDC_BORROW_CAP=${USDC_BORROW_CAP:-50000000000}
MIN_KEEPER_XLM=${MIN_KEEPER_XLM:-10}
BORROW_CAP_KEY_XDR=AAAAEAAAAAEAAAABAAAADwAAAAlCb3Jyb3dDYXAAAAA=

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

unquote() {
  local value=$1
  value=${value#\"}
  value=${value%\"}
  printf '%s\n' "$value"
}

expect() {
  local label=$1 actual expected=$3
  actual=$(unquote "$2")
  [[ "$actual" == "$expected" ]] || fail "$label mismatch: expected=$expected actual=$actual"
}

view() {
  local id=$1
  shift
  stellar contract invoke --no-cache --id "$id" --source-account "$IDENTITY" \
    --network "$NETWORK" --send no -- "$@"
}

invoke() {
  local id=$1
  shift
  stellar contract invoke --no-cache --inclusion-fee "$INCLUSION_FEE" \
    --id "$id" --source-account "$IDENTITY" --network "$NETWORK" -- "$@"
}

contract_hash() {
  stellar contract info hash --no-cache --id "$1" --network "$NETWORK"
}

read_borrow_cap() {
  stellar contract read --no-cache --id "$1" --network "$NETWORK" --output string \
    --key-xdr "$BORROW_CAP_KEY_XDR" | awk -F, 'NR == 1 { gsub(/"/, "", $2); print $2 }'
}

keeper_balance() {
  curl -fsSL "https://horizon.stellar.org/accounts/$LIQUIDATOR_PUBLIC_KEY" | \
    jq -er '.balances[] | select(.asset_type == "native") | .balance'
}

check_cap_state() {
  local label=$1 vault=$2 target=$3 current
  current=$(read_borrow_cap "$vault")
  [[ "$current" == 0 || "$current" == "$target" ]] || \
    fail "$label has unexpected existing borrow cap $current (expected 0 or $target)"
  echo "    $label borrow cap: current=$current target=$target"
}

check_vault_exposure() {
  local label=$1 vault=$2 cap=$3 borrowed available
  borrowed=$(unquote "$(view "$vault" get_total_borrowed)")
  available=$(unquote "$(view "$vault" get_available_liquidity)")
  [[ "$borrowed" =~ ^[0-9]+$ ]] || fail "$label total borrowed is invalid: $borrowed"
  [[ "$available" =~ ^[0-9]+$ ]] || fail "$label available liquidity is invalid: $available"
  (( borrowed <= cap )) || \
    fail "$label total borrowed $borrowed exceeds proposed cap $cap"
  (( available > 0 )) || fail "$label has no available liquidity"
  echo "    $label exposure: total_borrowed=$borrowed available_liquidity=$available"
}

set_cap_if_needed() {
  local label=$1 vault=$2 target=$3 current
  current=$(read_borrow_cap "$vault")
  if [[ "$current" == "$target" ]]; then
    echo "    $label borrow cap already set"
    return
  fi
  [[ "$current" == 0 ]] || fail "$label borrow cap changed since preflight: $current"
  invoke "$vault" set_borrow_cap --cap "$target"
  expect "$label borrow cap" "$(read_borrow_cap "$vault")" "$target"
}

[[ "$PREFLIGHT_ONLY" == true || "$PREFLIGHT_ONLY" == false ]] || \
  fail "PREFLIGHT_ONLY must be true or false"
[[ "$LIQUIDATOR_PUBLIC_KEY" =~ ^G[A-Z2-7]{55}$ ]] || \
  fail "set LIQUIDATOR_PUBLIC_KEY to the dedicated funded Mainnet keeper account"
expect "operator identity" "$(stellar keys public-key "$IDENTITY")" "$ADMIN"

echo "==> Re-running the complete Mainnet margin dependency preflight"
MODE=verify VERIFY_INERT=false NETWORK="$NETWORK" IDENTITY="$IDENTITY" \
  bash scripts/deploy_margin_mainnet.sh

echo "==> Verifying deployed code, governance, and inert vault bindings"
expect "MarginController hash" "$(contract_hash "$MARGIN_ID")" "$MARGIN_WASM_HASH"
expect "SwapAdapter hash" "$(contract_hash "$SWAP_ID")" "$SWAP_WASM_HASH"
expect "MarginController governance" "$(view "$MARGIN_ID" get_admin)" "$GOVERNANCE_ADMIN"
expect "SwapAdapter governance" "$(view "$SWAP_ID" get_admin)" "$GOVERNANCE_ADMIN"
expect "Peridottroller admin" "$(view "$PERIDOTTROLLER" get_admin)" "$ADMIN"

xlm_binding=$(unquote "$(view "$XLM_VAULT" get_margin_controller)")
usdc_binding=$(unquote "$(view "$USDC_VAULT" get_margin_controller)")
[[ "$xlm_binding" == null || "$xlm_binding" == "$MARGIN_ID" ]] || \
  fail "XLM vault is wired to an unexpected controller: $xlm_binding"
[[ "$usdc_binding" == null || "$usdc_binding" == "$MARGIN_ID" ]] || \
  fail "USDC vault is wired to an unexpected controller: $usdc_binding"

check_cap_state XLM "$XLM_VAULT" "$XLM_BORROW_CAP"
check_cap_state USDC "$USDC_VAULT" "$USDC_BORROW_CAP"
check_vault_exposure XLM "$XLM_VAULT" "$XLM_BORROW_CAP"
check_vault_exposure USDC "$USDC_VAULT" "$USDC_BORROW_CAP"

echo "==> Verifying dedicated keeper funding"
balance=$(keeper_balance)
awk -v balance="$balance" -v minimum="$MIN_KEEPER_XLM" \
  'BEGIN { exit !(balance + 0 >= minimum + 0) }' || \
  fail "keeper has $balance XLM; fund it to at least $MIN_KEEPER_XLM XLM"
echo "    keeper $LIQUIDATOR_PUBLIC_KEY has $balance XLM"

if [[ "$PREFLIGHT_ONLY" == true ]]; then
  echo "==> Read-only activation preflight passed; no Mainnet transaction was submitted"
  exit 0
fi

[[ "$CONFIRM_MAINNET" == ACTIVATE_MARGIN ]] || \
  fail "set CONFIRM_MAINNET=ACTIVATE_MARGIN for the live activation"
[[ "$KEEPER_DRY_RUN_CONFIRMED" == YES ]] || \
  fail "set KEEPER_DRY_RUN_CONFIRMED=YES only after the Mainnet keeper dry run passes"

echo "==> Installing bounded pilot borrow caps"
set_cap_if_needed XLM "$XLM_VAULT" "$XLM_BORROW_CAP"
set_cap_if_needed USDC "$USDC_VAULT" "$USDC_BORROW_CAP"

echo "==> Authorizing liquidation and wiring both vault margin locks"
if [[ "$(view "$PERIDOTTROLLER" is_margin_liquidation_ctrl --controller "$MARGIN_ID")" != true ]]; then
  invoke "$PERIDOTTROLLER" set_margin_liquidation_ctrl --controller "$MARGIN_ID" --allowed true
fi
if [[ "$xlm_binding" == null ]]; then
  invoke "$XLM_VAULT" set_margin_controller --admin "$ADMIN" \
    --margin_controller "\"$MARGIN_ID\""
fi
if [[ "$usdc_binding" == null ]]; then
  invoke "$USDC_VAULT" set_margin_controller --admin "$ADMIN" \
    --margin_controller "\"$MARGIN_ID\""
fi

expect "XLM vault margin controller" "$(view "$XLM_VAULT" get_margin_controller)" "$MARGIN_ID"
expect "USDC vault margin controller" "$(view "$USDC_VAULT" get_margin_controller)" "$MARGIN_ID"
expect "liquidation controller authorization" \
  "$(view "$PERIDOTTROLLER" is_margin_liquidation_ctrl --controller "$MARGIN_ID")" true

cat <<SUMMARY

Mainnet margin pilot activated.
  MarginController: $MARGIN_ID
  SwapAdapter:      $SWAP_ID
  Governance:       $GOVERNANCE_ADMIN (2-of-3)
  XLM borrow cap:   $XLM_BORROW_CAP raw
  USDC borrow cap:  $USDC_BORROW_CAP raw
  Keeper:           $LIQUIDATOR_PUBLIC_KEY

Keep the liquidation keeper live before exposing these addresses in the frontend.
SUMMARY
