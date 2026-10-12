//! Lean-only real-SAC/native Aquarius tests, not deployed pool-WASM evidence.
use super::*;
use coordinator::Outcome;

fn completed(outcome: Outcome) -> u128 {
    match outcome {
        Outcome::Completed(value) => value,
        other => panic!("expected completion, got {other:?}"),
    }
}
fn state(f: &Fixture) -> backing::BackingState {
    f.env.as_contract(&f.receipt, || {
        coordinator::check(&f.env);
        backing::state(&f.env)
    })
}
fn recycled(f: &Fixture, asset: &Address) -> u128 {
    f.env
        .as_contract(&f.receipt, || coordinator::recycled(&f.env, asset))
}
fn stream(f: &Fixture, asset: &Address) -> ledger::Stream {
    f.env
        .as_contract(&f.receipt, || ledger::stream(&f.env, asset))
}
fn reset(f: &Fixture) {
    // Reset only cumulative host accounting. Transaction limits remain enabled.
    f.env.cost_estimate().budget().reset_unlimited();
}
pub(super) fn second_token(f: &Fixture) -> (Address, Address) {
    let reward = f
        .env
        .register_stellar_asset_contract_v2(f.admin.clone())
        .address();
    let route = f.env.register(MockAquariusPool, ());
    let pool = MockAquariusPoolClient::new(&f.env, &route);
    pool.initialize(&reward, &f.asset, &60, &0);
    for asset in [&reward, &f.asset] {
        token::StellarAssetClient::new(&f.env, asset).mint(&f.user, &1_000_000_000_000);
    }
    f.env.mock_all_auths_allowing_non_root_auth();
    pool.deposit_position(
        &f.user,
        &-887220,
        &887220,
        &soroban_sdk::vec![&f.env, 1_000_000_000_000u128, 1_000_000_000_000u128],
        &0,
    );
    f.env.mock_all_auths();
    let strategy = AquariusLpVaultClient::new(&f.env, &f.strategy);
    strategy.set_reward_route(&f.admin, &reward, &Some(route.clone()));
    strategy.set_reward_min_rate(&f.admin, &reward, &9_000_000);
    f.receipt().co_register(&reward);
    (reward, route)
}
fn credit_two(f: &Fixture, second: &Address, primary: u128, gauge: u128) {
    let pool = MockAquariusPoolClient::new(&f.env, &f.pool);
    pool.set_reward_tokens(&f.reward, second);
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.pool, &(primary as i128));
    token::StellarAssetClient::new(&f.env, second).mint(&f.pool, &(gauge as i128));
    pool.credit_rewards(&f.strategy, &primary, &gauge);
}

#[test]
fn coordinator_recycled_yield_enriches_old_units_before_later_claims() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    f.credit(100_000, 0);
    reset(&f);
    let initial_units = completed(f.receipt().co_compound(&f.reward));
    let old = state(&f);
    assert_eq!(old.units, initial_units);
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_000);
    f.credit(110_000, 0);
    reset(&f);
    f.receipt().co_deposit(&late, &1_000_000);
    let recycle = recycled(&f, &f.reward);
    assert_eq!(
        recycle,
        110_000 - 110_000 * 1_000_000 / (1_000_000 + old.ptokens)
    );
    assert_eq!(state(&f).pending_rewards, 1);
    assert_eq!(f.receipt().earned(&f.reward, &late).raw_scaled, 0);
    assert_eq!(f.receipt().earned(&f.reward, &late).units_scaled, 0);
    // Transfer ordinary principal: historical units/rewards stay with old owner.
    reset(&f);
    f.receipt().co_transfer(&f.user, &late, &1_000_000);
    f.credit(220_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    let after = state(&f);
    assert_eq!(after.pending_rewards, 0);
    assert_eq!(recycled(&f, &f.reward), 0);
    // Unit price increased; new issuance cannot capture escrow's earlier yield.
    assert!(after.ptokens * old.units > old.ptokens * after.units);
    let old_earned = f.receipt().earned(&f.reward, &f.user).units_scaled / ledger::SCALE;
    let late_earned = f.receipt().earned(&f.reward, &late).units_scaled / ledger::SCALE;
    assert!(old_earned > initial_units);
    assert!(late_earned > 0);
    let old_value = old_earned * after.ptokens / after.units;
    assert!(old_value > 200_000);
    reset(&f);
    let paid = completed(f.receipt().co_redeem(&f.user, &1));
    assert!(paid >= old_value.saturating_sub(2));
    assert_eq!(f.receipt().balance(&f.user), 0);
    assert_eq!(f.receipt().co_redeem(&f.user, &1), Outcome::Nothing);
    state(&f);
}

#[test]
fn coordinator_other_stream_failure_blocks_unit_changes_but_not_principal_exit() {
    let f = Fixture::new();
    f.receipt().co_register(&f.reward);
    let (second, route) = second_token(&f);
    f.credit(100_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    let initial = state(&f);
    credit_two(&f, &second, 110_000, 220_000);
    MockAquariusPoolClient::new(&f.env, &route).set_kill_swap(&true);
    reset(&f);
    assert_eq!(f.receipt().co_compound(&f.reward), Outcome::Deferred);
    let partial = state(&f);
    assert_eq!(partial.units, initial.units);
    assert!(partial.ptokens > initial.ptokens); // first stream settled successfully
    assert_eq!(recycled(&f, &f.reward), 0);
    assert!(recycled(&f, &second) > 0);
    assert_eq!(partial.pending_rewards, 1);
    let raw = stream(&f, &second);
    reset(&f);
    assert_eq!(f.receipt().co_redeem(&f.user, &1), Outcome::Deferred);
    assert_eq!(state(&f), partial);
    let before = f.cash(&f.user);
    reset(&f);
    let reservations = f.receipt().co_withdraw(&f.user, &1_000_000);
    let r = f.env.cost_estimate().resources();
    std::println!(
        "two-stream strategy principal exit: {} entries / {} instructions",
        r.memory_read_entries + r.write_entries,
        r.instructions
    );
    assert!(r.memory_read_entries + r.write_entries <= 100);
    assert!(f.cash(&f.user) > before);
    assert_eq!(f.receipt().balance(&f.user), 0);
    assert_eq!(reservations.get(second.clone()), Some(raw.raw));
    assert_eq!(state(&f).pending_rewards, 1);
    reset(&f);
    assert_eq!(
        f.receipt().co_settle(&second, &f.user, &raw.raw, &1),
        Outcome::Deferred
    );
    assert_eq!(stream(&f, &second).reserved, raw.raw);
    MockAquariusPoolClient::new(&f.env, &route).set_kill_swap(&false);
    reset(&f);
    completed(f.receipt().co_settle(&second, &f.user, &raw.raw, &1));
    assert_eq!(stream(&f, &second).reserved, 0);
    // Preparing the recycle separately keeps the subsequent pool-unwind payout
    // below the SDK fixture's conservative 100-entry ceiling. With a new emission
    // between calls the payout MUST claim it again, not skip this requirement.
    reset(&f);
    completed(f.receipt().co_recycle(&second));
    assert_eq!(state(&f).units, initial.units);
    reset(&f);
    completed(f.receipt().co_redeem(&f.user, &1));
    let r = f.env.cost_estimate().resources();
    std::println!(
        "two-stream staged reward payout: {} entries / {} instructions",
        r.memory_read_entries + r.write_entries,
        r.instructions
    );
    assert!(r.memory_read_entries + r.write_entries <= 100);
    assert_eq!(state(&f).pending_rewards, 0);
    assert_eq!(recycled(&f, &second), 0);
}

#[test]
fn coordinator_permissionless_conversion_and_exact_owner_redemption_auth() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    f.credit(100_000, 0);
    f.env.mock_auths(&[]);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward)); // explicit contract auth only
    f.env.mock_all_auths();
    f.credit(110_000, 0);
    let before = state(&f);
    let raw_before = f.raw(&f.pool);
    f.env.mock_auths(&[]);
    reset(&f);
    assert!(f.receipt().try_co_redeem(&f.user, &1).is_err());
    assert_eq!(state(&f), before);
    assert_eq!(f.raw(&f.pool), raw_before); // even the successful claim/swap rolls back
    reset(&f);
    let paid = f
        .receipt()
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "co_redeem",
                args: (f.user.clone(), 1u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .co_redeem(&f.user, &1);
    assert!(completed(paid) > 100_000);
    assert_eq!(state(&f).pending_rewards, 0);
    assert_eq!(f.receipt().balance(&f.user), 1_000_000);
}

#[test]
fn coordinator_failed_payout_minimum_rolls_back_recycled_settlement_too() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    f.credit(100_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    f.credit(110_000, 0);
    let before = state(&f);
    let wallet = f.cash(&f.user);
    let pool_raw = f.raw(&f.pool);
    reset(&f);
    assert!(f.receipt().try_co_redeem(&f.user, &1_000_000).is_err());
    assert_eq!(state(&f), before);
    assert_eq!(f.cash(&f.user), wallet);
    assert_eq!(f.raw(&f.pool), pool_raw);
    assert_eq!(stream(&f, &f.reward).raw, 0);
    assert_eq!(recycled(&f, &f.reward), 0);
}

#[test]
fn coordinator_four_retained_streams_settle_without_resetting_history() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    let (b, _) = second_token(&f);
    let (c, _) = second_token(&f);
    let (d, _) = second_token(&f);
    f.credit(100_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    let old = state(&f);
    for asset in [&b, &c, &d] {
        credit_two(&f, asset, 100_000, 100_000);
        reset(&f);
        f.receipt().co_claim();
    }
    assert_eq!(state(&f).pending_rewards, 4);
    for asset in [&f.reward, &b, &c, &d] {
        reset(&f);
        completed(f.receipt().co_recycle(asset));
        let r = f.env.cost_estimate().resources();
        std::println!(
            "four-registry single-stream recycle: {} entries / {} instructions",
            r.memory_read_entries + r.write_entries,
            r.instructions
        );
        assert!(r.memory_read_entries + r.write_entries <= 100);
        assert_eq!(state(&f).units, old.units);
    }
    assert_eq!(state(&f).pending_rewards, 0);
    for asset in [&f.reward, &b, &c, &d] {
        reset(&f);
        completed(f.receipt().co_compound(asset));
        assert_eq!(
            stream(&f, asset).epoch,
            if asset == &f.reward { 2 } else { 1 }
        );
        assert_eq!(stream(&f, asset).raw, 0);
        assert_eq!(recycled(&f, asset), 0);
    }
    state(&f);
}

#[test]
fn coordinator_recycled_storage_ttl_renews_and_missing_registry_fails_closed() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    f.env.ledger().set_sequence_number(600_001);
    // No timestamp advance: this is only a persistent-storage TTL regression.
    reset(&f);
    f.receipt().co_claim();
    f.env.as_contract(&f.receipt, || {
        let key = coordinator::CoordinatorKey::RecycledRaw(f.reward.clone());
        assert!(f.env.storage().persistent().get_ttl(&key) > 500_000);
        f.env
            .storage()
            .persistent()
            .remove(&ledger::LedgerKey::HybridStreams);
    });
    assert!(f.receipt().try_co_claim().is_err());
    assert_eq!(f.receipt().balance(&f.user), 1_000_000);
    let new = f
        .env
        .register_stellar_asset_contract_v2(f.admin.clone())
        .address();
    assert!(f.receipt().try_co_register(&new).is_err());
}

#[test]
fn coordinator_preparation_does_not_skip_fresh_emissions_on_later_redemption() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    f.credit(100_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    f.credit(110_000, 0);
    reset(&f);
    completed(f.receipt().co_recycle(&f.reward));
    assert_eq!(state(&f).pending_rewards, 0);
    let old = state(&f);
    f.credit(110_000, 0); // arriving after preparation, before the payout
    MockAquariusPoolClient::new(&f.env, &f.route).set_kill_swap(&true);
    reset(&f);
    assert_eq!(f.receipt().co_redeem(&f.user, &1), Outcome::Deferred);
    assert_eq!(state(&f).units, old.units);
    assert_eq!(state(&f).pending_rewards, 1);
    assert!(recycled(&f, &f.reward) > 0);
}

#[test]
fn coordinator_atomic_four_route_budget_diagnostic_is_not_network_fit_evidence() {
    let f = Fixture::new();
    f.receipt().co_register(&f.reward);
    let (b, _) = second_token(&f);
    let (c, _) = second_token(&f);
    let (d, _) = second_token(&f);
    f.credit(100_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    for asset in [&b, &c, &d] {
        credit_two(&f, asset, 100_000, 100_000);
        reset(&f);
        f.receipt().co_claim();
    }
    assert_eq!(state(&f).pending_rewards, 4);
    // ONLY this diagnostic disables the SDK fixture limits. It measures the
    // unresolved combined shape, not a successful network-executable lifecycle.
    f.env.cost_estimate().disable_resource_limits();
    reset(&f);
    completed(f.receipt().co_redeem(&f.user, &1));
    let r = f.env.cost_estimate().resources();
    let entries = r.memory_read_entries + r.write_entries;
    std::println!(
        "BLOCKED atomic four-route reward payout: {entries}/100 entries / {} instructions",
        r.instructions
    );
    assert!(
        entries > 100,
        "budget improved: replace diagnostic with normal-limit regression"
    );
    state(&f);
}

#[test]
fn coordinator_full_exit_reserves_both_tokens_without_gifting_them_on_reentry() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    let (second, _) = second_token(&f);
    credit_two(&f, &second, 100_000, 200_000);
    reset(&f);
    f.receipt().co_withdraw(&f.user, &1_000_000);
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_000);
    reset(&f);
    f.receipt().co_deposit(&late, &1_000_000);
    for asset in [&f.reward, &second] {
        let late_account = f.receipt().earned(asset, &late);
        assert_eq!(late_account.raw_scaled, 0);
        assert_eq!(late_account.reserved, 0);
        assert_eq!(late_account.units_scaled, 0);
    }
    credit_two(&f, &second, 30_000, 40_000);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    assert_eq!(stream(&f, &f.reward).reserved, 100_000);
    assert_eq!(stream(&f, &second).reserved, 200_000);
    reset(&f);
    completed(f.receipt().co_settle(&f.reward, &f.user, &100_000, &1));
    assert_eq!(stream(&f, &f.reward).reserved, 0);
    assert!(f
        .receipt()
        .try_co_settle(&f.reward, &f.user, &1, &1)
        .is_err());
    assert_eq!(f.receipt().earned(&f.reward, &f.user).units_scaled, 0);
    assert!(f.receipt().earned(&f.reward, &late).units_scaled > 0);
    state(&f);
}

#[test]
fn coordinator_second_settlement_leg_recycles_and_keeps_partial_exit_claims() {
    let f = Fixture::with_settlement_index(0, 1);
    f.receipt().co_register(&f.reward);
    f.credit(100_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    reset(&f);
    f.receipt().reinvest(&f.receipt_admin);
    f.credit(110_000, 0);
    reset(&f);
    let reserved = f
        .receipt()
        .co_withdraw(&f.user, &500_000)
        .get(f.reward.clone())
        .unwrap();
    assert!(reserved > 0);
    assert!(recycled(&f, &f.reward) > 0);
    reset(&f);
    completed(f.receipt().co_recycle(&f.reward));
    reset(&f);
    completed(f.receipt().co_redeem(&f.user, &1));
    assert_eq!(f.receipt().balance(&f.user), 500_000);
    assert_eq!(stream(&f, &f.reward).reserved, reserved);
    reset(&f);
    completed(f.receipt().co_settle(&f.reward, &f.user, &reserved, &1));
    assert_eq!(stream(&f, &f.reward).reserved, 0);
    state(&f);
}

fn pool_owed(f: &Fixture, asset: &Address) -> u128 {
    f.env
        .as_contract(&f.receipt, || coordinator::owed(&f.env, asset))
}

#[test]
fn outage_exit_preserves_two_token_debt_and_excludes_later_depositors() {
    let f = Fixture::new();
    f.receipt().co_register(&f.reward);
    let (second, _) = second_token(&f);
    credit_two(&f, &second, 100_000, 200_000);
    let p = MockAquariusPoolClient::new(&f.env, &f.pool);
    p.set_kill_claim(&true);
    reset(&f);
    let (reserved, paid) = f.receipt().co_exit(&f.user, &500_000, &400_000);
    assert!(paid >= 400_000);
    assert_eq!(reserved.get(f.reward.clone()), Some(50_000));
    assert_eq!(reserved.get(second.clone()), Some(100_000));
    assert_eq!(pool_owed(&f, &f.reward), 100_000);
    assert_eq!(pool_owed(&f, &second), 200_000);
    assert_eq!(f.raw(&f.receipt), 0);
    reset(&f);
    assert_eq!(
        f.receipt().co_settle(&f.reward, &f.user, &50_000, &1),
        Outcome::Deferred
    );
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_000);
    reset(&f);
    f.receipt().co_deposit(&late, &1_000_000);
    assert_eq!(f.receipt().earned(&f.reward, &late).raw_scaled, 0);
    assert_eq!(f.receipt().earned(&second, &late).raw_scaled, 0);
    // Repeating a failed claim at unchanged pool debt cannot index it twice.
    reset(&f);
    f.receipt().co_claim();
    assert_eq!(stream(&f, &f.reward).raw, 50_000);
    p.set_kill_claim(&false);
    reset(&f);
    completed(f.receipt().co_settle(&f.reward, &f.user, &50_000, &1));
    assert_eq!(pool_owed(&f, &f.reward), 0);
    assert_eq!(pool_owed(&f, &second), 0);
    assert_eq!(f.raw(&f.receipt), 50_000);
    assert_eq!(f.receipt().earned(&second, &late).raw_scaled, 0);
    assert_eq!(stream(&f, &second).reserved, 100_000);
    state(&f);
}

#[test]
fn outage_future_rewards_use_remaining_weights_and_old_escrow_cannot_be_paid_unfunded() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    f.credit(100_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    f.credit(110_000, 0);
    let p = MockAquariusPoolClient::new(&f.env, &f.pool);
    p.set_kill_claim(&true);
    reset(&f);
    f.receipt().co_exit(&f.user, &1_000_000, &900_000);
    let old_backing = state(&f);
    assert!(recycled(&f, &f.reward) > 0);
    let reserved = stream(&f, &f.reward).reserved;
    let prior_recycled = recycled(&f, &f.reward);
    // Only escrow remains. Add50k newly accrued rewards to the existing debt.
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.pool, &50_000);
    p.credit_rewards(&f.strategy, &160_000, &0);
    reset(&f);
    assert_eq!(f.receipt().co_redeem(&f.user, &1), Outcome::Deferred);
    assert_eq!(state(&f).units, old_backing.units);
    assert_eq!(stream(&f, &f.reward).reserved, reserved);
    assert_eq!(recycled(&f, &f.reward), prior_recycled + 50_000);
    assert_eq!(pool_owed(&f, &f.reward), 160_000);
    p.set_kill_claim(&false);
    reset(&f);
    completed(f.receipt().co_redeem(&f.user, &1));
    assert_eq!(pool_owed(&f, &f.reward), 0);
    assert_eq!(stream(&f, &f.reward).reserved, reserved);
    reset(&f);
    completed(f.receipt().co_settle(&f.reward, &f.user, &reserved, &1));
    state(&f);
}

#[test]
fn outage_claim_debt_cannot_shrink_or_disappear_without_actual_collection() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    f.credit(100_000, 0);
    let p = MockAquariusPoolClient::new(&f.env, &f.pool);
    p.set_kill_claim(&true);
    reset(&f);
    f.receipt().co_claim();
    p.credit_rewards(&f.strategy, &90_000, &0);
    reset(&f);
    assert!(f.receipt().try_co_exit(&f.user, &1_000_000, &1).is_err());
    assert_eq!(f.receipt().balance(&f.user), 1_000_000);
    assert_eq!(pool_owed(&f, &f.reward), 100_000);
    p.set_kill_claim(&false);
    assert!(f.receipt().try_co_claim().is_err()); // short real collection rolls back
    assert_eq!(f.raw(&f.receipt), 0);
    assert_eq!(p.get_user_reward(&f.strategy), 90_000);
}

#[test]
fn outage_stale_oracle_exit_needs_no_receipt_nav_quote_and_keeps_minimum() {
    for index in [0, 1] {
        let f = Fixture::with_settlement_index(0, index);
        f.receipt().co_register(&f.reward);
        let s = AquariusLpVaultClient::new(&f.env, &f.strategy);
        s.set_nav_root_max_stale(&f.admin, &301);
        MockOracleClient::new(&f.env, &f.oracle).set_stale(&true);
        f.env.ledger().set_timestamp(400);
        f.credit(100_000, 0);
        MockAquariusPoolClient::new(&f.env, &f.pool).set_kill_claim(&true);
        reset(&f);
        assert!(f
            .receipt()
            .try_co_exit(&f.user, &1_000_000, &2_000_000)
            .is_err());
        assert_eq!(f.receipt().balance(&f.user), 1_000_000);
        assert_eq!(pool_owed(&f, &f.reward), 0); // even debt indexing was rolled back
        reset(&f);
        let (reserved, paid) = f.receipt().co_exit(&f.user, &1_000_000, &900_000);
        assert!(paid >= 900_000);
        assert_eq!(reserved.get(f.reward.clone()), Some(100_000));
        let resources = f.env.cost_estimate().resources();
        std::println!(
            "outage exit leg{index}: {} entries / {} instructions",
            resources.memory_read_entries + resources.write_entries,
            resources.instructions
        );
        assert!(resources.memory_read_entries + resources.write_entries <= 100);
        assert_eq!(f.receipt().balance(&f.user), 0);
        assert_eq!(s.balance(&f.receipt), 0);
    }
}

#[test]
fn outage_proportional_minimum_cannot_drain_another_holders_idle_cash() {
    let f = Fixture::with_buffer(5000);
    f.receipt().co_register(&f.reward);
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_000);
    reset(&f);
    f.receipt().co_deposit(&late, &1_000_000);
    let shares = f.receipt().balance(&late) as u128;
    let old_shares = f.receipt().balance(&f.user);
    let cash_before = f.cash(&f.receipt);
    reset(&f);
    assert!(f.receipt().try_co_exit(&late, &shares, &1_500_000).is_err());
    assert_eq!(f.cash(&f.receipt), cash_before);
    assert_eq!(f.receipt().balance(&f.user), old_shares);
    assert_eq!(f.receipt().balance(&late), shares as i128);
    reset(&f);
    let (_, paid) = f.receipt().co_exit(&late, &shares, &800_000);
    assert!(paid < 1_100_000);
    reset(&f);
    assert!(
        f.receipt()
            .co_exit(&f.user, &(old_shares as u128), &800_000)
            .1
            >= 800_000
    );
}

#[test]
fn outage_exit_and_later_reserved_payout_require_exact_owner_authorization() {
    let f = Fixture::new();
    f.receipt().co_register(&f.reward);
    f.credit(100_000, 0);
    MockAquariusPoolClient::new(&f.env, &f.pool).set_kill_claim(&true);
    f.env.mock_auths(&[]);
    reset(&f);
    assert!(f
        .receipt()
        .try_co_exit(&f.user, &1_000_000, &900_000)
        .is_err());
    assert_eq!(pool_owed(&f, &f.reward), 0);
    reset(&f);
    f.receipt()
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "co_exit",
                args: (f.user.clone(), 1_000_000u128, 900_000u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .co_exit(&f.user, &1_000_000, &900_000);
    assert_eq!(pool_owed(&f, &f.reward), 100_000);
    f.env.mock_auths(&[]);
    reset(&f);
    assert!(f
        .receipt()
        .try_co_settle(&f.reward, &f.user, &100_000, &1)
        .is_err());
    assert_eq!(stream(&f, &f.reward).reserved, 100_000);
}

#[test]
fn coordinator_all_escrow_supply_recycles_without_issuing_units() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    f.credit(100_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    reset(&f);
    f.receipt().co_withdraw(&f.user, &1_000_000);
    let before = state(&f);
    f.credit(100_000, 0);
    reset(&f);
    assert_eq!(f.receipt().co_compound(&f.reward), Outcome::Nothing);
    let after = state(&f);
    assert_eq!(before.units, after.units);
    assert!(after.ptokens > before.ptokens);
    assert_eq!(stream(&f, &f.reward).raw, 0);
    reset(&f);
    assert!(completed(f.receipt().co_redeem(&f.user, &1)) > 190_000);
}

#[test]
fn coordinator_retired_tokens_keep_claims_and_unregistered_emissions_roll_back() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    f.credit(100_000, 0);
    f.receipt().co_claim();
    let new = f
        .env
        .register_stellar_asset_contract_v2(f.admin.clone())
        .address();
    credit_two(&f, &new, 10_000, 20_000);
    reset(&f);
    assert!(f.receipt().try_co_claim().is_err());
    assert_eq!(stream(&f, &f.reward).raw, 100_000);
    assert_eq!(f.raw(&f.receipt), 100_000);
    assert_eq!(token::Client::new(&f.env, &new).balance(&f.receipt), 0);
    f.receipt().co_register(&new);
    reset(&f);
    f.receipt().co_claim();
    assert_eq!(stream(&f, &f.reward).raw, 110_000);
    assert_eq!(stream(&f, &new).raw, 20_000);
    let route = f.env.register(MockAquariusPool, ());
    MockAquariusPoolClient::new(&f.env, &route).initialize(&new, &f.asset, &60, &0);
    let strategy = AquariusLpVaultClient::new(&f.env, &f.strategy);
    strategy.set_reward_route(&f.admin, &new, &Some(route));
    strategy.set_reward_min_rate(&f.admin, &new, &9_000_000);
    reset(&f);
    f.receipt().co_rotate(&new, &1);
    MockAquariusPoolClient::new(&f.env, &f.pool).set_reward_tokens(&new, &new);
    let receiver = Address::generate(&f.env);
    reset(&f);
    f.receipt().co_transfer(&f.user, &receiver, &500_000);
    assert_eq!(
        f.receipt().earned(&f.reward, &f.user).raw_scaled,
        110_000 * ledger::SCALE
    );
    assert_eq!(f.receipt().earned(&f.reward, &receiver).raw_scaled, 0);
    assert_eq!(f.receipt().earned(&new, &receiver).raw_scaled, 0);
    assert!(f.receipt().try_co_register(&f.reward).is_err());
}

#[path = "reward_lifecycle_test.rs"]
mod reward_lifecycle_test;

#[test]
fn coordinator_donations_missing_history_and_registry_cap_fail_safely() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    let donation = 77_777;
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.receipt, &donation);
    f.credit(100_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    assert_eq!(f.raw(&f.receipt), donation);
    f.env.as_contract(&f.receipt, || {
        f.env
            .storage()
            .persistent()
            .remove(&coordinator::CoordinatorKey::RecycledRaw(f.reward.clone()));
    });
    f.credit(10_000, 0);
    assert!(f.receipt().try_co_claim().is_err());
    assert!(f.receipt().try_co_register(&f.reward).is_err());
    assert_eq!(f.raw(&f.receipt), donation);

    let clean = Fixture::with_buffer(10_000);
    clean.receipt().co_register(&clean.reward);
    for _ in 0..3 {
        second_token(&clean);
    }
    let fifth = clean
        .env
        .register_stellar_asset_contract_v2(clean.admin.clone())
        .address();
    assert!(clean.receipt().try_co_register(&fifth).is_err());
    clean.env.as_contract(&clean.receipt, || {
        assert_eq!(ledger::assets(&clean.env).len(), 4);
        coordinator::check(&clean.env);
    });
}

#[test]
fn coordinator_unobservable_claim_failure_keeps_principal_and_old_liabilities_unchanged() {
    let f = Fixture::new();
    f.receipt().co_register(&f.reward);
    // Deliberately unfunded claim: the real SAC transfer fails in the pool.
    MockAquariusPoolClient::new(&f.env, &f.pool).credit_rewards(&f.strategy, &100_000, &0);
    MockAquariusPoolClient::new(&f.env, &f.pool).set_fail_reward_quote(&true);
    reset(&f);
    assert!(f.receipt().try_co_withdraw(&f.user, &1_000_000).is_err());
    assert_eq!(f.receipt().balance(&f.user), 1_000_000);
    assert_eq!(f.raw(&f.receipt), 0);
    assert_eq!(stream(&f, &f.reward).raw, 0);
    // Explicit remaining liveness blocker, not a claimed outage-safe exit.
}
