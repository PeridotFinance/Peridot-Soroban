//! Controlled genesis, compiled contract, protocol29 archival qualification.
//! No account signatures, deployment, RPC or production state mutation.
#![cfg(test)]
use soroban_sdk::{Address, Env, IntoVal, Map, Symbol, Val, Vec, TryFromVal, testutils::{Address as _, Ledger}};
use serde_json::{json, Value};
#[path="../../../test-support/cl_resource_limits.rs"] mod cl_resource_limits;
#[path="../../../contracts/cl-share-pool-facade/src/resource_profile.rs"] mod resource_profile;
mod transaction_evidence;
mod financial_caps;
fn key(e:&Env,n:&str)->Vec<Val>{ Vec::from_array(e,[Symbol::new(e,n).into_val(e)]) }
fn put(e:&Env,a:&Address,n:&str,v:Val,instance:bool){e.as_contract(a,||{if instance{e.storage().instance().set(&key(e,n),&v)}else{e.storage().persistent().set(&key(e,n),&v)}})}
fn fixture()->(Env,Address,Address,Address,Address){
 let mut e=Env::default();e.set_config(soroban_sdk::testutils::EnvTestConfig{capture_snapshot_at_drop:false,..Default::default()});
 e.ledger().with_mut(|l|{l.sequence_number=100;l.timestamp=1000;l.max_entry_ttl=6_312_000;});
 cl_resource_limits::configure(&e);
 let dir=std::env::var("LENDING_ARCHIVE_WASM_DIR").expect("exact compiled artifact directory required");
 let load=|n:&str|std::fs::read(format!("{dir}/{n}.wasm")).unwrap();
 let r=e.register(load("receipt_vault").as_slice(),()); let c=e.register(load("simple_peridottroller").as_slice(),());let j=e.register(load("jump_rate_model").as_slice(),());
 let user=Address::generate(&e);
 for(n,v)in[("UnderlyingToken",user.clone().into_val(&e)),("Initialized",true.into_val(&e)),("Admin",user.clone().into_val(&e))]{put(&e,&r,n,v,false)}
 for(n,v)in[("MinCashReserveBps",1250u32.into_val(&e)),("CashBorrowingEnabled",false.into_val(&e)),("PermanentMinimumSupply",1000u128.into_val(&e))]{put(&e,&r,n,v,true)}
 put(&e,&j,"Admin",user.clone().into_val(&e),false);
 for(n,v)in[("BaseRatePerYear",10_000u128),("MultiplierPerYear",20_000),("JumpMultiplierPerYear",100_000),("Kink",800_000)]{put(&e,&j,n,v.into_val(&e),false)}
 resource_profile::register_contracts(&e,&[("receipt-instance",&r),("controller",&c),("rate",&j)]);
 (e,r,c,j,user)
}
fn run(e:&Env,to:&Address,name:&str,args:Vec<Val>,archive:Option<&str>,expect:Option<Val>)->Value{
 let row=transaction_evidence::simulate_one(e,to,name,&args,None,true,None,archive).unwrap();
 {use std::io::Write;let path=std::env::var("LENDING_ARCHIVE_REPORT").unwrap();let mut f=std::fs::OpenOptions::new().create(true).append(true).open(format!("{path}.attempts.jsonl")).unwrap();writeln!(f,"{}",json!({"function":name,"archive":archive,"evidence":row})).unwrap();}
 if let Some(v)=expect{use soroban_sdk::{TryFromVal,xdr::{ScVal,WriteXdr,Limits}};assert_eq!(row["success"],true,"{row}");assert_eq!(row["return_value_xdr"],ScVal::try_from_val(e,&v).unwrap().to_xdr_base64(Limits::none()).unwrap());assert_eq!(row["limits_pass"],true,"{row}");}else{assert_eq!(row["success"],false,"missing mandatory config must reject: {row}");}
 if archive.is_some(){assert_eq!(row["archive"]["restoration_observed"],true,"TTL-expired entries must be restored, not defaulted: {row}");}
 if archive.is_none() && expect.is_some(){
  cl_resource_limits::reset(e);let result:Val=e.invoke_contract(to,&Symbol::new(e,name),args);assert_eq!(soroban_sdk::xdr::ScVal::try_from_val(e,&result).unwrap(),soroban_sdk::xdr::ScVal::try_from_val(e,&expect.unwrap()).unwrap());
  let r=e.cost_estimate().resources();let mut record=json!({"flow":format!("lending_archive_{name}"),"ok":true,"scope":"compiled controlled-genesis warm SDK invocation; unsigned preflight restoration separately reported","instructions":r.instructions,"memory_bytes":r.mem_bytes,"read_entries":r.memory_read_entries,"write_entries":r.write_entries,"disk_read_entries":r.disk_read_entries,"read_bytes":r.disk_read_bytes,"write_bytes":r.write_bytes,"contract_events_size_bytes":r.contract_events_size_bytes,"persistent_rent_ledger_bytes":r.persistent_rent_ledger_bytes,"persistent_entry_rent_bumps":r.persistent_entry_rent_bumps,"temporary_rent_ledger_bytes":r.temporary_rent_ledger_bytes,"temporary_entry_rent_bumps":r.temporary_entry_rent_bumps});
  for(k,v)in cl_resource_limits::metadata().as_object().unwrap(){record[k]=v.clone();}for(k,v)in cl_resource_limits::diagnostics(e).as_object().unwrap(){record[k]=v.clone();}
  use std::io::Write;let mut f=std::fs::OpenOptions::new().create(true).append(true).open(format!("{}.sdk.jsonl",std::env::var("LENDING_ARCHIVE_REPORT").unwrap())).unwrap();writeln!(f,"{record}").unwrap();
 }
 row
}
#[test]
fn actual_compiled_archival_and_never_configured_states(){
 let(e,r,c,j,user)=fixture();let empty=Vec::new(&e);let mut rows=std::vec::Vec::new();
 let missing_policy=e.register(std::fs::read(format!("{}/receipt_vault.wasm",std::env::var("LENDING_ARCHIVE_WASM_DIR").unwrap())).unwrap().as_slice(),());
 put(&e,&missing_policy,"Initialized",true.into_val(&e),false);put(&e,&missing_policy,"UnderlyingToken",user.clone().into_val(&e),false);
 rows.push(json!({"label":"never-configured-reserve-is-full-cash-reserve","evidence":run(&e,&missing_policy,"get_min_cash_reserve_bps",empty.clone(),None,Some(Some(10_000u32).into_val(&e)))}));
 rows.push(json!({"label":"never-configured-borrowing-disabled","evidence":run(&e,&missing_policy,"get_cash_borrowing_enabled",empty.clone(),None,Some(false.into_val(&e)))}));
 e.to_ledger_snapshot().write_file(format!("{}.snapshot.json",std::env::var("LENDING_ARCHIVE_REPORT").unwrap())).unwrap();
 for(name,expected)in[("get_min_cash_reserve_bps",Some(1250u32).into_val(&e)),("get_cash_borrowing_enabled",false.into_val(&e)),("get_minimum_supply",1000u128.into_val(&e))]{
  for archive in [None,Some("receipt-instance")]{rows.push(json!({"label":format!("{name}-{}",if archive.is_some(){"expired-restored"}else{"warm"}),"evidence":run(&e,&r,name,empty.clone(),archive,Some(expected))}));}
 }
 // This key was NEVER configured. This is not an archival simulation.
 rows.push(json!({"label":"never-configured-pause-map-fails-closed","evidence":run(&e,&c,"is_borrow_paused",(user.clone(),).into_val(&e),None,Some(true.into_val(&e)))}));
 let mut flags=Map::<Address,bool>::new(&e);flags.set(user.clone(),true);put(&e,&c,"PauseBorrow",flags.into_val(&e),false);
 let mut until=Map::<Address,u64>::new(&e);until.set(user.clone(),10_000);put(&e,&c,"PauseBorrowUntil",until.into_val(&e),false);
 for archive in[None,Some("controller")]{rows.push(json!({"label":format!("pause-policy-{}",if archive.is_some(){"expired-restored"}else{"warm"}),"evidence":run(&e,&c,"is_borrow_paused",(user.clone(),).into_val(&e),archive,Some(true.into_val(&e)))}));}
 for archive in[None,Some("rate")]{rows.push(json!({"label":format!("rate-policy-{}",if archive.is_some(){"expired-restored"}else{"warm"}),"evidence":run(&e,&j,"get_borrow_rate",(100u128,0u128,0u128).into_val(&e),archive,Some(10_000u128.into_val(&e)))}));}
 let dir=std::env::var("LENDING_ARCHIVE_WASM_DIR").unwrap();let uninitialized=e.register(std::fs::read(format!("{dir}/jump_rate_model.wasm")).unwrap().as_slice(),());
 rows.push(json!({"label":"never-configured-rate-rejects","evidence":run(&e,&uninitialized,"get_borrow_rate",(100u128,0u128,0u128).into_val(&e),None,None)}));
 // A still-live instance below the threshold; exact data is unchanged.
 let mut short=e.to_ledger_snapshot();let address:soroban_sdk::xdr::ScAddress=(&r).into();
 for(k,(_,ttl))in &mut short.ledger_entries{if matches!(k.as_ref(),soroban_sdk::xdr::LedgerKey::ContractData(d) if d.contract==address && d.key==soroban_sdk::xdr::ScVal::LedgerKeyContractInstance){*ttl=Some(1100);}}
 let mut renewed=Env::from_ledger_snapshot(short);renewed.set_config(soroban_sdk::testutils::EnvTestConfig{capture_snapshot_at_drop:false,..Default::default()});cl_resource_limits::configure(&renewed);let receiver=Address::from_str(&renewed,&r.to_string().to_string());
 rows.push(json!({"label":"explicit-receipt-keeper-renews-instance","evidence":run(&renewed,&receiver,"bump_ttl",Vec::new(&renewed),None,Some(().into_val(&renewed)))}));
 let last=rows.last().unwrap();assert!(last["evidence"]["rent_changes"].as_array().unwrap().iter().any(|x|x["old_live_until_ledger"]==1100 && x["new_live_until_ledger"]==1_000_100),"keeper must renew exact short-lived instance");
 let out=std::env::var("LENDING_ARCHIVE_REPORT").expect("report path required");std::fs::write(out,serde_json::to_vec_pretty(&json!({"scope":"compiled controlled-genesis unsigned host29; exact data retained with TTL expiration; NOT Core application/live eviction/signature evidence","metadata":cl_resource_limits::metadata(),"rows":rows})).unwrap()).unwrap();
}
