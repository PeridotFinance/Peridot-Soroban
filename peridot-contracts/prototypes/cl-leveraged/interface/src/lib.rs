#![no_std]
//! Wire types and client traits that `cl-leveraged` needs from its dependencies.
//!
//! Sources (see LEVERAGE-SPEC.md §3):
//! - `Ck`, checkpoint views: CL collateral group DESIGN.md §2 and §4 (in DESIGN, dependency G1/G2).
//! - Controller getters: `simple-peridottroller` source (G3).
//! - Margin namespace on the lending market: `receipt-vault` source (X4).
//! - Reflector `Asset` / `PriceData`: Reflector oracle ABI (CALI Stellar-DEX feed).
//! - `PairOpenTermsV1`, `prepare_open_pair`, `assign_beneficiary`: NOT in DESIGN.md or the stage-1
//!   freeze (X1, X3). Mocked here; dependency on the group/vault engineer.
pub mod stage1;
use stage1::{CloseTermsV1, CommandCreatedV1, CommandV1, FundedV1, ReleasedV1};
use soroban_sdk::{contractclient, contracttype, Address, Env, Symbol};

/// Bounded-age value checkpoint (DESIGN §4): `rate` is USDC per unit, 1e6-scaled.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ck {
    pub rate: u128,
    pub ts: u64,
    pub ledger: u32,
}

/// Two-token funded open (dependency X1). `swap_dir` 0: token0→token1, 1: token1→token0.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairOpenTermsV1 {
    pub amount0: u128,
    pub amount1: u128,
    pub swap_dir: u32,
    pub swap_in: u128,
    pub min_swap: u128,
    pub min_shares: u128,
    pub reserve0: u128,
    pub deadline: u64,
    pub config_revision: u64,
}

/// The group receipt (StgV2 sleeve class) as seen by `cl-leveraged`.
#[contractclient(name = "GroupReceiptClient")]
pub trait GroupReceipt {
    fn nav_checkpoint(e: Env) -> Option<Ck>;
    fn fallback_checkpoint(e: Env) -> Option<Ck>;
    fn checkpoint_nav(e: Env) -> u128;
    fn checkpoint_fallback(e: Env) -> u128;
    fn prepare_open_pair(e: Env, owner: Address, terms: PairOpenTermsV1) -> CommandCreatedV1;
    fn fund_open(e: Env, id: u64) -> FundedV1;
    fn prepare_close(e: Env, owner: Address, shares: u128, terms: CloseTermsV1) -> CommandCreatedV1;
    fn cancel(e: Env, id: u64) -> ReleasedV1;
    fn stage1_job(e: Env, id: u64) -> Option<CommandV1>;
    fn stage1_revision(e: Env) -> u64;
    fn assign_beneficiary(e: Env, custodian: Address, beneficiary: Address, shares: u128, add: bool);
}

#[contractclient(name = "GroupControllerClient")]
pub trait GroupController {
    fn get_market_cf(e: Env, market: Address) -> u128;
    fn get_liquidation_incentive_scaled(e: Env) -> u128;
    fn is_borrow_paused(e: Env, market: Address) -> bool;
}

#[contractclient(name = "MarginMarketClient")]
pub trait MarginMarket {
    fn borrow_for_margin_to_controller(e: Env, position_id: u64, amount: u128);
    fn repay_for_margin(e: Env, position_id: u64, payer: Address, amount: u128);
    fn get_margin_borrow_balance(e: Env, position_id: u64) -> u128;
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

#[contractclient(name = "ReflectorClient")]
pub trait Reflector {
    fn lastprice(e: Env, asset: Asset) -> Option<PriceData>;
    fn decimals(e: Env) -> u32;
}

#[contractclient(name = "SwapAdapterClient")]
pub trait SwapAdapter {
    fn swap_pool(e: Env, user: Address, pool: Address, in_idx: u32, out_idx: u32, amount_in: u128, amount_out_min: u128) -> u128;
}

/// Reflector feeds used here have 14 decimals (DESIGN §1.1).
pub const PRICE_SCALE: u128 = 100_000_000_000_000;
pub const SCALE_1E6: u128 = 1_000_000;
pub const BPS: u128 = 10_000;
