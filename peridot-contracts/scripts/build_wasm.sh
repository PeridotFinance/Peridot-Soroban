#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)

echo "Building all contract WASMs (wasm32v1-none release)..."

pushd "$ROOT_DIR" >/dev/null

INIT_ADMIN_VALUE=${INIT_ADMIN:-${ADMIN:-}}
if [[ -n "$INIT_ADMIN_VALUE" ]]; then
  for var in \
    RECEIPT_VAULT_INIT_ADMIN \
    SIMPLE_PERIDOTTROLLER_INIT_ADMIN \
    JUMP_RATE_MODEL_INIT_ADMIN \
    PERIDOT_TOKEN_INIT_ADMIN \
    SWAP_ADAPTER_INIT_ADMIN \
    AQUARIUS_LP_VAULT_INIT_ADMIN \
    PRICE_ROUTER_INIT_ADMIN \
    MARGIN_CONTROLLER_INIT_ADMIN \
    SMART_ACCOUNT_FACTORY_INIT_ADMIN
  do
    if [[ -z "${!var:-}" ]]; then
      export "$var=$INIT_ADMIN_VALUE"
    fi
  done
  echo "Using shared INIT_ADMIN/ADMIN for unset init-admin build guards."
fi

missing_init_admin_vars=()
for var in \
  RECEIPT_VAULT_INIT_ADMIN \
  SIMPLE_PERIDOTTROLLER_INIT_ADMIN \
  JUMP_RATE_MODEL_INIT_ADMIN \
  PERIDOT_TOKEN_INIT_ADMIN \
  SWAP_ADAPTER_INIT_ADMIN \
  AQUARIUS_LP_VAULT_INIT_ADMIN \
  PRICE_ROUTER_INIT_ADMIN \
  MARGIN_CONTROLLER_INIT_ADMIN \
  SMART_ACCOUNT_FACTORY_INIT_ADMIN
do
  if [[ -z "${!var:-}" ]]; then
    missing_init_admin_vars+=("$var")
  fi
done

if (( ${#missing_init_admin_vars[@]} > 0 )); then
  echo "ERROR: missing init-admin build guard(s): ${missing_init_admin_vars[*]}" >&2
  echo "Set INIT_ADMIN=<admin public key> to use one admin for all contracts, or set per-contract *_INIT_ADMIN variables." >&2
  exit 1
fi

for crate in receipt-vault simple-peridottroller jump-rate-model peridot-token mock-token mock-lending-vault swap-adapter aquarius-lp-vault price-router margin-controller smart-account-basic smart-account-factory; do
  echo "→ $crate"
  stellar contract build --package "$crate"
  case "$crate" in
    receipt-vault) wasm_name=receipt_vault ;;
    simple-peridottroller) wasm_name=simple_peridottroller ;;
    jump-rate-model) wasm_name=jump_rate_model ;;
    peridot-token) wasm_name=peridot_token ;;
    mock-token) wasm_name=mock_token ;;
    mock-lending-vault) wasm_name=mock_lending_vault ;;
    swap-adapter) wasm_name=swap_adapter ;;
    aquarius-lp-vault) wasm_name=aquarius_lp_vault ;;
    price-router) wasm_name=price_router ;;
    margin-controller) wasm_name=margin_controller ;;
    smart-account-basic) wasm_name=smart_account_basic ;;
    smart-account-factory) wasm_name=smart_account_factory ;;
    *) echo "unknown crate: $crate" >&2; exit 1 ;;
  esac
  wasm_path="target/wasm32v1-none/release/${wasm_name}.wasm"
  optimized_path="target/wasm32v1-none/release/${wasm_name}.optimized.wasm"
  if [[ -f "$wasm_path" ]]; then
    stellar contract optimize --wasm "$wasm_path" --wasm-out "$optimized_path"
  fi
done

echo "Artifacts:"
ls -lh target/wasm32v1-none/release/*.wasm || true

popd >/dev/null
echo "Done."
