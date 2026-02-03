use soroban_sdk::{contracttype, Address, Env, IntoVal};
use stellar_tokens::fungible::Base as TokenBase;

// Storage key types for the contract
#[contracttype]
pub enum DataKey {
    UnderlyingToken,
    TotalDeposited,
    InterestRatePerSecond, // u128, scaled by 1_000_000 (6 decimals)
    LastUpdateTime,        // u64
    AccumulatedInterest,   // u128
    YearlyRateScaled,      // u128, scaled by 1_000_000 (6 decimals)
    InitialExchangeRate,   // u128, scaled 1e6
    // Borrowing-related keys
    BorrowSnapshots(Address), // BorrowSnapshot per user
    HasBorrowed(Address),     // bool flag per user
    TotalBorrowed,            // u128
    BorrowIndex,              // u128 (scaled 1e18)
    BorrowYearlyRateScaled,   // u128, scaled 1e6
    CollateralFactorScaled,   // u128, scaled 1e6 (e.g., 500_000 = 50%)
    Admin,                    // Address
    Peridottroller,           // Address (optional)
    InterestModel,            // Address (optional)
    ReserveFactorScaled,      // u128 (scaled 1e6), defaults 0
    AdminFeeScaled,           // u128 (scaled 1e6), defaults 0
    FlashLoanFeeScaled,       // u128 (scaled 1e6), defaults 0
    TotalAdminFees,           // u128 accumulated admin fees
    TotalReserves,            // u128 accumulated reserves
    SupplyCap,                // u128, max total underlying (principal + interest)
    BorrowCap,                // u128, max total borrowed
    Initialized,              // bool flag to prevent re-initialization
    BoostedVault,             // Optional DeFindex vault address for boosted markets
}

const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND_TO: u32 = 200_000;

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
    let token: Address = env
        .storage()
        .persistent()
        .get(&DataKey::UnderlyingToken)
        .expect("Vault not initialized");
    if !env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::Initialized)
        .unwrap_or(false)
    {
        env.storage().persistent().set(&DataKey::Initialized, &true);
    }
    bump_core_ttl(env);
    token
}

pub fn bump_core_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::Admin) {
        persistent.extend_ttl(&DataKey::Admin, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::UnderlyingToken) {
        persistent.extend_ttl(&DataKey::UnderlyingToken, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Initialized) {
        persistent.extend_ttl(&DataKey::Initialized, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_borrow_snapshot_ttl(env: &Env, user: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::BorrowSnapshots(user.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_has_borrowed_ttl(env: &Env, user: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::HasBorrowed(user.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_borrow_state_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::YearlyRateScaled) {
        persistent.extend_ttl(&DataKey::YearlyRateScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::BorrowYearlyRateScaled) {
        persistent.extend_ttl(&DataKey::BorrowYearlyRateScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
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
