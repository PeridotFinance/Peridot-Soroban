use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    PauseGuardian,              // Address (optional)
    SupportedMarkets,           // Map<Address, bool>
    UserMarkets(Address),       // Vec<Address>
    Oracle,                     // Address
    CloseFactorScaled,          // u128 scaled 1e6
    LiquidationIncentiveScaled, // u128 scaled 1e6
    ReserveRecipient,           // Address for liquidation fee pTokens
    PauseBorrow,                // Map<Address, bool>
    PauseRedeem,                // Map<Address, bool>
    PauseLiquidation,           // Map<Address, bool>
    PauseDeposit,               // Map<Address, bool>
    LiquidationFeeScaled,       // u128 scaled 1e6, portion to reserves
    OracleMaxAgeMultiplier,     // u64 multiplier of resolution (default 2)
    OracleAssetSymbol(Address), // Optional Reflector symbol override
    // Collateral factors per market (scaled 1e6)
    MarketCF(Address),
    // Rewards
    PeridotToken,
    SupplySpeed(Address),
    BorrowSpeed(Address),
    SupplyIndex(Address),
    BorrowIndex(Address),
    SupplyIndexTime(Address),
    BorrowIndexTime(Address),
    UserSupplyIndex(Address, Address),
    UserBorrowIndex(Address, Address),
    Accrued(Address),
    PriceCache(Address),
    FallbackPrice(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccrualHint {
    pub total_ptokens: Option<u128>,
    pub total_borrowed: Option<u128>,
    pub user_ptokens: Option<u128>,
    pub user_borrowed: Option<u128>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketLiquidityHint {
    pub ptoken_balance: u128,
    pub user_borrowed: u128,
    pub exchange_rate: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeizeContext {
    pub liquidity: u128,
    pub shortfall: u128,
    pub max_redeem_ptokens: u128,
    pub seize_ptokens: u128,
    pub fee_recipient: Option<Address>,
    pub fee_ptokens: u128,
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedPrice {
    pub price: u128,
    pub scale: u128,
    pub timestamp: u64,
    pub resolution: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FallbackPrice {
    pub price: u128,
    pub scale: u128,
}

pub fn require_admin(env: Env) {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");
    bump_core_ttl(&env);
    admin.require_auth();
}

const TTL_THRESHOLD: u32 = 100_000_000;
const TTL_EXTEND_TO: u32 = 200_000_000;
const MAX_DECIMALS: u32 = 38;

pub fn bump_core_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::Admin) {
        persistent.extend_ttl(&DataKey::Admin, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::SupportedMarkets) {
        persistent.extend_ttl(&DataKey::SupportedMarkets, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Oracle) {
        persistent.extend_ttl(&DataKey::Oracle, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::CloseFactorScaled) {
        persistent.extend_ttl(&DataKey::CloseFactorScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::LiquidationIncentiveScaled) {
        persistent.extend_ttl(&DataKey::LiquidationIncentiveScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::ReserveRecipient) {
        persistent.extend_ttl(&DataKey::ReserveRecipient, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PauseBorrow) {
        persistent.extend_ttl(&DataKey::PauseBorrow, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PauseRedeem) {
        persistent.extend_ttl(&DataKey::PauseRedeem, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PauseLiquidation) {
        persistent.extend_ttl(&DataKey::PauseLiquidation, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PauseDeposit) {
        persistent.extend_ttl(&DataKey::PauseDeposit, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::LiquidationFeeScaled) {
        persistent.extend_ttl(&DataKey::LiquidationFeeScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::OracleMaxAgeMultiplier) {
        persistent.extend_ttl(&DataKey::OracleMaxAgeMultiplier, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PeridotToken) {
        persistent.extend_ttl(&DataKey::PeridotToken, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if env.storage().instance().has(&DataKey::Initialized) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn pow10_u128(decimals: u32) -> u128 {
    if decimals > MAX_DECIMALS {
        panic!("decimals too large");
    }
    let mut result: u128 = 1;
    let mut i = 0u32;
    while i < decimals {
        result = result.saturating_mul(10);
        i += 1;
    }
    result
}
