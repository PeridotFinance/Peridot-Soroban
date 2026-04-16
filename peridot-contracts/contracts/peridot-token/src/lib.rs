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
    PendingAdmin,
    MaxSupply,
    Initialized,
    PendingUpgradeHash,
    PendingUpgradeEta,
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
        let expected_admin_str = expected_admin_config();
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

    pub fn approve(
        env: Env,
        owner: Address,
        spender: Address,
        amount: i128,
        live_until_ledger: u32,
    ) {
        bump_critical_ttl(&env);
        owner.require_auth();
        if amount < 0 {
            panic!("bad amount");
        }
        TokenBase::approve(&env, &owner, &spender, amount, live_until_ledger);
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
        spender.require_auth();
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
        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &new_admin);
    }

    pub fn accept_admin(env: Env) {
        bump_critical_ttl(&env);
        let new_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .expect("pending admin not set");
        new_admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &new_admin);
        env.storage().persistent().remove(&DataKey::PendingAdmin);
    }

    pub fn propose_upgrade_wasm(env: Env, new_wasm_hash: BytesN<32>) {
        bump_critical_ttl(&env);
        require_admin(&env);
        let execute_after = env
            .ledger()
            .timestamp()
            .saturating_add(UPGRADE_TIMELOCK_SECS);
        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgradeHash, &new_wasm_hash);
        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgradeEta, &execute_after);
        bump_pending_upgrade_ttl(&env);
    }

    pub fn upgrade_wasm(env: Env, new_wasm_hash: BytesN<32>) {
        bump_critical_ttl(&env);
        require_admin(&env);
        bump_pending_upgrade_ttl(&env);
        let pending_hash: BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgradeHash)
            .expect("pending upgrade not set");
        let execute_after: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgradeEta)
            .expect("pending upgrade eta not set");
        if pending_hash != new_wasm_hash {
            panic!("upgrade hash mismatch");
        }
        if env.ledger().timestamp() < execute_after {
            panic!("upgrade timelocked");
        }
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeHash);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeEta);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND_TO: u32 = 1_000_000;
const UPGRADE_TIMELOCK_SECS: u64 = 24 * 60 * 60;

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
    if persistent.has(&DataKey::PendingAdmin) {
        persistent.extend_ttl(&DataKey::PendingAdmin, TTL_THRESHOLD, TTL_EXTEND_TO);
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

fn bump_pending_upgrade_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::PendingUpgradeHash) {
        persistent.extend_ttl(&DataKey::PendingUpgradeHash, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PendingUpgradeEta) {
        persistent.extend_ttl(&DataKey::PendingUpgradeEta, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn expected_admin_config() -> &'static str {
    if cfg!(any(test, feature = "test-default-admin")) {
        option_env!("PERIDOT_TOKEN_INIT_ADMIN").unwrap_or(DEFAULT_INIT_ADMIN)
    } else {
        option_env!("PERIDOT_TOKEN_INIT_ADMIN")
            .expect("PERIDOT_TOKEN_INIT_ADMIN must be set at build time")
    }
}

#[cfg(test)]
mod test;
