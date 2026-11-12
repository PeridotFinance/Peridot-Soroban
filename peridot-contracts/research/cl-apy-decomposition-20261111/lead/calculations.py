"""Offline conditional continuation; never a realized position backtest."""
import json
from pathlib import Path
P = Path(__file__).resolve().parents[1]
old = P.parent / 'cl-pool-selection-20261111'
read = lambda p: json.loads(p.read_text())
data = read(old / 'pool-data/calculations.json')
entries = read(old / 'lead/net-sensitivity.json')['rows']
locks = {x['pool']: x for x in read(P / 'apy-boost/boost-math.json')}
sales = read(P / 'sale-fx/calculated.json')
quotes = {x['label']: float(x['output_USDC']) for x in sales['quotes']}
budget = read(P / 'sale-fx/budget-reads.json')['views']
signed = read(P / 'lead/signed-table.json')['rows']
warm_fees = {i:sum(x['application']['fee_charged'] for x in signed if x['pair']==pair and x['root'] not in ['stage_withdraw','stage_close_convert'])/1e7 for i,pair in [(1,'pyusd-usdc'),(4,'usdc-eurc')]}
print('Budget quote labels:', [(x['label'], x.get('value')) for x in budget])
historic = {1: sales['historical_sales'][0], 4: sales['historical_sales'][2], 5: sales['historical_sales'][1]}
rows = []
for i in [1,4,5]:
    pool = data['pools'][i-1]
    lock = next(x for x in locks[i]['locks'] if x['multiplier'] == 7)
    L0 = float(locks[i]['L10k'])
    for mode in ['fees_only', 'base', 'boost_including_lock']:
        lp = float(lock['budgetLP']) if mode.startswith('boost') else 10000.
        L = L0 * lp / 10000
        weight = L / (pool['raw_active_liquidity'] + L)
        weekly_fee = pool['week_lp_fee_estimate_historical_usd'] * weight
        aqua = 0 if mode == 'fees_only' else pool['aqua_per_second'] * 604800 * ((2.5 if mode.startswith('boost') else 1)*L) / (pool['working_liquidity'] + (2.5 if mode.startswith('boost') else 1)*L)
        # Historical executed third-party sale leg price is a valuation benchmark,
        # not a claim that our weekly amount was sold at this price.
        sale_price = float(historic[i]['realized_leg_USDC_per_AQUA'])
        annual_gross = (weekly_fee + aqua * sale_price) * 365/7
        nets = []
        for leg in [0,1]:
            e = next(x for x in entries if x['pool'] == pool['address'] and x['range'] == 'wide' and x['capital_usd'] == 10000 and x['deposited_leg'] == leg)
            # For the smaller boosted LP sleeve, prorated old quote is expressly
            # a sensitivity; not a new executable entry or lock acquisition quote.
            warm = warm_fees.get(i,e['warm_lifecycle_xlm_anchor'])*data['xlm_price_usdc']
            cost = e['entry_marked_cost_usd'] * lp/10000 + warm
            fraction = e['notional_liquidity_fraction_after_quote']
            actual_L = L*fraction
            fee = pool['week_lp_fee_estimate_historical_usd']*actual_L/(pool['raw_active_liquidity']+actual_L)
            reward_weight = actual_L*(2.5 if mode.startswith('boost') else 1)
            reward = 0 if mode == 'fees_only' else pool['aqua_per_second']*604800*reward_weight/(pool['working_liquidity']+reward_weight)
            net = (fee+reward*sale_price)*365/7-cost
            nets.append({'deposited_leg':leg,'hold_benchmark':e['hold_benchmark'],'entry_cost_usd_sensitivity':e['entry_marked_cost_usd']*lp/10000,'effective_liquidity':actual_L,'weekly_aqua':reward,'annual_conditional_net_before_unmeasured_costs_usd':net,'percent_total_10000':net/100})
        row = {'pool':i,'pair':pool['pair'],'mode':mode,'total_capital':10000,'LP_capital':lp,'locked_capital':10000-lp,'weekly_fees_before_entry_scaling':weekly_fee,'weekly_AQUA_before_entry_scaling':aqua,'historical_sale_price':sale_price,'historical_sale_transaction':historic[i]['transaction_hash'],'annual_gross_before_entry_scaling':annual_gross,'warm_lifecycle_xlm_anchor':warm_fees.get(i,e['warm_lifecycle_xlm_anchor']),'warm_scope':'sum of independently applied private current-code roots' if i in warm_fees else 'accepted old-code anchor; USDT0 not remeasured','nets':nets,'realized_net':None}
        rows.append(row)
        print(i, mode, 'LP/lock',round(lp,2),round(10000-lp,2),'weekly fee/aqua',round(weekly_fee,6),round(aqua,5),'annual nets',*[round(x['annual_conditional_net_before_unmeasured_costs_usd'],4)for x in nets])
out = {'scope':'conditional constant-mark continuation, not realized net yield or deployable boosted vault','annualizer':365/7,'income_price':'observed historic third-party AQUA sale-leg price; not our execution','missing_costs':['weekly claim/sale transaction fees','ICE acquisition slippage and lock transaction fees','recurring keeper/restoration/hosting','new-WASM lifecycle fee difference','price-path losses and mark changes'],'annual_net_formula':'tabulated net - (365/7)*weekly claim/sale cost - other missing costs; locked AQUA terminal mark change separately','rows':rows}
(P/'lead/calculations.json').write_text(json.dumps(out,indent=2)+'\n')
assert len(rows)==9
assert all(abs(r['LP_capital']+r['locked_capital']-10000)<1e-8 for r in rows)
assert all(r['realized_net'] is None for r in rows)
