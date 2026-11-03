//! Assertion-only observations after measured execution. No state is fed back
//! into either runtime, and these checks do not qualify signatures or Core apply.
use super::*;
use soroban_sdk::testutils::Events;

fn isolated(original: &Env, ledger: &LedgerSnapshot) -> Env {
    let mut snapshot = original.to_snapshot();
    snapshot.ledger = ledger.clone();
    let mut e = Env::from_snapshot(snapshot);
    e.set_config(soroban_sdk::testutils::EnvTestConfig {
        capture_snapshot_at_drop: false,
        ..Default::default()
    });
    crate::cl_resource_limits::configure(&e);
    e
}

fn role(e: &Env, name: &str) -> Address {
    Address::from_str(e, &crate::resource_profile::contract_address(name).unwrap())
}

fn number(e: &Env, m: &Map<Symbol, Val>, name: &str) -> u128 {
    u128::try_from_val(e, &m.get(Symbol::new(e, name)).unwrap()).unwrap()
}

fn totals(e: &Env) -> (u128, u128) {
    crate::cl_resource_limits::reset(e);
    let t: Map<Symbol, Val> = e.invoke_contract(
        &role(e, "vault"),
        &Symbol::new(e, "reward_totals"),
        ().into_val(e),
    );
    (number(e, &t, "received"), number(e, &t, "paid"))
}

fn holder(e: &Env, who: &ScAddress) -> (u128, u128) {
    let who = Address::try_from_val(e, &ScVal::Address(who.clone())).unwrap();
    crate::cl_resource_limits::reset(e);
    let h: Map<Symbol, Val> = e.invoke_contract(
        &role(e, "vault"),
        &Symbol::new(e, "reward_account"),
        (who,).into_val(e),
    );
    (number(e, &h, "pending"), number(e, &h, "fraction"))
}

fn holder_json(v: (u128, u128)) -> Value {
    json!({"pending":v.0.to_string(), "fraction":v.1.to_string()})
}

fn historical_records(
    snapshot: &LedgerSnapshot,
    vault: &ScAddress,
    borrower: &ScAddress,
) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for (key, (entry, _)) in &snapshot.ledger_entries {
        let LedgerKey::ContractData(k) = key.as_ref() else {
            continue;
        };
        if &k.contract != vault {
            continue;
        }
        let ScVal::Vec(Some(parts)) = &k.key else {
            continue;
        };
        if parts.len() != 3 {
            continue;
        }
        let ScVal::Symbol(tag) = &parts[0] else {
            continue;
        };
        let name = tag.0.to_vec();
        let matches = if name == b"HistoryWeight" {
            parts[1] == ScVal::Address(borrower.clone())
        } else if name == b"HistorySettled" || name == b"HistoryWaiverIndex" {
            parts[2] == ScVal::Address(borrower.clone())
        } else {
            false
        };
        if matches {
            out.insert(b64(key.as_ref()), b64(&entry.data));
        }
    }
    out
}

fn contract_map(snapshot: &LedgerSnapshot) -> std::collections::BTreeMap<String, String> {
    snapshot
        .ledger_entries
        .iter()
        .filter_map(|(k, (v, _))| {
            matches!(
                v.data,
                LedgerEntryData::ContractData(_) | LedgerEntryData::ContractCode(_)
            )
            .then(|| (b64(k.as_ref()), b64(&v.data)))
        })
        .collect()
}

/// Substitute only a fresh, concrete mock-authorization nonce for the same
/// declared external actor's recorded envelope nonce. All other bytes remain
/// exact. Neither nonce is evidence of real account-signature behavior.
fn exact_mock_nonce_comparison(
    prepared: &LedgerSnapshot,
    post: &LedgerSnapshot,
    after: &Env,
    host: &Value,
) -> (String, std::vec::Vec<Value>) {
    let before = contract_map(prepared);
    let mut compared = contract_map(post);
    let TransactionEnvelope::Tx(envelope) = TransactionEnvelope::from_xdr_base64(
        host["unsigned_envelope_xdr"].as_str().unwrap(),
        Limits::none(),
    )
    .unwrap() else {
        panic!("transaction envelope")
    };
    let OperationBody::InvokeHostFunction(op) = &envelope.tx.operations[0].body else {
        panic!("invoke operation")
    };
    let declared: std::collections::BTreeSet<String> = host["expected_external_authorizers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_owned())
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut substitutions = std::vec::Vec::new();
    for auth in op.auth.iter() {
        let (actor, nonce) = match &auth.credentials {
            SorobanCredentials::Address(c) => (&c.address, c.nonce),
            SorobanCredentials::AddressV2(c) => (&c.address, c.nonce),
            _ => panic!("unsupported authorization credential for exact nonce comparison"),
        };
        let actor_name =
            strkey(&Address::try_from_val(after, &ScVal::Address(actor.clone())).unwrap());
        assert!(
            declared.contains(&actor_name),
            "nonce actor is not a declared external authorizer"
        );
        assert!(
            seen.insert(actor_name.clone()),
            "multiple credentials for one external actor unsupported"
        );
        let host_key = LedgerKey::ContractData(LedgerKeyContractData {
            contract: actor.clone(),
            key: ScVal::LedgerKeyNonce(ScNonceKey { nonce }),
            durability: ContractDataDurability::Temporary,
        });
        let host_key_xdr = b64(&host_key);
        assert!(
            !before.contains_key(&host_key_xdr),
            "recorded host nonce was not fresh"
        );
        let rows = host["entry_size_checks"].as_array().unwrap();
        let host_entries: std::vec::Vec<_> = rows
            .iter()
            .filter(|r| r["phase"] == "after" && r["key_xdr"] == host_key_xdr)
            .collect();
        assert_eq!(
            host_entries.len(),
            1,
            "exactly one exported nonce entry required"
        );
        let host_entry = LedgerEntry::from_xdr_base64(
            host_entries[0]["entry_xdr"].as_str().unwrap(),
            Limits::none(),
        )
        .unwrap();
        let LedgerEntryData::ContractData(host_data) = &host_entry.data else {
            panic!("nonce data")
        };
        assert_eq!(&host_data.contract, actor);
        assert_eq!(host_data.durability, ContractDataDurability::Temporary);
        assert_eq!(host_data.key, ScVal::LedgerKeyNonce(ScNonceKey { nonce }));
        assert_eq!(host_data.val, ScVal::Void);
        let sdk_entries: std::vec::Vec<_> = post
            .ledger_entries
            .iter()
            .filter(|(key, _)| {
                let LedgerKey::ContractData(k) = key.as_ref() else {
                    return false;
                };
                k.contract == *actor
                    && k.durability == ContractDataDurability::Temporary
                    && matches!(k.key, ScVal::LedgerKeyNonce(_))
                    && !before.contains_key(&b64(key.as_ref()))
            })
            .collect();
        assert_eq!(
            sdk_entries.len(),
            1,
            "exactly one fresh SDK nonce for external actor required"
        );
        let (sdk_key, (sdk_entry, _)) = sdk_entries[0];
        let LedgerKey::ContractData(sdk_key_data) = sdk_key.as_ref() else {
            unreachable!()
        };
        let LedgerEntryData::ContractData(sdk_data) = &sdk_entry.data else {
            panic!("SDK nonce data")
        };
        assert_eq!(&sdk_data.contract, actor);
        assert_eq!(sdk_data.durability, ContractDataDurability::Temporary);
        assert_eq!(sdk_data.key, sdk_key_data.key);
        assert_eq!(sdk_data.val, ScVal::Void);
        let mut substituted_data = sdk_data.clone();
        substituted_data.key = host_data.key.clone();
        assert_eq!(
            &substituted_data, host_data,
            "nonce entries differ beyond their nonce number"
        );
        let sdk_key_xdr = b64(sdk_key.as_ref());
        if sdk_key_xdr != host_key_xdr {
            assert!(
                !compared.contains_key(&host_key_xdr),
                "host nonce collides with SDK state"
            );
            assert_eq!(compared.remove(&sdk_key_xdr), Some(b64(&sdk_entry.data)));
            compared.insert(host_key_xdr.clone(), b64(&host_entry.data));
            substitutions.push(json!({"actor":actor_name,"host_key_xdr":host_key_xdr,
                "sdk_key_xdr":sdk_key_xdr,"host_entry_data_xdr":b64(&host_entry.data),
                "sdk_entry_data_xdr":b64(&sdk_entry.data),"host_credential_nonce":nonce.to_string(),
                "fresh_in_both_runtimes":true,"same_declared_external_actor":true,
                "source":"exact unsigned envelope credential plus exported host after-entry and SDK mock-auth after-entry; temporary Void nonce only"}));
        }
    }
    assert_eq!(
        seen, declared,
        "declared external authorizers must match exact credential actors"
    );
    let compared_hash = hash(&serde_json::to_vec(&compared).unwrap());
    assert_eq!(
        host["post_state_full_data_sha256"], compared_hash,
        "SDK/host contract state differs beyond validated exact external mock nonce substitution"
    );
    (compared_hash, substitutions)
}

/// Call after successful SDK and official-host roots with identical input.
/// Complete data/code comparison permits only explicitly recorded fresh external
/// mock-nonce substitutions; it does not normalize identities, financial data,
/// existing nonces, other auth state, or arbitrary temporary entries.
pub(super) fn check(prepared: &LedgerSnapshot, after: &Env, host: &Value, flow: &str) -> Value {
    assert_eq!(
        host["success"], true,
        "postconditions require successful official host root"
    );
    let post = after.to_ledger_snapshot();
    // Capture events before any assertion reads; reads below use fresh clones.
    let sdk_events = after
        .events()
        .all()
        .events()
        .iter()
        .map(b64)
        .collect::<std::vec::Vec<_>>();
    let sdk_hash = full_data_hash(&post);
    assert_eq!(host["pre_state_full_data_sha256"], full_data_hash(prepared));
    let raw_state_matches = host["post_state_full_data_sha256"] == sdk_hash;
    let (compared_hash, nonce_substitutions) =
        exact_mock_nonce_comparison(prepared, &post, after, host);
    assert_eq!(
        host["contract_events_xdr"],
        json!(sdk_events),
        "SDK/official host contract events mismatch in {flow}"
    );

    let e = isolated(after, &post);
    let (received, paid) = totals(&e);
    crate::cl_resource_limits::reset(&e);
    let balance: i128 = e.invoke_contract(
        &role(&e, "reward"),
        &Symbol::new(&e, "balance"),
        (role(&e, "vault"),).into_val(&e),
    );
    assert!(balance >= 0);
    assert_eq!(
        balance as u128,
        received.checked_sub(paid).unwrap(),
        "actual reward token backing must equal received minus paid"
    );

    let mut beneficiaries =
        json!({"supported":false,"reason":"not a liquidation or debt-free release root"});
    if matches!(
        flow,
        "liquidate_funded_v3" | "release_debt_free_position_v3"
    ) {
        let TransactionEnvelope::Tx(envelope) = TransactionEnvelope::from_xdr_base64(
            host["unsigned_envelope_xdr"].as_str().unwrap(),
            Limits::none(),
        )
        .unwrap() else {
            panic!("transaction envelope")
        };
        let OperationBody::InvokeHostFunction(op) = &envelope.tx.operations[0].body else {
            panic!("invoke operation")
        };
        let HostFunction::InvokeContract(call) = &op.host_function else {
            panic!("contract invocation")
        };
        assert_eq!(call.function_name.0.to_vec(), flow.as_bytes());
        let ScVal::U64(id) = &call.args[1] else {
            panic!("position id")
        };
        let before = isolated(after, prepared);
        crate::cl_resource_limits::reset(&before);
        let position: Option<Map<Symbol, Val>> = before.invoke_contract(
            &role(&before, "v3"),
            &Symbol::new(&before, "get_position"),
            (*id,).into_val(&before),
        );
        let owner = Address::try_from_val(
            &before,
            &position
                .unwrap()
                .get(Symbol::new(&before, "owner"))
                .unwrap(),
        )
        .unwrap();
        let borrower = addr(&before, &owner);
        let old = holder(&before, &borrower);
        let new = holder(&e, &borrower);
        let before_totals = totals(&before);
        // Unchanged received/paid permits an exact booked-entitlement check;
        // it does not by itself prove an upstream outage. The focused compiled
        // fixture independently induces and checks both upstream failures.
        // Successful new accrual is allowed; no exact preservation claim then.
        let no_new_reward_or_payout = before_totals == (received, paid);
        let scale = 1_000_000_000_000_000_000u128;
        let old_scaled = old
            .0
            .checked_mul(scale)
            .unwrap()
            .checked_add(old.1)
            .unwrap();
        let new_scaled = new
            .0
            .checked_mul(scale)
            .unwrap()
            .checked_add(new.1)
            .unwrap();
        assert!(
            new_scaled >= old_scaled,
            "forced recovery reduced borrower entitlement"
        );
        if no_new_reward_or_payout {
            assert_eq!(new, old, "outage recovery altered booked entitlement");
        }
        let vault = addr(&before, &role(&before, "vault"));
        let prior_history = historical_records(prepared, &vault, &borrower);
        let after_history = historical_records(&post, &vault, &borrower);
        for (key, value) in &prior_history {
            assert_eq!(
                after_history.get(key),
                Some(value),
                "pre-existing borrower historical record changed"
            );
        }
        // Waiver-index additions must not be hidden by checking only old keys.
        let waiver_keys = |m: &std::collections::BTreeMap<String, String>| {
            m.iter().filter(|(k, _)| {
                let LedgerKey::ContractData(d) = LedgerKey::from_xdr_base64(k, Limits::none()).unwrap() else { unreachable!() };
                matches!(d.key, ScVal::Vec(Some(v)) if matches!(&v[0],ScVal::Symbol(s) if s.0.to_vec()==b"HistoryWaiverIndex"))
            }).map(|(k,v)|(k.clone(),v.clone())).collect::<std::collections::BTreeMap<_,_>>()
        };
        assert_eq!(
            waiver_keys(&prior_history),
            waiver_keys(&after_history),
            "forced recovery created a borrower waiver"
        );
        let mut liquidator = Value::Null;
        if flow == "liquidate_funded_v3" {
            let ScVal::Address(who) = &call.args[0] else {
                panic!("liquidator")
            };
            let liq_before = holder(&before, who);
            let liq_after = holder(&e, who);
            if no_new_reward_or_payout {
                assert_eq!(liq_after, liq_before, "liquidator received earlier rewards");
            }
            liquidator = json!({"address_xdr":b64(who),"before":holder_json(liq_before),"after":holder_json(liq_after),"exact_unchanged":no_new_reward_or_payout});
        }
        beneficiaries = json!({"supported":true,"borrower":strkey(&owner),"position_id":id,
            "before":holder_json(old),"after":holder_json(new),"entitlement_not_reduced":true,
            "exact_unchanged":no_new_reward_or_payout,"preexisting_historical_records_preserved":prior_history.len(),
            "no_new_borrower_waiver_index":true,"liquidator":liquidator,
            "scope":"booked entitlement plus existing weight/settled/waiver-index records; does not reconstruct unresolved intervals"});
    }
    json!({"schema":"cl-restoration-postconditions-v1","flow":flow,
        "source_sha256":hash(include_bytes!("postconditions.rs")),
        "sdk_full_contract_data_sha256":sdk_hash,"host_full_contract_data_sha256":host["post_state_full_data_sha256"],
        "raw_full_contract_state_matches_host":raw_state_matches,
        "contract_state_matches_after_exact_mock_nonce_substitution":true,
        "compared_contract_data_sha256":compared_hash,"nonce_substitutions":nonce_substitutions,
        "comparison_scope":"all contract data/code exact except listed fresh temporary Void external mock nonce pairs; TTL and ledger headers not compared",
        "contract_events_match_host":true,"contract_event_count":sdk_events.len(),
        "actual_reward_token_balance":balance.to_string(),"received":received.to_string(),"paid":paid.to_string(),
        "actual_reward_backing_matches_received_minus_paid":true,"beneficiaries":beneficiaries,
        "assertion_scope":"separate post-execution clones; no writes returned to measured runtime",
        "ttl_equivalence_verified":false,"core_application_verified":false,"signature_verification_measured":false,
        "live_state_validation":false,"release_approval":false})
}
