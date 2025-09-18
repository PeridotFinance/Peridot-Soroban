#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, Env, String, Symbol};

#[contracttype]
pub enum DataKey {
    Name,
    SymbolK,
    Decimals,
    Admin,
    Bal(Address),
    Allow(Address, Address),
    Total,
}

#[contract]
pub struct PeridotToken;

#[contractimpl]
impl PeridotToken {
    pub fn initialize(env: Env, name: String, symbol: String, decimals: u32, admin: Address) {
        env.storage().persistent().set(&DataKey::Name, &name);
        env.storage().persistent().set(&DataKey::SymbolK, &symbol);
        env.storage().persistent().set(&DataKey::Decimals, &decimals);
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::Total, &0i128);
    }

    pub fn name(env: Env) -> String { env.storage().persistent().get(&DataKey::Name).unwrap() }
    pub fn symbol(env: Env) -> String { env.storage().persistent().get(&DataKey::SymbolK).unwrap() }
    pub fn decimals(env: Env) -> u32 { env.storage().persistent().get(&DataKey::Decimals).unwrap_or(6u32) }

    pub fn total_supply(env: Env) -> i128 { env.storage().persistent().get(&DataKey::Total).unwrap_or(0i128) }
    pub fn balance_of(env: Env, who: Address) -> i128 { env.storage().persistent().get(&DataKey::Bal(who)).unwrap_or(0i128) }
    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 { env.storage().persistent().get(&DataKey::Allow(owner, spender)).unwrap_or(0i128) }

    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
        owner.require_auth();
        env.storage().persistent().set(&DataKey::Allow(owner.clone(), spender.clone()), &amount);
        env.events().publish((Symbol::new(&env, "approve"),), (owner, spender, amount));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        Self::transfer_internal(env, from, to, amount, false);
    }

    pub fn transfer_from(env: Env, spender: Address, owner: Address, to: Address, amount: i128) {
        spender.require_auth();
        let allowed: i128 = env.storage().persistent().get(&DataKey::Allow(owner.clone(), spender.clone())).unwrap_or(0i128);
        if allowed < amount { panic!("insufficient allowance"); }
        env.storage().persistent().set(&DataKey::Allow(owner.clone(), spender.clone()), &(allowed - amount));
        Self::transfer_internal(env, owner, to, amount, true);
    }

    fn transfer_internal(env: Env, from: Address, to: Address, amount: i128, via_spender: bool) {
        if amount <= 0 { panic!("bad amount"); }
        let fb: i128 = env.storage().persistent().get(&DataKey::Bal(from.clone())).unwrap_or(0i128);
        if fb < amount { panic!("insufficient balance"); }
        let tb: i128 = env.storage().persistent().get(&DataKey::Bal(to.clone())).unwrap_or(0i128);
        env.storage().persistent().set(&DataKey::Bal(from.clone()), &(fb - amount));
        env.storage().persistent().set(&DataKey::Bal(to.clone()), &(tb + amount));
        let evt = if via_spender { Symbol::new(&env, "transfer_from") } else { Symbol::new(&env, "transfer") };
        env.events().publish((evt,), (from, to, amount));
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("no admin");
        admin.require_auth();
        if amount <= 0 { panic!("bad amount"); }
        let tb: i128 = env.storage().persistent().get(&DataKey::Bal(to.clone())).unwrap_or(0i128);
        env.storage().persistent().set(&DataKey::Bal(to.clone()), &(tb + amount));
        let total: i128 = env.storage().persistent().get(&DataKey::Total).unwrap_or(0i128);
        env.storage().persistent().set(&DataKey::Total, &(total + amount));
        env.events().publish((Symbol::new(&env, "mint"),), (to, amount));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("no admin");
        admin.require_auth();
        if amount <= 0 { panic!("bad amount"); }
        let fb: i128 = env.storage().persistent().get(&DataKey::Bal(from.clone())).unwrap_or(0i128);
        if fb < amount { panic!("insufficient balance"); }
        env.storage().persistent().set(&DataKey::Bal(from.clone()), &(fb - amount));
        let total: i128 = env.storage().persistent().get(&DataKey::Total).unwrap_or(0i128);
        env.storage().persistent().set(&DataKey::Total, &(total - amount));
        env.events().publish((Symbol::new(&env, "burn"),), (from, amount));
    }

    pub fn set_admin(env: Env, new_admin: Address) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("no admin");
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &new_admin);
        env.events().publish((Symbol::new(&env, "admin_set"),), new_admin);
    }
}
