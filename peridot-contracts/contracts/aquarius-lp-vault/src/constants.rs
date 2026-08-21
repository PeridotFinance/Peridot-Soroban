/// Working scale for the NAV square root. `isqrt` of a 1e18-scaled ratio
/// yields a 1e9-scaled root, which is the divisor used to bring it back down.
pub const NAV_RATIO_SCALE: u128 = 1_000_000_000_000_000_000u128; // 1e18
pub const NAV_ROOT_SCALE: u128 = 1_000_000_000u128; // 1e9

/// Aquarius concentrated pools accept ticks within +/- 887272. A full-range
/// position uses the widest tick-spacing-aligned bounds inside that limit.
pub const MAX_TICK_ABS: i32 = 887_272;

/// Vault shares are minted 1:1 with underlying on the first deposit, then pro
/// rata. Matching the pToken convention keeps share math readable.
pub const SHARE_DECIMALS: u32 = 7;

/// Deposits below this many raw underlying units are refused: the swap +
/// deposit round trip cannot mint non-zero liquidity for dust, and the
/// receipt-vault keeps dust in idle cash rather than deploying it.
pub const MIN_DEPLOY_AMOUNT: u128 = 10_000u128;

/// Upgrade timelock, matching the other Peridot contracts.
pub const UPGRADE_TIMELOCK_SECS: u64 = 24 * 60 * 60;

/// Default slippage guard applied to internal rebalancing swaps (1%).
pub const DEFAULT_SLIPPAGE_BPS: u32 = 100u32;
/// Hard ceiling on the configurable slippage guard (5%).
pub const MAX_SLIPPAGE_BPS: u32 = 500u32;

/// Default minimum gap between permissionless harvests (1 hour).
pub const DEFAULT_HARVEST_COOLDOWN_SECS: u64 = 3_600u64;

/// Default lifetime of a cached NAV root before the oracle is re-read.
pub const DEFAULT_NAV_ROOT_MAX_AGE_SECS: u64 = 300u64;

/// Oracle staleness multiplier: a price older than `k * resolution` is
/// rejected. Mirrors the peridottroller default.
pub const DEFAULT_ORACLE_MAX_AGE_MULT: u64 = 2u64;
