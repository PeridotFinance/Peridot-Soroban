# Source this file in the operator's existing terminal. It does not send anything
# when sourced. Each cash_topup call requires the operator's Ledger approval.
cash_topup() {
  local market_name="$1" stage="$2"
  local receipt_id asset_id seed_raw before_owner amount_raw before_total after_owner after_total
  case "$stage" in 1|2) ;; *) printf 'STOP: stage must be 1 or 2\n'; return 1 ;; esac
  if [ "$stage" = 2 ]; then
    node scripts/cash_operator_checks.cjs second-gate || return 1
  else
    python3 - <<'PY' || return 1
import json
from pathlib import Path
p=Path('research/stellar-cash-ttl-20261112/usdc-redeposited.json')
assert p.exists(), 'Full USDC exit/redeposit checkpoint is missing'
assert json.loads(p.read_text())['result']=='PASS'
PY
  fi
  local parameters
  parameters="$(python3 - "$market_name" "$stage" <<'PY'
import json,sys
from pathlib import Path
m=json.loads(Path('research/stellar-redeployment-20261111/fresh-deployment-manifest.json').read_text())
p=json.loads(Path('research/stellar-cash-ttl-20261112/topup-plan.json').read_text())
name,stage=sys.argv[1],int(sys.argv[2]);row=m['markets'][name];amount=p['amounts'][name]
before=int(row['ownerSharesRaw'])+(int(amount['firstRaw']) if stage==2 else 0)
print(row['receipt'],row['underlying'],row['permanentSeedRaw'],before,amount['firstRaw' if stage==1 else 'secondRaw'])
PY
  )" || return 1
  read -r receipt_id asset_id seed_raw before_owner amount_raw <<< "$parameters"
  before_total=$((seed_raw + before_owner))
  after_owner=$((before_owner + amount_raw))
  after_total=$((seed_raw + after_owner))
  check_admin "$receipt_id" || return 1
  expect_read "$receipt_id" false get_cash_borrowing_enabled || return 1
  expect_read "$receipt_id" 10000 get_min_cash_reserve_bps || return 1
  expect_read "$receipt_id" "$seed_raw" get_minimum_supply || return 1
  expect_read "$receipt_id" "$before_owner" get_ptoken_balance --user "$LEDGER_PUBLIC" || return 1
  expect_read "$receipt_id" "$before_total" get_total_ptokens || return 1
  expect_read "$receipt_id" "$before_total" get_total_underlying || return 1
  expect_read "$asset_id" "$before_total" balance --id "$receipt_id" || return 1
  expect_read "$receipt_id" 1000000 get_exchange_rate || return 1
  ledger_call "$receipt_id" deposit --user "$LEDGER_PUBLIC" --amount "$amount_raw" || return 1
  expect_read "$receipt_id" "$after_owner" get_ptoken_balance --user "$LEDGER_PUBLIC" || return 1
  expect_read "$receipt_id" "$seed_raw" get_ptoken_balance --user "$receipt_id" || return 1
  expect_read "$receipt_id" "$after_total" get_total_ptokens || return 1
  expect_read "$receipt_id" "$after_total" get_total_underlying || return 1
  expect_read "$asset_id" "$after_total" balance --id "$receipt_id" || return 1
  expect_read "$receipt_id" 1000000 get_exchange_rate || return 1
  expect_read "$receipt_id" 0 get_total_borrowed || return 1
  expect_read "$receipt_id" false get_cash_borrowing_enabled || return 1
  printf 'Verified %s top-up half %s\n' "$market_name" "$stage"
}
