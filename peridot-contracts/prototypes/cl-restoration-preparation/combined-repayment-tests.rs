//! Focused combined storage + full-repayment qualification. Included as a
//! child of a copied atomic combined.rs fixture; only combined WASMs qualify.
//! This suite retains64 owners/eight debt positions/407 historical checkpoints,
//! but executes two owner repayment lifecycles (exact maximum and refund).
use super::*;
use soroban_sdk::testutils::Events;
use soroban_sdk::TryFromVal;

const FULL: &str = "repay_full_margin_position_v3";

fn require_combined_artifacts() {
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    let build = PathBuf::from(
        std::env::var("CL_ATOMIC_BUILD_DIR").expect("combined build directory required"),
    );
    let artifacts =
        PathBuf::from(std::env::var("CL_ARTIFACTS").expect("combined artifacts required"));
    assert_eq!(
        artifacts.canonicalize().unwrap(),
        build.join("artifacts").canonicalize().unwrap()
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(build.join("build-manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["qualification_variant"],
        "combined-storage-and-full-repayment"
    );
    assert_eq!(
        manifest["margin_build_feature"],
        "funded-liquidation-validation"
    );
    assert_eq!(manifest["release_approval"], false);
    for (name, bytes) in [
        (
            "storage",
            include_bytes!("../cl-storage-qualification/storage.patch").as_slice(),
        ),
        (
            "repayment",
            include_bytes!("../cl-storage-qualification/repayment.patch").as_slice(),
        ),
    ] {
        assert_eq!(
            manifest["combined_patch_sha256"][name],
            format!("{:x}", Sha256::digest(bytes)),
            "combined source overlay pin: {name}"
        );
    }
    for name in [
        "margin_controller.wasm",
        "receipt_vault.wasm",
        "simple_peridottroller.wasm",
    ] {
        let bytes = std::fs::read(artifacts.join(name)).unwrap();
        let actual = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            manifest["wasm"][name]["sha256"], actual,
            "combined artifact pin: {name}"
        );
        assert_eq!(
            manifest["wasm"][name]["bytes"],
            bytes.len(),
            "combined artifact size: {name}"
        );
        std::println!("COMBINED_ARTIFACT_IDENTITY artifact={name} sha256={actual} bytes={} storage_and_full_repayment=true production_approval=false", bytes.len());
    }
}

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
    std::println!("COMBINED_REPAYMENT_EXPECTED_REJECTION label={label} expected={expected} complete_ledger_rollback=true result={result:?}");
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
fn compiled_combined_repayment_aged_full_max_refund_release_and_beneficiary_claims() {
    require_combined_artifacts();
    let (f, owners, ids, selected) = fixture(true, true);
    let e = &f.e;
    let booked: std::vec::Vec<_> = owners[1..9].iter().map(|o| holder(&f, o)).collect();
    let mut cursor = 0;
    let mut intervals = std::vec::Vec::new();
    enable_measurement(&f, &selected);
    for (n, id) in ids.iter().take(2).enumerate() {
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
        measure(e, "combined_repayment_aged_full_max", true);
        assert_eq!(paid, expected);
        assert_eq!(before - balance(&f, &f.under, owner), paid as i128);
        assert_eq!(balance(&f, &f.under, &f.cash) - cash_before, paid as i128);
        assert_payment_events(&f, owner, maximum, paid);
        debt_zero(&f, *id);
        assert_eq!(holder(&f, owner), booked[n]);
        let repeat: u128 = funded(&f, owner, *id, FULL, false, maximum);
        measure(e, "combined_repayment_zero_debt_repeat", true);
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
        measure(e, "combined_repayment_debt_free_release", true);
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
            "combined_repayment_custody_release_both_calls_failed",
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
        measure(e, "combined_repayment_beneficiary_claim_outage", true);
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
        measure(e, "combined_repayment_late_quarantined_rewards", true);
        auth::<()>(
            e,
            owner,
            &f.receipt,
            "withdraw",
            (owner.clone(), ptokens).into_val(e),
            &[],
        );
        measure(e, "combined_repayment_receipt_exit", true);
        no_custody(&f, owner);
        verify_quarantine(&f, &intervals);
        std::println!("COMBINED_REPAYMENT_FULL_PROOF borrower={n} maximum={maximum} post_accrual_debt={expected} paid={paid} refund={} debt_zero=true repeated_payment=0 prior_beneficiary_claim={claimed} custody_cleared=true", maximum-paid);
    }
    assert_eq!(intervals.len(), 2);
    conserve(
        &f,
        &owners,
        None,
        "combined_dynamic_repay_release_beneficiaries_2_of_8",
    );
}

#[test]
fn compiled_combined_repayment_insufficient_max_unauthorized_and_closed_rollback() {
    require_combined_artifacts();
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
        "combined_repayment_insufficient_max_rollback",
        "max repay too small",
    );
    rejected(
        &f,
        owner,
        id,
        FULL,
        Some(0),
        true,
        "combined_repayment_zero_max_rollback",
        "bad amount",
    );
    rejected(
        &f,
        owner,
        id,
        FULL,
        Some(MAX_REPAY),
        false,
        "combined_repayment_unauthorized_rollback",
        "Auth",
    );
    rejected(
        &f,
        &owners[2],
        id,
        FULL,
        Some(MAX_REPAY),
        true,
        "combined_repayment_wrong_owner_rollback",
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
        "combined_repayment_unfunded_max_rollback",
        "balance is not sufficient to spend",
    );
    let paid: u128 = funded(&f, owner, id, FULL, false, MAX_REPAY);
    measure(e, "combined_repayment_after_rejections_full_repay", true);
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
    measure(e, "combined_repayment_after_rejections_release", true);
    rejected(
        &f,
        owner,
        id,
        FULL,
        Some(MAX_REPAY),
        true,
        "combined_repayment_closed_repay_rollback",
        "position missing",
    );
    rejected(
        &f,
        owner,
        id,
        "release_debt_free_position_v3",
        None,
        true,
        "combined_repayment_repeated_release_rollback",
        "position missing",
    );
    conserve(&f, &owners, None, "combined_dynamic_repay_adversarial");
}

#[test]
fn compiled_combined_funded_liquidation_without_borrower_authorization() {
    require_combined_artifacts();
    let (f, owners, ids, selected) = fixture(true, true);
    let e = &f.e;
    let owner = &owners[1];
    let id = ids[0];
    let liquidator = fresh_liquidator(&f, &owners);
    assert_ne!(&liquidator, owner);
    // Funding and TEST-only oracle configuration happen before the assessed
    // critical root. The borrower grants no authority for liquidation.
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), liquidator.clone(), 300_000_000i128).into_val(e),
        &[],
    );
    auth::<()>(
        e,
        &f.admin,
        &f.oracle,
        "set_test_price",
        (
            f.admin.clone(),
            Asset::Stellar(f.share.clone()),
            45_000_000_000_000i128,
        )
            .into_val(e),
        &[],
    );
    let booked = holder(&f, owner);
    assert!(booked.0 > 0);
    advance(&f, 3_600 + 1200);
    let expected = repay_debt(&f, id);
    assert!(expected > MARGIN && expected < MAX_REPAY);
    let saved = fault(&f);
    let mut cursor = 0;
    let mut intervals = std::vec::Vec::new();
    enable_measurement(&f, &selected);
    prove_both_calls_fail(&f);
    let cash_before = balance(&f, &f.under, &liquidator);
    // funded() installs exactly the liquidator's root/receipt/SAC auth tree.
    // V3, the receipt, facade and share vault perform their real nested calls.
    crate::transaction_evidence::set_restoration_trade_actor(&f.user);
    let (paid, seized): (u128, u128) =
        funded(&f, &liquidator, id, "liquidate_funded_v3", true, MAX_REPAY);
    measure(e, "combined_liquidation_without_borrower_auth", true);
    assert_eq!(paid, expected);
    assert!(seized > 0);
    assert_eq!(
        cash_before - balance(&f, &f.under, &liquidator),
        paid as i128
    );
    assert_payment_events(&f, &liquidator, MAX_REPAY, paid);
    debt_zero(&f, id);
    no_custody(&f, owner);
    assert_eq!(
        holder(&f, owner),
        booked,
        "forced liquidation cannot waive rewards"
    );
    assert_eq!(
        holder(&f, &liquidator),
        (0, 0),
        "liquidator cannot receive prior rewards"
    );
    check_boundaries(&f, &mut cursor, &mut intervals);
    assert_eq!(intervals.len(), 1);
    restore(&f, saved);
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    measure(e, "combined_liquidation_late_quarantine", true);
    verify_quarantine(&f, &intervals);
    conserve(
        &f,
        &owners,
        Some(&liquidator),
        "combined_liquidation_without_borrower_auth",
    );
    std::println!("COMBINED_BORROWER_INDEPENDENT_LIQUIDATION paid={paid} seized={seized} refund={} borrower_booked_reward={} borrower_authorization_supplied=false debt_zero=true no_forced_waiver=true restoration_evidence_is_separate=true", MAX_REPAY-paid,booked.0);
}
