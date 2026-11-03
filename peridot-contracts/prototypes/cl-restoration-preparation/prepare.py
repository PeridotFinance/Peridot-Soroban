#!/usr/bin/env python3
"""Add isolated restoration evidence to a hash-checked combined build."""
import argparse,hashlib,json,shutil
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--build',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
assert not a.output.exists()
m=json.loads((a.build/'build-manifest.json').read_text())
source=Path(__file__).resolve().parent
expected_overlay=(source.parent/'cl-atomic-qualification/optimization.patch').read_bytes()+(source.parent/'cl-storage-qualification/storage.patch').read_bytes()+(source.parent/'cl-storage-qualification/repayment.patch').read_bytes()
assert m['optimization_sha256']==hashlib.sha256(expected_overlay).hexdigest(), 'Requires the exact combined three-patch build'
assert m['margin_build_feature']=='funded-liquidation-validation'
assert m['contract_cache_scope']=='fresh build-specific directory; never reuse previous workspace contract outputs'
for f,h in m['harness_generated_sources'].items():assert hashlib.sha256((a.build/f).read_bytes()).hexdigest()==h
for f,pin in m['wasm'].items():assert hashlib.sha256((a.build/'artifacts'/f).read_bytes()).hexdigest()==pin['sha256']
a.output.mkdir(parents=True)
for d in ['artifacts','harness']:shutil.copytree(a.build/d,a.output/d)
f=a.output/'harness/src/transaction_evidence.rs';s=f.read_text()
start=s.index('fn simulate(\n');end=s.index('fn simulate_one(\n',start)
s=s[:start]+'''#[path = "restoration.rs"]
mod restoration;
pub fn set_restoration_trade_actor(actor:&Address) { restoration::set_trade_actor(actor); }
fn simulate(e: &Env, to: &Address, name: &str, args: &SVec<Val>, external: Option<&Address>, enforce_empty_auth: bool, wrong_args: Option<&SVec<Val>>) -> Result<Value,String> {
    let mut baseline = simulate_one(e,to,name,args,external,enforce_empty_auth,wrong_args,None)?;
    if baseline["success"] == true && matches!(name,"swap_open_position_v3"|"activate_open_position_v3"|"swap_close_position_v3"|"liquidate_funded_v3"|"repay_full_margin_position_v3"|"release_debt_free_position_v3"|"recover_pending_open_funded") {
        baseline["restoration_journey"] = restoration::journey(e,to,name,args,external,&baseline)?;
    }
    Ok(baseline)
}
''' +s[end:]
s=s.replace('let mut snapshot = e.to_ledger_snapshot();','let mut snapshot = restoration::OVERRIDE.with(|cell| cell.borrow_mut().take()).unwrap_or_else(|| e.to_ledger_snapshot());',1)
f.write_text(s)
shutil.copyfile(source/'restoration.rs',a.output/'harness/src/restoration.rs')
shutil.copyfile(source/'postconditions.rs',a.output/'harness/src/postconditions.rs')
c=a.output/'harness/Cargo.toml';s=c.read_text().replace('[dependencies]','[dependencies]\nsoroban-simulation = "=29.0.1"\nsoroban-ledger-snapshot = "=29.0.1"');c.write_text(s)
shutil.copyfile(source/'runtime-Cargo.lock',a.output/'harness/Cargo.lock')
# Use the final focused module when available, never alter preserved fixtures.
tests=(source/'combined-repayment-tests.rs').read_text()
for name in ['storage','repayment']:
 tests=tests.replace('../cl-storage-qualification/'+name+'.patch',name+'.patch')
 shutil.copyfile(source.parent/'cl-storage-qualification'/f'{name}.patch',a.output/'harness/src'/f'{name}.patch')
(a.output/'harness/src/combined-repayment-tests.rs').write_text(tests)
r=a.output/'harness/src/repayment_fixture.rs';s=r.read_text();s=s[:s.rindex('#[path =')]+ '#[path = "combined-repayment-tests.rs"]\nmod combined_repayment_cases;\n';r.write_text(s)
m['qualification_variant']='combined-storage-and-full-repayment'
m['combined_patch_sha256']={name:hashlib.sha256((source.parent/'cl-storage-qualification'/f'{name}.patch').read_bytes()).hexdigest() for name in ['storage','repayment']}
m['restoration_prototype']={'source_sha256':hashlib.sha256((source/'restoration.rs').read_bytes()).hexdigest(),'preparation_core_application_verified':False,'financial_stages_changed':False}
m['harness_generated_sources']={str(f.relative_to(a.output)):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted((a.output/'harness/src').glob('*.rs'))}
m['included_patch_sources']={str(f.relative_to(a.output)):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted((a.output/'harness/src').glob('*.patch'))}
(a.output/'build-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(a.output)
