use soroban_sdk::{contracttype, Address, Env, Vec};

use crate::helpers::{bump_core_ttl, bump_market_ttl};

#[soroban_sdk::contractclient(name = "ReceiptVaultClient")]
pub trait ReceiptVaultContract {
    fn deposit(env: Env, user: Address, amount: u128);
    fn withdraw(env: Env, user: Address, ptoken_amount: u128);
    fn borrow(env: Env, user: Address, amount: u128);
    fn repay(env: Env, user: Address, amount: u128);
    fn get_exchange_rate(env: Env) -> u128;
    fn get_ptoken_balance(env: Env, user: Address) -> u128;
    fn get_user_borrow_balance(env: Env, user: Address) -> u128;
}

#[soroban_sdk::contractclient(name = "PeridottrollerClient")]
pub trait PeridottrollerContract {
    fn account_liquidity(env: Env, user: Address) -> (u128, u128);
    fn get_price_usd(env: Env, token: Address) -> Option<(u128, u128)>;
    fn liquidate(
        env: Env,
        liquidator: Address,
        borrower: Address,
        repay_market: Address,
        collateral_market: Address,
        repay_amount: u128,
    );
}

#[soroban_sdk::contractclient(name = "SwapAdapterClient")]
pub trait SwapAdapterContract {
    fn swap_exact_tokens_for_tokens(
        env: Env,
        user: Address,
        amount_in: u128,
        amount_out_min: u128,
        path: Vec<Address>,
        deadline: u64,
    ) -> u128;
}

#[contracttype]
pub enum DataKey {
    Admin,
    Peridottroller,
    SwapAdapter,
    MaxLeverage,
    LiquidationBonus,
    Market(Address),
    PositionCounter,
    Position(u64),
    UserPositions(Address),
    DebtSharesTotal(Address, Address), // (user, debt_asset)
    Initialized,
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
    let addr: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Peridottroller)
        .expect("peridottroller not set");
    PeridottrollerClient::new(env, &addr)
}

pub fn get_swap_adapter(env: &Env) -> Address {
    env.storage()
        .persistent()
        .get(&DataKey::SwapAdapter)
        .expect("swap adapter not set")
}

pub fn get_max_leverage(env: &Env) -> u128 {
    env.storage()
        .persistent()
        .get(&DataKey::MaxLeverage)
        .unwrap_or(1u128)
}

pub fn get_price_usd(env: &Env, asset: &Address) -> (u128, u128) {
    let peridottroller = get_peridottroller(env);
    peridottroller
        .get_price_usd(asset)
        .expect("price unavailable")
}
