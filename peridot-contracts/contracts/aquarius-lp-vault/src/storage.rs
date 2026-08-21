use soroban_sdk::{contracttype, Address, Env, Symbol};

/// Immutable wiring, written once at `initialize`.
///
/// Everything here lives in a single **instance** storage entry. That is not a
/// style preference: the market-deposit path spans ReceiptVault -> this vault
/// -> the Aquarius pool -> three token contracts, and Soroban caps a
/// transaction at 100 footprint ledger entries. One entry per config field put
/// the end-to-end deposit at 113 and made it unexecutable on-chain.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub pool: Address,
    pub token0: Address,
    pub token1: Address,
    pub dec0: u32,
    pub dec1: u32,
    pub underlying_index: u32,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub oracle: Address,
}

/// Admin-tunable risk parameters. Also a single instance entry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Params {
    /// Cap on underlying deployed into the pool; 0 disables it.
    pub max_deploy: u128,
    pub slippage_bps: u32,
    pub harvest_cooldown: u64,
    pub oracle_max_age_mult: u64,
    /// How long a cached NAV root stays usable without re-reading the oracle.
    ///
    /// This is a footprint budget decision as much as a freshness one: the
    /// ReceiptVault withdraw path already spans six contracts, and pulling two
    /// Reflector prices inside it pushed the transaction over Soroban's
    /// 100-entry cap. The ratio being cached is between two pegged assets, so
    /// it moves slowly; `refresh_nav_root()` forces an update out of band.
    pub nav_root_max_age: u64,
    /// Maximum tolerated gap between the pool's own swap quote and the
    /// oracle-implied fair rate, in basis points. `0` disables the check.
    ///
    /// The per-swap slippage floor is derived from `estimate_swap`, so it only
    /// guards against movement between quote and execution — it cannot tell
    /// that the pool itself is mispriced. Entering a dislocated pool realises
    /// that gap immediately: on testnet a ~7% pool/oracle divergence cost
    /// 4.65% of a deposit. This is the guard for that.
    pub max_pool_divergence_bps: u32,
    pub paused: bool,
}

/// Mutable global accounting. Also a single instance entry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub total_shares: u128,
    pub position_liquidity: u128,
    /// Last good `sqrt(other_price / underlying_price)`, 1e9-scaled.
    pub last_nav_root: u128,
    pub last_nav_root_at: u64,
    pub last_harvest: u64,
}

#[contracttype]
pub enum DataKey {
    // instance
    Config,
    Params,
    State,
    Admin,
    Initialized,
    // persistent
    PendingAdmin,
    PendingUpgradeHash,
    PendingUpgradeEta,
    Shares(Address),
    RewardRoute(Address),
    OracleSymbol(Address),
}

const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND_TO: u32 = 1_000_000;
const DAY_IN_LEDGERS: u32 = 17_280;
const SHARE_TTL_EXTEND_TO: u32 = 5_000_000;
const SHARE_TTL_THRESHOLD: u32 = SHARE_TTL_EXTEND_TO - DAY_IN_LEDGERS;

/// One `extend_ttl` covers Config, Params, State and Admin together.
pub fn bump_critical_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn bump_share_ttl(env: &Env, owner: &Address) {
    let key = DataKey::Shares(owner.clone());
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, SHARE_TTL_THRESHOLD, SHARE_TTL_EXTEND_TO);
    }
}

pub fn bump_pending_admin_ttl(env: &Env) {
    let key = DataKey::PendingAdmin;
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_pending_upgrade_ttl(env: &Env) {
    let p = env.storage().persistent();
    for key in [DataKey::PendingUpgradeHash, DataKey::PendingUpgradeEta] {
        if p.has(&key) {
            p.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
    }
}

pub fn config(env: &Env) -> Config {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .expect("vault not initialized")
}

pub fn set_config(env: &Env, cfg: &Config) {
    env.storage().instance().set(&DataKey::Config, cfg);
}

pub fn params(env: &Env) -> Params {
    env.storage()
        .instance()
        .get(&DataKey::Params)
        .expect("vault not initialized")
}

pub fn set_params(env: &Env, p: &Params) {
    env.storage().instance().set(&DataKey::Params, p);
}

pub fn state(env: &Env) -> State {
    env.storage()
        .instance()
        .get(&DataKey::State)
        .unwrap_or(State {
            total_shares: 0,
            position_liquidity: 0,
            last_nav_root: 0,
            last_nav_root_at: 0,
            last_harvest: 0,
        })
}

pub fn set_state(env: &Env, s: &State) {
    env.storage().instance().set(&DataKey::State, s);
}

pub fn admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .expect("admin not set")
}

pub fn shares_of(env: &Env, owner: &Address) -> u128 {
    env.storage()
        .persistent()
        .get(&DataKey::Shares(owner.clone()))
        .unwrap_or(0u128)
}

pub fn set_shares(env: &Env, owner: &Address, amount: u128) {
    let key = DataKey::Shares(owner.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
        bump_share_ttl(env, owner);
    }
}

pub fn oracle_symbol(env: &Env, token: &Address) -> Option<Symbol> {
    env.storage()
        .persistent()
        .get(&DataKey::OracleSymbol(token.clone()))
}
