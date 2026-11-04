#!/usr/bin/env python3
"""Inventory the reviewed experiment directories, excluding tool/compiler caches."""
import argparse,hashlib,json,shutil
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
RUNS='''pending-captured-v1 pending-captured-v2 pending-captured-v3 pending-internal-v2
closing-captured-v1 closing-captured-v2 joined-closing-captured-v1
joined-final-captured-v1 joined-final-captured-v2 joined-final-captured-v3 joined-final-internal-v3
pending-restoration-v1 pending-restoration-v2 shared-v1 shared-v2
mixed-captured-v1 mixed-captured-v2 mixed-captured-v3 mixed-internal-v3
proof pending-expired-only-results pending-source-review'''.split()
BUILDS='''pending-build-v1 pending-build-v3 pending-restoration-build-v1
closing-build-v1 closing-build-v2 joined-build-v1 joined-build-v2 joined-build-v3
shared-build-v1 shared-build-v2 mixed-build-v1 mixed-build-v2 mixed-build-v3
mixed-portability-check'''.split()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);a=p.parse_args();assert not a.output.exists()
    files=set()
    def add(p):
        assert p.is_file() and not p.is_symlink(),p
        files.add(str(p.relative_to(ROOT)))
    for p in HERE.rglob('*'):
        if p.is_file() and p.suffix in ('.py','.rs','.md','.json','.sh','.patch','.toml','.lock','.cjs','.cpp','.h','.txt','.ac','.am') and p.name!='checkpoint-draft.md':add(p)
    for name in RUNS:
        directory=ROOT/'target'/('cl-option1-'+name);assert directory.is_dir(),directory
        for p in directory.rglob('*'):
            if p.is_file():add(p)
    for name in BUILDS:
        build=ROOT/'target'/('cl-option1-'+name);assert build.is_dir(),build
        for leaf in ['build-manifest.json','optimization-build.log']:
            if (build/leaf).exists():add(build/leaf)
        for leaf in ['artifacts','harness/src']:
            for p in (build/leaf).rglob('*'):
                if p.is_file():add(p)
        for leaf in ['Cargo.toml','Cargo.lock']:
            if (build/'harness'/leaf).exists():add(build/'harness'/leaf)
        manifest=build/'build-manifest.json'
        if manifest.exists():
            m=json.loads(manifest.read_text())
            for rel,h in m.get('optimized_source_hashes',{}).items():
                source=build/'workspace'/rel
                if source.exists():assert sha(source)==h;add(source)
    # Explicit single-file drivers, original proof map, assessments and retry
    # records. Do not traverse other target directories or wallet artifacts.
    for p in (ROOT/'target').glob('cl-option1-*'):
        if p.is_file() and p.suffix in ('.json','.jsonl','.log','.patch','.py','.md','.txt'):add(p)
    old=ROOT/'target/cl-option1-closing-work/pre-live-close-fix'
    for p in old.iterdir():
        if p.is_file():add(p)
    for p in (ROOT/'target/cl-option1-closing-work').iterdir():
        if p.is_file() and p.suffix in ('.log','.json','.patch'):add(p)
    core=ROOT/'target/cl-option1-core'
    approved=json.loads((core/'archive-allowlist.json').read_text())
    for r in approved['files']:
        p=core/r['path'];assert sha(p)==r['sha256'],f'Core producer still active: {p}';add(p)
    add(core/'archive-allowlist.json')
    checker=ROOT/'target/cl-option1-checker-source';assert not checker.exists()
    source_names=['scripts/check_budget.sh','scripts/check_local_budget.py','scripts/cl_resource_profiles.py','scripts/cl_fee_estimate.py','scripts/cl_proof_evidence.py','scripts/cl_transaction_evidence.py','scripts/make_cl_proof_manifest.py','scripts/decode_cl_transaction_evidence.cjs','bots/margin-liquidator/package.json','bots/margin-liquidator/package-lock.json']
    pins={}
    for rel in source_names:
        src=ROOT/rel;dest=checker/rel;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dest);pins[rel]=sha(dest);add(dest)
    workflow=ROOT.parent/'.github/workflows/cl-option1-prototype.yml'
    dest=checker/'cl-option1-prototype.yml';shutil.copyfile(workflow,dest);add(dest);pins['../.github/workflows/cl-option1-prototype.yml']=sha(dest)
    dest=checker/'source-index.json';dest.write_text(json.dumps(pins,indent=2)+'\n');add(dest)
    a.output.write_text(json.dumps({'schema':'cl-option1-explicit-file-list-v1',
        'scope':'Local isolated prototype runs, preserved failures, exact compiled artifacts, shared snapshots and supported Core proof subset; CI9bb is a separate durable archive; no caches or wallet artifacts',
        'files':sorted(files),'bytes':sum((ROOT/rel).stat().st_size for rel in files)},indent=2)+'\n')
    print(json.dumps({'files':len(files),'raw_bytes':sum((ROOT/rel).stat().st_size for rel in files)}))
if __name__=='__main__':main()
