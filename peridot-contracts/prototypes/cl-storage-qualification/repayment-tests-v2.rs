//! Isolated additive full-repayment ABI qualification. Included as a child of
//! an unchanged copy of combined.rs to reuse its controlled-state fixture.
use super::*;
use soroban_sdk::testutils::Events;
use soroban_sdk::TryFromVal;

const FULL: &str = "repay_full_margin_position_v3";

fn repay_debt(f: &F, id: u64) -> u128 {
    inspect(
        f,
        &f.cash,
        "get_margin_borrow_balance",
        (id,).into_val(&f.e),
    )
}

fn rejected(
    f: &F,
    owner: &Address,
    id: u64,
    name: &str,
    maximum: Option<u128>,
    authorize: bool,
    label: &str,
    expected: &str,
) {
    let e = &f.e;
    // Check each negative trigger from cloned state before the measured root.
    // Optimized WASM strips panic strings; the assertions below therefore bind
    // exact host error types to actual reached calls and these state predicates.
    match expected {
        "debt remains" => assert!(repay_debt(f, id) > 0),
        "max repay too small" => assert!(maximum.unwrap() < repay_debt(f, id)),
        "bad amount" => assert_eq!(maximum, Some(0)),
        "not owner" => {
            let position: Option<Map<Symbol, Val>> =
                inspect(f, &f.margin, "get_position", (id,).into_val(e));
            assert_ne!(field::<Address>(e, &position.unwrap(), "owner"), *owner);
        }
        "position missing" => {
            let position: Option<Map<Symbol, Val>> =
                inspect(f, &f.margin, "get_position", (id,).into_val(e));
            assert!(position.is_none());
        }
        "balance is not sufficient to spend" => {
            let liquid = balance(f, &f.under, owner) as u128;
            assert!(maximum.unwrap() > liquid && liquid >= repay_debt(f, id));
        }
        "Auth" => assert!(!authorize),
        _ => panic!("unmapped negative proof"),
    }
    let args: Vec<Val> = match maximum {
        Some(max) => (owner.clone(), id, max).into_val(e),
        None => (owner.clone(), id).into_val(e),
    };
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (owner.clone(), f.cash.clone(), maximum.unwrap_or(0) as i128).into_val(e),
        sub_invokes: &[],
    };
    let transfers = [transfer];
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_full_for_margin",
        args: (id, owner.clone(), maximum.unwrap_or(0)).into_val(e),
        sub_invokes: &transfers,
    };
    let repayments = [repay];
    let subs: &[MockAuthInvoke] = if maximum.is_some() { &repayments } else { &[] };
    if authorize {
        e.mock_auths(&[MockAuth {
            address: owner,
            invoke: &MockAuthInvoke {
                contract: &f.margin,
                fn_name: name,
                args: args.clone(),
                sub_invokes: subs,
            },
        }]);
    } else {
        e.mock_auths(&[]);
    }
    crate::cl_resource_limits::reset(e);
    let before = e.to_ledger_snapshot();
    #[cfg(feature = "transaction-evidence")]
    crate::transaction_evidence::capture_rejection_before(
        e,
        &f.margin,
        name,
        &args,
        if authorize { Some(owner) } else { None },
        !authorize,
        label,
    );
    crate::cl_resource_limits::reset(e);
    let result =
        e.try_invoke_contract::<Val, soroban_sdk::Error>(&f.margin, &Symbol::new(e, name), args);
    assert!(result.is_err(), "{label}: operation unexpectedly succeeded");
    let diagnostics = e.host().get_diagnostic_events().unwrap();
    // Host29 invocation metering clears diagnostics at each new root.
    // These are this rejection's events, not prior failed-call diagnostics.
    let diagnostic = format!("{:?}", diagnostics);
    use soroban_sdk::xdr::{ContractEventBody, ScError, ScErrorCode, ScErrorType, ScVal};
    let error = match &result {
        Err(Ok(error)) => *error,
        _ => panic!("{label}: expected a specific host error: {result:?}"),
    };
    let called = |method: &str| {
        diagnostics.0.iter().any(|event| {
            let ContractEventBody::V0(body) = &event.event.body;
            body.topics.len() == 3
                && body.topics[0] == ScVal::Symbol("fn_call".try_into().unwrap())
                && body.topics[2] == ScVal::Symbol(method.try_into().unwrap())
        })
    };
    let has_error_type = |auth: bool| {
        diagnostics.0.iter().any(|event| {
            let ContractEventBody::V0(body) = &event.event.body;
            body.topics.iter().any(|topic| match topic {
                ScVal::Error(ScError::Auth(_)) if auth => true,
                ScVal::Error(ScError::WasmVm(ScErrorCode::InvalidAction)) if !auth => true,
                _ => false,
            })
        })
    };
    assert!(called(name), "{label}: root was not reached");
    if !authorize {
        assert!(
            has_error_type(true),
            "{label}: missing actual authorization rejection: {diagnostic}"
        );
        assert_eq!(
            error,
            soroban_sdk::Error::from_type_and_code(
                ScErrorType::Context,
                ScErrorCode::InvalidAction
            )
        );
        assert!(!called("repay_full_for_margin"));
    } else {
        assert!(
            !error.is_type(ScErrorType::Auth)
                && !diagnostic.contains("Error(Auth(")
                && !diagnostic.contains("Error(Auth,"),
            "{label}: fixture authorization failure: {diagnostic}"
        );
        if expected == "balance is not sufficient to spend" {
            assert!(
                diagnostic.contains(expected)
                    && called("transfer")
                    && called("repay_full_for_margin")
            );
        } else {
            assert_eq!(
                error,
                soroban_sdk::Error::from_type_and_code(
                    ScErrorType::Context,
                    ScErrorCode::InvalidAction
                ),
                "{label}: incorrect compiled guard error"
            );
            assert!(
                has_error_type(false),
                "{label}: missing underlying VM guard error: {diagnostic}"
            );
            match expected {
                "debt remains" => {
                    assert!(called("update_interest") && called("get_margin_borrow_balance"))
                }
                "max repay too small" => {
                    assert!(called("repay_full_for_margin") && !called("transfer"))
                }
                "bad amount" | "not owner" | "position missing" => {
                    assert!(!called("repay_full_for_margin"))
                }
                _ => panic!("unmapped compiled guard"),
            }
        }
    }
    measure(e, label, true);
    crate::cl_resource_limits::reset(e);
    assert!(
        before == e.to_ledger_snapshot(),
        "{label}: failed root must roll back all ledger entries, TTLs, debt, rewards and balances"
    );
    std::println!("REPAYMENT_EXPECTED_REJECTION label={label} expected={expected} complete_ledger_rollback=true result={result:?}");
}

fn assert_payment_events(f: &F, owner: &Address, maximum: u128, paid: u128) {
    use soroban_sdk::xdr::{ContractEventBody, ScVal};
    let from = ScVal::try_from_val(&f.e, &owner.to_val()).unwrap();
    let cash = ScVal::try_from_val(&f.e, &f.cash.to_val()).unwrap();
    let transfer = ScVal::Symbol("transfer".try_into().unwrap());
    let debit_val: Val = (maximum as i128).into_val(&f.e);
    let debit = ScVal::try_from_val(&f.e, &debit_val).unwrap();
    let surplus_val: Val = ((maximum - paid) as i128).into_val(&f.e);
    let surplus = ScVal::try_from_val(&f.e, &surplus_val).unwrap();
    let mut outgoing = false;
    let mut refund = paid == maximum;
    let events = f.e.events().all().filter_by_contract(&f.under);
    for event in events.events() {
        let ContractEventBody::V0(body) = &event.body;
        if body.topics.len() < 3 || body.topics[0] != transfer {
            continue;
        }
        outgoing |= body.topics[1] == from && body.topics[2] == cash && body.data == debit;
        refund |= body.topics[1] == cash && body.topics[2] == from && body.data == surplus;
    }
    assert!(outgoing, "the exact payer-authorized maximum must transfer");
    assert!(refund, "overpayment must refund inside the same root");
}

#[test]
fn compiled_repayment_baseline_aged_partial_route_leaves_residual_and_release_rejects() {
    let (f, owners, ids, selected) = fixture(true, true);
    let e = &f.e;
    let owner = &owners[1];
    let id = ids[0];
    advance(&f, 3_600 + 1100);
    let expected = repay_debt(&f, id);
    assert!(expected > MARGIN && expected < MAX_REPAY);
    let before = balance(&f, &f.under, owner);
    let reward_before = holder(&f, owner);
    enable_measurement(&f, &selected);
    // Reproduce the existing amount cap, authorizing the actual pre-accrual
    // transfer while offering a root amount large enough for post-accrual debt.
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (owner.clone(), f.cash.clone(), MARGIN as i128).into_val(e),
        sub_invokes: &[],
    };
    let transfers = [transfer];
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_for_margin",
        args: (id, owner.clone(), MAX_REPAY).into_val(e),
        sub_invokes: &transfers,
    };
    auth::<()>(
        e,
        owner,
        &f.margin,
        "repay_margin_position_v3",
        (owner.clone(), id, MAX_REPAY).into_val(e),
        &[repay],
    );
    measure(e, "repayment_baseline_pre_accrual_cap_reproduced", true);
    let residual = repay_debt(&f, id);
    assert_eq!(before - balance(&f, &f.under, owner), MARGIN as i128);
    assert_eq!(residual, expected - MARGIN);
    assert!(residual > 0, "old route must reproduce nonzero aged debt");
    assert_eq!(holder(&f, owner), reward_before);
    rejected(
        &f,
        owner,
        id,
        "release_debt_free_position_v3",
        None,
        true,
        "repayment_baseline_release_rejected",
        "debt remains",
    );
    conserve(&f, &owners, None, "baseline_residual_not_qualified");
    std::println!("REPAYMENT_BASELINE_REGRESSION offered_max={MAX_REPAY} post_accrual_debt={expected} paid={MARGIN} residual={residual} debtfree_release=false full_repayment_qualified=false");
}

#[test]
fn compiled_repayment_candidate_aged_full_max_refund_release_and_beneficiary_claims() {
    let (f, owners, ids, selected) = fixture(true, true);
    let e = &f.e;
    let booked: std::vec::Vec<_> = owners[1..9].iter().map(|o| holder(&f, o)).collect();
    let mut cursor = 0;
    let mut intervals = std::vec::Vec::new();
    enable_measurement(&f, &selected);
    for (n, id) in ids.iter().enumerate() {
        let owner = &owners[n + 1];
        advance(&f, 3_600 + 1200 + n as u64 * 80);
        let expected = repay_debt(&f, *id);
        assert!(expected > MARGIN && expected < MAX_REPAY);
        let saved = fault(&f);
        prove_both_calls_fail(&f);
        let before = balance(&f, &f.under, owner);
        let cash_before = balance(&f, &f.under, &f.cash);
        // First borrower exercises exact post-accrual maximum; later borrowers
        // prove explicit surplus transfer/refund with aged dynamic debt.
        let maximum = if n == 0 { expected } else { MAX_REPAY };
        let paid: u128 = funded(&f, owner, *id, FULL, false, maximum);
        measure(e, "repayment_candidate_aged_full_max", true);
        assert_eq!(paid, expected);
        assert_eq!(before - balance(&f, &f.under, owner), paid as i128);
        assert_eq!(balance(&f, &f.under, &f.cash) - cash_before, paid as i128);
        assert_payment_events(&f, owner, maximum, paid);
        debt_zero(&f, *id);
        assert_eq!(holder(&f, owner), booked[n]);
        let repeat: u128 = funded(&f, owner, *id, FULL, false, maximum);
        measure(e, "repayment_candidate_zero_debt_repeat", true);
        assert_eq!(repeat, 0);
        assert_eq!(before - balance(&f, &f.under, owner), paid as i128);
        auth::<()>(
            e,
            owner,
            &f.margin,
            "release_debt_free_position_v3",
            (owner.clone(), *id).into_val(e),
            &[],
        );
        measure(e, "repayment_candidate_debt_free_release", true);
        debt_zero(&f, *id);
        assert_eq!(holder(&f, owner), booked[n]);
        let ptokens: u128 = inspect(
            &f,
            &f.margin,
            "get_margin_balance_ptokens",
            (owner.clone(), f.share.clone()).into_val(e),
        );
        assert!(ptokens > 0);
        auth::<()>(
            e,
            owner,
            &f.margin,
            "transfer_margin_to_spot",
            (owner.clone(), f.share.clone(), ptokens).into_val(e),
            &[],
        );
        measure(
            e,
            "repayment_candidate_custody_release_both_calls_failed",
            true,
        );
        check_boundaries(&f, &mut cursor, &mut intervals);
        let reward_before = balance(&f, &f.reward, owner);
        let claimed: u128 = auth(
            e,
            owner,
            &f.share,
            "claim_rewards",
            (owner.clone(),).into_val(e),
            &[],
        );
        measure(e, "repayment_candidate_beneficiary_claim_outage", true);
        assert_eq!(claimed, booked[n].0);
        assert_eq!(
            balance(&f, &f.reward, owner) - reward_before,
            claimed as i128
        );
        assert_eq!(holder(&f, owner), (0, booked[n].1));
        restore(&f, saved);
        assert!(call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
        measure(e, "repayment_candidate_late_quarantined_rewards", true);
        auth::<()>(
            e,
            owner,
            &f.receipt,
            "withdraw",
            (owner.clone(), ptokens).into_val(e),
            &[],
        );
        measure(e, "repayment_candidate_receipt_exit", true);
        no_custody(&f, owner);
        verify_quarantine(&f, &intervals);
        std::println!("REPAYMENT_FULL_PROOF borrower={n} maximum={maximum} post_accrual_debt={expected} paid={paid} refund={} debt_zero=true repeated_payment=0 prior_beneficiary_claim={claimed} custody_cleared=true", maximum-paid);
    }
    assert_eq!(intervals.len(), 8);
    conserve(&f, &owners, None, "dynamic_repay_release_beneficiaries");
}

#[test]
fn compiled_repayment_candidate_insufficient_max_unauthorized_and_closed_rollback() {
    let (f, owners, ids, selected) = fixture(true, true);
    let e = &f.e;
    let owner = &owners[1];
    let id = ids[0];
    advance(&f, 3_600 + 1200);
    let debt = repay_debt(&f, id);
    assert!(debt > MARGIN && debt < MAX_REPAY);
    enable_measurement(&f, &selected);
    rejected(
        &f,
        owner,
        id,
        FULL,
        Some(debt - 1),
        true,
        "repayment_candidate_insufficient_max_rollback",
        "max repay too small",
    );
    rejected(
        &f,
        owner,
        id,
        FULL,
        Some(0),
        true,
        "repayment_candidate_zero_max_rollback",
        "bad amount",
    );
    rejected(
        &f,
        owner,
        id,
        FULL,
        Some(MAX_REPAY),
        false,
        "repayment_candidate_unauthorized_rollback",
        "Auth",
    );
    rejected(
        &f,
        &owners[2],
        id,
        FULL,
        Some(MAX_REPAY),
        true,
        "repayment_candidate_wrong_owner_rollback",
        "not owner",
    );
    // A valid payer with funds sufficient for debt but below the authorized
    // maximum must fail the actual SAC debit rather than silently lower it.
    let balance_before = balance(&f, &f.under, owner);
    let too_large = balance_before as u128 + 1;
    rejected(
        &f,
        owner,
        id,
        FULL,
        Some(too_large),
        true,
        "repayment_candidate_unfunded_max_rollback",
        "balance is not sufficient to spend",
    );
    let paid: u128 = funded(&f, owner, id, FULL, false, MAX_REPAY);
    measure(e, "repayment_candidate_after_rejections_full_repay", true);
    assert_eq!(paid, debt);
    debt_zero(&f, id);
    auth::<()>(
        e,
        owner,
        &f.margin,
        "release_debt_free_position_v3",
        (owner.clone(), id).into_val(e),
        &[],
    );
    measure(e, "repayment_candidate_after_rejections_release", true);
    rejected(
        &f,
        owner,
        id,
        FULL,
        Some(MAX_REPAY),
        true,
        "repayment_candidate_closed_repay_rollback",
        "position missing",
    );
    rejected(
        &f,
        owner,
        id,
        "release_debt_free_position_v3",
        None,
        true,
        "repayment_candidate_repeated_release_rollback",
        "position missing",
    );
    conserve(&f, &owners, None, "dynamic_repay_adversarial");
}
