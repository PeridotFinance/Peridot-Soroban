#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, Symbol
};

// Storage key types for the contract
#[contracttype]
pub enum DataKey {
    UnderlyingToken,
    UserDeposits(Address),
    TotalDeposited,
}

// Events
#[contracttype]
pub enum EventType {
    Deposit,
    Withdraw,
}

#[contract]
pub struct SimpleVault;

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
impl SimpleVault {
    /// Initialize the vault with the underlying token address
    pub fn initialize(env: Env, token_address: Address) {
        // Store the underlying token address
        env.storage()
            .persistent()
            .set(&DataKey::UnderlyingToken, &token_address);
        
        // Initialize total deposited to 0
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &0u128);
    }

    /// Deposit tokens into the vault
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

        // Update user's deposit balance
        let current_balance = env.storage()
            .persistent()
            .get(&DataKey::UserDeposits(user.clone()))
            .unwrap_or(0u128);
        
        let new_balance = current_balance + amount;
        env.storage()
            .persistent()
            .set(&DataKey::UserDeposits(user.clone()), &new_balance);

        // Update total deposited
        let total_deposited: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &(total_deposited + amount));

        // Emit deposit event
        env.events().publish(
            (Symbol::new(&env, "deposit"), user.clone()),
            (amount, new_balance)
        );
    }

    /// Withdraw tokens from the vault
    pub fn withdraw(env: Env, user: Address, amount: u128) {
        // Require authorization from the user
        user.require_auth();

        // Check user has sufficient balance
        let current_balance = env.storage()
            .persistent()
            .get(&DataKey::UserDeposits(user.clone()))
            .unwrap_or(0u128);
        
        if current_balance < amount {
            panic!("Insufficient balance");
        }

        // Get the underlying token
        let token_address: Address = env.storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");

        // Create token client
        let token_client = token::Client::new(&env, &token_address);

        // Update user's deposit balance
        let new_balance = current_balance - amount;
        env.storage()
            .persistent()
            .set(&DataKey::UserDeposits(user.clone()), &new_balance);

        // Update total deposited
        let total_deposited: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &(total_deposited - amount));

        // Transfer tokens back to user
        token_client.transfer(&env.current_contract_address(), &user, &(amount as i128));

        // Emit withdraw event
        env.events().publish(
            (Symbol::new(&env, "withdraw"), user.clone()),
            (amount, new_balance)
        );
    }

    /// Get user's balance in the vault
    pub fn get_user_balance(env: Env, user: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::UserDeposits(user))
            .unwrap_or(0u128)
    }

    /// Get total amount deposited in the vault
    pub fn get_total_deposited(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128)
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
