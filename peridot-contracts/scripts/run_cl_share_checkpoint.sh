#!/usr/bin/env bash
# Local only; generated CONTRACT-address test administrator, not a production identity or real account-signature proof.
set -euo pipefail
cd "$(dirname "$0")/.."
export RECEIPT_VAULT_INIT_ADMIN=CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM
export SIMPLE_PERIDOTTROLLER_INIT_ADMIN="$RECEIPT_VAULT_INIT_ADMIN"
export MARGIN_CONTROLLER_INIT_ADMIN="$RECEIPT_VAULT_INIT_ADMIN"
export SWAP_ADAPTER_INIT_ADMIN="$RECEIPT_VAULT_INIT_ADMIN"
export JUMP_RATE_MODEL_INIT_ADMIN="$RECEIPT_VAULT_INIT_ADMIN"
export AQUARIUS_LP_VAULT_INIT_ADMIN="$RECEIPT_VAULT_INIT_ADMIN"
export CL_ARTIFACTS="$PWD/target/cl-share-checkpoint/artifacts"
export AQUARIUS_CONCENTRATED_WASM="$PWD/target/cl-share-checkpoint/aquarius-pool.wasm"
export CL_RESOURCE_REPORT="$PWD/target/cl-share-checkpoint/resources.jsonl"
mkdir -p "$CL_ARTIFACTS" target/cl-share-checkpoint/logs
for package in cl-share-vault cl-share-pool-facade margin-controller swap-adapter receipt-vault simple-peridottroller jump-rate-model; do
  flags=()
  if [[ "$package" == margin-controller ]]; then flags=(--features funded-liquidation-validation); fi
  stellar contract build --package "$package" ${flags[@]+"${flags[@]}"} --out-dir "$CL_ARTIFACTS" > "target/cl-share-checkpoint/logs/build-$package.log" 2>&1
done
cargo test -p cl-share-vault -- --nocapture > target/cl-share-checkpoint/logs/reward-tests.log 2>&1
# Truncate only this local run's report. Never represents a chain journal.
: > "$CL_RESOURCE_REPORT"
status=0
cargo test -p cl-share-pool-facade test::compiled -- --nocapture --test-threads=1 > target/cl-share-checkpoint/logs/harness.log 2>&1 || status=$?
FACADE_VALIDATION_WASM="$CL_ARTIFACTS/cl_share_pool_facade.wasm" cargo test -p cl-share-pool-facade compiled_facade_roundtrip -- --ignored --nocapture > target/cl-share-checkpoint/logs/compiled-facade-fixture.log 2>&1 || status=1
cargo test -p cl-share-pool-facade pr74_auth_tests -- --nocapture > target/cl-share-checkpoint/logs/pr74-auth-tests.log 2>&1 || status=1
cargo test -p cl-share-pool-facade sdk_25_3_1_g_admin_mock_setup_regression -- --nocapture > target/cl-share-checkpoint/logs/minimal-auth.log 2>&1 || status=1
bash scripts/check_budget.sh --local-report "$CL_RESOURCE_REPORT" > target/cl-share-checkpoint/logs/budget.log 2>&1 || status=1
cat target/cl-share-checkpoint/logs/budget.log
exit "$status"
