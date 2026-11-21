#!/usr/bin/env bash
# TESTNET, stack R2, while the pool is out of band and the D3 chain is >= 10 min old. All txs are real
# signed transactions with real auth trees (CLI signs the liquidator's source/auth; no mock_all_auths).
#  1. Manual D3-fallback liquidation by clgr-liq3, which holds a USDC trustline but NO PYUSD trustline:
#     L1b, L2, approve, L3 (BOR2), L4, L5.
#  2. Retry test: liq3 emergency-exits its seized units; stage_emergency_pay must fail (SAC #13,
#     trustline missing) and leave the command HELD; then add the PYUSD trustline, pay again, and it pays.
source "$(dirname "$0")/lib.sh"
L3SRC=clgr-liq3; job() { view "$RECEIPT" stage_job --id "$1"; }
once M_L1B "L1b checkpoint_fallback (liq3)" "$RECEIPT" $L3SRC checkpoint_fallback
once M_L2 "L2 cache_price USDC (liq3)" "$CTRL" $L3SRC cache_price --token "$USDC"
once M_APPROVE "liq3 approves the USDC market 100" "$USDC" $L3SRC approve --from "$LIQ3" --spender "$USDCM" --amount 1000000000 --expiration_ledger 6000000
NP=$(view "$RECEIPT" stage2_status | python3 -c 'import json,sys;print(json.load(sys.stdin)["next_pending"])'); put M_PENDING_ID "$NP"
once M_L3 "L3 liquidate BOR2 100 USDC on fallback (liq3)" "$CTRL" $L3SRC liquidate --borrower "$BOR2" --repay_market "$USDCM" --collateral_market "$RECEIPT" --repay_amount 1000000000 --liquidator "$LIQ3"
once M_L4 "L4 checkpoint_rewards (liq3)" "$VAULT" $L3SRC checkpoint_rewards
once M_L5 "L5 sync_seized($M_PENDING_ID) (liq3)" "$RECEIPT" $L3SRC sync_seized --id "$M_PENDING_ID"
N=$(view "$RECEIPT" stage_units --owner "$LIQ3" | tr -d '"'); echo "liq3 units=$N beneficial=$(view "$VAULT" stage_beneficial --owner "$LIQ3")"
if [[ -z "${M_EX_ID:-}" ]]; then
  out=$(inv "liq3 stage_emergency_exit all seized units (waiver)" "$RECEIPT" $L3SRC stage_emergency_exit --owner "$LIQ3" --shares "$N" --accept_reward_waiver true)
  echo "$out"; put M_EX_ID "$(echo "$out" | grep -oE 'result=[0-9]+' | cut -d= -f2)"
fi
echo "job before pay: $(job "$M_EX_ID")"
if [[ -z "${DONE_M_PAY_OK:-}" ]]; then
  if [[ -z "${DONE_M_PAY_FAIL:-}" ]]; then
    # Simulation shows the SAC error; then SEND it so the failed attempt is on-chain (CLI sends only if
    # simulation passes, so the failing attempt is recorded as the simulated refusal plus the HELD state).
    sim "stage_emergency_pay without PYUSD trustline (expect SAC #13)" "$RECEIPT" $L3SRC stage_emergency_pay --id "$M_EX_ID"
    echo "job after failed pay: $(job "$M_EX_ID")"; put DONE_M_PAY_FAIL 1
  fi
  classic TRUST_LIQ3_PYUSD "trust clgr-liq3 PYUSD (repair)" $L3SRC change-trust --line "PYUSD:$ISS"
  b0=$(view "$PYUSD" balance --id "$LIQ3"); u0=$(view "$USDC" balance --id "$LIQ3")
  once M_PAY_OK "stage_emergency_pay after adding the trustline" "$RECEIPT" $L3SRC stage_emergency_pay --id "$M_EX_ID"
  echo "job after pay: $(job "$M_EX_ID")"
  echo "liq3 PYUSD $b0 -> $(view "$PYUSD" balance --id "$LIQ3"); USDC $u0 -> $(view "$USDC" balance --id "$LIQ3")"
fi
