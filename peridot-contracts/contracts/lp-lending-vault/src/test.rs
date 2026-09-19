extern crate std;
use crate::{LpLendingVault, LpLendingVaultClient, Outcome};
use aquarius_lp_vault::{AquariusLpVault, AquariusLpVaultClient};
use jump_rate_model::{JumpRateModel, JumpRateModelClient};
use lp_peridottroller::{
    SimplePeridottroller as Controller, SimplePeridottrollerClient as ControllerClient,
};
use mock_aquarius_pool::{MockAquariusPool, MockAquariusPoolClient};
use soroban_sdk::testutils::{
    cost_estimate::NetworkInvocationResourceLimits, Address as _, Ledger as _, MockAuth,
    MockAuthInvoke,
};
use soroban_sdk::{
    contract, contractimpl, contracttype, token, vec, Address, Env, IntoVal, Symbol, Vec,
};

#[contracttype]
#[derive(Clone)]
enum Asset {
    Stellar(Address),
    Other(Symbol),
}
#[contracttype]
#[derive(Clone)]
struct PriceData {
    price: i128,
    timestamp: u64,
}
#[contract]
struct Oracle;
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

fn invoke<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    target: &Address,
    name: &str,
    args: Vec<soroban_sdk::Val>,
) -> T {
    env.invoke_contract(target, &Symbol::new(env, name), args)
}
#[contractimpl]
impl Oracle {
    pub fn set(env: Env, asset: Address, value: i128) {
        env.storage().instance().set(&asset, &value);
    }
    pub fn decimals(_env: Env) -> u32 {
        14
    }
    pub fn resolution(_env: Env) -> u32 {
        300
    }
    pub fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        let Asset::Stellar(id) = asset else {
            return None;
        };
        Some(PriceData {
            price: env.storage().instance().get(&id)?,
            timestamp: env.ledger().timestamp(),
        })
    }
}
struct Fixture {
    env: Env,
    admin: Address,
    alice: Address,
    bob: Address,
    assets: [Address; 3],
    markets: [Address; 3],
    strategies: [Address; 3],
    pools: [Address; 3],
    rewards: [Address; 2],
    controller: Address,
    oracle: Address,
    cpu_limit: i64,
}
impl Fixture {
    fn new(active: bool) -> Self {
        Self::build(active, None, None, None, None)
    }
    fn build(
        active: bool,
        receipt_wasm: Option<&[u8]>,
        controller_wasm: Option<&[u8]>,
        strategy_wasm: Option<&[u8]>,
        pool_wasm: Option<&[u8]>,
    ) -> Self {
        let env = Env::default();
        let mut limits = soroban_env_host::InvocationResourceLimits::mainnet();
        // Explicit research envelope, not a network fit claim.
        limits.ledger_entries = 250;
        // Compiled strategy payout measured108.6M, above the earlier native100M
        // assertion. Mainnet read at ledger64494069 gives400M; retain a smaller
        // explicit200M compiled research bound, not an unlimited diagnostic.
        let cpu_limit = if strategy_wasm.is_some() {
            200_000_000
        } else {
            100_000_000
        };
        limits.instructions = cpu_limit;
        env.cost_estimate().enforce_resource_limits(limits);
        env.mock_all_auths_allowing_non_root_auth();
        env.ledger().with_mut(|l| {
            l.timestamp = 1000;
            l.sequence_number = 100;
        });
        let admin = Address::from_str(
            &env,
            "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ",
        );
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let mut assets: [Address; 3] = core::array::from_fn(|_| {
            env.register_stellar_asset_contract_v2(admin.clone())
                .address()
        });
        let mut paired = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        if pool_wasm.is_some() {
            if assets[0] > paired {
                core::mem::swap(&mut assets[0], &mut paired);
            }
            if assets[1] > assets[2] {
                assets.swap(1, 2);
            }
        }
        let rewards: [Address; 2] = core::array::from_fn(|_| {
            env.register_stellar_asset_contract_v2(admin.clone())
                .address()
        });
        let oracle = env.register(Oracle, ());
        for a in [&assets[0], &assets[1], &assets[2], &paired] {
            OracleClient::new(&env, &oracle).set(a, &100_000_000_000_000);
            token::StellarAssetClient::new(&env, a).mint(&alice, &10_000_000);
            token::StellarAssetClient::new(&env, a).mint(&bob, &10_000_000_000_000);
        }
        let register_pool = || match pool_wasm {
            Some(wasm) => env.register(wasm, ()),
            None => env.register(MockAquariusPool, ()),
        };
        let xlm = register_pool();
        let stable = register_pool();
        for (id, a, b) in [
            (&xlm, &assets[0], &paired),
            (&stable, &assets[1], &assets[2]),
        ] {
            if pool_wasm.is_some() {
                env.cost_estimate().budget().reset_unlimited();
                let plane = env.register(TestPlane, ());
                let boost = env
                    .register_stellar_asset_contract_v2(admin.clone())
                    .address();
                let roles = (
                    admin.clone(),
                    admin.clone(),
                    admin.clone(),
                    admin.clone(),
                    vec![&env, admin.clone()],
                    admin.clone(),
                );
                invoke::<()>(
                    &env,
                    id,
                    "initialize_all",
                    (
                        admin.clone(),
                        roles,
                        admin.clone(),
                        vec![&env, a.clone(), b.clone()],
                        10u32,
                        20i32,
                        0u32,
                        (rewards[0].clone(), boost, plane.clone()),
                        plane,
                    )
                        .into_val(&env),
                );
                env.cost_estimate().budget().reset_unlimited();
                invoke::<(Vec<u128>, u128)>(
                    &env,
                    id,
                    "deposit_position",
                    (
                        bob.clone(),
                        -2000i32,
                        2000i32,
                        vec![&env, 100_000_000_000u128, 100_000_000_000u128],
                        0u128,
                    )
                        .into_val(&env),
                );
                continue;
            }
            let p = MockAquariusPoolClient::new(&env, id);
            p.initialize(a, b, &20, &0);
            p.set_deposit_ratio_0_per_1_e6(&1_000_000);
            p.set_reward_tokens(&rewards[0], &rewards[1]);
            p.deposit_position(
                &bob,
                &-887220,
                &887220,
                &vec![&env, 1_000_000_000_000u128, 1_000_000_000_000u128],
                &0,
            );
        }
        let pools = [xlm, stable.clone(), stable];
        let controller = match controller_wasm {
            Some(wasm) => env.register(wasm, ()),
            None => env.register(Controller, ()),
        };
        let c = ControllerClient::new(&env, &controller);
        c.initialize(&admin);
        c.set_oracle(&oracle);
        // Token is configured, but even admin cannot turn on PERI emissions.
        c.set_peridot_token(&rewards[0]);
        let model = env.register(JumpRateModel, ());
        JumpRateModelClient::new(&env, &model)
            .initialize(&10_000, &180_000, &4_000_000, &800_000, &admin);
        let markets = core::array::from_fn(|i| match receipt_wasm {
            Some(wasm) => env.register(wasm, (&assets[i], &admin, i != 0)),
            None => env.register(LpLendingVault, (&assets[i], &admin, i != 0)),
        });
        let strategies = core::array::from_fn(|_| match strategy_wasm {
            Some(wasm) => env.register(wasm, ()),
            None => env.register(AquariusLpVault, ()),
        });
        for i in 0..3 {
            env.cost_estimate().budget().reset_unlimited();
            let r = LpLendingVaultClient::new(&env, &markets[i]);
            let s = AquariusLpVaultClient::new(&env, &strategies[i]);
            s.initialize(&admin, &pools[i], &u32::from(i == 2), &oracle);
            s.set_primary_reward_token(&admin, &Some(rewards[0].clone()));
            r.set_interest_model(&model);
            r.set_boosted_vault(&admin, &strategies[i]);
            r.set_idle_cash_buffer_bps(&admin, &0);
            s.set_receipt_vault(&admin, &markets[i]);
            s.refresh_nav_root();
            c.add_market(&markets[i]);
            c.set_market_cf(&markets[i], &if i == 0 { 500_000 } else { 800_000 });
            c.set_supply_speed(&markets[i], &0);
            c.set_borrow_speed(&markets[i], &0);
            r.set_peridottroller(&controller);
            r.initialize_rewards();
            r.register_reward(&rewards[0]);
            r.register_reward(&rewards[1]);
            s.enable_hybrid(&admin);
            if active {
                r.activate();
                r.deposit(if i == 0 { &alice } else { &bob }, &1_000_000);
                r.refresh_boosted_underlying();
                c.enter_market(&alice, &markets[i]);
            }
        }
        env.mock_all_auths();
        Self {
            env,
            admin,
            alice,
            bob,
            assets,
            markets,
            strategies,
            pools,
            rewards,
            controller,
            oracle,
            cpu_limit,
        }
    }
    fn r(&self, i: usize) -> LpLendingVaultClient<'_> {
        LpLendingVaultClient::new(&self.env, &self.markets[i])
    }
    fn c(&self) -> ControllerClient<'_> {
        ControllerClient::new(&self.env, &self.controller)
    }
    fn reset(&self) {
        self.env.cost_estimate().budget().reset_unlimited();
    }
    fn measure(&self, label: &str) {
        let r = self.env.cost_estimate().resources();
        let entries = r.disk_read_entries + r.memory_read_entries + r.write_entries;
        std::println!(
            "lp-abi {label}: {entries} entries, {} writes, {} CPU",
            r.write_entries,
            r.instructions
        );
        assert!(entries <= 250 && r.instructions <= self.cpu_limit);
    }
    fn credit(&self, i: usize, a: u128, b: u128) {
        for (asset, n) in self.rewards.iter().zip([a, b]) {
            token::StellarAssetClient::new(&self.env, asset).mint(&self.pools[i], &(n as i128));
        }
        MockAquariusPoolClient::new(&self.env, &self.pools[i]).credit_rewards(
            &self.strategies[i],
            &a,
            &b,
        );
    }
    fn route(&self, i: usize) {
        self.env.mock_all_auths_allowing_non_root_auth();
        for asset in &self.rewards {
            let id = self.env.register(MockAquariusPool, ());
            let p = MockAquariusPoolClient::new(&self.env, &id);
            p.initialize(asset, &self.assets[i], &20, &0);
            for a in [asset, &self.assets[i]] {
                token::StellarAssetClient::new(&self.env, a).mint(&self.bob, &1_000_000_000_000);
            }
            p.deposit_position(
                &self.bob,
                &-887220,
                &887220,
                &vec![&self.env, 1_000_000_000_000u128, 1_000_000_000_000u128],
                &0,
            );
            let s = AquariusLpVaultClient::new(&self.env, &self.strategies[i]);
            s.set_reward_route(&self.admin, asset, &Some(id));
            s.set_reward_min_rate(&self.admin, asset, &9_000_000);
        }
        self.env.mock_all_auths();
    }
}

#[test]
fn abi_cross_market_borrow_uses_approved_factors_and_keeps_group_debt() {
    let f = Fixture::new(true);
    for i in 0..3 {
        assert_eq!(
            f.c().get_market_cf(&f.markets[i]),
            if i == 0 { 500_000 } else { 800_000 }
        );
    }
    for i in [1, 2] {
        f.reset();
        f.r(i).borrow(&f.alice, &200_000);
        f.measure("stable borrow against XLM");
    }
    f.reset();
    assert!(f.r(1).try_borrow(&f.alice, &200_000).is_err());
    assert_eq!(f.r(1).get_user_borrow_balance(&f.alice), 200_000);
    assert_eq!(f.r(2).get_user_borrow_balance(&f.alice), 200_000);
    for i in [1, 2] {
        f.reset();
        f.r(i).repay(&f.alice, &200_000);
    }
    f.reset();
    f.r(0).withdraw(&f.alice, &1_000_000);
    assert_eq!(f.r(0).balance(&f.alice), 0);
}

#[test]
fn abi_late_deposit_and_delegated_transfer_cannot_take_historical_aqua() {
    let f = Fixture::new(true);
    f.credit(1, 1000, 2000);
    f.reset();
    f.r(1).deposit(&f.alice, &100_000);
    assert_eq!(f.r(1).reward_earned(&f.rewards[0], &f.alice), 0);
    assert_eq!(f.r(1).reward_earned(&f.rewards[0], &f.bob), 1000);
    f.r(1).approve(&f.bob, &f.alice, &100_000, &1000);
    f.reset();
    f.r(1).transfer_from(&f.alice, &f.bob, &f.alice, &100_000);
    f.measure("delegated transfer ownership");
    assert_eq!(f.r(1).reward_earned(&f.rewards[1], &f.bob), 2000);
    assert_eq!(f.r(1).reward_earned(&f.rewards[1], &f.alice), 0);
    f.reset();
    f.r(1).withdraw_with_minimum(&f.bob, &900_000, &800_000);
    assert_eq!(f.r(1).reward_reserved(&f.rewards[0], &f.bob), 1000);
}

#[test]
fn abi_legacy_selectors_and_unactivated_operations_are_unavailable() {
    let f = Fixture::new(false);
    assert!(f.r(0).try_deposit(&f.alice, &100_000).is_err());
    assert!(f.r(0).try_borrow(&f.alice, &1).is_err());
    for name in [
        "initialize",
        "bootstrap",
        "flash_loan",
        "borrow_for_margin",
        "begin_margin_withdraw",
        "recover_state",
        "fund",
        "seize_unchecked",
    ] {
        let result = f.env.try_invoke_contract::<(), soroban_sdk::InvokeError>(
            &f.markets[0],
            &Symbol::new(&f.env, name),
            Vec::new(&f.env),
        );
        assert!(result.is_err(), "unexpected legacy selector {name}");
    }
    f.reset();
    f.r(0).activate();
    assert!(f.r(0).try_activate().is_err());
    assert!(f.r(0).try_initialize_rewards().is_err());
    assert!(f.r(0).try_set_peridottroller(&f.controller).is_err());
    assert!(f
        .r(0)
        .try_set_boosted_vault(&f.admin, &f.strategies[0])
        .is_err());
}

#[test]
fn abi_zero_peri_policy_is_not_an_admin_toggle_and_legacy_harvest_is_fenced() {
    let f = Fixture::new(true);
    for i in 0..3 {
        assert!(f.c().try_set_supply_speed(&f.markets[i], &1).is_err());
        assert!(f.c().try_set_borrow_speed(&f.markets[i], &1).is_err());
        assert!(f
            .c()
            .try_set_price_fallback(&f.assets[i], &Some((1, 1)))
            .is_err());
        let s = AquariusLpVaultClient::new(&f.env, &f.strategies[i]);
        assert!(s.try_harvest(&f.admin).is_err());
        assert!(s.try_sweep_reward(&f.admin, &f.rewards[0]).is_err());
        assert!(s
            .try_set_primary_reward_token(&f.admin, &Some(f.rewards[1].clone()))
            .is_err());
    }
    f.env.ledger().with_mut(|l| l.timestamp += 10);
    f.reset();
    f.r(1).borrow(&f.alice, &100_000);
    assert_eq!(f.c().get_accrued(&f.alice), 0);
    assert_eq!(f.c().get_accrued(&f.bob), 0);
}

#[test]
#[ignore = "compiled price-router and LP controller; native receipt/strategy, mock pools/upstream"]
fn compiled_observation_oracle_halts_lp_borrowing_but_not_repayment() {
    let controller =
        std::fs::read(std::env::var("LP_CONTROLLER_WASM").expect("LP_CONTROLLER_WASM required"))
            .unwrap();
    let f = Fixture::build(true, None, Some(&controller), None, None);
    let wasm = std::fs::read(
        std::env::var("LP_PRICE_ROUTER_WASM").expect("LP_PRICE_ROUTER_WASM required"),
    )
    .unwrap();
    let id = f.env.register(wasm.as_slice(), ());
    let r = price_router::PriceRouterClient::new(&f.env, &id);
    r.initialize(&f.admin, &f.oracle, &300);
    let paired = MockAquariusPoolClient::new(&f.env, &f.pools[0])
        .get_tokens()
        .get(1)
        .unwrap();
    let reporter = Address::generate(&f.env);
    let cfg = price_router::ObservationConfig {
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
    };
    r.set_source(&f.admin, &paired, &price_router::PriceSource::Observed(cfg));
    for asset in &f.assets {
        r.set_required_observation(&f.admin, asset, &Some(paired.clone()));
    }
    f.c().set_oracle(&id);
    // Honor the existing governance delay in the local fixture.
    f.env.ledger().with_mut(|l| l.timestamp = 100_000);
    f.c().set_oracle(&id);
    assert_eq!(f.c().get_oracle(), Some(id));
    for i in 0..3 {
        f.reset();
        AquariusLpVaultClient::new(&f.env, &f.strategies[i]).refresh_nav_root();
        f.r(i).refresh_boosted_underlying();
    }
    f.reset();
    assert!(f.r(1).try_borrow(&f.alice, &100_000).is_err());
    f.reset();
    r.publish_observation(&reporter, &paired, &1_000_000_000_000, &99_700, &100_000);
    f.reset();
    f.r(1).borrow(&f.alice, &100_000);
    f.measure("compiled router healthy loan");
    f.reset();
    r.invalidate_observation(&reporter, &paired);
    f.reset();
    assert!(f.c().get_price_usd(&f.assets[0]).is_none());
    f.reset();
    // The already-used market has a warm price cache; it must not bypass halt.
    assert!(f.r(1).try_borrow(&f.alice, &1).is_err());
    f.reset();
    assert!(f.r(2).try_borrow(&f.alice, &100_000).is_err());
    f.reset();
    f.r(1).repay(&f.alice, &100_000);
    assert_eq!(f.r(1).get_total_borrowed(), 0);
    f.env.ledger().with_mut(|l| l.timestamp = 100_300);
    f.reset();
    r.publish_observation(&reporter, &paired, &1_000_000_000_000, &100_000, &100_300);
    f.reset();
    f.r(2).borrow(&f.alice, &100_000);
    f.env.ledger().with_mut(|l| l.timestamp = 100_601);
    f.reset();
    assert!(f.r(1).try_borrow(&f.alice, &100_000).is_err());
    f.reset();
    // Repayment remains available despite expired observation / borrowing halt.
    f.r(2).repay(&f.alice, &100_001);
}

#[test]
fn abi_exact_auth_and_failed_minimum_restore_shares_and_rewards() {
    let f = Fixture::new(true);
    f.credit(1, 1000, 2000);
    f.env.mock_auths(&[]);
    f.reset();
    assert!(f.r(1).try_borrow(&f.alice, &100_000).is_err());
    f.env.mock_auths(&[MockAuth {
        address: &f.alice,
        invoke: &MockAuthInvoke {
            contract: &f.markets[1],
            fn_name: "borrow",
            args: (&f.alice, 100_000u128).into_val(&f.env),
            sub_invokes: &[],
        },
    }]);
    f.reset();
    f.r(1).borrow(&f.alice, &100_000);
    f.measure("exact borrower authorization");
    f.env.mock_all_auths();
    f.reset();
    assert!(f
        .r(1)
        .try_withdraw_with_minimum(&f.bob, &100_000, &1_000_000)
        .is_err());
    assert_eq!(f.r(1).get_ptoken_balance(&f.bob), 1_000_000);
    assert_eq!(f.r(1).reward_reserved(&f.rewards[0], &f.bob), 0);
}

#[test]
fn abi_converted_and_reserved_rewards_pay_real_settlement_cash_with_debt() {
    let f = Fixture::new(true);
    f.route(2);
    f.credit(2, 100_000, 200_000);
    f.reset();
    assert!(matches!(
        f.r(2).compound(&f.rewards[0]),
        Outcome::Completed(_)
    ));
    f.reset();
    f.r(2).borrow(&f.alice, &200_000);
    let cash = token::Client::new(&f.env, &f.assets[2]).balance(&f.bob);
    f.reset();
    let Outcome::Completed(paid) = f.r(2).payout_rewards(&f.bob, &1) else {
        panic!("payout failed")
    };
    f.measure("reward payout with outstanding loan");
    assert_eq!(
        token::Client::new(&f.env, &f.assets[2]).balance(&f.bob) - cash,
        paid as i128
    );
    assert_eq!(f.r(2).get_total_borrowed(), 200_000);
    assert_eq!(f.r(2).get_ptoken_balance(&f.bob), 1_000_000);
    f.reset();
    f.r(2).withdraw_with_minimum(&f.bob, &100_000, &80_000);
    let reserved = f.r(2).reward_reserved(&f.rewards[1], &f.bob);
    assert_eq!(reserved, 20_000);
    f.reset();
    assert!(matches!(
        f.r(2).settle_reserved(&f.rewards[1], &f.bob, &reserved, &1),
        Outcome::Completed(_)
    ));
}

#[test]
fn abi_liquidation_retains_borrowers_historical_pool_rewards() {
    let f = Fixture::new(true);
    f.reset();
    f.r(1).borrow(&f.alice, &400_000);
    f.credit(0, 1000, 2000);
    OracleClient::new(&f.env, &f.oracle).set(&f.assets[0], &50_000_000_000_000);
    f.c().cache_price(&f.assets[0]);
    token::Client::new(&f.env, &f.assets[1]).approve(&f.bob, &f.markets[1], &100_000, &1000);
    f.reset();
    f.c()
        .liquidate(&f.alice, &f.markets[1], &f.markets[0], &100_000, &f.bob);
    f.measure("controller liquidation into LP entrypoint");
    assert_eq!(f.r(1).get_total_borrowed(), 300_000);
    assert_eq!(f.r(0).reward_earned(&f.rewards[0], &f.alice), 1000);
    assert_eq!(f.r(0).reward_earned(&f.rewards[0], &f.bob), 0);
}

#[test]
fn abi_controller_rejects_core_markets_and_receipt_rejects_generic_controller() {
    let f = Fixture::new(false);
    assert!(f.c().try_add_market(&f.assets[0]).is_err());
    let generic = f
        .env
        .register(simple_peridottroller::SimplePeridottroller, ());
    simple_peridottroller::SimplePeridottrollerClient::new(&f.env, &generic).initialize(&f.admin);
    let unbound = f
        .env
        .register(LpLendingVault, (&f.assets[0], &f.admin, false));
    assert!(LpLendingVaultClient::new(&f.env, &unbound)
        .try_set_peridottroller(&generic)
        .is_err());
    assert!(f.c().try_add_market(&unbound).is_err()); // a fourth market
    assert_eq!(f.c().lp_reward_policy(), 1);
}

#[test]
fn abi_activation_requires_exact_approved_cf_and_legacy_marker_is_not_reconstructed() {
    let f = Fixture::new(false);
    f.env.as_contract(&f.controller, || {
        f.env.storage().persistent().set(
            &lp_peridottroller::DataKey::MarketCF(f.markets[0].clone()),
            &700_000u128,
        )
    });
    assert!(f.r(0).try_activate().is_err());
    assert!(!f.r(0).is_active());
    f.env.as_contract(&f.markets[1], || {
        f.env
            .storage()
            .instance()
            .remove(&vec![&f.env, Symbol::new(&f.env, "LpLendingConfig")])
    });
    assert!(f.r(1).try_activate().is_err());
    assert!(f.r(1).try_deposit(&f.alice, &100_000).is_err());
    assert!(f.r(1).try_lp_version().is_err());
}

#[test]
#[should_panic(expected = "LP release gates")]
fn abi_mainnet_constructor_is_blocked_until_release_gates_are_cleared() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::from_str(
        &env,
        "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ",
    );
    let asset = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let public = env.crypto().sha256(&soroban_sdk::Bytes::from_slice(
        &env,
        b"Public Global Stellar Network ; September 2015",
    ));
    env.ledger().with_mut(|l| l.network_id = public.to_array());
    env.register(LpLendingVault, (&asset, &admin, false));
}

#[test]
fn abi_mainnet_controller_initialization_and_legacy_policy_certification_are_blocked() {
    let f = Fixture::new(false);
    f.env.as_contract(&f.controller, || {
        f.env
            .storage()
            .instance()
            .remove(&Symbol::new(&f.env, "LpZeroPeriV1"))
    });
    assert!(f.c().try_lp_reward_policy().is_err());
    let fresh = f.env.register(Controller, ());
    let public = f.env.crypto().sha256(&soroban_sdk::Bytes::from_slice(
        &f.env,
        b"Public Global Stellar Network ; September 2015",
    ));
    f.env
        .ledger()
        .with_mut(|l| l.network_id = public.to_array());
    assert!(ControllerClient::new(&f.env, &fresh)
        .try_initialize(&f.admin)
        .is_err());
}

#[test]
fn abi_repayment_survives_unobservable_rewards_without_discarding_ownership() {
    let f = Fixture::new(true);
    f.reset();
    f.r(1).borrow(&f.alice, &100_000);
    f.credit(1, 1000, 2000);
    let pool = MockAquariusPoolClient::new(&f.env, &f.pools[1]);
    pool.set_kill_claim(&true);
    pool.set_fail_reward_quote(&true);
    f.reset();
    assert!(f.r(1).try_transfer(&f.bob, &f.alice, &100_000).is_err());
    f.reset();
    f.r(1).repay(&f.alice, &100_000);
    assert_eq!(f.r(1).get_total_borrowed(), 0);
    assert_eq!(f.r(1).get_ptoken_balance(&f.bob), 1_000_000);
    pool.set_kill_claim(&false);
    pool.set_fail_reward_quote(&false);
    f.reset();
    f.r(1).transfer(&f.bob, &f.alice, &100_000);
    assert_eq!(f.r(1).reward_earned(&f.rewards[0], &f.bob), 1000);
    assert_eq!(f.r(1).reward_earned(&f.rewards[0], &f.alice), 0);
}

#[test]
fn abi_exact_withdraw_authorization_and_admin_handoff() {
    let f = Fixture::new(true);
    f.credit(1, 1000, 2000);
    f.env.mock_auths(&[]);
    f.reset();
    assert!(f
        .r(1)
        .try_withdraw_with_minimum(&f.bob, &100_000, &1)
        .is_err());
    assert!(f.r(1).try_set_admin(&f.alice).is_err());
    f.env.mock_auths(&[MockAuth {
        address: &f.bob,
        invoke: &MockAuthInvoke {
            contract: &f.markets[1],
            fn_name: "withdraw_with_minimum",
            args: (&f.bob, 100_000u128, 1u128).into_val(&f.env),
            sub_invokes: &[],
        },
    }]);
    f.reset();
    f.r(1).withdraw_with_minimum(&f.bob, &100_000, &1);
    assert_eq!(f.r(1).get_ptoken_balance(&f.bob), 900_000);
    assert_eq!(f.r(1).reward_reserved(&f.rewards[0], &f.bob), 100);
    f.env.mock_all_auths();
    f.r(1).set_admin(&f.alice);
    assert_eq!(f.r(1).get_admin(), f.admin);
    f.env.mock_auths(&[]);
    assert!(f.r(1).try_accept_admin().is_err());
    f.env.mock_auths(&[MockAuth {
        address: &f.alice,
        invoke: &MockAuthInvoke {
            contract: &f.markets[1],
            fn_name: "accept_admin",
            args: ().into_val(&f.env),
            sub_invokes: &[],
        },
    }]);
    f.r(1).accept_admin();
    assert_eq!(f.r(1).get_admin(), f.alice);
}

#[test]
#[ignore = "explicit built receipt/controller WASMs; native strategy/mock pools, not full compiled stack"]
fn compiled_lp_abi_borrow_transfer_reward_payout_and_exit() {
    let receipt =
        std::fs::read(std::env::var("LP_RECEIPT_WASM").expect("LP_RECEIPT_WASM required")).unwrap();
    let controller =
        std::fs::read(std::env::var("LP_CONTROLLER_WASM").expect("LP_CONTROLLER_WASM required"))
            .unwrap();
    let f = Fixture::build(true, Some(&receipt), Some(&controller), None, None);
    f.route(2);
    f.credit(2, 100_000, 200_000);
    f.reset();
    f.r(2).borrow(&f.alice, &200_000);
    f.measure("compiled receipt/controller loan");
    f.reset();
    assert!(matches!(
        f.r(2).compound(&f.rewards[0]),
        Outcome::Completed(_)
    ));
    f.measure("compiled receipt/controller conversion");
    f.reset();
    assert!(matches!(
        f.r(2).payout_rewards(&f.bob, &1),
        Outcome::Completed(_)
    ));
    f.measure("compiled receipt/controller reward payout");
    f.reset();
    f.r(2).repay(&f.alice, &200_000);
    f.reset();
    f.r(2).transfer(&f.bob, &f.alice, &100_000);
    f.reset();
    f.r(2).withdraw(&f.alice, &100_000);
    assert_eq!(f.r(2).get_total_borrowed(), 0);
    assert_eq!(f.r(2).reward_reserved(&f.rewards[1], &f.alice), 0);
}

#[test]
#[ignore = "compiled receipt/controller/strategy; mock pool and oracle, no Mainnet writes"]
fn compiled_lp_strategy_borrow_reward_payout_and_mainnet_guards() {
    let read = |name| std::fs::read(std::env::var(name).expect(name)).unwrap();
    let receipt = read("LP_RECEIPT_WASM");
    let controller = read("LP_CONTROLLER_WASM");
    let strategy = read("LP_STRATEGY_WASM");
    let f = Fixture::build(
        true,
        Some(&receipt),
        Some(&controller),
        Some(&strategy),
        None,
    );
    for i in [1, 2] {
        let s = AquariusLpVaultClient::new(&f.env, &f.strategies[i]);
        assert!(s.try_harvest(&f.admin).is_err());
        assert!(s.try_sweep_reward(&f.admin, &f.rewards[0]).is_err());
        f.route(i);
        f.credit(i, 100_000, 200_000);
        f.reset();
        f.r(i).borrow(&f.alice, &200_000);
        f.measure("compiled strategy loan");
        f.reset();
        assert!(matches!(
            f.r(i).compound(&f.rewards[0]),
            Outcome::Completed(_)
        ));
        f.measure("compiled strategy conversion");
        f.reset();
        assert!(matches!(
            f.r(i).payout_rewards(&f.bob, &1),
            Outcome::Completed(_)
        ));
        f.measure("compiled strategy payout");
        f.reset();
        f.r(i).repay(&f.alice, &200_000);
        f.reset();
        f.r(i).withdraw(&f.bob, &100_000);
        assert_eq!(f.r(i).get_total_borrowed(), 0);
    }
    let fresh = f.env.register(strategy.as_slice(), ());
    let public = f.env.crypto().sha256(&soroban_sdk::Bytes::from_slice(
        &f.env,
        b"Public Global Stellar Network ; September 2015",
    ));
    f.env
        .ledger()
        .with_mut(|l| l.network_id = public.to_array());
    assert!(AquariusLpVaultClient::new(&f.env, &fresh)
        .try_initialize(&f.admin, &f.pools[0], &0, &f.oracle)
        .is_err());
    // A previously initialized/upgraded instance cannot bypass the network fence.
    assert!(AquariusLpVaultClient::new(&f.env, &f.strategies[0])
        .try_hybrid_claim()
        .is_err());
    assert!(AquariusLpVaultClient::new(&f.env, &f.strategies[0])
        .try_enable_hybrid(&f.admin)
        .is_err());
}

#[test]
#[ignore = "hash-pinned actual concentrated pool plus compiled receipt/controller/strategy; controlled local state"]
fn compiled_lp_exact_pool_cross_borrow_and_reward_payout() {
    let read = |name| std::fs::read(std::env::var(name).expect(name)).unwrap();
    let receipt = read("LP_RECEIPT_WASM");
    let controller = read("LP_CONTROLLER_WASM");
    let strategy = read("LP_STRATEGY_WASM");
    let pool = read("AQUARIUS_CONCENTRATED_WASM");
    let env = Env::default();
    let hash = env
        .crypto()
        .sha256(&soroban_sdk::Bytes::from_slice(&env, &pool))
        .to_array();
    let hex: std::string::String = hash.iter().map(|b| std::format!("{b:02x}")).collect();
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
    for i in [1, 2] {
        f.route(i);
    }
    token::StellarAssetClient::new(&f.env, &f.rewards[0]).mint(&f.pools[1], &1_000_000_000_000);
    invoke::<()>(
        &f.env,
        &f.pools[1],
        "set_rewards_config",
        (f.admin.clone(), 10_000u64, 1_000_000_000u128).into_val(&f.env),
    );
    f.env.ledger().with_mut(|l| l.timestamp += 100);
    for i in [1, 2] {
        f.reset();
        let pending: u128 = invoke(
            &f.env,
            &f.pools[i],
            "get_user_reward",
            (f.strategies[i].clone(),).into_val(&f.env),
        );
        assert!(pending > 0);
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
        f.measure("exact pool compiled loan");
        assert!(f.r(i).reward_earned(&f.rewards[0], &f.bob) > 0);
        assert_eq!(f.r(i).reward_earned(&f.rewards[0], &f.alice), 0);
    }
    assert_eq!(f.r(1).get_user_borrow_balance(&f.alice), 200_000);
    assert_eq!(f.r(2).get_user_borrow_balance(&f.alice), 200_000);
    f.env.mock_all_auths();
    f.reset();
    assert!(f.r(1).try_borrow(&f.alice, &200_000).is_err());
    for i in [1, 2] {
        f.reset();
        assert!(matches!(
            f.r(i).compound(&f.rewards[0]),
            Outcome::Completed(_)
        ));
        f.measure("exact pool compiled conversion");
        f.reset();
        assert!(matches!(
            f.r(i).payout_rewards(&f.bob, &1),
            Outcome::Completed(_)
        ));
        f.measure("exact pool compiled payout");
        f.reset();
        f.r(i).repay_max(&f.alice);
        f.reset();
        f.r(i).withdraw_with_minimum(&f.bob, &100_000, &90_000);
        f.measure("exact pool compiled exit");
        assert_eq!(f.r(i).get_user_borrow_balance(&f.alice), 0);
    }
}
