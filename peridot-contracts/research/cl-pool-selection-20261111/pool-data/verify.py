from pathlib import Path
import json,hashlib
P=Path(__file__).resolve().parent
for s in json.loads((P/'sources.json').read_text()):
 b=(P/s['file']).read_bytes();assert hashlib.sha256(b).hexdigest()==s['sha256'],s['file']
for s in json.loads((P/'manifest.json').read_text())['files']:
 b=(P/s['file']).read_bytes();assert len(b)==s['bytes']and hashlib.sha256(b).hexdigest()==s['sha256'],s['file']
g=json.loads((P/'getters.json').read_text());assert len(g['pools'])==5
for p in g['pools']:
 v={x['label']:x.get('value')for x in p['views']};assert v['get_protocol_fee_fraction']==5000
 assert all(x.get('value')==7 for x in p['views']if x['label'].endswith('-decimals'))
q=json.loads((P/'quotes.json').read_text())['rows'];assert len(q)==30 and all('value'in x for x in q)
original=json.loads((P/'conversion-quotes.json').read_text())['rows'];retry=json.loads((P/'entry-retries.json').read_text())['rows'];key=lambda x:(x['pool'],x['range'],x['capital_usd'],x['deposited_leg'],x['direction'])
merged={key(x):x for x in original};merged.update({key(x):x for x in retry});entry=[x for x in merged.values()if x['direction']=='entry'];assert len(entry)==60 and all('output'in x for x in entry)
assert len(json.loads((P/'fee-density.json').read_text()))==4
print('PASS:5 identities/fee splits,30 rangequotes,60 merged entryquotes,4 partial historical fee-density windows and file identities; no realized-net/release qualification.')
