#!/usr/bin/env python3
"""Build the exact five-overlay, validation-only pending-recovery experiment."""
import hashlib, importlib.util, json, subprocess, sys
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
out=Path(sys.argv[sys.argv.index('--output')+1]).resolve()
patch=Path(sys.argv[sys.argv.index('--optimization')+1]).resolve()
inputs=[ROOT/'prototypes/cl-atomic-qualification/optimization.patch',ROOT/'prototypes/cl-storage-qualification/storage.patch',ROOT/'prototypes/cl-storage-qualification/repayment.patch',HERE/'pending-recovery.patch',HERE/'pending-unhealthy.patch']
assert patch.read_bytes()==b''.join(p.read_bytes() for p in inputs),'exact five-overlay input required'
spec=importlib.util.spec_from_file_location('isolated_base',ROOT/'prototypes/cl-storage-qualification/prepare.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
run=subprocess.run
def guarded_build(command,**kwargs):
    if command[:3]==['stellar','contract','build'] and command[command.index('--package')+1]=='margin-controller':
        command+=['--features','funded-liquidation-validation']
    return run(command,**kwargs)
m.subprocess.run=guarded_build
m.main()
fixture=out/'harness/src/pending_fixture.rs'
fixture.write_text((ROOT/'prototypes/cl-atomic-qualification/tests/combined.rs').read_text()+'\n#[path = "pending_test_bundle.rs"]\nmod option1_cases;\n')
bundle='use super::*;\n'
for name,module in [('pending-recovery-tests.rs','expired_pending'),('pending-unhealthy-tests.rs','early_pending'),('combined-pending-regression-tests.rs','combined_regressions')]:
    data=(HERE/name).read_text()
    for old in ['storage','repayment']:data=data.replace('../cl-storage-qualification/'+old+'.patch',old+'.patch')
    (out/'harness/src'/name).write_text(data)
    bundle+='#[path = "'+name+'"]\nmod '+module+';\n'
bundle=bundle.replace('#\[path', '#[path')
(out/'harness/src/pending_test_bundle.rs').write_text(bundle)
for name in ['pending-recovery.patch','pending-unhealthy.patch']:
    (out/'harness/src'/name).write_bytes((HERE/name).read_bytes())
for name in ['storage','repayment']:
    (out/'harness/src'/f'{name}.patch').write_bytes((ROOT/f'prototypes/cl-storage-qualification/{name}.patch').read_bytes())
test=out/'harness/src/test.rs';test.write_text(test.read_text()+'\n#[path = "pending_fixture.rs"]\nmod pending_qualification;\n')
f=out/'build-manifest.json';manifest=json.loads(f.read_text())
manifest.update({'qualification_variant':'option1-pending-independent-recovery-unadopted','margin_build_feature':'funded-liquidation-validation','pending_recovery_tests':True,'pending_unhealthy_overlay_sha256':hashlib.sha256((HERE/'pending-unhealthy.patch').read_bytes()).hexdigest(),'combined_patch_sha256':{n:hashlib.sha256((ROOT/f'prototypes/cl-storage-qualification/{n}.patch').read_bytes()).hexdigest() for n in ['storage','repayment']},'pending_overlay_sha256':hashlib.sha256((HERE/'pending-recovery.patch').read_bytes()).hexdigest(),'overlay_inputs':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs},'harness_generated_sources':{str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((out/'harness/src').glob('*.rs'))}})
f.write_text(json.dumps(manifest,indent=2)+'\n')
