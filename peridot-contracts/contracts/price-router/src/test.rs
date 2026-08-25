#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, testutils::Ledger as _, Address,
    Env, String, Symbol,
};

use crate::{Asset, PegConfig, PriceData, PriceRouter, PriceRouterClient, PriceSource, PushGuard};

const ADMIN_G: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";
/// Reflector convention: 1e14-scaled.
const PRICE_XLM: i128 = 19_571_505_057_876; // ~$0.1957

// ─────────────────────────────────────────────────────────────────────────────
// Mock upstream oracle (stands in for Reflector)
// ─────────────────────────────────────────────────────────────────────────────

#[contracttype]
enum OKey {
    Price(Address),
    SymPrice(Symbol),
    Down,
}

#[contract]
pub struct MockUpstream;

#[contractimpl]
impl MockUpstream {
    pub fn set_price(env: Env, token: Address, price: i128) {
        env.storage().persistent().set(&OKey::Price(token), &price);
    }

    pub fn set_symbol_price(env: Env, sym: Symbol, price: i128) {
        env.storage().persistent().set(&OKey::SymPrice(sym), &price);
    }

    /// Simulates the upstream being unreachable, not merely unaware of an asset.
    pub fn set_down(env: Env, down: bool) {
        env.storage().persistent().set(&OKey::Down, &down);
    }

    pub fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&OKey::Down)
            .unwrap_or(false)
        {
            panic!("upstream down");
        }
        let price: i128 = match asset {
            Asset::Stellar(a) => env.storage().persistent().get(&OKey::Price(a)).unwrap_or(0),
            Asset::Other(s) => env
                .storage()
                .persistent()
                .get(&OKey::SymPrice(s))
                .unwrap_or(0),
        };
        if price == 0 {
            return None;
        }
        Some(PriceData {
            price,
            timestamp: env.ledger().timestamp(),
        })
    }

    pub fn resolution(_env: Env) -> u32 {
        300
    }

    pub fn decimals(_env: Env) -> u32 {
        14
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mock observation pool
// ─────────────────────────────────────────────────────────────────────────────

#[contracttype]
enum PKey {
    RatioBps,
    Broken,
}

#[contract]
pub struct MockPool;

#[contractimpl]
impl MockPool {
    /// Sets how many out-units a probe returns, in bps of the input.
    pub fn set_ratio_bps(env: Env, bps: u32) {
        env.storage().persistent().set(&PKey::RatioBps, &bps);
    }

    pub fn set_broken(env: Env, broken: bool) {
        env.storage().persistent().set(&PKey::Broken, &broken);
    }

    pub fn estimate_swap(env: Env, _in_idx: u32, _out_idx: u32, in_amount: u128) -> u128 {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&PKey::Broken)
            .unwrap_or(false)
        {
            panic!("pool unavailable");
        }
        let bps: u32 = env
            .storage()
            .persistent()
            .get(&PKey::RatioBps)
            .unwrap_or(10_000);
        in_amount.saturating_mul(bps as u128) / 10_000
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fixture
// ─────────────────────────────────────────────────────────────────────────────

struct F {
    env: Env,
    admin: Address,
    router: PriceRouterClient<'static>,
    up: MockUpstreamClient<'static>,
    pool: MockPoolClient<'static>,
    xlm: Address,
    yxlm: Address,
}

fn setup() -> F {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let admin = Address::from_string(&String::from_str(&env, ADMIN_G));
    let xlm = Address::generate(&env);
    let yxlm = Address::generate(&env);

    let up_id = env.register(MockUpstream, ());
    let up = MockUpstreamClient::new(&env, &up_id);
    up.set_price(&xlm, &PRICE_XLM);

    let pool_id = env.register(MockPool, ());
    let pool = MockPoolClient::new(&env, &pool_id);
    // yXLM trades at parity on SDEX; the pool probe reflects that.
    pool.set_ratio_bps(&10_000u32);

    let router_id = env.register(PriceRouter, ());
    let router = PriceRouterClient::new(&env, &router_id);
    router.initialize(&admin, &up_id, &300u32);

    router.set_source(
        &admin,
        &yxlm,
        &PriceSource::Pegged(PegConfig {
            peg_to: xlm.clone(),
            pool: Some(pool_id),
            in_idx: 1,
            out_idx: 0,
            probe_amount: 1_000_0000000u128,
            min_ratio_bps: 9_000, // refuse to price below 90% of peg
        }),
    );

    F {
        env,
        admin,
        router,
        up,
        pool,
        xlm,
        yxlm,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pass-through
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unconfigured_assets_pass_through_to_upstream() {
    let f = setup();
    let p = f.router.lastprice(&Asset::Stellar(f.xlm.clone())).unwrap();
    assert_eq!(p.price, PRICE_XLM);
}

#[test]
fn a_dead_upstream_returns_none_rather_than_reverting() {
    let f = setup();
    f.up.set_down(&true);
    // Must not propagate the panic — a dead upstream cannot take the router,
    // and therefore every consumer, down with it.
    assert!(f.router.lastprice(&Asset::Stellar(f.xlm.clone())).is_none());
}

#[test]
fn symbol_assets_resolve_through_the_mapping() {
    let f = setup();
    let sym = Symbol::new(&f.env, "yXLM");
    f.router
        .set_symbol_asset(&f.admin, &sym, &Some(f.yxlm.clone()));
    // Routed to the pegged config, not blindly forwarded upstream.
    let p = f.router.lastprice(&Asset::Other(sym)).unwrap();
    assert_eq!(p.price, PRICE_XLM);
}

#[test]
fn a_pegged_asset_can_reference_a_symbol_keyed_upstream_price() {
    let f = setup();
    // Mainnet Reflector publishes XLM as `Other("XLM")`, not by the native
    // asset's SAC address. Reproduce that exact source shape.
    f.up.set_price(&f.xlm, &0i128);
    let xlm_symbol = Symbol::new(&f.env, "XLM");
    f.up.set_symbol_price(&xlm_symbol, &PRICE_XLM);
    f.router
        .set_symbol_asset(&f.admin, &xlm_symbol, &Some(f.xlm.clone()));

    let xlm = f.router.lastprice(&Asset::Stellar(f.xlm.clone())).unwrap();
    assert_eq!(xlm.price, PRICE_XLM);
    let p = f.router.lastprice(&Asset::Stellar(f.yxlm.clone())).unwrap();
    assert_eq!(p.price, PRICE_XLM);
}

// ─────────────────────────────────────────────────────────────────────────────
// The peg clamp — the property the whole contract exists for
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn at_parity_the_pegged_asset_prices_at_the_peg() {
    let f = setup();
    let p = f.router.lastprice(&Asset::Stellar(f.yxlm.clone())).unwrap();
    assert_eq!(p.price, PRICE_XLM);
}

/// The load-bearing property: no observation, from any source, can price the
/// asset *above* the thing it is a claim on. This is what makes a manipulated
/// pool, a compromised keeper, and a compromised upstream all survivable — the
/// direction that produces bad debt is closed by construction.
#[test]
fn a_pegged_asset_can_never_price_above_its_peg() {
    let f = setup();
    // Attacker pushes the observed ratio far above parity.
    f.pool.set_ratio_bps(&30_000u32);
    let p = f.router.lastprice(&Asset::Stellar(f.yxlm.clone())).unwrap();
    assert_eq!(
        p.price, PRICE_XLM,
        "clamp failed: a 3x observation moved the price above the peg"
    );
}

#[test]
fn trading_below_the_peg_marks_the_price_down_proportionally() {
    let f = setup();
    f.pool.set_ratio_bps(&9_500u32); // 5% under
    let p = f.router.lastprice(&Asset::Stellar(f.yxlm.clone())).unwrap();
    let expected = PRICE_XLM * 9_500 / 10_000;
    assert_eq!(p.price, expected);
    assert!(p.price < PRICE_XLM);
}

/// Below the floor the router refuses to publish rather than emitting a
/// catastrophic mark-down. A consumer that gets no price degrades safely; one
/// that gets a very wrong price does not.
#[test]
fn a_hard_depeg_halts_pricing_instead_of_publishing_a_collapse() {
    let f = setup();
    f.pool.set_ratio_bps(&5_000u32); // 50% — well through the 90% floor
    assert!(
        f.router
            .lastprice(&Asset::Stellar(f.yxlm.clone()))
            .is_none(),
        "a hard depeg should halt pricing, not publish it"
    );
}

/// An unobservable peg is exactly when a depeg would hide, so the router must
/// not assume parity. It claims no more than the configured floor.
#[test]
fn an_unreadable_pool_falls_back_to_the_floor_not_to_parity() {
    let f = setup();
    f.pool.set_broken(&true);
    let p = f.router.lastprice(&Asset::Stellar(f.yxlm.clone())).unwrap();
    assert_eq!(p.price, PRICE_XLM * 9_000 / 10_000);
    assert!(p.price < PRICE_XLM, "must not assume parity when blind");
}

#[test]
fn a_pegged_asset_without_its_peg_price_returns_none() {
    let f = setup();
    let orphan = Address::generate(&f.env);
    f.router.set_source(
        &f.admin,
        &f.yxlm,
        &PriceSource::Pegged(PegConfig {
            peg_to: orphan, // upstream has no price for this
            pool: None,
            in_idx: 1,
            out_idx: 0,
            probe_amount: 1_000u128,
            min_ratio_bps: 9_000,
        }),
    );
    assert!(f
        .router
        .lastprice(&Asset::Stellar(f.yxlm.clone()))
        .is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// Pushed prices
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn pushed_prices_are_served_and_expire() {
    let f = setup();
    let asset = Address::generate(&f.env);
    f.router.set_source(&f.admin, &asset, &PriceSource::Pushed);
    f.router.push_price(&f.admin, &asset, &PRICE_XLM);

    let p = f.router.lastprice(&Asset::Stellar(asset.clone())).unwrap();
    assert_eq!(p.price, PRICE_XLM);

    // Past max_age the price is treated as absent rather than served stale.
    f.env.ledger().set_timestamp(1_700_000_000 + 3_601);
    assert!(f.router.lastprice(&Asset::Stellar(asset)).is_none());
}

/// A push oracle puts a key into the collateral path. One bad update — fat
/// finger or compromise — must not be able to move the price far.
#[test]
fn a_single_push_cannot_move_the_price_beyond_the_step_guard() {
    let f = setup();
    let asset = Address::generate(&f.env);
    f.router.set_source(&f.admin, &asset, &PriceSource::Pushed);
    f.router.push_price(&f.admin, &asset, &PRICE_XLM);

    // Default guard is 5%; a doubling must be refused.
    assert!(f
        .router
        .try_push_price(&f.admin, &asset, &(PRICE_XLM * 2))
        .is_err());
    // And so must a collapse.
    assert!(f
        .router
        .try_push_price(&f.admin, &asset, &(PRICE_XLM / 2))
        .is_err());
    // A move inside the guard is fine.
    f.router
        .push_price(&f.admin, &asset, &(PRICE_XLM * 10_200 / 10_000));
}

#[test]
fn the_step_guard_is_configurable_but_bounded() {
    let f = setup();
    f.router.set_push_guard(
        &f.admin,
        &PushGuard {
            max_step_bps: 1_000,
            max_age_secs: 600,
        },
    );
    assert_eq!(f.router.get_push_guard().max_step_bps, 1_000);
    // A guard at or above 100% would defeat the point.
    assert!(f
        .router
        .try_set_push_guard(
            &f.admin,
            &PushGuard {
                max_step_bps: 10_000,
                max_age_secs: 600,
            }
        )
        .is_err());
}

#[test]
fn pushing_requires_admin() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    let asset = Address::generate(&f.env);
    f.router.set_source(&f.admin, &asset, &PriceSource::Pushed);
    assert!(f
        .router
        .try_push_price(&stranger, &asset, &PRICE_XLM)
        .is_err());
}

#[test]
fn non_positive_pushes_are_refused() {
    let f = setup();
    let asset = Address::generate(&f.env);
    f.router.set_source(&f.admin, &asset, &PriceSource::Pushed);
    assert!(f.router.try_push_price(&f.admin, &asset, &0i128).is_err());
    assert!(f.router.try_push_price(&f.admin, &asset, &-1i128).is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_floor_above_the_peg_is_refused() {
    let f = setup();
    assert!(f
        .router
        .try_set_source(
            &f.admin,
            &f.yxlm,
            &PriceSource::Pegged(PegConfig {
                peg_to: f.xlm.clone(),
                pool: None,
                in_idx: 1,
                out_idx: 0,
                probe_amount: 1_000u128,
                min_ratio_bps: 10_001,
            })
        )
        .is_err());
}

#[test]
fn identical_pool_indices_are_refused() {
    let f = setup();
    assert!(f
        .router
        .try_set_source(
            &f.admin,
            &f.yxlm,
            &PriceSource::Pegged(PegConfig {
                peg_to: f.xlm.clone(),
                pool: None,
                in_idx: 1,
                out_idx: 1,
                probe_amount: 1_000u128,
                min_ratio_bps: 9_000,
            })
        )
        .is_err());
}

#[test]
fn only_admin_can_configure() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert!(f
        .router
        .try_set_source(&stranger, &f.yxlm, &PriceSource::Upstream)
        .is_err());
    assert!(f
        .router
        .try_set_upstream(&stranger, &f.xlm.clone())
        .is_err());
}

#[test]
fn initialize_is_not_repeatable() {
    let f = setup();
    let up = f.router.get_upstream();
    assert!(f.router.try_initialize(&f.admin, &up, &300u32).is_err());
}

#[test]
fn zero_resolution_is_refused_at_initialization_and_update() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::from_string(&String::from_str(&env, ADMIN_G));
    let upstream = Address::generate(&env);
    let router_id = env.register(PriceRouter, ());
    let router = PriceRouterClient::new(&env, &router_id);

    assert!(router.try_initialize(&admin, &upstream, &0u32).is_err());
    router.initialize(&admin, &upstream, &300u32);
    assert!(router.try_set_resolution(&admin, &0u32).is_err());
    assert_eq!(router.resolution(), 300u32);
}

#[test]
fn observed_ratio_is_exposed_for_monitoring() {
    let f = setup();
    f.pool.set_ratio_bps(&9_800u32);
    assert_eq!(f.router.get_observed_ratio_bps(&f.yxlm), Some(9_800));
    // Non-pegged assets report nothing.
    assert_eq!(f.router.get_observed_ratio_bps(&f.xlm), None);
}
