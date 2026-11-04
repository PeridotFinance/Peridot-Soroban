#!/usr/bin/env bash
# Focused local measurement only. No identity, signing, network or deployment.
set -euo pipefail
cd "$(dirname "$0")/../.."
profile="${1:?existing profile required}"
filter="${2:?specific test filter required}"
run="${3:?fresh evidence directory required}"
[[ "$profile" == internal-policy || "$profile" == captured-network-diagnostic ]]
[[ ! -e "$run" ]] || { echo "Refusing existing output: $run" >&2; exit 2; }
: "${CL_ATOMIC_BUILD_DIR:?set prepared atomic build directory}"
mkdir -p "$run"
run="$(cd "$run" && pwd)"
build="$(cd "$CL_ATOMIC_BUILD_DIR" && pwd)"
cp ../docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json "$run/network-config.json"
python3 scripts/cl_resource_profiles.py "$run/network-config.json" "$run/profile.json" --profile "$profile"
python3 scripts/cl_resource_profiles.py "$run/network-config.json" "$run/setup-profile.json" --profile captured-network-diagnostic
export CARGO_INCREMENTAL=0
export CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR="$PWD/harness/cl-runtime29/target"
export CL_ARTIFACTS="$build/artifacts"
export AQUARIUS_CONCENTRATED_WASM="$PWD/target/cl-share-checkpoint/aquarius-pool.wasm"
export CL_RESOURCE_PROFILE="$run/profile.json"
export CL_ATOMIC_SETUP_PROFILE="$run/setup-profile.json"
export CL_RESOURCE_REPORT="$run/resources.jsonl"
export CL_CALL_PROFILE_REPORT="$run/calls.jsonl"
export CL_TRANSACTION_REPORT="$run/transactions.jsonl"
: > "$CL_RESOURCE_REPORT"
: > "$CL_CALL_PROFILE_REPORT"
: > "$CL_TRANSACTION_REPORT"
python3 - "$run" "$build" "$filter" <<'PIN'
import hashlib,json,os,sys
from pathlib import Path
out=Path(sys.argv[1]);build=Path(sys.argv[2])
source=Path('prototypes/cl-atomic-qualification')
files=list(source.rglob('*.rs'))+[source/n for n in ['run.sh','prepare.py','optimization.patch']]+list((build/'artifacts').glob('*.wasm'))+list((build/'harness/src').glob('*.rs'))+[build/'build-manifest.json',Path(os.environ['AQUARIUS_CONCENTRATED_WASM'])]
files += [Path('scripts')/n for n in ['check_budget.sh','check_local_budget.py','cl_resource_profiles.py','cl_transaction_evidence.py','decode_cl_transaction_evidence.cjs']]
files += list(Path('scripts').glob('cl_*.py'))+[Path('scripts/make_cl_proof_manifest.py'),Path('bots/margin-liquidator/package.json'),Path('bots/margin-liquidator/package-lock.json')]
manifest=json.loads((build/'build-manifest.json').read_text())
for name,expected in manifest['baseline_source_hashes'].items():
    p=Path(name);assert hashlib.sha256(p.read_bytes()).hexdigest()==expected,'Baseline source differs: '+name
    files.append(p)
files += [build/'harness/Cargo.toml',build/'harness/Cargo.lock',Path('harness/cl-runtime29/Cargo.toml'),Path('harness/cl-runtime29/Cargo.lock'),out/'profile.json',out/'setup-profile.json',out/'network-config.json',Path('prototypes/cl-atomic-qualification/proof-map.json')]
for name,expected in manifest['harness_generated_sources'].items():
    assert hashlib.sha256((build/name).read_bytes()).hexdigest()==expected,'Generated harness differs: '+name
files=[p for p in files if p.is_file() and '__pycache__' not in p.parts]
(out/'source-before.json').write_text(json.dumps({str(p.resolve()):hashlib.sha256(p.read_bytes()).hexdigest() for p in files},indent=2)+'\n')
(out/'run-configuration.json').write_text(json.dumps({'filter':sys.argv[3],'build':str(build),'archive_roles':os.environ.get('CL_TRANSACTION_ARCHIVE_ROLE'),'scope':'controlled-state-compiled-atomic-unsigned','stage1_complete':False,'release_approval':False,'policy_change_implemented':False,'setup':'real calls, captured-network limits enforced; excluded from assessed transactions'},indent=2)+'\n')
PIN
test_status=0
cargo test --locked --manifest-path "$build/harness/Cargo.toml" "$filter" -- --nocapture --test-threads=1 > "$run/tests.log" 2>&1 || test_status=$?
printf '%s\n' "$test_status" > "$run/test-exit-code.txt"
python3 - "$run/source-before.json" <<'PIN'
import hashlib,json,sys
from pathlib import Path
for name,h in json.loads(Path(sys.argv[1]).read_text()).items():
    assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==h,'Source/artifact changed during run: '+name
PIN
budget_status=0
python3 scripts/make_cl_proof_manifest.py --map prototypes/cl-atomic-qualification/proof-map.json --profile "$CL_RESOURCE_PROFILE" --report "$CL_RESOURCE_REPORT" --test-log "$run/tests.log" --test-exit-code "$test_status" --call-profile "$CL_CALL_PROFILE_REPORT" --output "$run/proof-manifest.json"
bash scripts/check_budget.sh --local-report "$CL_RESOURCE_REPORT" --profile "$CL_RESOURCE_PROFILE" --transaction-evidence "$CL_TRANSACTION_REPORT" --proof-manifest "$run/proof-manifest.json" --gate invocation --summary-json "$run/gates.json" > "$run/budget.log" 2>&1 || budget_status=$?
printf '%s\n' "$budget_status" > "$run/budget-exit-code.txt"
echo "Tests=$test_status; invocation gate=$budget_status. Missing Stage 1 lifecycle/transaction proofs remain failures."
[[ "$test_status" == 0 && "$budget_status" == 0 ]]
