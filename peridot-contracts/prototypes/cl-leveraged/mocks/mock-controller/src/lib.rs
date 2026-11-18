#![no_std]
//! MOCK group controller: only the three getters `cl-leveraged` reads (LEVERAGE-SPEC §3, G3).
//! Values are test fixtures, never proposed parameters.
use soroban_sdk::{contract, contracttype, contractimpl, Address, Env};

#[contracttype]
enum K {
    Cf(Address),
    Li,
    Paused(Address),
}

#[contract]
pub struct MockController;

#[contractimpl]
impl MockController {
    pub fn mock_set_cf(e: Env, market: Address, cf: u128) {
        e.storage().persistent().set(&K::Cf(market), &cf);
    }
    pub fn mock_set_li(e: Env, li: u128) {
        e.storage().instance().set(&K::Li, &li);
    }
    pub fn mock_set_paused(e: Env, market: Address, paused: bool) {
        e.storage().persistent().set(&K::Paused(market), &paused);
    }
    pub fn get_market_cf(e: Env, market: Address) -> u128 {
        e.storage().persistent().get(&K::Cf(market)).unwrap_or(0)
    }
    pub fn get_liquidation_incentive_scaled(e: Env) -> u128 {
        e.storage().instance().get(&K::Li).unwrap_or(1_000_000)
    }
    pub fn is_borrow_paused(e: Env, market: Address) -> bool {
        e.storage().persistent().get(&K::Paused(market)).unwrap_or(false)
    }
}
