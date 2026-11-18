#!/usr/bin/env bash
# Iterate local run <-> read-only fetch of missing keys until no unknown keys remain. Group harness.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
export SPIKE_STATE_DIR="${SPIKE_STATE_DIR:-$root/.scratch/cl-collateral-group/state}" SPIKE_WASM_DIR="${SPIKE_WASM_DIR:-$root/.scratch/cl-collateral-group/wasm}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/.scratch/cl-collateral-group/target-harness}" CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0
for i in $(seq 1 ${ITER:-8}); do
  (cd "$here/harness" && cargo test --release group -- --exact --nocapture --test-threads 1 > "$SPIKE_STATE_DIR/test.log" 2>&1); code=$?
  n=$(python3 -c "import json;print(len(json.load(open('$SPIKE_STATE_DIR/misses.json'))))")
  echo "iter $i exit $code unknown $n"; grep -E "panicked|setup .* failed" "$SPIKE_STATE_DIR/test.log" | head -3
  [[ "$n" == 0 ]] && exit $code
  node "$here/state-driver.cjs" fill
done
