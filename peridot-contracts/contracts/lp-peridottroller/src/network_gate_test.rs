//! The LP controller is validation-only: it must refuse to initialise on Stellar Mainnet.
use super::*;
use soroban_sdk::{testutils::Ledger, Env};

fn mainnet_id(env: &Env) -> [u8; 32] {
    env.crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(
            env,
            b"Public Global Stellar Network ; September 2015",
        ))
        .to_bytes()
        .to_array()
}

fn set_network(env: &Env, id: [u8; 32]) {
    env.ledger().with_mut(|li| li.network_id = id);
}

#[test]
fn validation_networks_are_allowed() {
    let env = Env::default();
    // Default test network, and the Stellar testnet passphrase.
    require_validation_network(&env);
    let testnet = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(
            &env,
            b"Test SDF Network ; September 2015",
        ))
        .to_bytes()
        .to_array();
    set_network(&env, testnet);
    require_validation_network(&env);
}

#[test]
#[should_panic(expected = "Mainnet initialization disabled")]
fn mainnet_is_rejected() {
    let env = Env::default();
    let id = mainnet_id(&env);
    set_network(&env, id);
    require_validation_network(&env);
}

#[test]
fn a_single_flipped_bit_in_the_mainnet_id_is_not_mainnet() {
    let env = Env::default();
    let mut id = mainnet_id(&env);
    id[31] ^= 1;
    set_network(&env, id);
    require_validation_network(&env);
}
