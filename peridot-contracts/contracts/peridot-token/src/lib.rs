#![no_std]
#[cfg(test)]
extern crate std;
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, String};
use stellar_tokens::fungible::burnable::emit_burn;
use stellar_tokens::fungible::Base as TokenBase;

pub const DEFAULT_INIT_ADMIN: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";

#[contracttype]
pub enum DataKey {
    Admin,
    MaxSupply,
    Initialized,
}

/// Peridot reward token contract.
///
/// # Example (doctest, no_run)
/// ```no_run
/// use soroban_sdk::{Env, Address, String};
/// use peridot_token::{PeridotToken, PeridotTokenClient, DEFAULT_INIT_ADMIN};
///
/// let env = Env::default();
/// env.mock_all_auths();
/// let admin = Address::from_string(&String::from_str(&env, DEFAULT_INIT_ADMIN));
/// let contract_id = env.register(PeridotToken, ());
/// let client = PeridotTokenClient::new(&env, &contract_id);
/// client.initialize(
///     &String::from_str(&env, "Peridot"),
///     &String::from_str(&env, "P"),
///     &6u32,
///     &admin,
///     &1_000_000i128,
/// );
/// ```
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
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        let expected_admin_str =
            option_env!("PERIDOT_TOKEN_INIT_ADMIN").unwrap_or(DEFAULT_INIT_ADMIN);
        let expected_admin = Address::from_string(&String::from_str(&env, expected_admin_str));
        if admin != expected_admin {
            panic!("unexpected admin");
        }
        if env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::MaxSupply)
            .is_some()
        {
            panic!("already initialized");
        }
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
        env.storage().instance().set(&DataKey::Initialized, &true);
        bump_critical_ttl(&env);
    }

    pub fn name(env: Env) -> String {
        bump_critical_ttl(&env);
        TokenBase::name(&env)
    }

    pub fn symbol(env: Env) -> String {
        bump_critical_ttl(&env);
        TokenBase::symbol(&env)
    }

    pub fn decimals(env: Env) -> u32 {
        bump_critical_ttl(&env);
        TokenBase::decimals(&env)
    }

    pub fn total_supply(env: Env) -> i128 {
        bump_critical_ttl(&env);
        TokenBase::total_supply(&env)
    }

    pub fn balance(env: Env, who: Address) -> i128 {
        bump_critical_ttl(&env);
        TokenBase::balance(&env, &who)
    }

    pub fn balance_of(env: Env, who: Address) -> i128 {
        Self::balance(env, who)
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        bump_critical_ttl(&env);
        TokenBase::allowance(&env, &owner, &spender)
    }

    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
        bump_critical_ttl(&env);
        owner.require_auth();
        if amount < 0 {
            panic!("bad amount");
        }
        TokenBase::approve(&env, &owner, &spender, amount, u32::MAX);
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        bump_critical_ttl(&env);
        from.require_auth();
        if amount <= 0 {
            panic!("bad amount");
        }
        TokenBase::transfer(&env, &from, &to, amount);
    }

    pub fn transfer_from(env: Env, spender: Address, owner: Address, to: Address, amount: i128) {
        bump_critical_ttl(&env);
        if amount <= 0 {
            panic!("bad amount");
        }
        TokenBase::transfer_from(&env, &spender, &owner, &to, amount);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        bump_critical_ttl(&env);
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
        bump_critical_ttl(&env);
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
        bump_critical_ttl(&env);
        require_admin(&env);
        env.storage().persistent().set(&DataKey::Admin, &new_admin);
    }

    pub fn upgrade_wasm(env: Env, new_wasm_hash: BytesN<32>) {
        bump_critical_ttl(&env);
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

const TTL_THRESHOLD: u32 = 100_000_000;
const TTL_EXTEND_TO: u32 = 200_000_000;

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("no admin");
    bump_critical_ttl(env);
    admin.require_auth();
    admin
}

fn bump_critical_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::Admin) {
        persistent.extend_ttl(&DataKey::Admin, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::MaxSupply) {
        persistent.extend_ttl(&DataKey::MaxSupply, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if env.storage().instance().has(&DataKey::Initialized) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}
