#!/usr/bin/env python3
"""Static conditional sensitivity, never a realized-return backtest."""
import json,hashlib,datetime
from pathlib import Path
HERE=Path(__file__).resolve().parent;P=HERE.parent/'pool-data'
read=lambda n:json.loads((P/n).read_text())
data=read('calculations.json');quotes=read('quotes.json');allentry=read('conversion-quotes.json')['rows']+read('entry-retries.json')['rows']
key=lambda q:(q['pool'],q['range'],q['capital_usd'],q['deposited_leg'])
entries={key(q):q for q in allentry if q['direction']=='entry' and 'output'in q}
assert len(entries)==60
rows=[];year=365/7;warm_xlm=.5744714;warm=warm_xlm*data['xlm_price_usdc']
for p in data['pools']:
 for pos in p['positions']:
  for leg in [0,1]:
   e=entries[(p['address'],pos['range'],pos['capital_usd'],leg)];prices=e['prices'];amounts=list(map(int,pos['actual_quoted_amounts']));have=[0,0];have[leg]=int(e['keep_quantity']);have[1-leg]=int(e['output'])
   # Bounded notional liquidity under the original quote; post-swap pool state
   # remains unexecuted, so this is NOT a sequential investment quote.
   fraction=min(1.,*(have[i]/amounts[i] for i in [0,1] if amounts[i]>0));liq=pos['liquidity']*fraction
   fweight=liq/(p['raw_active_liquidity']+liq);rweight=liq/(p['working_liquidity']+liq)
   bw=2.5*liq/(p['working_liquidity']+2.5*liq)
   fees=p['week_lp_fee_estimate_historical_usd']*fweight
   aqua=p['aqua_per_second']*604800*rweight; aqua_usd=aqua*data['aqua_price_usdc'];boost=p['aqua_per_second']*604800*bw*data['aqua_price_usdc']
   capital=pos['capital_usd'];after=sum(have[i]/1e7*prices[i]for i in [0,1]);entry=capital-after;cost=entry+warm
   net=(fees+aqua_usd)*year-cost
   row={'pool':p['address'],'pair':p['pair'],'range':pos['range'],'capital_usd':capital,'deposited_leg':leg,'hold_benchmark':p['pair'][leg],'prices_usdc':prices,'entry_quote_ledger':e['ledger'],'entry_marked_cost_usd':entry,'warm_lifecycle_xlm_anchor':warm_xlm,'warm_lifecycle_usd_anchor':warm,'original_liquidity':pos['liquidity'],'notional_liquidity_fraction_after_quote':fraction,'weekly_fee_current_weight_sensitivity_usd':fees,'weekly_aqua_tokens_conditional':aqua,'weekly_aqua_usd_conditional':aqua_usd,'weekly_maxboost_aqua_usd_conditional':boost,'annual_net_constant_marks_renewed_rewards_usd':net,'annual_net_percent':100*net/capital,'annual_fee_only_net_usd':fees*year-cost,'annual_maxboost_net_usd':(fees+boost)*year-cost,'annual_50pct_retention_net_usd':.5*(fees+aqua_usd)*year-cost,'blend_3_07pct_annual_usd':capital*.0307,'annual_difference_vs_blend_usd':net-capital*.0307,'realized_net_yield_usd':None,'realized_arbitrage_loss_usd':None}
   rows.append(row)
out={'scope':'CONDITIONAL STATIC-MARK SENSITIVITY, not realized net return, safe capacity or production fee quote','assumptions':['constant indexed leg marks normalized to USDC1','same historical week repeats and present active liquidity remains','range stays in range; no recenter transactions','emission renewals all year; onlycurrentepochknown','all modeled reward receipts distributable; otherwisefeeonly','zero incremental OPEX, strategy price-path loss and holdassetissuerdistributions','entry and liquidity estimates use independent rolling prestates, not sequentialexecution','old warm lifecycle XLM cost is a comparison anchor, not currentpairWASMqualification'], 'inputs':{n:hashlib.sha256((P/n).read_bytes()).hexdigest()for n in ['calculations.json','quotes.json','conversion-quotes.json','entry-retries.json']},'warm_lifecycle_usd_anchor':warm,'rows':rows}
(HERE/'net-sensitivity.json').write_text(json.dumps(out,indent=2)+'\n')
for p in data['pools']:
 print('\n'+ '/'.join(x.split(':')[0]for x in p['pair']))
 for cap in [1000,10000]:
  for ran in ['full','wide','tight']:
   rs=[r for r in rows if r['pool']==p['address']and r['capital_usd']==cap and r['range']==ran]
   print(cap,ran,'net legs',*[round(r['annual_net_constant_marks_renewed_rewards_usd'],4)for r in rs],'entry',*[round(r['entry_marked_cost_usd'],4)for r in rs])
print('PASS60entryquotes; allrealizednet fields remain null')
