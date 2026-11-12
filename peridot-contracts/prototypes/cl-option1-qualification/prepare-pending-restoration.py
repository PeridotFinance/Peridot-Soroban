#!/usr/bin/env python3
"""New pending ABIs through conditional restoration; synthetic re-expiry is explicit."""
import argparse,hashlib,json,shutil
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--build',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
here=Path(__file__).resolve().parent;root=here.parents[1];rest=here.parent/'cl-restoration-preparation'
assert not a.output.exists()
m=json.loads((a.build/'build-manifest.json').read_text())
patches=[here.parent/'cl-atomic-qualification/optimization.patch',here.parent/'cl-storage-qualification/storage.patch',here.parent/'cl-storage-qualification/repayment.patch',here/'pending-recovery.patch',here/'pending-unhealthy.patch']
assert m['optimization_sha256']==hashlib.sha256(b''.join(f.read_bytes() for f in patches)).hexdigest()
assert m['margin_build_feature']=='funded-liquidation-validation'
for name,sha in m['harness_generated_sources'].items():assert hashlib.sha256((a.build/name).read_bytes()).hexdigest()==sha
for name,pin in m['wasm'].items():assert hashlib.sha256((a.build/'artifacts'/name).read_bytes()).hexdigest()==pin['sha256']
a.output.mkdir(parents=True)
for d in ['artifacts','harness']:shutil.copytree(a.build/d,a.output/d)
f=a.output/'harness/src/transaction_evidence.rs';s=f.read_text();start=s.index('fn simulate(\n');end=s.index('fn simulate_one(\n',start)
s=s[:start]+'''#[path="restoration.rs"]
mod restoration;
fn simulate(e:&Env,to:&Address,name:&str,args:&SVec<Val>,external:Option<&Address>,enforce_empty_auth:bool,wrong_args:Option<&SVec<Val>>)->Result<Value,String>{
    let mut baseline=simulate_one(e,to,name,args,external,enforce_empty_auth,wrong_args,None)?;
    if baseline["success"]==true && matches!(name,"recover_expired_pending_funded"|"liquidate_pending_open_funded") {
        baseline["restoration_journey"]=restoration::journey(e,to,name,args,external,&baseline)?;
    }
    Ok(baseline)
}
'''+s[end:]
s=s.replace('let mut snapshot = e.to_ledger_snapshot();','let mut snapshot = restoration::OVERRIDE.with(|cell|cell.borrow_mut().take()).unwrap_or_else(||e.to_ledger_snapshot());',1);f.write_text(s)
s=(rest/'restoration.rs').read_text()
s=s.replace('let mut partial = Value::Null;', 'let mut partial = Value::Null;\n        let mut reexpiry = Value::Null;',1)
needle='            state = resumed;';assert needle in s
s=s.replace(needle,needle+'''\n            if i==0 { reexpiry = synthetic_reexpiry_probe(&mut state,keys,&network,&selected,warm,name)?; }''',1)
needle='"intervening_pool_trade":intervening,';assert needle in s
s=s.replace(needle,needle+'''"synthetic_reexpiry_probe":reexpiry,"journey_fee_total_with_synthetic_reexpiry_probe":final_fee.map(|v|v+sum+reexpiry["extra_fee"].as_i64().unwrap()),"transaction_count_with_synthetic_reexpiry_probe":prefix.len()+2,''',1)
s+='''
// Controlled test-state TTL fault, NOT passage of time or actual Core expiry.
// Re-preparation is charged separately and its exact state is used by the
// subsequent financial invocation. Every original main-journey fee is retained.
fn synthetic_reexpiry_probe(state:&mut LedgerSnapshot,keys:&[LedgerKey],network:&NetworkConfig,p:&Value,warm:&Value,flow:&str)->Result<Value,String>{
    struct ReportGuard(String);
    impl Drop for ReportGuard {fn drop(&mut self){std::env::set_var("CL_RESTORE_REPORT",&self.0);}}
    let prepared_hash=snap_hash(state);let financial_hash=full_data_hash(state);
    let mut expired=state.clone();let sequence=expired.ledger_info().sequence_number;
    for key in keys {let (_,(_,ttl))=expired.ledger_entries.iter_mut().find(|(k,_)|k.as_ref()==key).unwrap();assert!(ttl.unwrap()>=sequence);*ttl=Some(sequence-1);}
    assert_eq!(full_data_hash(&expired),financial_hash);
    let expired_hash=snap_hash(&expired);assert_ne!(expired_hash,prepared_hash);
    let bytes=serde_json::to_vec(&expired).unwrap();expired=serde_json::from_slice(&bytes).unwrap();
    let old=std::env::var("CL_RESTORE_REPORT").unwrap();let _guard=ReportGuard(old);
    std::env::set_var("CL_RESTORE_REPORT",std::env::var("CL_PENDING_REEXPIRY_BATCHES").unwrap());
    let repeated=batch(&mut expired,keys,network,p,warm,flow,0)?;
    assert_eq!(snap_hash(&expired),prepared_hash,"re-prepared prefix must restore exact intended snapshot");
    assert_eq!(full_data_hash(&expired),financial_hash);
    *state=expired;
    let row=json!({"schema":"cl-pending-synthetic-reexpiry-v1","profile":p["name"],"flow":flow,"source_sha256":hash(include_bytes!("restoration.rs")),"prepared_snapshot_sha256":prepared_hash,"expired_snapshot_sha256":expired_hash,"reprepared_snapshot_sha256":snap_hash(state),"financial_data_sha256":financial_hash,"financial_data_unchanged":true,"fixture_ledger_time_advance":0,"expiry_mechanism":"explicit controlled TTL reset to current ledger minus one; not elapsed-time or Core expiry","serialized_interruption_before_reprepare":true,"extra_transaction_count":1,"extra_fee":repeated["fees"]["total"],"repeated_batch":repeated,"core_application_verified":false,"real_reexpiry_verified":false,"release_approval":false});
    append("CL_PENDING_REEXPIRY_REPORT",&row);Ok(row)
}
'''
(a.output/'harness/src/restoration.rs').write_text(s)
s=(rest/'postconditions.rs').read_text()
s=s.replace('"liquidate_funded_v3" | "release_debt_free_position_v3"','"liquidate_funded_v3" | "release_debt_free_position_v3" | "recover_expired_pending_funded" | "liquidate_pending_open_funded"')
s=s.replace('if flow == "liquidate_funded_v3" {','if matches!(flow,"liquidate_funded_v3"|"recover_expired_pending_funded"|"liquidate_pending_open_funded") {')
(a.output/'harness/src/postconditions.rs').write_text(s)
f=a.output/'harness/Cargo.toml';s=f.read_text().replace('[dependencies]','[dependencies]\nsoroban-simulation = "=29.0.1"\nsoroban-ledger-snapshot = "=29.0.1"');f.write_text(s)
lock=(rest/'runtime-Cargo.lock').read_text();old=json.loads((a.build/'build-manifest.json').read_text()).get('cargo_harness_identity',{}).get('name')
import re
name=re.search(r'^name = "([^"]+)"',s,re.M)[1]
lock=lock.replace('name = "cl-atomic-qualification-harness"','name = "'+name+'"')
(a.output/'harness/Cargo.lock').write_text(lock)
m['conditional_pending_restoration']={'ledger_time_advance':0,'reexpiry':'synthetic TTL reset, not real expiry','core_application_verified':False,'source_script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
m['harness_generated_sources']={str(f.relative_to(a.output)):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted((a.output/'harness/src').glob('*.rs'))}
(a.output/'build-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(a.output)
