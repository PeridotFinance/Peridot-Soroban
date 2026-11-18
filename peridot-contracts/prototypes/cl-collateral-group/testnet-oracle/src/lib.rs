//! TESTNET-ONLY Reflector-ABI price fixture (CL collateral group rehearsal, 18 Nov 2026).
//! `lastprice(asset)` returns the admin-set price stamped with the CURRENT ledger time (a live
//! feed), unless the admin froze the feed (`set_frozen(true)`: timestamp stays at the freeze time,
//! so consumers see it go stale) or removed the asset. 14 decimals, 300 s resolution. Unset assets
//! return 1.0. The constructor refuses the Mainnet network id. Not a price source for anything else.
#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone, PartialEq)]
pub enum Asset { Stellar(Address), Other(Symbol) }
#[contracttype]
#[derive(Clone)]
pub struct PriceData { pub price: i128, pub timestamp: u64 }
#[contracttype]
#[derive(Clone)]
enum K { Admin, Base, Frozen, P(Asset), Gone(Asset) }

#[contract]
pub struct TestOracle;

fn admin(e: &Env) -> Address { e.storage().instance().get(&K::Admin).unwrap() }

#[contractimpl]
impl TestOracle {
    pub fn __constructor(e: Env, admin: Address, base: Asset) {
        let public = e.crypto().sha256(&Bytes::from_slice(&e, b"Public Global Stellar Network ; September 2015"));
        assert!(e.ledger().network_id() != public.to_bytes(), "testnet fixture: Mainnet disabled");
        e.storage().instance().set(&K::Admin, &admin);
        e.storage().instance().set(&K::Base, &base);
    }
    pub fn set_price(e: Env, asset: Asset, price: i128) {
        admin(&e).require_auth();
        assert!(price > 0);
        e.storage().instance().remove(&K::Gone(asset.clone()));
        e.storage().instance().set(&K::P(asset), &price);
        e.storage().instance().extend_ttl(100_000, 500_000);
    }
    pub fn remove_price(e: Env, asset: Asset) { admin(&e).require_auth(); e.storage().instance().set(&K::Gone(asset), &true); }
    /// Freeze: lastprice keeps the freeze timestamp (stale after 2 x resolution). Unfreeze: live again.
    pub fn set_frozen(e: Env, frozen: bool) {
        admin(&e).require_auth();
        if frozen { e.storage().instance().set(&K::Frozen, &e.ledger().timestamp()); } else { e.storage().instance().remove(&K::Frozen); }
    }
    pub fn lastprice(e: Env, asset: Asset) -> Option<PriceData> {
        if e.storage().instance().has(&K::Gone(asset.clone())) { return None; }
        let price: i128 = e.storage().instance().get(&K::P(asset)).unwrap_or(100_000_000_000_000);
        let ts = e.storage().instance().get::<_, u64>(&K::Frozen).unwrap_or(e.ledger().timestamp() / 300 * 300);
        Some(PriceData { price, timestamp: ts })
    }
    pub fn price(e: Env, asset: Asset, _timestamp: u64) -> Option<PriceData> { Self::lastprice(e, asset) }
    pub fn decimals(_e: Env) -> u32 { 14 }
    pub fn resolution(_e: Env) -> u32 { 300 }
    pub fn base(e: Env) -> Asset { e.storage().instance().get(&K::Base).unwrap() }
    pub fn assets(e: Env) -> Vec<Asset> { Vec::new(&e) }
    pub fn last_timestamp(e: Env) -> u64 { e.storage().instance().get::<_, u64>(&K::Frozen).unwrap_or(e.ledger().timestamp() / 300 * 300) }
    pub fn version(_e: Env) -> u32 { 2 }
}
