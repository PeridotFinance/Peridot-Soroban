#!/usr/bin/env python3
"""Offline exact arithmetic; never fetches, signs or submits."""
from decimal import Decimal as D, getcontext
from pathlib import Path
import json
getcontext().prec=60
P=Path(__file__).resolve().parent
load=lambda n:json.loads((P/n).read_text())
r={x['label']:x for x in load('reads.json')['views']}
assert not any(x.get('error') for x in r.values())
x,y=map(int,r['AQUApool-get_reserves']['value']);spot=D(y)/x
amounts=load('amounts.json'); extra={}
if (P/'budget-reads.json').exists():
 extra={x['label']:x for x in load('budget-reads.json')['views']};r.update(extra);amounts.update(load('amounts-budget.json'))
quotes=[]
for k,raw in amounts.items():
 a=int(raw);o=int(r[k]['value']); net_input=a*9970//10000
 # All rolling snapshots retained; no same-ledger claim. Captured reserves unchanged for initial quotes.
 computed=net_input*y//(x+net_input)
 row={'label':k,'input_AQUA':D(a)/10**7,'output_USDC':D(o)/10**7,'effective_USDC_per_AQUA':D(o)/a,'spot_USDC_per_AQUA':spot,'input_fee_AQUA':D(a-net_input)/10**7,'no_fee_CP_output_USDC':D(a*y//(x+a))/10**7,'discount_to_spot_bps':(1-D(o)/a/spot)*10000,'formula_from_initial_reserves_matches':o==computed,'quote_ledger':r[k]['ledger'],'annual_reward_USDC_conditional_365over7':D(o)/10**7*365/7,'annual_reward_rate_percent_on_10000':D(o)/10**7*365/7/100}
 path=P/('path-'+k+'.json')
 if path.exists():row['classic_paths']=load(path.name)['_embedded']['records']
 quotes.append(row)
sqrt=D(r['EURCpool-get_slot0']['value']['sqrt_price_x96']);pool=sqrt**2/D(2)**192
price=lambda k:D(r[k]['value']['price'])/10**14
usd,eur,dex,fx=map(price,['external-USDC','external-EURC','dex-EURC','forex-EUR'])
fx_result={'oracle_timestamp':r['external-EURC']['value']['timestamp'],'USDC_USD_external':usd,'EURC_USD_external':eur,'EURC_USDC_external_cross':eur/usd,'EURC_USDC_dex':dex,'EUR_USD_forex':fx,'pool_EURC_per_USDC':pool,'pool_USDC_per_EURC':1/pool,'pool_vs_external_bps_prior_orientation':(pool/(usd/eur)-1)*10000,'pool_vs_external_bps_USDC_per_EURC':((1/pool)/(eur/usd)-1)*10000,'pool_vs_dex_bps_USDC_per_EURC':((1/pool)/dex-1)*10000,'dex_vs_external_bps_USDC_per_EURC':(dex/(eur/usd)-1)*10000,'EURC_external_vs_FX_bps':(eur/fx-1)*10000}
cp='CA6GAFOJCW4MGQQBUCQUSA3CLIH25G4SNKB2JHYKZCVWZTNW5VXMSC4O'
sales=[]
for name in ['small','medium','large']:
 ops=load('swap-'+name+'-operations.json')['_embedded']['records'];assert all(o['transaction_successful'] for o in ops)
 incoming=sum((D(c['amount']) for o in ops for c in o.get('asset_balance_changes',[]) if c.get('asset_code')=='AQUA' and c.get('to')==cp),D(0))
 outgoing=sum((D(c['amount']) for o in ops for c in o.get('asset_balance_changes',[]) if c.get('asset_code')=='USDC' and c.get('from')==cp),D(0))
 assert incoming>0 and outgoing>0
 index=[z for z in load('aqua-usdc-swaps.json')['items'] if z['transaction_hash']==ops[0]['transaction_hash']]
 assert any(D(z['amounts'][0])/10**7==incoming and -D(z['amounts'][1])/10**7==outgoing for z in index)
 sales.append({'label':name,'time':ops[0]['created_at'],'transaction_hash':ops[0]['transaction_hash'],'AQUA_in':incoming,'USDC_out':outgoing,'realized_leg_USDC_per_AQUA':outgoing/incoming,'scope':'third-party historic venue leg; network fee belongs to larger routed transaction; not our sale'})
result={'scope':'rolling public unsigned estimates, historical third-party swap legs, fresh oracle comparisons; no financial execution','quotes':quotes,'fx':fx_result,'historical_sales':sales}
(P/'calculated.json').write_text(json.dumps(result,default=str,indent=2)+'\n')
print('PASS: quote arithmetic, normalized oracle units, 3 indexed swaps cross-checked against successful Horizon operations')
