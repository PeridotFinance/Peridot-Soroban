//! Private local test accounts only; no keys, wallet config or RPC access.
use super::*;
use soroban_sdk::{xdr::*, TryFromVal, Val, testutils::{Ledger,Events}};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
const PRIVATE_NETWORK:&str="Peridot Stage1 signature application tests ONLY 2026-11-10";
#[test]
fn compiled_private_signed_root() {
    let input:Value=serde_json::from_slice(&std::fs::read(std::env::var("CL_PRIVATE_INPUT").unwrap()).unwrap()).unwrap();
    assert_eq!(input["network_passphrase"],PRIVATE_NETWORK);
    let snapshot_path=input["snapshot_path"].as_str().unwrap();
    let mut snapshot=Env::from_ledger_snapshot_file(snapshot_path).to_ledger_snapshot();
    snapshot.network_id=Sha256::digest(PRIVATE_NETWORK.as_bytes()).into();
    snapshot.sequence_number=input["ledger"].as_u64().unwrap() as u32;
    snapshot.timestamp=input["timestamp"].as_u64().unwrap();
    snapshot.ledger_entries=input["entries"].as_array().unwrap().iter().map(|v|{
        let le=LedgerEntry::from_xdr_base64(v["entry_xdr"].as_str().unwrap(),Limits::none()).unwrap();
        let key=LedgerKey::from_xdr_base64(v["key_xdr"].as_str().unwrap(),Limits::none()).unwrap();
        (Box::new(key),(Box::new(le),v["ttl"].as_u64().map(|n|n as u32)))
    }).collect();
    let mut e=Env::from_ledger_snapshot(snapshot);
    e.set_config(soroban_sdk::testutils::EnvTestConfig{capture_snapshot_at_drop:false,..Default::default()});
    assert_eq!(e.ledger().get().network_id.as_slice(),Sha256::digest(PRIVATE_NETWORK.as_bytes()).as_slice());
    crate::cl_resource_limits::configure(&e);
    let source=AccountId::from_xdr_base64(input["source_account_xdr"].as_str().unwrap(),Limits::none()).unwrap();
    e.host().set_source_account(source).unwrap();
    let hf=HostFunction::from_xdr_base64(input["host_function_xdr"].as_str().unwrap(),Limits::none()).unwrap();
    let HostFunction::InvokeContract(invoke)=hf else {panic!("financial root only")};
    let to=Address::try_from_val(&e,&ScVal::Address(invoke.contract_address)).unwrap();
    let name=std::str::from_utf8(invoke.function_name.0.as_slice()).unwrap();
    let native=invoke.args.iter().map(|v|Val::try_from_val(&e,v).unwrap()).collect::<std::vec::Vec<_>>();
    let args=Vec::from_slice(&e,&native);
    let mut result=crate::transaction_evidence::simulate_one(&e,&to,name,&args,None,false,None,None).unwrap();
    for(k,v)in crate::cl_resource_limits::metadata().as_object().unwrap(){result[k]=v.clone();}
    result["scope"]=json!("private-local-test-ledger");result["network_passphrase"]=json!(PRIVATE_NETWORK);
    result["account_authorization_mode"]=input["mode"].clone();
    std::fs::write(input["result_path"].as_str().unwrap(),serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    assert_eq!(result["success"],true,"{:?}",result["error"]);
    if input["mode"]!="record" {
        let auths:std::vec::Vec<_>=input["auth_xdr"].as_array().unwrap().iter().map(|v|SorobanAuthorizationEntry::from_xdr_base64(v.as_str().unwrap(),Limits::none()).unwrap()).collect();
        e.set_auths(&auths);crate::cl_resource_limits::reset(&e);
        let _:Val=e.invoke_contract(&to,&Symbol::new(&e,name),args);
        crate::test::measure_scope(&e,"signed_private",true,"private-local-signed-account-authorization");
        let _=e.events().all();
    }
}

#[test]
fn export_existing_snapshots_as_xdr() {
    let dir=std::path::PathBuf::from(std::env::var("CL_SNAPSHOT_EXPORT_DIR").unwrap());
    for f in std::fs::read_dir(&dir).unwrap() {
        let p=f.unwrap().path();if !p.to_string_lossy().ends_with(".snapshot.json"){continue;}
        let snapshot=Env::from_ledger_snapshot_file(&p).to_ledger_snapshot();
        let rows=snapshot.ledger_entries.iter().map(|(k,(e,t))|json!({"entry_xdr":e.to_xdr_base64(Limits::none()).unwrap(),"key_xdr":k.to_xdr_base64(Limits::none()).unwrap(),"ttl":t})).collect::<std::vec::Vec<_>>();
        let packet=json!({"entries":rows,"timestamp":snapshot.timestamp,"ledger":snapshot.sequence_number,"network_id":snapshot.network_id});
        std::fs::write(p.with_extension("packet.json"),serde_json::to_vec(&packet).unwrap()).unwrap();
    }
}
