#!/usr/bin/env python3
"""Preserve completed 9bb51aa CI; verify exact gates/errors/tests/WASM identities."""
import argparse, hashlib, io, json, re, subprocess, sys, tarfile, zipfile
from pathlib import Path, PurePosixPath

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
from cl_proof_evidence import cargo_outcomes
COMMIT='9bb51aa108d9cac72fd2dbc46051e89ac7a839b0'
def sha(b): return hashlib.sha256(b).hexdigest()
def describe(members):
    result={'gates':{},'tests':{},'wasm':{},'exits':{}}
    for name,data in sorted(members.items()):
        if not name.startswith('artifact/'): continue
        if name.endswith('.wasm'): result['wasm'][name]={'sha256':sha(data),'bytes':len(data)}
        elif name.endswith('gates.json') or name.endswith('checkpoint-summary.json'):
            report=json.loads(data)
            assert report.get('release_approval') is False
            assert report.get('gates') and all(type(v) is bool for v in report['gates'].values())
            result['gates'][name]=report
        elif name.endswith('.log'):
            text=data.decode(errors='replace')
            totals=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out',text)
            names=re.findall(r'^test ([^\n]+?) \.\.\.',text,re.M)
            if totals or names:
                outcomes=cargo_outcomes(text)
                expected=sum(int(t[1])+int(t[2])+int(t[3]) for t in totals)
                result['tests'][name]={'cargo_totals':totals,'announced_tests':names,'observed_named_outcomes':outcomes,'named_outcomes_complete':bool(totals) and len(outcomes)==expected and all(v in ('ok','FAILED','ignored') for v in outcomes.values()),'named_parser_scope':'Serial libtest lines; parallel native logs may have incomplete named coverage. Exact Cargo totals and raw logs are retained.','sha256':sha(data),'shared_log_views_not_additional_executions':True}
        elif name.endswith('exit-code.txt') or name.endswith('-exit.txt'):
            result['exits'][name]=int(data.strip())
    for name,data in members.items():
        if name.startswith('artifact/') and name.endswith('build-manifest.json'):
            manifest=json.loads(data)
            for leaf,identity in manifest.get('wasm',{}).items():
                key=str(PurePosixPath(name).parent/'artifacts'/leaf)
                assert result['wasm'][key]==identity,(key,'WASM mismatch')
    assert result['gates'] and result['wasm']
    return json.loads(json.dumps(result))

def verify(directory):
    index=json.loads((directory/'index.json').read_text());assert index['commit']==COMMIT
    assert index['release_approval'] is False
    archive=directory/'ci-payload.tar.xz';raw=archive.read_bytes()
    assert {'bytes':len(raw),'sha256':sha(raw)}==index['archive']
    members={};seen=set()
    with tarfile.open(archive,'r:xz') as t:
        for e in t:
            p=PurePosixPath(e.name)
            assert e.isfile() and not p.is_absolute() and '..' not in p.parts and e.name not in seen
            b=t.extractfile(e).read();assert {'bytes':len(b),'sha256':sha(b)}==index['members'][e.name]
            seen.add(e.name)
            # Raw JSONL dominates the 2GB payload. Hash every member, but retain
            # only inputs needed for summary verification, not all raw reports.
            if e.name.startswith('artifact/') and e.name.endswith(('.wasm','gates.json','checkpoint-summary.json','.log','exit-code.txt','-exit.txt','build-manifest.json')):
                members[e.name]=b
    assert seen==set(index['members'])
    assert describe(members)==index['results'],'per-profile gates/errors/tests/identities differ'
    print(json.dumps({'archive_sha256':sha(raw),'members':len(seen),'gate_reports':len(index['results']['gates']),'wasm_identities':len(index['results']['wasm']),'exact_summary_verified':True,'checker_replay_performed':False,'release_approval':False}))

def package(source,output):
    assert not output.exists();members={};github={};zips={}
    for directory in sorted(source.iterdir()):
        if not directory.is_dir():continue
        meta=json.loads((directory/'github.json').read_text());github[directory.name]=meta
        expected={a['digest'].removeprefix('sha256:'):a for a in meta['artifacts']['artifacts']}
        for p in sorted(directory.glob('*.zip')):
            digest=sha(p.read_bytes());a=expected.pop(digest);assert a['workflow_run']['head_sha']==COMMIT
            zips[str(a['id'])]={'sha256':digest,'bytes':p.stat().st_size,'metadata':a}
            with zipfile.ZipFile(p) as z:
                for e in z.infolist():
                    n=PurePosixPath(e.filename);assert not e.is_dir() and not n.is_absolute() and '..' not in n.parts
                    key='artifact/'+str(a['id'])+'/'+str(n);assert key not in members;members[key]=z.read(e)
        assert not expected,'missing artifact'
        members['github/'+directory.name+'.json']=(directory/'github.json').read_bytes()
    # Pin replay implementation, producer and assertion sources without local dirty work.
    paths=subprocess.check_output(['git','ls-tree','-r','--name-only',COMMIT,'--','scripts','prototypes','config','contracts/cl-share-pool-facade','contracts/cl-share-vault','bots/margin-liquidator/package.json','bots/margin-liquidator/package-lock.json'],cwd=ROOT,text=True).splitlines()
    for p in paths:
        if p.endswith(('.py','.sh','.cjs','.rs','.json','.md','.patch','.lock','.toml')):
            members['source/'+p]=subprocess.check_output(['git','show',COMMIT+':peridot-contracts/'+p],cwd=ROOT)
    output.mkdir(parents=True)
    archive=output/'ci-payload.tar.xz'
    with tarfile.open(archive,'w:xz',preset=6) as t:
        for name,data in sorted(members.items()):
            e=tarfile.TarInfo(name);e.size=len(data);e.mode=0o644;t.addfile(e,io.BytesIO(data))
    raw=archive.read_bytes()
    index={'schema':'cl-option1-prior-ci-v1','commit':COMMIT,'github':github,'original_artifacts':zips,'archive':{'sha256':sha(raw),'bytes':len(raw)},'members':{n:{'sha256':sha(b),'bytes':len(b)} for n,b in sorted(members.items())},'results':describe(members),'release_approval':False}
    (output/'index.json').write_text(json.dumps(index,indent=2)+'\n');verify(output)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--source',type=Path);p.add_argument('--output',type=Path);p.add_argument('--verify',type=Path);a=p.parse_args()
    if a.verify:verify(a.verify)
    else:package(a.source,a.output)
