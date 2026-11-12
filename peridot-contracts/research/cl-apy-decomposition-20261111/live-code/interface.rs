#[soroban_sdk::contractargs(name = "Args")]
#[soroban_sdk::contractclient(name = "Client")]
pub trait Contract {
    fn get_tick(env: soroban_sdk::Env, tick: i32) -> TickInfo;
    fn get_slot0(env: soroban_sdk::Env) -> Slot0;
    fn get_position(
        env: soroban_sdk::Env,
        recipient: soroban_sdk::Address,
        tick_lower: i32,
        tick_upper: i32,
    ) -> PositionData;
    fn get_tick_bounds(env: soroban_sdk::Env) -> (i32, i32);
    fn get_ticks_batch(
        env: soroban_sdk::Env,
        ticks: soroban_sdk::Vec<i32>,
    ) -> soroban_sdk::Vec<TickInfo>;
    fn deposit_position(
        env: soroban_sdk::Env,
        sender: soroban_sdk::Address,
        tick_lower: i32,
        tick_upper: i32,
        desired_amounts: soroban_sdk::Vec<u128>,
        min_liquidity: u128,
    ) -> (soroban_sdk::Vec<u128>, u128);
    fn get_chunk_bitmap(env: soroban_sdk::Env, word_pos: i32) -> soroban_sdk::U256;
    fn get_tick_spacing(env: soroban_sdk::Env) -> i32;
    fn get_position_fees(
        env: soroban_sdk::Env,
        owner: soroban_sdk::Address,
        tick_lower: i32,
        tick_upper: i32,
    ) -> soroban_sdk::Vec<u128>;
    fn tick_from_amounts(env: soroban_sdk::Env, amount0: u128, amount1: u128) -> i32;
    fn withdraw_position(
        env: soroban_sdk::Env,
        owner: soroban_sdk::Address,
        tick_lower: i32,
        tick_upper: i32,
        amount: u128,
        min_amounts: soroban_sdk::Vec<u128>,
    ) -> soroban_sdk::Vec<u128>;
    fn claim_position_fees(
        env: soroban_sdk::Env,
        owner: soroban_sdk::Address,
        tick_lower: i32,
        tick_upper: i32,
    ) -> soroban_sdk::Vec<u128>;
    fn get_active_liquidity(env: soroban_sdk::Env) -> u128;
    fn get_all_position_fees(
        env: soroban_sdk::Env,
        owner: soroban_sdk::Address,
    ) -> soroban_sdk::Vec<u128>;
    fn get_chunk_bitmap_batch(
        env: soroban_sdk::Env,
        start_word: i32,
        count: u32,
    ) -> soroban_sdk::Vec<soroban_sdk::U256>;
    fn claim_all_position_fees(
        env: soroban_sdk::Env,
        owner: soroban_sdk::Address,
    ) -> soroban_sdk::Vec<u128>;
    fn get_total_raw_liquidity(env: soroban_sdk::Env) -> u128;
    fn estimate_deposit_position(
        env: soroban_sdk::Env,
        tick_lower: i32,
        tick_upper: i32,
        desired_amounts: soroban_sdk::Vec<u128>,
    ) -> (soroban_sdk::Vec<u128>, u128);
    fn estimate_withdraw_position(
        env: soroban_sdk::Env,
        owner: soroban_sdk::Address,
        tick_lower: i32,
        tick_upper: i32,
        amount: u128,
    ) -> soroban_sdk::Vec<u128>;
    fn get_user_position_snapshot(
        env: soroban_sdk::Env,
        user: soroban_sdk::Address,
    ) -> UserPositionSnapshot;
    fn get_fee_growth_global_0_x128(env: soroban_sdk::Env) -> soroban_sdk::U256;
    fn get_fee_growth_global_1_x128(env: soroban_sdk::Env) -> soroban_sdk::U256;
    fn get_total_weighted_liquidity(env: soroban_sdk::Env) -> u128;
    fn version(env: soroban_sdk::Env) -> u32;
    fn apply_upgrade(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
    ) -> (soroban_sdk::BytesN<32>, soroban_sdk::BytesN<32>);
    fn contract_name(env: soroban_sdk::Env) -> soroban_sdk::Symbol;
    fn commit_upgrade(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        new_wasm_hash: soroban_sdk::BytesN<32>,
        token_new_wasm_hash: soroban_sdk::BytesN<32>,
        gauges_new_wasm_hash: soroban_sdk::BytesN<32>,
        vault_new_wasm_hash: soroban_sdk::BytesN<32>,
    );
    fn finalize_sweep(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn revert_upgrade(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn get_emergency_mode(env: soroban_sdk::Env) -> bool;
    fn set_emergency_mode(
        env: soroban_sdk::Env,
        emergency_admin: soroban_sdk::Address,
        value: bool,
    );
    fn activate_accumulator(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        legacy_total_at_apply: u128,
        config_storage_addr: soroban_sdk::Address,
        reward_tps: u128,
        reward_expired_at: u64,
    );
    fn materialize_positions(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        user: soroban_sdk::Address,
    );
    fn claim(env: soroban_sdk::Env, user: soroban_sdk::Address) -> u128;
    fn get_vault_addr(env: soroban_sdk::Env) -> soroban_sdk::Address;
    fn get_user_reward(env: soroban_sdk::Env, user: soroban_sdk::Address) -> u128;
    fn get_rewards_info(
        env: soroban_sdk::Env,
        user: soroban_sdk::Address,
    ) -> soroban_sdk::Map<soroban_sdk::Symbol, i128>;
    fn get_rewards_state(env: soroban_sdk::Env, user: soroban_sdk::Address) -> bool;
    fn get_unused_reward(env: soroban_sdk::Env) -> u128;
    fn set_rewards_state(env: soroban_sdk::Env, user: soroban_sdk::Address, state: bool);
    fn set_rewards_config(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        expired_at: u64,
        tps: u128,
    );
    fn return_unused_reward(env: soroban_sdk::Env, admin: soroban_sdk::Address) -> u128;
    fn tick_schedule_reward(
        env: soroban_sdk::Env,
        router: soroban_sdk::Address,
        distributor: soroban_sdk::Address,
        token: soroban_sdk::Address,
        start_at: u64,
        duration: u64,
        tps: u128,
    ) -> u32;
    fn get_reward_destination(env: soroban_sdk::Env) -> soroban_sdk::Address;
    fn admin_set_rewards_state(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        user: soroban_sdk::Address,
        state: bool,
    );
    fn initialize_boost_config(
        env: soroban_sdk::Env,
        reward_boost_token: soroban_sdk::Address,
        reward_boost_feed: soroban_sdk::Address,
    );
    fn set_reward_boost_config(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        reward_boost_token: soroban_sdk::Address,
        reward_boost_feed: soroban_sdk::Address,
    );
    fn estimate_working_balance(
        env: soroban_sdk::Env,
        user: soroban_sdk::Address,
        tick_lower: i32,
        tick_upper: i32,
        new_liquidity: u128,
    ) -> (u128, u128);
    fn get_total_claimed_reward(env: soroban_sdk::Env) -> u128;
    fn get_incentives_vault_addr(env: soroban_sdk::Env) -> Option<soroban_sdk::Address>;
    fn initialize_rewards_config(
        env: soroban_sdk::Env,
        reward_token: soroban_sdk::Address,
    );
    fn get_total_configured_reward(env: soroban_sdk::Env) -> u128;
    fn get_total_accumulated_reward(env: soroban_sdk::Env) -> u128;
    fn get_total_outstanding_reward(env: soroban_sdk::Env) -> u128;
    fn get_reserved_other_rid_balance(env: soroban_sdk::Env) -> u128;
    fn get_future_address(
        env: soroban_sdk::Env,
        role_name: soroban_sdk::Symbol,
    ) -> soroban_sdk::Address;
    fn apply_transfer_ownership(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        role_name: soroban_sdk::Symbol,
    );
    fn commit_transfer_ownership(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        role_name: soroban_sdk::Symbol,
        new_address: soroban_sdk::Address,
    );
    fn revert_transfer_ownership(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        role_name: soroban_sdk::Symbol,
    );
    fn get_gauges(
        env: soroban_sdk::Env,
    ) -> soroban_sdk::Map<soroban_sdk::Address, soroban_sdk::Address>;
    fn gauges_claim(
        env: soroban_sdk::Env,
        user: soroban_sdk::Address,
    ) -> soroban_sdk::Map<soroban_sdk::Address, u128>;
    fn kill_gauges_claim(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn unkill_gauges_claim(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn gauges_claim_by_token(
        env: soroban_sdk::Env,
        user: soroban_sdk::Address,
        reward_token: soroban_sdk::Address,
    ) -> u128;
    fn gauges_get_reward_info(
        env: soroban_sdk::Env,
        user: soroban_sdk::Address,
    ) -> soroban_sdk::Map<
        soroban_sdk::Address,
        soroban_sdk::Map<soroban_sdk::Symbol, i128>,
    >;
    fn gauges_get_reward_configs(
        env: soroban_sdk::Env,
    ) -> soroban_sdk::Map<soroban_sdk::Address, soroban_sdk::Vec<RewardConfig>>;
    fn get_is_killed_gauges_claim(env: soroban_sdk::Env) -> bool;
    fn swap(
        env: soroban_sdk::Env,
        user: soroban_sdk::Address,
        in_idx: u32,
        out_idx: u32,
        in_amount: u128,
        out_min: u128,
    ) -> u128;
    fn deposit(
        env: soroban_sdk::Env,
        user: soroban_sdk::Address,
        desired_amounts: soroban_sdk::Vec<u128>,
        min_shares: u128,
    ) -> (soroban_sdk::Vec<u128>, u128);
    fn get_info(
        env: soroban_sdk::Env,
    ) -> soroban_sdk::Map<soroban_sdk::Symbol, soroban_sdk::Val>;
    fn share_id(env: soroban_sdk::Env) -> soroban_sdk::Address;
    fn withdraw(
        env: soroban_sdk::Env,
        user: soroban_sdk::Address,
        share_amount: u128,
        min_amounts: soroban_sdk::Vec<u128>,
    ) -> soroban_sdk::Vec<u128>;
    fn pool_type(env: soroban_sdk::Env) -> soroban_sdk::Symbol;
    fn get_tokens(env: soroban_sdk::Env) -> soroban_sdk::Vec<soroban_sdk::Address>;
    fn initialize(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        privileged_addrs: (
            soroban_sdk::Address,
            soroban_sdk::Address,
            soroban_sdk::Address,
            soroban_sdk::Address,
            soroban_sdk::Vec<soroban_sdk::Address>,
            soroban_sdk::Address,
        ),
        router: soroban_sdk::Address,
        tokens: soroban_sdk::Vec<soroban_sdk::Address>,
        fee: u32,
        tick_spacing: i32,
        protocol_fee_fraction: u32,
    );
    fn get_reserves(env: soroban_sdk::Env) -> soroban_sdk::Vec<u128>;
    fn estimate_swap(
        env: soroban_sdk::Env,
        in_idx: u32,
        out_idx: u32,
        in_amount: u128,
    ) -> u128;
    fn get_user_shares(env: soroban_sdk::Env, user: soroban_sdk::Address) -> u128;
    fn estimate_deposit(
        env: soroban_sdk::Env,
        desired_amounts: soroban_sdk::Vec<u128>,
    ) -> u128;
    fn get_fee_fraction(env: soroban_sdk::Env) -> u32;
    fn get_total_shares(env: soroban_sdk::Env) -> u128;
    fn swap_strict_receive(
        env: soroban_sdk::Env,
        user: soroban_sdk::Address,
        in_idx: u32,
        out_idx: u32,
        out_amount: u128,
        in_max: u128,
    ) -> u128;
    fn get_protocol_fee_fraction(env: soroban_sdk::Env) -> u32;
    fn get_total_excluded_shares(env: soroban_sdk::Env) -> u128;
    fn estimate_swap_strict_receive(
        env: soroban_sdk::Env,
        in_idx: u32,
        out_idx: u32,
        out_amount: u128,
    ) -> u128;
    fn kill_swap(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn kill_claim(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn unkill_swap(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn kill_deposit(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn unkill_claim(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn unkill_deposit(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn get_protocol_fees(env: soroban_sdk::Env) -> soroban_sdk::Vec<u128>;
    fn get_is_killed_swap(env: soroban_sdk::Env) -> bool;
    fn claim_protocol_fees(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        destination: soroban_sdk::Address,
    ) -> soroban_sdk::Vec<u128>;
    fn get_is_killed_claim(env: soroban_sdk::Env) -> bool;
    fn get_privileged_addrs(
        env: soroban_sdk::Env,
    ) -> soroban_sdk::Map<soroban_sdk::Symbol, soroban_sdk::Vec<soroban_sdk::Address>>;
    fn set_privileged_addrs(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        rewards_admin: soroban_sdk::Address,
        operations_admin: soroban_sdk::Address,
        pause_admin: soroban_sdk::Address,
        emergency_pause_admins: soroban_sdk::Vec<soroban_sdk::Address>,
        system_fee_admin: soroban_sdk::Address,
    );
    fn get_is_killed_deposit(env: soroban_sdk::Env) -> bool;
    fn set_protocol_fee_fraction(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        new_fraction: u32,
    );
    fn get_pools_plane(env: soroban_sdk::Env) -> soroban_sdk::Address;
    fn set_pools_plane(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        plane: soroban_sdk::Address,
    );
    fn init_pools_plane(env: soroban_sdk::Env, plane: soroban_sdk::Address);
    fn backfill_plane_data(env: soroban_sdk::Env);
    fn initialize_all(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        privileged_addrs: (
            soroban_sdk::Address,
            soroban_sdk::Address,
            soroban_sdk::Address,
            soroban_sdk::Address,
            soroban_sdk::Vec<soroban_sdk::Address>,
            soroban_sdk::Address,
        ),
        router: soroban_sdk::Address,
        tokens: soroban_sdk::Vec<soroban_sdk::Address>,
        fee: u32,
        tick_spacing: i32,
        protocol_fee_fraction: u32,
        reward_config: (
            soroban_sdk::Address,
            soroban_sdk::Address,
            soroban_sdk::Address,
        ),
        plane: soroban_sdk::Address,
        config_storage_addr: soroban_sdk::Address,
    );
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Slot0 {
    pub sqrt_price_x96: soroban_sdk::U256,
    pub tick: i32,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct TickData(pub soroban_sdk::U256, pub soroban_sdk::U256, pub u128, pub i128);
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct TickInfo {
    pub fee_growth_outside_0_x128: soroban_sdk::U256,
    pub fee_growth_outside_1_x128: soroban_sdk::U256,
    pub liquidity_gross: u128,
    pub liquidity_net: i128,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PoolState {
    pub fee: u32,
    pub liquidity: u128,
    pub sqrt_price_x96: soroban_sdk::U256,
    pub tick: i32,
    pub tick_spacing: i32,
    pub token0: soroban_sdk::Address,
    pub token1: soroban_sdk::Address,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct UserState {
    pub positions: soroban_sdk::Vec<PositionRange>,
    pub raw_liquidity: u128,
    pub weighted_liquidity: u128,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct SwapResult {
    pub amount0: i128,
    pub amount1: i128,
    pub liquidity: u128,
    pub sqrt_price_x96: soroban_sdk::U256,
    pub tick: i32,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PerRidGlobal {
    pub config_queue: soroban_sdk::Vec<RewardConfig>,
    pub growth_global: soroban_sdk::U256,
    pub last_update: u64,
    pub totals: AccumulatorTotals,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PositionData {
    pub fee_growth_inside_0_last_x128: soroban_sdk::U256,
    pub fee_growth_inside_1_last_x128: soroban_sdk::U256,
    pub liquidity: u128,
    pub tokens_owed_0: u128,
    pub tokens_owed_1: u128,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProtocolFees {
    pub token0: u128,
    pub token1: u128,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct RewardConfig {
    pub expired_at: u64,
    pub start_at: u64,
    pub tps: u128,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PositionRange {
    pub tick_lower: i32,
    pub tick_upper: i32,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct TickPlaneSlot(pub soroban_sdk::Vec<soroban_sdk::U256>, pub i128);
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PositionDataV2 {
    pub fee_growth_inside_0_last_x128: soroban_sdk::U256,
    pub fee_growth_inside_1_last_x128: soroban_sdk::U256,
    pub liquidity: u128,
    pub reward_last: soroban_sdk::Vec<Option<soroban_sdk::U256>>,
    pub tokens_owed_0: u128,
    pub tokens_owed_1: u128,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct UserBoostFactor(pub u128, pub u128);
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct UserRewardState {
    pub boost_established: bool,
    pub boost_factor: UserBoostFactor,
    pub pending_gauge_rewards: soroban_sdk::Vec<u128>,
    pub pending_reward_token: u128,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PerRidGlobalData(
    pub soroban_sdk::U256,
    pub u64,
    pub AccumulatorTotals,
    pub soroban_sdk::Vec<RewardConfigData>,
);
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct RewardConfigData(pub u64, pub u64, pub u128);
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct AccumulatorTotals(pub u128, pub u128);
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct UserPositionSnapshot {
    pub ranges: soroban_sdk::Vec<PositionRange>,
    pub raw_liquidity: u128,
    pub weighted_liquidity: u128,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PoolStateWithBalances {
    pub reserve0: i128,
    pub reserve1: i128,
    pub state: PoolState,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PackedRewardGlobalState(pub soroban_sdk::Vec<Option<PerRidGlobalData>>);
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum DataKey {
    Router,
    Token0,
    Token1,
    Decimals,
    Fee,
    TickSpacing,
    ProtocolFeeFraction,
    Plane,
    Slot0,
    Liquidity,
    FeeGrowthGlobal0X128,
    FeeGrowthGlobal1X128,
    ProtocolFees,
    Reserve0,
    Reserve1,
    MinInitTick,
    MaxInitTick,
    TotalRawLiquidity,
    FullRangeLiquidity,
    IsKilledDeposit,
    IsKilledSwap,
    ClaimKilled,
    IsKilledGaugesClaim,
    TokenFutureWasm,
    VaultFutureWasm,
    TickChunk(i32),
    ChunkBitmap(i32),
    WordBitmap(i32),
    Position(soroban_sdk::Address, i32, i32),
    PositionV2(soroban_sdk::Address, i32, i32),
    User(soroban_sdk::Address),
    RewardGlobalStatePacked,
    LiquidityActiveBoosted,
    VaultAddr,
    IncentivesVaultAddr,
    MigrationBarrier,
    LegacyTotalRemaining,
    ConfigStorage,
    VaultWASM,
    TickRewardPlaneChunk(i32),
    RewardTokenRegistry,
    UserRewardState(soroban_sdk::Address),
    RewardToken,
    RewardBoostToken,
    RewardBoostFeed,
    UserRewardsState(soroban_sdk::Address),
    ExcludedLiquidity,
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum LegacyDataKey {
    UserRewardData(soroban_sdk::Address),
}
#[soroban_sdk::contracttype(export = false)]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum WASMDataKey {
    TokenHash,
    TokenFutureWASM,
    GaugeWASM,
    FutureGaugeWASM,
    ConstantPoolHash,
    StableSwapPoolHash,
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum ConcentratedPoolError {
    Unauthorized = 102,
    PoolAlreadyInitialized = 201,
    PlaneAlreadyInitialized = 202,
    RewardsAlreadyInitialized = 203,
    DepositKilled = 205,
    SwapKilled = 206,
    ClaimKilled = 207,
    TokensNotSorted = 2002,
    InvalidTickRange = 2101,
    InvalidAmount = 2103,
    InvalidSqrtPrice = 2104,
    InvalidTickSpacing = 2106,
    TickOutOfBounds = 2107,
    PriceOutOfBounds = 2108,
    TickNotSpacedCorrectly = 2109,
    TickLowerNotLessThanUpper = 2110,
    TickLowerTooLow = 2111,
    TickUpperTooHigh = 2112,
    InvalidPriceLimit = 2113,
    PositionNotFound = 2118,
    TooManyPositions = 2119,
    LiquidityAmountTooLarge = 2120,
    InsufficientLiquidity = 2121,
    LiquidityOverflow = 2122,
    LiquidityUnderflow = 2123,
    AccumulatorNotActivated = 2124,
    RewardCapExceeded = 2125,
    MigrationBarrierActive = 2126,
    InvalidRewardState = 2127,
    MigrationAlreadyActivated = 2129,
    AmountTooLargeForDistribute = 2130,
    InvalidRewardParams = 2133,
    RewardConfigQueueFull = 2134,
    BoostFactorNotEstablished = 2136,
    VaultWASMNotStaged = 2137,
    IncentivesVaultNotDeployed = 2138,
    GaugesClaimKilled = 2139,
    InvalidGaugeRewardToken = 2140,
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum AccessControlError {
    RoleNotFound = 101,
    Unauthorized = 102,
    AdminAlreadySet = 103,
    BadRoleUsage = 104,
    AnotherActionActive = 2906,
    NoActionActive = 2907,
    ActionNotReadyYet = 2908,
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum ConcentratedRewardsError {
    RewardGrowthOverflow = 2203,
    BoostOverflow = 2204,
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum LiquidityPoolValidationError {
    WrongInputVecSize = 2001,
    FeeOutOfBounds = 2003,
    AllCoinsRequired = 2004,
    InMinNotSatisfied = 2005,
    OutMinNotSatisfied = 2006,
    CannotSwapSameToken = 2007,
    InTokenOutOfBounds = 2008,
    OutTokenOutOfBounds = 2009,
    EmptyPool = 2010,
    InvalidDepositAmount = 2011,
    AdminFeeOutOfBounds = 2012,
    UnknownPoolType = 2013,
    ZeroSharesBurned = 2014,
    TooManySharesBurned = 2015,
    CannotComparePools = 2017,
    ZeroAmount = 2018,
    InsufficientBalance = 2019,
    InMaxNotSatisfied = 2020,
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Error {
    AnotherActionActive = 2906,
    NoActionActive = 2907,
    ActionNotReadyYet = 2908,
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum MathError {
    NumberOverflow = 510,
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum StorageError {
    AlreadyInitialized = 201,
    ValueNotInitialized = 501,
    ValueMissing = 502,
    ValueConversionError = 503,
}
#[soroban_sdk::contractevent(export = false, topics = ["claim_fees"])]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ClaimFees {
    #[topic]
    pub owner: soroban_sdk::Address,
    #[topic]
    pub token0: soroban_sdk::Address,
    #[topic]
    pub token1: soroban_sdk::Address,
    pub amount0: i128,
    pub amount1: i128,
}

