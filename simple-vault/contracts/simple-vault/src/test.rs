#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env,
};

fn create_test_token<'a>(env: &'a Env, admin: &'a Address) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone()).address();
    (
        contract_address.clone(),
        token::Client::new(env, &contract_address),
        token::StellarAssetClient::new(env, &contract_address),
    )
}

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_contract_id = env.register(SimpleVault, ());
    let vault_client = SimpleVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault
    vault_client.initialize(&token_address);

    // Verify initialization
    assert_eq!(vault_client.get_underlying_token(), token_address);
    assert_eq!(vault_client.get_total_deposited(), 0u128);
}

#[test]
fn test_deposit_and_withdraw() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint some tokens to the user
    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(SimpleVault, ());
    let vault_client = SimpleVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault
    vault_client.initialize(&token_address);

    // Test deposit
    vault_client.deposit(&user, &100u128);

    // Verify deposit
    assert_eq!(vault_client.get_user_balance(&user), 100u128);
    assert_eq!(vault_client.get_total_deposited(), 100u128);
    assert_eq!(token_client.balance(&vault_contract_id), 100i128);
    assert_eq!(token_client.balance(&user), 900i128);

    // Test partial withdraw
    vault_client.withdraw(&user, &30u128);

    // Verify partial withdraw
    assert_eq!(vault_client.get_user_balance(&user), 70u128);
    assert_eq!(vault_client.get_total_deposited(), 70u128);
    assert_eq!(token_client.balance(&vault_contract_id), 70i128);
    assert_eq!(token_client.balance(&user), 930i128);

    // Test full withdraw
    vault_client.withdraw(&user, &70u128);

    // Verify full withdraw
    assert_eq!(vault_client.get_user_balance(&user), 0u128);
    assert_eq!(vault_client.get_total_deposited(), 0u128);
    assert_eq!(token_client.balance(&vault_contract_id), 0i128);
    assert_eq!(token_client.balance(&user), 1000i128);
}

#[test]
fn test_multiple_users() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint tokens to both users
    token_admin_client.mint(&user1, &500i128);
    token_admin_client.mint(&user2, &300i128);

    let vault_contract_id = env.register(SimpleVault, ());
    let vault_client = SimpleVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault
    vault_client.initialize(&token_address);

    // Both users deposit
    vault_client.deposit(&user1, &200u128);
    vault_client.deposit(&user2, &150u128);

    // Verify individual balances
    assert_eq!(vault_client.get_user_balance(&user1), 200u128);
    assert_eq!(vault_client.get_user_balance(&user2), 150u128);
    assert_eq!(vault_client.get_total_deposited(), 350u128);

    // User1 withdraws some
    vault_client.withdraw(&user1, &50u128);

    // Verify balances after user1 withdraw
    assert_eq!(vault_client.get_user_balance(&user1), 150u128);
    assert_eq!(vault_client.get_user_balance(&user2), 150u128);
    assert_eq!(vault_client.get_total_deposited(), 300u128);

    // Verify token balances
    assert_eq!(token_client.balance(&user1), 350i128); // 500 - 200 + 50
    assert_eq!(token_client.balance(&user2), 150i128); // 300 - 150
    assert_eq!(token_client.balance(&vault_contract_id), 300i128);
}

#[test]
#[should_panic(expected = "Insufficient balance")]
fn test_withdraw_insufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint some tokens to the user
    token_admin_client.mint(&user, &100i128);

    let vault_contract_id = env.register(SimpleVault, ());
    let vault_client = SimpleVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault
    vault_client.initialize(&token_address);

    // Deposit 50
    vault_client.deposit(&user, &50u128);

    // Try to withdraw 100 (should panic)
    vault_client.withdraw(&user, &100u128);
}

#[test]
#[should_panic(expected = "Vault not initialized")]
fn test_deposit_uninitialized_vault() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    let vault_contract_id = env.register(SimpleVault, ());
    let vault_client = SimpleVaultClient::new(&env, &vault_contract_id);

    // Try to deposit without initializing (should panic)
    vault_client.deposit(&user, &100u128);
}

#[test]
fn test_zero_balance_user() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_contract_id = env.register(SimpleVault, ());
    let vault_client = SimpleVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault
    vault_client.initialize(&token_address);

    // Check balance of user who never deposited
    assert_eq!(vault_client.get_user_balance(&user), 0u128);
}
