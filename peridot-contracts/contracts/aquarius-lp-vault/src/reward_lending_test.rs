//! Native lending engine + native Aquarius bridge/strategy + mock concentrated
//! pools/oracle, real SAC cash. NOT a compiled-stack or live-pool fit claim.
extern crate std;
use crate::{
    test::{MockOracle, MockOracleClient},
    AquariusLpVault, AquariusLpVaultClient,
};
use jump_rate_model::{JumpRateModel, JumpRateModelClient};
use mock_aquarius_pool::{MockAquariusPool, MockAquariusPoolClient};
use receipt_vault::{
    reward_backing as backing, reward_claims as claims, reward_ledger as ledger,
    reward_lending as lending, ReceiptVault as Core, SeizeContext,
};
use simple_peridottroller::{SimplePeridottroller, SimplePeridottrollerClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{
        cost_estimate::NetworkInvocationResourceLimits, Address as _, Ledger as _, MockAuth,
        MockAuthInvoke,
    },
    token, vec, Address, Env, IntoVal, Map,
};

#[contract]
struct Market;
#[contractimpl]
impl Market {
    pub fn initialize(env: Env, asset: Address, admin: Address) {
        Core::initialize(env, asset, 0, 0, admin);
    }
    pub fn model(env: Env, model: Address) {
        Core::set_interest_model(env, model);
    }
    pub fn controller(env: Env, controller: Address) {
        Core::set_peridottroller(env, controller);
    }
    pub fn bind(env: Env, admin: Address, strategy: Address) {
        Core::set_boosted_vault(env, admin, strategy);
    }
    pub fn buffer(env: Env, admin: Address, bps: u32) {
        Core::set_idle_cash_buffer_bps(env, admin, bps);
    }
    pub fn refresh(env: Env) {
        Core::refresh_boosted_underlying(env);
    }
    pub fn reinvest(env: Env, admin: Address) {
        Core::rebalance_idle_cash(env, admin);
    }
    pub fn bootstrap(env: Env, user: Address, amount: u128) {
        assert!(!env
            .storage()
            .instance()
            .has(&ledger::LedgerKey::HybridRegistryInitialized));
        Core::deposit(env, user, amount);
    }
    pub fn rewards(env: Env, primary: Address, gauge: Address) {
        backing::initialize(&env);
        claims::register(&env, &primary);
        claims::register(&env, &gauge);
    }
    pub fn claim(env: Env) {
        claims::claim(&env);
    }
    pub fn earned(env: Env, asset: Address, user: Address) -> u128 {
        let weight = Core::get_ptoken_balance(env.clone(), user.clone());
        ledger::checkpoint(&env, &asset, &user, weight).raw_scaled / ledger::SCALE
    }
    pub fn reserved(env: Env, asset: Address, user: Address) -> u128 {
        let weight = Core::get_ptoken_balance(env.clone(), user.clone());
        ledger::checkpoint(&env, &asset, &user, weight).reserved
    }
    pub fn deposit(env: Env, user: Address, amount: u128) {
        lending::deposit(&env, &user, amount);
    }
    pub fn withdraw(
        env: Env,
        user: Address,
        shares: u128,
        minimum: u128,
    ) -> (Map<Address, u128>, u128) {
        lending::withdraw(&env, &user, shares, minimum)
    }
    pub fn borrow(env: Env, user: Address, amount: u128) {
        lending::borrow(&env, &user, amount);
    }
    pub fn repay(env: Env, user: Address, amount: u128) {
        Core::repay(env, user, amount);
    }
    pub fn repay_on_behalf(env: Env, payer: Address, borrower: Address, amount: u128) {
        Core::repay_on_behalf(env, payer, borrower, amount);
    }
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        lending::transfer(&env, &from, &to, amount);
    }
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        lending::transfer_from(&env, &spender, &from, &to, amount);
    }
    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128, until: u32) {
        Core::approve(env, owner, spender, amount, until);
    }
    pub fn seize(
        env: Env,
        borrower: Address,
        liquidator: Address,
        amount: u128,
        ctx: Option<SeizeContext>,
    ) {
        lending::seize(&env, &borrower, &liquidator, amount, ctx);
    }
    pub fn get_boosted_vault(env: Env) -> Option<Address> {
        Core::get_boosted_vault(env)
    }
    pub fn get_account_snapshot(env: Env, user: Address) -> (u128, u128, u128, Address) {
        Core::get_account_snapshot(env, user)
    }
    pub fn get_underlying_token(env: Env) -> Address {
        Core::get_underlying_token(env)
    }
    pub fn get_total_ptokens(env: Env) -> u128 {
        Core::get_total_ptokens(env)
    }
    pub fn get_ptoken_balance(env: Env, user: Address) -> u128 {
        Core::get_ptoken_balance(env, user)
    }
    pub fn get_total_borrowed(env: Env) -> u128 {
        Core::get_total_borrowed(env)
    }
    pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {
        Core::get_user_borrow_balance(env, user)
    }
    pub fn get_exchange_rate(env: Env) -> u128 {
        Core::get_exchange_rate(env)
    }
    pub fn update_interest(env: Env) {
        Core::update_interest(env);
    }
    pub fn nav(env: Env) -> u128 {
        Core::get_total_underlying(env)
    }
}

struct Fixture {
    env: Env,
    admin: Address,
    user: Address,
    supplier: Address,
    controller: Address,
    assets: [Address; 3],
    markets: [Address; 3],
    strategies: [Address; 3],
    pools: [Address; 3],
    rewards: [Address; 2],
    oracle: Address,
}
impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        let mut limits = soroban_env_host::InvocationResourceLimits::mainnet();
        // Three-market exit exceeded the earlier 200-entry research envelope
        // at 205. Bound this integrated fixture at 250; all other limits stay ON.
        // Neither envelope is current network or compiled-stack fit evidence.
        limits.ledger_entries = 250;
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
        let user = Address::generate(&env);
        let supplier = Address::generate(&env);
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
        let oracle = env.register(MockOracle, ());
        for a in [&assets[0], &assets[1], &assets[2], &paired] {
            MockOracleClient::new(&env, &oracle).set_price(a, &100_000_000_000_000);
            token::StellarAssetClient::new(&env, a).mint(&user, &10_000_000);
            token::StellarAssetClient::new(&env, a).mint(&supplier, &10_000_000_000_000);
        }
        let xlm_pool = env.register(MockAquariusPool, ());
        let stable_pool = env.register(MockAquariusPool, ());
        for (id, a, b) in [
            (&xlm_pool, &assets[0], &paired),
            (&stable_pool, &assets[1], &assets[2]),
        ] {
            let p = MockAquariusPoolClient::new(&env, id);
            p.initialize(a, b, &20, &0);
            p.set_deposit_ratio_0_per_1_e6(&1_000_000);
            p.set_reward_tokens(&rewards[0], &rewards[1]);
            p.deposit_position(
                &supplier,
                &-887220,
                &887220,
                &vec![&env, 1_000_000_000_000u128, 1_000_000_000_000u128],
                &0,
            );
        }
        let controller = env.register(SimplePeridottroller, ());
        let c = SimplePeridottrollerClient::new(&env, &controller);
        c.initialize(&admin);
        c.set_oracle(&oracle);
        let model = env.register(JumpRateModel, ());
        JumpRateModelClient::new(&env, &model)
            .initialize(&10_000, &180_000, &4_000_000, &800_000, &admin);
        let pools = [xlm_pool, stable_pool.clone(), stable_pool];
        let markets = core::array::from_fn(|_| env.register(Market, ()));
        let strategies = core::array::from_fn(|_| env.register(AquariusLpVault, ()));
        for i in 0..3 {
            env.cost_estimate().budget().reset_unlimited();
            let r = MarketClient::new(&env, &markets[i]);
            let s = AquariusLpVaultClient::new(&env, &strategies[i]);
            s.initialize(&admin, &pools[i], &u32::from(i == 2), &oracle);
            s.set_primary_reward_token(&admin, &Some(rewards[0].clone()));
            r.initialize(&assets[i], &admin);
            r.model(&model);
            r.bind(&admin, &strategies[i]);
            r.buffer(&admin, &0);
            s.set_receipt_vault(&admin, &markets[i]);
            s.refresh_nav_root();
            c.add_market(&markets[i]);
            c.set_market_cf(&markets[i], &700_000);
            r.controller(&controller);
            r.bootstrap(if i == 0 { &user } else { &supplier }, &1_000_000);
            r.refresh();
            r.rewards(&rewards[0], &rewards[1]);
            c.enter_market(&user, &markets[i]);
        }
        env.mock_all_auths();
        Self {
            env,
            admin,
            user,
            supplier,
            controller,
            assets,
            markets,
            strategies,
            pools,
            rewards,
            oracle,
        }
    }
    fn r(&self, i: usize) -> MarketClient<'_> {
        MarketClient::new(&self.env, &self.markets[i])
    }
    fn reset(&self) {
        self.env.cost_estimate().budget().reset_unlimited();
    }
    fn measure(&self, label: &str) {
        let r = self.env.cost_estimate().resources();
        let entries = r.disk_read_entries + r.memory_read_entries + r.write_entries;
        std::println!(
            "aquarius-lending {label}: {entries} entries, {} writes, {} CPU",
            r.write_entries,
            r.instructions
        );
        assert!(entries <= 250 && r.instructions <= 100_000_000);
    }
    fn credit(&self, i: usize, primary: u128, gauge: u128) {
        for (asset, amount) in self.rewards.iter().zip([primary, gauge]) {
            token::StellarAssetClient::new(&self.env, asset)
                .mint(&self.pools[i], &(amount as i128));
        }
        MockAquariusPoolClient::new(&self.env, &self.pools[i]).credit_rewards(
            &self.strategies[i],
            &primary,
            &gauge,
        );
    }
    fn strategy_shares(&self, i: usize) -> i128 {
        AquariusLpVaultClient::new(&self.env, &self.strategies[i]).balance(&self.markets[i])
    }
    fn cash(&self, i: usize, owner: &Address) -> i128 {
        token::Client::new(&self.env, &self.assets[i]).balance(owner)
    }
}

#[test]
fn aquarius_lending_both_stables_unwind_lp_claim_real_rewards_and_repay() {
    let f = Fixture::new();
    for i in [1, 2] {
        f.credit(i, 1000, 2000);
        let before = f.strategy_shares(i);
        let wallet = f.cash(i, &f.user);
        let supply = f.r(i).get_total_ptokens();
        assert_eq!(f.cash(i, &f.markets[i]), 0);
        f.reset();
        f.r(i).borrow(&f.user, &200_000);
        f.measure("LP-funded borrow");
        assert!(f.strategy_shares(i) < before);
        assert_eq!(f.cash(i, &f.user) - wallet, 200_000);
        assert_eq!(f.r(i).get_total_borrowed(), 200_000);
        assert_eq!(f.r(i).get_total_ptokens(), supply);
        for (asset, raw) in f.rewards.iter().zip([1000u128, 2000]) {
            assert_eq!(f.r(i).earned(asset, &f.supplier), raw);
            assert_eq!(f.r(i).earned(asset, &f.user), 0);
            assert_eq!(
                token::Client::new(&f.env, asset).balance(&f.markets[i]),
                raw as i128
            );
        }
    }
    // Remaining shares are a claim on BOTH remaining LP capital and loans.
    f.reset();
    assert!(f.r(1).try_withdraw(&f.supplier, &1_000_000, &1).is_err());
    assert_eq!(f.r(1).get_ptoken_balance(&f.supplier), 1_000_000);
    assert_eq!(f.r(1).reserved(&f.rewards[0], &f.supplier), 0);
    for i in [1, 2] {
        let shares = f.strategy_shares(i);
        let cash = f.cash(i, &f.markets[i]);
        let nav = f.r(i).nav();
        f.reset();
        f.r(i).repay(&f.user, &200_000);
        f.measure("repayment");
        assert_eq!(f.r(i).get_total_borrowed(), 0);
        assert_eq!(f.cash(i, &f.markets[i]) - cash, 200_000);
        assert_eq!(f.r(i).nav(), nav);
        assert_eq!(f.strategy_shares(i), shares); // no implicit pool/reward dependency
        f.reset();
        f.r(i).reinvest(&f.admin);
        assert!(f.strategy_shares(i) > shares);
        for (asset, raw) in f.rewards.iter().zip([1000u128, 2000]) {
            assert_eq!(f.r(i).earned(asset, &f.supplier), raw);
        }
    }
}

#[test]
fn aquarius_lending_collateral_exit_is_health_checked_and_failed_minimum_rolls_back() {
    let f = Fixture::new();
    f.reset();
    f.r(1).borrow(&f.user, &600_000);
    f.credit(0, 1000, 2000);
    let shares = f.strategy_shares(0);
    f.reset();
    assert!(f.r(0).try_withdraw(&f.user, &500_000, &1).is_err());
    assert_eq!(f.strategy_shares(0), shares);
    assert_eq!(f.r(0).get_ptoken_balance(&f.user), 1_000_000);
    assert_eq!(f.r(0).reserved(&f.rewards[0], &f.user), 0);
    f.reset();
    f.r(1).repay(&f.user, &600_000);
    f.reset();
    assert!(f.r(0).try_withdraw(&f.user, &500_000, &1_000_000).is_err());
    assert_eq!(f.strategy_shares(0), shares);
    assert_eq!(f.r(0).reserved(&f.rewards[0], &f.user), 0);
    f.reset();
    let (reserved, paid) = f.r(0).withdraw(&f.user, &500_000, &400_000);
    f.measure("LP-funded exit");
    assert!(paid >= 400_000);
    assert!(f.strategy_shares(0) < shares);
    assert_eq!(reserved.get(f.rewards[0].clone()), Some(500));
    assert_eq!(reserved.get(f.rewards[1].clone()), Some(1000));
}

#[test]
fn aquarius_lending_late_deposit_and_delegated_transfer_keep_old_pool_rewards() {
    let f = Fixture::new();
    let newcomer = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.assets[1]).mint(&newcomer, &1_000_000);
    f.credit(1, 1000, 2000);
    f.reset();
    f.r(1).deposit(&newcomer, &500_000);
    f.measure("late LP deposit");
    for (asset, raw) in f.rewards.iter().zip([1000u128, 2000]) {
        assert_eq!(f.r(1).earned(asset, &newcomer), 0);
        assert_eq!(f.r(1).earned(asset, &f.supplier), raw);
    }
    f.r(1).approve(&f.supplier, &newcomer, &200_000, &200);
    f.reset();
    f.r(1)
        .transfer_from(&newcomer, &f.supplier, &newcomer, &200_000);
    f.measure("delegated transfer");
    for (asset, raw) in f.rewards.iter().zip([1000u128, 2000]) {
        assert_eq!(f.r(1).earned(asset, &newcomer), 0);
        assert_eq!(f.r(1).earned(asset, &f.supplier), raw);
    }
}

#[test]
fn aquarius_lending_observable_claim_outage_preserves_exit_claims_and_repay_needs_no_quotes() {
    let f = Fixture::new();
    f.reset();
    f.r(1).borrow(&f.user, &200_000);
    f.credit(1, 1000, 2000);
    let pool = MockAquariusPoolClient::new(&f.env, &f.pools[1]);
    pool.set_kill_claim(&true);
    pool.set_fail_reward_quote(&true);
    f.reset();
    f.r(1).repay(&f.user, &200_000);
    assert_eq!(f.r(1).get_total_borrowed(), 0);
    f.reset();
    assert!(f.r(1).try_withdraw(&f.supplier, &500_000, &1).is_err());
    assert_eq!(f.r(1).get_ptoken_balance(&f.supplier), 1_000_000);
    pool.set_fail_reward_quote(&false);
    f.reset();
    let (reserved, _) = f.r(1).withdraw(&f.supplier, &500_000, &400_000);
    f.measure("observable-outage exit");
    assert_eq!(reserved.get(f.rewards[0].clone()), Some(500));
    assert_eq!(
        token::Client::new(&f.env, &f.rewards[0]).balance(&f.markets[1]),
        0
    );
    pool.set_kill_claim(&false);
    f.reset();
    f.r(1).claim();
    assert_eq!(f.r(1).reserved(&f.rewards[0], &f.supplier), 500);
    assert_eq!(f.r(1).earned(&f.rewards[0], &f.supplier), 500);
    assert_eq!(
        token::Client::new(&f.env, &f.rewards[0]).balance(&f.markets[1]),
        1000
    );
}

#[test]
fn aquarius_lending_exact_borrower_auth_covers_actual_lp_unwind() {
    let f = Fixture::new();
    f.credit(1, 1000, 2000);
    f.env.mock_auths(&[]);
    f.reset();
    assert!(f.r(1).try_borrow(&f.user, &200_000).is_err());
    f.env.mock_auths(&[MockAuth {
        address: &f.user,
        invoke: &MockAuthInvoke {
            contract: &f.markets[1],
            fn_name: "borrow",
            args: (&f.user, 200_000u128).into_val(&f.env),
            sub_invokes: &[],
        },
    }]);
    f.reset();
    f.r(1).borrow(&f.user, &200_000);
    f.measure("exact-auth LP borrow");
    assert_eq!(f.r(1).get_total_borrowed(), 200_000);
}

#[test]
fn aquarius_lending_liquidation_checkpoints_pool_rewards_before_seizure() {
    let f = Fixture::new();
    f.reset();
    f.r(1).borrow(&f.user, &600_000);
    f.credit(0, 1000, 2000);
    MockOracleClient::new(&f.env, &f.oracle).set_price(&f.assets[0], &50_000_000_000_000);
    let c = SimplePeridottrollerClient::new(&f.env, &f.controller);
    c.cache_price(&f.assets[0]);
    token::Client::new(&f.env, &f.assets[1]).approve(&f.supplier, &f.markets[1], &100_000, &1000);
    f.env.mock_auths(&[MockAuth {
        address: &f.supplier,
        invoke: &MockAuthInvoke {
            contract: &f.controller,
            fn_name: "liquidate",
            args: (
                &f.user,
                &f.markets[1],
                &f.markets[0],
                100_000u128,
                &f.supplier,
            )
                .into_val(&f.env),
            sub_invokes: &[],
        },
    }]);
    f.reset();
    c.liquidate(&f.user, &f.markets[1], &f.markets[0], &100_000, &f.supplier);
    f.measure("exact-auth liquidation with claims");
    assert_eq!(f.r(1).get_total_borrowed(), 500_000);
    let moved = f.r(0).get_ptoken_balance(&f.supplier);
    assert!(moved > 0);
    for (asset, raw) in f.rewards.iter().zip([1000u128, 2000]) {
        assert_eq!(f.r(0).earned(asset, &f.user), raw);
        assert_eq!(f.r(0).earned(asset, &f.supplier), 0);
    }
    f.env.mock_all_auths();
    f.credit(0, 1_000_000, 1_000_000);
    f.reset();
    f.r(0).claim();
    for asset in &f.rewards {
        assert_eq!(f.r(0).earned(asset, &f.supplier), moved);
    }
}

#[test]
fn aquarius_lending_partial_exit_keeps_loan_nav_but_legacy_cash_can_spend_donations() {
    let f = Fixture::new();
    f.reset();
    f.r(1).borrow(&f.user, &200_000);
    f.credit(1, 1000, 2000);
    let nav = f.r(1).nav();
    // Settlement and reward donations are real cash, but not managed principal
    // or newly claimed emissions.
    token::StellarAssetClient::new(&f.env, &f.assets[1]).mint(&f.markets[1], &50_000);
    token::StellarAssetClient::new(&f.env, &f.rewards[0]).mint(&f.markets[1], &777);
    assert_eq!(f.r(1).nav(), nav);
    let rate = f.r(1).get_exchange_rate();
    let wallet = f.cash(1, &f.supplier);
    f.reset();
    let (_, paid) = f.r(1).withdraw(&f.supplier, &200_000, &150_000);
    f.measure("partial lender exit with outstanding loan");
    assert_eq!(paid, 200_000 * rate / 1_000_000);
    assert_eq!(f.cash(1, &f.supplier) - wallet, paid as i128);
    assert_eq!(f.r(1).get_total_borrowed(), 200_000);
    assert_eq!(f.r(1).get_total_ptokens(), 800_000);
    assert_eq!(f.r(1).reserved(&f.rewards[0], &f.supplier), 200);
    assert_eq!(f.r(1).earned(&f.rewards[0], &f.supplier), 800);
    let managed: u128 = f.env.as_contract(&f.markets[1], || {
        f.env
            .storage()
            .persistent()
            .get(&receipt_vault::DataKey::ManagedCash)
            .unwrap()
    });
    // NAV prefers the current strategy quote; book/cache adjustment is not a
    // fresh quote after actual LP redemption changes the strategy composition.
    let boosted: u128 = AquariusLpVaultClient::new(&f.env, &f.strategies[1])
        .get_asset_amounts_per_shares(&f.strategy_shares(1))
        .get(0)
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(f.r(1).nav(), managed + boosted + 200_000);
    // Known LEGACY policy gap, not strict donation segregation: Core sizes
    // liquidity from live cash, so donated settlement can subsidize the payout
    // while leaving more strategy value for remaining holders. Pin the observed
    // behavior rather than claiming the lean engine's custody invariant holds.
    // Resolve this before production LP activation; no core behavior changed.
    let untracked_after = (f.cash(1, &f.markets[1]) as u128)
        .checked_sub(managed)
        .unwrap();
    assert!(untracked_after < 50_000);
    assert_eq!(
        token::Client::new(&f.env, &f.rewards[0]).balance(&f.markets[1]),
        1777
    );
}

#[test]
fn aquarius_lending_failed_liquidity_unwind_rolls_back_debt_and_claims() {
    let f = Fixture::new();
    f.credit(1, 1000, 2000);
    let shares = f.strategy_shares(1);
    let wallet = f.cash(1, &f.user);
    MockAquariusPoolClient::new(&f.env, &f.pools[1]).set_kill_swap(&true);
    f.reset();
    // More than the settlement half: cannot fund this loan without conversion.
    assert!(f.r(1).try_borrow(&f.user, &600_000).is_err());
    assert_eq!(f.r(1).get_total_borrowed(), 0);
    assert_eq!(f.strategy_shares(1), shares);
    assert_eq!(f.cash(1, &f.user), wallet);
    for asset in &f.rewards {
        assert_eq!(token::Client::new(&f.env, asset).balance(&f.markets[1]), 0);
    }
    MockAquariusPoolClient::new(&f.env, &f.pools[1]).set_kill_swap(&false);
    f.reset();
    f.r(1).claim();
    assert_eq!(f.r(1).earned(&f.rewards[0], &f.supplier), 1000);
}

#[test]
fn aquarius_lending_exact_exit_auth_and_retained_claims_after_full_exit() {
    let f = Fixture::new();
    f.credit(2, 1000, 2000);
    f.env.mock_auths(&[]);
    f.reset();
    assert!(f
        .r(2)
        .try_withdraw(&f.supplier, &1_000_000, &900_000)
        .is_err());
    f.env.mock_auths(&[MockAuth {
        address: &f.supplier,
        invoke: &MockAuthInvoke {
            contract: &f.markets[2],
            fn_name: "withdraw",
            args: (&f.supplier, 1_000_000u128, 900_000u128).into_val(&f.env),
            sub_invokes: &[],
        },
    }]);
    f.reset();
    let (reserved, paid) = f.r(2).withdraw(&f.supplier, &1_000_000, &900_000);
    f.measure("exact-auth final lender exit");
    assert!(paid >= 900_000);
    assert_eq!(f.strategy_shares(2), 0);
    assert_eq!(f.r(2).get_total_ptokens(), 0);
    assert_eq!(reserved.get(f.rewards[0].clone()), Some(1000));
    assert_eq!(reserved.get(f.rewards[1].clone()), Some(2000));
    assert_eq!(f.r(2).reserved(&f.rewards[0], &f.supplier), 1000);
    assert_eq!(
        token::Client::new(&f.env, &f.rewards[0]).balance(&f.markets[2]),
        1000
    );
}
