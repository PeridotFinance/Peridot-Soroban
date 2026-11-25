//! WP2 native tests: positions v2 against simple mock receipt / controller / pool contracts.
//! The mock controller health-checks the pooled account with CFs, prices and exchange rates
//! (receipt passes its own row, so there is no re-entry). Backstop and pool check are the real
//! modules (WP4: no shims); the mocks are shared with test_integration.rs.
extern crate std;
use crate::{Backstop, Caps, Error, Fees, Key, MarginController, MarginControllerClient, Position};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{vec, Address, Env};
use std::vec::Vec as SVec;

pub(crate) const U: u128 = 10_000_000; // one unit of a 7-decimal token / one usd7 dollar
pub(crate) const PS: u128 = 10_000_000; // mock oracle price scale
pub(crate) const YEAR: u64 = 31_536_000;

pub(crate) mod mrec {
    use super::mctrl::MockCtrlClient;
    use soroban_sdk::{contract, contractimpl, contracttype, token::TokenClient, Address, Env};
    #[contracttype]
    pub enum K {
        Und,
        Ctrl,
        Rate,
        Pt(Address),
        Debt(Address),
    }
    #[contract]
    pub struct MockReceipt;
    fn g(env: &Env, k: &K) -> u128 {
        env.storage().instance().get(k).unwrap_or(0)
    }
    fn s(env: &Env, k: &K, v: u128) {
        env.storage().instance().set(k, &v);
    }
    fn und(env: &Env) -> Address {
        env.storage().instance().get(&K::Und).unwrap()
    }
    fn health(env: &Env, user: &Address) {
        let c: Address = env.storage().instance().get(&K::Ctrl).unwrap();
        MockCtrlClient::new(env, &c).check(
            user,
            &env.current_contract_address(),
            &und(env),
            &g(env, &K::Pt(user.clone())),
            &g(env, &K::Debt(user.clone())),
            &g(env, &K::Rate),
        );
    }
    #[contractimpl]
    impl MockReceipt {
        pub fn __constructor(env: Env, underlying: Address, ctrl: Address) {
            env.storage().instance().set(&K::Und, &underlying);
            env.storage().instance().set(&K::Ctrl, &ctrl);
            s(&env, &K::Rate, 1_000_000);
        }
        pub fn deposit(env: Env, user: Address, amount: u128) {
            user.require_auth();
            TokenClient::new(&env, &und(&env)).transfer(
                &user,
                &env.current_contract_address(),
                &(amount as i128),
            );
            let pt = amount * 1_000_000 / g(&env, &K::Rate);
            assert!(pt > 0, "receipt: zero mint");
            let k = K::Pt(user);
            s(&env, &k, g(&env, &k) + pt);
        }
        pub fn withdraw(env: Env, user: Address, ptoken_amount: u128) {
            user.require_auth();
            let k = K::Pt(user.clone());
            let b = g(&env, &k);
            assert!(b >= ptoken_amount, "receipt: balance");
            s(&env, &k, b - ptoken_amount);
            health(&env, &user);
            let amt = ptoken_amount * g(&env, &K::Rate) / 1_000_000;
            TokenClient::new(&env, &und(&env)).transfer(
                &env.current_contract_address(),
                &user,
                &(amt as i128),
            );
        }
        pub fn borrow(env: Env, user: Address, amount: u128) {
            user.require_auth();
            let k = K::Debt(user.clone());
            s(&env, &k, g(&env, &k) + amount);
            health(&env, &user);
            TokenClient::new(&env, &und(&env)).transfer(
                &env.current_contract_address(),
                &user,
                &(amount as i128),
            );
        }
        pub fn repay(env: Env, user: Address, amount: u128) {
            user.require_auth();
            let k = K::Debt(user.clone());
            let d = g(&env, &k);
            let x = amount.min(d);
            TokenClient::new(&env, &und(&env)).transfer(
                &user,
                &env.current_contract_address(),
                &(x as i128),
            );
            s(&env, &k, d - x);
        }
        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            from.require_auth();
            let a = amount as u128;
            let kf = K::Pt(from.clone());
            let b = g(&env, &kf);
            assert!(b >= a, "receipt: balance");
            s(&env, &kf, b - a);
            let kt = K::Pt(to);
            s(&env, &kt, g(&env, &kt) + a);
            health(&env, &from);
        }
        pub fn get_underlying_token(env: Env) -> Address {
            und(&env)
        }
        pub fn get_exchange_rate(env: Env) -> u128 {
            g(&env, &K::Rate)
        }
        pub fn get_ptoken_balance(env: Env, user: Address) -> u128 {
            g(&env, &K::Pt(user))
        }
        pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {
            g(&env, &K::Debt(user))
        }
        pub fn update_interest(_env: Env) {}
        // ---- test controls
        pub fn set_rate(env: Env, rate: u128) {
            s(&env, &K::Rate, rate);
        }
        pub fn add_debt(env: Env, user: Address, amount: u128) {
            let k = K::Debt(user);
            s(&env, &k, g(&env, &k) + amount);
        }
        pub fn mint_pt(env: Env, to: Address, amount: u128) {
            let k = K::Pt(to);
            s(&env, &k, g(&env, &k) + amount);
        }
        /// External seizure (e.g. the cash controller liquidating `from`): moves pTokens
        /// without `from`'s auth or a health check.
        pub fn seize(env: Env, from: Address, to: Address, amount: u128) {
            let kf = K::Pt(from);
            s(&env, &kf, g(&env, &kf) - amount);
            let kt = K::Pt(to);
            s(&env, &kt, g(&env, &kt) + amount);
        }
        /// External repayment of `user`'s debt (the seizing liquidator's repay).
        pub fn sub_debt(env: Env, user: Address, amount: u128) {
            let k = K::Debt(user);
            s(&env, &k, g(&env, &k) - amount);
        }
    }
}

pub(crate) mod mctrl {
    use super::mrec::MockReceiptClient;
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};
    #[contracttype]
    pub enum K {
        Price(Address),
        Markets,
        Trap,
    }
    #[contract]
    pub struct MockCtrl;
    fn markets(env: &Env) -> Vec<(Address, u128)> {
        env.storage()
            .instance()
            .get(&K::Markets)
            .unwrap_or(Vec::new(env))
    }
    fn px(env: &Env, t: &Address) -> u128 {
        env.storage()
            .instance()
            .get(&K::Price(t.clone()))
            .unwrap_or(0)
    }
    #[contractimpl]
    impl MockCtrl {
        pub fn set_price(env: Env, token: Address, p: u128) {
            env.storage().instance().set(&K::Price(token), &p);
        }
        pub fn set_trap(env: Env, on: bool) {
            env.storage().instance().set(&K::Trap, &on);
        }
        pub fn add_market(env: Env, market: Address, cf: u128) {
            let mut m = markets(&env);
            m.push_back((market, cf));
            env.storage().instance().set(&K::Markets, &m);
        }
        pub fn get_price_usd(env: Env, token: Address) -> Option<(u128, u128)> {
            if env.storage().instance().get(&K::Trap).unwrap_or(false) {
                panic!("oracle trap");
            }
            let p = px(&env, &token);
            if p == 0 {
                None
            } else {
                Some((p, super::PS))
            }
        }
        pub fn enter_market(_env: Env, _user: Address, _market: Address) {}
        pub fn get_market_cf(env: Env, market: Address) -> u128 {
            for (m, cf) in markets(&env).iter() {
                if m == market {
                    return cf;
                }
            }
            0
        }
        /// Live `portfolio` shape and truncation order (simple-peridottroller):
        /// coll = ((pbal * rate / 1e6) * cf / 1e6) * price / scale, debt_usd = debt * price / scale.
        pub fn portfolio(
            env: Env,
            user: Address,
        ) -> (Vec<(Address, u128, u128, u128, u128)>, (u128, u128)) {
            rows(&env, &user, None)
        }
        pub fn account_liquidity(env: Env, user: Address) -> (u128, u128) {
            let (_, (c, d)) = rows(&env, &user, None);
            (c.saturating_sub(d), d.saturating_sub(c))
        }
        /// Health check of `user`; the calling receipt passes its own row (no re-entry).
        /// Same rows as `portfolio`, so the pool check and the receipt health check agree.
        pub fn check(
            env: Env,
            user: Address,
            market: Address,
            und: Address,
            pt: u128,
            debt: u128,
            rate: u128,
        ) {
            let (_, (coll, dv)) = rows(&env, &user, Some((market, und, pt, debt, rate)));
            assert!(dv <= coll, "ctrl: insufficient liquidity");
        }
    }
    #[allow(clippy::type_complexity)]
    fn rows(
        env: &Env,
        user: &Address,
        own: Option<(Address, Address, u128, u128, u128)>,
    ) -> (Vec<(Address, u128, u128, u128, u128)>, (u128, u128)) {
        let mut out = Vec::new(env);
        let (mut ct, mut dt) = (0u128, 0u128);
        for (m, cf) in markets(env).iter() {
            let (u, pbal, debt, rate) = match &own {
                Some((om, ou, p_, d_, r_)) if *om == m => (ou.clone(), *p_, *d_, *r_),
                _ => {
                    let c = MockReceiptClient::new(env, &m);
                    (
                        c.get_underlying_token(),
                        c.get_ptoken_balance(user),
                        c.get_user_borrow_balance(user),
                        c.get_exchange_rate(),
                    )
                }
            };
            let p = px(env, &u);
            let coll = pbal * rate / 1_000_000 * cf / 1_000_000 * p / super::PS;
            let dusd = debt * p / super::PS;
            out.push_back((m, pbal, debt, coll, dusd));
            ct += coll;
            dt += dusd;
        }
        (out, (ct, dt))
    }
}

pub(crate) mod mpool {
    use super::mctrl::MockCtrlClient;
    use soroban_sdk::{
        contract, contractimpl, contracttype, token::TokenClient, Address, Env, Vec,
    };
    #[contracttype]
    pub enum K {
        Toks,
        Ctrl,
        Fill,
    }
    #[contract]
    pub struct MockPool;
    #[contractimpl]
    impl MockPool {
        pub fn __constructor(env: Env, t0: Address, t1: Address, ctrl: Address) {
            let mut v = Vec::new(&env);
            v.push_back(t0);
            v.push_back(t1);
            env.storage().instance().set(&K::Toks, &v);
            env.storage().instance().set(&K::Ctrl, &ctrl);
            env.storage().instance().set(&K::Fill, &1_000_000u128);
        }
        pub fn set_fill(env: Env, fill: u128) {
            env.storage().instance().set(&K::Fill, &fill);
        }
        pub fn get_tokens(env: Env) -> Vec<Address> {
            env.storage().instance().get(&K::Toks).unwrap()
        }
        /// Aquarius shape: no require_auth on swap; the input transfer carries the caller's auth.
        pub fn swap(
            env: Env,
            user: Address,
            in_idx: u32,
            out_idx: u32,
            amount_in: u128,
            out_min: u128,
        ) -> u128 {
            let toks = Self::get_tokens(env.clone());
            let (ti, to) = (toks.get(in_idx).unwrap(), toks.get(out_idx).unwrap());
            let me = env.current_contract_address();
            TokenClient::new(&env, &ti).transfer(&user, &me, &(amount_in as i128));
            let c: Address = env.storage().instance().get(&K::Ctrl).unwrap();
            let cc = MockCtrlClient::new(&env, &c);
            let (pi, po) = (
                cc.get_price_usd(&ti).unwrap().0,
                cc.get_price_usd(&to).unwrap().0,
            );
            let fill: u128 = env.storage().instance().get(&K::Fill).unwrap();
            let out = amount_in * pi / po * fill / 1_000_000;
            assert!(out >= out_min, "pool: out_min");
            TokenClient::new(&env, &to).transfer(&me, &user, &(out as i128));
            out
        }
    }
}

pub(crate) use mctrl::{MockCtrl, MockCtrlClient};
pub(crate) use mpool::{MockPool, MockPoolClient};
pub(crate) use mrec::{MockReceipt, MockReceiptClient};

pub(crate) struct T {
    pub(crate) env: Env,
    pub(crate) m: MarginControllerClient<'static>,
    pub(crate) ctrl: MockCtrlClient<'static>,
    pub(crate) ru: MockReceiptClient<'static>,
    pub(crate) rx: MockReceiptClient<'static>,
    pub(crate) pool: MockPoolClient<'static>,
    pub(crate) usdc: Address,
    pub(crate) xlm: Address,
    pub(crate) admin: Address,
    pub(crate) user: Address,
    pub(crate) liq: Address,
}

pub(crate) fn caps(per: u128, side: u128, oi: u128) -> Caps {
    Caps {
        per_position_equity_usd: per * U,
        side_equity_usd: side * U,
        side_oi_usd: oi * U,
        min_equity_usd: U,
    }
}

impl T {
    pub(crate) fn new() -> T {
        let env = Env::default();
        env.mock_all_auths();
        // Long sequences in one Env; resource measurement is WP5 (fork), not these unit tests.
        env.cost_estimate().budget().reset_unlimited();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let guardian = Address::generate(&env);
        let usdc = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let xlm = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let ctrl_a = env.register(MockCtrl, ());
        let ctrl = MockCtrlClient::new(&env, &ctrl_a);
        ctrl.set_price(&usdc, &PS);
        ctrl.set_price(&xlm, &2_500_000); // $0.25
        let ru_a = env.register(MockReceipt, (usdc.clone(), ctrl_a.clone()));
        let rx_a = env.register(MockReceipt, (xlm.clone(), ctrl_a.clone()));
        ctrl.add_market(&ru_a, &750_000);
        ctrl.add_market(&rx_a, &550_000);
        let pool_a = env.register(MockPool, (usdc.clone(), xlm.clone(), ctrl_a.clone()));
        for t in [&usdc, &xlm] {
            let sac = StellarAssetClient::new(&env, t);
            sac.mint(&ru_a, &(1_000_000_000 * U as i128));
            sac.mint(&rx_a, &(1_000_000_000 * U as i128));
            sac.mint(&pool_a, &(1_000_000_000 * U as i128));
        }
        let m_a = env.register(
            MarginController,
            (
                admin.clone(),
                ctrl_a.clone(),
                guardian,
                usdc.clone(),
                xlm.clone(),
            ),
        );
        let m = MarginControllerClient::new(&env, &m_a);
        m.set_market(&admin, &usdc, &ru_a);
        m.set_market(&admin, &xlm, &rx_a);
        m.set_pool(&admin, &pool_a);
        m.enter_markets(&admin, &vec![&env, usdc.clone(), xlm.clone()]);
        m.set_risk(&admin, &xlm, &5_000_000);
        m.set_risk(&admin, &usdc, &5_000_000);
        m.set_caps(&admin, &xlm, &caps(25, 100, 500));
        m.set_caps(&admin, &usdc, &caps(25, 100, 500));
        m.set_opens_paused(&admin, &false);
        let user = Address::generate(&env);
        let liq = Address::generate(&env);
        for a in [&user, &liq] {
            StellarAssetClient::new(&env, &usdc).mint(a, &(10_000_000 * U as i128));
            StellarAssetClient::new(&env, &xlm).mint(a, &(10_000_000 * U as i128));
        }
        let t = T {
            env: env.clone(),
            m,
            ctrl,
            ru: MockReceiptClient::new(&env, &ru_a),
            rx: MockReceiptClient::new(&env, &rx_a),
            pool: MockPoolClient::new(&env, &pool_a),
            usdc,
            xlm,
            admin,
            user,
            liq,
        };
        t.seed_backstop(750 * U);
        t
    }
    /// Treasury seed through the real entrypoint: the admin supplies USDC to the receipt,
    /// then deposits the minted pTokens into the backstop (the first deposit adds dead shares).
    pub(crate) fn seed_backstop(&self, pt: u128) -> u128 {
        self.backer_deposit(&self.admin, pt)
    }
    /// `who` supplies `amount` USDC to the receipt and deposits the minted pTokens.
    pub(crate) fn backer_deposit(&self, who: &Address, amount: u128) -> u128 {
        StellarAssetClient::new(&self.env, &self.usdc).mint(who, &(amount as i128));
        let before = self.ru.get_ptoken_balance(who);
        self.ru.deposit(who, &amount);
        let pt = self.ru.get_ptoken_balance(who) - before;
        self.m.backstop_deposit(who, &pt, &0)
    }
    pub(crate) fn get<V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(&self, k: &Key) -> Option<V> {
        self.env
            .as_contract(&self.m.address, || self.env.storage().instance().get(k))
    }
    pub(crate) fn bs_pt(&self) -> u128 {
        self.get::<Backstop>(&Key::Bs)
            .map(|b| b.ptokens)
            .unwrap_or(0)
    }
    pub(crate) fn shares(&self, a: &Address) -> u128 {
        self.get(&Key::DebtShares(a.clone())).unwrap_or(0)
    }
    pub(crate) fn bs_shares(&self, a: &Address) -> u128 {
        self.get(&Key::BsDebtShares(a.clone())).unwrap_or(0)
    }
    pub(crate) fn ledger(&self, a: &Address) -> u128 {
        self.get(&Key::Ledger(a.clone())).unwrap_or(0)
    }
    pub(crate) fn claims_total(&self, a: &Address) -> u128 {
        self.get(&Key::ClaimsTotal(a.clone())).unwrap_or(0)
    }
    pub(crate) fn pooled(&self, a: &Address) -> u128 {
        self.rec(a).get_user_borrow_balance(&self.m.address)
    }
    pub(crate) fn rec(&self, a: &Address) -> &MockReceiptClient<'static> {
        if *a == self.usdc {
            &self.ru
        } else {
            &self.rx
        }
    }
    pub(crate) fn set_xlm(&self, p: u128) {
        self.ctrl.set_price(&self.xlm, &p);
    }
    pub(crate) fn fees(&self, open: u128, close: u128, funding: u128) {
        self.m.set_fees(
            &self.admin,
            &Fees {
                open_fee: open,
                close_fee: close,
                funding_apr: funding,
                bonus_liquidator: 30_000,
                bonus_backstop: 10_000,
            },
        );
    }
    pub(crate) fn long(&self, coll_xlm: u128, borrow_usdc: u128) -> u64 {
        self.m.open(
            &self.user,
            &self.xlm,
            &self.usdc,
            &coll_xlm,
            &borrow_usdc,
            &0,
            &u128::MAX,
        )
    }
    pub(crate) fn short(&self, coll_usdc: u128, borrow_xlm: u128) -> u64 {
        self.m.open(
            &self.user,
            &self.usdc,
            &self.xlm,
            &coll_usdc,
            &borrow_xlm,
            &0,
            &u128::MAX,
        )
    }
    pub(crate) fn pos(&self, id: u64) -> Position {
        self.m.get_position(&id).unwrap()
    }
    pub(crate) fn tbal(&self, t: &Address, a: &Address) -> i128 {
        TokenClient::new(&self.env, t).balance(a)
    }
    pub(crate) fn assert_ledgers(&self) {
        for a in [&self.usdc, &self.xlm] {
            assert_eq!(
                self.ledger(a),
                self.rec(a).get_ptoken_balance(&self.m.address),
                "ledger vs balance"
            );
        }
    }
    pub(crate) fn skip(&self, s: u64) {
        let now = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(now + s);
    }
}

pub(crate) fn ce(e: Error) -> soroban_sdk::Error {
    e.into()
}
pub(crate) fn err<T, E1, E2: core::fmt::Debug>(
    r: Result<Result<T, E1>, Result<soroban_sdk::Error, E2>>,
) -> soroban_sdk::Error {
    match r {
        Err(Ok(e)) => e,
        Err(Err(e)) => panic!("not a contract error: {:?}", e),
        Ok(_) => panic!("expected an error"),
    }
}

// ------------------------------------------------------------------ admin bounds
#[test]
fn set_risk_caps_fees_slippage_bounds() {
    let t = T::new();
    let a = &t.admin;
    assert_eq!(
        err(t.m.try_set_risk(a, &t.xlm, &1_000_000)),
        ce(Error::BadParams)
    );
    assert_eq!(
        err(t.m.try_set_risk(a, &t.xlm, &5_000_001)),
        ce(Error::BadParams)
    );
    t.m.set_risk(a, &t.xlm, &5_000_000);
    t.m.set_risk(a, &t.xlm, &1_000_001);
    t.m.set_risk(a, &t.xlm, &5_000_000);
    // solvency: 5x liq 0.9 * (1 + 15 %) * 1.01 > 1
    let hi = Fees {
        open_fee: 1_000,
        close_fee: 1_000,
        funding_apr: 20_000,
        bonus_liquidator: 100_000,
        bonus_backstop: 50_000,
    };
    assert_eq!(err(t.m.try_set_fees(a, &hi)), ce(Error::BadParams));
    let over = Fees {
        open_fee: 10_001,
        ..hi.clone()
    };
    assert_eq!(err(t.m.try_set_fees(a, &over)), ce(Error::BadParams));
    let fund = Fees {
        open_fee: 0,
        close_fee: 0,
        funding_apr: 100_001,
        bonus_liquidator: 30_000,
        bonus_backstop: 10_000,
    };
    assert_eq!(err(t.m.try_set_fees(a, &fund)), ce(Error::BadParams));
    let bonus = Fees {
        funding_apr: 0,
        bonus_liquidator: 100_000,
        bonus_backstop: 50_001,
        ..fund.clone()
    };
    assert_eq!(err(t.m.try_set_fees(a, &bonus)), ce(Error::BadParams));
    // slippage (0, 10 %]; 10 % breaks the 5x solvency bound (0.9 * 1.04 * 1.10 > 1)
    assert_eq!(err(t.m.try_set_max_slippage(a, &0)), ce(Error::BadParams));
    assert_eq!(
        err(t.m.try_set_max_slippage(a, &100_001)),
        ce(Error::BadParams)
    );
    assert_eq!(
        err(t.m.try_set_max_slippage(a, &100_000)),
        ce(Error::BadParams)
    );
    t.m.set_max_slippage(a, &20_000);
    // caps
    let c = caps(25, 100, 500);
    assert_eq!(
        err(t.m.try_set_caps(
            a,
            &t.xlm,
            &Caps {
                min_equity_usd: 26 * U,
                ..c.clone()
            }
        )),
        ce(Error::BadParams)
    );
    assert_eq!(
        err(t.m.try_set_caps(
            a,
            &t.xlm,
            &Caps {
                side_equity_usd: 24 * U,
                ..c.clone()
            }
        )),
        ce(Error::BadParams)
    );
    assert_eq!(
        err(t.m.try_set_caps(
            a,
            &t.xlm,
            &Caps {
                side_oi_usd: 0,
                ..c.clone()
            }
        )),
        ce(Error::BadParams)
    );
    let other = Address::generate(&t.env);
    assert_eq!(err(t.m.try_set_caps(a, &other, &c)), ce(Error::BadParams));
    assert_eq!(
        err(t.m.try_set_risk(a, &other, &2_000_000)),
        ce(Error::BadParams)
    );
}

// ------------------------------------------------------------------ open at 5x
#[test]
fn open_long_and_short_exactly_5x_refused_above() {
    let t = T::new();
    t.fees(0, 0, 20_000);
    // long: $25 of XLM, $100 USDC debt -> 5x
    assert_eq!(
        err(t.m.try_open(
            &t.user,
            &t.xlm,
            &t.usdc,
            &(100 * U),
            &(100 * U + 1),
            &0,
            &u128::MAX
        )),
        ce(Error::MaxLeverage)
    );
    let id = t.long(100 * U, 100 * U);
    let p = t.pos(id);
    assert_eq!(p.held_ptokens, 500 * U);
    assert_eq!(p.equity_usd, 25 * U);
    assert_eq!(p.notional_usd, 125 * U);
    assert_eq!(p.liq_ltv, 900_000); // 5x -> 0.9
    assert_eq!(p.collateral, 100 * U);
    assert_eq!(p.funding_ts, 1_000_000);
    assert_eq!(t.m.get_position_ltv(&id), 800_000);
    assert_eq!(t.m.get_position_counter(), 1);
    // short: $25 USDC, 400 XLM ($100) debt -> 5x
    assert_eq!(
        err(t.m.try_open(
            &t.user,
            &t.usdc,
            &t.xlm,
            &(25 * U),
            &(400 * U + 4),
            &0,
            &u128::MAX
        )),
        ce(Error::MaxLeverage)
    );
    let id2 = t.short(25 * U, 400 * U);
    let q = t.pos(id2);
    assert_eq!(q.held_ptokens, 125 * U);
    assert_eq!(q.liq_ltv, 900_000);
    let s = t.m.get_side(&t.xlm);
    assert_eq!(
        (s.equity_usd, s.oi_usd, s.open_positions),
        (25 * U, 125 * U, 1)
    );
    assert_eq!(t.ledger(&t.xlm), 500 * U);
    t.assert_ledgers();
    // a 1x-ish position: liq LTV half way between its LTV and 1
    let id3 = t.long(40 * U, 2 * U); // $10 equity, $2 debt, 48 XLM = $12 held
    let r = t.pos(id3);
    assert_eq!(r.liq_ltv, (1_000_000 + 166_667) / 2);
}

#[test]
fn liq_ltv_with_default_open_fee() {
    let t = T::new();
    let id = t.long(100 * U, 100 * U); // default open fee 0.1 % of $125 gross = 0.125 USDC
    let p = t.pos(id);
    assert_eq!(p.held_ptokens, 4_995_000_000); // 100 + 99.875 / 0.25 XLM
                                               // ltv = ceil(100 / 124.875) = 800_801 -> liq 900_400
    assert_eq!(p.liq_ltv, 900_400);
    assert_eq!(t.bs_pt(), 750 * U + 1_250_000);
}

#[test]
fn open_order_paused_impaired_and_fee_limit() {
    let t = T::new();
    assert_eq!(
        err(t.m.try_open(
            &t.user,
            &t.xlm,
            &t.usdc,
            &(100 * U),
            &(100 * U),
            &0,
            &1_249_999
        )),
        ce(Error::MaxFee)
    );
    t.m.set_opens_paused(&t.admin, &true);
    assert_eq!(
        err(t.m.try_open(
            &t.user,
            &t.xlm,
            &t.usdc,
            &(100 * U),
            &(100 * U),
            &0,
            &u128::MAX
        )),
        ce(Error::OpensPaused)
    );
    // Impaired through the real path: an external seizure of one backstop USDC pToken,
    // detected by reconcile.
    t.ru.seize(&t.m.address, &t.liq, &1);
    assert!(t.m.reconcile(&t.liq));
    // Impaired is checked before the pause
    assert_eq!(
        err(t.m.try_open(
            &t.user,
            &t.xlm,
            &t.usdc,
            &(100 * U),
            &(100 * U),
            &0,
            &u128::MAX
        )),
        ce(Error::Impaired)
    );
    t.m.admin_resolve_impairment(&t.admin, &1, &0); // the backstop absorbs the missing pToken
    assert!(!t.m.is_impaired());
    t.m.set_opens_paused(&t.admin, &false);
    assert_eq!(
        err(t.m.try_open(
            &t.user,
            &t.xlm,
            &t.xlm,
            &(100 * U),
            &(100 * U),
            &0,
            &u128::MAX
        )),
        ce(Error::BadAmount)
    );
    assert_eq!(
        err(t
            .m
            .try_open(&t.user, &t.xlm, &t.usdc, &0, &(100 * U), &0, &u128::MAX)),
        ce(Error::BadAmount)
    );
    // min_out above the fill -> pool refuses
    assert!(t
        .m
        .try_open(
            &t.user,
            &t.xlm,
            &t.usdc,
            &(100 * U),
            &(100 * U),
            &(500 * U),
            &u128::MAX
        )
        .is_err());
}

// ------------------------------------------------------------------ caps and OI
#[test]
fn caps_and_oi() {
    let t = T::new();
    t.fees(0, 0, 0);
    // dust (< $1) and per position ($25)
    assert_eq!(
        err(t
            .m
            .try_open(&t.user, &t.xlm, &t.usdc, &(3 * U), &U, &0, &u128::MAX)),
        ce(Error::BadAmount)
    );
    assert_eq!(
        err(t
            .m
            .try_open(&t.user, &t.xlm, &t.usdc, &(100 * U + 4), &U, &0, &u128::MAX)),
        ce(Error::PositionCapExceeded)
    );
    // side equity $100: four $25 positions, the fifth is refused
    let mut ids = SVec::new();
    for _ in 0..4 {
        ids.push(t.long(100 * U, 100 * U));
    }
    assert_eq!(t.m.get_side(&t.xlm).oi_usd, 500 * U);
    assert_eq!(
        err(t
            .m
            .try_open(&t.user, &t.xlm, &t.usdc, &(4 * U), &U, &0, &u128::MAX)),
        ce(Error::GlobalCapExceeded)
    );
    // closing one frees exactly what it added
    t.m.close(&t.user, &ids[0], &0);
    let s = t.m.get_side(&t.xlm);
    assert_eq!(
        (s.equity_usd, s.oi_usd, s.open_positions),
        (75 * U, 375 * U, 3)
    );
    // OI cap on the short side: $200 notional -> one 5x $25 short fits, a second does not
    t.m.set_caps(&t.admin, &t.usdc, &caps(25, 100, 200));
    t.short(25 * U, 400 * U);
    assert_eq!(
        err(t.m.try_open(
            &t.user,
            &t.usdc,
            &t.xlm,
            &(25 * U),
            &(400 * U),
            &0,
            &u128::MAX
        )),
        ce(Error::OiCapExceeded)
    );
    t.short(10 * U, 160 * U); // $50 notional fits (125 + 50 <= 200)
    t.assert_ledgers();
}

// ------------------------------------------------------------------ fees + funding, four paths
#[test]
fn fees_and_funding_four_paths() {
    let t = T::new();
    // open fees: long from borrowed USDC, short from proceeds
    let b0 = t.bs_pt();
    let l = t.long(100 * U, 100 * U);
    assert_eq!(t.bs_pt() - b0, 1_250_000);
    let s = t.short(25 * U, 400 * U);
    assert_eq!(t.bs_pt() - b0, 2_500_000);
    let sp = t.pos(s);
    assert_eq!(sp.held_ptokens, 125 * U - 1_250_000);
    // funding at elapsed 0 is 0
    assert_eq!(t.m.get_position_funding(&l), 0);
    assert_eq!(t.m.get_position_funding(&s), 0);
    t.skip(YEAR / 2);
    // 2 % APR on $100 for half a year = 1 USDC
    assert_eq!(t.m.get_position_funding(&l), U);
    assert_eq!(t.m.get_position_funding(&s), U);
    t.skip(YEAR / 2);
    assert_eq!(t.m.get_position_funding(&l), 2 * U);
    // funding counts as debt in the LTV view: (100 + 2) / 124.875
    assert_eq!(t.m.get_position_ltv(&l), 816_817);

    // close long: fee (0.1 % of 124.875) + funding from USDC proceeds
    let b1 = t.bs_pt();
    let lp = t.pos(l);
    let close_fee = (lp.notional_usd * 1_000).div_ceil(1_000_000);
    t.m.close(&t.user, &l, &0);
    assert_eq!(t.bs_pt() - b1, close_fee + 2 * U);
    // close short: fee + funding moved internally from held USDC pTokens
    let b2 = t.bs_pt();
    let user_pt0 = t.ru.get_ptoken_balance(&t.user);
    let fee_s = (sp.notional_usd * 1_000).div_ceil(1_000_000);
    t.m.close(&t.user, &s, &0);
    assert_eq!(t.bs_pt() - b2, fee_s + 2 * U);
    assert!(t.ru.get_ptoken_balance(&t.user) > user_pt0);
    assert_eq!(t.pooled(&t.usdc), 0);
    assert_eq!(t.pooled(&t.xlm), 0);
    t.assert_ledgers();

    // close_with_repay paths, after 1 year
    let l2 = t.long(100 * U, 100 * U);
    let s2 = t.short(25 * U, 400 * U);
    t.skip(YEAR);
    let n_l = t.pos(l2).notional_usd;
    let n_s = t.pos(s2).notional_usd;
    let b3 = t.bs_pt();
    let u0 = t.tbal(&t.usdc, &t.user);
    t.m.close_with_repay(&t.user, &l2);
    let fee_l = (n_l * 1_000).div_ceil(1_000_000);
    assert_eq!(t.bs_pt() - b3, fee_l + 2 * U);
    assert_eq!(
        u0 - t.tbal(&t.usdc, &t.user),
        (100 * U + fee_l + 2 * U) as i128
    );
    let b4 = t.bs_pt();
    t.m.close_with_repay(&t.user, &s2);
    assert_eq!(t.bs_pt() - b4, (n_s * 1_000).div_ceil(1_000_000) + 2 * U);
    t.assert_ledgers();
    assert_eq!(t.shares(&t.usdc), 0);
    assert_eq!(t.shares(&t.xlm), 0);
}

#[test]
fn funding_same_timestamp_close_charges_none() {
    let t = T::new();
    t.fees(0, 0, 20_000);
    let l = t.long(100 * U, 100 * U);
    let b = t.bs_pt();
    t.m.close(&t.user, &l, &0); // same ledger timestamp (CAP-88 repeats): no funding
    assert_eq!(t.bs_pt(), b);
}

// ------------------------------------------------------------------ close / close_with_repay
#[test]
fn close_both_sides_pays_owner() {
    let t = T::new();
    t.fees(0, 0, 0);
    let x0 = t.tbal(&t.xlm, &t.user);
    let u0 = t.tbal(&t.usdc, &t.user);
    let l = t.long(100 * U, 100 * U);
    t.set_xlm(3_000_000); // XLM +20 %: 500 XLM = $150, debt $100
    t.m.close(&t.user, &l, &0);
    let p = t.pos(l);
    assert!(!p.open);
    assert_eq!((p.held_ptokens, p.debt_shares), (0, 0));
    // user got the unsold XLM pTokens and the USDC surplus; total gain ~ $50 - $25 equity
    let xpt = t.rx.get_ptoken_balance(&t.user);
    let usdc_gain = t.tbal(&t.usdc, &t.user) - u0;
    let value = xpt * 3 / 10 + usdc_gain as u128;
    assert!(value > 49 * U && value <= 50 * U, "value {}", value);
    assert_eq!(x0 - t.tbal(&t.xlm, &t.user), (100 * U) as i128);
    t.set_xlm(2_500_000);
    let s = t.short(25 * U, 400 * U);
    t.set_xlm(2_000_000); // XLM -20 %: debt $80, held $125
    t.m.close(&t.user, &s, &0);
    assert_eq!(t.pooled(&t.xlm), 0);
    assert_eq!(t.m.get_side(&t.usdc).open_positions, 0);
    assert_eq!(err(t.m.try_close(&t.user, &s, &0)), ce(Error::NotOpen));
    t.assert_ledgers();
    assert_eq!(t.ledger(&t.xlm), 0);
}

#[test]
fn close_with_repay_long_is_oracle_free_and_short_works() {
    let t = T::new();
    let l = t.long(100 * U, 100 * U);
    let s = t.short(25 * U, 400 * U);
    let other = Address::generate(&t.env);
    assert_eq!(
        err(t.m.try_close_with_repay(&other, &l)),
        ce(Error::NotOwner)
    );
    t.skip(1000);
    t.ctrl.set_trap(&true); // any oracle read now traps
    t.m.close_with_repay(&t.user, &l);
    assert_eq!(t.rx.get_ptoken_balance(&t.user), 4_995_000_000);
    assert!(t.m.try_close_with_repay(&t.user, &s).is_err()); // short needs the price for fees
    t.ctrl.set_trap(&false);
    let x0 = t.tbal(&t.xlm, &t.user);
    t.m.close_with_repay(&t.user, &s);
    assert_eq!(x0 - t.tbal(&t.xlm, &t.user), (400 * U) as i128);
    assert_eq!(t.pooled(&t.usdc), 0);
    assert_eq!(t.pooled(&t.xlm), 0);
    t.assert_ledgers();
}

// ------------------------------------------------------------------ liquidation
#[test]
fn solvent_liquidation_long_split_and_claim() {
    let t = T::new();
    t.fees(0, 0, 0);
    let l = t.long(100 * U, 100 * U); // 500 XLM held, 100 USDC debt, liq 0.9
    assert_eq!(
        err(t.m.try_liquidate(&t.liq, &l, &u128::MAX)),
        ce(Error::NotLiquidatable)
    );
    t.set_xlm(2_200_000); // $110 held -> LTV 0.909
    assert!(t.m.is_liquidatable(&l));
    assert_eq!(
        err(t.m.try_liquidate(&t.liq, &l, &(100 * U))),
        ce(Error::MaxPay)
    ); // needs 101
    let b0 = t.bs_pt();
    let u0 = t.tbal(&t.usdc, &t.liq);
    t.m.liquidate(&t.liq, &l, &(101 * U));
    assert_eq!(u0 - t.tbal(&t.usdc, &t.liq), (101 * U) as i128); // D + 1 % backstop bonus
    assert_eq!(t.bs_pt() - b0, U);
    let seize = 103 * U * PS / 2_200_000; // $103 of XLM at $0.22
    assert_eq!(t.rx.get_ptoken_balance(&t.liq), seize);
    let rem = 500 * U - seize;
    assert_eq!(t.m.get_claim(&t.user, &t.xlm), rem);
    assert_eq!(t.claims_total(&t.xlm), rem);
    assert_eq!(t.rx.get_ptoken_balance(&t.user), 0); // never transferred inside liquidate
    assert_eq!(t.pooled(&t.usdc), 0);
    t.assert_ledgers();
    // claim
    let other = Address::generate(&t.env);
    assert_eq!(err(t.m.try_claim(&other, &t.xlm)), ce(Error::NoClaim));
    assert_eq!(t.m.claim(&t.user, &t.xlm), rem);
    assert_eq!(t.rx.get_ptoken_balance(&t.user), rem);
    assert_eq!(t.claims_total(&t.xlm), 0);
    assert_eq!(t.ledger(&t.xlm), 0);
    assert_eq!(err(t.m.try_claim(&t.user, &t.xlm)), ce(Error::NoClaim));
    t.assert_ledgers();
}

#[test]
fn solvent_liquidation_short_split_and_middle_band() {
    let t = T::new();
    t.fees(0, 0, 0);
    let s = t.short(25 * U, 400 * U); // 125 USDC held, 400 XLM debt
    t.set_xlm(2_900_000); // debt $116 -> LTV 0.928
    let b0 = t.bs_pt();
    let x0 = t.tbal(&t.xlm, &t.liq);
    assert_eq!(
        err(t.m.try_liquidate(&t.liq, &s, &(400 * U - 1))),
        ce(Error::MaxPay)
    );
    t.m.liquidate(&t.liq, &s, &(400 * U));
    assert_eq!(x0 - t.tbal(&t.xlm, &t.liq), (400 * U) as i128);
    assert_eq!(t.ru.get_ptoken_balance(&t.liq), 1_194_800_000); // 116 * 1.03
    assert_eq!(t.bs_pt() - b0, 11_600_000); // 116 * 1 %
    assert_eq!(t.m.get_claim(&t.user, &t.usdc), 43_600_000);
    t.assert_ledgers();

    // middle band: held covers D * 1.03 but not the backstop bonus in full
    t.set_xlm(2_500_000);
    let s2 = t.short(25 * U, 400 * U);
    t.set_xlm(3_020_000); // D $120.8: seize 124.424, backstop gets the 0.576 left, owner 0
    let b1 = t.bs_pt();
    let c0 = t.m.get_claim(&t.user, &t.usdc);
    let liq0 = t.ru.get_ptoken_balance(&t.liq);
    t.m.liquidate(&t.liq, &s2, &(400 * U));
    assert_eq!(t.ru.get_ptoken_balance(&t.liq) - liq0, 1_244_240_000);
    assert_eq!(t.bs_pt() - b1, 5_760_000);
    assert_eq!(t.m.get_claim(&t.user, &t.usdc), c0);
    assert_eq!(t.bs_shares(&t.xlm), 0);
    t.assert_ledgers();
}

#[test]
fn liquidation_long_with_funding_paid_by_liquidator() {
    let t = T::new();
    t.fees(0, 0, 20_000);
    let l = t.long(100 * U, 100 * U);
    t.skip(YEAR); // funding 2 USDC
    t.set_xlm(2_200_000);
    let b0 = t.bs_pt();
    let u0 = t.tbal(&t.usdc, &t.liq);
    t.m.liquidate(&t.liq, &l, &u128::MAX);
    assert_eq!(u0 - t.tbal(&t.usdc, &t.liq), (103 * U) as i128); // D + 1 + funding 2
    assert_eq!(t.bs_pt() - b0, 3 * U);
    assert_eq!(t.rx.get_ptoken_balance(&t.liq), 105 * U * PS / 2_200_000);
    t.assert_ledgers();
}

fn underwater_case(interest: u128) {
    let t = T::new();
    t.fees(0, 0, 0);
    let a = t.long(100 * U, 100 * U); // 5x, liq 0.9
    let b = t.long(100 * U, 25 * U); // 2x, liq 0.75
    assert_eq!(t.shares(&t.usdc), 125 * U);
    if interest > 0 {
        t.ru.add_debt(&t.m.address, &interest);
    }
    t.set_xlm(1_800_000); // A: 500 XLM = $90 < D * 1.03 -> underwater; B: LTV 0.69, safe
    assert!(!t.m.is_liquidatable(&b));
    let b_debt = t.m.get_position_debt(&b);
    let b_pos = t.pos(b);
    let d_a = t.m.get_position_debt(&a);
    let u0 = t.tbal(&t.usdc, &t.liq);
    t.m.liquidate(&t.liq, &a, &u128::MAX);
    let r = 90 * U * 1_000_000 / 1_030_000; // H / (1 + bonus_liq), USDC at $1
    assert_eq!(u0 - t.tbal(&t.usdc, &t.liq), r as i128);
    assert_eq!(t.rx.get_ptoken_balance(&t.liq), 500 * U); // all held
    let burned = 100 * U * r / d_a;
    assert_eq!(t.bs_shares(&t.usdc), 100 * U - burned);
    assert_eq!(t.shares(&t.usdc), 25 * U);
    assert_eq!(t.m.get_claim(&t.user, &t.xlm), 0);
    // the other position is untouched
    assert_eq!(t.pos(b), b_pos);
    let b_after = t.m.get_position_debt(&b);
    if interest == 0 {
        assert_eq!(b_after, b_debt);
        assert_eq!(b_after, 25 * U);
    } else {
        assert!(
            b_after <= b_debt && b_after + 1 >= b_debt,
            "{} vs {}",
            b_after,
            b_debt
        );
    }
    // closing the last position pays only its share, not the backstop's
    t.set_xlm(2_500_000);
    let pooled_before = t.pooled(&t.usdc);
    t.m.close(&t.user, &b, &0);
    assert_eq!(pooled_before - t.pooled(&t.usdc), b_after);
    assert_eq!(t.shares(&t.usdc), 0);
    if interest == 0 {
        assert_eq!(t.pooled(&t.usdc), 100 * U - r);
        assert_eq!(t.pooled(&t.usdc), t.bs_shares(&t.usdc));
    }
    t.assert_ledgers();
}

#[test]
fn underwater_liquidation_moves_shares_to_backstop_exact() {
    underwater_case(0);
}

#[test]
fn underwater_liquidation_with_interest_never_raises_other_debt() {
    underwater_case(7_777_777);
}

#[test]
fn underwater_short_liquidation() {
    let t = T::new();
    t.fees(0, 0, 0);
    let s = t.short(25 * U, 400 * U);
    let s2 = t.short(25 * U, 100 * U); // 2x
    let d2 = t.m.get_position_debt(&s2);
    t.set_xlm(3_500_000); // debt $140 > 125 / 1.03
    t.m.liquidate(&t.liq, &s, &u128::MAX);
    assert_eq!(t.ru.get_ptoken_balance(&t.liq), 125 * U);
    assert!(t.bs_shares(&t.xlm) > 0);
    assert_eq!(t.m.get_position_debt(&s2), d2);
    t.assert_ledgers();
}

// ------------------------------------------------------------------ property loop
pub(crate) struct Rng(u64);
impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub(crate) fn range(&mut self, lo: u128, hi: u128) -> u128 {
        lo + (self.next() as u128) % (hi - lo + 1)
    }
}

pub(crate) fn check_debt_invariant(t: &T, open: &[u64]) {
    for a in [t.usdc.clone(), t.xlm.clone()] {
        let pooled = t.pooled(&a);
        let (ps, bs) = (t.shares(&a), t.bs_shares(&a));
        let ts = ps + bs;
        if ts == 0 {
            assert_eq!(pooled, 0, "debt without shares");
            continue;
        }
        let mut sum = 0u128;
        let mut n = 0u128;
        let mut share_sum = 0u128;
        for id in open {
            let p = t.pos(*id);
            if p.debt_asset == a {
                sum += t.m.get_position_debt(id);
                share_sum += p.debt_shares;
                n += 1;
            }
        }
        assert_eq!(share_sum, ps, "DebtShares = sum of position shares");
        if bs > 0 {
            sum += if bs >= ts {
                pooled
            } else {
                (bs * pooled).div_ceil(ts)
            };
            n += 1;
        }
        assert!(
            sum >= pooled && sum <= pooled + n,
            "sum {} pooled {} n {}",
            sum,
            pooled,
            n
        );
    }
    // Ledger = held + backstop + claims (USDC), held + claims (XLM), = receipt balance
    let (mut hu, mut hx) = (0u128, 0u128);
    for id in open {
        let p = t.pos(*id);
        if p.held_asset == t.usdc {
            hu += p.held_ptokens
        } else {
            hx += p.held_ptokens
        }
    }
    assert_eq!(t.ledger(&t.usdc), hu + t.bs_pt() + t.claims_total(&t.usdc));
    assert_eq!(t.ledger(&t.xlm), hx + t.claims_total(&t.xlm));
    t.assert_ledgers();
}

#[test]
fn property_debt_conservation() {
    let t = T::new();
    t.m.set_caps(&t.admin, &t.xlm, &caps(1_000, 1_000_000, 10_000_000));
    t.m.set_caps(&t.admin, &t.usdc, &caps(1_000, 1_000_000, 10_000_000));
    t.seed_backstop(1_000_000 * U);
    t.ru.set_rate(&1_020_000);
    t.rx.set_rate(&1_010_000);
    t.pool.set_fill(&997_000);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut open: SVec<u64> = SVec::new();
    let (mut opens, mut closes, mut liqs, mut bad) = (0, 0, 0, 0);
    let mut xp: u128 = 2_500_000;
    for _step in 0..160 {
        match rng.next() % 9 {
            0 | 1 => {
                let coll = rng.range(8, 3_000) * U; // XLM
                let eq = coll * xp / PS;
                let lev = rng.range(1_100_000, 4_990_000);
                let b = eq * (lev - 1_000_000) / 1_000_000;
                if let Ok(Ok(id)) =
                    t.m.try_open(&t.user, &t.xlm, &t.usdc, &coll, &b, &0, &u128::MAX)
                {
                    open.push(id);
                    opens += 1;
                }
            }
            2 | 3 => {
                let coll = rng.range(2, 800) * U; // USDC
                let lev = rng.range(1_100_000, 4_990_000);
                let b = coll * (lev - 1_000_000) / 1_000_000 * PS / xp;
                if let Ok(Ok(id)) =
                    t.m.try_open(&t.user, &t.usdc, &t.xlm, &coll, &b, &0, &u128::MAX)
                {
                    open.push(id);
                    opens += 1;
                }
            }
            4 if !open.is_empty() => {
                let k = (rng.next() as usize) % open.len();
                let ok = if rng.next() % 2 == 0 {
                    t.m.try_close(&t.user, &open[k], &0).is_ok()
                } else {
                    t.m.try_close_with_repay(&t.user, &open[k]).is_ok()
                };
                if ok {
                    open.remove(k);
                    closes += 1;
                }
            }
            5 | 6 => {
                // price move: mostly small, sometimes a gap
                let pct = if rng.next() % 4 == 0 {
                    rng.range(600, 1_400)
                } else {
                    rng.range(940, 1_060)
                };
                xp = (xp * pct / 1_000).clamp(500_000, 10_000_000);
                t.set_xlm(xp);
            }
            7 => {
                for a in [t.usdc.clone(), t.xlm.clone()] {
                    let pooled = t.pooled(&a);
                    if pooled > 0 && t.shares(&a) + t.bs_shares(&a) > 0 {
                        t.rec(&a)
                            .add_debt(&t.m.address, &(pooled * rng.range(1, 20) / 1_000));
                    }
                }
                t.ru.set_rate(&(t.ru.get_exchange_rate() + rng.range(0, 2_000)));
                t.rx.set_rate(&(t.rx.get_exchange_rate() + rng.range(0, 2_000)));
            }
            _ => t.skip(rng.range(0, 86_400 * 10) as u64),
        }
        // keeper pass
        let mut k = 0;
        while k < open.len() {
            let id = open[k];
            if t.m.is_liquidatable(&id) {
                let bs0 = t.bs_shares(&t.usdc) + t.bs_shares(&t.xlm);
                t.m.liquidate(&t.liq, &id, &u128::MAX);
                if t.bs_shares(&t.usdc) + t.bs_shares(&t.xlm) > bs0 {
                    bad += 1;
                }
                liqs += 1;
                open.remove(k);
            } else {
                k += 1;
            }
        }
        check_debt_invariant(&t, &open);
    }
    std::println!(
        "opens {} closes {} liquidations {} bad-debt {} open {}",
        opens,
        closes,
        liqs,
        bad,
        open.len()
    );
    assert!(
        opens > 20 && closes > 3 && liqs > 3 && bad > 0,
        "coverage: {} {} {} {}",
        opens,
        closes,
        liqs,
        bad
    );
}
