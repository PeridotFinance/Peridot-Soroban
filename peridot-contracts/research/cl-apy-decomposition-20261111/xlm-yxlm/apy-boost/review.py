"""Bounded independent offline check; no network or financial execution."""
from pathlib import Path
from decimal import Decimal as D
import json,hashlib,datetime,re
P=Path(__file__).resolve().parent;lead=P.parent/'lead';old=P.parents[1]/'lead'
read=lambda p:json.loads(p.read_text())
j=read(lead/'calculations.json');q=read(lead/'quotes.json');v={x['label']:x for x in q['views']};stats=read(lead/'stats.json');events=read(lead/'events-1.json');assert stats['next'] is None and events['next'] is None
week=[x for x in stats['items']if '2026-11-04 00:00:00'<=x['datetime_str']<'2026-11-11 00:00:00'];vol=sum(D(x['volume_usd'])/10**7 for x in week)
assert D(j['volume_usdc_historical'])==vol
assert D(j['week_aggregate_fee_estimate'])==vol*D('.001')*D('.5')
assert len(events['items'])==291 and all(x['ledger_close_at_str']<'2026-11-11 00:00:00'for x in events['items'])
L=D(j['L10k']);A=D(j['A']);W=D(j['W']);tps=D(j['tps']);price=D(j['historical_sale_benchmark']['realized_leg_USDC_per_AQUA']);prices=list(map(lambda x:D(str(x)),q['prices']));lpfee=D(j['week_aggregate_fee_estimate']);warm=D('.5744714')*prices[0]
for row in j['rows']:
 capital=D(row['LP_capital']);assert capital+D(row['lock_capital'])==10000
 boost=D('2.5')if row['mode']=='boost_including_lock'else D(1)
 for n in row['nets']:
  leg=n['leg'];e=v['entry_'+str(leg)];assert D(e['initial'])==D(e['keep'])+D(e['input'])
  have=[D(0),D(0)];have[leg]=D(e['keep']);have[1-leg]=D(e['value']);desired=list(map(D,v['deposit']['value'][0]));f=min(D(1),*(have[k]/desired[k]for k in [0,1]));l=L*capital/10000*f
  fee=lpfee*l/(A+l);aqua=D(0)if row['mode']=='fees_only'else tps*604800*boost*l/(W+boost*l);cost=(10000-sum(have[k]/10**7*prices[k]for k in[0,1]))*capital/10000;net=(fee+aqua*price)*365/7-cost-warm
  for key,val in [('entry_liquidity_fraction',f),('weekly_fee_usdc_sensitivity',fee),('weekly_aqua',aqua),('entry_marked_cost_usdc',cost),('annual_net_excluding_issuer_distributions',net)]:assert abs(D(n[key])-val)<D('1e-20'),key
  assert D(n['annual_net_vs_hold_with_advertised_1_6pct_coupon'])==net-(160 if leg==1 else 0)
  assert n['coupon_credited_to_vault']==0
report=(lead/'report.md').read_text();previous=read(old/'calculations.json')['rows'];newrows=[x for x in report.splitlines()if re.match(r'^\|(PYUSD/USDC|USDC/EURC|USDT0/USDC|XLM/yXLM)\|',x)]
assert len(newrows)==12
for i,oldrow in enumerate(previous):
 cols=newrows[i].split('|');expected=[oldrow['LP_capital'],oldrow['locked_capital'],*[n['annual_conditional_net_before_unmeasured_costs_usd']for n in oldrow['nets']]]
 actual=re.findall(r'[−+]?\$[\d,]+\.\d\d|[−+]?\$[\d,]+',cols[3]+' '+cols[4]+' '+cols[5]);actual=[float(x.replace('−','-').replace('$','').replace(',',''))for x in actual]
 assert actual==[round(x,2)for x in expected],(i,actual,expected)
for i,row in enumerate(j['rows']):
 cols=newrows[i+9].split('|');expected=[D(row['LP_capital']),D(row['lock_capital']),D(row['nets'][0]['annual_net_excluding_issuer_distributions']),D(row['nets'][1]['annual_net_vs_hold_with_advertised_1_6pct_coupon'])];actual=re.findall(r'[−+]?\$[\d,]+\.\d\d|[−+]?\$[\d,]+',cols[3]+' '+cols[4]+' '+cols[5]);actual=[D(x.replace('−','-').replace('$','').replace(',',''))for x in actual];assert actual==[x.quantize(D('.01'))for x in expected]
paths=[lead/'calculate.py',lead/'calculations.json',lead/'report.md',lead/'quotes.json',lead/'stats.json',lead/'events-1.json',old/'calculations.json',P/'boost-math.json']
out={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'bounded independent offline economics review only','result':'PASS','checks':['7-day indexed volume and50%ordinary LP share recomputed','all6directional results independently reconstructed using entry mixture shortfall, entrant dilution and warm anchor','all3total capital sums equal10000; boost lockinsidebudget with7xendyearprefunding','160conditional holding coupon subtracted only foryXLMbenchmark; no CLcoupon credited','prior9displayed net/capitalrows match preserved calculations to2dp; sixth3rows match new arithmetic','no signed XLM/yXLM fixture, realized position return or guaranteed coupon inferred'],'files':[{'file':str(x.relative_to(P.parents[1])),'sha256':hashlib.sha256(x.read_bytes()).hexdigest()}for x in paths],'limitations':['UIfee annualization window unknown','new native pair lifecycle costs use old anchor','AQUArewardannualrenewal and staticweights conditional','vaultICEeligibility and issuercouponpayment/entitlement unproven','ICEacquisition and weeklyclaim/salecosts missing']}
(P/'independent-review.json').write_text(json.dumps(out,indent=2)+'\n');print('PASS')
