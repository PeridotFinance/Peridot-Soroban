//! Local compiled oracle/plane fixture; no production oracle assurance.
#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, Env, Symbol, Vec};
#[contracttype]
#[derive(Clone)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}
#[contracttype]
#[derive(Clone)]
pub struct Price {
    pub price: i128,
    pub timestamp: u64,
}
#[contract]
pub struct IntegrationFixture;
#[contractimpl]
impl IntegrationFixture {
    pub fn __constructor(e: Env) {
        let public = e.crypto().sha256(&Bytes::from_slice(
            &e,
            b"Public Global Stellar Network ; September 2015",
        ));
        assert_ne!(
            e.ledger().network_id(),
            public.to_bytes(),
            "local fixture: Mainnet disabled"
        );
    }
    pub fn lastprice(e: Env, asset: Asset) -> Option<Price> {
        Some(Price {
            price: e
                .storage()
                .instance()
                .get(&asset)
                .unwrap_or(100_000_000_000_000),
            timestamp: e.ledger().timestamp(),
        })
    }
    pub fn set_test_price(e: Env, admin: Address, asset: Asset, price: i128) {
        admin.require_auth();
        e.storage().instance().set(&asset, &price);
    }
    pub fn decimals(_e: Env) -> u32 {
        14
    }
    pub fn resolution(_e: Env) -> u32 {
        300
    }
    pub fn base(e: Env) -> Asset {
        Asset::Other(Symbol::new(&e, "USD"))
    }
    pub fn total_supply(_e: Env) -> u128 {
        0
    }
    pub fn update(_e: Env, _pool: Address, _kind: Symbol, _args: Vec<u128>, _reserves: Vec<u128>) {}
}
