"""Public, unsigned cash-only deployment plan. No RPC, identities, or secret inputs."""
import json
import re
import sys
from pathlib import Path

c = json.loads(Path(sys.argv[1]).read_text())
allowed = {'admin', 'oracle', 'markets', 'maintenance_margin'}
if set(c) - allowed:
    raise SystemExit('Unexpected config fields; no identities, secrets, or strategies accepted')
if c.get('maintenance_margin') is not None:
    raise SystemExit('Maintenance margin must remain unset')
for key, prefix in [('admin', 'G'), ('oracle', 'C')]:
    if not re.fullmatch(prefix + '[A-Z2-7]{55}', c[key]):
        raise SystemExit(f'Invalid public {key}')
if set(c['markets']) != {'XLM', 'USDC'}:
    raise SystemExit('Exactly XLM and USDC required')
assets = {'XLM': 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',
          'USDC': 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75'}
actions = [{'action': 'deploy_fresh', 'name': 'controller', 'package': 'simple-peridottroller'},
           {'target': 'controller', 'method': 'initialize', 'args': {'admin': c['admin']}},
           {'target': 'controller', 'method': 'set_oracle', 'args': {'oracle': c['oracle']}}]
for name, m in c['markets'].items():
    fields = {'cf_scaled', 'supply_cap', 'borrow_cap', 'target_cash_bps', 'min_cash_reserve_bps', 'rate_model'}
    if set(m) != fields:
        raise SystemExit(f'{name}: explicit CF, positive caps, cash parameters and rate model required')
    if any(type(m[k]) is not int or m[k] <= 0 for k in fields - {'rate_model'}):
        raise SystemExit(f'{name}: zero means uncapped, not disabled; all parameters must be positive integers')
    if m['cf_scaled'] > 900_000 or not 0 < m['min_cash_reserve_bps'] <= m['target_cash_bps'] <= 10_000:
        raise SystemExit(f'{name}: invalid cash/CF bounds')
    actions.extend([
        {'action': 'deploy_fresh', 'name': name + '_rate', 'package': 'jump-rate-model', 'parameters': m['rate_model']},
        {'action': 'deploy_fresh', 'name': name, 'package': 'receipt-vault'},
        {'target': name, 'method': 'initialize', 'args': {'token_address': assets[name], 'supply_yearly_rate_scaled': 0, 'borrow_yearly_rate_scaled': 0, 'admin': c['admin']}},
        {'target': name, 'method': 'set_peridottroller', 'args': {'peridottroller': '$controller'}},
        {'target': name, 'method': 'set_interest_model', 'args': {'model': '$' + name + '_rate'}},
        {'target': 'controller', 'method': 'add_market', 'args': {'market': '$' + name}},
        {'target': 'controller', 'method': 'set_market_cf', 'args': {'market': '$' + name, 'cf_scaled': m['cf_scaled']}},
        {'target': name, 'method': 'set_collateral_factor', 'args': {'new_factor_scaled': m['cf_scaled']}},
        {'target': name, 'method': 'set_supply_cap', 'args': {'cap': m['supply_cap']}},
        {'target': name, 'method': 'set_borrow_cap', 'args': {'cap': m['borrow_cap']}},
        {'target': name, 'method': 'set_idle_cash_buffer_bps', 'args': {'admin': c['admin'], 'idle_cash_buffer_bps': m['target_cash_bps']}},
        {'target': name, 'method': 'set_min_cash_reserve_bps', 'args': {'admin': c['admin'], 'bps': m['min_cash_reserve_bps']}, 'status': 'MISSING_ABI'},
        {'target': name, 'verify': {'get_boosted_vault': None, 'get_margin_controller': None, 'get_collateral_factor': m['cf_scaled']}}
    ])
print(json.dumps({'status': 'BLOCKED', 'reason': 'ReceiptVault has no minimum-cash-reserve ABI; initialization/rate-model arguments and all resource gates require validation before an executable deployer is enabled',
                  'maintenance_margin': None, 'strategies': [], 'actions': actions}, indent=2))
sys.exit(2)
