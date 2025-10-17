#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)

echo "Building all contract WASMs (wasm32v1-none release)..."

pushd "$ROOT_DIR" >/dev/null

for crate in receipt-vault simple-peridottroller jump-rate-model peridot-token; do
  echo "→ $crate"
  stellar contract build --package "$crate"
done

echo "Artifacts:"
ls -lh target/wasm32v1-none/release/*.wasm || true

popd >/dev/null
echo "Done."


