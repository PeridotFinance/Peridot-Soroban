//! Reporter-attested SDEX windows. The contract verifies time, rate and actual
//! two-way Aquarius quotes, NOT the off-chain SDEX history. Reporter trust remains.
use crate::{try_mul_div, try_to_i128, PriceData, PriceRouter, PriceSource};
use soroban_sdk::{contracttype, Address, Env, IntoVal, Map, Symbol, Vec};

pub const RATIO_SCALE: u128 = 1_000_000_000_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationConfig {
    pub reporter: Address,
    pub quote_to: Address,
    pub pool: Address,
    pub in_idx: u32,
    pub out_idx: u32,
    pub probe_amount: u128,
    pub window_secs: u64,
    pub max_age_secs: u64,
    pub min_interval_secs: u64,
    pub max_deviation_bps: u32,
    pub max_step_bps: u32,
    pub min_ratio: u128,
    pub max_ratio: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    pub ratio: u128,
    pub start: u64,
    pub end: u64,
    pub valid: bool,
    pub invalidated_at: u64,
}

#[contracttype]
enum Key {
    State(Address),
    Registry,
    Dependencies,
}

fn config(env: &Env, asset: &Address) -> ObservationConfig {
    match PriceRouter::source_of(env, asset) {
        PriceSource::Observed(c) => c,
        _ => panic!("observed source required"),
    }
}

pub fn configure(env: &Env, asset: &Address, c: &ObservationConfig) {
    assert_ne!(asset, &c.quote_to, "self quote");
    assert!(c.in_idx <= 1 && c.out_idx <= 1 && c.in_idx != c.out_idx);
    assert!(c.probe_amount > 0 && c.probe_amount <= i128::MAX as u128);
    assert!((300..=3600).contains(&c.window_secs));
    assert!((60..=300).contains(&c.max_age_secs));
    assert!(c.min_interval_secs >= 60 && c.min_interval_secs <= c.max_age_secs);
    assert!((1..=500).contains(&c.max_deviation_bps));
    assert!((1..=500).contains(&c.max_step_bps));
    assert!(c.min_ratio > 0 && c.min_ratio < c.max_ratio && c.max_ratio <= 2 * RATIO_SCALE);
    let tokens: Vec<Address> =
        env.invoke_contract(&c.pool, &Symbol::new(env, "get_tokens"), Vec::new(env));
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens.get(c.in_idx).unwrap(), *asset);
    assert_eq!(tokens.get(c.out_idx).unwrap(), c.quote_to);
    // This source intentionally supports the seven-decimal XLM/yXLM pair only.
    for token in [asset, &c.quote_to] {
        assert_eq!(soroban_sdk::token::Client::new(env, token).decimals(), 7);
    }
    let mut registry: Map<Address, bool> = env
        .storage()
        .instance()
        .get(&Key::Registry)
        .unwrap_or(Map::new(env));
    registry.set(asset.clone(), true);
    assert!(registry.len() <= 4, "observation registry full");
    env.storage().instance().set(&Key::Registry, &registry);
    // Configuration changes invalidate old reports; do NOT reset rate/replay history.
    let mut previous: Observation = env
        .storage()
        .instance()
        .get(&Key::State(asset.clone()))
        .unwrap_or(Observation {
            ratio: 0,
            start: 0,
            end: 0,
            valid: false,
            invalidated_at: 0,
        });
    previous.valid = false;
    previous.invalidated_at = env.ledger().timestamp();
    env.storage()
        .instance()
        .set(&Key::State(asset.clone()), &previous);
}

pub fn set_dependency(env: &Env, asset: &Address, required: Option<Address>) {
    let mut deps: Map<Address, Address> = env
        .storage()
        .instance()
        .get(&Key::Dependencies)
        .unwrap_or(Map::new(env));
    if let Some(required) = required {
        config(env, &required);
        deps.set(asset.clone(), required);
        assert!(deps.len() <= 8, "dependency registry full");
    } else {
        deps.remove(asset.clone());
    }
    env.storage().instance().set(&Key::Dependencies, &deps);
}

pub fn dependency_ready(env: &Env, asset: &Address) -> Option<()> {
    let deps: Map<Address, Address> = env
        .storage()
        .instance()
        .get(&Key::Dependencies)
        .unwrap_or(Map::new(env));
    if let Some(required) = deps.get(asset.clone()) {
        let c = match PriceRouter::source_of(env, &required) {
            PriceSource::Observed(c) => c,
            _ => return None,
        };
        price(env, &required, &c)?;
    }
    Some(())
}

pub fn get(env: &Env, asset: &Address) -> Option<Observation> {
    env.storage().instance().get(&Key::State(asset.clone()))
}

pub fn invalidate(env: &Env, caller: &Address, asset: &Address) {
    let c = config(env, asset);
    assert_eq!(*caller, c.reporter, "not reporter");
    caller.require_auth();
    let mut value = get(env, asset).expect("observation state missing");
    value.valid = false;
    value.invalidated_at = env.ledger().timestamp();
    env.storage()
        .instance()
        .set(&Key::State(asset.clone()), &value);
}

fn near(a: u128, b: u128, bps: u32) -> bool {
    a > 0 && b > 0 && a.abs_diff(b) <= try_mul_div(b, bps as u128, 10_000).unwrap_or(0)
}

pub fn publish(env: &Env, caller: &Address, asset: &Address, ratio: u128, start: u64, end: u64) {
    let c = config(env, asset);
    assert_eq!(*caller, c.reporter, "not reporter");
    caller.require_auth();
    let now = env.ledger().timestamp();
    assert!(
        start < end && end <= now && now - end <= c.max_age_secs,
        "stale/future window"
    );
    assert_eq!(end - start, c.window_secs, "wrong window");
    assert!(ratio >= c.min_ratio && ratio <= c.max_ratio, "ratio bounds");
    let prev = get(env, asset).expect("observation state missing");
    assert!(
        end > prev.invalidated_at && end > prev.end,
        "replayed window"
    );
    if prev.end != 0 {
        assert!(
            end - prev.end >= c.min_interval_secs,
            "updates too frequent"
        );
        assert!(near(ratio, prev.ratio, c.max_step_bps), "ratio step");
    }
    // Both directions must be executable and close to the independent window.
    // Never average a manipulated pool quote into the reported SDEX price.
    let sell: u128 = env.invoke_contract(
        &c.pool,
        &Symbol::new(env, "estimate_swap"),
        (c.in_idx, c.out_idx, c.probe_amount).into_val(env),
    );
    let buy: u128 = env.invoke_contract(
        &c.pool,
        &Symbol::new(env, "estimate_swap"),
        (c.out_idx, c.in_idx, c.probe_amount).into_val(env),
    );
    let bid = try_mul_div(sell, RATIO_SCALE, c.probe_amount).expect("quote overflow");
    let ask = try_mul_div(c.probe_amount, RATIO_SCALE, buy).expect("empty/overflow quote");
    assert!(
        near(bid, ratio, c.max_deviation_bps) && near(ask, ratio, c.max_deviation_bps),
        "pool deviation"
    );
    env.storage().instance().set(
        &Key::State(asset.clone()),
        &Observation {
            ratio,
            start,
            end,
            valid: true,
            invalidated_at: prev.invalidated_at,
        },
    );
}

pub fn price(env: &Env, asset: &Address, c: &ObservationConfig) -> Option<PriceData> {
    let value = get(env, asset)?;
    let now = env.ledger().timestamp();
    if !value.valid || value.end > now || now - value.end > c.max_age_secs {
        return None;
    }
    // Query upstream directly, not the dependency-gated router: no recursive graph.
    let quote = PriceRouter::upstream_price(env, &PriceRouter::upstream_asset(env, &c.quote_to))?;
    if quote.price <= 0 || quote.timestamp > now || now - quote.timestamp > c.max_age_secs {
        return None;
    }
    let decimals = env.try_invoke_contract::<u32, soroban_sdk::InvokeError>(
        &PriceRouter::upstream(env),
        &Symbol::new(env, "decimals"),
        Vec::new(env),
    );
    if !matches!(decimals, Ok(Ok(0..=18))) {
        return None;
    }
    let p = try_to_i128(try_mul_div(value.ratio, quote.price as u128, RATIO_SCALE)?)?;
    if p <= 0 {
        return None;
    }
    Some(PriceData {
        price: p,
        timestamp: core::cmp::min(value.end, quote.timestamp),
    })
}
