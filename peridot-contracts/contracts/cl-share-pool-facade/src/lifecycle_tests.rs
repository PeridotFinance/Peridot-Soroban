//! Compiled vault with pinned Aquarius bytecode and controlled pool state.
//! Synthetic generated signers are narrowly mocked; own-contract calls are real.
use super::*;

const SCALE: u128 = 1_000_000_000_000_000_000;

#[contracttype]
#[derive(Clone)]
struct Boundary {
    holder: Address,
    shares: u128,
    index: u128,
    reading: Option<u128>,
    timestamp: u64,
    interval: u64,
    waived: bool,
    proven: bool,
}

fn deposit(f: &F) -> i128 {
    let e = &f.e;
    let sub = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.share.clone(), 100_000_000i128).into_val(e),
        sub_invokes: &[],
    };
    auth(
        e,
        &f.user,
        &f.share,
        "deposit_underlying",
        (f.user.clone(), 100_000_000i128, 1i128).into_val(e),
        &[sub],
    )
}
fn rewards(f: &F) {
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
fn advance(e: &Env, timestamp: u64) {
    e.ledger().with_mut(|l| {
        l.timestamp = timestamp;
        l.sequence_number += 20;
    });
}
fn exact(e: &Env, who: &Address, to: &Address, name: &str, args: &Vec<Val>) {
    e.mock_auths(&[MockAuth {
        address: who,
        invoke: &MockAuthInvoke {
            contract: to,
            fn_name: name,
            args: args.clone(),
            sub_invokes: &[],
        },
    }]);
    crate::cl_resource_limits::reset(e);
}

#[test]
fn compiled_allowance_reward_boundary_and_claim_rollback() {
    let f = F::new();
    let e = &f.e;
    let other = Address::generate(e);
    let spender = Address::generate(e);
    let minted = deposit(&f);
    let half = minted / 2;
    rewards(&f);
    advance(e, 1100);
    let args: Vec<Val> = (f.user.clone(), spender.clone(), minted, 1000u32).into_val(e);
    auth::<()>(e, &f.user, &f.share, "approve", args, &[]);
    measure(e, "allowance_approve", true);
    let unclaimed: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    assert_eq!(
        unclaimed.received, 0,
        "approval must not move reward ownership"
    );

    // Exact spender signature is required, including the recipient and amount.
    let args: Vec<Val> = (spender.clone(), f.user.clone(), other.clone(), half).into_val(e);
    e.mock_auths(&[]);
    #[cfg(feature="transaction-evidence")]
    crate::transaction_evidence::capture_rejection_before(e,&f.share,"transfer_from",&args,None,true,"allowance_missing_spender_auth");
    crate::cl_resource_limits::reset(e);
    assert!(e
        .try_invoke_contract::<(), soroban_sdk::Error>(
            &f.share,
            &Symbol::new(e, "transfer_from"),
            args.clone()
        )
        .is_err());
    assert!(format!("{:?}", e.host().get_diagnostic_events().unwrap()).contains("Auth"));
    measure(e, "allowance_missing_spender_auth", true);
    let wrong_args: Vec<Val> = (spender.clone(), f.user.clone(), spender.clone(), half).into_val(e);
    exact(e, &spender, &f.share, "transfer_from", &wrong_args);
    #[cfg(feature="transaction-evidence")]
    crate::transaction_evidence::capture_mismatched_root_rejection(e,&f.share,"transfer_from",&args,&spender,&wrong_args,"allowance_wrong_recipient_auth");
    crate::cl_resource_limits::reset(e);
    assert!(e
        .try_invoke_contract::<(), soroban_sdk::Error>(
            &f.share,
            &Symbol::new(e, "transfer_from"),
            args.clone()
        )
        .is_err());
    assert!(format!("{:?}", e.host().get_diagnostic_events().unwrap()).contains("Auth"));
    measure(e, "allowance_wrong_recipient_auth", true);

    auth::<()>(e, &spender, &f.share, "transfer_from", args, &[]);
    measure(e, "allowance_transfer_from", true);
    let old: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    let new: RewardHolder = call(e, &f.share, "reward_account", (other.clone(),).into_val(e));
    assert!(old.pending > 0);
    assert_eq!(new.pending, 0);
    let remaining: i128 = call(
        e,
        &f.share,
        "allowance",
        (f.user.clone(), spender.clone()).into_val(e),
    );
    assert_eq!(remaining, minted - half);

    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "kill_claim",
        (f.admin.clone(),).into_val(e),
        &[],
    );
    advance(e, 1200);
    let before: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    let args: Vec<Val> = (spender.clone(), f.user.clone(), other.clone(), remaining).into_val(e);
    exact(e, &spender, &f.share, "transfer_from", &args);
    #[cfg(feature="transaction-evidence")]
    crate::transaction_evidence::capture_rejection_before(e,&f.share,"transfer_from",&args,Some(&spender),false,"allowance_failed_claim_rollback");
    crate::cl_resource_limits::reset(e);
    assert_eq!(
        e.try_invoke_contract::<(), soroban_sdk::Error>(
            &f.share,
            &Symbol::new(e, "transfer_from"),
            args
        ),
        Err(Ok(soroban_sdk::Error::from_contract_error(801)))
    );
    measure(e, "allowance_failed_claim_rollback", true);
    assert_eq!(
        call::<i128>(
            e,
            &f.share,
            "allowance",
            (f.user.clone(), spender.clone()).into_val(e)
        ),
        remaining
    );
    assert_eq!(token::Client::new(e, &f.share).balance(&f.user), remaining);
    assert_eq!(token::Client::new(e, &f.share).balance(&other), half);
    assert_eq!(
        call::<RewardTotals>(e, &f.share, "reward_totals", ().into_val(e)).received,
        before.received
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
    let old: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    let new: RewardHolder = call(e, &f.share, "reward_account", (other.clone(),).into_val(e));
    let agent: RewardHolder = call(
        e,
        &f.share,
        "reward_account",
        (spender.clone(),).into_val(e),
    );
    let totals: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    assert!(old.pending > new.pending && new.pending > 0);
    assert_eq!(agent.pending, 0);
    assert_eq!(
        totals.received * SCALE,
        (old.pending + new.pending + totals.paid + totals.quarantine) * SCALE
            + old.fraction
            + new.fraction
            + totals.rounding_scaled
    );
    assert_eq!(
        token::Client::new(e, &f.reward).balance(&f.share) as u128,
        totals.received - totals.paid
    );
    std::println!("COMPILED_ALLOWANCE_CONSERVATION received={} old={} newcomer={} spender=0 fractional={} rounding={} failed_claim_allowance_rollback=true", totals.received, old.pending, new.pending, old.fraction + new.fraction, totals.rounding_scaled);
}

#[test]
fn compiled_waived_interval_remains_unresolved_and_quarantined() {
    let f = F::new();
    let e = &f.e;
    let other = Address::generate(e);
    let minted = deposit(&f);
    let half = minted / 2;
    auth::<()>(
        e,
        &f.user,
        &f.share,
        "transfer",
        (f.user.clone(), other.clone(), half).into_val(e),
        &[],
    );
    rewards(&f);
    advance(e, 1100);
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "kill_claim",
        (f.admin.clone(),).into_val(e),
        &[],
    );
    let before_other: RewardHolder =
        call(e, &f.share, "reward_account", (other.clone(),).into_val(e));
    let out: Vec<u128> = auth(
        e,
        &f.user,
        &f.share,
        "emergency_redeem",
        (f.user.clone(), (minted - half) as u128, true).into_val(e),
        &[],
    );
    assert!(out.get(0).unwrap() + out.get(1).unwrap() > 0);
    measure(e, "waived_partial_principal_exit", true);
    let boundary: Option<Boundary> = call(e, &f.share, "reward_boundary", (0u64,).into_val(e));
    let boundary = boundary.expect("failed claim must persist the ownership boundary");
    assert!(boundary.waived && !boundary.proven);
    assert_eq!(boundary.holder, f.user);
    assert_eq!(boundary.shares, (minted - half) as u128);
    assert_eq!(token::Client::new(e, &f.share).balance(&f.user), 0);
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
    measure(e, "unresolved_interval_quarantined", true);
    let totals: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    let old: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    let other_account: RewardHolder =
        call(e, &f.share, "reward_account", (other.clone(),).into_val(e));
    assert!(totals.quarantine > 0);
    assert_eq!(old.pending, 0);
    assert_eq!(
        other_account.pending, before_other.pending,
        "unresolved historical rewards must not be guessed into current ownership"
    );
    assert_eq!(totals.received, totals.paid + totals.quarantine);
    assert_eq!(
        token::Client::new(e, &f.reward).balance(&f.share) as u128,
        totals.quarantine
    );
    std::println!("COMPILED_UNRESOLVED_INTERVAL received={} quarantined={} waived_owner=0 original_coowner_credit=0 proven=false", totals.received, totals.quarantine);
    // These are explicit missing acceptance proofs, not passing reconciliation.
    // Keep their required flow names absent from successful resource records.
    std::println!("UNVERIFIED_CASE_QUARANTINE: this default-unverified interval has no completed attribution");
    std::println!("UNVERIFIED_CASE_NO_REDISTRIBUTION: original coowner is not credited without verified/governed attribution");
}
