#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, testutils::Ledger as _, Address,
    Env, String, Symbol, Vec,
};

use mock_token::{MockToken, MockTokenClient};
use receipt_vault::{ReceiptVault, ReceiptVaultClient};

use crate::math::{isqrt, mul_div, mul_div_ceil, try_mul_div};
use crate::oracle::{Asset, PriceData};
use crate::storage::DataKey;
use crate::{AquariusLpVault, AquariusLpVaultClient};

// The admin the contract pins at build time (test builds fall back to this).
const ADMIN_G: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";

// ─────────────────────────────────────────────────────────────────────────────
// Mock Reflector oracle
// ─────────────────────────────────────────────────────────────────────────────

#[contracttype]
enum OracleKey {
    Price(Address),
    /// Reflector testnet publishes most assets as `Other(Symbol)` rather than
    /// `Stellar(Address)`, and the vault's `set_oracle_symbol` exists for
    /// exactly that. The mock has to serve both or it cannot exercise the
    /// symbol-override path at all.
    SymbolPrice(Symbol),
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

    pub fn set_symbol_price(env: Env, symbol: Symbol, price: i128) {
        env.storage()
            .persistent()
            .set(&OracleKey::SymbolPrice(symbol), &price);
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
        let price: i128 = match asset {
            Asset::Stellar(a) => env
                .storage()
                .persistent()
                .get(&OracleKey::Price(a))
                .unwrap_or(0),
            Asset::Other(sym) => env
                .storage()
                .persistent()
                .get(&OracleKey::SymbolPrice(sym))
                .unwrap_or(0),
        };
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

    pub fn decimals(_env: Env) -> u32 {
        14
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
    receipt_market_id: Address,
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

fn setup_with_binding(bind_receipt_market: bool) -> Fixture {
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
    // The primary AQUA claim is deliberately distinct from the optional gauge
    // token. That keeps harvest tests honest: AQUA must be discovered from the
    // configured primary reward, not accidentally through `gauges_claim()`.
    pool.set_reward_tokens(&aqua_id, &eurc_id);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.set_price(&usdc_id, &PRICE_USDC);
    oracle.set_price(&eurc_id, &PRICE_EURC);
    oracle.set_price(&aqua_id, &PRICE_AQUA);
    // Same prices under their symbol encodings, mirroring Reflector testnet.
    oracle.set_symbol_price(&Symbol::new(&env, "USDC"), &PRICE_USDC);
    oracle.set_symbol_price(&Symbol::new(&env, "EURC"), &PRICE_EURC);

    let vault_id = env.register(AquariusLpVault, ());
    let vault = AquariusLpVaultClient::new(&env, &vault_id);
    vault.initialize(&admin, &pool_id, &underlying_index, &oracle_id);

    let receipt_market_id = env.register(ReceiptVault, ());
    let receipt_market = ReceiptVaultClient::new(&env, &receipt_market_id);
    receipt_market.initialize(&usdc_id, &0u128, &0u128, &admin);
    receipt_market.set_boosted_vault(&admin, &vault_id);
    if bind_receipt_market {
        vault.set_receipt_vault(&admin, &receipt_market_id);
    }
    vault.set_primary_reward_token(&admin, &Some(aqua_id.clone()));

    Fixture {
        env,
        admin,
        vault,
        vault_id,
        receipt_market_id,
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

fn setup() -> Fixture {
    setup_with_binding(true)
}

/// Adds capital through the sole bound ReceiptVault identity. Some standalone
/// accounting tests need distinct holders to exercise pro-rata math that was
/// supported by older builds, so the native-only fixture moves their entries
/// directly. Production code cannot create or transfer those holders.
fn deposit_for(f: &Fixture, holder: &Address, amount: i128) -> i128 {
    f.usdc.mint(&f.receipt_market_id, &amount);
    let shares = f
        .vault
        .deposit_underlying(&f.receipt_market_id, &amount, &0i128);
    if *holder != f.receipt_market_id {
        f.env.as_contract(&f.vault_id, || {
            let from_key = DataKey::Shares(f.receipt_market_id.clone());
            let to_key = DataKey::Shares(holder.clone());
            let from_balance: u128 = f.env.storage().persistent().get(&from_key).unwrap_or(0);
            assert!(from_balance >= shares as u128);
            let next_from = from_balance - shares as u128;
            if next_from == 0 {
                f.env.storage().persistent().remove(&from_key);
            } else {
                f.env.storage().persistent().set(&from_key, &next_from);
            }
            let to_balance: u128 = f.env.storage().persistent().get(&to_key).unwrap_or(0);
            f.env
                .storage()
                .persistent()
                .set(&to_key, &to_balance.saturating_add(shares as u128));
        });
    }
    shares
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
    let env = Env::default();
    let huge = u128::MAX / 2;
    // Would overflow if computed as `a * b` first.
    assert_eq!(mul_div(&env, huge, 4, 4), huge);
    assert_eq!(mul_div(&env, 10, 3, 4), 7);
    assert_eq!(mul_div_ceil(&env, 10, 3, 4), 8);
    assert_eq!(mul_div_ceil(&env, 8, 2, 4), 4);
    assert_eq!(mul_div(&env, 0, 5, 3), 0);
    assert_eq!(
        try_mul_div(&env, u128::MAX - 1, u128::MAX - 1, u128::MAX),
        Some(u128::MAX - 2),
        "a representable quotient must not fail because its product exceeds u128"
    );
    assert_eq!(
        mul_div_ceil(&env, u128::MAX - 1, u128::MAX - 1, u128::MAX),
        u128::MAX - 1
    );
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
    assert_eq!(
        f.vault.get_receipt_vault(),
        Some(f.receipt_market_id.clone())
    );
    assert_eq!(f.vault.get_primary_reward_token(), Some(f.aqua_id.clone()));
}

#[test]
fn unbound_vault_rejects_deposits() {
    let f = setup_with_binding(false);
    f.usdc.mint(&f.receipt_market_id, &1_000_0000000i128);
    assert!(f
        .vault
        .try_deposit_underlying(&f.receipt_market_id, &1_000_0000000i128, &0i128)
        .is_err());
}

#[test]
fn direct_user_cannot_bypass_the_receipt_vault() {
    let f = setup();
    let user = Address::generate(&f.env);
    f.usdc.mint(&user, &1_000_0000000i128);
    assert!(f
        .vault
        .try_deposit_underlying(&user, &1_000_0000000i128, &0i128)
        .is_err());
}

#[test]
fn receipt_vault_binding_is_one_time() {
    let f = setup_with_binding(false);
    f.vault.set_receipt_vault(&f.admin, &f.receipt_market_id);
    assert!(f
        .vault
        .try_set_receipt_vault(&f.admin, &f.receipt_market_id)
        .is_err());
}

#[test]
fn primary_reward_token_can_be_rotated() {
    let f = setup();
    f.vault
        .set_primary_reward_token(&f.admin, &Some(f.eurc_id.clone()));
    assert_eq!(f.vault.get_primary_reward_token(), Some(f.eurc_id.clone()));
    f.vault.set_primary_reward_token(&f.admin, &None);
    assert_eq!(f.vault.get_primary_reward_token(), None);
}

#[test]
#[should_panic(expected = "already initialized")]
fn initialize_is_not_repeatable() {
    let f = setup();
    let oracle_id = env_oracle(&f);
    f.vault.initialize(&f.admin, &f.pool_id, &0u32, &oracle_id);
}

#[test]
fn initialize_rejects_token_decimals_that_overflow_nav_scaling() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::from_string(&String::from_str(&env, ADMIN_G));

    let (token0, _) = deploy_token(&env, "SAFE");
    let token1 = env.register(MockToken, ());
    MockTokenClient::new(&env, &token1).initialize(
        &String::from_str(&env, "UNSAFE"),
        &String::from_str(&env, "UNSAFE"),
        &39u32,
    );

    let pool_id = env.register(mock_aquarius_pool::MockAquariusPool, ());
    mock_aquarius_pool::MockAquariusPoolClient::new(&env, &pool_id)
        .initialize(&token0, &token1, &60i32, &30u32);
    let oracle_id = env.register(MockOracle, ());
    let vault_id = env.register(AquariusLpVault, ());
    let vault = AquariusLpVaultClient::new(&env, &vault_id);

    assert!(
        vault
            .try_initialize(&admin, &pool_id, &0u32, &oracle_id)
            .is_err(),
        "unsafe token decimals should be rejected during initialization"
    );
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

    let amount = 2_000_0000000i128; // 2000 USDC
    deposit_for(&f, &f.receipt_market_id, amount);

    let liq = f.vault.get_position_liquidity();
    assert!(liq > 0, "expected liquidity to be minted");

    // 2 * L * sqrt(1.167) with 1e9-scaled root arithmetic.
    let ratio = mul_div(
        &f.env,
        PRICE_EURC as u128,
        1_000_000_000_000_000_000u128,
        PRICE_USDC as u128,
    );
    let expected = mul_div(&f.env, liq, isqrt(ratio), 1_000_000_000u128) * 2;

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

    deposit_for(&f, &f.receipt_market_id, 2_000_0000000i128);

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
    deposit_for(&f, &f.receipt_market_id, 2_000_0000000i128);

    let nav_before = f.vault.get_total_underlying();
    f.oracle.set_fail(&true);
    assert_eq!(f.vault.get_total_underlying(), nav_before);

    // Staleness takes the same path.
    f.oracle.set_fail(&false);
    f.oracle.set_stale(&true);
    assert_eq!(f.vault.get_total_underlying(), nav_before);
}

#[test]
fn nav_quotes_fail_soft_after_the_stale_bound() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let shares = deposit_for(&f, &f.receipt_market_id, 2_000_0000000i128);
    assert!(f.vault.get_total_underlying() > 0);

    // Move beyond both the 5-minute cache window and the configured stale
    // bound, then make the live source unreachable. Quote paths must return a
    // conservative zero instead of propagating a panic into ReceiptVault.
    f.vault.set_nav_root_max_stale(&f.admin, &301u64);
    f.oracle.set_fail(&true);
    f.env.ledger().set_timestamp(1_700_000_302);

    assert_eq!(f.vault.refresh_nav_root(), 0);
    let idle = f.usdc.balance(&f.vault_id);
    assert_eq!(f.vault.get_total_underlying(), idle);
    assert!(
        f.vault
            .get_asset_amounts_per_shares(&shares)
            .get(0)
            .unwrap()
            <= idle,
        "stale LP value leaked into the fail-soft quote"
    );
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

    let shares = deposit_for(&f, &f.receipt_market_id, 1_000_0000000i128);

    let amounts = f.vault.get_asset_amounts_per_shares(&shares);
    assert_eq!(amounts.len(), 1);
    assert!(amounts.get(0).unwrap() > 0);
}

#[test]
fn deposit_then_withdraw_returns_underlying_only() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let market = f.receipt_market_id.clone();
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
    let market = f.receipt_market_id.clone();
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
    let shares = deposit_for(&f, &market, 1_000_0000000i128);

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
    let shares_a = deposit_for(&f, &a, 1_000_0000000i128);
    let shares_b = deposit_for(&f, &b, 1_000_0000000i128);

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
    let shares_a = deposit_for(&f, &a, 1_000_0000000i128);

    // Simulate fees accruing to the vault's idle balance.
    f.usdc.mint(&f.vault_id, &100_0000000i128);

    let b = Address::generate(&f.env);
    let shares_b = deposit_for(&f, &b, 1_000_0000000i128);

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

/// The last holder out must take everything with them. Anything retained
/// would sit behind a zero share supply and be captured by whoever deposits
/// next.
#[test]
fn a_full_exit_leaves_nothing_behind() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    deposit_for(&f, &a, 1_000_0000000i128);
    deposit_for(&f, &b, 1_000_0000000i128);

    let mut min_out: Vec<i128> = Vec::new(&f.env);
    min_out.push_back(0i128);
    f.vault.withdraw(&f.vault.balance(&a), &min_out, &a);
    f.vault.withdraw(&f.vault.balance(&b), &min_out, &b);

    assert_eq!(f.vault.total_supply(), 0, "all shares should be burned");
    assert_eq!(
        f.vault.get_idle(),
        0,
        "no underlying may be stranded behind a zero share supply"
    );
    assert_eq!(f.vault.get_position_liquidity(), 0);
}

/// Swapping the oracle must not leave the previous feed's valuation live.
/// The cache window is long enough to borrow against a price the replacement
/// oracle would reject.
#[test]
fn changing_the_oracle_invalidates_the_cached_ratio() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let user = Address::generate(&f.env);
    deposit_for(&f, &user, 1_000_0000000i128);

    let nav_before = f.vault.get_total_underlying();
    assert!(nav_before > 0);

    // A replacement oracle that prices the pair very differently.
    let new_oracle = f.env.register(MockOracle, ());
    let new_client = MockOracleClient::new(&f.env, &new_oracle);
    new_client.set_price(&f.usdc_id, &PRICE_USDC);
    new_client.set_price(&f.eurc_id, &(PRICE_EURC * 4));
    f.vault.set_oracle(&f.admin, &new_oracle);

    // Must reflect the new feed immediately, not serve the old cached ratio
    // for the remainder of the cache window.
    assert_ne!(
        f.vault.get_total_underlying(),
        nav_before,
        "stale ratio from the previous oracle survived the swap"
    );
}

#[test]
fn extreme_oracle_price_falls_back_to_the_last_good_root() {
    let f = setup();
    let cached = f.vault.refresh_nav_root();
    assert!(cached > 0);

    // A compromised feed can return any positive i128. Its ratio cannot be
    // represented in the vault's u128 fixed-point scale, so it must behave as
    // an unavailable observation rather than trapping supplier paths.
    f.oracle.set_price(&f.eurc_id, &i128::MAX);
    assert_eq!(f.vault.refresh_nav_root(), cached);
}

/// Correcting a token's Reflector encoding changes which price is fetched, so
/// the cached ratio is as invalid as it is after swapping the oracle itself.
#[test]
fn changing_an_oracle_symbol_invalidates_the_cached_ratio() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let user = Address::generate(&f.env);
    deposit_for(&f, &user, 1_000_0000000i128);

    assert!(
        f.vault.get_last_nav_root_at() > 0,
        "fixture should start with a cached ratio"
    );

    // Correcting the encoding must drop the cache immediately, without
    // waiting for a keeper or the cache window to lapse.
    f.vault
        .set_oracle_symbol(&f.admin, &f.eurc_id, &Some(Symbol::new(&f.env, "EURC")));
    assert_eq!(
        f.vault.get_last_nav_root_at(),
        0,
        "cached ratio survived an oracle-symbol change"
    );

    // Clearing the override drops it too.
    f.vault.refresh_nav_root();
    assert!(f.vault.get_last_nav_root_at() > 0);
    f.vault.set_oracle_symbol(&f.admin, &f.eurc_id, &None);
    assert_eq!(f.vault.get_last_nav_root_at(), 0);
}

/// A partial exit must be valued against the same assets a deposit is, or the
/// exiting holder forfeits their share of idle paired-token residue to
/// whoever stays.
#[test]
fn a_partial_exit_is_valued_against_the_same_assets_as_a_deposit() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let a = Address::generate(&f.env);
    deposit_for(&f, &a, 2_000_0000000i128);

    // Donate paired-token residue directly, as a deposit swap would leave.
    f.eurc.mint(&f.vault_id, &50_0000000i128);
    let residue_value = f.vault.get_other_idle_value() as u128;
    assert!(residue_value > 0, "fixture should have paired residue");

    let shares = f.vault.balance(&a);
    let full_nav = f.vault.get_total_underlying() as u128 + residue_value;

    let mut min_out: Vec<i128> = Vec::new(&f.env);
    min_out.push_back(0i128);
    let out = f.vault.withdraw(&(shares / 2), &min_out, &a);
    let received = out.get(0).unwrap() as u128;

    // Half the shares must be worth about half of the *full* asset value,
    // not half of a NAV that pretends the residue is not there.
    let half_full = full_nav / 2;
    assert!(
        received * 100 > half_full * 90,
        "partial exit shortchanged: got {} against a fair {}",
        received,
        half_full
    );
}

/// Aquarius can pause its own pool at any time (errors 205/206) and nothing on
/// our side can prevent that. What must not happen is that pause propagating
/// into the market above: `receipt-vault` invokes our `deposit` directly, so a
/// revert here would stop users depositing into the Peridot market entirely.
#[test]
fn a_paused_pool_does_not_block_market_deposits() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    // Aquarius pauses deposits.
    f.pool.set_kill_deposit(&true);

    // Must still succeed — the cash simply stays idle instead of reaching the pool.
    let shares = deposit_for(&f, &f.receipt_market_id, 1_000_0000000i128);
    assert!(shares > 0, "a paused pool must not block deposits");
    assert_eq!(
        f.vault.get_position_liquidity(),
        0,
        "nothing should have reached the paused pool"
    );
    assert_eq!(
        f.vault.get_idle() as u128,
        1_000_0000000u128,
        "the full deposit should be sitting idle"
    );

    // And it deploys once Aquarius reopens.
    f.pool.set_kill_deposit(&false);
    f.vault.deploy();
    assert!(
        f.vault.get_position_liquidity() > 0,
        "should deploy after unpause"
    );
}

/// Principal must remain recoverable while swaps are paused. Aquarius
/// guarantees `withdraw_position` has no kill switch, so the LP legs always
/// come back; only converting the paired leg can be blocked.
#[test]
fn a_paused_swap_still_lets_principal_out() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let user = Address::generate(&f.env);
    let shares = deposit_for(&f, &user, 1_000_0000000i128);

    f.pool.set_kill_swap(&true);

    let mut min_out: Vec<i128> = Vec::new(&f.env);
    min_out.push_back(0i128);
    // Must not panic. The underlying leg of the position comes back; the paired
    // leg cannot be converted and stays for a later harvest.
    let out = f.vault.withdraw(&shares, &min_out, &user);
    assert!(
        out.get(0).unwrap() > 0,
        "the underlying leg should still be recoverable with swaps paused"
    );
}

#[test]
fn strategy_shares_are_non_transferable() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let shares = deposit_for(&f, &a, 1_000_0000000i128);

    let supply_before = f.vault.total_supply();
    assert!(f.vault.try_transfer(&a, &b, &shares).is_err());
    assert_eq!(f.vault.balance(&a), shares);
    assert_eq!(f.vault.balance(&b), 0);
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

    deposit_for(&f, &f.receipt_market_id, 5_000_0000000i128);

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
fn deploy_rejects_a_pool_quote_above_the_offered_token_amounts() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    f.vault.set_max_deploy(&f.admin, &100_0000000u128);

    // Inflate only the underlying leg. Because max_deploy is much smaller
    // than the deposit, the vault has enough idle underlying for a malicious
    // pool to pull the excess unless the authorization is explicitly bounded.
    if f.usdc_id < f.eurc_id {
        f.pool.set_deposit_quote_extra(&100_0000000u128, &0u128);
    } else {
        f.pool.set_deposit_quote_extra(&0u128, &100_0000000u128);
    }
    deposit_for(&f, &f.receipt_market_id, 1_000_0000000i128);

    assert_eq!(
        f.vault.get_position_liquidity(),
        0,
        "vault accepted a deposit quote above its desired amounts"
    );
}

#[test]
fn replayed_swap_transfer_reverts_without_losing_vault_assets() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let market = f.receipt_market_id.clone();
    let amount = 1_000_0000000i128;
    f.usdc.mint(&market, &amount);
    let market_before = f.usdc.balance(&market);
    let reserves_before = f.pool.get_reserves();
    f.pool.set_replay_swap_transfer(&true);

    assert!(
        f.vault
            .try_deposit_underlying(&market, &amount, &0i128)
            .is_err(),
        "a replayed root transfer must fail the enclosing invocation"
    );
    assert_eq!(f.usdc.balance(&market), market_before);
    assert_eq!(f.usdc.balance(&f.vault_id), 0);
    assert_eq!(f.pool.get_reserves(), reserves_before);
    assert_eq!(f.vault.get_position_liquidity(), 0);
}

#[test]
fn underdelivered_swap_reverts_without_losing_vault_assets() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let market = f.receipt_market_id.clone();
    let amount = 1_000_0000000i128;
    f.usdc.mint(&market, &amount);
    let market_before = f.usdc.balance(&market);
    let reserves_before = f.pool.get_reserves();
    f.pool.set_swap_output_bps(&5_000u32);

    assert!(
        f.vault
            .try_deposit_underlying(&market, &amount, &0i128)
            .is_err(),
        "an underdelivering pool must fail the enclosing invocation"
    );
    assert_eq!(f.usdc.balance(&market), market_before);
    assert_eq!(f.usdc.balance(&f.vault_id), 0);
    assert_eq!(f.pool.get_reserves(), reserves_before);
    assert_eq!(f.vault.get_position_liquidity(), 0);
}

#[test]
fn pausing_blocks_deposits_but_never_withdrawals() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

    let market = f.receipt_market_id.clone();
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

    let market = f.receipt_market_id.clone();
    let shares = deposit_for(&f, &market, 1_000_0000000i128);

    f.pool.set_fail_withdraw(&true);

    // Nothing can be raised, so the call must NOT quietly burn the holder's
    // claim — the shares have to survive so they can retry once the pool
    // recovers. (An earlier version paid zero and burned everything.)
    let mut min_out: Vec<i128> = Vec::new(&f.env);
    min_out.push_back(0i128);
    assert!(
        f.vault.try_withdraw(&shares, &min_out, &market).is_err(),
        "withdraw should refuse rather than burn shares for nothing"
    );
    assert_eq!(
        f.vault.balance(&market),
        shares,
        "holder's claim must be preserved through a pool outage"
    );

    // Once the pool recovers the same shares redeem normally.
    f.pool.set_fail_withdraw(&false);
    let out = f.vault.withdraw(&shares, &min_out, &market);
    assert!(out.get(0).unwrap() > 0);
    assert_eq!(f.vault.balance(&market), 0);
}

/// A pool priced far from the oracle must be refused, not silently entered.
/// The per-swap slippage floor cannot catch this: it is derived from the
/// pool's own quote, so a mispriced pool quotes its own bad price confidently.
#[test]
fn deposit_is_refused_when_the_pool_price_diverges_from_the_oracle() {
    let f = setup();
    // Pool seeded at ~1.167 EURC per USDC, matching the oracle.
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    f.usdc.mint(&f.receipt_market_id, &1_500_0000000i128);
    // Sanity: at parity the deposit goes through.
    f.vault
        .deposit_underlying(&f.receipt_market_id, &500_0000000i128, &0i128);

    // Halve the oracle's EURC price: fair value now says the deposit's swap
    // should buy roughly twice as much EURC as the pool is offering, so the
    // pool is overpricing EURC and entering it would realise that loss.
    f.oracle.set_price(&f.eurc_id, &(PRICE_EURC / 2));
    f.vault.refresh_nav_root();

    assert!(
        f.vault
            .try_deposit_underlying(&f.receipt_market_id, &500_0000000i128, &0i128)
            .is_err(),
        "deposit should be refused while the pool is dislocated"
    );

    // Widening the tolerance lets it through again — the guard is a policy,
    // not a hard stop.
    f.vault.set_max_pool_divergence_bps(&f.admin, &9_000u32);
    f.vault
        .deposit_underlying(&f.receipt_market_id, &500_0000000i128, &0i128);
}

/// The exit swap must not trust a manipulated pool's own quote. Reverting the
/// transaction preserves the holder's shares and the LP position for a later
/// retry instead of realizing the bad rate.
#[test]
fn withdrawal_is_refused_when_the_pool_price_diverges_from_the_oracle() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let user = Address::generate(&f.env);
    let shares = deposit_for(&f, &user, 1_000_0000000i128);
    let liquidity = f.vault.get_position_liquidity();

    // Make the paired token four times as valuable according to the oracle.
    // The unchanged pool then offers materially too little underlying for the
    // paired leg released during withdrawal.
    f.oracle.set_price(&f.eurc_id, &(PRICE_EURC * 4));
    f.vault.refresh_nav_root();

    let mut min_out: Vec<i128> = Vec::new(&f.env);
    min_out.push_back(0i128);
    assert!(
        f.vault.try_withdraw(&shares, &min_out, &user).is_err(),
        "withdrawal should reject a pool quote below the oracle floor"
    );
    assert_eq!(f.vault.balance(&user), shares, "shares were not preserved");
    assert_eq!(
        f.vault.get_position_liquidity(),
        liquidity,
        "LP position changed despite the reverted withdrawal"
    );
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

    deposit_for(&f, &f.receipt_market_id, 1_000_0000000i128);

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

    // A route alone is deliberately insufficient: permissionless callers may
    // not sell until governance supplies an independent minimum rate.
    let guarded_dust = 1_0000000i128;
    f.aqua.mint(&f.vault_id, &guarded_dust);
    let caller = Address::generate(&f.env);
    assert_eq!(f.vault.sweep_reward(&caller, &f.aqua_id), 0);
    assert_eq!(f.aqua.balance(&f.vault_id), guarded_dust);

    let probe = 1_000_0000000u128;
    let (in_idx, out_idx) = if f.aqua_id < f.usdc_id {
        (0u32, 1u32)
    } else {
        (1u32, 0u32)
    };
    let quote = route.estimate_swap(&in_idx, &out_idx, &probe);
    let min_rate = mul_div(&f.env, quote, 9_500_000u128, probe);
    f.vault.set_reward_min_rate(&f.admin, &f.aqua_id, &min_rate);

    // Pool owes the vault some AQUA.
    f.aqua.mint(&f.pool_id, &100_000_0000000i128);
    f.pool
        .credit_rewards(&f.vault_id, &100_000_0000000u128, &0u128);

    let nav_before = f.vault.get_total_underlying();
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
fn harvest_keeps_rewards_when_the_route_quote_breaches_governance_floor() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    deposit_for(&f, &f.receipt_market_id, 1_000_0000000i128);

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
    let mut desired: Vec<u128> = Vec::new(&f.env);
    desired.push_back(a0);
    desired.push_back(a1);
    route.deposit_position(&lp, &-887_220i32, &887_220i32, &desired, &0u128);
    f.vault
        .set_reward_route(&f.admin, &f.aqua_id, &Some(route_id.clone()));

    let probe = 1_000_0000000u128;
    let (in_idx, out_idx) = if f.aqua_id < f.usdc_id {
        (0u32, 1u32)
    } else {
        (1u32, 0u32)
    };
    let fair_quote = route.estimate_swap(&in_idx, &out_idx, &probe);
    let min_rate = mul_div(&f.env, fair_quote, 9_500_000u128, probe);
    f.vault.set_reward_min_rate(&f.admin, &f.aqua_id, &min_rate);

    // Flood the route with AQUA so the permissionless caller sees a quote far
    // below the governance-approved rate.
    let attacker = Address::generate(&f.env);
    f.aqua.mint(&attacker, &100_000_000_0000000i128);
    if f.aqua_id < f.usdc_id {
        route.donate(&attacker, &100_000_000_0000000u128, &0u128);
    } else {
        route.donate(&attacker, &0u128, &100_000_000_0000000u128);
    }

    let reward = 100_000_0000000u128;
    f.aqua.mint(&f.pool_id, &(reward as i128));
    f.pool.credit_rewards(&f.vault_id, &reward, &0u128);
    f.vault.harvest(&Address::generate(&f.env));

    assert_eq!(
        f.aqua.balance(&f.vault_id),
        reward as i128,
        "harvest sold rewards below the governance floor"
    );
}

#[test]
fn harvest_is_rate_limited() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let caller = Address::generate(&f.env);

    // Give the call real work. Empty/failed harvests deliberately do not
    // consume the global cooldown, so they cannot be used for griefing.
    f.eurc.mint(&f.vault_id, &1_0000000i128);
    f.vault.harvest(&caller);
    assert!(f.vault.try_harvest(&caller).is_err());

    f.env.ledger().set_timestamp(1_700_000_000 + 3_601);
    f.vault.refresh_nav_root();
    f.vault.harvest(&caller);
}

#[test]
fn empty_harvest_does_not_start_the_cooldown() {
    let f = setup();
    seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);
    let caller = Address::generate(&f.env);

    assert_eq!(f.vault.harvest(&caller), 0);
    assert_eq!(f.vault.get_last_harvest(), 0);
    assert_eq!(f.vault.harvest(&caller), 0);
    assert_eq!(f.vault.get_last_harvest(), 0);
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
    deposit_for(&f, &f.receipt_market_id, 1_000_0000000i128);

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
    use jump_rate_model::{JumpRateModel, JumpRateModelClient};
    use simple_peridottroller::{SimplePeridottroller, SimplePeridottrollerClient};

    struct Market {
        f: Fixture,
        market: ReceiptVaultClient<'static>,
    }

    fn setup_market() -> Market {
        let f = setup();
        seed_pool(&f, 1_000_000_0000000i128, 857_000_0000000i128);

        let market = ReceiptVaultClient::new(&f.env, &f.receipt_market_id);
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
    fn market_withdraw_survives_an_oracle_outage_past_the_stale_bound() {
        let m = setup_market();
        let user = Address::generate(&m.f.env);
        let amount = 10_000_0000000i128;
        m.f.usdc.mint(&user, &amount);
        m.market.deposit(&user, &(amount as u128));
        m.market.refresh_boosted_underlying();

        let ptokens = m.market.balance(&user) as u128;
        m.f.vault.set_nav_root_max_stale(&m.f.admin, &301u64);
        m.f.oracle.set_fail(&true);
        m.f.env.ledger().set_timestamp(1_700_000_302);

        // The strategy quote is now deliberately zero. ReceiptVault sizes the
        // unwind from its cache and supplies the non-zero cash floor that
        // authorizes use of the last ratio for this exit only.
        assert!(
            m.f.vault
                .get_asset_amounts_per_shares(&m.f.vault.balance(&m.market.address))
                .get(0)
                .unwrap()
                <= m.f.usdc.balance(&m.f.vault_id)
        );
        m.market.withdraw(&user, &(ptokens / 2));
        let resources = m.f.env.cost_estimate().resources();
        assert!(
            resources.memory_read_entries + resources.write_entries < 100,
            "stale-oracle market withdraw exceeded the entry cap"
        );
        assert!(
            resources.instructions < 100_000_000,
            "stale-oracle market withdraw exceeded the CPU cap"
        );
        assert!(m.f.usdc.balance(&user) > amount * 48 / 100);
    }

    /// What actually happens on a borrow: the market pays out of idle cash if
    /// it can, and otherwise **unwinds the LP position** — burning liquidity,
    /// taking back both legs, and swapping the paired leg into the underlying.
    /// A borrower never touches the paired asset; they only ever see the
    /// market's own underlying.
    #[test]
    fn borrowing_beyond_idle_cash_unwinds_the_lp_position() {
        let m = setup_market();
        m.market.enable_static_rates(&m.f.admin);
        m.market.set_collateral_factor(&1_000_000u128);

        let lender = Address::generate(&m.f.env);
        let borrower = Address::generate(&m.f.env);
        m.f.usdc.mint(&lender, &10_000_0000000i128);
        m.f.usdc.mint(&borrower, &10_000_0000000i128);
        m.market.deposit(&lender, &10_000_0000000u128);
        m.market.deposit(&borrower, &10_000_0000000u128);

        let liq_before = m.f.vault.get_position_liquidity();
        assert!(liq_before > 0, "deposits should have reached the pool");
        let idle_before = m.f.usdc.balance(&m.market.address) as u128;

        // Borrow more than the market is holding as idle cash, forcing a pull
        // from the LP position.
        let amount = idle_before + 2_000_0000000u128;
        let before = m.f.usdc.balance(&borrower);
        m.market.borrow(&borrower, &amount);
        let received = (m.f.usdc.balance(&borrower) - before) as u128;

        assert_eq!(received, amount, "borrower must receive the full amount");
        assert!(
            m.f.vault.get_position_liquidity() < liq_before,
            "the LP position should have been unwound to fund the borrow"
        );
        // The borrower is paid in underlying only — never the paired asset.
        assert_eq!(m.f.eurc.balance(&borrower), 0);
    }

    /// The boosted-vault socket must quote liquidation value, not gross NAV.
    /// Near parity, withdrawing the gross-NAV share count used to realize a
    /// few basis points less after the exit swap and make this borrow fail with
    /// `borrow liquidity shortfall` despite ample LP assets.
    #[test]
    fn small_borrow_accounts_for_lp_exit_costs() {
        let m = setup_market();
        m.market.enable_static_rates(&m.f.admin);
        m.market.set_collateral_factor(&1_000_000u128);

        let user = Address::generate(&m.f.env);
        m.f.usdc.mint(&user, &3_000_0000000i128);
        m.market.deposit(&user, &3_000_0000000u128);

        let before = m.f.usdc.balance(&user);
        m.market.borrow(&user, &200_0000000u128);
        assert_eq!(m.f.usdc.balance(&user) - before, 200_0000000i128);
        assert_eq!(m.f.eurc.balance(&user), 0);
    }

    /// The borrow path is heavier than deposit or withdraw and has its own
    /// footprint budget. Pinned separately because it is the path a lending
    /// market actually exists for.
    #[test]
    fn borrow_through_the_lp_position_fits_in_a_transaction() {
        let m = setup_market();
        m.market.enable_static_rates(&m.f.admin);
        m.market.set_collateral_factor(&1_000_000u128);

        let user = Address::generate(&m.f.env);
        m.f.usdc.mint(&user, &20_000_0000000i128);
        m.market.deposit(&user, &20_000_0000000u128);

        let idle = m.f.usdc.balance(&m.market.address) as u128;
        m.market.borrow(&user, &(idle + 2_000_0000000u128));

        let r = m.f.env.cost_estimate().resources();
        let entries = r.memory_read_entries + r.write_entries;
        assert!(
            entries < 100,
            "market borrow needs {} ledger entries, over the cap",
            entries
        );
        assert!(r.instructions < 100_000_000, "market borrow CPU too high");
    }

    /// Pin the largest currently executable controller-wired shape: the user
    /// borrows against collateral in this market only. Cross-market positions
    /// add enough footprint to exceed Soroban's transaction limit; see the
    /// measurements in this contract's CLAUDE.md.
    #[test]
    fn controller_wired_single_market_borrow_fits_in_a_transaction() {
        let m = setup_market();
        m.f.env.cost_estimate().disable_resource_limits();

        let model_id = m.f.env.register(JumpRateModel, ());
        let model = JumpRateModelClient::new(&m.f.env, &model_id);
        model.initialize(
            &10_000u128,
            &180_000u128,
            &4_000_000u128,
            &800_000u128,
            &m.f.admin,
        );

        let controller_id = m.f.env.register(SimplePeridottroller, ());
        let controller = SimplePeridottrollerClient::new(&m.f.env, &controller_id);
        controller.initialize(&m.f.admin);
        controller.set_oracle(&m.f.oracle.address);
        controller.add_market(&m.market.address);
        controller.set_market_cf(&m.market.address, &700_000u128);

        m.market.set_interest_model(&model_id);
        m.market.set_peridottroller(&controller_id);

        let borrower = Address::generate(&m.f.env);
        m.f.usdc.mint(&borrower, &20_000_0000000i128);

        controller.enter_market(&borrower, &m.market.address);
        m.market.deposit(&borrower, &20_000_0000000u128);

        let idle = m.f.usdc.balance(&m.market.address) as u128;
        let amount = idle.saturating_add(2_000_0000000u128);
        m.f.env.cost_estimate().budget().reset_unlimited();
        m.market.borrow(&borrower, &amount);

        let r = m.f.env.cost_estimate().resources();
        let entries = r.memory_read_entries + r.write_entries;
        assert!(
            entries < 100,
            "controller-wired single-market borrow needs {} ledger entries, over the cap: {:?}",
            entries,
            r
        );
        assert!(
            r.write_entries <= 50,
            "controller-wired single-market borrow needs too many writes: {:?}",
            r
        );
        assert!(
            r.instructions < 100_000_000,
            "controller-wired single-market borrow CPU too high"
        );
        assert_eq!(
            m.f.eurc.balance(&borrower),
            0,
            "borrower must only receive market underlying"
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
    let market = f.receipt_market_id.clone();
    let shares = deposit_for(&f, &market, 10_000_0000000i128);

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
