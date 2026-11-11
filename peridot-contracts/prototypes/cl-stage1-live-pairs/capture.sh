#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
build="$(cd "${1:?prepared build}" && pwd)"
run="${2:?fresh capture output}"
[[ ! -e "$run" ]] || exit 2
mkdir -p "$run"
run="$(cd "$run" && pwd)"
config="$PWD/../docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json"
python3 scripts/cl_resource_profiles.py "$config" "$run/profile.json" --profile internal-policy
python3 scripts/cl_resource_profiles.py "$config" "$run/setup-profile.json" --profile captured-network-diagnostic
export CL_RESOURCE_PROFILE="$run/profile.json" CL_STAGE_SETUP_PROFILE="$run/setup-profile.json"
export CL_ARTIFACTS="$build/artifacts" AQUARIUS_CONCENTRATED_WASM="$PWD/target/cl-stage1-live-pairs/public/aquarius-current.wasm"
export CL_RESOURCE_REPORT="$run/resources.jsonl" CL_CALL_PROFILE_REPORT="$run/calls.jsonl" CL_TRANSACTION_REPORT="$run/transactions.jsonl" CL_PRIVATE_CAPTURE_DIR="$run"
export CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$PWD/harness/cl-runtime29/target"
status=0
cargo test --locked --manifest-path "$build/harness/Cargo.toml" compiled_stage1_full_open -- --nocapture --test-threads=1 > "$run/tests.log" 2>&1 || status=$?
printf '%s\n' "$status" > "$run/test-exit-code.txt"
budget=0
# Missing reports must fail the actual checker as well as the test runner.
bash scripts/check_budget.sh --local-report "$run/resources.jsonl" --profile "$CL_RESOURCE_PROFILE" --transaction-evidence "$CL_TRANSACTION_REPORT" --gate invocation --summary-json "$run/gates.json" > "$run/budget.log" 2>&1 || budget=$?
printf '%s\n' "$budget" > "$run/budget-exit-code.txt"
[[ "$status" == 0 && "$budget" == 0 ]] || exit 1
unset CL_PRIVATE_CAPTURE_DIR
export CL_SNAPSHOT_EXPORT_DIR="$run"
cargo test --locked --manifest-path "$build/harness/Cargo.toml" export_existing_snapshots_as_xdr -- --nocapture --test-threads=1 > "$run/export.log" 2>&1
node "$build/accounts.cjs" "$run" "$build/private-inputs"
