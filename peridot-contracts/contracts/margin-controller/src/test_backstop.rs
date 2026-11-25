//! WP1 backstop unit tests (native). Mock receipt / controller / pool live here; tokens are
//! real Stellar Asset Contracts, and the mocks pull with `token.transfer(user -> self)` so the
//! margin contract's `authorize_as_current_contract` trees are exercised.
extern crate std;

use crate::backstop;
use crate::common::{ledger, ledger_add};
use crate::types::*;
use crate::MarginControllerClient;
use soroban_sdk::testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{contract, contractimpl, token, vec, Address, Env, IntoVal, Symbol, Vec};

// ---------------------------------------------------------------- mocks
#[contract]
pub struct MockReceipt;

fn rk(e: &Env, tag: &str) -> Symbol {
    Symbol::new(e, tag)
}

#[contractimpl]
impl MockReceipt {
    pub fn init(e: Env, underlying: Address, rate: u128) {
        e.storage().instance().set(&rk(&e, "u"), &underlying);
        e.storage().instance().set(&rk(&e, "rate"), &rate);
    }
    pub fn set_rate(e: Env, rate: u128) {
        e.storage().instance().set(&rk(&e, "rate"), &rate);
    }
    pub fn set_borrow(e: Env, user: Address, amount: u128) {
        e.storage().persistent().set(&(rk(&e, "b"), user), &amount);
    }
    pub fn mint_pt(e: Env, user: Address, amount: u128) {
        let b = Self::get_ptoken_balance(e.clone(), user.clone());
        e.storage().persistent().set(&(rk(&e, "p"), user), &(b + amount));
    }
    pub fn deposit(e: Env, user: Address, amount: u128) {
        user.require_auth();
        let u: Address = e.storage().instance().get(&rk(&e, "u")).unwrap();
        token::TokenClient::new(&e, &u).transfer(&user, e.current_contract_address(), &(amount as i128));
        let pt = amount * 1_000_000 / Self::get_exchange_rate(e.clone());
        Self::mint_pt(e, user, pt);
    }
    pub fn withdraw(e: Env, user: Address, ptoken_amount: u128) {
        user.require_auth();
        let b = Self::get_ptoken_balance(e.clone(), user.clone());
        assert!(b >= ptoken_amount, "pt balance");
        e.storage().persistent().set(&(rk(&e, "p"), user.clone()), &(b - ptoken_amount));
        let out = ptoken_amount * Self::get_exchange_rate(e.clone()) / 1_000_000;
        let u: Address = e.storage().instance().get(&rk(&e, "u")).unwrap();
        token::TokenClient::new(&e, &u).transfer(&e.current_contract_address(), &user, &(out as i128));
    }
    pub fn repay(e: Env, user: Address, amount: u128) {
        user.require_auth();
        let d = Self::get_user_borrow_balance(e.clone(), user.clone());
        assert!(amount <= d, "repay > debt");
        let u: Address = e.storage().instance().get(&rk(&e, "u")).unwrap();
        token::TokenClient::new(&e, &u).transfer(&user, e.current_contract_address(), &(amount as i128));
        Self::set_borrow(e, user, d - amount);
    }
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let a = amount as u128;
        let fb = Self::get_ptoken_balance(e.clone(), from.clone());
        assert!(fb >= a, "pt balance");
        e.storage().persistent().set(&(rk(&e, "p"), from), &(fb - a));
        Self::mint_pt(e, to, a);
    }
    pub fn get_underlying_token(e: Env) -> Address {
        e.storage().instance().get(&rk(&e, "u")).unwrap()
    }
    pub fn get_exchange_rate(e: Env) -> u128 {
        e.storage().instance().get(&rk(&e, "rate")).unwrap()
    }
    pub fn get_ptoken_balance(e: Env, user: Address) -> u128 {
        e.storage().persistent().get(&(rk(&e, "p"), user)).unwrap_or(0)
    }
    pub fn get_user_borrow_balance(e: Env, user: Address) -> u128 {
        e.storage().persistent().get(&(rk(&e, "b"), user)).unwrap_or(0)
    }
    pub fn update_interest(_e: Env) {}
}

#[contract]
pub struct MockCtrl;

#[contractimpl]
impl MockCtrl {
    pub fn set_price(e: Env, token: Address, p: u128, s: u128) {
        e.storage().persistent().set(&token, &(p, s));
    }
    pub fn get_price_usd(e: Env, token: Address) -> Option<(u128, u128)> {
        e.storage().persistent().get(&token)
    }
    pub fn add_market(e: Env, market: Address, cf: u128) {
        let mut v: Vec<(Address, u128)> = e.storage().instance().get(&rk(&e, "mk")).unwrap_or(Vec::new(&e));
        v.push_back((market, cf));
        e.storage().instance().set(&rk(&e, "mk"), &v);
    }
    pub fn get_market_cf(e: Env, market: Address) -> u128 {
        let v: Vec<(Address, u128)> = e.storage().instance().get(&rk(&e, "mk")).unwrap_or(Vec::new(&e));
        v.iter().find(|(m, _)| *m == market).map(|(_, cf)| cf).unwrap_or(0)
    }
    /// Live `portfolio` shape and truncation order, over the configured markets (the real
    /// pool check runs on every backstop exit).
    #[allow(clippy::type_complexity)]
    pub fn portfolio(e: Env, user: Address) -> (Vec<(Address, u128, u128, u128, u128)>, (u128, u128)) {
        let v: Vec<(Address, u128)> = e.storage().instance().get(&rk(&e, "mk")).unwrap_or(Vec::new(&e));
        let mut rows = Vec::new(&e);
        let (mut ct, mut dt) = (0u128, 0u128);
        for (m, cf) in v.iter() {
            let r = MockReceiptClient::new(&e, &m);
            let (pbal, debt) = (r.get_ptoken_balance(&user), r.get_user_borrow_balance(&user));
            let (p, sc) = Self::get_price_usd(e.clone(), r.get_underlying_token()).expect("price");
            let coll = pbal * r.get_exchange_rate() / 1_000_000 * cf / 1_000_000 * p / sc;
            let dusd = debt * p / sc;
            rows.push_back((m, pbal, debt, coll, dusd));
            ct += coll;
            dt += dusd;
        }
        (rows, (ct, dt))
    }
}

/// Exact-input pool: out = in * num / den. Pulls the input with `transfer(user -> pool)`
/// (no require_auth on swap, like Aquarius). Does not check out_min itself, so the margin
/// contract's own Slippage check is what is tested.
#[contract]
pub struct MockPool;

#[contractimpl]
impl MockPool {
    pub fn init(e: Env, tokens: Vec<Address>, num: u128, den: u128) {
        e.storage().instance().set(&rk(&e, "t"), &tokens);
        e.storage().instance().set(&rk(&e, "r"), &(num, den));
    }
    pub fn get_tokens(e: Env) -> Vec<Address> {
        e.storage().instance().get(&rk(&e, "t")).unwrap()
    }
    pub fn swap(e: Env, user: Address, in_idx: u32, out_idx: u32, amount_in: u128, _out_min: u128) -> u128 {
        let t = Self::get_tokens(e.clone());
        let (num, den): (u128, u128) = e.storage().instance().get(&rk(&e, "r")).unwrap();
        let me = e.current_contract_address();
        token::TokenClient::new(&e, &t.get(in_idx).unwrap()).transfer(&user, &me, &(amount_in as i128));
        let out = amount_in * num / den;
        token::TokenClient::new(&e, &t.get(out_idx).unwrap()).transfer(&me, &user, &(out as i128));
        out
    }
}

// ---------------------------------------------------------------- fixture
const RATE: u128 = 1_234_567; // pTokens -> USDC underlying, 1e6 (deliberately not 1e6)
const PS: u128 = 10_000_000; // price scale
const P_USDC: u128 = 10_000_000; // $1
const P_XLM: u128 = 2_500_000; // $0.25
const T0: u64 = 1_000_000;

struct F {
    env: Env,
    m: MarginControllerClient<'static>,
    admin: Address,
    usdc: Address,
    xlm: Address,
    ru: MockReceiptClient<'static>,
    rx: MockReceiptClient<'static>,
    ctrl: MockCtrlClient<'static>,
    pool: Address,
}

fn setup() -> F {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);
    let admin = Address::generate(&env);
    let guardian = Address::generate(&env);
    let issuer = Address::generate(&env);
    let usdc = env.register_stellar_asset_contract_v2(issuer.clone()).address();
    let xlm = env.register_stellar_asset_contract_v2(issuer).address();
    let ctrl_id = env.register(MockCtrl, ());
    let ctrl = MockCtrlClient::new(&env, &ctrl_id);
    ctrl.set_price(&usdc, &P_USDC, &PS);
    ctrl.set_price(&xlm, &P_XLM, &PS);
    let ru_id = env.register(MockReceipt, ());
    let ru = MockReceiptClient::new(&env, &ru_id);
    ru.init(&usdc, &RATE);
    let rx_id = env.register(MockReceipt, ());
    let rx = MockReceiptClient::new(&env, &rx_id);
    rx.init(&xlm, &1_000_000);
    // 1 USDC -> 4 XLM less a 0.2 % fee (oracle says 4).
    let pool = env.register(MockPool, ());
    MockPoolClient::new(&env, &pool).init(&vec![&env, usdc.clone(), xlm.clone()], &3_992, &1_000);
    let m_id = env.register(crate::MarginController, (admin.clone(), ctrl_id, guardian, usdc.clone(), xlm.clone()));
    let m = MarginControllerClient::new(&env, &m_id);
    m.set_market(&admin, &usdc, &ru_id);
    m.set_market(&admin, &xlm, &rx_id);
    m.set_pool(&admin, &pool);
    ctrl.add_market(&ru_id, &750_000);
    ctrl.add_market(&rx_id, &550_000);
    // Liquidity: receipt pays USDC on withdraw; pool pays XLM on swaps.
    token::StellarAssetClient::new(&env, &usdc).mint(&ru_id, &1_000_000_000_000);
    token::StellarAssetClient::new(&env, &xlm).mint(&pool, &1_000_000_000_000);
    F { env, m, admin, usdc, xlm, ru, rx, ctrl, pool }
}

impl F {
    fn backer(&self, pt: u128) -> Address {
        let a = Address::generate(&self.env);
        self.ru.mint_pt(&a, &pt);
        a
    }
    fn seed(&self, pt: u128) {
        self.ru.mint_pt(&self.admin, &pt);
        assert_eq!(self.m.backstop_deposit(&self.admin, &pt, &pt), pt);
    }
    fn at(&self, t: u64) {
        self.env.ledger().set_timestamp(t);
    }
    fn set_inst<V: IntoVal<Env, soroban_sdk::Val>>(&self, k: Key, v: V) {
        let val: soroban_sdk::Val = v.into_val(&self.env);
        self.env.as_contract(&self.m.address, || self.env.storage().instance().set(&k, &val));
    }
    /// Fee income the way WP2 books it: pTokens already in the contract, Ledger, then credit.
    fn fee(&self, pt: u128) {
        self.ru.mint_pt(&self.m.address, &pt);
        self.env.as_contract(&self.m.address, || {
            ledger_add(&self.env, &self.usdc, pt);
            backstop::credit_pt(&self.env, pt);
        });
    }
    fn bad_debt(&self, asset: &Address, shares: u128) {
        self.env.as_contract(&self.m.address, || backstop::assign_bad_debt(&self.env, asset, shares));
    }
    fn ledger_u(&self) -> u128 {
        self.env.as_contract(&self.m.address, || ledger(&self.env, &self.usdc))
    }
    fn nav(&self) -> i128 {
        self.m.backstop_nav().0
    }
    fn params(&self, cooldown: u64, window: u64) {
        self.m.set_backstop_params(&self.admin, &BackstopParams { cooldown_s: cooldown, window_s: window, min_deposit_pt: 1_000 });
    }
}

macro_rules! err {
    ($e:expr) => {
        match $e {
            Err(Ok(e)) => {
                let e: soroban_sdk::Error = e;
                Error::try_from(e).expect("not a margin error")
            }
            other => panic!("expected contract error, got {:?}", other),
        }
    };
}

fn ceil(a: u128, b: u128, d: u128) -> u128 {
    (a * b).div_ceil(d)
}

// ---------------------------------------------------------------- tests
#[test]
fn dead_shares_and_first_deposit_rules() {
    let f = setup();
    // First deposit must be the admin's.
    let alice = f.backer(5_000_000);
    assert_eq!(err!(f.m.try_backstop_deposit(&alice, &5_000_000, &0)), Error::NotAdmin);
    // Below min_deposit_pt (default 1e6).
    f.ru.mint_pt(&f.admin, &999_999);
    assert_eq!(err!(f.m.try_backstop_deposit(&f.admin, &999_999, &0)), Error::BadAmount);

    f.seed(10_000_000);
    let b = f.m.get_backstop();
    assert_eq!(b.shares, 10_000_000 + BS_DEAD_SHARES);
    assert_eq!(b.ptokens, 10_000_000);
    assert_eq!(f.m.get_backer(&f.admin), 10_000_000);
    assert_eq!(f.m.get_backer(&f.m.address), BS_DEAD_SHARES);
    assert_eq!(f.ledger_u(), 10_000_000);
    assert_eq!(f.ru.get_ptoken_balance(&f.m.address), 10_000_000);
    // Dead shares sit with the contract: the admin's full exit leaves them (and their value) in the pool.
    f.params(3_600, 3_600);
    f.m.backstop_request_withdraw(&f.admin, &10_000_000);
    f.at(T0 + 3_600);
    let out = f.m.backstop_withdraw(&f.admin, &0);
    assert_eq!(out, 10_000_000u128 * 10_000_000 / 11_000_000);
    let b = f.m.get_backstop();
    assert_eq!(b.shares, BS_DEAD_SHARES);
    assert_eq!(b.ptokens, 10_000_000 - out);
    assert!(f.nav() > 0);
    // After the seed, anyone may deposit (shares never return to 0).
    let m = f.m.backstop_deposit(&alice, &5_000_000, &0);
    assert!(m > 0);
}

#[test]
fn mint_and_redeem_round_against_the_backer() {
    let f = setup();
    f.params(3_600, 3_600);
    f.seed(10_000_000);
    f.fee(7); // NAV per share is no longer a round number
    let b0 = f.m.get_backstop();
    let nav0 = f.nav() as u128;
    assert_eq!(nav0, 10_000_007);

    let alice = f.backer(3_333_333);
    let exact_num = 3_333_333u128 * b0.shares; // shares = pt * S / nav
    let expect = exact_num / nav0;
    assert!(!exact_num.is_multiple_of(nav0), "test needs a non-exact division");
    assert_eq!(err!(f.m.try_backstop_deposit(&alice, &3_333_333, &(expect + 1))), Error::MinShares);
    let minted = f.m.backstop_deposit(&alice, &3_333_333, &expect);
    assert_eq!(minted, expect);
    assert_eq!(f.ledger_u(), 10_000_007 + 3_333_333);

    // Immediate roundtrip never pays out more than was deposited.
    f.m.backstop_request_withdraw(&alice, &minted);
    f.at(T0 + 3_600);
    let b1 = f.m.get_backstop();
    let nav1 = f.nav() as u128;
    let expect_out = minted * nav1 / b1.shares;
    assert_eq!(err!(f.m.try_backstop_withdraw(&alice, &(expect_out + 1))), Error::MinPtokens);
    let out = f.m.backstop_withdraw(&alice, &expect_out);
    assert_eq!(out, expect_out);
    assert!(out <= 3_333_333);
    assert_eq!(f.ru.get_ptoken_balance(&alice), out);
    let b2 = f.m.get_backstop();
    assert_eq!(b2.ptokens, 10_000_007 + 3_333_333 - out);
    assert_eq!(b2.pending_shares, 0);
    assert_eq!(f.ledger_u(), b2.ptokens);
    assert_eq!(f.m.get_withdraw_req(&alice), None);
    // The remaining holders' NAV per share did not fall.
    assert!(b2.ptokens * b0.shares >= nav0 * b2.shares);
}

#[test]
fn donations_are_ignored() {
    let f = setup();
    f.seed(10_000_000);
    let nav = f.nav();
    // Someone transfers pTokens directly to the margin contract.
    f.ru.mint_pt(&f.m.address, &50_000_000);
    assert_eq!(f.nav(), nav);
    assert_eq!(f.m.get_backstop().ptokens, 10_000_000);
    assert_eq!(f.ledger_u(), 10_000_000);
    // A later depositor still gets the undonated price.
    let alice = f.backer(2_000_000);
    let b = f.m.get_backstop();
    let m = f.m.backstop_deposit(&alice, &2_000_000, &0);
    assert_eq!(m, 2_000_000 * b.shares / 10_000_000);
}

#[test]
fn cooldown_window_expiry_and_repeated_timestamps() {
    let f = setup();
    assert_eq!(err!(f.m.try_set_backstop_params(&f.admin, &BackstopParams { cooldown_s: 3_599, window_s: 3_600, min_deposit_pt: 1 })), Error::BadParams);
    assert_eq!(err!(f.m.try_set_backstop_params(&f.admin, &BackstopParams { cooldown_s: 3_600, window_s: 3_599, min_deposit_pt: 1 })), Error::BadParams);
    f.params(3_600, 7_200);
    f.seed(30_000_000);
    let alice = f.backer(10_000_000);
    let bob = f.backer(10_000_000);
    let sa = f.m.backstop_deposit(&alice, &10_000_000, &0);
    let sb = f.m.backstop_deposit(&bob, &10_000_000, &0);

    assert_eq!(err!(f.m.try_backstop_withdraw(&alice, &0)), Error::NoRequest);
    assert_eq!(err!(f.m.try_backstop_request_withdraw(&alice, &(sa + 1))), Error::BadAmount);
    assert_eq!(err!(f.m.try_backstop_request_withdraw(&alice, &0)), Error::BadAmount);
    f.m.backstop_request_withdraw(&alice, &sa);
    f.m.backstop_request_withdraw(&bob, &sb);
    let r = f.m.get_withdraw_req(&alice).unwrap();
    assert_eq!((r.shares, r.ready_at, r.expires_at), (sa, T0 + 3_600, T0 + 3_600 + 7_200));
    assert_eq!(f.m.get_backer(&alice), 0);
    assert_eq!(f.m.get_backstop().pending_shares, sa + sb);
    // Live request: no second one.
    assert_eq!(err!(f.m.try_backstop_request_withdraw(&alice, &1)), Error::BadParams);

    f.at(T0 + 3_599);
    assert_eq!(err!(f.m.try_backstop_withdraw(&alice, &0)), Error::Cooldown);
    // ready_at is inclusive, and several calls may share the same whole-second timestamp.
    f.at(T0 + 3_600);
    f.m.backstop_withdraw(&alice, &0);
    assert_eq!(err!(f.m.try_backstop_withdraw(&alice, &0)), Error::NoRequest);
    // Same timestamp again: Alice re-deposits and re-requests; the new request is a full cooldown away.
    let a_pt = f.ru.get_ptoken_balance(&alice);
    let sa2 = f.m.backstop_deposit(&alice, &a_pt, &0);
    f.m.backstop_request_withdraw(&alice, &sa2);
    assert_eq!(f.m.get_withdraw_req(&alice).unwrap().ready_at, T0 + 7_200);
    f.m.backstop_cancel_withdraw(&alice);
    f.m.backstop_request_withdraw(&alice, &sa2); // same second, after a cancel
    assert_eq!(f.m.get_backer(&alice), 0);

    // expires_at is inclusive; one second later the window is closed.
    let exp = T0 + 3_600 + 7_200;
    f.at(exp + 1);
    assert_eq!(err!(f.m.try_backstop_withdraw(&bob, &0)), Error::WindowExpired);
    // Expired shares still count as pending until cancelled or replaced.
    assert_eq!(f.m.get_backstop().pending_shares, sa2 + sb);
    // A new request replaces the expired one (old shares returned first).
    f.m.backstop_request_withdraw(&bob, &(sb / 2));
    assert_eq!(f.m.get_backer(&bob), sb - sb / 2);
    assert_eq!(f.m.get_backstop().pending_shares, sa2 + sb / 2);
    // Cancel works after expiry too.
    f.at(exp + 1 + 3_600 + 7_200 + 1);
    f.m.backstop_cancel_withdraw(&bob);
    assert_eq!(f.m.get_backer(&bob), sb);
    assert_eq!(err!(f.m.try_backstop_cancel_withdraw(&bob)), Error::NoRequest);

    // Exactly at expires_at the withdraw still works.
    let carol = f.backer(5_000_000);
    let sc = f.m.backstop_deposit(&carol, &5_000_000, &0);
    let t = f.env.ledger().timestamp();
    f.m.backstop_request_withdraw(&carol, &sc);
    // A live request (now == expires_at) cannot be replaced.
    f.at(t + 3_600 + 7_200);
    assert_eq!(err!(f.m.try_backstop_request_withdraw(&carol, &1)), Error::BadParams);
    assert!(f.m.backstop_withdraw(&carol, &0) > 0);
}

#[test]
fn partial_nav_after_usdc_bad_debt() {
    let f = setup();
    f.params(3_600, 3_600);
    f.seed(10_000_000);
    let alice = f.backer(10_000_000);
    let sa = f.m.backstop_deposit(&alice, &10_000_000, &0);
    // Pooled USDC debt 5_000_003 across 4_000_001 position shares; WP2 moves 1_000_000 shares
    // of an underwater position into the backstop bucket (DebtShares drops by the same amount).
    f.ru.set_borrow(&f.m.address, &5_000_003);
    f.set_inst(Key::DebtShares(f.usdc.clone()), 3_000_001u128);
    let per_share_before = (5_000_003u128, 4_000_001u128);
    f.bad_debt(&f.usdc, 1_000_000);
    assert_eq!(f.m.get_bs_debt_shares(&f.usdc), 1_000_000);
    // Other positions' debt per share is unchanged (same pooled, same summed divisor).
    let total = 3_000_001u128 + f.m.get_bs_debt_shares(&f.usdc);
    assert_eq!(total, per_share_before.1);

    let d = ceil(1_000_000, 5_000_003, 4_000_001);
    let debt_pt = ceil(d, 1_000_000, RATE);
    let gross = 20_000_000u128;
    assert_eq!(f.nav(), (gross - debt_pt) as i128);

    // pending_pt and value_usd.
    f.m.backstop_request_withdraw(&alice, &sa);
    let b = f.m.get_backstop();
    let nav = gross - debt_pt;
    f.env.as_contract(&f.m.address, || {
        assert_eq!(backstop::pending_pt(&f.env), ceil(sa, nav, b.shares));
        assert_eq!(backstop::value_usd(&f.env), nav * RATE / 1_000_000 * P_USDC / PS);
    });

    // Alice exits at the reduced NAV.
    f.at(T0 + 3_600);
    let out = f.m.backstop_withdraw(&alice, &0);
    assert_eq!(out, sa * nav / b.shares);
    assert!(out < 10_000_000);

    // Exhaust the backstop: NAV <= 0 blocks deposits and exits.
    f.bad_debt(&f.usdc, 100_000_000);
    f.ru.set_borrow(&f.m.address, &500_000_000);
    assert!(f.nav() < 0);
    let bob = f.backer(5_000_000);
    assert_eq!(err!(f.m.try_backstop_deposit(&bob, &5_000_000, &0)), Error::BackstopEmpty);
    f.m.backstop_request_withdraw(&f.admin, &1_000);
    f.at(T0 + 7_200);
    assert_eq!(err!(f.m.try_backstop_withdraw(&f.admin, &0)), Error::BackstopEmpty);
    f.env.as_contract(&f.m.address, || {
        assert_eq!(backstop::pending_pt(&f.env), 0);
        assert_eq!(backstop::value_usd(&f.env), 0);
    });
}

#[test]
fn frozen_while_xlm_bad_debt_or_impaired() {
    let f = setup();
    f.params(3_600, 3_600);
    f.seed(100_000_000);
    let alice = f.backer(10_000_000);
    let sa = f.m.backstop_deposit(&alice, &10_000_000, &0);
    f.m.backstop_request_withdraw(&alice, &sa);
    f.at(T0 + 3_600);

    f.rx.set_borrow(&f.m.address, &40_000_001); // ~4 XLM
    f.bad_debt(&f.xlm, 7);
    let bob = f.backer(5_000_000);
    assert_eq!(err!(f.m.try_backstop_deposit(&bob, &5_000_000, &0)), Error::BackstopDebtOutstanding);
    assert_eq!(err!(f.m.try_backstop_withdraw(&alice, &0)), Error::BackstopDebtOutstanding);
    // Non-strict NAV values XLM debt at the oracle, rounded up at each step.
    let usd_up = ceil(40_000_001, P_XLM, PS);
    let usdc_up = ceil(usd_up, PS, P_USDC);
    assert_eq!(f.nav(), (110_000_000 - ceil(usdc_up, 1_000_000, RATE)) as i128);
    // Queueing is not frozen.
    f.m.backstop_cancel_withdraw(&alice);
    f.m.backstop_request_withdraw(&alice, &sa);
    // Stale oracle: non-strict NAV cannot value XLM debt.
    f.ctrl.set_price(&f.xlm, &0, &PS);
    assert_eq!(err!(f.m.try_backstop_nav()), Error::PriceUnavailable);
    f.ctrl.set_price(&f.xlm, &P_XLM, &PS);

    // Cleared (settled elsewhere); then the Impaired flag freezes too.
    f.set_inst(Key::BsDebtShares(f.xlm.clone()), 0u128);
    f.set_inst(Key::Impaired, true);
    f.at(T0 + 3_600 * 3); // last second of Alice's re-requested window
    assert_eq!(err!(f.m.try_backstop_deposit(&bob, &5_000_000, &0)), Error::Impaired);
    assert_eq!(err!(f.m.try_backstop_withdraw(&alice, &0)), Error::Impaired);
    f.set_inst(Key::Impaired, false);
    assert!(f.m.backstop_withdraw(&alice, &0) > 0);
    assert!(f.m.backstop_deposit(&bob, &5_000_000, &0) > 0);
}

#[test]
fn settle_usdc_strict_auth() {
    let f = setup();
    f.seed(10_000_000);
    f.ru.set_borrow(&f.m.address, &2_000_001);
    f.set_inst(Key::DebtShares(f.usdc.clone()), 1_499_999u128);
    f.bad_debt(&f.usdc, 500_000);
    let keeper = Address::generate(&f.env);
    let total = 1_999_999u128;
    let d = ceil(500_000, 2_000_001, total);
    let pt = ceil(d, 1_000_000, RATE);

    // Partial settle first (max_ptokens caps the withdrawal).
    let cap = 100_000u128;
    let args = (keeper.clone(), f.usdc.clone(), cap, 0u128).into_val(&f.env);
    f.env.mock_auths(&[MockAuth {
        address: &keeper,
        invoke: &MockAuthInvoke { contract: &f.m.address, fn_name: "settle_backstop_debt", args, sub_invokes: &[] },
    }]);
    let repaid1 = f.m.settle_backstop_debt(&keeper, &f.usdc, &cap, &0);
    assert_eq!(repaid1, cap * RATE / 1_000_000);
    let burned1 = repaid1 * total / 2_000_001;
    assert_eq!(f.m.get_bs_debt_shares(&f.usdc), 500_000 - burned1);
    // Other positions' debt per share never rises: pooled'/total' <= pooled/total.
    let pooled1 = 2_000_001 - repaid1;
    let total1 = total - burned1;
    assert!(pooled1 * total <= 2_000_001 * total1);
    assert_eq!(f.m.get_backstop().ptokens, 10_000_000 - cap);
    assert_eq!(f.ledger_u(), 10_000_000 - cap);

    // Full settle with a generous cap: repays exactly the backstop's own debt, never more.
    let s1 = f.m.get_bs_debt_shares(&f.usdc);
    let d1 = ceil(s1, pooled1, total1);
    let pt1 = ceil(d1, 1_000_000, RATE);
    let big = u128::MAX / 2;
    let args = (keeper.clone(), f.usdc.clone(), big, 0u128).into_val(&f.env);
    f.env.mock_auths(&[MockAuth {
        address: &keeper,
        invoke: &MockAuthInvoke { contract: &f.m.address, fn_name: "settle_backstop_debt", args, sub_invokes: &[] },
    }]);
    let repaid2 = f.m.settle_backstop_debt(&keeper, &f.usdc, &big, &0);
    assert_eq!(repaid2, d1);
    assert_eq!(f.m.get_bs_debt_shares(&f.usdc), 0);
    assert_eq!(f.ru.get_user_borrow_balance(&f.m.address), pooled1 - d1);
    assert_eq!(f.m.get_backstop().ptokens, 10_000_000 - cap - pt1);
    assert_eq!(f.ledger_u(), f.m.get_backstop().ptokens);
    assert_eq!(f.nav(), f.m.get_backstop().ptokens as i128);
    assert!(pt1 + cap >= pt - 1); // total cost within rounding of the one-shot cost
    // Nothing left: settle is a no-op.
    f.env.mock_all_auths();
    assert_eq!(f.m.settle_backstop_debt(&keeper, &f.usdc, &big, &0), 0);
}

#[test]
fn settle_xlm_strict_auth_and_unfreeze() {
    let f = setup();
    f.params(3_600, 3_600);
    f.seed(100_000_000);
    f.rx.set_borrow(&f.m.address, &40_000_000); // 4 XLM = $1
    f.bad_debt(&f.xlm, 40_000_000); // the backstop owns all XLM debt shares
    let keeper = Address::generate(&f.env);

    // A min_out above what the pool gives -> Slippage from the margin contract's own check.
    assert_eq!(err!(f.m.try_settle_backstop_debt(&keeper, &f.xlm, &u128::MAX, &u128::MAX)), Error::Slippage);
    // Pool far below the oracle -> the oracle floor rejects it.
    MockPoolClient::new(&f.env, &f.pool).init(&vec![&f.env, f.usdc.clone(), f.xlm.clone()], &3_000, &1_000);
    assert_eq!(err!(f.m.try_settle_backstop_debt(&keeper, &f.xlm, &u128::MAX, &0)), Error::Slippage);
    MockPoolClient::new(&f.env, &f.pool).init(&vec![&f.env, f.usdc.clone(), f.xlm.clone()], &3_992, &1_000);

    // Permissionless caller cannot pull more than $1 x (1 + 1 % max slippage) of pTokens.
    let usdc_need = ceil(ceil(40_000_000, P_XLM, PS), PS, P_USDC);
    let need_pt = ceil(ceil(usdc_need, 1_010_000, 1_000_000), 1_000_000, RATE);
    let got_usdc = need_pt * RATE / 1_000_000;
    let got_xlm = got_usdc * 3_992 / 1_000;
    let big = u128::MAX / 2;
    let args = (keeper.clone(), f.xlm.clone(), big, 0u128).into_val(&f.env);
    f.env.mock_auths(&[MockAuth {
        address: &keeper,
        invoke: &MockAuthInvoke { contract: &f.m.address, fn_name: "settle_backstop_debt", args, sub_invokes: &[] },
    }]);
    let repaid = f.m.settle_backstop_debt(&keeper, &f.xlm, &big, &0);
    f.env.mock_all_auths();
    assert_eq!(repaid, 40_000_000);
    assert_eq!(f.rx.get_user_borrow_balance(&f.m.address), 0);
    assert_eq!(f.m.get_bs_debt_shares(&f.xlm), 0);
    assert_eq!(f.m.get_backstop().ptokens, 100_000_000 - need_pt);
    assert_eq!(f.ledger_u(), 100_000_000 - need_pt);
    // Surplus XLM stays idle and untracked in the contract.
    let idle = token::TokenClient::new(&f.env, &f.xlm).balance(&f.m.address) as u128;
    assert_eq!(idle, got_xlm - 40_000_000);
    // Unfrozen.
    let bob = f.backer(5_000_000);
    assert!(f.m.backstop_deposit(&bob, &5_000_000, &0) > 0);
}

#[test]
fn settle_partial_xlm_when_others_share_the_debt() {
    let f = setup();
    f.seed(100_000_000);
    // Positions own 3 of 4 XLM debt shares; pooled debt 80 XLM with interest.
    f.rx.set_borrow(&f.m.address, &800_000_003);
    f.set_inst(Key::DebtShares(f.xlm.clone()), 600_000_000u128);
    f.bad_debt(&f.xlm, 200_000_000);
    let d = ceil(200_000_000, 800_000_003, 800_000_000);
    let keeper = Address::generate(&f.env);
    let repaid = f.m.settle_backstop_debt(&keeper, &f.xlm, &u128::MAX, &0);
    assert_eq!(repaid, d); // capped at the backstop's own debt
    assert_eq!(f.m.get_bs_debt_shares(&f.xlm), 0);
    let left = f.rx.get_user_borrow_balance(&f.m.address);
    assert_eq!(left, 800_000_003 - d);
    // Positions still owe exactly their share (within the round-up of d).
    assert!(left * 800_000_000 <= 800_000_003 * 600_000_000);
}

#[test]
fn auth_is_enforced() {
    let f = setup();
    let mallory = Address::generate(&f.env);
    assert_eq!(err!(f.m.try_set_backstop_params(&mallory, &BackstopParams { cooldown_s: 3_600, window_s: 3_600, min_deposit_pt: 1 })), Error::NotAdmin);

    // Seed with an exact auth tree: admin authorizes backstop_deposit and the nested pToken transfer.
    f.ru.mint_pt(&f.admin, &10_000_000);
    let amt = 10_000_000u128;
    f.env.mock_auths(&[MockAuth {
        address: &f.admin,
        invoke: &MockAuthInvoke {
            contract: &f.m.address,
            fn_name: "backstop_deposit",
            args: (f.admin.clone(), amt, 0u128).into_val(&f.env),
            sub_invokes: &[MockAuthInvoke {
                contract: &f.ru.address,
                fn_name: "transfer",
                args: (f.admin.clone(), f.m.address.clone(), amt as i128).into_val(&f.env),
                sub_invokes: &[],
            }],
        },
    }]);
    f.m.backstop_deposit(&f.admin, &amt, &0);

    // No auth at all -> rejected.
    let alice = f.backer(5_000_000);
    f.env.set_auths(&[]);
    assert!(f.m.try_backstop_deposit(&alice, &5_000_000, &0).is_err());
    assert!(f.m.try_backstop_request_withdraw(&f.admin, &1).is_err());
    // Mallory's authorization cannot act for the admin.
    f.env.mock_auths(&[MockAuth {
        address: &mallory,
        invoke: &MockAuthInvoke {
            contract: &f.m.address,
            fn_name: "backstop_request_withdraw",
            args: (f.admin.clone(), 1u128).into_val(&f.env),
            sub_invokes: &[],
        },
    }]);
    assert!(f.m.try_backstop_request_withdraw(&f.admin, &1).is_err());
    // Settle requires the caller's own auth.
    f.env.set_auths(&[]);
    assert!(f.m.try_settle_backstop_debt(&mallory, &f.usdc, &1, &0).is_err());
    // Recorded auths: backer is the root authorizer of its own request.
    f.env.mock_all_auths();
    f.m.backstop_request_withdraw(&f.admin, &1);
    let auths = f.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, f.admin);
}
