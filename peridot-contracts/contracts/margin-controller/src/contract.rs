//! Constructor, admin configuration and positions (WP2 owns the position flows).
// Events keep the v0.2 single-tuple `publish` form (keeper/indexer compatibility).
#![allow(deprecated)]
use crate::common::*;
use crate::types::*;
use soroban_sdk::{contractimpl, panic_with_error, token, Address, BytesN, Env, Symbol, Vec};

// ---------------------------------------------------------------- WP2 integration shims
// WP1 (backstop.rs) and WP3 (headroom.rs) are written in parallel; until they land, unit tests
// run against these test-only stand-ins. Outside `cfg(test)` every shim calls the real crate
// API. INTEGRATOR: delete the `#[cfg(test)]` variants (and inline the calls) once WP1/WP3 merge.
#[cfg(not(test))]
fn shim_require_not_impaired(env: &Env) {
    crate::headroom::require_not_impaired(env)
}
#[cfg(test)]
fn shim_require_not_impaired(env: &Env) {
    if is_impaired(env) {
        panic_with_error!(env, Error::Impaired);
    }
}
#[cfg(not(test))]
fn shim_pool_check_open(env: &Env) {
    crate::headroom::pool_check(env, Check::Open)
}
#[cfg(test)]
fn shim_pool_check_open(_env: &Env) {}
#[cfg(not(test))]
fn shim_credit_pt(env: &Env, usdc_pt: u128) {
    crate::backstop::credit_pt(env, usdc_pt)
}
#[cfg(test)]
fn shim_credit_pt(env: &Env, usdc_pt: u128) {
    let mut b: Backstop = inst_get(env, &Key::Bs).unwrap_or(Backstop {
        shares: 0,
        ptokens: 0,
        pending_shares: 0,
    });
    b.ptokens = b.ptokens.checked_add(usdc_pt).expect("overflow");
    env.storage().instance().set(&Key::Bs, &b);
}
#[cfg(not(test))]
fn shim_assign_bad_debt(env: &Env, debt_asset: &Address, shares: u128) {
    crate::backstop::assign_bad_debt(env, debt_asset, shares)
}
#[cfg(test)]
fn shim_assign_bad_debt(env: &Env, debt_asset: &Address, shares: u128) {
    let k = Key::BsDebtShares(debt_asset.clone());
    let v: u128 = inst_get(env, &k).unwrap_or(0);
    env.storage()
        .instance()
        .set(&k, &v.checked_add(shares).expect("overflow"));
}

// ---------------------------------------------------------------- WP2 helpers
fn slippage(env: &Env) -> u128 {
    inst_get(env, &Key::MaxSlippage).unwrap_or(10_000)
}
/// liq_ltv * (1e6 + bonuses) * (1e6 + slippage) < 1e18 (seizing at liquidation stays covered).
fn solvent(liq_ltv: u128, f: &Fees, slip: u128) -> bool {
    let b = SCALE_1E6 + f.bonus_liquidator + f.bonus_backstop;
    match liq_ltv
        .checked_mul(b)
        .and_then(|x| x.checked_mul(SCALE_1E6 + slip))
    {
        Some(x) => x < SCALE_1E6 * SCALE_1E6 * SCALE_1E6,
        None => false,
    }
}
/// Largest per-position liq LTV a side allows: 1 - 1/(2L) (rounded up = stricter check).
fn liq_ltv_max(max_leverage: u128) -> u128 {
    SCALE_1E6 - (SCALE_1E6 * SCALE_1E6) / (2 * max_leverage)
}
fn check_sides_solvent(env: &Env, f: &Fees, slip: u128) {
    for side in [usdc(env), xlm(env)] {
        if let Some(rp) = inst_get::<RiskParams>(env, &Key::Risk(side)) {
            if !solvent(liq_ltv_max(rp.max_leverage), f, slip) {
                panic_with_error!(env, Error::BadParams);
            }
        }
    }
}
fn liq_of(ltv_open: u128) -> u128 {
    (SCALE_1E6 + ltv_open) / 2
}
/// Open-time LTV guard: the liq LTV this open would store must satisfy the set_risk solvency
/// inequality at the current bonuses and slippage. (Fees and slippage push the actual LTV above
/// 1 - 1/L at L-times leverage, so 1 - 1/L itself cannot be the limit; MaxLeverage bounds L.)
fn check_open_ltv(env: &Env, ltv_open: u128, f: &Fees, slip: u128) {
    if ltv_open >= SCALE_1E6 || !solvent(liq_of(ltv_open), f, slip) {
        panic_with_error!(env, Error::LtvTooHigh);
    }
}
fn frac_up(x: u128, f: u128) -> u128 {
    mul_div_up(x, f, SCALE_1E6)
}
fn frac_down(x: u128, f: u128) -> u128 {
    x.checked_mul(f).expect("overflow") / SCALE_1E6
}
fn usd_to_amt_up(v: u128, p: (u128, u128)) -> u128 {
    mul_div_up(v, p.1, p.0)
}
fn usd_to_amt_down(v: u128, p: (u128, u128)) -> u128 {
    v.checked_mul(p.1).expect("overflow") / p.0
}
fn amt_to_pt_up(a: u128, rate: u128) -> u128 {
    mul_div_up(a, SCALE_1E6, rate)
}
fn amt_to_pt_down(a: u128, rate: u128) -> u128 {
    a.checked_mul(SCALE_1E6).expect("overflow") / rate
}
fn pt_to_amt(pt: u128, rate: u128) -> u128 {
    pt.checked_mul(rate).expect("overflow") / SCALE_1E6
}
fn rate_of(env: &Env, asset: &Address) -> u128 {
    ReceiptClient::new(env, &receipt(env, asset)).get_exchange_rate()
}
fn ltv_of(debt_usd: u128, held_usd: u128) -> u128 {
    if held_usd == 0 {
        return u128::MAX;
    }
    mul_div_up(debt_usd, SCALE_1E6, held_usd)
}
fn side_get(env: &Env, held: &Address) -> SideTotals {
    inst_get(env, &Key::Side(held.clone())).unwrap_or(SideTotals {
        equity_usd: 0,
        oi_usd: 0,
        open_positions: 0,
    })
}
fn set_shares(env: &Env, debt: &Address, v: u128) {
    env.storage()
        .instance()
        .set(&Key::DebtShares(debt.clone()), &v);
}
fn xfer(env: &Env, tok: &Address, from: &Address, to: &Address, amount: u128) {
    if amount > 0 {
        token::TokenClient::new(env, tok).transfer(from, to, &i(env, amount));
    }
}
/// Funding owed in raw USDC: funding_apr * debt_usd * elapsed / YEAR_SECS, rounded up, saturating.
/// USDC debt (long) is oracle-free; XLM debt (short) is valued at the oracle. CAP-88: a repeated
/// ledger timestamp gives elapsed 0 -> 0, and funding_ts never moves backwards.
fn funding_raw(env: &Env, p: &Position, debt: u128) -> u128 {
    let elapsed = env.ledger().timestamp().saturating_sub(p.funding_ts) as u128;
    let apr = fees(env).funding_apr;
    if elapsed == 0 || apr == 0 || debt == 0 {
        return 0;
    }
    let base = if p.debt_asset == usdc(env) {
        debt
    } else {
        usd_to_amt_up(usd(debt, price(env, &p.debt_asset)), price(env, &usdc(env)))
    };
    let n = apr.saturating_mul(base).saturating_mul(elapsed);
    let d = YEAR_SECS * SCALE_1E6;
    n / d + if n % d == 0 { 0 } else { 1 }
}
/// Deposit `amount` USDC (already held by this contract) for the backstop. Returns the USDC
/// that was NOT deposited because it would mint 0 pTokens (dust), for the caller to hand back,
/// so dust never blocks an exit or a liquidation.
fn deposit_to_backstop(env: &Env, amount: u128) -> u128 {
    if amount == 0 {
        return 0;
    }
    let u = usdc(env);
    if amt_to_pt_down(amount, rate_of(env, &u)) == 0 {
        return amount;
    }
    let pt = deposit_self(env, &u, amount);
    ledger_add(env, &u, pt);
    shim_credit_pt(env, pt);
    0
}
/// pTokens of `held` to sell for `amount` of `debt` (oracle price grossed up by the slippage bound).
fn sell_pt(
    amount: u128,
    pd: (u128, u128),
    ph: (u128, u128),
    slip: u128,
    rate_h: u128,
    cap: u128,
) -> u128 {
    let need_held = mul_div_up(
        mul_div_up(usd(amount, pd).saturating_add(1), ph.1, ph.0),
        SCALE_1E6,
        SCALE_1E6 - slip,
    );
    amt_to_pt_up(need_held, rate_h).saturating_add(1).min(cap)
}
/// (LTV incl. funding, debt, funding USDC). Shared by get_position_ltv, is_liquidatable, liquidate.
fn pos_ltv(env: &Env, p: &Position) -> (u128, u128, u128) {
    let d = position_debt(env, p);
    let f = funding_raw(env, p, d);
    let debt_usd = usd(d, price(env, &p.debt_asset))
        .checked_add(usd(f, price(env, &usdc(env))))
        .expect("overflow");
    let held_usd = usd(held_underlying(env, p), price(env, &p.held_asset));
    (ltv_of(debt_usd, held_usd), d, f)
}
fn add_claim(env: &Env, owner: &Address, asset: &Address, pt: u128) {
    if pt == 0 {
        return;
    }
    let k = Key::Claim(owner.clone(), asset.clone());
    let v: u128 = env.storage().persistent().get(&k).unwrap_or(0);
    env.storage()
        .persistent()
        .set(&k, &v.checked_add(pt).expect("overflow"));
    env.storage()
        .persistent()
        .extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND_TO);
    let tk = Key::ClaimsTotal(asset.clone());
    let t: u128 = inst_get(env, &tk).unwrap_or(0);
    env.storage()
        .instance()
        .set(&tk, &t.checked_add(pt).expect("overflow"));
}
fn owned(env: &Env, user: &Address, id: u64) -> Position {
    let p = load(env, id);
    if p.owner != *user {
        panic_with_error!(env, Error::NotOwner);
    }
    p
}

pub use crate::{MarginController, MarginControllerArgs, MarginControllerClient};

#[contractimpl]
impl MarginController {
    /// Runs atomically with creation (no front-runnable initialize). Opens start paused.
    pub fn __constructor(
        env: Env,
        admin: Address,
        peridottroller: Address,
        guardian: Address,
        usdc: Address,
        xlm: Address,
    ) {
        if env.storage().instance().has(&Key::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        let s = env.storage().instance();
        s.set(&Key::Admin, &admin);
        s.set(&Key::Guardian, &guardian);
        s.set(&Key::Ctrl, &peridottroller);
        s.set(&Key::Usdc, &usdc);
        s.set(&Key::Xlm, &xlm);
        s.set(&Key::OpensPaused, &true);
        s.set(&Key::MaxSlippage, &10_000u128); // 1 %
        s.set(
            &Key::Fees,
            &Fees {
                open_fee: 1_000,
                close_fee: 1_000,
                funding_apr: 20_000,
                bonus_liquidator: 30_000,
                bonus_backstop: 10_000,
            },
        );
        s.set(
            &Key::BsParams,
            &BackstopParams {
                cooldown_s: 259_200,
                window_s: 172_800,
                min_deposit_pt: 1_000_000,
            },
        );
        s.set(
            &Key::Headroom,
            &Headroom {
                hf_target: 1_250_000,
                hf_stress: 1_100_000,
                stress: 200_000,
                close_reserve_usd: 0,
                coef_long: 3_000_000,
                coef_short: 2_100_000,
                coef_largest: 4_600_000,
            },
        );
        s.set(&Key::Counter, &0u64);
        bump(&env);
    }

    // ---- admin configuration (immediate; affects only this contract)
    pub fn set_market(env: Env, admin: Address, asset: Address, receipt_addr: Address) {
        require_admin(&env, &admin);
        if ReceiptClient::new(&env, &receipt_addr).get_underlying_token() != asset {
            panic_with_error!(&env, Error::BadParams);
        }
        env.storage()
            .instance()
            .set(&Key::Market(asset), &receipt_addr);
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

    // ---- WP2: risk, caps, fees (signatures frozen)
    /// 1e6 < max_leverage <= MAX_LEVERAGE_CAP; solvency: liq_ltv_max * (1e6 + bonuses) * (1e6 + slippage) < 1e18,
    /// with liq_ltv_max = 1e6 - 1e12 / (2 * max_leverage).
    pub fn set_risk(env: Env, admin: Address, held_asset: Address, max_leverage: u128) {
        require_admin(&env, &admin);
        if held_asset != usdc(&env) && held_asset != xlm(&env) {
            panic_with_error!(&env, Error::BadParams);
        }
        if max_leverage <= SCALE_1E6 || max_leverage > MAX_LEVERAGE_CAP {
            panic_with_error!(&env, Error::BadParams);
        }
        if !solvent(liq_ltv_max(max_leverage), &fees(&env), slippage(&env)) {
            panic_with_error!(&env, Error::BadParams);
        }
        env.storage()
            .instance()
            .set(&Key::Risk(held_asset), &RiskParams { max_leverage });
        bump(&env);
    }
    pub fn set_caps(env: Env, admin: Address, held_asset: Address, caps: Caps) {
        require_admin(&env, &admin);
        if held_asset != usdc(&env) && held_asset != xlm(&env) {
            panic_with_error!(&env, Error::BadParams);
        }
        if caps.per_position_equity_usd == 0
            || caps.min_equity_usd > caps.per_position_equity_usd
            || caps.per_position_equity_usd > caps.side_equity_usd
            || caps.side_oi_usd == 0
        {
            panic_with_error!(&env, Error::BadParams);
        }
        env.storage().instance().set(&Key::Caps(held_asset), &caps);
        bump(&env);
    }
    /// Bounds: fees <= MAX_FEE, funding <= MAX_FUNDING_APR, bonuses sum <= MAX_TOTAL_BONUS; re-checks set_risk solvency for both sides.
    pub fn set_fees(env: Env, admin: Address, fees: Fees) {
        require_admin(&env, &admin);
        let bonus = fees.bonus_liquidator.checked_add(fees.bonus_backstop);
        if fees.open_fee > MAX_FEE
            || fees.close_fee > MAX_FEE
            || fees.funding_apr > MAX_FUNDING_APR
            || bonus.is_none_or(|b| b > MAX_TOTAL_BONUS)
        {
            panic_with_error!(&env, Error::BadParams);
        }
        check_sides_solvent(&env, &fees, slippage(&env));
        env.storage().instance().set(&Key::Fees, &fees);
        bump(&env);
    }
    /// (0, MAX_SLIPPAGE_CAP]; also re-checks the set_risk solvency bound for both sides.
    pub fn set_max_slippage(env: Env, admin: Address, max_slippage: u128) {
        require_admin(&env, &admin);
        if max_slippage == 0 || max_slippage > MAX_SLIPPAGE_CAP {
            panic_with_error!(&env, Error::BadParams);
        }
        check_sides_solvent(&env, &fees(&env), max_slippage);
        env.storage()
            .instance()
            .set(&Key::MaxSlippage, &max_slippage);
        bump(&env);
    }

    // ---- WP2: user flows (signatures frozen)
    /// Long XLM: held XLM, debt USDC. Short XLM: held USDC, debt XLM. `collateral` raw held units
    /// from the user's wallet; `max_fee` = most USDC (raw) the user accepts for the open fee.
    ///
    /// Leverage: `debt_usd <= equity_usd * (L - 1)` before the trade (gross notional
    /// `equity + debt <= L * equity`), and `notional_usd <= L * equity_usd` after it.
    /// Open fee = `open_fee * (equity_usd + debt_usd)` (the gross notional, known before the
    /// swap on both sides), converted to USDC at the oracle.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        env: Env,
        user: Address,
        held_asset: Address,
        debt_asset: Address,
        collateral: u128,
        borrow: u128,
        min_out: u128,
        max_fee: u128,
    ) -> u64 {
        user.require_auth();
        bump(&env);
        shim_require_not_impaired(&env);
        if inst_get::<bool>(&env, &Key::OpensPaused).unwrap_or(true) {
            panic_with_error!(&env, Error::OpensPaused);
        }
        let (u, x) = (usdc(&env), xlm(&env));
        let long = held_asset == x && debt_asset == u;
        let short = held_asset == u && debt_asset == x;
        if collateral == 0 || borrow == 0 || !(long || short) {
            panic_with_error!(&env, Error::BadAmount);
        }
        let rp: RiskParams = need(&env, &Key::Risk(held_asset.clone()));
        let caps: Caps = need(&env, &Key::Caps(held_asset.clone()));
        let fs = fees(&env);
        let slip = slippage(&env);
        let (ph, pd) = (price(&env, &held_asset), price(&env, &debt_asset));
        let pu = if long { pd } else { ph };

        // ---- caps on equity (per position, per side)
        let equity = usd(collateral, ph);
        if equity == 0 || equity < caps.min_equity_usd {
            panic_with_error!(&env, Error::BadAmount);
        }
        if equity > caps.per_position_equity_usd {
            panic_with_error!(&env, Error::PositionCapExceeded);
        }
        let mut side = side_get(&env, &held_asset);
        let side_equity = side.equity_usd.checked_add(equity).expect("overflow");
        if side_equity > caps.side_equity_usd {
            panic_with_error!(&env, Error::GlobalCapExceeded);
        }
        // ---- leverage (pre-trade, gross)
        let debt_usd = usd(borrow, pd);
        if debt_usd.checked_mul(SCALE_1E6).expect("overflow")
            > equity
                .checked_mul(rp.max_leverage - SCALE_1E6)
                .expect("overflow")
        {
            panic_with_error!(&env, Error::MaxLeverage);
        }
        let gross = equity.checked_add(debt_usd).expect("overflow");
        let mut fee = usd_to_amt_up(frac_up(gross, fs.open_fee), pu);
        if amt_to_pt_down(fee, rate_of(&env, &u)) == 0 {
            fee = 0; // would mint 0 pTokens: not chargeable
        }
        if fee > max_fee {
            panic_with_error!(&env, Error::MaxFee);
        }
        if long && fee >= borrow {
            panic_with_error!(&env, Error::BadAmount);
        }
        // ---- pre-trade LTV and OI at the oracle-minimum fill
        let (oracle_out, held_min) = if long {
            let o = oracle_min_out(&env, &u, &x, borrow - fee);
            (o, collateral.checked_add(o).expect("overflow"))
        } else {
            let o = oracle_min_out(&env, &x, &u, borrow);
            (
                o,
                collateral
                    .checked_add(o)
                    .expect("overflow")
                    .saturating_sub(fee),
            )
        };
        let held_min_usd = usd(held_min, ph);
        if side.oi_usd.checked_add(held_min_usd).expect("overflow") > caps.side_oi_usd {
            panic_with_error!(&env, Error::OiCapExceeded);
        }
        check_open_ltv(&env, ltv_of(debt_usd, held_min_usd), &fs, slip);

        // ---- collateral in
        xfer(&env, &held_asset, &user, &me(&env), collateral);
        let mut held_pt = deposit_self(&env, &held_asset, collateral);

        // ---- borrow; shares priced over ALL holders (positions + backstop)
        let rd = ReceiptClient::new(&env, &receipt(&env, &debt_asset));
        let pooled_before = pooled_debt(&env, &debt_asset);
        let ts_all = all_shares(&env, &debt_asset);
        let bal_before = tok_balance(&env, &debt_asset);
        rd.borrow(&me(&env), &borrow); // health-checked by the live controller
        let borrowed = tok_balance(&env, &debt_asset).saturating_sub(bal_before);
        let added = rd
            .get_user_borrow_balance(&me(&env))
            .saturating_sub(pooled_before);
        if borrowed < borrow || added == 0 {
            panic_with_error!(&env, Error::BadAmount);
        }
        let new_shares = if ts_all == 0 || pooled_before == 0 {
            added
        } else {
            mul_div_up(added, ts_all, pooled_before)
        };
        set_shares(
            &env,
            &debt_asset,
            total_shares(&env, &debt_asset)
                .checked_add(new_shares)
                .expect("overflow"),
        );

        // ---- fee + trade
        let fee_pt = if long {
            let fee_pt = if fee > 0 {
                deposit_self(&env, &u, fee)
            } else {
                0
            };
            let bought = swap(&env, &u, &x, borrow - fee, oracle_out.max(min_out));
            held_pt = held_pt
                .checked_add(deposit_self(&env, &x, bought))
                .expect("overflow");
            fee_pt
        } else {
            let proceeds = swap(&env, &x, &u, borrow, oracle_out.max(min_out));
            if proceeds <= fee {
                panic_with_error!(&env, Error::BadAmount);
            }
            let minted = deposit_self(&env, &u, proceeds);
            let fee_pt = if fee > 0 {
                mul_div_up(minted, fee, proceeds).min(minted)
            } else {
                0
            };
            held_pt = held_pt.checked_add(minted - fee_pt).expect("overflow");
            fee_pt
        };
        if fee_pt > 0 {
            ledger_add(&env, &u, fee_pt);
            shim_credit_pt(&env, fee_pt);
        }

        // ---- post-trade checks on actual values
        let now = env.ledger().timestamp();
        let id: u64 = need::<u64>(&env, &Key::Counter)
            .checked_add(1)
            .expect("overflow");
        let mut p = Position {
            owner: user.clone(),
            held_asset: held_asset.clone(),
            debt_asset,
            held_ptokens: held_pt,
            debt_shares: new_shares,
            collateral,
            equity_usd: equity,
            notional_usd: 0,
            liq_ltv: 0,
            opened_at: now,
            funding_ts: now,
            open: true,
        };
        let notional = usd(held_underlying(&env, &p), ph);
        let ltv_open = ltv_of(usd(position_debt(&env, &p), pd), notional);
        check_open_ltv(&env, ltv_open, &fs, slip);
        if notional.checked_mul(SCALE_1E6).expect("overflow")
            > equity.checked_mul(rp.max_leverage).expect("overflow")
        {
            panic_with_error!(&env, Error::MaxLeverage);
        }
        let side_oi = side.oi_usd.checked_add(notional).expect("overflow");
        if side_oi > caps.side_oi_usd {
            panic_with_error!(&env, Error::OiCapExceeded);
        }
        p.notional_usd = notional;
        p.liq_ltv = liq_of(ltv_open);

        side.equity_usd = side_equity;
        side.oi_usd = side_oi;
        side.open_positions = side.open_positions.checked_add(1).expect("overflow");
        env.storage()
            .instance()
            .set(&Key::Side(held_asset.clone()), &side);
        env.storage().instance().set(&Key::Counter, &id);
        ledger_add(&env, &held_asset, held_pt);
        store(&env, id, &p);
        env.events().publish(
            (Symbol::new(&env, "margin_open"),),
            (id, user, held_asset, borrow, held_pt, fee, p.liq_ltv),
        );
        shim_pool_check_open(&env); // last: sees the final pooled state
        id
    }

    /// Close by selling enough held to repay the debt (long: + close fee + funding, from the USDC
    /// proceeds; short: close fee + funding are moved internally from held USDC pTokens).
    /// Debt has priority; fees/funding take what is left; the surplus goes to the owner.
    pub fn close(env: Env, user: Address, id: u64, min_out: u128) {
        user.require_auth();
        bump(&env);
        let p = owned(&env, &user, id);
        let long = p.held_asset == xlm(&env);
        let fs = fees(&env);
        let slip = slippage(&env);
        let (ph, pd) = (price(&env, &p.held_asset), price(&env, &p.debt_asset));
        let pu = if long { pd } else { ph };
        let d = position_debt(&env, &p);
        let f = funding_raw(&env, &p, d);
        let fee = usd_to_amt_up(frac_up(p.notional_usd, fs.close_fee), pu);
        let fee_f = fee.checked_add(f).expect("overflow");
        let rh = ReceiptClient::new(&env, &receipt(&env, &p.held_asset));
        let rate_h = rh.get_exchange_rate();
        let held = p.held_ptokens;

        // pTokens moved internally to the backstop (short only)
        let (sell, bs_pt) = if long {
            (
                sell_pt(
                    d.checked_add(fee_f).expect("overflow"),
                    pd,
                    ph,
                    slip,
                    rate_h,
                    held,
                ),
                0,
            )
        } else {
            let sell = sell_pt(d, pd, ph, slip, rate_h, held);
            (sell, amt_to_pt_up(fee_f, rate_h).min(held - sell))
        };
        let before = tok_balance(&env, &p.held_asset);
        rh.withdraw(&me(&env), &sell);
        let got = tok_balance(&env, &p.held_asset).saturating_sub(before);
        let floor = oracle_min_out(&env, &p.held_asset, &p.debt_asset, got).max(min_out);
        let proceeds = swap(&env, &p.held_asset, &p.debt_asset, got, floor);
        if proceeds < d {
            panic_with_error!(&env, Error::InsufficientSwapOutput); // use close_with_repay
        }
        let repaid = repay_self(&env, &p.debt_asset, d);
        let mut surplus = proceeds - repaid;
        let to_bs = if long {
            let to_bs = fee_f.min(surplus);
            let dust = deposit_to_backstop(&env, to_bs);
            surplus = surplus - to_bs + dust;
            to_bs - dust
        } else {
            if bs_pt > 0 {
                shim_credit_pt(&env, bs_pt);
            }
            pt_to_amt(bs_pt, rate_h)
        };
        xfer(&env, &p.debt_asset, &me(&env), &user, surplus);
        let rest = held - sell - bs_pt;
        send_ptokens(&env, &p.held_asset, &user, rest);
        ledger_sub(&env, &p.held_asset, sell + rest);
        finish(&env, id, p);
        env.events().publish(
            (Symbol::new(&env, "margin_close"),),
            (id, repaid, surplus, to_bs),
        );
    }

    /// Oracle-free for USDC debt (long); short needs the price only for fees.
    /// Long: the owner pays debt + close fee + funding in USDC (the close fee is converted from
    /// usd7 at par, 1 usd7 = 1 raw USDC, so no oracle call is made). Short: the owner pays the
    /// XLM debt; close fee + funding come from held USDC pTokens.
    pub fn close_with_repay(env: Env, user: Address, id: u64) {
        user.require_auth();
        bump(&env);
        let p = owned(&env, &user, id);
        let u = usdc(&env);
        let long = p.held_asset != u;
        let fs = fees(&env);
        let d = position_debt(&env, &p);
        let f = funding_raw(&env, &p, d);
        let held = p.held_ptokens;
        let (repaid, to_bs, sent) = if long {
            let fee_f = frac_up(p.notional_usd, fs.close_fee)
                .checked_add(f)
                .expect("overflow");
            xfer(
                &env,
                &u,
                &user,
                &me(&env),
                d.checked_add(fee_f).expect("overflow"),
            );
            let repaid = repay_self(&env, &u, d);
            let dust = deposit_to_backstop(&env, fee_f);
            xfer(&env, &u, &me(&env), &user, d - repaid + dust);
            send_ptokens(&env, &p.held_asset, &user, held);
            ledger_sub(&env, &p.held_asset, held);
            (repaid, fee_f - dust, held)
        } else {
            xfer(&env, &p.debt_asset, &user, &me(&env), d);
            let repaid = repay_self(&env, &p.debt_asset, d);
            xfer(&env, &p.debt_asset, &me(&env), &user, d - repaid);
            let fee = usd_to_amt_up(frac_up(p.notional_usd, fs.close_fee), price(&env, &u));
            let rate_u = rate_of(&env, &u);
            let bs_pt = amt_to_pt_up(fee.checked_add(f).expect("overflow"), rate_u).min(held);
            if bs_pt > 0 {
                shim_credit_pt(&env, bs_pt);
            }
            send_ptokens(&env, &u, &user, held - bs_pt);
            ledger_sub(&env, &u, held - bs_pt);
            (repaid, pt_to_amt(bs_pt, rate_u), held - bs_pt)
        };
        finish(&env, id, p);
        env.events().publish(
            (Symbol::new(&env, "margin_close"),),
            (id, repaid, sent, to_bs),
        );
    }

    /// At LTV >= position.liq_ltv. Liquidator pays at most `max_pay` (raw debt units) in the debt asset.
    ///
    /// Solvent (held >= D * (1 + bonus_liq)): the liquidator repays D (long: + D*bonus_backstop
    /// + funding, in USDC) and receives held pTokens worth D * (1 + bonus_liq) (long: + funding).
    /// Short: backstop bonus and funding come from held USDC pTokens (capped by what is left).
    /// The owner remainder becomes a Claim. Underwater: the liquidator pays R = H/(1 + bonus_liq),
    /// receives all held; the unpaid shares move to the backstop's bad-debt bucket.
    pub fn liquidate(env: Env, liquidator: Address, id: u64, max_pay: u128) {
        liquidator.require_auth();
        bump(&env);
        let mut p = load(&env, id);
        let x = xlm(&env);
        let long = p.held_asset == x;
        let fs = fees(&env);
        let (ph, pd) = (price(&env, &p.held_asset), price(&env, &p.debt_asset));
        let pu = if long { pd } else { ph };
        let (ltv, d, f) = pos_ltv(&env, &p);
        if d == 0 || ltv < p.liq_ltv {
            panic_with_error!(&env, Error::NotLiquidatable);
        }
        let rate_h = rate_of(&env, &p.held_asset);
        let held = p.held_ptokens;
        let h_usd = usd(pt_to_amt(held, rate_h), ph);
        let d_usd = usd(d, pd);
        let seize_req_usd = frac_down(d_usd, SCALE_1E6 + fs.bonus_liquidator);
        let me_ = me(&env);
        let owner = p.owner.clone();

        if h_usd < seize_req_usd {
            // ---- underwater (CODE-PLAN §3.1)
            let r = usd_to_amt_down(
                h_usd.checked_mul(SCALE_1E6).expect("overflow") / (SCALE_1E6 + fs.bonus_liquidator),
                pd,
            )
            .min(d);
            if r > max_pay {
                panic_with_error!(&env, Error::MaxPay);
            }
            xfer(&env, &p.debt_asset, &liquidator, &me_, r);
            let repaid = repay_self(&env, &p.debt_asset, r);
            xfer(&env, &p.debt_asset, &me_, &liquidator, r - repaid);
            // burned rounded down -> residual rounded up: the position side pays at least its share
            let burned = p.debt_shares.checked_mul(repaid).expect("overflow") / d;
            let s_res = p.debt_shares - burned;
            set_shares(
                &env,
                &p.debt_asset,
                total_shares(&env, &p.debt_asset).saturating_sub(s_res),
            );
            shim_assign_bad_debt(&env, &p.debt_asset, s_res);
            p.debt_shares = burned;
            send_ptokens(&env, &p.held_asset, &liquidator, held);
            ledger_sub(&env, &p.held_asset, held);
            env.events().publish(
                (Symbol::new(&env, "margin_bad_debt"),),
                (id, p.debt_asset.clone(), s_res, d - repaid),
            );
            env.events().publish(
                (Symbol::new(&env, "margin_liquidate"),),
                (id, owner, liquidator, repaid, held, 0u128),
            );
            finish(&env, id, p);
            return;
        }

        let (repaid, seize, bs_pt) = if long {
            let bb = frac_up(d, fs.bonus_backstop);
            let f_eff = f.min(usd_to_amt_down(h_usd - seize_req_usd, pu));
            let pay = d
                .checked_add(bb)
                .and_then(|v| v.checked_add(f_eff))
                .expect("overflow");
            if pay > max_pay {
                panic_with_error!(&env, Error::MaxPay);
            }
            xfer(&env, &p.debt_asset, &liquidator, &me_, pay);
            let repaid = repay_self(&env, &p.debt_asset, d);
            let dust = deposit_to_backstop(&env, bb + f_eff);
            xfer(&env, &p.debt_asset, &me_, &liquidator, d - repaid + dust);
            let seize_usd = frac_down(usd(repaid, pd), SCALE_1E6 + fs.bonus_liquidator)
                .checked_add(usd(f_eff, pu))
                .expect("overflow");
            (
                repaid,
                amt_to_pt_down(usd_to_amt_down(seize_usd, ph), rate_h).min(held),
                0,
            )
        } else {
            if d > max_pay {
                panic_with_error!(&env, Error::MaxPay);
            }
            xfer(&env, &p.debt_asset, &liquidator, &me_, d);
            let repaid = repay_self(&env, &p.debt_asset, d);
            xfer(&env, &p.debt_asset, &me_, &liquidator, d - repaid);
            let seize_usd = frac_down(usd(repaid, pd), SCALE_1E6 + fs.bonus_liquidator);
            let seize = amt_to_pt_down(usd_to_amt_down(seize_usd, ph), rate_h).min(held);
            let bs_usd = frac_up(d_usd, fs.bonus_backstop)
                .checked_add(usd(f, pu))
                .expect("overflow");
            let bs_pt = amt_to_pt_up(usd_to_amt_up(bs_usd, pu), rate_h).min(held - seize);
            if bs_pt > 0 {
                shim_credit_pt(&env, bs_pt);
            }
            (repaid, seize, bs_pt)
        };
        send_ptokens(&env, &p.held_asset, &liquidator, seize);
        ledger_sub(&env, &p.held_asset, seize);
        let remainder = held - seize - bs_pt;
        add_claim(&env, &owner, &p.held_asset, remainder);
        env.events().publish(
            (Symbol::new(&env, "margin_liquidate"),),
            (id, owner, liquidator, repaid, seize, remainder),
        );
        finish(&env, id, p);
    }

    /// Owner remainder after liquidation, kept internally until claimed. Returns pTokens sent.
    pub fn claim(env: Env, owner: Address, asset: Address) -> u128 {
        owner.require_auth();
        bump(&env);
        let k = Key::Claim(owner.clone(), asset.clone());
        let amt: u128 = env.storage().persistent().get(&k).unwrap_or(0);
        if amt == 0 {
            panic_with_error!(&env, Error::NoClaim);
        }
        env.storage().persistent().remove(&k);
        let tk = Key::ClaimsTotal(asset.clone());
        let t: u128 = inst_get(&env, &tk).unwrap_or(0);
        env.storage().instance().set(&tk, &t.saturating_sub(amt));
        ledger_sub(&env, &asset, amt);
        send_ptokens(&env, &asset, &owner, amt);
        env.events()
            .publish((Symbol::new(&env, "margin_claim"),), (owner, asset, amt));
        amt
    }

    // ---- WP2 views (signatures frozen)
    pub fn get_position(env: Env, id: u64) -> Option<Position> {
        env.storage().persistent().get(&Key::Position(id))
    }
    pub fn get_position_counter(env: Env) -> u64 {
        inst_get(&env, &Key::Counter).unwrap_or(0)
    }
    /// Debt incl. interest, raw debt units (rounded up).
    pub fn get_position_debt(env: Env, id: u64) -> u128 {
        position_debt(&env, &load(&env, id))
    }
    /// Funding owed so far, raw USDC.
    pub fn get_position_funding(env: Env, id: u64) -> u128 {
        let p = load(&env, id);
        let d = position_debt(&env, &p);
        funding_raw(&env, &p, d)
    }
    /// (debt_usd + funding_usd) / held_usd, 1e6, rounded up.
    pub fn get_position_ltv(env: Env, id: u64) -> u128 {
        pos_ltv(&env, &load(&env, id)).0
    }
    pub fn is_liquidatable(env: Env, id: u64) -> bool {
        let p = load(&env, id);
        pos_ltv(&env, &p).0 >= p.liq_ltv
    }
    pub fn get_claim(env: Env, owner: Address, asset: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&Key::Claim(owner, asset))
            .unwrap_or(0)
    }
    pub fn get_side(env: Env, held_asset: Address) -> SideTotals {
        side_get(&env, &held_asset)
    }
    pub fn opens_paused(env: Env) -> bool {
        inst_get(&env, &Key::OpensPaused).unwrap_or(true)
    }
    pub fn bump_ttl(env: Env, ids: Vec<u64>) {
        bump(&env);
        for id in ids.iter() {
            let k = Key::Position(id);
            if env.storage().persistent().has(&k) {
                env.storage()
                    .persistent()
                    .extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND_TO);
            }
        }
    }
}
