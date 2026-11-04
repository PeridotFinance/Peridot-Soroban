#!/usr/bin/env python3
"""Build six isolated overlays and their focused joined recovery test modules."""
import hashlib, importlib.util, json, subprocess, sys
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
out=Path(sys.argv[sys.argv.index('--output')+1]).resolve()
patch=Path(sys.argv[sys.argv.index('--optimization')+1]).resolve()
inputs=[ROOT/'prototypes/cl-atomic-qualification/optimization.patch',ROOT/'prototypes/cl-storage-qualification/storage.patch',ROOT/'prototypes/cl-storage-qualification/repayment.patch',HERE/'pending-recovery.patch',HERE/'pending-unhealthy.patch',HERE/'closing-recovery.patch']
assert patch.read_bytes()==b''.join(p.read_bytes()for p in inputs),'exact six-overlay input required'
spec=importlib.util.spec_from_file_location('isolated_base',ROOT/'prototypes/cl-storage-qualification/prepare.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
run=subprocess.run
def guarded(command,**kwargs):
    if command[:3]==['stellar','contract','build'] and command[command.index('--package')+1]=='margin-controller':command+=['--features','funded-liquidation-validation']
    return run(command,**kwargs)
m.subprocess.run=guarded
m.main()
fixture=out/'harness/src/closing_fixture.rs'
declarations='\n'
for name,module in [('closing-recovery-tests.rs','closing_cases'),('pending-recovery-tests.rs','expired_pending'),('pending-unhealthy-tests.rs','early_pending'),('combined-pending-regression-tests.rs','combined_regressions')]:
    text=(HERE/name).read_text().replace('option1-pending-independent-recovery-unadopted','option1-stalled-funded-recovery-unadopted')
    for old in ['storage','repayment']:text=text.replace('../cl-storage-qualification/'+old+'.patch',old+'.patch')
    (out/'harness/src'/name).write_text(text)
    declarations+='#[path="'+name+'"]\nmod '+module+';\n'
fixture.write_text((ROOT/'prototypes/cl-atomic-qualification/tests/combined.rs').read_text()+declarations)
for name in ['closing-recovery.patch','pending-recovery.patch','pending-unhealthy.patch']:(out/'harness/src'/name).write_bytes((HERE/name).read_bytes())
for name in ['storage','repayment']:(out/'harness/src'/f'{name}.patch').write_bytes((ROOT/f'prototypes/cl-storage-qualification/{name}.patch').read_bytes())
test=out/'harness/src/test.rs';test.write_text(test.read_text()+'\n#[path="closing_fixture.rs"]\nmod closing_qualification;\n')
p=out/'build-manifest.json';d=json.loads(p.read_text())
module='contracts/margin-controller/src/funded_stalled_settlement.rs'
d['optimized_source_hashes'][module]=hashlib.sha256((out/'workspace'/module).read_bytes()).hexdigest()
d.update({'qualification_variant':'option1-stalled-funded-recovery-unadopted','margin_build_feature':'funded-liquidation-validation','closing_overlay_sha256':hashlib.sha256((HERE/'closing-recovery.patch').read_bytes()).hexdigest(),'pending_overlay_sha256':hashlib.sha256((HERE/'pending-recovery.patch').read_bytes()).hexdigest(),'pending_unhealthy_overlay_sha256':hashlib.sha256((HERE/'pending-unhealthy.patch').read_bytes()).hexdigest(),'combined_patch_sha256':{n:hashlib.sha256((ROOT/f'prototypes/cl-storage-qualification/{n}.patch').read_bytes()).hexdigest()for n in ['storage','repayment']},'overlay_inputs':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest()for p in inputs},'harness_generated_sources':{str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest()for p in sorted((out/'harness/src').glob('*.rs'))}})
p.write_text(json.dumps(d,indent=2)+'\n')
