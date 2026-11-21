#!/usr/bin/env bash
# TESTNET upgrade-path test on stack R2 (release candidates, Testnet admin). There is no short-timelock
# config (ETA = now + 86,400 s is a constant), so this runs in two sittings:
#   CLG_TN_DIR=.../testnet-real2 bash 05-upgrade.sh propose   # snapshot, refusals, cancel/re-propose, propose x3
#   CLG_TN_DIR=.../testnet-real2 bash 05-upgrade.sh execute   # after every ETA: pause, upgrade, markers, state diff, unpause
# Targets are the upgrade-marker builds of the same sources (v2-prime: marker() == 2, nothing else changed).
source "$(dirname "$0")/lib.sh"
VM=b68acb142e44b2ecdf9f76638bf5fccae5df06ac9603962ed397deb39fb786e3   # vault-v2-marker
RM=88c387ddf8d533482c9370c90bbc8ed1ce8aacc639646ef000e06c66f12bc314   # receipt-v2-marker
CM=d5b551fef714c20344ef9e5dd8dedb19506b409d97f6193b36c2f6911ec86557   # ctrl-group-marker
snap() { # <file>: every value an upgrade must preserve, one JSON object per line
  { for w in BOR1 BOR2; do
      echo "{\"k\":\"units $w\",\"v\":$(view "$RECEIPT" stage_units --owner "${!w}")}"
      echo "{\"k\":\"beneficial $w\",\"v\":$(view "$VAULT" stage_beneficial --owner "${!w}")}"
      echo "{\"k\":\"reward_account $w\",\"v\":$(view "$VAULT" reward_account --holder "${!w}")}"
      echo "{\"k\":\"debt $w\",\"v\":$(view "$USDCM" get_user_borrow_balance --user "${!w}")}"
      echo "{\"k\":\"markets $w\",\"v\":$(view "$CTRL" get_user_markets --user "${!w}" 2>/dev/null || echo null)}"
    done
    echo "{\"k\":\"vault position\",\"v\":$(view "$POOL" get_user_position_snapshot --user "$VAULT")}"
    echo "{\"k\":\"vault idle\",\"v\":$(view "$VAULT" stage_idle)}"
    echo "{\"k\":\"vault reward_totals\",\"v\":$(view "$VAULT" reward_totals)}"
    echo "{\"k\":\"vault admin\",\"v\":$(view "$VAULT" get_admin 2>/dev/null || echo null)}"
    echo "{\"k\":\"receipt stage2_config\",\"v\":$(view "$RECEIPT" stage2_config)}"
    echo "{\"k\":\"receipt next_pending\",\"v\":$(view "$RECEIPT" stage2_status | python3 -c 'import json,sys;print(json.load(sys.stdin)["next_pending"])')}"
    echo "{\"k\":\"receipt custody\",\"v\":$(view "$VAULT" stage_beneficial --owner "$RECEIPT")}"
    echo "{\"k\":\"ctrl cf\",\"v\":$(view "$CTRL" get_market_cf --market "$RECEIPT")}"
    echo "{\"k\":\"ctrl close factor\",\"v\":$(view "$CTRL" get_close_factor_scaled)}"
    echo "{\"k\":\"ctrl oracle\",\"v\":$(view "$CTRL" get_oracle 2>/dev/null || echo null)}"
    echo "{\"k\":\"usdcm total borrowed\",\"v\":$(view "$USDCM" get_total_borrowed 2>/dev/null || echo null)}"
  } > "$1"; echo "snapshot -> $1 ($(wc -l < "$1") values)"; }
case ${1:-} in
propose)
  snap "$TN/upgrade-before-propose.jsonl"
  sim "vault execute with nothing pending (expect refusal)" "$VAULT" clg-admin upgrade_wasm --admin_addr "$ADM" --new_wasm_hash "$VM"
  sim "ctrl propose by a non-admin source (expect auth refusal)" "$CTRL" clgr-bor1 propose_upgrade_wasm --new_wasm_hash "$CM"
  once UPG_CTRL_P1 "ctrl propose (to be cancelled)" "$CTRL" clg-admin propose_upgrade_wasm --new_wasm_hash "$CM"
  echo "ctrl pending=$(view "$CTRL" get_pending_upgrade)"
  once UPG_CTRL_CANCEL "ctrl cancel_upgrade" "$CTRL" clg-admin cancel_upgrade
  echo "ctrl pending after cancel=$(view "$CTRL" get_pending_upgrade)"
  sim "ctrl second cancel (expect refusal)" "$CTRL" clg-admin cancel_upgrade
  sim "ctrl execute cancelled hash (expect refusal)" "$CTRL" clg-admin upgrade_wasm --new_wasm_hash "$CM"
  once UPG_CTRL_P2 "ctrl propose (live)" "$CTRL" clg-admin propose_upgrade_wasm --new_wasm_hash "$CM"
  once UPG_RECEIPT_P "receipt propose" "$RECEIPT" clg-admin propose_upgrade_wasm --new_wasm_hash "$RM"
  once UPG_VAULT_P "vault propose" "$VAULT" clg-admin propose_upgrade_wasm --admin_addr "$ADM" --new_wasm_hash "$VM"
  for c in CTRL RECEIPT VAULT; do echo "$c pending=$(view "${!c}" get_pending_upgrade)"; done
  sim "ctrl execute before ETA (expect refusal)" "$CTRL" clg-admin upgrade_wasm --new_wasm_hash "$CM"
  sim "receipt execute before ETA (expect refusal)" "$RECEIPT" clg-admin upgrade_wasm --new_wasm_hash "$RM"
  sim "vault execute before ETA (expect refusal)" "$VAULT" clg-admin upgrade_wasm --admin_addr "$ADM" --new_wasm_hash "$VM"
  ;;
execute)
  for c in CTRL RECEIPT VAULT; do echo "$c pending=$(view "${!c}" get_pending_upgrade) ledger_now=$(date +%s)"; done
  snap "$TN/upgrade-before-execute.jsonl"
  sim "vault execute while unpaused (expect refusal)" "$VAULT" clg-admin upgrade_wasm --admin_addr "$ADM" --new_wasm_hash "$VM"
  sim "receipt execute with the controller pauses off (expect refusal)" "$RECEIPT" clg-admin upgrade_wasm --new_wasm_hash "$RM"
  once UPG_CTRL_X "ctrl upgrade_wasm -> marker" "$CTRL" clg-admin upgrade_wasm --new_wasm_hash "$CM"
  echo "ctrl marker=$(view "$CTRL" upgrade_marker)"
  once UPG_PAUSE_DEP "ctrl pause deposit (receipt)" "$CTRL" clg-admin set_pause_deposit --market "$RECEIPT" --paused true
  once UPG_PAUSE_RED "ctrl pause redeem (receipt)" "$CTRL" clg-admin set_pause_redeem --market "$RECEIPT" --paused true
  # receipt borrow is already paused at deploy (CTRL_BORROW_PAUSE_RECEIPT)
  once UPG_RECEIPT_X "receipt upgrade_wasm -> marker" "$RECEIPT" clg-admin upgrade_wasm --new_wasm_hash "$RM"
  echo "receipt marker=$(view "$RECEIPT" upgrade_marker)"
  once UPG_VAULT_PAUSE "vault set_paused true" "$VAULT" clg-admin set_paused --admin_addr "$ADM" --paused true
  once UPG_VAULT_X "vault upgrade_wasm -> marker" "$VAULT" clg-admin upgrade_wasm --admin_addr "$ADM" --new_wasm_hash "$VM"
  echo "vault marker=$(view "$VAULT" upgrade_marker)"
  once UPG_VAULT_UNPAUSE "vault set_paused false" "$VAULT" clg-admin set_paused --admin_addr "$ADM" --paused false
  once UPG_UNPAUSE_DEP "ctrl unpause deposit (receipt)" "$CTRL" clg-admin set_pause_deposit --market "$RECEIPT" --paused false
  once UPG_UNPAUSE_RED "ctrl unpause redeem (receipt)" "$CTRL" clg-admin set_pause_redeem --market "$RECEIPT" --paused false
  snap "$TN/upgrade-after-execute.jsonl"
  diff "$TN/upgrade-before-execute.jsonl" "$TN/upgrade-after-execute.jsonl" && echo "STATE SURVIVED: identical snapshot" || echo "STATE DIFF above (review: interest accrual on debt is expected)"
  inv "post-upgrade L1a checkpoint_nav" "$RECEIPT" clg-admin checkpoint_nav
  ;;
*) echo "usage: $0 propose|execute" >&2; exit 2;;
esac
