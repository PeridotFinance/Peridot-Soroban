//! Local exact pool-WASM rehearsal, with native LP/strategy and controlled mocks
//! for oracle/plane/reward route. This is NOT a full compiled-stack/network test.
use super::*;
use soroban_sdk::{Bytes, Val};

#[contract]
struct TestPlane;
#[contractimpl]
impl TestPlane {
    pub fn total_supply(_env: Env) -> u128 {
        0
    }
    pub fn update(
        _env: Env,
        _pool: Address,
        _kind: Symbol,
        _args: Vec<u128>,
        _reserves: Vec<u128>,
    ) {
    }
}

fn invoke<T: soroban_sdk::TryFromVal<Env, Val>>(
    env: &Env,
    target: &Address,
    name: &str,
    args: Vec<Val>,
) -> T {
    env.invoke_contract(target, &Symbol::new(env, name), args)
}

fn fixture(index: u32) -> Fixture {
    let path =
        std::env::var("AQUARIUS_CONCENTRATED_WASM").expect("set exact deployed pool WASM path");
    let wasm = std::fs::read(path).unwrap();
    let env = Env::default();
    let hash = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &wasm))
        .to_array();
    let hex: std::string::String = hash.iter().map(|b| std::format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        "12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6"
    );
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::from_str(&env, crate::contract::DEFAULT_INIT_ADMIN);
    let receipt_admin = Address::generate(&env);
    let user = Address::generate(&env);
    let a = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let b = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let reward = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let boost = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let (token0, token1) = if a < b { (a, b) } else { (b, a) };
    let (asset, paired) = if index == 0 {
        (token0.clone(), token1.clone())
    } else {
        (token1.clone(), token0.clone())
    };
    env.cost_estimate().budget().reset_unlimited();
    let pool = env.register(wasm.as_slice(), ());
    let plane = env.register(TestPlane, ());
    let roles = (
        admin.clone(),
        admin.clone(),
        admin.clone(),
        admin.clone(),
        soroban_sdk::vec![&env, admin.clone()],
        admin.clone(),
    );
    invoke::<()>(
        &env,
        &pool,
        "initialize_all",
        (
            admin.clone(),
            roles,
            admin.clone(),
            soroban_sdk::vec![&env, token0.clone(), token1.clone()],
            10u32,
            20i32,
            0u32,
            (reward.clone(), boost, plane.clone()),
            plane,
        )
            .into_val(&env),
    );
    for token in [&asset, &paired] {
        token::StellarAssetClient::new(&env, token).mint(&user, &1_000_000_000_000);
    }
    env.cost_estimate().budget().reset_unlimited();
    invoke::<(Vec<u128>, u128)>(
        &env,
        &pool,
        "deposit_position",
        (
            user.clone(),
            -2000i32,
            2000i32,
            soroban_sdk::vec![&env, 100_000_000_000u128, 100_000_000_000u128],
            0u128,
        )
            .into_val(&env),
    );
    let route = env.register(MockAquariusPool, ());
    let r = MockAquariusPoolClient::new(&env, &route);
    r.initialize(&reward, &asset, &60, &0);
    for token in [&reward, &asset] {
        token::StellarAssetClient::new(&env, token).mint(&user, &1_000_000_000_000);
    }
    r.deposit_position(
        &user,
        &-887220,
        &887220,
        &soroban_sdk::vec![&env, 1_000_000_000_000u128, 1_000_000_000_000u128],
        &0,
    );
    let oracle = env.register(MockOracle, ());
    for token in [&asset, &paired] {
        MockOracleClient::new(&env, &oracle).set_price(token, &100_000_000_000_000);
    }
    let strategy = env.register(AquariusLpVault, ());
    let s = AquariusLpVaultClient::new(&env, &strategy);
    s.initialize(&admin, &pool, &index, &oracle);
    s.set_primary_reward_token(&admin, &Some(reward.clone()));
    s.set_reward_route(&admin, &reward, &Some(route.clone()));
    s.set_reward_min_rate(&admin, &reward, &9_000_000);
    let receipt = env.register(HybridReceipt, ());
    let r = HybridReceiptClient::new(&env, &receipt);
    r.initialize(&asset, &receipt_admin);
    r.static_rates(&receipt_admin);
    r.init_backing();
    r.bind(&receipt_admin, &strategy);
    r.set_buffer(&receipt_admin, &0);
    s.set_receipt_vault(&admin, &receipt);
    s.refresh_nav_root();
    env.cost_estimate().budget().reset_unlimited();
    r.deposit(&user, &1_000_000);
    r.co_register(&reward);
    env.mock_all_auths();
    Fixture {
        env,
        admin,
        receipt_admin,
        user,
        receipt,
        strategy,
        pool,
        route,
        asset,
        paired,
        reward,
        oracle,
    }
}

#[test]
#[ignore = "requires the hash-pinned actual Mainnet concentrated pool WASM; local execution only"]
fn exact_deployed_pool_claim_pause_preserves_quote_and_proportional_exit() {
    paused_exit(0, false);
}

#[test]
#[ignore = "requires the hash-pinned actual Mainnet concentrated pool WASM; local execution only"]
fn exact_deployed_pool_stale_oracle_and_claim_pause_second_settlement_leg() {
    paused_exit(1, true);
}

#[test]
#[ignore = "requires the hash-pinned actual Mainnet concentrated pool WASM; local execution only"]
fn exact_deployed_pool_rotation_unwinds_and_preserves_old_rewards() {
    for index in [0, 1] {
        let f = fixture(index);
        let (next, _) = super::reward_coordinator_test::second_token(&f);
        token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.pool, &1_000_000_000);
        invoke::<()>(
            &f.env,
            &f.pool,
            "set_rewards_config",
            (f.admin.clone(), 10_000u64, 100_000u128).into_val(&f.env),
        );
        f.env.ledger().set_timestamp(100);
        let pending: u128 = invoke(
            &f.env,
            &f.pool,
            "get_user_reward",
            (f.strategy.clone(),).into_val(&f.env),
        );
        assert!(pending > 0);
        f.env.cost_estimate().budget().reset_unlimited();
        f.env.mock_auths(&[]);
        f.receipt()
            .mock_auths(&[MockAuth {
                address: &f.receipt_admin,
                invoke: &MockAuthInvoke {
                    contract: &f.receipt,
                    fn_name: "co_prepare_rotate",
                    args: (900_000u128,).into_val(&f.env),
                    sub_invokes: &[],
                },
            }])
            .co_prepare_rotate(&900_000);
        let prepared = f.env.cost_estimate().resources();
        std::println!(
            "exact pool rotation preparation leg{index}: {} entries / {} instructions",
            prepared.memory_read_entries + prepared.write_entries,
            prepared.instructions
        );
        assert!(prepared.memory_read_entries + prepared.write_entries <= 100);
        assert!(prepared.instructions <= 100_000_000);
        f.env.cost_estimate().budget().reset_unlimited();
        let cash = f
            .receipt()
            .mock_auths(&[MockAuth {
                address: &f.receipt_admin,
                invoke: &MockAuthInvoke {
                    contract: &f.receipt,
                    fn_name: "co_rotate",
                    args: (next.clone(), 900_000u128).into_val(&f.env),
                    sub_invokes: &[],
                },
            }])
            .co_rotate(&next, &900_000);
        let r = f.env.cost_estimate().resources();
        std::println!(
            "exact pool rotation leg{index}: {} entries / {} instructions",
            r.memory_read_entries + r.write_entries,
            r.instructions
        );
        assert!(r.memory_read_entries + r.write_entries <= 100);
        assert!(r.instructions <= 100_000_000);
        assert!(cash >= 900_000);
        assert_eq!(f.receipt().balance(&f.user), 1_000_000);
        assert_eq!(
            AquariusLpVaultClient::new(&f.env, &f.strategy).balance(&f.receipt),
            0
        );
        let account = f.receipt().earned(&f.reward, &f.user);
        assert_eq!(account.raw_scaled, pending * ledger::SCALE);
        let position: crate::pool::UserPositionSnapshot = invoke(
            &f.env,
            &f.pool,
            "get_user_position_snapshot",
            (f.strategy.clone(),).into_val(&f.env),
        );
        assert_eq!(position.raw_liquidity, 0);
        assert_eq!(position.weighted_liquidity, 0);
        // This only changes our native strategy's expectation. No Aquarius token
        // mutation is exposed by this pinned binary; real new-token claim proof
        // remains unavailable here, so never call this a live reward migration.
    }
}

fn paused_exit(index: u32, stale: bool) {
    let f = fixture(index);
    let p = &f.pool;
    token::StellarAssetClient::new(&f.env, &f.reward).mint(p, &1_000_000_000);
    invoke::<()>(
        &f.env,
        p,
        "set_rewards_config",
        (f.admin.clone(), 10000u64, 100_000u128).into_val(&f.env),
    );
    if stale {
        AquariusLpVaultClient::new(&f.env, &f.strategy).set_nav_root_max_stale(&f.admin, &301);
        MockOracleClient::new(&f.env, &f.oracle).set_stale(&true);
    }
    f.env.ledger().set_timestamp(if stale { 400 } else { 100 });
    let pending: u128 = invoke(
        &f.env,
        p,
        "get_user_reward",
        (f.strategy.clone(),).into_val(&f.env),
    );
    assert!(pending > 0);
    invoke::<()>(&f.env, p, "kill_claim", (f.admin.clone(),).into_val(&f.env));
    f.env.cost_estimate().budget().reset_unlimited();
    f.env.mock_auths(&[]);
    let (reserved, paid) = f
        .receipt()
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "co_exit",
                args: (f.user.clone(), 1_000_000u128, 900_000u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .co_exit(&f.user, &1_000_000, &900_000);
    assert_eq!(reserved.get(f.reward.clone()), Some(pending));
    assert!(paid >= 900_000);
    let resources = f.env.cost_estimate().resources();
    std::println!(
        "exact deployed pool paused-claim exit leg{index}, stale={stale}: {} entries / {} instructions",
        resources.memory_read_entries + resources.write_entries,
        resources.instructions
    );
    assert!(resources.memory_read_entries + resources.write_entries <= 100);
    assert!(resources.instructions <= 100_000_000);
    assert_eq!(f.receipt().balance(&f.user), 0);
    assert_eq!(
        AquariusLpVaultClient::new(&f.env, &f.strategy).balance(&f.receipt),
        0
    );
    f.env.mock_all_auths();
    invoke::<()>(
        &f.env,
        p,
        "unkill_claim",
        (f.admin.clone(),).into_val(&f.env),
    );
    f.env.cost_estimate().budget().reset_unlimited();
    f.env.mock_auths(&[]);
    let outcome = f
        .receipt()
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "co_settle",
                args: (f.reward.clone(), f.user.clone(), pending, 1u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .co_settle(&f.reward, &f.user, &pending, &1);
    assert!(matches!(outcome, coordinator::Outcome::Completed(_)));
    f.env.as_contract(&f.receipt, || {
        coordinator::check(&f.env);
        assert_eq!(coordinator::owed(&f.env, &f.reward), 0);
        assert_eq!(ledger::stream(&f.env, &f.reward).reserved, 0);
    });
}
