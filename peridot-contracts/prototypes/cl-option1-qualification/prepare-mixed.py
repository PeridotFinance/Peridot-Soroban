#!/usr/bin/env python3
"""Prepare the isolated mixed companion, binding an explicit reviewed build identity."""
import argparse,hashlib,json,shutil,subprocess
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
EXPECTED_V3='ad9fbb7669ff126ff54e8d9b021d610b9313cb5f3dc595df6e9d7dabc3d7d482'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--build',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--rebind-reviewed-platform',action='store_true',help='Bind generated test to the freshly built same-six-overlay platform hash; never relabel Mac evidence');a=p.parse_args();a.build=a.build.resolve();a.output=a.output.resolve()
assert not a.output.exists(),'Refusing existing output'
m=json.loads((a.build/'build-manifest.json').read_text())
assert m['qualification_variant']=='option1-stalled-funded-recovery-unadopted'
assert m['margin_build_feature']=='funded-liquidation-validation' and m['release_approval'] is False
patches=[ROOT/'prototypes/cl-atomic-qualification/optimization.patch',ROOT/'prototypes/cl-storage-qualification/storage.patch',ROOT/'prototypes/cl-storage-qualification/repayment.patch']+[HERE/n for n in ['pending-recovery.patch','pending-unhealthy.patch','closing-recovery.patch']]
assert m['optimization_sha256']==hashlib.sha256(b''.join(p.read_bytes() for p in patches)).hexdigest(),'exact six-overlay parent required'
actual_v3=m['wasm']['margin_controller.wasm']['sha256']
assert a.rebind_reviewed_platform or actual_v3==EXPECTED_V3
for name,pin in m['wasm'].items():assert sha(a.build/'artifacts'/name)==pin['sha256'] and (a.build/'artifacts'/name).stat().st_size==pin['bytes'],name
for name,h in m['harness_generated_sources'].items():assert sha(a.build/name)==h,name
for name,h in m['optimized_source_hashes'].items():assert sha(a.build/'workspace'/name)==h,name
parent_fixture=a.build/'harness/src/closing_fixture.rs';body=parent_fixture.read_text();marker='\n#[path="closing-recovery-tests.rs"]';assert body.count(marker)==1
common=body.split(marker,1)[0]
for folder in ['artifacts','harness']:shutil.copytree(a.build/folder,a.output/folder,ignore=shutil.ignore_patterns('target','test_snapshots'))
(a.output/'harness/src/mixed_fixture.rs').write_text(common+'\n#[path="mixed-funding-tests.rs"]\nmod mixed_cases;\n')
original=(HERE/'mixed-funding-tests.rs').read_text()
needle='const V3: &str = "'+EXPECTED_V3+'";';assert original.count(needle)==1
generated=original.replace(needle,'const V3: &str = "'+actual_v3+'";')
(a.output/'harness/src/mixed-funding-tests.rs').write_text(generated)
f=a.output/'harness/src/test.rs';f.write_text(f.read_text()+'\n#[path="mixed_fixture.rs"]\nmod mixed_qualification;\n')
m.update({'mixed_funding_companion':True,'mixed_test_sha256':sha(a.output/'harness/src/mixed-funding-tests.rs'),'mixed_input_test_sha256':sha(HERE/'mixed-funding-tests.rs'),'mixed_platform_rebind_requested':a.rebind_reviewed_platform,'mixed_platform_byte_identity_changed':actual_v3!=EXPECTED_V3,'mixed_measured_mac_v3_sha256':EXPECTED_V3,'mixed_prepare_sha256':sha(Path(__file__)),'mixed_parent_build':str(a.build),'mixed_parent_manifest_sha256':sha(a.build/'build-manifest.json'),'mixed_parent_fixture_sha256':sha(parent_fixture),'mixed_common_fixture_prefix_sha256':hashlib.sha256(common.encode()).hexdigest(),'mixed_scope':'Separate companion: positive held proceeds plus external deficit in one atomic settlement; public-operation reserved fee preservation; no frozen-seven-test edits','contract_rebuilt_for_mixed_companion':False})
m['harness_generated_sources']={str(f.relative_to(a.output)):sha(f) for f in sorted((a.output/'harness/src').glob('*.rs'))}
(a.output/'build-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
subprocess.run(['python3',str(HERE.parent/'cl-storage-qualification/identify_build.py'),'--build',str(a.output)],check=True)
print(a.output)
