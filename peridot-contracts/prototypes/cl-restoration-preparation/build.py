#!/usr/bin/env python3
"""Fresh isolated WASM cache: no cross-workspace reuse of our contract outputs."""
import hashlib,importlib.util,json,subprocess,sys
from pathlib import Path
here=Path(__file__).resolve().parent;root=here.parents[1]
spec=importlib.util.spec_from_file_location('baseline_prepare',here.parent/'cl-storage-qualification/prepare.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
run=subprocess.run
def build(command,**kwargs):
 if command[:3]==['stellar','contract','build'] and command[command.index('--package')+1]=='margin-controller':command+=['--features','funded-liquidation-validation']
 return run(command,**kwargs)
m.subprocess.run=build
m.main()
out=Path(sys.argv[sys.argv.index('--output')+1]).resolve()
p=out/'harness/src/repayment_fixture.rs';p.write_text((root/'prototypes/cl-atomic-qualification/tests/combined.rs').read_text()+'\n#[path = "'+str(here/'combined-repayment-tests.rs')+'"]\nmod combined_repayment_cases;\n')
f=out/'harness/src/test.rs';f.write_text(f.read_text()+'\n#[path = "repayment_fixture.rs"]\nmod repayment_qualification;\n')
f=out/'build-manifest.json';manifest=json.loads(f.read_text());manifest.update({'margin_build_feature':'funded-liquidation-validation','repayment_tests':True,'contract_cache_scope':'fresh build-specific directory; never reuse previous workspace contract outputs','harness_generated_sources':{str(p.relative_to(out)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((out/'harness/src').glob('*.rs'))}});f.write_text(json.dumps(manifest,indent=2)+'\n')
