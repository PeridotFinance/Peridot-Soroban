#!/usr/bin/env python3
"""Rolling captured primary/pool comparison, no policy adoption or execution."""
import json,hashlib
from pathlib import Path
HERE=Path(__file__).resolve().parent;P=HERE.parent
names=['price-history/oracles.json','pool-data/getters.json']
o=json.loads((P/names[0]).read_text());v={r['label']:r.get('value')for r in o['views']};n=lambda k:int(v[k]['price'])/1e14
u=n('external-USDC');ep=n('external-EURC');eur=n('forex-EUR');py=n('dex-PYUSD')*u;ut=n('dex-USDT0')*u
get=json.loads((P/names[1]).read_text());prices=[[py,u],[py,None],[u,None],[u,ep],[ut,u]];rows=[]
for i,x in enumerate(get['pools']):
 vv={r['label']:r.get('value')for r in x['views']};s=int(vv['get_slot0']['sqrt_price_x96']);spot=(s/2**96)**2;a,b=prices[i]
 row={'pool':i+1,'address':x['address'],'spot_token1_per_token0':spot,'oracle_usd_prices':[a,b],'warning_bps_test_candidate':50,'invalid_bps_test_candidate':200}
 if a and b:
  fair=a/b;row.update(oracle_fair_ratio=fair,pool_relative_deviation_bps=abs(spot/fair-1)*10000,reciprocal_deviation_bps=abs(fair/spot-1)*10000,leg0_peg_deviation_bps=abs(a-1)*10000,leg1_peg_deviation_bps=abs(b/(eur if i==3 else 1)-1)*10000)
  mx=max(row[k]for k in ['pool_relative_deviation_bps','reciprocal_deviation_bps','leg0_peg_deviation_bps','leg1_peg_deviation_bps']);row['hypothetical_state']='invalid'if mx>200 else 'pause_new_risk'if mx>50 else 'within_candidate_bounds'
 else:row['hypothetical_state']='invalid_missing_sUSD_primary'
 rows.append(row)
out={'scope':'rolling non-atomic price comparison only; thresholds proposed not adopted; no execution/primary approval','inputs':{n:hashlib.sha256((P/n).read_bytes()).hexdigest()for n in names},'EURUSD':eur,'rows':rows}
(HERE/'current-guard-sensitivity.json').write_text(json.dumps(out,indent=2)+'\n')
print('PASS5capturedcomparisons; no productionguardconfigured')
