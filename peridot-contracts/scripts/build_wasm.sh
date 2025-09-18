#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)

echo "Building all contract WASMs (release)..."
pushd "$ROOT_DIR" >/dev/null

RUSTFLAGS="-C target-cpu=generic" \
cargo build -p simple-peridottroller -p receipt-vault -p jump-rate-model -p peridot-token \
  --release --target wasm32-unknown-unknown

echo "Artifacts:"
ls -lh target/wasm32-unknown-unknown/release/*.wasm || true

popd >/dev/null
echo "Done."


