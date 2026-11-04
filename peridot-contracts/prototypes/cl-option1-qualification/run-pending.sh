#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
profile="${1:?existing resource profile}"
run="${2:?fresh output}"
export CL_ATOMIC_BUILD_DIR="$(cd "${CL_ATOMIC_BUILD_DIR:?isolated pending build}" && pwd)"
status=0
bash prototypes/cl-storage-qualification/run_variant.sh "$profile" pending_cases "$run" || status=$?
printf '%s\n' "$status" > "$run/runner-exit.txt"
python3 scripts/make_cl_proof_manifest.py \
  --map prototypes/cl-option1-qualification/pending-proof-map.json \
  --profile "$run/profile.json" --report "$run/resources.jsonl" \
  --test-log "$run/tests.log" --test-exit-code "$(cat "$run/test-exit-code.txt")" \
  --call-profile "$run/calls.jsonl" --output "$run/pending-proof-manifest.json"
gate=0
bash scripts/check_budget.sh --local-report "$run/resources.jsonl" \
  --profile "$run/profile.json" --transaction-evidence "$run/transactions.jsonl" \
  --proof-manifest "$run/pending-proof-manifest.json" --gate checkpoint \
  --summary-json "$run/pending-gates.json" > "$run/pending-budget.log" 2>&1 || gate=$?
printf '%s\n' "$gate" > "$run/pending-gate-exit.txt"
if [[ "$gate" != 0 ]]; then exit "$gate"; fi
exit "$status"
