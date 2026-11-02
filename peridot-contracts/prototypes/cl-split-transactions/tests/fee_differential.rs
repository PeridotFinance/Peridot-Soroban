//! Differential actual-bytecode proof: an additional same-ledger zero fee
//! collection versus no additional collection, followed by future accrual.
//! Neither branch changes the captured profile or mocks a contract's own auth.
use super::*;
use soroban_sdk::{xdr::ToXdr, U256};

#[contracttype]
#[derive(Clone)]
struct FeePosition {
    fee_growth_inside_0_last_x128: U256,
    fee_growth_inside_1_last_x128: U256,
    liquidity: u128,
    tokens_owed_0: u128,
    tokens_owed_1: u128,
}
#[contracttype]
#[derive(Clone)]
struct FeeSlot { sqrt_price_x96: U256, tick: i32 }

#[derive(Debug, PartialEq, Eq)]
struct Reading {
    label: &'static str,
    supply: i128,
    position: std::vec::Vec<u8>,
    fee_growth0: std::vec::Vec<u8>,
    fee_growth1: std::vec::Vec<u8>,
    fees: std::vec::Vec<u128>,
    withdrawal_quote: std::vec::Vec<u128>,
    idle: [i128; 2],
    fee_inclusive_nav: u128,
    custody: u128,
    direct: [i128; 2],
    rewards: [u128; 4],
    holders: [[u128; 3]; 3],
}

fn advance_fee(e: &Env, n: u64) {
    e.ledger().with_mut(|l| { l.timestamp += n; l.sequence_number += 1; });
}

fn fee_trade(f: &F, n: u128, reverse: bool) {
    let e = &f.e;
    let (i, j, asset) = if reverse { (1u32, 0u32, &f.paired) } else { (0u32, 1u32, &f.under) };
    let quote: u128 = call(e, &f.pool, "estimate_swap", (i, j, n).into_val(e));
    assert!(quote > 0, "rounding workload must execute a nonzero swap");
    let child = MockAuthInvoke { contract: asset, fn_name: "transfer",
        args: (f.user.clone(), f.pool.clone(), n as i128).into_val(e), sub_invokes: &[] };
    let actual: u128 = auth(e, &f.user, &f.pool, "swap",
        (f.user.clone(), i, j, n, quote).into_val(e), &[child]);
    assert_eq!(actual, quote);
    measure(e, "split_fee_future_actual_swap", true);
}

fn record(f: &F, escrow: &Address, bob: &Address, label: &'static str) -> Reading {
    let e = &f.e;
    let ticks: (i32, i32) = call(e, &f.share, "get_ticks", ().into_val(e));
    let position: FeePosition = call(e, &f.pool, "get_position",
        (f.share.clone(), ticks.0, ticks.1).into_val(e));
    let fees: Vec<u128> = call(e, &f.pool, "get_all_position_fees", (f.share.clone(),).into_val(e));
    assert_eq!(fees.len(), 2);
    let principal: Vec<u128> = call(e, &f.pool, "estimate_withdraw_position",
        (f.share.clone(), ticks.0, ticks.1, position.liquidity).into_val(e));
    assert_eq!(principal.len(), 2);
    // Equal-price, equal-decimal controlled fixture, without vault donations.
    // Actual pinned-bytecode ClaimFees/WithdrawLiquidity events establish that
    // estimate_withdraw_position includes ALL pending position fees. Thus the
    // full-position quote is already fee-inclusive: do not add fee_view again.
    // Fresh oracle root is asserted; no stale prepared NAV is reused.
    let root: u128 = call(e, &f.share, "refresh_nav_root", ().into_val(e));
    assert_eq!(root, 1_000_000_000);
    let idle = [token::Client::new(e, &f.under).balance(&f.share),
        token::Client::new(e, &f.paired).balance(&f.share)];
    assert!(idle[0] >= 0 && idle[1] >= 0);
    let fee_inclusive_nav = (idle[0] as u128).checked_add(idle[1] as u128).unwrap()
        .checked_add(principal.get(0).unwrap()).unwrap().checked_add(principal.get(1).unwrap()).unwrap();
    let total: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    let mut holders = [[0; 3]; 3];
    for (n, a) in [&f.user, bob, escrow].iter().enumerate() {
        let h: RewardHolder = call(e, &f.share, "reward_account", ((*a).clone(),).into_val(e));
        holders[n] = [h.debt, h.pending, h.fraction];
    }
    assert_eq!(holders[2][1], 0, "escrow must not own beneficiary rewards");
    let scale = 1_000_000_000_000_000_000u128;
    let credits: u128 = holders.iter().map(|h|h[1]).sum();
    let fractions: u128 = holders.iter().map(|h|h[2]).sum();
    assert_eq!(total.received * scale,
        (credits + total.paid + total.quarantine) * scale + fractions + total.rounding_scaled,
        "exact scaled conservation at {label}");
    assert_eq!(token::Client::new(e, &f.reward).balance(&f.share) as u128, total.received - total.paid);
    let growth0: U256 = call(e, &f.pool, "get_fee_growth_global_0_x128", ().into_val(e));
    let growth1: U256 = call(e, &f.pool, "get_fee_growth_global_1_x128", ().into_val(e));
    let reading = Reading {
        label,
        supply: call(e, &f.share, "total_supply", ().into_val(e)),
        position: position.to_xdr(e).iter().collect::<std::vec::Vec<u8>>(),
        fee_growth0: growth0.to_xdr(e).iter().collect::<std::vec::Vec<u8>>(),
        fee_growth1: growth1.to_xdr(e).iter().collect::<std::vec::Vec<u8>>(),
        fees: fees.iter().collect(), withdrawal_quote: principal.iter().collect(), idle,
        fee_inclusive_nav,
        custody: call(e, &f.share, "custody_allocation", (escrow.clone(), f.user.clone()).into_val(e)),
        direct: [token::Client::new(e, &f.share).balance(&f.user), token::Client::new(e, &f.share).balance(bob)],
        rewards: [total.received, total.paid, total.quarantine, total.rounding_scaled], holders,
    };
    std::println!("SPLIT_FEE_BOUNDARY label={label} fee_inclusive_full_quote_plus_idle={} fee_view={:?} received={} quarantine={} fractions={} rounding={}",
        reading.fee_inclusive_nav, reading.fees, total.received, total.quarantine, fractions, total.rounding_scaled);
    reading
}

fn branch(recollect: bool, cross_range: bool) -> (u128, std::vec::Vec<Reading>, std::vec::Vec<u128>) {
    let (f, escrow) = setup();
    let e = &f.e;
    let bob = Address::generate(e);
    let direct = token::Client::new(e, &f.share).balance(&f.user);
    let bob_shares = direct / 3;
    auth::<()>(e, &f.user, &f.share, "transfer", (f.user.clone(), bob.clone(), bob_shares).into_val(e), &[]);
    auth::<()>(e, &f.user, &f.reward, "transfer", (f.user.clone(), f.pool.clone(), 100_000_000i128).into_val(e), &[]);
    auth::<()>(e, &f.admin, &f.pool, "set_rewards_config",
        (f.admin.clone(), 10_000u64, 10_003u128).into_val(e), &[]);
    let id = begin(&f);
    fund(&f, id);
    prepare(&f, id);
    let first_fence: BytesN<32> = call(e, &f.share, "split_fence", ().into_val(e));
    let before = record(&f, &escrow, &bob, "after_first_fee_collection");
    assert_eq!(before.fees, std::vec![0, 0]);
    // Unlike stage(), auth() does not advance either timestamp or ledger. The
    // second real fee collection is the ONLY difference between these branches.
    if recollect {
        let again: BytesN<32> = auth(e, &f.user, &f.margin, "split_prepare",
            (f.user.clone(), id).into_val(e), &[]);
        assert_eq!(again, first_fence, "zero recollection changed exact fee/position fence");
        measure(e, "split_fee_second_zero_collection", true);
    }
    let after = record(&f, &escrow, &bob, "before_mint_after_optional_zero_collection");
    let minted = mint(&f, id);
    let mut records = std::vec![before, after, record(&f, &escrow, &bob, "after_split_mint")];
    for (n, reverse) in [(10_003u128, false), (10_007, true), (1_000_003, false), (999_983, true)] {
        advance_fee(e, 7);
        fee_trade(&f, n, reverse);
    }
    assert!(call::<bool>(e, &f.share, "checkpoint_rewards", ().into_val(e)));
    records.push(record(&f, &escrow, &bob, "future_rounding_fee_accrual"));
    assert!(records.last().unwrap().fees.iter().copied().sum::<u128>() > 0,
        "future trades must produce actual outstanding fees, not only zero views");
    auth::<()>(e, &f.admin, &f.pool, "set_rewards_config",
        (f.admin.clone(), 10_001u64, 90_007u128).into_val(e), &[]);
    if cross_range {
        fee_trade(&f, 5_000_000_000, false);
        let shifted: FeeSlot = call(e, &f.pool, "get_slot0", ().into_val(e));
        let ticks: (i32, i32) = call(e, &f.share, "get_ticks", ().into_val(e));
        assert!(shifted.tick < ticks.0 || shifted.tick >= ticks.1, "must execute actual range exit");
        advance_fee(e, 17);
        assert!(call::<bool>(e, &f.share, "checkpoint_rewards", ().into_val(e)));
        records.push(record(&f, &escrow, &bob, "outside_range_after_emission_change"));
        fee_trade(&f, 4_900_000_000, true);
        let back: FeeSlot = call(e, &f.pool, "get_slot0", ().into_val(e));
        assert!(back.tick >= ticks.0 && back.tick < ticks.1, "must execute actual range reentry");
    }
    advance_fee(e, 11);
    let moved = bob_shares / 13;
    auth::<()>(e, &bob, &f.share, "transfer", (bob.clone(), f.user.clone(), moved).into_val(e), &[]);
    records.push(record(&f, &escrow, &bob, "claim_first_ownership_change"));
    let partial: Vec<u128> = auth(e, &bob, &f.share, "emergency_redeem",
        (bob.clone(), (bob_shares / 7) as u128, false).into_val(e), &[]);
    assert!(partial.get(0).unwrap() + partial.get(1).unwrap() > 0);
    measure(e, "split_fee_future_partial_exit_actual_receipt", true);
    records.push(record(&f, &escrow, &bob, "after_partial_exit_and_actual_fee_collection"));
    assert_eq!(records.last().unwrap().fees, std::vec![0, 0],
        "partial exit must actually collect the outstanding fee balance");
    advance_fee(e, 23);
    fee_trade(&f, 3_000_017, false);
    assert!(call::<bool>(e, &f.share, "checkpoint_rewards", ().into_val(e)));
    records.push(record(&f, &escrow, &bob, "later_accrual_after_partial_exit"));
    (minted, records, partial.iter().collect())
}

#[test]
fn compiled_split_fee_zero_recollection_future_rounding_partial_exit() {
    let skipped = branch(false, false);
    let repeated = branch(true, false);
    assert_eq!(skipped, repeated, "second zero collection differs in current/future fees, minted value, actual partial payout or reward ownership");
    std::println!("SPLIT_FEE_DIFFERENTIAL repeated_zero_collection_equivalent=true future_rounding=true partial_exit=true exact_reward_conservation=true range_crossing=false");
}

#[test]
fn compiled_split_fee_zero_recollection_range_emission_matrix() {
    let skipped = branch(false, true);
    let repeated = branch(true, true);
    assert_eq!(skipped, repeated, "second zero collection differs through actual range/emission/partial-exit boundaries");
    std::println!("SPLIT_FEE_DIFFERENTIAL repeated_zero_collection_equivalent=true future_rounding=true partial_exit=true exact_reward_conservation=true range_crossing=true emission_changes=true");
}
