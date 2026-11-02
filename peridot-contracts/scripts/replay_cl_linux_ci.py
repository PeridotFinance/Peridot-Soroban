"""Verify/extract durable Linux payload, replay pinned checker, compare each gate/error.

Exit 1 means exact historical failure reproduced; exit 2 means replay mismatch.
Never grants release approval. No network, signing, or contract execution.
"""
import argparse, hashlib, json, subprocess, sys, tarfile
from pathlib import Path
from archive_cl_ci_evidence import COMMIT, ZIP_SHA

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('evidence',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    root=a.evidence.resolve();out=a.output.resolve()
    if out.exists(): raise ValueError('output must be new')
    index=json.loads((root/'index.json').read_text());archive=root/'linux-ci-payload.tar.xz'
    if (index['schema'] != 'cl-linux-ci-archive-v1' or index['commit'] != COMMIT or index['original_zip_sha256'] != ZIP_SHA or index['release_approval'] is not False or set(index['profiles']) != {'internal-policy','captured-network-diagnostic'}): raise ValueError('historical index identity differs')
    if archive.stat().st_size != index['archive_bytes']: raise ValueError('archive size differs')
    if sha(archive)!=index['archive_sha256']: raise ValueError('archive hash differs')
    out.mkdir(parents=True)
    with tarfile.open(archive,'r:xz') as tar:
        entries=tar.getmembers()
        if len({e.name for e in entries})!=len(entries) or set(e.name for e in entries)!=set(index['payload_members']): raise ValueError('archive coverage differs')
        for e in entries:
            dest=(out/e.name).resolve()
            if not dest.is_relative_to(out) or not e.isfile(): raise ValueError('unsafe member')
            data=tar.extractfile(e).read();expected=index['payload_members'][e.name]
            if len(data)!=expected['bytes'] or hashlib.sha256(data).hexdigest()!=expected['sha256']: raise ValueError('member hash differs')
            dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
    source=out/'source';package=out/'evidence';package.mkdir()
    original=out/'artifact/cl-ci-checkpoint'
    for file in original.rglob('*'):
        if file.is_file() and ('artifacts' not in file.relative_to(original).parts):
            dest=package/file.relative_to(original);dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(file.read_bytes())
    for name,summary in index['profiles'].items():
        original_summary=json.loads((package/(name+'-checkpoint-summary.json')).read_text())
        if original_summary!=summary: raise ValueError('index gate summary differs')
        (package/(name+'-summary.json')).write_text(json.dumps(summary,indent=2)+'\n')
    (package/'proof-map.json').write_bytes((source/'config/cl-proof-map.json').read_bytes())
    (package/'package-checksums.json').write_text(json.dumps({str(f.relative_to(package)):sha(f) for f in package.rglob('*') if f.is_file()},indent=2)+'\n')
    repo=Path(__file__).resolve().parent.parent
    dependency='bots/margin-liquidator'
    for filename in ('package.json','package-lock.json'):
        if sha(repo/dependency/filename)!=sha(source/dependency/filename): raise ValueError('installed dependency manifest differs from pinned source')
    modules=repo/dependency/'node_modules'
    if not modules.is_dir(): raise ValueError('run npm ci --ignore-scripts in bots/margin-liquidator first')
    (source/dependency/'node_modules').symlink_to(modules,target_is_directory=True)
    result=subprocess.run([sys.executable,str(repo/'scripts/replay_cl_evidence.py'),str(package),'--source-root',str(source),'--output',str(out/'replay')],check=False)
    if result.returncode not in (0,1): raise ValueError('historical replay failed verification')
    verified=json.loads((out/'replay/replay-verification.json').read_text())
    if set(verified)!=set(index['profiles']) or not all(v['verified'] for v in verified.values()): raise ValueError('incomplete profile replay')
    return result.returncode
if __name__=='__main__':
    try: sys.exit(main())
    except (ValueError,OSError,KeyError,TypeError) as e:
        print('LINUX REPLAY FAIL: '+str(e),file=sys.stderr);sys.exit(2)
