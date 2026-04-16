use soroban_sdk::{contracttype, Address, Env, IntoVal};
use stellar_tokens::fungible::Base as TokenBase;

// Storage key types for the contract
#[contracttype]
pub enum DataKey {
    UnderlyingToken,
    ManagedCash, // u128 internal cash accounting; excludes direct donations
    TotalDeposited,
    InterestRatePerSecond, // u128, scaled by 1_000_000 (6 decimals)
    LastUpdateTime,        // u64
    AccumulatedInterest,   // u128
    YearlyRateScaled,      // u128, scaled by 1_000_000 (6 decimals)
    InitialExchangeRate,   // u128, scaled 1e6
    // Borrowing-related keys
    BorrowSnapshots(Address),      // BorrowSnapshot per user
    HasBorrowed(Address),          // bool flag per user
    TotalBorrowed,                 // u128
    BorrowIndex,                   // u128 (scaled 1e18)
    BorrowYearlyRateScaled,        // u128, scaled 1e6
    CollateralFactorScaled,        // u128, scaled 1e6 (e.g., 500_000 = 50%)
    Admin,                         // Address
    PendingAdmin,                  // Address pending acceptance
    Peridottroller,                // Address (optional)
    InterestModel,                 // Address (optional)
    ReserveFactorScaled,           // u128 (scaled 1e6), defaults 0
    AdminFeeScaled,                // u128 (scaled 1e6), defaults 0
    FlashLoanFeeScaled,            // u128 (scaled 1e6), defaults 0
    TotalAdminFees,                // u128 accumulated admin fees
    TotalReserves,                 // u128 accumulated reserves
    SupplyCap,                     // u128, max total underlying (principal + interest)
    BorrowCap,                     // u128, max total borrowed
    Initialized,                   // bool flag to prevent re-initialization
    BoostedVault,                  // Optional DeFindex vault address for boosted markets
    BoostedUnderlyingCached,       // u128 cached underlying amount for boosted vault
    BoostedUnderlyingUpdatedAt,    // u64 timestamp of cached boosted underlying
    TotalBorrowPrincipal,          // u128 principal-only global borrow total
    RatesReady,                    // bool, borrow/rate-sensitive operations enabled
    IdleCashBufferBps,             // u32, target idle cash in basis points (0..=10_000)
    FlashLoanActive,               // bool reentrancy guard for accounting-sensitive paths
    MarginController,              // Address (optional), enforces margin collateral locks
    MarginWithdrawBypass(Address), // bool one-shot bypass for margin-controller-managed withdraw
    PendingUpgradeHash,            // BytesN<32> target wasm hash for timelocked upgrade
    PendingUpgradeEta,             // u64 unix timestamp when upgrade becomes executable
}

const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND_TO: u32 = 1_000_000;
const BORROW_SNAPSHOT_TTL_THRESHOLD: u32 = 500_000;
const BORROW_SNAPSHOT_TTL_EXTEND_TO: u32 = 1_000_000;
const HAS_BORROWED_TTL_THRESHOLD: u32 = 500_000;
const HAS_BORROWED_TTL_EXTEND_TO: u32 = 1_000_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketLiquidityHint {
    pub ptoken_balance: u128,
    pub user_borrowed: u128,
    pub exchange_rate: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowSnapshot {
    pub principal: u128,
    pub interest_index: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ControllerAccrualHint {
    pub total_ptokens: Option<u128>,
    pub total_borrowed: Option<u128>,
    pub user_ptokens: Option<u128>,
    pub user_borrowed: Option<u128>,
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

pub fn ensure_initialized(env: &Env) -> Address {
    bump_core_ttl(env);
    bump_borrow_state_ttl(env);
    let persistent = env.storage().persistent();
    let token: Address = persistent
        .get(&DataKey::UnderlyingToken)
        .expect("Vault not initialized");
    if !persistent
        .get::<_, bool>(&DataKey::Initialized)
        .unwrap_or(false)
    {
        panic!("Vault not initialized");
    }
    token
}

pub fn bump_core_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::Admin) {
        persistent.extend_ttl(&DataKey::Admin, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PendingAdmin) {
        persistent.extend_ttl(&DataKey::PendingAdmin, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::UnderlyingToken) {
        persistent.extend_ttl(&DataKey::UnderlyingToken, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::ManagedCash) {
        persistent.extend_ttl(&DataKey::ManagedCash, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Initialized) {
        persistent.extend_ttl(&DataKey::Initialized, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::CollateralFactorScaled) {
        persistent.extend_ttl(
            &DataKey::CollateralFactorScaled,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }
    if persistent.has(&DataKey::Peridottroller) {
        persistent.extend_ttl(&DataKey::Peridottroller, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::InterestModel) {
        persistent.extend_ttl(&DataKey::InterestModel, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::ReserveFactorScaled) {
        persistent.extend_ttl(&DataKey::ReserveFactorScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::AdminFeeScaled) {
        persistent.extend_ttl(&DataKey::AdminFeeScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::FlashLoanFeeScaled) {
        persistent.extend_ttl(&DataKey::FlashLoanFeeScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::TotalAdminFees) {
        persistent.extend_ttl(&DataKey::TotalAdminFees, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::TotalReserves) {
        persistent.extend_ttl(&DataKey::TotalReserves, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::SupplyCap) {
        persistent.extend_ttl(&DataKey::SupplyCap, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::BorrowCap) {
        persistent.extend_ttl(&DataKey::BorrowCap, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::BoostedVault) {
        persistent.extend_ttl(&DataKey::BoostedVault, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::BoostedUnderlyingCached) {
        persistent.extend_ttl(
            &DataKey::BoostedUnderlyingCached,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }
    if persistent.has(&DataKey::BoostedUnderlyingUpdatedAt) {
        persistent.extend_ttl(
            &DataKey::BoostedUnderlyingUpdatedAt,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }
    if persistent.has(&DataKey::InitialExchangeRate) {
        persistent.extend_ttl(&DataKey::InitialExchangeRate, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::FlashLoanActive) {
        persistent.extend_ttl(&DataKey::FlashLoanActive, TTL_THRESHOLD, TTL_EXTEND_TO);
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

pub fn bump_borrow_snapshot_ttl(env: &Env, user: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::BorrowSnapshots(user.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(
            &key,
            BORROW_SNAPSHOT_TTL_THRESHOLD,
            BORROW_SNAPSHOT_TTL_EXTEND_TO,
        );
    }
}

pub fn bump_has_borrowed_ttl(env: &Env, user: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::HasBorrowed(user.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, HAS_BORROWED_TTL_THRESHOLD, HAS_BORROWED_TTL_EXTEND_TO);
    }
}

pub fn bump_user_borrow_state_ttl(env: &Env, user: &Address) {
    bump_borrow_snapshot_ttl(env, user);
    bump_has_borrowed_ttl(env, user);
}

pub fn bump_borrow_state_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::YearlyRateScaled) {
        persistent.extend_ttl(&DataKey::YearlyRateScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::BorrowYearlyRateScaled) {
        persistent.extend_ttl(
            &DataKey::BorrowYearlyRateScaled,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }
    if persistent.has(&DataKey::TotalBorrowed) {
        persistent.extend_ttl(&DataKey::TotalBorrowed, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::BorrowIndex) {
        persistent.extend_ttl(&DataKey::BorrowIndex, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::LastUpdateTime) {
        persistent.extend_ttl(&DataKey::LastUpdateTime, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::AccumulatedInterest) {
        persistent.extend_ttl(&DataKey::AccumulatedInterest, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::TotalDeposited) {
        persistent.extend_ttl(&DataKey::TotalDeposited, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::TotalBorrowPrincipal) {
        persistent.extend_ttl(&DataKey::TotalBorrowPrincipal, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_idle_cash_buffer_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::IdleCashBufferBps) {
        persistent.extend_ttl(&DataKey::IdleCashBufferBps, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_rates_ready_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::RatesReady) {
        persistent.extend_ttl(&DataKey::RatesReady, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn ptoken_balance(env: &Env, addr: &Address) -> u128 {
    let bal = TokenBase::balance(env, addr);
    if bal < 0 {
        panic!("negative ptokens");
    }
    bal as u128
}

pub fn total_ptokens_supply(env: &Env) -> u128 {
    let supply = TokenBase::total_supply(env);
    if supply < 0 {
        panic!("negative supply");
    }
    supply as u128
}

pub fn token_balance(env: &Env, token: &Address, owner: &Address) -> i128 {
    use soroban_sdk::{InvokeError, Symbol, Val, Vec};
    let args: Vec<Val> = (owner.clone(),).into_val(env);
    let sym_balance = Symbol::new(env, "balance");
    match env.try_invoke_contract::<i128, InvokeError>(token, &sym_balance, args.clone()) {
        Ok(Ok(result)) => result,
        _ => {
            let sym_balance_of = Symbol::new(env, "balance_of");
            match env.try_invoke_contract::<i128, InvokeError>(token, &sym_balance_of, args) {
                Ok(Ok(result)) => result,
                _ => panic!("balance lookup failed"),
            }
        }
    }
}

pub fn to_i128(amount: u128) -> i128 {
    if amount > i128::MAX as u128 {
        panic!("amount exceeds i128");
    }
    amount as i128
}
