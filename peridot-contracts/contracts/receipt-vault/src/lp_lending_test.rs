//! Native lending integration boundary, NOT an LP deployment candidate.
//! Real receipts/controllers/JRM/SACs, idle principal and controlled funded
//! reward receipts. No Aquarius claims/quotes, production hooks or outage proof.
extern crate std;
use crate::{
    reward_backing as backing, reward_ledger as ledger, reward_share_hooks as hooks,
    test::{OracleAsset, OraclePriceData},
    ReceiptVault as Core, SeizeContext,
};
use jump_rate_model::{JumpRateModel, JumpRateModelClient};
use simple_peridottroller::{SimplePeridottroller, SimplePeridottrollerClient};
use soroban_sdk::testutils::cost_estimate::NetworkInvocationResourceLimits;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
    token, vec, Address, Env, IntoVal,
};
use stellar_tokens::fungible::Base as TokenBase;

#[contract]
struct Prices;
#[contractimpl]
impl Prices {
    pub fn set(env: Env, asset: Address, price: i128) {
        env.storage().instance().set(&asset, &price);
    }
    pub fn decimals() -> u32 {
        7
    }
    pub fn resolution() -> u32 {
        60
    }
    pub fn lastprice(env: Env, asset: OracleAsset) -> Option<OraclePriceData> {
        let OracleAsset::Stellar(asset) = asset else {
            return None;
        };
        env.storage()
            .instance()
            .get(&asset)
            .map(|price| OraclePriceData {
                price,
                timestamp: env.ledger().timestamp(),
            })
    }
}

#[contract]
struct Lending;
#[contractimpl]
impl Lending {
    pub fn initialize(env: Env, asset: Address, admin: Address) {
        Core::initialize(env, asset, 0, 0, admin);
    }
    pub fn model(env: Env, model: Address) {
        Core::set_interest_model(env, model);
    }
    pub fn controller(env: Env, controller: Address) {
        Core::set_peridottroller(env, controller);
    }
    pub fn deposit(env: Env, user: Address, amount: u128) {
        // Bootstrap only. No uncheckpointed deposit after the reward boundary.
        assert!(!env
            .storage()
            .instance()
            .has(&ledger::LedgerKey::HybridRegistryInitialized));
        Core::deposit(env, user, amount);
    }
    pub fn rewards(env: Env, a: Address, b: Address) {
        backing::initialize(&env); // existing admin auth
        let supply = Core::get_total_ptokens(env.clone());
        ledger::register(&env, &a, supply);
        ledger::register(&env, &b, supply);
    }
    pub fn receive(env: Env, payer: Address, asset: Address, amount: i128) {
        payer.require_auth();
        let before = ledger::begin_receive(&env, &asset);
        token::Client::new(&env, &asset).transfer(&payer, &env.current_contract_address(), &amount);
        ledger::finish_receive(&env, before, amount.try_into().unwrap());
    }
    pub fn earned(env: Env, asset: Address, user: Address) -> u128 {
        let weight = TokenBase::balance(&env, &user).try_into().unwrap();
        ledger::checkpoint(&env, &asset, &user, weight).raw_scaled / ledger::SCALE
    }
    pub fn borrow(env: Env, user: Address, amount: u128) {
        Core::borrow(env, user, amount);
    }
    pub fn repay(env: Env, user: Address, amount: u128) {
        Core::repay(env, user, amount);
    }
    pub fn repay_on_behalf(env: Env, payer: Address, borrower: Address, amount: u128) {
        Core::repay_on_behalf(env, payer, borrower, amount);
    }
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let s = hooks::begin(&env, vec![&env, from.clone(), to.clone()]);
        Core::transfer(env.clone(), from, to.into(), amount);
        hooks::finish(&env, s);
    }
    pub fn bad_transfer(env: Env, from: Address, to: Address, amount: i128) {
        // Deliberately faulty TEST-ONLY integration to prove finish rollback.
        let s = hooks::begin(&env, vec![&env, from.clone()]);
        Core::transfer(env.clone(), from, to.into(), amount);
        hooks::finish(&env, s);
    }
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        let s = hooks::begin(&env, vec![&env, from.clone(), to.clone()]);
        Core::transfer_from(env.clone(), spender, from, to, amount);
        hooks::finish(&env, s);
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
        let mut owners = vec![&env, borrower.clone(), liquidator.clone()];
        if let Some(ref c) = ctx {
            if let Some(ref recipient) = c.fee_recipient {
                owners.push_back(recipient.clone());
            }
        }
        let s = hooks::begin(&env, owners);
        Core::seize(env.clone(), borrower, liquidator, amount, ctx);
        hooks::finish(&env, s);
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
    oracle: Address,
    lp: Address,
    core: Address,
    assets: [Address; 3],
    markets: [Address; 3],
    core_markets: [Address; 2],
    rewards: [Address; 2],
}
impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        // Explicit RESEARCH envelope, not a fresh network-limit assertion.
        // Three entered markets exceed SDK25's bundled 100-entry fixture.
        // Keep CPU/memory/write limits ON; no disable_resource_limits call.
        let mut limits = soroban_env_host::InvocationResourceLimits::mainnet();
        limits.ledger_entries = 200;
        env.cost_estimate().enforce_resource_limits(limits);
        env.mock_all_auths();
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
        let oracle = env.register(Prices, ());
        let lp = env.register(SimplePeridottroller, ());
        let core = env.register(SimplePeridottroller, ());
        for c in [&lp, &core] {
            let c = SimplePeridottrollerClient::new(&env, c);
            c.initialize(&admin);
            c.set_oracle(&oracle);
        }
        let assets = core::array::from_fn(|_| {
            env.register_stellar_asset_contract_v2(admin.clone())
                .address()
        });
        let rewards = core::array::from_fn(|_| {
            env.register_stellar_asset_contract_v2(admin.clone())
                .address()
        });
        for asset in &assets {
            PricesClient::new(&env, &oracle).set(asset, &10_000_000);
        }
        let model = env.register(JumpRateModel, ());
        // TEST parameters, not proposed production risk policy.
        JumpRateModelClient::new(&env, &model)
            .initialize(&10_000, &180_000, &4_000_000, &800_000, &admin);
        let new_market = |asset: &Address, group: &Address| {
            let id = env.register(Lending, ());
            let r = LendingClient::new(&env, &id);
            r.initialize(asset, &admin);
            r.model(&model);
            let c = SimplePeridottrollerClient::new(&env, group);
            c.add_market(&id);
            c.set_market_cf(&id, &700_000);
            r.controller(group);
            id
        };
        let markets = core::array::from_fn(|i| new_market(&assets[i], &lp));
        let core_markets = core::array::from_fn(|i| new_market(&assets[i], &core));
        for asset in &assets {
            let sac = token::StellarAssetClient::new(&env, asset);
            sac.mint(&user, &10_000_000);
            sac.mint(&supplier, &10_000_000);
        }
        for (i, market) in markets.iter().enumerate() {
            let owner = if i == 0 { &user } else { &supplier };
            LendingClient::new(&env, market).deposit(owner, &1_000_000);
        }
        LendingClient::new(&env, &core_markets[1]).deposit(&supplier, &1_000_000);
        for r in markets.iter().chain(core_markets.iter()) {
            LendingClient::new(&env, r).rewards(&rewards[0], &rewards[1]);
        }
        for reward in &rewards {
            token::StellarAssetClient::new(&env, reward).mint(&supplier, &10_000_000);
        }
        for market in &markets {
            SimplePeridottrollerClient::new(&env, &lp).enter_market(&user, market);
        }
        Self {
            env,
            admin,
            user,
            supplier,
            oracle,
            lp,
            core,
            assets,
            markets,
            core_markets,
            rewards,
        }
    }
    fn r(&self, i: usize) -> LendingClient<'_> {
        LendingClient::new(&self.env, &self.markets[i])
    }
    fn credit(&self, i: usize, amount: i128) {
        for reward in &self.rewards {
            self.r(i).receive(&self.supplier, reward, &amount);
        }
    }
    fn reset(&self) {
        self.env.cost_estimate().budget().reset_unlimited();
    }
    fn measured(&self, label: &str) {
        let r = self.env.cost_estimate().resources();
        let entries = r.disk_read_entries + r.memory_read_entries + r.write_entries;
        assert!(entries <= 200 && r.instructions <= 100_000_000);
        std::println!(
            "native-lending {label}: {entries} entries, {} writes, {} CPU",
            r.write_entries,
            r.instructions
        );
    }
}

#[test]
fn lp_lending_xlm_collateral_supports_both_stables_and_repayment_preserves_nav() {
    let f = Fixture::new();
    f.credit(0, 1000);
    f.credit(1, 1000);
    f.credit(2, 1000);
    for i in [1, 2] {
        let before = f.r(i).nav();
        let shares = f.r(i).get_total_ptokens();
        let cash_before = token::Client::new(&f.env, &f.assets[i]).balance(&f.user);
        f.reset();
        f.r(i).borrow(&f.user, &200_000);
        f.measured("borrow");
        assert_eq!(f.r(i).get_user_borrow_balance(&f.user), 200_000);
        assert_eq!(
            token::Client::new(&f.env, &f.assets[i]).balance(&f.user) - cash_before,
            200_000
        );
        assert_eq!(f.r(i).nav(), before); // cash became a debt receivable, not a loss
        assert_eq!(f.r(i).get_total_ptokens(), shares);
        assert_eq!(f.r(i).get_ptoken_balance(&f.user), 0);
        for reward in &f.rewards {
            assert_eq!(f.r(i).earned(reward, &f.user), 0);
            assert_eq!(f.r(i).earned(reward, &f.supplier), 1000);
        }
    }
    f.reset();
    assert!(f.r(1).try_borrow(&f.user, &300_001).is_err());
    assert_eq!(f.r(1).get_user_borrow_balance(&f.user), 200_000);
    for i in [1, 2] {
        f.reset();
        f.r(i).repay(&f.user, &200_000);
        assert_eq!(f.r(i).get_user_borrow_balance(&f.user), 0);
    }
    for reward in &f.rewards {
        assert_eq!(f.r(0).earned(reward, &f.user), 1000);
    }
}

#[test]
fn lp_lending_identical_underlying_in_another_group_is_not_collateral() {
    let f = Fixture::new();
    let outsider = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.assets[0]).mint(&outsider, &1_000_000);
    // The alternate group's receipt uses the SAME XLM SAC. Native fixture-only
    // balance bootstrap via real core deposit, before any rewards are received.
    f.env.as_contract(&f.core_markets[0], || {
        Core::deposit(f.env.clone(), outsider.clone(), 1_000_000);
        for reward in &f.rewards {
            ledger::change_weight(&f.env, reward, &outsider, 0, 1_000_000);
        }
    });
    let core = SimplePeridottrollerClient::new(&f.env, &f.core);
    core.enter_market(&outsider, &f.core_markets[0]);
    for market in &f.markets {
        SimplePeridottrollerClient::new(&f.env, &f.lp).enter_market(&outsider, market);
    }
    for i in [1, 2] {
        f.reset();
        assert!(f.r(i).try_borrow(&outsider, &1).is_err());
        assert_eq!(f.r(i).get_user_borrow_balance(&outsider), 0);
    }
    // Isolation is symmetric: LP XLM cannot collateralize the core USDC market.
    for market in &f.core_markets {
        core.enter_market(&f.user, market);
    }
    f.reset();
    assert!(LendingClient::new(&f.env, &f.core_markets[1])
        .try_borrow(&f.user, &1)
        .is_err());
    assert!(SimplePeridottrollerClient::new(&f.env, &f.lp)
        .try_enter_market(&outsider, &f.core_markets[0])
        .is_err());
}

#[test]
fn lp_lending_liquidation_moves_shares_not_previously_earned_rewards() {
    let f = Fixture::new();
    let liquidator = f.supplier.clone();
    f.reset();
    f.r(1).borrow(&f.user, &600_000);
    f.credit(0, 1000);
    PricesClient::new(&f.env, &f.oracle).set(&f.assets[0], &5_000_000);
    SimplePeridottrollerClient::new(&f.env, &f.lp).cache_price(&f.assets[0]);
    token::Client::new(&f.env, &f.assets[1]).approve(&liquidator, &f.markets[1], &100_000, &1000);
    f.reset();
    f.env.mock_auths(&[]);
    SimplePeridottrollerClient::new(&f.env, &f.lp)
        .mock_auths(&[MockAuth {
            address: &liquidator,
            invoke: &MockAuthInvoke {
                contract: &f.lp,
                fn_name: "liquidate",
                args: (
                    f.user.clone(),
                    f.markets[1].clone(),
                    f.markets[0].clone(),
                    100_000u128,
                    liquidator.clone(),
                )
                    .into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .liquidate(&f.user, &f.markets[1], &f.markets[0], &100_000, &liquidator);
    f.measured("liquidate");
    f.env.mock_all_auths();
    let moved = f.r(0).get_ptoken_balance(&liquidator);
    assert!(moved > 0);
    assert_eq!(f.r(0).get_total_ptokens(), 1_000_000);
    assert_eq!(f.r(1).get_user_borrow_balance(&f.user), 500_000);
    for reward in &f.rewards {
        assert_eq!(f.r(0).earned(reward, &f.user), 1000);
        assert_eq!(f.r(0).earned(reward, &liquidator), 0);
    }
    f.credit(0, 1_000_000);
    for reward in &f.rewards {
        assert_eq!(f.r(0).earned(reward, &liquidator), moved);
        assert_eq!(f.r(0).earned(reward, &f.user), 1000 + 1_000_000 - moved);
    }
}

#[test]
fn lp_lending_unsafe_delegated_transfer_rolls_back_allowance_and_rewards() {
    let f = Fixture::new();
    f.credit(0, 1000);
    f.reset();
    f.r(1).borrow(&f.user, &600_000);
    f.r(0).approve(&f.user, &f.supplier, &500_000, &1000);
    f.reset();
    assert!(f
        .r(0)
        .try_transfer_from(&f.supplier, &f.user, &f.supplier, &500_000)
        .is_err());
    assert_eq!(f.r(0).get_ptoken_balance(&f.user), 1_000_000);
    f.env.as_contract(&f.markets[0], || {
        assert_eq!(TokenBase::allowance(&f.env, &f.user, &f.supplier), 500_000);
    });
    for reward in &f.rewards {
        assert_eq!(f.r(0).earned(reward, &f.user), 1000);
        assert_eq!(f.r(0).earned(reward, &f.supplier), 0);
    }
}

#[test]
fn lp_lending_delegated_transfer_and_self_transfer_keep_old_rewards() {
    let f = Fixture::new();
    f.credit(0, 1000);
    f.r(0).approve(&f.user, &f.supplier, &400_000, &1000);
    f.reset();
    f.r(0)
        .transfer_from(&f.supplier, &f.user, &f.supplier, &400_000);
    f.measured("transfer_from");
    f.r(0).transfer(&f.user, &f.user, &1);
    for reward in &f.rewards {
        assert_eq!(f.r(0).earned(reward, &f.user), 1000);
        assert_eq!(f.r(0).earned(reward, &f.supplier), 0);
    }
    f.credit(0, 1000);
    for reward in &f.rewards {
        assert_eq!(f.r(0).earned(reward, &f.user), 1600);
        assert_eq!(f.r(0).earned(reward, &f.supplier), 400);
    }
}

#[test]
fn lp_lending_exact_borrower_auth_and_unauthorized_seizure() {
    let f = Fixture::new();
    f.env.mock_auths(&[]);
    assert!(f.r(1).try_borrow(&f.user, &200_000).is_err());
    f.r(1)
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.markets[1],
                fn_name: "borrow",
                args: (f.user.clone(), 200_000u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .borrow(&f.user, &200_000);
    assert_eq!(f.r(1).get_user_borrow_balance(&f.user), 200_000);
    f.env.mock_auths(&[]);
    let ctx = SeizeContext {
        liquidity: 0,
        shortfall: 1,
        max_redeem_ptokens: 1,
        seize_ptokens: 1,
        fee_recipient: None,
        fee_ptokens: 0,
        expires_at: 1005,
    };
    assert!(f
        .r(0)
        .try_seize(&f.user, &f.supplier, &1, &Some(ctx))
        .is_err());
    assert_eq!(f.r(0).get_ptoken_balance(&f.user), 1_000_000);
}

#[test]
fn lp_lending_liquidation_fee_recipient_can_equal_liquidator() {
    for same in [true, false] {
        let f = Fixture::new();
        let fee_recipient = if same {
            f.supplier.clone()
        } else {
            Address::generate(&f.env)
        };
        let c = SimplePeridottrollerClient::new(&f.env, &f.lp);
        c.set_reserve_recipient(&fee_recipient);
        c.set_liquidation_fee(&100_000);
        f.reset();
        f.r(1).borrow(&f.user, &600_000);
        f.credit(0, 1000);
        PricesClient::new(&f.env, &f.oracle).set(&f.assets[0], &5_000_000);
        c.cache_price(&f.assets[0]);
        token::Client::new(&f.env, &f.assets[1]).approve(
            &f.supplier,
            &f.markets[1],
            &100_000,
            &1000,
        );
        f.reset();
        c.liquidate(&f.user, &f.markets[1], &f.markets[0], &100_000, &f.supplier);
        f.measured(if same {
            "liquidate-duplicate-fee-owner"
        } else {
            "liquidate-three-owners"
        });
        let liquidator_shares = f.r(0).get_ptoken_balance(&f.supplier);
        let fee_shares = if same {
            0
        } else {
            f.r(0).get_ptoken_balance(&fee_recipient)
        };
        assert!(liquidator_shares > 0 && (same || fee_shares > 0));
        assert_eq!(
            f.r(0).get_ptoken_balance(&f.user) + liquidator_shares + fee_shares,
            1_000_000
        );
        for reward in &f.rewards {
            assert_eq!(f.r(0).earned(reward, &f.user), 1000);
            assert_eq!(f.r(0).earned(reward, &f.supplier), 0);
            assert_eq!(f.r(0).earned(reward, &fee_recipient), 0);
        }
    }
}

#[test]
fn lp_lending_finish_rejects_omitted_recipient_and_rolls_back_shares() {
    let f = Fixture::new();
    f.credit(0, 1000);
    f.reset();
    assert!(f.r(0).try_bad_transfer(&f.user, &f.supplier, &100).is_err());
    assert_eq!(f.r(0).get_ptoken_balance(&f.user), 1_000_000);
    assert_eq!(f.r(0).get_ptoken_balance(&f.supplier), 0);
    for reward in &f.rewards {
        assert_eq!(f.r(0).earned(reward, &f.user), 1000);
    }
}

#[test]
fn lp_lending_interest_changes_nav_not_reward_weights() {
    let f = Fixture::new();
    f.credit(1, 1000);
    f.reset();
    f.r(1).borrow(&f.user, &200_000);
    f.env.ledger().with_mut(|l| l.timestamp += 31_536_000);
    f.reset();
    f.r(1).update_interest();
    let debt = f.r(1).get_user_borrow_balance(&f.user);
    assert!(debt > 200_000);
    assert_eq!(f.r(1).get_total_ptokens(), 1_000_000);
    assert_eq!(f.r(1).nav(), 800_000 + f.r(1).get_total_borrowed());
    for reward in &f.rewards {
        assert_eq!(f.r(1).earned(reward, &f.supplier), 1000);
        assert_eq!(f.r(1).earned(reward, &f.user), 0);
    }
    f.reset();
    f.r(1).repay(&f.user, &debt);
    assert_eq!(f.r(1).get_user_borrow_balance(&f.user), 0);
}

#[test]
fn lp_lending_share_hooks_checkpoint_four_retained_streams() {
    let f = Fixture::new();
    let mut rewards = vec![&f.env, f.rewards[0].clone(), f.rewards[1].clone()];
    for _ in 0..2 {
        let reward = f
            .env
            .register_stellar_asset_contract_v2(f.admin.clone())
            .address();
        token::StellarAssetClient::new(&f.env, &reward).mint(&f.supplier, &1000);
        f.env.as_contract(&f.markets[0], || {
            ledger::register(&f.env, &reward, 1_000_000)
        });
        rewards.push_back(reward);
    }
    for reward in rewards.iter() {
        f.r(0).receive(&f.supplier, &reward, &1000);
    }
    f.reset();
    f.r(0).transfer(&f.user, &f.supplier, &200_000);
    f.measured("transfer-four-streams");
    for reward in rewards.iter() {
        assert_eq!(f.r(0).earned(&reward, &f.user), 1000);
        assert_eq!(f.r(0).earned(&reward, &f.supplier), 0);
    }
}

#[test]
fn lp_lending_missing_stream_and_escrow_target_fail_closed() {
    let f = Fixture::new();
    f.credit(0, 1000);
    f.reset();
    assert!(f.r(0).try_transfer(&f.user, &f.markets[0], &1).is_err());
    f.env.as_contract(&f.markets[0], || {
        f.env
            .storage()
            .persistent()
            .remove(&ledger::LedgerKey::HybridStream(f.rewards[1].clone()));
    });
    f.reset();
    assert!(f.r(0).try_transfer(&f.user, &f.supplier, &1).is_err());
    assert_eq!(f.r(0).get_ptoken_balance(&f.user), 1_000_000);
    assert_eq!(f.r(0).get_ptoken_balance(&f.supplier), 0);
}
