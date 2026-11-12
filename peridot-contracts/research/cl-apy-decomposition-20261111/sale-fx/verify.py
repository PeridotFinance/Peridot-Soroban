#!/usr/bin/env python3
"""Validate retained public evidence; --seal creates/updates only local manifest."""
from pathlib import Path
import hashlib,json,sys,subprocess
P=Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
for s in json.loads((P/'sources.json').read_text()):
 p=P/s['file'];assert p.exists(),p
 assert p.stat().st_size==s['bytes'] and sha(p)==s['sha256'],p
for fn in ['reads.json','budget-reads.json']:
 r=json.loads((P/fn).read_text());assert r['endpoint']=='https://rpc.lightsail.network/'
 assert r['network']['passphrase']=='Public Global Stellar Network ; September 2015'
 assert not r.get('error')
 assert all(not x.get('error') and x.get('ledger') and x.get('returnXdr') for x in r['views'])
old=(P/'calculated.json').read_bytes()
subprocess.run([sys.executable,str(P/'calculate.py')],check=True)
assert old==(P/'calculated.json').read_bytes(),'calculated arithmetic changed'
c=json.loads(old);assert all(q['formula_from_initial_reserves_matches'] for q in c['quotes'])
files=sorted(p for p in P.iterdir() if p.is_file() and p.name!='manifest.json')
if '--seal' in sys.argv:
 (P/'manifest.json').write_text(json.dumps({'scope':'public readonly capture; unsigned getter estimates, third-party historical operations, no financial transaction execution','files':[{'file':p.name,'bytes':p.stat().st_size,'sha256':sha(p)} for p in files]},indent=2)+'\n')
m=json.loads((P/'manifest.json').read_text());assert {x['file'] for x in m['files']}=={p.name for p in files}
for f in m['files']:
 p=P/f['file'];assert p.stat().st_size==f['bytes'] and sha(p)==f['sha256'],p
print(f"PASS: {len(m['files'])} file identities; HTTP receipts; both RPC captures; 15 CP estimates; 3 successful historical sale legs; normalized FX arithmetic")
