//! E/F evidence on DEPLOYED logic (847894a6 controller + 4229b7ef market source).
//! Uses only deployed entrypoints so it compiles against the unmodified tree.
//! Native fixture: real ReceiptVault cash markets, SACs, per-asset mock oracle
//! with explicit observation timestamps. Not a network proof.
#![cfg(test)]
extern crate std;

use crate::{SimplePeridottroller, SimplePeridottrollerClient};
use receipt_vault as rv;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

pub(crate) const T0: u64 = 1_000_000;
pub(crate) const ONE: i128 = 10_000_000; // $1 at 7 decimals

#[contracttype]
enum OKey {
    P(Address),
    Ts(Address),
    Res,
}

#[contract]
pub struct AgeOracle;

#[contractimpl]
impl AgeOracle {
    pub fn set(env: Env, asset: Address, price: i128, ts: u64) {
        env.storage().persistent().set(&OKey::P(asset.clone()), &price);
        env.storage().persistent().set(&OKey::Ts(asset), &ts);
    }
    pub fn set_res(env: Env, res: u32) {
        env.storage().persistent().set(&OKey::Res, &res);
    }
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn resolution(env: Env) -> u32 {
        env.storage().persistent().get(&OKey::Res).unwrap_or(300)
    }
    pub fn lastprice(env: Env, asset: crate::reflector::Asset) -> Option<crate::reflector::PriceData> {
        let crate::reflector::Asset::Stellar(a) = asset else { return None };
        let price: i128 = env.storage().persistent().get(&OKey::P(a.clone()))?;
        let timestamp: u64 = env.storage().persistent().get(&OKey::Ts(a))?;
        Some(crate::reflector::PriceData { price, timestamp })
    }
}

pub(crate) struct Px {
    pub env: Env,
    #[allow(dead_code)]
    pub admin: Address,
    pub comp: Address,
    pub oracle: Address,
    pub v: std::vec::Vec<Address>,
    pub t: std::vec::Vec<Address>,
}

impl Px {
    pub fn new(n: usize) -> Self {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        env.cost_estimate().disable_resource_limits();
        env.cost_estimate().budget().reset_unlimited();
        env.ledger().with_mut(|l| l.timestamp = T0);
        let admin = Address::generate(&env);
        let comp = env.register(SimplePeridottroller, ());
        let cc = SimplePeridottrollerClient::new(&env, &comp);
        cc.initialize(&admin);
        let oracle = env.register(AgeOracle, ());
        cc.set_oracle(&oracle);
        let mut v = std::vec::Vec::new();
        let mut t = std::vec::Vec::new();
        for _ in 0..n {
            let tok = env.register_stellar_asset_contract_v2(admin.clone()).address();
            let vid = env.register(rv::ReceiptVault, ());
            let c = rv::ReceiptVaultClient::new(&env, &vid);
            c.initialize(&tok, &0u128, &0u128, &admin);
            c.enable_static_rates(&admin);
            c.set_min_cash_reserve_bps(&admin, &0u32);
            token::StellarAssetClient::new(&env, &tok).mint(&admin, &1_000);
            c.seed_minimum_supply(&admin, &1_000u128);
            c.set_peridottroller(&comp);
            cc.add_market(&vid);
            cc.set_market_cf(&vid, &500_000u128);
            c.set_cash_borrowing_enabled(&admin, &true);
            AgeOracleClient::new(&env, &oracle).set(&tok, &ONE, &T0);
            v.push(vid);
            t.push(tok);
        }
        let p = Px { env, admin, comp, oracle, v, t };
        let supplier = Address::generate(&p.env);
        for i in 0..n {
            p.deposit(&supplier, i, 1_000_000);
        }
        p
    }
    pub fn cc(&self) -> SimplePeridottrollerClient<'_> {
        SimplePeridottrollerClient::new(&self.env, &self.comp)
    }
    pub fn vc(&self, i: usize) -> rv::ReceiptVaultClient<'_> {
        rv::ReceiptVaultClient::new(&self.env, &self.v[i])
    }
    pub fn deposit(&self, who: &Address, i: usize, amt: u128) {
        token::StellarAssetClient::new(&self.env, &self.t[i]).mint(who, &(amt as i128));
        self.vc(i).deposit(who, &amt);
    }
    pub fn price(&self, i: usize, price: i128, ts: u64) {
        AgeOracleClient::new(&self.env, &self.oracle).set(&self.t[i], &price, &ts);
    }
    pub fn res(&self, r: u32) {
        AgeOracleClient::new(&self.env, &self.oracle).set_res(&r);
    }
    /// Advance the ledger to T0 + dt; every token observation stays at T0
    /// unless re-published. Caches written at T0 carry timestamp T0.
    pub fn at(&self, dt: u64) {
        self.env.ledger().with_mut(|l| l.timestamp = T0 + dt);
    }
    pub fn now(&self) -> u64 {
        self.env.ledger().timestamp()
    }
    /// User with collateral in market `c` (1000) and debt `debt` in market `d`.
    pub fn borrower(&self, c: usize, d: usize, debt: u128) -> Address {
        let u = Address::generate(&self.env);
        self.deposit(&u, c, 1_000);
        self.cc().enter_market(&u, &self.v[c]);
        if c != d {
            self.cc().enter_market(&u, &self.v[d]);
        }
        if debt > 0 {
            self.vc(d).borrow(&u, &debt);
        }
        u
    }
}

// --- E: deployed staleness behaviour (k = 2 default, resolution 300 => 600 s) ---

#[test]
fn dep_e_borrow_reverts_when_collateral_price_stale() {
    let p = Px::new(2);
    let u = p.borrower(0, 1, 100);
    p.at(601);
    p.price(1, ONE, p.now()); // borrowed token fresh
    assert!(p.vc(1).try_borrow(&u, &10u128).is_err(), "stale collateral must block borrow");
    p.price(0, ONE, p.now() - 600); // exactly 600 s old: still fresh
    p.vc(1).borrow(&u, &10u128);
}

#[test]
fn dep_e_borrow_reverts_when_borrowed_token_price_stale() {
    let p = Px::new(2);
    let u = p.borrower(0, 1, 100);
    p.at(601);
    p.price(0, ONE, p.now()); // collateral fresh; borrowed token stays at T0
    assert!(p.vc(1).try_borrow(&u, &10u128).is_err());
}

#[test]
fn dep_e_k3_bound_is_900_seconds() {
    let p = Px::new(2);
    p.cc().set_oracle_max_age_multiplier(&3u64);
    let u = p.borrower(0, 1, 100);
    p.at(900);
    p.vc(1).borrow(&u, &10u128); // both observations exactly 900 s old: served
    p.at(901);
    assert!(p.vc(1).try_borrow(&u, &10u128).is_err());
}

#[test]
fn dep_e_withdraw_with_debt_reverts_when_own_token_stale() {
    let p = Px::new(2);
    let u = p.borrower(0, 1, 100);
    p.at(601);
    p.price(1, ONE, p.now());
    assert!(p.vc(0).try_withdraw(&u, &10u128).is_err());
}

#[test]
fn dep_e_withdraw_with_debt_reverts_when_other_collateral_stale() {
    // Collateral in markets 0 and 1, debt in 0; market 1's price goes stale.
    let p = Px::new(2);
    let u = Address::generate(&p.env);
    p.deposit(&u, 0, 300);
    p.deposit(&u, 1, 1_000);
    p.cc().enter_market(&u, &p.v[0]);
    p.cc().enter_market(&u, &p.v[1]);
    p.vc(0).borrow(&u, &300u128); // needs market-1 collateral
    p.at(601);
    p.price(0, ONE, p.now());
    assert!(p.vc(0).try_withdraw(&u, &10u128).is_err(), "other collateral counts as 0 when stale");
}

#[test]
fn dep_e_withdraw_with_debt_reverts_when_other_debt_stale() {
    let p = Px::new(2);
    let u = p.borrower(0, 1, 100);
    p.at(601);
    p.price(0, ONE, p.now()); // own token fresh; debt token stays stale
    assert!(p.vc(0).try_withdraw(&u, &10u128).is_err(), "unpriced debt fails closed");
}

#[test]
fn dep_e_withdraw_without_debt_works_with_all_prices_stale() {
    let p = Px::new(2);
    let u = Address::generate(&p.env);
    p.deposit(&u, 0, 1_000);
    p.deposit(&u, 1, 1_000);
    p.cc().enter_market(&u, &p.v[0]);
    p.cc().enter_market(&u, &p.v[1]);
    p.at(10_000);
    assert!(p.cc().get_price_usd(&p.t[0]).is_none());
    p.vc(0).withdraw(&u, &500u128);
    p.vc(1).withdraw(&u, &1_000u128);
}

#[test]
#[ignore = "deployed-only behaviour: the upgrade candidate caps age at 900 s (see upg_e_absolute_bound_*); run with --include-ignored on the pristine tree"]
fn dep_e_bound_follows_oracle_reported_resolution() {
    // Freshness = k * resolution reported by the oracle itself: 3600 s => 7200 s.
    let p = Px::new(2);
    p.res(3600);
    let u = p.borrower(0, 1, 100);
    p.at(7200);
    p.vc(1).borrow(&u, &10u128);
    p.at(7201);
    assert!(p.vc(1).try_borrow(&u, &10u128).is_err());
}

#[test]
#[ignore = "deployed-only behaviour: the upgrade candidate ignores fallback prices by default (see upg_e_fallback_*); run with --include-ignored on the pristine tree"]
fn dep_e_fresh_fallback_is_served_for_24h_when_oracle_stale() {
    let p = Px::new(2);
    let u = p.borrower(0, 1, 100);
    p.cc().set_price_fallback(&p.t[0], &Some((10_000_000u128, 10_000_000u128)));
    p.cc().set_price_fallback(&p.t[1], &Some((10_000_000u128, 10_000_000u128)));
    p.at(601);
    assert_eq!(p.cc().get_price_usd(&p.t[0]), Some((10_000_000, 10_000_000)));
    p.vc(1).borrow(&u, &10u128); // served from static fallback despite stale oracle
    p.at(24 * 3600 + 1);
    assert!(p.cc().get_price_usd(&p.t[0]).is_none());
    assert!(p.vc(1).try_borrow(&u, &10u128).is_err());
}

#[test]
fn dep_e_cache_price_is_permissionless() {
    let p = Px::new(1);
    p.env.set_auths(&[]);
    assert_eq!(p.cc().cache_price(&p.t[0]), Some((10_000_000, 10_000_000)));
}

// --- F: guardian is pause-only; guardian setter is admin-only ---

#[test]
fn dep_f_guardian_variants_reject_unpause() {
    let p = Px::new(1);
    let g = Address::generate(&p.env);
    p.cc().set_pause_guardian(&g);
    let m = &p.v[0];
    assert!(p.cc().try_pause_borrow_g(&g, m, &false).is_err());
    assert!(p.cc().try_pause_redeem_g(&g, m, &false).is_err());
    assert!(p.cc().try_pause_liquidation_g(&g, m, &false).is_err());
    assert!(p.cc().try_pause_deposit_g(&g, m, &false).is_err());
    p.cc().pause_borrow_g(&g, m, &true);
    assert!(p.cc().is_borrow_paused(m));
    // Wrong guardian address rejected even with auth.
    let other = Address::generate(&p.env);
    assert!(p.cc().try_pause_redeem_g(&other, m, &true).is_err());
}

#[test]
fn dep_f_set_pause_guardian_requires_admin_auth() {
    let p = Px::new(1);
    let g = Address::generate(&p.env);
    p.env.set_auths(&[]);
    assert!(p.cc().try_set_pause_guardian(&g).is_err());
}
