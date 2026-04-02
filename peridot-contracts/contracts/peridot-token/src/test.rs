use super::*;
use soroban_sdk::testutils::Address as _;

#[test]
fn test_initialize_and_mint() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::from_string(&String::from_str(&env, DEFAULT_INIT_ADMIN));
    let id = env.register(PeridotToken, ());
    let client = PeridotTokenClient::new(&env, &id);
    client.initialize(
        &String::from_str(&env, "Peridot"),
        &String::from_str(&env, "P"),
        &6u32,
        &admin,
        &1_000_000i128,
    );

    let user = Address::generate(&env);
    client.mint(&user, &100i128);
    assert_eq!(client.balance(&user), 100i128);
    assert_eq!(client.total_supply(), 100i128);
}

#[test]
#[should_panic]
fn test_transfer_from_requires_spender_auth() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::from_string(&String::from_str(&env, DEFAULT_INIT_ADMIN));
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    let id = env.register(PeridotToken, ());
    let client = PeridotTokenClient::new(&env, &id);
    client.initialize(
        &String::from_str(&env, "Peridot"),
        &String::from_str(&env, "P"),
        &6u32,
        &admin,
        &1_000_000i128,
    );
    client.mint(&owner, &1000i128);
    client.approve(&owner, &spender, &500i128, &u32::MAX);

    // Remove mocked auth entries and ensure spender auth is required.
    env.set_auths(&[]);
    client.transfer_from(&spender, &owner, &recipient, &100i128);
}
