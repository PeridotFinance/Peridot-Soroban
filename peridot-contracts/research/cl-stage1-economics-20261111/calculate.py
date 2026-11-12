#!/usr/bin/env python3
"""Offline conditional economics from captured public observations. No network or keys."""
import json, pathlib, math, hashlib, datetime
P=pathlib.Path(__file__).resolve().parent
r=json.loads((P/'chain-read.json').read_text()); pool=json.loads((P/'pool-detail.raw').read_text()); tokens=json.loads((P/'tokens.raw').read_text())['items']; views={v['label']:v['value'] for v in r['views'] if 'value' in v}
def token(address):return next(x for x in tokens if x['address']==address)
USDC=token(r['assetIDs']['USDC']);PYUSD=token(r['assetIDs']['PYUSD']); AQUA=next(x for x in tokens if x['code']=='AQUA' and x['issuer']=='GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA')
# USD unit assumes USDC redemption parity; venue-index mark, not an oracle validation.
xlm_usd=1/float(USDC['price_xlm']);aqua_usd=float(AQUA['price_xlm'])*xlm_usd
W=int(views['get_rewards_info']['working_supply']);Lactive=int(views['get_active_liquidity']);sec_year=365*86400
reward_year=int(views['get_rewards_info']['tps'])/1e7*sec_year*aqua_usd
lp_fee_day=float(pool['volume_usd'])/1e7*views['get_fee_fraction']/10000*(1-views['get_protocol_fee_fraction']/10000)
journeys={'in_kind':0.5744714,'converted':0.5394184}
rows=[]
for capital in [1000,10000]:
 for name in ['full','wide']:
  amounts,L=views[f'{name}-{capital}'];L=int(L); rw=L/(W+L); fw=L/(Lactive+L)
  gross_rewards=reward_year*rw; gross_fees=lp_fee_day*365*fw
  conversion_in=capital/2-int(views[f'convert-PYUSD-half-{capital}'])/1e7
  conversion_out=capital/2-int(views[f'convert-USDC-half-{capital}'])/1e7
  row={'capital_nominal_usd':capital,'range':name,'raw_liquidity':L,'desired_units':[capital/2,capital/2],'consumed_units':[int(x)/1e7 for x in amounts], 'unspent_units':[capital/2-int(x)/1e7 for x in amounts], 'reward_weight_share_no_ice':rw,'current_active_fee_share':fw,'annualized_conditional_aqua_usd':gross_rewards,'annualized_conditional_lp_fee_usd':gross_fees,'annualized_conditional_gross_usd':gross_rewards+gross_fees,'conditional_gross_apr_percent':100*(gross_rewards+gross_fees)/capital,'entry_conversion_par_loss_usd':conversion_in,'optional_exit_quote_par_loss_usd':conversion_out,'outcomes':[]}
  for days in [30,365]:
   for journey,xlm in journeys.items():
    costs=conversion_in+xlm*xlm_usd+(conversion_out if journey=='converted' else 0)
    earned=(gross_rewards+gross_fees)*days/365
    for retained in [0,.5]:
     # No credit for quarantine; conditional row assumes all modeled earned amounts distributable.
     net=earned*(1-retained)-costs
     row['outcomes'].append({'days':days,'journey':journey,'retained_income_fraction':retained,'gross_earned_usd':earned,'warm_fee_xlm':xlm,'warm_fee_usd':xlm*xlm_usd,'conversion_cost_usd':costs-xlm*xlm_usd,'net_vs_par_hold_pyusd_usd':net,'simple_horizon_return_percent':100*net/capital,'zero_distributable_rewards_net_usd':gross_fees*days/365*(1-retained)-costs})
  rows.append(row)
# Exact integer current Blend rate formula from official Reserve.setRates; no transaction,
# no supply-cap/withdrawal execution approval, no projected compounding asserted as realized.
Q=10**7;R=10**12
ceil=lambda a,b:(a+b-1)//b
lending=[]
for id in ['CBAKAZUEJBAFA2DCGBRS2WKJT3FNIMJBHTFL76AY7XMAVEINQNNIQWAO','CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD']:
 es=[e for e in r['entries'] if e['contract']==id];inst=dict(es[0]['entries'][0]['value']['storage']);c=es[1]['entries'][0]['value'];d={k:int(v) for k,v in es[2]['entries'][0]['value'].items()};bs=inst['Config']['bstop_rate']
 supplied=d['b_supply']*d['b_rate']//R;borrowed=ceil(d['d_supply']*d['d_rate'],R);u=min(Q,ceil(borrowed*Q,supplied));t=c['util']
 if u<=t: rate=ceil((ceil(ceil(u*Q,t)*c['r_one'],Q)+c['r_base'])*d['ir_mod'],Q)
 elif u<=9500000: rate=ceil((ceil(ceil((u-t)*Q,9500000-t)*c['r_two'],Q)+c['r_one']+c['r_base'])*d['ir_mod'],Q)
 else: rate=ceil(ceil((u-9500000)*Q,500000)*c['r_three'],Q)+ceil((c['r_two']+c['r_one']+c['r_base'])*d['ir_mod'],Q)
 supply_rate=rate*((Q-bs)*u//Q)//Q
 lending.append({'pool':id,'name':inst['Name'],'ledger':es[0]['ledger'],'pool_status':inst['Config']['status'],'reserve_enabled':c['enabled'],'supplied_usdc':supplied/Q,'borrowed_usdc':borrowed/Q,'utilization':u/Q,'reserve_last_time':d['last_time'],'backstop_take_rate':bs/Q,'supply_apr_percent':100*supply_rate/Q,'sdk_estimated_weekly_compounded_supply_apy_percent':100*((1+supply_rate/Q/52)**52-1),'annual_supply_interest_per_1000_if_constant':1000*supply_rate/Q,'annual_supply_interest_per_10000_if_constant':10000*supply_rate/Q,'limitation':'snapshot marginal rate before new supply; no incentives, own transaction costs or risk-adjustment; reserve rate model as captured (not guaranteed future income)'})
# Path illustration only, no LVR subtraction: compare terminal portfolio to all PYUSD
# and to initial equal-token mix; flat initial peg, no fees, no management.
losses=[]
for range_name in ['full','wide']:
 for py,us in [(1,.99),(.99,1)]:
  a,b=(0,float('inf')) if range_name=='full' else (1.0001**-1000,1.0001**1000)
  L=1000/((1-1/b)+(1-a));s=math.sqrt(py/us);x=L*(1/s-1/b);y=L*(s-a);nav=x*py+y*us;mix=500*py+500*us;hold=1000*py
  losses.append({'capital':1000,'range':range_name,'pyusd_usd':py,'usdc_usd':us,'portfolio_terminal_usd':nav,'hold_pyusd_terminal_usd':hold,'net_vs_hold_pyusd_usd':nav-hold,'initial_mix_terminal_usd':mix,'additional_lp_shortfall_vs_mix_usd':mix-nav,'underlying_loss_vs_start_usd':1000-mix})
output={'scope':'CONDITIONAL PLANNING, not realized return, deployable yield, safe capacity or funding guarantee','ledger_range':[min(v['ledger'] for v in r['views']),max(v['ledger'] for v in r['views'])],'xlm_usd_from_indexed_usdc_par':xlm_usd,'aqua_usd_from_indexed_usdc_par':aqua_usd,'pyusd_usd_from_indexed_usdc_par':float(PYUSD['price_xlm'])*xlm_usd,'pool_tvl_usd':float(pool['liquidity_usd'])/1e7,'pool_24h_volume_usd':float(pool['volume_usd'])/1e7,'emissions_aqua_per_second':int(views['get_rewards_info']['tps'])/1e7,'emission_expiration_utc':datetime.datetime.fromtimestamp(int(views['get_rewards_info']['exp_at']),datetime.timezone.utc).isoformat(),'emission_remaining_seconds_at_ledger':int(views['get_rewards_info']['exp_at'])-int(r['latest']['closeTime']),'reward_weight_denominator':W,'active_fee_liquidity':Lactive,'pool_aqua_usd_per_day':reward_year/365,'pool_lp_fee_usd_per_day':lp_fee_day,'rows':rows,'lending':lending,'one_percent_depeg_analytic_illustrations':losses}
marked=json.loads((P/'marked-quotes.json').read_text()); mv={v['label']:v for v in marked['rows']}; mark=marked['pyusdUsdIndexedMark']; marked_rows=[]
for capital in [1000,10000]:
 for name in ['full','wide']:
  amounts,L=mv[f'marked-{name}-{capital}']['value'];L=int(L); rw=L/(W+L);fw=L/(Lactive+L)
  rewards=reward_year*rw;fees=lp_fee_day*365*fw;gross=rewards+fees
  bought=int(mv[f'marked-in-{capital}']['value'])/1e7;returned=int(mv[f'marked-out-{capital}']['value'])/1e7
  entry_cost=capital/2-bought;exit_cost=bought-returned*mark
  row={'capital_usd':capital,'range':name,'raw_liquidity':L,'reward_share_no_ice':rw,'active_fee_share':fw,'conditional_gross_rewards_year_usd':rewards,'conditional_gross_lp_fees_year_usd':fees,'conditional_gross_apr_percent':100*gross/capital,'entry_conversion_marked_loss_usd':entry_cost,'optional_exit_conversion_marked_loss_usd':exit_cost,'outcomes':[]}
  for days in [30,365]:
   for journey,xlm in journeys.items():
    costs=entry_cost+xlm*xlm_usd+(exit_cost if journey=='converted' else 0)
    for retained in [0,.5]:
     net=gross*days/365*(1-retained)-costs
     row['outcomes'].append({'days':days,'journey':journey,'retention':retained,'net_vs_marked_hold_pyusd_usd':net,'horizon_return_percent':100*net/capital,'zero_distributable_rewards_net_usd':fees*days/365*(1-retained)-costs,'zero_income_total_cost_usd':costs})
  marked_rows.append(row)
output['marked_rows']=marked_rows
print('MARKED TABLE')
for row in marked_rows:
 print(row['capital_usd'],row['range'],row['conditional_gross_apr_percent'],row['entry_conversion_marked_loss_usd'])
 for o in row['outcomes']:
  if not o['retention']:print(o['days'],o['journey'],o['net_vs_marked_hold_pyusd_usd'])
(P/'calculations.json').write_text(json.dumps(output,indent=2)+'\n')
print('prices',xlm_usd,aqua_usd,'pool',output['pool_tvl_usd'],output['pool_aqua_usd_per_day'])
for a in rows:
 print(a['capital_nominal_usd'],a['range'],'gross APR',a['conditional_gross_apr_percent'],'rewards/y',a['annualized_conditional_aqua_usd'],'entry',a['entry_conversion_par_loss_usd'])
 for o in a['outcomes']:
  if not o['retained_income_fraction']:print(' ',o['days'],o['journey'],o['net_vs_par_hold_pyusd_usd'],o['simple_horizon_return_percent'])
print('LENDING',json.dumps(lending,indent=2));print('LOSSES',losses)
# Internal arithmetic checks on evidence, not a financial qualification test.
assert r['assetIDs']['PYUSD']==views['get_tokens'][0] and r['assetIDs']['USDC']==views['get_tokens'][1]
assert all(x['conditional_gross_apr_percent']<min(y['supply_apr_percent'] for y in lending) for x in rows)
assert all(o['net_vs_par_hold_pyusd_usd']<0 for x in rows for o in x['outcomes'])
print('PASS: identity derivation, dilution arithmetic and underperformance assertions')
