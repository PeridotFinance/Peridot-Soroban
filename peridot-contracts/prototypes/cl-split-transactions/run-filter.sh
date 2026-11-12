#!/usr/bin/env bash
# Isolated compiled fixture. No network, signing, identities or deployment.
set -euo pipefail
cd "$(dirname "$0")/../.."
profile="${1:?profile required}"
filter="${2:?test filter required}"
run="${3:?fresh output directory required}"
[[ "$profile" == internal-policy || "$profile" == captured-network-diagnostic ]]
[[ ! -e "$run" ]] || { echo "Refusing existing output: $run" >&2; exit 2; }
mkdir -p "$run"
run="$(cd "$run" && pwd)"
config="$PWD/../docs/deployment-handoffs/evidence/cl-resource-reconciliation-20261102/network-config.json"
python3 scripts/cl_resource_profiles.py "$config" "$run/profile.json" --profile "$profile"
python3 scripts/cl_resource_profiles.py "$config" "$run/fixture-profile.json" --profile captured-network-diagnostic
export CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR="$PWD/harness/cl-runtime29/target"
export CL_ARTIFACTS="$PWD/target/cl-split-artifacts"
export AQUARIUS_CONCENTRATED_WASM="$PWD/target/cl-share-checkpoint/aquarius-pool.wasm"
export CL_RESOURCE_PROFILE="$run/profile.json"
export CL_SPLIT_SETUP_PROFILE="$run/fixture-profile.json"
export CL_RESOURCE_REPORT="$run/resources.jsonl"
export CL_CALL_PROFILE_REPORT="$run/calls.jsonl"
export CL_TRANSACTION_REPORT="$run/transactions.jsonl"
: > "$CL_RESOURCE_REPORT"
: > "$CL_CALL_PROFILE_REPORT"
: > "$CL_TRANSACTION_REPORT"
status=0
cargo test --manifest-path target/cl-split-runtime29/Cargo.toml "$filter" -- --nocapture --test-threads=1 > "$run/tests.log" 2>&1 || status=$?
printf '%s\n' "$status" > "$run/test-exit-code.txt"
budget=0
bash scripts/check_budget.sh --local-report "$CL_RESOURCE_REPORT" --profile "$CL_RESOURCE_PROFILE" --transaction-evidence "$CL_TRANSACTION_REPORT" --gate invocation --summary-json "$run/gates.json" > "$run/budget.log" 2>&1 || budget=$?
printf '%s\n' "$budget" > "$run/budget-exit-code.txt"
echo "Test exit=$status invocation checker exit=$budget; full lifecycle/transaction/release gates remain separate."
[[ "$status" == 0 && "$budget" == 0 ]]
