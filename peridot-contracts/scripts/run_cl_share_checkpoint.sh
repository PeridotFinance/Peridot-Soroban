#!/usr/bin/env bash
# Local synthetic administrators only; no signing, broadcasting or secret access.
# Preserve previous evidence: an existing output directory is refused.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${CL_CAPTURED_CONFIG:?set CL_CAPTURED_CONFIG to the captured network-config.json}"
: "${AQUARIUS_CONCENTRATED_WASM:?set pinned Aquarius WASM path}"
export AQUARIUS_CONCENTRATED_WASM="$(python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$AQUARIUS_CONCENTRATED_WASM")"
run_dir="${CL_CHECKPOINT_DIR:-$PWD/target/cl-resource-checkpoint}"
[[ ! -e "$run_dir" ]] || { echo "Refusing to overwrite checkpoint: $run_dir" >&2; exit 1; }
mkdir -p "$run_dir/artifacts" "$run_dir/logs"
run_dir="$(cd "$run_dir" && pwd)"
cp "$CL_CAPTURED_CONFIG" "$run_dir/network-config.json"
export CL_ARTIFACTS="$run_dir/artifacts"
export RECEIPT_VAULT_INIT_ADMIN=CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM
export SIMPLE_PERIDOTTROLLER_INIT_ADMIN="$RECEIPT_VAULT_INIT_ADMIN"
export MARGIN_CONTROLLER_INIT_ADMIN="$RECEIPT_VAULT_INIT_ADMIN"
export SWAP_ADAPTER_INIT_ADMIN="$RECEIPT_VAULT_INIT_ADMIN"
export JUMP_RATE_MODEL_INIT_ADMIN="$RECEIPT_VAULT_INIT_ADMIN"
export AQUARIUS_LP_VAULT_INIT_ADMIN="$RECEIPT_VAULT_INIT_ADMIN"
for package in cl-share-vault cl-share-pool-facade margin-controller swap-adapter receipt-vault simple-peridottroller jump-rate-model; do
  flags=()
  if [[ "$package" == margin-controller ]]; then flags=(--features funded-liquidation-validation); fi
  stellar contract build --package "$package" ${flags[@]+"${flags[@]}"} --out-dir "$CL_ARTIFACTS" > "$run_dir/logs/build-$package.log" 2>&1
done
status=0
cargo test -p cl-share-vault -- --nocapture > "$run_dir/logs/reward-tests.log" 2>&1 || status=1
for profile in internal-policy captured-network-diagnostic; do
  export CL_RESOURCE_PROFILE="$run_dir/$profile.json"
  export CL_RESOURCE_REPORT="$run_dir/$profile-resources.jsonl"
  export CL_CALL_PROFILE_REPORT="$run_dir/$profile-calls.jsonl"
  python3 scripts/cl_resource_profiles.py "$run_dir/network-config.json" "$CL_RESOURCE_PROFILE" --profile "$profile"
  cargo test --manifest-path harness/cl-runtime29/Cargo.toml runtime_integrity -- --nocapture > "$run_dir/logs/$profile-integrity.log" 2>&1 || status=1
  cargo test --manifest-path harness/cl-runtime29/Cargo.toml compiled -- --nocapture --test-threads=1 > "$run_dir/logs/$profile-harness.log" 2>&1 || status=1
  bash scripts/check_budget.sh --local-report "$CL_RESOURCE_REPORT" --profile "$CL_RESOURCE_PROFILE" > "$run_dir/logs/$profile-budget.log" 2>&1 || status=1
  # Native funded prototype evidence is separate from compiled protocol29 flows.
  CL_RESOURCE_REPORT="$run_dir/$profile-native-funded.jsonl" cargo test -p margin-controller --features funded-liquidation-validation funded_prototype -- --nocapture --test-threads=1 > "$run_dir/logs/$profile-native-funded.log" 2>&1 || status=1
  bash scripts/check_budget.sh --local-report "$run_dir/$profile-native-funded.jsonl" --profile "$CL_RESOURCE_PROFILE" > "$run_dir/logs/$profile-native-funded-budget.log" 2>&1 || status=1
done
printf 'Checkpoint: %s\nStatus: %s (missing proofs remain failures)\n' "$run_dir" "$status"
exit "$status"
