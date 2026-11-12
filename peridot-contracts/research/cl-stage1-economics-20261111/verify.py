#!/usr/bin/env python3
"""Verify this captured public-only evidence without network access."""
import hashlib,json,pathlib
P=pathlib.Path(__file__).resolve().parent
sources=json.loads((P/'sources.json').read_text())
for s in sources:
 b=(P/s['file']).read_bytes();assert hashlib.sha256(b).hexdigest()==s['sha256'],s['file']
 if 'bytes' in s:assert len(b)==s['bytes'],s['file']
manifest=json.loads((P/'manifest.json').read_text())
for m in manifest['files']:
 b=(P/m['file']).read_bytes();assert len(b)==m['bytes'] and hashlib.sha256(b).hexdigest()==m['sha256'],m['file']
assert sorted(x['file'] for x in manifest['files'])==sorted(x.name for x in P.iterdir() if x.is_file() and x.name!='manifest.json')
r=json.loads((P/'chain-read.json').read_text());v={x['label']:x['value'] for x in r['views'] if 'value' in x}
assert r['network']['passphrase']=='Public Global Stellar Network ; September 2015'
assert list(r['assetIDs'].values())==v['get_tokens']
assert r['assetIDs']['PYUSD'] in (P/'paxos-sac.raw').read_text()
assert 'GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5' in (P/'paxos-issuer.raw').read_text()
assert 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN' in (P/'circle-addresses.raw').read_text()
c=json.loads((P/'calculations.json').read_text());assert len(c['marked_rows'])==4 and all(len(x['outcomes'])==8 for x in c['marked_rows'])
assert all(x['conditional_gross_apr_percent']<min(l['supply_apr_percent'] for l in c['lending']) for x in c['marked_rows'])
assert all(o['zero_distributable_rewards_net_usd']<0 for x in c['marked_rows'] for o in x['outcomes'])
print(f'PASS: {len(sources)} public source responses, {len(manifest["files"])} evidence members, exact identities, outcome matrix and lending-underperformance comparison. This is arithmetic/evidence validation, not a lifecycle or release gate.')
