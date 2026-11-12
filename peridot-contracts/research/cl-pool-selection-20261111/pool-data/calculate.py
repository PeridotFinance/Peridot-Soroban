from pathlib import Path
from decimal import Decimal as D
import json,collections,datetime
P=Path(__file__).resolve().parent
load=lambda n:json.loads((P/n).read_text());tokens=load('all-tokens.json')['items'];token={x['address']:x for x in tokens};usdc=next(x for x in tokens if x['name']=='USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN');price=lambda a:float(token[a]['price_xlm'])/float(usdc['price_xlm']);aqua=next(x for x in tokens if x['name']=='AQUA:GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA');aqua_price=price(aqua['address']);chain=load('getters.json');quotes=load('quotes.json');out={'window':['2026-11-04T00:00:00Z','2026-11-11T00:00:00Z'],'aqua_price_usdc':aqua_price,'xlm_price_usdc':1/float(usdc['price_xlm']),'pools':[]}
for i,pool in enumerate(load('selected-pools.json'),1):
 d=load(f'p{i}-detail.json');stats=load(f'p{i}-stats.json');week=[x for x in stats['items']if '2026-11-04 00:00:00'<=x['datetime_str']<'2026-11-11 00:00:00'];vol=sum(float(x['volume_usd'])/1e7 for x in week);c=chain['pools'][i-1];v={x['label']:x.get('value')for x in c['views']};rew=v['get_rewards_info'];fee=v['get_fee_fraction']/10000;lp=1-v['get_protocol_fee_fraction']/10000
 events=[];pages=[]
 for path in sorted(P.glob(f'p{i}-week-events-[0-9]*.json')):pages.append(load(path));events+=pages[-1]['items']
 assert len(events)==pages[0]['total'];assert len(pages)==pages[0]['pages'];assert all('2026-11-04 00:00:00'<=x['ledger_close_at_str']<='2026-11-11 00:00:00'for x in events)
 inputs=collections.defaultdict(D)
 for e in events:
  if e['event_type']=='swap':
   assert sum(D(x)>0 for x in e['amounts'])==1
   for t,a in zip(e['tokens'],e['amounts']):
    if D(a)>0:inputs[t]+=D(a)/D(10**token[t]['decimals'])
 fees={t:float(a)*fee*lp for t,a in inputs.items()}
 row={'address':pool['address'],'pair':pool['tokens_str'],'type':d['pool_type'],'fee_fraction':fee,'lp_fee_share':lp,'tvl_usd':float(d['liquidity_usd'])/1e7,'week_volume_historical_indexed_usd':vol,'week_lp_fee_estimate_historical_usd':vol*fee*lp,'week_event_counts':dict(collections.Counter(e['event_type']for e in events)),'week_swap_input_tokens':{k:str(x)for k,x in inputs.items()},'week_lp_fee_tokens_estimate':fees,'week_lp_fee_marked_now_usd_estimate':sum(x*price(t)for t,x in fees.items()),'history_stats_first':stats['items'][-1]['datetime_str'],'history_stats_last':stats['items'][0]['datetime_str'],'ui_fee_apy_percent':float(d['apy'])*100,'ui_aqua_apy_percent':float(d['rewards_apy'])*100,'ui_total_apy_percent':float(d['total_apy'])*100,'aqua_per_second':int(rew['tps'])/1e7,'aqua_schedule_end':int(rew['exp_at']),'raw_active_liquidity':int(v['get_active_liquidity']),'working_liquidity':int(rew['working_supply']),'wasm':c['instance']['entries'][0]['wasmHash'],'positions':[]}
 for q in quotes['rows']:
  if q['pool']!=pool['address']:continue
  if 'value'not in q:row['positions'].append(q);continue
  amounts,L=q['value'];L=int(L);weight=L/(row['raw_active_liquidity']+L);rw=L/(row['working_liquidity']+L);bw=2.5*L/(row['working_liquidity']+2.5*L);aqua_base=row['aqua_per_second']*604800*rw;aqua_boost=row['aqua_per_second']*604800*bw;usd_deployed=sum(int(a)/1e7*price(t)for t,a in zip(d['tokens_addresses'],amounts));row['positions'].append({'range':q['range'],'capital_usd':q['capital_usd'],'tick_lower':q['tick_lower'],'tick_upper':q['tick_upper'],'liquidity':L,'actual_quoted_amounts':amounts,'actual_deployed_usd_at_snapshot':usd_deployed,'current_fee_weight_after_dilution':weight,'current_reward_weight_after_dilution':rw,'conditional_week_fees_using_current_weight_usd':row['week_lp_fee_estimate_historical_usd']*weight,'conditional_week_aqua_tokens':aqua_base,'conditional_week_aqua_usd':aqua_base*aqua_price,'conditional_week_aqua_maxboost_tokens':aqua_boost,'conditional_week_aqua_maxboost_usd':aqua_boost*aqua_price,'realized_position_week_fees_usd':None,'historical_time_in_range':None,'realized_arbitrage_loss_usd':None})
 out['pools'].append(row)
(P/'calculations.json').write_text(json.dumps(out,indent=2)+'\n')
for p in out['pools']:
 print(p['pair'],p['week_event_counts'],p['week_volume_historical_indexed_usd'],p['week_lp_fee_estimate_historical_usd'])
 for x in p['positions']:
  if x['capital_usd']==1000:print(x['range'],x['liquidity'],x['current_fee_weight_after_dilution'],x['conditional_week_fees_using_current_weight_usd'],x['conditional_week_aqua_usd'],x['conditional_week_aqua_maxboost_usd'])
