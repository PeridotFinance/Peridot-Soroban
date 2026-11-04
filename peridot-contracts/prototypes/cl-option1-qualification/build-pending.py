#!/usr/bin/env python3
"""Build the exact four-overlay, validation-only pending-recovery experiment."""
import hashlib, importlib.util, json, subprocess, sys
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
out=Path(sys.argv[sys.argv.index('--output')+1]).resolve()
patch=Path(sys.argv[sys.argv.index('--optimization')+1]).resolve()
inputs=[ROOT/'prototypes/cl-atomic-qualification/optimization.patch',ROOT/'prototypes/cl-storage-qualification/storage.patch',ROOT/'prototypes/cl-storage-qualification/repayment.patch',HERE/'pending-recovery.patch']
assert patch.read_bytes()==b''.join(p.read_bytes() for p in inputs),'exact four-overlay input required'
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
fixture.write_text((ROOT/'prototypes/cl-atomic-qualification/tests/combined.rs').read_text()+'\n#[path = "pending-recovery-tests.rs"]\nmod pending_cases;\n')
for name in ['pending-recovery-tests.rs','pending-recovery.patch']:
    (out/'harness/src'/name).write_bytes((HERE/name).read_bytes())
test=out/'harness/src/test.rs';test.write_text(test.read_text()+'\n#[path = "pending_fixture.rs"]\nmod pending_qualification;\n')
f=out/'build-manifest.json';manifest=json.loads(f.read_text())
manifest.update({'qualification_variant':'option1-pending-independent-recovery-unadopted','margin_build_feature':'funded-liquidation-validation','pending_recovery_tests':True,'pending_overlay_sha256':hashlib.sha256((HERE/'pending-recovery.patch').read_bytes()).hexdigest(),'overlay_inputs':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs},'harness_generated_sources':{str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((out/'harness/src').glob('*.rs'))}})
f.write_text(json.dumps(manifest,indent=2)+'\n')
