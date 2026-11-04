#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# Scope is exactly our isolated unsigned fixture, never the ordinary signing tests.
export CL_CORE_FORBID_SIGNING=1
unset CL_CORE_RESTORE_CHAIN_INPUT
case "${1:-code}" in
  code) export CL_CORE_RESTORE_INPUT="$ROOT/receipt-code-input.txt" ;;
  state) export CL_CORE_RESTORE_INPUT="$ROOT/receipt-state-input.txt" ;;
  partial) export CL_CORE_RESTORE_INPUT="$ROOT/partial-input.txt" ;;
  chain) export CL_CORE_RESTORE_INPUT="$ROOT/receipt-code-input.txt"; export CL_CORE_RESTORE_CHAIN_INPUT="$ROOT/maintenance-input.txt" ;;
  *) echo "Only code, state, partial or chain controlled unsigned fixtures are supported" >&2; exit 2 ;;
esac
cd "$ROOT/build-core"
exec ./src/stellar-core test '[cl-option1-restore]' --version 29 --rng-seed 20261103 --ll warning
