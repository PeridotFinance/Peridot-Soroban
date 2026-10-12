//! Local compiled-stack regressions, not live Mainnet simulations.
//! Actual concentrated pool code, controlled state; upstream feeds, boost/plane
//! and interest model remain native fixture dependencies. No configured gauges.
use super::*;
use price_router::{CrossQuoteConfig, ObservationConfig, PriceRouterClient, PriceSource};

#[contract]
struct StableFeed;
#[contractimpl]
impl StableFeed {
    pub fn __constructor(env: Env, quote: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "quote"), &quote);
    }
    pub fn base(env: Env) -> Asset {
        Asset::Stellar(
            env.storage()
                .instance()
                .get(&Symbol::new(&env, "quote"))
                .unwrap(),
        )
    }
    pub fn decimals(_env: Env) -> u32 {
        14
    }
    pub fn lastprice(env: Env, _asset: Asset) -> Option<PriceData> {
        Some(PriceData {
            price: 100_000_000_000_000,
            timestamp: env.ledger().timestamp(),
        })
    }
}

fn compiled_fixture() -> (Fixture, Address, Address, Address) {
    let read = |name| std::fs::read(std::env::var(name).expect(name)).unwrap();
    let receipt = read("LP_RECEIPT_WASM");
    let controller = read("LP_CONTROLLER_WASM");
    let strategy = read("LP_STRATEGY_WASM");
    let pool = read("AQUARIUS_CONCENTRATED_WASM");
    let router = read("LP_PRICE_ROUTER_WASM");
    let e = Env::default();
    let hash = e
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&e, &pool));
    let hex: std::string::String = hash
        .to_array()
        .iter()
        .map(|b| std::format!("{b:02x}"))
        .collect();
    assert_eq!(
        hex,
        "12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6"
    );
    let f = Fixture::build(
        true,
        Some(&receipt),
        Some(&controller),
        Some(&strategy),
        Some(&pool),
    );
    let id = f.env.register(router.as_slice(), ());
    let r = PriceRouterClient::new(&f.env, &id);
    r.initialize(&f.admin, &f.oracle, &300);
    let tokens: Vec<Address> = invoke(&f.env, &f.pools[0], "get_tokens", vec![&f.env]);
    let paired = tokens.get(1).unwrap();
    let reporter = Address::generate(&f.env);
    let stable_feed = f.env.register(StableFeed, (&f.assets[2],));
    r.set_source(
        &f.admin,
        &paired,
        &PriceSource::Observed(ObservationConfig {
            reporter: reporter.clone(),
            quote_to: f.assets[0].clone(),
            pool: f.pools[0].clone(),
            in_idx: 1,
            out_idx: 0,
            probe_amount: 1_000_000_000,
            window_secs: 300,
            max_age_secs: 300,
            min_interval_secs: 300,
            max_deviation_bps: 100,
            max_step_bps: 100,
            min_ratio: 800_000_000_000,
            max_ratio: 1_050_000_000_000,
        }),
    );
    // Synthetic PYUSD/USDC upstream is one, USDC/USD is one. Exercise the
    // compiled cross-quote path, without claiming these are real Reflector feeds.
    r.set_source(
        &f.admin,
        &f.assets[1],
        &PriceSource::CrossQuoted(CrossQuoteConfig {
            oracle: stable_feed,
            quote_to: f.assets[2].clone(),
            max_age_secs: 300,
        }),
    );
    for asset in &f.assets {
        r.set_required_observation(&f.admin, asset, &Some(paired.clone()));
    }
    f.c().set_oracle(&id);
    f.env.ledger().with_mut(|l| l.timestamp = 100_000);
    f.c().set_oracle(&id); // Honor controller governance delay.
    r.publish_observation(&reporter, &paired, &1_000_000_000_000, &99_700, &100_000);
    for asset in &f.assets {
        assert!(r
            .lastprice(&price_router::Asset::Stellar(asset.clone()))
            .is_some());
    }
    for i in 0..3 {
        f.reset();
        let s = AquariusLpVaultClient::new(&f.env, &f.strategies[i]);
        s.set_oracle(&f.admin, &id);
        s.refresh_nav_root();
        f.r(i).refresh_boosted_underlying();
    }
    (f, id, reporter, paired)
}

fn exact_primary_reward_route(f: &Fixture, market: usize) {
    let wasm = std::fs::read(std::env::var("AQUARIUS_CONCENTRATED_WASM").unwrap()).unwrap();
    let pool = f.env.register(wasm.as_slice(), ());
    let plane = f.env.register(TestPlane, ());
    let boost = f
        .env
        .register_stellar_asset_contract_v2(f.admin.clone())
        .address();
    let mut pair = [f.rewards[0].clone(), f.assets[market].clone()];
    pair.sort();
    for asset in &pair {
        token::StellarAssetClient::new(&f.env, asset).mint(&f.bob, &1_000_000_000_000);
    }
    let roles = (
        f.admin.clone(),
        f.admin.clone(),
        f.admin.clone(),
        f.admin.clone(),
        vec![&f.env, f.admin.clone()],
        f.admin.clone(),
    );
    f.env.mock_all_auths_allowing_non_root_auth();
    f.reset();
    invoke::<()>(
        &f.env,
        &pool,
        "initialize_all",
        (
            f.admin.clone(),
            roles,
            f.admin.clone(),
            vec![&f.env, pair[0].clone(), pair[1].clone()],
            10u32,
            20i32,
            0u32,
            (f.rewards[1].clone(), boost, plane.clone()),
            plane,
        )
            .into_val(&f.env),
    );
    f.reset();
    invoke::<(Vec<u128>, u128)>(
        &f.env,
        &pool,
        "deposit_position",
        (
            f.bob.clone(),
            -2000i32,
            2000i32,
            vec![&f.env, 100_000_000_000u128, 100_000_000_000u128],
            0u128,
        )
            .into_val(&f.env),
    );
    let s = AquariusLpVaultClient::new(&f.env, &f.strategies[market]);
    s.set_reward_route(&f.admin, &f.rewards[0], &Some(pool));
    s.set_reward_min_rate(&f.admin, &f.rewards[0], &9_000_000);
    f.env.mock_all_auths();
}

#[test]
#[ignore = "five explicit WASMs including exact pool; controlled upstream/routes, no chain writes"]
fn compiled_full_stack_oracle_outage_repayment_and_liquidation_recovery() {
    let (f, router, reporter, paired) = compiled_fixture();
    let r = PriceRouterClient::new(&f.env, &router);
    f.reset();
    f.r(1).borrow(&f.alice, &400_000);
    f.measure("full stack single-market loan");
    // Test both warm and unused debt markets after invalidation.
    r.invalidate_observation(&reporter, &paired);
    for i in [1, 2] {
        f.reset();
        assert!(f.r(i).try_borrow(&f.alice, &1).is_err());
    }
    f.reset();
    f.r(1).repay(&f.alice, &200_000);
    f.measure("full stack invalidated observation repayment");
    assert_eq!(f.r(1).get_user_borrow_balance(&f.alice), 200_000);
    assert_eq!(f.r(2).get_user_borrow_balance(&f.alice), 0);

    // XLM falls while the observation is absent. Liquidation cannot use a stale
    // price: prove atomic refusal, then recovery with a valid observation. This
    // explicitly documents the outage-liveness release gate, not a solution.
    OracleClient::new(&f.env, &f.oracle).set(&f.assets[0], &20_000_000_000_000);
    token::Client::new(&f.env, &f.assets[1]).approve(&f.bob, &f.markets[1], &100_000, &1000);
    let shares = f.r(0).get_ptoken_balance(&f.alice);
    let cash = token::Client::new(&f.env, &f.assets[1]).balance(&f.bob);
    f.reset();
    assert!(f
        .c()
        .try_liquidate(&f.alice, &f.markets[1], &f.markets[0], &100_000, &f.bob)
        .is_err());
    assert_eq!(f.r(0).get_ptoken_balance(&f.alice), shares);
    assert_eq!(
        token::Client::new(&f.env, &f.assets[1]).balance(&f.bob),
        cash
    );
    assert_eq!(f.r(1).get_user_borrow_balance(&f.alice), 200_000);
    f.env.ledger().with_mut(|l| l.timestamp = 100_300);
    f.reset();
    r.publish_observation(&reporter, &paired, &1_000_000_000_000, &100_000, &100_300);
    for i in 0..3 {
        f.reset();
        AquariusLpVaultClient::new(&f.env, &f.strategies[i]).refresh_nav_root();
        f.r(i).refresh_boosted_underlying();
    }
    f.reset();
    let debt = f.r(1).get_user_borrow_balance(&f.alice);
    f.env.mock_auths(&[]);
    f.reset();
    assert!(f
        .c()
        .try_liquidate(&f.alice, &f.markets[1], &f.markets[0], &100_000, &f.bob)
        .is_err());
    f.env.mock_auths(&[MockAuth {
        address: &f.bob,
        invoke: &MockAuthInvoke {
            contract: &f.controller,
            fn_name: "liquidate",
            args: (&f.alice, &f.markets[1], &f.markets[0], 100_000u128, &f.bob).into_val(&f.env),
            sub_invokes: &[],
        },
    }]);
    f.reset();
    f.c()
        .liquidate(&f.alice, &f.markets[1], &f.markets[0], &100_000, &f.bob);
    f.measure("full stack recovered oracle liquidation");
    assert_eq!(f.r(1).get_user_borrow_balance(&f.alice), debt - 100_000);
    assert!(f.r(0).get_ptoken_balance(&f.alice) < shares);
    f.env.mock_all_auths();
    f.env.ledger().with_mut(|l| l.timestamp = 100_601);
    f.reset();
    assert!(f.r(2).try_borrow(&f.alice, &1).is_err());
    f.reset();
    f.r(1).repay_max(&f.alice);
    f.measure("full stack expired observation repayment");
    assert_eq!(f.r(1).get_user_borrow_balance(&f.alice), 0);
}

#[test]
#[ignore = "compiled recovery router plus exact pool; controlled upstream/state, not Mainnet"]
fn compiled_full_stack_governed_recovery_keeps_borrowing_paused_until_completion() {
    let (f, router, reporter, paired) = compiled_fixture();
    let r = PriceRouterClient::new(&f.env, &router);
    f.reset();
    f.r(1).borrow(&f.alice, &200_000);
    let mut cfg = match r.get_source(&paired) {
        PriceSource::Observed(c) => c,
        _ => panic!("wrong fixture source"),
    };
    cfg.window_secs = 1800;
    cfg.min_interval_secs = 120;
    r.set_source(&f.admin, &paired, &PriceSource::Observed(cfg));
    f.reset();
    r.begin_observation_recovery(&f.admin, &paired, &1_000_000_000_000);
    for i in [1, 2] {
        f.reset();
        assert!(f.r(i).try_borrow(&f.alice, &1).is_err());
    }
    f.reset();
    f.r(1).repay(&f.alice, &100_000);
    f.measure("governed recovery pending repayment");
    f.env.ledger().with_mut(|l| l.timestamp = 101_800);
    f.reset();
    r.publish_recovery_observation(&reporter, &paired, &1_000_000_000_000, &100_000, &101_800);
    f.measure("governed recovery exact pool publication");
    assert!(r.get_observation(&paired).unwrap().valid);
    for i in [1, 2] {
        f.reset();
        assert!(f.r(i).try_borrow(&f.alice, &1).is_err());
    }
    f.reset();
    f.r(1).repay_max(&f.alice);
    assert_eq!(f.r(1).get_user_borrow_balance(&f.alice), 0);
    f.env.ledger().with_mut(|l| l.timestamp = 102_100);
    f.reset();
    r.publish_observation(&reporter, &paired, &1_000_000_000_000, &100_300, &102_100);
    f.reset();
    assert!(f.r(2).try_borrow(&f.alice, &1).is_err());
    f.reset();
    r.finish_observation_recovery(&f.admin, &paired);
    f.measure("governed recovery admin completion");
    for i in 0..3 {
        f.reset();
        AquariusLpVaultClient::new(&f.env, &f.strategies[i]).refresh_nav_root();
        f.r(i).refresh_boosted_underlying();
    }
    f.reset();
    f.r(2).borrow(&f.alice, &100_000);
    f.measure("governed recovery resumed borrowing");
    f.reset();
    f.r(2).repay_max(&f.alice);
    assert_eq!(f.r(2).get_user_borrow_balance(&f.alice), 0);
}

#[test]
#[ignore = "explicit compiled stack; second loan must fit without increasing Mainnet memory limit"]
fn compiled_full_stack_simultaneous_debts() {
    let (f, _, _, _) = compiled_fixture();
    for i in [1, 2] {
        f.env.mock_auths(&[]);
        f.reset();
        assert!(f.r(i).try_borrow(&f.alice, &200_000).is_err());
        f.env.mock_auths(&[MockAuth {
            address: &f.alice,
            invoke: &MockAuthInvoke {
                contract: &f.markets[i],
                fn_name: "borrow",
                args: (&f.alice, 200_000u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }]);
        f.reset();
        f.r(i).borrow(&f.alice, &200_000);
        f.measure("full stack simultaneous loan");
    }
    f.env.mock_all_auths();
    for i in [1, 2] {
        f.reset();
        f.r(i).repay_max(&f.alice);
        assert_eq!(f.r(i).get_user_borrow_balance(&f.alice), 0);
    }
}

#[test]
fn lp_price_snapshot_errors_do_not_fall_back_to_warm_cache_or_legacy_price() {
    let f = Fixture::new(true);
    assert!(f.c().cache_price(&f.assets[0]).is_some());
    for fault in 1..=5 {
        OracleClient::new(&f.env, &f.oracle).set_snapshot_fault(&fault);
        // Legacy method still reports a valid price; the LP path MUST NOT use it.
        assert!(OracleClient::new(&f.env, &f.oracle)
            .lastprice(&Asset::Stellar(f.assets[0].clone()))
            .is_some());
        assert!(f.c().get_price_usd(&f.assets[0]).is_none());
        f.reset();
        assert!(f.r(1).try_borrow(&f.alice, &1).is_err());
    }
    OracleClient::new(&f.env, &f.oracle).set_snapshot_fault(&0);
    f.reset();
    f.r(1).borrow(&f.alice, &1);
}

#[test]
#[ignore = "five explicit WASMs; actual primary pool and swap code, controlled local state, no real gauges"]
fn compiled_full_stack_primary_rewards_keep_owners_through_claim_outage() {
    let (f, _, _, _) = compiled_fixture();
    exact_primary_reward_route(&f, 1);
    token::StellarAssetClient::new(&f.env, &f.rewards[0]).mint(&f.pools[1], &1_000_000_000_000);
    invoke::<()>(
        &f.env,
        &f.pools[1],
        "set_rewards_config",
        (f.admin.clone(), 110_000u64, 1_000_000_000u128).into_val(&f.env),
    );
    f.env.ledger().with_mut(|l| l.timestamp += 100);
    let pending: u128 = invoke(
        &f.env,
        &f.pools[1],
        "get_user_reward",
        (f.strategies[1].clone(),).into_val(&f.env),
    );
    assert!(pending > 0);
    invoke::<()>(
        &f.env,
        &f.pools[1],
        "kill_claim",
        (f.admin.clone(),).into_val(&f.env),
    );
    f.reset();
    f.r(1).borrow(&f.alice, &200_000);
    f.measure("full stack borrow during observable primary-claim outage");
    assert_eq!(f.r(1).reward_earned(&f.rewards[0], &f.bob), pending);
    assert_eq!(f.r(1).reward_earned(&f.rewards[0], &f.alice), 0);
    f.reset();
    f.r(1).repay_max(&f.alice);
    assert_eq!(f.r(1).get_user_borrow_balance(&f.alice), 0);
    invoke::<()>(
        &f.env,
        &f.pools[1],
        "unkill_claim",
        (f.admin.clone(),).into_val(&f.env),
    );
    f.reset();
    assert!(matches!(
        f.r(1).compound(&f.rewards[0]),
        Outcome::Completed(_)
    ));
    f.measure("full stack recovered primary reward conversion");
    let before = token::Client::new(&f.env, &f.assets[1]).balance(&f.bob);
    f.reset();
    let Outcome::Completed(paid) = f.r(1).payout_rewards(&f.bob, &1) else {
        panic!("expected payout")
    };
    f.measure("full stack converted reward payout");
    assert!(paid > 0);
    assert_eq!(
        token::Client::new(&f.env, &f.assets[1]).balance(&f.bob) - before,
        paid as i128
    );
    assert_eq!(f.r(1).reward_earned(&f.rewards[0], &f.alice), 0);
}

#[test]
fn accrued_snapshot_refreshes_debt_and_rolls_back_when_health_is_stale() {
    let f = Fixture::new(true);
    f.r(1).borrow(&f.alice, &200_000);
    let before = f.r(1).get_user_borrow_balance(&f.alice);
    f.env.ledger().with_mut(|l| l.timestamp += 10_000);
    f.reset();
    assert!(f.r(1).try_get_accrued_account_snapshot(&f.alice).is_err());
    // Failed cache validation must roll back the interest accrual as well.
    assert_eq!(f.r(1).get_user_borrow_balance(&f.alice), before);
    AquariusLpVaultClient::new(&f.env, &f.strategies[1]).refresh_nav_root();
    f.r(1).refresh_boosted_underlying();
    let shares = f.r(1).get_ptoken_balance(&f.bob);
    f.reset();
    let (_, debt, _, asset) = f.r(1).get_accrued_account_snapshot(&f.alice);
    assert!(debt > before);
    assert_eq!(asset, f.assets[1]);
    assert_eq!(f.r(1).get_ptoken_balance(&f.bob), shares);
}
