use soroban_sdk::{contracterror, contracttype, Address};

/// Bound addresses. Set once; there is no setter (a changed dependency means a new deployment).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevConfigV1 {
    pub admin: Address,
    pub receipt: Address,
    pub controller: Address,
    pub market: Address,
    pub swap: Address,
    pub pool: Address,
    pub oracle: Address,
    pub token0: Address,
    pub token1: Address,
    pub treasury: Address,
}

/// Risk policy. NO DEFAULTS in code: every value is supplied by governance (tests use fixtures).
/// The liquidation threshold is the group controller's sleeve CF, read live; it is not stored here.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevRiskV1 {
    pub revision: u64,
    pub new_risk_enabled: bool,
    /// b: open LTV = CF * (1 - b).
    pub open_buffer_bps: u32,
    pub max_ckpt_age_secs: u64,
    pub max_oracle_age_secs: u64,
    pub max_depeg_bps: u32,
    pub max_slippage_bps: u32,
    pub open_fee_bps: u32,
    pub close_fee_bps: u32,
    pub exit_incentive_bps: u32,
    pub tp_min_gap_bps: u32,
    pub tp_confirm_delay_secs: u64,
    pub tp_confirm_window_secs: u64,
    pub max_deadline_secs: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingRiskV1 {
    pub risk: LevRiskV1,
    pub eta: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TakeProfitV1 {
    None,
    /// Position equity in USDC raw units: shares * NAV rate + escrow - debt.
    EquityAtLeast(u128),
    /// Reflector CALI PYUSD/USDC, 14 decimals.
    PriceAtLeast(u128),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExitPolicyV1 {
    pub take_profit: TakeProfitV1,
    /// Owner stop: exit when HF < stop_hf_bps / 10_000 (must be > 10_000).
    pub stop_hf_bps: Option<u32>,
}

/// Open (or add) terms. Amounts are raw token units; `swap_*`/`min_*` are passed to the
/// receipt's pair-funded open unchanged (dependency X1).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenLevV1 {
    pub equity0: u128,
    pub borrow1: u128,
    pub swap_dir: u32,
    pub swap_in: u128,
    pub min_swap: u128,
    pub min_shares: u128,
    pub reserve0: u128,
    pub deadline: u64,
    /// Owner's own LTV ceiling (bps of value), checked in addition to the protocol ceiling.
    pub max_ltv_bps: u32,
    pub exit: ExitPolicyV1,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum LevPhaseV1 {
    PendingFund = 0,
    Opening = 1,
    Active = 2,
    Closing = 3,
    Settling = 4,
    Closed = 5,
    Refunded = 6,
    Defaulted = 7,
}

pub const REASON_NONE: u32 = 0;
pub const REASON_VOLUNTARY: u32 = 1;
pub const REASON_TAKE_PROFIT: u32 = 2;
pub const REASON_STOP: u32 = 3;
pub const REASON_HEALTH: u32 = 4;
pub const REASON_REFUND: u32 = 5;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArmedV1 {
    pub ts: u64,
    pub ckpt_ts: u64,
}

/// `Option<UDT>` fields do not convert under SDK 25 testutils, so optional records are enums.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArmV1 {
    Off,
    On(ArmedV1),
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PendingV1 {
    None,
    Open(OpenLevV1),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionV1 {
    pub owner: Address,
    pub phase: LevPhaseV1,
    pub cmd: Option<u64>,
    pub shares: u128,
    pub escrow0: u128,
    pub escrow1: u128,
    pub reserve0: u128,
    pub pending: PendingV1,
    pub exit: ExitPolicyV1,
    pub armed: ArmV1,
    pub reason: u32,
    pub equity_in: u128,
    pub shortfall: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthV1 {
    pub value: u128,
    pub debt: u128,
    /// value * CF / debt, 1e6-scaled; u128::MAX when debt is zero.
    pub hf: u128,
    pub fallback: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum LevKey {
    LevV1Cfg,
    LevV1Risk,
    LevV1PendingRisk,
    LevV1Next,
    LevV1Pos(u64),
}

/// Reserved block 1800–1829 (stage-1 freeze uses 1700–1718).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum LevError {
    AlreadyInitialized = 1800,
    NotInitialized = 1801,
    BadPhase = 1802,
    InvalidTerms = 1803,
    NewRiskDisabled = 1804,
    BorrowPaused = 1805,
    OracleStale = 1806,
    Depeg = 1807,
    CheckpointStale = 1808,
    LtvExceeded = 1809,
    NoExitCondition = 1810,
    NotArmed = 1811,
    ConfirmTooEarly = 1812,
    ConfirmWindowPassed = 1813,
    CheckpointNotNewer = 1814,
    TakeProfitTooClose = 1815,
    CommandNotSettled = 1816,
    Expired = 1817,
    NotOwner = 1818,
    HealthExitNotCancellable = 1819,
    Timelock = 1820,
    NoPendingRisk = 1821,
    InvalidRisk = 1822,
    NoCollateralFactor = 1823,
    NoTakeProfit = 1824,
    SwapRequired = 1825,
    NoDeficit = 1826,
}
