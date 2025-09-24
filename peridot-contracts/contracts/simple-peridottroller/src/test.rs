#![cfg(test)]
use super::*;
use receipt_vault as rv;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::token;
use soroban_sdk::BytesN;
use soroban_sdk::{contract, contractimpl, contracttype};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_peridottroller_add_and_enter_market() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let _lender = Address::generate(&env);
    // Use a real vault as market to satisfy safety checks
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let market_vault_id = env.register(rv::ReceiptVault, ());
    let market_vault = rv::ReceiptVaultClient::new(&env, &market_vault_id);
    market_vault.initialize(&token, &0u128, &0u128, &admin);

    let id = env.register(SimplePeridottroller, ());
    let client = SimplePeridottrollerClient::new(&env, &id);

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
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    // Token B
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

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

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    // Wire peridottroller to both vaults (after comp_id exists)
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
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
        env.storage()
            .persistent()
            .set(&OracleKey::Decimals, &decimals);
    }
    pub fn set_price(env: Env, asset: Address, price: i128) {
        env.storage()
            .persistent()
            .set(&OracleKey::Price(asset), &OraclePrice { price });
    }
    pub fn decimals(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&OracleKey::Decimals)
            .unwrap_or(6u32)
    }
    pub fn lastprice(
        env: Env,
        asset: crate::reflector::Asset,
    ) -> Option<crate::reflector::PriceData> {
        match asset {
            crate::reflector::Asset::Stellar(addr) => {
                let rec: Option<OraclePrice> =
                    env.storage().persistent().get(&OracleKey::Price(addr));
                rec.map(|r| crate::reflector::PriceData {
                    price: r.price,
                    timestamp: env.ledger().timestamp(),
                })
            }
            _ => None,
        }
    }
    pub fn resolution(_env: Env) -> u32 {
        300
    }
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
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    // Vaults
    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
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

    // Wire peridottroller in vaults
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

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
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    // Vaults
    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.set_market_cf(&vault_b_id, &1_000_000u128);
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

    // Wire peridottroller in vaults
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

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

#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_redeem_gating_prevents_over_withdraw() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Tokens
    let token_admin_a = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();

    // Vault
    let vault_id = env.register(rv::ReceiptVault, ());
    let vault = rv::ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_a, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.enter_market(&user, &vault_id);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    // token_a $1
    oracle.set_price(&token_a, &1_000_000i128);
    comp.set_oracle(&oracle_id);

    // Wire peridottroller
    vault.set_peridottroller(&comp_id);

    // Liquidity and collateral
    let admin_a = token::StellarAssetClient::new(&env, &token_a);
    admin_a.mint(&user, &1_000i128);
    vault.set_collateral_factor(&500_000u128); // 50%

    // Deposit 100 -> collateral 50 USD cap
    vault.deposit(&user, &100u128);

    // Borrow 50 (at limit)
    vault.borrow(&user, &50u128);

    // Try to withdraw 1 pToken (would reduce collateral below borrows)
    vault.withdraw(&user, &1u128);
}

#[test]
fn test_redeem_gating_allows_within_limit() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lender = Address::generate(&env);

    // Tokens
    let token_admin_a = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    // Vaults
    let vault_id = env.register(rv::ReceiptVault, ());
    let vault = rv::ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_a, &0u128, &0u128, &admin);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&user, &vault_id);
    comp.enter_market(&user, &vault_b_id);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    // token_a $1, token_b $1
    oracle.set_price(&token_a, &1_000_000i128);
    oracle.set_price(&token_b, &1_000_000i128);
    comp.set_oracle(&oracle_id);

    // Wire peridottroller
    vault.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    // Liquidity and collateral
    let admin_a = token::StellarAssetClient::new(&env, &token_a);
    let admin_b = token::StellarAssetClient::new(&env, &token_b);
    admin_a.mint(&user, &1_000i128);
    admin_b.mint(&user, &1_000i128);
    admin_a.mint(&lender, &1_000i128);
    vault.set_collateral_factor(&1_000_000u128); // 100%
    vault_b.set_collateral_factor(&1_000_000u128); // 100%

    // Provide collateral in market B, provide liquidity in A by lender, borrow from A
    vault_b.deposit(&user, &100u128); // $100 collateral in B
    vault.deposit(&lender, &100u128); // $100 liquidity in A by lender
    vault.deposit(&user, &10u128); // small deposit in A to allow withdraw
    vault.borrow(&user, &50u128); // Allowed due to B collateral

    // Withdraw 10 pTokens from A -> still safe
    vault.withdraw(&user, &10u128);
}

#[test]
fn test_liquidation_flow_basic() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);

    // Tokens
    let token_admin_a = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    // Vaults
    let vault_a_id = env.register(rv::ReceiptVault, ()); // borrow market
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ()); // collateral market
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    // token_a $1, token_b $1
    oracle.set_price(&token_a, &1_000_000i128);
    oracle.set_price(&token_b, &1_000_000i128);
    comp.set_oracle(&oracle_id);

    // Wire peridottroller
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    // Mint tokens
    let admin_a = token::StellarAssetClient::new(&env, &token_a);
    let admin_b = token::StellarAssetClient::new(&env, &token_b);
    admin_a.mint(&borrower, &1_000i128);
    admin_b.mint(&borrower, &1_000i128);
    admin_a.mint(&liquidator, &1_000i128);

    // Setup: borrower deposits 100 in B as collateral (CF=50%), borrows 50 in A
    vault_b.set_collateral_factor(&500_000u128);
    vault_b.deposit(&borrower, &100u128);
    // Provide liquidity in A so borrow can succeed
    vault_a.deposit(&liquidator, &200u128);
    vault_a.borrow(&borrower, &50u128);

    // Now push price of collateral down by 50% to cause shortfall
    oracle.set_price(&token_b, &500_000i128); // $0.50

    // Liquidate up to close factor (default 50%): repay 25, seize collateral with 8% bonus
    // Call liquidate
    comp.liquidate(&borrower, &vault_a_id, &vault_b_id, &25u128, &liquidator);

    // Borrower's debt should reduce
    let debt_after = vault_a.get_user_borrow_balance(&borrower);
    assert!(debt_after <= 25u128);
    // Liquidator should have received some pTokens from B
    let p_liq = vault_b.get_ptoken_balance(&liquidator);
    assert!(p_liq > 0u128);
}

#[test]
fn test_liquidation_capped_by_close_factor() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);

    // Tokens
    let token_admin_a = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    // Vaults
    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&token_a, &1_000_000i128); // $1
    oracle.set_price(&token_b, &1_000_000i128); // $1
    comp.set_oracle(&oracle_id);

    // Wire peridottroller
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    // Mint
    let admin_a = token::StellarAssetClient::new(&env, &token_a);
    let admin_b = token::StellarAssetClient::new(&env, &token_b);
    admin_a.mint(&borrower, &1_000i128);
    admin_b.mint(&borrower, &1_000i128);
    admin_a.mint(&liquidator, &1_000i128);

    // Setup positions
    vault_b.set_collateral_factor(&500_000u128); // 50%
    vault_b.deposit(&borrower, &200u128); // 200 pTokens
    vault_a.deposit(&liquidator, &300u128);
    vault_a.borrow(&borrower, &100u128); // debt 100

    // Cause shortfall: drop collateral price to $0.40
    oracle.set_price(&token_b, &400_000i128);

    // Attempt to liquidate 80 but CF=50% caps at 50
    comp.liquidate(&borrower, &vault_a_id, &vault_b_id, &80u128, &liquidator);

    // Debt should be exactly 50
    let debt_after = vault_a.get_user_borrow_balance(&borrower);
    assert_eq!(debt_after, 50u128);

    // Seized pTokens should be 135 (50 * 1.08 / 0.40)
    let seized = vault_b.get_ptoken_balance(&liquidator);
    assert_eq!(seized, 135u128);
}

#[test]
#[should_panic(expected = "no shortfall")]
fn test_liquidation_no_shortfall_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);

    // Tokens
    let token_admin_a = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    // Vaults
    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&token_a, &1_000_000i128);
    oracle.set_price(&token_b, &1_000_000i128);
    comp.set_oracle(&oracle_id);

    // Wire peridottroller
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    // Mint and positions
    let admin_a = token::StellarAssetClient::new(&env, &token_a);
    let admin_b = token::StellarAssetClient::new(&env, &token_b);
    admin_a.mint(&borrower, &1_000i128);
    admin_b.mint(&borrower, &1_000i128);
    admin_a.mint(&liquidator, &1_000i128);

    vault_b.set_collateral_factor(&500_000u128);
    vault_b.deposit(&borrower, &200u128);
    vault_a.deposit(&liquidator, &300u128);
    vault_a.borrow(&borrower, &50u128);

    // No price drop -> healthy -> should panic
    comp.liquidate(&borrower, &vault_a_id, &vault_b_id, &10u128, &liquidator);
}

#[test]
#[should_panic(expected = "repay too small")]
fn test_liquidation_zero_repay_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let token_admin_a = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&token_a, &1_000_000i128);
    oracle.set_price(&token_b, &1_000_000i128); // start healthy to allow borrow
    comp.set_oracle(&oracle_id);

    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    let admin_a = token::StellarAssetClient::new(&env, &token_a);
    let admin_b = token::StellarAssetClient::new(&env, &token_b);
    admin_a.mint(&borrower, &1_000i128);
    admin_b.mint(&borrower, &1_000i128);
    admin_a.mint(&liquidator, &1_000i128);

    vault_b.set_collateral_factor(&500_000u128);
    vault_b.deposit(&borrower, &100u128);
    vault_a.deposit(&liquidator, &300u128);
    vault_a.borrow(&borrower, &50u128);

    // Now cause shortfall by dropping collateral price
    oracle.set_price(&token_b, &500_000i128);
    // zero repay -> panic
    comp.liquidate(&borrower, &vault_a_id, &vault_b_id, &0u128, &liquidator);
}

#[test]
fn test_preview_helpers_basic() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    // Single market
    let vault_id = env.register(rv::ReceiptVault, ());
    let vault = rv::ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.enter_market(&user, &vault_id);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&token, &1_000_000i128); // $1
    comp.set_oracle(&oracle_id);

    // Wire and fund
    vault.set_peridottroller(&comp_id);
    let mint = token::StellarAssetClient::new(&env, &token);
    mint.mint(&user, &1_000i128);
    // User deposits 200, CF=50%
    vault.set_collateral_factor(&500_000u128);
    vault.deposit(&user, &200u128);

    // Available liquidity 500
    mint.mint(&admin, &1_000i128);
    vault.deposit(&admin, &500u128);

    // Max borrow should be min(100 (CF of 200), 500 (available)) = 100
    let max_borrow = comp.preview_borrow_max(&user, &vault_id);
    assert_eq!(max_borrow, 100u128);

    // Max redeem with no debt should be limited by avail. Convert to pTokens: underlying 500 -> p 500
    let max_redeem = comp.preview_redeem_max(&user, &vault_id);
    assert_eq!(max_redeem, 200u128); // user has only 200 pTokens
}

#[test]
fn test_preview_helpers_extended() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquid = Address::generate(&env);

    // Tokens + markets (A=borrow, B=collateral)
    let t_admin_a = Address::generate(&env);
    let t_a = env
        .register_stellar_asset_contract_v2(t_admin_a.clone())
        .address();
    let t_admin_b = Address::generate(&env);
    let t_b = env
        .register_stellar_asset_contract_v2(t_admin_b.clone())
        .address();
    let va_id = env.register(rv::ReceiptVault, ());
    let va = rv::ReceiptVaultClient::new(&env, &va_id);
    let vb_id = env.register(rv::ReceiptVault, ());
    let vb = rv::ReceiptVaultClient::new(&env, &vb_id);
    va.initialize(&t_a, &0u128, &0u128, &admin);
    vb.initialize(&t_b, &0u128, &0u128, &admin);

    // Comptroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.set_market_cf(&vb_id, &1_000_000u128);
    comp.enter_market(&borrower, &va_id);
    comp.enter_market(&borrower, &vb_id);
    va.set_peridottroller(&comp_id);
    vb.set_peridottroller(&comp_id);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.initialize(&6u32);
    oracle.set_price(&t_a, &1_000_000i128);
    oracle.set_price(&t_b, &1_000_000i128);
    comp.set_oracle(&oracle_id);

    // Fund
    let mint_a = token::StellarAssetClient::new(&env, &t_a);
    let mint_b = token::StellarAssetClient::new(&env, &t_b);
    mint_a.mint(&liquid, &1_000i128);
    mint_b.mint(&borrower, &1_000i128);
    vb.set_collateral_factor(&1_000_000u128);
    vb.deposit(&borrower, &200u128); // collateral in B
    va.deposit(&liquid, &500u128); // liquidity in A

    // Borrow 120
    va.borrow(&borrower, &120u128);

    // Close factor default 50% => repay cap 60
    let cap = comp.preview_repay_cap(&borrower, &va_id);
    assert_eq!(cap, 60u128);

    // Seize preview for repay 60 with LI=1.08 -> seize = 64.8 underlying -> pTokens = 64.8
    let seize = comp.preview_seize_ptokens(&va_id, &vb_id, &60u128);
    // exchange rate is 1 so pTokens == underlying
    assert_eq!(seize, (60u128 * 1_080_000u128 / 1_000_000u128));
}

#[test]
fn test_set_admin_transfers_admin_role() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let id = env.register(SimplePeridottroller, ());
    let c = SimplePeridottrollerClient::new(&env, &id);
    c.initialize(&admin);
    assert_eq!(c.get_admin(), admin);
    c.set_admin(&new_admin);
    assert_eq!(c.get_admin(), new_admin);
}

#[test]
#[should_panic]
fn test_peridottroller_upgrade_requires_admin() {
    let env = Env::default();
    // no mock_all_auths to enforce real auth
    let admin = Address::generate(&env);
    let id = env.register(SimplePeridottroller, ());
    let c = SimplePeridottrollerClient::new(&env, &id);
    c.initialize(&admin);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    // caller is not admin (no mocked auth) -> should panic
    c.upgrade_wasm(&hash);
}

#[test]
#[should_panic(expected = "borrow paused")]
fn test_pause_borrow_blocks_borrow() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let vault_id = env.register(rv::ReceiptVault, ());
    let vault = rv::ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token, &0u128, &0u128, &admin);
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.enter_market(&user, &vault_id);
    vault.set_peridottroller(&comp_id);
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&token, &1_000_000i128);
    comp.set_oracle(&oracle_id);
    let mint = token::StellarAssetClient::new(&env, &token);
    mint.mint(&user, &1_000i128);
    mint.mint(&admin, &1_000i128);
    vault.set_collateral_factor(&1_000_000u128);
    vault.deposit(&user, &100u128);
    vault.deposit(&admin, &200u128);
    comp.set_pause_borrow(&vault_id, &true);
    vault.borrow(&user, &10u128);
}

#[test]
#[should_panic(expected = "redeem paused")]
fn test_pause_redeem_blocks_withdraw() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let vault_id = env.register(rv::ReceiptVault, ());
    let vault = rv::ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token, &0u128, &0u128, &admin);
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.enter_market(&user, &vault_id);
    vault.set_peridottroller(&comp_id);
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&token, &1_000_000i128);
    comp.set_oracle(&oracle_id);
    let mint = token::StellarAssetClient::new(&env, &token);
    mint.mint(&user, &1_000i128);
    vault.deposit(&user, &100u128);
    comp.set_pause_redeem(&vault_id, &true);
    vault.withdraw(&user, &10u128);
}

#[test]
#[should_panic(expected = "liquidation paused")]
fn test_pause_liquidation_blocks_liquidate() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    // Markets A (borrow) and B (collateral)
    let t_admin_a = Address::generate(&env);
    let t_a = env
        .register_stellar_asset_contract_v2(t_admin_a.clone())
        .address();
    let t_admin_b = Address::generate(&env);
    let t_b = env
        .register_stellar_asset_contract_v2(t_admin_b.clone())
        .address();
    let va_id = env.register(rv::ReceiptVault, ());
    let va = rv::ReceiptVaultClient::new(&env, &va_id);
    let vb_id = env.register(rv::ReceiptVault, ());
    let vb = rv::ReceiptVaultClient::new(&env, &vb_id);
    va.initialize(&t_a, &0u128, &0u128, &admin);
    vb.initialize(&t_b, &0u128, &0u128, &admin);
    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.enter_market(&user, &va_id);
    comp.enter_market(&user, &vb_id);
    va.set_peridottroller(&comp_id);
    vb.set_peridottroller(&comp_id);
    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&t_a, &1_000_000i128); // $1
    oracle.set_price(&t_b, &1_000_000i128); // $1
    comp.set_oracle(&oracle_id);
    // Fund and positions
    let mint_a = token::StellarAssetClient::new(&env, &t_a);
    let mint_b = token::StellarAssetClient::new(&env, &t_b);
    mint_a.mint(&user, &1_000i128);
    mint_b.mint(&user, &1_000i128);
    mint_a.mint(&admin, &1_000i128);
    vb.set_collateral_factor(&500_000u128);
    vb.deposit(&user, &100u128);
    va.deposit(&admin, &200u128);
    va.borrow(&user, &50u128);
    // Pause liquidation on repay market A and create shortfall by dropping collateral price
    comp.set_pause_liquidation(&va_id, &true);
    oracle.set_price(&t_b, &100_000i128); // $0.10
                                          // Should panic
    comp.liquidate(&user, &va_id, &vb_id, &10u128, &admin);
}

#[test]
#[should_panic(expected = "deposit paused")]
fn test_pause_deposit_blocks_deposit() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let t_admin = Address::generate(&env);
    let t = env
        .register_stellar_asset_contract_v2(t_admin.clone())
        .address();
    let v_id = env.register(rv::ReceiptVault, ());
    let v = rv::ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&t, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&v_id);
    comp.set_market_cf(&v_id, &1_000_000u128);
    comp.enter_market(&user, &v_id);
    v.set_peridottroller(&comp_id);

    // Oracle price so borrow/redeem gating paths are fine
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&t, &1_000_000i128);
    comp.set_oracle(&oracle_id);

    // Mint user tokens
    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&user, &1_000i128);

    // Admin pauses deposit at per-market level
    comp.set_pause_deposit(&v_id, &true);

    // Attempt deposit -> should panic in vault via peridottroller check
    v.deposit(&user, &10u128);
}

#[test]
#[should_panic(expected = "deposit paused")]
fn test_pause_deposit_blocks_deposit_guardian() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let guardian = Address::generate(&env);
    let user = Address::generate(&env);
    let t_admin = Address::generate(&env);
    let t = env
        .register_stellar_asset_contract_v2(t_admin.clone())
        .address();
    let v_id = env.register(rv::ReceiptVault, ());
    let v = rv::ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&t, &0u128, &0u128, &admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&v_id);
    comp.set_market_cf(&v_id, &1_000_000u128);
    comp.enter_market(&user, &v_id);
    v.set_peridottroller(&comp_id);

    // Set guardian and pause via guardian
    comp.set_pause_guardian(&guardian);
    comp.pause_deposit_g(&guardian, &v_id, &true);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&t, &1_000_000i128);
    comp.set_oracle(&oracle_id);

    // Mint and attempt deposit
    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&user, &1_000i128);
    v.deposit(&user, &10u128);
}

#[test]
fn test_liquidation_fee_routed_to_reserves() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let reserve = Address::generate(&env);

    // Tokens
    let t_admin_a = Address::generate(&env);
    let t_a = env
        .register_stellar_asset_contract_v2(t_admin_a.clone())
        .address();
    let t_admin_b = Address::generate(&env);
    let t_b = env
        .register_stellar_asset_contract_v2(t_admin_b.clone())
        .address();

    // Vaults
    let va_id = env.register(rv::ReceiptVault, ());
    let va = rv::ReceiptVaultClient::new(&env, &va_id);
    let vb_id = env.register(rv::ReceiptVault, ());
    let vb = rv::ReceiptVaultClient::new(&env, &vb_id);
    va.initialize(&t_a, &0u128, &0u128, &admin);
    vb.initialize(&t_b, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.enter_market(&borrower, &va_id);
    comp.enter_market(&borrower, &vb_id);

    // Oracle + params
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&t_a, &1_000_000i128); // $1
    oracle.set_price(&t_b, &1_000_000i128); // $1
    comp.set_oracle(&oracle_id);
    comp.set_liquidation_fee(&200_000u128); // 20%
    comp.set_reserve_recipient(&reserve);

    // Wire
    va.set_peridottroller(&comp_id);
    vb.set_peridottroller(&comp_id);

    // Mint
    let mint_a = token::StellarAssetClient::new(&env, &t_a);
    let mint_b = token::StellarAssetClient::new(&env, &t_b);
    mint_a.mint(&borrower, &1_000i128);
    mint_b.mint(&borrower, &1_000i128);
    mint_a.mint(&liquidator, &1_000i128);

    // Positions: borrower collateral in B, debt in A
    vb.set_collateral_factor(&500_000u128);
    vb.deposit(&borrower, &100u128);
    va.deposit(&liquidator, &200u128);
    va.borrow(&borrower, &50u128);

    // Create shortfall by dropping price
    oracle.set_price(&t_b, &500_000i128); // $0.5

    // Liquidate repay 25 -> seize_ptokens computed with 8% incentive; 20% of seize goes to reserve
    comp.liquidate(&borrower, &va_id, &vb_id, &25u128, &liquidator);

    // Check pTokens routed
    let p_res = vb.get_ptoken_balance(&reserve);
    let p_liq = vb.get_ptoken_balance(&liquidator);
    assert!(p_res > 0u128);
    assert!(p_liq > 0u128);
    // Ensure total seized equals expected split of total seize
    let rate: u128 = vb.get_exchange_rate();
    let pb = 1_000_000u128; // $1 with 1e6 scale
    let sb = 1_000_000u128;
    let repay_usd = (25u128.saturating_mul(pb)) / sb;
    let seize_underlying_usd = (repay_usd.saturating_mul(1_080_000u128)) / 1_000_000u128;
    // Collateral price was set to $0.5 -> pc=500_000, sc=1_000_000
    let pc = 500_000u128;
    let sc = 1_000_000u128;
    let seize_underlying = (seize_underlying_usd.saturating_mul(sc)) / pc;
    let seize_ptokens = (seize_underlying.saturating_mul(1_000_000u128)) / rate;
    assert_eq!(p_res + p_liq, seize_ptokens);
}

#[test]
fn test_liquidation_seize_clamps_to_available_ptokens() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let token_admin_a = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);

    // Oracle + params
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&token_a, &1_000_000i128); // $1
    oracle.set_price(&token_b, &5_000_000i128); // $5 initial to allow borrow
    comp.set_oracle(&oracle_id);
    // Increase liquidation incentive to 2.0x
    comp.set_liquidation_incentive(&2_000_000u128);

    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    let admin_a = token::StellarAssetClient::new(&env, &token_a);
    let admin_b = token::StellarAssetClient::new(&env, &token_b);
    admin_a.mint(&borrower, &1_000i128);
    admin_b.mint(&borrower, &1_000i128);
    admin_a.mint(&liquidator, &1_000i128);

    vault_b.set_collateral_factor(&500_000u128);
    vault_b.deposit(&borrower, &50u128); // 50 pTokens
    vault_a.deposit(&liquidator, &300u128);
    // Borrow 50 allowed (discounted collateral = 50*0.5*5 = $125)
    vault_a.borrow(&borrower, &50u128);

    // Now drop price to $0.5; close factor=50% caps repay to 25; 2x LI => seize = 25*2/0.5 = 100 > 50 -> clamp to 50
    oracle.set_price(&token_b, &500_000i128);
    comp.liquidate(&borrower, &vault_a_id, &vault_b_id, &50u128, &liquidator);

    // Borrower collateral pTokens fully seized (50), liquidator receives 50 since no fee configured
    assert_eq!(vault_b.get_ptoken_balance(&borrower), 0u128);
    assert_eq!(vault_b.get_ptoken_balance(&liquidator), 50u128);
}

#[test]
fn test_oracle_missing_price_allows_borrow_with_zero_priced_assets() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Token + markets
    let t_admin = Address::generate(&env);
    let t = env
        .register_stellar_asset_contract_v2(t_admin.clone())
        .address();
    let v_id = env.register(rv::ReceiptVault, ());
    let v = rv::ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&t, &0u128, &0u128, &admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&v_id);
    comp.enter_market(&user, &v_id);
    v.set_peridottroller(&comp_id);

    // Oracle configured but no price set -> treated as missing price
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    comp.set_oracle(&oracle_id);

    // Provide liquidity & deposit to get into borrow path
    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&user, &1_000i128);
    v.set_collateral_factor(&1_000_000u128);
    v.deposit(&user, &100u128);

    // With missing price, risk calc ignores values (treated as zero USD), so borrow path won't shortfall
    v.borrow(&user, &1u128);
    assert_eq!(v.get_user_borrow_balance(&user), 1u128);
}

#[test]
fn test_oracle_decimals_normalization() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Token + market
    let t_admin = Address::generate(&env);
    let t = env
        .register_stellar_asset_contract_v2(t_admin.clone())
        .address();
    let v_id = env.register(rv::ReceiptVault, ());
    let v = rv::ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&t, &0u128, &0u128, &admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&v_id);
    comp.set_market_cf(&v_id, &1_000_000u128);
    comp.enter_market(&user, &v_id);
    v.set_peridottroller(&comp_id);

    // Oracle with 8 decimals
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&8u32);
    // Price 1.00 with 8 decimals = 100_000_000
    oracle.set_price(&t, &100_000_000i128);
    comp.set_oracle(&oracle_id);

    // Fund
    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&user, &1_000i128);
    v.set_collateral_factor(&1_000_000u128);
    v.deposit(&user, &200u128);

    // Add some extra liquidity (not binding vs collateral)
    mint.mint(&admin, &1_000i128);
    v.deposit(&admin, &100u128);

    // Preview borrow max should equal collateral capacity (200) given available >= 200
    let max_borrow = comp.preview_borrow_max(&user, &v_id);
    assert_eq!(max_borrow, 200u128);

    // Note: MockOracle.lastprice timestamps at call time, so staleness is exercised in integration tests.
}

#[test]
#[should_panic(expected = "no peridottroller")]
fn test_vault_repay_on_behalf_requires_peridottroller() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let t_admin = Address::generate(&env);
    let t = env
        .register_stellar_asset_contract_v2(t_admin.clone())
        .address();
    let v_id = env.register(rv::ReceiptVault, ());
    let v = rv::ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&t, &0u128, &0u128, &admin);

    // Fund and create debt without wiring peridottroller
    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&borrower, &1_000i128);
    v.set_collateral_factor(&1_000_000u128);
    v.deposit(&borrower, &100u128);
    v.borrow(&borrower, &50u128);

    // Liquidator tries to call repay_on_behalf directly -> should panic "no peridottroller"
    v.repay_on_behalf(&liquidator, &borrower, &10u128);
}

#[test]
#[should_panic(expected = "no peridottroller")]
fn test_vault_seize_requires_peridottroller() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let t_admin = Address::generate(&env);
    let t = env
        .register_stellar_asset_contract_v2(t_admin.clone())
        .address();
    let v_id = env.register(rv::ReceiptVault, ());
    let v = rv::ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&t, &0u128, &0u128, &admin);

    // Give borrower some pTokens
    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&borrower, &1_000i128);
    v.deposit(&borrower, &100u128);

    // Direct seize without peridottroller wired -> should panic
    v.seize(&borrower, &liquidator, &10u128);
}

#[test]
fn test_rewards_accrual_and_claim() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Token + market
    let t_admin = Address::generate(&env);
    let t = env
        .register_stellar_asset_contract_v2(t_admin.clone())
        .address();
    let v_id = env.register(rv::ReceiptVault, ());
    let v = rv::ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&t, &0u128, &0u128, &admin);

    // Comptroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&v_id);
    comp.set_market_cf(&v_id, &1_000_000u128);
    comp.enter_market(&user, &v_id);
    v.set_peridottroller(&comp_id);

    // Oracle with $1 so previews work
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&t, &1_000_000i128);
    comp.set_oracle(&oracle_id);

    // PERI token under comptroller admin
    use peridot_token as pt;
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &comp_id,
    );
    comp.set_peridot_token(&peri_id);

    // Supply reward speed = 10 PERI/sec
    comp.set_supply_speed(&v_id, &10u128);

    // Fund and deposit
    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&user, &1_000i128);
    v.deposit(&user, &100u128);

    // Advance 5 seconds (accrual indexes evolve)
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 5);

    // Claim
    comp.claim(&user);
    // Expect minted PERI ~ 10 * 5 = 50 to user
    assert_eq!(peri.balance_of(&user), 50i128);
}

#[test]
fn test_borrow_side_rewards_and_claim() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let lender = Address::generate(&env);

    // Tokens + markets (A=borrow, B=collateral)
    let ta_admin = Address::generate(&env);
    let tb_admin = Address::generate(&env);
    let ta = env
        .register_stellar_asset_contract_v2(ta_admin.clone())
        .address();
    let tb = env
        .register_stellar_asset_contract_v2(tb_admin.clone())
        .address();
    let va_id = env.register(rv::ReceiptVault, ());
    let va = rv::ReceiptVaultClient::new(&env, &va_id);
    let vb_id = env.register(rv::ReceiptVault, ());
    let vb = rv::ReceiptVaultClient::new(&env, &vb_id);
    va.initialize(&ta, &0u128, &0u128, &admin);
    vb.initialize(&tb, &0u128, &0u128, &admin);

    // Comptroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.enter_market(&borrower, &va_id);
    comp.enter_market(&borrower, &vb_id);
    va.set_peridottroller(&comp_id);
    vb.set_peridottroller(&comp_id);

    // Oracle $1
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&ta, &1_000_000i128);
    oracle.set_price(&tb, &1_000_000i128);
    comp.set_oracle(&oracle_id);

    // PERI token
    use peridot_token as pt;
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &comp_id,
    );
    comp.set_peridot_token(&peri_id);

    // Borrow speed 7 P/sec on market A
    comp.set_borrow_speed(&va_id, &7u128);

    // Fund lender liquidity and borrower collateral; CF=100%
    let mint_a = token::StellarAssetClient::new(&env, &ta);
    let mint_b = token::StellarAssetClient::new(&env, &tb);
    mint_a.mint(&lender, &1_000i128);
    mint_b.mint(&borrower, &1_000i128);
    vb.set_collateral_factor(&1_000_000u128);
    va.deposit(&lender, &300u128); // liquidity in A
    vb.deposit(&borrower, &100u128); // collateral in B

    // Borrow 50 -> single borrower => all borrow rewards to borrower
    va.borrow(&borrower, &50u128);

    // Advance 6s and claim
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 6);
    comp.claim(&borrower);
    assert_eq!(peri.balance_of(&borrower), 42i128); // 7*6
}

#[test]
fn test_multi_market_supply_rewards() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Two tokens and markets
    let ta_admin = Address::generate(&env);
    let tb_admin = Address::generate(&env);
    let ta = env
        .register_stellar_asset_contract_v2(ta_admin.clone())
        .address();
    let tb = env
        .register_stellar_asset_contract_v2(tb_admin.clone())
        .address();
    let va_id = env.register(rv::ReceiptVault, ());
    let va = rv::ReceiptVaultClient::new(&env, &va_id);
    let vb_id = env.register(rv::ReceiptVault, ());
    let vb = rv::ReceiptVaultClient::new(&env, &vb_id);
    va.initialize(&ta, &0u128, &0u128, &admin);
    vb.initialize(&tb, &0u128, &0u128, &admin);

    // Comptroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.enter_market(&user, &va_id);
    comp.enter_market(&user, &vb_id);
    va.set_peridottroller(&comp_id);
    vb.set_peridottroller(&comp_id);

    // Oracle $1 for both
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&ta, &1_000_000i128);
    oracle.set_price(&tb, &1_000_000i128);
    comp.set_oracle(&oracle_id);

    // PERI token
    use peridot_token as pt;
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &comp_id,
    );
    comp.set_peridot_token(&peri_id);

    // Speeds: A=5, B=3 P/sec
    comp.set_supply_speed(&va_id, &5u128);
    comp.set_supply_speed(&vb_id, &3u128);

    // Fund and deposit in both
    let mint_a = token::StellarAssetClient::new(&env, &ta);
    let mint_b = token::StellarAssetClient::new(&env, &tb);
    mint_a.mint(&user, &1_000i128);
    mint_b.mint(&user, &1_000i128);
    va.deposit(&user, &100u128);
    vb.deposit(&user, &200u128);

    // Advance 4s
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 4);

    // Claim -> expect (5+3)*4 = 32 P
    comp.claim(&user);
    assert_eq!(peri.balance_of(&user), 32i128);
}
