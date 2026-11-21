//! StgV2: CL collateral class over the StgV1 staged receipt (option B) -- PROTOTYPE overlay.
//! research/cl-collateral-group-20261118/{DESIGN.md, D3-CONTINUITY.md, STGV2.md}.
//!
//! The staged units (`StgV1Units`, AVAILABLE units) become collateral in ONE fresh, isolated
//! group controller. Valuation numeraire = USDC, the group's only debt asset.
//!
//! * `checkpoint_nav()` (permissionless, no auth): whole-vault NAV of the staged units, the PYUSD
//!   leg priced ONLY by the Reflector Stellar-DEX feed (CALI, PYUSD/USDC). Guards: oracle age
//!   <= 600 s (stale/missing: REVERT, records nothing); |p - 1| <= 100 bps; pool spot within 50 bps
//!   of p and tick inside [-20, 20); venue valuation must answer. A failing guard does not revert:
//!   it is a D3 chain observation (returns 0). Success stores `{rate, ts}` and deletes the chain.
//! * `checkpoint_fallback()` (permissionless): liquidation-only lower-bound valuation, admitted only
//!   after the normal checkpoint has been CONTINUOUSLY blocked for 600 s (d3_chain.rs).
//! * A checkpoint is valid for 60 s. Borrow and exits read the NAV checkpoint only; liquidation
//!   reads NAV, else the fallback; repay reads nothing.
//! * `seize` (controller only, inside `liquidate`, stage L3) moves units and queues the vault-side
//!   settle-only reassign; `sync_seized(id)` (permissionless, stage L5) applies it. No reward claim
//!   inside seizure (condition b); the claim is the separate stage L4.
//! * Exits of an owner with group debt (`stage_prepare_close`, `stage_emergency_exit`) require a
//!   fresh NAV checkpoint and that the remaining units still cover the debt.
//!
//! Storage keys are all `StgV2*` symbols (new collision domain; nothing existing is re-typed).
use super::ReceiptVault;
#[allow(unused_imports)]
use super::{ReceiptVaultArgs, ReceiptVaultClient};
use crate::storage::{ensure_initialized_for_snapshot, DataKey, SeizeContext};
use soroban_sdk::{
    contracterror, contractimpl, contracttype, panic_with_error, Address, Bytes, Env, IntoVal, Symbol, Val, Vec, U256,
};

#[path = "d3_chain.rs"]
mod d3;

pub const STG2_MAX_AGE: u64 = 60;
pub const STG2_ORACLE_MAX_AGE: u64 = 600;
pub const STG2_DEPEG_BPS: u128 = 100;
pub const STG2_BAND_BPS: u128 = 50;
pub const STG2_HAIRCUT_BPS: u128 = 200;
const RATE: u128 = 1_000_000;
const P_SCALE: u128 = 100_000_000_000_000; // Reflector 14 decimals
const Q: u128 = 1_000_000_000_000_000_000;
// sqrt(1.0001^-20), sqrt(1.0001^20) at 1e18, rounded inward (vault range [-20, 20]).
const S_LO: u128 = 999_000_549_780_071_480;
const S_HI: u128 = 1_001_000_450_120_021_002;
const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND: u32 = 1_000_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Stg2Error {
    NavStale = 2101,
    NavRefreshable = 2104,
    OracleStale = 2105,
    InsufficientCollateral = 2106,
    NoUnits = 2108,
    NoPending = 2109,
    FallbackZero = 2110,
    NotBlocked = 2111,
    BlockTooRecent = 2112,
    ChainBroken = 2113,
    Inactive = 2114,
    SeizeRejected = 2115,
    EnteredMarket = 2116,
}

#[contracttype]
#[derive(Clone)]
enum Stg2Key {
    StgV2Config,       // INSTANCE Stg2Config; presence == collateral class active
    StgV2Nav,          // INSTANCE Stg2Ck
    StgV2Fb,           // INSTANCE Stg2Ck (liquidation only)
    StgV2Blk,          // INSTANCE Stg2Blk (D3 chain)
    StgV2PendNext,     // INSTANCE u64
    StgV2Pend(u64),    // PERSISTENT Stg2Pending
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stg2Config {
    /// The fresh group controller; must equal this receipt's bound Peridottroller.
    pub controller: Address,
    /// Aquarius concentrated pool of the vault (band and range guard only; never a price source).
    pub pool: Address,
    /// Reflector Stellar-DEX feed (CALI on Mainnet): lastprice(Stellar(PYUSD)) in USDC.
    pub oracle: Address,
    /// Numeraire and debt asset (USDC SAC).
    pub usdc: Address,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stg2Ck { pub rate: u128, pub ts: u64 }
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stg2Blk { pub since: u64, pub last: u64, pub n: u32 }
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stg2Pending { pub from: Address, pub to: Address, pub n: u128 }
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stg2Status { pub nav: Option<Stg2Ck>, pub fallback: Option<Stg2Ck>, pub chain: Option<Stg2Blk>, pub next_pending: u64 }

// Reflector / Aquarius wire shapes (same encodings as the live contracts).
#[contracttype]
#[derive(Clone)]
pub enum Stg2Asset { Stellar(Address), Other(Symbol) }
#[contracttype]
#[derive(Clone)]
pub struct Stg2PriceData { pub price: i128, pub timestamp: u64 }
#[contracttype]
#[derive(Clone)]
pub struct Stg2Slot0 { pub sqrt_price_x96: U256, pub tick: i32 }

fn i(e: &Env) -> soroban_sdk::storage::Instance { e.storage().instance() }
pub(super) fn stg2_active(e: &Env) -> bool { i(e).has(&Stg2Key::StgV2Config) }
fn cfg(e: &Env) -> Stg2Config {
    i(e).get(&Stg2Key::StgV2Config).unwrap_or_else(|| panic_with_error!(e, Stg2Error::Inactive))
}
fn now(e: &Env) -> u64 { e.ledger().timestamp() }
fn vault(e: &Env) -> Address { super::staged::stg_vault(e) }
fn call<T: soroban_sdk::TryFromVal<Env, Val>>(e: &Env, c: &Address, f: &str, a: Vec<Val>) -> T {
    e.invoke_contract(c, &Symbol::new(e, f), a)
}
fn u(e: &Env, n: u128) -> U256 { U256::from_u128(e, n) }
fn fresh(e: &Env, k: &Stg2Key) -> Option<u128> {
    match i(e).get::<_, Stg2Ck>(k) {
        Some(c) if now(e) >= c.ts && now(e) - c.ts <= STG2_MAX_AGE => Some(c.rate),
        _ => None,
    }
}
/// Borrow / exit rate: NAV checkpoint only; 0 = invalid (the controller then fails the borrow).
fn nav_rate(e: &Env) -> u128 { fresh(e, &Stg2Key::StgV2Nav).unwrap_or(0) }
/// Liquidation rate: NAV checkpoint, else the fallback checkpoint, else 0 (health indeterminate).
fn liq_rate(e: &Env) -> u128 { fresh(e, &Stg2Key::StgV2Nav).or_else(|| fresh(e, &Stg2Key::StgV2Fb)).unwrap_or(0) }
fn units(e: &Env, a: &Address) -> u128 { super::staged::stg_units_pub(e, a) }

/// CALI lastprice(Stellar(PYUSD)) in USDC (14 decimals). Missing / future / older than 600 s /
/// non-positive => OracleStale (revert): both checkpoints become impossible, repay-only (D4).
fn price(e: &Env, c: &Stg2Config, pyusd: &Address) -> u128 {
    let pd: Option<Stg2PriceData> = call(e, &c.oracle, "lastprice", (Stg2Asset::Stellar(pyusd.clone()),).into_val(e));
    match pd {
        Some(p) if p.price > 0 && p.timestamp <= now(e) && now(e) - p.timestamp <= STG2_ORACLE_MAX_AGE => p.price as u128,
        _ => panic_with_error!(e, Stg2Error::OracleStale),
    }
}
fn depeg_ok(p: u128) -> bool { p.abs_diff(P_SCALE).checked_mul(10_000).map_or(false, |x| x <= STG2_DEPEG_BPS * P_SCALE) }
/// Pool spot (USDC per PYUSD) within 50 bps of the oracle AND tick inside the position's range.
fn band_ok(e: &Env, c: &Stg2Config, p: u128) -> bool {
    let s0: Stg2Slot0 = match e.try_invoke_contract::<Stg2Slot0, soroban_sdk::InvokeError>(&c.pool, &Symbol::new(e, "get_slot0"), Vec::new(e)) {
        Ok(Ok(s)) => s,
        _ => return false,
    };
    if s0.tick < -20 || s0.tick >= 20 { return false; }
    let s = s0.sqrt_price_x96.mul(&u(e, Q)).div(&u(e, 1u128 << 96));
    if s > u(e, 2 * Q) { return false; }
    let spot = s.mul(&s).div(&u(e, Q)).to_u128().unwrap(); // 1e18
    let Some(p18) = p.checked_mul(10_000) else { return false; };
    match (spot.abs_diff(p18).checked_mul(10_000), p18.checked_mul(STG2_BAND_BPS)) { (Some(x), Some(y)) => x <= y, _ => false }
}
fn chain(e: &Env) -> Option<d3::Chain> { i(e).get::<_, Stg2Blk>(&Stg2Key::StgV2Blk).map(|b| d3::Chain { since: b.since, last: b.last, n: b.n }) }
fn put_chain(e: &Env, c: d3::Chain) { i(e).set(&Stg2Key::StgV2Blk, &Stg2Blk { since: c.since, last: c.last, n: c.n }); }
#[allow(deprecated)]
fn ev(e: &Env, name: &str, data: impl IntoVal<Env, Val>) { e.events().publish((Symbol::new(e, name),), data); }

/// Exit guards. The emergency exit is refused while the owner is ENTERED in this market (its
/// units are collateral): exit the market first (the controller checks health there), then exit.
/// Reason: the no-claim extraction plus any health read exceeds the 100-entry limit (rows RD5/RD8).
/// The check reads only the controller's UserMarkets entry, so a non-entered owner keeps the
/// dependency-free path. Prepare-close of an owner with group debt needs a fresh NAV checkpoint
/// and remaining cover; owners without debt are unaffected.
pub(super) fn stg2_emergency_check(e: &Env, owner: &Address) {
    if !stg2_active(e) { return; }
    let c = cfg(e);
    // F1 (21 Nov 2026): a holder can always emergency-exit while the controller is broken. Fail
    // OPEN only when the controller call itself errors or traps (bad upgrade, unreachable); an
    // answer that lists this market always refuses. Trade-off: an entered borrower can then exit
    // only once the controller is dead, when liquidation is impossible anyway (TESTS-STGV2.md).
    let m = match e.try_invoke_contract::<Vec<Address>, soroban_sdk::InvokeError>(&c.controller, &Symbol::new(e, "get_user_markets"), (owner.clone(),).into_val(e)) {
        Ok(Ok(m)) => m,
        _ => return,
    };
    if m.contains(e.current_contract_address()) { panic_with_error!(e, Stg2Error::EnteredMarket); }
}
pub(super) fn stg2_exit_check(e: &Env, owner: &Address, available: u128, shares: u128) {
    if !stg2_active(e) { return; }
    let c = cfg(e);
    let me = e.current_contract_address();
    let borrows: u128 = call(e, &c.controller, "get_borrows_excl", (owner.clone(), me.clone()).into_val(e));
    if borrows == 0 { return; }
    let rate = fresh(e, &Stg2Key::StgV2Nav).unwrap_or_else(|| panic_with_error!(e, Stg2Error::NavStale));
    let left = available.checked_sub(shares).expect("insufficient staged units");
    let other: u128 = call(e, &c.controller, "get_collateral_excl_usd", (owner.clone(), me.clone()).into_val(e));
    let cf: u128 = call(e, &c.controller, "get_market_cf", (me,).into_val(e));
    let (p, s): (u128, u128) = call::<Option<(u128, u128)>>(e, &c.controller, "get_price_usd", (c.usdc.clone(),).into_val(e)).expect("price unavailable");
    let usd = left.checked_mul(rate).unwrap() / RATE * cf / RATE * p / s;
    if other.checked_add(usd).unwrap() < borrows { panic_with_error!(e, Stg2Error::InsufficientCollateral); }
}

/// L3 part, called from the accepted `seize` entrypoint when the class is active.
pub(super) fn stg2_seize(e: &Env, borrower: Address, liquidator: Address, n: u128, ctx: Option<SeizeContext>) {
    let c = cfg(e);
    c.controller.require_auth();
    super::staged::stg_require_idle(e);
    let x = ctx.unwrap_or_else(|| panic_with_error!(e, Stg2Error::SeizeRejected));
    if n == 0 || x.seize_ptokens != n || x.fee_ptokens > n || x.shortfall == 0 || x.expires_at < now(e)
        || (x.fee_ptokens > 0 && x.fee_recipient.is_none()) || borrower == liquidator {
        panic_with_error!(e, Stg2Error::SeizeRejected);
    }
    let have = units(e, &borrower);
    if have < n { panic_with_error!(e, Stg2Error::SeizeRejected); }
    super::staged::stg_set_units_pub(e, &borrower, have - n);
    let mut next: u64 = i(e).get(&Stg2Key::StgV2PendNext).unwrap_or(0);
    let mut queue = |to: Address, k: u128| {
        if k == 0 { return; }
        super::staged::stg_set_units_pub(e, &to, units(e, &to).checked_add(k).unwrap());
        let key = Stg2Key::StgV2Pend(next);
        e.storage().persistent().set(&key, &Stg2Pending { from: borrower.clone(), to, n: k });
        e.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND);
        next += 1;
    };
    queue(liquidator.clone(), n - x.fee_ptokens);
    if let Some(r) = x.fee_recipient { queue(r, x.fee_ptokens); }
    i(e).set(&Stg2Key::StgV2PendNext, &next);
    ev(e, "stg2_seize", (borrower, liquidator, n, next));
}

#[contractimpl]
impl ReceiptVault {
    /// One-time, admin-only activation of the collateral class on an ACTIVE staged-only receipt
    /// that is already bound to the fresh group controller. Default builds refuse Mainnet.
    pub fn stage2_activate(e: Env, admin: Address, c: Stg2Config) {
        assert!(admin == Self::get_admin(e.clone()), "not admin");
        admin.require_auth();
        let public = e.ledger().network_id() == e.crypto().sha256(&Bytes::from_slice(&e, b"Public Global Stellar Network ; September 2015")).to_bytes();
        #[cfg(not(feature = "stg2-mainnet-accepted"))]
        assert!(!public, "StgV2 collateral class is not accepted for Mainnet in this build");
        let _ = public;
        assert!(super::staged::stg_active(&e), "staged mode inactive");
        assert!(!stg2_active(&e), "StgV2 already active");
        let bound: Option<Address> = e.storage().persistent().get(&DataKey::Peridottroller);
        assert!(bound.as_ref() == Some(&c.controller), "receipt not bound to the group controller");
        let pyusd = ensure_initialized_for_snapshot(&e);
        let v = vault(&e);
        assert!(c.usdc != pyusd && c.pool != v && c.oracle != v && c.controller != v, "invalid StgV2 binding");
        i(&e).set(&Stg2Key::StgV2Config, &c);
        i(&e).set(&Stg2Key::StgV2PendNext, &0u64);
    }
    pub fn stage2_config(e: Env) -> Stg2Config { cfg(&e) }
    pub fn stage2_status(e: Env) -> Stg2Status {
        Stg2Status { nav: i(&e).get(&Stg2Key::StgV2Nav), fallback: i(&e).get(&Stg2Key::StgV2Fb), chain: i(&e).get(&Stg2Key::StgV2Blk), next_pending: i(&e).get(&Stg2Key::StgV2PendNext).unwrap_or(0) }
    }
    pub fn stage2_pending(e: Env, id: u64) -> Option<Stg2Pending> { e.storage().persistent().get(&Stg2Key::StgV2Pend(id)) }

    /// Stage L1a. Permissionless. Returns the stored rate, or 0 for a D3 blocked observation.
    pub fn checkpoint_nav(e: Env) -> u128 {
        let c = cfg(&e);
        super::staged::stg_require_idle(&e);
        let pyusd = ensure_initialized_for_snapshot(&e);
        let total = super::staged::stg_total_units(&e);
        if total == 0 || total > i128::MAX as u128 { panic_with_error!(&e, Stg2Error::NoUnits); }
        let p = price(&e, &c, &pyusd);
        let nav = match e.try_invoke_contract::<Vec<i128>, soroban_sdk::InvokeError>(&vault(&e), &Symbol::new(&e, "get_asset_amounts_per_shares"), (total as i128,).into_val(&e)) {
            Ok(Ok(v)) if v.len() == 1 => v.get(0).unwrap_or(0),
            _ => 0,
        };
        let cause: u32 = if nav <= 0 { 1 } else if !depeg_ok(p) { 2 } else if !band_ok(&e, &c, p) { 3 } else { 0 };
        if cause != 0 {
            let ch = d3::observe(chain(&e), now(&e));
            put_chain(&e, ch);
            ev(&e, "stg2_blocked", (cause, ch.since, ch.n));
            return 0;
        }
        let rate = (nav as u128).checked_mul(p).unwrap() / P_SCALE * RATE / total;
        if rate == 0 { panic_with_error!(&e, Stg2Error::NoUnits); }
        i(&e).remove(&Stg2Key::StgV2Blk);
        i(&e).set(&Stg2Key::StgV2Nav, &Stg2Ck { rate, ts: now(&e) });
        i(&e).extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
        ev(&e, "stg2_nav", rate);
        rate
    }

    /// Stage L1b. Permissionless, liquidation-only. Re-proves a blocking cause NOW, then requires the
    /// D3 chain (alive, >= 600 s). Value: the position as if entirely PYUSD (the cheaper asset at or
    /// below the lower band edge) at min(p, P_lo) x (1 - 2%), plus booked idle; no fees, rewards or
    /// pool quote. Lower bound: value(s) - a0*P_lo = L(s - s_lo)(2 - (s + s_lo)/s_hi) >= 0 in range.
    pub fn checkpoint_fallback(e: Env) -> u128 {
        let c = cfg(&e);
        super::staged::stg_require_idle(&e);
        let pyusd = ensure_initialized_for_snapshot(&e);
        let v = vault(&e);
        let p = price(&e, &c, &pyusd);
        let supply: i128 = call(&e, &v, "total_supply", Vec::new(&e));
        if supply <= 0 { panic_with_error!(&e, Stg2Error::NoUnits); }
        if depeg_ok(p) && band_ok(&e, &c, p) {
            let r = e.try_invoke_contract::<Vec<i128>, soroban_sdk::InvokeError>(&v, &Symbol::new(&e, "get_asset_amounts_per_shares"), (supply,).into_val(&e));
            if matches!(r, Ok(Ok(_))) { panic_with_error!(&e, Stg2Error::NavRefreshable); }
        }
        match d3::admit_fallback(chain(&e), now(&e)) {
            Ok(ch) => put_chain(&e, ch),
            Err(d3::Refusal::NotBlocked) => panic_with_error!(&e, Stg2Error::NotBlocked),
            Err(d3::Refusal::ChainBroken) => panic_with_error!(&e, Stg2Error::ChainBroken),
            Err(d3::Refusal::BlockTooRecent) => panic_with_error!(&e, Stg2Error::BlockTooRecent),
        }
        let l: u128 = call(&e, &v, "get_position_liquidity", Vec::new(&e));
        let idle: Vec<u128> = call(&e, &v, "stage_idle", Vec::new(&e));
        let p_lo = S_LO * S_LO / 10_000_000_000_000_000_000_000u128; // 1e14 scale
        let pf = p.min(p_lo) * (10_000 - STG2_HAIRCUT_BPS) / 10_000;
        let a0 = u(&e, l).mul(&u(&e, Q)).mul(&u(&e, S_HI - S_LO)).div(&u(&e, S_LO).mul(&u(&e, S_HI))).to_u128().unwrap();
        let pyusd_amt = a0.checked_add(idle.get(0).unwrap_or(0)).unwrap();
        let value = pyusd_amt.checked_mul(pf).unwrap() / P_SCALE + idle.get(1).unwrap_or(0);
        let rate = value.checked_mul(RATE).unwrap() / supply as u128;
        if rate == 0 { panic_with_error!(&e, Stg2Error::FallbackZero); }
        i(&e).set(&Stg2Key::StgV2Fb, &Stg2Ck { rate, ts: now(&e) });
        ev(&e, "stg2_fallback", rate);
        rate
    }

    /// Liquidation-context health snapshot (group controller, liquidation path only).
    pub fn get_liquidation_account_snapshot(e: Env, user: Address) -> (u128, u128, u128, Address) {
        let n = units(&e, &user);
        (n, 0, if n == 0 { RATE } else { liq_rate(&e) }, cfg(&e).usdc)
    }
    pub fn get_liquidation_snapshot(e: Env, user: Address) -> (u128, u128, Address) {
        (units(&e, &user), liq_rate(&e), cfg(&e).usdc)
    }

    /// Stage L5. Permissionless: apply a queued seizure to the vault's custody/reward ledger
    /// (settle-only reassign; boundary if the last claim is > 60 s old, D6).
    pub fn sync_seized(e: Env, id: u64) -> bool {
        cfg(&e);
        super::staged::stg_require_idle(&e);
        let k = Stg2Key::StgV2Pend(id);
        let p: Stg2Pending = e.storage().persistent().get(&k).unwrap_or_else(|| panic_with_error!(&e, Stg2Error::NoPending));
        let opened: bool = call(&e, &vault(&e), "stage_seize_reassign", (e.current_contract_address(), p.from.clone(), p.to.clone(), p.n).into_val(&e));
        e.storage().persistent().remove(&k);
        ev(&e, "stg2_synced", (id, p.from, p.to, p.n, opened));
        opened
    }
}

// ---- controller-facing dispatch, prepended to the accepted entrypoints (StgV2 active only) ----
pub(super) fn stg2_account_snapshot(e: &Env, user: &Address) -> (u128, u128, u128, Address) {
    let n = units(e, user);
    (n, 0, if n == 0 { RATE } else { nav_rate(e) }, cfg(e).usdc)
}
pub(super) fn stg2_exchange_rate(e: &Env) -> u128 { nav_rate(e) }
pub(super) fn stg2_units(e: &Env, user: &Address) -> u128 { units(e, user) }
pub(super) fn stg2_numeraire(e: &Env) -> Address { cfg(e).usdc }
pub(super) fn stg2_total_units(e: &Env) -> u128 { super::staged::stg_total_units(e) }

/// MEASUREMENT BUILDS ONLY (`--features stg2-measure`, never a deployment candidate): seed the D3
/// chain so footprint rows on a frozen-oracle capture need not wait 10 minutes. The time-edge
/// logic is proven by the d3-chain unit tests. Refuses Mainnet even in a measurement build.
#[cfg(feature = "stg2-measure")]
#[contractimpl]
impl ReceiptVault {
    pub fn stage2_measure_set_chain(e: Env, b: Option<Stg2Blk>) {
        let public = e.ledger().network_id() == e.crypto().sha256(&Bytes::from_slice(&e, b"Public Global Stellar Network ; September 2015")).to_bytes();
        assert!(!public, "measurement build");
        match b { Some(x) => i(&e).set(&Stg2Key::StgV2Blk, &x), None => i(&e).remove(&Stg2Key::StgV2Blk) }
    }
}
