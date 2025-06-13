#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, Symbol
};

// Storage key types for the contract
#[contracttype]
pub enum DataKey {
    UnderlyingToken,
    UserDeposits(Address),    // Keep for now (will remove in later steps)
    PTokenBalances(Address),  // New: receipt token balances
    TotalDeposited,
    TotalPTokens,            // New: total receipt tokens issued
}

#[contract]
pub struct ReceiptVault;

// This is a sample contract. Replace this placeholder with your own contract logic.
// A corresponding test example is available in `test.rs`.
//
// For comprehensive examples, visit <https://github.com/stellar/soroban-examples>.
// The repository includes use cases for the Stellar ecosystem, such as data storage on
// the blockchain, token swaps, liquidity pools, and more.
//
// Refer to the official documentation:
// <https://developers.stellar.org/docs/build/smart-contracts/overview>.
#[contractimpl]
impl ReceiptVault {
    /// Initialize the vault with the underlying token address
    pub fn initialize(env: Env, token_address: Address) {
        // Store the underlying token address
        env.storage()
            .persistent()
            .set(&DataKey::UnderlyingToken, &token_address);
        
        // Initialize totals to 0
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &0u128);
        env.storage()
            .persistent()
            .set(&DataKey::TotalPTokens, &0u128);
    }

    /// Deposit tokens into the vault and receive pTokens
    pub fn deposit(env: Env, user: Address, amount: u128) {
        // Require authorization from the user
        user.require_auth();

        // Get the underlying token
        let token_address: Address = env.storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");

        // Create token client
        let token_client = token::Client::new(&env, &token_address);

        // Transfer tokens from user to contract
        token_client.transfer(&user, &env.current_contract_address(), &(amount as i128));

        // Calculate pTokens to mint (1:1 ratio for now)
        let ptokens_to_mint = amount;

        // Update user's deposit balance (keep for compatibility)
        let current_deposits = env.storage()
            .persistent()
            .get(&DataKey::UserDeposits(user.clone()))
            .unwrap_or(0u128);
        env.storage()
            .persistent()
            .set(&DataKey::UserDeposits(user.clone()), &(current_deposits + amount));

        // Update user's pToken balance
        let current_ptokens = env.storage()
            .persistent()
            .get(&DataKey::PTokenBalances(user.clone()))
            .unwrap_or(0u128);
        env.storage()
            .persistent()
            .set(&DataKey::PTokenBalances(user.clone()), &(current_ptokens + ptokens_to_mint));

        // Update totals
        let total_deposited: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        let total_ptokens: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalPTokens)
            .unwrap_or(0u128);
        
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &(total_deposited + amount));
        env.storage()
            .persistent()
            .set(&DataKey::TotalPTokens, &(total_ptokens + ptokens_to_mint));

        // Emit deposit event
        env.events().publish(
            (Symbol::new(&env, "deposit"), user.clone()),
            (amount, ptokens_to_mint)
        );
    }

    /// Withdraw tokens using pTokens
    pub fn withdraw(env: Env, user: Address, ptoken_amount: u128) {
        // Require authorization from the user
        user.require_auth();

        // Check user has sufficient pTokens
        let current_ptokens = env.storage()
            .persistent()
            .get(&DataKey::PTokenBalances(user.clone()))
            .unwrap_or(0u128);
        
        if current_ptokens < ptoken_amount {
            panic!("Insufficient pTokens");
        }

        // Calculate underlying tokens to return (1:1 ratio for now)
        let underlying_to_return = ptoken_amount;

        // Check we have enough total deposited
        let total_deposited: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        
        if total_deposited < underlying_to_return {
            panic!("Not enough liquidity");
        }

        // Get the underlying token
        let token_address: Address = env.storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");

        // Create token client
        let token_client = token::Client::new(&env, &token_address);

        // Update user's pToken balance
        env.storage()
            .persistent()
            .set(&DataKey::PTokenBalances(user.clone()), &(current_ptokens - ptoken_amount));

        // Update user's deposit balance (for compatibility)
        let current_deposits = env.storage()
            .persistent()
            .get(&DataKey::UserDeposits(user.clone()))
            .unwrap_or(0u128);
        env.storage()
            .persistent()
            .set(&DataKey::UserDeposits(user.clone()), &(current_deposits - underlying_to_return));

        // Update totals
        let total_ptokens: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalPTokens)
            .unwrap_or(0u128);
        
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &(total_deposited - underlying_to_return));
        env.storage()
            .persistent()
            .set(&DataKey::TotalPTokens, &(total_ptokens - ptoken_amount));

        // Transfer tokens back to user
        token_client.transfer(&env.current_contract_address(), &user, &(underlying_to_return as i128));

        // Emit withdraw event
        env.events().publish(
            (Symbol::new(&env, "withdraw"), user.clone()),
            (underlying_to_return, ptoken_amount)
        );
    }

    /// Get user's balance in the vault (original deposits)
    pub fn get_user_balance(env: Env, user: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::UserDeposits(user))
            .unwrap_or(0u128)
    }

    /// Get user's pToken balance
    pub fn get_ptoken_balance(env: Env, user: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::PTokenBalances(user))
            .unwrap_or(0u128)
    }

    /// Get total amount deposited in the vault
    pub fn get_total_deposited(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128)
    }

    /// Get total pTokens issued
    pub fn get_total_ptokens(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalPTokens)
            .unwrap_or(0u128)
    }

    /// Get the exchange rate (pToken to underlying ratio)
    /// For now, always 1:1 (1 pToken = 1 underlying)
    pub fn get_exchange_rate(_env: Env) -> u128 {
        // Using 6 decimals for precision: 1_000_000 = 1.0
        1_000_000
    }

    /// Get the underlying token address
    pub fn get_underlying_token(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized")
    }
}

mod test;
