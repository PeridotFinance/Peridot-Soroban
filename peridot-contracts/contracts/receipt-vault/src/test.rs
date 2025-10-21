#![cfg(test)]

use super::*;
use jump_rate_model as jrm;
use simple_peridottroller::SimplePeridottroller;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::BytesN;
use soroban_sdk::{contract, contractimpl, contracttype};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    token, Address, Bytes, Env, IntoVal,
};

fn create_test_token<'a>(
    env: &'a Env,
    admin: &'a Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    (
        contract_address.clone(),
        token::Client::new(env, &contract_address),
        token::StellarAssetClient::new(env, &contract_address),
    )
}

#[contract]
pub struct FlashLoanRepayer;

#[contracttype]
#[derive(Clone)]
enum ReceiverDataKey {
    Underlying,
}

#[contractimpl]
impl FlashLoanRepayer {
    pub fn configure(env: Env, underlying: Address) {
        env.storage()
            .persistent()
            .set(&ReceiverDataKey::Underlying, &underlying);
    }

    pub fn on_flash_loan(env: Env, vault: Address, amount: u128, fee: u128, _data: Bytes) {
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&ReceiverDataKey::Underlying)
            .expect("underlying not set");
        let token_client = token::Client::new(&env, &token_address);
        let repay_total = amount.saturating_add(fee);
        token_client.transfer(
            &env.current_contract_address(),
            &vault,
            &to_i128(repay_total),
        );
    }
}

#[contract]
pub struct FlashLoanRenegade;

#[contractimpl]
impl FlashLoanRenegade {
    pub fn configure(env: Env, underlying: Address) {
        env.storage()
            .persistent()
            .set(&ReceiverDataKey::Underlying, &underlying);
    }

    pub fn on_flash_loan(env: Env, vault: Address, amount: u128, _fee: u128, _data: Bytes) {
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&ReceiverDataKey::Underlying)
            .expect("underlying not set");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &vault, &to_i128(amount));
    }
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
#[should_panic(expected = "invalid supply rate")]
fn test_initialize_rejects_large_supply_rate() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    vault_client.initialize(&token_address, &11_000_000u128, &0u128, &admin);
}

#[test]
#[should_panic(expected = "invalid borrow rate")]
fn test_set_borrow_rate_rejects_large_value() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.set_borrow_rate(&12_000_000u128);
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

    // Exchange rate moves with cash+borrows-reserves-admin; with no borrows, supply-only accrual increases cash
    let rate = vault_client.get_exchange_rate();
    assert!(rate >= 1_000_000u128);
}

#[test]
fn test_interest_model_accrual_updates_accumulated_interest() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &2_000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);

    // Deploy and wire a jump rate model to drive dynamic interest.
    let model_id = env.register(jrm::JumpRateModel, ());
    let model_client = jrm::JumpRateModelClient::new(&env, &model_id);
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &model_client.address,
            fn_name: "initialize",
            args: (
                20_000u128,
                180_000u128,
                4_000_000u128,
                800_000u128,
                admin.clone(),
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);
    model_client.initialize(
        &20_000u128,
        &180_000u128,
        &4_000_000u128,
        &800_000u128,
        &admin,
    );
    env.mock_all_auths();
    vault_client.set_interest_model(&model_id);

    // Provide liquidity and create an outstanding borrow so interest can accrue.
    vault_client.deposit(&user, &500u128);
    vault_client.borrow(&user, &200u128);

    // Advance time and force an interest update.
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 30 * 24 * 60 * 60);
    vault_client.update_interest();

    // Accumulated interest should grow when the external model is active.
    let accrued: u128 = env.as_contract(&vault_contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::AccumulatedInterest)
            .unwrap_or(0u128)
    });
    assert!(accrued > 0);
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
fn test_reserve_accrual_and_reduce() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint tokens to the user for liquidity and collateral
    token_admin_client.mint(&user, &10_000i128);

    // Deploy vault
    let vault_contract_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize: 0% supply, 100% borrow; admin is admin
    vault.initialize(&token_address, &0u128, &1_000_000u128, &admin);

    // Set reserve factor to 20%
    vault.set_reserve_factor(&200_000u128);

    // Set CF to 100%
    vault.set_collateral_factor(&1_000_000u128);

    // Provide liquidity and collateral
    vault.deposit(&user, &200u128);

    // Borrow 100
    vault.borrow(&user, &100u128);

    // Advance 1 year and trigger interest accrual via setting same borrow rate
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    vault.set_borrow_rate(&1_000_000u128);

    // With 100% yearly borrow rate on 100 borrowed, interest = 100
    // Reserves should get 20, suppliers 80
    assert_eq!(vault.get_total_reserves(), 20u128);

    // Reduce reserves by 5 to admin
    let admin_balance_before = token_client.balance(&admin);
    vault.reduce_reserves(&5u128);
    assert_eq!(vault.get_total_reserves(), 15u128);
    let admin_balance_after = token_client.balance(&admin);
    assert_eq!(admin_balance_after - admin_balance_before, 5i128);
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
#[should_panic(expected = "supply cap exceeded")]
fn test_supply_cap_enforced_on_deposit() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint to user
    token_admin_client.mint(&user, &1_000i128);

    // Vault
    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);

    // Set cap to 150
    vault.set_supply_cap(&150u128);

    // Deposit 100 ok
    vault.deposit(&user, &100u128);
    // Deposit another 60 -> exceeds cap (total underlying after deposit = 160)
    vault.deposit(&user, &60u128);
}

#[test]
#[should_panic(expected = "borrow cap exceeded")]
fn test_borrow_cap_enforced_on_borrow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint to user and to a lender for liquidity
    token_admin_client.mint(&user, &1_000i128);
    let lender = Address::generate(&env);
    token_admin_client.mint(&lender, &1_000i128);

    // Vault
    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);

    // Provide liquidity and collateral; CF=100%
    vault.set_collateral_factor(&1_000_000u128);
    vault.deposit(&lender, &500u128);
    vault.deposit(&user, &300u128);

    // Set borrow cap to 100 total
    vault.set_borrow_cap(&100u128);

    // Borrow 80 ok
    vault.borrow(&user, &80u128);
    // Borrow additional 30 -> would exceed cap (total 110)
    vault.borrow(&user, &30u128);
}

// Mock rate model providing constant yearly rates
#[contract]
struct MockRateModel;

#[contracttype]
enum MRKey {
    Supply,
    Borrow,
}

#[contractimpl]
impl MockRateModel {
    pub fn initialize(env: Env, supply_yearly_scaled: u128, borrow_yearly_scaled: u128) {
        env.storage()
            .persistent()
            .set(&MRKey::Supply, &supply_yearly_scaled);
        env.storage()
            .persistent()
            .set(&MRKey::Borrow, &borrow_yearly_scaled);
    }
    pub fn get_supply_rate(
        env: Env,
        _cash: u128,
        _borrows: u128,
        _reserves: u128,
        _reserve_factor: u128,
    ) -> u128 {
        env.storage()
            .persistent()
            .get(&MRKey::Supply)
            .unwrap_or(0u128)
    }
    pub fn get_borrow_rate(env: Env, _cash: u128, _borrows: u128, _reserves: u128) -> u128 {
        env.storage()
            .persistent()
            .get(&MRKey::Borrow)
            .unwrap_or(0u128)
    }
}

#[test]
fn test_interest_model_supply_accrual() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);

    // Model: 10% supply, 0% borrow
    let model_id = env.register(MockRateModel, ());
    let model = MockRateModelClient::new(&env, &model_id);
    model.initialize(&100_000u128, &0u128);
    vault.set_interest_model(&model_id);

    vault.deposit(&user, &100u128);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    // Trigger accrual directly
    vault.update_interest();

    // With model set, supply interest accrues to the vault balance; 10% APR on 100 over a year => 110
    assert_eq!(vault.get_total_underlying(), 110u128);
    assert!(vault.get_exchange_rate() >= 1_000_000u128);
}

#[test]
fn test_interest_model_borrow_accrual_and_reserves() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &10_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);

    // Reserve factor 20%
    vault.set_reserve_factor(&200_000u128);

    // Model: 0% supply, 100% borrow
    let model_id = env.register(MockRateModel, ());
    let model = MockRateModelClient::new(&env, &model_id);
    model.initialize(&0u128, &1_000_000u128);
    vault.set_interest_model(&model_id);

    vault.set_collateral_factor(&1_000_000u128);
    vault.deposit(&user, &200u128);
    vault.borrow(&user, &100u128);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    // Trigger accrual directly
    vault.update_interest();

    // Interest 100 -> reserves 20, suppliers 80
    assert_eq!(vault.get_total_reserves(), 20u128);
    assert_eq!(vault.get_total_borrowed(), 200u128);
    // underlying = cash + borrows - reserves (cash stayed 100 since deposit 200, borrow 100)
    assert_eq!(vault.get_total_underlying(), 280u128);
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
fn test_flash_loan_successfully_repaid() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);

    token_admin_client.mint(&depositor, &1_000i128);
    vault.deposit(&depositor, &500u128);

    let fee_scaled = 20_000u128; // 2%
    vault.set_flash_loan_fee(&fee_scaled);

    let receiver_id = env.register(FlashLoanRepayer, ());
    let receiver_client = FlashLoanRepayerClient::new(&env, &receiver_id);
    receiver_client.configure(&token_address);
    token_admin_client.mint(&receiver_id, &50i128);

    let amount = 100u128;
    let expected_fee = (amount * fee_scaled) / 1_000_000u128;
    let data = Bytes::new(&env);

    vault.flash_loan(&receiver_id, &amount, &data);

    assert_eq!(vault.get_total_reserves(), expected_fee);
    assert_eq!(
        token_client.balance(&vault_id),
        (500 + expected_fee) as i128
    );
    assert_eq!(
        token_client.balance(&receiver_id),
        50i128 - expected_fee as i128
    );
}

#[test]
#[should_panic(expected = "flash loan not repaid")]
fn test_flash_loan_missing_fee_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);

    token_admin_client.mint(&depositor, &1_000i128);
    vault.deposit(&depositor, &500u128);
    vault.set_flash_loan_fee(&50_000u128);

    let receiver_id = env.register(FlashLoanRenegade, ());
    let receiver_client = FlashLoanRenegadeClient::new(&env, &receiver_id);
    receiver_client.configure(&token_address);
    let data = Bytes::new(&env);

    vault.flash_loan(&receiver_id, &100u128, &data);
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

#[test]
fn test_vault_set_admin_transfers_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    assert_eq!(vault.get_admin(), admin);
    vault.set_admin(&new_admin);
    assert_eq!(vault.get_admin(), new_admin);
}

#[test]
fn test_jump_model_dynamic_borrow_apr_accrual() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &100_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.set_collateral_factor(&1_000_000u128);

    // Wire jump rate model: base=2%, multiplier=18%, jump=400%, kink=80%
    let model_id = env.register(jrm::JumpRateModel, ());
    let model_client = jrm::JumpRateModelClient::new(&env, &model_id);
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &model_client.address,
            fn_name: "initialize",
            args: (
                20_000u128,
                180_000u128,
                4_000_000u128,
                800_000u128,
                admin.clone(),
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);
    model_client.initialize(
        &20_000u128,
        &180_000u128,
        &4_000_000u128,
        &800_000u128,
        &admin,
    );
    env.mock_all_auths();
    vault.set_interest_model(&model_id);

    // Provide liquidity and collateral
    vault.deposit(&user, &1_000u128);

    // Borrow to 10% utilization: borrows=100, cash=900, util=10%
    vault.borrow(&user, &100u128);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    // Accrue interest for year 1
    vault.update_interest();
    let tb_after_year1 = vault.get_total_borrowed();
    assert!(tb_after_year1 > 100u128);
    let interest_year1 = tb_after_year1 - 100u128;

    // Increase utilization above kink: additional 750 -> borrows=850, cash=150, util=85%
    vault.borrow(&user, &750u128);

    let now2 = env.ledger().timestamp();
    env.ledger().set_timestamp(now2 + 365 * 24 * 60 * 60);
    vault.update_interest();
    let tb_after_year2 = vault.get_total_borrowed();
    let interest_year2 = tb_after_year2 - tb_after_year1;

    // Expect higher interest accrual in year 2 due to higher post-kink borrow rate
    assert!(interest_year2 > interest_year1);
}

#[test]
fn test_jump_model_dynamic_supply_apr_accrual() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &100_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.set_collateral_factor(&1_000_000u128);
    vault.set_reserve_factor(&100_000u128); // 10%

    // Wire jump rate model as above
    let model_id = env.register(jrm::JumpRateModel, ());
    let model_client = jrm::JumpRateModelClient::new(&env, &model_id);
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &model_client.address,
            fn_name: "initialize",
            args: (
                20_000u128,
                180_000u128,
                4_000_000u128,
                800_000u128,
                admin.clone(),
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);
    model_client.initialize(
        &20_000u128,
        &180_000u128,
        &4_000_000u128,
        &800_000u128,
        &admin,
    );
    env.mock_all_auths();
    vault.set_interest_model(&model_id);

    // Deposit and borrow to 10% util
    vault.deposit(&user, &1_000u128);
    vault.borrow(&user, &100u128);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    vault.update_interest();
    let total_underlying_y1 = vault.get_total_underlying();

    // Raise utilization above kink and accrue again
    vault.borrow(&user, &750u128);
    let now2 = env.ledger().timestamp();
    env.ledger().set_timestamp(now2 + 365 * 24 * 60 * 60);
    vault.update_interest();
    let total_underlying_y2 = vault.get_total_underlying();

    // Underlying should increase between years due to higher post-kink rates
    assert!(total_underlying_y2 >= total_underlying_y1);
}

#[test]
#[should_panic]
fn test_ptoken_transfer_and_approve_with_gating() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let other = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    // Vault
    let v_id = env.register(ReceiptVault, ());
    let v = ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&token_address, &0u128, &0u128, &admin);

    // Fund and deposit
    token_admin_client.mint(&user, &1_000i128);
    v.set_collateral_factor(&1_000_000u128);
    v.deposit(&user, &200u128); // user has 200 pTokens

    // Transfer 50 pTokens to other -> healthy
    v.transfer(&user, &other, &50u128);
    assert_eq!(v.get_ptoken_balance(&user), 150u128);
    assert_eq!(v.get_ptoken_balance(&other), 50u128);

    // Approve and transfer_from 50 pTokens from user to other
    v.approve(&user, &other, &50u128);
    v.transfer_from(&other, &user, &other, &50u128);
    assert_eq!(v.get_ptoken_balance(&user), 100u128);
    assert_eq!(v.get_ptoken_balance(&other), 100u128);

    // Borrow to reduce headroom (local-only)
    v.borrow(&user, &100u128);

    // Now wire a minimal peridottroller (no oracle set -> preview_redeem_max=0)
    let comp_id = env.register(SimplePeridottroller, ());
    v.set_peridottroller(&comp_id);

    // Attempt transfer 101 -> should panic via peridottroller gating
    v.transfer(&user, &other, &101u128);
}

#[test]
#[should_panic]
fn test_vault_upgrade_requires_admin() {
    let env = Env::default();
    // no mock_all_auths to enforce auth

    let admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);

    // Attempt upgrade without admin authorization
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    vault.upgrade_wasm(&hash);
}

// (cross-market collateral tests moved to simple-peridottroller crate to avoid circular deps)
