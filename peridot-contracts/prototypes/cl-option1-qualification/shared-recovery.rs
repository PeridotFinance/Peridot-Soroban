//! Eight distinct borrowers recovered sequentially from ONE joint ledger state.
//! Native RestoreFootprint estimates and projected TTLs, not Core application.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub struct PlannedRoot {
    pub payer: Address,
    pub owner: Address,
    pub id: u64,
    pub maximum: u128,
}
const FLOW: &str = "recover_expired_pending_funded";

// Preserve explicit deletions, TTL-only changes and handled-failure placeholders.
// Never infer deletion from absence in the old entry-size diagnostic list.
fn apply_exact(s: &mut LedgerSnapshot, row: &Value) {
    assert_eq!(row["success"], true);
    assert_eq!(row["pre_state_full_data_sha256"], full_data_hash(s));
    for change in row["shared_exact_ledger_changes"]
        .as_array()
        .expect("exact changes required")
    {
        let key = LedgerKey::from_xdr_base64(change["key_xdr"].as_str().unwrap(), Limits::none())
            .unwrap();
        let found = s
            .ledger_entries
            .iter()
            .position(|(k, _)| k.as_ref() == &key);
        if change["footprint_only_placeholder"] == true {
            continue;
        }
        let ttl = change["new_live_until"]
            .as_u64()
            .map(|v| u32::try_from(v).unwrap());
        if change["read_only"] != true {
            if let Some(encoded) = change["new_entry_xdr"].as_str() {
                let value = LedgerEntry::from_xdr_base64(encoded, Limits::none()).unwrap();
                if let Some(i) = found {
                    s.ledger_entries[i].1 .0 = Box::new(value);
                } else {
                    s.ledger_entries
                        .push((Box::new(key.clone()), (Box::new(value), ttl)));
                }
            } else if let Some(i) = found {
                s.ledger_entries.remove(i);
            }
        }
        if let Some(until) = ttl {
            if let Some((_, (_, old))) = s
                .ledger_entries
                .iter_mut()
                .find(|(k, _)| k.as_ref() == &key)
            {
                *old = Some(until);
            }
        }
    }
    assert_eq!(
        row["post_state_full_data_sha256"],
        full_data_hash(s),
        "complete projected poststate including explicit deletions"
    );
}
fn invoke(e: &Env, s: &LedgerSnapshot, to: &Address, root: &PlannedRoot) -> Value {
    let args = (root.payer.clone(), root.id, root.maximum, 1u128).into_val(e);
    on_snapshot(e, s, to, FLOW, &args, Some(&root.payer))
}
fn required_keys(row: &Value, initial: &LedgerSnapshot) -> BTreeSet<LedgerKey> {
    row["footprint"]["read_only_xdr"]
        .as_array()
        .unwrap()
        .iter()
        .chain(row["footprint"]["read_write_xdr"].as_array().unwrap())
        .filter_map(|encoded| {
            let key =
                LedgerKey::from_xdr_base64(encoded.as_str().unwrap(), Limits::none()).unwrap();
            let persistent = match &key {
                LedgerKey::ContractCode(_) => true,
                LedgerKey::ContractData(d) => d.durability == ContractDataDurability::Persistent,
                _ => false,
            };
            if persistent
                && initial
                    .ledger_entries
                    .iter()
                    .any(|(k, _)| k.as_ref() == &key)
            {
                Some(key)
            } else {
                None
            }
        })
        .collect()
}
fn sdk_existing_keys(row: &Value, initial: &LedgerSnapshot) -> BTreeSet<LedgerKey> {
    row["sdk_observed_storage_keys_xdr"]
        .as_array()
        .expect("SDK stored-key evidence required")
        .iter()
        .filter_map(|v| {
            let key = LedgerKey::from_xdr_base64(v.as_str().unwrap(), Limits::none()).unwrap();
            let persistent = match &key {
                LedgerKey::ContractCode(_) => true,
                LedgerKey::ContractData(d) => d.durability == ContractDataDurability::Persistent,
                _ => false,
            };
            if persistent
                && initial
                    .ledger_entries
                    .iter()
                    .any(|(k, _)| k.as_ref() == &key)
            {
                Some(key)
            } else {
                None
            }
        })
        .collect()
}
fn assertions(e: &Env, s: &LedgerSnapshot, roots: &[PlannedRoot], completed: usize) -> Value {
    let mut sdk = e.to_snapshot();
    sdk.ledger = s.clone();
    let fresh = Env::from_snapshot(sdk);
    crate::cl_resource_limits::configure(&fresh);
    let cash = Address::from_str(
        &fresh,
        &crate::resource_profile::contract_address("cash_receipt").unwrap(),
    );
    let margin = Address::from_str(
        &fresh,
        &crate::resource_profile::contract_address("v3").unwrap(),
    );
    let mut debts = vec![];
    for (i, r) in roots.iter().enumerate() {
        crate::cl_resource_limits::reset(&fresh);
        let debt: u128 = fresh.invoke_contract(
            &cash,
            &Symbol::new(&fresh, "get_margin_borrow_balance"),
            (r.id,).into_val(&fresh),
        );
        crate::cl_resource_limits::reset(&fresh);
        let pending: Option<Map<Symbol, Val>> = fresh.invoke_contract(
            &margin,
            &Symbol::new(&fresh, "get_pending_perps_open"),
            (r.id,).into_val(&fresh),
        );
        if i < completed {
            assert_eq!(debt, 0);
            assert!(pending.is_none());
        } else {
            assert!(debt > 0);
            assert!(pending.is_some());
        }
        debts.push(json!({"id":r.id,"borrower":strkey(&r.owner),"debt":debt.to_string(),"pending":pending.is_some()}));
    }
    json!({"completed_borrowers_zero_debt":true,"unexecuted_borrowers_remain_pending_and_indebted":true,"positions":debts,"scope":"post-root assertions on separate clone"})
}

pub fn run(e: &Env, to: &Address, roots: &[PlannedRoot]) -> Value {
    assert_eq!(roots.len(), 8);
    assert_eq!(
        roots
            .iter()
            .map(|r| strkey(&r.payer))
            .collect::<BTreeSet<_>>()
            .len(),
        8
    );
    assert_eq!(
        roots
            .iter()
            .map(|r| strkey(&r.owner))
            .collect::<BTreeSet<_>>()
            .len(),
        8
    );
    let p = crate::cl_resource_limits::profile();
    assert_eq!(p["name"], "captured-network-diagnostic");
    let c: Value =
        serde_json::from_slice(&std::fs::read(p["network_config_path"].as_str().unwrap()).unwrap())
            .unwrap();
    let evidence = std::path::PathBuf::from(std::env::var("CL_SHARED_EVIDENCE_DIR").unwrap());
    std::fs::create_dir_all(&evidence).unwrap();
    let original = e.to_ledger_snapshot();
    std::fs::write(
        evidence.join("joint-original.json"),
        serde_json::to_vec(&original).unwrap(),
    )
    .unwrap();
    let mut host_union = BTreeSet::new();

    let mut discovery = original.clone();
    let mut union = BTreeSet::new();
    let mut multiplicity = BTreeMap::<String, usize>::new();
    let mut discovery_rows = vec![];
    for root in roots {
        let mut row = invoke(e, &discovery, to, root);
        assert_eq!(row["success"], true, "discovery financial root");
        assert_eq!(row["limits_pass"], true);
        let mut keys = required_keys(&row, &original);
        host_union.extend(keys.clone());
        let sdk = shared_sdk_discovery(e, &discovery, &row, FLOW);
        assert_eq!(sdk["ok"], true, "captured SDK warm discovery must execute");
        assert_eq!(sdk["return_matches_final"], true);
        keys.extend(sdk_existing_keys(&sdk, &original));
        row["shared_sdk_discovery"] = sdk;
        let mut stream = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(evidence.join("discovery.jsonl"))
            .unwrap();
        writeln!(stream, "{row}").unwrap();

        for key in &keys {
            *multiplicity.entry(b64(key)).or_default() += 1;
        }
        union.extend(keys);
        apply_exact(&mut discovery, &row);
        discovery_rows.push(row);
    }
    let payer_addresses: BTreeSet<_> = roots.iter().map(|r| addr(e, &r.payer)).collect();
    let marker_keys: BTreeSet<_> = union.iter().filter(|k| matches!(k,LedgerKey::ContractData(d) if payer_addresses.contains(&d.contract) && d.key==ScVal::LedgerKeyContractInstance)).map(b64).collect();
    assert_eq!(
        marker_keys.len(),
        8,
        "each synthetic external authorization instance explicitly identified"
    );
    let mut marker_executables = vec![];
    for encoded in &marker_keys {
        let key = LedgerKey::from_xdr_base64(encoded, Limits::none()).unwrap();
        let (_, (entry, _)) = original
            .ledger_entries
            .iter()
            .find(|(k, _)| k.as_ref() == &key)
            .unwrap();
        let LedgerEntryData::ContractData(d) = &entry.data else {
            unreachable!()
        };
        let ScVal::ContractInstance(instance) = &d.val else {
            panic!("external marker is not an instance")
        };
        let code = match &instance.executable {
            ContractExecutable::Wasm(h) => Some(LedgerKey::ContractCode(LedgerKeyContractCode {
                hash: h.clone(),
            })),
            ContractExecutable::StellarAsset | ContractExecutable::ExternalRef(_) => None,
        };
        marker_executables.push(json!({"marker_key_xdr":encoded,"executable_xdr":b64(&instance.executable),"code_key_xdr":code.as_ref().map(b64),"code_present_in_joint_prestate":code.as_ref().map(|key|original.ledger_entries.iter().any(|(k,_)|k.as_ref()==key)),"code_in_restoration_union":code.as_ref().map(|key|union.contains(key)),"native_handler_is_test_only":true}));
    }
    let warm = &discovery_rows[0];
    let mut cold = original.clone();
    for key in &union {
        let (_, (_, ttl)) = cold
            .ledger_entries
            .iter_mut()
            .find(|(k, _)| k.as_ref() == key)
            .unwrap();
        *ttl = Some(original.sequence_number - 1);
    }
    assert_eq!(full_data_hash(&cold), full_data_hash(&original));
    let mut batches = vec![];
    let mut part = vec![];
    let mut bytes = 0;
    for key in &union {
        let (_, (entry, _)) = cold
            .ledger_entries
            .iter()
            .find(|(k, _)| k.as_ref() == key)
            .unwrap();
        let n = encoded_len(entry.as_ref());
        assert!(n <= 132096);
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
    let evidence = std::path::PathBuf::from(std::env::var("CL_SHARED_EVIDENCE_DIR").unwrap());
    std::fs::create_dir_all(&evidence).unwrap();
    std::fs::write(
        evidence.join("joint-original.json"),
        serde_json::to_vec(&original).unwrap(),
    )
    .unwrap();
    std::fs::write(
        evidence.join("joint-cold.json"),
        serde_json::to_vec(&cold).unwrap(),
    )
    .unwrap();
    std::fs::write(
        evidence.join("discovery.json"),
        serde_json::to_vec(&discovery_rows).unwrap(),
    )
    .unwrap();
    let mut profiles = vec![];
    for profile in ["internal-policy", "captured-network-diagnostic"] {
        let _guard = ProfileGuard::set(
            &std::env::var(if profile == "internal-policy" {
                "CL_RESTORE_INTERNAL_PROFILE"
            } else {
                "CL_RESTORE_CAPTURED_PROFILE"
            })
            .unwrap(),
        );
        let selected = crate::cl_resource_limits::profile();
        let network = config(&selected, &c, warm);
        let mut state = cold.clone();
        let mut preparation = vec![];
        for (i, keys) in batches.iter().enumerate() {
            preparation.push(
                batch(
                    &mut state,
                    keys,
                    &network,
                    &selected,
                    warm,
                    "shared_eight_pending_recovery",
                    i,
                )
                .unwrap(),
            );
        }
        assert_eq!(full_data_hash(&state), full_data_hash(&original));
        let mut marker_rent_marginal = 0i64;
        for row in &preparation {
            let keys = row["keys"].as_array().unwrap();
            let changes: std::vec::Vec<_> = row["rent_changes"]
                .as_array()
                .unwrap()
                .iter()
                .zip(keys)
                .filter(|(_, k)| !marker_keys.contains(k["key_xdr"].as_str().unwrap()))
                .map(|(r, _)| LedgerEntryRentChange {
                    is_persistent: true,
                    is_code_entry: r["is_code_entry"].as_bool().unwrap(),
                    old_size_bytes: r["old_size_bytes"].as_u64().unwrap() as u32,
                    new_size_bytes: r["new_size_bytes"].as_u64().unwrap() as u32,
                    old_live_until_ledger: r["old_live_until_ledger"].as_u64().unwrap() as u32,
                    new_live_until_ledger: r["new_live_until_ledger"].as_u64().unwrap() as u32,
                })
                .collect();
            marker_rent_marginal += row["fees"]["rent"].as_i64().unwrap()
                - compute_rent_fee(
                    &changes,
                    &network.rent_fee_configuration,
                    state.sequence_number,
                );
        }
        let prepared = snap_hash(&state);
        let mut chain = vec![];
        let mut completed = 0usize;
        for root in roots {
            let before = snap_hash(&state);
            let host = invoke(e, &state, to, root);
            append(
                if profile == "internal-policy" {
                    "CL_SHARED_INTERNAL_TRANSACTIONS"
                } else {
                    "CL_TRANSACTION_REPORT"
                },
                &host,
            );
            append("CL_RESTORE_FINANCIAL_REPORT", &host);
            // A finite SDK rejection is retained as failure, never applied.
            let sdk = if host["success"] == true {
                sdk_after(e, &state, &host, FLOW)
            } else {
                json!({"ok":false,"not_executed":"host invocation failed"})
            };
            let sdk_unprepared: std::vec::Vec<_> = if sdk["ok"] == true {
                sdk_existing_keys(&sdk, &original)
                    .difference(&union)
                    .map(b64)
                    .collect()
            } else {
                vec![]
            };
            let coverage =
                sdk["ok"] == true && sdk_unprepared.is_empty() && sdk["disk_read_entries"] == 0;
            let fit = host["success"] == true
                && host["limits_pass"] == true
                && sdk["ok"] == true
                && sdk["return_matches_final"] == true
                && coverage;
            let mut item = json!({"id":root.id,"payer":strkey(&root.payer),"borrower":strkey(&root.owner),"input_snapshot_sha256":before,"host":host,"sdk":sdk,"accepted":fit,"sdk_preexisting_keys_not_prepared":sdk_unprepared,"sdk_observed_dependency_union_covered":coverage});
            if fit {
                assert!(
                    host["restored_rw_entry_indices"]
                        .as_array()
                        .unwrap()
                        .is_empty(),
                    "union prep must cover every required old key"
                );
                apply_exact(&mut state, &host);
                completed += 1;
                item["output_snapshot_sha256"] = json!(snap_hash(&state));
                item["postconditions"] = assertions(e, &state, roots, completed);
                std::fs::write(
                    evidence.join(format!("{profile}-after-{completed}.json")),
                    serde_json::to_vec(&state).unwrap(),
                )
                .unwrap();
            }
            chain.push(item);
            if !fit {
                break;
            }
        }
        let prep_fee: i64 = preparation
            .iter()
            .map(|v| v["fees"]["total"].as_i64().unwrap())
            .sum();
        let accepted_fee: i64 = chain
            .iter()
            .filter(|v| v["accepted"] == true)
            .map(|v| v["host"]["fees"]["total"].as_i64().unwrap())
            .sum();
        let report = json!({"schema":"cl-shared-eight-recovery-v1","profile":profile,"source_sha256":hash(include_bytes!("shared-recovery.rs")),"original_joint_snapshot_sha256":snap_hash(&original),"cold_joint_snapshot_sha256":snap_hash(&cold),"prepared_snapshot_sha256":prepared,"final_snapshot_sha256":snap_hash(&state),"required_union_keys":union.len(),"host_recording_union_keys":host_union.len(),"sdk_observed_additional_key_xdrs":union.difference(&host_union).map(b64).collect::<std::vec::Vec<_>>(),"cold_scope":"union of official recording footprint plus SDK observed persistent/code dependencies from same sequential discovery; native auth setup keys included; real account signatures unqualified","sum_of_discovery_key_counts":multiplicity.values().sum::<usize>(),"shared_dependency_key_counts":multiplicity,"preparation":preparation,"chain":chain,"completed_borrowers":completed,"required_borrowers":8,"complete":completed==8,"preparation_fee":prep_fee,"accepted_financial_fee":accepted_fee,"complete_journey_fee":if completed==8{Some(prep_fee+accepted_fee)}else{None},"complete_journey_transaction_count":if completed==8{Some(batches.len()+8)}else{None},"discovery_is_offchain_preflight":true,"cold_probe_fees_summed":false,"exact_host_ledger_changes_and_ttls_carried":true,"financial_roots_use_eight_distinct_funders":true,"reward_outage":"one continuous actual pool User fault affects both claim and pending view throughout eight ownership boundaries","real_elapsed_time_reexpiry":false,"core_application_verified":false,"real_signatures":false,"native_mock_authorizer_markers":{"keys":marker_keys,"executables":marker_executables,"restoration_rent_marginal":marker_rent_marginal,"scope":"exact batch rent minus same batch rent excluding these eight native external marker instances; other fee components and invocation costs retained, not attributed or replaced by production signature estimates"},"release_approval":false});
        std::fs::write(
            evidence.join(format!("{profile}.json")),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        profiles.push(report);
    }
    assert_eq!(
        profiles[1]["completed_borrowers"], 8,
        "captured joint chain incomplete"
    );
    json!(profiles)
}
