pub const INDEX_SCALE_1E18: u128 = 1_000_000_000_000_000_000u128;
// Hard cap to keep health-check loops within practical Soroban compute budgets.
pub const MAX_USER_MARKETS: u32 = 8;
pub const MAX_CLAIM_BATCH: u32 = 32;
pub const MAX_ORACLE_MAX_AGE_MULTIPLIER: u64 = 10;
pub const MIN_MARKET_CF: u128 = 10_000u128; // 1%
                                            // Keep close-factor math viable for every non-dust debt amount:
                                            // for debt >= 2, debt * 50% floors to at least 1 smallest debt unit.
pub const MIN_CLOSE_FACTOR: u128 = 500_000u128; // 50%
pub const MAX_CLOSE_FACTOR: u128 = 900_000u128; // 90%
pub const MAX_LIQUIDATION_INCENTIVE: u128 = 1_200_000u128; // 120%
pub const MAX_REWARD_SPEED_PER_SEC: u128 = 1_000_000_000_000u128;
pub const FORCE_REMOVE_ZERO_TOTALS_MAX_AGE_SECS: u64 = 24 * 60 * 60;
pub const MAX_FALLBACK_PRICE_AGE_SECS: u64 = 24 * 60 * 60;
pub const MAX_FALLBACK_PRICE: u128 = 1_000_000_000_000_000_000_000_000_000_000u128; // 1e30
pub const MAX_FALLBACK_SCALE: u128 = 1_000_000_000_000_000_000u128; // 1e18
pub const MAX_PAUSE_DURATION_SECS: u64 = 72 * 60 * 60;
pub const UPGRADE_TIMELOCK_SECS: u64 = 24 * 60 * 60;
pub const ADMIN_PARAM_CHANGE_DELAY_SECS: u64 = 24 * 60 * 60;
// Upgrade-candidate bounds (cash-borrow upgrade checks, 2026-11-18).
// An unset MaxUserMarkets key means this strict default; no migration runs on upgrade.
pub const DEFAULT_MAX_USER_MARKETS: u32 = 2;
pub const MIN_MAX_USER_MARKETS: u32 = 2;
// Absolute price-age bound, independent of the oracle-reported resolution.
pub const DEFAULT_MAX_PRICE_AGE_SECS: u64 = 900;
pub const MIN_MAX_PRICE_AGE_SECS: u64 = 60;
pub const MAX_MAX_PRICE_AGE_SECS: u64 = 3600;
// Price bands are USD prices scaled 1e6 (980_000 = $0.98), independent of oracle decimals.
pub const PRICE_BAND_SCALE: u128 = 1_000_000u128;
pub const MAX_PRICE_BAND_BOUND: u128 = 1_000_000_000_000_000_000u128;
