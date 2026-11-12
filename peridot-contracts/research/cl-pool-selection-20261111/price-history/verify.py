#!/usr/bin/env python3
import hashlib,json
from pathlib import Path
P=Path(__file__).resolve().parent
for r in json.loads((P/'sources.json').read_text()):
 b=(P/r['file']).read_bytes();assert len(b)==r['bytes'] and hashlib.sha256(b).hexdigest()==r['sha256'],r['file']
for r in json.loads((P/'manifest.json').read_text())['files']:
 b=(P/r['file']).read_bytes();assert len(b)==r['bytes'] and hashlib.sha256(b).hexdigest()==r['sha256'],r['file']
x=json.loads((P/'analysis.json').read_text())
for pair,v in x.items():
 assert v['daily_buckets']==(70 if pair=='USDT0-USDC' else 90)
 ts=[int(r['timestamp'])for r in v['days']];assert all(b-a==86400000 for a,b in zip(ts,ts[1:])),pair
assert x['EURC-USDC']['ecb_comparison']['matched_business_days']==64
v={r['label']:r for r in json.loads((P/'oracles.json').read_text())['views']}
for key in ['external-PYUSD','external-sUSD','external-USDT0','dex-sUSD']:assert v[key]['value'] is None
for key in ['external-USDC','external-EURC','dex-PYUSD','dex-USDT0','dex-EURC','forex-EUR']:assert int(v[key]['value']['price'])>0
print('PASS public response/file hashes, observed history coverage and exact oracle absence/presence. No financial/resource/depeg-depth qualification.')
