"""Sixth pool, same conditional method; no realized position return implied."""
from pathlib import Path
from decimal import Decimal as D
import collections,json
P=Path(__file__).resolve().parent
read=lambda p:json.loads(p.read_text())
detail=read(P/'../apy-boost/detail.json')
chain=read(P/'../apy-boost/getters.json')['pools'][0]
v={r['method']:r.get('value')for r in chain['views']}
q=read(P/'quotes.json');views={x['label']:x for x in q['views']}
stats=read(P/'stats.json');assert stats['next'] is None
week=[x for x in stats['items']if '2026-11-04 00:00:00'<=x['datetime_str']<'2026-11-11 00:00:00']
volume=sum(D(x['volume_usd'])/D(10**7)for x in week)
events=read(P/'events-1.json');assert events['next'] is None and len(events['items'])==events['total']==291
assert all('2026-11-04 00:00:00'<=x['ledger_close_at_str']<='2026-11-11 00:00:00'for x in events['items'])
fee=D(v['get_fee_fraction'])/10000;lp_share=1-D(v['get_protocol_fee_fraction'])/10000
aggregate_fees=volume*fee*lp_share
L=D(views['deposit']['value'][1]);A=D(v['get_active_liquidity']);W=D(v['get_rewards_info']['working_supply']);tps=D(v['get_rewards_info']['tps'])/10**7
locks=read(P/'../apy-boost/boost-math.json');lock=next(x for x in locks['locks']if x['multiplier']==7)
historic=read(P/'../../sale-fx/calculated.json')['historical_sales'][0]
sale_price=D(historic['realized_leg_USDC_per_AQUA'])
prices=list(map(lambda x:D(str(x)),q['prices']));warm=D('0.5744714')*prices[0]
rows=[]
for mode in ['fees_only','base','boost_including_lock']:
    capital=D(lock['budgetLP']) if mode.startswith('boost') else D(10000)
    boost=D('2.5')if mode.startswith('boost')else D(1)
    nets=[]
    for leg in [0,1]:
        e=views['entry_'+str(leg)];have=[D(0),D(0)]
        have[leg]=D(e['keep']);have[1-leg]=D(e['value'])
        amount=list(map(D,views['deposit']['value'][0]))
        fraction=min(D(1),*(have[i]/amount[i]for i in [0,1]))
        actual_L=L*capital/10000*fraction
        income_fee=aggregate_fees*actual_L/(A+actual_L)
        income_aqua=D(0)if mode=='fees_only'else tps*604800*boost*actual_L/(W+boost*actual_L)
        entry_cost=(D(10000)-sum(have[i]/10**7*prices[i]for i in [0,1]))*capital/10000
        net=(income_fee+income_aqua*sale_price)*D(365)/7-entry_cost-warm
        nets.append({'leg':leg,'holding':'XLM'if leg==0 else 'yXLM','entry_marked_cost_usdc':str(entry_cost),'entry_liquidity_fraction':str(fraction),'weekly_fee_usdc_sensitivity':str(income_fee),'weekly_aqua':str(income_aqua),'annual_net_excluding_issuer_distributions':str(net),'annual_net_vs_hold_with_advertised_1_6pct_coupon':str(net-(D(160)if leg==1 else 0)),'coupon_credited_to_vault':0,'warm_cost_usdc_old_fixture_anchor':str(warm)})
    rows.append({'mode':mode,'total_capital':10000,'LP_capital':str(capital),'lock_capital':str(10000-capital),'nets':nets,'realized_net':None})
out={'scope':'conditional static-mark continuation; no native-XLM signed fixture/realized CL issuer coupon/finite historical position return','pool':detail['address'],'window':['2026-11-04T00:00:00Z','2026-11-11T00:00:00Z'],'stats_buckets':len(week),'event_counts':dict(collections.Counter(x['event_type']for x in events['items'])),'volume_usdc_historical':str(volume),'LP_fee_share':str(lp_share),'week_aggregate_fee_estimate':str(aggregate_fees),'annual_week_fee_over_current_TVL_percent':str(aggregate_fees*D(365)/7/(D(detail['liquidity_usd'])/10**7)*100),'range':q['range'],'L10k':str(L),'A':str(A),'W':str(W),'tps':str(tps),'schedule_end':v['get_rewards_info']['exp_at'],'historical_sale_benchmark':historic,'rows':rows}
(P/'calculations.json').write_text(json.dumps(out,indent=2)+'\n')
print('volume/fees',volume,aggregate_fees,'poolannual',out['annual_week_fee_over_current_TVL_percent'])
for r in rows:print(r['mode'],r['LP_capital'],r['lock_capital'],*[round(float(x['annual_net_excluding_issuer_distributions']),6)for x in r['nets']], 'coupon benchmark',r['nets'][1]['annual_net_vs_hold_with_advertised_1_6pct_coupon'])
