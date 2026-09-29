#!/usr/bin/env bash
# Keep the existing Mainnet XLM/USDC/EURC ReceiptVault boosted-NAV caches fresh.
# Account-health snapshots fail closed when this cache is older than five minutes.
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

NETWORK=${NETWORK:-mainnet-gateway}
IDENTITY=${IDENTITY:-peridot-mainnet}
INTERVAL_SECONDS=${INTERVAL_SECONDS:-120}
TTL_INTERVAL_SECONDS=${TTL_INTERVAL_SECONDS:-86400}
DRY_RUN=${DRY_RUN:-true}
ONCE=${ONCE:-false}
INCLUSION_FEE=${INCLUSION_FEE:-100000}

LABELS=(XLM USDC EURC)
MARKETS=(
  CBU4Y7CJFOUZZE3QBOXTKM54UTUYW3SDJWTNMDGJBNCR5HS5UCEKV3BE
  CBVUJJIJTRJNOORPPCVH72DP7YDCOMDHI6WYKP3WOFVEPSCVP3TBXHIN
  CD3WN3PLW63HFZXE56OTRLMBV46WG54TFPGRL4RDQ43HQTTWVB4RPO3G
)

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

timestamp() {
  date -u +%Y-%m-%dT%H:%M:%SZ
}

invoke() {
  local market=$1
  shift
  local args=(
    stellar contract invoke --no-cache --id "$market"
    --source-account "$IDENTITY" --network "$NETWORK"
  )
  if [[ "$DRY_RUN" == "true" ]]; then
    args+=(--send no)
  else
    args+=(--inclusion-fee "$INCLUSION_FEE")
  fi
  "${args[@]}" -- "$@"
}

for value in "$INTERVAL_SECONDS" "$TTL_INTERVAL_SECONDS"; do
  [[ "$value" =~ ^[1-9][0-9]*$ ]] || fail "keeper intervals must be positive integers"
done
[[ "$DRY_RUN" == "true" || "$DRY_RUN" == "false" ]] || fail "DRY_RUN must be true or false"
[[ "$ONCE" == "true" || "$ONCE" == "false" ]] || fail "ONCE must be true or false"
((INTERVAL_SECONDS < 300)) || fail "INTERVAL_SECONDS must stay below the five-minute health-cache limit"

stopping=false
trap 'stopping=true' INT TERM
next_ttl=0

echo "[$(timestamp)] boosted market keeper started (network=$NETWORK identity=$IDENTITY dry_run=$DRY_RUN)"

while [[ "$stopping" == "false" ]]; do
  cycle_failed=false
  now=$(date +%s)
  run_ttl=false
  if ((now >= next_ttl)); then
    run_ttl=true
  fi

  for i in 0 1 2; do
    label=${LABELS[$i]}
    market=${MARKETS[$i]}
    if [[ "$run_ttl" == "true" ]]; then
      echo "[$(timestamp)] $label: maintaining global TTL"
      if ! invoke "$market" bump_ttl >/dev/null; then
        echo "[$(timestamp)] $label: bump_ttl failed" >&2
        cycle_failed=true
      fi
    fi

    echo "[$(timestamp)] $label: refreshing boosted NAV cache"
    if ! invoke "$market" refresh_boosted_underlying >/dev/null; then
      echo "[$(timestamp)] $label: refresh_boosted_underlying failed" >&2
      cycle_failed=true
    fi
  done

  if [[ "$run_ttl" == "true" && "$cycle_failed" == "false" ]]; then
    next_ttl=$((now + TTL_INTERVAL_SECONDS))
  fi
  if [[ "$ONCE" == "true" ]]; then
    [[ "$cycle_failed" == "false" ]] || exit 1
    exit 0
  fi
  if [[ "$stopping" == "true" ]]; then
    break
  fi
  sleep "$INTERVAL_SECONDS" &
  wait $!
done

echo "[$(timestamp)] boosted market keeper stopped"
