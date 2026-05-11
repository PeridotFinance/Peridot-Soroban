pub const SCALE_1E6: u128 = 1_000_000u128;
pub const INDEX_SCALE_1E18: u128 = 1_000_000_000_000_000_000u128; // 1e18
pub const PTOKEN_DECIMALS: u32 = 6;
pub const MAX_YEARLY_RATE_SCALED: u128 = 10_000_000u128; // 1000% APY cap to prevent overflow
pub const UPGRADE_TIMELOCK_SECS: u64 = 24 * 60 * 60;
// DeFindex rejects deposits that cannot mint a non-zero share. Keep dust in
// live cash so rebalance/deposit paths remain idempotent around the target.
pub const MIN_BOOSTED_DEPLOY_AMOUNT: u128 = 10_000u128;
