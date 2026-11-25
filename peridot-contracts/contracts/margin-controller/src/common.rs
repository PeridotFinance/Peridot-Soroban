//! WP0: shared helpers and live ABIs (moved verbatim from v0.2 contract.rs; helpers made pub(crate)).
//! Owned by the integrator. Add helpers only via the integrator.
use crate::types::*;
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{contractclient, panic_with_error, token, Address, Env, IntoVal, Symbol, Vec};
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
    fn update_interest(env: Env);
}

#[contractclient(name = "CtrlClient")]
pub trait Ctrl {
    fn enter_market(env: Env, user: Address, market: Address);
    fn get_price_usd(env: Env, token: Address) -> Option<(u128, u128)>;
    fn get_market_cf(env: Env, market: Address) -> u128;
    /// (rows: (market, ptoken_balance, debt, collateral_usd_after_cf, debt_usd), (coll_total, debt_total))
    fn portfolio(env: Env, user: Address)
        -> (Vec<(Address, u128, u128, u128, u128)>, (u128, u128));
    fn account_liquidity(env: Env, user: Address) -> (u128, u128);
}

/// Aquarius pool: `swap` does NOT require_auth; the input `transfer` carries the
/// caller's authorization, so that entry must sit at the root.
#[contractclient(name = "PoolClient")]
pub trait Pool {
    fn get_tokens(env: Env) -> Vec<Address>;
    fn swap(
        env: Env,
        user: Address,
        in_idx: u32,
        out_idx: u32,
        amount_in: u128,
        out_min: u128,
    ) -> u128;
}

// ---------------------------------------------------------------- helpers
pub(crate) fn inst_get<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    k: &Key,
) -> Option<T> {
    env.storage().instance().get(k)
}
pub(crate) fn need<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(env: &Env, k: &Key) -> T {
    inst_get(env, k).unwrap_or_else(|| panic_with_error!(env, Error::NotConfigured))
}
pub(crate) fn bump(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}
pub(crate) fn me(env: &Env) -> Address {
    env.current_contract_address()
}
pub(crate) fn require_admin(env: &Env, admin: &Address) {
    let stored: Address = need(env, &Key::Admin);
    if stored != *admin {
        panic_with_error!(env, Error::NotAdmin);
    }
    admin.require_auth();
}
pub(crate) fn receipt(env: &Env, asset: &Address) -> Address {
    need(env, &Key::Market(asset.clone()))
}
pub(crate) fn ctrl(env: &Env) -> CtrlClient<'_> {
    let a: Address = need(env, &Key::Ctrl);
    CtrlClient::new(env, &a)
}
pub(crate) fn price(env: &Env, asset: &Address) -> (u128, u128) {
    match ctrl(env).get_price_usd(asset) {
        Some((p, s)) if p > 0 && s > 0 => (p, s),
        _ => panic_with_error!(env, Error::PriceUnavailable),
    }
}
pub(crate) fn usd(amount: u128, p: (u128, u128)) -> u128 {
    amount.checked_mul(p.0).expect("overflow") / p.1
}
pub(crate) fn mul_div_up(a: u128, b: u128, d: u128) -> u128 {
    let n = a.checked_mul(b).expect("overflow");
    n / d + if n % d == 0 { 0 } else { 1 }
}
pub(crate) fn tok_balance(env: &Env, t: &Address) -> u128 {
    let b = token::TokenClient::new(env, t).balance(&me(env));
    if b < 0 {
        0
    } else {
        b as u128
    }
}
pub(crate) fn i(env: &Env, x: u128) -> i128 {
    if x > i128::MAX as u128 {
        panic_with_error!(env, Error::BadAmount);
    }
    x as i128
}

/// Root entry for `token.transfer(self -> to, amount)` performed inside the NEXT call.
pub(crate) fn authorize_transfer(env: &Env, tok: &Address, to: &Address, amount: u128) {
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

/// Deposit `amount` of `asset` (held by this contract) into its receipt without measuring the
/// mint; the caller brackets one or more deposits with `get_ptoken_balance` itself (WP6).
pub(crate) fn deposit_raw(env: &Env, asset: &Address, amount: u128) {
    let r = receipt(env, asset);
    authorize_transfer(env, asset, &r, amount);
    ReceiptClient::new(env, &r).deposit(&me(env), &amount);
}

pub(crate) fn deposit_self(env: &Env, asset: &Address, amount: u128) -> u128 {
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
pub(crate) fn repay_self(env: &Env, asset: &Address, amount: u128) -> u128 {
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

fn pool_idx(env: &Env, t: &Address) -> u32 {
    let toks: Vec<Address> = need(env, &Key::PoolTokens);
    for k in 0..toks.len() {
        if toks.get_unchecked(k) == *t {
            return k;
        }
    }
    panic_with_error!(env, Error::NotConfigured)
}

/// The pool's own quote of the `t_in` input needed to receive `out` of `t_out`
/// (Aquarius `estimate_swap_strict_receive`). Untrusted: callers bound it by the oracle.
/// `None` if the pool lacks the function or the call fails (the caller falls back).
pub(crate) fn pool_quote_in(env: &Env, t_in: &Address, t_out: &Address, out: u128) -> Option<u128> {
    let pool: Address = need(env, &Key::Pool);
    let args = (pool_idx(env, t_in), pool_idx(env, t_out), out).into_val(env);
    match env.try_invoke_contract::<u128, soroban_sdk::Error>(
        &pool,
        &Symbol::new(env, "estimate_swap_strict_receive"),
        args,
    ) {
        Ok(Ok(v)) if v > 0 => Some(v),
        _ => None,
    }
}

pub(crate) fn swap(
    env: &Env,
    t_in: &Address,
    t_out: &Address,
    amount_in: u128,
    min_out: u128,
) -> u128 {
    if amount_in == 0 || min_out == 0 {
        panic_with_error!(env, Error::BadAmount);
    }
    let pool: Address = need(env, &Key::Pool);
    let (ii, oi) = (pool_idx(env, t_in), pool_idx(env, t_out));
    let before = tok_balance(env, t_out);
    // Both shapes, as V3 (4c03928 perps.rs:2389): root transfer, and pool.swap with the
    // transfer nested. Mainnet simulation of the live CL pool records swap -> transfer.
    let xfer = InvokerContractAuthEntry::Contract(SubContractInvocation {
        context: ContractContext {
            contract: t_in.clone(),
            fn_name: Symbol::new(env, "transfer"),
            args: (me(env), pool.clone(), i(env, amount_in)).into_val(env),
        },
        sub_invocations: Vec::new(env),
    });
    let mut nested = Vec::new(env);
    nested.push_back(xfer.clone());
    let mut auths = Vec::new(env);
    auths.push_back(xfer);
    auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
        context: ContractContext {
            contract: pool.clone(),
            fn_name: Symbol::new(env, "swap"),
            args: (me(env), ii, oi, amount_in, min_out).into_val(env),
        },
        sub_invocations: nested,
    }));
    env.authorize_as_current_contract(auths);
    PoolClient::new(env, &pool).swap(&me(env), &ii, &oi, &amount_in, &min_out);
    let got = tok_balance(env, t_out).saturating_sub(before);
    if got < min_out {
        panic_with_error!(env, Error::Slippage);
    }
    got
}

pub(crate) fn oracle_min_out(env: &Env, t_in: &Address, t_out: &Address, amount_in: u128) -> u128 {
    oracle_min_out_p(env, amount_in, price(env, t_in), price(env, t_out))
}
/// `oracle_min_out` with already-loaded prices (WP6: no repeated price calls per invocation).
pub(crate) fn oracle_min_out_p(env: &Env, amount_in: u128, pi: (u128, u128), po: (u128, u128)) -> u128 {
    let slip: u128 = inst_get(env, &Key::MaxSlippage).unwrap_or(DEFAULT_MAX_SLIPPAGE);
    let out = usd(amount_in, pi)
        .checked_mul(po.1)
        .expect("overflow")
        / po.0;
    let m = out.checked_mul(SCALE_1E6 - slip).expect("overflow") / SCALE_1E6;
    if m == 0 {
        panic_with_error!(env, Error::BadAmount);
    }
    m
}

pub(crate) fn total_shares(env: &Env, debt: &Address) -> u128 {
    inst_get(env, &Key::DebtShares(debt.clone())).unwrap_or(0)
}
pub(crate) fn pooled_debt(env: &Env, debt: &Address) -> u128 {
    // Accrue first: the plain balance view can lag the index that the next
    // state-changing receipt call applies (observed on the Mainnet fork).
    let rc = ReceiptClient::new(env, &receipt(env, debt));
    rc.update_interest();
    rc.get_user_borrow_balance(&me(env))
}
/// Debt shares of all holders of `debt`: positions (`DebtShares`) + backstop (`BsDebtShares`).
pub(crate) fn all_shares(env: &Env, debt: &Address) -> u128 {
    let bs: u128 = inst_get(env, &Key::BsDebtShares(debt.clone())).unwrap_or(0);
    total_shares(env, debt).checked_add(bs).expect("overflow")
}
/// Position debt rounded up. Debt per share = pooled / (DebtShares + BsDebtShares), so moving
/// shares into the backstop bucket leaves every other position's debt unchanged. Only a holder
/// of ALL shares (positions and backstop) owes the whole pooled balance.
pub(crate) fn position_debt(env: &Env, p: &Position) -> u128 {
    let ts = all_shares(env, &p.debt_asset);
    if p.debt_shares == 0 || ts == 0 {
        return 0;
    }
    share_debt(p.debt_shares, pooled_debt(env, &p.debt_asset), ts)
}
/// Debt of `shares` out of `total` (all holders) given the pooled balance, rounded up.
pub(crate) fn share_debt(shares: u128, pooled: u128, total: u128) -> u128 {
    if shares == 0 || total == 0 {
        return 0;
    }
    if shares >= total {
        return pooled;
    }
    mul_div_up(shares, pooled, total).min(pooled)
}
pub(crate) fn held_underlying(env: &Env, p: &Position) -> u128 {
    let rate = ReceiptClient::new(env, &receipt(env, &p.held_asset)).get_exchange_rate();
    p.held_ptokens.checked_mul(rate).expect("overflow") / SCALE_1E6
}
pub(crate) fn ltv(env: &Env, p: &Position, debt: u128) -> u128 {
    let held_usd = usd(held_underlying(env, p), price(env, &p.held_asset));
    let debt_usd = usd(debt, price(env, &p.debt_asset));
    if held_usd == 0 {
        return u128::MAX;
    }
    mul_div_up(debt_usd, SCALE_1E6, held_usd)
}
pub(crate) fn load(env: &Env, id: u64) -> Position {
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
pub(crate) fn store(env: &Env, id: u64, p: &Position) {
    let k = Key::Position(id);
    env.storage().persistent().set(&k, p);
    env.storage()
        .persistent()
        .extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND_TO);
}
/// Close the position: remove its remaining debt shares from `DebtShares`, take exactly what
/// it added out of `SideTotals`, and store it closed. Never fails on aggregates (exits stay open).
pub(crate) fn finish(env: &Env, id: u64, p: Position) {
    let ts = total_shares(env, &p.debt_asset);
    env.storage().instance().set(
        &Key::DebtShares(p.debt_asset.clone()),
        &ts.saturating_sub(p.debt_shares),
    );
    let sk = Key::Side(p.held_asset.clone());
    if let Some(mut s) = inst_get::<SideTotals>(env, &sk) {
        s.equity_usd = s.equity_usd.saturating_sub(p.equity_usd);
        s.oi_usd = s.oi_usd.saturating_sub(p.notional_usd);
        s.open_positions = s.open_positions.saturating_sub(1);
        env.storage().instance().set(&sk, &s);
    }
    let mut q = p;
    q.open = false;
    q.held_ptokens = 0;
    q.debt_shares = 0;
    store(env, id, &q);
}
pub(crate) fn send_ptokens(env: &Env, asset: &Address, to: &Address, amount: u128) {
    if amount == 0 {
        return;
    }
    ReceiptClient::new(env, &receipt(env, asset)).transfer(&me(env), to, &i(env, amount));
}

// ---- WP0 additions: ledger bookkeeping (Ledger(asset) = pTokens this contract accounts for)
pub(crate) fn ledger(env: &Env, asset: &Address) -> u128 {
    inst_get(env, &Key::Ledger(asset.clone())).unwrap_or(0)
}
pub(crate) fn ledger_add(env: &Env, asset: &Address, pt: u128) {
    let v = ledger(env, asset).checked_add(pt).expect("overflow");
    env.storage()
        .instance()
        .set(&Key::Ledger(asset.clone()), &v);
}
pub(crate) fn ledger_sub(env: &Env, asset: &Address, pt: u128) {
    let v = ledger(env, asset)
        .checked_sub(pt)
        .unwrap_or_else(|| panic_with_error!(env, Error::BadAmount));
    env.storage()
        .instance()
        .set(&Key::Ledger(asset.clone()), &v);
}
pub(crate) fn usdc(env: &Env) -> Address {
    need(env, &Key::Usdc)
}
pub(crate) fn xlm(env: &Env) -> Address {
    need(env, &Key::Xlm)
}
pub(crate) fn fees(env: &Env) -> Fees {
    need(env, &Key::Fees)
}
pub(crate) fn is_impaired(env: &Env) -> bool {
    inst_get(env, &Key::Impaired).unwrap_or(false)
}
pub(crate) fn set_impaired(env: &Env, v: bool) {
    env.storage().instance().set(&Key::Impaired, &v);
}
