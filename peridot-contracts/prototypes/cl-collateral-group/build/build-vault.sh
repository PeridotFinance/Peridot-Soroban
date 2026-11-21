#!/usr/bin/env bash
# usage: build-vault.sh <workspace dir> <out dir>   (touches sources: one shared CARGO_TARGET_DIR)
set -euo pipefail
T="$(cd "$(dirname "$0")/../../.." && pwd)/target/clg"
ws=$1; out=$2; mkdir -p "$out"
find "$ws/contracts/cl-share-vault/src" -name '*.rs' -exec touch {} +
A=${INIT_ADMIN:-GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP}
cd "$ws" && env CARGO_TARGET_DIR=$T/cargo CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true RECEIPT_VAULT_INIT_ADMIN=$A SIMPLE_PERIDOTTROLLER_INIT_ADMIN=$A MARGIN_CONTROLLER_INIT_ADMIN=$A SWAP_ADAPTER_INIT_ADMIN=$A JUMP_RATE_MODEL_INIT_ADMIN=$A AQUARIUS_LP_VAULT_INIT_ADMIN=$A stellar contract build --manifest-path Cargo.toml --package cl-share-vault --locked --out-dir "$out" --features "cl-pyusd-vault-b${FEATURES_EXTRA:+,$FEATURES_EXTRA}" ${EXTRA:-} 2>&1 | grep -E "^error|Complete" || true
ls -l "$out"/*.wasm | awk '{print $5, $9}'; shasum -a 256 "$out"/*.wasm
