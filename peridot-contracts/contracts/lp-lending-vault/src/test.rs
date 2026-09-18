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
}
impl Fixture {
    fn new(active: bool) -> Self {
        Self::build(active, None, None)
    }
    fn build(active: bool, receipt_wasm: Option<&[u8]>, controller_wasm: Option<&[u8]>) -> Self {
        let env = Env::default();
        let mut limits = soroban_env_host::InvocationResourceLimits::mainnet();
        limits.ledger_entries = 250; // Explicit research envelope, not a network fit claim.
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
        let assets: [Address; 3] = core::array::from_fn(|_| {
            env.register_stellar_asset_contract_v2(admin.clone())
                .address()
        });
        let paired = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
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
        let xlm = env.register(MockAquariusPool, ());
        let stable = env.register(MockAquariusPool, ());
        for (id, a, b) in [
            (&xlm, &assets[0], &paired),
            (&stable, &assets[1], &assets[2]),
        ] {
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
        let strategies = core::array::from_fn(|_| env.register(AquariusLpVault, ()));
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
        assert!(entries <= 250 && r.instructions <= 100_000_000);
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
        let s = AquariusLpVaultClient::new(&f.env, &f.strategies[i]);
        assert!(s.try_harvest(&f.admin).is_err());
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
    let f = Fixture::build(true, Some(&receipt), Some(&controller));
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
