#![no_std]
//! Price router — a Reflector-shaped oracle that no single source can dictate.
//!
//! Implements Reflector's `lastprice` / `resolution` surface, so anything that
//! already speaks to Reflector (notably `aquarius-lp-vault`) can be pointed at
//! this instead with a plain `set_oracle` and no code change.
//!
//! It exists for two reasons:
//!
//! 1. **Coverage.** Reflector does not price every asset. yXLM and PYUSD, for
//!    example, are absent from the external feed. Both can instead be bounded
//!    against a supported reference asset with an executable pool quote.
//!
//! 2. **Containment — the more important one.** Reflector's Stellar-native feed
//!    derives from on-chain DEX venues, so for a thinly traded asset "Reflector
//!    supports it" is *not* the same as "this price is safe": manipulate the
//!    venue and you manipulate the oracle, and every consumer inherits it. That
//!    has already caused real losses in this ecosystem. So this router treats
//!    **every** source as untrusted and bounds all of them the same way.
//!
//! The strongest bound available is the peg clamp. For an asset that is a
//! wrapper or claim on another (yXLM → XLM), the true price can never exceed
//! the thing it wraps. `min(peg, observed)` therefore caps the upside
//! *regardless of where the bad number came from* — manipulated pool spot, a
//! compromised keeper, or a compromised upstream oracle. Over-valuation is the
//! direction that produces bad debt, so that is the direction to make
//! impossible by construction.

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, Address, BytesN, Env, IntoVal, String,
    Symbol, Val, Vec,
};

pub const DEFAULT_INIT_ADMIN: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";

/// Basis-point denominator.
const BPS: u128 = 10_000u128;
/// Upgrade timelock, matching the other Peridot contracts.
const UPGRADE_TIMELOCK_SECS: u64 = 24 * 60 * 60;

const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND_TO: u32 = 1_000_000;
/// Renew configured mappings only when genuinely near expiry — a high threshold
/// re-bumps on every read and the rent charge lands on the caller's budget.
const MAPPING_TTL_THRESHOLD: u32 = 1_000_000;
const MAPPING_TTL_EXTEND_TO: u32 = 5_000_000;

// ─────────────────────────────────────────────────────────────────────────────
// Reflector-compatible surface
// ─────────────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

#[soroban_sdk::contractclient(name = "UpstreamOracleClient")]
pub trait UpstreamOracle {
    fn lastprice(e: Env, asset: Asset) -> Option<PriceData>;
    fn resolution(e: Env) -> u32;
    fn decimals(e: Env) -> u32;
}

/// Minimal view of an Aquarius pool. Only `estimate_swap` is used: an
/// executable quote reflects real depth, and probing costs no state.
#[soroban_sdk::contractclient(name = "PoolClient")]
pub trait ObservationPool {
    fn estimate_swap(e: Env, in_idx: u32, out_idx: u32, in_amount: u128) -> u128;
}

// ─────────────────────────────────────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Observation of a pegged asset against the thing it is pegged to.
#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PegConfig {
    /// The asset this one is a claim on. Its price is the ceiling.
    pub peg_to: Address,
    /// Aquarius pool used to observe the ratio. The `Option` is retained for
    /// storage compatibility, but `None` never produces a price: without an
    /// executable observation the router cannot safely vouch for the peg.
    pub pool: Option<Address>,
    /// Index of *this* asset in the pool's sorted token vector.
    pub in_idx: u32,
    /// Index of `peg_to` in that vector.
    pub out_idx: u32,
    /// Probe size for `estimate_swap`, in this asset's raw units. Large enough
    /// to be meaningful against pool depth, small enough not to price in its
    /// own slippage.
    pub probe_amount: u128,
    /// Floor, in basis points below the peg. If the observed ratio falls below
    /// this the router returns `None` (halt) rather than publishing a
    /// catastrophic mark-down. A consumer that cannot get a price degrades
    /// safely; one that gets a very wrong price does not.
    pub min_ratio_bps: u32,
}

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PriceSource {
    /// Forward to the configured upstream oracle unchanged.
    Upstream,
    /// A price pushed on-chain by the admin/keeper, bounded by `PushGuard`.
    Pushed,
    /// Priced as a claim on another asset: `min(peg_price, observed)`.
    Pegged(PegConfig),
}

/// Bounds applied to every pushed price. A push oracle puts a key into the
/// collateral path, so it is rate-limited as well as staleness-bounded: a
/// single compromised or fat-fingered update cannot move the price far.
#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PushGuard {
    /// Maximum move per update, in basis points, against the previous value.
    pub max_step_bps: u32,
    /// A pushed price older than this is treated as absent.
    pub max_age_secs: u64,
}

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PushedPrice {
    pub price: i128,
    pub timestamp: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    PendingAdmin,
    Initialized,
    Upstream,
    Resolution,
    PushGuardKey,
    PendingUpgradeHash,
    PendingUpgradeEta,
    Source(Address),
    Pushed(Address),
    /// Maps a Reflector `Other(Symbol)` encoding onto a contract address, so
    /// symbol-keyed feeds and address-keyed config agree.
    SymbolFor(Symbol),
    /// Reverse lookup used when a pegged asset's reference price is published
    /// by Reflector as `Other(Symbol)` rather than `Stellar(Address)`.
    SymbolOf(Address),
}

// ─────────────────────────────────────────────────────────────────────────────
// Events
// ─────────────────────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouterInitialized {
    pub admin: Address,
    pub upstream: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSet {
    pub asset: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PricePushed {
    pub asset: Address,
    pub price: i128,
}

/// The peg clamp bound the price below the observed ratio — i.e. the asset is
/// trading under its peg and the router marked it down. Monitor this.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PegClamped {
    pub asset: Address,
    pub observed_bps: u32,
}

/// Observed ratio fell through the configured floor and the router refused to
/// publish a price at all. This is the depeg alarm.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PegFloorBreached {
    pub asset: Address,
    pub observed_bps: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferProposed {
    pub current_admin: Address,
    pub pending_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferred {
    pub previous_admin: Address,
    pub new_admin: Address,
}

// ─────────────────────────────────────────────────────────────────────────────

fn try_mul_div(a: u128, b: u128, denom: u128) -> Option<u128> {
    if denom == 0 {
        return None;
    }
    if a == 0 || b == 0 {
        return Some(0);
    }
    // Reduce against the denominator before multiplying so the intermediate
    // product cannot overflow.
    fn gcd(mut a: u128, mut b: u128) -> u128 {
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        a
    }
    let mut left = a;
    let mut right = b;
    let mut d = denom;
    let g1 = gcd(left, d);
    left /= g1;
    d /= g1;
    let g2 = gcd(right, d);
    right /= g2;
    d /= g2;
    left.checked_mul(right).map(|num| num / d)
}

fn to_u128(v: i128) -> u128 {
    if v <= 0 {
        0
    } else {
        v as u128
    }
}

fn try_to_i128(v: u128) -> Option<i128> {
    if v > i128::MAX as u128 {
        None
    } else {
        Some(v as i128)
    }
}

#[contract]
pub struct PriceRouter;

#[cfg(all(feature = "test-default-admin", target_arch = "wasm32"))]
compile_error!("price-router test-default-admin must not be enabled for Wasm builds");

#[contractimpl]
impl PriceRouter {
    // ── Internals ─────────────────────────────────────────────────────────

    fn admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not set")
    }

    fn require_admin(env: &Env, who: &Address) {
        if Self::admin(env) != *who {
            panic!("not admin");
        }
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
        who.require_auth();
    }

    fn upstream(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Upstream)
            .expect("upstream not set")
    }

    fn bump_mapping(env: &Env, key: &DataKey) {
        if env.storage().persistent().has(key) {
            env.storage().persistent().extend_ttl(
                key,
                MAPPING_TTL_THRESHOLD,
                MAPPING_TTL_EXTEND_TO,
            );
        }
    }

    /// Resolves a Reflector-style asset to the contract address this router
    /// keys its configuration on.
    fn asset_address(env: &Env, asset: &Asset) -> Option<Address> {
        match asset {
            Asset::Stellar(a) => Some(a.clone()),
            Asset::Other(sym) => {
                let key = DataKey::SymbolFor(sym.clone());
                Self::bump_mapping(env, &key);
                env.storage().persistent().get(&key)
            }
        }
    }

    /// Selects the encoding the upstream oracle actually uses for an asset.
    /// Reflector's external feed publishes assets such as XLM and USDC as
    /// `Other(Symbol)`, while router configuration is address-keyed. Keeping
    /// the reverse mapping lets pegged assets such as yXLM and PYUSD reference
    /// those feeds without weakening the address-based source registry.
    fn upstream_asset(env: &Env, asset: &Address) -> Asset {
        let key = DataKey::SymbolOf(asset.clone());
        Self::bump_mapping(env, &key);
        match env.storage().persistent().get::<_, Symbol>(&key) {
            Some(symbol) => Asset::Other(symbol),
            None => Asset::Stellar(asset.clone()),
        }
    }

    fn source_of(env: &Env, asset: &Address) -> PriceSource {
        let key = DataKey::Source(asset.clone());
        Self::bump_mapping(env, &key);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(PriceSource::Upstream)
    }

    /// Reads the upstream oracle, returning `None` on any failure rather than
    /// propagating. A dead upstream must not take the router down with it.
    fn upstream_price(env: &Env, asset: &Asset) -> Option<PriceData> {
        let up = Self::upstream(env);
        let args: Vec<Val> = (asset.clone(),).into_val(env);
        match env.try_invoke_contract::<Option<PriceData>, soroban_sdk::InvokeError>(
            &up,
            &Symbol::new(env, "lastprice"),
            args,
        ) {
            Ok(Ok(v)) => v,
            _ => None,
        }
    }

    /// Observed ratio of `asset` against its peg, in basis points, from an
    /// executable quote.
    ///
    /// Uses `estimate_swap` rather than spot price deliberately: an executable
    /// quote prices in real depth, so a manipulator has to actually move
    /// liquidity rather than nudge a tick. It also slightly *understates* the
    /// ratio because the probe pays its own slippage — and understating is the
    /// safe direction for a `min()` clamp.
    fn observed_ratio_bps(env: &Env, cfg: &PegConfig) -> Option<u32> {
        let pool = cfg.pool.clone()?;
        if cfg.probe_amount == 0 {
            return None;
        }
        let args: Vec<Val> = (cfg.in_idx, cfg.out_idx, cfg.probe_amount).into_val(env);
        let out: u128 = match env.try_invoke_contract::<u128, soroban_sdk::InvokeError>(
            &pool,
            &Symbol::new(env, "estimate_swap"),
            args,
        ) {
            Ok(Ok(v)) => v,
            _ => return None,
        };
        if out == 0 {
            return None;
        }
        let bps = try_mul_div(out, BPS, cfg.probe_amount)?;
        if bps > u32::MAX as u128 {
            return Some(u32::MAX);
        }
        Some(bps as u32)
    }

    // ── Reflector-compatible reads ────────────────────────────────────────

    /// Price for `asset`, or `None` if the router will not vouch for one.
    ///
    /// Returning `None` is a deliberate outcome, not just an error path:
    /// a consumer that cannot get a price degrades safely, whereas one that
    /// gets a confidently wrong price does not.
    pub fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
        let addr = Self::asset_address(&env, &asset)?;
        match Self::source_of(&env, &addr) {
            PriceSource::Upstream => {
                let upstream_asset = Self::upstream_asset(&env, &addr);
                Self::upstream_price(&env, &upstream_asset)
            }
            PriceSource::Pushed => Self::pushed_price(&env, &addr),
            PriceSource::Pegged(cfg) => Self::pegged_price(&env, &addr, &cfg),
        }
    }

    fn pushed_price(env: &Env, asset: &Address) -> Option<PriceData> {
        let key = DataKey::Pushed(asset.clone());
        Self::bump_mapping(env, &key);
        let stored: PushedPrice = env.storage().persistent().get(&key)?;
        let guard = Self::push_guard(env);
        let now = env.ledger().timestamp();
        if stored.timestamp > now {
            return None;
        }
        if now.saturating_sub(stored.timestamp) > guard.max_age_secs {
            return None;
        }
        Some(PriceData {
            price: stored.price,
            timestamp: stored.timestamp,
        })
    }

    /// `min(peg_price, peg_price * observed_ratio)`, floored.
    ///
    /// The clamp is the point. It holds no matter which source is lying:
    /// manipulated pool spot, a compromised keeper, or a compromised upstream
    /// oracle all get capped at the peg. Over-valuation is what produces bad
    /// debt, so it is made impossible rather than merely unlikely.
    fn pegged_price(env: &Env, asset: &Address, cfg: &PegConfig) -> Option<PriceData> {
        let peg_asset = Self::upstream_asset(env, &cfg.peg_to);
        let peg = Self::upstream_price(env, &peg_asset)?;
        if peg.price <= 0 {
            return None;
        }
        let peg_price = to_u128(peg.price);

        // An unavailable pool quote is not evidence that the asset is still
        // worth the configured floor. Fail closed so a pool outage cannot hide
        // a depeg and leave the lending markets with an assumed collateral
        // price.
        let ratio_bps = Self::observed_ratio_bps(env, cfg)?;

        if (ratio_bps as u128) < cfg.min_ratio_bps as u128 {
            PegFloorBreached {
                asset: asset.clone(),
                observed_bps: ratio_bps,
            }
            .publish(env);
            return None;
        }

        // Never above the peg, whatever the observation says.
        let effective_bps = if (ratio_bps as u128) > BPS {
            BPS
        } else {
            ratio_bps as u128
        };
        if effective_bps < BPS {
            PegClamped {
                asset: asset.clone(),
                observed_bps: ratio_bps,
            }
            .publish(env);
        }

        let price = try_to_i128(try_mul_div(peg_price, effective_bps, BPS)?)?;
        Some(PriceData {
            price,
            timestamp: peg.timestamp,
        })
    }

    pub fn resolution(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Resolution)
            .unwrap_or(300u32)
    }

    pub fn decimals(env: Env) -> u32 {
        let up = Self::upstream(&env);
        match env.try_invoke_contract::<u32, soroban_sdk::InvokeError>(
            &up,
            &Symbol::new(&env, "decimals"),
            Vec::new(&env),
        ) {
            Ok(Ok(v)) => v,
            _ => 14u32,
        }
    }

    // ── Keeper ────────────────────────────────────────────────────────────

    /// Pushes a price for an asset configured as `Pushed`.
    ///
    /// Rate-limited against the previous value by `max_step_bps`: a push oracle
    /// puts a key into the collateral path, so one bad update — fat finger or
    /// compromise — must not be able to move the price far. Blending SDEX and
    /// AMM observations off-chain is fine; the bound here is what makes it
    /// survivable when that blend is wrong.
    pub fn push_price(env: Env, caller: Address, asset: Address, price: i128) {
        Self::require_admin(&env, &caller);
        if price <= 0 {
            panic!("price must be positive");
        }
        let key = DataKey::Pushed(asset.clone());
        let guard = Self::push_guard(&env);
        if let Some(prev) = env.storage().persistent().get::<_, PushedPrice>(&key) {
            let prev_p = to_u128(prev.price);
            let new_p = to_u128(price);
            let lo = try_mul_div(prev_p, BPS - guard.max_step_bps.min(9_999) as u128, BPS)
                .unwrap_or_else(|| panic!("push guard math overflow"));
            let hi = try_mul_div(prev_p, BPS + guard.max_step_bps as u128, BPS)
                .unwrap_or_else(|| panic!("push guard math overflow"));
            if new_p < lo || new_p > hi {
                panic!("price step exceeds guard");
            }
        }
        env.storage().persistent().set(
            &key,
            &PushedPrice {
                price,
                timestamp: env.ledger().timestamp(),
            },
        );
        Self::bump_mapping(&env, &key);
        PricePushed { asset, price }.publish(&env);
    }

    fn push_guard(env: &Env) -> PushGuard {
        env.storage()
            .instance()
            .get(&DataKey::PushGuardKey)
            .unwrap_or(PushGuard {
                max_step_bps: 500,
                max_age_secs: 3_600,
            })
    }

    // ── Administration ────────────────────────────────────────────────────

    pub fn initialize(env: Env, admin: Address, upstream: Address, resolution: u32) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        if resolution == 0 {
            panic!("resolution must be positive");
        }
        let expected = Address::from_string(&String::from_str(&env, expected_admin_config()));
        if admin != expected {
            panic!("unexpected admin");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Upstream, &upstream);
        env.storage()
            .instance()
            .set(&DataKey::Resolution, &resolution);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
        RouterInitialized { admin, upstream }.publish(&env);
    }

    pub fn set_source(env: Env, caller: Address, asset: Address, source: PriceSource) {
        Self::require_admin(&env, &caller);
        if let PriceSource::Pegged(ref cfg) = source {
            if cfg.min_ratio_bps as u128 > BPS {
                panic!("floor above peg");
            }
            if cfg.in_idx == cfg.out_idx {
                panic!("indices must differ");
            }
        }
        let key = DataKey::Source(asset.clone());
        env.storage().persistent().set(&key, &source);
        Self::bump_mapping(&env, &key);
        SourceSet { asset }.publish(&env);
    }

    /// Maps a Reflector `Other(Symbol)` encoding onto a contract address.
    pub fn set_symbol_asset(env: Env, caller: Address, symbol: Symbol, asset: Option<Address>) {
        Self::require_admin(&env, &caller);
        let key = DataKey::SymbolFor(symbol.clone());
        let previous: Option<Address> = env.storage().persistent().get(&key);
        if let Some(previous_asset) = previous {
            let reverse_key = DataKey::SymbolOf(previous_asset);
            let reverse: Option<Symbol> = env.storage().persistent().get(&reverse_key);
            if reverse == Some(symbol.clone()) {
                env.storage().persistent().remove(&reverse_key);
            }
        }
        match asset {
            Some(a) => {
                let reverse_key = DataKey::SymbolOf(a.clone());
                let old_symbol: Option<Symbol> = env.storage().persistent().get(&reverse_key);
                if let Some(old_symbol) = old_symbol {
                    if old_symbol != symbol {
                        let old_forward_key = DataKey::SymbolFor(old_symbol);
                        let old_forward: Option<Address> =
                            env.storage().persistent().get(&old_forward_key);
                        if old_forward == Some(a.clone()) {
                            env.storage().persistent().remove(&old_forward_key);
                        }
                    }
                }
                env.storage().persistent().set(&key, &a);
                Self::bump_mapping(&env, &key);
                env.storage().persistent().set(&reverse_key, &symbol);
                Self::bump_mapping(&env, &reverse_key);
            }
            None => env.storage().persistent().remove(&key),
        }
    }

    pub fn set_upstream(env: Env, caller: Address, upstream: Address) {
        Self::require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::Upstream, &upstream);
    }

    pub fn set_push_guard(env: Env, caller: Address, guard: PushGuard) {
        Self::require_admin(&env, &caller);
        if guard.max_step_bps as u128 >= BPS {
            panic!("step guard must be below 100%");
        }
        env.storage().instance().set(&DataKey::PushGuardKey, &guard);
    }

    pub fn set_resolution(env: Env, caller: Address, resolution: u32) {
        Self::require_admin(&env, &caller);
        if resolution == 0 {
            panic!("resolution must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::Resolution, &resolution);
    }

    pub fn get_admin(env: Env) -> Address {
        Self::admin(&env)
    }

    pub fn get_upstream(env: Env) -> Address {
        Self::upstream(&env)
    }

    pub fn get_source(env: Env, asset: Address) -> PriceSource {
        Self::source_of(&env, &asset)
    }

    pub fn get_push_guard(env: Env) -> PushGuard {
        Self::push_guard(&env)
    }

    /// Observed peg ratio in basis points, for monitoring. `None` if the asset
    /// is not pegged or the pool cannot be read.
    pub fn get_observed_ratio_bps(env: Env, asset: Address) -> Option<u32> {
        match Self::source_of(&env, &asset) {
            PriceSource::Pegged(cfg) => Self::observed_ratio_bps(&env, &cfg),
            _ => None,
        }
    }

    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        Self::require_admin(&env, &caller);
        if caller == new_admin {
            panic!("admin unchanged");
        }
        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &new_admin);
        AdminTransferProposed {
            current_admin: caller,
            pending_admin: new_admin,
        }
        .publish(&env);
    }

    pub fn accept_admin(env: Env) {
        let pending: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .expect("no pending admin");
        pending.require_auth();
        let previous = Self::admin(&env);
        env.storage().instance().set(&DataKey::Admin, &pending);
        env.storage().persistent().remove(&DataKey::PendingAdmin);
        AdminTransferred {
            previous_admin: previous,
            new_admin: pending,
        }
        .publish(&env);
    }

    pub fn propose_upgrade_wasm(env: Env, caller: Address, new_wasm_hash: BytesN<32>) {
        Self::require_admin(&env, &caller);
        let eta = env
            .ledger()
            .timestamp()
            .saturating_add(UPGRADE_TIMELOCK_SECS);
        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgradeHash, &new_wasm_hash);
        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgradeEta, &eta);
    }

    pub fn upgrade_wasm(env: Env, caller: Address, new_wasm_hash: BytesN<32>) {
        Self::require_admin(&env, &caller);
        let pending: BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgradeHash)
            .expect("pending upgrade not set");
        let eta: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgradeEta)
            .expect("pending upgrade eta not set");
        if pending != new_wasm_hash {
            panic!("upgrade hash mismatch");
        }
        if env.ledger().timestamp() < eta {
            panic!("upgrade timelocked");
        }
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeHash);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeEta);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

fn expected_admin_config() -> &'static str {
    if cfg!(any(test, feature = "test-default-admin")) {
        option_env!("PRICE_ROUTER_INIT_ADMIN").unwrap_or(DEFAULT_INIT_ADMIN)
    } else {
        option_env!("PRICE_ROUTER_INIT_ADMIN")
            .expect("PRICE_ROUTER_INIT_ADMIN must be set at build time")
    }
}

#[cfg(test)]
mod test;
