use soroban_sdk::{
    contracttype, Address, BytesN, Env, IntoVal, InvokeError, Symbol, Vec,
};

use crate::constants::*;
use crate::helpers::{bump_core_ttl, bump_market_ttl};

#[soroban_sdk::contractclient(name = "ReceiptVaultClient")]
pub trait ReceiptVaultContract {
    fn deposit(env: Env, user: Address, amount: u128);
    fn withdraw(env: Env, user: Address, ptoken_amount: u128);
    fn borrow(env: Env, user: Address, amount: u128);
    fn repay(env: Env, user: Address, amount: u128);
    fn get_underlying_token(env: Env) -> Address;
    fn get_exchange_rate(env: Env) -> u128;
    fn get_ptoken_balance(env: Env, user: Address) -> u128;
    fn get_user_borrow_balance(env: Env, user: Address) -> u128;
}

#[soroban_sdk::contractclient(name = "PeridottrollerClient")]
pub trait PeridottrollerContract {
    fn account_liquidity(env: Env, user: Address) -> (u128, u128);
    fn get_price_usd(env: Env, token: Address) -> Option<(u128, u128)>;
    fn cache_price(env: Env, token: Address) -> Option<(u128, u128)>;
    fn enter_market(env: Env, user: Address, market: Address);
    fn get_market_cf(env: Env, market: Address) -> u128;
    fn liquidate(
        env: Env,
        borrower: Address,
        repay_market: Address,
        collateral_market: Address,
        repay_amount: u128,
        liquidator: Address,
    );
    fn liquidate_for_margin(
        env: Env,
        controller: Address,
        borrower: Address,
        repay_market: Address,
        collateral_market: Address,
        repay_amount: u128,
        liquidator: Address,
    );
}

#[soroban_sdk::contractclient(name = "SwapAdapterClient")]
pub trait SwapAdapterContract {
    fn is_pool_allowed(env: Env, pool: Address) -> bool;

    fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        token_in: Address,
        amount: u128,
        amount_with_slippage: u128,
    ) -> u128;
}

#[contracttype]
pub enum DataKey {
    Admin,
    Peridottroller,
    SwapAdapter,
    MaxLeverage,
    MaxSlippageBps,
    Market(Address),
    PositionCounter,
    Position(u64),
    UserPositions(Address),
    DebtSharesTotal(Address, Address), // (user, debt_asset)
    Initialized,
    PositionInitialLockMarket(u64),
    PositionInitialLockPtokens(u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PositionSide {
    Long,
    Short,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PositionStatus {
    Open,
    Closed,
    Liquidated,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub owner: Address,
    pub side: PositionSide,
    pub collateral_asset: Address,
    pub debt_asset: Address,
    pub collateral_ptokens: u128,
    pub debt_shares: u128,
    pub entry_price_scaled: u128,
    pub opened_at: u64,
    pub status: PositionStatus,
}

pub fn require_admin(env: &Env, admin: &Address) {
    let stored: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");
    bump_core_ttl(env);
    if stored != *admin {
        panic!("not admin");
    }
    admin.require_auth();
}

pub fn get_market(env: &Env, asset: &Address) -> Address {
    bump_market_ttl(env, asset);
    env.storage()
        .persistent()
        .get(&DataKey::Market(asset.clone()))
        .expect("unsupported market")
}

pub fn get_peridottroller(env: &Env) -> PeridottrollerClient<'_> {
    bump_core_ttl(env);
    let addr: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Peridottroller)
        .expect("peridottroller not set");
    PeridottrollerClient::new(env, &addr)
}

pub fn get_swap_adapter(env: &Env) -> Address {
    bump_core_ttl(env);
    env.storage()
        .persistent()
        .get(&DataKey::SwapAdapter)
        .expect("swap adapter not set")
}

pub fn get_max_leverage(env: &Env) -> u128 {
    bump_core_ttl(env);
    env.storage()
        .persistent()
        .get(&DataKey::MaxLeverage)
        .unwrap_or(1u128)
}

pub fn get_max_slippage_bps(env: &Env) -> u128 {
    bump_core_ttl(env);
    env.storage()
        .persistent()
        .get(&DataKey::MaxSlippageBps)
        .unwrap_or(DEFAULT_MAX_SLIPPAGE_BPS)
}

pub fn get_price_usd(env: &Env, asset: &Address) -> (u128, u128) {
    let peridottroller_addr: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Peridottroller)
        .expect("peridottroller not set");
    let _ = env.try_invoke_contract::<Option<(u128, u128)>, InvokeError>(
        &peridottroller_addr,
        &Symbol::new(env, "cache_price"),
        (asset.clone(),).into_val(env),
    );
    let peridottroller = get_peridottroller(env);
    let (num, den) = peridottroller
        .get_price_usd(asset)
        .expect("price unavailable");
    if num == 0 || den == 0 {
        panic!("invalid price");
    }
    (num, den)
}
