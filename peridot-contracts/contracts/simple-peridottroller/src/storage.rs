use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
pub enum DataKey {
    Admin,
    PendingAdmin,
    Initialized,
    PauseGuardian,                // Address (optional)
    PauseExpiryMigrationDone,     // bool: legacy pause-expiry migration completed
    PauseExpiryMigrationCursor,   // u32: next supported-market index to migrate
    SupportedMarkets,             // Map<Address, bool>
    UserMarkets(Address),         // Vec<Address>
    Oracle,                       // Address
    CloseFactorScaled,            // u128 scaled 1e6
    LiquidationIncentiveScaled,   // u128 scaled 1e6
    MarginLiquidationControllers, // Map<Address, bool>
    ReserveRecipient,             // Address for liquidation fee pTokens
    PauseBorrow,                  // Map<Address, bool>
    PauseBorrowUntil,             // Map<Address, u64> pause expiry
    PauseRedeem,                  // Map<Address, bool>
    PauseRedeemUntil,             // Map<Address, u64> pause expiry
    PauseLiquidation,             // Map<Address, bool>
    PauseLiquidationUntil,        // Map<Address, u64> pause expiry
    PauseDeposit,                 // Map<Address, bool>
    PauseDepositUntil,            // Map<Address, u64> pause expiry
    LiquidationFeeScaled,         // u128 scaled 1e6, portion to reserves
    OracleMaxAgeMultiplier,       // u64 multiplier of resolution (default 2)
    OracleAssetSymbol(Address),   // Optional Reflector symbol override
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
    FallbackPriceSetAt(Address), // u64 timestamp when fallback was set
    SupportedToken(Address),     // bool: token belongs to at least one supported market
    MarketUnderlying(Address),   // Address: cached market -> underlying token
    MarketZeroTotalsVerifiedAt(Address), // u64 timestamp for emergency delist gating
    BoostedVaultOwner(Address),  // Address: boosted vault -> owning receipt-vault
    MarketUserCounts,            // Map<Address, u32>: number of users with market in UserMarkets
    PendingUpgradeHash,          // BytesN<32>: timelocked controller upgrade target
    PendingUpgradeEta,           // u64: earliest timestamp when upgrade can execute
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

const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND_TO: u32 = 1_000_000;
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
        persistent.extend_ttl(
            &DataKey::LiquidationIncentiveScaled,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }
    if persistent.has(&DataKey::ReserveRecipient) {
        persistent.extend_ttl(&DataKey::ReserveRecipient, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::LiquidationFeeScaled) {
        persistent.extend_ttl(&DataKey::LiquidationFeeScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::OracleMaxAgeMultiplier) {
        persistent.extend_ttl(
            &DataKey::OracleMaxAgeMultiplier,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
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

pub fn bump_pending_upgrade_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::PendingUpgradeHash) {
        persistent.extend_ttl(&DataKey::PendingUpgradeHash, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PendingUpgradeEta) {
        persistent.extend_ttl(&DataKey::PendingUpgradeEta, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_pending_admin_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::PendingAdmin) {
        persistent.extend_ttl(&DataKey::PendingAdmin, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_pause_guardian_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::PauseGuardian) {
        persistent.extend_ttl(&DataKey::PauseGuardian, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_margin_liquidation_controllers_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::MarginLiquidationControllers) {
        persistent.extend_ttl(
            &DataKey::MarginLiquidationControllers,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }
}

pub fn bump_user_markets_ttl(env: &Env, user: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::UserMarkets(user.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_market_cf_ttl(env: &Env, market: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::MarketCF(market.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_price_cache_ttl(env: &Env, token: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::PriceCache(token.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_fallback_price_ttl(env: &Env, token: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::FallbackPrice(token.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_fallback_price_set_at_ttl(env: &Env, token: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::FallbackPriceSetAt(token.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_oracle_asset_symbol_ttl(env: &Env, token: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::OracleAssetSymbol(token.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_supported_token_ttl(env: &Env, token: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::SupportedToken(token.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_market_underlying_ttl(env: &Env, market: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::MarketUnderlying(market.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_market_zero_totals_verified_ttl(env: &Env, market: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::MarketZeroTotalsVerifiedAt(market.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_boosted_vault_owner_ttl(env: &Env, boosted_vault: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::BoostedVaultOwner(boosted_vault.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_reward_market_ttl(env: &Env, market: &Address) {
    let persistent = env.storage().persistent();
    let keys = [
        DataKey::SupplySpeed(market.clone()),
        DataKey::BorrowSpeed(market.clone()),
        DataKey::SupplyIndex(market.clone()),
        DataKey::BorrowIndex(market.clone()),
        DataKey::SupplyIndexTime(market.clone()),
        DataKey::BorrowIndexTime(market.clone()),
    ];
    for key in keys {
        if persistent.has(&key) {
            persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
    }
}

pub fn bump_reward_user_ttl(env: &Env, user: &Address, market: &Address) {
    let persistent = env.storage().persistent();
    let keys = [
        DataKey::UserSupplyIndex(user.clone(), market.clone()),
        DataKey::UserBorrowIndex(user.clone(), market.clone()),
        DataKey::Accrued(user.clone()),
    ];
    for key in keys {
        if persistent.has(&key) {
            persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
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
