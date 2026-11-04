#!/usr/bin/env python3
"""Run one identified mixed companion and preserve every original/rebound gate."""
import argparse,hashlib,json,os,re,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser();p.add_argument('--build',type=Path,required=True);p.add_argument('--run',type=Path,required=True);p.add_argument('--profile',choices=['internal-policy','captured-network-diagnostic'],required=True);a=p.parse_args()
    build=a.build.resolve();run=a.run.resolve();assert not run.exists()
    m=json.loads((build/'build-manifest.json').read_text());assert m['mixed_funding_companion'] and not m['release_approval']
    generated=build/'harness/src/mixed-funding-tests.rs'
    assert sha(generated)==m['mixed_test_sha256']==m['harness_generated_sources']['harness/src/mixed-funding-tests.rs']
    pins={str(p):sha(p) for p in [Path(__file__).resolve(),HERE/'mixed-proof-map.json',generated,build/'build-manifest.json']}
    env=dict(os.environ,CL_ATOMIC_BUILD_DIR=str(build),CARGO_BUILD_JOBS='1')
    command=['bash','prototypes/cl-storage-qualification/run_variant.sh',a.profile,'compiled_option1_mixed_proceeds_and_external_deficit_preserve_reserved_fees',str(run)]
    result=subprocess.run(command,cwd=ROOT,env=env)
    assert all(sha(Path(p))==h for p,h in pins.items())
    counts=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;',(run/'tests.log').read_text())
    completed=len(counts)==1 and sum(map(int,counts[0]))==1
    mapping=json.loads((HERE/'mixed-proof-map.json').read_text())
    assert len(mapping['sources'])==1
    mapping['sources']={name:str(generated) for name in mapping['sources']}
    map_file=run/'mixed-proof-map.json';map_file.write_text(json.dumps(mapping,indent=2)+'\n')
    proof=run/'mixed-proof-manifest.json'
    subprocess.run(['python3','scripts/make_cl_proof_manifest.py','--map',str(map_file),'--profile',str(run/'profile.json'),'--report',str(run/'resources.jsonl'),'--test-log',str(run/'tests.log'),'--test-exit-code',(run/'test-exit-code.txt').read_text().strip(),'--call-profile',str(run/'calls.jsonl'),'--output',str(proof)],cwd=ROOT,check=True)
    with (run/'mixed-budget.log').open('w') as f:
        gate=subprocess.run(['bash','scripts/check_budget.sh','--local-report',str(run/'resources.jsonl'),'--profile',str(run/'profile.json'),'--transaction-evidence',str(run/'transactions.jsonl'),'--proof-manifest',str(proof),'--gate','checkpoint','--summary-json',str(run/'mixed-gates.json')],cwd=ROOT,stdout=f,stderr=subprocess.STDOUT)
    (run/'mixed-driver.json').write_text(json.dumps({'source_sha256':pins,'command':command,'profile':a.profile,'test_counts':counts,'one_case_completed':completed,'original_wrapper_exit':result.returncode,'checkpoint_exit':gate.returncode,'release_approval':False},indent=2)+'\n')
    raise SystemExit(result.returncode or gate.returncode or (0 if completed else 2))
if __name__=='__main__':main()
