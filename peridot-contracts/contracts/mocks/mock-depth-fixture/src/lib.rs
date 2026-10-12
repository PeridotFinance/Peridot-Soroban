#![no_std]
//! Testnet-only quote/upstream/decimal stub, NOT a token or executable AMM.
//! Separate instances supply the asset identity and controlled pair quotes.
use soroban_sdk::{contract, contractimpl, contracttype, vec, Address, Bytes, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone)]
enum Key {
    Admin,
    Tokens,
    Ratio,
}
#[contracttype]
#[derive(Clone)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}
#[contracttype]
#[derive(Clone)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

fn testnet(e: &Env) {
    let expected = e
        .crypto()
        .sha256(&Bytes::from_slice(e, b"Test SDF Network ; September 2015"));
    assert_eq!(
        e.ledger().network_id(),
        expected.to_bytes(),
        "Testnet fixture only"
    );
}
#[contract]
pub struct DepthFixture;
#[contractimpl]
impl DepthFixture {
    pub fn __constructor(e: Env, admin: Address, token0: Address, token1: Address) {
        testnet(&e);
        admin.require_auth();
        e.storage().instance().set(&Key::Admin, &admin);
        e.storage()
            .instance()
            .set(&Key::Tokens, &vec![&e, token0, token1]);
        e.storage()
            .instance()
            .set(&Key::Ratio, &966_000_000_000u128);
    }
    pub fn set_ratio(e: Env, ratio: u128) {
        testnet(&e);
        let admin: Address = e.storage().instance().get(&Key::Admin).unwrap();
        admin.require_auth();
        assert!((800_000_000_000..=1_050_000_000_000).contains(&ratio));
        e.storage().instance().set(&Key::Ratio, &ratio);
    }
    pub fn get_tokens(e: Env) -> Vec<Address> {
        e.storage().instance().get(&Key::Tokens).unwrap()
    }
    pub fn estimate_swap(e: Env, in_idx: u32, out_idx: u32, in_amount: u128) -> u128 {
        assert!(in_idx <= 1 && out_idx <= 1 && in_idx != out_idx);
        let ratio: u128 = e.storage().instance().get(&Key::Ratio).unwrap();
        if in_idx == 0 {
            in_amount.checked_mul(ratio).unwrap() / 1_000_000_000_000
        } else {
            in_amount.checked_mul(1_000_000_000_000).unwrap() / ratio
        }
    }
    pub fn decimals() -> u32 {
        7
    }
    pub fn resolution() -> u32 {
        60
    }
    pub fn lastprice(e: Env, asset: Asset) -> Option<PriceData> {
        let tokens = Self::get_tokens(e.clone());
        match asset {
            Asset::Stellar(a) if a == tokens.get(1).unwrap() => Some(PriceData {
                price: 2_500_000,
                timestamp: e.ledger().timestamp(),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    #[test]
    fn testnet_quotes_and_admin_guards() {
        let e = Env::default();
        e.mock_all_auths();
        let id = e
            .crypto()
            .sha256(&Bytes::from_slice(&e, b"Test SDF Network ; September 2015"));
        e.ledger().set_network_id(id.to_array());
        let admin = Address::generate(&e);
        let quote = Address::generate(&e);
        let c = e.register(DepthFixture, (admin, Address::generate(&e), quote.clone()));
        let client = DepthFixtureClient::new(&e, &c);
        client.set_ratio(&970_000_000_000);
        assert_eq!(client.estimate_swap(&0, &1, &1_000_000_000), 970_000_000);
        assert!(client.try_estimate_swap(&0, &0, &1).is_err());
        assert!(client.try_set_ratio(&1).is_err());
        assert_eq!(
            client.lastprice(&Asset::Stellar(quote)).unwrap().price,
            2_500_000
        );
        e.mock_auths(&[]);
        assert!(client.try_set_ratio(&980_000_000_000).is_err());
    }
    #[test]
    #[should_panic(expected = "Testnet fixture only")]
    fn refuses_public_network_deployment() {
        let e = Env::default();
        e.mock_all_auths();
        let id = e.crypto().sha256(&Bytes::from_slice(
            &e,
            b"Public Global Stellar Network ; September 2015",
        ));
        e.ledger().set_network_id(id.to_array());
        e.register(
            DepthFixture,
            (
                Address::generate(&e),
                Address::generate(&e),
                Address::generate(&e),
            ),
        );
    }
}
