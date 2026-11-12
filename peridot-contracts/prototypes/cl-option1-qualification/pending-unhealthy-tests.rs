//! Isolated Option1 experiment; actual Aquarius controlled state and compiled
//! V3/receipt/vault/facade, synthetic compiled oracle and external identities.
//! A timeout zero-incentive forced unwind is UNADOPTED policy, not release approval.
use super::*;
use soroban_sdk::{testutils::Events, TryFromVal};
const RECOVER: &str = "liquidate_pending_open_funded";

fn artifact_guard() {
    use sha2::{Digest, Sha256};
    let build = std::path::PathBuf::from(std::env::var("CL_ATOMIC_BUILD_DIR").unwrap());
    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(build.join("build-manifest.json")).unwrap()).unwrap();
    assert_eq!(
        m["qualification_variant"],
        "option1-pending-independent-recovery-unadopted"
    );
    assert_eq!(m["margin_build_feature"], "funded-liquidation-validation");
    assert_eq!(m["release_approval"], false);
    assert_eq!(
        m["pending_overlay_sha256"],
        format!(
            "{:x}",
            Sha256::digest(include_bytes!("pending-recovery.patch"))
        )
    );
    assert_eq!(
        build.join("artifacts").canonicalize().unwrap(),
        std::path::PathBuf::from(std::env::var("CL_ARTIFACTS").unwrap())
            .canonicalize()
            .unwrap()
    );
    for name in [
        "margin_controller.wasm",
        "receipt_vault.wasm",
        "simple_peridottroller.wasm",
    ] {
        let bytes = std::fs::read(build.join("artifacts").join(name)).unwrap();
        assert_eq!(
            m["wasm"][name]["sha256"],
            format!("{:x}", Sha256::digest(&bytes))
        );
    }
}

fn reject(f: &F, payer: &Address, id: u64, max: u128, min: u128, auth_mode: u32, label: &str) {
    let e = &f.e;
    let args: Vec<Val> = (payer.clone(), id, max, min).into_val(e);
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (payer.clone(), f.cash.clone(), max as i128).into_val(e),
        sub_invokes: &[],
    };
    let transfers = [transfer];
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_full_for_margin",
        args: (id, payer.clone(), max).into_val(e),
        sub_invokes: &transfers,
    };
    let repayment = [repay];
    let sub = if auth_mode == 2 { &repayment[..] } else { &[] };
    if auth_mode > 0 {
        e.mock_auths(&[MockAuth {
            address: payer,
            invoke: &MockAuthInvoke {
                contract: &f.margin,
                fn_name: RECOVER,
                args: args.clone(),
                sub_invokes: sub,
            },
        }]);
    } else {
        e.mock_auths(&[]);
    }
    crate::cl_resource_limits::reset(e);
    let before = e.to_ledger_snapshot();
    #[cfg(feature = "transaction-evidence")]
    if auth_mode == 1 {
        // Host recording reconstructs the full external tree; it cannot prove
        // this SDK-only omitted-nested-tree rejection. Keep its positive
        // companion honestly classified as recording-only execution.
        crate::transaction_evidence::capture_before(e, &f.margin, RECOVER, &args, Some(payer));
    } else {
        crate::transaction_evidence::capture_rejection_before(
            e,
            &f.margin,
            RECOVER,
            &args,
            if auth_mode > 0 { Some(payer) } else { None },
            auth_mode == 0,
            label,
        );
    }
    crate::cl_resource_limits::reset(e);
    let result =
        e.try_invoke_contract::<Val, soroban_sdk::Error>(&f.margin, &Symbol::new(e, RECOVER), args);
    assert!(result.is_err(), "{label}: expected rejection");
    use soroban_sdk::xdr::{ContractEventBody, ScError, ScVal};
    let diagnostics = e.host().get_diagnostic_events().unwrap();
    let reached = diagnostics.0.iter().any(|event| {
        let ContractEventBody::V0(body) = &event.event.body;
        body.topics.len() == 3
            && body.topics[0] == ScVal::Symbol("fn_call".try_into().unwrap())
            && body.topics[2] == ScVal::Symbol(RECOVER.try_into().unwrap())
    });
    assert!(reached, "{label}: actual root not reached");
    let auth_error = diagnostics.0.iter().any(|event| {
        let ContractEventBody::V0(body) = &event.event.body;
        body.topics
            .iter()
            .any(|topic| matches!(topic, ScVal::Error(ScError::Auth(_))))
    });
    assert_eq!(
        auth_error,
        auth_mode < 2,
        "{label}: guard/auth failure classification"
    );
    measure(e, label, true);
    assert_eq!(
        before,
        e.to_ledger_snapshot(),
        "{label}: complete ledger, debt, custody, reward and TTL rollback"
    );
    std::println!("OPTION1_PENDING_REJECTION label={label} root_reached=true complete_rollback=true authorization_error={auth_error}");
}

fn payment_events(f: &F, payer: &Address, max: u128, paid: u128) {
    use soroban_sdk::xdr::{ContractEventBody, ScVal};
    let who = ScVal::try_from_val(&f.e, &payer.to_val()).unwrap();
    let cash = ScVal::try_from_val(&f.e, &f.cash.to_val()).unwrap();
    let debit_val: Val = (max as i128).into_val(&f.e);
    let refund_val: Val = ((max - paid) as i128).into_val(&f.e);
    let debit = ScVal::try_from_val(&f.e, &debit_val).unwrap();
    let refund = ScVal::try_from_val(&f.e, &refund_val).unwrap();
    let mut seen_debit = false;
    let mut seen_refund = paid == max;
    for event in f.e.events().all().filter_by_contract(&f.under).events() {
        let ContractEventBody::V0(body) = &event.body;
        if body.topics.len() < 3 || body.topics[0] != ScVal::Symbol("transfer".try_into().unwrap())
        {
            continue;
        }
        seen_debit |= body.topics[1] == who && body.topics[2] == cash && body.data == debit;
        seen_refund |= body.topics[1] == cash && body.topics[2] == who && body.data == refund;
    }
    assert!(
        seen_debit && seen_refund,
        "exact authorized maximum and atomic refund events"
    );
}

#[test]
fn compiled_option1_unhealthy_pending_before_expiry_independent_recovery() {
    artifact_guard();
    let (f, owners, ids, selected) = fixture(false, true);
    let e = &f.e;
    let owner = &owners[1];
    let id = ids[0];
    let payer = fresh_liquidator(&f, &owners);
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), payer.clone(), 300_000_000i128).into_val(e),
        &[],
    );
    let pending: Option<Map<Symbol, Val>> =
        inspect(&f, &f.margin, "get_pending_perps_open", (id,).into_val(e));
    let expiry: u64 = field(e, &pending.unwrap(), "expires_at");
    let execution: Option<Map<Symbol, Val>> = inspect(
        &f,
        &f.margin,
        "get_pending_perps_open_execution",
        (id,).into_val(e),
    );
    let held: u128 = field(e, &execution.unwrap(), "position_amount");
    advance(&f, expiry - 600);
    assert!(e.ledger().timestamp() < expiry);
    enable_measurement(&f, &selected);
    reject(
        &f,
        &payer,
        id,
        MAX_REPAY,
        1,
        2,
        "option1_pending_healthy_preexpiry_reject",
    );
    // Existing TEST-ONLY 25% maintenance with fresh synthetic price0.65 makes
    // equity < maintenance while retaining a positive residual after fair seizure.
    auth::<()>(
        e,
        &f.admin,
        &f.oracle,
        "set_test_price",
        (
            f.admin.clone(),
            Asset::Stellar(f.share.clone()),
            65_000_000_000_000i128,
        )
            .into_val(e),
        &[],
    );
    measure(e, "option1_pending_test_price_unhealthy", true);
    let expected: u128 = inspect(&f, &f.cash, "get_margin_borrow_balance", (id,).into_val(e));
    assert!(expected > MARGIN && expected < MAX_REPAY);
    let collateral_value = held * 65 / 100;
    assert!(collateral_value.saturating_sub(expected) <= collateral_value / 4);
    let booked = holder(&f, owner);
    assert!(booked.0 > 0);
    let saved = fault(&f);
    prove_both_calls_fail(&f);
    assert_eq!(balance(&f, &f.under, &payer), 300_000_000);
    reject(
        &f, &payer, id, 300_000_001, 1, 2,
        "option1_pending_unhealthy_insufficient_payer_cash",
    );
    reject(
        &f,
        &payer,
        id,
        expected - 1,
        1,
        2,
        "option1_pending_unhealthy_insufficient_max",
    );
    reject(
        &f,
        &payer,
        id,
        MAX_REPAY,
        1,
        0,
        "option1_pending_unhealthy_missing_auth",
    );
    let payer_cash = balance(&f, &f.under, &payer);
    let owner_shares = balance(&f, &f.share, owner);
    let (paid, seized): (u128, u128) = funded(&f, &payer, id, RECOVER, true, MAX_REPAY);
    measure(e, "option1_pending_preexpiry_unhealthy_funded", true);
    payment_events(&f, &payer, MAX_REPAY, paid);
    assert_eq!(paid, expected);
    assert_eq!(seized, expected * 100 / 65);
    assert!(seized < held);
    assert_eq!(payer_cash - balance(&f, &f.under, &payer), paid as i128);
    assert_eq!(
        balance(&f, &f.share, owner) - owner_shares,
        (held - seized) as i128
    );
    debt_zero(&f, id);
    no_custody(&f, owner);
    assert_eq!(holder(&f, owner), booked);
    assert_eq!(holder(&f, &payer), (0, 0));
    reject(
        &f,
        &payer,
        id,
        MAX_REPAY,
        1,
        2,
        "option1_pending_unhealthy_replay",
    );
    let mut cursor = 0;
    let mut intervals = std::vec::Vec::new();
    check_boundaries(&f, &mut cursor, &mut intervals);
    restore(&f, saved);
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    measure(e, "option1_pending_unhealthy_late_quarantine", true);
    verify_quarantine(&f, &intervals);
    conserve(&f, &owners, Some(&payer), "option1_unhealthy_preexpiry");
    std::println!("OPTION1_PENDING_UNHEALTHY paid={paid} seized={seized} returned={} before_expiry=true existing_test_maintenance=250000 zero_bonus=true owner_absent=true live_oracle_qualification=false", held - seized);
}
