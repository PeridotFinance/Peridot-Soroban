#![no_std]
//! Package 1 wire contract only. No deployable contract, storage implementation,
//! financial execution, signing or production configuration is provided here.
use soroban_sdk::{contractclient, contracterror, contracttype, Address, BytesN, Env};
pub const SCHEMA_VERSION: u32 = 1;
pub const TERMS_DOMAIN: &str = "PERIDOT_STAGE1_TERMS_V1";
pub const BINDING_DOMAIN: &str = "PERIDOT_STAGE1_BINDING_V1";

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
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiabilityV1 {
    pub schema: u32,
    pub id: u64,
    pub owner: Address,
    pub terms_hash: BytesN<32>,
    pub phase: PhaseV1,
    pub unminted: AmountsV1,
    pub reserve0: u128,
    pub exit_claim: AmountsV1,
    pub locked_shares: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveTotalsV1 {
    pub unminted: u128,
    pub execution: u128,
    pub exit_claim: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocationV1 {
    pub supply: u128,
    pub shares: u128,
    pub liquidity_burn: u128,
    pub fees: AmountsV1,
    pub principal: AmountsV1,
    pub allocated: AmountsV1,
    pub retained: AmountsV1,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaiverConsentV1 {
    pub command_id: u64,
    pub terms_hash: BytesN<32>,
    pub holder: Address,
    pub max_shares: u128,
    pub interval_id: u64,
    pub history_revision: u64,
    pub boundary_sequence: u64,
    pub execution_ledger: u32,
}
/// Domain for direct, unlocked emergency redemption. It cannot impersonate a command.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HolderConsentV1 {
    pub holder: Address,
    pub max_shares: u128,
    pub interval_id: u64,
    pub history_revision: u64,
    pub boundary_sequence: u64,
    pub execution_ledger: u32,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingV1 {
    pub schema: u32,
    pub receipt: Address,
    pub vault: Address,
    pub pool: Address,
    pub token0: Address,
    pub token1: Address,
    pub decimals0: u32,
    pub decimals1: u32,
    pub tick_lower: i32,
    pub tick_upper: i32,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptConfigV1 {
    pub schema: u32,
    pub vault: Address,
    pub binding_hash: BytesN<32>,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskV1 {
    pub revision: u64,
    pub new_risk_enabled: bool,
    pub max_deadline_secs: u64,
    pub min_execution_reserve: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultBindingV1 {
    pub binding: BindingV1,
    pub binding_hash: BytesN<32>,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConfigV1 {
    pub binding: BindingV1,
    pub binding_hash: BytesN<32>,
    pub risk: RiskV1,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionV1 {
    pub schema: u32,
    pub id: u64,
    pub owner: Address,
    pub before: Option<PhaseV1>,
    pub after: PhaseV1,
    pub terms_hash: BytesN<32>,
    pub config_revision: u64,
    pub result: ResultV1,
}

/// Explicit prefixes are part of wire keys. Durability is frozen in the manifest.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReceiptKeyV1 {
    Stage1ConfigV1,
    Stage1Next,
    Stage1Busy,
    Stage1Job(u64),
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VaultKeyV1 {
    Stage1Receipt,
    Stage1RiskV1,
    Stage1Reserved(Address),
    Stage1Liability(u64),
    Stage1Locked(Address),
    Stage1Allocation(u64),
}
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Stage1Error {
    UnknownCommand = 1700,
    BadPhase = 1701,
    AlreadyTerminal = 1702,
    Expired = 1703,
    RevisionChanged = 1704,
    BindingMismatch = 1705,
    TermsMismatch = 1706,
    AsyncStageRequired = 1707,
    InsufficientExecutionReserve = 1708,
    CashUnavailable = 1709,
    MinimumNotMet = 1710,
    FreshPriceUnavailable = 1711,
    ConsentMismatch = 1712,
    SharesLocked = 1713,
    BackingShortfall = 1714,
    UpgradeFrozen = 1715,
    InvalidTerms = 1716,
    NotConfigured = 1717,
    Reentry = 1718,
}

#[contractclient(name = "Stage1ReceiptClientV1")]
pub trait Stage1ReceiptV1 {
    fn stage1_bind(e: Env, admin: Address, vault: Address, binding_hash: BytesN<32>);
    fn stage1_config(e: Env) -> Option<ReceiptConfigV1>;
    fn stage1_job(e: Env, id: u64) -> Option<CommandV1>;
    fn stage1_next(e: Env) -> u64;
    fn prepare_open(e: Env, owner: Address, terms: OpenTermsV1) -> CommandCreatedV1;
    fn fund_open(e: Env, id: u64) -> FundedV1;
    fn convert_open(e: Env, id: u64) -> ConvertedV1;
    fn mint_cash(e: Env, id: u64) -> MintedV1;
    fn invest_cash(e: Env, id: u64) -> InvestedV1;
    fn prepare_close(e: Env, owner: Address, shares: u128, terms: CloseTermsV1)
        -> CommandCreatedV1;
    fn withdraw_close(e: Env, id: u64) -> ExitV1;
    fn convert_close(e: Env, id: u64) -> PaidV1;
    fn cancel(e: Env, id: u64) -> ReleasedV1;
    fn recover_in_kind(
        e: Env,
        id: u64,
        accept_reward_waiver: bool,
        consent: Option<WaiverConsentV1>,
    ) -> ExitV1;
}
#[contractclient(name = "Stage1VaultClientV1")]
pub trait Stage1VaultV1 {
    fn stage1_bind_receipt(e: Env, admin: Address, binding: BindingV1);
    fn stage1_config(e: Env) -> Option<VaultConfigV1>;
    fn stage1_liability(e: Env, id: u64) -> Option<LiabilityV1>;
    fn stage1_set_risk(e: Env, admin: Address, expected_revision: u64, risk: RiskV1);
    fn stage1_check_open(e: Env, receipt: Address, id: u64, job: CommandV1);
    fn stage1_accept_funds(e: Env, receipt: Address, id: u64, job: CommandV1) -> FundedV1;
    fn stage1_convert_open(e: Env, receipt: Address, id: u64, job: CommandV1) -> ConvertedV1;
    fn stage1_cash_mint(e: Env, receipt: Address, id: u64, job: CommandV1) -> MintedV1;
    fn stage1_invest_cash(e: Env, receipt: Address, id: u64, job: CommandV1) -> InvestedV1;
    fn stage1_lock_close(e: Env, receipt: Address, id: u64, job: CommandV1);
    fn stage1_withdraw_kind(e: Env, receipt: Address, id: u64, job: CommandV1) -> ExitV1;
    fn stage1_convert_exit(e: Env, receipt: Address, id: u64, job: CommandV1) -> PaidV1;
    fn stage1_cancel_kind(e: Env, receipt: Address, id: u64, job: CommandV1) -> ReleasedV1;
    fn stage1_recover_kind(
        e: Env,
        receipt: Address,
        id: u64,
        job: CommandV1,
        accept_reward_waiver: bool,
        consent: Option<WaiverConsentV1>,
    ) -> ExitV1;
    fn emergency_redeem_v1(
        e: Env,
        holder: Address,
        shares: u128,
        accept_reward_waiver: bool,
        consent: Option<HolderConsentV1>,
    ) -> ExitV1;
}

/// SHA256 of canonical ScVal XDR, NOT JSON or a self-referential terms hash.
/// Requested close shares are immutable and bound independently of mutable shares.
pub fn terms_hash_v1(
    e: &Env,
    network_id: &BytesN<32>,
    receipt: &Address,
    vault: &Address,
    id: u64,
    owner: &Address,
    binding_hash: &BytesN<32>,
    requested_shares: u128,
    terms: &CommandTermsV1,
) -> BytesN<32> {
    use soroban_sdk::xdr::ToXdr;
    e.crypto()
        .sha256(
            &(
                soroban_sdk::Symbol::new(e, TERMS_DOMAIN),
                network_id.clone(),
                receipt.clone(),
                vault.clone(),
                id,
                owner.clone(),
                binding_hash.clone(),
                requested_shares,
                terms.clone(),
            )
                .to_xdr(e),
        )
        .into()
}
pub fn binding_hash_v1(e: &Env, network_id: &BytesN<32>, binding: &BindingV1) -> BytesN<32> {
    use soroban_sdk::xdr::ToXdr;
    e.crypto()
        .sha256(
            &(
                soroban_sdk::Symbol::new(e, BINDING_DOMAIN),
                network_id.clone(),
                binding.clone(),
            )
                .to_xdr(e),
        )
        .into()
}
