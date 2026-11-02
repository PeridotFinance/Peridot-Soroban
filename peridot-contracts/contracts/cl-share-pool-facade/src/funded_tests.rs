//! Compiled lifecycle with actual Aquarius bytecode and controlled prices/state.
use super::*;

fn open(f: &F) -> u64 {
    let id = begin(f);
    auth::<()>(
        &f.e,
        &f.user,
        &f.margin,
        "swap_open_position_v3",
        (f.user.clone(), id).into_val(&f.e),
        &[],
    );
    auth::<()>(
        &f.e,
        &f.user,
        &f.margin,
        "activate_open_position_v3",
        (f.user.clone(), id).into_val(&f.e),
        &[],
    );
    id
}
fn funded<T: soroban_sdk::TryFromVal<Env, Val>>(
    f: &F,
    who: &Address,
    id: u64,
    name: &str,
    max: u128,
    extra: bool,
) -> T {
    let e = &f.e;
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (who.clone(), f.cash.clone(), max as i128).into_val(e),
        sub_invokes: &[],
    };
    let transfers = [transfer];
    let repayment = if name == "repay_margin_position_v3" {
        "repay_for_margin"
    } else {
        "repay_full_for_margin"
    };
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: repayment,
        args: (id, who.clone(), max).into_val(e),
        sub_invokes: &transfers,
    };
    let args = if extra {
        (who.clone(), id, max, 1u128).into_val(e)
    } else {
        (who.clone(), id, max).into_val(e)
    };
    auth(e, who, &f.margin, name, args, &[repay])
}
#[test]
fn compiled_repay_release() {
    let f = F::new();
    let e = &f.e;
    let id = open(&f);
    schedule_rewards(&f);
    advance_rewards(e, 1100);
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    let booked: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    assert!(booked.pending > 0);
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "kill_claim",
        (f.admin.clone(),).into_val(e),
        &[],
    );
    let debt: u128 = call(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e));
    funded::<()>(&f, &f.user, id, "repay_margin_position_v3", debt, false);
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "release_debt_free_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    measure(e, "repay_release", true);
    let paid: u128 = auth(
        e,
        &f.user,
        &f.share,
        "claim_rewards",
        (f.user.clone(),).into_val(e),
        &[],
    );
    assert_eq!(
        paid, booked.pending,
        "release must preserve booked rewards during upstream outage"
    );
    assert_reward_conservation(&f, &[&f.user]);
}
#[test]
fn compiled_funded_liquidation() {
    let f = F::new();
    let e = &f.e;
    let id = open(&f);
    schedule_rewards(&f);
    advance_rewards(e, 1100);
    let liquidator = Address::generate(e);
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), liquidator.clone(), 200_000_000i128).into_val(e),
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
            55_000_000_000_000i128,
        )
            .into_val(e),
        &[],
    );
    let before = token::Client::new(e, &f.under).balance(&liquidator);
    let result: (u128, u128) = funded(
        &f,
        &liquidator,
        id,
        "liquidate_funded_v3",
        110_000_000,
        true,
    );
    measure(e, "funded_liquidation", true);
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    assert_eq!(
        before - token::Client::new(e, &f.under).balance(&liquidator),
        result.0 as i128
    );
    assert_eq!(
        token::Client::new(e, &f.receipt).balance(&liquidator),
        result.1 as i128
    );
    assert!(result.0 > 0 && result.1 > 0);
    let old: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    let new: RewardHolder = call(
        e,
        &f.share,
        "reward_account",
        (liquidator.clone(),).into_val(e),
    );
    assert!(old.pending > 0);
    assert_eq!(
        new.pending, 0,
        "seizure must not transfer historical borrower rewards"
    );
    advance_rewards(e, 1200);
    let paid_liquidator: u128 = auth(
        e,
        &liquidator,
        &f.share,
        "claim_rewards",
        (liquidator.clone(),).into_val(e),
        &[],
    );
    let paid_owner: u128 = auth(
        e,
        &f.user,
        &f.share,
        "claim_rewards",
        (f.user.clone(),).into_val(e),
        &[],
    );
    assert!(paid_liquidator > 0);
    assert!(paid_owner >= old.pending);
    assert_reward_conservation(&f, &[&f.user, &liquidator]);
    std::println!("COMPILED_FUNDED_REWARD_OWNERSHIP borrower_historical={} borrower_paid={paid_owner} liquidator_paid={paid_liquidator}",old.pending);
}
#[test]
fn compiled_interrupted_open_recovery_claim_outage() {
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
    schedule_rewards(&f);
    advance_rewards(e, 1100);
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    let booked: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    assert!(booked.pending > 0);
    advance_rewards(e, 1200);
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "kill_claim",
        (f.admin.clone(),).into_val(e),
        &[],
    );
    assert_failed_recovery_atomic(&f, id);
    let before = token::Client::new(e, &f.share).balance(&f.user);
    let paid: u128 = funded(
        &f,
        &f.user,
        id,
        "recover_pending_open_funded",
        110_000_000,
        false,
    );
    measure(e, "cancel_after_swap", true);
    assert!(paid > 0);
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    assert!(token::Client::new(e, &f.share).balance(&f.user) > before);
    assert_eq!(token::Client::new(e, &f.share).balance(&f.margin), 0);
    let boundary: Option<RecoveryBoundary> =
        call(e, &f.share, "reward_boundary", (0u64,).into_val(e));
    let boundary = boundary.expect("outage recovery must record borrower boundary");
    assert_eq!(boundary.holder, f.user);
    assert!(!boundary.waived && !boundary.proven);
    let claimed: u128 = auth(
        e,
        &f.user,
        &f.share,
        "claim_rewards",
        (f.user.clone(),).into_val(e),
        &[],
    );
    assert_eq!(
        claimed, booked.pending,
        "previously booked rewards remain payable during outage"
    );
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "unkill_claim",
        (f.admin.clone(),).into_val(e),
        &[],
    );
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    let totals: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    assert!(totals.quarantine > 0);
    assert_reward_conservation(&f, &[&f.user]);
    std::println!("COMPILED_RECOVERY_REWARD_CONSERVATION booked_paid={claimed} late_claim_quarantined={} borrower_waived=false unresolved=true",totals.quarantine);
}

#[contracttype]
#[derive(Clone)]
struct RecoveryBoundary {
    holder: Address,
    shares: u128,
    index: u128,
    reading: Option<u128>,
    timestamp: u64,
    interval: u64,
    waived: bool,
    proven: bool,
}
fn schedule_rewards(f: &F) {
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
}
fn advance_rewards(e: &Env, timestamp: u64) {
    e.ledger().with_mut(|l| {
        l.timestamp = timestamp;
        l.sequence_number += 20;
    });
}
fn assert_reward_conservation(f: &F, owners: &[&Address]) {
    const SCALE: u128 = 1_000_000_000_000_000_000;
    let e = &f.e;
    let mut pending = 0;
    let mut fractions = 0;
    for owner in owners {
        let h: RewardHolder = call(
            e,
            &f.share,
            "reward_account",
            ((*owner).clone(),).into_val(e),
        );
        pending += h.pending;
        fractions += h.fraction;
    }
    for custodian in [&f.facade, &f.margin, &f.receipt] {
        let h: RewardHolder = call(
            e,
            &f.share,
            "reward_account",
            (custodian.clone(),).into_val(e),
        );
        assert_eq!(h.pending, 0, "custodian cannot own borrower rewards");
        assert_eq!(h.fraction, 0);
    }
    let t: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    assert_eq!(
        t.received * SCALE,
        (t.paid + t.quarantine + pending) * SCALE + fractions + t.rounding_scaled
    );
    assert_eq!(
        token::Client::new(e, &f.reward).balance(&f.share) as u128,
        t.received - t.paid
    );
}
fn assert_failed_recovery_atomic(f: &F, id: u64) {
    let e = &f.e;
    let debt: u128 = call(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e));
    assert!(debt > 1);
    let payer_before = token::Client::new(e, &f.under).balance(&f.user);
    let shares_before = token::Client::new(e, &f.share).balance(&f.margin);
    let allowance = debt - 1;
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.cash.clone(), allowance as i128).into_val(e),
        sub_invokes: &[],
    };
    let transfers = [transfer];
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_full_for_margin",
        args: (id, f.user.clone(), allowance).into_val(e),
        sub_invokes: &transfers,
    };
    let repayments = [repay];
    let args: Vec<Val> = (f.user.clone(), id, allowance).into_val(e);
    e.mock_auths(&[MockAuth {
        address: &f.user,
        invoke: &MockAuthInvoke {
            contract: &f.margin,
            fn_name: "recover_pending_open_funded",
            args: args.clone(),
            sub_invokes: &repayments,
        },
    }]);
    crate::cl_resource_limits::reset(e);
    let result = e.try_invoke_contract::<u128, soroban_sdk::Error>(
        &f.margin,
        &Symbol::new(e, "recover_pending_open_funded"),
        args,
    );
    assert!(result.is_err(), "underfunded recovery must reject");
    let diagnostics = format!("{:?}", e.host().get_diagnostic_events().unwrap());
    assert!(
        diagnostics.contains("repay_full_for_margin") && !diagnostics.contains("Error(Auth,"),
        "failure must reach debt repayment, not fixture authorization: {diagnostics}"
    );
    measure(e, "underfunded_recovery_atomic_rejection", true);
    assert_eq!(
        token::Client::new(e, &f.under).balance(&f.user),
        payer_before
    );
    assert_eq!(
        token::Client::new(e, &f.share).balance(&f.margin),
        shares_before
    );
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        debt
    );
    assert!(
        call::<Option<RecoveryBoundary>>(e, &f.share, "reward_boundary", (0u64,).into_val(e))
            .is_none()
    );
}
