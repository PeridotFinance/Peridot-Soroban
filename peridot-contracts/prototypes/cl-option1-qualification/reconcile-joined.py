#!/usr/bin/env python3
"""Rebind reviewed labels without rerunning or replacing original execution/gates."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser();p.add_argument('--run',required=True,type=Path);a=p.parse_args()
    run=a.run.resolve();driver=json.loads(run.with_name(run.name+'.driver.json').read_text())
    assert driver['seven_cases_completed']
    build=Path(driver['build']);manifest=json.loads((build/'build-manifest.json').read_text())
    old=json.loads((run/'joined-proof-manifest.json').read_text())
    assert old['resource_report_sha256']==sha(run/'resources.jsonl')
    mapping=json.loads((HERE/'joined-proof-map.json').read_text())
    assert set(mapping['sources'])==set(old['sources'])
    for name,source in mapping['sources'].items():
        leaf='closing_fixture.rs' if source.endswith('/tests/combined.rs') else Path(source).name
        generated=build/'harness/src'/leaf
        assert sha(generated)==manifest['harness_generated_sources'][str(generated.relative_to(build))]==old['sources'][name]['sha256']
        mapping['sources'][name]=str(generated)
    reviewed=run/'reviewed-proof-map.json';proof=run/'reviewed-proof-manifest.json';gates=run/'reviewed-gates.json'
    assert not any(p.exists() for p in [reviewed,proof,gates,run/'reviewed-budget.log'])
    reviewed.write_text(json.dumps(mapping,indent=2)+'\n')
    commands=[]
    cmd=['python3','scripts/make_cl_proof_manifest.py','--map',str(reviewed),'--profile',str(run/'profile.json'),
         '--report',str(run/'resources.jsonl'),'--test-log',str(run/'tests.log'),'--test-exit-code',(run/'test-exit-code.txt').read_text().strip(),
         '--call-profile',str(run/'calls.jsonl'),'--output',str(proof)]
    commands.append(cmd);subprocess.run(cmd,cwd=ROOT,check=True)
    cmd=['bash','scripts/check_budget.sh','--local-report',str(run/'resources.jsonl'),'--profile',str(run/'profile.json'),
         '--transaction-evidence',str(run/'transactions.jsonl'),'--proof-manifest',str(proof),'--gate','checkpoint','--summary-json',str(gates)]
    commands.append(cmd)
    with (run/'reviewed-budget.log').open('w') as f:r=subprocess.run(cmd,cwd=ROOT,stdout=f,stderr=subprocess.STDOUT)
    result=json.loads(gates.read_text())
    (run/'mapping-review.json').write_text(json.dumps({'reason':'Bind the existing closed-repay rejection to its actual measured label; no test, resource, transaction or checker change',
        'original_map_sha256':sha(run/'joined-proof-map.json'),'reviewed_map_sha256':sha(reviewed),
        'original_gates_sha256':sha(run/'joined-gates.json'),'reviewed_gates_sha256':sha(gates),
        'commands':commands,'checkpoint_exit_code':r.returncode,'gates':result['gates'],'release_approval':False},indent=2)+'\n')
    print(json.dumps({'profile':result['profile'],'gates':result['gates'],'lifecycle_errors':len(result['lifecycle_errors']),'transaction_errors':len(result['transaction_errors'])}))
    raise SystemExit(r.returncode)

if __name__=='__main__':main()
