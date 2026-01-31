#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, String, Vec};

pub const DEFAULT_INIT_ADMIN: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";

#[soroban_sdk::contractclient(name = "SoroswapRouterClient")]
pub trait SoroswapRouter {
    fn swap_exact_tokens_for_tokens(
        env: Env,
        amount_in: i128,
        amount_out_min: i128,
        path: Vec<Address>,
        to: Address,
        deadline: u64,
    ) -> Vec<i128>;
}

#[contracttype]
pub enum DataKey {
    Admin,
    Router,
    Initialized,
}

#[contract]
pub struct SwapAdapter;

#[contractimpl]
impl SwapAdapter {
    pub fn initialize(env: Env, admin: Address, router: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        if env.storage().persistent().get::<_, Address>(&DataKey::Admin).is_some() {
            panic!("already initialized");
        }
        let expected_admin_str =
            option_env!("SWAP_ADAPTER_INIT_ADMIN").unwrap_or(DEFAULT_INIT_ADMIN);
        let expected_admin = Address::from_string(&String::from_str(&env, expected_admin_str));
        if admin != expected_admin {
            panic!("unexpected admin");
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::Router, &router);
        env.storage().instance().set(&DataKey::Initialized, &true);
        bump_critical_ttl(&env);
    }

    pub fn set_router(env: Env, admin: Address, router: Address) {
        bump_critical_ttl(&env);
        require_admin(&env, &admin);
        env.storage().persistent().set(&DataKey::Router, &router);
    }

    pub fn swap_exact_tokens_for_tokens(
        env: Env,
        user: Address,
        amount_in: u128,
        amount_out_min: u128,
        path: Vec<Address>,
        deadline: u64,
    ) -> u128 {
        bump_critical_ttl(&env);
        user.require_auth();
        let router: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Router)
            .expect("router not set");
        let amounts = SoroswapRouterClient::new(&env, &router).swap_exact_tokens_for_tokens(
            &amount_in.try_into().unwrap(),
            &amount_out_min.try_into().unwrap(),
            &path,
            &user,
            &deadline,
        );
        let last = amounts.get(amounts.len() - 1).unwrap();
        if last < 0 {
            panic!("invalid swap output");
        }
        last.try_into().unwrap()
    }

    pub fn bump_ttl(env: Env) {
        bump_critical_ttl(&env);
    }

    pub fn upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_critical_ttl(&env);
        require_admin(&env, &admin);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

fn require_admin(env: &Env, admin: &Address) {
    let stored: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");
    if stored != *admin {
        panic!("not admin");
    }
    bump_critical_ttl(env);
    admin.require_auth();
}

const TTL_THRESHOLD: u32 = 100_000_000;
const TTL_EXTEND_TO: u32 = 200_000_000;

#[cfg(test)]
mod test;

fn bump_critical_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::Admin) {
        persistent.extend_ttl(&DataKey::Admin, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Router) {
        persistent.extend_ttl(&DataKey::Router, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if env.storage().instance().has(&DataKey::Initialized) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}
