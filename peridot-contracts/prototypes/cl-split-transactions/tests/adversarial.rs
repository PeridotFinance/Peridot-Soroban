//! Bounded isolated prototype failures. Actual compiled contracts and pinned
//! Aquarius; explicitly marked storage faults are controlled-state fault injection.
use super::*;

fn reject_business(f: &F, who: &Address, name: &str, args: Vec<Val>, label: &str) {
    reject(f, who, name, args, label);
    let events = format!("{:?}", f.e.host().get_diagnostic_events().unwrap());
    assert!(
        !events.contains("Error(Auth"),
        "{label}: setup authorization is not a business rejection"
    );
    assert!(
        !events.contains("Error(Budget"),
        "{label}: a resource limit is not the required business rejection"
    );
}
fn pause_claim(f: &F, killed: bool) {
    auth::<()>(
        &f.e,
        &f.admin,
        &f.pool,
        if killed { "kill_claim" } else { "unkill_claim" },
        (f.admin.clone(),).into_val(&f.e),
        &[],
    );
}
fn rewards(f: &F) {
    auth::<()>(
        &f.e,
        &f.user,
        &f.reward,
        "transfer",
        (f.user.clone(), f.pool.clone(), 100_000_000i128).into_val(&f.e),
        &[],
    );
    auth::<()>(
        &f.e,
        &f.admin,
        &f.pool,
        "set_rewards_config",
        (f.admin.clone(), 10_000u64, 10_000u128).into_val(&f.e),
        &[],
    );
}
fn advance(e: &Env, n: u64) {
    e.ledger().with_mut(|l| {
        l.timestamp += n;
        l.sequence_number += 20;
    });
}
fn job_of(f: &F, escrow: &Address, id: u64) -> Job {
    call(&f.e, escrow, "get_job", (id,).into_val(&f.e))
}
fn assert_recovered(f: &F, escrow: &Address, id: u64) {
    let j = job_of(f, escrow, id);
    assert_eq!(j.phase, 6);
    assert_eq!(j.underlying + j.other + j.shares, 0);
    let p: Val = call(&f.e, &f.margin, "get_position", (id,).into_val(&f.e));
    assert_eq!(
        soroban_sdk::xdr::ScVal::try_from_val(&f.e, &p).unwrap(),
        soroban_sdk::xdr::ScVal::Void
    );
}

#[test]
fn compiled_split_each_phase_legacy_replay_guards_dust_recovery() {
    for phase in 1..=5u32 {
        let (f, escrow) = setup();
        let e = &f.e;
        auth::<()>(
            e,
            &f.user,
            &f.under,
            "transfer",
            (f.user.clone(), escrow.clone(), 777i128).into_val(e),
            &[],
        );
        auth::<()>(
            e,
            &f.user,
            &f.paired,
            "transfer",
            (f.user.clone(), escrow.clone(), 333i128).into_val(e),
            &[],
        );
        let id = begin(&f);
        fund(&f, id);
        if phase == 2 {
            let _: u128 = stage(
                &f,
                "split_convert",
                (f.user.clone(), id, 100_000_000u128, 90_000_000u128).into_val(e),
            );
        }
        if phase >= 3 {
            prepare(&f, id);
        }
        if phase >= 4 {
            mint(&f, id);
        }
        if phase == 5 {
            let _: u128 = stage(&f, "split_deliver", (f.user.clone(), id).into_val(e));
        }
        assert_eq!(state(&f, id).phase, phase);
        let other = Address::generate(e);
        reject_business(
            &f,
            &other,
            "split_recover",
            (other.clone(), id, 110_000_000u128).into_val(e),
            "split_wrong_owner_each_phase",
        );
        let (method, args) = match phase {
            1 => (
                "split_fund",
                (f.user.clone(), id, 110_000_000u128, 210_000_000u128).into_val(e),
            ),
            2 => (
                "split_convert",
                (f.user.clone(), id, 100_000_000u128, 90_000_000u128).into_val(e),
            ),
            3 => (
                "split_convert_prepare",
                (f.user.clone(), id, 100_000_000u128, 90_000_000u128).into_val(e),
            ),
            4 => (
                "split_mint",
                (f.user.clone(), id, 194_040_000u128).into_val(e),
            ),
            _ => ("split_deliver", (f.user.clone(), id).into_val(e)),
        };
        reject_business(
            &f,
            &f.user,
            method,
            args,
            "split_duplicate_transition_each_phase",
        );
        for name in ["swap_open_position_v3", "cancel_pending_open_v3"] {
            reject_business(
                &f,
                &f.user,
                name,
                (f.user.clone(), id).into_val(e),
                "split_legacy_swap_cancel_each_phase",
            );
        }
        if phase < 5 {
            for name in [
                "execute_open_position_v3",
                "activate_open_position_v3",
                "liquidate_position_v3",
            ] {
                reject_business(
                    &f,
                    &f.user,
                    name,
                    (f.user.clone(), id).into_val(e),
                    "split_legacy_finalizer_before_delivery",
                );
            }
            reject_business(
                &f,
                &f.user,
                "recover_pending_open_funded",
                (f.user.clone(), id, 110_000_000u128).into_val(e),
                "split_old_recovery_before_delivery",
            );
        }
        policy(&f, 10000, false);
        recover(&f, id);
        if phase < 5 {
            assert_recovered(&f, &escrow, id);
        }
        assert_eq!(
            token::Client::new(e, &f.under).balance(&escrow),
            777,
            "donated underlying is never assigned to job"
        );
        assert_eq!(
            token::Client::new(e, &f.paired).balance(&escrow),
            333,
            "donated paired dust is never assigned to job"
        );
        assert_eq!(token::Client::new(e, &f.share).balance(&escrow), 0);
        reject_business(
            &f,
            &f.user,
            "split_recover",
            (f.user.clone(), id, 110_000_000u128).into_val(e),
            "split_repeated_recovery_each_phase",
        );
        std::println!("SPLIT_PHASE_RECOVERED phase={phase} debt=0 principal_dust_retained=777,333 old_entrypoints_rejected=true");
    }
}

fn external_pool_trade(f: &F, input: u128) {
    let e = &f.e;
    next(e);
    // Actual pinned bytecode requires this external sender at the pool root,
    // then at its nested token transfer. Only that external address is mocked.
    let transfers = [MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.pool.clone(), input as i128).into_val(e),
        sub_invokes: &[],
    }];
    let out: u128 = auth(
        e,
        &f.user,
        &f.pool,
        "swap",
        (f.user.clone(), 0u32, 1u32, input, 1u128).into_val(e),
        &transfers,
    );
    assert!(out > 0);
    measure(e, "split_actual_intervening_pool_swap", true);
}

#[test]
fn compiled_split_price_move_stale_fence_reprepare_reserve_pause() {
    let (f, escrow) = setup();
    let e = &f.e;
    let id = begin(&f);
    fund(&f, id);
    auth::<()>(
        e,
        &f.admin,
        &f.oracle,
        "set_test_price",
        (
            f.admin.clone(),
            Asset::Stellar(f.under.clone()),
            200_000_000_000_000i128,
        )
            .into_val(e),
        &[],
    );
    reject_business(
        &f,
        &f.user,
        "split_convert_prepare",
        (f.user.clone(), id, 100_000_000u128, 90_000_000u128).into_val(e),
        "split_fresh_price_divergence_reject",
    );
    assert_eq!(state(&f, id).phase, 1);
    auth::<()>(
        e,
        &f.admin,
        &f.oracle,
        "set_test_price",
        (
            f.admin.clone(),
            Asset::Stellar(f.under.clone()),
            100_000_000_000_000i128,
        )
            .into_val(e),
        &[],
    );
    prepare(&f, id);
    let old = job_of(&f, &escrow, id).fence.unwrap();
    external_pool_trade(&f, 1_000_000);
    reject_business(
        &f,
        &f.user,
        "split_mint",
        (f.user.clone(), id, 194_040_000u128).into_val(e),
        "split_external_swap_stale_fence_reject",
    );
    assert_eq!(state(&f, id).phase, 3);
    let new: BytesN<32> = stage(&f, "split_prepare", (f.user.clone(), id).into_val(e));
    assert_ne!(old, new);
    mint(&f, id);
    let _: u128 = stage(&f, "split_deliver", (f.user.clone(), id).into_val(e));
    policy(&f, 10000, false);
    reject_business(
        &f,
        &f.user,
        "activate_open_position_v3",
        (f.user.clone(), id).into_val(e),
        "split_post_delivery_paused_activation_reject",
    );
    recover(&f, id);
}

#[test]
fn compiled_split_claim_failure_mint_and_delivery_retry() {
    let (f, escrow) = setup();
    let e = &f.e;
    let id = begin(&f);
    fund(&f, id);
    prepare(&f, id);
    pause_claim(&f, true);
    reject_business(
        &f,
        &f.user,
        "split_mint",
        (f.user.clone(), id, 194_040_000u128).into_val(e),
        "split_claim_failure_mint_rollback",
    );
    assert_eq!(state(&f, id).phase, 3);
    assert_eq!(token::Client::new(e, &f.share).balance(&escrow), 0);
    pause_claim(&f, false);
    let minted = mint(&f, id);
    let allocation: u128 = call(
        e,
        &f.share,
        "custody_allocation",
        (escrow.clone(), f.user.clone()).into_val(e),
    );
    assert_eq!(allocation, minted);
    pause_claim(&f, true);
    reject_business(
        &f,
        &f.user,
        "split_deliver",
        (f.user.clone(), id).into_val(e),
        "split_claim_failure_delivery_rollback",
    );
    assert_eq!(state(&f, id).phase, 4);
    assert_eq!(
        token::Client::new(e, &f.share).balance(&escrow),
        minted as i128
    );
    pause_claim(&f, false);
    let delivered: u128 = stage(&f, "split_deliver", (f.user.clone(), id).into_val(e));
    assert_eq!(delivered, minted);
    assert_eq!(
        call::<u128>(
            e,
            &f.share,
            "custody_allocation",
            (escrow, f.user.clone()).into_val(e)
        ),
        0
    );
    assert_eq!(
        call::<u128>(
            e,
            &f.share,
            "custody_allocation",
            (f.margin.clone(), f.user.clone()).into_val(e)
        ),
        minted
    );
    recover(&f, id);
}

#[contracttype]
#[derive(Clone)]
enum PoolFaultKey {
    User(Address),
}
#[test]
fn compiled_split_both_reward_calls_outage_preserves_escrow_beneficiary() {
    let (f, escrow) = setup();
    let e = &f.e;
    rewards(&f);
    let id = begin(&f);
    fund(&f, id);
    prepare(&f, id);
    mint(&f, id);
    advance(e, 100);
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    let old: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    assert!(old.pending > 0);
    let escrow_rewards: RewardHolder =
        call(e, &f.share, "reward_account", (escrow.clone(),).into_val(e));
    assert_eq!(escrow_rewards.pending, 0);
    auth::<()>(
        e,
        &f.admin,
        &f.share,
        "enable_verified_reward_view",
        ().into_val(e),
        &[],
    );
    advance(e, 100);
    // Fault only Aquarius's per-vault user record. Bytecode is unchanged; this
    // is explicitly not a naturally observed Mainnet state or signature proof.
    let key = PoolFaultKey::User(f.share.clone());
    let saved: Val = e.as_contract(&f.pool, || {
        let old = e
            .storage()
            .persistent()
            .get(&key)
            .expect("pool record exists");
        e.storage().persistent().set(&key, &123u32);
        old
    });
    e.mock_auths(&[]);
    crate::cl_resource_limits::reset(e);
    assert!(e
        .try_invoke_contract::<u128, soroban_sdk::Error>(
            &f.pool,
            &Symbol::new(e, "get_user_reward"),
            (f.share.clone(),).into_val(e)
        )
        .is_err());
    measure(e, "split_actual_bytecode_pending_view_fault", true);
    assert!(!call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    measure(e, "split_actual_bytecode_claim_fault", true);
    policy(&f, 10000, false);
    recover(&f, id);
    assert_recovered(&f, &escrow, id);
    let after: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    assert_eq!(after.pending, old.pending);
    assert_eq!(after.fraction, old.fraction);
    assert_eq!(
        call::<u128>(
            e,
            &f.share,
            "custody_allocation",
            (escrow.clone(), f.user.clone()).into_val(e)
        ),
        0
    );
    e.as_contract(&f.pool, || e.storage().persistent().set(&key, &saved));
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    let totals: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    assert!(totals.quarantine > 0);
    assert_eq!(
        totals.received,
        totals.paid + token::Client::new(e, &f.reward).balance(&f.share) as u128
    );
    std::println!("SPLIT_OUTAGE_CONSERVATION old_pending={} late_quarantine={} unchanged_bytecode=true controlled_fault=true owner_funded_recovery=true",old.pending,totals.quarantine);
}

fn recover_reject(f: &F, id: u64, nested: bool, empty: bool, label: &str) {
    let e = &f.e;
    next(e);
    let max = 110_000_000u128;
    let args: Vec<Val> = (f.user.clone(), id, max).into_val(e);
    let transfer = [MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.cash.clone(), max as i128).into_val(e),
        sub_invokes: &[],
    }];
    let repay = [MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_full_for_margin",
        args: (id, f.user.clone(), max).into_val(e),
        sub_invokes: &transfer,
    }];
    if empty {
        e.mock_auths(&[]);
    } else {
        e.mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.margin,
                fn_name: "split_recover",
                args: args.clone(),
                sub_invokes: if nested { &repay } else { &[] },
            },
        }]);
    }
    let before = e.to_ledger_snapshot().ledger_entries;
    if empty {
        crate::transaction_evidence::capture_rejection_before(
            e,
            &f.margin,
            "split_recover",
            &args,
            None,
            true,
            label,
        );
    } else if !nested {
        // Recording-mode unsigned replay cannot prove the missing nested auth.
        // This companion measures positive execution only; SDK proves rejection.
        crate::transaction_evidence::capture_before(
            e,
            &f.margin,
            "split_recover",
            &args,
            Some(&f.user),
        );
    } else {
        crate::transaction_evidence::capture_rejection_before(
            e,
            &f.margin,
            "split_recover",
            &args,
            Some(&f.user),
            false,
            label,
        );
    }
    crate::cl_resource_limits::reset(e);
    assert!(e
        .try_invoke_contract::<u128, soroban_sdk::Error>(
            &f.margin,
            &Symbol::new(e, "split_recover"),
            args
        )
        .is_err());
    let diagnostics = e.host().get_diagnostic_events().unwrap();
    let events = format!("{diagnostics:?}");
    if empty || !nested {
        assert!(events.contains("Error(Auth"));
    } else {
        assert!(!events.contains("Error(Auth"));
        assert!(
            diagnostics.0.iter().any(|v| {
                let s = format!("{v:?}");
                s.contains("fn_return") && s.contains("repay_full_for_margin")
            }),
            "repayment must return before release failure"
        );
    }
    measure(e, label, true);
    assert_eq!(
        before,
        e.to_ledger_snapshot().ledger_entries,
        "{label}: complete rollback"
    );
}
#[contracttype]
#[derive(Clone)]
enum SacFaultKey {
    Balance(Address),
}
// Official host29 SAC storage shape; preserve valid serialization while injecting
// a controlled cash shortfall. A malformed value only proves a runtime failure.
#[contracttype]
#[derive(Clone)]
struct SacBalanceValue {
    amount: i128,
    authorized: bool,
    clawback: bool,
}
#[test]
fn compiled_split_auth_and_post_repayment_release_failure_atomicity() {
    let (f, escrow) = setup();
    let e = &f.e;
    let id = begin(&f);
    fund(&f, id);
    recover_reject(&f, id, false, true, "split_unauthorized_recovery_reject");
    recover_reject(
        &f,
        id,
        false,
        false,
        "split_missing_nested_repayment_auth_reject",
    );
    // Controlled valid SAC balance shortfall makes the final cash release fail
    // AFTER full receipt repayment. Real SAC bytecode executes both transfers.
    let key = SacFaultKey::Balance(escrow.clone());
    let saved: SacBalanceValue = e.as_contract(&f.under, || {
        let old: SacBalanceValue = e
            .storage()
            .persistent()
            .get(&key)
            .expect("escrow SAC balance exists");
        let mut short = old.clone();
        short.amount = 0;
        e.storage().persistent().set(&key, &short);
        old
    });
    recover_reject(
        &f,
        id,
        true,
        false,
        "split_post_repayment_release_failure_rollback",
    );
    assert!(call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)) > 0);
    e.as_contract(&f.under, || e.storage().persistent().set(&key, &saved));
    recover(&f, id);
    assert_recovered(&f, &escrow, id);
}

#[test]
fn compiled_split_expiry_before_fund_and_after_conversion() {
    let (f, _) = setup();
    let e = &f.e;
    let id = begin(&f);
    advance(e, 50_000);
    reject_business(
        &f,
        &f.user,
        "split_fund",
        (f.user.clone(), id, 110_000_000u128, 210_000_000u128).into_val(e),
        "split_expired_before_fund_reject",
    );
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    call::<()>(e, &f.margin, "expire_pending_open_v3", (id,).into_val(e));
    measure(e, "split_expired_unfunded_margin_release", true);
    assert_eq!(
        call::<u128>(
            e,
            &f.margin,
            "get_margin_balance_ptokens",
            (f.user.clone(), f.under.clone()).into_val(e)
        ),
        100_000_000
    );
    let (f, escrow) = setup();
    let e = &f.e;
    let id = begin(&f);
    fund(&f, id);
    let _: u128 = stage(
        &f,
        "split_convert",
        (f.user.clone(), id, 100_000_000u128, 90_000_000u128).into_val(e),
    );
    let j = job_of(&f, &escrow, id);
    e.ledger().with_mut(|l| {
        l.timestamp = j.deadline + 1;
        l.sequence_number += 500;
    });
    reject_business(
        &f,
        &f.user,
        "split_prepare",
        (f.user.clone(), id).into_val(e),
        "split_expired_after_conversion_prepare_reject",
    );
    reject_business(
        &f,
        &f.user,
        "expire_pending_open_v3",
        (id,).into_val(e),
        "split_funded_expiry_cannot_cancel",
    );
    reject_business(
        &f,
        &f.user,
        "resolve_pending_open_v3",
        (id,).into_val(e),
        "split_expired_legacy_resolve_before_delivery_reject",
    );
    let paired = token::Client::new(e, &f.paired).balance(&f.user);
    policy(&f, 10000, false);
    recover(&f, id);
    assert_eq!(
        token::Client::new(e, &f.paired).balance(&f.user),
        paired + j.other as i128
    );
    assert_recovered(&f, &escrow, id);
}

#[test]
fn compiled_split_separate_convert_prepare_roots_and_recovery() {
    let (f, escrow) = setup();
    let e = &f.e;
    let id = begin(&f);
    fund(&f, id);
    let output: u128 = stage(
        &f,
        "split_convert",
        (f.user.clone(), id, 100_000_000u128, 90_000_000u128).into_val(e),
    );
    assert!(output > 0);
    assert_eq!(state(&f, id).phase, 2);
    let _: BytesN<32> = stage(&f, "split_prepare", (f.user.clone(), id).into_val(e));
    assert_eq!(state(&f, id).phase, 3);
    recover(&f, id);
    assert_recovered(&f, &escrow, id);
    std::println!("SPLIT_SEPARATE_PREPARATION_TRIAL measured_separate_roots=true full_open=false no_production_stage_selection=true");
}
