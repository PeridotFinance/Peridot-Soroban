#!/usr/bin/env bash
set -uo pipefail

# Keeper for a ReceiptVault backed by AquariusLpVault.
#
# Each cycle:
#   1. Refreshes the vault's cached oracle NAV ratio.
#   2. Recenters a configured concentrated range when needed.
#   3. Harvests rewards when the local harvest interval is due.
#   4. Refreshes the market's cached boosted-underlying value.
#
# Required:
#   VAULT_ID       AquariusLpVault contract id
#   MARKET_ID      ReceiptVault contract id attached to VAULT_ID
#
# Optional:
#   IDENTITY                    stellar CLI identity (default: dev)
#   NETWORK                     stellar CLI network (default: testnet)
#   INTERVAL_SECONDS            refresh cadence (default: 300)
#   HARVEST_INTERVAL_SECONDS    harvest cadence (default: 3600)
#   HARVEST_RETRY_SECONDS       retry after failed harvest (default: 300)
#   HARVEST_ON_START            true starts with a harvest (default: false)
#   RUN_HARVEST                 false disables harvesting (default: true)
#   RUN_REBALANCE               true enables guarded range maintenance (default: false)
#   ONCE                        true runs one cycle and exits (default: false)
#   DRY_RUN                     true simulates every call (default: false)
#
# Example:
#   VAULT_ID=C... MARKET_ID=C... IDENTITY=keeper NETWORK=mainnet-public \
#     bash scripts/run_aquarius_vault_keeper.sh

IDENTITY=${IDENTITY:-dev}
NETWORK=${NETWORK:-testnet}
INTERVAL_SECONDS=${INTERVAL_SECONDS:-300}
HARVEST_INTERVAL_SECONDS=${HARVEST_INTERVAL_SECONDS:-3600}
HARVEST_RETRY_SECONDS=${HARVEST_RETRY_SECONDS:-300}
HARVEST_ON_START=${HARVEST_ON_START:-false}
RUN_HARVEST=${RUN_HARVEST:-true}
RUN_REBALANCE=${RUN_REBALANCE:-false}
ONCE=${ONCE:-false}
DRY_RUN=${DRY_RUN:-false}

: "${VAULT_ID:?set VAULT_ID to the AquariusLpVault contract id}"
: "${MARKET_ID:?set MARKET_ID to the attached ReceiptVault contract id}"

validate_positive_integer() {
  local name=$1
  local value=$2
  case "$value" in
    ''|*[!0-9]*)
      echo "$name must be a positive integer." >&2
      exit 2
      ;;
  esac
  if ((value == 0)); then
    echo "$name must be greater than zero." >&2
    exit 2
  fi
}

validate_boolean() {
  local name=$1
  local value=$2
  if [[ "$value" != "true" && "$value" != "false" ]]; then
    echo "$name must be true or false." >&2
    exit 2
  fi
}

validate_positive_integer INTERVAL_SECONDS "$INTERVAL_SECONDS"
validate_positive_integer HARVEST_INTERVAL_SECONDS "$HARVEST_INTERVAL_SECONDS"
validate_positive_integer HARVEST_RETRY_SECONDS "$HARVEST_RETRY_SECONDS"
validate_boolean HARVEST_ON_START "$HARVEST_ON_START"
validate_boolean RUN_HARVEST "$RUN_HARVEST"
validate_boolean RUN_REBALANCE "$RUN_REBALANCE"
validate_boolean ONCE "$ONCE"
validate_boolean DRY_RUN "$DRY_RUN"

if [[ -z "${KEEPER_ADDRESS:-}" ]]; then
  if ! KEEPER_ADDRESS=$(stellar keys public-key "$IDENTITY"); then
    echo "Could not resolve public key for stellar identity: $IDENTITY" >&2
    exit 2
  fi
fi
if [[ -z "$KEEPER_ADDRESS" ]]; then
  echo "KEEPER_ADDRESS resolved to an empty value." >&2
  exit 2
fi

timestamp() {
  date -u +%Y-%m-%dT%H:%M:%SZ
}

invoke() {
  local contract_id=$1
  local method=$2
  shift 2

  local -a command=(
    stellar contract invoke
    --id "$contract_id"
    --source-account "$IDENTITY"
    --network "$NETWORK"
  )
  if [[ "$DRY_RUN" == "true" ]]; then
    command+=(--send no --cost)
  fi
  command+=(-- "$method" "$@")
  "${command[@]}"
}

read_contract() {
  local contract_id=$1
  local method=$2
  shift 2
  stellar contract invoke --id "$contract_id" --source-account "$IDENTITY" \
    --network "$NETWORK" --send no -- "$method" "$@"
}

run_step() {
  local label=$1
  shift
  echo "[$(timestamp)] $label"
  "$@"
}

stopping=false
stop_keeper() {
  stopping=true
}
trap stop_keeper INT TERM

now=$(date +%s)
if [[ "$HARVEST_ON_START" == "true" ]]; then
  next_harvest=$now
else
  next_harvest=$((now + HARVEST_INTERVAL_SECONDS))
fi

echo "[$(timestamp)] Aquarius vault keeper started"
echo "Vault: $VAULT_ID; market: $MARKET_ID; identity: $IDENTITY; network: $NETWORK; dry run: $DRY_RUN"

while [[ "$stopping" == "false" ]]; do
  cycle_failed=false

  if ! run_step "Refreshing vault NAV root" invoke "$VAULT_ID" refresh_nav_root; then
    echo "[$(timestamp)] NAV refresh failed; continuing cycle." >&2
    cycle_failed=true
  fi

  if [[ "$RUN_REBALANCE" == "true" ]]; then
    if needs_rebalance=$(read_contract "$VAULT_ID" needs_rebalance); then
      if [[ "$needs_rebalance" == "true" || "$needs_rebalance" == '"true"' ]]; then
        if ! run_step "Recentering concentrated position" \
          invoke "$VAULT_ID" rebalance --caller "$KEEPER_ADDRESS"; then
          echo "[$(timestamp)] Rebalance failed; continuing cycle." >&2
          cycle_failed=true
        fi
      fi
    else
      echo "[$(timestamp)] Rebalance check failed; continuing cycle." >&2
      cycle_failed=true
    fi
  fi

  now=$(date +%s)
  if [[ "$RUN_HARVEST" == "true" ]] && ((now >= next_harvest)); then
    if run_step "Harvesting and compounding vault rewards" \
      invoke "$VAULT_ID" harvest --caller "$KEEPER_ADDRESS"; then
      next_harvest=$((now + HARVEST_INTERVAL_SECONDS))
    else
      echo "[$(timestamp)] Harvest failed; retrying after ${HARVEST_RETRY_SECONDS}s." >&2
      next_harvest=$((now + HARVEST_RETRY_SECONDS))
      cycle_failed=true
    fi
  fi

  if ! run_step "Refreshing market boosted-underlying cache" \
    invoke "$MARKET_ID" refresh_boosted_underlying; then
    echo "[$(timestamp)] Market cache refresh failed." >&2
    cycle_failed=true
  fi

  if [[ "$ONCE" == "true" ]]; then
    if [[ "$cycle_failed" == "true" ]]; then
      exit 1
    fi
    exit 0
  fi

  if [[ "$stopping" == "true" ]]; then
    break
  fi
  sleep "$INTERVAL_SECONDS" &
  wait $!
done

echo "[$(timestamp)] Aquarius vault keeper stopped"
