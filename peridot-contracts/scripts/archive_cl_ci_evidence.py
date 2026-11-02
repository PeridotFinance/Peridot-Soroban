"""Losslessly preserve the final Linux CI payload and exact replay source offline."""
import argparse, hashlib, io, json, subprocess, tarfile, zipfile
from pathlib import Path, PurePosixPath
COMMIT = 'da4e1a598b7c652c79745836417d9943e561c6f2'
ZIP_SHA = '9b01db8227a07f0b8a4bc13dfbf9b043002df79f0c6de068d126e3dd74d87d03'
def sha(b): return hashlib.sha256(b).hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--artifact',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    if a.output.exists(): raise ValueError('output must be new')
    if sha(a.artifact.read_bytes()) != ZIP_SHA: raise ValueError('artifact hash mismatch')
    members={}
    with zipfile.ZipFile(a.artifact) as z:
        for info in z.infolist():
            n=PurePosixPath(info.filename)
            if n.is_absolute() or '..' in n.parts or info.is_dir() or 'artifact/'+str(n) in members: raise ValueError('invalid artifact member')
            members['artifact/'+str(n)]=z.read(info)
    def source(path):
        return subprocess.check_output(['git','show',COMMIT+':peridot-contracts/'+path])
    mapping=json.loads(source('config/cl-proof-map.json'))
    files=set(mapping['sources'].values()) | {'config/cl-proof-map.json','bots/margin-liquidator/package.json','bots/margin-liquidator/package-lock.json'}
    files.update('scripts/'+n for n in ['check_budget.sh','check_local_budget.py','cl_resource_profiles.py','cl_fee_estimate.py','cl_proof_evidence.py','cl_transaction_evidence.py','decode_cl_transaction_evidence.cjs'])
    for path in sorted(files): members['source/'+path]=source(path)
    a.output.mkdir(parents=True)
    archive=a.output/'linux-ci-payload.tar.xz'
    with tarfile.open(archive,'w:xz',preset=9) as tar:
        for name,data in sorted(members.items()):
            entry=tarfile.TarInfo(name);entry.size=len(data);entry.mode=0o644;tar.addfile(entry,io.BytesIO(data))
    profiles={n:json.loads(members['artifact/cl-ci-checkpoint/'+n+'-checkpoint-summary.json']) for n in ['internal-policy','captured-network-diagnostic']}
    wasm={n:{'sha256':sha(data),'bytes':len(data)} for n,data in members.items() if n.endswith('.wasm')}
    index={'schema':'cl-linux-ci-archive-v1','commit':COMMIT,'merge_commit':'e42e3067795124f457b43e5e4d0e2893f7d22815','run_url':'https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41434868065','artifact_id':13136354990,'original_expiry':'2026-11-16T11:30:36Z','original_zip_sha256':ZIP_SHA,'archive_sha256':sha(archive.read_bytes()),'archive_bytes':archive.stat().st_size,'payload_members':{n:{'sha256':sha(d),'bytes':len(d)} for n,d in sorted(members.items())},'linux_wasm':wasm,'profiles':profiles,'release_approval':False,'scope':'Historical Linux controlled-state protocol29 evidence; CI execution job failed. Not approval for subsequent source.'}
    (a.output/'index.json').write_text(json.dumps(index,indent=2)+'\n')
    print(json.dumps({'archive_bytes':archive.stat().st_size,'members':len(members),'profiles':{n:v['gates'] for n,v in profiles.items()}}))
if __name__=='__main__':main()
