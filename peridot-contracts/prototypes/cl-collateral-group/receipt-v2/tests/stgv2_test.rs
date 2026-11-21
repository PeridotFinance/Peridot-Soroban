//! NATIVE tests for the StgV2 CL collateral class (receipt `staged_v2.rs` + `d3_chain.rs`) wired to
//! the REAL group controller (G0-G10, compiled natively as a DEPENDENCY, so `cfg!(test)` is false in
//! it and its real 24 h admin timelock applies) and a second, real `ReceiptVault` instance as the
//! static-rate USDC lending market.
//!
//! SCOPE WARNING: the CL vault is `MockClVault`, a deterministic test double of the receipt-custody
//! stage ABI (copied from the StgV1 `MockStagedVault`, plus the four StgV2 views/entrypoints). It
//! prices both legs at parity and has a pre-funded venue inventory. The oracle (Reflector ABI) and
//! the pool (`get_slot0`) are settable doubles. These tests prove RECEIPT-side and CONTROLLER-side
//! logic. Reward conservation across a seizure boundary is a VAULT property, tested on the real
//! vault v2 code in `vault-v2/contracts/cl-share-vault/src/seize_tests.rs`. Footprints are NOT
//! measured here; the compiled-bytes rows are in BUILD-REPORT.md §2 (RD8 / RD9 for the emergency exit).
//!
//! Installed into a disposable copy of the candidate tree by `make-test-tree.sh`.
extern crate std;
use crate::{
    ReceiptVault, ReceiptVaultClient, Stg2Asset, Stg2Blk, Stg2Ck, Stg2Config, Stg2Pending, Stg2PriceData, Stg2Slot0, StgJob,
    STG_CLOSE_LOCKED, STG_CLOSE_MIXTURE, STG_CONVERTED, STG_EMERGENCY_HELD, STG_EMERGENCY_REQUESTED, STG_FUNDED,
    STG_INVESTED, STG_MINTED, STG_PREPARED,
};
use simple_peridottroller::SimplePeridottrollerClient;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    token, vec, Address, Env, IntoVal, Symbol, Vec, U256,
};

// NOTE: `stage2_status` is a crate-internal accessor in the TEST tree only (see make-test-tree.sh:
// soroban-sdk 25.3.1 testutils cannot convert the `Option<contracttype>` fields of `Stg2Status`).

// ------------------------------------------------------------------ mock CL vault
#[contracttype]
#[derive(Clone)]
enum MK { Receipt, Funding, Other, Sink, Shares(Address), Bene(Address), Locked(Address), Total, BackF, BackO, Held(u64), Flag(Symbol), Reassigns }
#[contracttype]
#[derive(Clone)]
struct Held { f: u128, o: u128, reserve: u128 }

#[contract]
pub struct MockClVault;
fn mg(e: &Env, k: MK) -> u128 { e.storage().instance().get(&k).unwrap_or(0) }
fn ms(e: &Env, k: MK, v: u128) { e.storage().instance().set(&k, &v); }
fn maddr(e: &Env, k: MK) -> Address { e.storage().instance().get(&k).unwrap() }
fn mflag(e: &Env, n: &str) -> u128 { mg(e, MK::Flag(Symbol::new(e, n))) }
fn mpay(e: &Env, k: MK, to: &Address, n: u128) {
    if n > 0 { token::Client::new(e, &maddr(e, k)).transfer(&e.current_contract_address(), to, &(n as i128)); }
}
fn mjob(e: &Env, r: &Address, phase: u32, j: &StgJob) {
    assert!(*r == maddr(e, MK::Receipt), "mock: wrong receipt");
    r.require_auth();
    assert!(j.phase == phase, "mock: wrong phase");
}
fn mheld(e: &Env, id: u64) -> Held { e.storage().instance().get(&MK::Held(id)).unwrap_or(Held { f: 0, o: 0, reserve: 0 }) }

#[contractimpl]
impl MockClVault {
    pub fn init(e: Env, receipt: Address, funding: Address, other: Address, sink: Address) {
        let i = e.storage().instance();
        i.set(&MK::Receipt, &receipt); i.set(&MK::Funding, &funding); i.set(&MK::Other, &other); i.set(&MK::Sink, &sink);
    }
    pub fn mock_set(e: Env, name: Symbol, v: u128) { ms(&e, MK::Flag(name), v); }
    /// Venue loss of `bps` on both legs (NAV falls; parity pricing unchanged).
    pub fn mock_lose(e: Env, bps: u128) {
        let sink = maddr(&e, MK::Sink);
        let (f, o) = (mg(&e, MK::BackF), mg(&e, MK::BackO));
        let (lf, lo) = (f * bps / 10_000, o * bps / 10_000);
        mpay(&e, MK::Funding, &sink, lf); mpay(&e, MK::Other, &sink, lo);
        ms(&e, MK::BackF, f - lf); ms(&e, MK::BackO, o - lo);
    }
    pub fn locked(e: Env, owner: Address) -> u128 { mg(&e, MK::Locked(owner)) }
    pub fn bene(e: Env, owner: Address) -> u128 { mg(&e, MK::Bene(owner)) }
    pub fn backing(e: Env) -> u128 { mg(&e, MK::BackF) + mg(&e, MK::BackO) }
    pub fn reassigns(e: Env) -> u128 { mg(&e, MK::Reassigns) }

    pub fn stage_receipt(e: Env) -> Address { maddr(&e, MK::Receipt) }
    pub fn stage_funding_asset(e: Env) -> Address { maddr(&e, MK::Funding) }
    pub fn balance(e: Env, a: Address) -> i128 { mg(&e, MK::Shares(a)) as i128 }
    pub fn get_asset_amounts_per_shares(e: Env, shares: i128) -> Vec<i128> {
        assert!(mflag(&e, "quote_fail") == 0, "mock: quote unavailable");
        let total = mg(&e, MK::Total);
        let v = if total == 0 { 0 } else { (shares as u128) * (mg(&e, MK::BackF) + mg(&e, MK::BackO)) / total };
        let mut out = vec![&e, if mflag(&e, "quote_zero") == 1 { 0 } else { v as i128 }];
        if mflag(&e, "quote_shape") == 2 { out.push_back(0); }
        out
    }
    // ---- StgV2 vault ABI (vault v2: total_supply, get_position_liquidity, stage_idle, stage_seize_reassign)
    pub fn total_supply(e: Env) -> i128 { (mg(&e, MK::Total) + mflag(&e, "extra_supply")) as i128 }
    pub fn get_position_liquidity(e: Env) -> u128 { mflag(&e, "liquidity") }
    pub fn stage_idle(e: Env) -> Vec<u128> { vec![&e, mflag(&e, "idle0"), mflag(&e, "idle1")] }
    /// Settle-only beneficiary move, gated like the real vault: receipt auth, n unlocked beneficial
    /// shares of `from`, distinct holders. Returns the "boundary opened" flag.
    pub fn stage_seize_reassign(e: Env, r: Address, from: Address, to: Address, n: u128) -> bool {
        assert!(r == maddr(&e, MK::Receipt), "mock: wrong receipt");
        r.require_auth();
        assert!(n > 0 && from != to, "mock: invalid seizure");
        assert!(mg(&e, MK::Bene(from.clone())) - mg(&e, MK::Locked(from.clone())) >= n, "mock: shares locked by command");
        ms(&e, MK::Bene(from.clone()), mg(&e, MK::Bene(from.clone())) - n);
        ms(&e, MK::Bene(to.clone()), mg(&e, MK::Bene(to)) + n);
        ms(&e, MK::Reassigns, mg(&e, MK::Reassigns) + 1);
        mflag(&e, "opened") == 1
    }
    // ---- StgV1 command ABI (as MockStagedVault)
    pub fn stage_accept_funds(e: Env, r: Address, id: u64, j: StgJob) {
        mjob(&e, &r, STG_PREPARED, &j);
        j.owner.require_auth();
        token::Client::new(&e, &maddr(&e, MK::Funding)).transfer(&j.owner, &e.current_contract_address(), &((j.a + j.reserve) as i128));
        e.storage().instance().set(&MK::Held(id), &Held { f: j.a, o: 0, reserve: j.reserve });
    }
    pub fn stage_convert_open(e: Env, r: Address, id: u64, j: StgJob) -> u128 {
        mjob(&e, &r, STG_FUNDED, &j);
        let out = j.swap_in * 9_990 / 10_000;
        e.storage().instance().set(&MK::Held(id), &Held { f: j.a - j.swap_in, o: out, reserve: j.reserve });
        out
    }
    pub fn stage_cash_mint(e: Env, r: Address, id: u64, j: StgJob) -> u128 {
        mjob(&e, &r, STG_CONVERTED, &j);
        let (value, total, nav) = (j.a + j.b, mg(&e, MK::Total), mg(&e, MK::BackF) + mg(&e, MK::BackO));
        let q = if total == 0 { value } else { value * total / nav };
        ms(&e, MK::Shares(r.clone()), mg(&e, MK::Shares(r.clone())) + q);
        ms(&e, MK::Bene(j.owner.clone()), mg(&e, MK::Bene(j.owner.clone())) + q);
        ms(&e, MK::Locked(j.owner.clone()), mg(&e, MK::Locked(j.owner.clone())) + q);
        ms(&e, MK::Total, total + q);
        ms(&e, MK::BackF, mg(&e, MK::BackF) + j.a); ms(&e, MK::BackO, mg(&e, MK::BackO) + j.b);
        e.storage().instance().set(&MK::Held(id), &Held { f: 0, o: 0, reserve: j.reserve });
        q
    }
    pub fn stage_invest_cash(e: Env, r: Address, _id: u64, j: StgJob) -> u128 { mjob(&e, &r, STG_MINTED, &j); j.a + j.b }
    pub fn stage_refresh_finalise(e: Env, r: Address, _id: u64, j: StgJob) { mjob(&e, &r, STG_INVESTED, &j); }
    pub fn stage_finalise_cash(e: Env, r: Address, id: u64, j: StgJob) {
        mjob(&e, &r, STG_INVESTED, &j);
        ms(&e, MK::Locked(j.owner.clone()), mg(&e, MK::Locked(j.owner.clone())) - j.shares);
        mpay(&e, MK::Funding, &j.owner, mheld(&e, id).reserve);
        e.storage().instance().remove(&MK::Held(id));
    }
    pub fn stage_lock_close(e: Env, r: Address, _id: u64, j: StgJob) {
        mjob(&e, &r, STG_CLOSE_LOCKED, &j);
        let locked = mg(&e, MK::Locked(j.owner.clone()));
        assert!(mg(&e, MK::Bene(j.owner.clone())) >= locked + j.shares, "mock: beneficiary shares insufficient");
        ms(&e, MK::Locked(j.owner.clone()), locked + j.shares);
    }
    pub fn stage_withdraw_kind(e: Env, r: Address, id: u64, j: StgJob) -> Vec<u128> {
        mjob(&e, &r, STG_CLOSE_LOCKED, &j);
        let (total, bf, bo) = (mg(&e, MK::Total), mg(&e, MK::BackF), mg(&e, MK::BackO));
        let (of, oo) = (bf * j.shares / total, bo * j.shares / total);
        ms(&e, MK::Shares(r.clone()), mg(&e, MK::Shares(r.clone())) - j.shares);
        ms(&e, MK::Total, total - j.shares);
        ms(&e, MK::Bene(j.owner.clone()), mg(&e, MK::Bene(j.owner.clone())) - j.shares);
        ms(&e, MK::Locked(j.owner.clone()), mg(&e, MK::Locked(j.owner.clone())) - j.shares);
        ms(&e, MK::BackF, bf - of); ms(&e, MK::BackO, bo - oo);
        if j.min_swap == 0 { mpay(&e, MK::Funding, &j.owner, of); mpay(&e, MK::Other, &j.owner, oo); }
        else { e.storage().instance().set(&MK::Held(id), &Held { f: of, o: oo, reserve: 0 }); }
        vec![&e, of, oo]
    }
    pub fn stage_convert_exit(e: Env, r: Address, id: u64, j: StgJob) -> u128 {
        mjob(&e, &r, STG_CLOSE_MIXTURE, &j);
        let h = mheld(&e, id);
        let total = h.f + h.o * 9_990 / 10_000;
        mpay(&e, MK::Funding, &j.owner, total);
        e.storage().instance().remove(&MK::Held(id));
        total
    }
    pub fn stage_emergency_extract(e: Env, r: Address, id: u64, j: StgJob, _accept_reward_waiver: bool) -> Vec<u128> {
        mjob(&e, &r, STG_EMERGENCY_REQUESTED, &j);
        assert!(j.kind == 2, "mock: wrong kind");
        j.owner.require_auth();
        assert!(mg(&e, MK::Bene(j.owner.clone())) - mg(&e, MK::Locked(j.owner.clone())) >= j.shares, "mock: shares not available");
        let (total, bf, bo) = (mg(&e, MK::Total), mg(&e, MK::BackF), mg(&e, MK::BackO));
        let (of, oo) = (bf * j.shares / total, bo * j.shares / total);
        ms(&e, MK::Shares(r.clone()), mg(&e, MK::Shares(r.clone())) - j.shares);
        ms(&e, MK::Total, total - j.shares);
        ms(&e, MK::Bene(j.owner.clone()), mg(&e, MK::Bene(j.owner.clone())) - j.shares);
        ms(&e, MK::BackF, bf - of); ms(&e, MK::BackO, bo - oo);
        e.storage().instance().set(&MK::Held(id), &Held { f: of, o: oo, reserve: 0 });
        vec![&e, of, oo]
    }
    pub fn stage_emergency_pay(e: Env, r: Address, id: u64, j: StgJob) -> Vec<u128> {
        mjob(&e, &r, STG_EMERGENCY_HELD, &j);
        let h = mheld(&e, id);
        mpay(&e, MK::Funding, &j.owner, h.f); mpay(&e, MK::Other, &j.owner, h.o);
        e.storage().instance().remove(&MK::Held(id));
        vec![&e, h.f, h.o]
    }
}

// ------------------------------------------------------------------ oracle (Reflector ABI) and pool doubles
#[contracttype]
#[derive(Clone)]
enum OK { P(Address), Trap }
#[contracttype]
#[derive(Clone)]
struct OP { price: i128, ts: u64, live: bool }
#[contract]
pub struct MockReflector;
#[contractimpl]
impl MockReflector {
    /// `live` = the timestamp follows the ledger (a feed that re-timestamps every round).
    pub fn set(e: Env, asset: Address, price: i128, ts: u64, live: bool) { e.storage().instance().set(&OK::P(asset), &OP { price, ts, live }); }
    pub fn unset(e: Env, asset: Address) { e.storage().instance().remove(&OK::P(asset)); }
    pub fn trap(e: Env, on: bool) { e.storage().instance().set(&OK::Trap, &on); }
    pub fn decimals(_e: Env) -> u32 { 14 }
    pub fn resolution(_e: Env) -> u32 { 300 }
    pub fn lastprice(e: Env, asset: Stg2Asset) -> Option<Stg2PriceData> {
        assert!(!e.storage().instance().get::<_, bool>(&OK::Trap).unwrap_or(false), "oracle trap");
        let Stg2Asset::Stellar(a) = asset else { return None };
        let p: OP = e.storage().instance().get(&OK::P(a))?;
        Some(Stg2PriceData { price: p.price, timestamp: if p.live { e.ledger().timestamp() } else { p.ts } })
    }
}
#[contracttype]
#[derive(Clone)]
enum PKey { S, Trap }
#[contract]
pub struct MockPool;
#[contractimpl]
impl MockPool {
    pub fn set(e: Env, sqrt_price_x96: U256, tick: i32) { e.storage().instance().set(&PKey::S, &Stg2Slot0 { sqrt_price_x96, tick }); }
    pub fn trap(e: Env, on: bool) { e.storage().instance().set(&PKey::Trap, &on); }
    pub fn get_slot0(e: Env) -> Stg2Slot0 {
        assert!(!e.storage().instance().get::<_, bool>(&PKey::Trap).unwrap_or(false), "pool trap");
        e.storage().instance().get(&PKey::S).unwrap()
    }
}
/// Replacement for the controller that traps on every call ("controller broken / bad upgrade").
#[contract]
pub struct TrapController;
#[contractimpl]
impl TrapController {
    pub fn get_user_markets(_e: Env, _user: Address) -> Vec<Address> { panic!("controller broken") }
    pub fn get_borrows_excl(_e: Env, _user: Address, _m: Address) -> u128 { panic!("controller broken") }
}

// ------------------------------------------------------------------ fixture
pub(crate) const SEED: u128 = 10_000_000;
pub(crate) const CAP: u128 = 1_000_000_000_000;
pub(crate) const T0: u64 = 1_800_000_000;
pub(crate) const P1: i128 = 100_000_000_000_000; // 1.0 at 14 decimals
pub(crate) const X96: u128 = 79_228_162_514_264_337_593_543_950_336; // 2^96: pool spot exactly 1.0
pub(crate) const CF: u128 = 500_000;
pub(crate) const DEP: u128 = 1_000_000_000; // 100 PYUSD opened
pub(crate) const UNITS: u128 = 999_500_000; // first open: half swapped at 99.9 %

pub(crate) struct G {
    pub env: Env, pub admin: Address, pub guardian: Address, pub alice: Address, pub bob: Address, pub liq: Address,
    pub supplier: Address, pub reserve: Address,
    pub pyusd: Address, pub usdc: Address, pub receipt: Address, pub ctl: Address, pub usdc_m: Address,
    pub vault: Address, pub oracle: Address, pub pool: Address,
}
impl G {
    pub fn c(&self) -> ReceiptVaultClient<'_> { ReceiptVaultClient::new(&self.env, &self.receipt) }
    pub fn m(&self) -> ReceiptVaultClient<'_> { ReceiptVaultClient::new(&self.env, &self.usdc_m) }
    pub fn v(&self) -> MockClVaultClient<'_> { MockClVaultClient::new(&self.env, &self.vault) }
    pub fn ctl(&self) -> SimplePeridottrollerClient<'_> { SimplePeridottrollerClient::new(&self.env, &self.ctl) }
    pub fn o(&self) -> MockReflectorClient<'_> { MockReflectorClient::new(&self.env, &self.oracle) }
    pub fn pool(&self) -> MockPoolClient<'_> { MockPoolClient::new(&self.env, &self.pool) }
    pub fn now(&self) -> u64 { self.env.ledger().timestamp() }
    pub fn adv(&self, dt: u64) { self.env.ledger().with_mut(|l| { l.timestamp += dt; l.sequence_number += 1; }); }
    pub fn bal(&self, t: &Address, who: &Address) -> u128 { token::Client::new(&self.env, t).balance(who) as u128 }
    pub fn mint(&self, t: &Address, to: &Address, n: u128) { token::StellarAssetClient::new(&self.env, t).mint(to, &(n as i128)); }
    pub fn flag(&self, n: &str, v: u128) { self.v().mock_set(&Symbol::new(&self.env, n), &v); }
    pub fn set_p(&self, p: i128) { self.o().set(&self.pyusd, &p, &0, &true); }
    pub fn stale_p(&self) { self.o().set(&self.pyusd, &P1, &(self.now() - 601), &false); }
    pub fn set_pool(&self, x: u128, tick: i32) { self.pool().set(&U256::from_u128(&self.env, x), &tick); }
    pub fn deadline(&self) -> u64 { self.now() + 600 }
    pub fn auth_all(&self) { self.env.mock_all_auths_allowing_non_root_auth(); }

    /// Full group stack in the `deploy_real` order (harness/src/real.rs): receipt with StgV1 then
    /// StgV2 active, USDC market, controller with sleeve + CF + cache-only liquidation.
    pub fn new() -> Self { Self::build(true) }
    pub fn build(cache_only: bool) -> Self {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        env.ledger().with_mut(|l| { l.timestamp = T0; l.sequence_number = 1_000; });
        let g = |e: &Env| Address::generate(e);
        let (admin, guardian, alice, bob, liq, supplier, reserve, sink) = (g(&env), g(&env), g(&env), g(&env), g(&env), g(&env), g(&env), g(&env));
        let pyusd = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let usdc = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let receipt = env.register(ReceiptVault, ());
        let usdc_m = env.register(ReceiptVault, ());
        let ctl = env.register(simple_peridottroller::SimplePeridottroller, ());
        let vault = env.register(MockClVault, ());
        let oracle = env.register(MockReflector, ());
        let pool = env.register(MockPool, ());
        let t = G { env, admin, guardian, alice, bob, liq, supplier, reserve, pyusd, usdc, receipt, ctl, usdc_m, vault, oracle, pool };
        t.o().set(&t.pyusd, &P1, &0, &true);
        t.o().set(&t.usdc, &P1, &0, &true);
        t.set_pool(X96, 0);
        t.ctl().initialize(&t.admin);
        t.ctl().set_oracle(&t.oracle);
        t.ctl().set_pause_guardian(&t.guardian);
        // Receipt: StgV1 operator checklist (as staged_test.rs `build`), then StgV2.
        t.c().initialize(&t.pyusd, &0, &0, &t.admin);
        t.ctl().add_market(&t.receipt);
        t.ctl().set_pause_deposit(&t.receipt, &false);
        t.ctl().set_pause_redeem(&t.receipt, &false);
        t.ctl().set_pause_borrow(&t.receipt, &true);
        t.c().set_peridottroller(&t.ctl);
        t.c().set_min_cash_reserve_bps(&t.admin, &10_000);
        t.c().set_supply_cap(&SEED);
        t.mint(&t.pyusd, &t.admin, SEED);
        t.c().seed_minimum_supply(&t.admin, &SEED);
        t.mint(&t.pyusd, &t.vault, 1_000_000_000_000); t.mint(&t.usdc, &t.vault, 1_000_000_000_000);
        t.v().init(&t.receipt, &t.pyusd, &t.usdc, &sink);
        t.flag("liquidity", 1_000_000_000_000); // default position liquidity (fallback tests override)
        t.c().stage_activate(&t.admin, &t.vault, &t.guardian, &CAP);
        t.c().stage2_activate(&t.admin, &Stg2Config { controller: t.ctl.clone(), pool: t.pool.clone(), oracle: t.oracle.clone(), usdc: t.usdc.clone() });
        t.ctl().add_market(&t.receipt); // re-list: MarketUnderlying now reads the StgV2 numeraire (USDC)
        // USDC lending market (static 8 %), as deploy_real.
        t.m().initialize(&t.usdc, &0, &0, &t.admin);
        t.m().set_min_cash_reserve_bps(&t.admin, &1_000);
        t.m().set_cash_borrowing_enabled(&t.admin, &true);
        t.m().set_borrow_rate(&80_000);
        t.m().enable_static_rates(&t.admin);
        t.mint(&t.usdc, &t.admin, 10_000);
        t.ctl().add_market(&t.usdc_m);
        t.m().set_peridottroller(&t.ctl);
        t.m().seed_minimum_supply(&t.admin, &10_000);
        t.ctl().set_pause_deposit(&t.usdc_m, &false);
        t.ctl().set_pause_redeem(&t.usdc_m, &false);
        t.ctl().set_pause_borrow(&t.usdc_m, &false);
        t.ctl().group_set_sleeve(&t.receipt);
        t.ctl().set_market_cf(&t.receipt, &CF);
        if cache_only { t.ctl().group_set_liq_cache_only(&true); }
        t.mint(&t.usdc, &t.supplier, 50_000_000_000);
        t.m().deposit(&t.supplier, &50_000_000_000);
        for who in [&t.alice, &t.bob] { t.mint(&t.pyusd, who, 50_000_000_000); }
        t.mint(&t.usdc, &t.liq, 50_000_000_000);
        t.mint(&t.usdc, &t.bob, 50_000_000_000);
        for who in [&t.liq, &t.bob] { token::Client::new(&t.env, &t.usdc).approve(who, &t.usdc_m, &(50_000_000_000i128), &3_000_000); }
        t
    }
    /// One finalised staged open of DEP for `who`; returns the units credited.
    pub fn stake(&self, who: &Address) -> u128 {
        let before = self.c().stage_units(who);
        let id = self.c().stage_open(who, &DEP, &(DEP / 2), &1, &1, &0, &self.deadline());
        self.c().stage_convert(&id); self.c().stage_mint(&id); self.c().stage_invest(&id); self.c().stage_finalise(&id);
        self.c().stage_units(who) - before
    }
    /// alice: staked, entered in the sleeve, fresh NAV checkpoint, then borrows `usdc`.
    pub fn borrower(&self, usdc: u128) -> u128 {
        let n = self.stake(&self.alice);
        self.ctl().enter_market(&self.alice, &self.receipt);
        self.ctl().enter_market(&self.alice, &self.usdc_m);
        self.c().checkpoint_nav();
        if usdc > 0 { self.m().borrow(&self.alice, &usdc); }
        n
    }
    /// Repay the whole debt (accrue first: `repay` caps at the pre-accrual balance).
    pub fn repay_all(&self, who: &Address) {
        self.m().update_interest();
        let debt = self.m().get_user_borrow_balance(who);
        self.mint(&self.usdc, who, debt);
        self.m().repay(who, &debt);
        assert_eq!(self.m().get_user_borrow_balance(who), 0);
    }
    pub fn status(&self) -> crate::Stg2Status { self.env.as_contract(&self.receipt, || ReceiptVault::stage2_status(self.env.clone())) }
    pub fn pend(&self, id: u64) -> Option<Stg2Pending> { self.c().stage2_pending(&id) }
    /// Keep a 600 s D3 chain alive (blocked observations every 60 s, prices re-cached).
    pub fn chain_600(&self) {
        assert_eq!(self.c().checkpoint_nav(), 0, "must be blocked");
        for _ in 0..10 { self.adv(60); assert_eq!(self.c().checkpoint_nav(), 0); }
    }
}
/// Contract error code of a failed `try_` call.
pub(crate) fn code<T: core::fmt::Debug, C: core::fmt::Debug>(r: Result<T, Result<soroban_sdk::Error, C>>) -> u32 {
    match r {
        Err(Ok(e)) => {
            assert!(e.is_type(soroban_sdk::xdr::ScErrorType::Contract), "not a contract error: {e:?}");
            e.get_code()
        }
        other => panic!("expected a contract error, got {other:?}"),
    }
}
pub(crate) fn fails<T, C>(r: Result<T, C>) -> bool { r.is_err() }
/// Independent copy of the NAV formula: nav(total) x p / 1e14 x 1e6 / total.
fn nav_rate(nav: u128, p: u128, total: u128) -> u128 { nav * p / 100_000_000_000_000 * 1_000_000 / total }

// ================================================================== value checkpoint (L1a)
#[test]
fn checkpoint_nav_is_permissionless_vault_wide_and_valid_for_exactly_60_s() {
    let t = G::new();
    let n = t.stake(&t.alice);
    assert_eq!(n, UNITS);
    t.stake(&t.bob);
    assert_eq!(t.c().get_exchange_rate(), 0, "no checkpoint: rate 0 (invalid)");
    assert_eq!(t.c().get_account_snapshot(&t.alice), (n, 0, 0, t.usdc.clone()));
    t.env.set_auths(&[]);
    let r = t.c().checkpoint_nav(); // no authorization at all
    t.auth_all();
    let total = t.c().stage_state().total_units;
    assert_eq!(r, nav_rate(t.v().backing(), P1 as u128, total));
    assert_eq!(t.status().nav, Some(Stg2Ck { rate: r, ts: T0 }));
    // USDC numeraire views; a zero-unit owner reads rate 1e6.
    assert_eq!(t.c().get_underlying_token(), t.usdc);
    assert_eq!(t.c().get_ptoken_balance(&t.alice), n);
    assert_eq!(t.c().get_total_ptokens(), total);
    assert_eq!(t.c().get_account_snapshot(&t.liq), (0, 0, 1_000_000, t.usdc.clone()));
    assert_eq!(t.c().get_liquidation_account_snapshot(&t.liq), (0, 0, 1_000_000, t.usdc.clone()));
    assert_eq!((t.c().get_available_liquidity(), t.c().get_user_borrow_balance(&t.alice), t.c().get_total_borrowed()), (0, 0, 0));
    t.c().update_interest();
    t.adv(60);
    assert_eq!(t.c().get_exchange_rate(), r, "60 s: valid");
    assert_eq!(t.c().get_account_snapshot(&t.alice).2, r);
    assert_eq!(t.c().get_liquidation_snapshot(&t.alice).1, r);
    t.adv(1);
    assert_eq!(t.c().get_exchange_rate(), 0, "61 s: invalid");
    assert_eq!(t.c().get_account_snapshot(&t.alice).2, 0);
    assert_eq!(t.c().get_liquidation_snapshot(&t.alice), (n, 0, t.usdc.clone()));
    // The PYUSD price moves the rate (pool moved with it, inside the band).
    t.set_p(99_500_000_000_000);
    t.set_pool(79_029_843_899_059_723_280_318_636_797, -5);
    let r2 = t.c().checkpoint_nav();
    assert_eq!(r2, nav_rate(t.v().backing(), 99_500_000_000_000, total));
    assert!(r2 < r);
}

#[test]
fn checkpoint_nav_refusals_no_units_oracle_stale_and_inactive_record_nothing() {
    let t = G::new();
    assert_eq!(code(t.c().try_checkpoint_nav()), 2108, "no units");
    t.stake(&t.alice);
    for (p, ts) in [(P1, T0 - 601), (P1, T0 + 1), (0, T0), (-1, T0)] {
        t.o().set(&t.pyusd, &p, &ts, &false);
        assert_eq!(code(t.c().try_checkpoint_nav()), 2105, "oracle stale/future/non-positive");
        assert_eq!(code(t.c().try_checkpoint_fallback()), 2105);
    }
    t.o().set(&t.pyusd, &P1, &(T0 - 600), &false);
    assert!(t.c().checkpoint_nav() > 0, "exactly 600 s old is accepted");
    t.o().unset(&t.pyusd);
    assert_eq!(code(t.c().try_checkpoint_nav()), 2105, "missing price");
    assert_eq!(t.status().chain, None, "an oracle failure is never a D3 observation");
    // A receipt without the class: StgV2 entrypoints refuse (#2114).
    let r2 = t.env.register(ReceiptVault, ());
    let c2 = ReceiptVaultClient::new(&t.env, &r2);
    c2.initialize(&t.pyusd, &0, &0, &t.admin);
    assert_eq!(code(c2.try_checkpoint_nav()), 2114);
    assert_eq!(code(c2.try_checkpoint_fallback()), 2114);
    assert_eq!(code(c2.try_sync_seized(&0)), 2114);
    assert_eq!(code(c2.try_stage2_config()), 2114);
}

#[test]
fn guards_depeg_100_bps_pool_band_50_bps_and_tick_range_edges() {
    let t = G::new();
    t.stake(&t.alice);
    let blocked = |t: &G| { assert_eq!(t.c().checkpoint_nav(), 0, "blocked observation returns 0"); assert!(t.status().chain.is_some()); };
    let ok = |t: &G| { assert!(t.c().checkpoint_nav() > 0); assert!(t.status().chain.is_none(), "healthy checkpoint deletes the chain"); };
    // Pool band at p = 1.0: spot exactly 1.005 ok, the next representable spot fails; 0.995 ok, below fails.
    t.set_pool(79_425_985_949_584_624_505_096_644_251, 0); ok(&t);
    t.set_pool(79_425_985_949_584_624_505_096_644_252, 0); blocked(&t);
    t.set_pool(79_029_843_899_059_723_280_318_636_797, 0); ok(&t);
    t.set_pool(79_029_843_899_059_723_280_318_636_796, 0); blocked(&t);
    // Tick range [-20, 20).
    for (tick, good) in [(-20, true), (-21, false), (19, true), (20, false)] {
        t.set_pool(X96, tick);
        if good { ok(&t) } else { blocked(&t) }
    }
    // sqrt price above 2 (spot > 4): blocked without overflow; exactly 2 is evaluated (and out of band).
    t.set_pool(X96 * 3, 0); blocked(&t);
    t.set_pool(X96 * 2, 0); blocked(&t);
    // Pool unreachable: blocked (try_invoke), never a revert.
    t.set_pool(X96, 0); t.pool().trap(&true); blocked(&t); t.pool().trap(&false);
    // Depeg: pool at 1.01 so the band passes; p = 1.01 ok, +1 unit fails; pool at 0.99: 0.99 ok, -1 fails.
    t.set_pool(79_623_317_895_830_914_510_639_640_423, 0);
    t.set_p(101_000_000_000_000); ok(&t);
    t.set_p(101_000_000_000_001); blocked(&t);
    t.set_pool(78_831_026_366_734_652_303_669_917_531, 0);
    t.set_p(99_000_000_000_000); ok(&t);
    t.set_p(98_999_999_999_999); blocked(&t);
    // Venue valuation unavailable / zero / wrong shape: blocked.
    t.set_p(P1); t.set_pool(X96, 0); ok(&t);
    for f in ["quote_fail", "quote_zero"] { t.flag(f, 1); blocked(&t); t.flag(f, 0); ok(&t); }
    t.flag("quote_shape", 2); blocked(&t); t.flag("quote_shape", 0); ok(&t);
}

#[test]
fn blocked_checkpoint_never_overwrites_or_refreshes_the_last_good_nav() {
    let t = G::new();
    t.stake(&t.alice);
    let r = t.c().checkpoint_nav();
    t.adv(30);
    t.set_pool(X96, 20);
    assert_eq!(t.c().checkpoint_nav(), 0);
    assert_eq!(t.status().nav, Some(Stg2Ck { rate: r, ts: T0 }), "old checkpoint kept, not refreshed");
    t.adv(31);
    assert_eq!(t.c().get_exchange_rate(), 0, "it still expires 60 s after its own timestamp");
}

// ================================================================== D3 heartbeat chain + fallback (L1b)
#[test]
fn d3_chain_real_clock_599_refused_600_admitted_and_gap_rules() {
    let t = G::new();
    t.stake(&t.alice);
    t.set_pool(X96, 25); // out of range: a named cause
    assert_eq!(code(t.c().try_checkpoint_fallback()), 2111, "no chain");
    let start = t.now();
    assert_eq!(t.c().checkpoint_nav(), 0);
    assert_eq!(t.status().chain, Some(Stg2Blk { since: start, last: start, n: 1 }));
    for _ in 0..9 { t.adv(60); assert_eq!(t.c().checkpoint_nav(), 0); } // 60 s gaps keep the chain
    assert_eq!(t.status().chain, Some(Stg2Blk { since: start, last: start + 540, n: 10 }));
    t.adv(59);
    assert_eq!(code(t.c().try_checkpoint_fallback()), 2112, "599 s: block too recent");
    t.adv(1);
    let fb = t.c().checkpoint_fallback(); // 600 s, last observation exactly 60 s ago
    assert!(fb > 0);
    assert_eq!(t.status().fallback, Some(Stg2Ck { rate: fb, ts: start + 600 }));
    assert_eq!(t.status().chain.unwrap().last, start + 600, "admission extends the chain");
    t.adv(60); t.c().checkpoint_fallback();
    t.adv(61);
    assert_eq!(code(t.c().try_checkpoint_fallback()), 2113, "61 s lapse: chain broken");
    // A blocked observation after a 61 s gap RESTARTS the chain: the fallback needs a new 600 s.
    let restart = t.now();
    assert_eq!(t.c().checkpoint_nav(), 0);
    assert_eq!(t.status().chain, Some(Stg2Blk { since: restart, last: restart, n: 1 }));
    t.adv(60);
    assert_eq!(code(t.c().try_checkpoint_fallback()), 2112);
}

#[test]
fn d3_healthy_checkpoint_clears_and_two_pushes_ten_minutes_apart_are_refused() {
    let t = G::new();
    t.stake(&t.alice);
    t.set_pool(X96, 25);
    t.c().checkpoint_nav();
    t.adv(30);
    t.set_pool(X96, 0);
    assert!(t.c().checkpoint_nav() > 0, "healthy moment observed");
    assert_eq!(t.status().chain, None);
    t.adv(570);
    t.set_pool(X96, 25);
    t.c().checkpoint_nav();
    assert_eq!(code(t.c().try_checkpoint_fallback()), 2112, "a new chain needs its own 600 s");
    t.adv(61);
    assert_eq!(code(t.c().try_checkpoint_fallback()), 2113);
}

#[test]
fn d3_fallback_requires_a_cause_now_and_is_refused_while_nav_is_refreshable() {
    let t = G::new();
    t.stake(&t.alice);
    t.set_pool(X96, 25);
    t.chain_600();
    t.set_pool(X96, 0); // healthy again
    assert_eq!(code(t.c().try_checkpoint_fallback()), 2104, "NAV refreshable");
    assert!(t.status().chain.is_some(), "a refused fallback records nothing");
    t.set_p(98_000_000_000_000);
    assert!(t.c().checkpoint_fallback() > 0, "depeg cause");
    t.set_p(P1); t.flag("quote_fail", 1);
    assert!(t.c().checkpoint_fallback() > 0, "venue valuation failure cause");
    t.flag("quote_fail", 0);
    t.pool().trap(&true);
    assert!(t.c().checkpoint_fallback() > 0, "pool unreachable cause");
}

#[test]
fn fallback_value_is_the_lower_bound_formula_with_2_pct_haircut() {
    let t = G::new();
    t.stake(&t.alice);
    // Independent expectations (exact integer floor of DESIGN §2.1, computed outside Rust):
    // supply 1e9, L 5e11, booked idle 3e6 PYUSD + 2e6 USDC.
    t.flag("liquidity", 500_000_000_000);
    t.flag("idle0", 3_000_000);
    t.flag("idle1", 2_000_000);
    let supply = t.v().total_supply() as u128;
    t.flag("extra_supply", 1_000_000_000 - supply);
    t.set_pool(X96, 25);
    t.chain_600();
    // p = 1.0 > P_lo = 0.99800209846088: valued at P_lo x 0.98.
    assert_eq!(t.c().checkpoint_fallback(), 982_927);
    // p = 0.99 < P_lo: valued at p x 0.98.
    t.set_p(99_000_000_000_000);
    assert_eq!(t.c().checkpoint_fallback(), 975_062);
    // Pushing the pool further changes nothing (no pool quote in the value).
    t.set_p(P1); t.set_pool(X96 * 3, 19);
    assert_eq!(t.c().checkpoint_fallback(), 982_927);
    // Zero value refuses (#2110).
    t.flag("liquidity", 0); t.flag("idle0", 0); t.flag("idle1", 0);
    assert_eq!(code(t.c().try_checkpoint_fallback()), 2110);
}

#[test]
fn fallback_is_liquidation_only_valid_60_s_and_nav_takes_precedence() {
    let t = G::new();
    let n = t.stake(&t.alice);
    t.flag("liquidity", 1_000_000_000);
    t.set_pool(X96, 25);
    t.chain_600();
    let fb = t.c().checkpoint_fallback();
    assert_eq!(t.c().get_exchange_rate(), 0, "borrow/exit rate ignores the fallback");
    assert_eq!(t.c().get_account_snapshot(&t.alice).2, 0);
    assert_eq!(t.c().get_liquidation_account_snapshot(&t.alice), (n, 0, fb, t.usdc.clone()));
    assert_eq!(t.c().get_liquidation_snapshot(&t.alice), (n, fb, t.usdc.clone()));
    t.adv(60);
    assert_eq!(t.c().get_liquidation_snapshot(&t.alice).1, fb, "60 s valid");
    t.adv(1);
    assert_eq!(t.c().get_liquidation_snapshot(&t.alice).1, 0, "61 s invalid");
    t.set_pool(X96, 0);
    let r = t.c().checkpoint_nav();
    assert_ne!(r, fb);
    assert_eq!(t.c().get_liquidation_snapshot(&t.alice).1, r, "NAV precedes");
}

// ================================================================== borrow / exits / repay-only (D4)
#[test]
fn borrow_needs_a_fresh_nav_and_respects_cf() {
    let t = G::new();
    let n = t.stake(&t.alice);
    t.ctl().enter_market(&t.alice, &t.receipt);
    t.ctl().enter_market(&t.alice, &t.usdc_m);
    assert!(fails(t.m().try_borrow(&t.alice, &1_000_000)), "no checkpoint");
    let r = t.c().checkpoint_nav();
    let max = n * r / 1_000_000 * CF / 1_000_000;
    assert!(fails(t.m().try_borrow(&t.alice, &(max + 1_000))), "above CF");
    t.m().borrow(&t.alice, &(max / 2));
    t.adv(60);
    t.m().borrow(&t.alice, &1_000);
    t.adv(1);
    assert!(fails(t.m().try_borrow(&t.alice, &1_000)), "61 s old checkpoint");
    t.c().checkpoint_nav();
    t.m().borrow(&t.alice, &1_000);
    // A fallback alone never supports a borrow.
    t.set_pool(X96, 25);
    t.adv(61);
    t.chain_600();
    t.flag("liquidity", 1_000_000_000_000);
    t.c().checkpoint_fallback();
    assert!(fails(t.m().try_borrow(&t.alice, &1_000)), "fallback only");
}

#[test]
fn prepare_close_with_debt_needs_fresh_nav_and_remaining_cover() {
    let t = G::new();
    let n = t.borrower(200_000_000); // 20 USDC against ~100 USDC at CF 50 %
    t.adv(61);
    assert_eq!(code(t.c().try_stage_prepare_close(&t.alice, &(n / 10), &0, &t.deadline())), 2101, "NAV stale");
    let r = t.c().checkpoint_nav();
    // Cover after the close must stay >= debt: left x rate x CF >= 20 USDC  <=>  left >= 400,000,000-ish.
    let debt = t.m().get_user_borrow_balance(&t.alice);
    let min_left = debt * 1_000_000 / CF * 1_000_000 / r + 1;
    assert_eq!(code(t.c().try_stage_prepare_close(&t.alice, &(n - min_left + 2), &0, &t.deadline())), 2106, "would leave a shortfall");
    t.c().stage_prepare_close(&t.alice, &(n - min_left - n / 1_000), &0, &t.deadline());
    // Without debt: no checkpoint needed at all (stale oracle too).
    t.stake(&t.bob);
    t.stale_p();
    t.adv(120);
    t.c().stage_prepare_close(&t.bob, &1_000, &0, &t.deadline());
}

#[test]
fn d4_oracle_stale_makes_the_account_repay_only() {
    let t = G::new();
    let n = t.borrower(300_000_000);
    t.stale_p();
    t.adv(61);
    assert_eq!(code(t.c().try_checkpoint_nav()), 2105);
    assert_eq!(code(t.c().try_checkpoint_fallback()), 2105);
    assert!(fails(t.m().try_borrow(&t.alice, &1_000)), "borrow refused");
    assert_eq!(code(t.c().try_stage_prepare_close(&t.alice, &1_000, &0, &t.deadline())), 2101, "exit refused");
    assert_eq!(code(t.c().try_stage_emergency_exit(&t.alice, &1_000, &true)), 2116, "emergency exit refused while entered");
    assert!(fails(t.ctl().try_exit_market(&t.alice, &t.receipt)), "exit_market refused with debt");
    t.ctl().cache_price(&t.usdc);
    assert!(fails(t.ctl().try_liquidate(&t.alice, &t.usdc_m, &t.receipt, &1_000_000, &t.liq)), "liquidation indeterminate");
    t.repay_all(&t.alice);
    assert_eq!(t.m().get_user_borrow_balance(&t.alice), 0, "repay works");
    t.c().stage_prepare_close(&t.alice, &1_000, &0, &t.deadline());
    t.ctl().exit_market(&t.alice, &t.receipt);
    t.c().stage_emergency_exit(&t.alice, &(n - 1_000), &true);
}

// ================================================================== USER-REQUIRED emergency-exit tests
/// USER-REQUIRED. A debt-free entered owner exits the CL market in ONE transaction (one
/// `exit_market` invocation, only the owner's authorization, CF > 0, units held, no checkpoint, no
/// cached price, stale oracle), then the no-claim emergency exit and its permissionless payout
/// succeed. Compiled-bytes footprint of the same sequence: BUILD-REPORT §2 row RD9 (58 + 89 entries).
#[test]
fn user_required_debt_free_account_exits_market_in_one_tx_then_emergency_exits() {
    let t = G::new();
    let n = t.stake(&t.alice);
    t.ctl().enter_market(&t.alice, &t.receipt);
    assert_eq!(t.ctl().get_market_cf(&t.receipt), CF);
    t.stale_p();
    t.o().set(&t.usdc, &P1, &(t.now() - 10_000), &false);
    t.adv(3_600);
    assert_eq!(t.status().nav, None);
    assert_eq!(code(t.c().try_stage_emergency_exit(&t.alice, &n, &true)), 2116, "entered: refused");
    // ONE transaction, the owner's signature only (strict auth, not mock_all_auths).
    t.env.set_auths(&[]);
    t.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &t.alice,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &t.ctl, fn_name: "exit_market", args: (t.alice.clone(), t.receipt.clone()).into_val(&t.env), sub_invokes: &[],
        },
    }]);
    t.ctl().exit_market(&t.alice, &t.receipt);
    let auths = t.env.auths();
    assert_eq!(auths.len(), 1, "exactly one authorization");
    assert_eq!(auths[0].0, t.alice);
    t.auth_all();
    assert!(!t.ctl().get_user_markets(&t.alice).contains(&t.receipt));
    let (p0, u0) = (t.bal(&t.pyusd, &t.alice), t.bal(&t.usdc, &t.alice));
    let id = t.c().stage_emergency_exit(&t.alice, &n, &true);
    assert_eq!(t.c().stage_units(&t.alice), 0);
    t.env.set_auths(&[]);
    t.c().stage_emergency_pay(&id); // permissionless
    t.auth_all();
    let got = (t.bal(&t.pyusd, &t.alice) - p0) + (t.bal(&t.usdc, &t.alice) - u0);
    assert!(got + 2 >= UNITS, "paid the full mixture: {got}");
}

#[test]
fn user_required_entered_account_is_refused_emergency_exit_with_and_without_debt() {
    // Without debt (row RD8).
    let t = G::new();
    let n = t.stake(&t.alice);
    t.ctl().enter_market(&t.alice, &t.receipt);
    let before = (t.c().stage_units(&t.alice), t.c().stage_state(), t.v().bene(&t.alice));
    assert_eq!(code(t.c().try_stage_emergency_exit(&t.alice, &1, &true)), 2116);
    assert_eq!(code(t.c().try_stage_emergency_exit(&t.alice, &n, &false)), 2116);
    assert_eq!((t.c().stage_units(&t.alice), t.c().stage_state(), t.v().bene(&t.alice)), before, "state unchanged");
    // With debt (row RD5): refused, and exit_market is refused too (CF > 0, debt; row RD10).
    let u = G::new();
    let m = u.borrower(100_000_000);
    assert_eq!(code(u.c().try_stage_emergency_exit(&u.alice, &(m / 10), &true)), 2116);
    assert!(fails(u.ctl().try_exit_market(&u.alice, &u.receipt)));
    assert!(u.ctl().get_user_markets(&u.alice).contains(&u.receipt));
    assert_eq!(u.c().stage_units(&u.alice), m);
}

/// USER-REQUIRED (coordinator, 21 Nov). A NOT-entered holder's emergency exit does not depend on
/// any pause (StgV1 deposit latch, every controller pause), the oracle (stale AND trapping), the
/// pool (trapping), checkpoints, or the collateral factor (CF 0).
#[test]
fn user_required_emergency_exit_works_while_paused_oracle_stale_pool_broken_or_cf_zero() {
    let t = G::new();
    let n = t.stake(&t.bob);
    t.c().stage_pause_deposits(&t.guardian);
    for m in [&t.receipt, &t.usdc_m] {
        t.ctl().set_pause_redeem(m, &true); t.ctl().set_pause_deposit(m, &true);
        t.ctl().set_pause_borrow(m, &true); t.ctl().set_pause_liquidation(m, &true);
    }
    t.stale_p();
    t.o().trap(&true);
    t.pool().trap(&true);
    t.ctl().emergency_disable_collateral(&t.receipt, &true);
    t.adv(10_000);
    let id = t.c().stage_emergency_exit(&t.bob, &(n / 2), &true);
    t.c().stage_emergency_pay(&id);
    t.c().stage_emergency_exit(&t.bob, &(n - n / 2), &false);
    assert_eq!(t.c().stage_units(&t.bob), 0);
}

/// USER-REQUIRED (coordinator, 21 Nov). An ENTERED debt-free holder under pauses + broken oracle,
/// and again at CF 0: exit_market then emergency exit.
#[test]
fn user_required_entered_debt_free_holder_exits_while_paused_oracle_broken_and_cf_zero() {
    let t = G::new();
    let n = t.stake(&t.alice);
    t.ctl().enter_market(&t.alice, &t.receipt);
    t.c().stage_pause_deposits(&t.guardian);
    for m in [&t.receipt, &t.usdc_m] {
        t.ctl().set_pause_redeem(m, &true); t.ctl().set_pause_liquidation(m, &true); t.ctl().set_pause_borrow(m, &true);
    }
    t.stale_p();
    t.o().trap(&true);
    t.pool().trap(&true);
    t.adv(10_000);
    t.ctl().exit_market(&t.alice, &t.receipt);
    t.c().stage_emergency_exit(&t.alice, &(n / 2), &true);
    t.ctl().enter_market(&t.alice, &t.receipt);
    t.ctl().emergency_disable_collateral(&t.receipt, &true);
    t.ctl().exit_market(&t.alice, &t.receipt);
    t.c().stage_emergency_exit(&t.alice, &(n - n / 2), &true);
    assert_eq!(t.c().stage_units(&t.alice), 0);
}

/// F1 FIX (commit "Receipt StgV2: emergency exit fails open when the controller traps"). With the
/// group controller BROKEN (traps after a bad upgrade, or unreachable) a holder can still
/// emergency-exit: `stg2_emergency_check` fails OPEN only when the controller call errors. Before
/// the fix this reverted for every holder. Covers a never-entered holder and an ENTERED one (the
/// documented trade-off: once the controller is dead, liquidation is dead too).
#[test]
fn f1_emergency_exit_works_when_the_controller_traps() {
    let t = G::new();
    let n = t.stake(&t.bob);
    let a = t.borrower(100_000_000); // entered, with debt
    t.env.register_at(&t.ctl, TrapController, ());
    let id = t.c().stage_emergency_exit(&t.bob, &n, &true);
    t.c().stage_emergency_pay(&id);
    assert_eq!(t.c().stage_units(&t.bob), 0);
    t.c().stage_emergency_exit(&t.alice, &a, &true);
    assert_eq!(t.c().stage_units(&t.alice), 0, "trade-off: entered borrower exits once the controller is dead");
}

/// F1: a controller that ANSWERS (even fully paused) is never bypassed: entered => #2116.
#[test]
fn f1_paused_but_answering_controller_still_refuses_an_entered_owner() {
    let t = G::new();
    let n = t.stake(&t.alice);
    t.stake(&t.bob);
    t.ctl().enter_market(&t.alice, &t.receipt);
    for m in [&t.receipt, &t.usdc_m] {
        t.ctl().set_pause_redeem(m, &true); t.ctl().set_pause_deposit(m, &true);
        t.ctl().set_pause_borrow(m, &true); t.ctl().set_pause_liquidation(m, &true);
    }
    assert_eq!(code(t.c().try_stage_emergency_exit(&t.alice, &n, &true)), 2116);
    t.c().stage_emergency_exit(&t.bob, &1_000, &true);
}

/// F2 FIX (commit "Group controller G10: sleeve exit guarded regardless of CF"). After
/// `emergency_disable_collateral` (CF 0) an entered borrower WITH debt is refused `exit_market`
/// (before the fix it passed and could then emergency-exit every unit, leaving the debt unbacked);
/// a debt-free holder at CF 0 still exits and emergency-exits.
#[test]
fn f2_cf_zero_does_not_let_an_indebted_borrower_leave_the_sleeve() {
    let t = G::new();
    let n = t.borrower(300_000_000);
    t.stake(&t.bob);
    t.ctl().enter_market(&t.bob, &t.receipt);
    t.ctl().emergency_disable_collateral(&t.receipt, &true);
    assert_eq!(t.ctl().get_market_cf(&t.receipt), 0);
    t.c().checkpoint_nav();
    assert_eq!(code(t.c().try_stage_prepare_close(&t.alice, &n, &0, &t.deadline())), 2106);
    assert!(fails(t.ctl().try_exit_market(&t.alice, &t.receipt)), "indebted borrower refused at CF 0");
    assert_eq!(code(t.c().try_stage_emergency_exit(&t.alice, &n, &true)), 2116);
    assert_eq!(t.c().stage_units(&t.alice), n);
    // Debt-free holder at CF 0: exits.
    t.ctl().exit_market(&t.bob, &t.receipt);
    t.c().stage_emergency_exit(&t.bob, &1_000, &true);
    // After full repayment the borrower exits too.
    t.repay_all(&t.alice);
    t.ctl().exit_market(&t.alice, &t.receipt);
    t.c().stage_emergency_exit(&t.alice, &n, &true);
}

// ================================================================== seizure (L3), queue, sync (L5), fee (D5)
/// alice borrows 45 USDC against ~100, then the venue loses 20 %: shortfall on a fresh NAV.
fn shortfall(t: &G) -> u128 {
    let n = t.borrower(450_000_000);
    t.v().mock_lose(&2_000);
    t.c().checkpoint_nav();
    t.ctl().cache_price(&t.usdc);
    assert!(t.ctl().account_liquidity(&t.alice).1 > 0, "in shortfall");
    n
}

#[test]
fn l3_seize_moves_the_mirror_queues_one_pending_and_l5_syncs_once_fee_off() {
    let t = G::new();
    let n = shortfall(&t);
    let lu0 = t.bal(&t.usdc, &t.liq);
    t.ctl().liquidate(&t.alice, &t.usdc_m, &t.receipt, &100_000_000, &t.liq);
    assert_eq!(lu0 - t.bal(&t.usdc, &t.liq), 100_000_000);
    let p = t.pend(0).expect("pending 0");
    assert_eq!((p.from.clone(), p.to.clone()), (t.alice.clone(), t.liq.clone()));
    // seize = repay x 1.08 / rate (USDC numeraire on both sides).
    let rate = t.status().nav.unwrap().rate;
    let expect = 100_000_000u128 * 1_080_000 / 1_000_000 * 1_000_000 / rate;
    assert!(p.n.abs_diff(expect) <= 1, "seized {} expected {}", p.n, expect);
    assert_eq!(t.status().next_pending, 1, "D5 fee off: exactly one queue entry");
    assert_eq!(t.pend(1), None);
    assert_eq!(t.c().stage_units(&t.alice), n - p.n);
    assert_eq!(t.c().stage_units(&t.liq), p.n);
    assert_eq!(t.c().stage_units(&t.reserve), 0);
    assert_eq!(t.c().stage_state().total_units, n, "units move, none are created");
    // Vault beneficiary NOT moved before L5; the liquidator cannot exit seized units yet.
    assert_eq!((t.v().bene(&t.alice), t.v().bene(&t.liq)), (n, 0));
    assert!(fails(t.c().try_stage_prepare_close(&t.liq, &p.n, &0, &t.deadline())));
    assert!(fails(t.c().try_stage_emergency_exit(&t.liq, &p.n, &true)));
    // The borrower cannot move the seized units either (the mirror gates every exit).
    t.repay_all(&t.alice);
    t.ctl().exit_market(&t.alice, &t.receipt);
    assert!(fails(t.c().try_stage_emergency_exit(&t.alice, &(n - p.n + 1), &true)));
    // L5: permissionless, exactly once.
    t.env.set_auths(&[]);
    assert!(!t.c().sync_seized(&0));
    t.auth_all();
    assert_eq!((t.v().bene(&t.alice), t.v().bene(&t.liq)), (n - p.n, p.n));
    assert_eq!(t.pend(0), None);
    assert_eq!(code(t.c().try_sync_seized(&0)), 2109, "replay");
    assert_eq!(code(t.c().try_sync_seized(&7)), 2109, "unknown id");
    t.c().stage_prepare_close(&t.liq, &p.n, &0, &t.deadline());
    t.c().stage_emergency_exit(&t.alice, &(n - p.n), &true);
}

#[test]
fn l5_forwards_the_vault_boundary_flag_and_out_of_order_syncs_work() {
    let t = G::new();
    shortfall(&t);
    t.ctl().liquidate(&t.alice, &t.usdc_m, &t.receipt, &50_000_000, &t.liq);
    t.ctl().liquidate(&t.alice, &t.usdc_m, &t.receipt, &10_000_000, &t.bob);
    assert_eq!(t.status().next_pending, 2);
    t.flag("opened", 1);
    assert!(t.c().sync_seized(&1), "boundary flag forwarded");
    t.flag("opened", 0);
    assert!(!t.c().sync_seized(&0));
    assert_eq!(t.v().reassigns(), 2);
}

#[test]
fn l3_fee_on_queues_a_second_entry_for_the_fee_recipient() {
    let t = G::new();
    let n = shortfall(&t);
    t.ctl().set_reserve_recipient(&t.reserve);
    t.ctl().set_liquidation_fee(&100_000);
    t.ctl().liquidate(&t.alice, &t.usdc_m, &t.receipt, &100_000_000, &t.liq);
    assert_eq!(t.status().next_pending, 2);
    let (a, b) = (t.pend(0).unwrap(), t.pend(1).unwrap());
    assert_eq!((a.to.clone(), b.to.clone()), (t.liq.clone(), t.reserve.clone()));
    assert_eq!((a.from.clone(), b.from.clone()), (t.alice.clone(), t.alice.clone()));
    assert!(b.n > 0 && a.n > b.n);
    assert_eq!(t.c().stage_units(&t.alice), n - a.n - b.n);
    assert_eq!(t.c().stage_units(&t.reserve), b.n);
    t.c().sync_seized(&1);
    t.c().sync_seized(&0);
    assert_eq!(t.v().bene(&t.reserve), b.n);
}

#[test]
fn l3_is_cache_only_and_needs_a_fresh_checkpoint_or_fallback() {
    let t = G::new();
    shortfall(&t);
    t.adv(601); // the cached USDC price is now older than 2 x 300 s
    t.c().checkpoint_nav();
    assert!(fails(t.ctl().try_liquidate(&t.alice, &t.usdc_m, &t.receipt, &10_000_000, &t.liq)), "cache-only: cold price refused");
    t.ctl().cache_price(&t.usdc);
    t.ctl().liquidate(&t.alice, &t.usdc_m, &t.receipt, &10_000_000, &t.liq);
    t.adv(61); // NAV 61 s old, no fallback: indeterminate
    t.ctl().cache_price(&t.usdc);
    assert!(fails(t.ctl().try_liquidate(&t.alice, &t.usdc_m, &t.receipt, &10_000_000, &t.liq)));
    // Control: without cache-only the cold price is fetched live.
    let u = G::build(false);
    shortfall(&u);
    u.adv(601);
    u.c().checkpoint_nav();
    u.ctl().liquidate(&u.alice, &u.usdc_m, &u.receipt, &10_000_000, &u.liq);
}

#[test]
fn l3_on_the_fallback_after_a_continuous_600_s_block() {
    let t = G::new();
    let n = shortfall(&t);
    t.flag("liquidity", 0);
    t.flag("idle0", t.v().backing() * 4 / 10); // L = 0: fallback value = booked idle only
    t.set_pool(X96, 25);
    t.chain_600();
    t.ctl().cache_price(&t.usdc);
    let fb = t.c().checkpoint_fallback();
    t.ctl().liquidate(&t.alice, &t.usdc_m, &t.receipt, &10_000_000, &t.liq);
    let p = t.pend(0).unwrap();
    let expect = 10_000_000u128 * 1_080_000 / 1_000_000 * 1_000_000 / fb;
    assert!(p.n.abs_diff(expect) <= 1);
    assert_eq!(t.c().stage_units(&t.alice), n - p.n);
}

#[test]
fn seize_entrypoint_rejects_every_malformed_context_and_needs_the_controller() {
    let t = G::new();
    let n = t.stake(&t.alice);
    let ctx = |seize: u128, fee: u128, rec: Option<Address>, short: u128, exp: u64| crate::SeizeContext {
        liquidity: 0, shortfall: short, max_redeem_ptokens: 0, seize_ptokens: seize, fee_recipient: rec, fee_ptokens: fee, expires_at: exp,
    };
    let now = t.now();
    let bad = [
        (10u128, None),
        (10, Some(ctx(11, 0, None, 1, now))),
        (10, Some(ctx(9, 0, None, 1, now))),
        (10, Some(ctx(10, 11, Some(t.reserve.clone()), 1, now))),
        (10, Some(ctx(10, 0, None, 0, now))),
        (10, Some(ctx(10, 0, None, 1, now - 1))),
        (10, Some(ctx(10, 1, None, 1, now))),
        (0, Some(ctx(0, 0, None, 1, now))),
        (n + 1, Some(ctx(n + 1, 0, None, 1, now))),
    ];
    for (k, c) in bad.iter() {
        assert_eq!(code(t.c().try_seize(&t.alice, &t.liq, k, c)), 2115, "ctx {k}");
    }
    assert_eq!(code(t.c().try_seize(&t.alice, &t.alice, &10, &Some(ctx(10, 0, None, 1, now)))), 2115, "self seizure");
    // Exactly at expiry is accepted; fee == n is accepted (all to the recipient); all units (have == n).
    t.c().seize(&t.alice, &t.liq, &10, &Some(ctx(10, 0, None, 1, now)));
    t.c().seize(&t.alice, &t.liq, &10, &Some(ctx(10, 10, Some(t.reserve.clone()), 1, now)));
    assert_eq!(t.status().next_pending, 2, "zero-unit legs are not queued");
    assert_eq!((t.c().stage_units(&t.liq), t.c().stage_units(&t.reserve)), (10, 10));
    let left = t.c().stage_units(&t.alice);
    t.c().seize(&t.alice, &t.bob, &left, &Some(ctx(left, 0, None, 1, now)));
    assert_eq!(t.c().stage_units(&t.alice), 0);
    // Without the controller's authorization.
    t.env.set_auths(&[]);
    assert!(fails(t.c().try_seize(&t.bob, &t.liq, &10, &Some(ctx(10, 0, None, 1, now)))));
}

#[test]
fn controller_binding_is_frozen_and_activation_is_one_time() {
    let t = G::new();
    assert!(fails(t.c().try_set_peridottroller(&t.ctl)), "binding fixed once StgV2 is active");
    let cfg = t.c().stage2_config();
    assert_eq!(cfg.controller, t.ctl);
    assert!(fails(t.c().try_stage2_activate(&t.admin, &cfg)), "one-time");
    assert_eq!(t.status().next_pending, 0);
}

#[test]
fn stage2_activate_guards() {
    // A fresh staged receipt, bound to the controller, not yet StgV2.
    let t = G::new();
    let r = t.env.register(ReceiptVault, ());
    let c = ReceiptVaultClient::new(&t.env, &r);
    c.initialize(&t.pyusd, &0, &0, &t.admin);
    let cfg = |ctl: &Address, pool: &Address, usdc: &Address| Stg2Config { controller: ctl.clone(), pool: pool.clone(), oracle: t.oracle.clone(), usdc: usdc.clone() };
    assert!(fails(c.try_stage2_activate(&t.admin, &cfg(&t.ctl, &t.pool, &t.usdc))), "staged mode inactive");
    let stranger = Address::generate(&t.env);
    assert!(fails(c.try_stage2_activate(&stranger, &cfg(&t.ctl, &t.pool, &t.usdc))), "not admin");
}
