import json,pathlib,decimal
from decimal import Decimal as D
P=pathlib.Path(__file__).resolve().parent
I=D('61627749070.9458557');price=D('0.00024442824');B=D('10000')
Ls={1:D('522251742503'),4:D('490359531124'),5:D('523157127173')};W={1:D('2575853465725930'),4:D('21840517812463'),5:D('84808049467833')};out=[]
for i in [1,4,5]:
 d=json.loads((P/f'p{i}-detail-live.json').read_text());T=D(d['total_share']);L=Ls[i];tps=D(d['reward_tps'])/10**7
 def row(m):
  ice=I*L/T;aqua=ice/D(m);cost=aqua*price;lp=B/(1+cost/B);lc=L*lp/B;icer=I*lc/T;ar=icer/D(m)
  reward=tps*604800*(D('2.5')*lc)/(W[i]+D('2.5')*lc)
  return dict(multiplier=m,ICEfor10kLP=str(ice),AQUAfor10kLP=str(aqua),ICEcapitalFor10kLP=str(cost),budgetLP=str(lp),budgetICE=str(icer),budgetAQUAlock=str(ar),budgetICEcapital=str(B-lp),budgetWeeklyRewardAQUA=str(reward),budgetWeeklyRewardRaw7=int(reward*10**7))
 out.append(dict(pool=i,T=str(T),L10k=str(L),workingSupply=str(W[i]),ICEtotal=str(I),mark=str(price),newICEentersDenominator=True,locks=[row(10),row(7),row(4)]))
(P/'boost-math.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps(out,indent=2))
