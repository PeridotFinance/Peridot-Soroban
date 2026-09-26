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

impl Fixture {
    fn incentives(&self, i: usize) -> Address {
        let peri = self
            .env
            .register_stellar_asset_contract_v2(self.admin.clone())
            .address();
        let c = SimplePeridottrollerClient::new(&self.env, &self.controller);
        c.set_peridot_token(&peri);
        c.set_supply_speed(&self.markets[i], &1000);
        // Explicitly initialize the ordinary supplier's index before time passes.
        self.reset();
        c.accrue_user_market(&self.supplier, &self.markets[i], &None);
        peri
    }
    fn advance_incentives(&self, seconds: u64) {
        self.env.ledger().with_mut(|l| l.timestamp += seconds);
        self.reset();
    }
}

#[test]
fn aquarius_lending_incentives_mint_preserves_pre_conversion_supply_interval() {
    let f = Fixture::new();
    f.routes(1);
    f.incentives(1);
    f.credit(1, 100_000, 0);
    f.advance_incentives(10);
    completed(f.r(1).compound(&f.rewards[0]));
    f.measure("incentive checkpoint before escrow mint");
    let c = SimplePeridottrollerClient::new(&f.env, &f.controller);
    f.reset();
    c.accrue_user_market(&f.supplier, &f.markets[1], &None);
    assert_eq!(c.get_accrued(&f.supplier), 10_000);
    assert_eq!(c.get_accrued(&f.markets[1]), 0);
}

#[test]
fn aquarius_lending_incentives_payout_cannot_capture_history_with_temporary_shares() {
    let f = Fixture::new();
    f.routes(1);
    f.incentives(1);
    f.credit(1, 100_000, 0);
    f.reset();
    completed(f.r(1).compound(&f.rewards[0]));
    let escrow = f.backing(1).ptokens;
    let supply = f.r(1).get_total_ptokens();
    let index_delta = 10_000u128 * 1_000_000_000_000_000_000 / supply;
    let ordinary_earned = 1_000_000 * index_delta / 1_000_000_000_000_000_000;
    let escrow_earned = escrow * index_delta / 1_000_000_000_000_000_000;
    f.advance_incentives(10);
    // The owner alone authorizes payout; controller hints must authenticate by
    // the actual calling receipt, not a blanket auth mock or a user signature.
    f.env.mock_auths(&[MockAuth {
        address: &f.supplier,
        invoke: &MockAuthInvoke {
            contract: &f.markets[1],
            fn_name: "payout",
            args: (&f.supplier, 1u128).into_val(&f.env),
            sub_invokes: &[],
        },
    }]);
    completed(f.r(1).payout(&f.supplier, &1));
    f.measure("incentive checkpoint exact-auth escrow payout");
    let c = SimplePeridottrollerClient::new(&f.env, &f.controller);
    assert_eq!(c.get_accrued(&f.supplier), ordinary_earned);
    assert_eq!(c.get_accrued(&f.markets[1]), escrow_earned);
    assert!(ordinary_earned + escrow_earned <= 10_000);
    assert_eq!(f.backing(1).units, 0);
}

#[test]
fn aquarius_lending_incentives_repeated_mints_keep_prior_escrow_interval() {
    let f = Fixture::new();
    f.routes(2);
    f.incentives(2);
    f.credit(2, 100_000, 0);
    f.reset();
    completed(f.r(2).compound(&f.rewards[0]));
    let first = f.backing(2).ptokens;
    let scale = 1_000_000_000_000_000_000u128;
    let d1 = 10_000 * scale / f.r(2).get_total_ptokens();
    f.credit(2, 0, 200_000);
    f.advance_incentives(10);
    completed(f.r(2).compound(&f.rewards[1]));
    f.measure("incentive checkpoint second escrow mint");
    let c = SimplePeridottrollerClient::new(&f.env, &f.controller);
    let first_earned = first * d1 / scale;
    assert_eq!(c.get_accrued(&f.markets[2]), first_earned);
    let second = f.backing(2).ptokens;
    assert!(second > first);
    let d2 = 20_000 * scale / f.r(2).get_total_ptokens();
    f.advance_incentives(20);
    completed(f.r(2).payout(&f.supplier, &1));
    assert_eq!(
        c.get_accrued(&f.markets[2]),
        first_earned + second * d2 / scale
    );
    assert_eq!(c.get_accrued(&f.supplier), 1_000_000 * (d1 + d2) / scale);
    assert!(c.get_accrued(&f.supplier) + c.get_accrued(&f.markets[2]) <= 30_000);
}

#[test]
fn aquarius_lending_incentives_failed_payout_rolls_back_controller_and_share_state() {
    let f = Fixture::new();
    f.routes(1);
    f.incentives(1);
    f.credit(1, 100_000, 0);
    f.reset();
    completed(f.r(1).compound(&f.rewards[0]));
    let before = f.backing(1);
    let supply = f.r(1).get_total_ptokens();
    let wallet = f.cash(1, &f.supplier);
    let c = SimplePeridottrollerClient::new(&f.env, &f.controller);
    f.advance_incentives(10);
    assert!(f.r(1).try_payout(&f.supplier, &1_000_000).is_err());
    assert_eq!(c.get_accrued(&f.supplier), 0);
    assert_eq!(c.get_accrued(&f.markets[1]), 0);
    assert_eq!(f.backing(1), before);
    assert_eq!(f.r(1).get_total_ptokens(), supply);
    assert_eq!(f.cash(1, &f.supplier), wallet);
    f.env.mock_auths(&[]);
    f.reset();
    assert!(f.r(1).try_payout(&f.supplier, &1).is_err());
    assert_eq!(c.get_accrued(&f.supplier), 0);
    assert_eq!(c.get_accrued(&f.markets[1]), 0);
    assert_eq!(f.backing(1), before);
    f.env.mock_all_auths();
    f.reset();
    completed(f.r(1).payout(&f.supplier, &1));
    let scale = 1_000_000_000_000_000_000u128;
    let d = 10_000 * scale / supply;
    assert_eq!(c.get_accrued(&f.supplier), 1_000_000 * d / scale);
    assert_eq!(c.get_accrued(&f.markets[1]), before.ptokens * d / scale);
}

#[test]
fn aquarius_lending_incentives_failed_controller_cannot_commit_conversion() {
    let f = Fixture::new();
    f.routes(1);
    f.incentives(1);
    f.credit(1, 100_000, 0);
    let before = f.backing(1);
    let primary = token::Client::new(&f.env, &f.rewards[0]);
    let pool_cash = primary.balance(&f.pools[1]);
    let bad = Address::generate(&f.env);
    // Test-only unavailable dependency. No production admin or selector bypass.
    f.env.as_contract(&f.markets[1], || {
        f.env
            .storage()
            .persistent()
            .set(&receipt_vault::DataKey::Peridottroller, &bad);
    });
    f.advance_incentives(10);
    assert!(f.r(1).try_compound(&f.rewards[0]).is_err());
    assert_eq!(f.backing(1), before);
    assert_eq!(f.r(1).get_total_ptokens(), 1_000_000);
    assert_eq!(primary.balance(&f.pools[1]), pool_cash);
    assert_eq!(primary.balance(&f.markets[1]), 0);
    f.env.as_contract(&f.markets[1], || {
        f.env
            .storage()
            .persistent()
            .set(&receipt_vault::DataKey::Peridottroller, &f.controller);
    });
    f.reset();
    completed(f.r(1).compound(&f.rewards[0]));
    let c = SimplePeridottrollerClient::new(&f.env, &f.controller);
    f.reset();
    c.accrue_user_market(&f.supplier, &f.markets[1], &None);
    assert_eq!(c.get_accrued(&f.supplier), 10_000);
}

#[test]
fn aquarius_lending_incentives_unfunded_and_later_paid_escrow_are_not_principal_nav() {
    let f = Fixture::new();
    f.routes(2);
    let peri = f.incentives(2);
    f.credit(2, 100_000, 0);
    f.reset();
    completed(f.r(2).compound(&f.rewards[0]));
    f.advance_incentives(10);
    completed(f.r(2).payout(&f.supplier, &1));
    let c = SimplePeridottrollerClient::new(&f.env, &f.controller);
    let ordinary = c.get_accrued(&f.supplier);
    let escrow = c.get_accrued(&f.markets[2]);
    assert!(ordinary > 0 && escrow > 0);
    let nav = f.r(2).nav();
    let managed = f.managed(2);
    let users = vec![&f.env, f.supplier.clone(), f.markets[2].clone()];
    f.env.mock_auths(&[]);
    f.reset();
    c.claim_all(&users);
    assert_eq!(c.get_accrued(&f.supplier), ordinary);
    assert_eq!(c.get_accrued(&f.markets[2]), escrow);
    f.env.mock_all_auths();
    token::StellarAssetClient::new(&f.env, &peri).mint(&f.controller, &10_000);
    f.env.mock_auths(&[]);
    f.reset();
    c.claim_all(&users);
    assert_eq!(
        token::Client::new(&f.env, &peri).balance(&f.supplier),
        ordinary as i128
    );
    assert_eq!(
        token::Client::new(&f.env, &peri).balance(&f.markets[2]),
        escrow as i128
    );
    assert_eq!(c.get_accrued(&f.supplier), 0);
    assert_eq!(c.get_accrued(&f.markets[2]), 0);
    assert_eq!(f.r(2).nav(), nav);
    assert_eq!(f.managed(2), managed);
    // This preserves the receipt's liability only. Attribution/payment of PERI
    // to the historical backing-unit owners is still a release blocker.
    assert_eq!(f.backing(2).units, 0);
}

#[test]
fn aquarius_lending_incentives_exited_owner_does_not_earn_on_temporary_payout_credit() {
    let f = Fixture::new();
    f.routes(1);
    f.incentives(1);
    f.credit(1, 100_000, 0);
    f.reset();
    completed(f.r(1).compound(&f.rewards[0]));
    f.reset();
    f.r(1).withdraw(&f.supplier, &1_000_000, &900_000);
    assert_eq!(f.r(1).get_ptoken_balance(&f.supplier), 0);
    let escrow = f.backing(1).ptokens;
    assert_eq!(f.r(1).get_total_ptokens(), escrow);
    f.advance_incentives(10);
    completed(f.r(1).payout(&f.supplier, &1));
    let c = SimplePeridottrollerClient::new(&f.env, &f.controller);
    assert_eq!(c.get_accrued(&f.supplier), 0);
    let scale = 1_000_000_000_000_000_000u128;
    assert_eq!(
        c.get_accrued(&f.markets[1]),
        (10_000 * scale / escrow) * escrow / scale
    );
    assert_eq!(f.r(1).get_total_ptokens(), 0);
}

#[test]
fn aquarius_lending_incentives_backing_hooks_preserve_separate_borrower_emissions() {
    let f = Fixture::new();
    f.routes(2);
    f.reset();
    f.r(2).borrow(&f.user, &200_000);
    f.incentives(2);
    let c = SimplePeridottrollerClient::new(&f.env, &f.controller);
    c.set_borrow_speed(&f.markets[2], &500);
    f.reset();
    c.accrue_user_market(&f.user, &f.markets[2], &None);
    f.credit(2, 100_000, 0);
    f.reset();
    completed(f.r(2).compound(&f.rewards[0]));
    let supply = f.r(2).get_total_ptokens();
    let escrow = f.backing(2).ptokens;
    f.advance_incentives(10);
    completed(f.r(2).payout(&f.supplier, &1));
    f.measure("incentive-enabled reward payout with loan");
    assert_eq!(f.r(2).get_user_borrow_balance(&f.user), 200_000);
    f.reset();
    c.accrue_user_market(&f.user, &f.markets[2], &None);
    assert_eq!(c.get_accrued(&f.user), 5000);
    let scale = 1_000_000_000_000_000_000u128;
    let d = 10_000 * scale / supply;
    assert_eq!(c.get_accrued(&f.supplier), 1_000_000 * d / scale);
    assert_eq!(c.get_accrued(&f.markets[2]), escrow * d / scale);
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
