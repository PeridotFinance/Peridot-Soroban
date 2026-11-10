#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
profile="${1:?profile}"; filter="${2:?filter}"; run="${3:?fresh output}"
[[ "$profile" == internal-policy || "$profile" == captured-network-diagnostic ]]
[[ ! -e "$run" ]] || exit 2
mkdir -p "$run";run="$(cd "$run" && pwd)"
build="$(cd "${CL_STAGE_BUILD_DIR:?isolated prepared build}" && pwd)"
config="$PWD/../docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json"
python3 scripts/cl_resource_profiles.py "$config" "$run/profile.json" --profile "$profile"
python3 scripts/cl_resource_profiles.py "$config" "$run/setup-profile.json" --profile captured-network-diagnostic
export CL_RESOURCE_PROFILE="$run/profile.json" CL_STAGE_SETUP_PROFILE="$run/setup-profile.json"
export CL_ARTIFACTS="$build/artifacts" AQUARIUS_CONCENTRATED_WASM="$PWD/target/cl-share-checkpoint/aquarius-pool.wasm"
export CL_RESOURCE_REPORT="$run/resources.jsonl" CL_CALL_PROFILE_REPORT="$run/calls.jsonl" CL_TRANSACTION_REPORT="$run/transactions.jsonl"
export CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$PWD/harness/cl-runtime29/target"
python3 - "$run" "$build" <<'PY'
import sys,hashlib,json
from pathlib import Path
out=Path(sys.argv[1]);build=Path(sys.argv[2]);manifest=json.loads((build/'build-manifest.json').read_text())
for name,pin in manifest['wasm'].items():assert hashlib.sha256((build/'artifacts'/name).read_bytes()).hexdigest()==pin['sha256']
files=list(Path('prototypes/cl-stage1-cash-first').rglob('*.rs'))+list((build/'artifacts').glob('*.wasm'))+list((build/'harness/src').glob('*.rs'))+[build/'harness/Cargo.toml',build/'harness/Cargo.lock',build/'build-manifest.json',Path('target/cl-share-checkpoint/aquarius-pool.wasm'),Path('test-support/cl_resource_limits.rs'),Path('harness/cl-runtime29/src/transaction_evidence.rs'),Path('prototypes/cl-stage1-cash-first/run.sh')]
source=out/'source';source.mkdir()
pins={}
for p in files:
 b=p.read_bytes();pins[str(p.resolve())]=hashlib.sha256(b).hexdigest()
 if p.suffix!='.wasm':
  dest=source/str(p.resolve()).removeprefix(str(Path.cwd())+'/');dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(b)
(out/'source-before.json').write_text(json.dumps(pins,indent=2)+'\n')
PY
status=0
cargo test --locked --manifest-path "$build/harness/Cargo.toml" "$filter" -- --nocapture --test-threads=1 > "$run/tests.log" 2>&1 || status=$?
printf '%s\n' "$status" > "$run/test-exit-code.txt"
python3 - "$run/source-before.json" <<'PY'
import sys,json,hashlib
from pathlib import Path
for p,h in json.loads(Path(sys.argv[1]).read_text()).items():assert hashlib.sha256(Path(p).read_bytes()).hexdigest()==h,p
PY
budget=0
bash scripts/check_budget.sh --local-report "$CL_RESOURCE_REPORT" --profile "$CL_RESOURCE_PROFILE" --transaction-evidence "$CL_TRANSACTION_REPORT" --gate invocation --summary-json "$run/gates.json" > "$run/budget.log" 2>&1 || budget=$?
printf '%s\n' "$budget" > "$run/budget-exit-code.txt"
echo "test=$status invocation_gate=$budget; full lifecycle/transaction/checkpoint remain separate"
[[ "$status" == 0 && "$budget" == 0 ]]
