#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, testutils::Ledger as _, Address,
    Env, String, Vec,
};

use mock_token::{MockToken, MockTokenClient};

use crate::math::{isqrt, mul_div, mul_div_ceil};
use crate::oracle::{Asset, PriceData};
use crate::{AquariusLpVault, AquariusLpVaultClient};

// The admin the contract pins at build time (test builds fall back to this).
const ADMIN_G: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";

// ─────────────────────────────────────────────────────────────────────────────
// Mock Reflector oracle
// ─────────────────────────────────────────────────────────────────────────────

#[contracttype]
enum OracleKey {
    Price(Address),
    Fail,
    Stale,
}

#[contract]
pub struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn set_price(env: Env, token: Address, price: i128) {
        env.storage()
            .persistent()
            .set(&OracleKey::Price(token), &price);
    }

    pub fn set_fail(env: Env, fail: bool) {
        env.storage().persistent().set(&OracleKey::Fail, &fail);
    }

    /// Backdates the reported timestamp so the vault's staleness check trips.
    pub fn set_stale(env: Env, stale: bool) {
        env.storage().persistent().set(&OracleKey::Stale, &stale);
    }

    pub fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&OracleKey::Fail)
            .unwrap_or(false)
        {
            panic!("oracle down");
        }
        let addr = match asset {
            Asset::Stellar(a) => a,
            Asset::Other(_) => return None,
        };
        let price: i128 = env
            .storage()
            .persistent()
            .get(&OracleKey::Price(addr))
            .unwrap_or(0);
        if price == 0 {
            return None;
        }
        let now = env.ledger().timestamp();
        let ts = if env
            .storage()
            .persistent()
            .get::<_, bool>(&OracleKey::Stale)
            .unwrap_or(false)
        {
            now.saturating_sub(100_000)
        } else {
            now
        };
        Some(PriceData {
            price,
            timestamp: ts,
        })
    }

    pub fn resolution(_env: Env) -> u32 {
        300
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fixture
// ─────────────────────────────────────────────────────────────────────────────

struct Fixture {
    env: Env,
    admin: Address,
    vault: AquariusLpVaultClient<'static>,
    vault_id: Address,
    pool: mock_aquarius_pool::MockAquariusPoolClient<'static>,
    pool_id: Address,
    usdc: MockTokenClient<'static>,
    eurc: MockTokenClient<'static>,
    aqua: MockTokenClient<'static>,
    usdc_id: Address,
    eurc_id: Address,
    aqua_id: Address,
    oracle: MockOracleClient<'static>,
}

/// 1e14-scaled prices, matching Reflector's convention.
const PRICE_USDC: i128 = 100_000_000_000_000; // $1.00
const PRICE_EURC: i128 = 116_700_000_000_000; // $1.167
const PRICE_AQUA: i128 = 37_400_000_000; // $0.000374

fn deploy_token(env: &Env, symbol: &str) -> (Address, MockTokenClient<'static>) {
    let id = env.register(MockToken, ());
    let client = MockTokenClient::new(env, &id);
    client.initialize(
        &String::from_str(env, symbol),
        &String::from_str(env, symbol),
        &7u32,
    );
    (id, client)
}

fn setup() -> Fixture {
    let env = Env::default();
    // The vault authorizes its own nested pool calls via
    // `authorize_as_current_contract`, which is non-root auth.
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().set_timestamp(1_700_000_000);

    let admin = Address::from_string(&String::from_str(&env, ADMIN_G));

    let (usdc_id, usdc) = deploy_token(&env, "USDC");
    let (eurc_id, eurc) = deploy_token(&env, "EURC");
    let (aqua_id, aqua) = deploy_token(&env, "AQUA");

    // Aquarius sorts pool tokens by contract id; the fixture only needs a
    // stable order, and the vault reads whichever index it is told to settle in.
    let (token0, token1) = if usdc_id < eurc_id {
        (usdc_id.clone(), eurc_id.clone())
    } else {
        (eurc_id.clone(), usdc_id.clone())
    };
    let underlying_index: u32 = if token0 == usdc_id { 0 } else { 1 };

    let pool_id = env.register(mock_aquarius_pool::MockAquariusPool, ());
    let pool = mock_aquarius_pool::MockAquariusPoolClient::new(&env, &pool_id);
    pool.initialize(&token0, &token1, &60i32, &30u32);
    pool.set_reward_tokens(&aqua_id, &aqua_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.set_price(&usdc_id, &PRICE_USDC);
    oracle.set_price(&eurc_id, &PRICE_EURC);
    oracle.set_price(&aqua_id, &PRICE_AQUA);

    let vault_id = env.register(AquariusLpVault, ());
    let vault = AquariusLpVaultClient::new(&env, &vault_id);
    vault.initialize(&admin, &pool_id, &underlying_index, &oracle_id);

    Fixture {
        env,
        admin,
        vault,
        vault_id,
        pool,
        pool_id,
        usdc,
        eurc,
        aqua,
        usdc_id,
        eurc_id,
        aqua_id,
        oracle,
    }
}

/// Seeds the pool with a balanced book so swaps and deposits have depth.
fn seed_pool(f: &Fixture, usdc_amount: i128, eurc_amount: i128) {
    let lp = Address::generate(&f.env);
    f.usdc.mint(&lp, &usdc_amount);
    f.eurc.mint(&lp, &eurc_amount);
    let (a0, a1) = if f.usdc_id < f.eurc_id {
        (usdc_amount as u128, eurc_amount as u128)
    } else {
        (eurc_amount as u128, usdc_amount as u128)
    };
    let mut desired: Vec<u128> = Vec::new(&f.env);
    desired.push_back(a0);
    desired.push_back(a1);
    f.pool
        .deposit_position(&lp, &-887_220i32, &887_220i32, &desired, &0u128);
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure math
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn isqrt_is_floor_of_the_true_root() {
    assert_eq!(isqrt(0), 0);
    assert_eq!(isqrt(1), 1);
    assert_eq!(isqrt(3), 1);
    assert_eq!(isqrt(4), 2);
    assert_eq!(isqrt(99), 9);
    assert_eq!(isqrt(100), 10);
    assert_eq!(isqrt(1_000_000_000_000_000_000u128), 1_000_000_000u128);
    // Largest perfect square that fits in u128.
    let big = u128::MAX;
    let r = isqrt(big);
    assert!(r.checked_mul(r).is_some());
    assert!((r + 1).checked_mul(r + 1).is_none() || (r + 1) * (r + 1) > big);
}

#[test]
fn mul_div_avoids_intermediate_overflow() {
    let huge = u128::MAX / 2;
    // Would overflow if computed as `a * b` first.
    assert_eq!(mul_div(huge, 4, 4), huge);
    assert_eq!(mul_div(10, 3, 4), 7);
    assert_eq!(mul_div_ceil(10, 3, 4), 8);
    assert_eq!(mul_div_ceil(8, 2, 4), 4);
    assert_eq!(mul_div(0, 5, 3), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Initialization
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn initialize_derives_full_range_from_tick_spacing() {
    let f = setup();
    // 887272 aligned down to a multiple of 60.
    assert_eq!(f.vault.get_ticks(), (-887_220i32, 887_220i32));
    assert_eq!(f.vault.get_pool(), f.pool_id);
    assert_eq!(f.vault.get_underlying(), f.usdc_id);
    assert_eq!(f.vault.get_admin(), f.admin);
}

#[test]
#[should_panic(expected = "already initialized")]
fn initialize_is_not_repeatable() {
    let f = setup();
    let oracle_id = env_oracle(&f);
    f.vault.initialize(&f.admin, &f.pool_id, &0u32, &oracle_id);
}

fn env_oracle(f: &Fixture) -> Address {
    f.oracle.address.clone()
}

// ─────────────────────────────────────────────────────────────────────────────
// NAV
// ─────────────────────────────────────────────────────────────────────────────

/// The load-bearing property: a full-range position of `L` liquidity units is
/// worth `2 * L * sqrt(p_other / p_underlying)` in the underlying token.
///
/// Checked against the real mainnet figures: 1000 USDC + 862 EURC in the
/// live pool mints 9_256_822_340 liquidity, which the formula values at
/// ~2000 USDC.
#[test]
fn nav_matches_the_full_range_closed_form() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let depositor = Address::generate(&f.env);
    let amount = 2_000_0000000i128; // 2000 USDC
    f.usdc.mint(&depositor, &amount);
    f.vault.deposit_underlying(&depositor, &amount, &0i128);

    let liq = f.vault.get_position_liquidity();
    assert!(liq > 0, "expected liquidity to be minted");

    // 2 * L * sqrt(1.167) with 1e9-scaled root arithmetic.
    let ratio = mul_div(
        PRICE_EURC as u128,
        1_000_000_000_000_000_000u128,
        PRICE_USDC as u128,
    );
    let expected = mul_div(liq, isqrt(ratio), 1_000_000_000u128) * 2;

    let nav = f.vault.get_total_underlying() as u128;
    let idle = f.vault.get_idle() as u128;
    let position = nav - idle;
    assert_eq!(position, expected);

    // And the round trip should be worth roughly what went in, minus the
    // swap fee paid to convert half the deposit into the paired token.
    assert!(nav > 1_970_0000000u128, "nav too low: {}", nav);
    assert!(nav <= 2_000_0000000u128, "nav above principal: {}", nav);
}

/// A swap that moves the pool's spot price must not move reported NAV.
/// This is the whole reason NAV is oracle-derived rather than read from
/// `get_slot0` or `get_reserves`.
#[test]
fn nav_is_immune_to_spot_price_manipulation() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let depositor = Address::generate(&f.env);
    f.usdc.mint(&depositor, &2_000_0000000i128);
    f.vault
        .deposit_underlying(&depositor, &2_000_0000000i128, &0i128);

    let nav_before = f.vault.get_total_underlying();
    let liq_before = f.vault.get_position_liquidity();

    // Whale swings the pool hard in one direction.
    let whale = Address::generate(&f.env);
    f.usdc.mint(&whale, &500_000_0000000i128);
    let (in_idx, out_idx) = if f.usdc_id < f.eurc_id {
        (0u32, 1u32)
    } else {
        (1u32, 0u32)
    };
    f.pool
        .swap(&whale, &in_idx, &out_idx, &500_000_0000000u128, &0u128);

    assert_eq!(f.vault.get_position_liquidity(), liq_before);
    assert_eq!(
        f.vault.get_total_underlying(),
        nav_before,
        "spot manipulation leaked into NAV"
    );
}

#[test]
fn nav_falls_back_to_last_good_root_when_the_oracle_fails() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let depositor = Address::generate(&f.env);
    f.usdc.mint(&depositor, &2_000_0000000i128);
    f.vault
        .deposit_underlying(&depositor, &2_000_0000000i128, &0i128);

    let nav_before = f.vault.get_total_underlying();
    f.oracle.set_fail(&true);
    assert_eq!(f.vault.get_total_underlying(), nav_before);

    // Staleness takes the same path.
    f.oracle.set_fail(&false);
    f.oracle.set_stale(&true);
    assert_eq!(f.vault.get_total_underlying(), nav_before);
}

// ─────────────────────────────────────────────────────────────────────────────
// DeFindex-compatible surface
// ─────────────────────────────────────────────────────────────────────────────

/// `receipt-vault` sizes its `min_amounts_out` vector from the length of this
/// return value and reads index 0 as the underlying amount.
#[test]
fn get_asset_amounts_per_shares_is_single_asset() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let empty = f.vault.get_asset_amounts_per_shares(&0i128);
    assert_eq!(empty.len(), 1);
    assert_eq!(empty.get(0).unwrap(), 0i128);

    let depositor = Address::generate(&f.env);
    f.usdc.mint(&depositor, &1_000_0000000i128);
    let shares = f
        .vault
        .deposit_underlying(&depositor, &1_000_0000000i128, &0i128);

    let amounts = f.vault.get_asset_amounts_per_shares(&shares);
    assert_eq!(amounts.len(), 1);
    assert!(amounts.get(0).unwrap() > 0);
}

#[test]
fn deposit_then_withdraw_returns_underlying_only() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let market = Address::generate(&f.env);
    let amount = 5_000_0000000i128;
    f.usdc.mint(&market, &amount);

    let mut desired: Vec<i128> = Vec::new(&f.env);
    desired.push_back(amount);
    let mut mins: Vec<i128> = Vec::new(&f.env);
    mins.push_back(amount);
    let shares = f.vault.deposit(&desired, &mins, &market, &true);
    assert!(shares > 0);
    assert_eq!(f.usdc.balance(&market), 0);
    assert_eq!(f.vault.balance(&market), shares);
    assert_eq!(f.vault.total_supply(), shares);

    let mut min_out: Vec<i128> = Vec::new(&f.env);
    min_out.push_back(0i128);
    let out = f.vault.withdraw(&shares, &min_out, &market);

    assert_eq!(out.len(), 1);
    let received = out.get(0).unwrap();
    assert!(received > 0);
    // Two swap legs of 0.3% each, so a ~0.6% round-trip cost is expected.
    assert!(
        received > amount * 985 / 1000,
        "round trip lost too much: {} of {}",
        received,
        amount
    );
    assert_eq!(f.usdc.balance(&market), received);
    assert_eq!(f.vault.balance(&market), 0);
    // No paired token is ever handed back to the market.
    assert_eq!(f.eurc.balance(&market), 0);
}

#[test]
fn deposit_rejects_amounts_below_the_stated_minimum() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let market = Address::generate(&f.env);
    f.usdc.mint(&market, &1_000_0000000i128);

    let mut desired: Vec<i128> = Vec::new(&f.env);
    desired.push_back(100i128);
    let mut mins: Vec<i128> = Vec::new(&f.env);
    mins.push_back(1_000i128);
    assert!(f
        .vault
        .try_deposit(&desired, &mins, &market, &true)
        .is_err());
}

#[test]
fn withdraw_requires_authorisation_from_the_share_holder() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let market = Address::generate(&f.env);
    f.usdc.mint(&market, &1_000_0000000i128);
    let shares = f
        .vault
        .deposit_underlying(&market, &1_000_0000000i128, &0i128);

    // With auths mocked the call succeeds; the assertion here is that the
    // authorisation was actually demanded of `market`.
    let mut min_out: Vec<i128> = Vec::new(&f.env);
    min_out.push_back(0i128);
    f.vault.withdraw(&shares, &min_out, &market);
    let auths = f.env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == market),
        "withdraw did not require auth from the holder"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Share accounting
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn second_depositor_is_not_diluted_by_the_first() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    f.usdc.mint(&a, &1_000_0000000i128);
    f.usdc.mint(&b, &1_000_0000000i128);

    let shares_a = f.vault.deposit_underlying(&a, &1_000_0000000i128, &0i128);
    let shares_b = f.vault.deposit_underlying(&b, &1_000_0000000i128, &0i128);

    // Equal deposits into an otherwise unchanged vault must mint near-equal
    // shares. Each depositor pays their own entry swap cost, so neither
    // subsidises the other; only pool-ratio drift between the two deposits
    // should separate them.
    let diff = (shares_a - shares_b).abs();
    assert!(
        diff * 1000 < shares_a,
        "shares diverged: {} vs {}",
        shares_a,
        shares_b
    );
}

#[test]
fn yield_accrues_to_existing_holders_before_a_new_deposit() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let a = Address::generate(&f.env);
    f.usdc.mint(&a, &1_000_0000000i128);
    let shares_a = f.vault.deposit_underlying(&a, &1_000_0000000i128, &0i128);

    // Simulate fees accruing to the vault's idle balance.
    f.usdc.mint(&f.vault_id, &100_0000000i128);

    let b = Address::generate(&f.env);
    f.usdc.mint(&b, &1_000_0000000i128);
    let shares_b = f.vault.deposit_underlying(&b, &1_000_0000000i128, &0i128);

    assert!(
        shares_b < shares_a,
        "later depositor should get fewer shares once NAV has grown: {} vs {}",
        shares_b,
        shares_a
    );
    let value_a = f
        .vault
        .get_asset_amounts_per_shares(&shares_a)
        .get(0)
        .unwrap();
    let value_b = f
        .vault
        .get_asset_amounts_per_shares(&shares_b)
        .get(0)
        .unwrap();
    assert!(value_a > value_b, "the earlier holder should have earned");
}

#[test]
fn shares_are_transferable() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    f.usdc.mint(&a, &1_000_0000000i128);
    let shares = f.vault.deposit_underlying(&a, &1_000_0000000i128, &0i128);

    let supply_before = f.vault.total_supply();
    f.vault.transfer(&a, &b, &shares);
    assert_eq!(f.vault.balance(&a), 0);
    assert_eq!(f.vault.balance(&b), shares);
    assert_eq!(f.vault.total_supply(), supply_before);
}

// ─────────────────────────────────────────────────────────────────────────────
// Risk controls
// ─────────────────────────────────────────────────────────────────────────────

/// The capacity cap is a yield control as much as a risk control: realised APR
/// scales with `pool_tvl / (pool_tvl + deployed)`.
#[test]
fn max_deploy_caps_what_reaches_the_pool() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    f.vault.set_max_deploy(&f.admin, &500_0000000u128);

    let market = Address::generate(&f.env);
    f.usdc.mint(&market, &5_000_0000000i128);
    f.vault
        .deposit_underlying(&market, &5_000_0000000i128, &0i128);

    let idle = f.vault.get_idle() as u128;
    let nav = f.vault.get_total_underlying() as u128;
    let deployed = nav - idle;
    assert!(
        deployed <= 520_0000000u128,
        "deployed {} exceeded the cap",
        deployed
    );
    assert!(idle > 4_000_0000000u128, "remainder should stay idle");
}

#[test]
fn pausing_blocks_deposits_but_never_withdrawals() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let market = Address::generate(&f.env);
    f.usdc.mint(&market, &2_000_0000000i128);
    let shares = f
        .vault
        .deposit_underlying(&market, &1_000_0000000i128, &0i128);

    f.vault.set_paused(&f.admin, &true);
    assert!(f.vault.is_paused());

    let mut desired: Vec<i128> = Vec::new(&f.env);
    desired.push_back(1_000_0000000i128);
    let mut mins: Vec<i128> = Vec::new(&f.env);
    mins.push_back(0i128);
    assert!(f
        .vault
        .try_deposit(&desired, &mins, &market, &true)
        .is_err());

    // Withdrawals must still work: the market above has to be able to get its
    // cash back regardless of vault state.
    let mut min_out: Vec<i128> = Vec::new(&f.env);
    min_out.push_back(0i128);
    let out = f.vault.withdraw(&shares, &min_out, &market);
    assert!(out.get(0).unwrap() > 0);
}

#[test]
fn withdrawal_survives_a_pool_that_refuses_to_release_liquidity() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let market = Address::generate(&f.env);
    f.usdc.mint(&market, &1_000_0000000i128);
    let shares = f
        .vault
        .deposit_underlying(&market, &1_000_0000000i128, &0i128);

    f.pool.set_fail_withdraw(&true);

    // Must not panic: a hard revert here would freeze every withdrawal in the
    // market this vault backs.
    let mut min_out: Vec<i128> = Vec::new(&f.env);
    min_out.push_back(0i128);
    let out = f.vault.withdraw(&shares, &min_out, &market);
    // Nothing could be raised, so nothing is paid, but the call completes.
    assert_eq!(out.len(), 1);
    assert_eq!(f.vault.balance(&market), 0);
}

#[test]
#[should_panic(expected = "slippage above cap")]
fn slippage_cannot_be_set_above_the_hard_ceiling() {
    let f = setup();
    f.vault.set_slippage_bps(&f.admin, &900u32);
}

#[test]
fn non_admin_cannot_change_configuration() {
    let f = setup();
    let stranger = Address::generate(&f.env);
    assert!(f.vault.try_set_max_deploy(&stranger, &1u128).is_err());
    assert!(f.vault.try_set_paused(&stranger, &true).is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Harvest
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn harvest_sells_rewards_into_underlying_and_redeploys() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let market = Address::generate(&f.env);
    f.usdc.mint(&market, &1_000_0000000i128);
    f.vault
        .deposit_underlying(&market, &1_000_0000000i128, &0i128);

    // A separate AQUA/USDC pool is the route the vault sells rewards through.
    let (t0, t1) = if f.aqua_id < f.usdc_id {
        (f.aqua_id.clone(), f.usdc_id.clone())
    } else {
        (f.usdc_id.clone(), f.aqua_id.clone())
    };
    let route_id = f.env.register(mock_aquarius_pool::MockAquariusPool, ());
    let route = mock_aquarius_pool::MockAquariusPoolClient::new(&f.env, &route_id);
    route.initialize(&t0, &t1, &60i32, &30u32);
    let lp = Address::generate(&f.env);
    f.aqua.mint(&lp, &10_000_000_0000000i128);
    f.usdc.mint(&lp, &3_740_0000000i128);
    let (a0, a1) = if f.aqua_id < f.usdc_id {
        (10_000_000_0000000u128, 3_740_0000000u128)
    } else {
        (3_740_0000000u128, 10_000_000_0000000u128)
    };
    let mut d: Vec<u128> = Vec::new(&f.env);
    d.push_back(a0);
    d.push_back(a1);
    route.deposit_position(&lp, &-887_220i32, &887_220i32, &d, &0u128);
    f.vault
        .set_reward_route(&f.admin, &f.aqua_id, &Some(route_id));

    // Pool owes the vault some AQUA.
    f.aqua.mint(&f.pool_id, &100_000_0000000i128);
    f.pool
        .credit_rewards(&f.vault_id, &100_000_0000000u128, &0u128);

    let nav_before = f.vault.get_total_underlying();
    let caller = Address::generate(&f.env);
    f.vault.harvest(&caller);

    assert_eq!(
        f.aqua.balance(&f.vault_id),
        0,
        "reward should be fully sold"
    );
    assert!(
        f.vault.get_total_underlying() > nav_before,
        "harvest did not increase NAV"
    );
}

#[test]
fn harvest_is_rate_limited() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let caller = Address::generate(&f.env);
    f.vault.harvest(&caller);
    assert!(f.vault.try_harvest(&caller).is_err());

    f.env.ledger().set_timestamp(1_700_000_000 + 3_601);
    f.vault.harvest(&caller);
}

#[test]
fn harvest_skips_a_reward_with_no_route_instead_of_reverting() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    f.aqua.mint(&f.pool_id, &1_000_0000000i128);
    f.pool
        .credit_rewards(&f.vault_id, &1_000_0000000u128, &0u128);

    let caller = Address::generate(&f.env);
    // No route configured for AQUA — must complete anyway.
    f.vault.harvest(&caller);
    assert_eq!(f.aqua.balance(&f.vault_id), 1_000_0000000i128);
}

#[test]
fn sync_liquidity_reconciles_against_the_pool() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let market = Address::generate(&f.env);
    f.usdc.mint(&market, &1_000_0000000i128);
    f.vault
        .deposit_underlying(&market, &1_000_0000000i128, &0i128);

    let tracked = f.vault.get_position_liquidity();
    assert_eq!(f.vault.sync_liquidity(), tracked);
}

#[test]
fn sweep_reward_refuses_to_sell_the_pair_tokens() {
    let f = setup();
    let caller = Address::generate(&f.env);
    assert!(f.vault.try_sweep_reward(&caller, &f.usdc_id).is_err());
    assert!(f.vault.try_sweep_reward(&caller, &f.eurc_id).is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration: drop-in for `receipt-vault`'s boosted-vault socket
//
// This is the test the whole design exists to pass. It runs the *real*
// audited ReceiptVault against this vault through `set_boosted_vault`, so the
// ABI shape, the `authorize_as_current_contract` trees and the single-asset
// accounting are all exercised exactly as they will be on mainnet — no stand-in
// for the market side.
// ─────────────────────────────────────────────────────────────────────────────

mod boosted_market {
    use super::*;
    use receipt_vault::{ReceiptVault, ReceiptVaultClient};

    struct Market {
        f: Fixture,
        market: ReceiptVaultClient<'static>,
    }

    fn setup_market() -> Market {
        let f = setup();
        seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

        let market_id = f.env.register(ReceiptVault, ());
        let market = ReceiptVaultClient::new(&f.env, &market_id);
        market.initialize(&f.usdc_id, &0u128, &0u128, &f.admin);
        market.set_boosted_vault(&f.admin, &f.vault_id);
        Market { f, market }
    }

    #[test]
    fn market_deposit_flows_through_into_the_lp_position() {
        let m = setup_market();
        let user = Address::generate(&m.f.env);
        let amount = 10_000_0000000i128;
        m.f.usdc.mint(&user, &amount);

        m.market.deposit(&user, &(amount as u128));

        // The market should now be holding vault shares, and the vault should
        // be holding an LP position rather than idle cash.
        let shares = m.f.vault.balance(&m.market.address);
        assert!(shares > 0, "market did not receive vault shares");
        assert!(
            m.f.vault.get_position_liquidity() > 0,
            "deposit never reached the pool"
        );
    }

    /// The end-to-end path spans ReceiptVault -> this vault -> the Aquarius
    /// pool -> three token contracts -> the oracle, and must still fit inside
    /// Soroban's 100-entry transaction footprint. It did not, at first: a
    /// key-per-config-field storage layout put the deposit at 113 entries, and
    /// reading Reflector inside the withdraw path blew the limit again. Both
    /// are cheap to reintroduce by accident, so both are pinned here.
    #[test]
    fn end_to_end_paths_fit_in_a_transaction() {
        let m = setup_market();
        let user = Address::generate(&m.f.env);
        m.f.usdc.mint(&user, &10_000_0000000i128);

        m.market.deposit(&user, &10_000_0000000u128);
        let r = m.f.env.cost_estimate().resources();
        let deposit_entries = r.memory_read_entries + r.write_entries;
        assert!(
            deposit_entries < 100,
            "market deposit needs {} ledger entries, over the cap",
            deposit_entries
        );
        assert!(r.instructions < 100_000_000, "market deposit CPU too high");

        let ptokens = m.market.balance(&user) as u128;
        m.market.withdraw(&user, &(ptokens / 2));
        let r = m.f.env.cost_estimate().resources();
        let withdraw_entries = r.memory_read_entries + r.write_entries;
        assert!(
            withdraw_entries < 100,
            "market withdraw needs {} ledger entries, over the cap",
            withdraw_entries
        );
        assert!(r.instructions < 100_000_000, "market withdraw CPU too high");
    }

    #[test]
    fn market_withdraw_pulls_back_through_the_boosted_vault() {
        let m = setup_market();
        let user = Address::generate(&m.f.env);
        let amount = 10_000_0000000i128;
        m.f.usdc.mint(&user, &amount);
        m.market.deposit(&user, &(amount as u128));
        assert_eq!(m.f.usdc.balance(&user), 0);

        // Nothing is left as idle cash in the market, so this withdrawal has
        // to be satisfied by redeeming from the LP position.
        let ptokens = m.market.balance(&user);
        assert!(ptokens > 0);
        m.market.withdraw(&user, &((ptokens as u128) / 2));

        let back = m.f.usdc.balance(&user);
        assert!(back > 0, "withdraw returned nothing");
        // Half the position, minus the round-trip swap cost.
        assert!(
            back > amount * 48 / 100,
            "withdraw returned too little: {} of {}",
            back,
            amount
        );
        assert!(
            back <= amount / 2 + 1,
            "withdraw returned too much: {}",
            back
        );
    }

    #[test]
    fn market_exchange_rate_reflects_harvested_yield() {
        let m = setup_market();
        let user = Address::generate(&m.f.env);
        let amount = 10_000_0000000i128;
        m.f.usdc.mint(&user, &amount);
        m.market.deposit(&user, &(amount as u128));

        let rate_before = m.market.get_exchange_rate();

        // Swap fees accrue to the vault's position and get folded back in.
        m.f.pool
            .credit_fees(&m.f.vault_id, &50_0000000u128, &50_0000000u128);
        m.f.usdc.mint(&m.f.pool_id, &50_0000000i128);
        m.f.eurc.mint(&m.f.pool_id, &50_0000000i128);

        let keeper = Address::generate(&m.f.env);
        m.f.vault.harvest(&keeper);
        m.market.refresh_boosted_underlying();

        assert!(
            m.market.get_exchange_rate() > rate_before,
            "harvested yield did not reach the market's exchange rate"
        );
    }

    #[test]
    fn idle_cash_buffer_keeps_part_of_the_market_liquid() {
        let m = setup_market();
        // Keep 30% of deposits as idle cash in the market itself.
        m.market.set_idle_cash_buffer_bps(&m.f.admin, &3_000u32);

        let user = Address::generate(&m.f.env);
        let amount = 10_000_0000000i128;
        m.f.usdc.mint(&user, &amount);
        m.market.deposit(&user, &(amount as u128));

        let idle = m.f.usdc.balance(&m.market.address);
        assert!(
            idle >= amount * 29 / 100,
            "buffer not honoured: {} of {}",
            idle,
            amount
        );
        assert!(
            m.f.vault.get_position_liquidity() > 0,
            "the rest should still have been deployed"
        );
    }
}

/// Soroban caps a transaction at 100 footprint ledger entries. The standalone
/// vault paths must leave plenty of headroom, because the market wraps them in
/// several more contracts' worth of state before they reach that cap.
#[test]
fn standalone_paths_stay_well_inside_the_entry_budget() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let market = Address::generate(&f.env);
    f.usdc.mint(&market, &10_000_0000000i128);
    let shares = f
        .vault
        .deposit_underlying(&market, &10_000_0000000i128, &0i128);

    let r = f.env.cost_estimate().resources();
    let deposit_entries = r.memory_read_entries + r.write_entries;
    assert!(deposit_entries <= 50, "vault deposit footprint regressed");

    let mut min_out: Vec<i128> = Vec::new(&f.env);
    min_out.push_back(0i128);
    f.vault.withdraw(&shares, &min_out, &market);
    let r = f.env.cost_estimate().resources();
    let withdraw_entries = r.memory_read_entries + r.write_entries;
    assert!(withdraw_entries <= 50, "vault withdraw footprint regressed");
}
