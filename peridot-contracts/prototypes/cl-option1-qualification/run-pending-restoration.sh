#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
run="${1:?fresh output directory}"
[[ ! -e "$run" ]] || exit 2
# Paths are external to the main preparation report: these are additional
# measured synthetic-reexpiry batches, not hidden or duplicate main-chain rows.
parent="$(cd "$(dirname "$run")" && pwd)"
name="$(basename "$run")"
export CL_PENDING_REEXPIRY_BATCHES="$parent/$name-reexpiry-batches.jsonl"
export CL_PENDING_REEXPIRY_REPORT="$parent/$name-reexpiry.jsonl"
[[ ! -e "$CL_PENDING_REEXPIRY_BATCHES" && ! -e "$CL_PENDING_REEXPIRY_REPORT" ]] || exit 2
: > "$CL_PENDING_REEXPIRY_BATCHES"
: > "$CL_PENDING_REEXPIRY_REPORT"
status=0
bash prototypes/cl-restoration-preparation/run.sh \
  compiled_option1_expired_pending_independent_funding_outage_trade_and_rollback \
  "$run" || status=$?
printf '%s\n' "$status" > "$run/conditional-restore-exit.txt"
exit "$status"
