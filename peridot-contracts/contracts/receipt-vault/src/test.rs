#![cfg(test)]

use super::*;
use jump_rate_model as jrm;
use mock_token::MockTokenClient;
use simple_peridottroller::{SimplePeridottroller, SimplePeridottrollerClient};
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::BytesN;
use soroban_sdk::{contract, contractimpl, contracttype};
use soroban_sdk::{testutils::Address as _, token, Address, Bytes, Env, IntoVal, Symbol, Val, Vec};

fn assert_budget_under(env: &Env, max_cpu: u64, max_mem: u64) {
    let budget = env.cost_estimate().budget();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();
    assert!(cpu <= max_cpu, "cpu cost {cpu} exceeds {max_cpu}");
    assert!(mem <= max_mem, "mem cost {mem} exceeds {max_mem}");
}

#[contract]
struct MockOracle;

#[contracttype]
enum OracleKey {
    Price,
    Decimals,
    Resolution,
}

#[contracttype(export = false)]
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum OracleAsset {
    Stellar(Address),
    Other(Symbol),
}

#[contracttype(export = false)]
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OraclePriceData {
    pub price: i128,
    pub timestamp: u64,
}

#[contractimpl]
impl MockOracle {
    pub fn initialize(env: Env, decimals: u32, price: i128) {
        env.storage()
            .persistent()
            .set(&OracleKey::Decimals, &decimals);
        env.storage().persistent().set(&OracleKey::Price, &price);
        env.storage()
            .persistent()
            .set(&OracleKey::Resolution, &1u32);
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&OracleKey::Decimals)
            .unwrap_or(7u32)
    }

    pub fn resolution(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&OracleKey::Resolution)
            .unwrap_or(1u32)
    }

    pub fn lastprice(env: Env, _asset: OracleAsset) -> Option<OraclePriceData> {
        let price: i128 = env
            .storage()
            .persistent()
            .get(&OracleKey::Price)
            .unwrap_or(0);
        Some(OraclePriceData {
            price,
            timestamp: env.ledger().timestamp(),
        })
    }
}

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

fn setup_peridottroller_with_fallback<'a>(
    env: &'a Env,
    admin: &'a Address,
    vault_id: &'a Address,
    token: &'a Address,
    cf: u128,
    price: u128,
    scale: u128,
) -> SimplePeridottrollerClient<'a> {
    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(env, &oracle_id);
    oracle.initialize(&7u32, &1_0000000i128);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(env, &comp_id);
    comp.initialize(admin);
    comp.set_oracle(&oracle_id);
    comp.add_market(vault_id);
    comp.set_market_cf(vault_id, &cf);
    comp.set_price_fallback(token, &Some((price, scale)));
    let vault = ReceiptVaultClient::new(env, vault_id);
    vault.set_peridottroller(&comp_id);
    comp
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

#[contract]
pub struct FailingRewardsPeridottroller;

#[contractimpl]
impl FailingRewardsPeridottroller {
    pub fn accrue_user_market(
        _env: Env,
        _user: Address,
        _market: Address,
        _hint: Option<ControllerAccrualHint>,
    ) {
        panic!("accrual failed");
    }
}

#[contract]
pub struct MockMarginLockController;

#[contracttype]
#[derive(Clone)]
enum MockMarginLockKey {
    Locked(Address, Address), // (user, market)
}

#[contractimpl]
impl MockMarginLockController {
    pub fn set_locked(env: Env, user: Address, market: Address, amount: u128) {
        env.storage()
            .persistent()
            .set(&MockMarginLockKey::Locked(user, market), &amount);
    }

    pub fn locked_ptokens_in_market(env: Env, user: Address, market: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&MockMarginLockKey::Locked(user, market))
            .unwrap_or(0u128)
    }
}

#[contract]
pub struct MockMarginPositionController;

#[contracttype]
#[derive(Clone)]
enum MockMarginPositionKey {
    Owner(u64),
    DebtVault(u64),
}

#[contractimpl]
impl MockMarginPositionController {
    pub fn set_position(env: Env, position_id: u64, owner: Address, debt_vault: Address) {
        env.storage()
            .persistent()
            .set(&MockMarginPositionKey::Owner(position_id), &owner);
        env.storage()
            .persistent()
            .set(&MockMarginPositionKey::DebtVault(position_id), &debt_vault);
    }

    pub fn get_margin_position_owner(env: Env, position_id: u64, debt_vault: Address) -> Address {
        let owner: Address = env
            .storage()
            .persistent()
            .get(&MockMarginPositionKey::Owner(position_id))
            .expect("position owner missing");
        let configured_debt_vault: Address = env
            .storage()
            .persistent()
            .get(&MockMarginPositionKey::DebtVault(position_id))
            .expect("position vault missing");
        if configured_debt_vault != debt_vault {
            panic!("wrong debt vault");
        }
        owner
    }

    pub fn locked_ptokens_in_market(_env: Env, _user: Address, _market: Address) -> u128 {
        0u128
    }
}

#[contract]
pub struct MockBoostedVault;

#[contracttype]
enum BoostedKey {
    Underlying,
    TotalShares,
    Share(Address),
    FailQuote,
    WithdrawHaircut,
    QuoteMultiplierBps,
}

#[contractimpl]
impl MockBoostedVault {
    pub fn initialize(env: Env, underlying: Address) {
        env.storage()
            .persistent()
            .set(&BoostedKey::Underlying, &underlying);
        env.storage()
            .persistent()
            .set(&BoostedKey::TotalShares, &0i128);
        env.storage()
            .persistent()
            .set(&BoostedKey::FailQuote, &false);
        env.storage()
            .persistent()
            .set(&BoostedKey::WithdrawHaircut, &0i128);
        env.storage()
            .persistent()
            .set(&BoostedKey::QuoteMultiplierBps, &1_000_000u128);
    }

    pub fn set_fail_quote(env: Env, fail: bool) {
        env.storage()
            .persistent()
            .set(&BoostedKey::FailQuote, &fail);
    }

    pub fn set_withdraw_haircut(env: Env, haircut: i128) {
        env.storage()
            .persistent()
            .set(&BoostedKey::WithdrawHaircut, &haircut);
    }

    pub fn set_quote_multiplier_bps(env: Env, bps: u128) {
        env.storage()
            .persistent()
            .set(&BoostedKey::QuoteMultiplierBps, &bps);
    }

    pub fn balance(env: Env, owner: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&BoostedKey::Share(owner))
            .unwrap_or(0i128)
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&BoostedKey::TotalShares)
            .unwrap_or(0i128)
    }

    pub fn get_asset_amounts_per_shares(env: Env, shares: i128) -> Vec<i128> {
        if env
            .storage()
            .persistent()
            .get(&BoostedKey::FailQuote)
            .unwrap_or(false)
        {
            panic!("quote failed");
        }
        let mut out = Vec::new(&env);
        if shares <= 0 {
            out.push_back(0i128);
            return out;
        }
        let total_shares = Self::total_supply(env.clone());
        if total_shares <= 0 {
            out.push_back(0i128);
            return out;
        }
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&BoostedKey::Underlying)
            .expect("underlying not set");
        let token_client = token::Client::new(&env, &token_address);
        let underlying_balance = token_client.balance(&env.current_contract_address());
        if underlying_balance <= 0 {
            out.push_back(0i128);
            return out;
        }
        let underlying_for_shares = shares.saturating_mul(underlying_balance) / total_shares;
        let quote_bps: u128 = env
            .storage()
            .persistent()
            .get(&BoostedKey::QuoteMultiplierBps)
            .unwrap_or(1_000_000u128);
        let quoted = underlying_for_shares.saturating_mul(to_i128(quote_bps)) / 1_000_000i128;
        out.push_back(quoted);
        out
    }

    pub fn deposit(
        env: Env,
        amounts_desired: Vec<i128>,
        _amounts_min: Vec<i128>,
        to: Address,
        _auto: bool,
    ) -> i128 {
        let amount = amounts_desired.get(0).unwrap_or(0);
        if amount <= 0 {
            return 0;
        }
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&BoostedKey::Underlying)
            .expect("underlying not set");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&to, &env.current_contract_address(), &amount);

        let prev = Self::balance(env.clone(), to.clone());
        let supply = Self::total_supply(env.clone());
        env.storage()
            .persistent()
            .set(&BoostedKey::Share(to), &(prev.saturating_add(amount)));
        env.storage()
            .persistent()
            .set(&BoostedKey::TotalShares, &(supply.saturating_add(amount)));
        amount
    }

    pub fn withdraw(env: Env, shares: i128, min_amounts_out: Vec<i128>, to: Address) -> i128 {
        if shares <= 0 {
            return 0;
        }
        let min_out = min_amounts_out.get(0).unwrap_or(0);
        let owner_shares = Self::balance(env.clone(), to.clone());
        if owner_shares < shares {
            panic!("insufficient shares");
        }
        let amounts = Self::get_asset_amounts_per_shares(env.clone(), shares);
        let mut out = amounts.get(0).unwrap_or(0);
        let haircut: i128 = env
            .storage()
            .persistent()
            .get(&BoostedKey::WithdrawHaircut)
            .unwrap_or(0i128);
        if haircut > 0 {
            out = out.saturating_sub(haircut);
        }
        if out < min_out {
            panic!("slippage");
        }
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&BoostedKey::Underlying)
            .expect("underlying not set");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &to, &out);

        let supply = Self::total_supply(env.clone());
        env.storage()
            .persistent()
            .set(&BoostedKey::Share(to), &(owner_shares - shares));
        env.storage()
            .persistent()
            .set(&BoostedKey::TotalShares, &(supply - shares));
        out
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
    vault_client.enable_static_rates(&admin);

    // Verify initialization
    assert_eq!(vault_client.get_underlying_token(), token_address);
    assert_eq!(vault_client.get_total_deposited(), 0u128);
    assert_eq!(vault_client.get_total_ptokens(), 0u128);
    assert_eq!(vault_client.get_exchange_rate(), 1_000_000u128); // 1:1 ratio
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_rejects_when_core_keys_exist_even_if_flag_missing() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);

    // Simulate a missing init flag while core state is still present.
    env.as_contract(&vault_contract_id, || {
        env.storage().persistent().remove(&DataKey::Initialized);
    });

    vault_client.initialize(&token_address, &0u128, &0u128, &attacker);
}

#[test]
fn test_borrow_redeems_boosted_liquidity_on_demand() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let lender = Address::generate(&env);
    let borrower = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&lender, &2_000i128);
    token_admin_client.mint(&borrower, &1_000i128);

    let boosted_id = env.register(MockBoostedVault, ());
    let boosted = MockBoostedVaultClient::new(&env, &boosted_id);
    boosted.initialize(&token_address);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);
    vault.set_boosted_vault(&admin, &boosted_id);

    vault.deposit(&lender, &1_200u128);
    vault.deposit(&borrower, &300u128);

    // All deposited liquidity was deployed into boosted vault.
    assert_eq!(token_client.balance(&vault_id), 0i128);
    assert_eq!(boosted.balance(&vault_id), 1_500i128);

    let borrower_before = token_client.balance(&borrower);
    vault.borrow(&borrower, &200u128);
    let borrower_after = token_client.balance(&borrower);

    assert_eq!(borrower_after - borrower_before, 200i128);
    assert_eq!(vault.get_user_borrow_balance(&borrower), 200u128);
    assert_eq!(vault.get_total_borrowed(), 200u128);
}

#[test]
fn test_borrow_uses_donated_cash_without_managed_cash_underflow() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let lender = Address::generate(&env);
    let borrower = Address::generate(&env);
    let donor = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&lender, &2_000i128);
    token_admin_client.mint(&borrower, &1_000i128);
    token_admin_client.mint(&donor, &500i128);

    let boosted_id = env.register(MockBoostedVault, ());
    let boosted = MockBoostedVaultClient::new(&env, &boosted_id);
    boosted.initialize(&token_address);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);
    vault.set_boosted_vault(&admin, &boosted_id);

    vault.deposit(&lender, &1_200u128);
    vault.deposit(&borrower, &300u128);
    // All managed cash is deployed.
    assert_eq!(token_client.balance(&vault_id), 0i128);

    // Donation increases live balance but does not change managed cash.
    token_client.transfer(&donor, &vault_id, &200i128);
    assert_eq!(token_client.balance(&vault_id), 200i128);

    // Borrow should use donated cash and not panic on managed-cash subtraction.
    vault.borrow(&borrower, &100u128);
    assert_eq!(vault.get_total_borrowed(), 100u128);
}

#[test]
#[should_panic(expected = "boosted vault already assigned")]
fn test_set_boosted_vault_rejects_duplicate_assignment_across_markets() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_a_id = env.register(ReceiptVault, ());
    let vault_a = ReceiptVaultClient::new(&env, &vault_a_id);
    vault_a.initialize(&token_address, &0u128, &0u128, &admin);
    vault_a.enable_static_rates(&admin);

    let vault_b_id = env.register(ReceiptVault, ());
    let vault_b = ReceiptVaultClient::new(&env, &vault_b_id);
    vault_b.initialize(&token_address, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&7u32, &1_0000000i128);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.set_oracle(&oracle_id);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.set_market_cf(&vault_a_id, &1_000_000u128);
    comp.set_market_cf(&vault_b_id, &1_000_000u128);
    comp.set_price_fallback(&token_address, &Some((1_000_000u128, 1_000_000u128)));

    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    let boosted_id = env.register(MockBoostedVault, ());
    let boosted = MockBoostedVaultClient::new(&env, &boosted_id);
    boosted.initialize(&token_address);

    vault_a.set_boosted_vault(&admin, &boosted_id);
    // Must fail: peridottroller registry enforces one boosted pool per market.
    vault_b.set_boosted_vault(&admin, &boosted_id);
}

#[test]
fn test_boosted_fallback_prefers_cached_value_when_stale_and_quote_fails() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&depositor, &2_000i128);

    let boosted_id = env.register(MockBoostedVault, ());
    let boosted = MockBoostedVaultClient::new(&env, &boosted_id);
    boosted.initialize(&token_address);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_boosted_vault(&admin, &boosted_id);

    vault.deposit(&depositor, &1_000u128);
    assert_eq!(boosted.balance(&vault_id), 1_000i128);

    // Simulate boosted yield by increasing underlying held by boosted vault.
    token_admin_client.mint(&boosted_id, &200i128);
    assert_eq!(token_client.balance(&boosted_id), 1_200i128);

    let healthy_total = vault.get_total_underlying();
    assert_eq!(healthy_total, 1_200u128);

    // Force quote failures and age cache beyond max freshness.
    boosted.set_fail_quote(&true);
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + (60 * 60 + 5) as u64);

    // Stale-failure path should not drop below last cached boosted value.
    let fallback_total = vault.get_total_underlying();
    assert_eq!(fallback_total, healthy_total);
}

#[test]
fn test_set_idle_cash_buffer_bps() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);
    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    assert_eq!(vault.get_idle_cash_buffer_bps(), 0u32);
    vault.set_idle_cash_buffer_bps(&admin, &1_500u32);
    assert_eq!(vault.get_idle_cash_buffer_bps(), 1_500u32);
}

#[test]
fn test_deposit_respects_idle_cash_buffer() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&depositor, &2_000i128);

    let boosted_id = env.register(MockBoostedVault, ());
    let boosted = MockBoostedVaultClient::new(&env, &boosted_id);
    boosted.initialize(&token_address);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_boosted_vault(&admin, &boosted_id);
    // Keep 10% idle in vault cash.
    vault.set_idle_cash_buffer_bps(&admin, &1_000u32);

    vault.deposit(&depositor, &1_000u128);

    assert_eq!(token_client.balance(&vault_id), 100i128);
    assert_eq!(boosted.balance(&vault_id), 900i128);
}

#[test]
fn test_rebalance_idle_cash_deploys_excess() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&depositor, &2_000i128);

    let boosted_id = env.register(MockBoostedVault, ());
    let boosted = MockBoostedVaultClient::new(&env, &boosted_id);
    boosted.initialize(&token_address);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_boosted_vault(&admin, &boosted_id);

    // Start with 100% idle cash.
    vault.set_idle_cash_buffer_bps(&admin, &10_000u32);
    vault.deposit(&depositor, &1_000u128);
    assert_eq!(token_client.balance(&vault_id), 1_000i128);
    assert_eq!(boosted.balance(&vault_id), 0i128);

    // Lower target to 10% and rebalance.
    vault.set_idle_cash_buffer_bps(&admin, &1_000u32);
    vault.rebalance_idle_cash(&admin);

    assert_eq!(token_client.balance(&vault_id), 100i128);
    assert_eq!(boosted.balance(&vault_id), 900i128);
}

#[test]
fn test_bump_user_borrow_ttl_permissionless() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);
    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    vault.bump_user_borrow_ttl(&user);
}

#[test]
#[should_panic(expected = "borrow state missing")]
fn test_missing_borrow_state_panics_for_collateralized_account() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&user, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.deposit(&user, &500u128);

    // Simulate archival/missing state for a collateralized account.
    env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::BorrowSnapshots(user.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::HasBorrowed(user.clone()));
    });

    let _ = vault.get_user_borrow_balance(&user);
}

#[test]
fn test_recover_user_borrow_snapshot_restores_missing_state() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&user, &2_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);
    vault.deposit(&user, &1_000u128);
    vault.borrow(&user, &100u128);
    let mut users = Vec::new(&env);
    users.push_back(user.clone());
    vault.migrate_borrow_state_batch(&users);

    env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::BorrowSnapshots(user.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::HasBorrowed(user.clone()));
    });

    let index: u128 = env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing")
    });
    vault.recover_user_borrow_snapshot(&admin, &user, &100u128, &index);
    assert_eq!(vault.get_user_borrow_balance(&user), 100u128);
}

#[test]
fn test_permissionless_recover_borrow_snapshot_restores_from_canonical_principal() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&user, &2_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);
    vault.deposit(&user, &1_000u128);
    vault.borrow(&user, &100u128);
    let mut users = Vec::new(&env);
    users.push_back(user.clone());
    vault.migrate_borrow_state_batch(&users);

    env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::BorrowSnapshots(user.clone()));
    });

    // Rebuild using canonical principal mirror without admin intervention.
    vault.recover_borrow_snapshot(&user);
    assert_eq!(vault.get_user_borrow_balance(&user), 100u128);
}

#[test]
#[should_panic(expected = "borrow snapshot missing")]
fn test_get_user_borrow_balance_missing_snapshot_panics_without_recovery() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&user, &2_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);
    vault.deposit(&user, &1_000u128);
    vault.borrow(&user, &100u128);

    env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::BorrowSnapshots(user.clone()));
    });

    let _ = vault.get_user_borrow_balance(&user);
}

#[test]
#[should_panic(expected = "non-empty vault at zero supply")]
fn test_exchange_rate_reverts_on_zero_supply_with_residual_underlying() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    // Craft inconsistent residual economics with zero pToken supply.
    env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::TotalBorrowed, &1u128);
    });

    vault.get_exchange_rate();
}

#[test]
fn test_core_ttl_bumps_all_critical_config_keys() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let model_admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let boosted_vault = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);

    let _comp = setup_peridottroller_with_fallback(
        &env,
        &admin,
        &vault_contract_id,
        &token_address,
        800_000u128,
        1_000_000u128,
        1_000_000u128,
    );

    let model_id = env.register(jrm::JumpRateModel, ());
    let model_client = jrm::JumpRateModelClient::new(&env, &model_id);
    model_client.initialize(
        &20_000u128,
        &180_000u128,
        &4_000_000u128,
        &800_000u128,
        &model_admin,
    );

    env.mock_all_auths_allowing_non_root_auth();
    vault_client.set_interest_model(&model_id);
    vault_client.set_collateral_factor(&750_000u128);
    vault_client.set_reserve_factor(&100_000u128);
    vault_client.set_admin_fee(&50_000u128);
    vault_client.set_flash_loan_fee(&3_000u128);
    vault_client.set_supply_cap(&5_000u128);
    vault_client.set_borrow_cap(&2_500u128);
    vault_client.set_idle_cash_buffer_bps(&admin, &500u32);
    vault_client.set_boosted_vault(&admin, &boosted_vault);

    // Any initialized read path should now bump all critical config keys.
    let _ = vault_client.get_underlying_token();

    env.as_contract(&vault_contract_id, || {
        fn assert_bumped(env: &Env, key: &DataKey, label: &str) {
            let ttl = env.storage().persistent().get_ttl(key);
            assert!(ttl > 100_000, "expected bumped ttl for {label}, got {ttl}");
        }

        assert_bumped(&env, &DataKey::Admin, "Admin");
        assert_bumped(&env, &DataKey::UnderlyingToken, "UnderlyingToken");
        assert_bumped(&env, &DataKey::Initialized, "Initialized");
        assert_bumped(
            &env,
            &DataKey::CollateralFactorScaled,
            "CollateralFactorScaled",
        );
        assert_bumped(&env, &DataKey::Peridottroller, "Peridottroller");
        assert_bumped(&env, &DataKey::InterestModel, "InterestModel");
        assert_bumped(&env, &DataKey::ReserveFactorScaled, "ReserveFactorScaled");
        assert_bumped(&env, &DataKey::AdminFeeScaled, "AdminFeeScaled");
        assert_bumped(&env, &DataKey::FlashLoanFeeScaled, "FlashLoanFeeScaled");
        assert_bumped(&env, &DataKey::TotalAdminFees, "TotalAdminFees");
        assert_bumped(&env, &DataKey::TotalReserves, "TotalReserves");
        assert_bumped(&env, &DataKey::SupplyCap, "SupplyCap");
        assert_bumped(&env, &DataKey::BorrowCap, "BorrowCap");
        assert_bumped(&env, &DataKey::RatesReady, "RatesReady");
        assert_bumped(&env, &DataKey::IdleCashBufferBps, "IdleCashBufferBps");
        assert_bumped(&env, &DataKey::BoostedVault, "BoostedVault");
        assert_bumped(&env, &DataKey::InitialExchangeRate, "InitialExchangeRate");
    });
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
    vault_client.enable_static_rates(&admin);
}

#[test]
#[should_panic(expected = "invalid rate relationship")]
fn test_initialize_rejects_supply_rate_above_borrow_rate() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    vault_client.initialize(&token_address, &100_000u128, &50_000u128, &admin);
}

#[test]
#[should_panic(expected = "rates not configured")]
fn test_borrow_rejected_until_interest_mode_ready() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lender = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1_000i128);
    token_admin_client.mint(&lender, &1_000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_contract_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.set_collateral_factor(&1_000_000u128);
    vault.deposit(&lender, &500u128);
    vault.deposit(&user, &300u128);

    // No model set and static mode not enabled.
    vault.borrow(&user, &1u128);
}

#[test]
#[should_panic(expected = "invalid borrow rate")]
fn test_set_borrow_rate_rejects_large_value() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);
    vault_client.set_borrow_rate(&12_000_000u128);
}

#[test]
fn test_deposit_receives_ptokens() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint some tokens to the user
    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault (0% interest)
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);

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
#[should_panic(expected = "reward accrual failed")]
fn test_deposit_reverts_when_reward_accrual_fails() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&user, &500i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    let failing_comp_id = env.register(FailingRewardsPeridottroller, ());
    env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Peridottroller, &failing_comp_id);
    });

    vault.deposit(&user, &100u128);
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
    vault_client.enable_static_rates(&admin);
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
#[should_panic(expected = "collateral locked")]
fn test_withdraw_rejects_margin_locked_ptokens() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&user, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.deposit(&user, &100u128);

    let margin_controller_id = env.register(MockMarginLockController, ());
    let margin_controller = MockMarginLockControllerClient::new(&env, &margin_controller_id);
    vault.set_margin_controller(&admin, &Some(margin_controller_id.clone()));
    margin_controller.set_locked(&user, &vault_id, &100u128);

    vault.withdraw(&user, &1u128);
}

#[test]
#[should_panic(expected = "margin borrow state missing")]
fn test_get_margin_borrow_balance_missing_state_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    let _ = vault.get_margin_borrow_balance(&77u64);
}

#[test]
#[should_panic(expected = "receiver must be position owner")]
fn test_borrow_for_margin_rejects_non_owner_receiver() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let attacker = Address::generate(&env);
    let lender = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&lender, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.deposit(&lender, &500u128);

    let margin_ctrl_id = env.register(MockMarginPositionController, ());
    let margin_ctrl = MockMarginPositionControllerClient::new(&env, &margin_ctrl_id);
    vault.set_margin_controller(&admin, &Some(margin_ctrl_id.clone()));

    let position_id = 1u64;
    margin_ctrl.set_position(&position_id, &user, &vault_id);
    vault.init_margin_borrow_state(&position_id);

    vault.borrow_for_margin(&position_id, &attacker, &1u128);
}

#[test]
fn test_margin_borrow_repay_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lender = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&lender, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.deposit(&lender, &500u128);

    let margin_ctrl_id = env.register(MockMarginPositionController, ());
    let margin_ctrl = MockMarginPositionControllerClient::new(&env, &margin_ctrl_id);
    vault.set_margin_controller(&admin, &Some(margin_ctrl_id.clone()));

    let position_id = 7u64;
    margin_ctrl.set_position(&position_id, &user, &vault_id);
    vault.init_margin_borrow_state(&position_id);
    assert_eq!(vault.get_margin_borrow_balance(&position_id), 0u128);

    vault.borrow_for_margin(&position_id, &user, &100u128);
    assert_eq!(vault.get_margin_borrow_balance(&position_id), 100u128);
    assert_eq!(vault.get_total_borrowed(), 100u128);
    assert_eq!(token_client.balance(&user), 100i128);

    vault.repay_for_margin(&position_id, &user, &40u128);
    assert_eq!(vault.get_margin_borrow_balance(&position_id), 60u128);
    assert_eq!(vault.get_total_borrowed(), 60u128);

    vault.repay_for_margin(&position_id, &user, &60u128);
    assert_eq!(vault.get_margin_borrow_balance(&position_id), 0u128);
    assert_eq!(vault.get_total_borrowed(), 0u128);
}

#[test]
fn test_recover_margin_borrow_snapshot_restores_missing_state() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lender = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&lender, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.deposit(&lender, &500u128);

    let margin_ctrl_id = env.register(MockMarginPositionController, ());
    let margin_ctrl = MockMarginPositionControllerClient::new(&env, &margin_ctrl_id);
    vault.set_margin_controller(&admin, &Some(margin_ctrl_id.clone()));

    let position_id = 8u64;
    margin_ctrl.set_position(&position_id, &user, &vault_id);
    vault.init_margin_borrow_state(&position_id);
    vault.borrow_for_margin(&position_id, &user, &50u128);
    assert_eq!(vault.get_margin_borrow_balance(&position_id), 50u128);

    env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::MarginBorrowSnapshots(position_id));
        env.storage()
            .persistent()
            .remove(&DataKey::MarginHasBorrowed(position_id));
    });

    let index: u128 = env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing")
    });
    vault.recover_margin_borrow_snapshot(&admin, &position_id, &50u128, &index);
    assert_eq!(vault.get_margin_borrow_balance(&position_id), 50u128);
}

#[test]
fn test_bump_margin_borrow_ttl_permissionless() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lender = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&lender, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.deposit(&lender, &500u128);

    let margin_ctrl_id = env.register(MockMarginPositionController, ());
    let margin_ctrl = MockMarginPositionControllerClient::new(&env, &margin_ctrl_id);
    vault.set_margin_controller(&admin, &Some(margin_ctrl_id.clone()));

    let position_id = 9u64;
    margin_ctrl.set_position(&position_id, &user, &vault_id);
    vault.init_margin_borrow_state(&position_id);
    vault.borrow_for_margin(&position_id, &user, &1u128);

    vault.bump_margin_borrow_ttl(&position_id);
    env.as_contract(&vault_id, || {
        let snap_ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::MarginBorrowSnapshots(position_id));
        let flag_ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::MarginHasBorrowed(position_id));
        assert!(
            snap_ttl > 100_000,
            "expected bumped ttl for margin borrow snapshot, got {snap_ttl}"
        );
        assert!(
            flag_ttl > 100_000,
            "expected bumped ttl for margin borrow flag, got {flag_ttl}"
        );
    });
}

#[test]
fn test_multiple_users_with_ptokens() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

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
    vault_client.enable_static_rates(&admin);

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
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize with 10% yearly interest (scaled 1e6 = 0.10e6)
    let yearly_rate = 100_000u128; // 10%
    vault_client.initialize(&token_address, &yearly_rate, &yearly_rate, &admin);
    vault_client.enable_static_rates(&admin);

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
fn test_interest_model_accrual_does_not_credit_accumulated_interest() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let model_admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &2_000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);

    // Deploy and wire a jump rate model to drive dynamic interest.
    let model_id = env.register(jrm::JumpRateModel, ());
    let model_client = jrm::JumpRateModelClient::new(&env, &model_id);
    model_client.initialize(
        &20_000u128,
        &180_000u128,
        &4_000_000u128,
        &800_000u128,
        &model_admin,
    );
    env.mock_all_auths_allowing_non_root_auth();
    vault_client.set_interest_model(&model_id);

    // Provide liquidity and create an outstanding borrow so interest can accrue.
    vault_client.deposit(&user, &500u128);
    vault_client.borrow(&user, &200u128);

    // Advance time and force an interest update.
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 30 * 24 * 60 * 60);
    vault_client.update_interest();

    // Supplier yield is reflected via total borrowed/exchange rate, not by
    // separately crediting AccumulatedInterest.
    let accrued: u128 = env.as_contract(&vault_contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::AccumulatedInterest)
            .unwrap_or(0u128)
    });
    assert_eq!(accrued, 0u128);
}

#[test]
#[should_panic(expected = "Insufficient pTokens")]
fn test_withdraw_insufficient_ptokens() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint some tokens to the user
    token_admin_client.mint(&user, &100i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);

    // Deposit 50, get 50 pTokens
    vault_client.deposit(&user, &50u128);

    // Try to withdraw using 100 pTokens (should panic)
    vault_client.withdraw(&user, &100u128);
}

#[test]
#[should_panic(expected = "Vault not initialized")]
fn test_deposit_uninitialized_vault() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let user = Address::generate(&env);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Try to deposit without initializing (should panic)
    vault_client.deposit(&user, &100u128);
}

#[test]
fn test_zero_balance_users() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize the vault
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);

    // Check balance of user who never deposited
    assert_eq!(vault_client.get_user_balance(&user), 0u128);
    assert_eq!(vault_client.get_ptoken_balance(&user), 0u128);
}

#[test]
fn test_reserve_accrual_and_reduce() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let token_address = env.register(mock_token::MockToken, ());
    let token_client = MockTokenClient::new(&env, &token_address);
    token_client.initialize(
        &soroban_sdk::String::from_str(&env, "Mock Token"),
        &soroban_sdk::String::from_str(&env, "MOCK"),
        &7u32,
    );

    // Mint tokens to the user for liquidity and collateral
    token_client.mint(&user, &10_000i128);

    // Deploy vault
    let vault_contract_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_contract_id);

    // Initialize: 0% supply, 100% borrow; admin is admin
    vault.initialize(&token_address, &0u128, &1_000_000u128, &admin);
    vault.enable_static_rates(&admin);

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
    vault.reduce_reserves(&5u128);
    assert_eq!(vault.get_total_reserves(), 15u128);
}

#[test]
fn test_borrow_and_repay_flow() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // 0% supply, 0% borrow to simplify
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);

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
fn test_borrow_with_peridottroller_same_market_hint() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&500_000u128);

    let comp = setup_peridottroller_with_fallback(
        &env,
        &admin,
        &vault_id,
        &token_address,
        500_000u128,
        1_000_000u128,
        1_000_000u128,
    );

    token_admin.mint(&admin, &to_i128(1_000_000u128));
    vault.deposit(&admin, &1_000_000u128);

    token_admin.mint(&user, &to_i128(200u128));
    vault.deposit(&user, &200u128);
    comp.enter_market(&user, &vault_id);

    vault.borrow(&user, &80u128);
    assert_eq!(vault.get_user_borrow_balance(&user), 80u128);
}

#[test]
fn test_repay_on_behalf_via_peridottroller_auth() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &500i128);
    token_admin_client.mint(&liquidator, &500i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    let comp = setup_peridottroller_with_fallback(
        &env,
        &admin,
        &vault_id,
        &token_address,
        800_000u128,
        1_000_000u128,
        1_000_000u128,
    );

    vault.deposit(&user, &200u128);
    comp.enter_market(&user, &vault_id);
    vault.borrow(&user, &100u128);
    let live_until = env.ledger().sequence().saturating_add(100_000);
    token_client.approve(&liquidator, &vault_id, &500i128, &live_until);

    let debt_before = vault.get_user_borrow_balance(&user);
    assert_eq!(debt_before, 100u128);

    let comp_id = comp.address.clone();
    env.as_contract(&comp_id, || {
        let repay_args: Vec<Val> = (liquidator.clone(), user.clone(), 40u128).into_val(&env);
        let mut auths = Vec::new(&env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: vault_id.clone(),
                fn_name: Symbol::new(&env, "repay_on_behalf"),
                args: repay_args,
            },
            sub_invocations: Vec::new(&env),
        }));
        env.authorize_as_current_contract(auths);
        let vault_client = ReceiptVaultClient::new(&env, &vault_id);
        vault_client.repay_on_behalf(&liquidator, &user, &40u128);
    });

    let debt_after = vault.get_user_borrow_balance(&user);
    assert_eq!(debt_after, 60u128);

    let liquidator_balance = token_client.balance(&liquidator);
    assert_eq!(liquidator_balance, 460i128);
}

#[test]
fn test_repay_overpay_after_interest_accrual_uses_pre_accrual_cap() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 1);

    let admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // 100% APR borrow rate so one-year elapsed time produces material interest.
    vault_client.initialize(&token_address, &0u128, &1_000_000u128, &admin);
    vault_client.enable_static_rates(&admin);

    vault_client.deposit(&user, &200u128);
    vault_client.borrow(&user, &100u128);
    assert_eq!(vault_client.get_user_borrow_balance(&user), 100u128);
    assert_eq!(token_client.balance(&user), 900i128);

    let t0 = env.ledger().timestamp();
    env.ledger()
        .with_mut(|l| l.timestamp = t0 + 365 * 24 * 60 * 60);

    // Overpay request is deterministically capped to pre-accrual debt (100).
    vault_client.repay(&user, &1000u128);

    // User only repaid the pre-accrual cap.
    assert_eq!(token_client.balance(&user), 800i128);
    // Interest accrued during update_interest remains outstanding.
    assert!(vault_client.get_user_borrow_balance(&user) > 0u128);
}

#[test]
fn test_repay_on_behalf_overpay_after_interest_accrual_uses_pre_accrual_cap() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 1);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let liquidator = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &500i128);
    token_admin_client.mint(&liquidator, &500i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    // 100% APR borrow rate so one-year elapsed time produces material interest.
    vault.initialize(&token_address, &0u128, &1_000_000u128, &admin);
    vault.enable_static_rates(&admin);

    let comp = setup_peridottroller_with_fallback(
        &env,
        &admin,
        &vault_id,
        &token_address,
        800_000u128,
        1_000_000u128,
        1_000_000u128,
    );

    vault.deposit(&user, &200u128);
    comp.enter_market(&user, &vault_id);
    vault.borrow(&user, &100u128);
    assert_eq!(vault.get_user_borrow_balance(&user), 100u128);
    let live_until = env.ledger().sequence().saturating_add(100_000);
    token_client.approve(&liquidator, &vault_id, &500i128, &live_until);

    let t0 = env.ledger().timestamp();
    env.ledger()
        .with_mut(|l| l.timestamp = t0 + 365 * 24 * 60 * 60);

    // Overpay request is deterministically capped to pre-accrual debt (100).
    let comp_id = comp.address.clone();
    env.as_contract(&comp_id, || {
        let repay_args: Vec<Val> = (liquidator.clone(), user.clone(), 1000u128).into_val(&env);
        let mut auths = Vec::new(&env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: vault_id.clone(),
                fn_name: Symbol::new(&env, "repay_on_behalf"),
                args: repay_args,
            },
            sub_invocations: Vec::new(&env),
        }));
        env.authorize_as_current_contract(auths);
        let vault_client = ReceiptVaultClient::new(&env, &vault_id);
        vault_client.repay_on_behalf(&liquidator, &user, &1000u128);
    });

    assert_eq!(token_client.balance(&liquidator), 400i128);
    assert!(vault.get_user_borrow_balance(&user) > 0u128);
}

#[test]
fn test_borrow_budget_peridottroller_same_market() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&500_000u128);

    let comp = setup_peridottroller_with_fallback(
        &env,
        &admin,
        &vault_id,
        &token_address,
        500_000u128,
        1_000_000u128,
        1_000_000u128,
    );

    token_admin.mint(&admin, &to_i128(1_000_000u128));
    vault.deposit(&admin, &1_000_000u128);

    token_admin.mint(&user, &to_i128(200u128));
    vault.deposit(&user, &200u128);
    comp.enter_market(&user, &vault_id);

    vault.borrow(&user, &80u128);
    assert_budget_under(&env, 5_300_000, 950_000);
}

#[test]
fn test_borrow_with_peridottroller_same_market_exact_threshold() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&500_000u128);

    let comp = setup_peridottroller_with_fallback(
        &env,
        &admin,
        &vault_id,
        &token_address,
        500_000u128,
        1_000_000u128,
        1_000_000u128,
    );

    token_admin.mint(&admin, &to_i128(1_000_000u128));
    vault.deposit(&admin, &1_000_000u128);

    token_admin.mint(&user, &to_i128(200u128));
    vault.deposit(&user, &200u128);
    comp.enter_market(&user, &vault_id);

    vault.borrow(&user, &100u128);
    assert_eq!(vault.get_user_borrow_balance(&user), 100u128);
}

#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_borrow_with_peridottroller_same_market_exceeds_threshold() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&500_000u128);

    let comp = setup_peridottroller_with_fallback(
        &env,
        &admin,
        &vault_id,
        &token_address,
        500_000u128,
        1_000_000u128,
        1_000_000u128,
    );

    token_admin.mint(&admin, &to_i128(1_000_000u128));
    vault.deposit(&admin, &1_000_000u128);

    token_admin.mint(&user, &to_i128(200u128));
    vault.deposit(&user, &200u128);
    comp.enter_market(&user, &vault_id);

    vault.borrow(&user, &101u128);
}

#[test]
fn test_track_borrow_market_entrypoint_records_market() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_a, _token_a_client, _token_a_admin) = create_test_token(&env, &admin);
    let (token_b, _token_b_client, _token_b_admin) = create_test_token(&env, &admin);

    let vault_a_id = env.register(ReceiptVault, ());
    let vault_a = ReceiptVaultClient::new(&env, &vault_a_id);
    vault_a.initialize(&token_a, &0u128, &0u128, &admin);
    vault_a.enable_static_rates(&admin);
    vault_a.set_collateral_factor(&1_000_000u128);

    let vault_b_id = env.register(ReceiptVault, ());
    let vault_b = ReceiptVaultClient::new(&env, &vault_b_id);
    vault_b.initialize(&token_b, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);
    vault_b.set_collateral_factor(&1_000_000u128);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&7u32, &1_0000000i128);

    let comp_id = env.register(SimplePeridottroller, ());
    let comp = SimplePeridottrollerClient::new(&env, &comp_id);
    comp.initialize(&admin);
    comp.set_oracle(&oracle_id);
    comp.add_market(&vault_a_id);
    comp.add_market(&vault_b_id);
    comp.set_market_cf(&vault_a_id, &1_000_000u128);
    comp.set_market_cf(&vault_b_id, &1_000_000u128);
    comp.set_price_fallback(&token_a, &Some((1_000_000u128, 1_000_000u128)));
    comp.set_price_fallback(&token_b, &Some((1_000_000u128, 1_000_000u128)));

    vault_a.set_peridottroller(&comp_id);
    vault_b.set_peridottroller(&comp_id);

    // User explicitly enters only collateral market A.
    comp.enter_market(&user, &vault_a_id);
    let before = comp.get_user_markets(&user);
    assert_eq!(before.len(), 1);

    // Simulate market-authenticated tracking call from vault B.
    env.as_contract(&vault_b_id, || {
        comp.track_borrow_market(&user, &vault_b_id);
    });

    let markets = comp.get_user_markets(&user);
    assert!(markets.contains(vault_a_id.clone()));
    assert!(markets.contains(vault_b_id.clone()));
}

#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_withdraw_with_peridottroller_same_market_blocks_undercollateralized() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&500_000u128);

    let comp = setup_peridottroller_with_fallback(
        &env,
        &admin,
        &vault_id,
        &token_address,
        500_000u128,
        1_000_000u128,
        1_000_000u128,
    );

    token_admin.mint(&admin, &to_i128(1_000_000u128));
    vault.deposit(&admin, &1_000_000u128);

    token_admin.mint(&user, &to_i128(200u128));
    vault.deposit(&user, &200u128);
    comp.enter_market(&user, &vault_id);
    vault.borrow(&user, &100u128);

    vault.withdraw(&user, &1u128);
}

#[test]
fn test_borrow_interest_accrues_and_index_updates() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // 0% supply, 10% borrow
    let borrow_rate = 100_000u128; // 10%
    vault_client.initialize(&token_address, &0u128, &borrow_rate, &admin);
    vault_client.enable_static_rates(&admin);

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
#[should_panic(expected = "borrow index overflow")]
fn test_update_interest_reverts_on_borrow_index_overflow() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let lender = Address::generate(&env);
    let borrower = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&lender, &1_000i128);
    token_admin_client.mint(&borrower, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &1_000_000u128, &admin); // 100% borrow APR
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);

    vault.deposit(&lender, &500u128);
    vault.deposit(&borrower, &200u128);
    vault.borrow(&borrower, &100u128);

    // Force old_index to max so one full-year accrual overflows on checked add.
    env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::BorrowIndex, &u128::MAX);
    });

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    vault.update_interest();
}

#[test]
#[should_panic(expected = "supply cap exceeded")]
fn test_supply_cap_enforced_on_deposit() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    // Mint to user
    token_admin_client.mint(&user, &1_000i128);

    // Vault
    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

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
    env.mock_all_auths_allowing_non_root_auth();

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
    vault.enable_static_rates(&admin);

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

#[test]
fn test_borrow_cap_uses_principal_not_interest() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lender = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &2_000i128);
    token_admin_client.mint(&lender, &2_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);

    // Provide liquidity + collateral
    vault.deposit(&lender, &1_000u128);
    vault.deposit(&user, &300u128);

    // Model: 0% supply, 100% borrow.
    let model_id = env.register(MockRateModel, ());
    let model = MockRateModelClient::new(&env, &model_id);
    model.initialize(&0u128, &1_000_000u128);
    vault.set_interest_model(&model_id);

    // Cap is enforced on principal outstanding.
    vault.set_borrow_cap(&220u128);
    vault.borrow(&user, &100u128);

    // Accrue one year so interest inflates TotalBorrowed.
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    vault.update_interest();

    let total_borrowed_before = vault.get_total_borrowed();
    assert!(
        total_borrowed_before.saturating_add(30u128) > 220u128,
        "sanity: old cap logic (using total borrowed) would reject this borrow"
    );

    // Should succeed because principal outstanding is 100, so 100 + 30 <= 220.
    vault.borrow(&user, &30u128);

    let user_debt = vault.get_user_borrow_balance(&user);
    assert!(user_debt >= 130u128);
}

#[test]
#[should_panic(expected = "borrow cap exceeded")]
fn test_borrow_cap_not_released_by_interest_only_repay() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lender = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &5_000i128);
    token_admin_client.mint(&lender, &5_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);

    vault.deposit(&lender, &1_000u128);
    vault.deposit(&user, &500u128);

    let model_id = env.register(MockRateModel, ());
    let model = MockRateModelClient::new(&env, &model_id);
    model.initialize(&0u128, &1_000_000u128);
    vault.set_interest_model(&model_id);

    vault.set_borrow_cap(&100u128);
    vault.borrow(&user, &100u128);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    vault.update_interest();

    let debt_after_accrual = vault.get_user_borrow_balance(&user);
    assert!(debt_after_accrual > 100u128);
    let interest_only = debt_after_accrual - 100u128;
    vault.repay(&user, &interest_only);

    // Principal outstanding is still 100, so additional borrow must fail.
    vault.borrow(&user, &1u128);
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
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

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

    // With no borrows, supply yield is not minted from thin air.
    assert_eq!(vault.get_total_underlying(), 100u128);
    assert!(vault.get_exchange_rate() >= 1_000_000u128);
}

#[test]
fn test_interest_model_borrow_accrual_and_reserves() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &10_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

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
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);

    // Deposit small amount -> low collateral
    vault_client.deposit(&user, &10u128);

    // Try to borrow more than 50% of collateral
    vault_client.borrow(&user, &100u128);
}

#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_borrow_insufficient_liquidity() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user_a, &2000i128);
    token_admin_client.mint(&user_b, &2000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);

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
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

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
fn test_flash_loan_redeems_boosted_liquidity_on_demand() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    let boosted_id = env.register(MockBoostedVault, ());
    let boosted = MockBoostedVaultClient::new(&env, &boosted_id);
    boosted.initialize(&token_address);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_boosted_vault(&admin, &boosted_id);

    token_admin_client.mint(&depositor, &1_000i128);
    vault.deposit(&depositor, &500u128);

    // Boosted deposit path deploys all live cash.
    assert_eq!(token_client.balance(&vault_id), 0i128);
    assert_eq!(boosted.balance(&vault_id), 500i128);

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

    // Buffered redemption pulls 101 from boosted, then 100 is loaned out and
    // 100 + fee is repaid, leaving 1 extra unit in live cash.
    assert_eq!(boosted.balance(&vault_id), 399i128);
    assert_eq!(
        token_client.balance(&vault_id),
        (101 + expected_fee) as i128
    );
    assert_eq!(vault.get_total_reserves(), expected_fee);
}

#[test]
fn test_flash_loan_boosted_redemption_tolerates_small_rounding_delta() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    let boosted_id = env.register(MockBoostedVault, ());
    let boosted = MockBoostedVaultClient::new(&env, &boosted_id);
    boosted.initialize(&token_address);
    boosted.set_withdraw_haircut(&1i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_boosted_vault(&admin, &boosted_id);

    token_admin_client.mint(&depositor, &1_000i128);
    vault.deposit(&depositor, &500u128);
    assert_eq!(token_client.balance(&vault_id), 0i128);

    let fee_scaled = 20_000u128; // 2%
    vault.set_flash_loan_fee(&fee_scaled);

    let receiver_id = env.register(FlashLoanRepayer, ());
    let receiver_client = FlashLoanRepayerClient::new(&env, &receiver_id);
    receiver_client.configure(&token_address);
    token_admin_client.mint(&receiver_id, &50i128);

    // With a 1-unit redemption haircut, strict no-buffer redemption could fail.
    // Buffered redemption should still source enough live cash for the loan.
    let amount = 100u128;
    let data = Bytes::new(&env);
    vault.flash_loan(&receiver_id, &amount, &data);
}

#[test]
#[should_panic]
fn test_flash_loan_requires_receiver_auth() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    token_admin_client.mint(&depositor, &1_000i128);
    vault.deposit(&depositor, &500u128);
    vault.set_flash_loan_fee(&20_000u128);

    let receiver_id = env.register(FlashLoanRepayer, ());
    let receiver_client = FlashLoanRepayerClient::new(&env, &receiver_id);
    receiver_client.configure(&token_address);
    token_admin_client.mint(&receiver_id, &50i128);

    // Explicitly remove mocked auth entries so receiver.require_auth() must fail.
    env.set_auths(&[]);
    let data = Bytes::new(&env);
    vault.flash_loan(&receiver_id, &100u128, &data);
}

#[test]
#[should_panic(expected = "flash loan not repaid")]
fn test_flash_loan_missing_fee_panics() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

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
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1000i128);

    let vault_contract_id = env.register(ReceiptVault, ());
    let vault_client = ReceiptVaultClient::new(&env, &vault_contract_id);

    // initialize sets admin = invoker; in test env, invoker is Address(0) unless auth mocked, so call via contract client with mock_all_auths covers auth
    vault_client.initialize(&token_address, &0u128, &0u128, &admin);
    vault_client.enable_static_rates(&admin);

    // Expect setters callable under mocked auth
    vault_client.set_collateral_factor(&600_000u128);
    vault_client.set_borrow_rate(&100_000u128);
    vault_client.set_interest_rate(&50_000u128);
}

#[test]
fn test_vault_set_admin_transfers_admin() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (token_address, _token_client, _token_admin_client) = create_test_token(&env, &admin);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    assert_eq!(vault.get_admin(), admin);
    vault.set_admin(&new_admin);
    assert_eq!(vault.get_admin(), admin);
    vault.accept_admin();
    assert_eq!(vault.get_admin(), new_admin);
}

#[test]
fn test_jump_model_dynamic_borrow_apr_accrual() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let model_admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &100_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);

    // Wire jump rate model: base=2%, multiplier=18%, jump=400%, kink=80%
    let model_id = env.register(jrm::JumpRateModel, ());
    let model_client = jrm::JumpRateModelClient::new(&env, &model_id);
    model_client.initialize(
        &20_000u128,
        &180_000u128,
        &4_000_000u128,
        &800_000u128,
        &model_admin,
    );
    env.mock_all_auths_allowing_non_root_auth();
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
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let model_admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &100_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);
    vault.set_reserve_factor(&100_000u128); // 10%

    // Wire jump rate model as above
    let model_id = env.register(jrm::JumpRateModel, ());
    let model_client = jrm::JumpRateModelClient::new(&env, &model_id);
    model_client.initialize(
        &20_000u128,
        &180_000u128,
        &4_000_000u128,
        &800_000u128,
        &model_admin,
    );
    env.mock_all_auths_allowing_non_root_auth();
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
fn test_update_interest_uses_gross_cash_for_model_with_boosted_assets() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let model_admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&user, &10_000i128);

    let boosted_id = env.register(MockBoostedVault, ());
    let boosted = MockBoostedVaultClient::new(&env, &boosted_id);
    boosted.initialize(&token_address);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);
    vault.set_boosted_vault(&admin, &boosted_id);

    // Linear model: yearly borrow rate ~= utilization.
    let model_id = env.register(jrm::JumpRateModel, ());
    let model_client = jrm::JumpRateModelClient::new(&env, &model_id);
    model_client.initialize(
        &0u128,
        &1_000_000u128,
        &1_000_000u128,
        &1_000_000u128,
        &model_admin,
    );
    env.mock_all_auths_allowing_non_root_auth();
    vault.set_interest_model(&model_id);

    vault.deposit(&user, &1_000u128);
    vault.borrow(&user, &500u128);

    // Seed non-zero reserves to exercise the reserve-subtraction path.
    env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::TotalReserves, &100u128);
    });

    let tb_prior = vault.get_total_borrowed();
    let pooled_reserves = vault
        .get_total_reserves()
        .saturating_add(vault.get_total_admin_fees());
    let live_cash = token_client.balance(&vault_id) as u128;
    let boosted_shares = boosted.balance(&vault_id);
    let boosted_underlying = boosted
        .get_asset_amounts_per_shares(&boosted_shares)
        .get(0)
        .unwrap_or(0) as u128;
    let gross_cash = live_cash.saturating_add(boosted_underlying);

    let expected_rate = model_client.get_borrow_rate(&gross_cash, &tb_prior, &pooled_reserves);
    let wrong_cash = vault.get_available_liquidity();
    let wrong_rate = model_client.get_borrow_rate(&wrong_cash, &tb_prior, &pooled_reserves);
    assert!(expected_rate < wrong_rate);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    vault.update_interest();

    let tb_after = vault.get_total_borrowed();
    let accrued = tb_after.saturating_sub(tb_prior);
    let expected_accrued = tb_prior.saturating_mul(expected_rate) / 1_000_000u128;
    let wrong_accrued = tb_prior.saturating_mul(wrong_rate) / 1_000_000u128;

    assert_eq!(accrued, expected_accrued);
    assert_ne!(accrued, wrong_accrued);
}

#[test]
fn test_update_interest_clamps_inflated_boosted_quote_for_model_cash() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let model_admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&user, &10_000i128);

    let model_id = env.register(jrm::JumpRateModel, ());
    let model = jrm::JumpRateModelClient::new(&env, &model_id);
    // Linear rate curve to make utilization effects obvious.
    model.initialize(
        &0u128,
        &1_000_000u128,
        &1_000_000u128,
        &1_000_000u128,
        &model_admin,
    );

    let boosted_normal_id = env.register(MockBoostedVault, ());
    let boosted_normal = MockBoostedVaultClient::new(&env, &boosted_normal_id);
    boosted_normal.initialize(&token_address);

    let boosted_inflated_id = env.register(MockBoostedVault, ());
    let boosted_inflated = MockBoostedVaultClient::new(&env, &boosted_inflated_id);
    boosted_inflated.initialize(&token_address);
    boosted_inflated.set_quote_multiplier_bps(&3_000_000u128);

    let vault_a_id = env.register(ReceiptVault, ());
    let vault_a = ReceiptVaultClient::new(&env, &vault_a_id);
    vault_a.initialize(&token_address, &0u128, &0u128, &admin);
    vault_a.enable_static_rates(&admin);
    vault_a.set_collateral_factor(&1_000_000u128);
    vault_a.set_interest_model(&model_id);
    vault_a.set_boosted_vault(&admin, &boosted_normal_id);

    let vault_b_id = env.register(ReceiptVault, ());
    let vault_b = ReceiptVaultClient::new(&env, &vault_b_id);
    vault_b.initialize(&token_address, &0u128, &0u128, &admin);
    vault_b.enable_static_rates(&admin);
    vault_b.set_collateral_factor(&1_000_000u128);
    vault_b.set_interest_model(&model_id);
    vault_b.set_boosted_vault(&admin, &boosted_inflated_id);

    // Identical state on both vaults.
    vault_a.deposit(&user, &1_000u128);
    vault_b.deposit(&user, &1_000u128);
    vault_a.borrow(&user, &500u128);
    vault_b.borrow(&user, &500u128);

    let tb_prior = vault_b.get_total_borrowed();
    let pooled_reserves = vault_b
        .get_total_reserves()
        .saturating_add(vault_b.get_total_admin_fees());
    let live_cash = token_client.balance(&vault_b_id) as u128;
    let reported = boosted_inflated
        .get_asset_amounts_per_shares(&boosted_inflated.balance(&vault_b_id))
        .get(0)
        .unwrap_or(0) as u128;
    let cached_before: u128 = env.as_contract(&vault_b_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::BoostedUnderlyingCached)
            .unwrap_or(0u128)
    });
    let accounting: u128 = env.as_contract(&vault_b_id, || {
        let storage = env.storage().persistent();
        let total_deposited: u128 = storage.get(&DataKey::TotalDeposited).unwrap_or(0u128);
        let total_reserves: u128 = storage.get(&DataKey::TotalReserves).unwrap_or(0u128);
        let total_admin_fees: u128 = storage.get(&DataKey::TotalAdminFees).unwrap_or(0u128);
        let total_borrowed: u128 = storage.get(&DataKey::TotalBorrowed).unwrap_or(0u128);
        let managed_cash: u128 = storage.get(&DataKey::ManagedCash).unwrap_or(0u128);
        total_deposited
            .saturating_add(total_reserves)
            .saturating_add(total_admin_fees)
            .saturating_sub(total_borrowed)
            .saturating_sub(managed_cash)
    });
    let baseline = cached_before.max(accounting);
    let cap = if baseline == 0 {
        reported
    } else {
        baseline.saturating_add((baseline.saturating_mul(500u128)) / 10_000u128)
    };
    let capped_cash = live_cash.saturating_add(reported.min(cap));
    let uncapped_cash = live_cash.saturating_add(reported);
    let expected_rate = model.get_borrow_rate(&capped_cash, &tb_prior, &pooled_reserves);
    let uncapped_rate = model.get_borrow_rate(&uncapped_cash, &tb_prior, &pooled_reserves);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    vault_b.update_interest();
    vault_a.update_interest();

    let accrued = vault_b.get_total_borrowed().saturating_sub(tb_prior);
    let expected_accrued = tb_prior.saturating_mul(expected_rate) / 1_000_000u128;
    let uncapped_accrued = tb_prior.saturating_mul(uncapped_rate) / 1_000_000u128;

    // The model input follows capped cash, not arbitrary inflated quotes.
    assert_eq!(accrued, expected_accrued);
    assert!(accrued >= uncapped_accrued);
}

#[test]
fn test_update_interest_ignores_boosted_quote_when_baseline_missing_with_debt() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let model_admin = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        jrm::DEFAULT_INIT_ADMIN,
    ));
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);
    token_admin_client.mint(&user, &10_000i128);

    let model_id = env.register(jrm::JumpRateModel, ());
    let model = jrm::JumpRateModelClient::new(&env, &model_id);
    model.initialize(
        &0u128,
        &1_000_000u128,
        &1_000_000u128,
        &1_000_000u128,
        &model_admin,
    );

    let boosted_id = env.register(MockBoostedVault, ());
    let boosted = MockBoostedVaultClient::new(&env, &boosted_id);
    boosted.initialize(&token_address);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);
    vault.set_interest_model(&model_id);
    vault.set_boosted_vault(&admin, &boosted_id);

    vault.deposit(&user, &1_000u128);
    vault.borrow(&user, &500u128);
    boosted.set_quote_multiplier_bps(&3_000_000u128);

    // Force the model-cash baseline to zero while debt remains outstanding.
    env.as_contract(&vault_id, || {
        let storage = env.storage().persistent();
        storage.set(&DataKey::BoostedUnderlyingCached, &0u128);
        storage.set(&DataKey::TotalDeposited, &0u128);
        storage.set(&DataKey::TotalReserves, &0u128);
        storage.set(&DataKey::TotalAdminFees, &0u128);
        storage.set(&DataKey::ManagedCash, &0u128);
    });

    let tb_prior = vault.get_total_borrowed();
    let pooled_reserves = vault
        .get_total_reserves()
        .saturating_add(vault.get_total_admin_fees());
    let live_cash = token_client.balance(&vault_id) as u128;
    let reported = boosted
        .get_asset_amounts_per_shares(&boosted.balance(&vault_id))
        .get(0)
        .unwrap_or(0) as u128;

    let expected_rate = model.get_borrow_rate(&live_cash, &tb_prior, &pooled_reserves);
    let uncapped_rate = model.get_borrow_rate(
        &live_cash.saturating_add(reported),
        &tb_prior,
        &pooled_reserves,
    );
    assert!(expected_rate > uncapped_rate);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);
    vault.update_interest();

    let accrued = vault.get_total_borrowed().saturating_sub(tb_prior);
    let expected_accrued = tb_prior.saturating_mul(expected_rate) / 1_000_000u128;
    assert_eq!(accrued, expected_accrued);
}

#[test]
fn test_update_interest_does_not_advance_time_when_rounds_to_zero() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &10_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &100_000u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&1_000_000u128);

    vault.deposit(&user, &100u128);
    vault.borrow(&user, &1u128);

    let last_before: u64 = env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::LastUpdateTime)
            .expect("last update missing")
    });
    env.ledger().set_timestamp(last_before + 1);
    vault.update_interest();
    let last_after: u64 = env.as_contract(&vault_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::LastUpdateTime)
            .expect("last update missing")
    });
    assert_eq!(last_after, last_before);
}

#[test]
#[should_panic]
fn test_ptoken_transfer_and_approve_with_gating() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let other = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    // Vault
    let v_id = env.register(ReceiptVault, ());
    let v = ReceiptVaultClient::new(&env, &v_id);
    v.initialize(&token_address, &0u128, &0u128, &admin);
    v.enable_static_rates(&admin);

    // Fund and deposit
    token_admin_client.mint(&user, &1_000i128);
    v.set_collateral_factor(&1_000_000u128);
    v.deposit(&user, &200u128); // user has 200 pTokens

    // Transfer 50 pTokens to other -> healthy
    v.transfer(&user, &other, &50i128);
    assert_eq!(v.get_ptoken_balance(&user), 150u128);
    assert_eq!(v.get_ptoken_balance(&other), 50u128);

    // Approve and transfer_from 50 pTokens from user to other
    let live_until_ledger = env.ledger().sequence() + 1000;
    v.approve(&user, &other, &50i128, &live_until_ledger);
    v.transfer_from(&other, &user, &other, &50i128);
    assert_eq!(v.get_ptoken_balance(&user), 100u128);
    assert_eq!(v.get_ptoken_balance(&other), 100u128);

    // Borrow to reduce headroom (local-only)
    v.borrow(&user, &100u128);

    // Now wire a minimal peridottroller (no oracle set -> preview_redeem_max=0)
    let comp_id = env.register(SimplePeridottroller, ());
    v.set_peridottroller(&comp_id);

    // Attempt transfer 101 -> should panic via peridottroller gating
    v.transfer(&user, &other, &101i128);
}

#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_transfer_accrues_interest_before_collateral_check() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let other = Address::generate(&env);
    let lender = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1_000i128);
    token_admin_client.mint(&lender, &2_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);
    vault.set_collateral_factor(&800_000u128);
    vault.set_borrow_rate(&1_000_000u128); // 100% APR

    vault.deposit(&lender, &1_000u128);
    vault.deposit(&user, &100u128);
    vault.borrow(&user, &79u128);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 365 * 24 * 60 * 60);

    // Must fail using freshly-accrued debt, not stale pre-accrual debt.
    vault.transfer(&user, &other, &1i128);
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
    vault.enable_static_rates(&admin);

    // Attempt upgrade without admin authorization
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    vault.upgrade_wasm(&hash);
}

// Security test: Verify local-only collateral check blocks withdrawals that would
// leave position undercollateralized when no Peridottroller is configured
#[test]
#[should_panic(expected = "Insufficient collateral")]
fn test_withdraw_local_only_blocks_undercollateralized_position() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, _token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);

    // Initialize WITHOUT setting a Peridottroller (local-only mode)
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    // Set collateral factor to 50% (default)
    vault.set_collateral_factor(&500_000u128);

    // User deposits 100 underlying -> gets 100 pTokens
    // Max borrow = 100 * 50% = 50
    vault.deposit(&user, &100u128);
    assert_eq!(vault.get_ptoken_balance(&user), 100u128);

    // User borrows 40 (under the 50 limit)
    vault.borrow(&user, &40u128);
    assert_eq!(vault.get_user_borrow_balance(&user), 40u128);

    // Now try to withdraw 30 pTokens
    // After withdrawal: remaining collateral = 70, max borrow = 70 * 50% = 35
    // But user has 40 debt > 35 max borrow -> should PANIC
    vault.withdraw(&user, &30u128);
}

// Security test: Verify withdrawal is allowed when position remains healthy
#[test]
fn test_withdraw_local_only_allows_healthy_position() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);

    // Initialize WITHOUT setting a Peridottroller (local-only mode)
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    // Set collateral factor to 50%
    vault.set_collateral_factor(&500_000u128);

    // User deposits 100 underlying -> gets 100 pTokens
    vault.deposit(&user, &100u128);

    // User borrows 40 (under the 50 limit)
    vault.borrow(&user, &40u128);

    // Withdraw 10 pTokens -> remaining collateral = 90, max borrow = 45
    // User has 40 debt <= 45 max borrow -> should SUCCEED
    vault.withdraw(&user, &10u128);

    assert_eq!(vault.get_ptoken_balance(&user), 90u128);
    // Start: 1000
    // After deposit 100: 900
    // After borrow 40: 940
    // After withdraw 10 underlying (10 pTokens at 1:1 rate): 950
    assert_eq!(token_client.balance(&user), 950i128);
}

// Security test: Verify withdrawal is unrestricted when user has no debt
#[test]
fn test_withdraw_local_only_unrestricted_with_no_debt() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&user, &1_000i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);

    // Initialize WITHOUT setting a Peridottroller (local-only mode)
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    // User deposits 100 underlying
    vault.deposit(&user, &100u128);

    // User has no debt, so can withdraw everything
    vault.withdraw(&user, &100u128);

    assert_eq!(vault.get_ptoken_balance(&user), 0u128);
    assert_eq!(token_client.balance(&user), 1000i128);
}

// (cross-market collateral tests moved to simple-peridottroller crate to avoid circular deps)

#[test]
fn test_direct_donation_does_not_inflate_exchange_rate() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let victim = Address::generate(&env);

    let (token_address, token_client, token_admin_client) = create_test_token(&env, &admin);

    token_admin_client.mint(&attacker, &2_000i128);
    token_admin_client.mint(&victim, &1_500i128);

    let vault_id = env.register(ReceiptVault, ());
    let vault = ReceiptVaultClient::new(&env, &vault_id);
    vault.initialize(&token_address, &0u128, &0u128, &admin);
    vault.enable_static_rates(&admin);

    vault.deposit(&attacker, &1u128);
    assert_eq!(vault.get_ptoken_balance(&attacker), 1u128);
    assert_eq!(vault.get_exchange_rate(), 1_000_000u128);

    // Donate directly to the vault address without minting pTokens.
    token_client.transfer(&attacker, &vault_id, &999i128);
    assert_eq!(token_client.balance(&vault_id), 1_000i128);
    assert_eq!(vault.get_total_ptokens(), 1u128);
    // Internal cash accounting ignores direct donations for exchange-rate purposes.
    assert_eq!(vault.get_exchange_rate(), 1_000_000u128);

    vault.deposit(&victim, &1_500u128);
    assert_eq!(vault.get_ptoken_balance(&victim), 1_500u128);
    assert_eq!(vault.get_total_ptokens(), 1_501u128);

    vault.withdraw(&attacker, &1u128);
    assert_eq!(token_client.balance(&attacker), 1_001i128);

    vault.withdraw(&victim, &1_500u128);
    assert_eq!(token_client.balance(&victim), 1_500i128);

    // Donated funds remain unaccounted for by the exchange rate and stay in the vault.
    assert_eq!(token_client.balance(&vault_id), 999i128);
}
