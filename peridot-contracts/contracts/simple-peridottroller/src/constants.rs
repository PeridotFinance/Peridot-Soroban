pub const INDEX_SCALE_1E18: u128 = 1_000_000_000_000_000_000u128;
// Hard cap to keep health-check loops within practical Soroban compute budgets.
pub const MAX_USER_MARKETS: u32 = 8;
pub const MAX_CLAIM_BATCH: u32 = 32;
pub const MAX_ORACLE_MAX_AGE_MULTIPLIER: u64 = 10;
pub const MIN_MARKET_CF: u128 = 10_000u128; // 1%
pub const MAX_CLOSE_FACTOR: u128 = 900_000u128; // 90%
pub const MAX_LIQUIDATION_INCENTIVE: u128 = 1_200_000u128; // 120%
pub const MAX_REWARD_SPEED_PER_SEC: u128 = 1_000_000_000_000u128;
pub const FORCE_REMOVE_ZERO_TOTALS_MAX_AGE_SECS: u64 = 24 * 60 * 60;
pub const MAX_FALLBACK_PRICE_AGE_SECS: u64 = 24 * 60 * 60;
pub const MAX_PAUSE_DURATION_SECS: u64 = 72 * 60 * 60;
