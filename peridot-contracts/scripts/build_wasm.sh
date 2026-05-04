#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)

echo "Building all contract WASMs (wasm32v1-none release)..."

pushd "$ROOT_DIR" >/dev/null

declare -A CRATE_TO_WASM=(
  [receipt-vault]=receipt_vault
  [simple-peridottroller]=simple_peridottroller
  [jump-rate-model]=jump_rate_model
  [peridot-token]=peridot_token
  [mock-token]=mock_token
  [mock-lending-vault]=mock_lending_vault
  [swap-adapter]=swap_adapter
  [margin-controller]=margin_controller
)

for crate in receipt-vault simple-peridottroller jump-rate-model peridot-token mock-token mock-lending-vault swap-adapter margin-controller; do
  echo "→ $crate"
  stellar contract build --package "$crate"
  wasm_name=${CRATE_TO_WASM[$crate]}
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
