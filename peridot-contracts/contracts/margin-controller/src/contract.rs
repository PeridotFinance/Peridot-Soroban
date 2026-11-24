use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error, token,
    Address, BytesN, Env, IntoVal, Symbol, Vec,
};

pub const SCALE_1E6: u128 = 1_000_000;
pub const MAX_SLIPPAGE_CAP: u128 = 100_000; // 10%
pub const MAX_LIQ_BONUS: u128 = 1_150_000; // 15%
pub const UPGRADE_TIMELOCK_SECS: u64 = 24 * 60 * 60;
const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND_TO: u32 = 1_000_000;

// ---------------------------------------------------------------- live ABIs
// Signatures checked against the deployed bytes (receipt 5630a701, controller f41e5260)
// with `stellar contract info interface`. Only plain-account entrypoints are used.
#[contractclient(name = "ReceiptClient")]
pub trait Receipt {
    fn deposit(env: Env, user: Address, amount: u128);
    fn withdraw(env: Env, user: Address, ptoken_amount: u128);
    fn borrow(env: Env, user: Address, amount: u128);
    fn repay(env: Env, user: Address, amount: u128);
    fn transfer(env: Env, from: Address, to: Address, amount: i128);
    fn get_underlying_token(env: Env) -> Address;
    fn get_exchange_rate(env: Env) -> u128;
    fn get_ptoken_balance(env: Env, user: Address) -> u128;
    fn get_user_borrow_balance(env: Env, user: Address) -> u128;
}

#[contractclient(name = "CtrlClient")]
pub trait Ctrl {
    fn enter_market(env: Env, user: Address, market: Address);
    fn get_price_usd(env: Env, token: Address) -> Option<(u128, u128)>;
    fn get_market_cf(env: Env, market: Address) -> u128;
}

/// Aquarius pool: `swap` does NOT require_auth; the input `transfer` carries the
/// caller's authorization, so that entry must sit at the root.
#[contractclient(name = "PoolClient")]
pub trait Pool {
    fn get_tokens(env: Env) -> Vec<Address>;
    fn swap(env: Env, user: Address, in_idx: u32, out_idx: u32, amount_in: u128, out_min: u128) -> u128;
}

// ---------------------------------------------------------------- storage
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotAdmin = 2,
    OpensPaused = 3,
    BadAmount = 4,
    NotConfigured = 5,
    BadParams = 6,
    PositionCapExceeded = 7,
    GlobalCapExceeded = 8,
    LtvTooHigh = 9,
    Slippage = 10,
    PriceUnavailable = 11,
    NotOwner = 12,
    NotOpen = 13,
    NotLiquidatable = 14,
    InsufficientSwapOutput = 15,
    Timelocked = 16,
    UpgradeMismatch = 17,
    NotGuardian = 18,
}

#[contracttype]
#[derive(Clone)]
pub enum Key {
    // instance
    Admin,
    Guardian,
    Ctrl,
    Pool,
    PoolTokens,
    OpensPaused,
    MaxSlippage,
    LiqBonus,
    Market(Address),     // underlying -> receipt
    Risk(Address),       // held underlying -> RiskParams
    Caps(Address),       // debt underlying -> Caps
    DebtShares(Address), // debt underlying -> total shares
    Counter,
    PendingUpgrade,
    // persistent
    Position(u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskParams {
    pub open_ltv: u128, // max debt/held value at open, 1e6
    pub liq_ltv: u128,  // liquidatable at or above, 1e6; must stay below the market CF
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Caps {
    pub per_position: u128, // max debt borrowed by one open, raw debt units
    pub global: u128,       // max pooled debt (incl. interest) after an open, raw debt units
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub owner: Address,
    pub held_asset: Address,
    pub debt_asset: Address,
    pub held_ptokens: u128,
    pub debt_shares: u128,
    pub opened_at: u64,
    pub open: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpgrade {
    pub hash: BytesN<32>,
    pub eta: u64,
}

#[contract]
pub struct MarginController;

// ---------------------------------------------------------------- helpers
fn inst_get<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(env: &Env, k: &Key) -> Option<T> {
    env.storage().instance().get(k)
}
fn need<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(env: &Env, k: &Key) -> T {
    inst_get(env, k).unwrap_or_else(|| panic_with_error!(env, Error::NotConfigured))
}
fn bump(env: &Env) {
    env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}
fn me(env: &Env) -> Address {
    env.current_contract_address()
}
fn require_admin(env: &Env, admin: &Address) {
    let stored: Address = need(env, &Key::Admin);
    if stored != *admin {
        panic_with_error!(env, Error::NotAdmin);
    }
    admin.require_auth();
}
fn receipt(env: &Env, asset: &Address) -> Address {
    need(env, &Key::Market(asset.clone()))
}
fn ctrl(env: &Env) -> CtrlClient<'_> {
    let a: Address = need(env, &Key::Ctrl);
    CtrlClient::new(env, &a)
}
fn price(env: &Env, asset: &Address) -> (u128, u128) {
    match ctrl(env).get_price_usd(asset) {
        Some((p, s)) if p > 0 && s > 0 => (p, s),
        _ => panic_with_error!(env, Error::PriceUnavailable),
    }
}
fn usd(amount: u128, p: (u128, u128)) -> u128 {
    amount.checked_mul(p.0).expect("overflow") / p.1
}
fn mul_div_up(a: u128, b: u128, d: u128) -> u128 {
    let n = a.checked_mul(b).expect("overflow");
    n / d + if n % d == 0 { 0 } else { 1 }
}
fn tok_balance(env: &Env, t: &Address) -> u128 {
    let b = token::TokenClient::new(env, t).balance(&me(env));
    if b < 0 {
        0
    } else {
        b as u128
    }
}
fn i(env: &Env, x: u128) -> i128 {
    if x > i128::MAX as u128 {
        panic_with_error!(env, Error::BadAmount);
    }
    x as i128
}

/// Root entry for `token.transfer(self -> to, amount)` performed inside the NEXT call.
fn authorize_transfer(env: &Env, tok: &Address, to: &Address, amount: u128) {
    let mut auths = Vec::new(env);
    auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
        context: ContractContext {
            contract: tok.clone(),
            fn_name: Symbol::new(env, "transfer"),
            args: (me(env), to.clone(), i(env, amount)).into_val(env),
        },
        sub_invocations: Vec::new(env),
    }));
    env.authorize_as_current_contract(auths);
}

fn deposit_self(env: &Env, asset: &Address, amount: u128) -> u128 {
    let r = receipt(env, asset);
    let rc = ReceiptClient::new(env, &r);
    let before = rc.get_ptoken_balance(&me(env));
    authorize_transfer(env, asset, &r, amount);
    rc.deposit(&me(env), &amount);
    let minted = rc.get_ptoken_balance(&me(env)).saturating_sub(before);
    if minted == 0 {
        panic_with_error!(env, Error::BadAmount);
    }
    minted
}

/// Repay up to `amount` of the pooled debt; returns what was actually repaid.
fn repay_self(env: &Env, asset: &Address, amount: u128) -> u128 {
    let r = receipt(env, asset);
    let rc = ReceiptClient::new(env, &r);
    // The receipt caps its pull at its pre-accrual debt view; mirror that exactly so
    // the authorized transfer arguments match.
    let view = rc.get_user_borrow_balance(&me(env));
    let x = amount.min(view);
    if x == 0 {
        return 0;
    }
    authorize_transfer(env, asset, &r, x);
    rc.repay(&me(env), &x);
    x
}

fn swap(env: &Env, t_in: &Address, t_out: &Address, amount_in: u128, min_out: u128) -> u128 {
    if amount_in == 0 || min_out == 0 {
        panic_with_error!(env, Error::BadAmount);
    }
    let pool: Address = need(env, &Key::Pool);
    let toks: Vec<Address> = need(env, &Key::PoolTokens);
    let idx = |t: &Address| -> u32 {
        for k in 0..toks.len() {
            if toks.get_unchecked(k) == *t {
                return k;
            }
        }
        panic_with_error!(env, Error::NotConfigured)
    };
    let (ii, oi) = (idx(t_in), idx(t_out));
    let before = tok_balance(env, t_out);
    authorize_transfer(env, t_in, &pool, amount_in);
    PoolClient::new(env, &pool).swap(&me(env), &ii, &oi, &amount_in, &min_out);
    let got = tok_balance(env, t_out).saturating_sub(before);
    if got < min_out {
        panic_with_error!(env, Error::Slippage);
    }
    got
}

fn oracle_min_out(env: &Env, t_in: &Address, t_out: &Address, amount_in: u128) -> u128 {
    let slip: u128 = inst_get(env, &Key::MaxSlippage).unwrap_or(10_000);
    let po = price(env, t_out);
    let out = usd(amount_in, price(env, t_in)).checked_mul(po.1).expect("overflow") / po.0;
    let m = out.checked_mul(SCALE_1E6 - slip).expect("overflow") / SCALE_1E6;
    if m == 0 {
        panic_with_error!(env, Error::BadAmount);
    }
    m
}

fn total_shares(env: &Env, debt: &Address) -> u128 {
    inst_get(env, &Key::DebtShares(debt.clone())).unwrap_or(0)
}
fn pooled_debt(env: &Env, debt: &Address) -> u128 {
    ReceiptClient::new(env, &receipt(env, debt)).get_user_borrow_balance(&me(env))
}
/// Position debt rounded up; the last share holder owes the whole pooled balance.
fn position_debt(env: &Env, p: &Position) -> u128 {
    let ts = total_shares(env, &p.debt_asset);
    if p.debt_shares == 0 || ts == 0 {
        return 0;
    }
    let pooled = pooled_debt(env, &p.debt_asset);
    if p.debt_shares >= ts {
        return pooled;
    }
    mul_div_up(p.debt_shares, pooled, ts).min(pooled)
}
fn held_underlying(env: &Env, p: &Position) -> u128 {
    let rate = ReceiptClient::new(env, &receipt(env, &p.held_asset)).get_exchange_rate();
    p.held_ptokens.checked_mul(rate).expect("overflow") / SCALE_1E6
}
fn ltv(env: &Env, p: &Position, debt: u128) -> u128 {
    let held_usd = usd(held_underlying(env, p), price(env, &p.held_asset));
    let debt_usd = usd(debt, price(env, &p.debt_asset));
    if held_usd == 0 {
        return u128::MAX;
    }
    mul_div_up(debt_usd, SCALE_1E6, held_usd)
}
fn load(env: &Env, id: u64) -> Position {
    let p: Position = env
        .storage()
        .persistent()
        .get(&Key::Position(id))
        .unwrap_or_else(|| panic_with_error!(env, Error::NotOpen));
    if !p.open {
        panic_with_error!(env, Error::NotOpen);
    }
    p
}
fn store(env: &Env, id: u64, p: &Position) {
    let k = Key::Position(id);
    env.storage().persistent().set(&k, p);
    env.storage().persistent().extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND_TO);
}
fn finish(env: &Env, id: u64, p: Position) {
    let ts = total_shares(env, &p.debt_asset);
    env.storage()
        .instance()
        .set(&Key::DebtShares(p.debt_asset.clone()), &ts.saturating_sub(p.debt_shares));
    let mut q = p;
    q.open = false;
    q.held_ptokens = 0;
    q.debt_shares = 0;
    store(env, id, &q);
}
fn send_ptokens(env: &Env, asset: &Address, to: &Address, amount: u128) {
    if amount == 0 {
        return;
    }
    ReceiptClient::new(env, &receipt(env, asset)).transfer(&me(env), to, &i(env, amount));
}

// ---------------------------------------------------------------- contract
#[contractimpl]
impl MarginController {
    /// Constructor: runs atomically with contract creation, so initialization cannot be
    /// front-run between the create and a separate initialize transaction.
    pub fn __constructor(env: Env, admin: Address, peridottroller: Address, guardian: Address) {
        if env.storage().instance().has(&Key::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        let s = env.storage().instance();
        s.set(&Key::Admin, &admin);
        s.set(&Key::Guardian, &guardian);
        s.set(&Key::Ctrl, &peridottroller);
        s.set(&Key::OpensPaused, &true); // opens start paused
        s.set(&Key::MaxSlippage, &10_000u128); // 1%
        s.set(&Key::LiqBonus, &1_050_000u128); // 5%
        s.set(&Key::Counter, &0u64);
        bump(&env);
    }

    // ---- admin configuration (immediate; affects only this contract)
    pub fn set_market(env: Env, admin: Address, asset: Address, receipt_addr: Address) {
        require_admin(&env, &admin);
        if ReceiptClient::new(&env, &receipt_addr).get_underlying_token() != asset {
            panic_with_error!(&env, Error::BadParams);
        }
        env.storage().instance().set(&Key::Market(asset), &receipt_addr);
        bump(&env);
    }

    /// Enters the configured receipts as the margin account (live entered-market cap is 2).
    pub fn enter_markets(env: Env, admin: Address, assets: Vec<Address>) {
        require_admin(&env, &admin);
        for a in assets.iter() {
            ctrl(&env).enter_market(&me(&env), &receipt(&env, &a));
        }
        bump(&env);
    }

    pub fn set_pool(env: Env, admin: Address, pool: Address) {
        require_admin(&env, &admin);
        let toks = PoolClient::new(&env, &pool).get_tokens();
        let s = env.storage().instance();
        s.set(&Key::Pool, &pool);
        s.set(&Key::PoolTokens, &toks);
        bump(&env);
    }

    pub fn set_risk(env: Env, admin: Address, held_asset: Address, open_ltv: u128, liq_ltv: u128) {
        require_admin(&env, &admin);
        let cf = ctrl(&env).get_market_cf(&receipt(&env, &held_asset));
        if open_ltv == 0 || open_ltv >= liq_ltv || liq_ltv >= cf {
            panic_with_error!(&env, Error::BadParams);
        }
        env.storage()
            .instance()
            .set(&Key::Risk(held_asset), &RiskParams { open_ltv, liq_ltv });
        bump(&env);
    }

    pub fn set_caps(env: Env, admin: Address, debt_asset: Address, per_position: u128, global: u128) {
        require_admin(&env, &admin);
        if per_position > global {
            panic_with_error!(&env, Error::BadParams);
        }
        env.storage()
            .instance()
            .set(&Key::Caps(debt_asset), &Caps { per_position, global });
        bump(&env);
    }

    pub fn set_limits(env: Env, admin: Address, max_slippage: u128, liq_bonus: u128) {
        require_admin(&env, &admin);
        if max_slippage == 0
            || max_slippage > MAX_SLIPPAGE_CAP
            || liq_bonus < SCALE_1E6
            || liq_bonus > MAX_LIQ_BONUS
        {
            panic_with_error!(&env, Error::BadParams);
        }
        env.storage().instance().set(&Key::MaxSlippage, &max_slippage);
        env.storage().instance().set(&Key::LiqBonus, &liq_bonus);
        bump(&env);
    }

    pub fn set_guardian(env: Env, admin: Address, guardian: Address) {
        require_admin(&env, &admin);
        env.storage().instance().set(&Key::Guardian, &guardian);
    }

    /// Kill switch for NEW positions: admin or guardian (guardian may only pause),
    /// immediate. Close, close_with_repay and liquidate always stay available.
    pub fn set_opens_paused(env: Env, caller: Address, paused: bool) {
        let admin: Address = need(&env, &Key::Admin);
        let guardian: Address = need(&env, &Key::Guardian);
        if caller != admin && caller != guardian {
            panic_with_error!(&env, Error::NotGuardian);
        }
        if caller != admin && !paused {
            panic_with_error!(&env, Error::NotAdmin);
        }
        caller.require_auth();
        env.storage().instance().set(&Key::OpensPaused, &paused);
        bump(&env);
    }

    pub fn propose_upgrade(env: Env, admin: Address, hash: BytesN<32>) {
        require_admin(&env, &admin);
        let eta = env.ledger().timestamp() + UPGRADE_TIMELOCK_SECS;
        env.storage()
            .instance()
            .set(&Key::PendingUpgrade, &PendingUpgrade { hash, eta });
    }

    pub fn upgrade(env: Env, admin: Address, hash: BytesN<32>) {
        require_admin(&env, &admin);
        let p: PendingUpgrade = need(&env, &Key::PendingUpgrade);
        if p.hash != hash {
            panic_with_error!(&env, Error::UpgradeMismatch);
        }
        if env.ledger().timestamp() < p.eta {
            panic_with_error!(&env, Error::Timelocked);
        }
        env.storage().instance().remove(&Key::PendingUpgrade);
        env.deployer().update_current_contract_wasm(hash);
    }

    // ---- user flows

    /// One-click open. `held_asset` is both the user's collateral and the asset bought
    /// (long XLM: held XLM, debt USDC; short XLM: held USDC, debt XLM).
    pub fn open(
        env: Env,
        user: Address,
        held_asset: Address,
        debt_asset: Address,
        collateral: u128,
        borrow: u128,
        min_out: u128,
    ) -> u64 {
        user.require_auth();
        bump(&env);
        if inst_get::<bool>(&env, &Key::OpensPaused).unwrap_or(true) {
            panic_with_error!(&env, Error::OpensPaused);
        }
        if collateral == 0 || borrow == 0 || held_asset == debt_asset {
            panic_with_error!(&env, Error::BadAmount);
        }
        let rp: RiskParams = need(&env, &Key::Risk(held_asset.clone()));
        let caps: Caps = need(&env, &Key::Caps(debt_asset.clone()));
        if borrow > caps.per_position {
            panic_with_error!(&env, Error::PositionCapExceeded);
        }
        let pooled_before = pooled_debt(&env, &debt_asset);
        if pooled_before.saturating_add(borrow) > caps.global {
            panic_with_error!(&env, Error::GlobalCapExceeded);
        }
        // Pre-trade LTV with the oracle-minimum swap output: debt / (collateral + bought).
        let oracle_out = oracle_min_out(&env, &debt_asset, &held_asset, borrow);
        let held_usd = usd(collateral.saturating_add(oracle_out), price(&env, &held_asset));
        let debt_usd = usd(borrow, price(&env, &debt_asset));
        if mul_div_up(debt_usd, SCALE_1E6, held_usd.max(1)) > rp.open_ltv {
            panic_with_error!(&env, Error::LtvTooHigh);
        }

        token::TokenClient::new(&env, &held_asset).transfer(&user, &me(&env), &i(&env, collateral));
        let mut held_ptokens = deposit_self(&env, &held_asset, collateral);

        let rd = ReceiptClient::new(&env, &receipt(&env, &debt_asset));
        let shares_before = total_shares(&env, &debt_asset);
        let bal_before = tok_balance(&env, &debt_asset);
        rd.borrow(&me(&env), &borrow); // health-checked by the live controller
        let borrowed = tok_balance(&env, &debt_asset).saturating_sub(bal_before);
        let added = rd.get_user_borrow_balance(&me(&env)).saturating_sub(pooled_before);
        if borrowed < borrow || added == 0 {
            panic_with_error!(&env, Error::BadAmount);
        }
        let new_shares = if shares_before == 0 || pooled_before == 0 {
            added
        } else {
            mul_div_up(added, shares_before, pooled_before)
        };
        env.storage()
            .instance()
            .set(&Key::DebtShares(debt_asset.clone()), &(shares_before + new_shares));

        let bought = swap(&env, &debt_asset, &held_asset, borrow, oracle_out.max(min_out));
        held_ptokens += deposit_self(&env, &held_asset, bought);

        let id: u64 = need::<u64>(&env, &Key::Counter) + 1;
        env.storage().instance().set(&Key::Counter, &id);
        let p = Position {
            owner: user.clone(),
            held_asset,
            debt_asset,
            held_ptokens,
            debt_shares: new_shares,
            opened_at: env.ledger().timestamp(),
            open: true,
        };
        // Post-trade LTV (actual fill and exchange rate) must still be inside the limit.
        if ltv(&env, &p, position_debt(&env, &p)) > rp.open_ltv {
            panic_with_error!(&env, Error::LtvTooHigh);
        }
        store(&env, id, &p);
        env.events()
            .publish((Symbol::new(&env, "margin_open"),), (id, user, borrow, bought));
        id
    }

    /// Close by selling just enough held collateral to repay the position's debt.
    /// The remaining held pTokens and surplus debt-asset proceeds go to the owner.
    pub fn close(env: Env, user: Address, id: u64, min_out: u128) {
        user.require_auth();
        bump(&env);
        let p = load(&env, id);
        if p.owner != user {
            panic_with_error!(&env, Error::NotOwner);
        }
        let debt = position_debt(&env, &p);
        let rh = ReceiptClient::new(&env, &receipt(&env, &p.held_asset));
        let slip: u128 = inst_get(&env, &Key::MaxSlippage).unwrap_or(10_000);
        let (pd, ph) = (price(&env, &p.debt_asset), price(&env, &p.held_asset));
        // held units needed at the oracle price, grossed up by the slippage bound
        let need_held = mul_div_up(
            mul_div_up(usd(debt, pd).saturating_add(1), ph.1, ph.0),
            SCALE_1E6,
            SCALE_1E6 - slip,
        );
        let rate = rh.get_exchange_rate();
        let sell = mul_div_up(need_held, SCALE_1E6, rate)
            .saturating_add(1)
            .min(p.held_ptokens);
        let before = tok_balance(&env, &p.held_asset);
        rh.withdraw(&me(&env), &sell);
        let got_held = tok_balance(&env, &p.held_asset).saturating_sub(before);
        let floor = oracle_min_out(&env, &p.held_asset, &p.debt_asset, got_held).max(min_out);
        let proceeds = swap(&env, &p.held_asset, &p.debt_asset, got_held, floor);
        if proceeds < debt {
            panic_with_error!(&env, Error::InsufficientSwapOutput); // use close_with_repay
        }
        let repaid = repay_self(&env, &p.debt_asset, debt);
        let surplus = proceeds - repaid;
        if surplus > 0 {
            token::TokenClient::new(&env, &p.debt_asset).transfer(&me(&env), &user, &i(&env, surplus));
        }
        send_ptokens(&env, &p.held_asset, &user, p.held_ptokens - sell);
        finish(&env, id, p);
        env.events()
            .publish((Symbol::new(&env, "margin_close"),), (id, repaid, surplus));
    }

    /// Close at any LTV by paying the debt from the owner's wallet; no swap.
    pub fn close_with_repay(env: Env, user: Address, id: u64) {
        user.require_auth();
        bump(&env);
        let p = load(&env, id);
        if p.owner != user {
            panic_with_error!(&env, Error::NotOwner);
        }
        let debt = position_debt(&env, &p);
        token::TokenClient::new(&env, &p.debt_asset).transfer(&user, &me(&env), &i(&env, debt));
        let repaid = repay_self(&env, &p.debt_asset, debt);
        if repaid < debt {
            token::TokenClient::new(&env, &p.debt_asset).transfer(&me(&env), &user, &i(&env, debt - repaid));
        }
        send_ptokens(&env, &p.held_asset, &user, p.held_ptokens);
        finish(&env, id, p);
        env.events()
            .publish((Symbol::new(&env, "margin_close"),), (id, repaid, 0u128));
    }

    /// Permissionless full liquidation at or above `liq_ltv`: the liquidator pays the
    /// whole position debt and receives held pTokens worth debt x bonus; the rest goes
    /// to the owner. An underwater position is seized in full (loss to the liquidator,
    /// i.e. the operator keeper at pilot size), never to lenders while it is solvent.
    pub fn liquidate(env: Env, liquidator: Address, id: u64) {
        liquidator.require_auth();
        bump(&env);
        let p = load(&env, id);
        let rp: RiskParams = need(&env, &Key::Risk(p.held_asset.clone()));
        let debt = position_debt(&env, &p);
        if ltv(&env, &p, debt) < rp.liq_ltv {
            panic_with_error!(&env, Error::NotLiquidatable);
        }
        token::TokenClient::new(&env, &p.debt_asset).transfer(&liquidator, &me(&env), &i(&env, debt));
        let repaid = repay_self(&env, &p.debt_asset, debt);
        if repaid < debt {
            token::TokenClient::new(&env, &p.debt_asset)
                .transfer(&me(&env), &liquidator, &i(&env, debt - repaid));
        }
        let bonus: u128 = inst_get(&env, &Key::LiqBonus).unwrap_or(1_050_000);
        let (pd, ph) = (price(&env, &p.debt_asset), price(&env, &p.held_asset));
        let seize_usd = usd(repaid, pd).checked_mul(bonus).expect("overflow") / SCALE_1E6;
        let seize_held = seize_usd.checked_mul(ph.1).expect("overflow") / ph.0;
        let rate = ReceiptClient::new(&env, &receipt(&env, &p.held_asset)).get_exchange_rate();
        let seize = (seize_held.checked_mul(SCALE_1E6).expect("overflow") / rate).min(p.held_ptokens);
        send_ptokens(&env, &p.held_asset, &liquidator, seize);
        send_ptokens(&env, &p.held_asset, &p.owner, p.held_ptokens - seize);
        let owner = p.owner.clone();
        finish(&env, id, p);
        env.events().publish(
            (Symbol::new(&env, "margin_liquidate"),),
            (id, owner, liquidator, repaid, seize),
        );
    }

    // ---- views
    pub fn get_position(env: Env, id: u64) -> Option<Position> {
        env.storage().persistent().get(&Key::Position(id))
    }
    pub fn get_position_debt(env: Env, id: u64) -> u128 {
        position_debt(&env, &load(&env, id))
    }
    pub fn get_position_ltv(env: Env, id: u64) -> u128 {
        let p = load(&env, id);
        let d = position_debt(&env, &p);
        ltv(&env, &p, d)
    }
    pub fn is_liquidatable(env: Env, id: u64) -> bool {
        let p = load(&env, id);
        let rp: RiskParams = need(&env, &Key::Risk(p.held_asset.clone()));
        let d = position_debt(&env, &p);
        ltv(&env, &p, d) >= rp.liq_ltv
    }
    pub fn opens_paused(env: Env) -> bool {
        inst_get(&env, &Key::OpensPaused).unwrap_or(true)
    }
    pub fn bump_ttl(env: Env, ids: Vec<u64>) {
        bump(&env);
        for id in ids.iter() {
            let k = Key::Position(id);
            if env.storage().persistent().has(&k) {
                env.storage().persistent().extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND_TO);
            }
        }
    }
}
