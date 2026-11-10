"""Preserve only explicitly named public prototype artifacts; never sweep target."""
import hashlib,json,tarfile,argparse
from pathlib import Path
root=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args();out=a.output.resolve();out.mkdir(exist_ok=False)
members={}
runs=['cl-stage1-first-roundtrip','cl-stage1-roundtrip-v2','cl-stage1-roundtrip-v3','cl-stage1-internal-v3','cl-stage1-internal-v4','cl-stage1-internal-v5','cl-stage1-captured-v5','cl-stage1-internal-v6','cl-stage1-captured-v6']
for name in runs:
 d=root/'target'/name
 for f in d.rglob('*'):
  if f.is_file():members['runs/'+name+'/'+str(f.relative_to(d))]=f
for v in range(1,5):
 name=f'cl-stage1-cash-first-build-v{v}';d=root/'target'/name
 for rel in ['build-manifest.json','logs','artifacts','harness','workspace/contracts/cl-share-vault/src','workspace/contracts/receipt-vault/src']:
  q=d/rel
  for f in ([q] if q.is_file() else q.rglob('*')):
   if f.is_file():members['builds/'+name+'/'+str(f.relative_to(d))]=f
for f in (root/'prototypes/cl-stage1-cash-first').rglob('*'):
 if f.is_file() and '__pycache__' not in str(f):members['prototype/'+str(f.relative_to(root/'prototypes/cl-stage1-cash-first'))]=f
for name in ['prompt.txt','status.json','internal-review.md']:
 members['review/'+name]=root/'target/cl-stage1-fable-review'/name
members['wasm/aquarius-pool.wasm']=root/'target/cl-share-checkpoint/aquarius-pool.wasm'
members['network-config.json']=root.parent/'docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json'
for v in [3,4]:
 d=root/'target'/f'cl-stage1-cash-first-build-v{v}'
 for n,pin in json.loads((d/'build-manifest.json').read_text())['wasm'].items():assert hashlib.sha256((d/'artifacts'/n).read_bytes()).hexdigest()==pin['sha256']
archive=out/'stage1-staged-evidence.tar.xz';index={}
with tarfile.open(archive,'w:xz') as tar:
 for name,f in sorted(members.items()):
  b=f.read_bytes();index[name]={'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()};tar.add(f,arcname=name,recursive=False)
manifest={'schema':'cl-stage1-staged-evidence-v1','archive':{'file':archive.name,'bytes':archive.stat().st_size,'sha256':hashlib.sha256(archive.read_bytes()).hexdigest()},'members':index}
with tarfile.open(archive,'r:xz') as tar:
 assert set(tar.getnames())==set(index)
 for f in tar:
  b=tar.extractfile(f).read();assert len(b)==index[f.name]['bytes'] and hashlib.sha256(b).hexdigest()==index[f.name]['sha256']
(out/'index.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps({'archive':manifest['archive'],'members':len(index)}))
