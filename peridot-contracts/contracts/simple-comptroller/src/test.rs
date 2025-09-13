#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};
use soroban_sdk::testutils::Ledger;
use receipt_vault as rv;
use soroban_sdk::token;
use soroban_sdk::{contract, contractimpl, contracttype};

#[test]
fn test_comptroller_add_and_enter_market() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    // Use a real vault as market to satisfy safety checks
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    let market_vault_id = env.register(rv::ReceiptVault, ());
    let market_vault = rv::ReceiptVaultClient::new(&env, &market_vault_id);
    market_vault.initialize(&token, &0u128, &0u128, &admin);

    let id = env.register(SimpleComptroller, ());
    let client = SimpleComptrollerClient::new(&env, &id);

    client.initialize(&admin);
    client.add_market(&market_vault_id);
    client.enter_market(&user, &market_vault_id);
    // Basic exit/remove flow (no balances => allowed)
    client.exit_market(&user, &market_vault_id);
    let markets_after = client.get_user_markets(&user);
    assert_eq!(markets_after.len(), 0);
    // Remove
    client.remove_market(&market_vault_id);

    // Re-add and re-enter to assert happy path
    client.add_market(&market_vault_id);
    client.enter_market(&user, &market_vault_id);
    let markets = client.get_user_markets(&user);
    assert_eq!(markets.len(), 1);

    let markets = client.get_user_markets(&user);
    assert_eq!(markets.len(), 1);
    assert_eq!(markets.get(0), Some(market_vault_id));
}

#[test]
fn test_total_collateral_and_borrows_across_markets() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Token A
    let token_admin = Address::generate(&env);
    let token_a = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    // Token B
    let token_admin_b = Address::generate(&env);
    let token_b = env.register_stellar_asset_contract_v2(token_admin_b.clone()).address();

    // Deploy two vaults
    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);

    // Initialize: 0% supply, 0% borrow, set admin
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    // Mint tokens to user for both assets
    let token_a_admin = token::StellarAssetClient::new(&env, &token_a);
    let token_b_admin = token::StellarAssetClient::new(&env, &token_b);
    token_a_admin.mint(&user, &1000i128);
    token_b_admin.mint(&user, &1000i128);

    // Comptroller
    let comp_id = env.register(SimpleComptroller, ());
    // Wire comptroller to both vaults (after comp_id exists)
    vault_a.set_comptroller(&comp_id);
    vault_b.set_comptroller(&comp_id);
    let comp = SimpleComptrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&user, &vault_a_id);
    comp.enter_market(&user, &vault_b_id);

    // (Exit guard tested in previous test with real vault; here focus on totals and borrows)

    // Deposit into both markets
    vault_a.deposit(&user, &200u128);
    vault_b.deposit(&user, &300u128);

    // Collateral should equal sum of deposits (1:1 rate)
    let total_collateral = comp.get_user_total_collateral(&user);
    assert_eq!(total_collateral, 500u128);

    // Borrow 100 from A and 50 from B
    // Increase collateral factor to 100% to avoid cap
    vault_a.set_collateral_factor(&1_000_000u128);
    vault_b.set_collateral_factor(&1_000_000u128);
    vault_a.borrow(&user, &100u128);
    vault_b.borrow(&user, &50u128);

    let total_borrows = comp.get_user_total_borrows(&user);
    assert_eq!(total_borrows, 150u128);
}


// Mock Reflector oracle for tests
#[contract]
struct MockOracle;

#[contracttype]
enum OracleKey {
    Decimals,
    Price(Address),
}

#[contracttype]
#[derive(Clone)]
struct OraclePrice {
    price: i128,
}

#[contractimpl]
impl MockOracle {
    pub fn initialize(env: Env, decimals: u32) {
        env.storage().persistent().set(&OracleKey::Decimals, &decimals);
    }
    pub fn set_price(env: Env, asset: Address, price: i128) {
        env.storage().persistent().set(&OracleKey::Price(asset), &OraclePrice { price });
    }
    pub fn decimals(env: Env) -> u32 {
        env.storage().persistent().get(&OracleKey::Decimals).unwrap_or(6u32)
    }
    pub fn lastprice(env: Env, asset: crate::reflector::Asset) -> Option<crate::reflector::PriceData> {
        match asset {
            crate::reflector::Asset::Stellar(addr) => {
                let rec: Option<OraclePrice> = env.storage().persistent().get(&OracleKey::Price(addr));
                rec.map(|r| crate::reflector::PriceData { price: r.price, timestamp: env.ledger().timestamp() })
            }
            _ => None,
        }
    }
    pub fn resolution(_env: Env) -> u32 { 300 }
}

#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_oracle_gating_prevents_over_borrow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Tokens
    let token_admin_a = Address::generate(&env);
    let token_a = env.register_stellar_asset_contract_v2(token_admin_a.clone()).address();
    let token_admin_b = Address::generate(&env);
    let token_b = env.register_stellar_asset_contract_v2(token_admin_b.clone()).address();

    // Vaults
    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    // Comptroller
    let comp_id = env.register(SimpleComptroller, ());
    let comp = SimpleComptrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&user, &vault_a_id);
    comp.enter_market(&user, &vault_b_id);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    // Prices: token_a $1, token_b $0.10
    oracle.set_price(&token_a, &1_000_000i128);
    oracle.set_price(&token_b, &100_000i128);
    comp.set_oracle(&oracle_id);

    // Wire comptroller in vaults
    vault_a.set_comptroller(&comp_id);
    vault_b.set_comptroller(&comp_id);

    // Liquidity: deposit into A for outflow, and into B as collateral
    let admin_a = token::StellarAssetClient::new(&env, &token_a);
    let admin_b = token::StellarAssetClient::new(&env, &token_b);
    admin_a.mint(&user, &1_000i128);
    admin_b.mint(&user, &1_000i128);

    vault_a.deposit(&user, &5u128); // small liquidity
    vault_b.set_collateral_factor(&1_000_000u128);
    vault_b.deposit(&user, &100u128); // $10 collateral

    // Try to borrow $20 USDC from A -> should panic
    vault_a.borrow(&user, &20u128);
}

#[test]
fn test_oracle_gating_allows_within_limit() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Tokens
    let token_admin_a = Address::generate(&env);
    let token_a = env.register_stellar_asset_contract_v2(token_admin_a.clone()).address();
    let token_admin_b = Address::generate(&env);
    let token_b = env.register_stellar_asset_contract_v2(token_admin_b.clone()).address();

    // Vaults
    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    // Comptroller
    let comp_id = env.register(SimpleComptroller, ());
    let comp = SimpleComptrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&user, &vault_a_id);
    comp.enter_market(&user, &vault_b_id);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    // Prices: token_a $1, token_b $0.10
    oracle.set_price(&token_a, &1_000_000i128);
    oracle.set_price(&token_b, &100_000i128);
    comp.set_oracle(&oracle_id);

    // Wire comptroller in vaults
    vault_a.set_comptroller(&comp_id);
    vault_b.set_comptroller(&comp_id);

    // Liquidity and collateral
    let admin_a = token::StellarAssetClient::new(&env, &token_a);
    let admin_b = token::StellarAssetClient::new(&env, &token_b);
    admin_a.mint(&user, &1_000i128);
    admin_b.mint(&user, &1_000i128);

    vault_a.deposit(&user, &20u128); // enough liquidity
    vault_b.set_collateral_factor(&1_000_000u128);
    vault_b.deposit(&user, &100u128); // $10 collateral

    // Borrow $10 USDC -> allowed
    vault_a.borrow(&user, &10u128);
}

