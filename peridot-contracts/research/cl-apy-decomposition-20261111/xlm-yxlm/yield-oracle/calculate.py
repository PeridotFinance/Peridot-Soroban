#!/usr/bin/env python3
from pathlib import Path
from decimal import Decimal as D,getcontext
import json
getcontext().prec=60
P=Path(__file__).resolve().parent
r={x['label']:x for x in json.loads((P/'reads.json').read_text())['views']}
assert all(not x.get('error') for x in r.values())
q=D(r['pool-get_slot0']['value']['sqrt_price_x96'])**2/D(2)**192
p=D(r['external-XLM']['value']['price'])/10**14
assert r['external-yXLM']['value'] is None and r['dex-yXLM']['value'] is None
assert json.loads((P/'sep24-withdraw-fee.json').read_text())['fee']==.5
assert json.loads((P/'sep24-deposit-fee.json').read_text())['fee']==0
x={'pool_yXLM_per_XLM':q,'pool_XLM_per_yXLM':1/q,'pool_yXLM_discount_vs_promised_par_XLM_bps':(1/q-1)*10000,'primary_XLM_USD':p,'primary_USDC_USD':D(r['external-USDC']['value']['price'])/10**14,'DEX_XLM_USDC':D(r['dex-XLM']['value']['price'])/10**14,'primary_yXLM':None,'nominal_redemption_price_not_oracle':1,'withdraw_fee_yXLM':'.5','withdraw_fee_USD_at_nominal_par_and_current_XLM_price':p/2,'publisher_current_yXLM_APY':'.016','daily_equivalent_rate_if_annual_compounded_APY':D('1.016')**(D(1)/365)-1,'holding_benchmarks_at_constant_XLM_USD_no_depeg_no_fees':{str(n):{'hold_XLM_coupon_USD':0,'hold_yXLM_publisher_conditional_coupon_USD':D(n)*D('.016'),'illustrative_half_yXLM_LP_coupon_only_if_entitlement_verified_USD':D(n)*D('.008'),'LP_coupon_booked_without_eligibility_evidence_USD':0} for n in [1000,10000]},'scope':'publisher coupon / nominal redemption terms; no measured coupon receipt, no reserve audit, no guaranteed year, no executable redemption or CL yield attribution proof'}

quotes=[]
for qf,af in [('base-quote.json','amounts-base.json'),('boost-quote.json','amounts-boost.json')]:
 amounts=json.loads((P/af).read_text());rows=json.loads((P/qf).read_text())['views']
 for row in rows:
  assert not row.get('error');a=D(amounts[row['label']])/10**7;o=D(row['value'])/10**7
  quotes.append({'label':row['label'],'AQUA_in':a,'USDC_out_quote':o,'effective_USDC_per_AQUA':o/a,'annual_USDC_conditional_365over7':o*365/7,'ledger':row['ledger']})
ops=json.loads((P/'historical-small-operations.json').read_text())['_embedded']['records']
cp='CA6GAFOJCW4MGQQBUCQUSA3CLIH25G4SNKB2JHYKZCVWZTNW5VXMSC4O'
assert all(o['transaction_successful'] for o in ops)
a=sum((D(c['amount']) for o in ops for c in o['asset_balance_changes'] if c.get('to')==cp and c.get('asset_code')=='AQUA'),D(0))
b=sum((D(c['amount']) for o in ops for c in o['asset_balance_changes'] if c.get('from')==cp and c.get('asset_code')=='USDC'),D(0))
assert a==D('534.4348539') and b==D('.1296981')
x['sale_quotes']=quotes
x['historical_reference']={'transaction':ops[0]['transaction_hash'],'time':ops[0]['created_at'],'AQUA_in':a,'USDC_out':b,'realized_leg_USDC_per_AQUA':b/a,'scope':'third-party venue leg, not this vault sale, fee of enclosing routed transaction unallocated'}
for q in quotes:q['historical_price_sensitivity_weekly_USDC']=q['AQUA_in']*b/a
(P/'calculated.json').write_text(json.dumps(x,default=str,indent=2)+'\n')
print('PASS: actual CL ratio, absent yXLM feeds, public redemption fee, conditional benchmarks, 3 unsigned AQUA estimates and reused historical sale leg')
