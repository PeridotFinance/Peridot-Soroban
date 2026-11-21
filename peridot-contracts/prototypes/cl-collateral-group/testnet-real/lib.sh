#!/usr/bin/env bash
# CL collateral group TESTNET rehearsal helpers. TESTNET ONLY: every call passes --network testnet,
# and `net_guard` refuses anything else. Keys stay inside the stellar CLI (identity NAMES only;
# nothing here reads, prints or exports key material). State and the append-only tx journal live
# in target/clg/testnet/ (git-ignored; transaction artifacts are never deleted).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
TN="${CLG_TN_DIR:-$ROOT/target/clg/testnet-real}"; mkdir -p "$TN"
STATE="$TN/state.env"; JOURNAL="$TN/journal.jsonl"; touch "$STATE" "$JOURNAL"
ART="${CLG_ART:-$ROOT/target/clg/art/testnet}"
NET=testnet
net_guard() { [[ "$NET" == "testnet" ]] || { echo "REFUSED: testnet only" >&2; exit 2; }; }
net_guard
# shellcheck disable=SC1090
source "$STATE"
put() { grep -v "^$1=" "$STATE" > "$STATE.tmp" || true; echo "$1=$2" >> "$STATE.tmp"; mv "$STATE.tmp" "$STATE"; eval "$1=$2"; }
addr() { stellar keys address "$1" 2>/dev/null | tail -1; }
now() { date -u +%Y-%m-%dT%H:%M:%SZ; }
jlog() { printf '%s\n' "$1" >> "$JOURNAL"; }
# deploy <var> <wasm> <source> [-- constructor args]
deploy() {
  local var=$1 wasm=$2 src=$3; shift 3
  if [[ -n "${!var:-}" ]]; then echo "$var=${!var} (exists)"; return; fi
  local err="$TN/last.err" id
  id=$(stellar contract deploy --wasm "$wasm" --source "$src" --network $NET "$@" 2>"$err" | tail -1)
  local h; h=$(grep -oE '[0-9a-f]{64}' "$err" | tail -1 || true)
  [[ "$id" =~ ^C[A-Z0-9]{55}$ ]] || { cat "$err" >&2; exit 1; }
  put "$var" "$id"
  jlog "{\"t\":\"$(now)\",\"kind\":\"deploy\",\"var\":\"$var\",\"id\":\"$id\",\"wasm_sha256\":\"$(shasum -a 256 "$wasm" | cut -d' ' -f1)\",\"tx\":\"$h\",\"source\":\"$src\"}"
  echo "$var=$id tx=$h"
}
# inv <label> <contract> <source> <fn> [args...]   (sends; records the hash)
inv() {
  local label=$1 id=$2 src=$3; shift 3
  local err="$TN/last.err" out rc=0
  out=$(stellar contract invoke --id "$id" --source "$src" --network $NET --send yes -- "$@" 2>"$err") || rc=$?
  local h; h=$(LC_ALL=C grep -aoE 'tx/[0-9a-f]{64}' "$err" | tail -1 | cut -d/ -f2 || true)
  [[ -n "$h" ]] || h=$(LC_ALL=C grep -aoE 'Signing transaction: [0-9a-f]{64}' "$err" | tail -1 | awk '{print $3}' || true)
  local ok=true; [[ $rc -eq 0 ]] || ok=false
  local errline=""; [[ $rc -eq 0 ]] || errline=$(grep -oE 'Error\((Contract|WasmVm|Auth|Storage|Context|Value|Budget), #?[A-Za-z0-9]+\)' "$err" | head -1 || true)
  jlog "{\"t\":\"$(now)\",\"kind\":\"invoke\",\"label\":\"$label\",\"id\":\"$id\",\"fn\":\"$1\",\"ok\":$ok,\"tx\":\"$h\",\"result\":$(printf '%s' "${out:-null}" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read()))'),\"error\":\"$errline\",\"source\":\"$src\"}"
  echo "[$label] ok=$ok tx=$h result=${out:-} $errline"
  [[ $rc -eq 0 ]] || return $rc
}
# view <contract> <fn> [args...]   (simulation only, never sent)
view() { local id=$1; shift; stellar contract invoke --id "$id" --source clg-admin --network $NET --send no -- "$@" 2>/dev/null; }
# sim <label> <contract> <source> <fn> [args...]  (simulate; journal the expected outcome, e.g. a refusal)
sim() {
  local label=$1 id=$2 src=$3; shift 3
  local err="$TN/last.err" out rc=0
  out=$(stellar contract invoke --id "$id" --source "$src" --network $NET --send no -- "$@" 2>"$err") || rc=$?
  local e=""; e=$(grep -oE 'Error\((Contract|WasmVm|Context), #?[A-Za-z0-9]+\)' "$err" | head -1 || true)
  jlog "{\"t\":\"$(now)\",\"kind\":\"simulate\",\"label\":\"$label\",\"id\":\"$id\",\"fn\":\"$1\",\"rc\":$rc,\"result\":\"${out:-}\",\"error\":\"$e\"}"
  echo "[$label] simulated rc=$rc result=${out:-} $e"
}
# once <step-key> <label> <contract> <source> <fn> [args...]: run a send exactly once (resumable).
once() {
  local key="DONE_$1"; shift
  if [[ -n "${!key:-}" ]]; then return 0; fi
  inv "$@"
  put "$key" 1
}
# deploy_hash <var> <wasm-hash> <source> [-- constructor args]: deploy an instance of code ALREADY on Testnet.
deploy_hash() {
  local var=$1 hash=$2 src=$3; shift 3
  if [[ -n "${!var:-}" ]]; then echo "$var=${!var} (exists)"; return; fi
  local err="$TN/last.err" id
  id=$(stellar contract deploy --wasm-hash "$hash" --source "$src" --network $NET "$@" 2>"$err" | tail -1)
  local h; h=$(grep -oE '[0-9a-f]{64}' "$err" | grep -v "$hash" | tail -1 || true)
  [[ "$id" =~ ^C[A-Z0-9]{55}$ ]] || { cat "$err" >&2; exit 1; }
  put "$var" "$id"
  jlog "{\"t\":\"$(now)\",\"kind\":\"deploy\",\"var\":\"$var\",\"id\":\"$id\",\"wasm_sha256\":\"$hash\",\"tx\":\"$h\",\"source\":\"$src\"}"
  echo "$var=$id tx=$h"
}
# classic: <step-key> <label> <source> <stellar tx new ... args>
classic() {
  local key="DONE_$1" label=$2 src=$3; shift 3
  if [[ -n "${!key:-}" ]]; then return 0; fi
  local err="$TN/last.err" rc=0
  stellar tx new "$@" --source-account "$src" --network $NET >"$TN/last.out" 2>"$err" || rc=$?
  local h; h=$(grep -ohE '[0-9a-f]{64}' "$err" "$TN/last.out" | tail -1 || true)
  jlog "{\"t\":\"$(now)\",\"kind\":\"classic\",\"label\":\"$label\",\"ok\":$([[ $rc -eq 0 ]] && echo true || echo false),\"tx\":\"$h\",\"source\":\"$src\"}"
  echo "[$label] rc=$rc tx=$h"; [[ $rc -eq 0 ]] || { cat "$err" >&2; return $rc; }
  put "$key" 1
}
