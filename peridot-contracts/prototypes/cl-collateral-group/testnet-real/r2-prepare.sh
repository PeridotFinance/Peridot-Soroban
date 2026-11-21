#!/usr/bin/env bash
# Stack R2 (TESTNET): the CURRENT release candidates (build-testnet.sh at a pinned commit, Testnet admin
# constant) deployed fresh on the SAME real Aquarius pool, SAC tokens and feeds as stack R1.
# Seeds target/clg/testnet-real2/state.env from R1's venue/token/identity keys only, links the R2 artefact
# dir, then 01-deploy.sh (resumable) deploys vault, controller, receipt and USDC market fresh.
# usage: r2-prepare.sh <tn-build dir>
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"; ROOT="$(cd "$HERE/../../.." && pwd)"
B=$(cd "$1" && pwd); R1="$ROOT/target/clg/testnet-real"; R2="$ROOT/target/clg/testnet-real2"; A="$ROOT/target/clg/art/testnet-r2"
mkdir -p "$R2" "$A"
[[ -s "$R2/state.env" ]] || grep -E '^(ADM|ISS|BOR1|BOR2|LIQ|SUP|EXT_ORACLE|PYUSD|USDC|AQUA|BOOST|PLANE|CONFIGS|BFEED|POOL|RVAULT|ORACLE)=|^DONE_(TRUST_|MINT_ADM|MINT_BOR|MINT_LIQ|PLANE_INIT|CFG_|POOL_|FUND_RV|ORACLE_PYUSD)' "$R1/state.env" > "$R2/state.env"
ln -sfn "$B/art/vault-v2" "$A/vault"; ln -sfn "$B/art/ctrl-group" "$A/ctrl"; ln -sfn "$B/art/receipt-v2" "$A/receipt"
ln -sfn "$ROOT/target/clg/art/testnet/cash" "$A/cash"; ln -sfn "$ROOT/target/clg/art/testnet/cl_group_testnet_oracle.wasm" "$A/cl_group_testnet_oracle.wasm"
echo "R2 state $(wc -l < "$R2/state.env") keys; run: CLG_TN_DIR=$R2 CLG_ART=$A bash $HERE/01-deploy.sh"
