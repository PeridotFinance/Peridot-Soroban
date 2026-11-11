#!/usr/bin/env python3
"""Observed history/analytic sensitivity, not Aquarius position or execution replay."""
from pathlib import Path
import json,math,datetime,xml.etree.ElementTree as ET
P=Path(__file__).resolve().parent
utc=lambda x:datetime.datetime.fromtimestamp(int(x)/1000,datetime.timezone.utc).date().isoformat()
def impact(r,capital=1000):
 # Relative risky-leg price r, counter-leg1. Start50/50 marked value at r=1.
 a=1.0001**-2000;b=1.0001**2000;L=capital/(2*(1-1/math.sqrt(b)))
 s=math.sqrt(max(a,min(r,b)));x=L*(1/s-1/math.sqrt(b));y=L*(s-math.sqrt(a));v=x*r+y
 def out(v):return {'value':v,'vs_hold_counter':v-capital,'vs_hold_base':v-capital*r,'vs_initial_50_50_hold':v-capital*(1+r)/2}
 return {'relative_price':r,'capital':capital,'full':out(capital*math.sqrt(r)),'wide':out(v),'wide_bounds':[a,b],'scope':'terminal constant-product analytic sensitivity, no fees, no path/LVR/exit execution; not replay of actual CL pool'}
def compact(r):return {k:r[k]for k in['timestamp','trade_count','base_volume','counter_volume','avg','high','low','close']}|{'date':utc(r['timestamp'])}
results={}
for f in P.glob('horizon-*-90complete.raw'):
 pair=f.name[len('horizon-'):-len('-90complete.raw')];rows=json.loads(f.read_text())['_embedded']['records']
 lo=min(rows,key=lambda r:float(r['avg']));hi=max(rows,key=lambda r:float(r['avg']));rawlo=min(rows,key=lambda r:float(r['low']));rawhi=max(rows,key=lambda r:float(r['high']))
 results[pair]={'window_start':'2026-08-13','window_end_exclusive':'2026-11-11','daily_buckets':len(rows),'first_date':utc(rows[0]['timestamp']),'last_date':utc(rows[-1]['timestamp']),'daily_vwap_min':compact(lo),'daily_vwap_max':compact(hi),'raw_trade_min':compact(rawlo),'raw_trade_max':compact(rawhi),'total_base_volume':sum(float(r['base_volume'])for r in rows),'days':[{**compact(r),'deviation_from_1_pct':100*(float(r['avg'])-1)}for r in rows]}
 if pair!='EURC-USDC':results[pair]['analytical_vwap_downside']=impact(float(lo['avg']));results[pair]['analytic_raw_low_not_depth_qualified']=impact(float(rawlo['low']))
 if pair=='USDT0-USDC':results[pair]['daily_max_sustained_depeg_status']='UNPROVEN: launch outliers require trade-size and independent-reference validation; do not infer redeemable price or attainable depth'
fx={}
for n in ET.fromstring((P/'ecb-90d.raw').read_text()).iter():
 if 'time'in n.attrib:
  for c in n:
   if c.attrib.get('currency')=='USD':fx[n.attrib['time']]=float(c.attrib['rate'])
matched=[]
for r in results['EURC-USDC']['days']:
 if r['date']in fx:matched.append({'date':r['date'],'eurc_usdc_daily_vwap':float(r['avg']),'ecb_eur_usd_reference':fx[r['date']],'relative_mark':float(r['avg'])/fx[r['date']],'deviation_pct':100*(float(r['avg'])/fx[r['date']]-1)})
elo=min(matched,key=lambda x:x['relative_mark']);ehi=max(matched,key=lambda x:x['relative_mark'])
results['EURC-USDC']['ecb_comparison']={'matched_business_days':len(matched),'min':elo,'max':ehi,'daily_comparison':matched,'limitations':'daily DEX VWAP vs ECB reference fixed at different intraday time; USDC assumedUSD1; not synchronized oracle depeg or executabledepth','analytic_min_relative_mark':impact(elo['relative_mark'])}
first=fx[min(fx)];results['EURC-USDC']['fx_path']={'initial_ecb_eur_usd':first,'min':min(fx.values()),'max':max(fx.values()),'end':fx[max(fx)],'end_vs_start':impact(fx[max(fx)]/first),'scope':'ordinary EUR/USD currency movement, not EURC depeg'}
(P/'analysis.json').write_text(json.dumps(results,indent=2)+'\n')
for pair,r in results.items():
 print(pair,r['daily_buckets'],'VWAP',r['daily_vwap_min']['avg'],r['daily_vwap_max']['avg'],'raw',r['raw_trade_min']['low'],r['raw_trade_max']['high'])
 if 'analytical_vwap_downside'in r:print(r['analytical_vwap_downside'])
print('EURC ECB',elo,ehi,'matched',len(matched))
print('USDT0max',results['USDT0-USDC']['daily_vwap_max'])
