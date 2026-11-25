//! WP3 tests: pool check / headroom, required backstop, reconcile / Impaired.
//! Mocks: a receipt (pToken + borrow balances, exchange rate, pending interest that only
//! `update_interest` applies) and a controller whose `portfolio` mirrors the live
//! simple-peridottroller code (entered markets only, same step-by-step truncation).
#![cfg(test)]
extern crate std;

use crate::contract::{MarginController, MarginControllerArgs, MarginControllerClient};
use crate::headroom::{close_reserve, pool_check, require_not_impaired, MarginImpaired};
use crate::types::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{contractimpl, vec, Address, Env, Event as _};

// ---------------------------------------------------------------- mock receipt
mod mrec {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

    #[contracttype]
    #[derive(Clone)]
    pub enum RK {
        Und,
        Rate,
        PBal(Address),
        Debt(Address),
        Pending,  // (Address, u128): interest applied to that borrower by update_interest
        Accruals, // u32: update_interest call count
    }

    #[contract]
    pub struct MockReceipt;

    #[contractimpl]
    impl MockReceipt {
        pub fn init(env: Env, und: Address, rate: u128) {
            env.storage().instance().set(&RK::Und, &und);
            env.storage().instance().set(&RK::Rate, &rate);
        }
        pub fn set_pbal(env: Env, user: Address, v: u128) {
            env.storage().instance().set(&RK::PBal(user), &v);
        }
        pub fn set_debt(env: Env, user: Address, v: u128) {
            env.storage().instance().set(&RK::Debt(user), &v);
        }
        pub fn set_rate(env: Env, v: u128) {
            env.storage().instance().set(&RK::Rate, &v);
        }
        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            from.require_auth();
            let a = amount as u128;
            let fb: u128 = env.storage().instance().get(&RK::PBal(from.clone())).unwrap_or(0);
            assert!(fb >= a, "pt balance");
            env.storage().instance().set(&RK::PBal(from), &(fb - a));
            let tb: u128 = env.storage().instance().get(&RK::PBal(to.clone())).unwrap_or(0);
            env.storage().instance().set(&RK::PBal(to), &(tb + a));
        }
        pub fn set_pending(env: Env, user: Address, v: u128) {
            env.storage().instance().set(&RK::Pending, &(user, v));
        }
        pub fn accruals(env: Env) -> u32 {
            env.storage().instance().get(&RK::Accruals).unwrap_or(0)
        }
        pub fn get_underlying_token(env: Env) -> Address {
            env.storage().instance().get(&RK::Und).unwrap()
        }
        pub fn get_exchange_rate(env: Env) -> u128 {
            env.storage().instance().get(&RK::Rate).unwrap()
        }
        pub fn get_ptoken_balance(env: Env, user: Address) -> u128 {
            env.storage().instance().get(&RK::PBal(user)).unwrap_or(0)
        }
        pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {
            env.storage().instance().get(&RK::Debt(user)).unwrap_or(0)
        }
        pub fn update_interest(env: Env) {
            let n: u32 = env.storage().instance().get(&RK::Accruals).unwrap_or(0);
            env.storage().instance().set(&RK::Accruals, &(n + 1));
            let p: Option<(Address, u128)> = env.storage().instance().get(&RK::Pending);
            if let Some((u, v)) = p {
                let d: u128 = env.storage().instance().get(&RK::Debt(u.clone())).unwrap_or(0);
                env.storage().instance().set(&RK::Debt(u.clone()), &(d + v));
                env.storage().instance().set(&RK::Pending, &(u, 0u128));
            }
        }
    }
}

// ---------------------------------------------------------------- mock controller
mod mctrl {
    use super::mrec::MockReceiptClient;
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

    #[contracttype]
    #[derive(Clone)]
    pub enum CK {
        Markets(Address),
        Cf(Address),
        Price(Address),
    }

    #[contract]
    pub struct MockCtrl;

    #[contractimpl]
    impl MockCtrl {
        pub fn set_cf(env: Env, market: Address, cf: u128) {
            env.storage().instance().set(&CK::Cf(market), &cf);
        }
        pub fn set_price(env: Env, token: Address, p: u128, scale: u128) {
            env.storage().instance().set(&CK::Price(token), &(p, scale));
        }
        pub fn enter_market(env: Env, user: Address, market: Address) {
            let mut v: Vec<Address> = env.storage().instance().get(&CK::Markets(user.clone())).unwrap_or(Vec::new(&env));
            v.push_back(market);
            env.storage().instance().set(&CK::Markets(user), &v);
        }
        pub fn get_price_usd(env: Env, token: Address) -> Option<(u128, u128)> {
            env.storage().instance().get(&CK::Price(token))
        }
        pub fn get_market_cf(env: Env, market: Address) -> u128 {
            env.storage().instance().get(&CK::Cf(market)).unwrap_or(0)
        }
        #[allow(clippy::type_complexity)]
        /// Mirrors simple-peridottroller `portfolio` (contract.rs ~3650): entered markets only,
        /// coll = ((pbal*rate/1e6)*cf/1e6)*price/scale, debt_usd = debt*price/scale.
        pub fn portfolio(env: Env, user: Address) -> (Vec<(Address, u128, u128, u128, u128)>, (u128, u128)) {
            let mut rows = Vec::new(&env);
            let (mut ct, mut dt) = (0u128, 0u128);
            let markets: Vec<Address> = env.storage().instance().get(&CK::Markets(user.clone())).unwrap_or(Vec::new(&env));
            for m in markets.iter() {
                let r = MockReceiptClient::new(&env, &m);
                let pbal = r.get_ptoken_balance(&user);
                let debt = r.get_user_borrow_balance(&user);
                let token = r.get_underlying_token();
                let (mut coll_usd, mut debt_usd) = (0u128, 0u128);
                if pbal > 0 || debt > 0 {
                    let (price, scale): (u128, u128) = Self::get_price_usd(env.clone(), token).expect("price");
                    if pbal > 0 {
                        let rate = r.get_exchange_rate();
                        let cf = Self::get_market_cf(env.clone(), m.clone());
                        let underlying = pbal.saturating_mul(rate) / 1_000_000;
                        let discounted = underlying.saturating_mul(cf) / 1_000_000;
                        coll_usd = discounted.saturating_mul(price) / scale;
                    }
                    if debt > 0 {
                        debt_usd = debt.saturating_mul(price) / scale;
                    }
                }
                rows.push_back((m, pbal, debt, coll_usd, debt_usd));
                ct = ct.saturating_add(coll_usd);
                dt = dt.saturating_add(debt_usd);
            }
            (rows, (ct, dt))
        }
    }
}

use mctrl::{MockCtrl, MockCtrlClient};
use mrec::{MockReceipt, MockReceiptClient};

// ---------------------------------------------------------------- test-only entrypoints
#[contractimpl]
impl MarginController {
    pub fn t_check(env: Env, open: bool) {
        pool_check(&env, if open { Check::Open } else { Check::BackstopExit });
    }
    pub fn t_reserve(env: Env) -> u128 {
        let h: Headroom = env.storage().instance().get(&Key::Headroom).unwrap();
        close_reserve(&env, &h)
    }
    pub fn t_not_impaired(env: Env) {
        require_not_impaired(&env);
    }
}

// ---------------------------------------------------------------- fixture
const E7: u128 = 10_000_000;
const CF_XLM: u128 = 550_000;
const CF_USDC: u128 = 750_000;
const PX_XLM: u128 = 2_500_000; // $0.25, scale 1e7
const PX_USDC: u128 = 10_000_000;

struct F {
    env: Env,
    m: MarginControllerClient<'static>,
    id: Address,
    admin: Address,
    usdc: Address,
    xlm: Address,
    ru: MockReceiptClient<'static>,
    rx: MockReceiptClient<'static>,
    ctrl: MockCtrlClient<'static>,
}

fn setup() -> F {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let guardian = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    let ru_id = env.register(MockReceipt, ());
    let rx_id = env.register(MockReceipt, ());
    let ctrl_id = env.register(MockCtrl, ());
    let ru = MockReceiptClient::new(&env, &ru_id);
    let rx = MockReceiptClient::new(&env, &rx_id);
    ru.init(&usdc, &1_000_000);
    rx.init(&xlm, &1_000_000);
    let ctrl = MockCtrlClient::new(&env, &ctrl_id);
    ctrl.set_cf(&ru_id, &CF_USDC);
    ctrl.set_cf(&rx_id, &CF_XLM);
    ctrl.set_price(&usdc, &PX_USDC, &E7);
    ctrl.set_price(&xlm, &PX_XLM, &E7);
    let id = env.register(MarginController, (admin.clone(), ctrl_id.clone(), guardian, usdc.clone(), xlm.clone()));
    let m = MarginControllerClient::new(&env, &id);
    m.set_market(&admin, &usdc, &ru_id);
    m.set_market(&admin, &xlm, &rx_id);
    m.enter_markets(&admin, &vec![&env, usdc.clone(), xlm.clone()]);
    F { env, m, id, admin, usdc, xlm, ru, rx, ctrl }
}

fn hr(hf_target: u128, hf_stress: u128, stress: u128, reserve: u128) -> Headroom {
    Headroom { hf_target, hf_stress, stress, close_reserve_usd: reserve, coef_long: 0, coef_short: 0, coef_largest: 0 }
}
fn default_hr() -> Headroom {
    Headroom { hf_target: 1_250_000, hf_stress: 1_100_000, stress: 200_000, close_reserve_usd: 0, coef_long: 3_000_000, coef_short: 2_100_000, coef_largest: 4_600_000 }
}

impl F {
    fn put<V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(&self, k: Key, v: V) {
        self.env.as_contract(&self.id, || self.env.storage().instance().set(&k, &v));
    }
    fn get<V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(&self, k: Key) -> Option<V> {
        self.env.as_contract(&self.id, || self.env.storage().instance().get(&k))
    }
    /// Long book: 2,000 XLM held, $400 USDC debt, 1,000 USDC pT (backstop) on the account.
    /// C_x = 2000*0.55*0.25 = $275, C_u = $750, D = $400.
    fn long_book(&self) {
        self.rx.set_pbal(&self.id, &(2_000 * E7));
        self.ru.set_pbal(&self.id, &(1_000 * E7));
        self.ru.set_debt(&self.id, &(400 * E7));
    }
    /// Short book: 1,500 USDC pT held (500 positions + 1,000 backstop), 1,600 XLM debt ($400).
    fn short_book(&self) {
        self.ru.set_pbal(&self.id, &(1_500 * E7));
        self.rx.set_debt(&self.id, &(1_600 * E7));
    }
    fn caps(&self, per_position: u128, side_eq: u128) -> Caps {
        let _ = self;
        Caps { per_position_equity_usd: per_position, side_equity_usd: side_eq, side_oi_usd: 5 * side_eq, min_equity_usd: E7 }
    }
    fn check(&self, open: bool) -> Result<(), u32> {
        match self.m.try_t_check(&open) {
            Ok(Ok(())) => Ok(()),
            Err(Ok(e)) => Err(e.get_code()),
            other => panic!("unexpected {:?}", other),
        }
    }
}

/// Contract error as returned by try_* on entrypoints that panic_with_error.
fn se(e: Error) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(e as u32)
}

// ---------------------------------------------------------------- independent recomputation
/// Off-chain-style HF from raw balances, rates, CFs and prices in f64 (no portfolio, no
/// on-chain helper). Returns (spot, down, up).
fn hf_f64(xpt: u128, upt: u128, xdebt: u128, udebt: u128, rate_x: u128, rate_u: u128, stress: f64) -> (f64, f64, f64) {
    let px = PX_XLM as f64 / E7 as f64;
    let pu = PX_USDC as f64 / E7 as f64;
    let cx = xpt as f64 * (rate_x as f64 / 1e6) * (CF_XLM as f64 / 1e6) * px;
    let cu = upt as f64 * (rate_u as f64 / 1e6) * (CF_USDC as f64 / 1e6) * pu;
    let dx = xdebt as f64 * px;
    let du = udebt as f64 * pu;
    let hf = |k: f64| (cx * k + cu) / (dx * k + du);
    (hf(1.0), hf(1.0 - stress), hf(1.0 + stress))
}

// ================================================================ tests
#[test]
fn hf_target_boundary_open_and_exit() {
    let f = setup();
    f.long_book();
    // HF spot = 1025 / 400 = 2.5625 exactly.
    f.m.set_headroom(&f.admin, &hr(2_562_500, 1_000_000, 200_000, 1));
    assert_eq!(f.check(true), Ok(()));
    assert_eq!(f.check(false), Ok(()));
    f.m.set_headroom(&f.admin, &hr(2_562_501, 1_000_000, 200_000, 1));
    assert_eq!(f.check(true), Err(Error::PoolCheck as u32));
    assert_eq!(f.check(false), Err(Error::BackstopHeadroom as u32));
}

#[test]
fn stress_long_only_binds_at_down_endpoint() {
    let f = setup();
    f.long_book();
    // down: (275*0.8 + 750)/400 = 2.425; up: (275*1.2 + 750)/400 = 2.7.
    assert_eq!(f.m.pooled_health(), (2_562_500, 2_425_000, 2_700_000, 625 * E7));
    f.m.set_headroom(&f.admin, &hr(2_562_500, 2_425_000, 200_000, 1));
    assert_eq!(f.check(true), Ok(()));
    f.m.set_headroom(&f.admin, &hr(2_562_500, 2_425_001, 200_000, 1));
    assert_eq!(f.check(true), Err(Error::PoolCheck as u32));
    assert_eq!(f.check(false), Err(Error::BackstopHeadroom as u32));
    // Narrower stress (10 %): down HF = (247.5 + 750)/400 = 2.49375 -> 2.425001 now passes.
    f.m.set_headroom(&f.admin, &hr(2_562_500, 2_425_001, 100_000, 1));
    assert_eq!(f.check(true), Ok(()));
}

#[test]
fn stress_short_only_binds_at_up_endpoint() {
    let f = setup();
    f.short_book();
    // C_u = 1125; D_x = 400. spot 2.8125, down 1125/320 = 3.515625, up 1125/480 = 2.34375.
    assert_eq!(f.m.pooled_health(), (2_812_500, 3_515_625, 2_343_750, 725 * E7));
    f.m.set_headroom(&f.admin, &hr(2_812_500, 2_343_750, 200_000, 1));
    assert_eq!(f.check(true), Ok(()));
    f.m.set_headroom(&f.admin, &hr(2_812_500, 2_343_751, 200_000, 1));
    assert_eq!(f.check(true), Err(Error::PoolCheck as u32));
    assert_eq!(f.check(false), Err(Error::BackstopHeadroom as u32));
}

#[test]
fn free_liquidity_boundary_explicit_reserve() {
    let f = setup();
    f.long_book();
    f.m.set_headroom(&f.admin, &hr(1_250_000, 1_100_000, 200_000, 625 * E7));
    assert_eq!(f.check(true), Ok(()));
    assert_eq!(f.check(false), Ok(()));
    f.m.set_headroom(&f.admin, &hr(1_250_000, 1_100_000, 200_000, 625 * E7 + 1));
    assert_eq!(f.check(true), Err(Error::PoolCheck as u32));
    assert_eq!(f.check(false), Err(Error::BackstopHeadroom as u32));
}

#[test]
fn derived_close_reserve_and_no_debt_still_needs_free_liquidity() {
    let f = setup();
    let caps = f.caps(25 * E7, 100 * E7);
    f.put(Key::Caps(f.xlm.clone()), caps.clone());
    f.put(Key::Caps(f.usdc.clone()), caps);
    f.put(Key::Risk(f.xlm.clone()), RiskParams { max_leverage: 5_000_000 });
    f.put(Key::Risk(f.usdc.clone()), RiskParams { max_leverage: 5_000_000 });
    // reserve = 0 -> derive: maxdebt = 25*4 = $100; max(0.55*100, 0.75*100) = 75; *1.01 = $75.75.
    f.m.set_headroom(&f.admin, &hr(5_000_000, 5_000_000, 500_000, 0));
    assert_eq!(f.m.t_reserve(), 757_500_000);
    // No debt: every HF check passes (even HF target 5x) but free liquidity must cover $75.75.
    f.ru.set_pbal(&f.id, &1_010_000_000); // C_u = 757_500_000
    assert_eq!(f.m.pooled_health(), (u128::MAX, u128::MAX, u128::MAX, 757_500_000));
    assert_eq!(f.check(false), Ok(()));
    f.ru.set_pbal(&f.id, &1_009_999_999); // C_u = 757_499_999
    assert_eq!(f.check(false), Err(Error::BackstopHeadroom as u32));
    // Lower USDC CF moves the binding side to the long: 0.55*100*1.01 = 55.55 vs 0.5*100*1.01.
    f.ctrl.set_cf(&f.ru.address, &500_000);
    assert_eq!(f.m.t_reserve(), 555_500_000);
    // One side unconfigured: only the other counts.
    f.env.as_contract(&f.id, || f.env.storage().instance().remove(&Key::Risk(f.xlm.clone())));
    assert_eq!(f.m.t_reserve(), 505_000_000);
    // Explicit reserve overrides derivation.
    f.m.set_headroom(&f.admin, &hr(5_000_000, 5_000_000, 500_000, 7));
    assert_eq!(f.m.t_reserve(), 7);
}

#[test]
fn pending_exit_excluded_for_open_only() {
    let f = setup();
    f.long_book(); // pbal_u = 1000, C_u = 750, D = 400, C_x = 275
    // HF 2.0 needs C >= 800 -> C_u_eff >= 525 -> pending <= 300 USDC pT.
    f.m.set_headroom(&f.admin, &hr(2_000_000, 1_000_000, 200_000, 1));
    // shares 1e7, ptokens 1000 pT: pending_pt = pending_shares * 1000e7 / 1e7.
    f.put(Key::Bs, Backstop { shares: E7, ptokens: 1_000 * E7, pending_shares: 3_000_000 });
    assert_eq!(f.check(true), Ok(()));
    f.put(Key::Bs, Backstop { shares: E7, ptokens: 1_000 * E7, pending_shares: 3_000_001 });
    assert_eq!(f.check(true), Err(Error::PoolCheck as u32));
    assert_eq!(f.check(false), Ok(())); // exits see the actual pooled account
    // pending >= pbal -> USDC collateral fully excluded.
    f.put(Key::Bs, Backstop { shares: E7, ptokens: 1_000 * E7, pending_shares: 2 * E7 });
    f.m.set_headroom(&f.admin, &hr(1_000_000, 1_000_000, 0, 1));
    assert_eq!(f.check(true), Err(Error::PoolCheck as u32)); // 275 < 400
    assert_eq!(f.check(false), Ok(()));
    // pooled_health ignores pending exits.
    assert_eq!(f.m.pooled_health().0, 2_562_500);
}

#[test]
fn required_backstop_pilot_matches_spec() {
    let f = setup();
    f.m.set_headroom(&f.admin, &default_hr());
    let caps = f.caps(25 * E7, 100 * E7);
    f.put(Key::Caps(f.xlm.clone()), caps.clone());
    f.put(Key::Caps(f.usdc.clone()), caps);
    // Empty book: only the largest-position term, 4.6 * 25 = $115.
    assert_eq!(f.m.required_backstop(), 115 * E7);
    // SPEC §5 pilot, full sides: 3.0*100 + 2.1*100 + 4.6*25 = $625.
    let side = SideTotals { equity_usd: 100 * E7, oi_usd: 500 * E7, open_positions: 4 };
    f.put(Key::Side(f.xlm.clone()), side.clone());
    f.put(Key::Side(f.usdc.clone()), side);
    assert_eq!(f.m.required_backstop(), 625 * E7);
    // Larger cap on one side drives the largest term: 4.6 * 500 = 2300 -> 300+210+2300.
    f.put(Key::Caps(f.usdc.clone()), f.caps(500 * E7, 2_000 * E7));
    assert_eq!(f.m.required_backstop(), 2_810 * E7);
}

#[test]
fn open_requires_backstop_size() {
    let f = setup();
    f.long_book();
    f.m.set_headroom(&f.admin, &default_hr());
    let caps = f.caps(25 * E7, 100 * E7);
    f.put(Key::Caps(f.xlm.clone()), caps.clone());
    f.put(Key::Caps(f.usdc.clone()), caps);
    f.put(Key::Risk(f.xlm.clone()), RiskParams { max_leverage: 5_000_000 });
    f.put(Key::Risk(f.usdc.clone()), RiskParams { max_leverage: 5_000_000 });
    let side = SideTotals { equity_usd: 100 * E7, oi_usd: 500 * E7, open_positions: 4 };
    f.put(Key::Side(f.xlm.clone()), side.clone());
    f.put(Key::Side(f.usdc.clone()), side);
    // Backstop value (backstop::value_usd: nav_pt * rate * USDC price) exactly $625 -> passes.
    f.put(Key::Bs, Backstop { shares: E7, ptokens: 625 * E7, pending_shares: 0 });
    assert_eq!(f.check(true), Ok(()));
    f.put(Key::Bs, Backstop { shares: E7, ptokens: 625 * E7 - 1, pending_shares: 0 });
    assert_eq!(f.check(true), Err(Error::BackstopTooSmall as u32));
    assert_eq!(f.check(false), Ok(())); // exit check does not size the backstop
    // Exchange rate above 1 lifts the value back over the requirement.
    f.ru.set_rate(&1_000_001);
    assert_eq!(f.check(true), Ok(()));
}

#[test]
fn pool_check_accrues_interest_before_portfolio() {
    let f = setup();
    f.long_book();
    f.m.set_headroom(&f.admin, &hr(2_562_500, 1_000_000, 200_000, 1));
    let (ax, au) = (f.rx.accruals(), f.ru.accruals());
    assert_eq!(f.check(true), Ok(()));
    assert!(f.rx.accruals() > ax && f.ru.accruals() > au);
    // One raw unit of interest visible only after update_interest flips the exact boundary.
    f.ru.set_pending(&f.id, &1);
    assert_eq!(f.check(true), Err(Error::PoolCheck as u32));
    // The failed call rolled back: the lagging view still shows 400, pending still 1.
    assert_eq!(f.ru.get_user_borrow_balance(&f.id), 400 * E7);
    assert_eq!(f.m.pooled_health().0, (1025 * E7 * 1_000_000) / (400 * E7 + 1));
}

#[test]
fn hf_matches_independent_recomputation() {
    let f = setup();
    let stress = 0.2;
    // (xpt, upt, xdebt, udebt, rate_x, rate_u)
    let books: [(u128, u128, u128, u128, u128, u128); 7] = [
        (2_000 * E7, 1_000 * E7, 0, 400 * E7, 1_000_000, 1_000_000),
        (0, 1_500 * E7, 1_600 * E7, 0, 1_000_000, 1_000_000),
        (1_234_567_891, 9_876_543_210, 777_777_777, 333_333_333, 1_013_456, 1_027_891),
        (3_100 * E7 + 7, 812 * E7 + 3, 900 * E7, 210 * E7 + 11, 1_000_123, 1_049_999),
        (5 * E7, 1_000 * E7, 3 * E7 + 1, 1, 1_200_000, 1_000_001),
        // spot HF ~1.17: between hf_stress and hf_target -> fails
        (0, 1_000 * E7, 0, 640 * E7, 1_000_000, 1_003_000),
        // spot ~1.28 passes, XLM -20 % ~1.02 fails (stress endpoint only)
        (8_000 * E7, 0, 0, 860 * E7, 1_001_000, 1_000_000),
    ];
    f.m.set_headroom(&f.admin, &hr(1_250_000, 1_100_000, 200_000, 1));
    let (mut passed, mut failed) = (0, 0);
    for (xpt, upt, xd, ud, rx_rate, ru_rate) in books {
        f.rx.set_pbal(&f.id, &xpt);
        f.ru.set_pbal(&f.id, &upt);
        f.rx.set_debt(&f.id, &xd);
        f.ru.set_debt(&f.id, &ud);
        f.rx.set_rate(&rx_rate);
        f.ru.set_rate(&ru_rate);
        let (s, d, u, _) = f.m.pooled_health();
        let (es, ed, eu) = hf_f64(xpt, upt, xd, ud, rx_rate, ru_rate, stress);
        for (got, exp) in [(s, es), (d, ed), (u, eu)] {
            let got = got as f64 / 1e6;
            // portfolio truncates each row to 1 raw usd7 unit; allow that plus HF floor.
            assert!(((got - exp) / exp).abs() < 1e-5, "hf {got} vs {exp}");
        }
        // pool_check agrees with the independent minimum over the stress interval.
        let pass = es >= 1.25 && ed.min(eu) >= 1.10;
        let res = f.check(false);
        if (es - 1.25).abs() > 1e-4 && (ed.min(eu) - 1.10).abs() > 1e-4 {
            assert_eq!(res.is_ok(), pass, "book {xpt} {upt} {xd} {ud}");
            if pass { passed += 1 } else { failed += 1 }
        }
    }
    assert!(passed >= 3 && failed >= 2, "passed {passed} failed {failed}");
}

#[test]
fn reconcile_deficit_sets_impaired_and_pauses() {
    let f = setup();
    f.long_book();
    f.put(Key::Ledger(f.usdc.clone()), 1_000 * E7);
    f.put(Key::Ledger(f.xlm.clone()), 2_000 * E7);
    f.m.set_opens_paused(&f.admin, &false);
    let k = soroban_sdk::Address::generate(&f.env);
    assert!(!f.m.reconcile(&k));
    // Donation (surplus) is ignored.
    f.ru.set_pbal(&f.id, &(1_000 * E7 + 5));
    assert!(!f.m.reconcile(&k));
    assert!(!f.m.is_impaired() && !f.m.opens_paused());
    f.m.t_not_impaired();
    // XLM deficit of one raw pToken (e.g. a controller seizure of the pooled account).
    f.rx.set_pbal(&f.id, &(2_000 * E7 - 1));
    assert!(f.m.reconcile(&k));
    let ev = MarginImpaired { caller: k.clone(), usdc_ledger: 1_000 * E7, usdc_balance: 1_000 * E7 + 5, xlm_ledger: 2_000 * E7, xlm_balance: 2_000 * E7 - 1 };
    let evs = f.env.events().all().filter_by_contract(&f.id);
    assert_eq!(evs, [ev.to_xdr(&f.env, &f.id)]);
    // Single-tuple topic (Symbol("margin_impaired"),), independent of the struct encoding.
    let soroban_sdk::xdr::ContractEventBody::V0(body) = &evs.events()[0].body;
    let want: soroban_sdk::xdr::ScVal = (&soroban_sdk::Symbol::new(&f.env, "margin_impaired")).try_into().unwrap();
    assert_eq!(body.topics.len(), 1);
    assert_eq!(body.topics[0], want);
    assert!(f.m.is_impaired() && f.m.opens_paused());
    assert_eq!(f.m.try_t_not_impaired(), Err(Ok(se(Error::Impaired))));
    // Already impaired: a further deficit returns false and emits nothing (keeper sends only
    // on true, so it never resends every cycle).
    f.rx.set_pbal(&f.id, &(2_000 * E7 - 2));
    assert!(!f.m.reconcile(&k));
    assert_eq!(f.env.events().all().filter_by_contract(&f.id).events().len(), 0);
    // Never cleared by reconcile, even after the balance returns.
    f.rx.set_pbal(&f.id, &(2_000 * E7));
    assert!(!f.m.reconcile(&k));
    assert!(f.m.is_impaired());
}

#[test]
fn resolve_impairment_books_charges() {
    let f = setup();
    // USDC: positions 1000 + backstop 100 = Ledger 1100; balance 5 short.
    f.put(Key::Bs, Backstop { shares: E7, ptokens: 100 * E7, pending_shares: 0 });
    f.put(Key::Ledger(f.usdc.clone()), 1_100 * E7);
    f.put(Key::Ledger(f.xlm.clone()), 2_000 * E7);
    f.ru.set_pbal(&f.id, &(1_100 * E7 - 5));
    f.rx.set_pbal(&f.id, &(2_000 * E7 - 3));
    let k = Address::generate(&f.env);
    assert!(f.m.reconcile(&k));
    // Non-admin, and a charge larger than the backstop, are refused.
    let other = Address::generate(&f.env);
    assert_eq!(f.m.try_admin_resolve_impairment(&other, &1, &0), Err(Ok(se(Error::NotAdmin))));
    assert_eq!(f.m.try_admin_resolve_impairment(&f.admin, &(100 * E7 + 1), &0), Err(Ok(se(Error::BadAmount))));
    // Partial: USDC still 1 short, XLM 3 short -> stays impaired.
    f.m.admin_resolve_impairment(&f.admin, &4, &0);
    assert!(f.m.is_impaired());
    let bs: Backstop = f.get(Key::Bs).unwrap();
    assert_eq!(bs.ptokens, 100 * E7 - 4);
    assert_eq!(f.get::<u128>(Key::Ledger(f.usdc.clone())).unwrap(), 1_100 * E7 - 4);
    // USDC fixed but XLM still short -> stays impaired.
    f.m.admin_resolve_impairment(&f.admin, &1, &0);
    assert!(f.m.is_impaired());
    // XLM: the admin restores the 3 missing XLM pTokens; Ledger(XLM) is unchanged, so every
    // XLM position and claim can still exit in full (no Ledger-only write-down).
    f.rx.set_pbal(&f.admin, &10);
    assert!(f.m.try_admin_resolve_impairment(&f.admin, &0, &11).is_err()); // admin holds only 10
    f.m.admin_resolve_impairment(&f.admin, &0, &3);
    assert!(!f.m.is_impaired());
    assert_eq!(f.get::<u128>(Key::Ledger(f.xlm.clone())).unwrap(), 2_000 * E7);
    assert_eq!(f.rx.get_ptoken_balance(&f.id), 2_000 * E7);
    assert_eq!(f.rx.get_ptoken_balance(&f.admin), 7);
    let bs: Backstop = f.get(Key::Bs).unwrap();
    assert_eq!(bs.ptokens, 100 * E7 - 5);
    assert!(f.m.opens_paused()); // admin must unpause explicitly
    f.m.t_not_impaired();
}

#[test]
fn set_headroom_bounds_and_roundtrip() {
    let f = setup();
    assert_eq!(f.m.get_headroom(), default_hr());
    let bad = [hr(1_250_000, 999_999, 200_000, 0), hr(1_099_999, 1_100_000, 200_000, 0), hr(1_250_000, 1_100_000, 500_001, 0)];
    for h in bad {
        assert_eq!(f.m.try_set_headroom(&f.admin, &h), Err(Ok(se(Error::BadParams))));
    }
    let ok = hr(1_000_000, 1_000_000, 500_000, 42);
    f.m.set_headroom(&f.admin, &ok);
    assert_eq!(f.m.get_headroom(), ok);
    let other = Address::generate(&f.env);
    assert_eq!(f.m.try_set_headroom(&other, &ok), Err(Ok(se(Error::NotAdmin))));
}

#[test]
fn default_parameters_accept_pilot_long_book() {
    // Realistic pilot: 4 long positions of $25 at 5x -> $500 XLM held (2,000 XLM), $400 USDC
    // debt, $750 backstop seed. Default headroom, derived close reserve.
    let f = setup();
    f.m.set_headroom(&f.admin, &default_hr());
    let caps = f.caps(25 * E7, 100 * E7);
    f.put(Key::Caps(f.xlm.clone()), caps.clone());
    f.put(Key::Caps(f.usdc.clone()), caps);
    f.put(Key::Risk(f.xlm.clone()), RiskParams { max_leverage: 5_000_000 });
    f.put(Key::Risk(f.usdc.clone()), RiskParams { max_leverage: 5_000_000 });
    f.put(Key::Side(f.xlm.clone()), SideTotals { equity_usd: 100 * E7, oi_usd: 500 * E7, open_positions: 4 });
    f.rx.set_pbal(&f.id, &(2_000 * E7));
    f.ru.set_pbal(&f.id, &(750 * E7));
    f.ru.set_debt(&f.id, &(400 * E7));
    f.put(Key::Bs, Backstop { shares: E7, ptokens: 750 * E7, pending_shares: 0 });
    // C = 275 + 562.5 = 837.5; D = 400 -> HF 2.09375; down (220+562.5)/400 = 1.95625.
    assert_eq!(f.m.pooled_health(), (2_093_750, 1_956_250, 2_231_250, 4_375_000_000));
    assert_eq!(f.m.required_backstop(), 415 * E7); // 300 + 0 + 115
    assert_eq!(f.check(true), Ok(()));
}
