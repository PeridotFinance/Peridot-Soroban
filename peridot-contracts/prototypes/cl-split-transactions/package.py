#!/usr/bin/env python3
"""Archive only named local split evidence and public source; never scan target/."""
import argparse, hashlib, io, json, lzma, tarfile
from pathlib import Path
from summarize import summarize

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()

def main():
    p=argparse.ArgumentParser();p.add_argument('--build',type=Path,required=True)
    p.add_argument('--internal',type=Path,required=True);p.add_argument('--captured',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    out=a.output.resolve();assert not out.exists(), 'fresh output required';out.mkdir(parents=True)
    paths={};summaries={};bindings={};build=a.build.resolve()
    def add(name,path):
        path=path.resolve();assert path.is_file(), str(path);paths[name]=path
    pins=json.loads((HERE/'baseline-source-hashes.json').read_text())
    for name,h in pins.items():
        assert sha(ROOT/name)==h, 'baseline changed: '+name
        add('source/'+name,ROOT/name)
    for path in HERE.rglob('*'):
        if path.is_file() and '__pycache__' not in str(path): add('source/'+str(path.relative_to(ROOT)),path)
    for name in ['check_budget.sh','check_local_budget.py','cl_fee_estimate.py','cl_resource_profiles.py',
                 'cl_proof_evidence.py','cl_transaction_evidence.py','decode_cl_transaction_evidence.cjs','cl_replay_summary.py']:
        add('source/scripts/'+name,ROOT/'scripts'/name)
    for name in ['Cargo.toml','Cargo.lock','src/lib.rs','src/transaction_evidence.rs']:
        add('source/harness/cl-runtime29/'+name,ROOT/'harness/cl-runtime29'/name)
    for name in ['package.json','package-lock.json']:
        add('source/bots/margin-liquidator/'+name,ROOT/'bots/margin-liquidator'/name)
    for folder in ['artifacts','baseline-artifacts','logs']:
        for path in (build/folder).iterdir():
            if path.is_file():add('build/'+str(path.relative_to(build)),path)
    for name in ['build-manifest.json','harness/src/lib.rs','harness/src/test.rs','harness/Cargo.toml','harness/Cargo.lock']:
        add('build/'+name,build/name)
    add('fixture/network-config.json',ROOT/'../docs/deployment-handoffs/evidence/cl-resource-reconciliation-20261102/network-config.json')
    add('fixture/aquarius-pool.wasm',ROOT/'target/cl-share-checkpoint/aquarius-pool.wasm')
    assert sha(paths['fixture/aquarius-pool.wasm'])=='12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6'
    for profile,run in [('internal-policy',a.internal.resolve()),('captured-network-diagnostic',a.captured.resolve())]:
        summaries[profile]=summarize(run)
        for path in run.iterdir():
            if path.is_file():add('runs/'+profile+'/'+path.name,path)
        for original,h in json.loads((run/'source-before.json').read_text()).items():
            original=Path(original);assert sha(original)==h, 'run source changed: '+str(original)
            matches=[name for name,path in paths.items() if path==original]
            assert matches, 'missing source artifact: '+str(original)
            bindings[str(original)]={'path':matches[0],'sha256':h}
    # Earlier runs are diagnostics only. Preserve their failures without mixing
    # them into the final profiles or claiming that interrupted suites passed.
    for name in ['cl-split-first','cl-split-second','cl-split-third','cl-split-final-captured',
                 'cl-split-final-internal','cl-split-acceptance-captured','cl-split-acceptance-internal',
                 'cl-split-transfer-fault-fixed']:
        for file in ['tests.log','test.log','budget.log','gates.json','INTERRUPTED.txt']:
            path=ROOT/'target'/name/file
            if path.is_file():add('diagnostic-only/'+name+'/'+file,path)
    for path in (ROOT/'target/cl-split-first-final-source').glob('*.rs'):
        add('diagnostic-only/first-final-test-source/'+path.name,path)
    for file in ['environment.json','independent-review.txt','independent-author-review.txt']:
        add('supplemental/'+file,ROOT/'target/cl-split-publication'/file)
    for file in ['analysis.md','accounting-review.md','review.md','v3-state-plan.md','v3-implementation.md','vault-abi.md']:
        path=ROOT/'target/cl-split-design'/file
        if path.is_file():add('diagnostic-only/design-not-final/'+file,path)
    # Evidence of the corrected shared-output-directory bug; not a valid baseline.
    add('diagnostic-only/shared-output-build-manifest.json',ROOT/'target/cl-split-repro-verified/build-manifest.json')
    add('diagnostic-only/shared-output-baseline-vault.log',ROOT/'target/cl-split-repro-verified/logs/baseline-cl-share-vault.log')
    wasm=json.loads((build/'build-manifest.json').read_text())['wasm']
    summary={'schema':'cl-split-experiment-v1','baseline_commit':'9cf43082bfb57334d57dbc58cfa90ccad4d98078',
             'stage1_complete':False,'release_approval':False,'profiles':summaries,'wasm':wasm}
    raw=(json.dumps(summary,indent=2)+'\n').encode();(out/'execution-summary.json.xz').write_bytes(lzma.compress(raw))
    generated={'execution-summary.json':raw};archive=out/'checkpoint-payload.tar.xz';files={}
    with tarfile.open(archive,'w:xz',preset=6) as tar:
        for name in sorted(set(paths)|set(generated)):
            data=generated[name] if name in generated else paths[name].read_bytes()
            files[name]={'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)}
            info=tarfile.TarInfo(name);info.size=len(data);info.mode=0o644;tar.addfile(info,io.BytesIO(data))
    index={'schema':'cl-split-experiment-archive-v1','baseline_commit':summary['baseline_commit'],
           'stage1_complete':False,'release_approval':False,'archive_sha256':sha(archive),
           'archive_bytes':archive.stat().st_size,'summary_sha256':hashlib.sha256(raw).hexdigest(),
           'files':files,'source_bindings':bindings,'wasm':wasm,
           'profiles':{name:{k:s[k] for k in ['test_exit','invocation_checker_exit','outcomes','test_counts','gates','raw_hashes']}
                       for name,s in summaries.items()}}
    (out/'index.json').write_text(json.dumps(index,indent=2)+'\n')
    print(len(files),'files;',archive.stat().st_size,'bytes')

if __name__=='__main__': main()
