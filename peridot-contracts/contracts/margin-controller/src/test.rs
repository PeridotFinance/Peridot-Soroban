use super::*;
use mock_token::{MockToken, MockTokenClient};
use receipt_vault::ReceiptVault;
use soroban_sdk::testutils::{Address as _, BytesN as _};
use soroban_sdk::{contract, contractimpl, contracttype, BytesN, Env, IntoVal, Symbol};
use simple_peridottroller::SimplePeridottroller;
use soroban_sdk::testutils::Ledger;

#[contract]
struct MockOracle;

#[contracttype]
enum OracleKey {
    Decimals,
    Price(Address),
}

#[contracttype]
#[derive(Clone)]
struct OraclePrice {
    price: i128,
}

#[contracttype]
enum Asset {
    Stellar(Address),
    Other(Symbol),
}

#[contracttype]
struct PriceData {
    price: i128,
    timestamp: u64,
}

#[contractimpl]
impl MockOracle {
    pub fn initialize(env: Env, decimals: u32) {
        env.storage()
            .persistent()
            .set(&OracleKey::Decimals, &decimals);
    }
    pub fn set_price(env: Env, asset: Address, price: i128) {
        env.storage()
            .persistent()
            .set(&OracleKey::Price(asset), &OraclePrice { price });
    }
    pub fn decimals(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&OracleKey::Decimals)
            .unwrap_or(6u32)
    }
    pub fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        match asset {
            Asset::Stellar(addr) => {
                let rec: Option<OraclePrice> =
                    env.storage().persistent().get(&OracleKey::Price(addr));
                rec.map(|r| PriceData {
                    price: r.price,
                    timestamp: env.ledger().timestamp(),
                })
            }
            _ => None,
        }
    }
    pub fn resolution(_env: Env) -> u32 {
        60
    }
}

#[contract]
struct MockSwapAdapter;

#[contractimpl]
impl MockSwapAdapter {
    pub fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        _token_in: Address,
        in_amount: u128,
        _out_min: u128,
    ) -> u128 {
        let last = swaps_chain.get(swaps_chain.len() - 1).unwrap();
        let token_out = last.2.clone();
        MockTokenClient::new(&env, &token_out).mint(&user, &(in_amount as i128));
        in_amount
    }
}
fn setup() -> (Env, Address, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let lender = Address::generate(&env);

    let usdt_id = env.register(MockToken, ());
    let xlm_id = env.register(MockToken, ());
    let usdt = MockTokenClient::new(&env, &usdt_id);
    let xlm = MockTokenClient::new(&env, &xlm_id);
    usdt.initialize(&"USDT".into_val(&env), &"USDT".into_val(&env), &6u32);
    xlm.initialize(&"XLM".into_val(&env), &"XLM".into_val(&env), &6u32);

    let usdt_vault_id = env.register(ReceiptVault, ());
    let xlm_vault_id = env.register(ReceiptVault, ());
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    usdt_vault.initialize(&usdt_id, &0u128, &0u128, &admin);
    xlm_vault.initialize(&xlm_id, &0u128, &0u128, &admin);

    let oracle_id = env.register(MockOracle, ());
    let oracle = MockOracleClient::new(&env, &oracle_id);
    oracle.initialize(&6u32);
    oracle.set_price(&usdt_id, &1_000_000i128);
    oracle.set_price(&xlm_id, &1_000_000i128);

    let peridottroller_id = env.register(SimplePeridottroller, ());
    let comp = simple_peridottroller::SimplePeridottrollerClient::new(&env, &peridottroller_id);
    comp.initialize(&admin);
    comp.set_oracle(&oracle_id);
    comp.add_market(&usdt_vault_id);
    comp.add_market(&xlm_vault_id);
    comp.set_market_cf(&usdt_vault_id, &1_000_000u128);
    comp.set_market_cf(&xlm_vault_id, &1_000_000u128);
    comp.cache_price(&usdt_id);
    comp.cache_price(&xlm_id);
    usdt_vault.set_peridottroller(&peridottroller_id);
    xlm_vault.set_peridottroller(&peridottroller_id);

    // Liquidity
    usdt.mint(&user, &1_000_000i128);
    usdt.mint(&lender, &1_000_000i128);
    xlm.mint(&lender, &1_000_000i128);
    usdt_vault.deposit(&lender, &500_000u128);
    xlm_vault.deposit(&lender, &500_000u128);

    let swap_adapter_id = env.register(MockSwapAdapter, ());

    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128, &50_000u128);
    controller.set_market(&admin, &usdt_id, &usdt_vault_id);
    controller.set_market(&admin, &xlm_id, &xlm_vault_id);

    (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        lender,
        usdt_vault_id,
        xlm_vault_id,
    )
}

#[test]
fn open_and_close_long() {
    let (env, controller_id, usdt_id, xlm_id, user, _lender, _usdt_vault_id, _xlm_vault_id) =
        setup();
    let controller = MarginControllerClient::new(&env, &controller_id);

    // simple 1-hop swaps
    let hop = (
        Vec::from_array(&env, [usdt_id.clone(), xlm_id.clone()]),
        BytesN::random(&env),
        xlm_id.clone(),
    );
    let swaps = Vec::from_array(&env, [hop]);

    let position_id = controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps,
        &0u128,
    );
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Open);

    // close using reverse hop
    let hop_close = (
        Vec::from_array(&env, [xlm_id.clone(), usdt_id.clone()]),
        BytesN::random(&env),
        usdt_id.clone(),
    );
    let swaps_close = Vec::from_array(&env, [hop_close]);
    controller.close_position(&user, &position_id, &swaps_close, &0u128);

    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Closed);
}
