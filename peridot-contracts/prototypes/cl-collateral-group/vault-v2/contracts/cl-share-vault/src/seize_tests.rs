//! StgV2 settle-only seizure reassign (CL collateral group, vault v2). NATIVE unit tests: the real
//! `ClShareVault::stage_seize_reassign` entrypoint and the real rewards/custody modules, with a
//! controlled reward pool and a staged-receipt address. Not a compiled-bytes test.
use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env,
};

// Same contracttype encodings as the private keys they mirror (enum name is not encoded).
#[contracttype]
#[derive(Clone)]
enum StageVaultKey { StgV1Receipt }
#[contracttype]
#[derive(Clone)]
enum RKey { Index, Count, Open }

#[contracttype]
#[derive(Clone)]
enum PK { Token, Pending, Claims }
#[contract]
struct Pool;
#[contractimpl]
impl Pool {
    pub fn initialize(e: Env, t: Address) { e.storage().instance().set(&PK::Token, &t); }
    pub fn accrue(e: Env, n: u128) {
        let p: u128 = e.storage().instance().get(&PK::Pending).unwrap_or(0);
        e.storage().instance().set(&PK::Pending, &(p + n));
    }
    pub fn get_user_reward(e: Env, _user: Address) -> u128 { e.storage().instance().get(&PK::Pending).unwrap_or(0) }
    pub fn claims(e: Env) -> u32 { e.storage().instance().get(&PK::Claims).unwrap_or(0) }
    pub fn claim(e: Env, user: Address) -> u128 {
        let n: u128 = e.storage().instance().get(&PK::Pending).unwrap_or(0);
        e.storage().instance().set(&PK::Pending, &0u128);
        let c: u32 = e.storage().instance().get(&PK::Claims).unwrap_or(0);
        e.storage().instance().set(&PK::Claims, &(c + 1));
        let t: Address = e.storage().instance().get(&PK::Token).unwrap();
        if n > 0 { token::Client::new(&e, &t).transfer(&e.current_contract_address(), &user, &(n as i128)); }
        n
    }
}
#[contract]
struct Receipt;
#[contractimpl]
impl Receipt {}

struct F { e: Env, v: Address, p: Address, t: Address, r: Address, alice: Address, bob: Address, liq: Address }
impl F {
    fn new() -> Self {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set_timestamp(10_000);
        let admin = Address::generate(&e);
        let t = e.register_stellar_asset_contract_v2(admin.clone()).address();
        let p = e.register(Pool, ());
        let r = e.register(Receipt, ());
        let v = e.register(ClShareVault, ());
        token::StellarAssetClient::new(&e, &t).mint(&p, &1_000_000);
        PoolClient::new(&e, &p).initialize(&t);
        e.as_contract(&v, || {
            storage::set_config(&e, &storage::Config {
                pool: p.clone(), token0: t.clone(), token1: t.clone(), dec0: 7, dec1: 7,
                underlying_index: 0, tick_lower: -20, tick_upper: 20, oracle: t.clone(),
            });
            storage::set_primary_reward(&e, &Some(t.clone()));
            e.storage().instance().set(&storage::DataKey::Admin, &admin);
            custody::stage_register(&e, &r);
            e.storage().instance().set(&StageVaultKey::StgV1Receipt, &r);
        });
        let (alice, bob, liq) = (Address::generate(&e), Address::generate(&e), Address::generate(&e));
        Self { e, v, p, t, r, alice, bob, liq }
    }
    fn c(&self) -> ClShareVaultClient<'_> { ClShareVaultClient::new(&self.e, &self.v) }
    fn pool(&self) -> PoolClient<'_> { PoolClient::new(&self.e, &self.p) }
    /// Staged custody mint as stage_cash_mint books it (claim-first, then label the beneficiary).
    fn mint(&self, owner: &Address, n: u128) {
        self.e.as_contract(&self.v, || {
            rewards::normal(&self.e, owner);
            storage::set_shares(&self.e, &self.r, storage::shares_of(&self.e, &self.r) + n);
            custody::stage_credit(&self.e, owner, n);
            let mut s = storage::state(&self.e); s.total_shares += n; storage::set_state(&self.e, &s);
            rewards::reset_debt(&self.e, owner);
        });
    }
    fn claim(&self, n: u128) { self.pool().accrue(&n); assert!(self.c().checkpoint_rewards()); }
    fn pending(&self, a: &Address) -> u128 { self.c().reward_account(a).pending }
    fn index(&self) -> u128 { self.e.as_contract(&self.v, || self.e.storage().instance().get(&RKey::Index).unwrap_or(0)) }
    fn count(&self) -> u64 { self.e.as_contract(&self.v, || self.e.storage().instance().get(&RKey::Count).unwrap_or(0)) }
    fn open(&self) -> bool { self.e.as_contract(&self.v, || self.e.storage().instance().get(&RKey::Open).unwrap_or(false)) }
    fn bal(&self) -> i128 { token::Client::new(&self.e, &self.t).balance(&self.v) }
    fn advance(&self, dt: u64) { let t = self.e.ledger().timestamp(); self.e.ledger().set_timestamp(t + dt); }
    fn seize(&self, from: &Address, to: &Address, n: u128) -> bool { self.c().stage_seize_reassign(&self.r, from, to, &n) }
}

#[test]
fn seize_makes_no_claim_and_borrower_keeps_accrued() {
    let f = F::new();
    f.mint(&f.alice, 1000);
    f.mint(&f.bob, 1000);
    f.claim(100);
    assert_eq!(f.c().stage_last_claim(), Some(10_000));
    // Unclaimed venue accrual exists; the seizure must not collect it.
    f.pool().accrue(&20);
    let (claims, bal, received, index) = (f.pool().claims(), f.bal(), f.c().reward_totals().received, f.index());
    assert!(!f.seize(&f.alice, &f.liq, 400), "fresh claim (0 s): no boundary");
    assert_eq!(f.pool().claims(), claims, "no venue claim call");
    assert_eq!(f.pool().get_user_reward(&f.v), 20, "venue accrual untouched");
    assert_eq!(f.bal(), bal);
    assert_eq!(f.c().reward_totals().received, received);
    assert_eq!(f.index(), index, "index unchanged");
    assert_eq!(f.c().stage_beneficial(&f.alice), 600);
    assert_eq!(f.c().stage_beneficial(&f.liq), 400);
    assert_eq!((f.pending(&f.alice), f.pending(&f.liq)), (50, 0), "borrower keeps everything indexed before the move");
    // Control: the claim-first path does collect and index the same accrual.
    assert!(f.c().checkpoint_rewards());
    assert_eq!(f.pool().claims(), claims + 1);
    assert_eq!(f.c().reward_totals().received, received + 20);
    assert!(f.index() > index);
}

#[test]
fn design_table_3_3_on_real_ledger() {
    let f = F::new();
    f.mint(&f.alice, 1000);
    f.mint(&f.bob, 1000);
    f.claim(100);
    assert_eq!((f.pending(&f.alice), f.pending(&f.bob)), (50, 50));
    // L3 (receipt side only): nothing changes in the vault. L4: claim at the OLD balances.
    f.advance(5);
    f.claim(20);
    assert_eq!((f.pending(&f.alice), f.pending(&f.bob), f.pending(&f.liq)), (60, 60, 0));
    // L5 within 60 s of the claim: settle-only move, no boundary.
    f.advance(30);
    assert!(!f.seize(&f.alice, &f.liq, 400));
    assert_eq!((f.pending(&f.alice), f.pending(&f.liq)), (60, 0));
    f.claim(30); // Alice 600, Bob 1000, liquidator 400 of 2000
    assert_eq!((f.pending(&f.alice), f.pending(&f.bob), f.pending(&f.liq)), (69, 75, 6));
    assert_eq!(f.c().reward_totals().received, 150);
    // Stale: L5 61 s after the last claim opens a boundary; the next claim is quarantined.
    f.advance(61);
    assert!(f.seize(&f.alice, &f.liq, 100));
    let b = f.c().reward_boundary(&0).unwrap();
    assert_eq!((b.holder.clone(), b.shares, b.reading, b.waived, b.proven), (f.alice.clone(), 600, None, false, false), "pre-move shares of the seized-from holder");
    f.claim(40);
    assert_eq!(f.c().reward_totals().quarantine, 40);
    assert_eq!((f.pending(&f.alice), f.pending(&f.bob), f.pending(&f.liq)), (69, 75, 6), "quarantined interval paid to no one automatically");
    // Release only by governed, timelocked attribution: credited to the boundary holder (borrower).
    let ev = BytesN::from_array(&f.e, &[7; 32]);
    f.c().propose_reward_attribution(&0, &40, &ev);
    f.advance(86_399);
    assert!(f.c().try_attribute_rewards(&0, &40, &ev).is_err(), "timelock");
    f.advance(1);
    f.c().attribute_rewards(&0, &40, &ev);
    assert_eq!((f.pending(&f.alice), f.pending(&f.bob), f.pending(&f.liq)), (109, 75, 6));
    assert_eq!(f.c().reward_totals().quarantine, 0);
    // Conservation: received 190 = paid-out-able credits 109 + 75 + 6.
    assert_eq!(f.c().reward_totals().received, 190);
    assert_eq!(f.c().claim_rewards(&f.liq), 6);
    std::println!("SEIZE_TABLE: 50/50; L4 60/60/0; L5 fresh 60/0; claim30 69/75/6; stale L5 boundary(600) -> claim40 quarantined; attribution after 86400 s -> Alice 109");
}

#[test]
fn boundary_edge_60_vs_61_seconds() {
    let f = F::new();
    f.mint(&f.alice, 1000);
    f.mint(&f.bob, 1000);
    f.claim(100);
    f.advance(60);
    assert!(!f.seize(&f.alice, &f.liq, 10), "exactly 60 s: no boundary");
    assert_eq!((f.count(), f.open()), (0, false));
    f.advance(1);
    assert!(f.seize(&f.alice, &f.liq, 10), "61 s: boundary");
    assert_eq!((f.count(), f.open()), (1, true));
    // A successful claim closes the interval and resets the clock.
    f.claim(10);
    assert!(!f.open());
    assert!(!f.seize(&f.alice, &f.liq, 10));
    // No claim ever seen by v2 (upgraded instance): stale.
    let g = F::new();
    g.e.as_contract(&g.v, || {
        storage::set_shares(&g.e, &g.r, 1000);
        custody::stage_credit(&g.e, &g.alice, 1000);
        let mut s = storage::state(&g.e); s.total_shares = 1000; storage::set_state(&g.e, &s);
    });
    assert_eq!(g.c().stage_last_claim(), None);
    assert!(g.seize(&g.alice, &g.liq, 1));
}

#[test]
fn seize_rejections() {
    let f = F::new();
    f.mint(&f.alice, 1000);
    f.claim(10);
    // Units locked by a close command cannot be seized.
    f.e.as_contract(&f.v, || stage_storage::lock(&f.e, &f.alice, 700));
    assert!(f.c().try_stage_seize_reassign(&f.r, &f.alice, &f.liq, &301).is_err());
    assert!(!f.seize(&f.alice, &f.liq, 300), "exactly the unlocked units; fresh claim, no boundary");
    assert_eq!((f.c().stage_beneficial(&f.alice), f.c().stage_beneficial(&f.liq)), (700, 300));
    // Non-receipt caller, zero amount, self-move, custodian as target.
    let other = Address::generate(&f.e);
    assert!(f.c().try_stage_seize_reassign(&other, &f.liq, &f.bob, &1).is_err());
    assert!(f.c().try_stage_seize_reassign(&f.r, &f.liq, &f.bob, &0).is_err());
    assert!(f.c().try_stage_seize_reassign(&f.r, &f.liq, &f.liq, &1).is_err());
    assert!(f.c().try_stage_seize_reassign(&f.r, &f.liq, &f.r, &1).is_err());
    // Receipt address without its authorization.
    f.e.set_auths(&[]);
    assert!(f.c().try_stage_seize_reassign(&f.r, &f.liq, &f.bob, &1).is_err());
}

#[test]
fn two_stale_seizures_share_one_bucket() {
    // Two boundaries in the same open interval (two stale L5 syncs before one claim): the
    // governed attribution can never release more than the interval's sealed bucket.
    let f = F::new();
    f.mint(&f.alice, 1000);
    f.mint(&f.bob, 1000);
    f.claim(100);
    f.advance(61);
    assert!(f.seize(&f.alice, &f.liq, 100));
    assert!(f.seize(&f.bob, &f.liq, 100));
    assert_eq!(f.count(), 2);
    let (b0, b1) = (f.c().reward_boundary(&0).unwrap(), f.c().reward_boundary(&1).unwrap());
    assert_eq!(b0.interval, b1.interval, "same interval");
    f.claim(40);
    assert_eq!(f.c().reward_totals().quarantine, 40);
    let ev = BytesN::from_array(&f.e, &[9; 32]);
    f.c().propose_reward_attribution(&0, &30, &ev);
    f.c().propose_reward_attribution(&1, &30, &ev);
    f.advance(86_400);
    f.c().attribute_rewards(&0, &30, &ev);
    assert!(f.c().try_attribute_rewards(&1, &30, &ev).is_err(), "bucket exhausted: 10 left");
    assert_eq!(f.c().reward_totals().quarantine, 10);
    // Pending sums never exceed received.
    let t = f.c().reward_totals();
    let credited = f.pending(&f.alice) + f.pending(&f.bob) + f.pending(&f.liq);
    assert!(credited + t.quarantine <= t.received, "conservation");
}

#[test]
fn zero_delta_claim_refreshes_last_claim() {
    // A successful claim that yields 0 still proves the index is current: it resets the D6 clock.
    let f = F::new();
    f.mint(&f.alice, 1000);
    f.claim(10);
    f.advance(500);
    assert!(f.c().checkpoint_rewards(), "zero-yield claim succeeds");
    assert_eq!(f.c().stage_last_claim(), Some(10_500));
    assert!(!f.seize(&f.alice, &f.liq, 1), "fresh after zero-delta claim");
}

#[test]
fn weight_history_follows_the_move() {
    // reset_debt records weight history for both holders (DESIGN L5 step 5).
    let f = F::new();
    f.mint(&f.alice, 1000);
    f.claim(10);
    let before = f.e.as_contract(&f.v, || crate::reward_history::count(&f.e, &f.liq));
    f.seize(&f.alice, &f.liq, 250);
    let after = f.e.as_contract(&f.v, || crate::reward_history::count(&f.e, &f.liq));
    assert_eq!(after, before + 1, "liquidator weight recorded");
}

// ---- BUILD-REPORT open issue 3/4: the D6 boundary quarantines the WHOLE next claim. Every release
// route driven to completion with two non-seized-side holders (Bob, and the liquidator), checking
// that nobody double-claims and recording who can be made whole (TESTS-STGV2.md §4).
fn stale_seizure_then_quarantined_claim() -> F {
    let f = F::new();
    f.mint(&f.alice, 1000);
    f.mint(&f.bob, 1000);
    f.claim(100); // 50 / 50
    f.advance(61);
    assert!(f.seize(&f.alice, &f.liq, 400), "stale L5 opens a boundary");
    f.claim(40); // whole claim quarantined
    assert_eq!(f.c().reward_totals().quarantine, 40);
    assert_eq!((f.pending(&f.alice), f.pending(&f.bob), f.pending(&f.liq)), (50, 50, 0));
    f
}
fn credited(f: &F) -> u128 { f.pending(&f.alice) + f.pending(&f.bob) + f.pending(&f.liq) }

#[test]
fn route_a_per_boundary_release_can_only_credit_the_seized_from_holder() {
    // Route A (rewards::attribute, the documented release): the bucket is credited ONLY to the
    // boundary holder. Bob held 50 % of the shares for the whole quarantined interval but no
    // route-A call can credit him, and once route A is used the segment route is closed.
    let f = stale_seizure_then_quarantined_claim();
    let ev = BytesN::from_array(&f.e, &[3; 32]);
    f.c().propose_reward_attribution(&0, &12, &ev); // e.g. Alice's own 600/2000 of 40
    f.advance(86_400);
    f.c().attribute_rewards(&0, &12, &ev);
    assert_eq!((f.pending(&f.alice), f.pending(&f.bob), f.pending(&f.liq)), (62, 50, 0));
    assert_eq!(f.c().reward_totals().quarantine, 28, "Bob's and the liquidator's 28 stay quarantined");
    // No second route-A release for the same boundary (no double claim) ...
    assert!(f.c().try_propose_reward_attribution(&0, &28, &ev).is_err(), "already attributed");
    // ... and the segment route is closed for this interval: Bob's share is unreachable.
    let seg = BytesN::from_array(&f.e, &[4; 32]);
    assert!(f.c().try_propose_reward_segment(&0, &40, &seg).is_err(), "direct-governed interval");
    let n = f.c().reward_history_count(&f.bob);
    assert!(f.c().try_settle_reward_segment(&f.bob, &0, &(n - 1)).is_err(), "Bob cannot settle");
    let t = f.c().reward_totals();
    assert!(credited(&f) + t.quarantine == t.received, "conservation: nothing created or lost from the ledger");
    assert_eq!(f.c().claim_rewards(&f.alice), 62);
    assert_eq!(f.c().claim_rewards(&f.alice), 0, "no double claim");
}

#[test]
fn route_b_segment_release_pays_every_holder_once_at_post_move_weights() {
    // Route B (reward_history segments): seize_gap records NO observation before the move, so the
    // interval is ONE segment sealed at the claim, at POST-move weights (Alice 600, Bob 1000,
    // liquidator 400). Governed segment attribution + per-holder settlement pays all 40, each once.
    let f = stale_seizure_then_quarantined_claim();
    let id = f.c().reward_boundary(&0).unwrap().interval;
    let iv = f.c().reward_interval(&id).expect("sealed interval");
    assert_eq!((iv.sealed, iv.segments, iv.received), (true, 1, 40), "one segment, no pre-move snapshot");
    assert_eq!(iv.first_segment, 0);
    let ev = BytesN::from_array(&f.e, &[5; 32]);
    f.c().propose_reward_segment(&0, &40, &ev);
    f.advance(86_399);
    assert!(f.c().try_attribute_reward_segment(&0, &40, &ev).is_err(), "timelock");
    f.advance(1);
    f.c().attribute_reward_segment(&0, &40, &ev);
    assert!(f.c().prepare_reward_segment(&0, &4));
    let last = |a: &Address| f.c().reward_history_count(a) - 1;
    let (a, b, l) = (
        f.c().settle_reward_segment(&f.alice, &0, &last(&f.alice)),
        f.c().settle_reward_segment(&f.bob, &0, &last(&f.bob)),
        f.c().settle_reward_segment(&f.liq, &0, &last(&f.liq)),
    );
    assert_eq!(a + b + l, 40 * 1_000_000_000_000_000_000, "whole bucket distributed (1e18-scaled)");
    assert_eq!((f.pending(&f.alice), f.pending(&f.bob), f.pending(&f.liq)), (62, 70, 8), "Bob made whole (20); Alice/liquidator split at post-move weights");
    assert_eq!(f.c().reward_totals().quarantine, 0);
    // Each holder settles once; route A is closed after segment settlement started.
    assert_eq!(f.c().settle_reward_segment(&f.bob, &0, &last(&f.bob)), 0, "no double settlement");
    let ra = BytesN::from_array(&f.e, &[6; 32]);
    f.c().propose_reward_attribution(&0, &1, &ra);
    f.advance(86_400);
    assert!(f.c().try_attribute_rewards(&0, &1, &ra).is_err(), "segment settlement already started");
    let t = f.c().reward_totals();
    assert_eq!(credited(&f) + t.quarantine, t.received);
    assert_eq!(f.c().claim_rewards(&f.bob), 70);
    assert_eq!(f.c().claim_rewards(&f.bob), 0);
}
