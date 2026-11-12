from pathlib import Path
import json,decimal,datetime
D=decimal.Decimal;decimal.getcontext().prec=70;P=Path(__file__).resolve().parent
b=json.loads((P/'fee-boundaries.json').read_text())['rows'];quotes=json.loads((P/'quotes.json').read_text())['rows'];pools=json.loads((P/'selected-pools.json').read_text());out=[]
for i in range(1,5):
 rows=[r for r in b if r['pool']==i];first=next(r for r in rows if r['label']=='first-retained'and r['status']=='SUCCESS');last=next(r for r in rows if r['label']=='last'and r['status']=='SUCCESS');a=next(r for r in first['states']if r['type']=='ledgerEntryState');z=next(r for r in reversed(last['states'])if r['type']=='ledgerEntryUpdated');assert a['wasmHash']==z['wasmHash'];delta=[D(z['feeGrowth'+str(t)])-D(a['feeGrowth'+str(t)])for t in [0,1]];assert min(delta)>=0;seconds=(datetime.datetime.fromisoformat(last['event_time'])-datetime.datetime.fromisoformat(first['event_time'])).total_seconds();r={'pool':i,'address':pools[i-1]['address'],'first_full_week_boundary':'NOT_FOUND','first_retained_transaction':first['hash'],'last_transaction':last['hash'],'start':first['event_time'],'end':last['event_time'],'elapsed_days':seconds/86400,'wasm':a['wasmHash'],'fee_growth_delta_x128':[str(x)for x in delta],'fee_density_per_raw_L':[str(x/D(2**128))for x in delta],'reference_full_range':[],'scope':'observed historical global fee-density change over available interval; same-bytecode endpoints, no proof of complete event history/no intervening resets; current-quoted reference L, no counterfactual finite-size price/liquidity impact'}
 for q in quotes:
  if q['pool']!=r['address']or q['range']!='full':continue
  L=D(q['value'][1]);tokens=[x*L/D(2**128)/D(1e7)for x in delta];usd=sum(t*D(str(p))for t,p in zip(tokens,q['prices']));r['reference_full_range'].append({'capital_reference_current_usd':q['capital_usd'],'current_quoted_L':str(L),'interval_fee_tokens':[str(x)for x in tokens],'interval_fee_usd_at_current_marks':float(usd),'seven_day_normalized_fee_usd_sensitivity':float(usd)*7/(seconds/86400)})
 out.append(r)
(P/'fee-density.json').write_text(json.dumps(out,indent=2)+'\n')
for r in out:print(r['pool'],r['start'],r['end'],r['elapsed_days'],r['reference_full_range'])
