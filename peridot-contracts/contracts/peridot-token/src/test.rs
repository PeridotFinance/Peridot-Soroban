#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_token_mint_transfer_burn() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    let id = env.register(PeridotToken, ());
    let c = PeridotTokenClient::new(&env, &id);

    c.initialize(&String::from_str(&env, "Peridot"), &String::from_str(&env, "P"), &6u32, &admin);

    // Mint
    c.mint(&a, &1000i128);
    assert_eq!(c.total_supply(), 1000i128);
    assert_eq!(c.balance_of(&a), 1000i128);

    // Transfer
    c.transfer(&a, &b, &300i128);
    assert_eq!(c.balance_of(&a), 700i128);
    assert_eq!(c.balance_of(&b), 300i128);

    // Approve + transfer_from
    c.approve(&b, &a, &100i128); // b approves a to spend 100 (symmetry for test)
    c.transfer_from(&a, &b, &a, &100i128);
    assert_eq!(c.balance_of(&a), 800i128);
    assert_eq!(c.balance_of(&b), 200i128);

    // Burn
    c.burn(&a, &200i128);
    assert_eq!(c.balance_of(&a), 600i128);
    assert_eq!(c.total_supply(), 800i128);
}
