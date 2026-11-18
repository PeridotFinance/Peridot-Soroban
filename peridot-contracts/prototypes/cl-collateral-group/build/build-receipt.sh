#!/usr/bin/env bash
# usage: build-receipt.sh <tree with peridot-contracts/ + openzeppelin-stellar-contracts/> <out dir> [features]
# Touches receipt sources first (one shared CARGO_TARGET_DIR). Public init-admin constant, as run-5.
set -euo pipefail
T="$(cd "$(dirname "$0")/../../.." && pwd)/target/clg"
tree=$1; out=$2; feat=${3:-}; mkdir -p "$out"
find "$tree/peridot-contracts/contracts/receipt-vault/src" -name '*.rs' -exec touch {} +
A=${INIT_ADMIN:-GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP}
args=(--package receipt-vault --locked --out-dir "$out"); [[ -n "$feat" ]] && args+=(--features "$feat")
cd "$tree/peridot-contracts" && env CARGO_TARGET_DIR=$T/cargo CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true RECEIPT_VAULT_INIT_ADMIN=$A SIMPLE_PERIDOTTROLLER_INIT_ADMIN=$A MARGIN_CONTROLLER_INIT_ADMIN=$A SWAP_ADAPTER_INIT_ADMIN=$A JUMP_RATE_MODEL_INIT_ADMIN=$A AQUARIUS_LP_VAULT_INIT_ADMIN=$A stellar contract build "${args[@]}" 2>&1 | grep -E "^error|^  -->|Complete" || true
ls -l "$out"/receipt_vault.wasm | awk '{print $5, $9}'; shasum -a 256 "$out"/receipt_vault.wasm
