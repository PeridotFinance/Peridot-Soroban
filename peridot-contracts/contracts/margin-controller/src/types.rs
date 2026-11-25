//! WP0 frozen interface (25 Nov 2026): errors, storage keys, structs, constants.
//! Changing anything here after WP0 needs the integrator (main thread) to agree,
//! because WP1 (backstop.rs), WP2 (contract.rs) and WP3 (headroom.rs) build on it.
//! Spec: research/pooled-margin-20261124/SPEC.md (rev 2) and INTERFACE.md.
use soroban_sdk::{contracterror, contracttype, Address, BytesN};

pub const SCALE_1E6: u128 = 1_000_000;
pub const YEAR_SECS: u128 = 31_536_000;
pub const MAX_SLIPPAGE_CAP: u128 = 100_000; // 10 %
/// Constructor default (WP4 25 Nov): 1.5 %. The real-pool fork saw a close fill 1.02 % below
/// Reflector. 5x solvency still holds: 0.9 x 1.04 x 1.015 = 0.950 < 1.
pub const DEFAULT_MAX_SLIPPAGE: u128 = 15_000;
pub const MAX_LEVERAGE_CAP: u128 = 5_000_000; // 5x, owner 24 Nov
pub const MAX_TOTAL_BONUS: u128 = 150_000; // 15 %, liquidator + backstop
pub const MAX_FEE: u128 = 10_000; // 1 % per open/close fee
pub const MAX_FUNDING_APR: u128 = 100_000; // 10 % APR
pub const BS_DEAD_SHARES: u128 = 1_000_000; // minted to the contract on the first backstop deposit
pub const UPGRADE_TIMELOCK_SECS: u64 = 24 * 60 * 60;
pub const TTL_THRESHOLD: u32 = 500_000;
pub const TTL_EXTEND_TO: u32 = 1_000_000;

/// Error codes 1..18 keep their v0.2 meaning; 19.. are new in v2.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotAdmin = 2,
    OpensPaused = 3,
    BadAmount = 4,
    NotConfigured = 5,
    BadParams = 6,
    PositionCapExceeded = 7,
    GlobalCapExceeded = 8, // side equity cap
    LtvTooHigh = 9,
    Slippage = 10,
    PriceUnavailable = 11,
    NotOwner = 12,
    NotOpen = 13,
    NotLiquidatable = 14,
    InsufficientSwapOutput = 15,
    Timelocked = 16,
    UpgradeMismatch = 17,
    NotGuardian = 18,
    MaxLeverage = 19,
    OiCapExceeded = 20,
    PoolCheck = 21, // open would break the pooled HF / stress / free-liquidity check
    BackstopHeadroom = 22, // backstop exit would break the pool check
    BackstopTooSmall = 23, // open refused: backstop below required size
    Impaired = 24,
    BackstopDebtOutstanding = 25, // XLM bad debt unsettled: backstop deposits/exits frozen
    Cooldown = 26,
    WindowExpired = 27,
    NoRequest = 28,
    MinShares = 29,
    MinPtokens = 30,
    NoClaim = 31,
    MaxPay = 32, // liquidator's max_pay below the required payment
    MaxFee = 33, // open: fee above the user's max_fee
    BackstopEmpty = 34,
}

#[contracttype]
#[derive(Clone)]
pub enum Key {
    // ---- instance
    Admin,
    Guardian,
    Ctrl,
    Pool,
    PoolTokens,
    OpensPaused,
    MaxSlippage,           // u128, 1e6
    Usdc,                  // Address: the backstop asset (USDC underlying)
    Xlm,                   // Address: the traded asset (XLM underlying)
    Market(Address),       // underlying -> receipt
    Risk(Address), // held underlying -> RiskParams (held XLM = long side, held USDC = short side)
    Caps(Address), // held underlying (side) -> Caps
    Side(Address), // held underlying (side) -> SideTotals
    Fees,          // Fees
    DebtShares(Address), // debt underlying -> total position debt shares (EXCLUDES backstop debt shares)
    BsDebtShares(Address), // debt underlying -> debt shares owned by the backstop (bad debt)
    Bs,                  // Backstop
    BsParams,            // BackstopParams
    Headroom,            // Headroom
    Ledger(Address), // underlying -> u128 pTokens the contract accounts for (positions + backstop + claims)
    ClaimsTotal(Address), // underlying -> u128 pTokens owed to owners via claim()
    Impaired,        // bool
    Counter,         // u64
    PendingUpgrade,
    // ---- persistent
    Position(u64),
    Backer(Address),         // u128 backstop shares
    WithdrawReq(Address),    // WithdrawReq
    Claim(Address, Address), // (owner, underlying) -> u128 pTokens
}

/// Per side, keyed by the held asset.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskParams {
    pub max_leverage: u128, // 1e6; 1x < L <= 5x. open LTV limit = 1 - 1/L; per-position liq LTV = (1 + open LTV)/2
}

/// Per side, keyed by the held asset. USD amounts in "usd7": raw 7-decimal units of USD
/// (what `usd(amount, price)` returns for 7-decimal tokens: XLM and USDC).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Caps {
    pub per_position_equity_usd: u128, // pilot $25
    pub side_equity_usd: u128,         // pilot $100
    pub side_oi_usd: u128,             // pilot $500 notional
    pub min_equity_usd: u128,          // anti-dust, e.g. $1
}

/// Per side aggregates at open-time values; decreased by exactly what the position added.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SideTotals {
    pub equity_usd: u128,
    pub oi_usd: u128,
    pub open_positions: u32,
}

/// All 1e6-scaled fractions. Defaults (owner 24 Nov): 1_000 / 1_000 / 20_000 / 30_000 / 10_000.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fees {
    pub open_fee: u128,         // fraction of notional, paid in USDC to the backstop
    pub close_fee: u128,        // fraction of notional, paid in USDC to the backstop
    pub funding_apr: u128, // simple APR on position debt (USD) since open, paid in USDC to the backstop
    pub bonus_liquidator: u128, // fraction of repaid debt, paid to the liquidator in held pTokens
    pub bonus_backstop: u128, // fraction of repaid debt, paid to the backstop in USDC
}

/// Backstop totals. Everything is USDC pTokens, tracked (never balanceOf).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Backstop {
    pub shares: u128,         // incl. BS_DEAD_SHARES
    pub ptokens: u128,        // gross USDC pTokens owned by the backstop
    pub pending_shares: u128, // shares under an exit request (still bear losses / earn fees)
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackstopParams {
    pub cooldown_s: u64, // 259_200 (72 h, owner)
    pub window_s: u64,   // 172_800 (48 h to execute after the cooldown)
    pub min_deposit_pt: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawReq {
    pub shares: u128,
    pub ready_at: u64,
    pub expires_at: u64,
}

/// Pool check parameters (SPEC §4). 1e6-scaled except *_usd (usd7).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Headroom {
    pub hf_target: u128,         // 1_250_000
    pub hf_stress: u128,         // 1_100_000
    pub stress: u128,            // 200_000 (XLM +-20 %)
    pub close_reserve_usd: u128, // 0 = derive from caps (see INTERFACE.md); else this floor
    pub coef_long: u128,         // 3_000_000: required backstop per $ long equity
    pub coef_short: u128,        // 2_100_000
    pub coef_largest: u128, // 4_600_000: per $ of the largest allowed position (per_position_equity_usd cap)
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub owner: Address,
    pub held_asset: Address,
    pub debt_asset: Address,
    pub held_ptokens: u128,
    pub debt_shares: u128,
    pub collateral: u128,   // raw held units the trader posted
    pub equity_usd: u128,   // usd7 at open (collateral value) -> side equity / caps
    pub notional_usd: u128, // usd7 at open (held value after the trade) -> OI / fees
    pub liq_ltv: u128, // 1e6, fixed at open: (1e6 + open LTV) / 2  (maintenance = half the starting margin)
    pub opened_at: u64,
    pub funding_ts: u64, // funding accrued up to here
    pub open: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpgrade {
    pub hash: BytesN<32>,
    pub eta: u64,
}

/// What the pool check is evaluating (headroom.rs).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Check {
    /// After an open: target, stress, free liquidity, backstop size; queued exits excluded.
    Open,
    /// After a backstop exit of `exiting_pt` (already transferred out): target, stress, free liquidity.
    BackstopExit,
}
