#!/usr/bin/env bash
set -uo pipefail

# Simulate contract calls and print cost/budget usage.
# Required env vars:
#   MARGIN_ID   - MarginManager contract id
#   USER        - user address
# Optional env vars:
#   IDENTITY    - stellar identity (default: dev)
#   NETWORK     - stellar network (default: testnet)
#   SIDE        - Long|Short (default: Short)
#   SIZE_QUOTE  - quote amount (default: 20000000)
#   LEVERAGE    - leverage (default: 2)
#   INSTRUCTIONS - instruction limit for sim (default: 20000000)

IDENTITY=${IDENTITY:-dev}
NETWORK=${NETWORK:-testnet}
SIDE=${SIDE:-Short}
SIZE_QUOTE=${SIZE_QUOTE:-20000000}
LEVERAGE=${LEVERAGE:-2}
INSTRUCTIONS=${INSTRUCTIONS:-20000000}

if [[ -z "${MARGIN_ID:-}" || -z "${USER:-}" ]]; then
  echo "Missing env vars: MARGIN_ID and USER are required." >&2
  exit 1
fi

run_sim() {
  local label="$1"
  shift
  echo "== $label =="
  if ! stellar contract invoke \
    --id "$MARGIN_ID" \
    --source-account "$IDENTITY" \
    --network "$NETWORK" \
    --cost \
    --instructions "$INSTRUCTIONS" \
    --send no \
    --no-cache \
    -- \
    "$@"; then
    echo "RESULT: FAILED (budget or simulation error)"
  else
    echo "RESULT: OK"
  fi
  echo
}

run_sim "open_position (full)" \
  open_position --user "$USER" --side "$SIDE" --size_quote "$SIZE_QUOTE" --leverage "$LEVERAGE"

run_sim "prepare_open_position (no borrow)" \
  prepare_open_position --user "$USER" --side "$SIDE" --size_quote "$SIZE_QUOTE" --leverage "$LEVERAGE"
