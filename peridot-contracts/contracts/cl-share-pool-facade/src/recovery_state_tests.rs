//! Public V3 state-machine recovery proofs with compiled contracts and real pool bytecode.
use super::*;
use soroban_sdk::{xdr::ScVal, TryFromVal};

fn canonical(e: &Env, to: &Address, name: &str, args: Vec<Val>) -> ScVal {
    let value: Val = call(e, to, name, args);
    ScVal::try_from_val(e, &value).unwrap()
}
fn state(f: &F, id: u64) -> std::vec::Vec<ScVal> {
    let e = &f.e;
    let mut rows = std::vec::Vec::new();
    for name in [
        "get_position",
        "get_pending_perps_open",
        "get_pending_perps_open_execution",
        "get_perps_position",
    ] {
        rows.push(canonical(e, &f.margin, name, (id,).into_val(e)));
    }
    rows.push(canonical(
        e,
        &f.cash,
        "get_margin_borrow_balance",
        (id,).into_val(e),
    ));
    for account in [&f.user, &f.margin, &f.receipt, &f.facade] {
        rows.push(canonical(
            e,
            &f.share,
            "balance",
            (account.clone(),).into_val(e),
        ));
        rows.push(canonical(
            e,
            &f.share,
            "reward_account",
            (account.clone(),).into_val(e),
        ));
    }
    rows.push(canonical(e, &f.share, "reward_totals", ().into_val(e)));
    rows.push(canonical(
        e,
        &f.receipt,
        "balance",
        (f.margin.clone(),).into_val(e),
    ));
    rows.push(canonical(
        e,
        &f.under,
        "balance",
        (f.user.clone(),).into_val(e),
    ));
    rows
}
fn owner_rejection(f: &F, id: u64, name: &str, label: &str) {
    let e = &f.e;
    let before = state(f, id);
    let args: Vec<Val> = (f.user.clone(), id).into_val(e);
    e.mock_auths(&[MockAuth {
        address: &f.user,
        invoke: &MockAuthInvoke {
            contract: &f.margin,
            fn_name: name,
            args: args.clone(),
            sub_invokes: &[],
        },
    }]);
    #[cfg(feature = "transaction-evidence")]
    crate::transaction_evidence::capture_rejection_before(
        e,
        &f.margin,
        name,
        &args,
        Some(&f.user),
        false,
        label,
    );
    crate::cl_resource_limits::reset(e);
    let result =
        e.try_invoke_contract::<Val, soroban_sdk::Error>(&f.margin, &Symbol::new(e, name), args);
    assert!(
        result.is_err(),
        "state-machine rejection unexpectedly succeeded"
    );
    let diagnostic = format!("{:?}", e.host().get_diagnostic_events().unwrap());
    assert!(
        !diagnostic.contains("Error(Auth,"),
        "setup/auth failure is not state rejection: {diagnostic}"
    );
    measure(e, label, true);
    assert_eq!(
        state(f, id),
        before,
        "rejected state transition changed principal, debt or rewards"
    );
    std::println!("PROOF_STATE_REJECTION {label} debt/collateral/rewards unchanged");
}
fn permissionless_rejection(f: &F, id: u64, name: &str, label: &str) {
    let before = state(f, id);
    rejection(&f.e, &f.margin, name, (id,).into_val(&f.e), label, false);
    assert_eq!(state(f, id), before);
}
fn reward_schedule(f: &F) {
    let e = &f.e;
    auth::<()>(
        e,
        &f.user,
        &f.reward,
        "transfer",
        (f.user.clone(), f.pool.clone(), 100_000_000i128).into_val(e),
        &[],
    );
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "set_rewards_config",
        (f.admin.clone(), 2000u64, 10_000u128).into_val(e),
        &[],
    );
    e.ledger().with_mut(|l| {
        l.timestamp = 1100;
        l.sequence_number += 20;
    });
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
}
fn repay_release(f: &F, id: u64) {
    let e = &f.e;
    let debt: u128 = call(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e));
    let t = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.cash.clone(), debt as i128).into_val(e),
        sub_invokes: &[],
    };
    let ts = [t];
    let r = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_for_margin",
        args: (id, f.user.clone(), debt).into_val(e),
        sub_invokes: &ts,
    };
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "repay_margin_position_v3",
        (f.user.clone(), id, debt).into_val(e),
        &[r],
    );
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "release_debt_free_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    assert_eq!(
        canonical(e, &f.margin, "get_position", (id,).into_val(e)),
        ScVal::Void
    );
}
fn funded_recovery(f: &F, id: u64) {
    let e = &f.e;
    let max = 110_000_000u128;
    let t = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.cash.clone(), max as i128).into_val(e),
        sub_invokes: &[],
    };
    let ts = [t];
    let r = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_full_for_margin",
        args: (id, f.user.clone(), max).into_val(e),
        sub_invokes: &ts,
    };
    let paid: u128 = auth(
        e,
        &f.user,
        &f.margin,
        "recover_pending_open_funded",
        (f.user.clone(), id, max).into_val(e),
        &[r],
    );
    assert!(paid > 0);
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    assert_eq!(token::Client::new(e, &f.share).balance(&f.margin), 0);
}
#[test]
fn compiled_cancel_after_execute_is_rejection_then_repay_release() {
    let f = F::new();
    let e = &f.e;
    let id = begin(&f);
    owner_rejection(
        &f,
        id,
        "execute_open_position_v3",
        "execute_before_swap_rejected",
    );
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "swap_open_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "execute_open_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    reward_schedule(&f);
    owner_rejection(
        &f,
        id,
        "cancel_pending_open_v3",
        "cancel_after_execute_rejection",
    );
    owner_rejection(
        &f,
        id,
        "execute_open_position_v3",
        "repeated_execute_rejected",
    );
    repay_release(&f, id);
    std::println!("LIFECYCLE_PROOF cancel_after_execute: explicit execute activation; cancellation rollback; authorized full repayment/release PASS");
}
#[test]
fn compiled_pending_before_swap_expiry_and_abandonment() {
    let f = F::new();
    let e = &f.e;
    let id = begin(&f);
    permissionless_rejection(
        &f,
        id,
        "expire_pending_open_v3",
        "premature_pending_expiry_rejected",
    );
    e.ledger().with_mut(|l| {
        l.timestamp = 2801;
        l.sequence_number += 360;
    });
    owner_rejection(
        &f,
        id,
        "swap_open_position_v3",
        "expired_unswapped_open_rejected",
    );
    call::<()>(e, &f.margin, "expire_pending_open_v3", (id,).into_val(e));
    measure(e, "expired_pending_before_swap", true);
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    assert_eq!(
        canonical(e, &f.margin, "get_position", (id,).into_val(e)),
        ScVal::Void
    );
}
#[test]
fn compiled_failed_activation_preserves_pending_recovery() {
    let f = F::new();
    let e = &f.e;
    let id = begin(&f);
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "swap_open_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    reward_schedule(&f);
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "kill_claim",
        (f.admin.clone(),).into_val(e),
        &[],
    );
    owner_rejection(
        &f,
        id,
        "activate_open_position_v3",
        "failed_activation_atomic",
    );
    owner_rejection(
        &f,
        id,
        "cancel_pending_open_v3",
        "swapped_pending_cancellation_rejected",
    );
    e.ledger().with_mut(|l| {
        l.timestamp = 2801;
        l.sequence_number += 360;
    });
    permissionless_rejection(
        &f,
        id,
        "expire_pending_open_v3",
        "expired_swapped_not_abandoned",
    );
    let rewards: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    assert!(rewards.pending > 0);
    funded_recovery(&f, id);
    let after: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    // Exact old entitlement remains preserved; share debt may legitimately reset.
    assert_eq!(
        (after.pending, after.fraction),
        (rewards.pending, rewards.fraction),
        "funded recovery must preserve booked historical entitlement exactly"
    );
    std::println!(
        "LIFECYCLE_PROOF failed_activation_and_expired_swap: rollback then funded recovery PASS"
    );
}
#[test]
fn compiled_expired_swapped_permissionless_resolution() {
    let f = F::new();
    let e = &f.e;
    let id = begin(&f);
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "swap_open_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    permissionless_rejection(
        &f,
        id,
        "resolve_pending_open_v3",
        "premature_pending_resolution_rejected",
    );
    e.ledger().with_mut(|l| {
        l.timestamp = 2801;
        l.sequence_number += 360;
    });
    call::<()>(e, &f.margin, "resolve_pending_open_v3", (id,).into_val(e));
    measure(e, "expired_pending_resolution", true);
    owner_rejection(
        &f,
        id,
        "cancel_pending_open_v3",
        "resolved_pending_cancel_rejected",
    );
    repay_release(&f, id);
}
