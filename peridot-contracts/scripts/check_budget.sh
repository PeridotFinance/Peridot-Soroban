#!/usr/bin/env bash
set -uo pipefail

# Simulate Perps V3 calls against a deployed MarginController and print costs.
# Simulations do not mutate state, so select follow-up stages that match the
# current on-chain state of POSITION_ID.
#
# Required:
#   MARGIN_ID - margin controller contract ID
#   USER      - user address or local identity
#
# Optional common values:
#   IDENTITY                 default: dev
#   NETWORK                  default: testnet
#   CHECK_BEGIN              default: true
#   POSITION_ID              existing V3 position or pending position
#   OPEN_STAGE               execute | swap | activate | cancel
#   CLOSE_STAGE              prepare | begin | withdraw | swap | swap-short |
#                            cancel | finish
#   LIQUIDATION_STAGE        preview | atomic | begin | swap | finish
#   LIQUIDATOR               default: USER
#   ADD_POSITION_PTOKENS     simulate add_position_collateral_v3
#   REPAY_AMOUNT             simulate repay_margin_position_v3
#   RELEASE_DEBT_FREE        non-empty simulates release_debt_free_position_v3
#
# Begin-open defaults target the current testnet XLM/mock-USDT pool. Override
# every address when checking a different deployment.

IDENTITY=${IDENTITY:-dev}
NETWORK=${NETWORK:-testnet}
CHECK_BEGIN=${CHECK_BEGIN:-true}

MARGIN_ASSET=${MARGIN_ASSET:-CDPXNHHVSLX3HFAHV7XOISM23MZH36WSXTO45RNDOBIDFZBGTSOVD4OY}
BASE_ASSET=${BASE_ASSET:-CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC}
SIDE=${SIDE:-Long}
MARGIN_PTOKENS=${MARGIN_PTOKENS:-1000000}
LEVERAGE=${LEVERAGE:-2}
POOL=${POOL:-CCMNSENXDBNJSY72BDIPH5CCXLLHBKZ4LXTRKDLKZN4UI2NJFQLWTLD6}
POOL_ID=${POOL_ID:-9ac7a9cde23ac2ada11105eeaa42e43c2ea8332ca0aa8f41f58d7160274d718e}
POOL_TOKENS=${POOL_TOKENS:-'["CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC","CDPXNHHVSLX3HFAHV7XOISM23MZH36WSXTO45RNDOBIDFZBGTSOVD4OY"]'}
AMOUNT_WITH_SLIPPAGE=${AMOUNT_WITH_SLIPPAGE:-1}
LIQUIDATOR=${LIQUIDATOR:-${USER:-}}

if [[ -z "${MARGIN_ID:-}" || -z "${USER:-}" ]]; then
  echo "Missing env vars: MARGIN_ID and USER are required." >&2
  exit 1
fi

run_margin_sim() {
  local label="$1"
  shift
  echo "== $label =="
  if stellar contract invoke \
    --id "$MARGIN_ID" \
    --source-account "$IDENTITY" \
    --network "$NETWORK" \
    --cost \
    --send no \
    --no-cache \
    -- \
    "$@"; then
    echo "RESULT: OK"
  else
    echo "RESULT: FAILED"
  fi
  echo
}

if [[ "$CHECK_BEGIN" == "true" ]]; then
  run_margin_sim "begin_open_position_v3" \
    begin_open_position_v3 \
    --user "$USER" \
    --margin_asset "$MARGIN_ASSET" \
    --base_asset "$BASE_ASSET" \
    --margin_ptokens "$MARGIN_PTOKENS" \
    --leverage "$LEVERAGE" \
    --side "$SIDE" \
    --pool_tokens "$POOL_TOKENS" \
    --pool_id "$POOL_ID" \
    --pool "$POOL" \
    --amount_with_slippage "$AMOUNT_WITH_SLIPPAGE"
fi

if [[ -z "${POSITION_ID:-}" ]]; then
  exit 0
fi

case "${OPEN_STAGE:-}" in
  "") ;;
  execute) run_margin_sim "execute_open_position_v3" execute_open_position_v3 --user "$USER" --position_id "$POSITION_ID" ;;
  swap) run_margin_sim "swap_open_position_v3" swap_open_position_v3 --user "$USER" --position_id "$POSITION_ID" ;;
  activate) run_margin_sim "activate_open_position_v3" activate_open_position_v3 --user "$USER" --position_id "$POSITION_ID" ;;
  cancel) run_margin_sim "cancel_pending_open_v3" cancel_pending_open_v3 --user "$USER" --position_id "$POSITION_ID" ;;
  *) echo "Unknown OPEN_STAGE: $OPEN_STAGE" >&2; exit 2 ;;
esac

if [[ -n "${ADD_POSITION_PTOKENS:-}" ]]; then
  run_margin_sim "add_position_collateral_v3" \
    add_position_collateral_v3 \
    --user "$USER" \
    --position_id "$POSITION_ID" \
    --position_ptokens "$ADD_POSITION_PTOKENS"
fi

if [[ -n "${REPAY_AMOUNT:-}" ]]; then
  run_margin_sim "repay_margin_position_v3" \
    repay_margin_position_v3 \
    --user "$USER" \
    --position_id "$POSITION_ID" \
    --amount "$REPAY_AMOUNT"
fi

if [[ -n "${RELEASE_DEBT_FREE:-}" ]]; then
  run_margin_sim "release_debt_free_position_v3" \
    release_debt_free_position_v3 \
    --user "$USER" \
    --position_id "$POSITION_ID"
fi

case "${CLOSE_STAGE:-}" in
  "") ;;
  prepare) run_margin_sim "prepare_close_position_v3" prepare_close_position_v3 --user "$USER" --position_id "$POSITION_ID" ;;
  begin) run_margin_sim "begin_close_position_v3" begin_close_position_v3 --user "$USER" --position_id "$POSITION_ID" ;;
  withdraw) run_margin_sim "withdraw_close_position_v3" withdraw_close_position_v3 --user "$USER" --position_id "$POSITION_ID" ;;
  swap) run_margin_sim "swap_close_position_v3" swap_close_position_v3 --user "$USER" --position_id "$POSITION_ID" --amount_with_slippage "$AMOUNT_WITH_SLIPPAGE" ;;
  swap-short)
    : "${SHORT_SWAP_AMOUNT_IN:?SHORT_SWAP_AMOUNT_IN is required for CLOSE_STAGE=swap-short}"
    : "${MIN_DEBT_OUT:?MIN_DEBT_OUT is required for CLOSE_STAGE=swap-short}"
    run_margin_sim "swap_close_short_position_v3" \
      swap_close_short_position_v3 \
      --user "$USER" \
      --position_id "$POSITION_ID" \
      --swap_amount_in "$SHORT_SWAP_AMOUNT_IN" \
      --min_debt_out "$MIN_DEBT_OUT"
    ;;
  cancel) run_margin_sim "cancel_close_position_v3" cancel_close_position_v3 --user "$USER" --position_id "$POSITION_ID" ;;
  finish) run_margin_sim "finish_close_position_v3" finish_close_position_v3 --position_id "$POSITION_ID" ;;
  *) echo "Unknown CLOSE_STAGE: $CLOSE_STAGE" >&2; exit 2 ;;
esac

case "${LIQUIDATION_STAGE:-}" in
  "") ;;
  preview) run_margin_sim "preview_liquidation_v3" preview_liquidation_v3 --position_id "$POSITION_ID" ;;
  atomic) run_margin_sim "liquidate_position_v3" liquidate_position_v3 --liquidator "$LIQUIDATOR" --position_id "$POSITION_ID" ;;
  begin) run_margin_sim "begin_liquidation_v3" begin_liquidation_v3 --liquidator "$LIQUIDATOR" --position_id "$POSITION_ID" ;;
  swap) run_margin_sim "swap_liquidation_v3" swap_liquidation_v3 --liquidator "$LIQUIDATOR" --position_id "$POSITION_ID" --amount_with_slippage "$AMOUNT_WITH_SLIPPAGE" ;;
  finish) run_margin_sim "finish_liquidation_v3" finish_liquidation_v3 --liquidator "$LIQUIDATOR" --position_id "$POSITION_ID" ;;
  *) echo "Unknown LIQUIDATION_STAGE: $LIQUIDATION_STAGE" >&2; exit 2 ;;
esac
