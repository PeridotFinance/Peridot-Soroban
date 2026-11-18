//! Wire-identical copy of the stage-1 freeze types this prototype uses
//! (`interfaces/cl-stage1-interface/src/lib.rs`, PhaseV1 .. CommandV1, unchanged text).
//! Copied rather than depended on: the frozen crate does not compile with the SDK `testutils`
//! feature (its `TransitionV1` has an `Option<PhaseV1>` field without an XDR conversion), and the
//! prototype's unit tests need `testutils`. Same field names/order give the same ScVal encoding.
use soroban_sdk::{contracttype, Address, BytesN};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PhaseV1 {
    Prepared = 0,
    Funded = 1,
    Converted = 2,
    MintedPendingInvestment = 3,
    Invested = 4,
    CloseLocked = 10,
    ExitMixture = 11,
    Paid = 12,
    Cancelled = 99,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CloseModeV1 {
    InKind = 0,
    ConvertToUnderlying = 1,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmountsV1 {
    pub token0: u128,
    pub token1: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenTermsV1 {
    pub amount: u128,
    pub swap_in: u128,
    pub min_swap: u128,
    pub min_shares: u128,
    pub max_strategy_cost: u128,
    pub deadline: u64,
    pub config_revision: u64,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloseTermsV1 {
    pub mode: CloseModeV1,
    pub min_amounts: AmountsV1,
    pub min_underlying: u128,
    pub deadline: u64,
    pub config_revision: u64,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandTermsV1 {
    Open(OpenTermsV1),
    Close(CloseTermsV1),
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandCreatedV1 {
    pub id: u64,
    pub terms_hash: BytesN<32>,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundedV1 {
    pub amount0: u128,
    pub reserve0: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvertedV1 {
    pub amounts: AmountsV1,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MintedV1 {
    pub shares: u128,
    pub locked: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestedV1 {
    pub liquidity: u128,
    pub cost0: u128,
    pub refund0: u128,
    pub unlocked: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExitV1 {
    pub amounts: AmountsV1,
    pub paid: bool,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaidV1 {
    pub amounts: AmountsV1,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleasedV1 {
    pub amounts: AmountsV1,
    pub shares: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResultV1 {
    None,
    Funded(FundedV1),
    Converted(ConvertedV1),
    Minted(MintedV1),
    Invested(InvestedV1),
    Exit(ExitV1),
    Paid(PaidV1),
    Released(ReleasedV1),
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandV1 {
    pub schema: u32,
    pub id: u64,
    pub owner: Address,
    pub terms: CommandTermsV1,
    pub terms_hash: BytesN<32>,
    pub phase: PhaseV1,
    pub amounts: AmountsV1,
    pub reserve0: u128,
    pub requested_shares: u128,
    pub shares: u128,
    pub result: ResultV1,
}
