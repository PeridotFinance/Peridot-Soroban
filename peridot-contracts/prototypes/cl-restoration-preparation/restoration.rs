//! Isolated protocol29 RestoreFootprintOp preparation. No Core application or
//! signature claim. Only validated, explicitly metered batches project TTLs.
use super::*;
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
use soroban_sdk::{IntoVal, Map, Symbol};
use soroban_simulation::{
    simulation::{simulate_restore_op, SimulationAdjustmentConfig},
    NetworkConfig,
};
use std::cell::RefCell;
#[path = "postconditions.rs"]
mod postconditions;
thread_local! {static TRADE_ACTOR:RefCell<Option<String>>=const{RefCell::new(None)}; pub static OVERRIDE:RefCell<Option<LedgerSnapshot>>=const{RefCell::new(None)};}

pub fn set_trade_actor(a: &Address) {
    TRADE_ACTOR.with(|v| *v.borrow_mut() = Some(strkey(a)));
}
struct ProfileGuard(String);
impl ProfileGuard {
    fn set(path: &str) -> Self {
        let old = std::env::var("CL_RESOURCE_PROFILE").unwrap();
        std::env::set_var("CL_RESOURCE_PROFILE", path);
        Self(old)
    }
}
impl Drop for ProfileGuard {
    fn drop(&mut self) {
        std::env::set_var("CL_RESOURCE_PROFILE", &self.0);
    }
}
fn snap_hash(s: &LedgerSnapshot) -> String {
    hash(&serde_json::to_vec(s).unwrap())
}
fn data_hash(s: &LedgerSnapshot) -> String {
    hash(
        &serde_json::to_vec(
            &s.ledger_entries
                .iter()
                .map(|(k, (v, _))| (b64(k.as_ref()), b64(&v.data)))
                .collect::<std::vec::Vec<_>>(),
        )
        .unwrap(),
    )
}
fn append(path: &str, v: &Value) {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::var(path).expect("explicit evidence path required"))
        .unwrap();
    writeln!(f, "{v}").unwrap();
}
fn config(p: &Value, c: &Value, warm: &Value) -> NetworkConfig {
    let f = &warm["fee_inputs"];
    let r = &warm["rent_fee_inputs"];
    let a = setting(c, "configSettingStateArchival");
    NetworkConfig {
        tx_max_instructions: p["limits"]["instructions"].as_i64().unwrap(),
        tx_memory_limit: p["limits"]["mem_bytes"].as_u64().unwrap() as u32,
        cpu_cost_params: params(c, "configSettingContractCostParamsCpuInstructions"),
        memory_cost_params: params(c, "configSettingContractCostParamsMemoryBytes"),
        min_temp_entry_ttl: num(a, "minTemporaryTtl") as u32,
        min_persistent_entry_ttl: num(a, "minPersistentTtl") as u32,
        max_entry_ttl: num(a, "maxEntryTtl") as u32,
        fee_configuration: FeeConfiguration {
            fee_per_instruction_increment: num(f, "fee_per_instruction_increment"),
            fee_per_disk_read_entry: num(f, "fee_per_disk_read_entry"),
            fee_per_write_entry: num(f, "fee_per_write_entry"),
            fee_per_disk_read_1kb: num(f, "fee_per_disk_read_1kb"),
            fee_per_write_1kb: num(f, "fee_per_write_1kb"),
            fee_per_historical_1kb: num(f, "fee_per_historical_1kb"),
            fee_per_contract_event_1kb: num(f, "fee_per_contract_event_1kb"),
            fee_per_transaction_size_1kb: num(f, "fee_per_transaction_size_1kb"),
        },
        rent_fee_configuration: RentFeeConfiguration {
            fee_per_write_1kb: num(r, "fee_per_write_1kb"),
            fee_per_rent_1kb: num(r, "fee_per_rent_1kb"),
            fee_per_write_entry: num(r, "fee_per_write_entry"),
            persistent_rent_rate_denominator: num(r, "persistent_rent_rate_denominator"),
            temporary_rent_rate_denominator: num(r, "temporary_rent_rate_denominator"),
        },
    }
}
fn restore_envelope(data: SorobanTransactionData, sequence: i64) -> TransactionEnvelope {
    TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([0; 32])),
            fee: (data.resource_fee + 100)
                .try_into()
                .expect("restore fee exceeds u32"),
            seq_num: SequenceNumber(sequence),
            cond: Preconditions::Time(TimeBounds {
                min_time: TimePoint(0),
                max_time: TimePoint(0),
            }),
            memo: Memo::None,
            operations: vec![Operation {
                source_account: None,
                body: OperationBody::RestoreFootprint(RestoreFootprintOp {
                    ext: ExtensionPoint::V0,
                }),
            }]
            .try_into()
            .unwrap(),
            ext: TransactionExt::V1(data),
        },
        signatures: VecM::default(),
    })
}
fn batch(
    s: &mut LedgerSnapshot,
    keys: &[LedgerKey],
    network: &NetworkConfig,
    p: &Value,
    warm: &Value,
    flow: &str,
    id: usize,
) -> Result<Value, String> {
    let before = snap_hash(s);
    let logical = data_hash(s);
    let info = s.ledger_info();
    let simulation = simulate_restore_op(
        s,
        network,
        &SimulationAdjustmentConfig::no_adjustments(),
        &info,
        keys,
    )
    .map_err(|e| format!("restore simulation: {e:?}"))?;
    let mut td = simulation.transaction_data.clone();
    let resources = td.resources.clone();
    assert!(resources.footprint.read_only.is_empty());
    assert_eq!(resources.instructions, 0);
    let until = info
        .sequence_number
        .checked_add(info.min_persistent_entry_ttl)
        .unwrap()
        - 1;
    let mut records = vec![];
    let mut rent_changes = vec![];
    for key in resources.footprint.read_write.iter() {
        let (_, (entry, ttl)) = s
            .ledger_entries
            .iter()
            .find(|(k, _)| k.as_ref() == key)
            .ok_or("missing restore bytes")?;
        assert!(ttl.unwrap() < info.sequence_number);
        let budget = Budget::try_from_configs(
            network.tx_max_instructions as u64,
            network.tx_memory_limit as u64,
            network.cpu_cost_params.clone(),
            network.memory_cost_params.clone(),
        )
        .unwrap();
        let size = soroban_env_host::e2e_invoke::entry_size_for_rent(
            &budget,
            entry,
            encoded_len(entry.as_ref()),
        )
        .map_err(|e| format!("rent size {e:?}"))?;
        rent_changes.push(LedgerEntryRentChange {
            is_persistent: true,
            is_code_entry: matches!(key, LedgerKey::ContractCode(_)),
            old_size_bytes: 0,
            new_size_bytes: size,
            old_live_until_ledger: 0,
            new_live_until_ledger: until,
        });
        records.push(json!({"key_xdr":b64(key),"entry_xdr":b64(entry.as_ref()),"predicted_entry_xdr":b64(entry.as_ref()),"old_ttl":ttl,"new_ttl":until,"rent_size_bytes":size}));
    }
    let rent = compute_rent_fee(
        &rent_changes,
        &network.rent_fee_configuration,
        info.sequence_number,
    );
    // Core counts every RestoreFootprint RW key as a disk read. Preserve the
    // official simulator estimate separately; do not silently trust its zero count.
    td.resource_fee = 0;
    let provisional = restore_envelope(td.clone(), id as i64 + 1);
    let bytes = encoded_len(&provisional);
    let tr = TransactionResources {
        instructions: 0,
        disk_read_entries: resources.footprint.read_write.len() as u32,
        write_entries: resources.footprint.read_write.len() as u32,
        disk_read_bytes: resources.disk_read_bytes,
        write_bytes: resources.write_bytes,
        contract_events_size_bytes: 0,
        transaction_size_bytes: bytes,
    };
    let (nr, rf) = compute_transaction_resource_fee(&tr, &network.fee_configuration);
    td.resource_fee = nr + rf + rent;
    let envelope = restore_envelope(td, id as i64 + 1);
    assert_eq!(encoded_len(&envelope), bytes);
    let l = &p["limits"];
    let fits = tr.write_entries <= 50
        && tr.write_entries as u64 <= l["write_entries"].as_u64().unwrap()
        && tr.write_entries as u64 <= l["ledger_entries"].as_u64().unwrap()
        && tr.disk_read_entries as u64 <= l["disk_read_entries"].as_u64().unwrap()
        && tr.disk_read_bytes as u64 <= l["disk_read_bytes"].as_u64().unwrap()
        && tr.write_bytes as u64 <= l["write_bytes"].as_u64().unwrap()
        && bytes as u64 <= l["transaction_size_bytes"].as_u64().unwrap();
    if !fits {
        return Err(format!("restore batch exceeds selected limits {flow}/{id}"));
    }
    for key in resources.footprint.read_write.iter() {
        let (_, (_, ttl)) = s
            .ledger_entries
            .iter_mut()
            .find(|(k, _)| k.as_ref() == key)
            .unwrap();
        *ttl = Some(until);
    }
    assert_eq!(
        data_hash(s),
        logical,
        "preparation cannot mutate financial data"
    );
    let row = json!({"schema":"cl-restore-preparation-v1","profile":p["name"],"limits":p["limits"],"config_sha256":p["network_sha256"],"current_ledger":info.sequence_number,"flow":flow,"batch":id,"source_sha256":hash(include_bytes!("restoration.rs")),"unsigned_envelope_xdr":b64(&envelope),"helper_transaction_data_xdr":b64(&simulation.transaction_data),"helper_resource_fee":simulation.transaction_data.resource_fee,"resources":{"instructions":0,"disk_read_entries":tr.disk_read_entries,"write_entries":tr.write_entries,"disk_read_bytes":tr.disk_read_bytes,"write_bytes":tr.write_bytes,"transaction_size_bytes":bytes},"memory_bytes":null,"memory_scope":"RestoreFootprint native Core memory not exposed by simulation; no WASM invocation meter","keys":records,"fees":{"non_refundable":nr,"refundable":rf,"rent":rent,"inclusion":100,"total":nr+rf+rent+100},"fee_inputs":warm["fee_inputs"],"rent_fee_inputs":warm["rent_fee_inputs"],"rent_changes":rent_changes.iter().map(|r|json!({"is_persistent":true,"is_code_entry":r.is_code_entry,"old_size_bytes":r.old_size_bytes,"new_size_bytes":r.new_size_bytes,"old_live_until_ledger":r.old_live_until_ledger,"new_live_until_ledger":r.new_live_until_ledger})).collect::<std::vec::Vec<_>>(),"snapshot_before_sha256":before,"snapshot_after_sha256":snap_hash(s),"financial_data_unchanged":true,"predicted_only":true,"sdk_invocation_applicable":false,"core_application_verified":false,"signature_verification_measured":false,"release_approval":false});
    append("CL_RESTORE_REPORT", &row);
    Ok(row)
}
fn invocation<'a>(e: &Env, i: &SorobanAuthorizedInvocation) -> MockAuthInvoke<'a> {
    let SorobanAuthorizedFunction::ContractFn(f) = &i.function else {
        panic!("unsupported external auth function")
    };
    let address = Address::try_from_val(e, &ScVal::Address(f.contract_address.clone())).unwrap();
    let name = String::from_utf8(f.function_name.0.to_vec()).unwrap();
    let args = f
        .args
        .iter()
        .map(|s| Val::try_from_val(e, s).unwrap())
        .collect::<std::vec::Vec<_>>();
    let subs = i
        .sub_invocations
        .iter()
        .map(|v| invocation(e, v))
        .collect::<std::vec::Vec<_>>();
    MockAuthInvoke {
        contract: Box::leak(Box::new(address)),
        fn_name: Box::leak(name.into_boxed_str()),
        args: SVec::from_slice(e, &args),
        sub_invokes: Box::leak(subs.into_boxed_slice()),
    }
}
fn sdk_after(e: &Env, s: &LedgerSnapshot, warm: &Value, flow: &str) -> Value {
    let mut sdk = e.to_snapshot();
    sdk.ledger = s.clone();
    let mut fresh = Env::from_snapshot(sdk);
    fresh.set_config(soroban_sdk::testutils::EnvTestConfig {
        capture_snapshot_at_drop: false,
        ..Default::default()
    });
    crate::cl_resource_limits::configure(&fresh);
    let TransactionEnvelope::Tx(env) = TransactionEnvelope::from_xdr_base64(
        warm["unsigned_envelope_xdr"].as_str().unwrap(),
        Limits::none(),
    )
    .unwrap() else {
        panic!("envelope")
    };
    let OperationBody::InvokeHostFunction(op) = &env.tx.operations[0].body else {
        panic!("invoke")
    };
    let mut auths = vec![];
    for a in op.auth.iter() {
        let address = match &a.credentials {
            SorobanCredentials::Address(c) => &c.address,
            SorobanCredentials::AddressV2(c) => &c.address,
            _ => panic!("unexpected external credential"),
        };
        let who = Box::leak(Box::new(
            Address::try_from_val(&fresh, &ScVal::Address(address.clone())).unwrap(),
        ));
        let root = Box::leak(Box::new(invocation(&fresh, &a.root_invocation)));
        auths.push(MockAuth {
            address: who,
            invoke: root,
        });
    }
    fresh.mock_auths(&auths);
    let HostFunction::InvokeContract(hf) = &op.host_function else {
        panic!("contract")
    };
    let to = Address::try_from_val(&fresh, &ScVal::Address(hf.contract_address.clone())).unwrap();
    let name = String::from_utf8(hf.function_name.0.to_vec()).unwrap();
    let args = hf
        .args
        .iter()
        .map(|s| Val::try_from_val(&fresh, s).unwrap())
        .collect::<std::vec::Vec<_>>();
    let sym = Symbol::new(&fresh, &name);
    let args = SVec::from_slice(&fresh, &args);
    crate::cl_resource_limits::reset(&fresh);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fresh.invoke_contract::<Val>(&to, &sym, args)
    }));
    let r = fresh.cost_estimate().resources();
    let mut row = json!({"flow":flow,"ok":result.is_ok(),"instructions":r.instructions,"memory_bytes":r.mem_bytes,"read_entries":r.memory_read_entries,"disk_read_entries":r.disk_read_entries,"write_entries":r.write_entries,"read_bytes":r.disk_read_bytes,"write_bytes":r.write_bytes,"contract_events_size_bytes":r.contract_events_size_bytes,"persistent_rent_ledger_bytes":r.persistent_rent_ledger_bytes,"persistent_entry_rent_bumps":r.persistent_entry_rent_bumps,"temporary_rent_ledger_bytes":r.temporary_rent_ledger_bytes,"temporary_entry_rent_bumps":r.temporary_entry_rent_bumps,"scope":"compiled-combined-post-preparation-conditional-core-TTL-projection","return_value_xdr":result.as_ref().ok().map(|v|b64(&ScVal::try_from_val(&fresh,v).unwrap())),"return_matches_final":result.as_ref().ok().map(|v|b64(&ScVal::try_from_val(&fresh,v).unwrap()))==warm["return_value_xdr"].as_str().map(String::from),"real_signature_proof":false});
    for (k, v) in crate::cl_resource_limits::metadata().as_object().unwrap() {
        row[k] = v.clone();
    }
    for (k, v) in crate::cl_resource_limits::diagnostics(&fresh)
        .as_object()
        .unwrap()
    {
        row[k] = v.clone();
    }
    if result.is_ok() {
        row["postconditions"] = postconditions::check(s, &fresh, warm, flow);
        row["reward_conservation"] = conservation(&fresh);
    }
    append("CL_RESTORE_SDK_REPORT", &row);
    row
}
fn on_snapshot(
    e: &Env,
    s: &LedgerSnapshot,
    to: &Address,
    name: &str,
    args: &SVec<Val>,
    external: Option<&Address>,
) -> Value {
    OVERRIDE.with(|x| *x.borrow_mut() = Some(s.clone()));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        super::simulate_one(e, to, name, args, external, false, None, None)
    }));
    OVERRIDE.with(|x| *x.borrow_mut() = None);
    let mut row = match out {
        Ok(Ok(v)) => v,
        Ok(Err(err)) => json!({"success":false,"error":err}),
        Err(_) => json!({"success":false,"error":"host panic; no complete measurement"}),
    };
    row["schema"] = json!("cl-transaction-evidence-v1");
    row["flow"] = json!(name);
    row["test_name"] = json!(std::thread::current().name().unwrap_or("unnamed"));
    row["scope"] = json!("controlled-state-unsigned-simulation");
    row["engine"] = json!("soroban-env-host-e2e");
    row["account_authorization_mode"] = json!("recording-only");
    row["expected_rejection"] = json!(false);
    row["release_approval"] = json!(false);
    for (k, v) in crate::cl_resource_limits::metadata().as_object().unwrap() {
        row[k] = v.clone();
    }
    row
}
pub fn journey(
    e: &Env,
    to: &Address,
    name: &str,
    args: &SVec<Val>,
    external: Option<&Address>,
    warm: &Value,
) -> Result<Value, String> {
    let p = crate::cl_resource_limits::profile();
    assert_eq!(
        p["name"], "captured-network-diagnostic",
        "discovery has explicit captured limits; each assessed execution selects its own profile"
    );
    let c: Value =
        serde_json::from_slice(&std::fs::read(p["network_config_path"].as_str().unwrap()).unwrap())
            .unwrap();
    let original = e.to_ledger_snapshot();
    let mut cold = original.clone();
    // Only necessary dependency keys from a successful fully metered preflight.
    // No all-holder scan or permanent keepalive premise inside a financial call.
    let footprint = warm["footprint"]["read_only_xdr"]
        .as_array()
        .unwrap()
        .iter()
        .chain(warm["footprint"]["read_write_xdr"].as_array().unwrap())
        .map(|s| LedgerKey::from_xdr_base64(s.as_str().unwrap(), Limits::none()).unwrap())
        .collect::<std::vec::Vec<_>>();
    let mut required = vec![];
    for key in footprint {
        if !match &key {
            LedgerKey::ContractCode(_) => true,
            LedgerKey::ContractData(k) => k.durability == ContractDataDurability::Persistent,
            _ => false,
        } {
            continue;
        }
        if let Some((_, (_, ttl))) = cold
            .ledger_entries
            .iter_mut()
            .find(|(k, _)| k.as_ref() == &key)
        {
            *ttl = Some(cold.sequence_number - 1);
            required.push(key);
        }
    }
    required.sort();
    required.dedup();
    let mut batches: std::vec::Vec<std::vec::Vec<LedgerKey>> = vec![];
    let mut part = vec![];
    let mut bytes = 0;
    for key in &required {
        let (_, (entry, _)) = cold
            .ledger_entries
            .iter()
            .find(|(k, _)| k.as_ref() == key)
            .unwrap();
        let n = encoded_len(entry.as_ref());
        assert!(
            n <= 132096,
            "one indivisible ledger entry exceeds restore write bytes"
        );
        if part.len() == 50 || bytes + n > 132096 {
            batches.push(part);
            part = vec![];
            bytes = 0;
        }
        part.push(key.clone());
        bytes += n;
    }
    if !part.is_empty() {
        batches.push(part);
    }
    assert!(!batches.is_empty());
    let mut results = vec![];
    for profile in ["internal-policy", "captured-network-diagnostic"] {
        let path = std::env::var(if profile == "internal-policy" {
            "CL_RESTORE_INTERNAL_PROFILE"
        } else {
            "CL_RESTORE_CAPTURED_PROFILE"
        })
        .unwrap();
        let _guard = ProfileGuard::set(&path);
        let selected = crate::cl_resource_limits::profile();
        let network = config(&selected, &c, warm);
        let mut state = cold.clone();
        let mut prefix = vec![];
        let mut partial = Value::Null;
        for (i, keys) in batches.iter().enumerate() {
            prefix.push(batch(&mut state, keys, &network, &selected, warm, name, i)?);
            // A crash/restart preserves exact archive bytes and successfully prepared TTLs.
            let saved = serde_json::to_vec(&state).unwrap();
            let resumed: LedgerSnapshot = serde_json::from_slice(&saved).unwrap();
            assert_eq!(state, resumed);
            state = resumed;
            let repeat = simulate_restore_op(
                &state,
                &network,
                &SimulationAdjustmentConfig::no_adjustments(),
                &state.ledger_info(),
                keys,
            )
            .map_err(|x| format!("retry {x:?}"))?;
            assert!(
                repeat
                    .transaction_data
                    .resources
                    .footprint
                    .read_write
                    .is_empty(),
                "retry must discover already-live keys"
            );
            if i == 0 && batches.len() > 1 {
                partial = on_snapshot(e, &state, to, name, args, external);
            }
        }
        assert_eq!(data_hash(&state), data_hash(&original));
        let prepared_hash = snap_hash(&state);
        let mut intervening = Value::Null;
        if name == "liquidate_funded_v3" {
            if let Some(actor) = TRADE_ACTOR.with(|v| v.borrow().clone()) {
                let pool = Address::from_str(
                    e,
                    &crate::resource_profile::contract_address("aquarius").unwrap(),
                );
                let trader = Address::from_str(e, &actor);
                let amount = 5_000_000_000u128;
                let quote = on_snapshot(
                    e,
                    &state,
                    &pool,
                    "estimate_swap",
                    &(0u32, 1u32, amount).into_val(e),
                    None,
                );
                if quote["success"] != true || quote["limits_pass"] != true {
                    return Err("intervening quote failed selected limits".into());
                }
                let value = ScVal::from_xdr_base64(
                    quote["return_value_xdr"].as_str().unwrap(),
                    Limits::none(),
                )
                .unwrap();
                let out_min =
                    u128::try_from_val(e, &Val::try_from_val(e, &value).unwrap()).unwrap();
                let trade = on_snapshot(
                    e,
                    &state,
                    &pool,
                    "swap",
                    &(trader.clone(), 0u32, 1u32, amount, out_min).into_val(e),
                    Some(&trader),
                );
                if trade["success"] != true || trade["limits_pass"] != true {
                    return Err("intervening swap failed selected limits".into());
                }
                let trade_sdk = sdk_after(e, &state, &trade, "swap");
                assert_eq!(trade_sdk["ok"], true);
                assert_eq!(trade_sdk["return_matches_final"], true);
                append("CL_RESTORE_FINANCIAL_REPORT", &trade);
                let original_hash = data_hash(&state);
                apply_host_changes(&mut state, &trade);
                assert_ne!(
                    data_hash(&state),
                    original_hash,
                    "pool trade must actually change state"
                );
                intervening = json!({"kind":"independent funded trader after preparation, before fresh financial simulation","quote":quote,"transaction":trade,"sdk_invocation":trade_sdk,"operator_fee_includes_trade":false,"third_party_fee":trade["fees"]["total"],"snapshot_after_trade_sha256":snap_hash(&state)});
            }
        }
        let final_tx = on_snapshot(e, &state, to, name, args, external);
        append("CL_RESTORE_FINANCIAL_REPORT", &final_tx);
        let sdk = sdk_after(
            e,
            &state,
            if final_tx["success"] == true {
                &final_tx
            } else {
                warm
            },
            name,
        );
        let sum: i64 = prefix
            .iter()
            .map(|r| r["fees"]["total"].as_i64().unwrap())
            .sum();
        let final_fee = final_tx["fees"]["total"].as_i64();
        let result = json!({"schema":"cl-restore-journey-v1","source_sha256":hash(include_bytes!("restoration.rs")),"config_sha256":selected["network_sha256"],"profile":profile,"operation":name,"intervening_pool_trade":intervening,"required_archived_keys":required.len(),"required_archived_key_xdrs":required.iter().map(b64).collect::<std::vec::Vec<_>>(),"preparation_transactions":prefix.len(),"preparation_fee_total":sum,"preparation_rent_total":prefix.iter().map(|r|r["fees"]["rent"].as_i64().unwrap()).sum::<i64>(),"final_fee":final_fee,"journey_fee_total":final_fee.map(|v|v+sum),"transaction_count":prefix.len()+1,"serial_confirmation_rounds_if_waiting_each":prefix.len()+1,"network_latency_measured":false,"fixture_ledger_time_advance":0,"source_account":"synthetic independent public payer; no borrower authorization for RestoreFootprint","borrower_authorization_for_financial_root":warm["expected_external_authorizers"],"cold_snapshot_sha256":snap_hash(&cold),"prepared_snapshot_sha256":prepared_hash,"final_input_snapshot_sha256":snap_hash(&state),"financial_data_unchanged_by_preparation":true,"interrupted_prefix_attempt":partial,"resume_from_serialized_snapshot":true,"resimulated_duplicate_batches_are_empty":true,"preparation":prefix,"final_transaction":final_tx,"sdk_invocation":sdk,"limits_fit":final_tx["success"]==true && final_tx["limits_pass"]==true && sdk["ok"]==true && sdk["return_matches_final"]==true && final_tx["restored_rw_entry_indices"].as_array().map(|v|v.is_empty())==Some(true),"core_restoration_application_verified":false,"live_state_validation":false,"signed_authorization_measured":false,"release_approval":false});
        append("CL_RESTORE_JOURNEY_REPORT", &result);
        results.push(result);
    }
    Ok(json!(results))
}
fn full_data_hash(s: &LedgerSnapshot) -> String {
    let mut values = std::collections::BTreeMap::new();
    for (k, (v, _)) in &s.ledger_entries {
        if matches!(
            v.data,
            LedgerEntryData::ContractData(_) | LedgerEntryData::ContractCode(_)
        ) {
            values.insert(b64(k.as_ref()), b64(&v.data));
        }
    }
    hash(&serde_json::to_vec(&values).unwrap())
}
fn apply_host_changes(s: &mut LedgerSnapshot, row: &Value) {
    assert_eq!(row["success"], true);
    assert_eq!(row["pre_state_full_data_sha256"], full_data_hash(s));
    for item in row["entry_size_checks"].as_array().unwrap() {
        if item["phase"] != "after" {
            continue;
        }
        let key =
            LedgerKey::from_xdr_base64(item["key_xdr"].as_str().unwrap(), Limits::none()).unwrap();
        let value =
            LedgerEntry::from_xdr_base64(item["entry_xdr"].as_str().unwrap(), Limits::none())
                .unwrap();
        let ttl = row["qualification_ttl_changes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["key_xdr"] == item["key_xdr"])
            .and_then(|t| t["new_live_until"].as_u64())
            .map(|v| v as u32);
        if let Some((_, (old, old_ttl))) = s
            .ledger_entries
            .iter_mut()
            .find(|(k, _)| k.as_ref() == &key)
        {
            *old = Box::new(value);
            if ttl.is_some() {
                *old_ttl = ttl;
            }
        } else {
            s.ledger_entries
                .push((Box::new(key), (Box::new(value), ttl)));
        }
    }
    // Fails instead of inventing a deletion/handled-failure projection.
    assert_eq!(row["post_state_full_data_sha256"], full_data_hash(s));
}
fn conservation(original: &Env) -> Value {
    // Assertions inspect a fresh snapshot AFTER measured execution. No holder
    // enumeration, settlement or required work is moved outside the financial root.
    let mut e = Env::from_snapshot(original.to_snapshot());
    e.set_config(soroban_sdk::testutils::EnvTestConfig {
        capture_snapshot_at_drop: false,
        ..Default::default()
    });
    let vault = Address::from_str(
        &e,
        &crate::resource_profile::contract_address("vault").unwrap(),
    );
    let vault_sc = addr(&e, &vault);
    let snapshot = e.to_ledger_snapshot();
    let mut owners = std::collections::BTreeSet::new();
    for (key, _) in &snapshot.ledger_entries {
        if let LedgerKey::ContractData(d) = key.as_ref() {
            if d.contract == vault_sc {
                if let ScVal::Vec(Some(v)) = &d.key {
                    if v.len() == 2 && matches!(&v[0],ScVal::Symbol(s) if s.0.to_vec()==b"Holder") {
                        if let ScVal::Address(a) = &v[1] {
                            owners.insert(a.clone());
                        }
                    }
                }
            }
        }
    }
    let number = |m: &Map<Symbol, Val>, key: &str| {
        u128::try_from_val(&e, &m.get(Symbol::new(&e, key)).unwrap()).unwrap()
    };
    let mut pending = 0u128;
    let mut fractions = 0u128;
    for owner in &owners {
        let who = Address::try_from_val(&e, &ScVal::Address(owner.clone())).unwrap();
        crate::cl_resource_limits::reset(&e);
        let h: Map<Symbol, Val> = e.invoke_contract(
            &vault,
            &Symbol::new(&e, "reward_account"),
            (who,).into_val(&e),
        );
        pending += number(&h, "pending");
        fractions += number(&h, "fraction");
    }
    crate::cl_resource_limits::reset(&e);
    let t: Map<Symbol, Val> =
        e.invoke_contract(&vault, &Symbol::new(&e, "reward_totals"), ().into_val(&e));
    let received = number(&t, "received");
    let paid = number(&t, "paid");
    let quarantine = number(&t, "quarantine");
    let rounding = number(&t, "rounding_scaled");
    assert_eq!(
        received * 1_000_000_000_000_000_000u128,
        (paid + pending + quarantine) * 1_000_000_000_000_000_000u128 + fractions + rounding
    );
    json!({"holders_including_exited":owners.len(),"received":received.to_string(),"paid":paid.to_string(),"pending":pending.to_string(),"quarantine":quarantine.to_string(),"fractions":fractions.to_string(),"rounding_scaled":rounding.to_string(),"exact_scaled":true,"scope":"post-execution assertion reads on separate cloned state"})
}
