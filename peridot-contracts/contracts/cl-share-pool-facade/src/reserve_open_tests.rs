//! Compiled, controlled-state reserve proofs. Enabling borrowing and the 60%
//! floor below are TEST ONLY, not a deployment policy or production approval.
use super::*;
use soroban_sdk::TryFromVal;

fn policy(f: &F, bps: u32, enabled: bool) {
    auth::<()>(
        &f.e,
        &f.admin,
        &f.cash,
        "set_min_cash_reserve_bps",
        (f.admin.clone(), bps).into_val(&f.e),
        &[],
    );
    auth::<()>(
        &f.e,
        &f.admin,
        &f.cash,
        "set_cash_borrowing_enabled",
        (f.admin.clone(), enabled).into_val(&f.e),
        &[],
    );
}
fn step(f: &F, id: u64, name: &str, label: &str) {
    auth::<()>(
        &f.e,
        &f.user,
        &f.margin,
        name,
        (f.user.clone(), id).into_val(&f.e),
        &[],
    );
    measure(&f.e, label, true);
}
fn reject(f: &F, id: u64, name: &str, label: &str, expected: &str) {
    let e = &f.e;
    if expected == "cash borrowing disabled" {
        assert!(!call::<bool>(
            e,
            &f.cash,
            "get_cash_borrowing_enabled",
            ().into_val(e)
        ));
    } else {
        assert_eq!(
            call::<u128>(e, &f.cash, "get_borrowable_cash", ().into_val(e)),
            0
        );
    }
    let permissionless = name == "resolve_pending_open_v3";
    let args: Vec<Val> = if permissionless {
        (id,).into_val(e)
    } else {
        (f.user.clone(), id).into_val(e)
    };
    if permissionless {
        e.mock_auths(&[]);
    } else {
        e.mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.margin,
                fn_name: name,
                args: args.clone(),
                sub_invokes: &[],
            },
        }]);
    }
    let before = e.to_ledger_snapshot().ledger_entries;
    #[cfg(feature = "transaction-evidence")]
    crate::transaction_evidence::capture_rejection_before(
        e,
        &f.margin,
        name,
        &args,
        if permissionless { None } else { Some(&f.user) },
        permissionless,
        label,
    );
    crate::cl_resource_limits::reset(e);
    assert!(e
        .try_invoke_contract::<Val, soroban_sdk::Error>(&f.margin, &Symbol::new(e, name), args)
        .is_err());
    let events = format!("{:?}", e.host().get_diagnostic_events().unwrap());
    // Release WASM strips Rust panic text. Check the actual failing ABI frame,
    // together with the independently read policy state, not a setup panic.
    let diagnostics = e.host().get_diagnostic_events().unwrap();
    let trap = diagnostics
        .0
        .iter()
        .map(|v| format!("{v:?}"))
        .find(|v| v.contains("VM call trapped: UnreachableCodeReached"))
        .expect("WASM policy rejection");
    let frame = if name == "swap_open_position_v3" {
        "borrow_for_margin_to_controller"
    } else if expected == "cash borrowing disabled" {
        name
    } else {
        "assert_borrowing_reserve"
    };
    assert!(
        trap.contains(&format!("StringM({frame})")),
        "{label}: wrong rejection frame: {trap}"
    );
    if name != "swap_open_position_v3" && expected == "cash borrowing disabled" {
        assert!(diagnostics.0.iter().any(|v| {
            let s = format!("{v:?}");
            s.contains("fn_return")
                && s.contains("get_cash_borrowing_enabled")
                && s.contains("Bool(false)")
        }));
    }
    assert!(
        !events.contains("Error(Auth"),
        "setup/auth rejection is not reserve proof"
    );
    measure(e, label, true);
    assert_eq!(
        e.to_ledger_snapshot().ledger_entries,
        before,
        "{label}: debt, custody and all historical reward entries must roll back"
    );
    std::println!("RESERVE_ATOMIC_ROLLBACK {label} complete_ledger_equal=true");
}
fn recover(f: &F, id: u64) {
    let e = &f.e;
    let max = 110_000_000u128;
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.cash.clone(), max as i128).into_val(e),
        sub_invokes: &[],
    };
    let transfers = [transfer];
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_full_for_margin",
        args: (id, f.user.clone(), max).into_val(e),
        sub_invokes: &transfers,
    };
    let paid: u128 = auth(
        e,
        &f.user,
        &f.margin,
        "recover_pending_open_funded",
        (f.user.clone(), id, max).into_val(e),
        &[repay],
    );
    measure(e, "reserve_blocked_pending_funded_recovery", true);
    assert!(paid > 0);
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    assert_eq!(token::Client::new(e, &f.share).balance(&f.margin), 0);
    let value: Val = call(e, &f.margin, "get_position", (id,).into_val(e));
    assert_eq!(
        soroban_sdk::xdr::ScVal::try_from_val(e, &value).unwrap(),
        soroban_sdk::xdr::ScVal::Void
    );
}

#[test]
fn compiled_reserve_liquidation_conversion_cannot_activate_healthy_pending() {
    let f = F::new();
    let e = &f.e;
    policy(&f, 6000, true);
    let id = begin(&f);
    step(
        &f,
        id,
        "swap_open_position_v3",
        "reserve_healthy_pending_swap",
    );
    policy(&f, 10000, false);
    e.ledger().with_mut(|l| {
        l.timestamp = 3000;
        l.sequence_number += 400;
    });
    let liquidator = Address::generate(e);
    let args: Vec<Val> = (liquidator.clone(), id).into_val(e);
    e.mock_auths(&[MockAuth {
        address: &liquidator,
        invoke: &MockAuthInvoke {
            contract: &f.margin,
            fn_name: "liquidate_position_v3",
            args: args.clone(),
            sub_invokes: &[],
        },
    }]);
    let before = e.to_ledger_snapshot().ledger_entries;
    #[cfg(feature = "transaction-evidence")]
    crate::transaction_evidence::capture_rejection_before(
        e,
        &f.margin,
        "liquidate_position_v3",
        &args,
        Some(&liquidator),
        false,
        "reserve_healthy_liquidation_conversion_rejected",
    );
    crate::cl_resource_limits::reset(e);
    assert!(e
        .try_invoke_contract::<Val, soroban_sdk::Error>(
            &f.margin,
            &Symbol::new(e, "liquidate_position_v3"),
            args
        )
        .is_err());
    let events = format!("{:?}", e.host().get_diagnostic_events().unwrap());
    let diagnostics = e.host().get_diagnostic_events().unwrap();
    let last_return = diagnostics
        .0
        .iter()
        .map(|v| format!("{v:?}"))
        .filter(|v| v.contains("fn_return"))
        .last()
        .unwrap();
    assert!(
        last_return.contains("get_exchange_rate"),
        "must complete fresh liquidation valuation: {last_return}"
    );
    let trap = diagnostics
        .0
        .iter()
        .map(|v| format!("{v:?}"))
        .find(|v| v.contains("VM call trapped: UnreachableCodeReached"))
        .unwrap();
    assert!(trap.contains("StringM(liquidate_position_v3)"));
    assert!(!events.contains("assert_borrowing_reserve"));
    assert!(!events.contains("Error(Auth"));
    measure(e, "reserve_healthy_liquidation_conversion_rejected", true);
    assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
    recover(&f, id);
}

#[test]
fn compiled_reserve_complete_open_positive() {
    let f = F::new();
    policy(&f, 6000, true);
    let id = begin(&f);
    step(&f, id, "swap_open_position_v3", "reserve_configured_swap");
    step(
        &f,
        id,
        "activate_open_position_v3",
        "reserve_configured_activate",
    );
    call::<()>(&f.e, &f.cash, "assert_borrowing_reserve", ().into_val(&f.e));
    assert!(
        call::<u128>(
            &f.e,
            &f.cash,
            "get_margin_borrow_balance",
            (id,).into_val(&f.e)
        ) > 0
    );
    assert!(token::Client::new(&f.e, &f.receipt).balance(&f.margin) > 0);

    // Borrowing restrictions also cannot strand an already active position.
    policy(&f, 10000, false);
    let e = &f.e;
    funded_tests::schedule_rewards(&f);
    funded_tests::advance_rewards(e, 1100);
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    let old: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "kill_claim",
        (f.admin.clone(),).into_val(e),
        &[],
    );
    let debt: u128 = call(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e));
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.cash.clone(), debt as i128).into_val(e),
        sub_invokes: &[],
    };
    let transfers = [transfer];
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_for_margin",
        args: (id, f.user.clone(), debt).into_val(e),
        sub_invokes: &transfers,
    };
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "repay_margin_position_v3",
        (f.user.clone(), id, debt).into_val(e),
        &[repay],
    );
    measure(e, "reserve_blocked_active_repayment", true);
    step(
        &f,
        id,
        "release_debt_free_position_v3",
        "reserve_blocked_active_release",
    );
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    // This public route releases receipt claims into the owner's free margin
    // balance; it does not promise underlying redemption or a wallet transfer.
    let free: u128 = call(
        e,
        &f.margin,
        "get_margin_balance_ptokens",
        (f.user.clone(), f.share.clone()).into_val(e),
    );
    assert!(free > 0);
    assert_eq!(
        token::Client::new(e, &f.receipt).balance(&f.margin),
        free as i128
    );
    let position: Val = call(e, &f.margin, "get_position", (id,).into_val(e));
    assert_eq!(
        soroban_sdk::xdr::ScVal::try_from_val(e, &position).unwrap(),
        soroban_sdk::xdr::ScVal::Void
    );
    let after: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    assert_eq!(after.pending, old.pending);
    assert_eq!(after.fraction, old.fraction);
}

#[test]
fn compiled_reserve_swap_rejects_floor_and_disabled_borrowing() {
    for (bps, enabled, label, expected) in [
        (
            10000,
            true,
            "reserve_swap_floor_rejected",
            "borrowing reserve",
        ),
        (
            6000,
            false,
            "reserve_swap_disabled_rejected",
            "cash borrowing disabled",
        ),
    ] {
        let f = F::new();
        let id = begin(&f);
        policy(&f, bps, enabled);
        reject(&f, id, "swap_open_position_v3", label, expected);
        assert_eq!(
            call::<u128>(
                &f.e,
                &f.cash,
                "get_margin_borrow_balance",
                (id,).into_val(&f.e)
            ),
            0
        );
        step(&f, id, "cancel_pending_open_v3", "reserve_unswapped_cancel");
    }
}

#[test]
fn compiled_reserve_all_finalizers_reject_interstage_exit_or_pause_then_recover() {
    for disabled in [false, true] {
        let f = F::new();
        let e = &f.e;
        policy(&f, 6000, true);
        let id = begin(&f);
        step(&f, id, "swap_open_position_v3", "reserve_interstage_swap");
        funded_tests::schedule_rewards(&f);
        funded_tests::advance_rewards(e, 1100);
        assert!(call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
        let old: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
        assert!(old.pending > 0);
        if disabled {
            policy(&f, 6000, false);
        } else {
            // A real supplier exit is permitted to cross the floor between
            // swap and finalization. No synthetic cash/state mutation.
            auth::<()>(
                e,
                &f.user,
                &f.cash,
                "withdraw",
                (f.user.clone(), 750_000_000u128).into_val(e),
                &[],
            );
            measure(e, "reserve_supplier_exit_below_floor", true);
            assert_eq!(
                call::<u128>(e, &f.cash, "get_borrowable_cash", ().into_val(e)),
                0
            );
        }
        for name in [
            "activate_open_position_v3",
            "execute_open_position_v3",
            "resolve_pending_open_v3",
        ] {
            if name == "resolve_pending_open_v3" {
                e.ledger().with_mut(|l| {
                    l.timestamp = 3000;
                    l.sequence_number += 400;
                });
            }
            let reason = if disabled {
                "cash borrowing disabled"
            } else {
                "minimum borrowing reserve"
            };
            let label = format!(
                "reserve_{}_{}_rollback",
                if disabled { "disabled" } else { "floor" },
                name
            );
            reject(&f, id, name, &label, reason);
        }
        // Both restrictions remain active for the risk-reducing funded route.
        policy(&f, 10000, false);
        auth::<()>(
            e,
            &f.admin,
            &f.pool,
            "kill_claim",
            (f.admin.clone(),).into_val(e),
            &[],
        );
        recover(&f, id);
        let after: RewardHolder =
            call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
        assert_eq!(after.pending, old.pending);
        assert_eq!(after.fraction, old.fraction);
        let totals: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
        const SCALE: u128 = 1_000_000_000_000_000_000;
        assert_eq!(
            totals.received * SCALE,
            (totals.paid + totals.quarantine + after.pending) * SCALE
                + after.fraction
                + totals.rounding_scaled
        );
        assert_eq!(
            token::Client::new(e, &f.reward).balance(&f.share) as u128,
            totals.received - totals.paid
        );
        std::println!("RESERVE_RECOVERY_CONSERVATION disabled={disabled} received={} pending={} quarantine={} no_waiver=true",
            totals.received, after.pending, totals.quarantine);
    }
}
