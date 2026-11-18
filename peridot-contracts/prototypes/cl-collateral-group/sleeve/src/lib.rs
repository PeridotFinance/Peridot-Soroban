//! CL collateral GROUP measurement double (18 Nov 2026). NOT production code.
//! Extends the item-S spike sleeve (prototypes/cl-collateral-spike/sleeve) with the design in
//! research/cl-collateral-group-20261118/DESIGN.md:
//!   * valuation numeraire = USDC (the only debt asset): the controller prices one token only;
//!   * vault-wide NAV checkpoint (`checkpoint_nav`, permissionless, <=60 s) with depeg and
//!     pool/oracle band guards; the PYUSD leg is priced by Reflector CALI (PYUSD/USDC);
//!   * liquidation-only FALLBACK checkpoint (`checkpoint_fallback`, permissionless, <=60 s):
//!     the position valued as if entirely the cheaper asset at the band edge, no pool quote;
//!     allowed only when a named cause makes the NAV checkpoint impossible;
//!   * borrow/redeem read only the NAV checkpoint; liquidation reads NAV, else fallback;
//!   * seizure moves the receipt-side unit mirror atomically with the repay (stage L3) and
//!     queues the vault-side settle-only reassign (`sync_seized`, stage L5).
//! The vault-side reward/custody keys of the real vault (Beneficial, Holder, weight history,
//! Locked, Boundary) are EMULATED in this contract's storage with the same per-key layout as
//! contracts/cl-share-vault/src/{custody,rewards,reward_history}.rs; the real vault code and
//! instance are loaded through a real view so their footprint is counted.
#![no_std]
#![allow(deprecated)]
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address, Env, IntoVal, Symbol, Val, Vec, U256};

const RATE: u128 = 1_000_000; // controller exchange-rate scale
const P_SCALE: u128 = 100_000_000_000_000; // Reflector 14 decimals
const Q: u128 = 1_000_000_000_000_000_000; // 1e18
const SCALE: u128 = 1_000_000_000_000_000_000; // reward index scale (rewards.rs SCALE)
// sqrt(1.0001^-20), sqrt(1.0001^20) at 1e18, rounded inward: the vault's fixed parity band
// (prototypes/cl-pyusd-vault-b/vault.rs stage_fair_lp, ticks [-20,20]).
const S_LO: u128 = 999_000_549_780_071_480;
const S_HI: u128 = 1_001_000_450_120_021_002;

#[contracttype]
#[derive(Clone)]
pub enum Asset { Stellar(Address), Other(Symbol) }
#[contracttype]
#[derive(Clone)]
pub struct PriceData { pub price: i128, pub timestamp: u64 }
#[contracttype]
#[derive(Clone)]
pub struct Slot0 { pub sqrt_price_x96: U256, pub tick: i32 }

#[contracttype]
#[derive(Clone)]
pub struct Cfg { pub ctrl: Address, pub vault: Address, pub pool: Address, pub oracle: Address, pub pyusd: Address, pub usdc: Address }
#[contracttype]
#[derive(Clone)]
pub struct Prm { pub max_age: u64, pub oracle_max_age: u64, pub depeg_bps: u32, pub band_bps: u32, pub fb_haircut_bps: u32, pub claim_fresh: u64 }
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Ck { pub rate: u128, pub ts: u64, pub ledger: u32 }
#[contracttype]
#[derive(Clone)]
pub struct Pending { pub from: Address, pub to: Address, pub n: u128 }
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Holder { pub debt: u128, pub pending: u128, pub fraction: u128 }
#[contracttype]
#[derive(Clone)]
pub struct Bnd { pub holder: Address, pub shares: u128, pub index: u128, pub ts: u64 }
#[contracttype]
#[derive(Clone, Debug)]
pub struct SeizeContext {
    pub liquidity: u128, pub shortfall: u128, pub max_redeem_ptokens: u128, pub seize_ptokens: u128,
    pub fee_recipient: Option<Address>, pub fee_ptokens: u128, pub expires_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum K {
    Cfg, Prm, Nav, Fb, Total, PCount, Inline,
    // emulated vault instance fields
    RIdx, RRev, LastClaim, BCount, Open, Quar,
    // persistent
    Units(Address), Pend(u64),
    VHolder(Address), VBen(Address), VLocked(Address), VCount(Address), VHw(Address, u32), VBnd(u64),
}

/// Distinct codes so harness rows show WHY a call reverted (Error(Contract, #n)).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum E { NavStale = 1, DepegGuard = 2, PoolOutOfBand = 3, NavRefreshable = 4, OracleStale = 5, InsufficientCollateral = 6, UnitsLocked = 7, NavInvalid = 8, NoPending = 9, FallbackZero = 10 }

#[contract]
pub struct Sleeve;

fn inst<T: soroban_sdk::TryFromVal<Env, Val>>(e: &Env, k: &K) -> Option<T> { e.storage().instance().get(k) }
fn cfg(e: &Env) -> Cfg { inst(e, &K::Cfg).unwrap() }
fn prm(e: &Env) -> Prm { inst(e, &K::Prm).unwrap() }
fn pget(e: &Env, k: &K) -> u128 { e.storage().persistent().get(k).unwrap_or(0) }
fn pset(e: &Env, k: &K, v: u128) { e.storage().persistent().set(k, &v) }
fn units(e: &Env, a: &Address) -> u128 { pget(e, &K::Units(a.clone())) }
fn call<T: soroban_sdk::TryFromVal<Env, Val>>(e: &Env, c: &Address, f: &str, args: Vec<Val>) -> T { e.invoke_contract(c, &Symbol::new(e, f), args) }
fn now(e: &Env) -> u64 { e.ledger().timestamp() }
fn u(e: &Env, n: u128) -> U256 { U256::from_u128(e, n) }

impl Sleeve {
    /// Reflector CALI lastprice(Stellar(PYUSD)) in USDC, 14 decimals; None when missing/stale/future.
    fn pyusd_price(e: &Env) -> Option<u128> {
        let c = cfg(e);
        let pd: Option<PriceData> = call(e, &c.oracle, "lastprice", (Asset::Stellar(c.pyusd.clone()),).into_val(e));
        let pd = pd?;
        let t = now(e);
        if pd.price <= 0 || pd.timestamp > t || t - pd.timestamp > prm(e).oracle_max_age { return None; }
        Some(pd.price as u128)
    }
    fn depeg_ok(p: u128, bps: u32) -> bool { p.abs_diff(P_SCALE).checked_mul(10_000).unwrap() <= (bps as u128).checked_mul(P_SCALE).unwrap() }
    /// Pool spot (USDC per PYUSD, 1e18) vs oracle within band, and the pool tick inside the position.
    fn band_ok(e: &Env, p: u128) -> bool {
        let s0: Slot0 = call(e, &cfg(e).pool, "get_slot0", Vec::new(e));
        if s0.tick < -20 || s0.tick >= 20 { return false; }
        let s = s0.sqrt_price_x96.mul(&u(e, Q)).div(&u(e, 1u128 << 96));
        let spot = s.mul(&s).div(&u(e, Q)).to_u128().unwrap();
        let p18 = p.checked_mul(10_000).unwrap();
        spot.abs_diff(p18).checked_mul(10_000).unwrap() <= (prm(e).band_bps as u128).checked_mul(p18).unwrap()
    }
    fn fresh(e: &Env, k: &K) -> Option<u128> {
        let ck: Option<Ck> = inst(e, k);
        match ck { Some(c) if now(e).saturating_sub(c.ts) <= prm(e).max_age => Some(c.rate), _ => None }
    }
    /// Borrow/redeem rate: NAV checkpoint only. 0 = invalid (controller then reverts the borrow).
    fn nav_rate(e: &Env) -> u128 { Self::fresh(e, &K::Nav).unwrap_or(0) }
    /// Liquidation rate: NAV checkpoint, else the liquidation-only fallback checkpoint, else 0.
    fn liq_rate(e: &Env) -> u128 { Self::fresh(e, &K::Nav).or_else(|| Self::fresh(e, &K::Fb)).unwrap_or(0) }
    fn supply(e: &Env) -> u128 { let s: i128 = call(e, &cfg(e).vault, "total_supply", Vec::new(e)); assert!(s > 0, "no supply"); s as u128 }

    // ---- emulated vault-side settle-only reassign (custody.rs/rewards.rs/reward_history.rs shapes) ----
    fn holder(e: &Env, a: &Address) -> Holder { e.storage().persistent().get(&K::VHolder(a.clone())).unwrap_or(Holder { debt: 0, pending: 0, fraction: 0 }) }
    fn idx(e: &Env) -> u128 { inst(e, &K::RIdx).unwrap_or(0) }
    fn settle(e: &Env, a: &Address) {
        let mut h = Self::holder(e, a);
        let earned = pget(e, &K::VBen(a.clone())).checked_mul(Self::idx(e)).unwrap();
        let scaled = earned.checked_sub(h.debt).unwrap().checked_add(h.fraction).unwrap();
        h.pending = h.pending.checked_add(scaled / SCALE).unwrap();
        h.fraction = scaled % SCALE;
        h.debt = earned;
        e.storage().persistent().set(&K::VHolder(a.clone()), &h);
    }
    fn reset_debt(e: &Env, a: &Address) {
        let mut h = Self::holder(e, a);
        h.debt = pget(e, &K::VBen(a.clone())).checked_mul(Self::idx(e)).unwrap();
        e.storage().persistent().set(&K::VHolder(a.clone()), &h);
    }
    fn record_weight(e: &Env, a: &Address) {
        let n: u32 = e.storage().persistent().get(&K::VCount(a.clone())).unwrap_or(0);
        let w = pget(e, &K::VBen(a.clone()));
        let old = if n == 0 { 0 } else { pget(e, &K::VHw(a.clone(), n - 1)) };
        if (n > 0 && old == w) || (n == 0 && w == 0) { return; }
        let rev: u64 = inst(e, &K::RRev).unwrap_or(0);
        e.storage().instance().set(&K::RRev, &(rev + 1));
        pset(e, &K::VHw(a.clone(), n), w);
        e.storage().persistent().set(&K::VCount(a.clone()), &(n + 1));
    }
    fn do_sync(e: &Env, from: &Address, to: &Address, n: u128) {
        // real vault instance + code enter the footprint (production: this runs inside the vault)
        let _: Address = call(e, &cfg(e).vault, "stage_receipt", Vec::new(e));
        let ben = pget(e, &K::VBen(from.clone()));
        if ben.checked_sub(pget(e, &K::VLocked(from.clone()))).unwrap() < n { panic_with_error!(e, E::UnitsLocked); }
        // Unindexed venue accrual since the last claim: open a boundary (quarantine to the borrower
        // on the next claim) unless the claim stage ran within `claim_fresh` seconds.
        let last: u64 = inst(e, &K::LastClaim).unwrap_or(0);
        if now(e).saturating_sub(last) > prm(e).claim_fresh {
            let id: u64 = inst(e, &K::BCount).unwrap_or(0);
            e.storage().persistent().set(&K::VBnd(id), &Bnd { holder: from.clone(), shares: ben, index: Self::idx(e), ts: now(e) });
            e.storage().instance().set(&K::BCount, &(id + 1));
            e.storage().instance().set(&K::Open, &true);
        }
        Self::settle(e, from);
        Self::settle(e, to);
        pset(e, &K::VBen(from.clone()), ben - n);
        pset(e, &K::VBen(to.clone()), pget(e, &K::VBen(to.clone())).checked_add(n).unwrap());
        Self::reset_debt(e, from);
        Self::reset_debt(e, to);
        Self::record_weight(e, from);
        Self::record_weight(e, to);
    }
}

#[contractimpl]
impl Sleeve {
    pub fn init(e: Env, c: Cfg, p: Prm) {
        assert!(!e.storage().instance().has(&K::Cfg));
        e.storage().instance().set(&K::Cfg, &c);
        e.storage().instance().set(&K::Prm, &p);
        e.storage().instance().set(&K::Total, &0u128);
    }
    // ---- measurement-only setters (no auth) ----
    pub fn spike_set_params(e: Env, p: Prm) { e.storage().instance().set(&K::Prm, &p); }
    pub fn spike_set_inline(e: Env, on: bool) { e.storage().instance().set(&K::Inline, &on); }
    pub fn spike_set_units(e: Env, owner: Address, n: u128) {
        let old = units(&e, &owner); let t: u128 = inst(&e, &K::Total).unwrap();
        pset(&e, &K::Units(owner.clone()), n);
        pset(&e, &K::VBen(owner.clone()), n);
        Self::record_weight(&e, &owner);
        e.storage().instance().set(&K::Total, &(t - old + n));
    }
    /// Emulated venue claim (rewards.rs checkpoint): the ONLY place the index advances.
    pub fn emu_claim(e: Env, delta: u128) {
        let total: u128 = inst(&e, &K::Total).unwrap();
        if inst::<bool>(&e, &K::Open).unwrap_or(false) || total == 0 {
            let q: u128 = inst(&e, &K::Quar).unwrap_or(0);
            e.storage().instance().set(&K::Quar, &(q + delta));
            e.storage().instance().set(&K::Open, &false);
        } else {
            e.storage().instance().set(&K::RIdx, &(Self::idx(&e) + delta * SCALE / total));
        }
        e.storage().instance().set(&K::LastClaim, &now(&e));
    }
    pub fn emu_pending(e: Env, a: Address) -> u128 {
        let h = Self::holder(&e, &a);
        let earned = pget(&e, &K::VBen(a.clone())) * Self::idx(&e);
        h.pending + (earned - h.debt + h.fraction) / SCALE
    }
    pub fn emu_quarantine(e: Env) -> u128 { inst(&e, &K::Quar).unwrap_or(0) }
    pub fn emu_beneficial(e: Env, a: Address) -> u128 { pget(&e, &K::VBen(a)) }

    /// Stage L1a. Permissionless NAV checkpoint over the WHOLE vault (no caller-chosen owner).
    pub fn checkpoint_nav(e: Env) -> u128 {
        let c = cfg(&e); let pr = prm(&e);
        let supply = Self::supply(&e);
        let out: Vec<i128> = call(&e, &c.vault, "get_asset_amounts_per_shares", (supply as i128,).into_val(&e));
        let nav = out.get(0).unwrap();
        if nav <= 0 { panic_with_error!(&e, E::NavInvalid); }
        let p = Self::pyusd_price(&e).unwrap_or_else(|| panic_with_error!(&e, E::OracleStale));
        if !Self::depeg_ok(p, pr.depeg_bps) { panic_with_error!(&e, E::DepegGuard); }
        if !Self::band_ok(&e, p) { panic_with_error!(&e, E::PoolOutOfBand); }
        let value = (nav as u128).checked_mul(p).unwrap() / P_SCALE; // USDC units
        let rate = value.checked_mul(RATE).unwrap() / supply;
        assert!(rate > 0);
        e.storage().instance().set(&K::Nav, &Ck { rate, ts: now(&e), ledger: e.ledger().sequence() });
        e.events().publish((symbol_short!("ck_nav"),), rate);
        rate
    }
    /// Stage L1b. Permissionless liquidation-only fallback checkpoint. Allowed only when a named
    /// cause makes checkpoint_nav impossible right now: depeg guard, pool out of band/range, or
    /// the venue valuation failing. Value: LP as if entirely the cheaper asset at the band edge,
    /// PYUSD at min(oracle, lower-edge price) x (1 - haircut); booked idle; no fees, no rewards, no pool quote.
    pub fn checkpoint_fallback(e: Env) -> u128 {
        let c = cfg(&e); let pr = prm(&e);
        let p = Self::pyusd_price(&e).unwrap_or_else(|| panic_with_error!(&e, E::OracleStale));
        let supply = Self::supply(&e);
        let cause_depeg = !Self::depeg_ok(p, pr.depeg_bps);
        let cause_band = !cause_depeg && !Self::band_ok(&e, p);
        if !cause_depeg && !cause_band {
            let r = e.try_invoke_contract::<Vec<i128>, soroban_sdk::InvokeError>(&c.vault, &Symbol::new(&e, "get_asset_amounts_per_shares"), (supply as i128,).into_val(&e));
            if matches!(r, Ok(Ok(_))) { panic_with_error!(&e, E::NavRefreshable); }
        }
        let l: u128 = call(&e, &c.vault, "get_position_liquidity", Vec::new(&e));
        let idle0: i128 = call(&e, &c.vault, "get_idle", Vec::new(&e));
        let idle1v: i128 = call(&e, &c.vault, "get_other_idle_value", Vec::new(&e)); // PYUSD units at oracle
        // Lower-edge price P_lo = s_lo^2 (1e14 scale, ~0.998001). An LP position's value rises with the
        // price, so its value at P_lo (entirely PYUSD, the cheaper asset) bounds every in-range value
        // from below: value(s) - a0*P_lo = L(s - s_lo)(2 - (s + s_lo)/s_hi) >= 0. Below the band the
        // position is all PYUSD worth a0*p, hence min(p, P_lo). Valuing a0 at p itself would OVERSTATE
        // an in-range position by L(s - s_lo)^2/s_lo.
        let p_lo = S_LO.checked_mul(S_LO).unwrap() / 10_000_000_000_000_000_000_000u128;
        let pf = p.min(p_lo).checked_mul(10_000 - pr.fb_haircut_bps as u128).unwrap() / 10_000;
        let a0 = u(&e, l).mul(&u(&e, Q)).mul(&u(&e, S_HI - S_LO)).div(&u(&e, S_LO).mul(&u(&e, S_HI))).to_u128().unwrap(); // all PYUSD
        let lp = a0.checked_mul(pf).unwrap() / P_SCALE;
        let idle = (idle0.max(0) as u128).checked_mul(pf).unwrap() / P_SCALE + (idle1v.max(0) as u128).checked_mul(p).unwrap() / P_SCALE;
        let rate = lp.checked_add(idle).unwrap().checked_mul(RATE).unwrap() / supply;
        if rate == 0 { panic_with_error!(&e, E::FallbackZero); }
        e.storage().instance().set(&K::Fb, &Ck { rate, ts: now(&e), ledger: e.ledger().sequence() });
        e.events().publish((symbol_short!("ck_fb"),), (rate, cause_depeg, cause_band));
        rate
    }
    pub fn nav_checkpoint(e: Env) -> Option<Ck> { inst(&e, &K::Nav) }
    pub fn fallback_checkpoint(e: Env) -> Option<Ck> { inst(&e, &K::Fb) }

    // ---- controller-facing views ----
    pub fn get_account_snapshot(e: Env, user: Address) -> (u128, u128, u128, Address) {
        let n = units(&e, &user);
        (n, 0, if n == 0 { RATE } else { Self::nav_rate(&e) }, cfg(&e).usdc)
    }
    /// Liquidation-context health snapshot (patched controller, liquidation path only).
    pub fn get_liquidation_account_snapshot(e: Env, user: Address) -> (u128, u128, u128, Address) {
        let n = units(&e, &user);
        (n, 0, if n == 0 { RATE } else { Self::liq_rate(&e) }, cfg(&e).usdc)
    }
    pub fn get_liquidation_snapshot(e: Env, user: Address) -> (u128, u128, Address) {
        (units(&e, &user), Self::liq_rate(&e), cfg(&e).usdc)
    }
    pub fn get_exchange_rate(e: Env) -> u128 { Self::nav_rate(&e) }
    pub fn get_ptoken_balance(e: Env, user: Address) -> u128 { units(&e, &user) }
    pub fn get_user_borrow_balance(_e: Env, _user: Address) -> u128 { 0 }
    pub fn get_underlying_token(e: Env) -> Address { cfg(&e).usdc }
    pub fn get_total_ptokens(e: Env) -> u128 { inst(&e, &K::Total).unwrap() }
    pub fn get_total_borrowed(_e: Env) -> u128 { 0 }
    pub fn get_available_liquidity(_e: Env) -> u128 { 0 }
    pub fn update_interest(_e: Env) {}

    /// Owner removes units from collateral (stands for stage_prepare_close of an entered owner).
    /// Reverts on a NAV checkpoint older than max_age; health-checked when the owner has debt.
    pub fn redeem_collateral(e: Env, owner: Address, n: u128) {
        owner.require_auth();
        let rate = Self::fresh(&e, &K::Nav).unwrap_or_else(|| panic_with_error!(&e, E::NavStale));
        let have = units(&e, &owner); assert!(n > 0 && n <= have, "units");
        let c = cfg(&e); let me = e.current_contract_address();
        let borrows: u128 = call(&e, &c.ctrl, "get_borrows_excl", (owner.clone(), me.clone()).into_val(&e));
        if borrows > 0 {
            let other: u128 = call(&e, &c.ctrl, "get_collateral_excl_usd", (owner.clone(), me.clone()).into_val(&e));
            let cf: u128 = call(&e, &c.ctrl, "get_market_cf", (me.clone(),).into_val(&e));
            let (pr, sc): (u128, u128) = call::<Option<(u128, u128)>>(&e, &c.ctrl, "get_price_usd", (c.usdc.clone(),).into_val(&e)).expect("price unavailable");
            let left = (have - n).checked_mul(rate).unwrap() / RATE * cf / RATE * pr / sc;
            if other.checked_add(left).unwrap() < borrows { panic_with_error!(&e, E::InsufficientCollateral); }
        }
        pset(&e, &K::Units(owner.clone()), have - n);
        pset(&e, &K::VLocked(owner.clone()), pget(&e, &K::VLocked(owner.clone())) + n);
        let t: u128 = inst(&e, &K::Total).unwrap();
        e.storage().instance().set(&K::Total, &(t - n));
    }

    /// Stage L3 part: units-only seizure, called by the controller only. The receipt-side unit
    /// mirror moves atomically with the repay; the vault-side settle-only reassign is queued.
    pub fn seize(e: Env, borrower: Address, liquidator: Address, ptoken_amount: u128, ctx: Option<SeizeContext>) {
        let c = cfg(&e); c.ctrl.require_auth();
        let b = units(&e, &borrower); assert!(ptoken_amount > 0 && ptoken_amount <= b, "seize exceeds units");
        pset(&e, &K::Units(borrower.clone()), b - ptoken_amount);
        // Liquidation fee (controller SeizeContext): fee units go to the reserve recipient.
        let (fee, rec) = match ctx { Some(x) if x.fee_ptokens > 0 && x.fee_recipient.is_some() => (x.fee_ptokens, x.fee_recipient), _ => (0, None) };
        let mut moves: Vec<(Address, u128)> = Vec::new(&e);
        moves.push_back((liquidator.clone(), ptoken_amount - fee));
        if let Some(r) = rec { moves.push_back((r, fee)); }
        for (to, n) in moves.iter() {
            pset(&e, &K::Units(to.clone()), units(&e, &to).checked_add(n).unwrap());
            if inst::<bool>(&e, &K::Inline).unwrap_or(false) {
                Self::do_sync(&e, &borrower, &to, n);
            } else {
                let id: u64 = inst(&e, &K::PCount).unwrap_or(0);
                e.storage().persistent().set(&K::Pend(id), &Pending { from: borrower.clone(), to: to.clone(), n });
                e.storage().instance().set(&K::PCount, &(id + 1));
            }
        }
        e.events().publish((symbol_short!("seize"),), (borrower, liquidator, ptoken_amount));
    }
    /// Stage L5. Permissionless: apply a queued seizure to the vault-side custody/reward ledger.
    pub fn sync_seized(e: Env, id: u64) {
        let p: Pending = e.storage().persistent().get(&K::Pend(id)).unwrap_or_else(|| panic_with_error!(&e, E::NoPending));
        Self::do_sync(&e, &p.from, &p.to, p.n);
        e.storage().persistent().remove(&K::Pend(id));
    }
    pub fn pending_seizures(e: Env) -> u64 { inst(&e, &K::PCount).unwrap_or(0) }
}
