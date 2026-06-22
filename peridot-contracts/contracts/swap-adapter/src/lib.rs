#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, String, Vec};

pub const DEFAULT_INIT_ADMIN: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";
pub const MAX_DEADLINE_SECONDS: u64 = 86_400; // 24h
pub const MAX_ALLOWED_POOLS: u32 = 64;
pub const MAX_ALLOWED_POOL_BINDINGS: u32 = 128;
pub const MAX_ROUTE_TTL_BUMP_PER_CALL: u32 = 16;

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

#[soroban_sdk::contractclient(name = "AquariusRouterClient")]
pub trait AquariusRouter {
    fn swap(
        env: Env,
        user: Address,
        tokens: Vec<Address>,
        token_in: Address,
        token_out: Address,
        pool_index: BytesN<32>,
        in_amount: u128,
        out_min: u128,
    ) -> u128;

    fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        token_in: Address,
        amount: u128,
        amount_with_slippage: u128,
    ) -> u128;
}

#[soroban_sdk::contractclient(name = "AquariusPoolClient")]
pub trait AquariusPool {
    fn estimate_swap(env: Env, in_idx: u32, out_idx: u32, amount_in: u128) -> u128;
    fn swap(
        env: Env,
        user: Address,
        in_idx: u32,
        out_idx: u32,
        amount_in: u128,
        amount_out_min: u128,
    ) -> u128;
}

#[contracttype]
pub enum DataKey {
    Admin,
    Router,
    AllowedPool(Address),
    Initialized,
    PendingUpgradeHash,
    PendingUpgradeEta,
    AllowedPoolBinding(BytesN<32>, Address),
    AllowedPoolsList,
    AllowedPoolBindingsList,
}

#[contract]
pub struct SwapAdapter;

#[contractimpl]
impl SwapAdapter {
    pub fn initialize(env: Env, admin: Address, router: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
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
        let expected_admin_str = expected_admin_config();
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

    pub fn set_pool_allowed(env: Env, admin: Address, pool: Address, allowed: bool) {
        bump_critical_ttl(&env);
        require_admin(&env, &admin);
        let key = DataKey::AllowedPool(pool.clone());
        if allowed {
            env.storage().persistent().set(&key, &true);
            add_allowed_pool_to_list(&env, &pool);
            bump_pool_ttl(&env, &pool);
        } else {
            env.storage().persistent().remove(&key);
            remove_allowed_pool_from_list(&env, &pool);
        }
    }

    pub fn is_pool_allowed(env: Env, pool: Address) -> bool {
        bump_critical_ttl(&env);
        pool_allowed(&env, &pool)
    }

    pub fn set_pool_binding(
        env: Env,
        admin: Address,
        pool_id: BytesN<32>,
        pool: Address,
        allowed: bool,
    ) {
        bump_critical_ttl(&env);
        require_admin(&env, &admin);
        if pool_id.to_array() == [0u8; 32] {
            panic!("bad pool id");
        }
        let key = DataKey::AllowedPoolBinding(pool_id.clone(), pool.clone());
        if allowed {
            env.storage().persistent().set(&key, &true);
            add_allowed_binding_to_list(&env, &pool_id, &pool);
            bump_pool_binding_ttl(&env, &pool_id, &pool);
        } else {
            env.storage().persistent().remove(&key);
            remove_allowed_binding_from_list(&env, &pool_id, &pool);
        }
    }

    pub fn is_pool_binding_allowed(env: Env, pool_id: BytesN<32>, pool: Address) -> bool {
        bump_critical_ttl(&env);
        if !pool_allowed(&env, &pool) {
            return false;
        }
        bump_pool_binding_ttl(&env, &pool_id, &pool);
        env.storage()
            .persistent()
            .get(&DataKey::AllowedPoolBinding(pool_id, pool))
            .unwrap_or(false)
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
        if amount_out_min == 0 {
            panic!("zero slippage");
        }
        if amount_in > i128::MAX as u128 || amount_out_min > i128::MAX as u128 {
            panic!("amount too large");
        }
        let amount_in_i128: i128 = amount_in
            .try_into()
            .unwrap_or_else(|_| panic!("amount too large"));
        let amount_out_min_i128: i128 = amount_out_min
            .try_into()
            .unwrap_or_else(|_| panic!("amount too large"));
        let now = env.ledger().timestamp();
        if deadline < now {
            panic!("deadline expired");
        }
        if deadline > now.saturating_add(MAX_DEADLINE_SECONDS) {
            panic!("deadline too far");
        }
        let router: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Router)
            .expect("router not set");
        let amounts = SoroswapRouterClient::new(&env, &router).swap_exact_tokens_for_tokens(
            &amount_in_i128,
            &amount_out_min_i128,
            &path,
            &user,
            &deadline,
        );
        if amounts.len() == 0 {
            panic!("router returned empty amounts");
        }
        let last = amounts.get(amounts.len() - 1).unwrap();
        if last < 0 {
            panic!("invalid swap output");
        }
        last.try_into()
            .unwrap_or_else(|_| panic!("invalid swap output"))
    }

    pub fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        token_in: Address,
        amount: u128,
        amount_with_slippage: u128,
    ) -> u128 {
        bump_critical_ttl(&env);
        user.require_auth();
        if swaps_chain.len() == 0 {
            panic!("bad swaps");
        }
        if amount == 0 || amount_with_slippage == 0 {
            panic!("bad amount");
        }
        for i in 0..swaps_chain.len() {
            let (pool_tokens, pool_id, pool) = swaps_chain.get(i).unwrap();
            if pool_tokens.len() != 2 {
                panic!("bad swaps");
            }
            ensure_pool_binding_allowed(&env, &pool_id, &pool);
            ensure_pool_allowed(&env, &pool);
        }
        let _router: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Router)
            .expect("router not set");
        let mut current_token = token_in;
        let mut current_amount = amount;
        for i in 0..swaps_chain.len() {
            let (pool_tokens, _pool_id, pool) = swaps_chain.get(i).unwrap();
            // Custom low-budget route format: two pool tokens plus the direct pool address.
            // The output token is inferred as the other token in the pool.
            let (hop_out, in_idx, out_idx) = infer_two_token_hop(&pool_tokens, &current_token);
            let min_out = if i == swaps_chain.len() - 1 {
                amount_with_slippage
            } else {
                1u128
            };
            current_amount = AquariusPoolClient::new(&env, &pool).swap(
                &user,
                &in_idx,
                &out_idx,
                &current_amount,
                &min_out,
            );
            if current_amount == 0 {
                panic!("swap failed");
            }
            current_token = hop_out;
        }
        current_amount
    }

    pub fn estimate_pool_swap(
        env: Env,
        pool: Address,
        in_idx: u32,
        out_idx: u32,
        amount_in: u128,
    ) -> u128 {
        bump_critical_ttl(&env);
        ensure_pool_allowed(&env, &pool);
        AquariusPoolClient::new(&env, &pool).estimate_swap(&in_idx, &out_idx, &amount_in)
    }

    pub fn swap_pool(
        env: Env,
        user: Address,
        pool: Address,
        in_idx: u32,
        out_idx: u32,
        amount_in: u128,
        amount_out_min: u128,
    ) -> u128 {
        bump_critical_ttl(&env);
        user.require_auth();
        ensure_pool_allowed(&env, &pool);
        AquariusPoolClient::new(&env, &pool).swap(
            &user,
            &in_idx,
            &out_idx,
            &amount_in,
            &amount_out_min,
        )
    }

    pub fn bump_ttl(env: Env) {
        bump_critical_ttl(&env);
        bump_route_list_ttl(&env);
    }

    pub fn bump_route_ttl_batch(
        env: Env,
        admin: Address,
        pools_start: u32,
        bindings_start: u32,
        limit: u32,
    ) -> (u32, u32) {
        bump_critical_ttl(&env);
        require_admin(&env, &admin);
        let bounded_limit = if limit == 0 || limit > MAX_ROUTE_TTL_BUMP_PER_CALL {
            MAX_ROUTE_TTL_BUMP_PER_CALL
        } else {
            limit
        };
        let next_pools = bump_pool_route_ttl_range(&env, pools_start, bounded_limit);
        let next_bindings = bump_binding_route_ttl_range(&env, bindings_start, bounded_limit);
        (next_pools, next_bindings)
    }

    pub fn propose_upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_critical_ttl(&env);
        require_admin(&env, &admin);
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

    pub fn upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_critical_ttl(&env);
        require_admin(&env, &admin);
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

fn infer_two_token_hop(pool_tokens: &Vec<Address>, current_token: &Address) -> (Address, u32, u32) {
    if pool_tokens.len() != 2 {
        panic!("bad swaps");
    }
    let token_0 = pool_tokens.get(0).unwrap();
    let token_1 = pool_tokens.get(1).unwrap();
    if token_0 == current_token.clone() {
        (token_1, 0u32, 1u32)
    } else if token_1 == current_token.clone() {
        (token_0, 1u32, 0u32)
    } else {
        panic!("bad swaps");
    }
}

fn expected_admin_config() -> &'static str {
    if cfg!(any(test, feature = "test-default-admin")) {
        option_env!("SWAP_ADAPTER_INIT_ADMIN").unwrap_or(DEFAULT_INIT_ADMIN)
    } else {
        option_env!("SWAP_ADAPTER_INIT_ADMIN")
            .expect("SWAP_ADAPTER_INIT_ADMIN must be set at build time")
    }
}

const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND_TO: u32 = 1_000_000;
const UPGRADE_TIMELOCK_SECS: u64 = 24 * 60 * 60;

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

fn bump_pending_upgrade_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::PendingUpgradeHash) {
        persistent.extend_ttl(&DataKey::PendingUpgradeHash, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PendingUpgradeEta) {
        persistent.extend_ttl(&DataKey::PendingUpgradeEta, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn add_allowed_pool_to_list(env: &Env, pool: &Address) {
    let key = DataKey::AllowedPoolsList;
    let mut pools: Vec<Address> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env));
    for existing in pools.iter() {
        if existing == *pool {
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
            return;
        }
    }
    pools.push_back(pool.clone());
    if pools.len() > MAX_ALLOWED_POOLS {
        panic!("too many pools");
    }
    env.storage().persistent().set(&key, &pools);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn remove_allowed_pool_from_list(env: &Env, pool: &Address) {
    let key = DataKey::AllowedPoolsList;
    let pools: Vec<Address> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env));
    let mut out = Vec::new(env);
    for existing in pools.iter() {
        if existing != *pool {
            out.push_back(existing);
        }
    }
    env.storage().persistent().set(&key, &out);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn add_allowed_binding_to_list(env: &Env, pool_id: &BytesN<32>, pool: &Address) {
    let key = DataKey::AllowedPoolBindingsList;
    let mut bindings: Vec<(BytesN<32>, Address)> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env));
    for (existing_id, existing_pool) in bindings.iter() {
        if existing_id == *pool_id && existing_pool == *pool {
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
            return;
        }
    }
    bindings.push_back((pool_id.clone(), pool.clone()));
    if bindings.len() > MAX_ALLOWED_POOL_BINDINGS {
        panic!("too many pool bindings");
    }
    env.storage().persistent().set(&key, &bindings);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn remove_allowed_binding_from_list(env: &Env, pool_id: &BytesN<32>, pool: &Address) {
    let key = DataKey::AllowedPoolBindingsList;
    let bindings: Vec<(BytesN<32>, Address)> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env));
    let mut out = Vec::new(env);
    for (existing_id, existing_pool) in bindings.iter() {
        if !(existing_id == *pool_id && existing_pool == *pool) {
            out.push_back((existing_id, existing_pool));
        }
    }
    env.storage().persistent().set(&key, &out);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn bump_route_list_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    let pool_list_key = DataKey::AllowedPoolsList;
    if persistent.has(&pool_list_key) {
        persistent.extend_ttl(&pool_list_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    let binding_list_key = DataKey::AllowedPoolBindingsList;
    if persistent.has(&binding_list_key) {
        persistent.extend_ttl(&binding_list_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn bump_pool_route_ttl_range(env: &Env, start: u32, limit: u32) -> u32 {
    let persistent = env.storage().persistent();
    let pool_list_key = DataKey::AllowedPoolsList;
    if persistent.has(&pool_list_key) {
        persistent.extend_ttl(&pool_list_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let pools: Vec<Address> = persistent.get(&pool_list_key).unwrap_or(Vec::new(env));
    let len = pools.len();
    if start >= len {
        return len;
    }
    let end = start.saturating_add(limit).min(len);
    let mut i = start;
    while i < end {
        let pool = pools.get(i).expect("pool missing");
        bump_pool_ttl(env, &pool);
        i += 1;
    }
    end
}

fn bump_binding_route_ttl_range(env: &Env, start: u32, limit: u32) -> u32 {
    let persistent = env.storage().persistent();
    let binding_list_key = DataKey::AllowedPoolBindingsList;
    if persistent.has(&binding_list_key) {
        persistent.extend_ttl(&binding_list_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let bindings: Vec<(BytesN<32>, Address)> =
        persistent.get(&binding_list_key).unwrap_or(Vec::new(env));
    let len = bindings.len();
    if start >= len {
        return len;
    }
    let end = start.saturating_add(limit).min(len);
    let mut i = start;
    while i < end {
        let (pool_id, pool) = bindings.get(i).expect("binding missing");
        bump_pool_binding_ttl(env, &pool_id, &pool);
        i += 1;
    }
    end
}

fn bump_pool_ttl(env: &Env, pool: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::AllowedPool(pool.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn bump_pool_binding_ttl(env: &Env, pool_id: &BytesN<32>, pool: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::AllowedPoolBinding(pool_id.clone(), pool.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn ensure_pool_allowed(env: &Env, pool: &Address) {
    if !pool_allowed(env, pool) {
        panic!("pool not allowed");
    }
}

fn pool_allowed(env: &Env, pool: &Address) -> bool {
    bump_pool_ttl(env, pool);
    env.storage()
        .persistent()
        .get(&DataKey::AllowedPool(pool.clone()))
        .unwrap_or(false)
}

fn ensure_pool_binding_allowed(env: &Env, pool_id: &BytesN<32>, pool: &Address) {
    if pool_id.to_array() == [0u8; 32] {
        panic!("bad swaps");
    }
    bump_pool_binding_ttl(env, pool_id, pool);
    let allowed: bool = env
        .storage()
        .persistent()
        .get(&DataKey::AllowedPoolBinding(pool_id.clone(), pool.clone()))
        .unwrap_or(false);
    if !allowed {
        panic!("pool binding not allowed");
    }
}
