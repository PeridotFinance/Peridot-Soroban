#![cfg(test)]
use super::*;
use peridot_token as pt;
use receipt_vault as rv;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::token;
use soroban_sdk::BytesN;
use soroban_sdk::{contract, contractimpl, contracttype};
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, Map, String, Symbol, Vec};

fn set_price_and_cache(
    comp: &SimplePeridottrollerClient,
    oracle: &MockOracleClient,
    oracle_id: &Address,
    token: &Address,
    price: i128,
) {
    comp.set_oracle(oracle_id);
    oracle.set_price(token, &price);
    comp.cache_price(token);
}

#[test]
fn test_peridottroller_add_and_enter_market() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

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
    market_vault.enable_static_rates(&admin);

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
    client.set_pause_borrow(&market_vault_id, &true);
    client.set_pause_deposit(&market_vault_id, &true);
    client.verify_market_zero_totals(&market_vault_id);
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
fn test_exit_market_non_entered_returns_without_external_calls() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    // BrokenMarket panics on pToken/debt reads. If exit_market performs external calls
    // before membership validation, this test would panic.
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let broken_market_id = env.register(BrokenMarket, ());
    let broken_market = BrokenMarketClient::new(&env, &broken_market_id);
    broken_market.initialize(&token);

    // User has not entered this market: exit is a no-op.
    comp.exit_market(&user, &broken_market_id);
    let markets = comp.get_user_markets(&user);
    assert_eq!(markets.len(), 0);
}

#[test]
#[should_panic(expected = "market not supported")]
fn test_exit_market_rejects_entered_but_unsupported_market() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let unsupported_market = Address::generate(&env);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    // Simulate legacy/corrupt state where user list contains an unsupported market.
    env.as_contract(&comp_id, || {
        let mut entered = Vec::new(&env);
        entered.push_back(unsupported_market.clone());
        env.storage()
            .persistent()
            .set(&DataKey::UserMarkets(user.clone()), &entered);
    });

    comp.exit_market(&user, &unsupported_market);
}

#[test]
#[should_panic(expected = "too many entered markets")]
fn test_enter_market_enforces_max_user_markets() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let id = env.register(SimplePeridottroller, ());
    let client = SimplePeridottrollerClient::new(&env, &id);
    client.initialize(&admin);

    // Entering MAX_USER_MARKETS distinct markets is allowed.
    for _ in 0..MAX_USER_MARKETS {
        let token_admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let vault_id = env.register(rv::ReceiptVault, ());
        let vault = rv::ReceiptVaultClient::new(&env, &vault_id);
        vault.initialize(&token, &0u128, &0u128, &admin);
        vault.enable_static_rates(&admin);
        client.add_market(&vault_id);
        client.enter_market(&user, &vault_id);
    }

    // The next distinct market must be rejected.
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let extra_vault_id = env.register(rv::ReceiptVault, ());
    let extra_vault = rv::ReceiptVaultClient::new(&env, &extra_vault_id);
    extra_vault.initialize(&token, &0u128, &0u128, &admin);
    extra_vault.enable_static_rates(&admin);
    client.add_market(&extra_vault_id);
    client.enter_market(&user, &extra_vault_id);
}

#[test]
#[should_panic(expected = "market has active users")]
fn test_remove_market_rejects_active_positions() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let market_vault_id = env.register(rv::ReceiptVault, ());
    let market_vault = rv::ReceiptVaultClient::new(&env, &market_vault_id);
    market_vault.initialize(&token, &0u128, &0u128, &admin);
    market_vault.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_vault_id);
    comp.enter_market(&user, &market_vault_id);

    // Leave an active supply position in the market.
    let mint = token::StellarAssetClient::new(&env, &token);
    mint.mint(&user, &1_000i128);
    market_vault.deposit(&user, &100u128);

    comp.remove_market(&market_vault_id);
}

#[test]
#[should_panic(expected = "market has active positions")]
fn test_remove_market_rejects_non_entered_supplier_state() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let market_vault_id = env.register(rv::ReceiptVault, ());
    let market_vault = rv::ReceiptVaultClient::new(&env, &market_vault_id);
    market_vault.initialize(&token, &0u128, &0u128, &admin);
    market_vault.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_vault_id);

    // Supplier never enters the market but still has pTokens.
    let mint = token::StellarAssetClient::new(&env, &token);
    mint.mint(&user, &1_000i128);
    market_vault.deposit(&user, &100u128);

    comp.set_pause_borrow(&market_vault_id, &true);
    comp.set_pause_deposit(&market_vault_id, &true);
    comp.verify_market_zero_totals(&market_vault_id);
}

#[test]
#[should_panic(expected = "market state unavailable")]
fn test_remove_market_fails_closed_when_market_state_unavailable() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let failing_market_id = env.register(FailingClaimMarket, ());
    let failing_market = FailingClaimMarketClient::new(&env, &failing_market_id);
    failing_market.initialize(&token);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&failing_market_id);

    comp.set_pause_borrow(&failing_market_id, &true);
    comp.set_pause_deposit(&failing_market_id, &true);
    comp.verify_market_zero_totals(&failing_market_id);
}

#[test]
#[should_panic(expected = "ack required")]
fn test_force_remove_market_requires_ack() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let failing_market_id = env.register(FailingClaimMarket, ());
    let failing_market = FailingClaimMarketClient::new(&env, &failing_market_id);
    failing_market.initialize(&token);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&failing_market_id);

    comp.force_remove_market(&failing_market_id, &token, &0u128, &0u128, &false);
}

#[test]
fn test_force_remove_market_allows_delist_when_market_state_unavailable() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let delist_market_id = env.register(DelistMarket, ());
    let delist_market = DelistMarketClient::new(&env, &delist_market_id);
    delist_market.initialize(&token);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&delist_market_id);
    comp.set_pause_borrow(&delist_market_id, &true);
    comp.set_pause_deposit(&delist_market_id, &true);
    comp.verify_market_zero_totals(&delist_market_id);
    delist_market.set_fail_underlying(&true);

    // Delist succeeds via emergency path even though the market state endpoint traps.
    comp.force_remove_market(&delist_market_id, &token, &0u128, &0u128, &true);

    // Verifies market is no longer supported.
    let enter_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        comp.enter_market(&user, &delist_market_id);
    }));
    assert!(enter_res.is_err());
}

#[test]
#[should_panic(expected = "expected active positions")]
fn test_force_remove_market_requires_zero_expected_totals() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let failing_market_id = env.register(FailingClaimMarket, ());
    let failing_market = FailingClaimMarketClient::new(&env, &failing_market_id);
    failing_market.initialize(&token);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&failing_market_id);

    comp.force_remove_market(&failing_market_id, &token, &1u128, &0u128, &true);
}

#[test]
fn test_force_remove_market_does_not_call_market_for_underlying() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let delist_market_id = env.register(DelistMarket, ());
    let delist_market = DelistMarketClient::new(&env, &delist_market_id);
    delist_market.initialize(&token);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&delist_market_id);
    comp.set_pause_borrow(&delist_market_id, &true);
    comp.set_pause_deposit(&delist_market_id, &true);
    comp.verify_market_zero_totals(&delist_market_id);

    // Break underlying getter after listing; emergency delist should still succeed
    // because it uses cached/supplied token, not market calls.
    delist_market.set_fail_underlying(&true);
    comp.force_remove_market(&delist_market_id, &token, &0u128, &0u128, &true);
}

#[test]
#[should_panic(expected = "removed token mismatch")]
fn test_force_remove_market_rejects_removed_token_mismatch() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin_a = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    let delist_market_id = env.register(DelistMarket, ());
    let delist_market = DelistMarketClient::new(&env, &delist_market_id);
    delist_market.initialize(&token_a);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&delist_market_id);
    comp.set_pause_borrow(&delist_market_id, &true);
    comp.set_pause_deposit(&delist_market_id, &true);
    comp.verify_market_zero_totals(&delist_market_id);

    // Cached market->underlying mapping is token_a, so token_b must be rejected.
    comp.force_remove_market(&delist_market_id, &token_b, &0u128, &0u128, &true);
}

#[test]
#[should_panic(expected = "missing zero-totals proof")]
fn test_force_remove_market_requires_zero_totals_proof() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let delist_market_id = env.register(DelistMarket, ());
    let delist_market = DelistMarketClient::new(&env, &delist_market_id);
    delist_market.initialize(&token);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&delist_market_id);
    comp.set_pause_borrow(&delist_market_id, &true);
    comp.set_pause_deposit(&delist_market_id, &true);

    comp.force_remove_market(&delist_market_id, &token, &0u128, &0u128, &true);
}

#[test]
#[should_panic(expected = "stale zero-totals proof")]
fn test_force_remove_market_requires_fresh_zero_totals_proof() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let delist_market_id = env.register(DelistMarket, ());
    let delist_market = DelistMarketClient::new(&env, &delist_market_id);
    delist_market.initialize(&token);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&delist_market_id);
    comp.set_pause_borrow(&delist_market_id, &true);
    comp.set_pause_deposit(&delist_market_id, &true);
    comp.verify_market_zero_totals(&delist_market_id);

    let now = env.ledger().timestamp();
    env.ledger()
        .set_timestamp(now + FORCE_REMOVE_ZERO_TOTALS_MAX_AGE_SECS + 1);
    comp.force_remove_market(&delist_market_id, &token, &0u128, &0u128, &true);
}

#[test]
fn test_remove_market_does_not_block_on_unavailable_remaining_market_underlying() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin_a = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    let market_a_id = env.register(rv::ReceiptVault, ());
    let market_a = rv::ReceiptVaultClient::new(&env, &market_a_id);
    market_a.initialize(&token_a, &0u128, &0u128, &admin);
    market_a.enable_static_rates(&admin);

    let market_b_id = env.register(FailingClaimMarket, ());
    let market_b = FailingClaimMarketClient::new(&env, &market_b_id);
    market_b.initialize(&token_b);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_a_id);
    comp.add_market(&market_b_id);

    // Simulate a legacy deployment where remaining market cache is missing.
    env.as_contract(&comp_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::MarketUnderlying(market_b_id.clone()));
    });
    market_b.set_fail_underlying(&true);

    // Must still delist market_a without being blocked by market_b read failures.
    comp.set_pause_borrow(&market_a_id, &true);
    comp.set_pause_deposit(&market_a_id, &true);
    comp.verify_market_zero_totals(&market_a_id);
    comp.remove_market(&market_a_id);

    // Removed market is no longer supported.
    let removed_enter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        comp.enter_market(&user, &market_a_id);
    }));
    assert!(removed_enter.is_err());

    // Remaining market stays supported.
    comp.enter_market(&user, &market_b_id);
}

#[test]
fn test_total_collateral_and_borrows_across_markets() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

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
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

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
    comp.set_market_cf(&vault_a_id, &1_000_000u128);
    comp.set_market_cf(&vault_b_id, &1_000_000u128);

    // Oracle with USD prices for both assets
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128);
    comp.set_oracle(&oracle_id);

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

#[test]
fn test_unconfigured_market_cf_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let market_id = env.register(rv::ReceiptVault, ());
    let market = rv::ReceiptVaultClient::new(&env, &market_id);
    market.initialize(&token, &0u128, &0u128, &admin);
    market.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_id);

    assert_eq!(comp.get_market_cf(&market_id), 0u128);
}

#[test]
fn test_removed_market_not_counted_as_collateral() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let lender = Address::generate(&env);

    let token_a = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let token_b = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.set_market_cf(&vault_b_id, &500_000u128);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);
    vault_b.set_collateral_factor(&500_000u128);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    let mint_a = token::StellarAssetClient::new(&env, &token_a);
    let mint_b = token::StellarAssetClient::new(&env, &token_b);
    mint_b.mint(&borrower, &200i128);
    mint_a.mint(&lender, &300i128);

    vault_b.deposit(&borrower, &200u128);
    vault_a.deposit(&lender, &300u128);
    vault_a.borrow(&borrower, &50u128);

    let (liq_before, shortfall_before) = comp.account_liquidity(&borrower);
    assert_eq!(liq_before, 50u128);
    assert_eq!(shortfall_before, 0u128);

    // Simulate a stale UserMarkets entry for an unsupported market (e.g., legacy state)
    // by removing the market from SupportedMarkets directly in storage.
    env.as_contract(&comp_id, || {
        let mut markets: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::SupportedMarkets)
            .unwrap_or(Map::new(&env));
        markets.remove(vault_b_id.clone());
        env.storage()
            .persistent()
            .set(&DataKey::SupportedMarkets, &markets);
    });

    // Removed collateral market is ignored in risk checks.
    let (liq_after, shortfall_after) = comp.account_liquidity(&borrower);
    assert_eq!(liq_after, 0u128);
    assert_eq!(shortfall_after, 50u128);
    assert_eq!(comp.get_user_total_collateral(&borrower), 0u128);
}

#[test]
fn test_get_collateral_excl_applies_market_cf() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let exclude_market = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let market_id = env.register(rv::ReceiptVault, ());
    let market = rv::ReceiptVaultClient::new(&env, &market_id);
    market.initialize(&token, &0u128, &0u128, &admin);
    market.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_id);
    comp.enter_market(&user, &market_id);
    comp.set_market_cf(&market_id, &500_000u128); // 50%

    let mint = token::StellarAssetClient::new(&env, &token);
    mint.mint(&user, &1_000i128);
    market.deposit(&user, &200u128); // 200 underlying collateral

    // get_collateral_excl returns CF-discounted underlying (not raw underlying).
    assert_eq!(comp.get_collateral_excl(&user, &exclude_market), 100u128);
}

#[test]
fn test_get_borrows_excl_accrues_interest_before_reading_debt() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let exclude_market = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let stale_market_id = env.register(StaleBorrowMarket, ());
    let stale_market = StaleBorrowMarketClient::new(&env, &stale_market_id);
    stale_market.initialize(&token);
    stale_market.set_debt(&user, &123u128);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&stale_market_id);
    comp.enter_market(&user, &stale_market_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token, 1_000_000i128); // $1.00

    // get_borrows_excl should call update_interest first, then read debt.
    assert_eq!(comp.get_borrows_excl(&user, &exclude_market), 123u128);
    assert!(stale_market.was_updated());
}

#[test]
fn test_get_borrows_excl_counts_debt_from_unsupported_entered_market() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let exclude_market = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let stale_market_id = env.register(StaleBorrowMarket, ());
    let stale_market = StaleBorrowMarketClient::new(&env, &stale_market_id);
    stale_market.initialize(&token);
    stale_market.set_debt(&user, &50u128);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&stale_market_id);
    comp.enter_market(&user, &stale_market_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token, 1_000_000i128); // $1.00

    // Simulate delist/misconfiguration: market removed from SupportedMarkets
    // but still present in UserMarkets with outstanding debt.
    env.as_contract(&comp_id, || {
        let mut markets: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::SupportedMarkets)
            .unwrap_or(Map::new(&env));
        markets.remove(stale_market_id.clone());
        env.storage()
            .persistent()
            .set(&DataKey::SupportedMarkets, &markets);
    });

    // Debt from the unsupported-but-entered market must still be counted.
    assert_eq!(comp.get_borrows_excl(&user, &exclude_market), 50u128);
}

#[test]
fn test_get_borrows_excl_fails_closed_when_market_unavailable() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let exclude_market = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let broken_id = env.register(BrokenMarket, ());
    let broken_market = BrokenMarketClient::new(&env, &broken_id);
    broken_market.initialize(&token);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&broken_id);
    comp.enter_market(&user, &broken_id);

    // Do not trap when a market in UserMarkets is unavailable; return fail-closed debt.
    assert_eq!(comp.get_borrows_excl(&user, &exclude_market), u128::MAX);
}

#[test]
fn test_account_liquidity_not_poisoned_by_pbal_read_failure_without_debt() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let fail_market_id = env.register(PbalReadFailMarket, ());
    let fail_market = PbalReadFailMarketClient::new(&env, &fail_market_id);
    fail_market.initialize(&token);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&fail_market_id);
    comp.enter_market(&user, &fail_market_id);

    // pToken read failure with zero debt should not force synthetic shortfall.
    let (liq, shortfall) = comp.account_liquidity(&user);
    assert_eq!(liq, 0u128);
    assert_eq!(shortfall, 0u128);
}

#[test]
fn test_account_liquidity_marks_indeterminate_when_underlying_unavailable_with_debt() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let failing_market_id = env.register(FailingClaimMarket, ());
    let failing_market = FailingClaimMarketClient::new(&env, &failing_market_id);
    failing_market.initialize(&token);
    failing_market.set_debt(&user, &25u128);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&failing_market_id);
    comp.enter_market(&user, &failing_market_id);

    // Market becomes unreadable for underlying token after entry.
    failing_market.set_fail_underlying(&true);

    let (liq, shortfall) = comp.account_liquidity(&user);
    assert_eq!(liq, 0u128);
    assert_eq!(shortfall, u128::MAX);
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

#[contract]
struct FailingClaimMarket;

#[contracttype]
enum FailingMarketKey {
    Underlying,
    FailUnderlying,
    Debt(Address),
}

#[contractimpl]
impl FailingClaimMarket {
    pub fn initialize(env: Env, underlying: Address) {
        env.storage()
            .persistent()
            .set(&FailingMarketKey::Underlying, &underlying);
        env.storage()
            .persistent()
            .set(&FailingMarketKey::FailUnderlying, &false);
    }

    pub fn set_fail_underlying(env: Env, fail: bool) {
        env.storage()
            .persistent()
            .set(&FailingMarketKey::FailUnderlying, &fail);
    }

    pub fn set_debt(env: Env, user: Address, debt: u128) {
        env.storage()
            .persistent()
            .set(&FailingMarketKey::Debt(user), &debt);
    }

    pub fn get_underlying_token(env: Env) -> Address {
        let fail = env
            .storage()
            .persistent()
            .get(&FailingMarketKey::FailUnderlying)
            .unwrap_or(false);
        if fail {
            panic!("underlying unavailable");
        }
        env.storage()
            .persistent()
            .get(&FailingMarketKey::Underlying)
            .expect("underlying not set")
    }

    pub fn get_total_ptokens(_env: Env) -> u128 {
        panic!("market unavailable");
    }

    pub fn get_total_borrowed(_env: Env) -> u128 {
        0u128
    }

    pub fn get_ptoken_balance(_env: Env, _user: Address) -> u128 {
        0u128
    }

    pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&FailingMarketKey::Debt(user))
            .unwrap_or(0u128)
    }
}

#[contract]
struct DelistMarket;

#[contracttype]
enum DelistMarketKey {
    Underlying,
    FailUnderlying,
}

#[contractimpl]
impl DelistMarket {
    pub fn initialize(env: Env, underlying: Address) {
        env.storage()
            .persistent()
            .set(&DelistMarketKey::Underlying, &underlying);
        env.storage()
            .persistent()
            .set(&DelistMarketKey::FailUnderlying, &false);
    }

    pub fn set_fail_underlying(env: Env, fail: bool) {
        env.storage()
            .persistent()
            .set(&DelistMarketKey::FailUnderlying, &fail);
    }

    pub fn get_underlying_token(env: Env) -> Address {
        let fail = env
            .storage()
            .persistent()
            .get(&DelistMarketKey::FailUnderlying)
            .unwrap_or(false);
        if fail {
            panic!("underlying unavailable");
        }
        env.storage()
            .persistent()
            .get(&DelistMarketKey::Underlying)
            .expect("underlying not set")
    }

    pub fn get_total_ptokens(_env: Env) -> u128 {
        0u128
    }

    pub fn get_total_borrowed(_env: Env) -> u128 {
        0u128
    }
}

#[contract]
struct FailingPeridotToken;

#[contractimpl]
impl FailingPeridotToken {
    pub fn mint(_env: Env, _to: Address, _amount: i128) {
        panic!("mint unavailable");
    }
}

// BrokenMarket simulates a market whose storage TTL has expired (FIND-039 PoC)
// It was healthy when added (get_underlying_token works), then expired (all other calls panic)
#[contract]
struct BrokenMarket;

#[contracttype]
enum BrokenMarketKey {
    Token,
}

#[contractimpl]
impl BrokenMarket {
    pub fn initialize(env: Env, token: Address) {
        env.storage()
            .instance()
            .set(&BrokenMarketKey::Token, &token);
    }

    pub fn get_underlying_token(env: Env) -> Address {
        // This works (market was healthy when added to peridottroller)
        env.storage()
            .instance()
            .get(&BrokenMarketKey::Token)
            .expect("token not set")
    }

    pub fn get_ptoken_balance(_env: Env, _user: Address) -> u128 {
        // Storage expired - simulate missing key panic
        panic!("storage: missing value for key");
    }

    pub fn get_user_borrow_balance(_env: Env, _user: Address) -> u128 {
        // Storage expired - simulate missing key panic
        panic!("storage: missing value for key");
    }

    pub fn get_exchange_rate(_env: Env) -> u128 {
        // Storage expired - simulate missing key panic
        panic!("storage: missing value for key");
    }
}

#[contract]
struct PbalReadFailMarket;

#[contracttype]
enum PbalReadFailMarketKey {
    Token,
}

#[contractimpl]
impl PbalReadFailMarket {
    pub fn initialize(env: Env, token: Address) {
        env.storage()
            .instance()
            .set(&PbalReadFailMarketKey::Token, &token);
    }

    pub fn get_underlying_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&PbalReadFailMarketKey::Token)
            .expect("token not set")
    }

    pub fn get_ptoken_balance(_env: Env, _user: Address) -> u128 {
        panic!("ptoken read unavailable");
    }

    pub fn get_user_borrow_balance(_env: Env, _user: Address) -> u128 {
        0u128
    }

    pub fn get_exchange_rate(_env: Env) -> u128 {
        1_000_000u128
    }
}

#[contract]
struct StaleBorrowMarket;

#[contracttype]
enum StaleBorrowMarketKey {
    Underlying,
    Updated,
    Debt(Address),
}

#[contractimpl]
impl StaleBorrowMarket {
    pub fn initialize(env: Env, underlying: Address) {
        env.storage()
            .persistent()
            .set(&StaleBorrowMarketKey::Underlying, &underlying);
        env.storage()
            .persistent()
            .set(&StaleBorrowMarketKey::Updated, &false);
    }

    pub fn set_debt(env: Env, user: Address, debt: u128) {
        env.storage()
            .persistent()
            .set(&StaleBorrowMarketKey::Debt(user), &debt);
        env.storage()
            .persistent()
            .set(&StaleBorrowMarketKey::Updated, &false);
    }

    pub fn get_underlying_token(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&StaleBorrowMarketKey::Underlying)
            .expect("underlying not set")
    }

    pub fn update_interest(env: Env) {
        env.storage()
            .persistent()
            .set(&StaleBorrowMarketKey::Updated, &true);
    }

    pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {
        let debt: u128 = env
            .storage()
            .persistent()
            .get(&StaleBorrowMarketKey::Debt(user))
            .unwrap_or(0u128);
        let updated = env
            .storage()
            .persistent()
            .get(&StaleBorrowMarketKey::Updated)
            .unwrap_or(false);
        if !updated {
            // Simulate stale accounting that under-reports until update_interest is called.
            return debt.saturating_sub(1);
        }
        debt
    }

    pub fn was_updated(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&StaleBorrowMarketKey::Updated)
            .unwrap_or(false)
    }
}

#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_oracle_gating_prevents_over_borrow() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

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
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

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
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 100_000i128);
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
    env.mock_all_auths_allowing_non_root_auth();

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
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

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
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 100_000i128);
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
    vault.enable_static_rates(&admin);

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
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
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
    vault.enable_static_rates(&admin);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&user, &vault_id);
    comp.enter_market(&user, &vault_b_id);
    comp.set_market_cf(&vault_b_id, &1_000_000u128);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    // token_a $1, token_b $1
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128);
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
    env.mock_all_auths_allowing_non_root_auth();

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
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);
    comp.set_market_cf(&vault_b_id, &500_000u128);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    // token_a $1, token_b $1
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128);
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
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 500_000i128); // $0.50

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
#[should_panic(expected = "collateral market not entered")]
fn test_liquidate_rejects_non_entered_collateral() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let t_admin_a = Address::generate(&env);
    let t_a = env
        .register_stellar_asset_contract_v2(t_admin_a.clone())
        .address();
    let t_admin_b = Address::generate(&env);
    let t_b = env
        .register_stellar_asset_contract_v2(t_admin_b.clone())
        .address();
    let t_admin_c = Address::generate(&env);
    let t_c = env
        .register_stellar_asset_contract_v2(t_admin_c.clone())
        .address();

    let va_id = env.register(rv::ReceiptVault, ());
    let va = rv::ReceiptVaultClient::new(&env, &va_id);
    let vb_id = env.register(rv::ReceiptVault, ());
    let vb = rv::ReceiptVaultClient::new(&env, &vb_id);
    let vc_id = env.register(rv::ReceiptVault, ());
    let vc = rv::ReceiptVaultClient::new(&env, &vc_id);

    va.initialize(&t_a, &0u128, &0u128, &admin);
    va.enable_static_rates(&admin);
    vb.initialize(&t_b, &0u128, &0u128, &admin);
    vb.enable_static_rates(&admin);
    vc.initialize(&t_c, &0u128, &0u128, &admin);
    vc.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.add_market(&vc_id);
    comp.enter_market(&borrower, &va_id);
    comp.enter_market(&borrower, &vb_id);
    va.set_peridottroller(&comp_id);
    vb.set_peridottroller(&comp_id);
    vc.set_peridottroller(&comp_id);
    comp.set_market_cf(&vb_id, &500_000u128);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_b, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_c, 1_000_000i128);

    let mint_a = token::StellarAssetClient::new(&env, &t_a);
    let mint_b = token::StellarAssetClient::new(&env, &t_b);
    let mint_c = token::StellarAssetClient::new(&env, &t_c);
    mint_a.mint(&admin, &1_000i128);
    mint_b.mint(&borrower, &200i128);
    mint_c.mint(&borrower, &200i128);

    va.deposit(&admin, &1_000u128);
    vb.deposit(&borrower, &100u128);
    vc.deposit(&borrower, &100u128); // not entered

    va.borrow(&borrower, &50u128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_b, 500_000i128);

    comp.liquidate(&borrower, &va_id, &vc_id, &25u128, &liquidator);
}

#[test]
fn test_preview_redeem_max_non_entered_market_allows_full_redeem() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let t_admin_a = Address::generate(&env);
    let t_a = env
        .register_stellar_asset_contract_v2(t_admin_a.clone())
        .address();
    let t_admin_c = Address::generate(&env);
    let t_c = env
        .register_stellar_asset_contract_v2(t_admin_c.clone())
        .address();

    let va_id = env.register(rv::ReceiptVault, ());
    let va = rv::ReceiptVaultClient::new(&env, &va_id);
    let vc_id = env.register(rv::ReceiptVault, ());
    let vc = rv::ReceiptVaultClient::new(&env, &vc_id);
    va.initialize(&t_a, &0u128, &0u128, &admin);
    va.enable_static_rates(&admin);
    vc.initialize(&t_c, &0u128, &0u128, &admin);
    vc.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vc_id);
    comp.enter_market(&user, &va_id);
    va.set_peridottroller(&comp_id);
    vc.set_peridottroller(&comp_id);
    comp.set_market_cf(&va_id, &500_000u128);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_c, 1_000_000i128);

    let mint_a = token::StellarAssetClient::new(&env, &t_a);
    let mint_c = token::StellarAssetClient::new(&env, &t_c);
    mint_a.mint(&admin, &1_000i128);
    mint_a.mint(&user, &200i128);
    mint_c.mint(&user, &200i128);

    va.deposit(&admin, &1_000u128);
    va.deposit(&user, &200u128);
    vc.deposit(&user, &100u128); // not entered
    va.borrow(&user, &50u128);

    let pbal: u128 = env.invoke_contract(
        &vc_id,
        &Symbol::new(&env, "get_ptoken_balance"),
        (user.clone(),).into_val(&env),
    );
    let redeem_max = comp.preview_redeem_max(&user, &vc_id);
    assert_eq!(redeem_max, pbal);
}

#[test]
fn test_repay_on_behalf_for_liquidator() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &token);
    let token_admin_client = token::StellarAssetClient::new(&env, &token);

    let vault_id = env.register(rv::ReceiptVault, ());
    let vault = rv::ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    vault.set_peridottroller(&comp_id);
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.enter_market(&borrower, &vault_id);
    comp.set_market_cf(&vault_id, &500_000u128);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token, 1_000_000i128);

    token_admin_client.mint(&admin, &1_000i128);
    vault.deposit(&admin, &1_000u128);
    token_admin_client.mint(&borrower, &200i128);
    vault.deposit(&borrower, &200u128);

    vault.borrow(&borrower, &100u128);
    assert_eq!(vault.get_user_borrow_balance(&borrower), 100u128);

    token_admin_client.mint(&liquidator, &200i128);
    comp.repay_on_behalf_for_liquidator(&borrower, &vault_id, &40u128, &liquidator);

    assert_eq!(vault.get_user_borrow_balance(&borrower), 60u128);
    assert_eq!(token_client.balance(&liquidator), 160i128);
}

#[test]
fn test_liquidation_capped_by_close_factor() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

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
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);
    comp.set_market_cf(&vault_b_id, &500_000u128);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128); // $1
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128); // $1
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
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 400_000i128);

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
fn test_liquidation_succeeds_when_post_repay_redeem_preview_exceeds_seize() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let lender = Address::generate(&env);

    // token_a = borrow asset, token_b = collateral asset
    let ta_admin = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(ta_admin.clone())
        .address();
    let tb_admin = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(tb_admin.clone())
        .address();

    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);
    comp.set_market_cf(&vault_b_id, &500_000u128);
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128); // $1
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128); // $1
    comp.set_oracle(&oracle_id);

    let mint_a = token::StellarAssetClient::new(&env, &token_a);
    let mint_b = token::StellarAssetClient::new(&env, &token_b);
    mint_a.mint(&lender, &500i128);
    mint_b.mint(&borrower, &500i128);
    mint_a.mint(&liquidator, &200i128);

    vault_b.set_collateral_factor(&500_000u128); // 50%
    vault_a.deposit(&lender, &200u128);
    vault_b.deposit(&borrower, &320u128);
    vault_a.borrow(&borrower, &100u128);

    // Price drop creates shortfall.
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 500_000i128); // $0.50
    let (_liq, shortfall) = comp.account_liquidity(&borrower);
    assert!(shortfall > 0);

    // With close-factor-capped repay (50), seize should proceed even though
    // post-repay redeem preview may exceed seize amount.
    comp.liquidate(&borrower, &vault_a_id, &vault_b_id, &100u128, &liquidator);

    assert_eq!(vault_a.get_user_borrow_balance(&borrower), 50u128);
    assert_eq!(vault_b.get_ptoken_balance(&liquidator), 108u128);
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
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);
    comp.set_market_cf(&vault_b_id, &500_000u128);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128);
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
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);
    comp.set_market_cf(&vault_b_id, &500_000u128);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128); // start healthy to allow borrow
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
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 500_000i128);
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
    vault.enable_static_rates(&admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.enter_market(&user, &vault_id);
    comp.set_market_cf(&vault_id, &500_000u128);

    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token, 1_000_000i128); // $1
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
    va.enable_static_rates(&admin);
    vb.initialize(&t_b, &0u128, &0u128, &admin);
    vb.enable_static_rates(&admin);

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
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_b, 1_000_000i128);
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
    assert_eq!(c.get_admin(), admin);
    c.accept_admin();
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
    vault.enable_static_rates(&admin);
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.enter_market(&user, &vault_id);
    vault.set_peridottroller(&comp_id);
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token, 1_000_000i128);
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
    vault.enable_static_rates(&admin);
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.enter_market(&user, &vault_id);
    vault.set_peridottroller(&comp_id);
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token, 1_000_000i128);
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
    va.enable_static_rates(&admin);
    vb.initialize(&t_b, &0u128, &0u128, &admin);
    vb.enable_static_rates(&admin);
    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.enter_market(&user, &va_id);
    comp.enter_market(&user, &vb_id);
    comp.set_market_cf(&vb_id, &500_000u128);
    va.set_peridottroller(&comp_id);
    vb.set_peridottroller(&comp_id);
    // Oracle
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_a, 1_000_000i128); // $1
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_b, 1_000_000i128); // $1
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
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_b, 100_000i128); // $0.10
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
    v.enable_static_rates(&admin);

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
    set_price_and_cache(&comp, &oracle, &oracle_id, &t, 1_000_000i128);
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
    v.enable_static_rates(&admin);

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
    set_price_and_cache(&comp, &oracle, &oracle_id, &t, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    // Mint and attempt deposit
    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&user, &1_000i128);
    v.deposit(&user, &10u128);
}

#[test]
fn test_pause_expires_automatically() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let market_id = env.register(rv::ReceiptVault, ());
    let market = rv::ReceiptVaultClient::new(&env, &market_id);
    market.initialize(&token, &0u128, &0u128, &admin);
    market.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_id);

    comp.set_pause_deposit(&market_id, &true);
    assert!(comp.is_deposit_paused(&market_id));

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + MAX_PAUSE_DURATION_SECS + 1);
    assert!(!comp.is_deposit_paused(&market_id));
    env.as_contract(&comp_id, || {
        let flags: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseDeposit)
            .unwrap_or(Map::new(&env));
        let untils: Map<Address, u64> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseDepositUntil)
            .unwrap_or(Map::new(&env));
        assert!(!flags.get(market_id.clone()).unwrap_or(false));
        assert!(untils.get(market_id.clone()).is_none());
    });
}

#[test]
fn test_legacy_pause_without_expiry_fails_closed_without_backfill() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let market_id = env.register(rv::ReceiptVault, ());
    let market = rv::ReceiptVaultClient::new(&env, &market_id);
    market.initialize(&token, &0u128, &0u128, &admin);
    market.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_id);

    // Start with a normal pause write, then drop expiry metadata to emulate
    // legacy pre-upgrade storage where only the pause flag existed.
    comp.set_pause_borrow(&market_id, &true);
    env.as_contract(&comp_id, || {
        let mut untils: Map<Address, u64> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseBorrowUntil)
            .unwrap_or(Map::new(&env));
        untils.remove(market_id.clone());
        env.storage()
            .persistent()
            .set(&DataKey::PauseBorrowUntil, &untils);
        // Emulate upgraded legacy deployment where migration-complete flag
        // does not exist yet.
        env.storage()
            .persistent()
            .remove(&DataKey::PauseExpiryMigrationDone);
    });

    // Missing expiry fails closed, and pause checks stay read-only.
    assert!(comp.is_borrow_paused(&market_id));
    env.as_contract(&comp_id, || {
        let untils: Map<Address, u64> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseBorrowUntil)
            .unwrap_or(Map::new(&env));
        assert!(untils.get(market_id.clone()).is_none());
    });
}

#[test]
fn test_migrate_legacy_pause_expiries_backfills_and_expires() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let market_id = env.register(rv::ReceiptVault, ());
    let market = rv::ReceiptVaultClient::new(&env, &market_id);
    market.initialize(&token, &0u128, &0u128, &admin);
    market.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_id);

    comp.set_pause_borrow(&market_id, &true);
    env.as_contract(&comp_id, || {
        let mut untils: Map<Address, u64> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseBorrowUntil)
            .unwrap_or(Map::new(&env));
        untils.remove(market_id.clone());
        env.storage()
            .persistent()
            .set(&DataKey::PauseBorrowUntil, &untils);
        // Emulate upgraded legacy deployment where migration-complete flag
        // does not exist yet.
        env.storage()
            .persistent()
            .remove(&DataKey::PauseExpiryMigrationDone);
    });

    assert!(comp.is_borrow_paused(&market_id));
    let next = comp.migrate_legacy_pause_expiries(&0u32, &8u32);
    assert_eq!(next, 1u32);
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + MAX_PAUSE_DURATION_SECS + 1);
    assert!(!comp.is_borrow_paused(&market_id));
}

#[test]
fn test_missing_pause_expiry_stays_fail_closed_even_after_migration_done() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let market_id = env.register(rv::ReceiptVault, ());
    let market = rv::ReceiptVaultClient::new(&env, &market_id);
    market.initialize(&token, &0u128, &0u128, &admin);
    market.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_id);

    comp.set_pause_borrow(&market_id, &true);
    env.as_contract(&comp_id, || {
        let mut untils: Map<Address, u64> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseBorrowUntil)
            .unwrap_or(Map::new(&env));
        untils.remove(market_id.clone());
        env.storage()
            .persistent()
            .set(&DataKey::PauseBorrowUntil, &untils);
        env.storage()
            .persistent()
            .set(&DataKey::PauseExpiryMigrationDone, &true);
    });

    assert!(comp.is_borrow_paused(&market_id));
}

#[test]
#[should_panic(expected = "bad start")]
fn test_migrate_legacy_pause_expiries_requires_sequential_start() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    for _ in 0..2 {
        let token_admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let market_id = env.register(rv::ReceiptVault, ());
        let market = rv::ReceiptVaultClient::new(&env, &market_id);
        market.initialize(&token, &0u128, &0u128, &admin);
        market.enable_static_rates(&admin);
        comp.add_market(&market_id);
    }

    // Cursor starts at 0, so starting from 1 must be rejected.
    comp.migrate_legacy_pause_expiries(&1u32, &1u32);
}

#[test]
#[should_panic(expected = "market not supported")]
fn test_pause_setter_rejects_unsupported_market() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let market_id = env.register(rv::ReceiptVault, ());
    let market = rv::ReceiptVaultClient::new(&env, &market_id);
    market.initialize(&token, &0u128, &0u128, &admin);
    market.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    // Market was never added via add_market.
    comp.set_pause_borrow(&market_id, &true);
}

#[test]
fn test_liquidation_pause_also_pauses_borrow() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let market_id = env.register(rv::ReceiptVault, ());
    let market = rv::ReceiptVaultClient::new(&env, &market_id);
    market.initialize(&token, &0u128, &0u128, &admin);
    market.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_id);

    comp.set_pause_liquidation(&market_id, &true);
    assert!(comp.is_liquidation_paused(&market_id));
    assert!(comp.is_borrow_paused(&market_id));
}

#[test]
#[should_panic(expected = "guardian can only pause")]
fn test_guardian_cannot_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let guardian = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let market_id = env.register(rv::ReceiptVault, ());
    let market = rv::ReceiptVaultClient::new(&env, &market_id);
    market.initialize(&token, &0u128, &0u128, &admin);
    market.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_id);
    comp.set_pause_guardian(&guardian);
    comp.pause_deposit_g(&guardian, &market_id, &true);

    // Guardian unpause must be rejected.
    comp.pause_deposit_g(&guardian, &market_id, &false);
}

#[test]
fn test_liquidation_fee_routed_to_reserves() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

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
    va.enable_static_rates(&admin);
    vb.initialize(&t_b, &0u128, &0u128, &admin);
    vb.enable_static_rates(&admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.enter_market(&borrower, &va_id);
    comp.enter_market(&borrower, &vb_id);
    comp.set_market_cf(&vb_id, &500_000u128);

    // Oracle + params
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_a, 1_000_000i128); // $1
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_b, 1_000_000i128); // $1
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
    set_price_and_cache(&comp, &oracle, &oracle_id, &t_b, 500_000i128); // $0.5

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
    env.mock_all_auths_allowing_non_root_auth();

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
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.enter_market(&borrower, &vault_a_id);
    comp.enter_market(&borrower, &vault_b_id);
    comp.set_market_cf(&vault_b_id, &500_000u128);

    // Oracle + params
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128); // $1
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 5_000_000i128); // $5 initial to allow borrow
    comp.set_oracle(&oracle_id);
    // Increase liquidation incentive to max allowed 1.2x
    comp.set_liquidation_incentive(&1_200_000u128);

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

    // Now drop price to $0.5; close factor=50% caps repay to 25; LI=1.2x => seize = 25*1.2/0.5 = 60 > 50 -> clamp to 50
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 500_000i128);
    let ta_client = token::Client::new(&env, &token_a);
    let liq_a_before = ta_client.balance(&liquidator);
    comp.liquidate(&borrower, &vault_a_id, &vault_b_id, &50u128, &liquidator);
    let liq_a_after = ta_client.balance(&liquidator);

    // Borrower collateral pTokens fully seized (50), liquidator receives 50 since no fee configured
    assert_eq!(vault_b.get_ptoken_balance(&borrower), 0u128);
    assert_eq!(vault_b.get_ptoken_balance(&liquidator), 50u128);
    // Repay is proportionally reduced when seize is clamped:
    // requested capped repay=25, computed seize=60, available seize=50 => repay=ceil(25*50/60)=21.
    assert_eq!((liq_a_before - liq_a_after) as u128, 21u128);
    assert_eq!(vault_a.get_user_borrow_balance(&borrower), 29u128);
}

#[test]
fn test_liquidation_clamp_rounding_keeps_nonzero_repay() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let lender = Address::generate(&env);

    let token_admin_a = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(token_admin_a.clone())
        .address();
    let token_admin_b = Address::generate(&env);
    let token_b = env
        .register_stellar_asset_contract_v2(token_admin_b.clone())
        .address();

    let va_id = env.register(rv::ReceiptVault, ());
    let va = rv::ReceiptVaultClient::new(&env, &va_id);
    let vb_id = env.register(rv::ReceiptVault, ());
    let vb = rv::ReceiptVaultClient::new(&env, &vb_id);
    va.initialize(&token_a, &0u128, &0u128, &admin);
    va.enable_static_rates(&admin);
    vb.initialize(&token_b, &0u128, &0u128, &admin);
    vb.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.enter_market(&borrower, &va_id);
    comp.enter_market(&borrower, &vb_id);
    comp.set_market_cf(&vb_id, &1_000_000u128);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128); // $1
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 4_000_000i128); // $4 to allow initial borrow
    comp.set_oracle(&oracle_id);

    va.set_peridottroller(&comp_id);
    vb.set_peridottroller(&comp_id);
    vb.set_collateral_factor(&1_000_000u128);

    let mint_a = token::StellarAssetClient::new(&env, &token_a);
    let mint_b = token::StellarAssetClient::new(&env, &token_b);
    mint_b.mint(&borrower, &10i128);
    mint_a.mint(&lender, &10i128);
    mint_a.mint(&liquidator, &10i128);

    vb.deposit(&borrower, &1u128); // 1 pToken collateral
    va.deposit(&lender, &5u128);
    va.borrow(&borrower, &2u128); // close-factor cap => max repay 1

    // Make borrower deeply underwater so seize (for repay=1) > 1 pToken:
    // repay_usd=1, seize_underlying_usd=1 (trunc), collateral price=0.5 => seize=2 pTokens.
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 500_000i128);

    let ta_client = token::Client::new(&env, &token_a);
    let liq_before = ta_client.balance(&liquidator);
    comp.liquidate(&borrower, &va_id, &vb_id, &10u128, &liquidator);
    let liq_after = ta_client.balance(&liquidator);

    // With ceil-div scaling, repay remains 1 (not rounded to zero), liquidation succeeds.
    assert_eq!((liq_before - liq_after) as u128, 1u128);
    assert_eq!(vb.get_ptoken_balance(&liquidator), 1u128);
    assert_eq!(va.get_user_borrow_balance(&borrower), 1u128);
}

#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_oracle_missing_price_panics() {
    // FIND-039: Missing prices no longer panic immediately.
    // Instead, markets with missing prices are skipped (treated as $0 collateral).
    // User with $0 collateral cannot borrow → "Insufficient collateral" panic.
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lender = Address::generate(&env);

    // Collateral token/market (missing price) and borrow token/market
    let coll_admin = Address::generate(&env);
    let coll_token = env
        .register_stellar_asset_contract_v2(coll_admin.clone())
        .address();
    let borrow_admin = Address::generate(&env);
    let borrow_token = env
        .register_stellar_asset_contract_v2(borrow_admin.clone())
        .address();

    let coll_vault_id = env.register(rv::ReceiptVault, ());
    let coll_vault = rv::ReceiptVaultClient::new(&env, &coll_vault_id);
    let borrow_vault_id = env.register(rv::ReceiptVault, ());
    let borrow_vault = rv::ReceiptVaultClient::new(&env, &borrow_vault_id);
    coll_vault.initialize(&coll_token, &0u128, &0u128, &admin);
    coll_vault.enable_static_rates(&admin);
    borrow_vault.initialize(&borrow_token, &0u128, &0u128, &admin);
    borrow_vault.enable_static_rates(&admin);

    // Peridottroller wiring
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&coll_vault_id);
    comp.add_market(&borrow_vault_id);
    comp.set_market_cf(&coll_vault_id, &1_000_000u128);
    comp.enter_market(&user, &coll_vault_id);
    comp.enter_market(&user, &borrow_vault_id);
    coll_vault.set_peridottroller(&comp_id);
    borrow_vault.set_peridottroller(&comp_id);

    // Oracle with price only for borrow token; collateral token missing
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &borrow_token, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    // Mint tokens and set up positions
    let coll_mint = token::StellarAssetClient::new(&env, &coll_token);
    let borrow_mint = token::StellarAssetClient::new(&env, &borrow_token);
    coll_mint.mint(&user, &1_000i128);
    borrow_mint.mint(&lender, &1_000i128);

    coll_vault.set_collateral_factor(&1_000_000u128);
    coll_vault.deposit(&user, &100u128);
    borrow_vault.deposit(&lender, &200u128);

    borrow_vault.borrow(&user, &1u128);
}

#[test]
fn test_cached_price_expires_at_k_times_resolution() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let vault_id = env.register(rv::ReceiptVault, ());
    let vault = rv::ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    // Default max age is resolution(300) * k(2) = 600s.
    env.ledger().set_timestamp(601);
    assert_eq!(comp.get_price_usd(&token), None);
}

#[test]
fn test_fallback_price_has_max_age() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    comp.set_price_fallback(&token, &Some((1_000_000u128, 1_000_000u128)));
    assert_eq!(comp.get_price_usd(&token), Some((1_000_000u128, 1_000_000u128)));

    env.ledger()
        .set_timestamp(MAX_FALLBACK_PRICE_AGE_SECS.saturating_add(1));
    assert_eq!(comp.get_price_usd(&token), None);
}

#[test]
fn test_fallback_without_timestamp_is_rejected() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    // Simulate legacy state where fallback existed without set-at metadata.
    env.as_contract(&comp_id, || {
        env.storage().persistent().set(
            &DataKey::FallbackPrice(token.clone()),
            &FallbackPrice {
                price: 1_000_000u128,
                scale: 1_000_000u128,
            },
        );
        env.storage()
            .persistent()
            .remove(&DataKey::FallbackPriceSetAt(token.clone()));
    });

    assert_eq!(comp.get_price_usd(&token), None);
}

#[test]
fn test_backfill_fallback_price_set_at_restores_legacy_fallback() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    // Simulate legacy fallback without set-at metadata.
    env.as_contract(&comp_id, || {
        env.storage().persistent().set(
            &DataKey::FallbackPrice(token.clone()),
            &FallbackPrice {
                price: 1_000_000u128,
                scale: 1_000_000u128,
            },
        );
        env.storage()
            .persistent()
            .remove(&DataKey::FallbackPriceSetAt(token.clone()));
    });
    assert_eq!(comp.get_price_usd(&token), None);

    comp.backfill_fallback_price_set_at(&token);
    assert_eq!(comp.get_price_usd(&token), Some((1_000_000u128, 1_000_000u128)));
}

#[test]
fn test_require_price_prefers_live_refresh_over_fallback() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let vault_id = env.register(rv::ReceiptVault, ());
    let vault = rv::ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.set_market_cf(&vault_id, &1_000_000u128);
    comp.enter_market(&user, &vault_id);
    vault.set_peridottroller(&comp_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token, 1_000_000i128);
    comp.set_oracle(&oracle_id);
    comp.set_price_fallback(&token, &Some((1_000_000u128, 1_000_000u128)));

    let mint = token::StellarAssetClient::new(&env, &token);
    mint.mint(&user, &1_000i128);
    vault.set_collateral_factor(&1_000_000u128);
    vault.deposit(&user, &100u128);

    // Expire cache (default 600s), then move oracle to $2.00 without pre-warming cache.
    env.ledger().set_timestamp(601);
    oracle.set_price(&token, &2_000_000i128);

    // require_price path (via account_liquidity) should refresh from oracle first, not fallback.
    let (liq, shortfall) = comp.account_liquidity(&user);
    assert_eq!(shortfall, 0u128);
    assert_eq!(liq, 200u128);
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
    v.enable_static_rates(&admin);

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
    set_price_and_cache(&comp, &oracle, &oracle_id, &t, 100_000_000i128);
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
    v.enable_static_rates(&admin);

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
#[should_panic(expected = "no_comp")]
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
    v.enable_static_rates(&admin);

    // Give borrower some pTokens
    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&borrower, &1_000i128);
    v.deposit(&borrower, &100u128);

    // Direct seize without peridottroller wired -> should panic
    v.seize(&borrower, &liquidator, &10u128, &None);
}

#[test]
fn test_rewards_accrual_and_claim() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

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
    v.enable_static_rates(&admin);

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
    set_price_and_cache(&comp, &oracle, &oracle_id, &t, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    // PERI token under comptroller admin
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    std::env::set_var("PERIDOT_TOKEN_INIT_ADMIN", pt::DEFAULT_INIT_ADMIN);
    let token_admin = Address::from_string(&String::from_str(&env, pt::DEFAULT_INIT_ADMIN));
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &token_admin,
        &1_000_000_000i128,
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
    env.mock_all_auths_allowing_non_root_auth();

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
    va.enable_static_rates(&admin);
    vb.initialize(&tb, &0u128, &0u128, &admin);
    vb.enable_static_rates(&admin);

    // Comptroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.enter_market(&borrower, &va_id);
    comp.enter_market(&borrower, &vb_id);
    comp.set_market_cf(&vb_id, &1_000_000u128);
    va.set_peridottroller(&comp_id);
    vb.set_peridottroller(&comp_id);

    // Oracle $1
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &ta, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &tb, 1_000_000i128);
    comp.set_oracle(&oracle_id);

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

    // Configure rewards after borrow so borrow path stays lightweight.
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    std::env::set_var("PERIDOT_TOKEN_INIT_ADMIN", pt::DEFAULT_INIT_ADMIN);
    let token_admin = Address::from_string(&String::from_str(&env, pt::DEFAULT_INIT_ADMIN));
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &token_admin,
        &1_000_000_000i128,
    );
    comp.set_peridot_token(&peri_id);
    comp.set_borrow_speed(&va_id, &7u128);

    // Advance 6s and claim
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 6);
    comp.claim(&borrower);
    assert_eq!(peri.balance_of(&borrower), 42i128); // 7*6
}

#[test]
fn test_liquidator_does_not_receive_retroactive_supply_rewards_after_seize() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.cost_estimate().budget().reset_unlimited();

    let admin = Address::generate(&env);
    let borrower = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let ta_admin = Address::generate(&env);
    let tb_admin = Address::generate(&env);
    let ta = env
        .register_stellar_asset_contract_v2(ta_admin.clone())
        .address();
    let tb = env
        .register_stellar_asset_contract_v2(tb_admin.clone())
        .address();

    // collateral market A and repay market B
    let va_id = env.register(rv::ReceiptVault, ());
    let va = rv::ReceiptVaultClient::new(&env, &va_id);
    let vb_id = env.register(rv::ReceiptVault, ());
    let vb = rv::ReceiptVaultClient::new(&env, &vb_id);
    va.initialize(&ta, &0u128, &0u128, &admin);
    va.enable_static_rates(&admin);
    vb.initialize(&tb, &0u128, &0u128, &admin);
    vb.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&va_id);
    comp.add_market(&vb_id);
    comp.set_market_cf(&va_id, &500_000u128);
    comp.enter_market(&borrower, &va_id);
    comp.enter_market(&borrower, &vb_id);
    comp.enter_market(&liquidator, &va_id);
    comp.enter_market(&liquidator, &vb_id);
    va.set_peridottroller(&comp_id);
    vb.set_peridottroller(&comp_id);
    va.set_collateral_factor(&500_000u128);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &ta, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &tb, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    let mint_a = token::StellarAssetClient::new(&env, &ta);
    let mint_b = token::StellarAssetClient::new(&env, &tb);
    mint_a.mint(&borrower, &1_000i128);
    mint_b.mint(&liquidator, &1_000i128);

    // Borrower mints collateral pTokens and borrows from market B.
    va.deposit(&borrower, &100u128);
    vb.deposit(&liquidator, &300u128);
    vb.borrow(&borrower, &40u128);

    // Configure rewards after borrow so borrow path stays under footprint limits.
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    std::env::set_var("PERIDOT_TOKEN_INIT_ADMIN", pt::DEFAULT_INIT_ADMIN);
    let token_admin = Address::from_string(&String::from_str(&env, pt::DEFAULT_INIT_ADMIN));
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &token_admin,
        &1_000_000_000i128,
    );
    comp.set_peridot_token(&peri_id);
    comp.set_supply_speed(&va_id, &10u128);

    // Let supply index grow before liquidator receives any collateral pTokens.
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 10);

    // Liquidate to receive seized collateral pTokens after index growth.
    set_price_and_cache(&comp, &oracle, &oracle_id, &ta, 500_000i128);
    comp.liquidate(&borrower, &vb_id, &va_id, &40u128, &liquidator);
    assert!(va.get_ptoken_balance(&liquidator) > 0u128);

    // Claim in the same timestamp as seize: no retroactive supply rewards.
    comp.claim(&liquidator);
    assert_eq!(peri.balance_of(&liquidator), 0i128);
}

#[test]
fn test_claim_skips_failing_market_and_claims_from_healthy_market() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Healthy market
    let t_admin = Address::generate(&env);
    let t = env
        .register_stellar_asset_contract_v2(t_admin.clone())
        .address();
    let v_id = env.register(rv::ReceiptVault, ());
    let v = rv::ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&t, &0u128, &0u128, &admin);
    v.enable_static_rates(&admin);

    // Failing market
    let failing_market_id = env.register(FailingClaimMarket, ());
    let failing_market = FailingClaimMarketClient::new(&env, &failing_market_id);
    failing_market.initialize(&t);

    // Comptroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&failing_market_id);
    comp.add_market(&v_id);
    // Enter failing market first to ensure claim continues past failure.
    comp.enter_market(&user, &failing_market_id);
    comp.enter_market(&user, &v_id);
    // Wire healthy market so deposit path anchors user supply index.
    v.set_peridottroller(&comp_id);

    // PERI token
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    std::env::set_var("PERIDOT_TOKEN_INIT_ADMIN", pt::DEFAULT_INIT_ADMIN);
    let token_admin = Address::from_string(&String::from_str(&env, pt::DEFAULT_INIT_ADMIN));
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &token_admin,
        &1_000_000_000i128,
    );
    comp.set_peridot_token(&peri_id);

    // Rewards only on the healthy market.
    comp.set_supply_speed(&v_id, &10u128);

    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&user, &1_000i128);
    v.deposit(&user, &100u128);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 5);
    comp.claim(&user);

    // Healthy-market rewards still claimed despite failing market.
    assert_eq!(peri.balance_of(&user), 50i128);
}

#[test]
fn test_claim_mint_failure_keeps_accrued_balance() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let t_admin = Address::generate(&env);
    let t = env
        .register_stellar_asset_contract_v2(t_admin.clone())
        .address();
    let v_id = env.register(rv::ReceiptVault, ());
    let v = rv::ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&t, &0u128, &0u128, &admin);
    v.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&v_id);
    comp.enter_market(&user, &v_id);
    // Wire market so deposit path anchors user supply index.
    v.set_peridottroller(&comp_id);

    // Broken reward token: mint() always reverts.
    let broken_peri_id = env.register(FailingPeridotToken, ());
    comp.set_peridot_token(&broken_peri_id);
    comp.set_supply_speed(&v_id, &10u128);

    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&user, &1_000i128);
    v.deposit(&user, &100u128);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 5);
    comp.claim(&user);

    // Claim returns without reverting and keeps accrued rewards for retry.
    assert_eq!(comp.get_accrued(&user), 50u128);
}

#[test]
fn test_claim_recovers_missing_supply_index_time() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let vault_id = env.register(rv::ReceiptVault, ());
    let vault = rv::ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_id);
    comp.enter_market(&user, &vault_id);
    comp.set_supply_speed(&vault_id, &10u128);

    let mint = token::StellarAssetClient::new(&env, &token);
    mint.mint(&user, &1_000i128);
    vault.deposit(&user, &100u128);

    // Simulate TTL loss for the index-time key.
    env.as_contract(&comp_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::SupplyIndexTime(vault_id.clone()));
    });

    // First claim re-anchors missing index-time at `now` without getting stuck.
    comp.claim(&user);
    let supply_time_after_reanchor: Option<u64> = env.as_contract(&comp_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::SupplyIndexTime(vault_id.clone()))
    });
    assert!(supply_time_after_reanchor.is_some());

    // Next claim after time advances should accrue rewards again.
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 5);
    comp.claim(&user);
    assert!(comp.get_accrued(&user) > 0);
}

#[test]
#[should_panic]
fn test_claim_requires_user_auth() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    // Disable mocked auth so user.require_auth() is enforced.
    env.set_auths(&[]);
    comp.claim(&user);
}

#[test]
#[should_panic(expected = "batch too large")]
fn test_claim_all_rejects_oversized_batch() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    let mut users = soroban_sdk::Vec::new(&env);
    for _ in 0..(MAX_CLAIM_BATCH + 1) {
        users.push_back(Address::generate(&env));
    }
    env.as_contract(&comp_id, || {
        SimplePeridottroller::claim_all(env.clone(), users.clone());
    });
}

#[test]
#[should_panic(expected = "invalid max age mult")]
fn test_set_oracle_max_age_multiplier_rejects_large_values() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.set_oracle_max_age_multiplier(&11u64);
}

#[test]
#[should_panic(expected = "invalid collateral factor")]
fn test_set_market_cf_rejects_zero() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.set_market_cf(&Address::generate(&env), &0u128);
}

#[test]
#[should_panic(expected = "invalid incentive")]
fn test_set_liquidation_incentive_rejects_above_cap() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.set_liquidation_incentive(&1_200_001u128);
}

#[test]
#[should_panic(expected = "peridot token already set")]
fn test_set_peridot_token_is_one_time() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.set_peridot_token(&Address::generate(&env));
    comp.set_peridot_token(&Address::generate(&env));
}

#[test]
#[should_panic(expected = "speed too high")]
fn test_set_supply_speed_rejects_excessive_values() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let market_id = env.register(rv::ReceiptVault, ());
    let market = rv::ReceiptVaultClient::new(&env, &market_id);
    market.initialize(&token, &0u128, &0u128, &admin);
    market.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&market_id);
    comp.set_supply_speed(&market_id, &(MAX_REWARD_SPEED_PER_SEC + 1));
}

// Security test: Verify that accrue_user_market rejects hints from non-market callers
// This prevents attackers from inflating reward indexes with malicious hint values
#[test]
#[should_panic] // Panics due to failed auth when attacker provides hints
fn test_accrue_user_market_rejects_external_hints() {
    let env = Env::default();
    // DO NOT mock_all_auths - we want auth to be enforced
    
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);

    // Token + market
    let t_admin = Address::generate(&env);
    let t = env
        .register_stellar_asset_contract_v2(t_admin.clone())
        .address();
    let v_id = env.register(rv::ReceiptVault, ());
    let v = rv::ReceiptVaultClient::new(&env, &v_id);
    
    // Mock only specific auths needed for setup (not for the attack)
    env.mock_all_auths();
    v.initialize(&t, &0u128, &0u128, &admin);
    v.enable_static_rates(&admin);

    // Comptroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&v_id);
    v.set_peridottroller(&comp_id);
    
    // Set up reward speeds so accrual matters
    comp.set_supply_speed(&v_id, &1000u128);
    
    // PERI token for rewards
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    std::env::set_var("PERIDOT_TOKEN_INIT_ADMIN", pt::DEFAULT_INIT_ADMIN);
    let token_admin = Address::from_string(&String::from_str(&env, pt::DEFAULT_INIT_ADMIN));
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &token_admin,
        &1_000_000_000i128,
    );
    comp.set_peridot_token(&peri_id);
    
    // Advance time so accrual produces rewards
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 100);

    // Stop mocking all auths - now the attacker cannot satisfy market.require_auth()
    env.set_auths(&[]);
    
    // Attacker tries to call accrue_user_market with malicious hints:
    // - tiny total_ptokens to inflate the index
    // - huge user_ptokens to get massive accrual
    let malicious_hint = AccrualHint {
        total_ptokens: Some(1u128),        // tiny -> inflates index
        total_borrowed: Some(0u128),
        user_ptokens: Some(1_000_000_000u128), // huge -> massive accrual
        user_borrowed: Some(0u128),
    };
    
    // This should PANIC because attacker can't auth as the market
    comp.accrue_user_market(&attacker, &v_id, &Some(malicious_hint));
}

// Security test: Verify that accrue_user_market works WITHOUT hints (safe path)
#[test]
fn test_accrue_user_market_allows_no_hints() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

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
    v.enable_static_rates(&admin);

    // Comptroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&v_id);
    comp.enter_market(&user, &v_id);
    v.set_peridottroller(&comp_id);
    
    // Oracle with $1 price
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &t, 1_000_000i128);
    comp.set_oracle(&oracle_id);
    
    // Set up reward speeds
    comp.set_supply_speed(&v_id, &10u128);
    
    // PERI token
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    std::env::set_var("PERIDOT_TOKEN_INIT_ADMIN", pt::DEFAULT_INIT_ADMIN);
    let token_admin = Address::from_string(&String::from_str(&env, pt::DEFAULT_INIT_ADMIN));
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &token_admin,
        &1_000_000_000i128,
    );
    comp.set_peridot_token(&peri_id);
    
    // Fund and deposit
    let mint = token::StellarAssetClient::new(&env, &t);
    mint.mint(&user, &1_000i128);
    v.deposit(&user, &100u128);
    
    // Advance time
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 5);
    
    // Call accrue_user_market WITHOUT hints (None) - this should succeed
    // because it fetches values on-chain instead of trusting hints
    comp.accrue_user_market(&user, &v_id, &None);
    
    // Verify accrual happened (user should have some accrued rewards)
    let accrued = comp.get_accrued(&user);
    assert!(accrued > 0u128, "Should have accrued rewards");
}

#[test]
#[should_panic(expected = "market not supported")]
fn test_accrue_user_market_rejects_unsupported_market_without_hints() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let unsupported_market = Address::generate(&env);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    // Configure reward token so accrue_user_market doesn't short-circuit early.
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    std::env::set_var("PERIDOT_TOKEN_INIT_ADMIN", pt::DEFAULT_INIT_ADMIN);
    let token_admin = Address::from_string(&String::from_str(&env, pt::DEFAULT_INIT_ADMIN));
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &token_admin,
        &1_000_000_000i128,
    );
    comp.set_peridot_token(&peri_id);

    // No add_market call for unsupported_market: should panic on validation.
    comp.accrue_user_market(&user, &unsupported_market, &None);
}

#[test]
#[should_panic(expected = "market not supported")]
fn test_bind_boosted_vault_rejects_unsupported_market() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let unsupported_market = Address::generate(&env);
    let boosted = Address::generate(&env);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);

    let none_addr: Option<Address> = None;
    comp.bind_boosted_vault(&unsupported_market, &none_addr, &Some(boosted));
}

#[test]
fn test_multi_market_supply_rewards() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

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
    va.enable_static_rates(&admin);
    vb.initialize(&tb, &0u128, &0u128, &admin);
    vb.enable_static_rates(&admin);

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
    set_price_and_cache(&comp, &oracle, &oracle_id, &ta, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &tb, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    // PERI token
    let peri_id = env.register(pt::PeridotToken, ());
    let peri = pt::PeridotTokenClient::new(&env, &peri_id);
    std::env::set_var("PERIDOT_TOKEN_INIT_ADMIN", pt::DEFAULT_INIT_ADMIN);
    let token_admin = Address::from_string(&String::from_str(&env, pt::DEFAULT_INIT_ADMIN));
    peri.initialize(
        &soroban_sdk::String::from_str(&env, "Peridot"),
        &soroban_sdk::String::from_str(&env, "P"),
        &6u32,
        &token_admin,
        &1_000_000_000i128,
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

    // Accrue rewards on both markets -> expect (5+3)*4 = 32 P accrued.
    comp.accrue_user_market(&user, &va_id, &None);
    comp.accrue_user_market(&user, &vb_id, &None);
    assert_eq!(comp.get_accrued(&user), 32u128);
}

/// FIND-039 follow-up: A broken market in UserMarkets yields fail-closed liquidity output.
#[test]
fn test_find_039_broken_market_returns_fail_closed_shortfall() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let liquidator = Address::generate(&env);

    // Real markets: vault_a = borrow market, vault_b = collateral market
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
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    // Peridottroller
    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    // Oracle: token_a and token_b both priced at $1.00 (6-decimal scale)
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128); // $1
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128); // $1
    comp.set_oracle(&oracle_id);

    // Fund participants
    let mint_b = token::StellarAssetClient::new(&env, &token_b);
    let mint_a = token::StellarAssetClient::new(&env, &token_a);
    mint_b.mint(&alice, &100i128);
    mint_a.mint(&liquidator, &1_000i128);

    // Alice's position:
    // Collateral: 100 token_b in vault_b, CF = 50% → $50 discounted collateral
    // Borrow:      40 token_a from vault_a           → $40 debt
    // Initial liquidity: $50 − $40 = $10 (solvent; borrow is allowed)
    comp.set_market_cf(&vault_b_id, &500_000u128); // 50% CF on peridottroller
    vault_b.set_collateral_factor(&500_000u128); // 50% CF on vault
    comp.enter_market(&alice, &vault_b_id); // vault_b as collateral source
    comp.enter_market(&alice, &vault_a_id); // vault_a entered (has debt, no deposit)
    vault_b.deposit(&alice, &100u128); // Alice receives 100 pTokens in vault_b
    vault_a.deposit(&liquidator, &200u128); // seed vault_a with borrowable liquidity
    vault_a.borrow(&alice, &40u128); // $40 debt, within $50 CF power

    // §A: Confirm real shortfall WITHOUT BrokenMarket in Alice's list
    // Drop token_b price from $1.00 to $0.60:
    //   CF-discounted collateral = 100 pTokens × $0.60 × 50% CF = $30
    //   Debt                     =  40 token_a × $1.00           = $40
    //   Shortfall                = $40 − $30                      = $10
    // At this point BrokenMarket is NOT yet in Alice's list; account_liquidity works.
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 600_000i128); // $0.60

    let (liq, shortfall) = comp.account_liquidity(&alice);
    assert_eq!(liq, 0u128, "§A: no excess liquidity after collateral price drop");
    assert_eq!(
        shortfall, 10u128,
        "§A: $10 shortfall — Alice IS legitimately liquidatable without BrokenMarket (baseline)"
    );

    // §B: Simulate TTL expiry of a dormant market Alice entered earlier
    // BrokenMarket represents a real market Alice entered when it was healthy (zero
    // balance — no capital required). Its persistent-storage TTL has since expired:
    // every cross-contract call into it now panics, just as Soroban does when a
    // contract reads an archived storage entry.
    //
    // Register and initialize BrokenMarket (was healthy when added)
    // Simulates a market Alice entered when it was functional, but whose
    // storage has since expired (other entry points now panic)
    let broken_id = env.register(BrokenMarket, ());
    let broken_market = BrokenMarketClient::new(&env, &broken_id);
    broken_market.initialize(&token_b); // Use token_b as underlying (arbitrary choice)

    comp.add_market(&broken_id); // This succeeds (get_underlying_token works)
    comp.enter_market(&alice, &broken_id); // Alice's list: [vault_b, vault_a, broken]

    // Health query remains non-reverting but fail-closed.
    let (liq_after, shortfall_after) = comp.account_liquidity(&alice);
    assert_eq!(liq_after, 0u128);
    assert_eq!(shortfall_after, u128::MAX);
}

#[test]
fn test_find_039_liquidation_allows_when_known_shortfall_exists() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let token_a = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let token_b = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    let mint_b = token::StellarAssetClient::new(&env, &token_b);
    let mint_a = token::StellarAssetClient::new(&env, &token_a);
    mint_b.mint(&alice, &100i128);
    mint_a.mint(&liquidator, &1_000i128);

    comp.set_market_cf(&vault_b_id, &500_000u128);
    vault_b.set_collateral_factor(&500_000u128);
    comp.enter_market(&alice, &vault_b_id);
    comp.enter_market(&alice, &vault_a_id);
    vault_b.deposit(&alice, &100u128);
    vault_a.deposit(&liquidator, &200u128);
    vault_a.borrow(&alice, &40u128);

    // Make position underwater before liquidation attempt.
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 600_000i128);

    let failing_market_id = env.register(FailingClaimMarket, ());
    let failing_market = FailingClaimMarketClient::new(&env, &failing_market_id);
    failing_market.initialize(&token_b);
    failing_market.set_debt(&alice, &1u128);
    comp.add_market(&failing_market_id);
    comp.enter_market(&alice, &failing_market_id);
    failing_market.set_fail_underlying(&true);

    // Known shortfall from real markets is already non-zero, so liquidation should proceed
    // even if another entered market is indeterminate.
    comp.liquidate(&alice, &vault_a_id, &vault_b_id, &10u128, &liquidator);

    // Borrower debt reduced by liquidation.
    assert!(vault_a.get_user_borrow_balance(&alice) < 40u128);
}

#[test]
fn test_liquidation_not_blocked_by_unrelated_pbal_read_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let token_a = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let token_b = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    let mint_b = token::StellarAssetClient::new(&env, &token_b);
    let mint_a = token::StellarAssetClient::new(&env, &token_a);
    mint_b.mint(&alice, &100i128);
    mint_a.mint(&liquidator, &1_000i128);

    comp.set_market_cf(&vault_b_id, &500_000u128);
    vault_b.set_collateral_factor(&500_000u128);
    comp.enter_market(&alice, &vault_b_id);
    comp.enter_market(&alice, &vault_a_id);
    vault_b.deposit(&alice, &100u128);
    vault_a.deposit(&liquidator, &200u128);
    vault_a.borrow(&alice, &40u128);

    // Make known position underwater.
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 600_000i128);

    // Add unrelated market that fails pToken-balance reads but has no debt.
    let fail_market_id = env.register(PbalReadFailMarket, ());
    let fail_market = PbalReadFailMarketClient::new(&env, &fail_market_id);
    fail_market.initialize(&token_b);
    comp.add_market(&fail_market_id);
    comp.enter_market(&alice, &fail_market_id);

    comp.liquidate(&alice, &vault_a_id, &vault_b_id, &10u128, &liquidator);
    assert!(vault_a.get_user_borrow_balance(&alice) < 40u128);
}

#[test]
#[should_panic(expected = "health indeterminate")]
fn test_liquidation_blocked_when_collateral_indeterminate_market_has_cf() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let token_a = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let token_b = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    let mint_b = token::StellarAssetClient::new(&env, &token_b);
    let mint_a = token::StellarAssetClient::new(&env, &token_a);
    mint_b.mint(&alice, &100i128);
    mint_a.mint(&liquidator, &1_000i128);

    comp.set_market_cf(&vault_b_id, &500_000u128);
    vault_b.set_collateral_factor(&500_000u128);
    comp.enter_market(&alice, &vault_b_id);
    comp.enter_market(&alice, &vault_a_id);
    vault_b.deposit(&alice, &100u128);
    vault_a.deposit(&liquidator, &200u128);
    vault_a.borrow(&alice, &40u128);

    // Make known position underwater.
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 600_000i128);

    // Add a market with non-zero CF where pToken reads are unavailable.
    // Unknown collateral in a collateral-enabled market must block liquidation.
    let fail_market_id = env.register(PbalReadFailMarket, ());
    let fail_market = PbalReadFailMarketClient::new(&env, &fail_market_id);
    fail_market.initialize(&token_b);
    comp.add_market(&fail_market_id);
    comp.set_market_cf(&fail_market_id, &500_000u128);
    comp.enter_market(&alice, &fail_market_id);

    comp.liquidate(&alice, &vault_a_id, &vault_b_id, &10u128, &liquidator);
}

#[test]
#[should_panic(expected = "health indeterminate")]
fn test_find_039_liquidation_rejects_when_only_indeterminate_signal() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let liquidator = Address::generate(&env);

    let token_a = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let token_b = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let vault_a_id = env.register(rv::ReceiptVault, ());
    let vault_a = rv::ReceiptVaultClient::new(&env, &vault_a_id);
    let vault_b_id = env.register(rv::ReceiptVault, ());
    let vault_b = rv::ReceiptVaultClient::new(&env, &vault_b_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_a.enable_static_rates(&admin);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_a, 1_000_000i128);
    set_price_and_cache(&comp, &oracle, &oracle_id, &token_b, 1_000_000i128);
    comp.set_oracle(&oracle_id);

    let mint_b = token::StellarAssetClient::new(&env, &token_b);
    let mint_a = token::StellarAssetClient::new(&env, &token_a);
    mint_b.mint(&alice, &100i128);
    mint_a.mint(&liquidator, &1_000i128);

    comp.set_market_cf(&vault_b_id, &500_000u128);
    vault_b.set_collateral_factor(&500_000u128);
    comp.enter_market(&alice, &vault_b_id);
    comp.enter_market(&alice, &vault_a_id);
    vault_b.deposit(&alice, &100u128);
    vault_a.deposit(&liquidator, &200u128);
    vault_a.borrow(&alice, &30u128); // healthy: collateral power $50 vs debt $30

    // Unrelated failing market introduces indeterminate state.
    let failing_market_id = env.register(FailingClaimMarket, ());
    let failing_market = FailingClaimMarketClient::new(&env, &failing_market_id);
    failing_market.initialize(&token_b);
    failing_market.set_debt(&alice, &1u128);
    comp.add_market(&failing_market_id);
    comp.enter_market(&alice, &failing_market_id);
    failing_market.set_fail_underlying(&true);

    // Known positions are not underwater; indeterminate alone must not authorize liquidation.
    comp.liquidate(&alice, &vault_a_id, &vault_b_id, &10u128, &liquidator);
}
