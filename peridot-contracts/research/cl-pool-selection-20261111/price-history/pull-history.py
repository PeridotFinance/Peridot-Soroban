#!/usr/bin/env python3
# PUBLIC reads only. Snapshot date explicit; no signing/submission.
import subprocess,urllib.parse,datetime,json
from pathlib import Path
P=Path(__file__).resolve().parent
end=datetime.datetime(2026,11,11,0,0,tzinfo=datetime.timezone.utc);start=end-datetime.timedelta(days=90)
assets={'sUSD':'GCHW7CWI7GMIYQYFXMFJNJX5645XGWIINIAEQK3SABQO6CAYL5T7JYIH','PYUSD':'GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5','USDC':'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN','USDT0':'GATISXX6BZ6NC7IKQBY37CJD4SOZL3CYZJWXEDG6JVIY4WBS6KXJHN6Q','EURC':'GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2'}
for a,b in [('sUSD','PYUSD'),('sUSD','USDC'),('PYUSD','USDC'),('EURC','USDC'),('USDT0','USDC')]:
 q={'base_asset_type':'credit_alphanum4'if len(a)<=4 else'credit_alphanum12','base_asset_code':a,'base_asset_issuer':assets[a],'counter_asset_type':'credit_alphanum4'if len(b)<=4 else'credit_alphanum12','counter_asset_code':b,'counter_asset_issuer':assets[b],'start_time':int(start.timestamp()*1000),'end_time':int(end.timestamp()*1000),'resolution':86400000,'order':'asc','limit':200}
 subprocess.run(['python3',str(P/'fetch.py'),f'horizon-{a}-{b}-90complete','https://horizon.stellar.org/trade_aggregations?'+urllib.parse.urlencode(q)],check=True)
