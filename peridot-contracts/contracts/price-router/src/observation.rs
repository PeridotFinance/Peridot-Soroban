//! Reporter-attested SDEX windows. The contract verifies time, rate and actual
//! two-way Aquarius quotes, NOT the off-chain SDEX history. Reporter trust remains.
use crate::{try_mul_div, try_to_i128, PriceData, PriceRouter, PriceSource};
use soroban_sdk::{contractevent, contracttype, Address, Bytes, Env, IntoVal, Map, Symbol, Vec};

pub const RATIO_SCALE: u128 = 1_000_000_000_000;
pub const STEP_WINDOW_SECS: u64 = 300;
pub const RECOVERY_LIFETIME_SECS: u64 = 7200;
pub const RECOVERY_REVIEW_SECS: u64 = 300;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationRecovery {
    pub admin: Address,
    pub reference_ratio: u128,
    pub proposed_at: u64,
    pub expires_at: u64,
    pub recovered_end: u64,
    pub cancelled: bool,
    pub config: ObservationConfig,
}

#[contractevent(topics = ["obs_recovery"])]
pub struct RecoveryEvent {
    pub asset: Address,
    pub stage: Symbol,
    pub ratio: u128,
    pub timestamp: u64,
}

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
    Recovery(Address),
}

fn recovery_network(env: &Env) {
    let public = env.crypto().sha256(&Bytes::from_slice(
        env,
        b"Public Global Stellar Network ; September 2015",
    ));
    assert_ne!(
        env.ledger().network_id(),
        public.to_bytes(),
        "recovery release gate"
    );
}

pub fn recovery(env: &Env, asset: &Address) -> Option<ObservationRecovery> {
    env.storage().instance().get(&Key::Recovery(asset.clone()))
}

pub fn begin_recovery(env: &Env, admin: &Address, asset: &Address, reference_ratio: u128) {
    recovery_network(env);
    let c = config(env, asset);
    assert!(
        reference_ratio >= c.min_ratio && reference_ratio <= c.max_ratio,
        "recovery bounds"
    );
    let mut previous = get(env, asset).expect("observation state missing");
    assert!(previous.end != 0, "recovery is not bootstrap");
    let now = env.ledger().timestamp();
    // Replacing an expired/cancelled proposal restarts the FULL observation window.
    previous.valid = false;
    previous.invalidated_at = now;
    env.storage()
        .instance()
        .set(&Key::State(asset.clone()), &previous);
    env.storage().instance().set(
        &Key::Recovery(asset.clone()),
        &ObservationRecovery {
            admin: admin.clone(),
            reference_ratio,
            proposed_at: now,
            expires_at: now.checked_add(RECOVERY_LIFETIME_SECS).unwrap(),
            recovered_end: 0,
            cancelled: false,
            config: c,
        },
    );
    RecoveryEvent {
        asset: asset.clone(),
        stage: Symbol::new(env, "proposed"),
        ratio: reference_ratio,
        timestamp: now,
    }
    .publish(env);
}

pub fn cancel_recovery(env: &Env, asset: &Address) {
    recovery_network(env);
    let mut r = recovery(env, asset).expect("no recovery");
    r.cancelled = true;
    // Retain a durable lock: cancellation/expiry must NEVER enable borrowing.
    env.storage()
        .instance()
        .set(&Key::Recovery(asset.clone()), &r);
    let mut value = get(env, asset).expect("observation state missing");
    value.valid = false;
    value.invalidated_at = env.ledger().timestamp();
    env.storage()
        .instance()
        .set(&Key::State(asset.clone()), &value);
    RecoveryEvent {
        asset: asset.clone(),
        stage: Symbol::new(env, "cancelled"),
        ratio: r.reference_ratio,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

fn active_recovery(env: &Env, asset: &Address, c: &ObservationConfig) -> ObservationRecovery {
    let r = recovery(env, asset).expect("no recovery");
    assert!(
        !r.cancelled && env.ledger().timestamp() <= r.expires_at,
        "recovery expired/cancelled"
    );
    assert_eq!(&r.config, c, "recovery configuration changed");
    r
}

pub fn finish_recovery(env: &Env, admin: &Address, asset: &Address) {
    recovery_network(env);
    let c = config(env, asset);
    let r = active_recovery(env, asset, &c);
    assert_eq!(admin, &r.admin, "recovery approver changed");
    let value = get(env, asset).expect("observation state missing");
    assert!(
        r.recovered_end > 0
            && value.end >= r.recovered_end.checked_add(RECOVERY_REVIEW_SECS).unwrap(),
        "recovery review incomplete"
    );
    assert!(
        value.valid && env.ledger().timestamp().saturating_sub(value.end) <= 60,
        "fresh recovery report required"
    );
    assert!(
        near(value.ratio, r.reference_ratio, c.max_deviation_bps),
        "recovery reference deviation"
    );
    price_unlocked(env, asset, &c).expect("recovery upstream unavailable");
    check_quotes(env, &c, value.ratio);
    env.storage()
        .instance()
        .remove(&Key::Recovery(asset.clone()));
    RecoveryEvent {
        asset: asset.clone(),
        stage: Symbol::new(env, "completed"),
        ratio: value.ratio,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
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
    publish_inner(env, caller, asset, ratio, start, end, false);
}

pub fn publish_recovery(
    env: &Env,
    caller: &Address,
    asset: &Address,
    ratio: u128,
    start: u64,
    end: u64,
) {
    recovery_network(env);
    publish_inner(env, caller, asset, ratio, start, end, true);
}

fn publish_inner(
    env: &Env,
    caller: &Address,
    asset: &Address,
    ratio: u128,
    start: u64,
    end: u64,
    recovering: bool,
) {
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
    let pending = recovery(env, asset);
    if recovering {
        let mut r = active_recovery(env, asset, &c);
        assert_eq!(r.recovered_end, 0, "recovery already published");
        assert!(
            start >= r.proposed_at,
            "fresh post-approval window required"
        );
        assert!(
            near(ratio, r.reference_ratio, c.max_deviation_bps),
            "recovery reference deviation"
        );
        r.recovered_end = end;
        env.storage()
            .instance()
            .set(&Key::Recovery(asset.clone()), &r);
    } else if pending.is_some() {
        let r = active_recovery(env, asset, &c);
        assert!(r.recovered_end > 0, "recovery publication required");
        assert!(
            near(ratio, r.reference_ratio, c.max_deviation_bps),
            "recovery reference deviation"
        );
    }
    if prev.end != 0 && !recovering {
        assert!(
            end - prev.end >= c.min_interval_secs,
            "updates too frequent"
        );
        // Faster heartbeats must not multiply the permitted price drift. The
        // old 1%/300s budget is pro-rated, capped at 1% even after long outages.
        let elapsed = core::cmp::min(end - prev.end, STEP_WINDOW_SECS);
        let step_bps = (u64::from(c.max_step_bps) * elapsed / STEP_WINDOW_SECS) as u32;
        assert!(near(ratio, prev.ratio, step_bps), "ratio step");
    }
    check_quotes(env, &c, ratio);
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
    if recovering {
        RecoveryEvent {
            asset: asset.clone(),
            stage: Symbol::new(env, "reported"),
            ratio,
            timestamp: now,
        }
        .publish(env);
    }
}

fn check_quotes(env: &Env, c: &ObservationConfig, ratio: u128) {
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
}

pub fn price(env: &Env, asset: &Address, c: &ObservationConfig) -> Option<PriceData> {
    if recovery(env, asset).is_some() {
        return None;
    }
    price_unlocked(env, asset, c)
}

fn price_unlocked(env: &Env, asset: &Address, c: &ObservationConfig) -> Option<PriceData> {
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
