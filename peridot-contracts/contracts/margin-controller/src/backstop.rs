//! WP1: backstop module. Signatures frozen in WP0; bodies owned by WP1.
//!
//! The backstop holds USDC pTokens only, tracked in `Bs` (never `balanceOf`, so donations are
//! ignored). Its NAV is oracle-free unless the backstop owns XLM bad debt; while it does,
//! deposits and exits are frozen until `settle_backstop_debt` clears it. Rounding always
//! favours the pool: shares minted down, pTokens redeemed down, bad debt rounded up.
use crate::common::*;
use crate::{MarginController, MarginControllerArgs, MarginControllerClient};
use crate::types::*;
use soroban_sdk::{contractimpl, panic_with_error, Address, Env, Symbol};

const MIN_PERIOD_S: u64 = 3_600;

// ---------------------------------------------------------------- internal helpers
pub(crate) fn bs_get(env: &Env) -> Backstop {
    inst_get(env, &Key::Bs).unwrap_or(Backstop { shares: 0, ptokens: 0, pending_shares: 0 })
}
fn bs_put(env: &Env, b: &Backstop) {
    env.storage().instance().set(&Key::Bs, b);
}
fn bs_debt_shares(env: &Env, asset: &Address) -> u128 {
    inst_get(env, &Key::BsDebtShares(asset.clone())).unwrap_or(0)
}
fn set_bs_debt_shares(env: &Env, asset: &Address, v: u128) {
    env.storage().instance().set(&Key::BsDebtShares(asset.clone()), &v);
}
fn backer_shares(env: &Env, backer: &Address) -> u128 {
    env.storage().persistent().get(&Key::Backer(backer.clone())).unwrap_or(0)
}
fn set_backer_shares(env: &Env, backer: &Address, v: u128) {
    let k = Key::Backer(backer.clone());
    env.storage().persistent().set(&k, &v);
    env.storage().persistent().extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND_TO);
}
fn get_req(env: &Env, backer: &Address) -> Option<WithdrawReq> {
    env.storage().persistent().get(&Key::WithdrawReq(backer.clone()))
}
fn put_req(env: &Env, backer: &Address, r: &WithdrawReq) {
    let k = Key::WithdrawReq(backer.clone());
    env.storage().persistent().set(&k, r);
    env.storage().persistent().extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND_TO);
}
fn add(env: &Env, a: u128, b: u128) -> u128 {
    a.checked_add(b).unwrap_or_else(|| panic_with_error!(env, Error::BadAmount))
}
fn sub(env: &Env, a: u128, b: u128) -> u128 {
    a.checked_sub(b).unwrap_or_else(|| panic_with_error!(env, Error::BadAmount))
}
fn mul_div_down(a: u128, b: u128, d: u128) -> u128 {
    a.checked_mul(b).expect("overflow") / d
}
fn usdc_rate(env: &Env) -> u128 {
    let r = ReceiptClient::new(env, &receipt(env, &usdc(env))).get_exchange_rate();
    if r == 0 {
        panic_with_error!(env, Error::BadParams);
    }
    r
}
/// USDC underlying -> USDC pTokens, rounded up.
fn to_pt_up(amount: u128, rate: u128) -> u128 {
    mul_div_up(amount, SCALE_1E6, rate)
}
/// Backstop debt in `asset` (raw debt units, rounded up), with the pooled balance and the
/// summed share divisor: (debt, pooled, DebtShares + BsDebtShares, BsDebtShares).
/// No receipt calls when the backstop owns no shares of `asset`.
fn bs_debt(env: &Env, asset: &Address) -> (u128, u128, u128, u128) {
    let s = bs_debt_shares(env, asset);
    if s == 0 {
        return (0, 0, 0, 0);
    }
    let pooled = pooled_debt(env, asset);
    let total = total_shares(env, asset).checked_add(s).expect("overflow");
    let debt = if pooled == 0 {
        0
    } else if s >= total {
        pooled
    } else {
        mul_div_up(s, pooled, total).min(pooled)
    };
    (debt, pooled, total, s)
}
/// USD value of `amount` XLM converted to USDC underlying, every step rounded up.
fn xlm_to_usdc_up(env: &Env, amount: u128) -> u128 {
    xlm_to_usdc_up_p(amount, price(env, &xlm(env)), price(env, &usdc(env)))
}
fn xlm_to_usdc_up_p(amount: u128, px: (u128, u128), pu: (u128, u128)) -> u128 {
    let usd_up = mul_div_up(amount, px.0, px.1);
    mul_div_up(usd_up, pu.1, pu.0)
}

// ---- WP6 settle sizing
/// Haircut on the controller's free liquidity when sizing a settle withdrawal (1 %).
const LIQ_HAIRCUT: u128 = 10_000;
/// Smallest XLM purchase of a settle swap (0.1 XLM); the excess over the debt is kept as backstop
/// XLM pTokens. Keeps a dust tail of XLM bad debt from needing a zero-floor swap.
const SETTLE_MIN_XLM_OUT: u128 = 1_000_000;

pub(crate) fn bs_xlm_pt(env: &Env) -> u128 {
    inst_get(env, &Key::BsXlmPt).unwrap_or(0)
}
fn set_bs_xlm_pt(env: &Env, v: u128) {
    env.storage().instance().set(&Key::BsXlmPt, &v);
}
/// XLM the settle swap buys for `d_rem` of XLM debt: the debt + 0.05 % + 10 stroops, at least
/// SETTLE_MIN_XLM_OUT.
fn xlm_want(d_rem: u128) -> u128 {
    d_rem
        .saturating_add(d_rem / 2_000)
        .saturating_add(10)
        .max(SETTLE_MIN_XLM_OUT)
}
/// USDC pTokens the pooled account can withdraw now and still pass the live controller's
/// health check: `account_liquidity(me)` (after accruing `other`, the receipt `bs_debt` did not
/// accrue) at the USDC CF, price and exchange rate, less LIQ_HAIRCUT. 0 under a shortfall.
fn liquidity_cap_pt(env: &Env, other: &Address, rate: u128, pu: (u128, u128)) -> u128 {
    ReceiptClient::new(env, &receipt(env, other)).update_interest();
    let c = ctrl(env);
    let (liq, _) = c.account_liquidity(&me(env));
    let cf = c.get_market_cf(&receipt(env, &usdc(env)));
    if liq == 0 || cf == 0 {
        return 0;
    }
    let v = mul_div_down(liq, SCALE_1E6 - LIQ_HAIRCUT, SCALE_1E6);
    let underlying = mul_div_down(mul_div_down(v, SCALE_1E6, cf), pu.1, pu.0);
    mul_div_down(underlying, SCALE_1E6, rate)
}
/// Backstop USDC pTokens to withdraw for `d_rem` of `debt_asset` debt (before max_ptokens,
/// Bs.ptokens and the liquidity cap). USDC: the debt at the exchange rate. XLM: the pool's
/// strict-receive quote for `xlm_want(d_rem)`, never above the oracle value of that amount plus
/// the max-slippage buffer (so a permissionless caller or a bad quote cannot push more of the
/// backstop through the pool); the oracle bound alone if the pool gives no quote.
fn settle_need_pt(env: &Env, is_usdc: bool, d_rem: u128, rate: u128, pu: (u128, u128), px: (u128, u128)) -> u128 {
    if is_usdc {
        return to_pt_up(d_rem, rate);
    }
    let slip: u128 = inst_get(env, &Key::MaxSlippage).unwrap_or(DEFAULT_MAX_SLIPPAGE);
    let want = xlm_want(d_rem);
    let bound = mul_div_up(xlm_to_usdc_up_p(want, px, pu), SCALE_1E6 + slip, SCALE_1E6);
    let usdc_in = match pool_quote_in(env, &usdc(env), &xlm(env), want) {
        Some(q) => q.saturating_add(1).min(bound),
        None => bound,
    };
    // +1 pT: the receipt's withdrawal rounds down
    to_pt_up(usdc_in, rate).saturating_add(1)
}

/// Backstop exit pool check (headroom::pool_check, Check::BackstopExit), always.
pub(crate) fn exit_check(env: &Env) {
    crate::headroom::pool_check(env, Check::BackstopExit);
}

#[allow(deprecated)]
fn emit<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, topic: &str, data: T) {
    env.events().publish((Symbol::new(env, topic),), data);
}

// ---- crate-internal API used by WP2 / WP3
/// Credit `usdc_pt` USDC pTokens (already held by this contract and already added to
/// Ledger(USDC) by the caller) to the backstop as fee income.
///
/// Never panics (closes and liquidations call it): saturating, and an empty backstop (no
/// shares yet) simply accumulates the pTokens, which the later seed's shares then own.
pub(crate) fn credit_pt(env: &Env, usdc_pt: u128) {
    if usdc_pt == 0 {
        return;
    }
    let mut b = bs_get(env);
    b.ptokens = b.ptokens.saturating_add(usdc_pt);
    bs_put(env, &b);
}
/// Move `shares` debt shares of `debt_asset` from a position (caller already removed them
/// from DebtShares) into BsDebtShares(debt_asset).
pub(crate) fn assign_bad_debt(env: &Env, debt_asset: &Address, shares: u128) {
    if shares == 0 {
        return;
    }
    let v = add(env, bs_debt_shares(env, debt_asset), shares);
    set_bs_debt_shares(env, debt_asset, v);
}
/// After bad debt has been assigned AND the position finished (so the share divisor no longer
/// holds the burned shares): if the backstop is exhausted (nav_pt(false) < 0), set Impaired,
/// pause opens and emit margin_impaired (once). Values XLM bad debt at the oracle.
pub(crate) fn impair_if_exhausted(env: &Env, caller: &Address) {
    if !is_impaired(env) && nav_pt(env, false) < 0 {
        crate::headroom::mark_impaired(env, caller);
    }
}
/// Net backstop USDC pTokens: Bs.ptokens - USDC value of BsDebtShares(USDC), rounded against
/// the pool. Negative if exhausted. Panics BackstopDebtOutstanding if BsDebtShares(XLM) > 0
/// only when `strict` (deposit/withdraw); otherwise values XLM bad debt at the oracle.
pub(crate) fn nav_pt(env: &Env, strict: bool) -> i128 {
    let x = xlm(env);
    if strict && bs_debt_shares(env, &x) > 0 {
        panic_with_error!(env, Error::BackstopDebtOutstanding);
    }
    let b = bs_get(env);
    let mut debt_pt: u128 = 0;
    let mut rate: Option<u128> = None;
    let (du, _, _, _) = bs_debt(env, &usdc(env));
    if du > 0 {
        let r = usdc_rate(env);
        rate = Some(r);
        debt_pt = to_pt_up(du, r);
    }
    if !strict {
        let (dx, _, _, _) = bs_debt(env, &x);
        if dx > 0 {
            let r = rate.unwrap_or_else(|| usdc_rate(env));
            let extra = to_pt_up(xlm_to_usdc_up(env, dx), r);
            debt_pt = debt_pt.checked_add(extra).expect("overflow");
        }
    }
    i(env, b.ptokens).checked_sub(i(env, debt_pt)).expect("overflow")
}
/// USDC pTokens equivalent of Bs.pending_shares at the current NAV (>= 0).
/// Rounded up (it is excluded from open headroom, so overstating it is the safe side).
#[cfg(test)]
pub(crate) fn pending_pt(env: &Env) -> u128 {
    pending_pt_at(env, nav_pt(env, false))
}
/// `pending_pt` at an already-computed `nav_pt(false)` (WP6: the pool check reads the NAV once).
pub(crate) fn pending_pt_at(env: &Env, nav: i128) -> u128 {
    let b = bs_get(env);
    if b.pending_shares == 0 || b.shares == 0 || nav <= 0 {
        return 0;
    }
    let nav = nav as u128;
    mul_div_up(b.pending_shares, nav, b.shares).min(nav)
}
/// Backstop value in usd7 (nav_pt * exchange rate * USDC price); 0 if negative.
#[cfg(test)]
pub(crate) fn value_usd(env: &Env) -> u128 {
    value_usd_at(env, nav_pt(env, false), None)
}
/// `value_usd` at an already-computed NAV, with the USDC (exchange rate, price) the caller
/// already loaded in this invocation (`None` reads them).
pub(crate) fn value_usd_at(env: &Env, nav: i128, pre: Option<(u128, (u128, u128))>) -> u128 {
    if nav <= 0 {
        return 0;
    }
    let (rate, pu) = pre.unwrap_or_else(|| (usdc_rate(env), price(env, &usdc(env))));
    let underlying = mul_div_down(nav as u128, rate, SCALE_1E6);
    usd(underlying, pu)
}

#[contractimpl]
impl MarginController {
    pub fn set_backstop_params(env: Env, admin: Address, p: BackstopParams) {
        require_admin(&env, &admin);
        if p.cooldown_s < MIN_PERIOD_S || p.window_s < MIN_PERIOD_S || p.min_deposit_pt == 0 {
            panic_with_error!(&env, Error::BadParams);
        }
        env.storage().instance().set(&Key::BsParams, &p);
        bump(&env);
    }

    /// First deposit must be by the admin (treasury seed) and mints BS_DEAD_SHARES to the contract.
    pub fn backstop_deposit(env: Env, backer: Address, ptokens: u128, min_shares: u128) -> u128 {
        backer.require_auth();
        if is_impaired(&env) {
            panic_with_error!(&env, Error::Impaired);
        }
        let x = xlm(&env);
        if bs_debt_shares(&env, &x) > 0 {
            panic_with_error!(&env, Error::BackstopDebtOutstanding);
        }
        let params: BackstopParams = need(&env, &Key::BsParams);
        if ptokens == 0 || ptokens < params.min_deposit_pt {
            panic_with_error!(&env, Error::BadAmount);
        }
        let u = usdc(&env);
        let mut b = bs_get(&env);
        let minted = if b.shares == 0 {
            // Seed: admin only, 1:1, plus dead shares held by this contract (never redeemable).
            let admin: Address = need(&env, &Key::Admin);
            if backer != admin {
                panic_with_error!(&env, Error::NotAdmin);
            }
            b.shares = add(&env, ptokens, BS_DEAD_SHARES);
            let dead = add(&env, backer_shares(&env, &me(&env)), BS_DEAD_SHARES);
            set_backer_shares(&env, &me(&env), dead);
            ptokens
        } else {
            let nav = nav_pt(&env, true);
            if nav <= 0 {
                panic_with_error!(&env, Error::BackstopEmpty);
            }
            let m = mul_div_down(ptokens, b.shares, nav as u128);
            if m == 0 {
                panic_with_error!(&env, Error::BadAmount);
            }
            b.shares = add(&env, b.shares, m);
            m
        };
        if minted < min_shares {
            panic_with_error!(&env, Error::MinShares);
        }
        ReceiptClient::new(&env, &receipt(&env, &u)).transfer(&backer, &me(&env), &i(&env, ptokens));
        ledger_add(&env, &u, ptokens);
        b.ptokens = add(&env, b.ptokens, ptokens);
        bs_put(&env, &b);
        let bal = add(&env, backer_shares(&env, &backer), minted);
        set_backer_shares(&env, &backer, bal);
        emit(&env, "bs_deposit", (backer, ptokens, minted));
        bump(&env);
        minted
    }

    /// One request per backer. A request whose window has passed is cancelled automatically
    /// (its shares returned) before the new one is recorded; a live request is refused (BadParams).
    /// Not gated by Impaired / XLM bad debt: it only queues.
    pub fn backstop_request_withdraw(env: Env, backer: Address, shares: u128) {
        backer.require_auth();
        if shares == 0 {
            panic_with_error!(&env, Error::BadAmount);
        }
        let now = env.ledger().timestamp();
        let mut b = bs_get(&env);
        let mut bal = backer_shares(&env, &backer);
        if let Some(old) = get_req(&env, &backer) {
            if now <= old.expires_at {
                panic_with_error!(&env, Error::BadParams);
            }
            bal = add(&env, bal, old.shares);
            b.pending_shares = sub(&env, b.pending_shares, old.shares);
            emit(&env, "bs_cancel", (backer.clone(), old.shares));
        }
        if shares > bal {
            panic_with_error!(&env, Error::BadAmount);
        }
        let params: BackstopParams = need(&env, &Key::BsParams);
        let ready_at = now.saturating_add(params.cooldown_s);
        let expires_at = ready_at.saturating_add(params.window_s);
        set_backer_shares(&env, &backer, bal - shares);
        b.pending_shares = add(&env, b.pending_shares, shares);
        bs_put(&env, &b);
        put_req(&env, &backer, &WithdrawReq { shares, ready_at, expires_at });
        emit(&env, "bs_request", (backer, shares, ready_at, expires_at));
        bump(&env);
    }

    /// Always available (also after expiry, while Impaired or frozen).
    pub fn backstop_cancel_withdraw(env: Env, backer: Address) {
        backer.require_auth();
        let r = get_req(&env, &backer).unwrap_or_else(|| panic_with_error!(&env, Error::NoRequest));
        let mut b = bs_get(&env);
        b.pending_shares = sub(&env, b.pending_shares, r.shares);
        bs_put(&env, &b);
        let bal = add(&env, backer_shares(&env, &backer), r.shares);
        set_backer_shares(&env, &backer, bal);
        env.storage().persistent().remove(&Key::WithdrawReq(backer.clone()));
        emit(&env, "bs_cancel", (backer, r.shares));
        bump(&env);
    }

    /// After ready_at and before expires_at; pays at execution-time NAV; then
    /// headroom::pool_check(Check::BackstopExit). Returns USDC pTokens paid.
    pub fn backstop_withdraw(env: Env, backer: Address, min_ptokens: u128) -> u128 {
        backer.require_auth();
        let r = get_req(&env, &backer).unwrap_or_else(|| panic_with_error!(&env, Error::NoRequest));
        // CAP-88: timestamps are whole seconds and can repeat; both bounds are inclusive.
        let now = env.ledger().timestamp();
        if now < r.ready_at {
            panic_with_error!(&env, Error::Cooldown);
        }
        if now > r.expires_at {
            panic_with_error!(&env, Error::WindowExpired);
        }
        if is_impaired(&env) {
            panic_with_error!(&env, Error::Impaired);
        }
        let nav = nav_pt(&env, true);
        if nav <= 0 {
            panic_with_error!(&env, Error::BackstopEmpty);
        }
        let mut b = bs_get(&env);
        let pt = mul_div_down(r.shares, nav as u128, b.shares);
        if pt == 0 {
            panic_with_error!(&env, Error::BadAmount);
        }
        if pt < min_ptokens {
            panic_with_error!(&env, Error::MinPtokens);
        }
        b.shares = sub(&env, b.shares, r.shares);
        b.pending_shares = sub(&env, b.pending_shares, r.shares);
        b.ptokens = sub(&env, b.ptokens, pt);
        bs_put(&env, &b);
        env.storage().persistent().remove(&Key::WithdrawReq(backer.clone()));
        let u = usdc(&env);
        ledger_sub(&env, &u, pt);
        send_ptokens(&env, &u, &backer, pt);
        exit_check(&env);
        emit(&env, "bs_withdraw", (backer, r.shares, pt));
        bump(&env);
        pt
    }

    /// Permissionless: return the shares of an EXPIRED request (now > expires_at) to its backer,
    /// so a stale request stops counting as pending (it is excluded from open headroom). Live
    /// requests are refused (BadParams). The backer's balance only ever grows here.
    pub fn backstop_clear_expired(env: Env, caller: Address, backer: Address) {
        caller.require_auth();
        let r = get_req(&env, &backer).unwrap_or_else(|| panic_with_error!(&env, Error::NoRequest));
        if env.ledger().timestamp() <= r.expires_at {
            panic_with_error!(&env, Error::BadParams);
        }
        let mut b = bs_get(&env);
        b.pending_shares = sub(&env, b.pending_shares, r.shares);
        bs_put(&env, &b);
        let bal = add(&env, backer_shares(&env, &backer), r.shares);
        set_backer_shares(&env, &backer, bal);
        env.storage().persistent().remove(&Key::WithdrawReq(backer.clone()));
        emit(&env, "bs_cancel", (backer, r.shares));
        bump(&env);
    }

    /// Admin recapitalisation: `ptokens` USDC pTokens from the admin into the backstop WITHOUT
    /// minting shares (a gift to the existing shares). Works in every state an exhausted
    /// backstop can be in: NAV <= 0, Impaired, XLM bad debt outstanding (no NAV read, no oracle).
    /// Needs an existing backstop (shares > 0); the first deposit is the seed (backstop_deposit).
    /// Clearing Impaired stays admin_resolve_impairment. Returns Bs.ptokens afterwards.
    pub fn admin_recapitalize(env: Env, admin: Address, ptokens: u128) -> u128 {
        require_admin(&env, &admin);
        if ptokens == 0 {
            panic_with_error!(&env, Error::BadAmount);
        }
        let mut b = bs_get(&env);
        if b.shares == 0 {
            panic_with_error!(&env, Error::BackstopEmpty);
        }
        let u = usdc(&env);
        ReceiptClient::new(&env, &receipt(&env, &u)).transfer(&admin, &me(&env), &i(&env, ptokens));
        ledger_add(&env, &u, ptokens);
        b.ptokens = add(&env, b.ptokens, ptokens);
        bs_put(&env, &b);
        emit(&env, "bs_recap", (admin, ptokens, b.ptokens));
        bump(&env);
        b.ptokens
    }

    /// Permissionless: repay backstop-owned bad debt from backstop USDC pTokens (XLM: swap).
    /// Returns debt repaid (raw debt units). Not gated by Impaired / the XLM freeze: it is the
    /// recovery path. Repays at most the backstop's own debt; never other positions' debt.
    ///
    /// WP6 (25 Nov):
    /// - Self-sizing: the USDC withdrawal (health-checked by the live controller BEFORE the
    ///   repay) is capped at what the pooled account's free liquidity allows (`account_liquidity`,
    ///   1 % haircut). Under a controller shortfall nothing can be withdrawn: `PooledShortfall`.
    ///   Each settle frees liquidity (repaid debt > CF-weighted collateral removed), so the keeper
    ///   repeats it until `get_bs_debt_shares` is 0; `settle_max_ptokens` is the sizing view.
    /// - XLM: the swap input comes from the pool's strict-receive quote for the debt + 0.05 %
    ///   (bounded by the oracle + max slippage), the output is still floored at the oracle less
    ///   max slippage, and any XLM left after the repay is deposited as backstop XLM pTokens
    ///   (`BsXlmPt`, in Ledger(XLM)); the next XLM settle repays from those first, without a swap.
    pub fn settle_backstop_debt(env: Env, caller: Address, debt_asset: Address, max_ptokens: u128, min_out: u128) -> u128 {
        caller.require_auth();
        let u = usdc(&env);
        let x = xlm(&env);
        let is_usdc = debt_asset == u;
        if !is_usdc && debt_asset != x {
            panic_with_error!(&env, Error::BadParams);
        }
        let (d, pooled, total, s) = bs_debt(&env, &debt_asset);
        if s == 0 {
            return 0;
        }
        if d == 0 {
            // Nothing owed on the receipt: the shares are worthless; clear them.
            set_bs_debt_shares(&env, &debt_asset, 0);
            emit(&env, "bs_settle", (debt_asset, 0u128, 0u128, s, 0u128));
            bump(&env);
            return 0;
        }
        let mut repaid: u128 = 0;
        // ---- XLM: the backstop's own XLM pTokens first (an earlier settle's surplus)
        if !is_usdc {
            let bx = bs_xlm_pt(&env);
            if bx > 0 {
                let rx = ReceiptClient::new(&env, &receipt(&env, &x));
                let w = bx.min(to_pt_up(d, rx.get_exchange_rate()).saturating_add(1));
                let before = tok_balance(&env, &x);
                rx.withdraw(&me(&env), &w);
                let got = tok_balance(&env, &x).saturating_sub(before);
                set_bs_xlm_pt(&env, bx - w);
                ledger_sub(&env, &x, w);
                repaid = repay_self(&env, &x, got.min(d));
            }
        }
        // ---- backstop USDC pTokens (XLM: swapped on the pool)
        let mut pt: u128 = 0;
        let mut surplus: u128 = 0;
        let d_rem = d - repaid;
        if d_rem > 0 {
            let rate = usdc_rate(&env);
            let pu = price(&env, &u);
            let px = if is_usdc { pu } else { price(&env, &x) };
            let need_pt = settle_need_pt(&env, is_usdc, d_rem, rate, pu, px);
            let cap = liquidity_cap_pt(&env, if is_usdc { &x } else { &u }, rate, pu);
            let mut b = bs_get(&env);
            pt = need_pt.min(max_ptokens).min(b.ptokens).min(cap);
            if pt == 0 {
                if cap == 0 {
                    panic_with_error!(&env, Error::PooledShortfall);
                }
                panic_with_error!(&env, Error::BadAmount);
            }
            let before = tok_balance(&env, &u);
            ReceiptClient::new(&env, &receipt(&env, &u)).withdraw(&me(&env), &pt);
            let got = tok_balance(&env, &u).saturating_sub(before);
            if got == 0 {
                panic_with_error!(&env, Error::BadAmount);
            }
            b.ptokens = sub(&env, b.ptokens, pt);
            bs_put(&env, &b);
            ledger_sub(&env, &u, pt);
            let amount = if is_usdc {
                got
            } else {
                let floor = oracle_min_out_p(&env, got, pu, px).max(min_out);
                swap(&env, &u, &x, got, floor)
            };
            let r = repay_self(&env, &debt_asset, amount.min(d_rem));
            repaid = add(&env, repaid, r);
            surplus = amount - r;
            // XLM left over is kept, tracked, as backstop XLM pTokens (sub-pToken dust stays idle).
            if !is_usdc && surplus > 0 {
                let rx = ReceiptClient::new(&env, &receipt(&env, &x));
                if mul_div_down(surplus, SCALE_1E6, rx.get_exchange_rate()) > 0 {
                    let minted = deposit_self(&env, &x, surplus);
                    ledger_add(&env, &x, minted);
                    set_bs_xlm_pt(&env, add(&env, bs_xlm_pt(&env), minted));
                }
            }
        }
        // Burn rounded down, so the other positions' debt per share can never rise.
        let burned = if repaid >= d { s } else { mul_div_down(repaid, total, pooled).min(s) };
        set_bs_debt_shares(&env, &debt_asset, s - burned);
        emit(&env, "bs_settle", (debt_asset, pt, repaid, burned, surplus));
        bump(&env);
        repaid
    }

    /// Keeper sizing view (WP6): the backstop USDC pTokens `settle_backstop_debt(.., u128::MAX, ..)`
    /// would withdraw now, after the pooled-liquidity cap. 0 = nothing to withdraw: no backstop
    /// debt, the backstop's XLM pTokens cover it, or a controller shortfall (recapitalise first).
    /// Conservative: it ignores the liquidity a first repay from backstop XLM pTokens frees.
    pub fn settle_max_ptokens(env: Env, debt_asset: Address) -> u128 {
        let (u, x) = (usdc(&env), xlm(&env));
        let is_usdc = debt_asset == u;
        if !is_usdc && debt_asset != x {
            panic_with_error!(&env, Error::BadParams);
        }
        let (d, _, _, _) = bs_debt(&env, &debt_asset);
        let d_rem = if is_usdc {
            d
        } else {
            let rate_x = ReceiptClient::new(&env, &receipt(&env, &x)).get_exchange_rate();
            d.saturating_sub(mul_div_down(bs_xlm_pt(&env), rate_x, SCALE_1E6))
        };
        if d_rem == 0 {
            return 0;
        }
        let rate = usdc_rate(&env);
        let pu = price(&env, &u);
        let px = if is_usdc { pu } else { price(&env, &x) };
        let need_pt = settle_need_pt(&env, is_usdc, d_rem, rate, pu, px);
        let cap = liquidity_cap_pt(&env, if is_usdc { &x } else { &u }, rate, pu);
        need_pt.min(bs_get(&env).ptokens).min(cap)
    }

    /// Backstop-owned XLM pTokens (XLM-settle surplus, consumed by the next XLM settle).
    pub fn get_bs_xlm_ptokens(env: Env) -> u128 {
        bs_xlm_pt(&env)
    }

    pub fn get_backstop(env: Env) -> Backstop {
        bs_get(&env)
    }
    /// (nav_pt, shares). Non-strict: values XLM bad debt at the oracle.
    pub fn backstop_nav(env: Env) -> (i128, u128) {
        (nav_pt(&env, false), bs_get(&env).shares)
    }
    pub fn get_backer(env: Env, backer: Address) -> u128 {
        backer_shares(&env, &backer)
    }
    pub fn get_withdraw_req(env: Env, backer: Address) -> Option<WithdrawReq> {
        get_req(&env, &backer)
    }
    pub fn get_bs_debt_shares(env: Env, debt_asset: Address) -> u128 {
        bs_debt_shares(&env, &debt_asset)
    }
}
