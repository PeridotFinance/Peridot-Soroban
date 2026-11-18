#![no_std]
//! MOCK Reflector feed (CALI-like: 14 decimals, USDC base). Test-only setter; no auth.
use cl_lev_interface::{Asset, PriceData};
use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn mock_set(e: Env, asset: Asset, price: i128, timestamp: u64) {
        e.storage().instance().set(&asset, &PriceData { price, timestamp });
    }
    pub fn lastprice(e: Env, asset: Asset) -> Option<PriceData> {
        e.storage().instance().get(&asset)
    }
    pub fn decimals(_e: Env) -> u32 {
        14
    }
}
