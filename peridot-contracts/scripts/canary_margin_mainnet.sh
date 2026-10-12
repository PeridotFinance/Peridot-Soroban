#!/usr/bin/env bash
# Run a tiny real Mainnet Margin V3 long lifecycle after activation.
# Default mode is read-only. Live mode moves 0.1 USDC pTokens from the
# peridot-mainnet account into margin, opens at 2x, and closes the position.
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

NETWORK=${NETWORK:-mainnet-gateway}
IDENTITY=${IDENTITY:-peridot-mainnet}
PREFLIGHT_ONLY=${PREFLIGHT_ONLY:-true}
CONFIRM_MAINNET=${CONFIRM_MAINNET:-}
INCLUSION_FEE=${INCLUSION_FEE:-100000}
CANARY_PTOKENS=${CANARY_PTOKENS:-1000000} # 0.1 pUSDC

USER=GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL
PERIDOTTROLLER=CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX
MARGIN_ID=CC2MVMWNUSEBJ5ITIKTKTLL3URUPYPGRQB6SWX2HAKEKKXDMYF4EYP5C
SWAP_ID=CCEMGF23WSRRR3C343LQ7SVQWGCNEQ2OA7I5VD7FY7FD4HJAYNQFV7BW
XLM=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
USDC=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75
XLM_VAULT=CBU4Y7CJFOUZZE3QBOXTKM54UTUYW3SDJWTNMDGJBNCR5HS5UCEKV3BE
USDC_VAULT=CBVUJJIJTRJNOORPPCVH72DP7YDCOMDHI6WYKP3WOFVEPSCVP3TBXHIN
POOL_ID=24f9c991c44acf33fff5f44031c40385d235dc212d7379e824ba3db1c35371f3
POOL=CBBMQBNHB2FYVZYV7VNHOJHUMTFJLR4PUMRVQYNW6RHIKZO2NQMIBUCV
POOL_TOKENS='["CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA","CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"]'

POSITION_ID=
STAGE=preflight

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

require_uint() {
  [[ "$2" =~ ^[0-9]+$ ]] || fail "$1 is not an unsigned integer: $2"
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

pool_min_out() {
  node -e '
    const [quote, slippage] = process.argv.slice(1).map(BigInt);
    console.log((quote * (1000000n - slippage) / 1000000n).toString());
  ' "$1" "$2"
}

report_failure() {
  local status=$?
  echo "Canary stopped during stage: $STAGE" >&2
  if [[ -n "$POSITION_ID" ]]; then
    echo "Position ID requiring inspection/recovery: $POSITION_ID" >&2
  elif [[ "$STAGE" != preflight && "$STAGE" != transfer_margin ]]; then
    echo "The canary pTokens may still be in the user's free margin balance." >&2
  fi
  exit "$status"
}
trap report_failure ERR

[[ "$PREFLIGHT_ONLY" == true || "$PREFLIGHT_ONLY" == false ]] || \
  fail "PREFLIGHT_ONLY must be true or false"
expect "operator identity" "$(stellar keys public-key "$IDENTITY")" "$USER"
expect "XLM vault binding" "$(view "$XLM_VAULT" get_margin_controller)" "$MARGIN_ID"
expect "USDC vault binding" "$(view "$USDC_VAULT" get_margin_controller)" "$MARGIN_ID"
expect "liquidation authorization" \
  "$(view "$PERIDOTTROLLER" is_margin_liquidation_ctrl --controller "$MARGIN_ID")" true
expect "controller pool permission" "$(view "$SWAP_ID" is_pool_allowed --pool "$MARGIN_ID")" true
expect "pool permission" "$(view "$SWAP_ID" is_pool_allowed --pool "$POOL")" true
expect "pool binding" \
  "$(view "$SWAP_ID" is_pool_binding_allowed --pool_id "$POOL_ID" --pool "$POOL")" true

spot_ptokens=$(unquote "$(view "$USDC_VAULT" get_ptoken_balance --user "$USER")")
require_uint "USDC pToken balance" "$spot_ptokens"
(( spot_ptokens >= CANARY_PTOKENS )) || \
  fail "need $CANARY_PTOKENS pUSDC; account has $spot_ptokens"

execution_config=$(view "$MARGIN_ID" get_perps_pair_execution_config \
  --margin_asset "$USDC" --base_asset "$XLM" --side Long)
open_slippage=$(printf '%s' "$execution_config" | jq -er '.open_slippage_scaled')
close_slippage=$(printf '%s' "$execution_config" | jq -er '.close_slippage_scaled')

margin_rate=$(unquote "$(view "$USDC_VAULT" get_exchange_rate)")
usdc_price=$(view "$PERIDOTTROLLER" get_price_usd --token "$USDC")
read -r margin_amount borrow_amount trade_amount < <(node -e '
  const ptokens = BigInt(process.argv[1]);
  const rate = BigInt(process.argv[2]);
  const [price, scale] = JSON.parse(process.argv[3]).map(BigInt);
  const margin = ptokens * rate / 1000000n;
  const value = margin * price / scale;
  const borrow = value * scale / price;
  console.log(`${margin} ${borrow} ${margin + borrow}`);
' "$CANARY_PTOKENS" "$margin_rate" "$usdc_price")

open_quote=$(unquote "$(view "$POOL" estimate_swap --in_idx 1 --out_idx 0 \
  --in_amount "$trade_amount")")
open_min=$(pool_min_out "$open_quote" "$open_slippage")
require_uint "open quote" "$open_quote"
require_uint "open minimum" "$open_min"
(( open_min > 0 )) || fail "open minimum is zero"

echo "Mainnet Margin V3 canary preflight passed."
echo "  pUSDC input:       $CANARY_PTOKENS"
echo "  margin underlying: $margin_amount"
echo "  estimated borrow:  $borrow_amount"
echo "  open pool quote:   $open_quote"
echo "  open min output:   $open_min"

if [[ "$PREFLIGHT_ONLY" == true ]]; then
  echo "No transaction was submitted."
  exit 0
fi

[[ "$CONFIRM_MAINNET" == RUN_MARGIN_CANARY ]] || \
  fail "set CONFIRM_MAINNET=RUN_MARGIN_CANARY for the live canary"

STAGE=transfer_margin
invoke "$MARGIN_ID" transfer_spot_to_margin --user "$USER" --asset "$USDC" \
  --ptoken_amount "$CANARY_PTOKENS"

STAGE=begin_open
POSITION_ID=$(unquote "$(invoke "$MARGIN_ID" begin_open_position_v3 \
  --user "$USER" --margin_asset "$USDC" --base_asset "$XLM" \
  --margin_ptokens "$CANARY_PTOKENS" --leverage 2 --side Long \
  --pool_tokens "$POOL_TOKENS" --pool_id "$POOL_ID" --pool "$POOL" \
  --amount_with_slippage "$open_min")")
require_uint "position ID" "$POSITION_ID"

STAGE=swap_open
invoke "$MARGIN_ID" swap_open_position_v3 --user "$USER" --position_id "$POSITION_ID"
STAGE=activate_open
invoke "$MARGIN_ID" activate_open_position_v3 --user "$USER" --position_id "$POSITION_ID"

health=$(unquote "$(view "$MARGIN_ID" get_health_factor --position_id "$POSITION_ID")")
require_uint "health factor" "$health"
(( health > 1000000 )) || fail "new canary position is not healthy: $health"

STAGE=prepare_close
invoke "$MARGIN_ID" prepare_close_position_v3 --user "$USER" --position_id "$POSITION_ID"
pending=$(view "$MARGIN_ID" get_pending_perps_close --position_id "$POSITION_ID")
collateral_underlying=$(printf '%s' "$pending" | jq -er '.collateral_underlying')
require_uint "close collateral" "$collateral_underlying"
(( collateral_underlying > 0 )) || fail "close collateral is zero"

close_quote=$(unquote "$(view "$POOL" estimate_swap --in_idx 0 --out_idx 1 \
  --in_amount "$collateral_underlying")")
close_min=$(pool_min_out "$close_quote" "$close_slippage")
require_uint "close minimum" "$close_min"
(( close_min > 0 )) || fail "close minimum is zero"

STAGE=swap_close
invoke "$MARGIN_ID" swap_close_position_v3 --user "$USER" --position_id "$POSITION_ID" \
  --amount_with_slippage "$close_min"
STAGE=finish_close
invoke "$MARGIN_ID" finish_close_position_v3 --position_id "$POSITION_ID"

STAGE=verify_closed
expect "closed position" "$(view "$MARGIN_ID" get_position --position_id "$POSITION_ID")" null
expect "cleared margin debt" \
  "$(view "$USDC_VAULT" get_margin_borrow_balance --position_id "$POSITION_ID")" 0
trap - ERR

cat <<SUMMARY

Mainnet Margin V3 canary passed.
  Position:      $POSITION_ID
  Open health:   $health
  Open min out:  $open_min
  Close min out: $close_min
  Final debt:    0
SUMMARY
