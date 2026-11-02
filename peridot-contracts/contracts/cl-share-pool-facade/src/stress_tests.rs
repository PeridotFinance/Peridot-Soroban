//! Bounded controlled-state stress cases. These are not live-state bounds.
use super::*;

fn run_stress(holders: u32, pool_trade: u128, label: &str) {
    let f = F::new();
    let e = &f.e;
    let initial = 100_000_000i128;
    let sub = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.share.clone(), initial).into_val(e),
        sub_invokes: &[],
    };
    let minted: i128 = auth(
        e,
        &f.user,
        &f.share,
        "deposit_underlying",
        (f.user.clone(), initial, 1i128).into_val(e),
        &[sub],
    );
    let per_holder = minted / (holders as i128 + 1);
    assert!(per_holder > 0);
    // Separate ledger entries for existing owners, never a synthetic unmetered
    // mutation of the vault. Every ownership boundary executes its real claim.
    for _ in 0..holders {
        let holder = Address::generate(e);
        auth::<()>(
            e,
            &f.user,
            &f.share,
            "transfer",
            (f.user.clone(), holder, per_holder).into_val(e),
            &[],
        );
    }
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

    // Move the actual pool's price through an authorized swap. Keep oracle,
    // valuation, slippage and authorization checks unchanged.
    let quote: u128 = call(
        e,
        &f.pool,
        "estimate_swap",
        (0u32, 1u32, pool_trade).into_val(e),
    );
    assert!(quote > 0);
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.pool.clone(), amount(pool_trade)).into_val(e),
        sub_invokes: &[],
    };
    let received: u128 = auth(
        e,
        &f.user,
        &f.pool,
        "swap",
        (f.user.clone(), 0u32, 1u32, pool_trade, quote).into_val(e),
        &[transfer],
    );
    measure(e, &format!("{label}_pool_shift"), true);
    assert_eq!(received, quote);
    let refreshed: bool = call(e, &f.share, "refresh_position_amounts", ().into_val(e));
    measure(e, &format!("{label}_refresh_position"), refreshed);
    assert!(refreshed);
    let nav: u128 = call(e, &f.share, "refresh_nav_root", ().into_val(e));
    measure(e, &format!("{label}_refresh_nav"), nav > 0);
    assert!(nav > 0);
    let before: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    let id = begin(&f);
    measure(e, &format!("{label}_begin"), true);
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "swap_open_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    measure(e, &format!("{label}_swap_open"), true);
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "activate_open_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    measure(e, &format!("{label}_activate"), true);
    let after: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    assert!(
        after.received > before.received,
        "stress must claim nonzero accrued rewards"
    );
    assert_eq!(
        after.received,
        after.paid + token::Client::new(e, &f.reward).balance(&f.share) as u128
    );
    assert_eq!(token::Client::new(e, &f.share).balance(&f.facade), 0);
    std::println!("CONTROLLED_STRESS {label} existing_holders={holders} pool_trade_raw={pool_trade} pool_received_raw={received} rewards_received={} rewards_paid={}; not a production-state bound",after.received,after.paid);
}

#[test]
fn compiled_stress_four_holders_shifted_pool() {
    run_stress(4, 1_000_000_000, "stress_4_holders_1pct_reserve_trade");
}

#[test]
fn compiled_stress_sixteen_holders_shifted_pool() {
    run_stress(16, 5_000_000_000, "stress_16_holders_5pct_reserve_trade");
}
