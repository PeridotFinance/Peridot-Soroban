#!/usr/bin/env bash
# Cash-only deployment preparation. This checkpoint cannot sign or submit.
# Build: MODE=build ADMIN=G... bash scripts/deploy_funding_group_mainnet.sh
# Plan: FUNDING_CONFIG=public-config.json bash scripts/deploy_funding_group_mainnet.sh
set -euo pipefail
ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"
case "${MODE:-plan}" in
  build)
    : "${ADMIN:?public pinned admin required}"
    [[ "$ADMIN" =~ ^G[A-Z2-7]{55}$ ]] || { echo 'Invalid public admin' >&2; exit 2; }
    for package in simple-peridottroller receipt-vault jump-rate-model; do
      SIMPLE_PERIDOTTROLLER_INIT_ADMIN="$ADMIN" RECEIPT_VAULT_INIT_ADMIN="$ADMIN" JUMP_RATE_MODEL_INIT_ADMIN="$ADMIN" \
        stellar contract build --package "$package" --out-dir target/funding-group-cash/artifacts
    done
    ;;
  plan)
    exec python3 scripts/prepare_funding_group.py "${FUNDING_CONFIG:?public funding config required}"
    ;;
  *) echo 'Only build and plan are available: checkpoint has not cleared deployment gates.' >&2; exit 2 ;;
esac
