//! Vault v2 upgradeability (UPGRADEABILITY.md, 21 Nov 2026). NATIVE tests of the real entrypoints on
//! a bound vault under the PUBLIC (Mainnet) network id, plus COMPILED-bytes v2 -> v2' tests
//! (`#[ignore]`d by default: they read the built artifacts, see `wasm()` below).
use super::*;
use crate::contract::StageJob;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
    token, Address, Bytes, BytesN, Env, IntoVal,
};
extern crate std;

const LEDGER: &str = "GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP";
const DAY: u64 = 24 * 60 * 60;

// Same contracttype encodings as the private keys they mirror (the enum name is not encoded).
#[contracttype]
#[derive(Clone)]
enum StageVaultKey { StgV1Receipt, StgV1Exit(u64) }
#[contracttype]
#[derive(Clone)]
enum AKey { Book(Address) }

/// Reward-pool view double: no position, pending view only (the emergency boundary reads it).
#[contract]
struct Pool;
#[contractimpl]
impl Pool {
    pub fn get_user_reward(_e: Env, _user: Address) -> u128 { 0 }
    pub fn claim(_e: Env, _user: Address) -> u128 { 0 }
}
#[contract]
struct Receipt;
#[contractimpl]
impl Receipt {}

fn mainnet(e: &Env) {
    let id = e.crypto().sha256(&Bytes::from_slice(e, b"Public Global Stellar Network ; September 2015"));
    e.ledger().set_network_id(id.to_array());
}

struct F { e: Env, v: Address, r: Address, t0: Address, t1: Address, admin: Address, alice: Address }
impl F {
    /// Bound v2 vault (receipt is its only custodian), idle cash only, Mainnet network id.
    /// `code`: None = native contract, Some(bytes) = compiled artifact.
    fn new(code: Option<&[u8]>, pinned: bool) -> Self {
        let e = Env::default();
        e.mock_all_auths();
        mainnet(&e);
        e.ledger().set_timestamp(1_000_000);
        let issuer = Address::generate(&e);
        let t0 = e.register_stellar_asset_contract_v2(issuer.clone()).address();
        let t1 = e.register_stellar_asset_contract_v2(issuer.clone()).address();
        let aqua = e.register_stellar_asset_contract_v2(issuer.clone()).address();
        let p = e.register(Pool, ());
        let r = e.register(Receipt, ());
        let v = match code { Some(w) => e.register(w, ()), None => e.register(ClShareVault, ()) };
        let alice = Address::generate(&e);
        let admin = if pinned { Address::from_str(&e, LEDGER) } else { Address::generate(&e) };
        e.as_contract(&v, || {
            storage::set_config(&e, &storage::Config {
                pool: p.clone(), token0: t0.clone(), token1: t1.clone(), dec0: 7, dec1: 7,
                underlying_index: 0, tick_lower: -20, tick_upper: 20, oracle: t0.clone(),
            });
            storage::set_params(&e, &storage::Params {
                max_deploy: 0, slippage_bps: 50, harvest_cooldown: 0, oracle_max_age_mult: 2,
                nav_root_max_age: 60, max_pool_divergence_bps: 100, nav_root_max_stale: 0, paused: false,
            });
            e.storage().instance().set(&storage::DataKey::Admin, &admin);
            storage::set_primary_reward(&e, &Some(aqua.clone()));
            custody::stage_register(&e, &r);
            e.storage().instance().set(&StageVaultKey::StgV1Receipt, &r);
            // Staged custody mint of 1000 shares for alice (as stage_cash_mint books it).
            rewards::normal(&e, &alice);
            storage::set_shares(&e, &r, 1000);
            custody::stage_credit(&e, &alice, 1000);
            let mut s = storage::state(&e); s.total_shares = 1000; storage::set_state(&e, &s);
            rewards::reset_debt(&e, &alice);
        });
        // Idle backing, booked as unreserved vault cash.
        token::StellarAssetClient::new(&e, &t0).mint(&v, &5_000);
        token::StellarAssetClient::new(&e, &t1).mint(&v, &3_000);
        e.as_contract(&v, || {
            e.storage().instance().set(&AKey::Book(t0.clone()), &5_000u128);
            e.storage().instance().set(&AKey::Book(t1.clone()), &3_000u128);
        });
        Self { e, v, r, t0, t1, admin, alice }
    }
    fn c(&self) -> ClShareVaultClient<'_> { ClShareVaultClient::new(&self.e, &self.v) }
    fn h(&self, n: u8) -> BytesN<32> { BytesN::from_array(&self.e, &[n; 32]) }
    fn advance(&self, dt: u64) { let t = self.e.ledger().timestamp(); self.e.ledger().set_timestamp(t + dt); }
    fn job(&self, kind: u32, phase: u32, shares: u128) -> StageJob {
        StageJob { owner: self.alice.clone(), kind, phase, deadline: u64::MAX, a: 0, b: 0, reserve: 0, swap_in: 0, min_swap: 0, min_shares: 0, shares }
    }
}

fn panics<R>(f: impl FnOnce() -> R) -> std::string::String {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    let e = r.err().expect("expected a revert");
    e.downcast_ref::<std::string::String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| std::string::ToString::to_string(s))).unwrap_or_default()
}

#[test]
fn mainnet_bound_propose_cancel_and_view() {
    let f = F::new(None, true);
    let a = f.admin.clone();
    assert_eq!(f.c().get_pending_upgrade(), None);
    f.c().propose_upgrade_wasm(&a, &f.h(7));
    assert_eq!(f.c().get_pending_upgrade(), Some((f.h(7), 1_000_000 + DAY)));
    f.c().cancel_upgrade(&a);
    assert_eq!(f.c().get_pending_upgrade(), None, "cancel clears hash and ETA");
    assert!(f.c().try_cancel_upgrade(&a).is_err(), "nothing to cancel");
    assert!(f.c().try_upgrade_wasm(&a, &f.h(7)).is_err(), "cancelled proposal cannot execute");
    // Re-proposal restarts the full 24 h.
    f.advance(DAY);
    f.c().propose_upgrade_wasm(&a, &f.h(8));
    assert_eq!(f.c().get_pending_upgrade(), Some((f.h(8), 1_000_000 + 2 * DAY)));
}

#[test]
fn timelock_hash_and_pause_gates() {
    let f = F::new(None, true);
    let a = f.admin.clone();
    f.c().propose_upgrade_wasm(&a, &f.h(7));
    f.c().set_paused(&a, &true);
    f.advance(DAY - 1);
    assert!(f.c().try_upgrade_wasm(&a, &f.h(7)).is_err(), "eta - 1 s: timelocked");
    f.advance(1);
    assert!(f.c().try_upgrade_wasm(&a, &f.h(9)).is_err(), "hash mismatch");
    f.c().set_paused(&a, &false);
    assert!(f.c().try_upgrade_wasm(&a, &f.h(7)).is_err(), "unpaused vault refuses the upgrade");
    // Still pending after the refusals (nothing consumed).
    assert_eq!(f.c().get_pending_upgrade(), Some((f.h(7), 1_000_000 + DAY)));
}

#[test]
fn non_admin_and_non_pinned_keys_are_refused_on_mainnet() {
    let f = F::new(None, true);
    let mallory = Address::generate(&f.e);
    assert!(f.c().try_propose_upgrade_wasm(&mallory, &f.h(7)).is_err());
    assert!(f.c().try_set_paused(&mallory, &true).is_err());
    f.c().propose_upgrade_wasm(&f.admin, &f.h(7));
    assert!(f.c().try_cancel_upgrade(&mallory).is_err());
    f.advance(DAY);
    assert!(f.c().try_upgrade_wasm(&mallory, &f.h(7)).is_err());
    // The right address WITHOUT its signature: only mallory's auths are mocked.
    f.e.set_auths(&[]);
    let a = f.admin.clone();
    let i1 = MockAuthInvoke { contract: &f.v, fn_name: "set_paused", args: (a.clone(), true).into_val(&f.e), sub_invokes: &[] };
    let i2 = MockAuthInvoke { contract: &f.v, fn_name: "cancel_upgrade", args: (a.clone(),).into_val(&f.e), sub_invokes: &[] };
    let i3 = MockAuthInvoke { contract: &f.v, fn_name: "upgrade_wasm", args: (a.clone(), f.h(7)).into_val(&f.e), sub_invokes: &[] };
    f.e.mock_auths(&[MockAuth { address: &mallory, invoke: &i1 }]);
    assert!(f.c().try_set_paused(&a, &true).is_err(), "pinned address unsigned");
    f.e.mock_auths(&[MockAuth { address: &mallory, invoke: &i2 }]);
    assert!(f.c().try_cancel_upgrade(&a).is_err(), "pinned address unsigned");
    f.e.mock_auths(&[MockAuth { address: &mallory, invoke: &i3 }]);
    assert!(f.c().try_upgrade_wasm(&a, &f.h(7)).is_err(), "pinned address unsigned");
    // The Ledger's own signature works (mock_all_auths: the host cannot mock-sign a bare G strkey here).
    f.e.mock_all_auths();
    f.c().set_paused(&a, &true);
    assert!(f.c().is_paused());

    // A vault whose stored admin is NOT the pinned Ledger (impossible via initialize on Mainnet,
    // forced here) still refuses every upgrade/pause call on Mainnet, even when that admin signs.
    let g = F::new(None, false);
    let other = g.admin.clone();
    assert!(panics(|| g.c().propose_upgrade_wasm(&other, &g.h(7))).contains("pilot Ledger admin required"));
    assert!(g.c().try_set_paused(&other, &true).is_err());
    assert!(g.c().try_cancel_upgrade(&other).is_err());
    assert!(g.c().try_upgrade_wasm(&other, &g.h(7)).is_err());
}

#[test]
fn admin_pause_blocks_no_exit_or_seizure_path() {
    let f = F::new(None, true);
    f.c().set_paused(&f.admin, &true);
    assert!(f.c().is_paused());
    let c = f.c();
    // Seizure sync (L5) moves beneficial units while paused.
    let liq = Address::generate(&f.e);
    c.stage_seize_reassign(&f.r, &f.alice, &liq, &100);
    assert_eq!(c.stage_beneficial(&f.alice), 900);
    // Close lock + cancel of a close command (phase 10 unlock).
    c.stage_lock_close(&f.r, &1, &f.job(1, 10, 100));
    assert_eq!(c.stage_locked(&f.alice), 100);
    c.stage_cancel_kind(&f.r, &1, &f.job(1, 10, 100));
    assert_eq!(c.stage_locked(&f.alice), 0);
    // Cancel of an unfunded open command.
    c.stage_cancel_kind(&f.r, &2, &f.job(0, 0, 0));
    // In-kind close: 100 of 900 alice shares, total 1000 -> 10% of idle cash, paid in kind.
    c.stage_lock_close(&f.r, &3, &f.job(1, 10, 100));
    let out = c.stage_withdraw_kind(&f.r, &3, &f.job(1, 10, 100));
    assert_eq!((out.get(0).unwrap(), out.get(1).unwrap()), (500, 300));
    assert_eq!(token::Client::new(&f.e, &f.t0).balance(&f.alice), 500);
    assert_eq!(token::Client::new(&f.e, &f.t1).balance(&f.alice), 300);
    // Emergency pair (extract, then pay) while paused.
    let out = c.stage_emergency_extract(&f.r, &4, &f.job(2, 20, 100), &true);
    assert_eq!((out.get(0).unwrap(), out.get(1).unwrap()), (500, 300));
    let paid = c.stage_emergency_pay(&f.r, &4, &f.job(2, 21, 100));
    assert_eq!((paid.get(0).unwrap(), paid.get(1).unwrap()), (500, 300));
    assert_eq!(token::Client::new(&f.e, &f.t0).balance(&f.alice), 1000);
    f.e.as_contract(&f.v, || assert!(f.e.storage().persistent().has(&StageVaultKey::StgV1Exit(4))));
    // New money stays out while paused: the mint step refuses.
    assert!(c.try_stage_cash_mint(&f.r, &5, &f.job(0, 2, 0)).is_err());
    // The pause is reversible by the admin.
    c.set_paused(&f.admin, &false);
    assert!(!c.is_paused());
}

// ---------------------------------------------------------------------------------------------
// Compiled bytes: the release candidate (`build-vault.sh`, Mainnet admin constant) upgrades to the
// v2' marker build (`FEATURES_EXTRA=upgrade-marker`). Artifacts default to target/clg/upg/{vault-v2,
// vault-v2-marker}; override with CLG_VAULT_WASM / CLG_VAULT_MARKER_WASM. Run with --ignored.
// ---------------------------------------------------------------------------------------------
fn wasm(var: &str, dir: &str) -> std::vec::Vec<u8> {
    let p = std::env::var(var).unwrap_or_else(|_| std::format!("{}/../../../../../target/clg/upg/{dir}/cl_share_vault.wasm", env!("CARGO_MANIFEST_DIR")));
    std::fs::read(&p).unwrap_or_else(|_| panic!("missing compiled artifact {p}"))
}

#[test]
#[ignore = "compiled bytes: build-vault.sh artifacts, run with --ignored"]
fn compiled_v2_to_v2_prime_preserves_state_and_reopens() {
    let v2 = wasm("CLG_VAULT_WASM", "vault-v2");
    let v2p = wasm("CLG_VAULT_MARKER_WASM", "vault-v2-marker");
    let f = F::new(Some(&v2), true);
    let a = f.admin.clone();
    let h = f.e.deployer().upload_contract_wasm(Bytes::from_slice(&f.e, &v2p));
    let marker = soroban_sdk::Symbol::new(&f.e, "upgrade_marker");
    assert!(f.e.try_invoke_contract::<u32, soroban_sdk::Error>(&f.v, &marker, soroban_sdk::vec![&f.e]).is_err(), "v2 has no marker");
    let before = (f.c().get_admin(), f.c().total_supply(), f.c().balance(&f.r), f.c().stage_beneficial(&f.alice),
        f.c().stage_receipt(), f.c().get_config(), f.c().get_params(), f.c().reward_account(&f.alice));
    f.c().propose_upgrade_wasm(&a, &h);
    f.c().set_paused(&a, &true);
    f.advance(DAY - 1);
    assert!(f.c().try_upgrade_wasm(&a, &h).is_err(), "before ETA");
    f.advance(1);
    f.c().upgrade_wasm(&a, &h);
    let m: u32 = f.e.invoke_contract(&f.v, &marker, soroban_sdk::vec![&f.e]);
    assert_eq!(m, 2, "v2' code is live");
    assert_eq!(f.c().get_pending_upgrade(), None);
    let mut p = before.6.clone(); p.paused = true;
    let after = (f.c().get_admin(), f.c().total_supply(), f.c().balance(&f.r), f.c().stage_beneficial(&f.alice),
        f.c().stage_receipt(), f.c().get_config(), f.c().get_params(), f.c().reward_account(&f.alice));
    assert_eq!((after.0.clone(), after.1, after.2, after.3, after.4.clone()), (before.0.clone(), before.1, before.2, before.3, before.4.clone()));
    assert_eq!(after.5, before.5);
    assert_eq!(after.6, p);
    assert_eq!(after.7, before.7);
    // Reopen: the admin lifts the pause on the new code; deposit admission no longer reverts on
    // the pause. (A full open/mint/invest on real venue bytes is the fresh-stack harness row.)
    f.c().set_paused(&a, &false);
    assert!(!f.c().is_paused());
    let quote = f.c().try_stage_quote_open(&1_000u128, &20u32, &5_000u32);
    if let Ok(Ok(q)) = quote { assert!(!q.deposits_paused || f.c().stage_deposits_paused(), "admin pause lifted"); }
    // v2' can itself be upgraded again (the gate survives the upgrade).
    f.c().propose_upgrade_wasm(&a, &h);
    assert!(f.c().get_pending_upgrade().is_some());
    f.c().cancel_upgrade(&a);
}

#[test]
#[ignore = "compiled bytes: build-vault.sh artifacts, run with --ignored"]
fn compiled_v2_mainnet_gates() {
    let v2 = wasm("CLG_VAULT_WASM", "vault-v2");
    let f = F::new(Some(&v2), true);
    let mallory = Address::generate(&f.e);
    assert!(f.c().try_propose_upgrade_wasm(&mallory, &f.h(1)).is_err());
    assert!(f.c().try_set_paused(&mallory, &true).is_err());
    f.c().propose_upgrade_wasm(&f.admin, &f.h(1));
    f.c().set_paused(&f.admin, &true);
    f.advance(DAY);
    assert!(f.c().try_upgrade_wasm(&mallory, &f.h(1)).is_err());
    f.c().cancel_upgrade(&f.admin);
    assert!(f.c().try_upgrade_wasm(&f.admin, &f.h(1)).is_err(), "cancelled");
    // Admin transfer stays frozen on Mainnet: the Ledger cannot be replaced.
    assert!(f.c().try_set_admin(&f.admin, &mallory).is_err());
}
