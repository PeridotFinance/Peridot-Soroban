#!/usr/bin/env python3
"""Run the joined candidate on CI; every missing qualification gate stays failed."""
import argparse,hashlib,importlib.util,json,os,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
def main(out):
    out=out.resolve();scratch=out.with_name(out.name+'-builds')
    assert out.is_relative_to(ROOT/'target') and not out.exists() and not scratch.exists()
    out.mkdir(parents=True);scratch.mkdir();commands=[]
    def run(label,args):
        args=list(map(str,args));log=out/(label+'.log')
        with log.open('w') as f:r=subprocess.run(args,cwd=ROOT,stdout=f,stderr=subprocess.STDOUT,env=dict(os.environ,CARGO_BUILD_JOBS='1'))
        commands.append({'label':label,'command':args,'exit_code':r.returncode,'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest()})
        (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n');return r.returncode
    if run('reporting-tests',['python3',HERE/'test_reporting.py']):return 1
    inputs=[ROOT/'prototypes/cl-atomic-qualification/optimization.patch',ROOT/'prototypes/cl-storage-qualification/storage.patch',ROOT/'prototypes/cl-storage-qualification/repayment.patch']+[HERE/n for n in ['pending-recovery.patch','pending-unhealthy.patch','closing-recovery.patch']]
    overlay=scratch/'joined.patch';overlay.write_bytes(b''.join(p.read_bytes() for p in inputs))
    build=scratch/'joined'
    for name,args in [('build',['python3',HERE/'build-closing.py','--output',build,'--optimization',overlay]),('identify',['python3',ROOT/'prototypes/cl-storage-qualification/identify_build.py','--build',build]),('fetch',['cargo','fetch','--locked','--manifest-path',build/'harness/Cargo.toml'])]:
        if run(name,args):return 1
    s=importlib.util.spec_from_file_location('storage_ci',ROOT/'prototypes/cl-storage-qualification/ci.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
    m.snapshot_build(build,out/'build')
    results={}
    for profile in ['captured-network-diagnostic','internal-policy']:
        run_dir=out/profile
        code=run(profile,['python3',HERE/'run-closing.py','--profile',profile,'--build',build,'--run',run_dir])
        gate_path=run_dir/'joined-gates.json'
        report=json.loads(gate_path.read_text()) if gate_path.exists() else None
        results[profile]={'exit_code':code,'gate_report':report,'required_evidence_missing':report is None}
    mixed=scratch/'mixed';companions={}
    preparation=run('mixed-prepare',['python3',HERE/'prepare-mixed.py','--build',build,'--output',mixed,'--rebind-reviewed-platform'])
    if preparation==0:
        m.snapshot_build(mixed,out/'mixed-build')
        for profile in ['captured-network-diagnostic','internal-policy']:
            run_dir=out/('mixed-'+profile)
            code=run('mixed-'+profile,['python3',HERE/'run-mixed.py','--profile',profile,'--build',mixed,'--run',run_dir])
            path=run_dir/'mixed-gates.json';report=json.loads(path.read_text()) if path.exists() else None
            companions[profile]={'exit_code':code,'gate_report':report,'required_evidence_missing':report is None}
    (out/'qualification.json').write_text(json.dumps({'profiles':results,'mixed_preparation_exit':preparation,'mixed_profiles':companions,'stage1_complete':False,'policy_adopted':False,'core_application_measured_by_this_job':False,'signed_authorization_measured':False,'release_approval':False},indent=2)+'\n')
    # A focused suite cannot grant full lifecycle or release approval. Report
    # actual child failures and fail if any required report/gate is absent.
    return 0 if preparation==0 and len(companions)==2 and all(r['exit_code']==0 and r['gate_report'] and r['gate_report']['gates']['checkpoint'] for r in list(results.values())+list(companions.values())) else 1
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--output',required=True,type=Path);a=p.parse_args();raise SystemExit(main(a.output))
