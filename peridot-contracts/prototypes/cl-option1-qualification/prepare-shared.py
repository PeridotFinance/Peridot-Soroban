#!/usr/bin/env python3
"""Prepare a joint-state 8-borrower experiment from already built six-overlay WASMs.
No contract edits/builds/signing/network calls. Parent owns final combined builder.
"""
import argparse, hashlib, json, re, shutil, subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--build',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
here=Path(__file__).resolve().parent;root=here.parents[1];rest=here.parent/'cl-restoration-preparation'
a.build=a.build.resolve();a.output=a.output.resolve();assert not a.output.exists()
m=json.loads((a.build/'build-manifest.json').read_text())
patches=[here.parent/'cl-atomic-qualification/optimization.patch',here.parent/'cl-storage-qualification/storage.patch',here.parent/'cl-storage-qualification/repayment.patch',here/'pending-recovery.patch',here/'pending-unhealthy.patch',here/'closing-recovery.patch']
assert m['optimization_sha256']==hashlib.sha256(b''.join(f.read_bytes() for f in patches)).hexdigest(),'exact current six-overlay source required'
assert m['margin_build_feature']=='funded-liquidation-validation'
for name,sha in m['harness_generated_sources'].items():assert hashlib.sha256((a.build/name).read_bytes()).hexdigest()==sha,name
for name,pin in m['wasm'].items():assert hashlib.sha256((a.build/'artifacts'/name).read_bytes()).hexdigest()==pin['sha256'],name
for d in ['artifacts','harness']:shutil.copytree(a.build/d,a.output/d)
f=a.output/'harness/src/transaction_evidence.rs';s=f.read_text();start=s.index('fn simulate(\n');end=s.index('fn simulate_one(\n',start)
s=s[:start]+'''#[path="restoration.rs"]
mod restoration;
pub use restoration::shared_recovery::PlannedRoot;
pub fn shared_recovery(e:&Env,to:&Address,roots:&[PlannedRoot])->Value {restoration::shared_recovery::run(e,to,roots)}
fn simulate(e:&Env,to:&Address,name:&str,args:&SVec<Val>,external:Option<&Address>,enforce_empty_auth:bool,wrong_args:Option<&SVec<Val>>)->Result<Value,String>{simulate_one(e,to,name,args,external,enforce_empty_auth,wrong_args,None)}
'''+s[end:]
s=s.replace('let mut snapshot = e.to_ledger_snapshot();','let mut snapshot = restoration::OVERRIDE.with(|cell|cell.borrow_mut().take()).unwrap_or_else(||e.to_ledger_snapshot());',1)
needle='    Ok(row)\n}';assert s.count(needle)==1
s=s.replace(needle,'''    row["shared_exact_ledger_changes"] = json!(result.ledger_changes.iter().map(|change| {
        let key=LedgerKey::from_xdr(&change.encoded_key,Limits::none()).unwrap();
        json!({"key_xdr":b64(&key),"read_only":change.read_only,"new_entry_xdr":change.encoded_new_value.as_ref().map(|v|b64(&LedgerEntry::from_xdr(v,Limits::none()).unwrap())),"footprint_only_placeholder":footprint_only_contract_placeholder(change,&key),"old_entry_size_bytes_for_rent":change.old_entry_size_bytes_for_rent,"new_entry_size_bytes_for_rent":change.new_entry_size_bytes_for_rent,"old_live_until":change.ttl_change.as_ref().map(|t|t.old_live_until_ledger),"new_live_until":change.ttl_change.as_ref().map(|t|t.new_live_until_ledger)})
    }).collect::<std::vec::Vec<_>>());
'''+needle)
f.write_text(s)
s=(rest/'restoration.rs').read_text()
observed = '    row["sdk_observed_storage_keys_xdr"] = json!(fresh.host().get_stored_entries().unwrap().iter().map(|(key,_)|b64(key.as_ref())).collect::<std::vec::Vec<_>>());\n    row["sdk_key_scope"] = json!("accessed storage-map keys, including native auth setup; not access-mode footprint or SDK resource-entry count");\n'
first=s.index('fn sdk_after(');last=s.index('fn on_snapshot(',first)
discovery=s[first:last].replace('fn sdk_after(', 'fn shared_sdk_discovery(',1)
block='    if result.is_ok() {\n        row["postconditions"] = postconditions::check(s, &fresh, warm, flow);\n        row["reward_conservation"] = conservation(&fresh);\n    }\n    append("CL_RESTORE_SDK_REPORT", &row);'
assert block in discovery
discovery=discovery.replace(block,observed)
# Record original SDK invocation meter before post-execution assertion clones.
needle='    if result.is_ok() {\n        row["postconditions"]';assert needle in s
s=s.replace(needle,'''    {
        let resources=std::env::var("CL_RESOURCE_REPORT").unwrap();
        let calls=std::env::var("CL_CALL_PROFILE_REPORT").unwrap();
        if row["profile"]=="internal-policy" {
            std::env::set_var("CL_RESOURCE_REPORT",std::env::var("CL_SHARED_INTERNAL_RESOURCES").unwrap());
            std::env::set_var("CL_CALL_PROFILE_REPORT",std::env::var("CL_SHARED_INTERNAL_CALLS").unwrap());
        }
        crate::test::measure_scope(&fresh,flow,result.is_ok(),"compiled-shared-sequential-restoration-projection");
        std::env::set_var("CL_RESOURCE_REPORT",resources);
        std::env::set_var("CL_CALL_PROFILE_REPORT",calls);
    }
'''+needle,1)
s=s.replace('    if result.is_ok() {\n        row["postconditions"]',observed+'    if result.is_ok() {\n        row["postconditions"]',1)
s+='\n'+discovery
s+='\n#[path="shared-recovery.rs"]\npub(super) mod shared_recovery;\n'
(a.output/'harness/src/restoration.rs').write_text(s)
(a.output/'harness/src/shared-recovery.rs').write_bytes((here/'shared-recovery.rs').read_bytes())
s=(rest/'postconditions.rs').read_text().replace('"liquidate_funded_v3" | "release_debt_free_position_v3"','"liquidate_funded_v3" | "release_debt_free_position_v3" | "recover_expired_pending_funded"').replace('if flow == "liquidate_funded_v3" {','if matches!(flow,"liquidate_funded_v3"|"recover_expired_pending_funded") {')
(a.output/'harness/src/postconditions.rs').write_text(s)
fixture=(here.parent/'cl-atomic-qualification/tests/combined.rs').read_text()
fixture+='''
#[test]
fn compiled_shared_eight_pending_recovery_joint_state() {
    use sha2::{Digest,Sha256};
    let build=std::path::PathBuf::from(std::env::var("CL_ATOMIC_BUILD_DIR").unwrap());
    let manifest:serde_json::Value=serde_json::from_slice(&std::fs::read(build.join("build-manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["shared_recovery_experiment"],true);
    for name in ["margin_controller.wasm","receipt_vault.wasm","simple_peridottroller.wasm"] {
        assert_eq!(manifest["wasm"][name]["sha256"],format!("{:x}",Sha256::digest(std::fs::read(build.join("artifacts").join(name)).unwrap())));
    }
    let (f,owners,ids,selected)=fixture(false,true);let e=&f.e;
    let mut plans=std::vec::Vec::new();let mut expiry=0u64;
    for (i,id) in ids.iter().enumerate() {
        let payer=fresh_liquidator(&f,&owners);
        auth::<()>(e,&f.user,&f.under,"transfer",(f.user.clone(),payer.clone(),300_000_000i128).into_val(e),&[]);
        let pending:Option<Map<Symbol,Val>>=inspect(&f,&f.margin,"get_pending_perps_open",(*id,).into_val(e));
        expiry=expiry.max(field(e,&pending.unwrap(),"expires_at"));
        // Establish every explicitly synthetic external authorizer marker before
        // freezing the ONE joint state; no contract's own auth is mocked.
        let tx=MockAuthInvoke{contract:&f.under,fn_name:"transfer",args:(payer.clone(),f.cash.clone(),MAX_REPAY as i128).into_val(e),sub_invokes:&[]};
        let transfers=[tx];let repay=MockAuthInvoke{contract:&f.cash,fn_name:"repay_full_for_margin",args:(*id,payer.clone(),MAX_REPAY).into_val(e),sub_invokes:&transfers};
        let repays=[repay];
        e.mock_auths(&[MockAuth{address:&payer,invoke:&MockAuthInvoke{contract:&f.margin,fn_name:"recover_expired_pending_funded",args:(payer.clone(),*id,MAX_REPAY,1u128).into_val(e),sub_invokes:&repays}}]);
        plans.push(crate::transaction_evidence::PlannedRoot{payer,owner:owners[i+1].clone(),id:*id,maximum:MAX_REPAY});
    }
    advance(&f,expiry+3600);
    auth::<()>(e,&f.admin,&f.cash,"set_cash_borrowing_enabled",(f.admin.clone(),false).into_val(e),&[]);
    fault(&f);
    // Verify both dependency calls actually fail before the frozen joint state.
    // Setup diagnostics are not counted as one of the eight financial roots.
    prove_both_calls_fail(&f);
    enable_measurement(&f,&selected);
    let report=crate::transaction_evidence::shared_recovery(e,&f.margin,&plans);
    for row in report.as_array().unwrap() {std::println!("SHARED_EIGHT_RECOVERY profile={} completed={} union_keys={} preparation_fee={} complete_journey_fee={} release_approval=false",row["profile"],row["completed_borrowers"],row["required_union_keys"],row["preparation_fee"],row["complete_journey_fee"]);}
}
'''
(a.output/'harness/src/shared_fixture.rs').write_text(fixture)
f=a.output/'harness/src/test.rs';f.write_text(f.read_text()+'\n#[path="shared_fixture.rs"]\nmod shared_qualification;\n')
f=a.output/'harness/Cargo.toml';s=f.read_text()
if 'soroban-simulation' not in s:s=s.replace('[dependencies]','[dependencies]\nsoroban-simulation = "=29.0.1"\nsoroban-ledger-snapshot = "=29.0.1"')
f.write_text(s)
name=re.search(r'^name = "([^"]+)"',s,re.M)[1]
lock=(rest/'runtime-Cargo.lock').read_text().replace('name = "cl-atomic-qualification-harness"','name = "'+name+'"')
(a.output/'harness/Cargo.lock').write_text(lock)
m.update({'shared_recovery_experiment':True,'shared_parent_build':str(a.build),'shared_prepare_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'shared_engine_sha256':hashlib.sha256((here/'shared-recovery.rs').read_bytes()).hexdigest(),'shared_evidence_scope':'one joint prestate; sequential exact host changes and SDK checks; native restoration TTL projection only','release_approval':False})
m['harness_generated_sources']={str(f.relative_to(a.output)):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted((a.output/'harness/src').glob('*.rs'))}
(a.output/'build-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
subprocess.run(['python3',str(here.parent/'cl-storage-qualification/identify_build.py'),'--build',str(a.output)],check=True)
print(a.output)
