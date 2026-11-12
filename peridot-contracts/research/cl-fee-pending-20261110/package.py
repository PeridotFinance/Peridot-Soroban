"""Archive only named part-(a) runs, source and public WASMs; no target sweep."""
import hashlib
import json
import tarfile
from pathlib import Path

root=Path(__file__).resolve().parents[2]
out=root/'target/cl-fee-pending-publish-v7'
out.mkdir(exist_ok=False)
members={}
runs=['cl-fee-pending-v1','cl-fee-pending-v2','cl-fee-pending-v3','cl-fee-pending-final','cl-fee-pending-final-v5','cl-fee-pending-final-v6','cl-fee-pending-final-v7']
for name in runs:
    run=root/'target'/name
    for p in run.rglob('*'):
        if p.is_file(): members['runs/'+name+'/'+str(p.relative_to(run))]=p
research=root/'research/cl-fee-pending-20261110'
for p in research.iterdir():
    if p.is_file():members['research/'+p.name]=p
for p in (root/'target/cl-restoration-combined-build/artifacts').glob('*.wasm'):
    members['wasm/'+p.name]=p
members['wasm/aquarius-pool.wasm']=root/'target/cl-share-checkpoint/aquarius-pool.wasm'
members['default-feature-isolation.log']=root/'target/cl-fee-pending-default-isolation.log'
members['network-config.json']=root.parent/'docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json'
pins=json.loads((root/'target/cl-fee-pending-final-v7/source-before.json').read_text())
for p in members.values():
    if p.suffix=='.wasm':assert hashlib.sha256(p.read_bytes()).hexdigest()==pins[str(p.resolve())]
archive=out/'fee-pending-evidence.tar.xz'
index={}
with tarfile.open(archive,'w:xz') as tar:
    for name,p in sorted(members.items()):
        b=p.read_bytes();index[name]=dict(bytes=len(b),sha256=hashlib.sha256(b).hexdigest())
        tar.add(p,arcname=name,recursive=False)
payload=dict(schema='cl-fee-pending-archive-v1',archive=dict(file=archive.name,bytes=archive.stat().st_size,sha256=hashlib.sha256(archive.read_bytes()).hexdigest()),members=index)
with tarfile.open(archive,'r:xz') as tar:
    assert set(tar.getnames())==set(index)
    for item in tar:
        b=tar.extractfile(item).read();assert len(b)==index[item.name]['bytes'] and hashlib.sha256(b).hexdigest()==index[item.name]['sha256']
(out/'index.json').write_text(json.dumps(payload,indent=2)+'\n')
print(json.dumps(dict(archive=payload['archive'],members=len(index))))
