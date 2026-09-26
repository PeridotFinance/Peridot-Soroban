//! Real-token bridge integration. Legacy admin stand-ins and the old single-stream
//! harness remain isolated tests. The lean feature also exercises the all-stream
//! coordinator; none of these test wrapper entrypoints is a production ABI.
extern crate std;
use crate::test::{MockOracle, MockOracleClient};
use crate::{AquariusLpVault, AquariusLpVaultClient};
#[cfg(feature = "lp-receipt-prototype")]
use lp_receipt_vault as receipt_core;
use mock_aquarius_pool::{MockAquariusPool, MockAquariusPoolClient};
#[cfg(feature = "lp-receipt-prototype")]
use receipt_core::reward_coordinator as coordinator;
use receipt_core::{
    reward_backing as backing, reward_ledger as ledger, DataKey as ReceiptKey, ReceiptVault,
};
#[cfg(not(feature = "lp-receipt-prototype"))]
use receipt_vault as receipt_core;
use soroban_sdk::testutils::{storage::Persistent as _, Ledger as _};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{contract, contractimpl, token, Address, Env, IntoVal, Map, Symbol, Vec};

#[contract]
struct HybridReceipt;

fn admin(env: &Env) {
    let admin: Address = env.storage().persistent().get(&ReceiptKey::Admin).unwrap();
    admin.require_auth();
}
fn strategy(env: &Env) -> Address {
    ReceiptVault::get_boosted_vault(env.clone()).unwrap()
}
fn weight(env: &Env, owner: &Address) -> u128 {
    ReceiptVault::balance(env.clone(), owner.clone())
        .try_into()
        .unwrap()
}
// Single-stream native coordinator. Other streams and rewards earned by escrow
// backing are explicitly blocked until their recycled-yield coordinator exists.
fn owned_claim(env: &Env, asset: &Address) {
    let s = ledger::stream(env, asset);
    let b = backing::state(env);
    assert_eq!(
        s.total_weight,
        ReceiptVault::get_total_ptokens(env.clone()) - b.ptokens,
        "uncheckpointed receipt mutation"
    );
    let snapshot = ledger::begin_receive(env, asset);
    let received: Map<Address, u128> = env.invoke_contract(
        &strategy(env),
        &Symbol::new(env, "hybrid_claim"),
        Vec::new(env),
    );
    for (token, _) in received.iter() {
        assert_eq!(token, *asset, "multi-stream coordinator required");
    }
    let amount = received.get(asset.clone()).unwrap_or(0);
    assert!(
        amount == 0 || b.ptokens == 0,
        "recycled backing coordinator required"
    );
    ledger::finish_receive(env, snapshot, amount);
}
fn swap(env: &Env, reward: &Address, amount: u128, minimum: u128) -> Result<u128, ()> {
    let strategy = strategy(env);
    // Grandchild transfer needs explicit receipt authorization; direct receipt
    // -> strategy authorization is provided by the invoker automatically.
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        crate::contract::auth_entry(
            env,
            reward,
            "transfer",
            (
                env.current_contract_address(),
                strategy.clone(),
                amount as i128
            )
                .into_val(env),
            Vec::new(env)
        )
    ]);
    match env.try_invoke_contract::<u128, soroban_sdk::InvokeError>(
        &strategy,
        &Symbol::new(env, "hybrid_swap"),
        (reward.clone(), amount, minimum).into_val(env),
    ) {
        Ok(Ok(output)) => Ok(output),
        _ => Err(()),
    }
}

#[cfg(feature = "lp-receipt-prototype")]
#[contractimpl]
impl HybridReceipt {
    pub fn co_prepare_rotate(env: Env, minimum: u128) -> u128 {
        coordinator::prepare_rotation(&env, minimum)
    }
    pub fn co_rotate(env: Env, next: Address, minimum: u128) -> u128 {
        coordinator::rotate_primary(&env, &next, minimum)
    }
    pub fn co_request(env: Env, owner: Address, shares: u128, minimum: u128) -> u64 {
        receipt_core::exit_request::request(&env, &owner, shares, minimum)
    }
    pub fn co_request_get(
        env: Env,
        owner: Address,
    ) -> Option<receipt_core::exit_request::ExitRecord> {
        receipt_core::exit_request::get(&env, &owner)
    }
    pub fn co_cancel(env: Env, owner: Address, nonce: u64) {
        receipt_core::exit_request::cancel(&env, &owner, nonce);
    }
    pub fn co_execute(env: Env, owner: Address, nonce: u64) -> (Map<Address, u128>, u128) {
        receipt_core::exit_request::execute(&env, &owner, nonce)
    }
    pub fn co_register(env: Env, asset: Address) {
        coordinator::register(&env, &asset);
    }
    pub fn co_claim(env: Env) {
        coordinator::claim(&env);
    }
    pub fn co_compound(env: Env, asset: Address) -> coordinator::Outcome {
        coordinator::compound(&env, &asset)
    }
    pub fn co_recycle(env: Env, asset: Address) -> coordinator::Outcome {
        coordinator::recycle(&env, &asset)
    }
    pub fn co_deposit(env: Env, owner: Address, amount: u128) {
        coordinator::deposit(&env, &owner, amount);
    }
    pub fn co_transfer(env: Env, from: Address, to: Address, amount: i128) {
        coordinator::transfer(&env, &from, &to, amount);
    }
    pub fn co_withdraw(env: Env, owner: Address, shares: u128) -> Map<Address, u128> {
        coordinator::withdraw(&env, &owner, shares)
    }
    pub fn co_exit(
        env: Env,
        owner: Address,
        shares: u128,
        minimum: u128,
    ) -> (Map<Address, u128>, u128) {
        coordinator::withdraw_proportional(&env, &owner, shares, minimum)
    }
    pub fn co_redeem(env: Env, owner: Address, minimum: u128) -> coordinator::Outcome {
        coordinator::redeem(&env, &owner, minimum)
    }
    pub fn co_settle(
        env: Env,
        asset: Address,
        owner: Address,
        raw: u128,
        minimum: u128,
    ) -> coordinator::Outcome {
        coordinator::settle_reserved(&env, &asset, &owner, raw, minimum)
    }
}

#[contractimpl]
impl HybridReceipt {
    pub fn initialize(env: Env, asset: Address, admin: Address) {
        ReceiptVault::initialize(env, asset, 0, 0, admin);
    }
    pub fn static_rates(env: Env, admin: Address) {
        ReceiptVault::enable_static_rates(env, admin);
    }
    pub fn init_backing(env: Env) {
        backing::initialize(&env);
    }
    pub fn bind(env: Env, admin: Address, strategy: Address) {
        ReceiptVault::set_boosted_vault(env, admin, strategy);
    }
    pub fn set_buffer(env: Env, admin: Address, bps: u32) {
        ReceiptVault::set_idle_cash_buffer_bps(env, admin, bps);
    }
    pub fn get_boosted_vault(env: Env) -> Option<Address> {
        ReceiptVault::get_boosted_vault(env)
    }
    pub fn get_underlying_token(env: Env) -> Address {
        ReceiptVault::get_underlying_token(env)
    }
    pub fn balance(env: Env, owner: Address) -> i128 {
        ReceiptVault::balance(env, owner)
    }
    pub fn deposit(env: Env, user: Address, amount: u128) {
        ReceiptVault::deposit(env, user, amount);
    }
    pub fn claim(env: Env) -> Map<Address, u128> {
        admin(&env);
        env.invoke_contract(
            &strategy(&env),
            &Symbol::new(&env, "hybrid_claim"),
            Vec::new(&env),
        )
    }
    pub fn convert(env: Env, reward: Address, amount: u128, minimum: u128) -> u128 {
        admin(&env);
        swap(&env, &reward, amount, minimum).unwrap_or(0)
    }
    pub fn compound(env: Env, reward: Address, amount: u128) -> u128 {
        admin(&env);
        let snapshot = backing::begin(&env);
        let Ok(output) = swap(&env, &reward, amount, 1) else {
            return 0;
        };
        backing::finish_idle(&env, snapshot, output, 0, 1)
    }
    pub fn reinvest(env: Env, admin: Address) {
        ReceiptVault::rebalance_idle_cash(env, admin);
    }
    pub fn allocate(env: Env, user: Address, units: u128) {
        admin(&env);
        backing::allocate(&env, &user, units);
    }
    pub fn redeem(env: Env, user: Address, units: u128) -> u128 {
        backing::redeem(&env, &user, units, 1)
    }
    pub fn start_ledger(env: Env, reward: Address) {
        admin(&env);
        ledger::register(&env, &reward, ReceiptVault::get_total_ptokens(env.clone()));
    }
    // Minimal measured-token adapter for testing storage across many epochs.
    // It deliberately does not model pool emissions on existing escrow backing.
    pub fn test_receive(env: Env, reward: Address, source: Address, raw: u128) {
        source.require_auth();
        let snapshot = ledger::begin_receive(&env, &reward);
        token::Client::new(&env, &reward).transfer(
            &source,
            &env.current_contract_address(),
            &(raw as i128),
        );
        ledger::finish_receive(&env, snapshot, raw);
    }
    pub fn test_convert(env: Env, reward: Address, source: Address, output: u128) -> u128 {
        let snapshot = ledger::begin_conversion(&env, &reward);
        token::Client::new(&env, &reward).transfer(
            &env.current_contract_address(),
            &source,
            &(snapshot.raw_amount() as i128),
        );
        let units = backing::fund(&env, &source, output, 0, 1);
        ledger::finish_conversion(&env, snapshot, units);
        units
    }
    pub fn owned_deposit(env: Env, reward: Address, user: Address, amount: u128) {
        owned_claim(&env, &reward);
        let before = weight(&env, &user);
        ledger::checkpoint(&env, &reward, &user, before);
        ReceiptVault::deposit(env.clone(), user.clone(), amount);
        ledger::change_weight(&env, &reward, &user, before, weight(&env, &user));
    }
    pub fn owned_transfer(env: Env, reward: Address, from: Address, to: Address, amount: i128) {
        assert_ne!(
            to,
            env.current_contract_address(),
            "cannot transfer into escrow"
        );
        owned_claim(&env, &reward);
        let from_before = weight(&env, &from);
        let to_before = weight(&env, &to);
        ledger::checkpoint(&env, &reward, &from, from_before);
        ledger::checkpoint(&env, &reward, &to, to_before);
        ReceiptVault::transfer(env.clone(), from.clone(), to.clone().into(), amount);
        ledger::change_weight(&env, &reward, &from, from_before, weight(&env, &from));
        if from != to {
            ledger::change_weight(&env, &reward, &to, to_before, weight(&env, &to));
        }
    }
    pub fn owned_withdraw(env: Env, reward: Address, owner: Address, shares: u128) -> u128 {
        owned_claim(&env, &reward);
        let before = weight(&env, &owner);
        let reserved = ledger::reserve(&env, &reward, &owner, before, shares);
        ReceiptVault::withdraw(env.clone(), owner.clone(), shares);
        ledger::change_weight(&env, &reward, &owner, before, weight(&env, &owner));
        reserved
    }
    pub fn owned_compound(env: Env, reward: Address) -> u128 {
        // Permissionless, but cannot choose the amount or an arbitrary recipient.
        owned_claim(&env, &reward);
        let conversion = ledger::begin_conversion(&env, &reward);
        let snapshot = backing::begin(&env);
        let Ok(output) = swap(&env, &reward, conversion.raw_amount(), 1) else {
            return 0;
        };
        let units = backing::finish_idle(&env, snapshot, output, 0, 1);
        ledger::finish_conversion(&env, conversion, units);
        units
    }
    pub fn earned(env: Env, reward: Address, owner: Address) -> ledger::Account {
        ledger::checkpoint(&env, &reward, &owner, weight(&env, &owner))
    }
    pub fn owned_redeem(env: Env, reward: Address, owner: Address) -> u128 {
        // The reused withdrawal requires owner auth once in this same frame.
        owned_claim(&env, &reward);
        let units = ledger::allocate(&env, &reward, &owner, weight(&env, &owner));
        backing::redeem(&env, &owner, units, 1)
    }
    pub fn owned_settle(
        env: Env,
        reward: Address,
        owner: Address,
        raw: u128,
        minimum: u128,
    ) -> u128 {
        owner.require_auth();
        let snapshot = ledger::begin_reserved(&env, &reward, &owner, weight(&env, &owner), raw);
        let Ok(output) = swap(&env, &reward, raw, minimum) else {
            return 0;
        };
        let asset = ReceiptVault::get_underlying_token(env.clone());
        let token = token::Client::new(&env, &asset);
        let before = token.balance(&owner);
        token.transfer(&env.current_contract_address(), &owner, &(output as i128));
        assert_eq!(token.balance(&owner) - before, output as i128);
        assert!(output >= minimum);
        ledger::finish_reserved(&env, snapshot);
        output
    }
}

#[cfg(feature = "lp-receipt-prototype")]
#[path = "deployed_pool_test.rs"]
mod deployed_pool_test;
#[cfg(feature = "lp-receipt-prototype")]
#[path = "reward_coordinator_test.rs"]
mod reward_coordinator_test;

struct Fixture {
    env: Env,
    admin: Address,
    receipt_admin: Address,
    user: Address,
    receipt: Address,
    strategy: Address,
    pool: Address,
    route: Address,
    asset: Address,
    paired: Address,
    #[cfg(feature = "lp-receipt-prototype")]
    oracle: Address,
    reward: Address,
}
impl Fixture {
    fn new() -> Self {
        Self::with_buffer(0)
    }
    fn with_buffer(buffer: u32) -> Self {
        Self::with_settlement_index(buffer, 0)
    }
    fn with_settlement_index(buffer: u32, underlying_index: u32) -> Self {
        let env = Env::default();
        // Liquidity bootstrap signs nested token transfers without pool auth.
        env.mock_all_auths_allowing_non_root_auth();
        let admin = Address::from_str(&env, crate::contract::DEFAULT_INIT_ADMIN);
        let receipt_admin = Address::generate(&env);
        let user = Address::generate(&env);
        let asset = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let paired = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let pool = env.register(MockAquariusPool, ());
        let p = MockAquariusPoolClient::new(&env, &pool);
        let (token0, token1) = if underlying_index == 0 {
            (&asset, &paired)
        } else {
            (&paired, &asset)
        };
        p.initialize(token0, token1, &60, &0);
        // Stable, idempotent parity range quote; reserve-ratio requoting in the
        // mock otherwise floors twice and mismatches exact token authorization.
        p.set_deposit_ratio_0_per_1_e6(&1_000_000);
        p.set_reward_tokens(&reward, &reward);
        let route = env.register(MockAquariusPool, ());
        MockAquariusPoolClient::new(&env, &route).initialize(&reward, &asset, &60, &0);
        for (pool, a, b) in [(&pool, &asset, &paired), (&route, &reward, &asset)] {
            token::StellarAssetClient::new(&env, a).mint(&user, &1_000_000_000_000);
            token::StellarAssetClient::new(&env, b).mint(&user, &1_000_000_000_000);
            MockAquariusPoolClient::new(&env, pool).deposit_position(
                &user,
                &-887220,
                &887220,
                &soroban_sdk::vec![&env, 1_000_000_000_000u128, 1_000_000_000_000u128],
                &0,
            );
        }
        let oracle = env.register(MockOracle, ());
        MockOracleClient::new(&env, &oracle).set_price(&asset, &100_000_000_000_000);
        MockOracleClient::new(&env, &oracle).set_price(&paired, &100_000_000_000_000);
        let strategy = env.register(AquariusLpVault, ());
        let s = AquariusLpVaultClient::new(&env, &strategy);
        s.initialize(&admin, &pool, &underlying_index, &oracle);
        s.set_primary_reward_token(&admin, &Some(reward.clone()));
        s.set_reward_route(&admin, &reward, &Some(route.clone()));
        s.set_reward_min_rate(&admin, &reward, &9_000_000);
        let receipt = env.register(HybridReceipt, ());
        let r = HybridReceiptClient::new(&env, &receipt);
        r.initialize(&asset, &receipt_admin);
        r.static_rates(&receipt_admin);
        r.init_backing();
        r.bind(&receipt_admin, &strategy);
        r.set_buffer(&receipt_admin, &buffer);
        s.set_receipt_vault(&admin, &receipt);
        s.refresh_nav_root();
        token::StellarAssetClient::new(&env, &asset).mint(&user, &10_000_000);
        r.deposit(&user, &1_000_000);
        env.mock_all_auths();
        Self {
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
            #[cfg(feature = "lp-receipt-prototype")]
            oracle,
            reward,
        }
    }
    fn receipt(&self) -> HybridReceiptClient<'_> {
        HybridReceiptClient::new(&self.env, &self.receipt)
    }
    fn credit(&self, primary: u128, gauge: u128) {
        token::StellarAssetClient::new(&self.env, &self.reward)
            .mint(&self.pool, &((primary + gauge) as i128));
        MockAquariusPoolClient::new(&self.env, &self.pool).credit_rewards(
            &self.strategy,
            &primary,
            &gauge,
        );
    }
    fn raw(&self, owner: &Address) -> i128 {
        token::Client::new(&self.env, &self.reward).balance(owner)
    }
    fn cash(&self, owner: &Address) -> i128 {
        token::Client::new(&self.env, &self.asset).balance(owner)
    }
}

#[test]
fn claims_use_actual_tokens_and_deduplicate_primary_plus_gauge() {
    let f = Fixture::new();
    f.credit(100_000, 40_000);
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.strategy, &7_000);
    let claimed = f.receipt().claim();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed.get(f.reward.clone()), Some(140_000));
    assert_eq!(f.raw(&f.receipt), 140_000);
    assert_eq!(f.raw(&f.strategy), 7_000); // old inventory is not silently reassigned
    assert!(f.receipt().claim().is_empty());
}

#[test]
fn exact_partial_swap_leaves_other_rewards_and_principal_untouched() {
    let f = Fixture::new();
    f.credit(100_000, 0);
    f.receipt().claim();
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.strategy, &30_000);
    let principal_before = f.cash(&f.strategy);
    let output = f.receipt().convert(&f.reward, &40_000, &35_000);
    assert!(output >= 35_000);
    assert_eq!(f.raw(&f.receipt), 60_000);
    assert_eq!(f.raw(&f.strategy), 30_000);
    assert_eq!(f.cash(&f.strategy), principal_before);
    assert_eq!(f.cash(&f.receipt), output as i128);
}

#[test]
fn replayed_input_transfer_rolls_back_even_with_legacy_inventory() {
    let f = Fixture::new();
    f.credit(100_000, 0);
    f.receipt().claim();
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.strategy, &100_000);
    MockAquariusPoolClient::new(&f.env, &f.route).set_replay_swap_transfer(&true);
    let route_before = f.raw(&f.route);
    assert_eq!(f.receipt().convert(&f.reward, &40_000, &1), 0);
    assert_eq!(f.raw(&f.receipt), 100_000);
    assert_eq!(f.raw(&f.strategy), 100_000);
    assert_eq!(f.raw(&f.route), route_before);
    assert_eq!(f.cash(&f.receipt), 0);
}

#[test]
fn underpaying_route_rolls_back_input_and_output() {
    let f = Fixture::new();
    f.credit(100_000, 0);
    f.receipt().claim();
    MockAquariusPoolClient::new(&f.env, &f.route).set_swap_output_bps(&5_000);
    assert_eq!(f.receipt().convert(&f.reward, &40_000, &1), 0);
    assert_eq!(f.raw(&f.receipt), 100_000);
    assert_eq!(f.raw(&f.strategy), 0);
    assert_eq!(f.cash(&f.receipt), 0);
}

#[test]
fn price_guard_and_killed_swap_retain_raw_rewards() {
    let f = Fixture::new();
    f.credit(100_000, 0);
    f.receipt().claim();
    let s = AquariusLpVaultClient::new(&f.env, &f.strategy);
    s.set_reward_min_rate(&f.admin, &f.reward, &20_000_000);
    assert_eq!(f.receipt().convert(&f.reward, &40_000, &1), 0);
    s.set_reward_min_rate(&f.admin, &f.reward, &9_000_000);
    MockAquariusPoolClient::new(&f.env, &f.route).set_kill_swap(&true);
    assert_eq!(f.receipt().convert(&f.reward, &40_000, &1), 0);
    assert_eq!(f.raw(&f.receipt), 100_000);
    assert_eq!(f.raw(&f.strategy), 0);
}

#[test]
fn pool_claim_to_conversion_to_backing_reinvests_through_aquarius() {
    let f = Fixture::new();
    f.credit(100_000, 0);
    f.receipt().claim();
    let before = AquariusLpVaultClient::new(&f.env, &f.strategy).total_supply();
    // Reset cumulative native-host accounting; transaction resource metering
    // stays enabled, and the measured transaction budget is asserted below.
    f.env.cost_estimate().budget().reset_unlimited();
    let units = f.receipt().compound(&f.reward, &100_000);
    let resources = f.env.cost_estimate().resources();
    assert!(
        resources.instructions < 100_000_000,
        "compound CPU exceeds transaction budget"
    );
    assert!(
        resources.memory_read_entries + resources.write_entries <= 100,
        "compound footprint exceeds transaction budget"
    );
    std::println!(
        "conversion/backing: {} entries, {} instructions",
        resources.memory_read_entries + resources.write_entries,
        resources.instructions
    );
    assert!(units > 0);
    assert_eq!(f.raw(&f.receipt), 0);
    assert_eq!(f.raw(&f.strategy), 0);
    assert!(f.cash(&f.receipt) > 0); // immediately backed, before reinvestment
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt().reinvest(&f.receipt_admin);
    let resources = f.env.cost_estimate().resources();
    assert!(resources.instructions < 100_000_000);
    assert!(resources.memory_read_entries + resources.write_entries <= 100);
    std::println!(
        "reinvestment: {} entries, {} instructions",
        resources.memory_read_entries + resources.write_entries,
        resources.instructions
    );
    assert!(AquariusLpVaultClient::new(&f.env, &f.strategy).total_supply() > before);
    assert!(f.receipt().balance(&f.receipt) > 0);
    f.receipt().allocate(&f.user, &units);
    let before = f.cash(&f.user);
    f.env.cost_estimate().budget().reset_unlimited();
    let paid = f.receipt().redeem(&f.user, &units);
    assert!(paid > 0);
    assert!(paid <= 100_000, "reward redemption took principal");
    assert_eq!(f.cash(&f.user) - before, paid as i128);
    assert!(f.receipt().try_redeem(&f.user, &units).is_err());
}

#[test]
fn bridge_rejects_external_callers_and_real_receipt_auth_tree_succeeds() {
    let f = Fixture::new();
    f.credit(100_000, 0);
    f.env.mock_auths(&[]);
    let s = AquariusLpVaultClient::new(&f.env, &f.strategy);
    assert!(s.try_hybrid_claim().is_err());
    assert!(s.try_hybrid_swap(&f.reward, &40_000, &1).is_err());
    let r = f.receipt();
    r.mock_auths(&[MockAuth {
        address: &f.receipt_admin,
        invoke: &MockAuthInvoke {
            contract: &f.receipt,
            fn_name: "claim",
            args: ().into_val(&f.env),
            sub_invokes: &[],
        },
    }])
    .claim();
    let output = r
        .mock_auths(&[MockAuth {
            address: &f.receipt_admin,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "convert",
                args: (f.reward.clone(), 40_000u128, 1u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .convert(&f.reward, &40_000, &1);
    assert!(output > 0);
    assert_eq!(f.raw(&f.receipt), 60_000);
}

#[test]
fn unsupported_pair_gauge_fails_before_consuming_primary_claim() {
    let f = Fixture::new();
    f.credit(100_000, 0);
    MockAquariusPoolClient::new(&f.env, &f.pool).set_reward_tokens(&f.reward, &f.paired);
    assert!(f.receipt().try_claim().is_err());
    assert_eq!(f.raw(&f.receipt), 0);
    assert_eq!(
        MockAquariusPoolClient::new(&f.env, &f.pool).get_user_reward(&f.strategy),
        100_000
    );
}

#[test]
fn failed_gauge_claim_rolls_back_a_successful_primary_claim() {
    let f = Fixture::new();
    // Primary transfer can succeed, but the gauge transfer cannot be funded.
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.pool, &100_000);
    let p = MockAquariusPoolClient::new(&f.env, &f.pool);
    p.credit_rewards(&f.strategy, &100_000, &50_000);
    assert!(f.receipt().try_claim().is_err());
    assert_eq!(f.raw(&f.receipt), 0);
    assert_eq!(f.raw(&f.strategy), 0);
    assert_eq!(p.get_user_reward(&f.strategy), 100_000);
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.pool, &50_000);
    assert_eq!(f.receipt().claim().get(f.reward.clone()), Some(150_000));
}

#[test]
fn incorrectly_configured_primary_cannot_consume_rewards_without_attribution() {
    let f = Fixture::new();
    f.credit(100_000, 0);
    let wrong = f
        .env
        .register_stellar_asset_contract_v2(f.admin.clone())
        .address();
    AquariusLpVaultClient::new(&f.env, &f.strategy)
        .set_primary_reward_token(&f.admin, &Some(wrong));
    assert!(f.receipt().try_claim().is_err());
    assert_eq!(f.raw(&f.receipt), 0);
    assert_eq!(f.raw(&f.pool), 100_000);
    assert_eq!(
        MockAquariusPoolClient::new(&f.env, &f.pool).get_user_reward(&f.strategy),
        100_000
    );
}

#[test]
fn reward_token_rotation_preserves_old_receipt_inventory() {
    let f = Fixture::new();
    f.credit(100_000, 0);
    f.receipt().claim();
    let new_reward = f
        .env
        .register_stellar_asset_contract_v2(f.admin.clone())
        .address();
    let p = MockAquariusPoolClient::new(&f.env, &f.pool);
    p.set_reward_tokens(&new_reward, &f.reward);
    p.credit_rewards(&f.strategy, &50_000, &20_000);
    token::StellarAssetClient::new(&f.env, &new_reward).mint(&f.pool, &50_000);
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.pool, &20_000);
    AquariusLpVaultClient::new(&f.env, &f.strategy)
        .set_primary_reward_token(&f.admin, &Some(new_reward.clone()));
    let claimed = f.receipt().claim();
    assert_eq!(claimed.get(new_reward.clone()), Some(50_000));
    assert_eq!(claimed.get(f.reward.clone()), Some(20_000));
    assert_eq!(f.raw(&f.receipt), 120_000);
    assert_eq!(
        token::Client::new(&f.env, &new_reward).balance(&f.receipt),
        50_000
    );
    // No route for the new asset yet: it remains in receipt custody, while the
    // old configured route can still settle existing old-token claims.
    assert_eq!(f.receipt().convert(&new_reward, &50_000, &1), 0);
    assert!(f.receipt().convert(&f.reward, &20_000, &1) > 0);
    assert_eq!(f.raw(&f.receipt), 100_000);
    assert_eq!(
        token::Client::new(&f.env, &new_reward).balance(&f.receipt),
        50_000
    );
}

#[test]
fn user_minimum_and_missing_route_preserve_exact_input() {
    let f = Fixture::new();
    f.credit(100_000, 0);
    f.receipt().claim();
    assert_eq!(f.receipt().convert(&f.reward, &40_000, &80_000), 0);
    AquariusLpVaultClient::new(&f.env, &f.strategy).set_reward_route(&f.admin, &f.reward, &None);
    assert_eq!(f.receipt().convert(&f.reward, &40_000, &1), 0);
    assert_eq!(f.raw(&f.receipt), 100_000);
    assert_eq!(f.raw(&f.strategy), 0);
    assert_eq!(f.cash(&f.receipt), 0);
}

#[test]
fn persistent_ownership_excludes_late_depositors_from_older_claims() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_000);
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt().owned_deposit(&f.reward, &late, &1_000_000);
    assert_eq!(
        f.receipt().earned(&f.reward, &f.user).raw_scaled,
        100_000 * ledger::SCALE
    );
    assert_eq!(f.receipt().earned(&f.reward, &late).raw_scaled, 0);
    f.env.cost_estimate().budget().reset_unlimited();
    let minted = f.receipt().owned_compound(&f.reward);
    let old = f.receipt().earned(&f.reward, &f.user);
    assert!(f.receipt().try_earned(&f.reward, &f.receipt).is_err());
    assert!(old.units_scaled / ledger::SCALE > 0);
    assert!(old.units_scaled / ledger::SCALE <= minted);
    assert_eq!(f.receipt().earned(&f.reward, &late).units_scaled, 0);
    let ordinary_before = f.receipt().balance(&f.user);
    f.env.cost_estimate().budget().reset_unlimited();
    assert!(f.receipt().owned_redeem(&f.reward, &f.user) > 0);
    assert_eq!(f.receipt().balance(&f.user), ordinary_before);
    assert!(f.receipt().try_owned_redeem(&f.reward, &f.user).is_err());
}

#[test]
fn persistent_transfer_retains_history_and_assigns_future_rewards_to_receiver() {
    let f = Fixture::new();
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    let receiver = Address::generate(&f.env);
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt()
        .owned_transfer(&f.reward, &f.user, &receiver, &500_000);
    assert_eq!(
        f.receipt().earned(&f.reward, &f.user).raw_scaled,
        100_000 * ledger::SCALE
    );
    assert_eq!(f.receipt().earned(&f.reward, &receiver).raw_scaled, 0);
    f.credit(200_000, 0);
    // Self-transfer checkpoints new rewards without changing either weight.
    f.receipt()
        .owned_transfer(&f.reward, &receiver, &receiver, &1);
    assert_eq!(
        f.receipt().earned(&f.reward, &f.user).raw_scaled,
        200_000 * ledger::SCALE
    );
    assert_eq!(
        f.receipt().earned(&f.reward, &receiver).raw_scaled,
        100_000 * ledger::SCALE
    );
}

#[test]
fn persistent_partial_exit_reserves_only_earned_portion_and_retries_guarded_swap() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    f.env.cost_estimate().budget().reset_unlimited();
    assert_eq!(
        f.receipt().owned_withdraw(&f.reward, &f.user, &500_000),
        50_000
    );
    let before = f.receipt().earned(&f.reward, &f.user);
    assert_eq!(before.reserved, 50_000);
    assert_eq!(before.raw_scaled, 50_000 * ledger::SCALE);
    MockAquariusPoolClient::new(&f.env, &f.route).set_kill_swap(&true);
    assert_eq!(f.receipt().owned_settle(&f.reward, &f.user, &50_000, &1), 0);
    assert_eq!(f.receipt().earned(&f.reward, &f.user), before);
    MockAquariusPoolClient::new(&f.env, &f.route).set_kill_swap(&false);
    let cash_before = f.cash(&f.user);
    let paid = f.receipt().owned_settle(&f.reward, &f.user, &50_000, &1);
    assert_eq!(f.cash(&f.user) - cash_before, paid as i128);
    assert_eq!(f.receipt().earned(&f.reward, &f.user).reserved, 0);
    assert!(f
        .receipt()
        .try_owned_settle(&f.reward, &f.user, &1, &1)
        .is_err());
    assert_eq!(f.raw(&f.receipt), 50_000);
}

#[test]
fn persistent_final_exit_keeps_reserved_reward_after_supply_reaches_zero() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    f.env.cost_estimate().budget().reset_unlimited();
    assert_eq!(
        f.receipt().owned_withdraw(&f.reward, &f.user, &1_000_000),
        100_000
    );
    assert_eq!(f.receipt().balance(&f.user), 0);
    let a = f.receipt().earned(&f.reward, &f.user);
    assert_eq!(a.reserved, 100_000);
    assert_eq!(a.weight, 0);
    f.env.cost_estimate().budget().reset_unlimited();
    assert!(f.receipt().owned_settle(&f.reward, &f.user, &100_000, &1) > 0);
    assert_eq!(f.receipt().earned(&f.reward, &f.user).reserved, 0);
}

#[test]
#[cfg(not(feature = "lp-receipt-prototype"))]
fn persistent_strategy_funded_deposit_budget_blocker_is_measured_not_hidden() {
    let f = Fixture::new();
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_000);
    // DIAGNOSTIC ONLY: the identical default-limit invocation fails at 106/100.
    // This measurement must not be cited as a network-executable success.
    f.env.cost_estimate().disable_resource_limits();
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt().owned_deposit(&f.reward, &late, &1_000_000);
    let r = f.env.cost_estimate().resources();
    let entries = r.memory_read_entries + r.write_entries;
    std::println!("BLOCKED owned strategy deposit: {entries}/100 entries");
    assert!(
        entries > 100,
        "budget fixed: remove this diagnostic quarantine and enable normal-limit test"
    );
    assert_eq!(f.receipt().earned(&f.reward, &late).raw_scaled, 0);
}

#[test]
#[cfg(feature = "lp-receipt-prototype")]
fn lean_strategy_funded_ownership_deposit_and_exits_fit_native_limits() {
    let f = Fixture::new();
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_000);
    // Unlike the generic-receipt diagnostic, transaction limits stay enabled.
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt().owned_deposit(&f.reward, &late, &1_000_000);
    let r = f.env.cost_estimate().resources();
    std::println!(
        "lean owned strategy deposit: {} entries / {} instructions",
        r.memory_read_entries + r.write_entries,
        r.instructions
    );
    assert!(r.memory_read_entries + r.write_entries <= 100);
    assert!(r.instructions < 100_000_000);
    assert_eq!(f.receipt().earned(&f.reward, &late).raw_scaled, 0);
    assert_eq!(
        f.receipt().earned(&f.reward, &f.user).raw_scaled,
        100_000 * ledger::SCALE
    );
    for owner in [&late, &f.user] {
        f.env.cost_estimate().budget().reset_unlimited();
        let shares = f.receipt().balance(owner) as u128;
        f.receipt().owned_withdraw(&f.reward, owner, &shares);
        let r = f.env.cost_estimate().resources();
        std::println!(
            "lean owned strategy exit: {} entries / {} instructions",
            r.memory_read_entries + r.write_entries,
            r.instructions
        );
        assert!(r.memory_read_entries + r.write_entries <= 100);
        assert!(r.instructions < 100_000_000);
    }
    assert_eq!(
        AquariusLpVaultClient::new(&f.env, &f.strategy).balance(&f.receipt),
        0
    );
    assert_eq!(f.receipt().earned(&f.reward, &f.user).reserved, 100_000);
    assert_eq!(f.cash(&f.receipt), 0);
}

#[test]
#[cfg(feature = "lp-receipt-prototype")]
fn lean_second_settlement_leg_compounds_and_preserves_old_owner_claims() {
    let f = Fixture::with_settlement_index(0, 1);
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    f.env.cost_estimate().budget().reset_unlimited();
    assert!(f.receipt().owned_compound(&f.reward) > 0);
    let shares = f.receipt().balance(&f.user);
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt().reinvest(&f.receipt_admin);
    f.env.cost_estimate().budget().reset_unlimited();
    assert!(f.receipt().owned_redeem(&f.reward, &f.user) > 0);
    assert_eq!(f.receipt().balance(&f.user), shares);
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt()
        .owned_withdraw(&f.reward, &f.user, &(shares as u128));
    assert_eq!(
        AquariusLpVaultClient::new(&f.env, &f.strategy).balance(&f.receipt),
        0
    );
    assert_eq!(f.receipt().balance(&f.user), 0);
}

#[test]
#[cfg(feature = "lp-receipt-prototype")]
fn lean_failed_strategy_exit_rolls_back_reward_claim_and_principal_burn() {
    let f = Fixture::new();
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    MockAquariusPoolClient::new(&f.env, &f.pool).set_fail_withdraw(&true);
    f.env.cost_estimate().budget().reset_unlimited();
    assert!(f
        .receipt()
        .try_owned_withdraw(&f.reward, &f.user, &1_000_000)
        .is_err());
    assert_eq!(f.receipt().balance(&f.user), 1_000_000);
    assert_eq!(f.raw(&f.receipt), 0);
    assert_eq!(f.receipt().earned(&f.reward, &f.user).reserved, 0);
    assert_eq!(
        MockAquariusPoolClient::new(&f.env, &f.pool).get_user_reward(&f.strategy),
        100_000
    );
}

#[test]
#[cfg(feature = "lp-receipt-prototype")]
fn lean_strategy_deposit_and_exit_use_exact_owner_auth_without_blanket_mocks() {
    let f = Fixture::new();
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_000);
    f.env.mock_auths(&[]);
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt()
        .mock_auths(&[MockAuth {
            address: &late,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "owned_deposit",
                args: (f.reward.clone(), late.clone(), 1_000_000u128).into_val(&f.env),
                sub_invokes: &[MockAuthInvoke {
                    contract: &f.asset,
                    fn_name: "transfer",
                    args: (late.clone(), f.receipt.clone(), 1_000_000i128).into_val(&f.env),
                    sub_invokes: &[],
                }],
            },
        }])
        .owned_deposit(&f.reward, &late, &1_000_000);
    let shares = f.receipt().balance(&late) as u128;
    f.env.mock_auths(&[]);
    f.env.cost_estimate().budget().reset_unlimited();
    assert!(f
        .receipt()
        .try_owned_withdraw(&f.reward, &late, &shares)
        .is_err());
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt()
        .mock_auths(&[MockAuth {
            address: &late,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "owned_withdraw",
                args: (f.reward.clone(), late.clone(), shares).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .owned_withdraw(&f.reward, &late, &shares);
    assert_eq!(f.receipt().balance(&late), 0);
}

#[test]
fn persistent_epochs_catch_up_without_loading_intermediate_history() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    let mut total = 0;
    for _ in 0..20 {
        f.env.cost_estimate().budget().reset_unlimited();
        token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.user, &1_000);
        f.receipt().test_receive(&f.reward, &f.user, &1_000);
        total += f.receipt().test_convert(&f.reward, &f.user, &1_000);
    }
    f.env.as_contract(&f.receipt, || {
        for epoch in 1..20 {
            f.env
                .storage()
                .persistent()
                .remove(&ledger::LedgerKey::HybridEpoch(f.reward.clone(), epoch));
        }
    });
    f.env.cost_estimate().budget().reset_unlimited();
    let a = f.receipt().earned(&f.reward, &f.user);
    assert_eq!(a.epoch, 20);
    assert_eq!(a.units_scaled / ledger::SCALE, total);
}

#[test]
fn persistent_missing_required_epoch_fails_without_erasing_claims() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    f.env.cost_estimate().budget().reset_unlimited();
    let units = f.receipt().owned_compound(&f.reward);
    let cash = f.cash(&f.receipt);
    f.env.as_contract(&f.receipt, || {
        f.env
            .storage()
            .persistent()
            .remove(&ledger::LedgerKey::HybridEpoch(f.reward.clone(), 0))
    });
    assert!(f.receipt().try_owned_redeem(&f.reward, &f.user).is_err());
    f.env.as_contract(&f.receipt, || {
        assert_eq!(backing::state(&f.env).unallocated_units, units);
        assert_eq!(backing::owner_units(&f.env, &f.user), 0);
    });
    assert_eq!(f.cash(&f.receipt), cash);
}

#[test]
fn persistent_account_stream_and_used_history_ttls_are_renewed() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    f.receipt().earned(&f.reward, &f.user); // persist epoch-zero account
    f.credit(100_000, 0);
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt().owned_compound(&f.reward);
    f.env
        .ledger()
        .with_mut(|info| info.sequence_number += 600_001);
    let keys = [
        ledger::LedgerKey::HybridStream(f.reward.clone()),
        ledger::LedgerKey::HybridAccount(f.reward.clone(), f.user.clone()),
        ledger::LedgerKey::HybridEpoch(f.reward.clone(), 0),
    ];
    f.env.as_contract(&f.receipt, || {
        for key in &keys {
            assert!(f.env.storage().persistent().get_ttl(key) < 500_000);
        }
    });
    f.receipt().earned(&f.reward, &f.user);
    f.env.as_contract(&f.receipt, || {
        for key in &keys {
            assert!(f.env.storage().persistent().get_ttl(key) >= 999_999);
        }
    });
}

#[test]
fn persistent_reserved_payout_requires_the_earning_owners_authorization() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt().owned_withdraw(&f.reward, &f.user, &1_000_000);
    f.env.mock_auths(&[]);
    assert!(f
        .receipt()
        .try_owned_settle(&f.reward, &f.user, &100_000, &1)
        .is_err());
    assert_eq!(f.receipt().earned(&f.reward, &f.user).reserved, 100_000);
    let paid = f
        .receipt()
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "owned_settle",
                args: (f.reward.clone(), f.user.clone(), 100_000u128, 1u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .owned_settle(&f.reward, &f.user, &100_000, &1);
    assert!(paid > 0);
}

#[test]
fn persistent_recycled_emissions_fail_closed_until_coordinator_exists() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt().owned_compound(&f.reward);
    f.credit(50_000, 0);
    let before = f.receipt().earned(&f.reward, &f.user);
    assert!(f.receipt().try_owned_compound(&f.reward).is_err());
    assert_eq!(f.receipt().earned(&f.reward, &f.user), before);
    assert_eq!(
        MockAquariusPoolClient::new(&f.env, &f.pool).get_user_reward(&f.strategy),
        50_000
    );
}

#[test]
fn persistent_stream_rotation_retains_historical_ownership() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    let receiver = Address::generate(&f.env);
    f.receipt()
        .owned_transfer(&f.reward, &f.user, &receiver, &500_000);
    let second = f
        .env
        .register_stellar_asset_contract_v2(f.admin.clone())
        .address();
    f.receipt().start_ledger(&second);
    token::StellarAssetClient::new(&f.env, &second).mint(&f.user, &20_000);
    f.receipt().test_receive(&second, &f.user, &20_000);
    assert_eq!(
        f.receipt().earned(&f.reward, &f.user).raw_scaled,
        100_000 * ledger::SCALE
    );
    assert_eq!(f.receipt().earned(&f.reward, &receiver).raw_scaled, 0);
    assert_eq!(
        f.receipt().earned(&second, &f.user).raw_scaled,
        10_000 * ledger::SCALE
    );
    assert_eq!(
        f.receipt().earned(&second, &receiver).raw_scaled,
        10_000 * ledger::SCALE
    );
    assert!(f.receipt().try_start_ledger(&f.reward).is_err());
}

#[test]
fn persistent_unrecognized_donations_are_neither_indexed_nor_converted() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.receipt, &7_000);
    token::StellarAssetClient::new(&f.env, &f.reward).mint(&f.user, &100_000);
    f.receipt().test_receive(&f.reward, &f.user, &100_000);
    assert_eq!(
        f.receipt().earned(&f.reward, &f.user).raw_scaled,
        100_000 * ledger::SCALE
    );
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt().test_convert(&f.reward, &f.user, &100_000);
    assert_eq!(f.raw(&f.receipt), 7_000);
    assert_eq!(
        f.receipt().earned(&f.reward, &f.user).units_scaled,
        100_000 * ledger::SCALE
    );
}

#[test]
fn persistent_unauthorized_unit_redemption_rolls_back_lazy_allocation() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().start_ledger(&f.reward);
    f.credit(100_000, 0);
    f.env.cost_estimate().budget().reset_unlimited();
    f.receipt().owned_compound(&f.reward);
    let before = f.receipt().earned(&f.reward, &f.user);
    f.env.mock_auths(&[]);
    assert!(f.receipt().try_owned_redeem(&f.reward, &f.user).is_err());
    assert_eq!(f.receipt().earned(&f.reward, &f.user), before);
    f.env.as_contract(&f.receipt, || {
        assert_eq!(backing::owner_units(&f.env, &f.user), 0)
    });
}
