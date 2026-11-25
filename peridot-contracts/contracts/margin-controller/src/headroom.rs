//! WP3: pool check, required backstop size, reconcile / Impaired. Signatures frozen in WP0.
//!
//! Units: USD values are "usd7" (what the controller's `portfolio` returns for 7-decimal
//! tokens), fractions and HF are 1e6-scaled.
use crate::common::*;
use crate::contract::{MarginController, MarginControllerArgs, MarginControllerClient};
use crate::types::*;
use soroban_sdk::{contractevent, contractimpl, panic_with_error, Address, Env};

/// Close-reserve safety margin when derived from caps (INTERFACE rule 5): 1.01x.
const RESERVE_MARGIN: u128 = 1_010_000;

// ---------------------------------------------------------------- events
#[contractevent(topics = ["margin_impaired"], data_format = "vec")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarginImpaired {
    pub caller: Address,
    pub usdc_ledger: u128,
    pub usdc_balance: u128,
    pub xlm_ledger: u128,
    pub xlm_balance: u128,
}

#[contractevent(topics = ["margin_resolved"], data_format = "vec")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarginResolved {
    pub usdc_pt_charge: u128,
    pub xlm_pt_charge: u128,
    pub cleared: bool,
}

// ---------------------------------------------------------------- pooled snapshot
/// Pooled account as the live controller sees it (portfolio rows, CF-weighted collateral).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Book {
    pub cx: u128,     // XLM row coll_usd_cf
    pub dx: u128,     // XLM row debt_usd
    pub cu: u128,     // USDC row coll_usd_cf
    pub du: u128,     // USDC row debt_usd (+ debt of any unexpected market)
    pub pbal_u: u128, // USDC row pToken balance
}

/// `update_interest` on both receipts (portfolio reads the lagging balance view), then one
/// `portfolio(me)` call. `portfolio` lists only entered markets: a missing row counts as zero.
/// A row for any other market (should not exist; the margin account enters only these two)
/// adds its debt to the unstressed debt and its collateral is ignored (conservative).
pub(crate) fn book(env: &Env) -> Book {
    let ru = receipt(env, &usdc(env));
    let rx = receipt(env, &xlm(env));
    ReceiptClient::new(env, &ru).update_interest();
    ReceiptClient::new(env, &rx).update_interest();
    let (rows, _) = ctrl(env).portfolio(&me(env));
    let mut b = Book::default();
    for (m, pbal, _debt, coll, debt_usd) in rows.iter() {
        if m == ru {
            b.cu = b.cu.checked_add(coll).expect("overflow");
            b.du = b.du.checked_add(debt_usd).expect("overflow");
            b.pbal_u = b.pbal_u.checked_add(pbal).expect("overflow");
        } else if m == rx {
            b.cx = b.cx.checked_add(coll).expect("overflow");
            b.dx = b.dx.checked_add(debt_usd).expect("overflow");
        } else {
            b.du = b.du.checked_add(debt_usd).expect("overflow");
        }
    }
    b
}

/// C_u_eff = C_u * (pbal_u - pending_pt) / pbal_u (rounded down; 0 if pending >= pbal).
fn exclude_pending(b: &mut Book, pending_pt: u128) {
    if pending_pt == 0 || b.pbal_u == 0 {
        return;
    }
    if pending_pt >= b.pbal_u {
        b.cu = 0;
        return;
    }
    b.cu = b.cu.checked_mul(b.pbal_u - pending_pt).expect("overflow") / b.pbal_u;
}

/// Both sides of HF with the XLM price multiplied by `f` (1e6 = spot), without intermediate
/// rounding: num = C_x * f + C_u * 1e6, den = D_x * f + D_u * 1e6, HF = num * 1e6 / den.
fn hf_terms(b: &Book, f: u128) -> (u128, u128) {
    let num = b
        .cx
        .checked_mul(f)
        .and_then(|x| x.checked_add(b.cu.checked_mul(SCALE_1E6)?))
        .expect("overflow");
    let den = b
        .dx
        .checked_mul(f)
        .and_then(|x| x.checked_add(b.du.checked_mul(SCALE_1E6)?))
        .expect("overflow");
    (num, den)
}
/// num * 1e6 >= hf * den. Zero debt always passes.
fn hf_ok(b: &Book, f: u128, hf: u128) -> bool {
    let (num, den) = hf_terms(b, f);
    num.checked_mul(SCALE_1E6).expect("overflow") >= hf.checked_mul(den).expect("overflow")
}
/// HF value (1e6, rounded down); u128::MAX with no debt.
fn hf_value(b: &Book, f: u128) -> u128 {
    let (num, den) = hf_terms(b, f);
    if den == 0 {
        return u128::MAX;
    }
    num.checked_mul(SCALE_1E6).expect("overflow") / den
}

/// close_reserve_usd, or (when 0) the INTERFACE rule 5 derivation:
/// 1.01 * max(cf_xlm * maxdebt_long, cf_usdc * maxdebt_short), rounded up, with
/// maxdebt_side = Caps(side).per_position_equity_usd * (Risk(side).max_leverage - 1e6) / 1e6.
/// A side without Caps or Risk contributes 0 (WP2 cannot open on it without both).
pub(crate) fn close_reserve(env: &Env, h: &Headroom) -> u128 {
    if h.close_reserve_usd != 0 {
        return h.close_reserve_usd;
    }
    let side = |held: &Address| -> u128 {
        let caps: Option<Caps> = inst_get(env, &Key::Caps(held.clone()));
        let risk: Option<RiskParams> = inst_get(env, &Key::Risk(held.clone()));
        let (Some(c), Some(r)) = (caps, risk) else { return 0 };
        let maxdebt = mul_div_up(c.per_position_equity_usd, r.max_leverage.saturating_sub(SCALE_1E6), SCALE_1E6);
        let cf = ctrl(env).get_market_cf(&receipt(env, held));
        mul_div_up(maxdebt, cf, SCALE_1E6)
    };
    let worst = side(&xlm(env)).max(side(&usdc(env)));
    mul_div_up(worst, RESERVE_MARGIN, SCALE_1E6)
}

/// SPEC §4 checks 1-3 on the post-trade pooled state (both receipts' update_interest first,
/// then controller.portfolio(me)). Check::Open also requires backstop::value_usd >=
/// required_backstop_usd (else BackstopTooSmall) and excludes backstop::pending_pt from USDC
/// collateral. Panics PoolCheck (Open) / BackstopHeadroom (BackstopExit).
///
/// Fixed order: HF spot -> HF stress down and up -> free liquidity -> (Open) backstop size.
///
/// Two stress points cover the whole interval: with XLM price p, C_x and D_x are proportional
/// to p while C_u and D_u do not depend on it, so HF(p) = (a*p + b) / (c*p + d) with
/// a, b, c, d >= 0. dHF/dp has the constant sign of (a*d - b*c), so HF is monotonic on
/// [p(1-s), p(1+s)] and its minimum over the interval is at one of the two endpoints.
pub(crate) fn pool_check(env: &Env, check: Check) {
    let h: Headroom = need(env, &Key::Headroom);
    let err = match check {
        Check::Open => Error::PoolCheck,
        Check::BackstopExit => Error::BackstopHeadroom,
    };
    let mut b = book(env);
    if check == Check::Open {
        exclude_pending(&mut b, crate::backstop::pending_pt(env));
    }
    if !hf_ok(&b, SCALE_1E6, h.hf_target) {
        panic_with_error!(env, err);
    }
    let s = h.stress.min(SCALE_1E6);
    if !hf_ok(&b, SCALE_1E6 - s, h.hf_stress) || !hf_ok(&b, SCALE_1E6 + s, h.hf_stress) {
        panic_with_error!(env, err);
    }
    let c = b.cx.checked_add(b.cu).expect("overflow");
    let d = b.dx.checked_add(b.du).expect("overflow");
    let need_free = d.checked_add(close_reserve(env, &h)).expect("overflow");
    if c < need_free {
        panic_with_error!(env, err);
    }
    if check == Check::Open && crate::backstop::value_usd(env) < required_backstop_usd(env) {
        panic_with_error!(env, Error::BackstopTooSmall);
    }
}

/// B_req = coef_long*long equity + coef_short*short equity + coef_largest*max(per_position caps), usd7.
/// Long side = held XLM, short side = held USDC; equity is the current open book (Key::Side),
/// the largest position is the larger per_position_equity_usd cap (Key::Caps). Each term is
/// rounded up; missing keys count as 0.
pub(crate) fn required_backstop_usd(env: &Env) -> u128 {
    let h: Headroom = need(env, &Key::Headroom);
    let (x, u) = (xlm(env), usdc(env));
    let eq = |a: &Address| -> u128 {
        let s: Option<SideTotals> = inst_get(env, &Key::Side(a.clone()));
        s.map(|s| s.equity_usd).unwrap_or(0)
    };
    let cap = |a: &Address| -> u128 {
        let c: Option<Caps> = inst_get(env, &Key::Caps(a.clone()));
        c.map(|c| c.per_position_equity_usd).unwrap_or(0)
    };
    let largest = cap(&x).max(cap(&u));
    mul_div_up(h.coef_long, eq(&x), SCALE_1E6)
        .checked_add(mul_div_up(h.coef_short, eq(&u), SCALE_1E6))
        .and_then(|v| v.checked_add(mul_div_up(h.coef_largest, largest, SCALE_1E6)))
        .expect("overflow")
}

/// Panics Impaired if the Impaired flag is set.
pub(crate) fn require_not_impaired(env: &Env) {
    if is_impaired(env) {
        panic_with_error!(env, Error::Impaired);
    }
}

/// (Ledger(USDC), balance USDC pT, Ledger(XLM), balance XLM pT)
fn balances(env: &Env) -> (u128, u128, u128, u128) {
    let (u, x) = (usdc(env), xlm(env));
    let bal_u = ReceiptClient::new(env, &receipt(env, &u)).get_ptoken_balance(&me(env));
    let bal_x = ReceiptClient::new(env, &receipt(env, &x)).get_ptoken_balance(&me(env));
    (ledger(env, &u), bal_u, ledger(env, &x), bal_x)
}

#[contractimpl]
impl MarginController {
    /// Admin. Bounds: hf_stress >= 1e6, hf_target >= hf_stress, stress <= 500_000.
    pub fn set_headroom(env: Env, admin: Address, h: Headroom) {
        require_admin(&env, &admin);
        if h.hf_stress < SCALE_1E6 || h.hf_target < h.hf_stress || h.stress > 500_000 {
            panic_with_error!(&env, Error::BadParams);
        }
        env.storage().instance().set(&Key::Headroom, &h);
        bump(&env);
    }
    pub fn get_headroom(env: Env) -> Headroom {
        need(&env, &Key::Headroom)
    }
    /// (hf_spot, hf_down, hf_up, free_usd) 1e6 / usd7; u128::MAX HF when no debt.
    /// Same snapshot and formulas as pool_check, on the ACTUAL pooled account: queued
    /// backstop exits are NOT excluded (controller-side liquidation view for the keeper).
    /// free_usd = max(0, C - D), before the close reserve is subtracted.
    pub fn pooled_health(env: Env) -> (u128, u128, u128, u128) {
        let h: Headroom = need(&env, &Key::Headroom);
        let b = book(&env);
        let s = h.stress.min(SCALE_1E6);
        let c = b.cx.saturating_add(b.cu);
        let d = b.dx.saturating_add(b.du);
        (
            hf_value(&b, SCALE_1E6),
            hf_value(&b, SCALE_1E6 - s),
            hf_value(&b, SCALE_1E6 + s),
            c.saturating_sub(d),
        )
    }
    pub fn required_backstop(env: Env) -> u128 {
        required_backstop_usd(&env)
    }
    /// Permissionless: compare receipt.get_ptoken_balance(me) with Ledger(asset) for USDC and
    /// XLM; on any deficit set Impaired, pause opens, emit margin_impaired. Returns true if impaired.
    /// Oracle-free. Never clears Impaired (only admin_resolve_impairment does); the return value
    /// is the flag after the call, so it is also true if the contract was already impaired.
    /// Surpluses (donations) are ignored.
    pub fn reconcile(env: Env, caller: Address) -> bool {
        caller.require_auth();
        let (lu, bu, lx, bx) = balances(&env);
        if bu < lu || bx < lx {
            set_impaired(&env, true);
            env.storage().instance().set(&Key::OpensPaused, &true);
            MarginImpaired { caller, usdc_ledger: lu, usdc_balance: bu, xlm_ledger: lx, xlm_balance: bx }.publish(&env);
        }
        bump(&env);
        is_impaired(&env)
    }
    /// Book a charge (pTokens) against the backstop and Ledger; clears Impaired only if
    /// Ledger <= balance for both markets afterwards.
    ///
    /// USDC: the missing USDC pTokens are a backstop loss: Bs.ptokens and Ledger(USDC) both
    /// drop by `usdc_pt_charge` (BadAmount if either is smaller).
    /// XLM: the backstop holds no XLM pTokens, so `xlm_pt_charge` is a write-down of
    /// Ledger(XLM) only. The operator compensates in USDC through `usdc_pt_charge` (oracle
    /// value of the XLM loss, computed off-chain) and/or restores XLM pTokens to the contract.
    /// After an XLM write-down Ledger(XLM) is below the sum of held XLM pTokens + XLM claims, so
    /// the last XLM closers would hit the shortfall. Never unpauses opens (admin does that).
    pub fn admin_resolve_impairment(env: Env, admin: Address, usdc_pt_charge: u128, xlm_pt_charge: u128) {
        require_admin(&env, &admin);
        let (u, x) = (usdc(&env), xlm(&env));
        if usdc_pt_charge > 0 {
            let mut bs: Backstop = inst_get(&env, &Key::Bs).unwrap_or_else(|| panic_with_error!(&env, Error::BadAmount));
            bs.ptokens = bs
                .ptokens
                .checked_sub(usdc_pt_charge)
                .unwrap_or_else(|| panic_with_error!(&env, Error::BadAmount));
            env.storage().instance().set(&Key::Bs, &bs);
            ledger_sub(&env, &u, usdc_pt_charge);
        }
        if xlm_pt_charge > 0 {
            ledger_sub(&env, &x, xlm_pt_charge);
        }
        let (lu, bu, lx, bx) = balances(&env);
        let cleared = lu <= bu && lx <= bx;
        if cleared {
            set_impaired(&env, false);
        }
        MarginResolved { usdc_pt_charge, xlm_pt_charge, cleared }.publish(&env);
        bump(&env);
    }
    pub fn is_impaired(env: Env) -> bool {
        is_impaired(&env)
    }
}
