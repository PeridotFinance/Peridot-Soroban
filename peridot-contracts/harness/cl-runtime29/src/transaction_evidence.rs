//! Unsigned controlled-state preflight using the official protocol29 embedder API.
//! Recording external authorization is NOT a signature proof. Any recorded
//! controller/vault authorization is forbidden rather than silently substituted.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use soroban_env_host::{
    budget::Budget,
    e2e_invoke::{
        extract_rent_changes, invoke_host_function_in_recording_mode, RecordingInvocationAuthMode,
    },
    fees::*,
    xdr::*,
};
use soroban_sdk::{Address, Env, TryFromVal, Val, Vec as SVec};
use std::{io::Write, rc::Rc};
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn b64<T: WriteXdr>(v: &T) -> String {
    v.to_xdr_base64(Limits::none()).unwrap()
}
fn encoded_len<T: WriteXdr>(v: &T) -> u32 {
    v.to_xdr(Limits::none()).unwrap().len().try_into().unwrap()
}
fn setting<'a>(c: &'a Value, key: &str) -> &'a Value {
    &c["settings"][key]["values"]
}
fn num(v: &Value, key: &str) -> i64 {
    v[key].as_i64().unwrap()
}
fn params(c: &Value, key: &str) -> ContractCostParams {
    ContractCostParams(
        setting(c, key)
            .as_array()
            .unwrap()
            .iter()
            .map(|p| ContractCostParamEntry {
                ext: ExtensionPoint::V0,
                const_term: num(p, "constTerm"),
                linear_term: num(p, "linearTerm"),
            })
            .collect::<std::vec::Vec<_>>()
            .try_into()
            .unwrap(),
    )
}
fn addr(e: &Env, a: &Address) -> ScAddress {
    match ScVal::try_from_val(e, &a.to_val()).unwrap() {
        ScVal::Address(x) => x,
        _ => unreachable!(),
    }
}
fn strkey(a: &Address) -> String {
    let s = a.to_string();
    let mut b = vec![0u8; s.len() as usize];
    s.copy_into_slice(&mut b);
    String::from_utf8(b).unwrap()
}
fn envelope(
    source: &AccountId,
    hf: &HostFunction,
    auth: &[SorobanAuthorizationEntry],
    resources: &SorobanResources,
    restored: &[u32],
    resource_fee: i64,
    total: u32,
) -> TransactionEnvelope {
    let source_account = match &source.0 {
        PublicKey::PublicKeyTypeEd25519(k) => MuxedAccount::Ed25519(k.clone()),
    };
    TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: Transaction {
            source_account,
            fee: total,
            seq_num: SequenceNumber(1),
            cond: Preconditions::Time(TimeBounds {
                min_time: TimePoint(0),
                max_time: TimePoint(0),
            }),
            memo: Memo::None,
            operations: vec![Operation {
                source_account: None,
                body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                    host_function: hf.clone(),
                    auth: auth.to_vec().try_into().unwrap(),
                }),
            }]
            .try_into()
            .unwrap(),
            ext: TransactionExt::V1(SorobanTransactionData {
                ext: SorobanTransactionDataExt::V1(SorobanResourcesExtV0 {
                    archived_soroban_entries: restored.to_vec().try_into().unwrap(),
                }),
                resources: resources.clone(),
                resource_fee,
            }),
        },
        signatures: VecM::default(),
    })
}
pub fn capture_before(
    e: &Env,
    to: &Address,
    name: &str,
    args: &SVec<Val>,
    external: Option<&Address>,
) {
    capture(e, to, name, args, external, false, None, None);
}
/// Negative proofs still execute the actual compiled code. Empty enforcement is
/// required when absence of account authorization is the condition being tested.
pub fn capture_rejection_before(
    e: &Env,
    to: &Address,
    name: &str,
    args: &SVec<Val>,
    external: Option<&Address>,
    enforce_empty_auth: bool,
    proof_case: &str,
) {
    assert!(!proof_case.is_empty(), "rejection proof case is required");
    capture(
        e,
        to,
        name,
        args,
        external,
        enforce_empty_auth,
        Some(proof_case),
        None,
    );
}
/// A deliberately mismatched external root authorization must reject before
/// account-signature evaluation. Never authorizes a controller/facade/vault.
pub fn capture_mismatched_root_rejection(e: &Env, to: &Address, name: &str,
    args: &SVec<Val>, external: &Address, wrong_args: &SVec<Val>, proof_case: &str) {
    assert_ne!(args, wrong_args);
    capture(e,to,name,args,Some(external),false,Some(proof_case),Some(wrong_args));
}
fn capture(
    e: &Env,
    to: &Address,
    name: &str,
    args: &SVec<Val>,
    external: Option<&Address>,
    enforce_empty_auth: bool,
    proof_case: Option<&str>,
    wrong_args: Option<&SVec<Val>>,
) {
    let Ok(path) = std::env::var("CL_TRANSACTION_REPORT") else {
        return;
    };
    let mut row = json!({"schema":"cl-transaction-evidence-v1","flow":name,"test_name":std::thread::current().name().unwrap_or("unnamed"),"success":false,"scope":"controlled-state-unsigned-simulation","engine":"soroban-env-host-e2e","account_authorization_mode":"recording-only","contract_authorization_enforced":false,"release_approval":false});
    for (k, v) in crate::cl_resource_limits::metadata().as_object().unwrap() {
        row[k] = v.clone();
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        simulate(e, to, name, args, external, enforce_empty_auth, wrong_args)
    }));
    match result {
        Ok(Ok(v)) => {
            for (k, x) in v.as_object().unwrap() {
                row[k] = x.clone();
            }
        }
        Ok(Err(err)) => {
            row["error"] = json!(err);
        }
        Err(_) => {
            row["error"] = json!("simulation setup panicked; not execution/auth proof");
        }
    }
    row["expected_rejection"] = json!(proof_case.is_some());
    row["proof_case"] = json!(proof_case);
    row["account_authorization_mode"] = json!(if wrong_args.is_some() {
        "enforcing-mismatched-root"
    } else if enforce_empty_auth {
        "enforcing-empty"
    } else {
        "recording-only"
    });
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(file, "{row}").unwrap();
}
fn simulate(
    e: &Env,
    to: &Address,
    name: &str,
    args: &SVec<Val>,
    external: Option<&Address>,
    enforce_empty_auth: bool,
    wrong_args: Option<&SVec<Val>>,
) -> Result<Value, String> {
    let baseline = simulate_one(e, to, name, args, external, enforce_empty_auth, wrong_args, None)?;
    if let Ok(role) = std::env::var("CL_TRANSACTION_ARCHIVE_ROLE") {
        let mut archived =
            simulate_one(e, to, name, args, external, enforce_empty_auth, wrong_args, Some(&role))?;
        archived["archive"]["baseline_return_value_xdr"] = baseline["return_value_xdr"].clone();
        archived["archive"]["return_matches_baseline"] =
            json!(archived["return_value_xdr"] == baseline["return_value_xdr"]);
        archived["archive"]["baseline_snapshot_sha256"] = baseline["snapshot_sha256"].clone();
        archived["archive"]["baseline_fees"] = baseline["fees"].clone();
        archived["archive"]["baseline_post_state_data_sha256"] =
            baseline["post_state_data_sha256"].clone();
        archived["archive"]["contract_changes_match_baseline"] =
            json!(archived["post_state_data_sha256"] == baseline["post_state_data_sha256"]);
        return Ok(archived);
    }
    Ok(baseline)
}
fn simulate_one(
    e: &Env,
    to: &Address,
    name: &str,
    args: &SVec<Val>,
    external: Option<&Address>,
    enforce_empty_auth: bool,
    wrong_args: Option<&SVec<Val>>,
    archive_role: Option<&str>,
) -> Result<Value, String> {
    let p = crate::cl_resource_limits::profile();
    if p["mode"] != "acceptance" {
        return Err("profiling-only is not transaction acceptance".into());
    }
    let raw = std::fs::read(p["network_config_path"].as_str().unwrap()).unwrap();
    assert_eq!(hash(&raw), p["network_sha256"]);
    let c: Value = serde_json::from_slice(&raw).unwrap();
    let mut snapshot = e.to_ledger_snapshot();
    let info = snapshot.ledger_info();
    let mut archive_keys = vec![];
    if let Some(role) = archive_role {
        let key = crate::resource_profile::contract_address(role)
            .ok_or("unknown archive fixture role")?;
        let selected = addr(e, &Address::from_str(e, &key));
        for (key, (_, ttl)) in &mut snapshot.ledger_entries {
            if matches!(key.as_ref(),LedgerKey::ContractData(k) if k.contract==selected && k.durability==ContractDataDurability::Persistent && k.key!=ScVal::LedgerKeyContractInstance)
            {
                archive_keys.push(json!({"key_xdr":b64(key.as_ref()),"original_live_until_ledger":ttl,"archived_live_until_ledger":info.sequence_number-1}));
                *ttl = Some(info.sequence_number - 1);
            }
        }
    }
    let snapshot = Rc::new(snapshot);
    assert_eq!(info.protocol_version, 29);
    let snapshot_hash = hash(&serde_json::to_vec(snapshot.as_ref()).unwrap());
    let budget = Budget::try_from_configs_with_shadow_limits(
        p["limits"]["instructions"].as_u64().unwrap(),
        p["limits"]["mem_bytes"].as_u64().unwrap(),
        20_000_000_000,
        4_294_967_296,
        params(&c, "configSettingContractCostParamsCpuInstructions"),
        params(&c, "configSettingContractCostParamsMemoryBytes"),
    )
    .unwrap();
    let hf = HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: addr(e, to),
        function_name: ScSymbol(name.as_bytes().to_vec().try_into().unwrap()),
        args: args
            .iter()
            .map(|v| ScVal::try_from_val(e, &v).unwrap())
            .collect::<std::vec::Vec<_>>()
            .try_into()
            .unwrap(),
    });
    let source = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0; 32])));
    let mut diag = vec![];
    let result = invoke_host_function_in_recording_mode(
        &budget,
        true,
        &hf,
        &source,
        if let Some(wrong) = wrong_args {
            let wrong_root = InvokeContractArgs {
                contract_address: addr(e,to),
                function_name: ScSymbol(name.as_bytes().to_vec().try_into().unwrap()),
                args: wrong.iter().map(|v|ScVal::try_from_val(e,&v).unwrap()).collect::<std::vec::Vec<_>>().try_into().unwrap(),
            };
            RecordingInvocationAuthMode::Enforcing(vec![SorobanAuthorizationEntry {
                credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                    address: addr(e,external.expect("external authorizer required")),
                    nonce: 0,
                    signature_expiration_ledger: info.sequence_number + 100,
                    signature: ScVal::Void,
                }),
                root_invocation: SorobanAuthorizedInvocation {
                    function: SorobanAuthorizedFunction::ContractFn(wrong_root),
                    sub_invocations: VecM::default(),
                },
            }])
        } else if enforce_empty_auth {
            RecordingInvocationAuthMode::Enforcing(vec![])
        } else {
            RecordingInvocationAuthMode::recording(false, false)
        },
        info.clone(),
        snapshot.clone(),
        [0; 32],
        &mut diag,
    )
    .map_err(|x| format!("host29 simulation failed: {x:?}"))?;
    let allowed = external.map(|a| addr(e, a));
    for a in &result.auth {
        let address = match &a.credentials {
            SorobanCredentials::Address(v) => Some(&v.address),
            SorobanCredentials::AddressV2(v) => Some(&v.address),
            SorobanCredentials::SourceAccount => None,
            _ => return Err("delegated authorization unsupported by controlled fixture".into()),
        };
        if address != allowed.as_ref() {
            return Err(format!("unexpected recorded authorizer {address:?}; controller/facade/vault authorization cannot be recorded as external"));
        }
    }
    let (return_xdr, return_bytes, error_xdr, error_text) = match &result.invoke_result {
        Ok(rv) => (Some(b64(rv)), encoded_len(rv), None, None),
        Err(err) => {
            assert!(result.ledger_changes.is_empty());
            assert!(result.contract_events.is_empty());
            assert_eq!(result.contract_events_and_return_value_size, 0);
            let error =
                ScError::try_from(err.error).map_err(|_| "invalid host error conversion")?;
            (None, 0, Some(b64(&error)), Some(format!("{error:?}")))
        }
    };
    let succeeded = result.invoke_result.is_ok();
    let cpu = budget.get_cpu_insns_consumed().unwrap();
    let mem = budget.get_mem_bytes_consumed().unwrap();
    let fc = setting(&c, "configSettingContractComputeV0");
    let lc = setting(&c, "configSettingContractLedgerCostV0");
    let ext = setting(&c, "configSettingContractLedgerCostExtV0");
    let ec = setting(&c, "configSettingContractEventsV0");
    let bw = setting(&c, "configSettingContractBandwidthV0");
    let hist = setting(&c, "configSettingContractHistoricalDataV0");
    let ar = setting(&c, "configSettingStateArchival");
    let fee = FeeConfiguration {
        fee_per_instruction_increment: num(fc, "feeRatePerInstructionsIncrement"),
        fee_per_disk_read_entry: num(lc, "feeDiskReadLedgerEntry"),
        fee_per_write_entry: num(lc, "feeWriteLedgerEntry"),
        fee_per_disk_read_1kb: num(lc, "feeDiskRead1Kb"),
        fee_per_write_1kb: num(ext, "feeWrite1Kb"),
        fee_per_historical_1kb: num(hist, "feeHistorical1Kb"),
        fee_per_contract_event_1kb: num(ec, "feeContractEvents1Kb"),
        fee_per_transaction_size_1kb: num(bw, "feeTxSize1Kb"),
    };
    let window = setting(&c, "configSettingLiveSorobanStateSizeWindow")
        .as_array()
        .unwrap();
    let state_size = window.iter().map(|v| v.as_i64().unwrap()).sum::<i64>() / window.len() as i64;
    let rent_rate = compute_rent_write_fee_per_1kb(
        state_size,
        &RentWriteFeeConfiguration {
            state_target_size_bytes: num(lc, "sorobanStateTargetSizeBytes"),
            rent_fee_1kb_state_size_low: num(lc, "rentFee1KbSorobanStateSizeLow"),
            rent_fee_1kb_state_size_high: num(lc, "rentFee1KbSorobanStateSizeHigh"),
            state_size_rent_fee_growth_factor: num(lc, "sorobanStateRentFeeGrowthFactor")
                .try_into()
                .unwrap(),
        },
    );
    let rent_config = RentFeeConfiguration {
        fee_per_write_1kb: fee.fee_per_write_1kb,
        fee_per_rent_1kb: rent_rate,
        fee_per_write_entry: fee.fee_per_write_entry,
        persistent_rent_rate_denominator: num(ar, "persistentRentRateDenominator"),
        temporary_rent_rate_denominator: num(ar, "tempRentRateDenominator"),
    };
    let rent_changes = extract_rent_changes(&result.ledger_changes);
    let rent = compute_rent_fee(&rent_changes, &rent_config, info.sequence_number);
    let diskreads = result
        .resources
        .footprint
        .read_only
        .iter()
        .chain(result.resources.footprint.read_write.iter())
        .filter(|k| !matches!(k, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_)))
        .count()
        + result.restored_rw_entry_indices.len();
    let provisional = envelope(
        &source,
        &hf,
        &result.auth,
        &result.resources,
        &result.restored_rw_entry_indices,
        0,
        0,
    );
    let txsize = encoded_len(&provisional);
    let tr = TransactionResources {
        instructions: result.resources.instructions,
        disk_read_entries: diskreads.try_into().unwrap(),
        write_entries: result
            .resources
            .footprint
            .read_write
            .len()
            .try_into()
            .unwrap(),
        disk_read_bytes: result.resources.disk_read_bytes,
        write_bytes: result.resources.write_bytes,
        contract_events_size_bytes: result.contract_events_and_return_value_size,
        transaction_size_bytes: txsize,
    };
    let (non_refundable, refundable) = compute_transaction_resource_fee(&tr, &fee);
    let inclusion = 100i64;
    let resource_fee = non_refundable + refundable + rent;
    let total = resource_fee + inclusion;
    let env = envelope(
        &source,
        &hf,
        &result.auth,
        &result.resources,
        &result.restored_rw_entry_indices,
        resource_fee,
        total.try_into().map_err(|_| "fee exceeds envelope u32")?,
    );
    assert_eq!(encoded_len(&env), txsize);
    let footprint =
        result.resources.footprint.read_only.len() + result.resources.footprint.read_write.len();
    // Preserve the SDK's per-entry/key/code checks in the clean embedder host.
    // These use serialized LedgerKey/LedgerEntry XDR, not just WASM byte length.
    let mut max_key = 0u32;
    let mut max_data = 0u32;
    let mut max_code = 0u32;
    let mut entry_sizes = vec![];
    for key in result
        .resources
        .footprint
        .read_only
        .iter()
        .chain(result.resources.footprint.read_write.iter())
    {
        let key_bytes = key.to_xdr(Limits::none()).unwrap();
        let change = result
            .ledger_changes
            .iter()
            .find(|c| c.encoded_key == key_bytes);
        if matches!(key, LedgerKey::ContractData(_)) {
            max_key = max_key.max(key_bytes.len() as u32);
        }
        let old = snapshot
            .ledger_entries
            .iter()
            .find(|(k, _)| k.as_ref() == key)
            .map(|(_, v)| v.0.to_xdr(Limits::none()).unwrap());
        for (phase, encoded) in [
            ("before", old),
            ("after", change.and_then(|c| c.encoded_new_value.clone())),
        ] {
            if let Some(bytes) = encoded {
                let entry = LedgerEntry::from_xdr(&bytes, Limits::none()).unwrap();
                match entry.data {
                    LedgerEntryData::ContractData(_) => max_data = max_data.max(bytes.len() as u32),
                    LedgerEntryData::ContractCode(_) => max_code = max_code.max(bytes.len() as u32),
                    _ => {}
                }
                entry_sizes.push(json!({"phase":phase,"key_xdr":b64(key),"entry_size_bytes":bytes.len(),"entry_sha256":hash(&bytes),"entry_xdr":b64(&entry)}));
            }
        }
    }
    // Compare effective post-invocation contract state, ignoring TTL and ledger
    // headers; restoration must not alter historical balances or entitlements.
    let mut post_state = std::collections::BTreeMap::<String, String>::new();
    for change in &result.ledger_changes {
        let key = LedgerKey::from_xdr(&change.encoded_key, Limits::none()).unwrap();
        let bytes = if let Some(v) = &change.encoded_new_value {
            Some(v.clone())
        } else if change.read_only {
            snapshot
                .ledger_entries
                .iter()
                .find(|(k, _)| k.as_ref() == &key)
                .map(|(_, v)| v.0.to_xdr(Limits::none()).unwrap())
        } else {
            None
        };
        if let Some(bytes) = bytes {
            let entry = LedgerEntry::from_xdr(&bytes, Limits::none()).unwrap();
            if matches!(
                entry.data,
                LedgerEntryData::ContractData(_) | LedgerEntryData::ContractCode(_)
            ) {
                post_state.insert(b64(&key), b64(&entry.data));
            }
        }
    }
    let post_state_data_sha256 = hash(&serde_json::to_vec(&post_state).unwrap());
    // A failed embedder invocation exports no ledger changes. Hash the complete
    // initial contract data/code and the effective state after applying changes,
    // so rollback evidence is not a vacuous hash of an empty changes list.
    let mut full_before = std::collections::BTreeMap::<String, String>::new();
    for (key, (entry, _)) in &snapshot.ledger_entries {
        if matches!(
            entry.data,
            LedgerEntryData::ContractData(_) | LedgerEntryData::ContractCode(_)
        ) {
            full_before.insert(b64(key.as_ref()), b64(&entry.data));
        }
    }
    let mut full_after = full_before.clone();
    for change in &result.ledger_changes {
        let key = LedgerKey::from_xdr(&change.encoded_key, Limits::none()).unwrap();
        if !matches!(key, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_))
            || change.read_only
        {
            continue;
        }
        if let Some(bytes) = &change.encoded_new_value {
            let entry = LedgerEntry::from_xdr(bytes, Limits::none()).unwrap();
            full_after.insert(b64(&key), b64(&entry.data));
        } else {
            full_after.remove(&b64(&key));
        }
    }
    let full_before_hash = hash(&serde_json::to_vec(&full_before).unwrap());
    let full_after_hash = hash(&serde_json::to_vec(&full_after).unwrap());
    let shadow_ok = !budget.shadow_cpu_limit_exceeded().unwrap()
        && !budget.shadow_mem_limit_exceeded().unwrap();
    let l = &p["limits"];
    let fits = cpu <= l["instructions"].as_u64().unwrap()
        && mem <= l["mem_bytes"].as_u64().unwrap()
        && footprint as u64 <= l["ledger_entries"].as_u64().unwrap()
        && tr.write_entries as u64 <= l["write_entries"].as_u64().unwrap()
        && tr.disk_read_entries as u64 <= l["disk_read_entries"].as_u64().unwrap()
        && tr.disk_read_bytes as u64 <= l["disk_read_bytes"].as_u64().unwrap()
        && tr.write_bytes as u64 <= l["write_bytes"].as_u64().unwrap()
        && tr.contract_events_size_bytes as u64
            <= l["contract_events_size_bytes"].as_u64().unwrap()
        && txsize as u64 <= l["transaction_size_bytes"].as_u64().unwrap()
        && max_key as u64 <= l["max_contract_data_key_size_bytes"].as_u64().unwrap()
        && max_data as u64 <= l["max_contract_data_entry_size_bytes"].as_u64().unwrap()
        && max_code as u64 <= l["max_contract_code_entry_size_bytes"].as_u64().unwrap()
        && shadow_ok;
    let mut row = json!({"success":succeeded,"post_state_data_sha256":post_state_data_sha256,"archive":{"enabled":archive_role.is_some(),"role":archive_role,"modified_keys":archive_keys,"restoration_observed":!result.restored_rw_entry_indices.is_empty(),"scope":"cloned controlled state only; original harness state untouched"},"contract_role":crate::resource_profile::contract_role(to),"contract_address":strkey(to),"limits_pass":fits,"diagnostic_shadow_ok":shadow_ok,"diagnostic_shadow_cpu_used":budget.get_shadow_cpu_insns_consumed().unwrap(),"diagnostic_shadow_memory_used":budget.get_shadow_mem_bytes_consumed().unwrap(),"exact_captured_cost_model":true,"cost_model_source":"hash-pinned captured coefficients supplied directly to official host Budget","signature_size_scope":"unsigned recording templates; no signed-envelope or signature-verification proof","max_contract_data_key_size_bytes":max_key,"max_contract_data_entry_size_bytes":max_data,"max_contract_code_entry_size_bytes":max_code,"entry_size_checks":entry_sizes,"contract_authorization_enforced":true,"current_ledger":info.sequence_number,"snapshot_sha256":snapshot_hash,"host_function_sha256":hash(&hf.to_xdr(Limits::none()).unwrap()),"envelope_sha256":hash(&env.to_xdr(Limits::none()).unwrap()),"unsigned_envelope_xdr":b64(&env),"expected_external_authorizers":external.map(|a|vec![strkey(a)]).unwrap_or_default(),"authorization_size_bytes":4+result.auth.iter().map(encoded_len).sum::<u32>(),"return_value_xdr":return_xdr,"return_value_size_bytes":return_bytes,"contract_events_xdr":result.contract_events.iter().map(b64).collect::<std::vec::Vec<_>>(),"contract_events_size_bytes":result.contract_events.iter().map(encoded_len).sum::<u32>(),"footprint":{"read_only_xdr":result.resources.footprint.read_only.iter().map(b64).collect::<std::vec::Vec<_>>(),"read_write_xdr":result.resources.footprint.read_write.iter().map(b64).collect::<std::vec::Vec<_>>()},"resources":{"instructions":tr.instructions,"memory_bytes":mem,"disk_read_entries":tr.disk_read_entries,"write_entries":tr.write_entries,"disk_read_bytes":tr.disk_read_bytes,"write_bytes":tr.write_bytes,"contract_events_size_bytes":tr.contract_events_size_bytes,"transaction_size_bytes":tr.transaction_size_bytes},"restored_rw_entry_indices":result.restored_rw_entry_indices,"fee_inputs":{"fee_per_instruction_increment":fee.fee_per_instruction_increment,"fee_per_disk_read_entry":fee.fee_per_disk_read_entry,"fee_per_write_entry":fee.fee_per_write_entry,"fee_per_disk_read_1kb":fee.fee_per_disk_read_1kb,"fee_per_write_1kb":fee.fee_per_write_1kb,"fee_per_historical_1kb":fee.fee_per_historical_1kb,"fee_per_contract_event_1kb":fee.fee_per_contract_event_1kb,"fee_per_transaction_size_1kb":fee.fee_per_transaction_size_1kb},"rent_fee_inputs":{"fee_per_write_1kb":rent_config.fee_per_write_1kb,"fee_per_rent_1kb":rent_config.fee_per_rent_1kb,"fee_per_write_entry":rent_config.fee_per_write_entry,"persistent_rent_rate_denominator":rent_config.persistent_rent_rate_denominator,"temporary_rent_rate_denominator":rent_config.temporary_rent_rate_denominator},"rent_changes":rent_changes.iter().map(|r|json!({"is_persistent":r.is_persistent,"is_code_entry":r.is_code_entry,"old_size_bytes":r.old_size_bytes,"new_size_bytes":r.new_size_bytes,"old_live_until_ledger":r.old_live_until_ledger,"new_live_until_ledger":r.new_live_until_ledger})).collect::<std::vec::Vec<_>>(),"base_fee_source":"controlled-state-ledger-setting","fees":{"non_refundable":non_refundable,"refundable":refundable,"rent":rent,"inclusion":inclusion,"total":total},"signature_verification_measured":false,"live_state_validation":false});
    row["outcome"] = json!(if succeeded { "succeeded" } else { "rejected" });
    row["error_xdr"] = json!(error_xdr);
    row["error"] = json!(error_text);
    row["ledger_changes_empty"] = json!(result.ledger_changes.is_empty());
    row["pre_state_full_data_sha256"] = json!(full_before_hash);
    row["post_state_full_data_sha256"] = json!(full_after_hash);
    row["post_state_unchanged"] = json!(full_before_hash == full_after_hash);
    Ok(row)
}
