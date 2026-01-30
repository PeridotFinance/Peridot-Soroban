#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String};
use stellar_tokens::fungible::Base as TokenBase;

#[contracttype]
enum DataKey {
    Initialized,
}

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn initialize(env: Env, name: String, symbol: String, decimals: u32) {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .is_some()
        {
            panic!("already initialized");
        }
        TokenBase::set_metadata(&env, decimals, name, symbol);
        env.storage().persistent().set(&DataKey::Initialized, &true);
    }

    pub fn name(env: Env) -> String {
        TokenBase::name(&env)
    }

    pub fn symbol(env: Env) -> String {
        TokenBase::symbol(&env)
    }

    pub fn decimals(env: Env) -> u32 {
        TokenBase::decimals(&env)
    }

    pub fn total_supply(env: Env) -> i128 {
        TokenBase::total_supply(&env)
    }

    pub fn balance(env: Env, who: Address) -> i128 {
        TokenBase::balance(&env, &who)
    }

    pub fn balance_of(env: Env, who: Address) -> i128 {
        Self::balance(env, who)
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        TokenBase::allowance(&env, &owner, &spender)
    }

    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
        owner.require_auth();
        if amount < 0 {
            panic!("bad amount");
        }
        TokenBase::approve(&env, &owner, &spender, amount, u32::MAX);
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        if amount <= 0 {
            panic!("bad amount");
        }
        TokenBase::transfer(&env, &from, &to, amount);
    }

    pub fn transfer_from(env: Env, spender: Address, owner: Address, to: Address, amount: i128) {
        if amount <= 0 {
            panic!("bad amount");
        }
        TokenBase::transfer_from(&env, &spender, &owner, &to, amount);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        if amount <= 0 {
            panic!("bad amount");
        }
        TokenBase::mint(&env, &to, amount);
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic!("bad amount");
        }
        let current = TokenBase::balance(&env, &from);
        if current < amount {
            panic!("insufficient balance");
        }
        TokenBase::update(&env, Some(&from), None, amount);
    }
}
