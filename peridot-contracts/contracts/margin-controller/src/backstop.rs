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
    let px = price(env, &xlm(env));
    let pu = price(env, &usdc(env));
    let usd_up = mul_div_up(amount, px.0, px.1);
    mul_div_up(usd_up, pu.1, pu.0)
}

/// Backstop exit pool check. `headroom::pool_check` belongs to WP3 and is still
/// `unimplemented!` on this branch, so native unit tests of this crate skip it.
/// The integrator removes the `cfg(test)` bypass once WP3 lands.
pub(crate) fn exit_check(env: &Env) {
    #[cfg(not(test))]
    crate::headroom::pool_check(env, Check::BackstopExit);
    #[cfg(test)]
    let _ = env;
}

#[allow(deprecated)]
fn emit<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, topic: &str, data: T) {
    env.events().publish((Symbol::new(env, topic),), data);
}

// ---- crate-internal API used by WP2 / WP3
/// Credit `usdc_pt` USDC pTokens (already held by this contract and already added to
/// Ledger(USDC) by the caller) to the backstop as fee income.
pub(crate) fn credit_pt(env: &Env, usdc_pt: u128) {
    if usdc_pt == 0 {
        return;
    }
    let mut b = bs_get(env);
    b.ptokens = add(env, b.ptokens, usdc_pt);
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
pub(crate) fn pending_pt(env: &Env) -> u128 {
    let b = bs_get(env);
    if b.pending_shares == 0 || b.shares == 0 {
        return 0;
    }
    let nav = nav_pt(env, false);
    if nav <= 0 {
        return 0;
    }
    let nav = nav as u128;
    mul_div_up(b.pending_shares, nav, b.shares).min(nav)
}
/// Backstop value in usd7 (nav_pt * exchange rate * USDC price); 0 if negative.
pub(crate) fn value_usd(env: &Env) -> u128 {
    let nav = nav_pt(env, false);
    if nav <= 0 {
        return 0;
    }
    let underlying = mul_div_down(nav as u128, usdc_rate(env), SCALE_1E6);
    usd(underlying, price(env, &usdc(env)))
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

    /// Permissionless: repay backstop-owned bad debt from backstop USDC pTokens (XLM: swap).
    /// Returns debt repaid (raw debt units). Not gated by Impaired / the XLM freeze: it is the
    /// recovery path. Repays at most the backstop's own debt; never other positions' debt.
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
        let rate = usdc_rate(&env);
        // pTokens needed. XLM: oracle USDC value of d plus the max-slippage buffer, so a
        // permissionless caller cannot push more of the backstop through the pool than that.
        let need_pt = if is_usdc {
            to_pt_up(d, rate)
        } else {
            let slip: u128 = inst_get(&env, &Key::MaxSlippage).unwrap_or(10_000);
            let usdc_need = xlm_to_usdc_up(&env, d);
            to_pt_up(mul_div_up(usdc_need, SCALE_1E6 + slip, SCALE_1E6), rate)
        };
        let mut b = bs_get(&env);
        let pt = need_pt.min(max_ptokens).min(b.ptokens);
        if pt == 0 {
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
            let floor = oracle_min_out(&env, &u, &x, got).max(min_out);
            swap(&env, &u, &x, got, floor)
        };
        let repaid = repay_self(&env, &debt_asset, amount.min(d));
        // Burn rounded down, so the other positions' debt per share can never rise.
        let burned = if repaid >= d { s } else { mul_div_down(repaid, total, pooled).min(s) };
        set_bs_debt_shares(&env, &debt_asset, s - burned);
        // Surplus (raw debt-asset units) stays idle and untracked in this contract.
        let surplus = amount.saturating_sub(repaid);
        emit(&env, "bs_settle", (debt_asset, pt, repaid, burned, surplus));
        bump(&env);
        repaid
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
