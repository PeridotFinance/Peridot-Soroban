#!/usr/bin/env bash
# Rebuild and re-measure the cl-leveraged prototype (local only: no network, keys or signing).
# Usage (from peridot-contracts/): bash prototypes/cl-leveraged/run.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/.scratch/cl-leveraged-target}" CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0
cd "$here"
cargo test -p cl-leveraged 2>&1 | grep -E "^test result"
for p in cl-leveraged cl-lev-mock-receipt cl-lev-mock-controller cl-lev-mock-market cl-lev-mock-oracle cl-lev-mock-swap; do
  stellar contract build --manifest-path Cargo.toml --package "$p" >/dev/null
done
w="$CARGO_TARGET_DIR/wasm32v1-none/release"
size=$(wc -c < "$w/cl_leveraged.wasm"); echo "cl_leveraged.wasm $size bytes (limit 131072)"; [ "$size" -le 131072 ]
(cd "$w" && shasum -a 256 cl_leveraged.wasm cl_lev_mock_*.wasm)
(cd harness && cargo build --release -q)
"$CARGO_TARGET_DIR/release/cl-leveraged-harness" "$w" "$root/research/cl-leveraged-20261118"
