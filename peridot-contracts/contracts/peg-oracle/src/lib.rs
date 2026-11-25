#![no_std]
//! RESEARCH PROTOTYPE, not reviewed for deployment.
//!
//! Group oracle for the Sentora market group. SimplePeridottroller keeps exactly
//! ONE oracle address and calls `decimals()`, `lastprice(asset)` and
//! `resolution()` on it for every market, so this contract is a Reflector-shaped
//! router, not a PYUSD-only feed:
//!
//! * Every asset except the pinned pegged one (PYUSD) is forwarded unchanged to
//!   Reflector (`CAFJZQ..`, USD base). `decimals()` and `resolution()` are
//!   Reflector's, read live, so forwarded prices and their scale always agree.
//! * The pegged asset, asked for as `Stellar(PYUSD SAC)` or `Other("PYUSD")`, is
//!   priced at `min($1, observed)` and is NEVER above $1:
//!   - `observed` is the median of three independent PYUSD/USD observations:
//!     the executable bid (`estimate_swap`, PYUSD -> USDC, `probe` raw) of each of
//!     the two pinned Aquarius PYUSD/USDC pools (concentrated and stable), and the
//!     round-sampled Reflector DEX feed (`CALI2BY..`, base USDC), each converted
//!     to USD with Reflector's USDC/USD. With one source unavailable the two
//!     remaining must agree within the band (then the LOWER is used); if they
//!     disagree, or fewer than two are available, the price is `None`: a single
//!     source never sets the price alone.
//!   - If `observed >= 1 - band`, the price is exactly `10^decimals` ($1).
//!   - Otherwise the price is `observed` (< $1 - band): a mark-down.
//!
//! How the controller splits "borrow/withdraw fail closed" from "liquidation keeps
//! working" without any controller change: the deployed cash-group controller
//! (f41e5260) has a per-token USD price band (`set_price_band`). With the PYUSD
//! band set to `[1 - band, 1.0]`, a mark-down makes new borrows (of PYUSD, or by
//! any account holding PYUSD) and redeems/collateral transfers by accounts with
//! debt revert with "price out of band", and `get_price_usd` returns `None`;
//! repay and liquidation use the unbanded marked-down price. So a depeg is
//! liquidated at `min($1, observed)`, not frozen.
//!
//! Why a median of three: moving the price requires moving TWO sources. A flash
//! swap can move an Aquarius pool inside one transaction (and the controller would
//! cache the result for `k x resolution`), but it cannot move the round-sampled
//! feed, and the stable pool is ~25x deeper than the concentrated one. Equally, a
//! flash restore of one pool cannot re-arm $1 during a real depeg.
//!
//! The reported timestamp is the OLDER of the Reflector timestamps used, never
//! `now`, so the controller's staleness rule ages this price like Reflector's.
//!
//! No admin, no upgrade, no storage beyond the constructor configuration. A policy
//! change is a new instance plus the controller's timelocked `set_oracle`.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, Address,
    Env, IntoVal, InvokeError, Symbol, Val, Vec,
};

const BPS: u128 = 10_000;
const MAX_BAND_BPS: u32 = 500;
const MAX_AGE_SECS: u64 = 3_600;
const DAY_LEDGERS: u32 = 17_280;
const INSTANCE_BUMP_THRESHOLD: u32 = 30 * DAY_LEDGERS;
const INSTANCE_BUMP_TO: u32 = 120 * DAY_LEDGERS;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    BadConfig = 1,
    PoolTokens = 2,
    Metadata = 3,
}

/// Reflector `Asset`, identical XDR.
#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

/// Reflector `PriceData`, identical XDR.
#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

/// An Aquarius pool holding both tokens, with their indices in `get_tokens()`.
#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PoolRef {
    pub pool: Address,
    pub pegged_idx: u32,
    pub quote_idx: u32,
}

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PegConfig {
    /// Reflector with a USD base: forwarded for every non-pegged asset and the
    /// source of the quote asset's USD price.
    pub reflector: Address,
    /// Reflector DEX feed whose base is `Stellar(quote)`.
    pub dex_feed: Address,
    pub pegged: Address,
    pub pegged_symbol: Symbol,
    pub quote: Address,
    pub quote_symbol: Symbol,
    /// Exactly two pools (concentrated, stable).
    pub pools: Vec<PoolRef>,
    /// Raw PYUSD sold in each bid probe.
    pub probe: u128,
    /// Observed >= 1 - band prices at exactly $1.
    pub band_bps: u32,
    /// Maximum age of both Reflector inputs.
    pub max_age_secs: u64,
}

/// Bit flags in `PegStatus.missing` (a source that could not be used).
pub const M_POOL0: u32 = 1;
pub const M_POOL1: u32 = 2;
pub const M_DEX: u32 = 4;

/// Everything the oracle saw, in `decimals` fixed-point USD (0 = unavailable).
#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PegStatus {
    /// A price is served (USDC/USD fresh and at least two sources).
    pub priced: bool,
    /// The price is exactly $1 (observed within the band).
    pub at_peg: bool,
    pub price: i128,
    pub observed: i128,
    pub decimals: u32,
    pub quote_usd: i128,
    pub pool0_bid_usd: i128,
    pub pool1_bid_usd: i128,
    pub dex_usd: i128,
    pub missing: u32,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PegMarkedDown {
    pub price: i128,
    pub pool0_bid_usd: i128,
    pub pool1_bid_usd: i128,
    pub dex_usd: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PegUnpriced {
    pub missing: u32,
}

#[contracttype]
#[derive(Clone)]
enum Key {
    Config,
}

#[contract]
pub struct PegOracle;

fn call<T: soroban_sdk::TryFromVal<Env, Val>>(
    e: &Env,
    c: &Address,
    f: &str,
    args: Vec<Val>,
) -> Option<T> {
    match e.try_invoke_contract::<T, InvokeError>(c, &Symbol::new(e, f), args) {
        Ok(Ok(v)) => Some(v),
        _ => None,
    }
}

fn mul_div(a: u128, b: u128, d: u128) -> Option<u128> {
    if d == 0 {
        return None;
    }
    a.checked_mul(b).map(|x| x / d)
}

fn fresh(e: &Env, p: &PriceData, max_age: u64) -> bool {
    let now = e.ledger().timestamp();
    p.price > 0 && p.timestamp <= now && now - p.timestamp <= max_age
}

impl PegOracle {
    fn cfg(e: &Env) -> PegConfig {
        e.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_TO);
        e.storage().instance().get(&Key::Config).unwrap()
    }

    fn is_pegged(c: &PegConfig, a: &Asset) -> bool {
        match a {
            Asset::Stellar(x) => *x == c.pegged,
            Asset::Other(s) => *s == c.pegged_symbol,
        }
    }

    fn pool_bid(e: &Env, c: &PegConfig, p: &PoolRef, qp: u128) -> Option<u128> {
        if call::<bool>(e, &p.pool, "get_is_killed_swap", Vec::new(e)) != Some(false) {
            return None;
        }
        let out: u128 = call(
            e,
            &p.pool,
            "estimate_swap",
            (p.pegged_idx, p.quote_idx, c.probe).into_val(e),
        )?;
        if out == 0 {
            return None;
        }
        mul_div(out, qp, c.probe)
    }

    fn dex(e: &Env, c: &PegConfig, dec: u32, qp: u128) -> Option<(u128, u64)> {
        match call::<u32>(e, &c.dex_feed, "decimals", Vec::new(e)) {
            Some(d) if d == dec => {}
            _ => return None,
        }
        match call::<Asset>(e, &c.dex_feed, "base", Vec::new(e)) {
            Some(Asset::Stellar(b)) if b == c.quote => {}
            _ => return None,
        }
        let d: PriceData = match call::<Option<PriceData>>(
            e,
            &c.dex_feed,
            "lastprice",
            (Asset::Stellar(c.pegged.clone()),).into_val(e),
        ) {
            Some(Some(p)) if fresh(e, &p, c.max_age_secs) => p,
            _ => return None,
        };
        Some((mul_div(d.price as u128, qp, 10u128.pow(dec))?, d.timestamp))
    }

    fn observe(e: &Env, c: &PegConfig) -> PegStatus {
        let mut st = PegStatus {
            priced: false,
            at_peg: false,
            price: 0,
            observed: 0,
            decimals: 0,
            quote_usd: 0,
            pool0_bid_usd: 0,
            pool1_bid_usd: 0,
            dex_usd: 0,
            missing: M_POOL0 | M_POOL1 | M_DEX,
            timestamp: 0,
        };
        let dec: u32 = match call(e, &c.reflector, "decimals", Vec::new(e)) {
            Some(d) if d <= 18 => d,
            _ => return st,
        };
        st.decimals = dec;
        let one = 10u128.pow(dec);
        let q: PriceData = match call::<Option<PriceData>>(
            e,
            &c.reflector,
            "lastprice",
            (Asset::Other(c.quote_symbol.clone()),).into_val(e),
        ) {
            Some(Some(p)) if fresh(e, &p, c.max_age_secs) => p,
            _ => return st,
        };
        let qp = q.price as u128;
        st.quote_usd = q.price;
        st.timestamp = q.timestamp;
        let mut obs = [0u128; 3];
        let mut n = 0usize;
        st.missing = 0;
        for (i, flag) in [(0u32, M_POOL0), (1u32, M_POOL1)] {
            match c.pools.get(i).and_then(|p| Self::pool_bid(e, c, &p, qp)) {
                Some(b) => {
                    obs[n] = b;
                    n += 1;
                    if i == 0 {
                        st.pool0_bid_usd = b as i128;
                    } else {
                        st.pool1_bid_usd = b as i128;
                    }
                }
                None => st.missing |= flag,
            }
        }
        match Self::dex(e, c, dec, qp) {
            Some((d, ts)) => {
                obs[n] = d;
                n += 1;
                st.dex_usd = d as i128;
                st.timestamp = core::cmp::min(st.timestamp, ts);
            }
            None => st.missing |= M_DEX,
        }
        let observed = match n {
            3 => {
                let mut s = obs;
                s.sort_unstable();
                s[1]
            }
            // Two sources must agree within the band; otherwise one of them (possibly the
            // cheap concentrated pool, moved inside this transaction) would set the price alone.
            2 => {
                let (lo, hi) = (obs[0].min(obs[1]), obs[0].max(obs[1]));
                if (hi - lo) * BPS > (c.band_bps as u128) * one {
                    return st;
                }
                lo
            }
            _ => return st,
        };
        let lower = one * (BPS - c.band_bps as u128) / BPS;
        let price = if observed >= lower { one } else { observed };
        st.priced = price > 0;
        st.at_peg = price == one;
        st.observed = observed as i128;
        st.price = price as i128;
        st
    }
}

#[contractimpl]
impl PegOracle {
    /// Pins the whole policy. Checks both pools' token order and both feeds' metadata.
    pub fn __constructor(e: Env, config: PegConfig) {
        let c = config;
        if c.band_bps == 0
            || c.band_bps > MAX_BAND_BPS
            || c.max_age_secs == 0
            || c.max_age_secs > MAX_AGE_SECS
            || c.probe == 0
            || c.pegged == c.quote
            || c.reflector == c.dex_feed
            || c.pools.len() != 2
        {
            panic_with_error!(&e, Error::BadConfig);
        }
        if c.pools.get(0).map(|p| p.pool) == c.pools.get(1).map(|p| p.pool) {
            panic_with_error!(&e, Error::BadConfig);
        }
        for p in c.pools.iter() {
            let tokens: Vec<Address> = call(&e, &p.pool, "get_tokens", Vec::new(&e))
                .unwrap_or_else(|| panic_with_error!(&e, Error::PoolTokens));
            if p.pegged_idx == p.quote_idx
                || tokens.get(p.pegged_idx) != Some(c.pegged.clone())
                || tokens.get(p.quote_idx) != Some(c.quote.clone())
            {
                panic_with_error!(&e, Error::PoolTokens);
            }
        }
        let dec: Option<u32> = call(&e, &c.reflector, "decimals", Vec::new(&e));
        let dec2: Option<u32> = call(&e, &c.dex_feed, "decimals", Vec::new(&e));
        let base: Option<Asset> = call(&e, &c.dex_feed, "base", Vec::new(&e));
        let res: Option<u32> = call(&e, &c.reflector, "resolution", Vec::new(&e));
        match (dec, dec2, base, res) {
            (Some(a), Some(b), Some(Asset::Stellar(q)), Some(r))
                if a == b && a <= 18 && q == c.quote && r > 0 => {}
            _ => panic_with_error!(&e, Error::Metadata),
        }
        e.storage().instance().set(&Key::Config, &c);
        e.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_TO);
    }

    /// Reflector `lastprice`. Pegged asset: `min($1, observed)`, exactly $1 inside the band,
    /// `None` without USDC/USD or with fewer than two sources. Anything else: Reflector's answer.
    pub fn lastprice(e: Env, asset: Asset) -> Option<PriceData> {
        let c = Self::cfg(&e);
        if !Self::is_pegged(&c, &asset) {
            return call::<Option<PriceData>>(&e, &c.reflector, "lastprice", (asset,).into_val(&e))
                .flatten();
        }
        let st = Self::observe(&e, &c);
        if !st.priced {
            PegUnpriced {
                missing: st.missing,
            }
            .publish(&e);
            return None;
        }
        if !st.at_peg {
            PegMarkedDown {
                price: st.price,
                pool0_bid_usd: st.pool0_bid_usd,
                pool1_bid_usd: st.pool1_bid_usd,
                dex_usd: st.dex_usd,
            }
            .publish(&e);
        }
        Some(PriceData {
            price: st.price,
            timestamp: st.timestamp,
        })
    }

    pub fn decimals(e: Env) -> u32 {
        let c = Self::cfg(&e);
        call(&e, &c.reflector, "decimals", Vec::new(&e))
            .unwrap_or_else(|| panic_with_error!(&e, Error::Metadata))
    }

    pub fn resolution(e: Env) -> u32 {
        let c = Self::cfg(&e);
        call(&e, &c.reflector, "resolution", Vec::new(&e))
            .unwrap_or_else(|| panic_with_error!(&e, Error::Metadata))
    }

    /// Observation view for keepers and monitors (no event).
    pub fn peg_status(e: Env) -> PegStatus {
        let c = Self::cfg(&e);
        Self::observe(&e, &c)
    }

    pub fn get_config(e: Env) -> PegConfig {
        Self::cfg(&e)
    }

    /// Permissionless TTL keepalive for the instance and code.
    pub fn bump_ttl(e: Env) {
        e.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_TO, INSTANCE_BUMP_TO);
    }
}

#[cfg(test)]
mod test;
