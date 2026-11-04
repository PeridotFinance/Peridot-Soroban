#!/usr/bin/env python3
"""Copy a verified joined build; correct only its legacy-error test assertion."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
OLD = 'assert!(rejected.is_err()&&format!("{:?}",rejected).contains("WasmVm")&&format!("{:?}",rejected).contains("InvalidAction"));'
NEW = 'assert!(rejected.is_err(), "live successor must fail before takeover deadline");'

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--build',required=True,type=Path)
    p.add_argument('--output',required=True,type=Path)
    a=p.parse_args(); old=a.build.resolve(); out=a.output.resolve()
    assert not out.exists(),'Refusing existing output'
    m=json.loads((old/'build-manifest.json').read_text())
    assert m['qualification_variant']=='option1-stalled-funded-recovery-unadopted'
    for name,value in m['wasm'].items():
        q=old/'artifacts'/name
        assert sha(q)==value['sha256'] and q.stat().st_size==value['bytes']
    for name,h in m['harness_generated_sources'].items():assert sha(old/name)==h,name
    for name,h in m['optimized_source_hashes'].items():assert sha(old/'workspace'/name)==h,name
    test=old/'harness/src/closing-recovery-tests.rs'
    text=test.read_text();assert text.count(OLD)==1,'exact old assertion required'
    repaired=text.replace(OLD,NEW)
    assert repaired==(HERE/'closing-recovery-tests.rs').read_text(),'canonical correction differs'
    out.mkdir(parents=True)
    for folder in ['artifacts','harness']:
        shutil.copytree(old/folder,out/folder,ignore=shutil.ignore_patterns('target','test_snapshots'))
    # Explicit public source list only; never copy build caches or scan siblings.
    names=set(m['baseline_source_hashes'])|set(m['optimized_source_hashes'])|{'Cargo.toml','Cargo.lock'}
    for name in sorted(names):
        src=old/'workspace'/name
        if src.is_file():
            dst=out/'workspace'/name;dst.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(src,dst)
    if (old/'optimization-build.log').is_file():shutil.copy2(old/'optimization-build.log',out/'optimization-build.log')
    (out/'harness/src/closing-recovery-tests.rs').write_text(repaired)
    m['parent_build_manifest_sha256']=sha(old/'build-manifest.json')
    m['fixture_repair']={'script_sha256':sha(Path(__file__)),
        'scope':'Only test result-text assertion removed; root Err, full rollback and paired timed-success assertions remain',
        'old_test_sha256':sha(test),'new_test_sha256':sha(out/'harness/src/closing-recovery-tests.rs'),
        'contract_rebuilt':False,'wasm_bytes_unchanged':True}
    m['harness_generated_sources']={str(q.relative_to(out)):sha(q)for q in sorted((out/'harness/src').glob('*.rs'))}
    (out/'build-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
    for name,value in m['wasm'].items():assert sha(out/'artifacts'/name)==value['sha256']
    print('Verified identical WASMs; corrected test assertion only:',out)

if __name__=='__main__':main()
