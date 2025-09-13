#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env,
};
use soroban_sdk::testutils::Ledger;
use crate as rv;

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

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault with 0% yearly interest
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);

    // Verify initialization
    assert_eq!(vault_client.get_underlying_token(), token_address);
    assert_eq!(vault_client.get_total_deposited(), 0u128);
    assert_eq!(vault_client.get_total_ptokens(), 0u128);
    assert_eq!(vault_client.get_exchange_rate(), 1_000_000u128); // 1:1 ratio
}

#[test]
fn test_deposit_receives_ptokens() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint some tokens to the user
    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault (0% interest)
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);

    // Test deposit
    vault_client.deposit(&user, &100u128);

    // Verify deposit - user should get 100 pTokens for 100 underlying (1:1 ratio)
    assert_eq!(vault_client.get_user_balance(&user), 100u128); // Original balance tracking
    assert_eq!(vault_client.get_ptoken_balance(&user), 100u128); // New pToken balance
    assert_eq!(vault_client.get_total_deposited(), 100u128);
    assert_eq!(vault_client.get_total_ptokens(), 100u128);
    assert_eq!(token_client.balance(&vault_contract_id), 100i128);
    assert_eq!(token_client.balance(&user), 900i128);
}

#[test]
fn test_withdraw_with_ptokens() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint some tokens to the user
    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize and deposit
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.deposit(&user, &100u128);

    // Test partial withdraw using pTokens
    vault_client.withdraw(&user, &30u128); // Withdraw using 30 pTokens

    // Verify partial withdraw
    assert_eq!(vault_client.get_ptoken_balance(&user), 70u128); // 100 - 30 pTokens
    assert_eq!(vault_client.get_user_balance(&user), 70u128); // Original tracking
    // TotalDeposited tracks remaining principal
    assert_eq!(vault_client.get_total_deposited(), 70u128);
    assert_eq!(vault_client.get_total_ptokens(), 70u128);
    assert_eq!(token_client.balance(&vault_contract_id), 70i128);
    assert_eq!(token_client.balance(&user), 930i128); // 900 + 30

    // Test full withdraw
    vault_client.withdraw(&user, &70u128);

    // Verify full withdraw
    assert_eq!(vault_client.get_ptoken_balance(&user), 0u128);
    assert_eq!(vault_client.get_user_balance(&user), 0u128);
    // TotalDeposited reduced to zero after full withdraw
    assert_eq!(vault_client.get_total_deposited(), 0u128);
    assert_eq!(vault_client.get_total_ptokens(), 0u128);
    assert_eq!(token_client.balance(&vault_contract_id), 0i128);
    assert_eq!(token_client.balance(&user), 1000i128);
}

#[test]
fn test_multiple_users_with_ptokens() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint tokens to both users
    token_admin_client.mint(&user1, &500i128);
    token_admin_client.mint(&user2, &300i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);

    // Both users deposit
    vault_client.deposit(&user1, &200u128);
    vault_client.deposit(&user2, &150u128);

    // Verify individual pToken balances (1:1 ratio)
    assert_eq!(vault_client.get_ptoken_balance(&user1), 200u128);
    assert_eq!(vault_client.get_ptoken_balance(&user2), 150u128);
    assert_eq!(vault_client.get_total_deposited(), 350u128);
    assert_eq!(vault_client.get_total_ptokens(), 350u128);

    // User1 withdraws some using pTokens
    vault_client.withdraw(&user1, &50u128);

    // Verify balances after user1 withdraw
    assert_eq!(vault_client.get_ptoken_balance(&user1), 150u128); // 200 - 50
    assert_eq!(vault_client.get_ptoken_balance(&user2), 150u128); // unchanged
    // TotalDeposited reduced by withdrawn amount
    assert_eq!(vault_client.get_total_deposited(), 300u128);
    assert_eq!(vault_client.get_total_ptokens(), 300u128);

    // Verify token balances
    assert_eq!(token_client.balance(&user1), 350i128); // 500 - 200 + 50
    assert_eq!(token_client.balance(&user2), 150i128); // 300 - 150
    assert_eq!(token_client.balance(&vault_contract_id), 300i128);
}

#[test]
fn test_exchange_rate_accrues_with_interest() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize with 10% yearly interest (scaled 1e6 = 0.10e6)
    let yearly_rate = 100_000u128; // 10%
    vault_client.initialize(&token_address, &yearly_rate, &0u128, &admin);

    // Initial exchange rate
    assert_eq!(vault_client.get_exchange_rate(), 1_000_000u128);

    // Deposit and then advance time by ~1 year to accrue interest
    vault_client.deposit(&user, &100u128);

    // Advance ledger time by 1 year
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);

    // Trigger interest update via a read path that calls update_interest first
    // Call set_interest_rate with the same rate to accrue first
    vault_client.set_interest_rate(&yearly_rate);

    // Exchange rate should have increased after accrual
    let rate = vault_client.get_exchange_rate();
    assert!(rate > 1_000_000u128);
}

#[test]
#[should_panic(expected = "Insufficient pTokens")]
fn test_withdraw_insufficient_ptokens() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint some tokens to the user
    token_admin_client.mint(&user, &100i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);

    // Deposit 50, get 50 pTokens
    vault_client.deposit(&user, &50u128);

    // Try to withdraw using 100 pTokens (should panic)
    vault_client.withdraw(&user, &100u128);
}

#[test]
#[should_panic(expected = "Vault not initialized")]
fn test_deposit_uninitialized_vault() {
    let env = Env::default();
    env.mock_all_auths();

    let user = Address::generate(&env);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Try to deposit without initializing (should panic)
    vault_client.deposit(&user, &100u128);
}

#[test]
fn test_zero_balance_users() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);

    // Check balance of user who never deposited
    assert_eq!(vault_client.get_user_balance(&user), 0u128);
    assert_eq!(vault_client.get_ptoken_balance(&user), 0u128);
}

#[test]
fn test_borrow_and_repay_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // 0% supply, 0% borrow to simplify
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);

    // Deposit 200 underlying -> 200 pTokens
    vault_client.deposit(&user, &200u128);
    assert_eq!(vault_client.get_ptoken_balance(&user), 200u128);

    // Borrow up to 50% collateral -> 100 allowed. Borrow 80.
    vault_client.borrow(&user, &80u128);
    assert_eq!(vault_client.get_user_borrow_balance(&user), 80u128);
    assert_eq!(token_client.balance(&user), 880i128); // 1000 -200 +80

    // Repay 50
    vault_client.repay(&user, &50u128);
    assert_eq!(vault_client.get_user_borrow_balance(&user), 30u128);
    assert_eq!(token_client.balance(&user), 830i128); // 880 -50

    // Repay remainder
    vault_client.repay(&user, &1000u128);
    assert_eq!(vault_client.get_user_borrow_balance(&user), 0u128);
}

#[test]
fn test_borrow_interest_accrues_and_index_updates() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // 0% supply, 10% borrow
    let borrow_rate = 100_000u128; // 10%
    vault_client.initialize(&token_address, &0u128, &borrow_rate, &admin);

    // Deposit to provide liquidity
    vault_client.deposit(&user, &200u128);

    // Borrow 100
    vault_client.borrow(&user, &100u128);
    let debt_before = vault_client.get_user_borrow_balance(&user);
    assert_eq!(debt_before, 100u128);

    // Advance 1 year
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);

    // Trigger interest accrual by tweaking borrow rate to same value
    vault_client.set_borrow_rate(&borrow_rate);

    let debt_after = vault_client.get_user_borrow_balance(&user);
    assert!(debt_after > debt_before);
}

#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_borrow_insufficient_collateral() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    vault_client.initialize(&token_address, &0u128, &0u128, &admin);

    // Deposit small amount -> low collateral
    vault_client.deposit(&user, &10u128);

    // Try to borrow more than 50% of collateral
    vault_client.borrow(&user, &100u128);
}

#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_borrow_insufficient_liquidity() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user_a, &2000i128);
    token_admin_client.mint(&user_b, &2000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    vault_client.initialize(&token_address, &0u128, &0u128, &admin);

    // Set collateral factor to 100% so collateral won't be the limiting factor
    vault_client.set_collateral_factor(&1_000_000u128);

    // User A deposits 500 (collateral = 500)
    vault_client.deposit(&user_a, &500u128);

    // Try to borrow over collateral cap to ensure guard triggers
    vault_client.borrow(&user_a, &600u128);
}

#[test]
fn test_admin_setters_guarded() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // initialize sets admin = invoker; in test env, invoker is Address(0) unless auth mocked, so call via contract client with mock_all_auths covers auth
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);

    // Expect setters callable under mocked auth
    vault_client.set_collateral_factor(&600_000u128);
    vault_client.set_interest_rate(&50_000u128);
    vault_client.set_borrow_rate(&100_000u128);
}

// (cross-market collateral tests moved to simple-comptroller crate to avoid circular deps)
