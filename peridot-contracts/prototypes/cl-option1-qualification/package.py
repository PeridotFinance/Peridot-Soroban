#!/usr/bin/env python3
"""Archive only explicitly reviewed research paths from a file-list plan."""
import argparse
import hashlib
import json
from pathlib import Path
import tarfile

ROOT=Path(__file__).resolve().parents[2]

def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
    return h.hexdigest()

def main():
    p=argparse.ArgumentParser();p.add_argument('--plan',required=True,type=Path);p.add_argument('--output',required=True,type=Path);a=p.parse_args()
    plan=json.loads(a.plan.read_text());assert not a.output.exists()
    assert plan['schema']=='cl-option1-explicit-file-list-v1'
    paths={}
    for rel in plan['files']:
        path=ROOT/rel
        assert not Path(rel).is_absolute() and '..' not in Path(rel).parts
        assert rel.startswith(('target/cl-option1-','prototypes/cl-option1-qualification/'))
        assert path.is_file() and not path.is_symlink(),rel
        cur=ROOT
        for part in Path(rel).parts:
            cur/=part;assert not cur.is_symlink(),rel
        # Reviewed public download metadata only; never include bootstrap payloads.
        download_index = 'target/cl-option1-core/bootstrap-downloads/index.json'
        assert not any(part in ('node_modules','.git','.aws','.stellar','rustup','bootstrap-src') for part in Path(rel).parts),rel
        assert 'bootstrap-downloads' not in Path(rel).parts or rel == download_index,rel
        assert path.suffix in ('.rs','.py','.json','.jsonl','.patch','.log','.txt','.md','.toml','.lock','.sh','.wasm','.cpp','.hpp','.h','.yml','.cjs','.ac','.am') or (rel.startswith('target/cl-option1-core/') and path.name in ('COPYING','Makefile')),rel
        assert rel not in paths
        paths[rel]=path
    index={'schema':'cl-option1-durable-v1','stage1_complete':False,'policy_adopted':False,'release_approval':False,
           'scope':plan['scope'],'members':{rel:{'bytes':p.stat().st_size,'sha256':digest(p)} for rel,p in sorted(paths.items())},
           'plan_sha256':digest(a.plan),'full_checker_replay_performed_by_packager':False}
    a.output.mkdir(parents=True)
    payload=a.output/'qualification-evidence.tar.xz'
    with tarfile.open(payload,'w:xz',preset=3) as tar:
        for rel,path in sorted(paths.items()):
            info=tarfile.TarInfo(rel);info.size=index['members'][rel]['bytes'];info.mode=0o644;info.mtime=0
            with path.open('rb') as f:tar.addfile(info,f)
    for rel,path in paths.items():assert digest(path)==index['members'][rel]['sha256'],f'Producer changed {rel}'
    index['archive']={'file':payload.name,'bytes':payload.stat().st_size,'sha256':digest(payload)}
    (a.output/'index.json').write_text(json.dumps(index,indent=2)+'\n')
    (a.output/'file-list.json').write_bytes(a.plan.read_bytes())
    (a.output/'verify.py').write_bytes((Path(__file__).parent/'verify.py').read_bytes())
    print(json.dumps(index['archive']))

if __name__=='__main__':main()
