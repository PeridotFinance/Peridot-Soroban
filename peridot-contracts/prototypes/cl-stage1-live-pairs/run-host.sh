#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
build="$(cd "${1:?prepared build}" && pwd)"
input="$(cd "$(dirname "${2:?private input}")" && pwd)/$(basename "$2")"
run="${3:?fresh result dir}"
[[ ! -e "$run" ]] || exit 2
mkdir -p "$run"
run="$(cd "$run" && pwd)"
export CL_PRIVATE_INPUT="$input" CL_RESOURCE_PROFILE="$PWD/target/cl-stage1-internal-v6/profile.json" CL_RESOURCE_REPORT="$run/resources.jsonl"
export CARGO_TARGET_DIR="$PWD/harness/cl-runtime29/target" CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0
status=0
cargo test --locked --manifest-path "$build/harness/Cargo.toml" compiled_private_signed_root -- --nocapture --test-threads=1 > "$run/tests.log" 2>&1 || status=$?
printf '%s\n' "$status" > "$run/test-exit-code.txt"
if [[ -s "$run/resources.jsonl" ]];then
 budget=0
 bash scripts/check_budget.sh --local-report "$run/resources.jsonl" --profile "$CL_RESOURCE_PROFILE" --gate invocation --summary-json "$run/gates.json" > "$run/budget.log" 2>&1 || budget=$?
 printf '%s\n' "$budget" > "$run/budget-exit-code.txt"
 [[ "$budget" == 0 ]] || status=1
fi
exit "$status"
