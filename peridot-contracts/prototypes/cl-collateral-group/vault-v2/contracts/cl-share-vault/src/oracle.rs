#![allow(dead_code)]
//! Reflector price feed client.
//!
//! Mirrors `simple-peridottroller::reflector` so both contracts agree on the
//! asset encoding and staleness rules.

use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

#[soroban_sdk::contractclient(name = "ReflectorClient")]
pub trait Contract {
    fn base(e: Env) -> Asset;
    fn assets(e: Env) -> Vec<Asset>;
    fn decimals(e: Env) -> u32;
    fn price(e: Env, asset: Asset, timestamp: u64) -> Option<PriceData>;
    fn lastprice(e: Env, asset: Asset) -> Option<PriceData>;
    fn resolution(e: Env) -> u32;
    fn last_timestamp(e: Env) -> u64;
    fn version(e: Env) -> u32;
}

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}
