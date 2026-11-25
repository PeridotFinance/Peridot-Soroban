extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger};
use soroban_sdk::{contract, contractimpl, symbol_short, Env};

const ONE: i128 = 100_000_000_000_000; // 1e14, Reflector decimals
const R7: u128 = 10_000_000;
const NOW: u64 = 1_795_606_800;

#[contracttype]
#[derive(Clone)]
enum MKey {
    Price(Asset),
    Decimals,
    Base,
    Tokens,
    Rate, // raw USDC out per 1e7 raw PYUSD in
    Killed,
    Broken,
}

#[contract]
struct MockReflector;

#[contractimpl]
impl MockReflector {
    pub fn set(e: Env, asset: Asset, price: i128, timestamp: u64) {
        e.storage()
            .instance()
            .set(&MKey::Price(asset), &PriceData { price, timestamp });
    }
    pub fn set_meta(e: Env, decimals: u32, base: Asset) {
        e.storage().instance().set(&MKey::Decimals, &decimals);
        e.storage().instance().set(&MKey::Base, &base);
    }
    pub fn lastprice(e: Env, asset: Asset) -> Option<PriceData> {
        e.storage().instance().get(&MKey::Price(asset))
    }
    pub fn decimals(e: Env) -> u32 {
        e.storage().instance().get(&MKey::Decimals).unwrap_or(14)
    }
    pub fn resolution(_e: Env) -> u32 {
        300
    }
    pub fn base(e: Env) -> Asset {
        e.storage().instance().get(&MKey::Base).unwrap()
    }
}

#[contract]
struct MockPool;

#[contractimpl]
impl MockPool {
    pub fn init(e: Env, tokens: Vec<Address>) {
        e.storage().instance().set(&MKey::Tokens, &tokens);
    }
    pub fn set_bid_bps(e: Env, bps: u128) {
        e.storage()
            .instance()
            .set(&MKey::Rate, &(bps * R7 / 10_000));
    }
    pub fn set_killed(e: Env, killed: bool) {
        e.storage().instance().set(&MKey::Killed, &killed);
    }
    pub fn set_broken(e: Env, broken: bool) {
        e.storage().instance().set(&MKey::Broken, &broken);
    }
    pub fn get_tokens(e: Env) -> Vec<Address> {
        e.storage().instance().get(&MKey::Tokens).unwrap()
    }
    pub fn get_is_killed_swap(e: Env) -> bool {
        e.storage().instance().get(&MKey::Killed).unwrap_or(false)
    }
    pub fn estimate_swap(e: Env, in_idx: u32, out_idx: u32, in_amount: u128) -> u128 {
        assert!(in_idx == 0 && out_idx == 1, "oracle only sells PYUSD");
        if e.storage().instance().get(&MKey::Broken).unwrap_or(false) {
            panic!("pool broken");
        }
        let r: u128 = e.storage().instance().get(&MKey::Rate).unwrap();
        in_amount * r / R7
    }
}

struct F {
    e: Env,
    oracle: PegOracleClient<'static>,
    refl: MockReflectorClient<'static>,
    dex: MockReflectorClient<'static>,
    p0: MockPoolClient<'static>,
    p1: MockPoolClient<'static>,
    pyusd: Address,
    usdc: Address,
}

fn usdc_sym(e: &Env) -> Asset {
    Asset::Other(Symbol::new(e, "USDC"))
}

fn setup() -> F {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = NOW);
    let pyusd = Address::generate(&e);
    let usdc = Address::generate(&e);
    let refl = MockReflectorClient::new(&e, &e.register(MockReflector, ()));
    let dex = MockReflectorClient::new(&e, &e.register(MockReflector, ()));
    let p0 = MockPoolClient::new(&e, &e.register(MockPool, ()));
    let p1 = MockPoolClient::new(&e, &e.register(MockPool, ()));
    refl.set_meta(&14, &Asset::Other(symbol_short!("USD")));
    dex.set_meta(&14, &Asset::Stellar(usdc.clone()));
    for p in [&p0, &p1] {
        p.init(&Vec::from_array(&e, [pyusd.clone(), usdc.clone()]));
        p.set_bid_bps(&9_990); // 10 bps fee, like the live 0.1 % pools
    }
    refl.set(&usdc_sym(&e), &ONE, &(NOW - 100));
    refl.set(
        &Asset::Other(symbol_short!("XLM")),
        &(ONE / 4),
        &(NOW - 100),
    );
    dex.set(&Asset::Stellar(pyusd.clone()), &ONE, &(NOW - 40));
    let c = PegConfig {
        reflector: refl.address.clone(),
        dex_feed: dex.address.clone(),
        pegged: pyusd.clone(),
        pegged_symbol: Symbol::new(&e, "PYUSD"),
        quote: usdc.clone(),
        quote_symbol: Symbol::new(&e, "USDC"),
        pools: Vec::from_array(
            &e,
            [
                PoolRef {
                    pool: p0.address.clone(),
                    pegged_idx: 0,
                    quote_idx: 1,
                },
                PoolRef {
                    pool: p1.address.clone(),
                    pegged_idx: 0,
                    quote_idx: 1,
                },
            ],
        ),
        probe: 1_000 * R7,
        band_bps: 100,
        max_age_secs: 600,
    };
    let oracle = PegOracleClient::new(&e, &e.register(PegOracle, (c,)));
    F {
        e,
        oracle,
        refl,
        dex,
        p0,
        p1,
        pyusd,
        usdc,
    }
}

fn pegged(f: &F) -> Asset {
    Asset::Stellar(f.pyusd.clone())
}
fn dex_bps(f: &F, bps: i128) {
    f.dex.set(&pegged(f), &(ONE * bps / 10_000), &(NOW - 40));
}
fn price(f: &F) -> Option<i128> {
    f.oracle.lastprice(&pegged(f)).map(|p| p.price)
}

#[test]
fn par_is_exactly_one_with_older_reflector_timestamp() {
    let f = setup();
    let p = f.oracle.lastprice(&pegged(&f)).unwrap();
    assert_eq!(p.price, ONE);
    assert_eq!(
        p.timestamp,
        NOW - 100,
        "older of USDC and DEX-feed timestamps, never now"
    );
    assert_eq!(
        f.oracle
            .lastprice(&Asset::Other(Symbol::new(&f.e, "PYUSD"))),
        Some(p)
    );
    let st = f.oracle.peg_status();
    assert!(st.priced && st.at_peg && st.missing == 0);
    assert_eq!(st.pool0_bid_usd, ONE * 9_990 / 10_000);
}

#[test]
fn other_assets_are_forwarded_and_pegged_is_never_forwarded() {
    let f = setup();
    let xlm = Asset::Other(symbol_short!("XLM"));
    assert_eq!(f.oracle.lastprice(&xlm), f.refl.lastprice(&xlm));
    assert_eq!(f.oracle.lastprice(&Asset::Stellar(f.usdc.clone())), None);
    assert_eq!((f.oracle.decimals(), f.oracle.resolution()), (14, 300));
    f.refl
        .set(&Asset::Other(Symbol::new(&f.e, "PYUSD")), &(2 * ONE), &NOW);
    assert_eq!(
        f.oracle
            .lastprice(&Asset::Other(Symbol::new(&f.e, "PYUSD")))
            .unwrap()
            .price,
        ONE
    );
}

#[test]
fn never_above_one() {
    let f = setup();
    f.p0.set_bid_bps(&10_300);
    f.p1.set_bid_bps(&10_300);
    dex_bps(&f, 10_300);
    assert_eq!(price(&f), Some(ONE));
    f.refl
        .set(&usdc_sym(&f.e), &(ONE * 102 / 100), &(NOW - 100));
    assert_eq!(price(&f), Some(ONE));
}

#[test]
fn one_manipulated_source_cannot_move_the_price() {
    let f = setup();
    f.p0.set_bid_bps(&9_000); // flash-crashed concentrated pool
    assert_eq!(price(&f), Some(ONE));
    f.p0.set_bid_bps(&9_990);
    dex_bps(&f, 9_000); // printed DEX-feed round
    assert_eq!(price(&f), Some(ONE));
    dex_bps(&f, 10_000);
    f.p1.set_bid_bps(&9_000); // stable pool moved
    assert_eq!(price(&f), Some(ONE));
}

#[test]
fn two_depressed_sources_mark_down_to_the_median_and_emit() {
    let f = setup();
    f.p1.set_bid_bps(&9_700);
    dex_bps(&f, 9_650);
    // Real depeg; the concentrated pool is flash-restored to par: the median stays depressed.
    f.p0.set_bid_bps(&10_000);
    assert_eq!(price(&f), Some(ONE * 9_700 / 10_000));
    let ev = f.e.events().all().filter_by_contract(&f.oracle.address);
    assert_eq!(ev.events().len(), 1, "PegMarkedDown");
    let st = f.oracle.peg_status();
    assert!(st.priced && !st.at_peg && st.observed == st.price);
}

#[test]
fn band_edge_is_inclusive() {
    let f = setup();
    for p in [&f.p0, &f.p1] {
        p.set_bid_bps(&9_900);
    }
    dex_bps(&f, 9_900);
    assert_eq!(price(&f), Some(ONE), "observed exactly 0.99 is at peg");
    f.refl.set(&usdc_sym(&f.e), &(ONE - 1), &(NOW - 100));
    let p = price(&f).unwrap();
    assert!(
        p < ONE * 9_900 / 10_000 && p > ONE * 9_899 / 10_000,
        "one unit lower marks down: {p}"
    );
}

#[test]
fn one_missing_source_uses_the_lower_of_the_other_two() {
    let f = setup();
    f.p1.set_killed(&true);
    assert_eq!(price(&f), Some(ONE));
    dex_bps(&f, 9_950); // agree within the band: the lower is used (inside the band -> $1)
    assert_eq!(price(&f), Some(ONE));
    dex_bps(&f, 9_850);
    f.p0.set_bid_bps(&9_900);
    assert_eq!(
        price(&f),
        Some(ONE * 9_850 / 10_000),
        "0.99 vs 0.985 agree: marked down to the lower"
    );
    f.p0.set_bid_bps(&9_990);
    assert_eq!(
        price(&f),
        None,
        "two sources disagree beyond the band: no single source decides"
    );
    assert_eq!(f.oracle.peg_status().missing, M_POOL1);
    f.p1.set_killed(&false);
    f.p1.set_broken(&true);
    assert_eq!(f.oracle.peg_status().missing, M_POOL1);
    f.dex.set_meta(&7, &Asset::Stellar(f.usdc.clone()));
    assert_eq!(price(&f), None, "two sources missing: unpriced");
    let ev = f.e.events().all().filter_by_contract(&f.oracle.address);
    assert_eq!(ev.events().len(), 1, "PegUnpriced");
}

#[test]
fn stale_feed_plus_crashed_concentrated_pool_cannot_set_the_price() {
    let f = setup();
    f.dex.set(&pegged(&f), &ONE, &(NOW - 700)); // feed missed a round
    f.p0.set_bid_bps(&4_550); // concentrated pool drained in-transaction
    assert_eq!(price(&f), None, "no liquidation at a one-pool price");
    f.p0.set_bid_bps(&9_990);
    assert_eq!(price(&f), Some(ONE));
}

#[test]
fn usdc_inputs_gate_everything() {
    let f = setup();
    f.refl.set(&usdc_sym(&f.e), &ONE, &(NOW - 601));
    assert_eq!(price(&f), None);
    f.refl.set(&usdc_sym(&f.e), &ONE, &(NOW + 1));
    assert_eq!(price(&f), None);
    f.refl.set(&usdc_sym(&f.e), &ONE, &(NOW - 600));
    assert_eq!(price(&f), Some(ONE), "age == max_age is fresh");
    f.refl.set(&usdc_sym(&f.e), &(ONE * 97 / 100), &(NOW - 100));
    let p = price(&f).unwrap();
    assert!(p < ONE * 98 / 100, "USDC depeg marks PYUSD down too: {p}");
    f.dex.set(&pegged(&f), &ONE, &(NOW - 700));
    assert_eq!(f.oracle.peg_status().missing, M_DEX);
}

#[test]
fn constructor_rejects_bad_policy_and_wrong_pools() {
    let f = setup();
    let e = &f.e;
    let c0 = f.oracle.get_config();
    let try_new = |c: PegConfig| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.register(PegOracle, (c,))))
    };
    let mut c = c0.clone();
    c.band_bps = 501;
    assert!(try_new(c).is_err());
    let mut c = c0.clone();
    c.max_age_secs = 3_601;
    assert!(try_new(c).is_err());
    let mut c = c0.clone();
    c.pools = Vec::from_array(e, [c0.pools.get(0).unwrap()]);
    assert!(try_new(c).is_err(), "exactly two pools");
    let mut c = c0.clone();
    c.pools = Vec::from_array(e, [c0.pools.get(0).unwrap(), c0.pools.get(0).unwrap()]);
    assert!(try_new(c).is_err(), "distinct pools");
    let mut c = c0.clone();
    c.pools.set(
        1,
        PoolRef {
            pool: f.p1.address.clone(),
            pegged_idx: 1,
            quote_idx: 0,
        },
    );
    assert!(try_new(c).is_err(), "token order must match the pool");
    let mut c = c0.clone();
    c.dex_feed = c.reflector.clone();
    assert!(try_new(c).is_err());
    assert!(try_new(c0).is_ok());
}
