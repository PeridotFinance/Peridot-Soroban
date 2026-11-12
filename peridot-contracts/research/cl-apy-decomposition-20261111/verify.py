"""Offline evidence identity/arithmetic checks, not financial qualification."""
from pathlib import Path
import hashlib, json, tarfile
P=Path(__file__).resolve().parent
manifest=json.loads((P/'manifest.json').read_text())
for row in manifest['files']:
    f=P/row['path']
    assert f.is_file(), f
    assert hashlib.sha256(f.read_bytes()).hexdigest()==row['sha256'], f
    assert f.stat().st_size==row['bytes'], f
rows=json.loads((P/'lead/calculations.json').read_text())['rows']
assert len(rows)==9
for r in rows:
    assert abs(r['LP_capital']+r['locked_capital']-10000)<1e-8
    assert r['realized_net'] is None
    for n in r['nets']:
        assert abs(n['percent_total_10000']*100-n['annual_conditional_net_before_unmeasured_costs_usd'])<1e-8
for i in [1,4,5]:
    ui=json.loads((P/f'apy-boost/p{i}-detail-live.json').read_text())
    assert ui['pool_type']=='concentrated'
    assert abs(float(ui['apy'])+float(ui['rewards_apy'])+float(ui['incentive_apy'])-float(ui['total_apy']))<2e-7
evidence=P/'execution-evidence'
e=json.loads((evidence/'manifest.json').read_text())
archive=evidence/e['archive']
assert hashlib.sha256(archive.read_bytes()).hexdigest()==e['archive_sha256']
expected={x['path']:x for x in e['files']}
with tarfile.open(archive,'r:xz') as t:
    assert set(t.getnames())==set(expected)
    for member in t:
        assert member.isfile() and not member.name.startswith('/') and '..' not in Path(member.name).parts
        raw=t.extractfile(member).read();x=expected[member.name]
        assert len(raw)==x['bytes'] and hashlib.sha256(raw).hexdigest()==x['sha256'], member.name
signed=json.loads((P/'lead/signed-table.json').read_text())
assert len(signed['rows'])==18 and not signed['release_approval']
assert all(x['pass'] and x['gate_summary']=={'invocation':True,'lifecycle':False,'transaction':False,'checkpoint':False} for x in signed['rows'])
six=json.loads((P/'xlm-yxlm/lead/calculations.json').read_text())
assert six['event_counts']['swap']==235 and len(six['rows'])==3
for row in six['rows']:
    assert abs(float(row['LP_capital'])+float(row['lock_capital'])-10000)<1e-8
    assert row['realized_net'] is None
    a=row['nets'][1]
    assert abs(float(a['annual_net_excluding_issuer_distributions'])-160-float(a['annual_net_vs_hold_with_advertised_1_6pct_coupon']))<1e-8
    assert all(n['coupon_credited_to_vault']==0 for n in row['nets'])
print(f"PASS {len(manifest['files'])} file identities, {len(expected)} archive members, 12 conditional capital rows, 18 preserved private applications; sixth pool remains unmeasured; not a release gate")
