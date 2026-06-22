use super::*;
use mock_token::{MockToken, MockTokenClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{contract, contractimpl, Env, IntoVal, String};

fn assert_budget_under(env: &Env, max_cpu: u64, max_mem: u64) {
    let budget = env.cost_estimate().budget();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();
    assert!(cpu <= max_cpu, "cpu cost {cpu} exceeds {max_cpu}");
    assert!(mem <= max_mem, "mem cost {mem} exceeds {max_mem}");
}

#[contract]
struct MockSoroswapRouter;

#[contractimpl]
impl MockSoroswapRouter {
    pub fn swap_exact_tokens_for_tokens(
        env: Env,
        amount_in: i128,
        _amount_out_min: i128,
        path: Vec<Address>,
        to: Address,
        _deadline: u64,
    ) -> Vec<i128> {
        let token_out = path.get(path.len() - 1).unwrap();
        MockTokenClient::new(&env, &token_out).mint(&to, &amount_in);
        Vec::from_array(&env, [amount_in, amount_in])
    }
}

#[contract]
struct MockAquariusRouter;

#[contractimpl]
impl MockAquariusRouter {
    pub fn swap(
        _env: Env,
        _user: Address,
        _tokens: Vec<Address>,
        _token_in: Address,
        _token_out: Address,
        _pool_index: BytesN<32>,
        _in_amount: u128,
        out_min: u128,
    ) -> u128 {
        out_min
    }

    pub fn swap_chained(
        _env: Env,
        _user: Address,
        _swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        _token_in: Address,
        _amount: u128,
        amount_with_slippage: u128,
    ) -> u128 {
        amount_with_slippage
    }
}

#[contract]
struct MockAquariusPool;

#[contract]
struct MockAquariusPoolIndices;

#[contract]
struct MockSoroswapRouterEmpty;

#[contractimpl]
impl MockAquariusPool {
    pub fn estimate_swap(_env: Env, _in_idx: u32, _out_idx: u32, amount_in: u128) -> u128 {
        amount_in
    }

    pub fn swap(
        _env: Env,
        _user: Address,
        _in_idx: u32,
        _out_idx: u32,
        _amount_in: u128,
        amount_out_min: u128,
    ) -> u128 {
        amount_out_min
    }
}

#[contractimpl]
impl MockAquariusPoolIndices {
    pub fn estimate_swap(_env: Env, _in_idx: u32, _out_idx: u32, amount_in: u128) -> u128 {
        amount_in
    }

    pub fn swap(
        _env: Env,
        _user: Address,
        in_idx: u32,
        out_idx: u32,
        _amount_in: u128,
        _amount_out_min: u128,
    ) -> u128 {
        (in_idx as u128)
            .saturating_mul(10)
            .saturating_add(out_idx as u128)
    }
}

#[contractimpl]
impl MockSoroswapRouterEmpty {
    pub fn swap_exact_tokens_for_tokens(
        env: Env,
        _amount_in: i128,
        _amount_out_min: i128,
        _path: Vec<Address>,
        _to: Address,
        _deadline: u64,
    ) -> Vec<i128> {
        Vec::new(&env)
    }
}

fn default_admin(env: &Env) -> Address {
    Address::from_string(&String::from_str(env, DEFAULT_INIT_ADMIN))
}

fn setup() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = default_admin(&env);
    let user = Address::generate(&env);

    let token_a_id = env.register(MockToken, ());
    let token_b_id = env.register(MockToken, ());
    let token_a = MockTokenClient::new(&env, &token_a_id);
    let token_b = MockTokenClient::new(&env, &token_b_id);
    token_a.initialize(&"TKA".into_val(&env), &"TKA".into_val(&env), &6u32);
    token_b.initialize(&"TKB".into_val(&env), &"TKB".into_val(&env), &6u32);

    token_a.mint(&user, &1_000_000i128);

    let router_id = env.register(MockSoroswapRouter, ());

    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.initialize(&admin, &router_id);

    (env, adapter_id, token_a_id, token_b_id, user)
}

#[test]
fn test_initialize() {
    let (env, adapter_id, token_a_id, token_b_id, user) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);

    let path = Vec::from_array(&env, [token_a_id.clone(), token_b_id.clone()]);
    let out = adapter.swap_exact_tokens_for_tokens(&user, &100u128, &1u128, &path, &9999u64);
    assert_eq!(out, 100u128);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice_panics() {
    let (env, adapter_id, _, _, _) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let admin = default_admin(&env);
    let router = Address::generate(&env);
    adapter.initialize(&admin, &router);
}

#[test]
#[should_panic(expected = "unexpected admin")]
fn test_initialize_wrong_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let wrong_admin = Address::generate(&env);
    let router = Address::generate(&env);
    adapter.initialize(&wrong_admin, &router);
}

#[test]
fn test_set_router() {
    let (env, adapter_id, token_a_id, token_b_id, user) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let admin = default_admin(&env);

    let new_router_id = env.register(MockSoroswapRouter, ());
    adapter.set_router(&admin, &new_router_id);

    let path = Vec::from_array(&env, [token_a_id.clone(), token_b_id.clone()]);
    let out = adapter.swap_exact_tokens_for_tokens(&user, &50u128, &1u128, &path, &9999u64);
    assert_eq!(out, 50u128);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_router_non_admin_panics() {
    let (env, adapter_id, _, _, _) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let non_admin = Address::generate(&env);
    let router = Address::generate(&env);
    adapter.set_router(&non_admin, &router);
}

#[test]
fn test_swap_exact_tokens() {
    let (env, adapter_id, token_a_id, token_b_id, user) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);

    let path = Vec::from_array(&env, [token_a_id.clone(), token_b_id.clone()]);
    let out = adapter.swap_exact_tokens_for_tokens(&user, &500u128, &100u128, &path, &9999u64);
    assert_eq!(out, 500u128);

    let token_b = MockTokenClient::new(&env, &token_b_id);
    assert_eq!(token_b.balance(&user), 500i128);
}

#[test]
#[should_panic(expected = "amount too large")]
fn test_swap_exact_tokens_rejects_amount_over_i128() {
    let (env, adapter_id, token_a_id, token_b_id, user) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let path = Vec::from_array(&env, [token_a_id.clone(), token_b_id.clone()]);
    let _ = adapter.swap_exact_tokens_for_tokens(
        &user,
        &(i128::MAX as u128 + 1),
        &1u128,
        &path,
        &9999u64,
    );
}

#[test]
#[should_panic(expected = "amount too large")]
fn test_swap_exact_tokens_rejects_amount_out_min_over_i128() {
    let (env, adapter_id, token_a_id, token_b_id, user) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let path = Vec::from_array(&env, [token_a_id.clone(), token_b_id.clone()]);
    let _ = adapter.swap_exact_tokens_for_tokens(
        &user,
        &1u128,
        &(i128::MAX as u128 + 1),
        &path,
        &9999u64,
    );
}

#[test]
#[should_panic(expected = "zero slippage")]
fn test_swap_exact_tokens_rejects_zero_slippage_min() {
    let (env, adapter_id, token_a_id, token_b_id, user) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let path = Vec::from_array(&env, [token_a_id.clone(), token_b_id.clone()]);
    let _ = adapter.swap_exact_tokens_for_tokens(&user, &100u128, &0u128, &path, &9999u64);
}

#[test]
#[should_panic(expected = "deadline too far")]
fn test_swap_exact_tokens_rejects_far_deadline() {
    let (env, adapter_id, token_a_id, token_b_id, user) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let path = Vec::from_array(&env, [token_a_id.clone(), token_b_id.clone()]);
    let now = env.ledger().timestamp();
    let too_far = now.saturating_add(MAX_DEADLINE_SECONDS).saturating_add(1);
    let _ = adapter.swap_exact_tokens_for_tokens(&user, &100u128, &1u128, &path, &too_far);
}

#[test]
#[should_panic(expected = "router returned empty amounts")]
fn test_swap_exact_tokens_rejects_empty_router_amounts() {
    let (env, adapter_id, token_a_id, token_b_id, user) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let admin = default_admin(&env);
    let empty_router_id = env.register(MockSoroswapRouterEmpty, ());
    adapter.set_router(&admin, &empty_router_id);
    let path = Vec::from_array(&env, [token_a_id.clone(), token_b_id.clone()]);
    let _ = adapter.swap_exact_tokens_for_tokens(&user, &100u128, &1u128, &path, &9999u64);
}

#[test]
#[should_panic(expected = "deadline expired")]
fn test_swap_exact_tokens_rejects_past_deadline() {
    let (env, adapter_id, token_a_id, token_b_id, user) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let path = Vec::from_array(&env, [token_a_id.clone(), token_b_id.clone()]);
    env.ledger().with_mut(|l| l.timestamp = 100);
    let _ = adapter.swap_exact_tokens_for_tokens(&user, &100u128, &1u128, &path, &99u64);
}

#[test]
fn test_swap_chained() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = default_admin(&env);
    let user = Address::generate(&env);

    let router_id = env.register(MockAquariusRouter, ());
    let pool = env.register(MockAquariusPool, ());
    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.initialize(&admin, &router_id);
    let pool_id = BytesN::from_array(&env, &[1u8; 32]);
    adapter.set_pool_allowed(&admin, &pool, &true);
    adapter.set_pool_binding(&admin, &pool_id, &pool, &true);

    let token_in = Address::generate(&env);
    let token_out = Address::generate(&env);
    let path = Vec::from_array(&env, [token_in.clone(), token_out]);
    let hops = Vec::from_array(&env, [(path, pool_id, pool)]);
    let out = adapter.swap_chained(&user, &hops, &token_in, &10u128, &9u128);
    assert_eq!(out, 9u128);
}

#[test]
fn test_swap_chained_infers_reverse_pool_indices() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = default_admin(&env);
    let user = Address::generate(&env);

    let router_id = env.register(MockAquariusRouter, ());
    let pool = env.register(MockAquariusPoolIndices, ());
    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.initialize(&admin, &router_id);
    let pool_id = BytesN::from_array(&env, &[7u8; 32]);
    adapter.set_pool_allowed(&admin, &pool, &true);
    adapter.set_pool_binding(&admin, &pool_id, &pool, &true);

    let token_0 = Address::generate(&env);
    let token_1 = Address::generate(&env);
    let pool_tokens = Vec::from_array(&env, [token_0.clone(), token_1.clone()]);
    let hops = Vec::from_array(&env, [(pool_tokens, pool_id, pool)]);
    let out = adapter.swap_chained(&user, &hops, &token_1, &10u128, &1u128);
    assert_eq!(out, 10u128);
}

#[test]
#[should_panic(expected = "pool binding not allowed")]
fn test_swap_chained_requires_allowlisted_pool_binding() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = default_admin(&env);
    let user = Address::generate(&env);

    let router_id = env.register(MockAquariusRouter, ());
    let pool_id = env.register(MockAquariusPool, ());
    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.initialize(&admin, &router_id);

    let token_in = Address::generate(&env);
    let token_out = Address::generate(&env);
    let path = Vec::from_array(&env, [token_in.clone(), token_out]);
    let hops = Vec::from_array(
        &env,
        [(path, BytesN::from_array(&env, &[2u8; 32]), pool_id)],
    );

    let _ = adapter.swap_chained(&user, &hops, &token_in, &10u128, &9u128);
}

#[test]
#[should_panic(expected = "pool binding not allowed")]
fn test_swap_chained_rejects_mismatched_pool_id() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = default_admin(&env);
    let user = Address::generate(&env);

    let router_id = env.register(MockAquariusRouter, ());
    let allowed_pool = env.register(MockAquariusPool, ());
    let other_pool = env.register(MockAquariusPool, ());
    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.initialize(&admin, &router_id);
    let pool_id = BytesN::from_array(&env, &[3u8; 32]);
    adapter.set_pool_binding(&admin, &pool_id, &allowed_pool, &true);

    let token_in = Address::generate(&env);
    let token_out = Address::generate(&env);
    let path = Vec::from_array(&env, [token_in.clone(), token_out]);
    let hops = Vec::from_array(&env, [(path, pool_id, other_pool)]);

    let _ = adapter.swap_chained(&user, &hops, &token_in, &10u128, &9u128);
}

#[test]
#[should_panic(expected = "pool not allowed")]
fn test_swap_chained_rejects_disabled_pool_even_with_binding() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = default_admin(&env);
    let user = Address::generate(&env);

    let router_id = env.register(MockAquariusRouter, ());
    let pool = env.register(MockAquariusPool, ());
    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.initialize(&admin, &router_id);
    let pool_id = BytesN::from_array(&env, &[4u8; 32]);
    adapter.set_pool_allowed(&admin, &pool, &true);
    adapter.set_pool_binding(&admin, &pool_id, &pool, &true);
    adapter.set_pool_allowed(&admin, &pool, &false);

    let token_in = Address::generate(&env);
    let token_out = Address::generate(&env);
    let path = Vec::from_array(&env, [token_in.clone(), token_out]);
    let hops = Vec::from_array(&env, [(path, pool_id, pool)]);

    let _ = adapter.swap_chained(&user, &hops, &token_in, &10u128, &9u128);
}

#[test]
fn test_is_pool_binding_allowed_respects_disabled_pool() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = default_admin(&env);

    let router_id = env.register(MockAquariusRouter, ());
    let pool = env.register(MockAquariusPool, ());
    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.initialize(&admin, &router_id);
    let pool_id = BytesN::from_array(&env, &[5u8; 32]);
    adapter.set_pool_allowed(&admin, &pool, &true);
    adapter.set_pool_binding(&admin, &pool_id, &pool, &true);

    assert!(adapter.is_pool_binding_allowed(&pool_id, &pool));

    adapter.set_pool_allowed(&admin, &pool, &false);

    assert!(!adapter.is_pool_binding_allowed(&pool_id, &pool));
}

#[test]
fn test_swap_pool_and_estimate() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = default_admin(&env);
    let user = Address::generate(&env);

    let router_id = env.register(MockAquariusRouter, ());
    let pool_id = env.register(MockAquariusPool, ());
    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.initialize(&admin, &router_id);
    adapter.set_pool_allowed(&admin, &pool_id, &true);

    let est = adapter.estimate_pool_swap(&pool_id, &0u32, &1u32, &123u128);
    assert_eq!(est, 123u128);

    let out = adapter.swap_pool(&user, &pool_id, &0u32, &1u32, &100u128, &99u128);
    assert_eq!(out, 99u128);
}

#[test]
#[should_panic(expected = "pool not allowed")]
fn test_swap_pool_requires_allowlisted_pool() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = default_admin(&env);
    let user = Address::generate(&env);

    let router_id = env.register(MockAquariusRouter, ());
    let pool_id = env.register(MockAquariusPool, ());
    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.initialize(&admin, &router_id);

    let _ = adapter.swap_pool(&user, &pool_id, &0u32, &1u32, &100u128, &99u128);
}

#[test]
#[should_panic(expected = "pool not allowed")]
fn test_estimate_pool_swap_requires_allowlisted_pool() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = default_admin(&env);

    let router_id = env.register(MockAquariusRouter, ());
    let pool_id = env.register(MockAquariusPool, ());
    let adapter_id = env.register(SwapAdapter, ());
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.initialize(&admin, &router_id);

    let _ = adapter.estimate_pool_swap(&pool_id, &0u32, &1u32, &100u128);
}

#[test]
fn test_bump_ttl() {
    let (env, adapter_id, _, _, _) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    adapter.bump_ttl();
}

#[test]
fn test_bump_ttl_does_not_iterate_large_route_lists() {
    let (env, adapter_id, _, _, _) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let admin = default_admin(&env);

    for i in 0..20u8 {
        let pool = env.register(MockAquariusPool, ());
        let pool_id = BytesN::from_array(&env, &[i.saturating_add(1); 32]);
        adapter.set_pool_allowed(&admin, &pool, &true);
        adapter.set_pool_binding(&admin, &pool_id, &pool, &true);
    }

    env.cost_estimate().budget().reset_unlimited();
    adapter.bump_ttl();
    assert_budget_under(&env, 1_500_000, 350_000);
}

#[test]
fn test_bump_route_ttl_batch_paginates_route_entries() {
    let (env, adapter_id, _, _, _) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let admin = default_admin(&env);

    for i in 0..10u8 {
        let pool = env.register(MockAquariusPool, ());
        let pool_id = BytesN::from_array(&env, &[i.saturating_add(1); 32]);
        adapter.set_pool_allowed(&admin, &pool, &true);
        adapter.set_pool_binding(&admin, &pool_id, &pool, &true);
    }

    let next = adapter.bump_route_ttl_batch(&admin, &0u32, &0u32, &4u32);
    assert_eq!(next, (4u32, 4u32));
    let next = adapter.bump_route_ttl_batch(&admin, &next.0, &next.1, &4u32);
    assert_eq!(next, (8u32, 8u32));
    let next = adapter.bump_route_ttl_batch(&admin, &next.0, &next.1, &4u32);
    assert_eq!(next, (10u32, 10u32));
}

#[test]
#[should_panic(expected = "not admin")]
fn test_bump_route_ttl_batch_requires_admin() {
    let (env, adapter_id, _, _, _) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let non_admin = Address::generate(&env);

    adapter.bump_route_ttl_batch(&non_admin, &0u32, &0u32, &4u32);
}

#[test]
#[should_panic(expected = "too many pools")]
fn test_set_pool_allowed_rejects_pool_list_over_cap() {
    let (env, adapter_id, _, _, _) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let admin = default_admin(&env);

    for _ in 0..=MAX_ALLOWED_POOLS {
        let pool = env.register(MockAquariusPool, ());
        adapter.set_pool_allowed(&admin, &pool, &true);
    }
}

#[test]
#[should_panic(expected = "too many pool bindings")]
fn test_set_pool_binding_rejects_binding_list_over_cap() {
    let (env, adapter_id, _, _, _) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let admin = default_admin(&env);

    for i in 0..=MAX_ALLOWED_POOL_BINDINGS {
        let pool = env.register(MockAquariusPool, ());
        let byte = (i as u8).wrapping_add(1);
        let pool_id = BytesN::from_array(&env, &[byte; 32]);
        adapter.set_pool_binding(&admin, &pool_id, &pool, &true);
    }
}

#[test]
#[should_panic(expected = "not admin")]
fn test_upgrade_wasm_non_admin_panics() {
    let (env, adapter_id, _, _, _) = setup();
    let adapter = SwapAdapterClient::new(&env, &adapter_id);
    let non_admin = Address::generate(&env);
    let fake_hash = BytesN::from_array(&env, &[0u8; 32]);
    adapter.upgrade_wasm(&non_admin, &fake_hash);
}
