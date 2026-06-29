#!/usr/bin/env bash
set -uo pipefail

# Simulate current margin split-open calls and print testnet resource costs.
#
# Required:
#   MARGIN_ID - margin controller contract id
#   USER      - user address or local identity
#
# Optional:
#   IDENTITY              default: dev
#   NETWORK               default: testnet
#   SWAP_ID               when set, also simulates swap_chained
#   COLLATERAL_ASSET      default: current mock USDT token
#   BASE_ASSET            default: current native XLM SAC
#   SIDE                  default: Short
#   COLLATERAL_PTOKENS    default: 1000000
#   LEVERAGE              default: 2
#   AMOUNT_WITH_SLIPPAGE  default: 1000000
#   SWAPS_CHAIN           default: current XLM -> mock-USDT Aquarius route
#   TOKEN_IN              default: current native XLM SAC
#   SWAP_AMOUNT           default: AMOUNT_WITH_SLIPPAGE
#   POSITION_ID           when set, simulates pending-position follow-up calls
#   POSITION_PTOKENS      when set with POSITION_ID, simulates supply_open_ptokens_v2
#   ACTIVATE_POSITION     when set with POSITION_ID, simulates activate_open_position_v2
#   LEGACY_POSITION_PTOKENS when set with POSITION_ID, simulates finalize_open_ptokens_v2
#   POSITION_AMOUNT       when set with POSITION_ID, simulates legacy finalize_open_position_v2
#   MAX_REPAY_AMOUNT      when set with POSITION_ID, simulates cancel_pending_open_v2

IDENTITY=${IDENTITY:-dev}
NETWORK=${NETWORK:-testnet}

COLLATERAL_ASSET=${COLLATERAL_ASSET:-CDPXNHHVSLX3HFAHV7XOISM23MZH36WSXTO45RNDOBIDFZBGTSOVD4OY}
BASE_ASSET=${BASE_ASSET:-CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC}
SIDE=${SIDE:-Short}
COLLATERAL_PTOKENS=${COLLATERAL_PTOKENS:-1000000}
LEVERAGE=${LEVERAGE:-2}
AMOUNT_WITH_SLIPPAGE=${AMOUNT_WITH_SLIPPAGE:-1000000}
TOKEN_IN=${TOKEN_IN:-$BASE_ASSET}
SWAP_AMOUNT=${SWAP_AMOUNT:-$AMOUNT_WITH_SLIPPAGE}

DEFAULT_SWAPS_CHAIN='[[["CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC","CDPXNHHVSLX3HFAHV7XOISM23MZH36WSXTO45RNDOBIDFZBGTSOVD4OY"],"9ac7a9cde23ac2ada11105eeaa42e43c2ea8332ca0aa8f41f58d7160274d718e","CCMNSENXDBNJSY72BDIPH5CCXLLHBKZ4LXTRKDLKZN4UI2NJFQLWTLD6"]]'
SWAPS_CHAIN=${SWAPS_CHAIN:-$DEFAULT_SWAPS_CHAIN}

if [[ -z "${MARGIN_ID:-}" || -z "${USER:-}" ]]; then
  echo "Missing env vars: MARGIN_ID and USER are required." >&2
  exit 1
fi

run_margin_sim() {
  local label="$1"
  shift
  echo "== $label =="
  if ! stellar contract invoke \
    --id "$MARGIN_ID" \
    --source-account "$IDENTITY" \
    --network "$NETWORK" \
    --cost \
    --send no \
    --no-cache \
    -- \
    "$@"; then
    echo "RESULT: FAILED"
  else
    echo "RESULT: OK"
  fi
  echo
}

run_swap_sim() {
  local label="$1"
  shift
  echo "== $label =="
  if ! stellar contract invoke \
    --id "$SWAP_ID" \
    --source-account "$IDENTITY" \
    --network "$NETWORK" \
    --cost \
    --send no \
    --no-cache \
    -- \
    "$@"; then
    echo "RESULT: FAILED"
  else
    echo "RESULT: OK"
  fi
  echo
}

run_margin_sim "begin_open_position_v2" \
  begin_open_position_v2 \
  --user "$USER" \
  --collateral_asset "$COLLATERAL_ASSET" \
  --base_asset "$BASE_ASSET" \
  --collateral_ptokens "$COLLATERAL_PTOKENS" \
  --leverage "$LEVERAGE" \
  --side "$SIDE" \
  --swaps_chain "$SWAPS_CHAIN" \
  --amount_with_slippage "$AMOUNT_WITH_SLIPPAGE"

if [[ -n "${SWAP_ID:-}" ]]; then
  run_swap_sim "swap_chained" \
    swap_chained \
    --user "$USER" \
    --swaps_chain "$SWAPS_CHAIN" \
    --token_in "$TOKEN_IN" \
    --amount "$SWAP_AMOUNT" \
    --amount_with_slippage "$AMOUNT_WITH_SLIPPAGE"
fi

if [[ -n "${POSITION_ID:-}" ]]; then
  if [[ -n "${POSITION_PTOKENS:-}" ]]; then
    run_margin_sim "supply_open_ptokens_v2" \
      supply_open_ptokens_v2 \
      --user "$USER" \
      --position_id "$POSITION_ID" \
      --position_ptokens "$POSITION_PTOKENS"
  fi

  if [[ -n "${ACTIVATE_POSITION:-}" ]]; then
    run_margin_sim "activate_open_position_v2" \
      activate_open_position_v2 \
      --user "$USER" \
      --position_id "$POSITION_ID"
  fi

  if [[ -n "${LEGACY_POSITION_PTOKENS:-}" ]]; then
    run_margin_sim "finalize_open_ptokens_v2" \
      finalize_open_ptokens_v2 \
      --user "$USER" \
      --position_id "$POSITION_ID" \
      --position_ptokens "$LEGACY_POSITION_PTOKENS"
  elif [[ -n "${POSITION_AMOUNT:-}" ]]; then
    run_margin_sim "finalize_open_position_v2" \
      finalize_open_position_v2 \
      --user "$USER" \
      --position_id "$POSITION_ID" \
      --position_amount "$POSITION_AMOUNT"
  fi

  if [[ -n "${MAX_REPAY_AMOUNT:-}" ]]; then
    run_margin_sim "cancel_pending_open_v2" \
      cancel_pending_open_v2 \
      --user "$USER" \
      --position_id "$POSITION_ID" \
      --max_repay_amount "$MAX_REPAY_AMOUNT"
  fi
fi
