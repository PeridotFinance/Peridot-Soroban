//! Lending-backed reward settlement, native contracts and real SAC cash, mock
//! pools/oracle. Keeps transaction limits enabled under the parent's envelope.
use super::*;
use receipt_vault::reward_settlement::Outcome;

impl Fixture {
    fn routes(&self, i: usize) -> [Address; 2] {
        self.env.mock_all_auths_allowing_non_root_auth();
        let ids = core::array::from_fn(|j| {
            let id = self.env.register(MockAquariusPool, ());
            let p = MockAquariusPoolClient::new(&self.env, &id);
            p.initialize(&self.rewards[j], &self.assets[i], &20, &0);
            for a in [&self.rewards[j], &self.assets[i]] {
                token::StellarAssetClient::new(&self.env, a)
                    .mint(&self.supplier, &1_000_000_000_000);
            }
            p.deposit_position(
                &self.supplier,
                &-887220,
                &887220,
                &vec![&self.env, 1_000_000_000_000u128, 1_000_000_000_000u128],
                &0,
            );
            let s = AquariusLpVaultClient::new(&self.env, &self.strategies[i]);
            s.set_reward_route(&self.admin, &self.rewards[j], &Some(id.clone()));
            s.set_reward_min_rate(&self.admin, &self.rewards[j], &9_000_000);
            id
        });
        self.env.mock_all_auths();
        ids
    }
    fn backing(&self, i: usize) -> backing::BackingState {
        self.env
            .as_contract(&self.markets[i], || backing::state(&self.env))
    }
    fn recycled(&self, i: usize, j: usize) -> u128 {
        self.env.as_contract(&self.markets[i], || {
            claims::recycled(&self.env, &self.rewards[j])
        })
    }
    fn managed(&self, i: usize) -> u128 {
        self.env.as_contract(&self.markets[i], || {
            self.env
                .storage()
                .persistent()
                .get(&receipt_vault::DataKey::ManagedCash)
                .unwrap()
        })
    }
    fn untracked(&self, i: usize) -> u128 {
        (self.cash(i, &self.markets[i]) as u128)
            .checked_sub(self.managed(i))
            .unwrap()
    }
}

fn completed(outcome: Outcome) -> u128 {
    let Outcome::Completed(value) = outcome else {
        panic!("expected successful settlement");
    };
    assert!(value > 0);
    value
}

#[test]
fn aquarius_lending_reward_payout_keeps_loans_and_donations_separate_both_legs() {
    for i in [1, 2] {
        let f = Fixture::new();
        f.routes(i);
        f.credit(i, 100_000, 200_000);
        for asset in &f.rewards {
            f.reset();
            completed(f.r(i).compound(asset));
            f.measure("loan-aware reward conversion");
        }
        let backing = f.backing(i);
        assert_eq!(f.r(i).get_total_ptokens(), 1_000_000 + backing.ptokens);
        assert_eq!(f.r(i).get_ptoken_balance(&f.supplier), 1_000_000);
        token::StellarAssetClient::new(&f.env, &f.assets[i]).mint(&f.markets[i], &50_000);
        f.reset();
        f.r(i).reinvest(&f.admin);
        assert_eq!(f.untracked(i), 50_000);
        let shares = f.strategy_shares(i);
        f.reset();
        f.r(i).borrow(&f.user, &200_000);
        assert!(f.strategy_shares(i) < shares);
        assert_eq!(f.untracked(i), 50_000);
        let wallet = f.cash(i, &f.supplier);
        let strategy = f.strategy_shares(i);
        f.reset();
        let paid = completed(f.r(i).payout(&f.supplier, &200_000));
        f.measure("strategy-funded reward payout with debt");
        assert_eq!(f.cash(i, &f.supplier) - wallet, paid as i128);
        assert!(f.strategy_shares(i) < strategy);
        assert_eq!(f.r(i).get_total_borrowed(), 200_000);
        assert_eq!(f.r(i).get_ptoken_balance(&f.supplier), 1_000_000);
        assert_eq!(f.r(i).get_total_ptokens(), 1_000_000);
        assert_eq!(f.backing(i).units, 0);
        assert_eq!(f.untracked(i), 50_000);
        f.reset();
        f.r(i).repay(&f.user, &200_000);
        f.reset();
        f.r(i).withdraw(&f.supplier, &1_000_000, &900_000);
        assert_eq!(f.strategy_shares(i), 0);
        assert_eq!(f.r(i).get_total_ptokens(), 0);
        assert_eq!(f.cash(i, &f.markets[i]), 50_000);
    }
}

#[test]
fn aquarius_lending_pending_recycled_stream_blocks_payout_not_principal_or_repay() {
    let f = Fixture::new();
    let routes = f.routes(1);
    f.credit(1, 100_000, 0);
    f.reset();
    completed(f.r(1).compound(&f.rewards[0]));
    f.reset();
    f.r(1).borrow(&f.user, &200_000);
    let units = f.backing(1).units;
    f.credit(1, 100_000, 200_000);
    f.reset();
    f.r(1).claim();
    assert_eq!(f.backing(1).pending_rewards, 2);
    assert!(f.recycled(1, 0) > 0 && f.recycled(1, 1) > 0);
    // Staged conversion preserves old units and cannot issue fresh units.
    f.reset();
    completed(f.r(1).recycle(&f.rewards[0]));
    assert_eq!(f.backing(1).units, units);
    assert_eq!(f.recycled(1, 0), 0);
    MockAquariusPoolClient::new(&f.env, &routes[1]).set_kill_swap(&true);
    let before = f.backing(1);
    let wallet = f.cash(1, &f.supplier);
    f.reset();
    assert_eq!(f.r(1).payout(&f.supplier, &1), Outcome::Deferred);
    assert_eq!(f.backing(1), before);
    assert_eq!(f.cash(1, &f.supplier), wallet);
    f.reset();
    f.r(1).withdraw(&f.supplier, &100_000, &80_000);
    assert_eq!(f.backing(1), before);
    f.reset();
    f.r(1).repay(&f.user, &200_000);
    MockAquariusPoolClient::new(&f.env, &routes[1]).set_kill_swap(&false);
    f.reset();
    completed(f.r(1).recycle(&f.rewards[1]));
    assert_eq!(f.backing(1).units, units);
    assert_eq!(f.backing(1).pending_rewards, 0);
    f.reset();
    completed(f.r(1).payout(&f.supplier, &1));
    f.measure("recycled reward payout after staged retry");
}

#[test]
fn aquarius_lending_earners_keep_converted_and_reserved_rewards_after_exit_and_reentry() {
    let f = Fixture::new();
    f.routes(2);
    f.credit(2, 100_000, 200_000);
    f.reset();
    completed(f.r(2).compound(&f.rewards[0]));
    f.reset();
    let (reserved, _) = f.r(2).withdraw(&f.supplier, &1_000_000, &900_000);
    assert_eq!(reserved.get(f.rewards[1].clone()), Some(200_000));
    let newcomer = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.assets[2]).mint(&newcomer, &1_000_000);
    f.reset();
    f.r(2).deposit(&newcomer, &500_000);
    f.reset();
    assert_eq!(f.r(2).payout(&newcomer, &1), Outcome::Nothing);
    let shares = f.r(2).get_ptoken_balance(&newcomer);
    f.reset();
    completed(f.r(2).payout(&f.supplier, &80_000));
    f.reset();
    completed(
        f.r(2)
            .settle(&f.rewards[1], &f.supplier, &200_000, &180_000),
    );
    assert_eq!(f.r(2).get_ptoken_balance(&newcomer), shares);
    assert_eq!(f.r(2).reserved(&f.rewards[1], &f.supplier), 0);
    assert_eq!(f.r(2).get_ptoken_balance(&f.supplier), 0);
}

#[test]
fn aquarius_lending_reward_payout_auth_and_minimum_failure_restore_backing() {
    let f = Fixture::new();
    f.routes(1);
    f.credit(1, 100_000, 0);
    f.reset();
    completed(f.r(1).compound(&f.rewards[0]));
    let b = f.backing(1);
    let supply = f.r(1).get_total_ptokens();
    f.reset();
    assert!(f.r(1).try_payout(&f.supplier, &1_000_000).is_err());
    assert_eq!(f.backing(1), b);
    assert_eq!(f.r(1).get_total_ptokens(), supply);
    f.env.mock_auths(&[]);
    f.reset();
    assert!(f.r(1).try_payout(&f.supplier, &1).is_err());
    assert_eq!(f.backing(1), b);
    f.env.mock_auths(&[MockAuth {
        address: &f.supplier,
        invoke: &MockAuthInvoke {
            contract: &f.markets[1],
            fn_name: "payout",
            args: (&f.supplier, 1u128).into_val(&f.env),
            sub_invokes: &[],
        },
    }]);
    f.reset();
    completed(f.r(1).payout(&f.supplier, &1));
    f.measure("exact owner reward payout");
}

#[test]
fn aquarius_lending_borrow_deposit_reinvest_and_exit_never_adopt_donated_cash() {
    let f = Fixture::new();
    token::StellarAssetClient::new(&f.env, &f.assets[1]).mint(&f.markets[1], &500_000);
    let nav = f.r(1).nav();
    let shares = f.strategy_shares(1);
    f.reset();
    f.r(1).reinvest(&f.admin);
    assert_eq!(f.strategy_shares(1), shares);
    assert_eq!(f.r(1).nav(), nav);
    f.reset();
    f.r(1).borrow(&f.user, &200_000);
    assert!(f.strategy_shares(1) < shares); // donation cannot fund the loan
    assert_eq!(f.untracked(1), 500_000);
    f.reset();
    f.r(1).repay(&f.user, &200_000);
    f.reset();
    f.r(1).deposit(&f.supplier, &200_000);
    assert_eq!(f.untracked(1), 500_000);
    f.reset();
    f.r(1).reinvest(&f.admin);
    assert_eq!(f.untracked(1), 500_000);
    let all = f.r(1).get_ptoken_balance(&f.supplier);
    f.reset();
    f.r(1).withdraw(&f.supplier, &all, &1_000_000);
    assert_eq!(f.cash(1, &f.markets[1]), 500_000);
    assert_eq!(f.r(1).nav(), 0);
}

#[test]
fn aquarius_lending_missing_or_unbacked_managed_cash_fails_closed() {
    for missing in [false, true] {
        let f = Fixture::new();
        f.env.as_contract(&f.markets[1], || {
            if missing {
                f.env
                    .storage()
                    .persistent()
                    .remove(&receipt_vault::DataKey::ManagedCash);
            } else {
                f.env
                    .storage()
                    .persistent()
                    .set(&receipt_vault::DataKey::ManagedCash, &1u128);
            }
        });
        f.reset();
        assert!(f.r(1).try_reinvest(&f.admin).is_err());
        f.reset();
        assert!(f.r(1).try_borrow(&f.user, &100_000).is_err());
        f.reset();
        assert!(f.r(1).try_withdraw(&f.supplier, &100_000, &1).is_err());
        f.reset();
        assert!(f.r(1).try_deposit(&f.supplier, &100_000).is_err());
        assert_eq!(f.r(1).get_total_borrowed(), 0);
    }
}

#[test]
fn aquarius_lending_new_backing_is_priced_against_nav_including_outstanding_debt() {
    let f = Fixture::new();
    f.routes(1);
    f.reset();
    f.r(1).borrow(&f.user, &200_000);
    f.credit(1, 100_000, 0);
    let nav = f.r(1).nav();
    let supply = f.r(1).get_total_ptokens();
    let cash = f.cash(1, &f.markets[1]);
    f.reset();
    completed(f.r(1).compound(&f.rewards[0]));
    f.measure("new backing while loan outstanding");
    let received = (f.cash(1, &f.markets[1]) - cash) as u128;
    assert_eq!(f.backing(1).ptokens, received * supply / nav);
    assert!(f.backing(1).ptokens < received * supply / (nav - 200_000));
    assert_eq!(f.r(1).get_total_borrowed(), 200_000);
    assert_eq!(f.r(1).nav(), nav + received);
}

#[test]
fn aquarius_lending_xlm_reward_payout_preserves_stable_debt_and_collateral_checks() {
    let f = Fixture::new();
    f.routes(0);
    f.credit(0, 100_000, 0);
    f.reset();
    completed(f.r(0).compound(&f.rewards[0]));
    f.reset();
    f.r(1).borrow(&f.user, &200_000);
    let original = f.backing(0);
    // Reward rights are not a bypass around the lending engine's health check.
    MockOracleClient::new(&f.env, &f.oracle).set_price(&f.assets[0], &10_000_000_000_000);
    SimplePeridottrollerClient::new(&f.env, &f.controller).cache_price(&f.assets[0]);
    f.reset();
    assert!(f.r(0).try_payout(&f.user, &1).is_err());
    assert_eq!(f.backing(0), original);
    assert_eq!(f.r(0).get_ptoken_balance(&f.user), 1_000_000);
    MockOracleClient::new(&f.env, &f.oracle).set_price(&f.assets[0], &100_000_000_000_000);
    SimplePeridottrollerClient::new(&f.env, &f.controller).cache_price(&f.assets[0]);
    let wallet = f.cash(0, &f.user);
    f.reset();
    let paid = completed(f.r(0).payout(&f.user, &80_000));
    f.measure("XLM reward payout with cross-market debt");
    assert_eq!(f.cash(0, &f.user) - wallet, paid as i128);
    assert_eq!(f.r(1).get_total_borrowed(), 200_000);
    assert_eq!(f.r(0).get_ptoken_balance(&f.user), 1_000_000);
}

#[test]
fn aquarius_lending_liquidation_does_not_transfer_historical_converted_rewards() {
    let f = Fixture::new();
    f.routes(0);
    f.credit(0, 100_000, 0);
    f.reset();
    completed(f.r(0).compound(&f.rewards[0]));
    let b = f.backing(0);
    f.reset();
    f.r(1).borrow(&f.user, &600_000);
    MockOracleClient::new(&f.env, &f.oracle).set_price(&f.assets[0], &50_000_000_000_000);
    let c = SimplePeridottrollerClient::new(&f.env, &f.controller);
    c.cache_price(&f.assets[0]);
    token::Client::new(&f.env, &f.assets[1]).approve(&f.supplier, &f.markets[1], &100_000, &1000);
    f.reset();
    c.liquidate(&f.user, &f.markets[1], &f.markets[0], &100_000, &f.supplier);
    assert_eq!(f.backing(0), b);
    f.reset();
    assert_eq!(f.r(0).payout(&f.supplier, &1), Outcome::Nothing);
    // Clear debt and restore pricing before exercising the original earner's
    // withdrawal; the prior test separately proves underwater payouts reject.
    f.reset();
    f.r(1).repay(&f.user, &500_000);
    MockOracleClient::new(&f.env, &f.oracle).set_price(&f.assets[0], &100_000_000_000_000);
    c.cache_price(&f.assets[0]);
    let seized = f.r(0).get_ptoken_balance(&f.supplier);
    assert!(seized > 0);
    f.reset();
    completed(f.r(0).payout(&f.user, &80_000));
    assert_eq!(f.r(0).get_ptoken_balance(&f.supplier), seized);
    assert_eq!(f.backing(0).units, 0);
}
