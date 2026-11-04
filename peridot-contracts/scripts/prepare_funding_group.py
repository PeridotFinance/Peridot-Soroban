"""Public unsigned cash-only deployment preparation; never signs or invokes RPC.

The plan uses actual implemented ABIs, but cannot activate borrowing. Resource
and fresh deployed-readback evidence remain mandatory external release gates.
"""
import json
import re
import sys
from pathlib import Path

ASSETS = {
    'XLM': 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',
    'USDC': 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75',
}


def prepare(c):
    if set(c) - {'admin', 'oracle', 'markets', 'maintenance_margin'}:
        raise ValueError('Unexpected config fields; no identities, secrets, or strategies accepted')
    if c.get('maintenance_margin') is not None:
        raise ValueError('Maintenance margin must remain unset')
    for key, prefix in [('admin', 'G'), ('oracle', 'C')]:
        if not isinstance(c.get(key), str) or not re.fullmatch(prefix + '[A-Z2-7]{55}', c[key]):
            raise ValueError(f'Invalid public {key}')
    if set(c['markets']) != set(ASSETS):
        raise ValueError('Exactly XLM and USDC required')
    actions = [
        {'action': 'deploy_fresh', 'name': 'controller', 'package': 'simple-peridottroller'},
        {'target': 'controller', 'method': 'initialize', 'args': {'admin': c['admin']}},
        {'target': 'controller', 'method': 'set_oracle', 'args': {'oracle': c['oracle']}},
    ]
    readbacks = [{'target': 'controller', 'method': 'get_oracle', 'args': {}, 'expected': c['oracle']}]
    for name, m in c['markets'].items():
        fields = {'cf_scaled', 'supply_cap', 'borrow_cap', 'target_cash_bps', 'min_cash_reserve_bps', 'rate_model'}
        if set(m) != fields:
            raise ValueError(f'{name}: explicit CF, positive caps, cash parameters and rate model required')
        if any(type(m[k]) is not int or not 0 < m[k] <= 2**128-1 for k in fields - {'rate_model'}):
            raise ValueError(f'{name}: zero means uncapped, not disabled; numeric parameters must be positive u128')
        if m['cf_scaled'] > 900_000 or not 0 < m['min_cash_reserve_bps'] <= m['target_cash_bps'] <= 10_000:
            raise ValueError(f'{name}: invalid cash/CF bounds')
        rates = m['rate_model']
        if not isinstance(rates, dict) or set(rates) != {'base', 'multiplier', 'jump', 'kink'}:
            raise ValueError(f'{name}: explicit rate-model initialize parameters required')
        if any(type(v) is not int or not 0 <= v <= (1_000_000 if k == 'kink' else 10_000_000) for k, v in rates.items()):
            raise ValueError(f'{name}: invalid rate-model parameter')
        actions.extend([
            {'action': 'deploy_fresh', 'name': name + '_rate', 'package': 'jump-rate-model'},
            {'target': name + '_rate', 'method': 'initialize', 'args': dict(rates, admin=c['admin'])},
            {'action': 'deploy_fresh', 'name': name, 'package': 'receipt-vault'},
            {'target': name, 'method': 'initialize', 'args': {'token_address': ASSETS[name], 'supply_yearly_rate_scaled': 0, 'borrow_yearly_rate_scaled': 0, 'admin': c['admin']}},
            # Before enabling rates: opt into durable disabled cash borrowing.
            {'target': name, 'method': 'set_min_cash_reserve_bps', 'args': {'admin': c['admin'], 'bps': m['min_cash_reserve_bps']}},
            {'target': name, 'method': 'set_cash_borrowing_enabled', 'args': {'admin': c['admin'], 'enabled': False}},
            {'target': 'controller', 'method': 'add_market', 'args': {'market': '$' + name}},
            {'target': 'controller', 'method': 'set_pause_borrow', 'args': {'market': '$' + name, 'paused': True}},
            {'target': name, 'method': 'set_peridottroller', 'args': {'peridottroller': '$controller'}},
            {'target': 'controller', 'method': 'set_market_cf', 'args': {'market': '$' + name, 'cf_scaled': m['cf_scaled']}},
            {'target': name, 'method': 'set_collateral_factor', 'args': {'new_factor_scaled': m['cf_scaled']}},
            {'target': name, 'method': 'set_supply_cap', 'args': {'cap': m['supply_cap']}},
            {'target': name, 'method': 'set_borrow_cap', 'args': {'cap': m['borrow_cap']}},
            {'target': name, 'method': 'set_idle_cash_buffer_bps', 'args': {'admin': c['admin'], 'idle_cash_buffer_bps': m['target_cash_bps']}},
            {'target': name, 'method': 'set_interest_model', 'args': {'model': '$' + name + '_rate'}},
        ])
        for method, expected in {
            'get_underlying_token': ASSETS[name], 'get_boosted_vault': None,
            'get_margin_controller': None, 'get_collateral_factor': m['cf_scaled'],
            'get_idle_cash_buffer_bps': m['target_cash_bps'],
            'get_min_cash_reserve_bps': m['min_cash_reserve_bps'],
            'get_cash_borrowing_enabled': False, 'get_borrowable_cash': 0,
        }.items():
            readbacks.append({'target': name, 'method': method, 'args': {}, 'expected': expected})
        readbacks.extend([
            {'target': 'controller', 'method': 'get_market_cf', 'args': {'market': '$'+name}, 'expected': m['cf_scaled']},
            {'target': 'controller', 'method': 'is_borrow_paused', 'args': {'market': '$'+name}, 'expected': True},
        ])
    return {
        'status': 'PREPARED_NOT_DEPLOYED', 'borrowing_enabled': False,
        'network_actions': False, 'maintenance_margin': None, 'strategies': [],
        'release_blockers': [
            'Fresh compiled authorization and complete-open reserve tests must pass',
            'check_budget.sh must pass measured deployment and funding lifecycle flows',
            'Fresh deployment receipt/cap/rate-model storage and ABI readbacks must be independently verified',
            'Controller pause expires; receipt durable cash-borrowing guard must remain false until separately approved activation',
        ],
        'actions': actions, 'required_readbacks': readbacks,
    }


if __name__ == '__main__':
    try:
        result = prepare(json.loads(Path(sys.argv[1]).read_text()))
    except (ValueError, KeyError, TypeError, IndexError) as error:
        raise SystemExit(str(error)) from error
    print(json.dumps(result, indent=2))
