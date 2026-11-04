//! Test-only profiles. Never changes contract policy or treats diagnostics as release approval.
extern crate std;
use soroban_sdk::{Bytes, Env};
use soroban_env_host::InvocationResourceLimits;
use serde_json::{Value,json};
use std::{format,string::String};

pub fn profile() -> Value {
    let path=std::env::var("CL_RESOURCE_PROFILE").expect("CL_RESOURCE_PROFILE is required; no implicit limits");
    let p:Value=serde_json::from_slice(&std::fs::read(path).expect("profile unreadable")).expect("invalid profile JSON");
    assert_eq!(p["schema"],"cl-resource-profile-v1");
    assert_eq!(p["release_approval"],false);
    assert!(matches!(p["name"].as_str(),Some("internal-policy"|"captured-network-diagnostic")));
    assert!(matches!(p["mode"].as_str(),Some("acceptance"|"profiling-only")));
    p
}
fn n(p:&Value,k:&str)->u64 {let v=p["limits"][k].as_u64().expect("missing positive resource limit");assert!(v>0);v}
pub fn metadata()->Value {let p=profile();json!({"profile":p["name"],"run_mode":p["mode"],"runtime_protocol":soroban_env_host::meta::INTERFACE_VERSION.protocol,"target_protocol":p["target_protocol"],"config_sha256":p["network_sha256"],"diagnostic_shadow_cpu_limit":20_000_000_000u64,"diagnostic_shadow_memory_limit":4_294_967_296u64,"sdk_limits_enforced":true})}

pub fn diagnostics(e:&Env)->Value {
 let b=e.host().budget_cloned();
 json!({"diagnostic_shadow_cpu_used":b.get_shadow_cpu_insns_consumed().unwrap(),"diagnostic_shadow_memory_used":b.get_shadow_mem_bytes_consumed().unwrap(),"diagnostic_shadow_ok":!b.shadow_cpu_limit_exceeded().unwrap() && !b.shadow_mem_limit_exceeded().unwrap(),"exact_captured_cost_model":soroban_env_host::meta::INTERFACE_VERSION.protocol as u64==profile()["target_protocol"].as_u64().unwrap()})
}

/// Validate exact host default coefficients against captured model coefficients.
/// This uses the pinned host's public Debug representation, whose last four
/// columns are raw const/linear CPU/memory coefficients. Any format/count/value
/// difference fails closed for matching-runtime acceptance; no coefficient edits.
fn model_matches(e:&Env,c:&Value)->bool {
    use soroban_sdk::xdr::ContractCostType;
    let debug=format!("{:?}",e.host().budget_cloned());
    let cpu=c["settings"]["configSettingContractCostParamsCpuInstructions"]["values"].as_array().expect("missing CPU model");
    let mem=c["settings"]["configSettingContractCostParamsMemoryBytes"]["values"].as_array().expect("missing memory model");
    let variants=ContractCostType::variants();
    if cpu.len()!=variants.len() || mem.len()!=variants.len(){return false;}
    for ty in variants {
        let i=ty as usize;
        let name=format!("{ty:?}");
        let line=debug.lines().find(|l|l.split_whitespace().next()==Some(name.as_str())).expect("host cost Debug format changed");
        let cols:std::vec::Vec<_>=line.split_whitespace().collect();
        let nums:std::vec::Vec<u64>=cols[cols.len()-4..].iter().map(|x|x.parse().expect("host raw cost coefficient parse failed")).collect();
        let expected=[cpu[i]["constTerm"].as_u64().unwrap(),cpu[i]["linearTerm"].as_u64().unwrap(),mem[i]["constTerm"].as_u64().unwrap(),mem[i]["linearTerm"].as_u64().unwrap()];
        if nums!=expected {std::println!("COST_MODEL_MISMATCH {name}: host={nums:?} captured={expected:?}");return false;}
    }
    true
}
pub fn configure(e:&Env) {
    let p=profile();
    let raw=std::fs::read(p["network_config_path"].as_str().expect("missing config path")).expect("captured config unreadable");
    let hash=e.crypto().sha256(&Bytes::from_slice(e,&raw)).to_array();
    let hex:String=hash.iter().map(|b|format!("{b:02x}")).collect();assert_eq!(p["network_sha256"].as_str().unwrap(),hex);
    let c:Value=serde_json::from_slice(&raw).expect("invalid captured config");
    assert_eq!(c["network"]["passphrase"],"Public Global Stellar Network ; September 2015");
    assert_eq!(p["captured_ledger"],c["ledger"]);assert_eq!(p["target_protocol"],c["network"]["protocolVersion"]);
    let mappings=[
      ("instructions","configSettingContractComputeV0",Some("txMaxInstructions"),100_000_000u64),
      ("mem_bytes","configSettingContractComputeV0",Some("txMemoryLimit"),41_943_040),
      ("disk_read_entries","configSettingContractLedgerCostV0",Some("txMaxDiskReadEntries"),100),
      ("write_entries","configSettingContractLedgerCostV0",Some("txMaxWriteLedgerEntries"),50),
      ("ledger_entries","configSettingContractLedgerCostExtV0",Some("txMaxFootprintEntries"),100),
      ("disk_read_bytes","configSettingContractLedgerCostV0",Some("txMaxDiskReadBytes"),200_000),
      ("write_bytes","configSettingContractLedgerCostV0",Some("txMaxWriteBytes"),132_096),
      ("contract_events_size_bytes","configSettingContractEventsV0",Some("txMaxContractEventsSizeBytes"),16_384),
      ("max_contract_data_key_size_bytes","configSettingContractDataKeySizeBytes",None,250),
      ("max_contract_data_entry_size_bytes","configSettingContractDataEntrySizeBytes",None,65_536),
      ("max_contract_code_entry_size_bytes","configSettingContractMaxSizeBytes",None,131_072),
      ("transaction_size_bytes","configSettingContractBandwidthV0",Some("txMaxSizeBytes"),132_096)];
    assert_eq!(p["limits"].as_object().unwrap().len(),mappings.len());
    for (field,setting,key,cap) in mappings {
       let raw=&c["settings"][setting]["values"];
       let captured=if let Some(k)=key {raw[k].as_u64().expect("missing captured limit")}else{raw.as_u64().expect("missing captured scalar")};
       let expected=if p["name"]=="internal-policy"{captured.min(cap)}else{captured};
       assert_eq!(n(&p,field),expected,"profile limit must match validated capture/policy: {field}");
    }
    for setting in ["configSettingContractCostParamsCpuInstructions","configSettingContractCostParamsMemoryBytes"]{
       for entry in c["settings"][setting]["values"].as_array().expect("costs required") {
         assert_eq!(entry["ext"],json!({"variant":0}));
         entry["constTerm"].as_u64().expect("invalid cost const");entry["linearTerm"].as_u64().expect("invalid cost linear");
       }
    }
    // Match captured archival settings too: rent and TTL changes are measured.
    // Keep the synthetic network ID; validation contracts explicitly reject Mainnet.
    use soroban_sdk::testutils::Ledger;
    let archival=&c["settings"]["configSettingStateArchival"]["values"];
    e.ledger().with_mut(|l| {
        l.min_persistent_entry_ttl=archival["minPersistentTtl"].as_u64().unwrap().try_into().unwrap();
        l.min_temp_entry_ttl=archival["minTemporaryTtl"].as_u64().unwrap().try_into().unwrap();
        l.max_entry_ttl=archival["maxEntryTtl"].as_u64().unwrap().try_into().unwrap();
    });
    let host=soroban_env_host::meta::INTERFACE_VERSION.protocol;
    let matched=model_matches(e,&c);
    if host as u64==p["target_protocol"].as_u64().unwrap(){assert!(matched,"captured cost model differs; definitive execution forbidden");}
    std::println!("RESOURCE_PROFILE {} host_protocol={host} exact_captured_cost_model={matched}",p);
    reset(e);
}
pub fn reset(e:&Env) {
    let p=profile();let profiling=p["mode"]=="profiling-only";
    let cpu=if profiling{2_000_000_000}else{n(&p,"instructions")};
    let mem=if profiling{536_870_912}else{n(&p,"mem_bytes")};
    e.cost_estimate().budget().reset_limits(cpu,mem);
    // Optional SDK auth/tree/storage snapshots use the shadow meter. This finite
    // diagnostic allowance does not alter any contract execution ceiling.
    e.cost_estimate().enforce_resource_limits(InvocationResourceLimits {
        instructions:cpu.try_into().unwrap(),mem_bytes:mem.try_into().unwrap(),
        disk_read_entries:n(&p,"disk_read_entries").try_into().unwrap(),write_entries:n(&p,"write_entries").try_into().unwrap(),ledger_entries:n(&p,"ledger_entries").try_into().unwrap(),
        disk_read_bytes:n(&p,"disk_read_bytes").try_into().unwrap(),write_bytes:n(&p,"write_bytes").try_into().unwrap(),contract_events_size_bytes:n(&p,"contract_events_size_bytes").try_into().unwrap(),max_contract_data_key_size_bytes:n(&p,"max_contract_data_key_size_bytes").try_into().unwrap(),max_contract_data_entry_size_bytes:n(&p,"max_contract_data_entry_size_bytes").try_into().unwrap(),max_contract_code_entry_size_bytes:n(&p,"max_contract_code_entry_size_bytes").try_into().unwrap(),
    });
    e.host().set_shadow_budget_limits(20_000_000_000,4_294_967_296).unwrap();
}
