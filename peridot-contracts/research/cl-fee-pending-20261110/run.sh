#!/usr/bin/env bash
# Existing SDK29 harness; existing pinned WASMs. No signing or network calls.
set -euo pipefail
cd "$(dirname "$0")/../.."
profile="${1:?internal-policy or captured-network-diagnostic}"
filter="${2:?focused filter}"
run="${3:?fresh output directory}"
[[ "$profile" == internal-policy || "$profile" == captured-network-diagnostic ]]
[[ ! -e "$run" ]] || exit 2
mkdir -p "$run"
run="$(cd "$run" && pwd)"
config="$PWD/../docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json"
python3 scripts/cl_resource_profiles.py "$config" "$run/profile.json" --profile "$profile"
python3 scripts/cl_resource_profiles.py "$config" "$run/setup-profile.json" --profile captured-network-diagnostic
export CL_ARTIFACTS="${CL_ARTIFACTS:-$PWD/target/cl-restoration-combined-build/artifacts}"
export AQUARIUS_CONCENTRATED_WASM="$PWD/target/cl-share-checkpoint/aquarius-pool.wasm"
export CL_RESOURCE_PROFILE="$run/profile.json" CL_FEE_SETUP_PROFILE="$run/setup-profile.json"
export CL_RESOURCE_REPORT="$run/resources.jsonl" CL_CALL_PROFILE_REPORT="$run/calls.jsonl"
export CL_TRANSACTION_REPORT="$run/transactions.jsonl" CL_FEE_PENDING_REPORT="$run/comparisons.jsonl"
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$PWD/harness/cl-runtime29/target"
python3 - "$run/source-before.json" <<'PY'
import os,json,hashlib,sys
from pathlib import Path
files=list(Path(os.environ['CL_ARTIFACTS']).glob('*.wasm'))+[Path(os.environ['AQUARIUS_CONCENTRATED_WASM'])]
files+=list(Path('research/cl-fee-pending-20261110').glob('*.rs'))+[Path('research/cl-fee-pending-20261110/run.sh')]
files+=[Path('contracts/cl-share-pool-facade/src/test.rs'),Path('harness/cl-runtime29/Cargo.lock'),Path('harness/cl-runtime29/Cargo.toml'),Path('harness/cl-runtime29/src/transaction_evidence.rs'),Path('test-support/cl_resource_limits.rs'),Path('scripts/cl_resource_profiles.py'),Path('scripts/check_local_budget.py')]
assert len(list(Path(os.environ['CL_ARTIFACTS']).glob('*.wasm')))==8
assert hashlib.sha256(Path(os.environ['AQUARIUS_CONCENTRATED_WASM']).read_bytes()).hexdigest()=='12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6'
out=Path(sys.argv[1])
out.write_text(json.dumps({str(p.resolve()):hashlib.sha256(p.read_bytes()).hexdigest() for p in files},indent=2)+'\n')
source=out.parent/'source';source.mkdir()
for p in files:
 if p.suffix!='.wasm':
  dest=source/p;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(p.read_bytes())
PY
status=0
cargo test --manifest-path harness/cl-runtime29/Cargo.toml --features fee-pending-comparison "$filter" -- --nocapture --test-threads=1 > "$run/tests.log" 2>&1 || status=$?
printf '%s\n' "$status" > "$run/test-exit-code.txt"
python3 - "$run/source-before.json" <<'PY'
import sys,json,hashlib
from pathlib import Path
for path,h in json.loads(Path(sys.argv[1]).read_text()).items():
 assert hashlib.sha256(Path(path).read_bytes()).hexdigest()==h, 'changed during execution: '+path
PY
budget=0
bash scripts/check_budget.sh --local-report "$CL_RESOURCE_REPORT" --profile "$CL_RESOURCE_PROFILE" --transaction-evidence "$CL_TRANSACTION_REPORT" --gate invocation --summary-json "$run/gates.json" > "$run/budget.log" 2>&1 || budget=$?
printf '%s\n' "$budget" > "$run/budget-exit-code.txt"
echo "test=$status invocation_checker=$budget; full lifecycle and transaction gates are separate"
[[ "$status" == 0 && "$budget" == 0 ]]
