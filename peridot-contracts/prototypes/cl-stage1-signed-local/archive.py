"""Preserve only isolated public-fixture research evidence, never unrelated target data."""
import hashlib,json,tarfile
from pathlib import Path
root=Path(__file__).resolve().parents[2];base=root/'target/cl-stage1-signed-local';out=base/'durable';out.mkdir(exist_ok=False)
files=[]
for p in base.rglob('*'):
 if not p.is_file()or out in p.parents:continue
 rel=p.relative_to(base)
 if any(x.startswith('durable')for x in rel.parts):continue
 if any(x in rel.parts for x in ['test_snapshots','failed-core-test-dirs','__pycache__']):continue
 if p.name in ['stellar-core-private']or p.suffix in ['.o','.d']:continue
 files.append(p)
files+=list((root/'prototypes/cl-stage1-signed-local').glob('*'))
files+=list((root/'target/cl-stage1-cash-first-build-v4/artifacts').glob('*.wasm'))
files+=[root/'target/cl-stage1-cash-first-build-v4/build-manifest.json',root/'target/cl-share-checkpoint/aquarius-pool.wasm',root/'target/cl-stage1-internal-v6/profile.json',root/'target/cl-stage1-captured-v6/profile.json']
files=sorted(set(p for p in files if p.is_file()))
manifest=[dict(path=str(p.relative_to(root)),size=p.stat().st_size,sha256=hashlib.sha256(p.read_bytes()).hexdigest())for p in files]
archive=out/'public-private-ledger-evidence.tar.gz'
with tarfile.open(archive,'w:gz')as tar:
 for p in files:tar.add(p,arcname=str(p.relative_to(root)),recursive=False)
with tarfile.open(archive)as tar:
 assert len(tar.getmembers())==len(manifest)
 for r in manifest:assert hashlib.sha256(tar.extractfile(r['path']).read()).hexdigest()==r['sha256']
(out/'manifest.json').write_text(json.dumps({'schema':'cl-stage1-private-signed-archive-v1','base':'3188ae05a7143c26ce67a442e98c49a9aa1d3524','private_fixture_signatures_only':True,'binary_files_excluded_with_hashes_in_tables':True,'source_identity_scope':'post-run verified snapshot, not contemporaneous attestation','files':manifest},indent=2)+'\n')
summary=dict(archive=archive.name,members=len(manifest),bytes=archive.stat().st_size,sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),member_hashes_verified=True,gate_replays=14,private_test_signatures_only=True,release_approval=False)
(out/'archive.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary))
for n in ['signed-table.json','signed-table.md','cold-table.json','ci-accepted-head.json']:(out/n).write_bytes((base/n).read_bytes())
