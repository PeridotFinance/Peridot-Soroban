#!/usr/bin/env python3
from pathlib import Path
import hashlib,json,sys,subprocess
P=Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
for s in json.loads((P/'sources.json').read_text()):
 p=P/s['file'];assert p.stat().st_size==s['bytes'] and sha(p)==s['sha256']
for fn in ['reads.json','base-quote.json','boost-quote.json']:
 r=json.loads((P/fn).read_text());assert r['network']['passphrase']=='Public Global Stellar Network ; September 2015'
 assert not r.get('error') and all(not x.get('error') for x in r['views'])
h=json.loads((P/'historical-reference.json').read_text());assert sha(P/'historical-small-operations.json')==h['sha256']==h['receipt']['sha256']
before=(P/'calculated.json').read_bytes();subprocess.run([sys.executable,str(P/'calculate.py')],check=True);assert before==(P/'calculated.json').read_bytes()
files=sorted(p for p in P.iterdir() if p.is_file() and p.name!='manifest.json')
if '--seal' in sys.argv:(P/'manifest.json').write_text(json.dumps({'scope':'bounded public read-only yXLM terms/oracle/coupon benchmark; unsigned sale estimates, reused public third-party historical leg; no signing/submission','files':[{'file':p.name,'bytes':p.stat().st_size,'sha256':sha(p)} for p in files]},indent=2)+'\n')
m=json.loads((P/'manifest.json').read_text());assert {r['file']for r in m['files']}=={p.name for p in files}
for f in m['files']:
 p=P/f['file'];assert p.stat().st_size==f['bytes'] and sha(p)==f['sha256']
print(f"PASS: {len(m['files'])} identities, HTTP receipts, RPC results, reused historical sale reference and arithmetic")
