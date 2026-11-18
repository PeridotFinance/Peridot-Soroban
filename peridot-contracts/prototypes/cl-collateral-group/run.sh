#!/usr/bin/env bash
# Rerun the CL collateral GROUP measurements (18 Nov 2026). Local host execution on the archived item-S
# Mainnet capture (ledger 65527300) + read-only public RPC for missing keys only. No signing, no keys.
# Usage (from peridot-contracts/): bash prototypes/cl-collateral-group/run.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
S="$root/.scratch/cl-collateral-group"; mkdir -p "$S/state" "$S/wasm"
export SPIKE_STATE_DIR="$S/state" SPIKE_WASM_DIR="$S/wasm" CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0
# 0. archived capture (never overwrite the spike's own copy)
[[ -f "$S/state/state.json" ]] || cp "$root/.scratch/cl-collateral-spike/state/state.json" "$S/state/state.json"
for w in controller market jrm sleeve; do cp "$root/.scratch/cl-collateral-spike/wasm/$w.wasm" "$S/wasm/$w.wasm"; done
# 1. patched group controller (public init-admin constant, as in the deployed build)
python3 -I "$here/controller/apply-patches.py"
(cd "$here/controller" && CARGO_TARGET_DIR="$S/target-ctrl" SIMPLE_PERIDOTTROLLER_INIT_ADMIN=GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP \
  stellar contract build --manifest-path Cargo.toml --features group >/dev/null)
cp "$S/target-ctrl/wasm32v1-none/release/simple_peridottroller.wasm" "$S/wasm/ctrl-group.wasm"
# 2. group sleeve double
(cd "$here/sleeve" && CARGO_TARGET_DIR="$S/target-sleeve" stellar contract build --manifest-path Cargo.toml >/dev/null)
cp "$S/target-sleeve/wasm32v1-none/release/cl_collateral_group_sleeve.wasm" "$S/wasm/group-sleeve.wasm"
(cd "$S/wasm" && shasum -a 256 *.wasm)
# 3. rows (iterates with read-only fills of missing keys), model tests, cost model, summary
CARGO_TARGET_DIR="$S/target-harness" bash "$here/loop.sh"
(cd "$here/harness" && CARGO_TARGET_DIR="$S/target-harness" cargo test --release -- --nocapture --test-threads 1 reward_semantics_model no_checkpoint_views_model cost_model_vs_live | grep -E "^test |REWARD_MODEL|COST_MODEL")
python3 -I "$here/summarize.py" "$S/state" "$S/state/out" > "$S/state/out-table.md"
echo "rows: $S/state/out-table.md"
