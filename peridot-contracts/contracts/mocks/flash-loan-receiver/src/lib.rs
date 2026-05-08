#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Bytes, Env, IntoVal, Symbol, Val,
};

#[contracttype]
enum DataKey {
    Token,
}

#[contract]
pub struct FlashLoanReceiver;

#[contractimpl]
impl FlashLoanReceiver {
    pub fn initialize(env: Env, token: Address) {
        env.storage().persistent().set(&DataKey::Token, &token);
    }

    pub fn flash(env: Env, vault: Address, amount: u128) {
        let receiver = env.current_contract_address();
        let data = Bytes::new(&env);
        let _: Val = env.invoke_contract(
            &vault,
            &Symbol::new(&env, "flash_loan"),
            (receiver, amount, data).into_val(&env),
        );
    }

    pub fn on_flash_loan(env: Env, vault: Address, amount: u128, fee: u128, _data: Bytes) {
        let token: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Token)
            .expect("token not set");
        let total = amount.saturating_add(fee);
        let total_i128: i128 = total.try_into().expect("repay too large");
        token::Client::new(&env, &token).transfer(
            &env.current_contract_address(),
            &vault,
            &total_i128,
        );
    }
}
