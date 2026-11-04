#!/usr/bin/env bash
# Run from the repository root. Existing outputs are never overwritten.
set -euo pipefail
build="$(cd "${1:?prepared shared build}" && pwd)"
run="${2:?fresh run directory}"
verification_script="$(cd "$(dirname "$0")" && pwd)/shared-verify.py"
[[ ! -e "$run" ]]
mkdir -p "$run"
run="$(cd "$run" && pwd)"
cp ../docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json "$run/network-config.json"
python3 scripts/cl_resource_profiles.py "$run/network-config.json" "$run/internal-profile.json" --profile internal-policy
python3 scripts/cl_resource_profiles.py "$run/network-config.json" "$run/captured-profile.json" --profile captured-network-diagnostic
export CL_ATOMIC_BUILD_DIR="$build" CARGO_BUILD_JOBS=1
export CL_SHARED_EVIDENCE_DIR="$run/state"
export CL_RESTORE_INTERNAL_PROFILE="$run/internal-profile.json" CL_RESTORE_CAPTURED_PROFILE="$run/captured-profile.json"
export CL_RESTORE_REPORT="$run/preparation.jsonl" CL_RESTORE_SDK_REPORT="$run/post-resources.jsonl" CL_RESTORE_FINANCIAL_REPORT="$run/post-transactions.jsonl"
export CL_SHARED_INTERNAL_RESOURCES="$run/internal-resources.jsonl" CL_SHARED_INTERNAL_TRANSACTIONS="$run/internal-transactions.jsonl" CL_SHARED_INTERNAL_CALLS="$run/internal-calls.jsonl"
for file in "$CL_RESTORE_REPORT" "$CL_RESTORE_SDK_REPORT" "$CL_RESTORE_FINANCIAL_REPORT" "$CL_SHARED_INTERNAL_RESOURCES" "$CL_SHARED_INTERNAL_TRANSACTIONS" "$CL_SHARED_INTERNAL_CALLS"; do : > "$file"; done
cl_shared_exit=0
bash prototypes/cl-storage-qualification/run_variant.sh captured-network-diagnostic compiled_shared_eight_pending_recovery_joint_state "$run/checkpoint" > "$run/runner.log" 2>&1 || cl_shared_exit=$?
printf '%s\n' "$cl_shared_exit" > "$run/runner-exit.txt"
python3 "$verification_script" --run "$run" --build "$build" > "$run/independent-check.log" 2>&1 || cl_shared_exit=$?
python3 - "$run" "$build" <<'PY'
import json,sys
from pathlib import Path
r,b=map(Path,sys.argv[1:]);name='test::shared_qualification::compiled_shared_eight_pending_recovery_joint_state'
m={'schema':'cl-proof-map-v1','sources':{name:str(b/'harness/src/shared_fixture.rs')},'proofs':[{'label':'option1_shared_eight_pending_recovery','tests':[name],'assertions':['One joint state; captured eight sequential independently funded expired PendingOpen recoveries; internal stops at finite resource rejection; exact SDK/host state and events with listed fresh native mock nonce substitution, booked reward preservation and scaled conservation. Unsigned controlled state; native restoration TTL projection only.'],'resource_flows':['recover_expired_pending_funded']}]}
(r/'shared-proof-map.json').write_text(json.dumps(m,indent=2)+'\n')
PY
for profile in captured internal; do
  if [[ "$profile" == captured ]]; then resources="$run/checkpoint/resources.jsonl"; transactions="$run/checkpoint/transactions.jsonl"; calls="$run/checkpoint/calls.jsonl";
  else resources="$run/internal-resources.jsonl"; transactions="$run/internal-transactions.jsonl"; calls="$run/internal-calls.jsonl"; fi
  python3 scripts/make_cl_proof_manifest.py --map "$run/shared-proof-map.json" --profile "$run/$profile-profile.json" --report "$resources" --test-log "$run/checkpoint/tests.log" --test-exit-code "$(cat "$run/checkpoint/test-exit-code.txt")" --call-profile "$calls" --output "$run/$profile-proof-manifest.json"
  cl_shared_gate=0
  bash scripts/check_budget.sh --local-report "$resources" --profile "$run/$profile-profile.json" --transaction-evidence "$transactions" --proof-manifest "$run/$profile-proof-manifest.json" --gate checkpoint --summary-json "$run/$profile-gates.json" > "$run/$profile-budget.log" 2>&1 || cl_shared_gate=$?
  printf '%s\n' "$cl_shared_gate" > "$run/$profile-checkpoint-exit.txt"
  if [[ "$cl_shared_gate" != 0 ]]; then cl_shared_exit="$cl_shared_gate"; fi
 done
# Missing Stage1 proofs and rejected policy remain a failed checkpoint.
exit "$cl_shared_exit"
