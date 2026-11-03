import hashlib,io,json,lzma,re,subprocess,sys,tarfile,zipfile
from pathlib import Path,PurePosixPath
ROOT=Path(__file__).resolve().parent
REPO=ROOT.parents[1]
COMMIT='627e2e2c926b2f62dbb1b1cd0ef6f9949caf2d6d';MERGE='1728feeebe092280ffc650c11b8cda1b2e4df3d6'
sha=lambda b:hashlib.sha256(b).hexdigest()
def source(p):return subprocess.check_output(['git','show',COMMIT+':'+p],cwd=REPO)
def dump(p,x):p.write_text(json.dumps(x,indent=2)+'\n')
def pack(kind,arts):
 out=ROOT/(kind+'-627e2e2');out.mkdir(exist_ok=False);members={};zips={};sources=set()
 for a in arts:
  p=ROOT/f"artifact-{a['id']}.zip";assert p.stat().st_size==a['size_in_bytes'];assert sha(p.read_bytes())==a['digest'].split(':')[1]
  z=zipfile.ZipFile(p);zips[a['id']]=z
  for name in z.namelist():
   n=PurePosixPath(name);assert not n.is_absolute() and '..' not in n.parts and not name.endswith('/')
   archive_name=('artifact/' if kind=='linux' else 'artifact-'+str(a['id'])+'/')+name
   assert archive_name not in members
   data=z.read(name);members[archive_name]={'sha256':sha(data),'bytes':len(data),'artifact_id':a['id'],'original_member':name}
  if kind=='storage':
   prof='internal-policy' if 'internal-policy' in a['name'] else 'captured-network-diagnostic';prefix='cl-storage-ci-'+prof+'/'
   index=json.loads(z.read(prefix+'artifact-index.json'))
   for n,v in index['files'].items():
    b=z.read(prefix+n);assert sha(b)==v['sha256'] and len(b)==v['bytes'],n
   before=json.loads(z.read(prefix+'source-before.json'));after=json.loads(z.read(prefix+'source-after.json'));assert before==after
   for n,h in before.items():assert sha(source(n))==h,(n,h);sources.add(n)
   for n in z.namelist():
    if n.endswith('build-manifest.json'):
     m=json.loads(z.read(n));base=n.rsplit('/',1)[0]
     for filename,v in m['wasm'].items():
      b=z.read(base+'/artifacts/'+filename);assert sha(b)==v['sha256'] and len(b)==v['bytes']
     for filename,h in m['baseline_source_hashes'].items():
      name='peridot-contracts/'+filename;assert sha(source(name))==h; sources.add(name)
 if kind=='linux':
  mapping=json.loads(source('peridot-contracts/config/cl-proof-map.json'))
  sources.update('peridot-contracts/'+n for n in set(mapping['sources'].values())|{'config/cl-proof-map.json','bots/margin-liquidator/package.json','bots/margin-liquidator/package-lock.json'})
  sources.update('peridot-contracts/scripts/'+n for n in ['check_budget.sh','check_local_budget.py','cl_resource_profiles.py','cl_fee_estimate.py','cl_proof_evidence.py','cl_transaction_evidence.py','decode_cl_transaction_evidence.cjs'])
 else:
  sources.update('peridot-contracts/scripts/'+n for n in ['check_budget.sh','check_local_budget.py','cl_resource_profiles.py','cl_fee_estimate.py','cl_proof_evidence.py','cl_transaction_evidence.py','decode_cl_transaction_evidence.cjs'])
  sources.update('peridot-contracts/bots/margin-liquidator/'+n for n in ['package.json','package-lock.json'])
 sourcebytes={}
 for name in sorted(sources):
  b=source(name);path='source/'+(name[len('peridot-contracts/'):] if kind=='linux' else name);sourcebytes[path]=b;members[path]={'sha256':sha(b),'bytes':len(b),'commit':COMMIT,'source_path':name}
 archive=out/('linux-ci-payload.tar.xz' if kind=='linux' else 'storage-ci-payload.tar.xz')
 with lzma.open(archive,'wb',preset=3) as compressed:
  with tarfile.open(fileobj=compressed,mode='w|') as tar:
   for name,v in sorted(members.items()):
    data=sourcebytes[name] if name in sourcebytes else zips[v['artifact_id']].read(v['original_member'])
    assert len(data)==v['bytes'] and sha(data)==v['sha256']
    info=tarfile.TarInfo(name);info.size=len(data);info.mode=0o644;info.uid=info.gid=info.mtime=0;tar.addfile(info,io.BytesIO(data))
 seen=set()
 with tarfile.open(archive,'r:xz') as tar:
  for info in tar:
   assert info.name not in seen and info.isfile();seen.add(info.name);data=tar.extractfile(info).read();v=members[info.name];assert len(data)==v['bytes'] and sha(data)==v['sha256']
   if info.name not in sourcebytes:assert data==zips[v['artifact_id']].read(v['original_member'])
 assert seen==set(members)
 idx={'schema':'cl-linux-ci-archive-v1' if kind=='linux' else 'cl-storage-ci-archive-v1','commit':COMMIT,'merge_commit':MERGE,'run_url':'https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/'+str(41613837844 if kind=='linux' else 41613837952),'artifacts':arts,'archive_sha256':sha(archive.read_bytes()),'archive_bytes':archive.stat().st_size,'archive_file':archive.name,'payload_members':members,'wasm':{n:v for n,v in members.items() if n.endswith('.wasm')},'release_approval':False,'replay_performed':False,'verified_exact_members':True,'scope':'Preserved completed CI evidence; per-profile gates/errors checked separately; no signed/live-state/Core application proof.'}
 if kind=='linux':
  a=arts[0];idx.update(artifact_id=a['id'],original_expiry=a['expires_at'],original_zip_sha256=a['digest'].split(':')[1]);z=zips[a['id']];idx['profiles']={n:json.loads(z.read('cl-ci-checkpoint/'+n+'-checkpoint-summary.json')) for n in ['internal-policy','captured-network-diagnostic']};idx['linux_wasm']=idx['wasm']
 else:
  idx['profiles']={}
  for a in arts:
   prof='internal-policy' if 'internal-policy' in a['name'] else 'captured-network-diagnostic';idx['profiles'][prof]=json.loads(zips[a['id']].read('cl-storage-ci-'+prof+'/qualification-summary.json'))
 dump(out/'index.json',idx)
 print(kind,json.dumps({'archive_sha256':idx['archive_sha256'],'archive_bytes':idx['archive_bytes'],'members':len(members)}),flush=True)
for kind,file in [('linux','github-0.json'),('storage','github-1.json')]:pack(kind,json.loads((ROOT/file).read_text())['artifacts'])
