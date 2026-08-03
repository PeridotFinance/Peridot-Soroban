use soroban_sdk::{contracttype, Address, BytesN, Env, IntoVal, InvokeError, Symbol, Vec};

use crate::constants::*;
use crate::helpers::{bump_core_ttl, bump_market_ttl};

#[soroban_sdk::contractclient(name = "ReceiptVaultClient")]
pub trait ReceiptVaultContract {
    fn deposit(env: Env, user: Address, amount: u128);
    fn withdraw(env: Env, user: Address, ptoken_amount: u128);
    fn transfer(env: Env, from: Address, to: Address, amount: i128);
    fn transfer_from(env: Env, spender: Address, owner: Address, to: Address, amount: i128);
    fn borrow(env: Env, user: Address, amount: u128);
    fn repay(env: Env, user: Address, amount: u128);
    fn init_margin_borrow_state(env: Env, position_id: u64);
    fn borrow_for_margin(env: Env, position_id: u64, receiver: Address, amount: u128);
    fn borrow_for_margin_to_controller(env: Env, position_id: u64, amount: u128);
    fn repay_for_margin(env: Env, position_id: u64, payer: Address, amount: u128);
    fn repay_full_for_margin(env: Env, position_id: u64, payer: Address, max_amount: u128) -> u128;
    fn absorb_margin_bad_debt(env: Env, position_id: u64) -> u128;
    fn update_interest(env: Env);
    fn get_underlying_token(env: Env) -> Address;
    fn get_exchange_rate(env: Env) -> u128;
    fn get_ptoken_balance(env: Env, user: Address) -> u128;
    fn get_user_borrow_balance(env: Env, user: Address) -> u128;
    fn get_margin_borrow_balance(env: Env, position_id: u64) -> u128;
}

#[soroban_sdk::contractclient(name = "AquariusPoolClient")]
pub trait AquariusPoolContract {
    fn estimate_swap(env: Env, in_idx: u32, out_idx: u32, amount_in: u128) -> u128;
    fn swap(
        env: Env,
        user: Address,
        in_idx: u32,
        out_idx: u32,
        amount_in: u128,
        amount_out_min: u128,
    ) -> u128;
}

#[soroban_sdk::contractclient(name = "PeridottrollerClient")]
pub trait PeridottrollerContract {
    fn account_liquidity(env: Env, user: Address) -> (u128, u128);
    fn get_price_usd(env: Env, token: Address) -> Option<(u128, u128)>;
    fn cache_price(env: Env, token: Address) -> Option<(u128, u128)>;
    fn enter_market(env: Env, user: Address, market: Address);
    fn is_market_supported(env: Env, market: Address) -> bool;
    fn is_borrow_paused(env: Env, market: Address) -> bool;
    fn is_liquidation_paused(env: Env, market: Address) -> bool;
    fn get_market_cf(env: Env, market: Address) -> u128;
    fn get_close_factor_scaled(env: Env) -> u128;
    fn get_liquidation_incentive_scaled(env: Env) -> u128;
    fn get_liquidation_fee_scaled(env: Env) -> u128;
    fn get_reserve_recipient(env: Env) -> Option<Address>;
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
        position_shortfall_usd: u128,
        max_seize_ptokens: u128,
    ) -> u128;
}

#[soroban_sdk::contractclient(name = "SwapAdapterClient")]
pub trait SwapAdapterContract {
    fn is_pool_allowed(env: Env, pool: Address) -> bool;
    fn is_pool_binding_allowed(env: Env, pool_id: BytesN<32>, pool: Address) -> bool;

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
    PendingAdmin,
    Peridottroller,
    SwapAdapter,
    MaxLeverage,
    MaxSlippageScaled,
    Market(Address),
    PositionCounter,
    Position(u64),
    UserPositions(Address),
    DebtSharesTotal(Address, Address), // (user, debt_asset)
    Initialized,
    PositionInitialLockMarket(u64),
    PositionInitialLockPtokens(u64),
    PositionCollateralVault(u64),
    PositionDebtVault(u64),
    PositionPositionVault(u64),
    PositionMode(u64),
    PendingOpenPosition(u64),
    MarginBalancePtokens(Address, Address), // (user, market)
    PendingUpgradeHash,
    PendingUpgradeEta,
    OpenFeeBps,
    CloseFeeBps,
    MarginFeeIndex(Address), // vault -> accumulated fee per pToken * 1e18
    UserMarginFeeIndex(Address, Address), // (user, vault) -> snapshot of MarginFeeIndex
    UserMarginFeeAccrued(Address, Address), // (user, vault) -> claimable pTokens
    TotalMarginPtokens(Address), // vault -> total free-margin pTokens
    MarginFeeOrphan(Address), // vault -> fee pTokens with no LP recipients
    MarginFeeRemainder(Address), // vault -> undistributed fee numerator (1e18-scaled) carried forward
    PendingPeridottroller,
    PendingPeridottrollerEta,
    PendingSwapAdapter,
    PendingSwapAdapterEta,
    PendingOpenSuppliedPtokens(u64),
    PendingOpenSuppliedAmount(u64),
    PerpsPairConfig(Address, Address, PositionSide), // (margin_asset, base_asset, side)
    PendingPerpsOpenPosition(u64),
    PendingPerpsOpenExecution(u64),
    PerpsPositionData(u64),
    PendingLiquidation(u64),
    PendingPerpsClose(u64),
    // Storage-key variants are append-only. Reordering changes on-chain key encoding.
    PendingPerpsCloseRemainder(u64),
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
    PendingOpen,
    Closing,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PositionMode {
    Legacy,
    MarginV2,
    PerpsV3,
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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingOpenPosition {
    pub owner: Address,
    pub collateral_asset: Address,
    pub debt_asset: Address,
    pub position_asset: Address,
    pub collateral_vault: Address,
    pub debt_vault: Address,
    pub position_vault: Address,
    pub collateral_ptokens: u128,
    pub open_fee_ptokens: u128,
    pub borrow_amount: u128,
    pub min_position_amount: u128,
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerpsPairConfig {
    pub max_leverage: u128,
    pub maintenance_margin_scaled: u128,
    pub liquidation_incentive_scaled: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingPerpsOpenPosition {
    pub owner: Address,
    pub margin_asset: Address,
    pub base_asset: Address,
    pub side: PositionSide,
    pub margin_vault: Address,
    pub debt_vault: Address,
    pub position_vault: Address,
    pub margin_ptokens: u128,
    pub margin_amount: u128,
    pub notional_value: u128,
    pub borrow_amount: u128,
    pub min_position_amount: u128,
    pub pool_tokens: Vec<Address>,
    pub pool_id: BytesN<32>,
    pub pool: Address,
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingPerpsOpenExecution {
    pub margin_received: u128,
    pub position_amount: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingPerpsClose {
    pub owner: Address,
    pub collateral_underlying: u128,
    pub debt_amount: u128,
    pub received_debt_asset: u128,
    pub expires_at: u64,
    pub prepared_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerpsPositionData {
    pub margin_asset: Address,
    pub base_asset: Address,
    pub side: PositionSide,
    pub margin_amount: u128,
    pub notional_value: u128,
    pub maintenance_margin_scaled: u128,
    pub liquidation_incentive_scaled: u128,
    pub pool_tokens: Vec<Address>,
    pub pool_id: BytesN<32>,
    pub pool: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerpsLiquidationQuote {
    pub collateral_underlying: u128,
    pub debt_amount: u128,
    pub oracle_min_out: u128,
    pub pool_estimated_out: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PendingLiquidationKind {
    MarginV2,
    PerpsV3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PendingLiquidationStage {
    Started,
    Repaid,
    CollateralConverted,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingLiquidation {
    pub kind: PendingLiquidationKind,
    pub stage: PendingLiquidationStage,
    pub owner: Address,
    pub liquidator: Address,
    pub debt_amount: u128,
    pub repay_amount: u128,
    pub received_debt_asset: u128,
    pub position_seize_ptokens: u128,
    pub position_fee_ptokens: u128,
    pub initial_market: Option<Address>,
    pub initial_seize_ptokens: u128,
    pub initial_fee_ptokens: u128,
    pub reserve_recipient: Option<Address>,
    pub liquidation_incentive_scaled: u128,
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

pub fn get_max_slippage_scaled(env: &Env) -> u128 {
    bump_core_ttl(env);
    env.storage()
        .persistent()
        .get(&DataKey::MaxSlippageScaled)
        .unwrap_or(DEFAULT_MAX_SLIPPAGE_SCALED)
}

pub fn get_price_usd(env: &Env, asset: &Address) -> (u128, u128) {
    let peridottroller_addr: Address = {
        bump_core_ttl(env);
        env.storage()
            .persistent()
            .get(&DataKey::Peridottroller)
            .expect("peridottroller not set")
    };
    let _ = env.try_invoke_contract::<Option<(u128, u128)>, InvokeError>(
        &peridottroller_addr,
        &Symbol::new(env, "cache_price"),
        (asset.clone(),).into_val(env),
    );
    let peridottroller = PeridottrollerClient::new(env, &peridottroller_addr);
    let (num, den) = peridottroller
        .get_price_usd(asset)
        .expect("price unavailable");
    if num == 0 || den == 0 {
        panic!("invalid price");
    }
    (num, den)
}

pub fn refresh_price_usd(env: &Env, asset: &Address) -> (u128, u128) {
    let peridottroller = get_peridottroller(env);
    let (num, den) = peridottroller
        .cache_price(asset)
        .expect("price unavailable");
    if num == 0 || den == 0 {
        panic!("invalid price");
    }
    (num, den)
}

pub fn get_price_usd_cache_first(env: &Env, asset: &Address) -> (u128, u128) {
    let peridottroller = get_peridottroller(env);
    let (num, den) = peridottroller
        .get_price_usd(asset)
        .expect("price unavailable");
    if num == 0 || den == 0 {
        panic!("invalid price");
    }
    (num, den)
}
