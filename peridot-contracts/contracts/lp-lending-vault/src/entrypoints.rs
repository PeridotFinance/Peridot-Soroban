use crate::{
    reward_backing as backing, reward_claims as claims, reward_ledger as ledger,
    reward_lending as lending, reward_settlement as settlement, storage::*, Outcome,
    ReceiptVault as Core, SeizeContext,
};
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Bytes, Env, IntoVal, Map, MuxedAddress,
    String, Symbol, Vec,
};

#[contracttype]
#[derive(Clone)]
enum LpKey {
    LpLendingConfig,
}
#[contracttype]
#[derive(Clone)]
struct Config {
    active: bool,
    collateral_factor: u128,
}

fn config(env: &Env) -> Config {
    ensure_initialized(env);
    let value = env
        .storage()
        .instance()
        .get(&LpKey::LpLendingConfig)
        .expect("not an initialized LP lending receipt");
    env.storage().instance().extend_ttl(500_000, 1_000_000);
    value
}
fn setup(env: &Env) {
    assert!(!config(env).active, "LP configuration is sealed");
}
fn ready(env: &Env) {
    assert!(config(env).active, "LP lending not activated");
}
fn admin(env: &Env) {
    Core::get_admin(env.clone()).require_auth();
}
fn call<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    to: &Address,
    method: &str,
    args: Vec<soroban_sdk::Val>,
) -> T {
    env.invoke_contract(to, &Symbol::new(env, method), args)
}
fn zero_controller(env: &Env, controller: &Address) {
    assert_eq!(
        call::<u32>(env, controller, "lp_reward_policy", Vec::new(env)),
        1,
        "fresh zero-PERI LP controller required"
    );
}

#[contract]
pub struct LpLendingVault;
#[contractimpl]
impl LpLendingVault {
    /// Fresh deployments only. Existing pilot state needs a separate migration;
    /// this constructor cannot run on upgrade. Mainnet release remains gated.
    pub fn __constructor(env: Env, asset: Address, admin: Address, stable: bool) {
        let public = env.crypto().sha256(&Bytes::from_slice(
            &env,
            b"Public Global Stellar Network ; September 2015",
        ));
        assert_ne!(
            env.ledger().network_id(),
            public.to_bytes(),
            "LP release gates: Mainnet initialization disabled"
        );
        Core::initialize(env.clone(), asset, 0, 0, admin);
        env.storage().instance().set(
            &LpKey::LpLendingConfig,
            &Config {
                active: false,
                collateral_factor: if stable { 800_000 } else { 500_000 },
            },
        );
    }
    pub fn lp_version(env: Env) -> u32 {
        config(&env);
        1
    }
    pub fn is_active(env: Env) -> bool {
        config(&env).active
    }
    pub fn target_collateral_factor(env: Env) -> u128 {
        config(&env).collateral_factor
    }
    pub fn set_peridottroller(env: Env, controller: Address) {
        setup(&env);
        assert!(
            !env.storage().persistent().has(&DataKey::Peridottroller),
            "controller already bound"
        );
        zero_controller(&env, &controller);
        Core::set_peridottroller(env, controller);
    }
    pub fn set_interest_model(env: Env, model: Address) {
        setup(&env);
        Core::set_interest_model(env, model);
    }
    pub fn set_boosted_vault(env: Env, admin: Address, strategy: Address) {
        setup(&env);
        assert!(
            Core::get_boosted_vault(env.clone()).is_none(),
            "strategy already bound"
        );
        let underlying: Address = call(&env, &strategy, "get_underlying", Vec::new(&env));
        assert_eq!(
            underlying,
            Core::get_underlying_token(env.clone()),
            "strategy asset mismatch"
        );
        Core::set_boosted_vault(env, admin, strategy);
    }
    pub fn initialize_rewards(env: Env) {
        setup(&env);
        backing::initialize(&env);
    }
    pub fn register_reward(env: Env, asset: Address) {
        config(&env);
        claims::register(&env, &asset);
    }
    pub fn activate(env: Env) {
        setup(&env);
        admin(&env);
        let mut cfg = config(&env);
        assert_eq!(
            Core::get_total_ptokens(env.clone()),
            0,
            "fresh activation requires empty supply"
        );
        assert_eq!(
            Core::get_total_borrowed(env.clone()),
            0,
            "fresh activation requires no debt"
        );
        assert_eq!(
            env.storage()
                .persistent()
                .get::<_, u128>(&DataKey::ManagedCash),
            Some(0)
        );
        assert!(
            env.storage().persistent().has(&DataKey::InterestModel),
            "interest model required"
        );
        let controller: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Peridottroller)
            .expect("controller missing");
        zero_controller(&env, &controller);
        let cf: u128 = call(
            &env,
            &controller,
            "get_market_cf",
            (env.current_contract_address(),).into_val(&env),
        );
        assert_eq!(
            cf, cfg.collateral_factor,
            "controller CF differs from approved target"
        );
        let strategy = Core::get_boosted_vault(env.clone()).expect("strategy missing");
        let bound: Option<Address> = call(&env, &strategy, "get_receipt_vault", Vec::new(&env));
        assert_eq!(
            bound,
            Some(env.current_contract_address()),
            "strategy receipt mismatch"
        );
        assert!(
            call::<bool>(&env, &strategy, "hybrid_is_enabled", Vec::new(&env)),
            "legacy harvest must be fenced"
        );
        assert_eq!(
            token::Client::new(&env, &strategy).balance(&env.current_contract_address()),
            0,
            "fresh strategy required"
        );
        claims::check(&env);
        env.storage()
            .persistent()
            .set(&DataKey::CollateralFactorScaled, &cf);
        cfg.active = true;
        env.storage().instance().set(&LpKey::LpLendingConfig, &cfg);
    }
    pub fn deposit(env: Env, user: Address, amount: u128) {
        ready(&env);
        lending::deposit(&env, &user, amount);
    }
    pub fn withdraw(env: Env, user: Address, ptoken_amount: u128) {
        ready(&env);
        lending::withdraw(&env, &user, ptoken_amount, 1);
    }
    pub fn withdraw_with_minimum(
        env: Env,
        user: Address,
        ptoken_amount: u128,
        minimum: u128,
    ) -> (Map<Address, u128>, u128) {
        ready(&env);
        lending::withdraw(&env, &user, ptoken_amount, minimum)
    }
    pub fn borrow(env: Env, user: Address, amount: u128) {
        ready(&env);
        lending::borrow(&env, &user, amount);
    }
    pub fn repay(env: Env, user: Address, amount: u128) {
        ready(&env);
        Core::repay(env, user, amount);
    }
    pub fn repay_max(env: Env, user: Address) {
        ready(&env);
        Core::repay_max(env, user);
    }
    pub fn repay_on_behalf(env: Env, liquidator: Address, borrower: Address, amount: u128) {
        ready(&env);
        Core::repay_on_behalf(env, liquidator, borrower, amount);
    }
    pub fn seize(
        env: Env,
        borrower: Address,
        liquidator: Address,
        ptoken_amount: u128,
        ctx: Option<SeizeContext>,
    ) {
        ready(&env);
        lending::seize(&env, &borrower, &liquidator, ptoken_amount, ctx);
    }
    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        ready(&env);
        lending::transfer_muxed(&env, &from, &to, amount);
    }
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        ready(&env);
        lending::transfer_from(&env, &spender, &from, &to, amount);
    }
    pub fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) {
        ready(&env);
        Core::approve(env, from, spender, amount, expiration_ledger);
    }
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        config(&env);
        Core::allowance(env, from, spender)
    }
    pub fn compound(env: Env, asset: Address) -> Outcome {
        ready(&env);
        settlement::compound(&env, &asset)
    }
    pub fn recycle(env: Env, asset: Address) -> Outcome {
        ready(&env);
        settlement::recycle(&env, &asset)
    }
    pub fn claim_pool_rewards(env: Env) {
        ready(&env);
        claims::claim(&env);
    }
    pub fn payout_rewards(env: Env, owner: Address, minimum: u128) -> Outcome {
        ready(&env);
        lending::redeem_rewards(&env, &owner, minimum)
    }
    pub fn settle_reserved(
        env: Env,
        asset: Address,
        owner: Address,
        raw: u128,
        minimum: u128,
    ) -> Outcome {
        ready(&env);
        settlement::settle_reserved(&env, &asset, &owner, raw, minimum)
    }
    pub fn reward_reserved(env: Env, asset: Address, owner: Address) -> u128 {
        ready(&env);
        ledger::checkpoint(
            &env,
            &asset,
            &owner,
            Core::get_ptoken_balance(env.clone(), owner.clone()),
        )
        .reserved
    }
    pub fn reward_earned(env: Env, asset: Address, owner: Address) -> u128 {
        ready(&env);
        ledger::checkpoint(
            &env,
            &asset,
            &owner,
            Core::get_ptoken_balance(env.clone(), owner.clone()),
        )
        .raw_scaled
            / ledger::SCALE
    }
    pub fn rebalance_idle_cash(env: Env, admin: Address) {
        ready(&env);
        lending::reinvest(&env, &admin);
    }
    pub fn set_idle_cash_buffer_bps(env: Env, admin: Address, bps: u32) {
        config(&env);
        Core::set_idle_cash_buffer_bps(env, admin, bps);
    }
    pub fn set_supply_cap(env: Env, cap: u128) {
        config(&env);
        Core::set_supply_cap(env, cap);
    }
    pub fn set_borrow_cap(env: Env, cap: u128) {
        config(&env);
        Core::set_borrow_cap(env, cap);
    }
    pub fn refresh_boosted_underlying(env: Env) {
        config(&env);
        Core::refresh_boosted_underlying(env);
    }
    pub fn update_interest(env: Env) {
        config(&env);
        Core::update_interest(env);
    }
    pub fn get_admin(env: Env) -> Address {
        config(&env);
        Core::get_admin(env)
    }
    pub fn set_admin(env: Env, new_admin: Address) {
        config(&env);
        Core::set_admin(env, new_admin);
    }
    pub fn accept_admin(env: Env) {
        config(&env);
        Core::accept_admin(env);
    }
    pub fn propose_upgrade_wasm(env: Env, hash: soroban_sdk::BytesN<32>) {
        config(&env);
        Core::propose_upgrade_wasm(env, hash);
    }
    pub fn upgrade_wasm(env: Env, hash: soroban_sdk::BytesN<32>) {
        config(&env);
        Core::upgrade_wasm(env, hash);
    }
    pub fn get_underlying_token(env: Env) -> Address {
        config(&env);
        Core::get_underlying_token(env)
    }
    pub fn get_boosted_vault(env: Env) -> Option<Address> {
        config(&env);
        Core::get_boosted_vault(env)
    }
    pub fn get_account_snapshot(env: Env, user: Address) -> (u128, u128, u128, Address) {
        config(&env);
        Core::get_account_snapshot(env, user)
    }
    pub fn get_user_balance(env: Env, user: Address) -> u128 {
        config(&env);
        Core::get_user_balance(env, user)
    }
    pub fn get_ptoken_balance(env: Env, user: Address) -> u128 {
        config(&env);
        Core::get_ptoken_balance(env, user)
    }
    pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {
        config(&env);
        Core::get_user_borrow_balance(env, user)
    }
    pub fn get_total_ptokens(env: Env) -> u128 {
        config(&env);
        Core::get_total_ptokens(env)
    }
    pub fn get_total_borrowed(env: Env) -> u128 {
        config(&env);
        Core::get_total_borrowed(env)
    }
    pub fn get_total_underlying(env: Env) -> u128 {
        config(&env);
        Core::get_total_underlying(env)
    }
    pub fn get_exchange_rate(env: Env) -> u128 {
        config(&env);
        Core::get_exchange_rate(env)
    }
    pub fn balance(env: Env, id: Address) -> i128 {
        config(&env);
        Core::balance(env, id)
    }
    pub fn total_supply(env: Env) -> i128 {
        config(&env);
        Core::total_supply(env)
    }
    pub fn decimals(env: Env) -> u32 {
        config(&env);
        Core::decimals(env)
    }
    pub fn name(env: Env) -> String {
        config(&env);
        Core::name(env)
    }
    pub fn symbol(env: Env) -> String {
        config(&env);
        Core::symbol(env)
    }
    pub fn bump_ttl(env: Env) {
        config(&env);
        Core::bump_ttl(env);
    }
}
