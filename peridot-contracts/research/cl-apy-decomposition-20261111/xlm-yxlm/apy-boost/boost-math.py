import json,pathlib
from decimal import Decimal as D
P=pathlib.Path(__file__).resolve().parent
q=json.loads((P.parent/'lead/quotes.json').read_text());s=json.loads((P/'summary.json').read_text());tokens=json.loads((P.parent/'lead/tokens-live.json').read_text())['items']
a=next(x for x in tokens if x['name']=='AQUA:GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA');u=next(x for x in tokens if x['name']=='USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN');price=D(a['price_xlm'])/D(u['price_xlm']);I=D(s['totalICE_raw7'])/10**7;T=D(s['raw_total']);W=D(s['working_total']);L=D(next(x for x in q['views']if x['label']=='deposit')['value'][1]);tps=D(s['tps_raw7'])/10**7;B=D('10000');out={'pool':s['pool'],'range':q['range'],'AQUA_USDC_mark':str(price),'ICE_total':str(I),'raw_total':str(T),'working_total':str(W),'L10k':str(L),'weekly_base_AQUA':str(tps*604800*L/(W+L)),'weekly_base_AQUA_raw7':int(tps*604800*L/(W+L)*10**7),'locks':[]}
for m in [10,7,4]:
 ice=I*L/T;aqua=ice/D(m);cost=aqua*price;lp=B/(1+cost/B);lc=L*lp/B;icer=I*lc/T;ar=icer/D(m);reward=tps*604800*D('2.5')*lc/(W+D('2.5')*lc)
 out['locks'].append(dict(multiplier=m,ICEfor10kLP=str(ice),AQUAfor10kLP=str(aqua),ICEcapitalFor10kLP=str(cost),budgetLP=str(lp),budgetICErequired=str(icer),budgetAQUAlock=str(ar),budgetICEcapital=str(B-lp),budgetWeeklyRewardAQUA=str(reward),budgetWeeklyRewardRaw7=int(reward*10**7),initialICEfor3yearlock=str(ar*10),end1yearICE=str(ar*7)))
(P/'boost-math.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2))
