#!/usr/bin/env python3
"""Replay pinned CI gates and errors; exit1 retains expected checkpoint failure."""
import argparse,hashlib,importlib.util,json,subprocess,sys,tarfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main(evidence,out,reuse=False,only_warm=False):
    if not reuse:assert not out.exists();out.mkdir(parents=True)
    else:assert out.is_dir()
    subprocess.run([sys.executable,str(Path(__file__).with_name('ci-evidence.py')),'--verify',str(evidence)],check=True)
    with tarfile.open(evidence/'ci-payload.tar.xz','r:xz') as t:
        for e in t:
            dest=out/e.name;assert dest.resolve().is_relative_to(out) and e.isfile()
            data=t.extractfile(e).read()
            if reuse:assert dest.read_bytes()==data,'extracted evidence changed'
            else:dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
    source=out/'source';artifact=out/'artifact';reports=out/('replay-'+str(len(list(out.glob('replay*')))));reports.mkdir()
    modules=source/'bots/margin-liquidator/node_modules'
    for leaf in ('package.json','package-lock.json'):
        assert sha(source/'bots/margin-liquidator'/leaf)==sha(ROOT/'bots/margin-liquidator'/leaf)
    if not modules.exists():modules.symlink_to(ROOT/'bots/margin-liquidator/node_modules',target_is_directory=True)
    assert modules.resolve()==(ROOT/'bots/margin-liquidator/node_modules').resolve()
    sys.path.insert(0,str(source/'scripts'))
    from cl_replay_summary import compare,compare_invocations
    from cl_resource_profiles import create
    indexed={}
    for p in list(source.rglob('*'))+list(artifact.rglob('*')):
        if p.is_file():indexed.setdefault(sha(p),[]).append(p)
    def relocate(item):
        old=item['path'];candidates=indexed[item['sha256']]
        exact=[p for p in candidates if p.name==Path(old).name]
        path=(exact or candidates)[0];item['path']=str(path);return old,str(path)
    results={}
    for expected_path in sorted(artifact.rglob('*gates.json')):
        # Post-preparation financial views and storage invocation views; baseline
        # complete proof manifests are replayed below by the existing exact tool.
        run=expected_path.parent;expected=json.loads(expected_path.read_text())
        if expected_path.name=='preparation-gates.json':continue
        if only_warm and not ('cl-restoration-ci' in run.parts and run.name=='checkpoint'):continue
        if 'cl-storage-ci-' not in str(run) and 'cl-restoration-ci' not in run.parts:continue
        name=expected['profile'];dest=reports/str(expected_path.relative_to(artifact)).replace('/','_');dest.mkdir()
        config=next(p for p in [run/'network-config.json',run.parent/'network-config.json'] if p.exists())
        profile=create(config,name);pf=dest/'profile.json';pf.write_text(json.dumps(profile))
        cmd=['bash','scripts/check_budget.sh','--local-report',str(run/'resources.jsonl'),'--profile',str(pf),'--transaction-evidence',str(run/'transactions.jsonl'),'--gate',expected['selected_gate'],'--summary-json',str(dest/'gates.json')]
        paths=[];manifest=run/'proof-manifest.json'
        if manifest.exists():
            m=json.loads(manifest.read_text());m['source_root']=str(source)
            relocate(m['reviewed_map']);relocate(m['call_profile'])
            for item in m['sources'].values():relocate(item)
            for item in m['test_runs']:
                old,new=relocate(item);paths.extend([(old,'<test-log>'),(new,'<test-log>')])
            mf=dest/'proof-manifest.json';mf.write_text(json.dumps(m));cmd+=['--proof-manifest',str(mf)]
        with (dest/'budget.log').open('w') as log:r=subprocess.run(cmd,cwd=source,stdout=log,stderr=subprocess.STDOUT)
        observed=json.loads((dest/'gates.json').read_text())
        if expected['selected_gate']=='checkpoint':v=compare(expected,observed,r.returncode,paths)
        else:
            def normalized(report):
                s=json.dumps(report,sort_keys=True)
                for old,new in paths:s=s.replace(old,new)
                return s
            assert normalized(expected)==normalized(observed),'invocation-selected gate/error report differs'
            assert r.returncode==(0 if observed['gates']['invocation'] else 1)
            v={'verified':True,'gates':observed['gates'],'errors':{k:observed[k] for k in ('missing_required_flows','lifecycle_errors','transaction_errors')},'selected_gate':'invocation','release_approval':False}
        oldlog=run/('financial.log' if 'cl-restoration-ci' in run.parts and run.name!='checkpoint' else 'budget.log')
        v['invocation_resources_and_errors_verified']=compare_invocations(oldlog.read_text(),(dest/'budget.log').read_text())
        results[str(expected_path.relative_to(artifact))]=v
        prep=run/'preparation-gates.json'
        if prep.exists():
            producer=artifact/'13226134371/cl-restoration-ci/builds/final/harness/src/restoration.rs'
            cmd=['python3','prototypes/cl-restoration-preparation/restore-checker.py','--input',str(run/'preparation.jsonl'),'--journeys-input',str(run/'journeys.jsonl'),'--profile',str(pf),'--network-config',str(config),'--producer-file',str(producer),'--output',str(dest/'preparation-gates.json')]
            with (dest/'preparation.log').open('w') as log:r=subprocess.run(cmd,cwd=source,stdout=log,stderr=subprocess.STDOUT)
            assert r.returncode==1 and json.loads(prep.read_text())==json.loads((dest/'preparation-gates.json').read_text()),'restoration gate/error replay differs'
            results[str(prep.relative_to(artifact))]={'exact_report_verified':True,'release_approval':False}
        (reports/'verification.json').write_text(json.dumps(results,indent=2)+'\n')
    if only_warm:
        assert len(results)==4
        print('Four original warm invocation gate/error reports replayed; global failures preserved.')
        return 1
    # Full baseline has all required labels; preserve its different outcome.
    baseline=artifact/'13228951408/cl-ci-checkpoint';package=out/'baseline-package';package.mkdir()
    for p in baseline.rglob('*'):
        if p.is_file() and 'artifacts' not in p.relative_to(baseline).parts:
            d=package/p.relative_to(baseline);d.parent.mkdir(parents=True,exist_ok=True);d.write_bytes(p.read_bytes())
    for name in ('internal-policy','captured-network-diagnostic'):
        (package/(name+'-summary.json')).write_bytes((package/(name+'-checkpoint-summary.json')).read_bytes())
    (package/'proof-map.json').write_bytes((source/'config/cl-proof-map.json').read_bytes())
    (package/'package-checksums.json').write_text(json.dumps({str(p.relative_to(package)):sha(p) for p in package.rglob('*') if p.is_file()}))
    r=subprocess.run([sys.executable,str(source/'scripts/replay_cl_evidence.py'),str(package),'--source-root',str(source),'--output',str(reports/'baseline')],cwd=source)
    assert r.returncode==1
    results['baseline']=json.loads((reports/'baseline/replay-verification.json').read_text())
    (reports/'verification.json').write_text(json.dumps(results,indent=2)+'\n')
    print('Exact per-profile gates/errors/resources replayed; failed release gates preserved.')
    return 1
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('evidence',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--reuse-extracted',action='store_true');p.add_argument('--only-warm',action='store_true');a=p.parse_args()
    raise SystemExit(main(a.evidence.resolve(),a.output.resolve(),a.reuse_extracted,a.only_warm))
