use super::*;
use crate::test::{MockUpstream, MockUpstreamClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
    token, vec,
};
const SCALE: u128 = observation::RATIO_SCALE;

#[test]
fn atomic_snapshot_preserves_observation_invalidation_and_expiry() {
    let (e, id, _, who, a, q, _) = fixture();
    let r = PriceRouterClient::new(&e, &id);
    let dependent = Asset::Stellar(q);
    assert!(r.price_snapshot(&dependent).is_none());
    r.publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300);
    assert!(r.price_snapshot(&dependent).is_some());
    r.invalidate_observation(&who, &a);
    assert!(r.price_snapshot(&dependent).is_none());
    e.ledger().set_timestamp(1_700_000_600);
    r.publish_observation(&who, &a, &SCALE, &1_699_998_800, &1_700_000_600);
    assert!(r.price_snapshot(&dependent).is_some());
    e.ledger().set_timestamp(1_700_000_901);
    assert!(r.price_snapshot(&dependent).is_none());
}
#[contract]
struct Quotes;
#[contractimpl]
impl Quotes {
    pub fn init(env: Env, a: Address, b: Address) {
        env.storage().instance().set(&0u32, &vec![&env, a, b]);
    }
    pub fn get_tokens(env: Env) -> Vec<Address> {
        env.storage().instance().get(&0u32).unwrap()
    }
    pub fn set(env: Env, bid: u128, ask: u128) {
        env.storage().instance().set(&1u32, &bid);
        env.storage().instance().set(&2u32, &ask);
    }
    pub fn estimate_swap(env: Env, i: u32, _j: u32, amount: u128) -> u128 {
        let key = if i == 0 { 1u32 } else { 2u32 };
        let ratio: u128 = env.storage().instance().get(&key).unwrap_or(SCALE);
        if i == 0 {
            amount * ratio / SCALE
        } else {
            amount * SCALE / ratio
        }
    }
}
fn fixture() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    ObservationConfig,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);
    let admin = Address::from_string(&String::from_str(
        &env,
        "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ",
    ));
    let reporter = Address::generate(&env);
    let asset = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let quote = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let pool = env.register(Quotes, ());
    QuotesClient::new(&env, &pool).init(&asset, &quote);
    let up = env.register(MockUpstream, ());
    MockUpstreamClient::new(&env, &up).set_price(&quote, &100_000_000_000_000);
    let id = env.register(PriceRouter, ());
    let r = PriceRouterClient::new(&env, &id);
    r.initialize(&admin, &up, &300);
    let cfg = ObservationConfig {
        reporter: reporter.clone(),
        quote_to: quote.clone(),
        pool,
        in_idx: 0,
        out_idx: 1,
        probe_amount: 1_000_000_000,
        window_secs: 1800,
        max_age_secs: 300,
        min_interval_secs: 300,
        max_deviation_bps: 100,
        max_step_bps: 100,
        min_ratio: SCALE * 80 / 100,
        max_ratio: SCALE * 105 / 100,
    };
    r.set_source(&admin, &asset, &PriceSource::Observed(cfg.clone()));
    r.set_required_observation(&admin, &quote, &Some(asset.clone()));
    env.ledger().set_timestamp(1_700_000_300);
    (env, id, admin, reporter, asset, quote, cfg)
}
#[test]
fn observations_expire_from_window_end_and_gate_reference_without_recursion() {
    let (e, id, _, who, a, q, _) = fixture();
    let r = PriceRouterClient::new(&e, &id);
    assert!(r.lastprice(&Asset::Stellar(q.clone())).is_none());
    r.publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300);
    assert_eq!(
        r.lastprice(&Asset::Stellar(a.clone())).unwrap().price,
        100_000_000_000_000
    );
    assert!(r.lastprice(&Asset::Stellar(q.clone())).is_some());
    e.ledger().set_timestamp(1_700_000_601);
    assert!(r.lastprice(&Asset::Stellar(a)).is_none());
    assert!(r.lastprice(&Asset::Stellar(q)).is_none());
}
#[test]
fn report_requires_exact_reporter_auth_and_never_grants_administration() {
    let (e, id, _, who, a, _, _) = fixture();
    let r = PriceRouterClient::new(&e, &id);
    e.mock_auths(&[]);
    assert!(r
        .try_publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300)
        .is_err());
    e.mock_auths(&[MockAuth {
        address: &who,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "publish_observation",
            args: (
                who.clone(),
                a.clone(),
                SCALE,
                1_699_998_500u64,
                1_700_000_300u64,
            )
                .into_val(&e),
            sub_invokes: &[],
        },
    }]);
    r.publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300);
    assert!(r.try_set_source(&who, &a, &PriceSource::Upstream).is_err());
}
#[test]
fn report_rejects_replays_future_stale_short_windows_and_excessive_steps() {
    let (e, id, _, who, a, _, _) = fixture();
    let r = PriceRouterClient::new(&e, &id);
    for (start, end) in [
        (1_699_998_501, 1_700_000_300),
        (1_699_998_501, 1_700_000_301),
        (1_699_998_100, 1_699_999_900),
    ] {
        assert!(r
            .try_publish_observation(&who, &a, &SCALE, &start, &end)
            .is_err());
    }
    r.publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300);
    assert!(r
        .try_publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300)
        .is_err());
    e.ledger().set_timestamp(1_700_000_600);
    assert!(r
        .try_publish_observation(
            &who,
            &a,
            &(SCALE * 102 / 100),
            &1_699_998_800,
            &1_700_000_600
        )
        .is_err());
    assert!(r
        .try_publish_observation(&who, &a, &SCALE, &1_699_998_600, &1_700_000_400)
        .is_err());
}
#[test]
fn report_cross_checks_both_directions_and_invalidation_cannot_refresh_old_data() {
    let (e, id, _, who, a, q, cfg) = fixture();
    let r = PriceRouterClient::new(&e, &id);
    QuotesClient::new(&e, &cfg.pool).set(&SCALE, &(SCALE * 105 / 100));
    assert!(r
        .try_publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300)
        .is_err());
    QuotesClient::new(&e, &cfg.pool).set(&SCALE, &SCALE);
    r.publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300);
    r.invalidate_observation(&who, &a);
    assert!(r.lastprice(&Asset::Stellar(q.clone())).is_none());
    assert!(r
        .try_publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300)
        .is_err());
    e.ledger().set_timestamp(1_700_000_600);
    r.publish_observation(&who, &a, &SCALE, &1_699_998_800, &1_700_000_600);
    assert!(r.lastprice(&Asset::Stellar(q)).is_some());
}
#[test]
fn missing_configuration_or_state_cannot_bypass_dependency() {
    let (e, id, admin, who, a, q, cfg) = fixture();
    let r = PriceRouterClient::new(&e, &id);
    r.publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300);
    e.as_contract(&id, || {
        e.storage().persistent().remove(&DataKey::Source(a.clone()))
    });
    assert!(r.lastprice(&Asset::Stellar(q.clone())).is_none());
    r.set_source(&admin, &a, &PriceSource::Observed(cfg));
    assert!(r.lastprice(&Asset::Stellar(q)).is_none());
    assert!(r
        .try_publish_observation(&who, &a, &SCALE, &1_699_998_500, &1_700_000_300)
        .is_err());
}
#[test]
fn configuration_checks_pool_asset_identity_and_decimal_compatibility() {
    let (e, id, admin, _, a, _, mut cfg) = fixture();
    let r = PriceRouterClient::new(&e, &id);
    cfg.in_idx = 1;
    cfg.out_idx = 0;
    assert!(r
        .try_set_source(&admin, &a, &PriceSource::Observed(cfg.clone()))
        .is_err());
    cfg.in_idx = 0;
    cfg.out_idx = 1;
    cfg.max_age_secs = 3600;
    assert!(r
        .try_set_source(&admin, &a, &PriceSource::Observed(cfg))
        .is_err());
    assert_eq!(token::Client::new(&e, &a).decimals(), 7);
}
