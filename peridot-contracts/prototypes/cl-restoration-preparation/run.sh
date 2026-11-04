#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export CL_ATOMIC_BUILD_DIR="$(cd "${CL_ATOMIC_BUILD_DIR:?prepared combined build required}" && pwd)"
filter="${1:?focused test filter}"; run="${2:?fresh directory}"
[[ ! -e "$run" ]] || exit 2
mkdir -p "$run/profiles"
run="$(cd "$run" && pwd)"
cp ../docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json "$run/network-config.json"
for profile in internal-policy captured-network-diagnostic; do
 python3 scripts/cl_resource_profiles.py "$run/network-config.json" "$run/profiles/$profile.json" --profile "$profile"
done
export CL_RESTORE_INTERNAL_PROFILE="$run/profiles/internal-policy.json"
export CL_RESTORE_CAPTURED_PROFILE="$run/profiles/captured-network-diagnostic.json"
export CL_RESTORE_REPORT="$run/preparation.jsonl"
export CL_RESTORE_SDK_REPORT="$run/post-resources.jsonl"
export CL_RESTORE_FINANCIAL_REPORT="$run/post-transactions.jsonl"
export CL_RESTORE_JOURNEY_REPORT="$run/journeys.jsonl"
for file in "$CL_RESTORE_REPORT" "$CL_RESTORE_SDK_REPORT" "$CL_RESTORE_FINANCIAL_REPORT" "$CL_RESTORE_JOURNEY_REPORT"; do : > "$file"; done
status=0
bash prototypes/cl-storage-qualification/run_variant.sh captured-network-diagnostic "$filter" "$run/checkpoint" > "$run/runner.log" 2>&1 || status=$?
printf '%s\n' "$status" > "$run/runner-exit.txt"
verification_status=0
python3 prototypes/cl-restoration-preparation/check_runs.py "$run" || verification_status=$?
printf '%s\n' "$verification_status" > "$run/verification-exit.txt"
if [[ "$verification_status" != 0 ]]; then exit "$verification_status"; fi
exit "$status"
