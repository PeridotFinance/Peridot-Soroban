#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String};
use stellar_tokens::fungible::burnable::emit_burn;
use stellar_tokens::fungible::Base as TokenBase;

#[contracttype]
pub enum DataKey {
    Admin,
    MaxSupply,
}

#[contract]
pub struct PeridotToken;

#[contractimpl]
impl PeridotToken {
    pub fn initialize(
        env: Env,
        name: String,
        symbol: String,
        decimals: u32,
        admin: Address,
        max_supply: i128,
    ) {
        if env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Admin)
            .is_some()
        {
            panic!("already initialized");
        }
        admin.require_auth();
        if max_supply <= 0 {
            panic!("invalid max supply");
        }
        TokenBase::set_metadata(&env, decimals, name, symbol);
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::MaxSupply, &max_supply);
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

    pub fn balance_of(env: Env, who: Address) -> i128 {
        TokenBase::balance(&env, &who)
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
        from.require_auth();
        if amount <= 0 {
            panic!("bad amount");
        }
        TokenBase::transfer(&env, &from, &to, amount);
    }

    pub fn transfer_from(env: Env, spender: Address, owner: Address, to: Address, amount: i128) {
        spender.require_auth();
        if amount <= 0 {
            panic!("bad amount");
        }
        TokenBase::transfer_from(&env, &spender, &owner, &to, amount);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        require_admin(&env);
        if amount <= 0 {
            panic!("bad amount");
        }
        let max_supply: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MaxSupply)
            .expect("max supply not set");
        let supply = TokenBase::total_supply(&env);
        if amount > max_supply.saturating_sub(supply) {
            panic!("max supply exceeded");
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
        emit_burn(&env, &from, amount);
    }

    pub fn set_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().persistent().set(&DataKey::Admin, &new_admin);
    }
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("no admin");
    admin.require_auth();
    admin
}
