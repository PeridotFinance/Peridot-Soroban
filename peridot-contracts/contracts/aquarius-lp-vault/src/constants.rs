/// Basis-point denominator.
pub const BPS_DENOM: u32 = 10_000u32;

/// Working scale for the NAV square root. `isqrt` of a 1e18-scaled ratio
/// yields a 1e9-scaled root, which is the divisor used to bring it back down.
pub const NAV_RATIO_SCALE: u128 = 1_000_000_000_000_000_000u128; // 1e18
pub const NAV_ROOT_SCALE: u128 = 1_000_000_000u128; // 1e9

/// Aquarius concentrated pools accept ticks within +/- 887272. A full-range
/// position uses the widest tick-spacing-aligned bounds inside that limit.
pub const MAX_TICK_ABS: i32 = 887_272;

/// New deployments start with an approximately +/-1% range. Governance can
/// widen individual strategies before capital enters; existing Mainnet
/// deployments remain disabled until a policy is explicitly installed.
pub const DEFAULT_HALF_WIDTH_TICKS: u32 = 100;
pub const DEFAULT_REBALANCE_MARGIN_TICKS: u32 = 40;
pub const DEFAULT_REBALANCE_COOLDOWN_SECS: u64 = 6 * 60 * 60;
pub const DEFAULT_MAX_REBALANCE_DIVERGENCE_BPS: u32 = 100;
/// Hard policy bounds. A position this wide is still materially concentrated,
/// while the lower bound prevents a one-spacing range that can be churned by
/// ordinary swaps.
pub const MAX_HALF_WIDTH_TICKS: u32 = 50_000;
pub const MAX_REBALANCE_DIVERGENCE_BPS: u32 = 500;
pub const MIN_REBALANCE_COOLDOWN_SECS: u64 = 300;
/// A successful rebalance must put at least 95% of the independently valued
/// pair back to work. This catches a bad range quote or a badly imbalanced
/// unwind before the transaction can strand most capital as idle dust.
pub const MAX_REBALANCE_IDLE_BPS: u128 = 500;
/// The pool's exact range quote must agree closely with the value share implied
/// independently by the live tick and the new centered bounds. Five percentage
/// points covers tick-spacing alignment and the separate oracle-price guard.
pub const MAX_REBALANCE_QUOTE_SHARE_DEVIATION_BPS: u128 = 500;

/// Vault shares are minted 1:1 with underlying on the first deposit, then pro
/// rata. Matching the pToken convention keeps share math readable.
pub const SHARE_DECIMALS: u32 = 7;

/// Largest token decimal count whose `10^decimals` scale fits in `u128`.
/// Token metadata is immutable configuration for the vault, so reject an
/// unsafe value at initialization instead of panicking later during NAV math.
pub const MAX_TOKEN_DECIMALS: u32 = 38;

/// Scale for governance's minimum raw-underlying/raw-reward exchange rate.
/// Seven decimals keeps deployment-script arithmetic within signed 64-bit
/// bounds for the target routes while retaining sub-basis-point precision.
pub const REWARD_RATE_SCALE: u128 = 10_000_000u128;

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

/// Default tolerance between the pool's swap quote and the oracle fair rate.
pub const DEFAULT_MAX_POOL_DIVERGENCE_BPS: u32 = 200u32; // 2%

/// Hard ceiling on the stale-oracle NAV fallback (1 hour), matching
/// `receipt-vault`'s own `BOOSTED_CACHE_MAX_AGE_SECS`.
pub const DEFAULT_NAV_ROOT_MAX_STALE_SECS: u64 = 3_600u64;

/// Oracle staleness multiplier: a price older than `k * resolution` is
/// rejected. Mirrors the peridottroller default.
pub const DEFAULT_ORACLE_MAX_AGE_MULT: u64 = 2u64;
