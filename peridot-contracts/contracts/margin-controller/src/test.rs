use super::*;
use mock_token::{MockToken, MockTokenClient};
use receipt_vault::ReceiptVault;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, BytesN, Env, IntoVal, Symbol, Vec,
};
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
enum MockPeridottrollerKey {
    LivePrice(Address),
    CachePriceCalls(Address),
    CachePriceShouldPanic,
    MarketCF(Address),
    Liquidity(Address),
    Shortfall(Address),
    LastBorrower,
    LastRepayMarket,
    LastCollateralMarket,
    LastRepayAmount,
    LastLiquidator,
    EnteredMarket(Address, Address),
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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct MarketLiquidityHint {
    ptoken_balance: u128,
    user_borrowed: u128,
    exchange_rate: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct ControllerAccrualHint {
    total_ptokens: Option<u128>,
    total_borrowed: Option<u128>,
    user_ptokens: Option<u128>,
    user_borrowed: Option<u128>,
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
    pub fn is_pool_allowed(_env: Env, _pool: Address) -> bool {
        true
    }

    pub fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        _token_in: Address,
        amount: u128,
        _amount_with_slippage: u128,
    ) -> u128 {
        let last = swaps_chain.get(swaps_chain.len() - 1).unwrap();
        let (path, _, _) = last;
        let token_out = path.get(path.len() - 1).unwrap();
        MockTokenClient::new(&env, &token_out).mint(&user, &(amount as i128));
        amount
    }
}

#[contract]
struct MockPeridottroller;

#[contract]
struct MockVault;

#[contracttype]
enum MockVaultKey {
    PTokenBalance(Address),
    BorrowBalance(Address),
    UnderlyingToken,
}

#[contractimpl]
impl MockPeridottroller {
    pub fn set_price(env: Env, asset: Address, price: u128, _scale: u128) {
        env.storage()
            .persistent()
            .set(&OracleKey::Price(asset), &OraclePrice { price: price as i128 });
    }

    pub fn set_live_price(env: Env, asset: Address, price: u128) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LivePrice(asset), &price);
    }

    pub fn get_price_usd(env: Env, asset: Address) -> Option<(u128, u128)> {
        let rec: Option<OraclePrice> = env.storage().persistent().get(&OracleKey::Price(asset));
        rec.map(|r| (r.price as u128, 1_000_000u128))
    }

    pub fn cache_price(env: Env, asset: Address) -> Option<(u128, u128)> {
        if env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::CachePriceShouldPanic)
            .unwrap_or(false)
        {
            panic!("cache refresh failed");
        }
        let live: Option<u128> = env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::LivePrice(asset.clone()));
        let Some(price) = live else {
            return None;
        };
        env.storage()
            .persistent()
            .set(&OracleKey::Price(asset.clone()), &OraclePrice { price: price as i128 });
        let calls_key = MockPeridottrollerKey::CachePriceCalls(asset.clone());
        let calls: u32 = env.storage().persistent().get(&calls_key).unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&calls_key, &calls.saturating_add(1));
        Some((price, 1_000_000u128))
    }

    pub fn get_cache_price_calls(env: Env, asset: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&MockPeridottrollerKey::CachePriceCalls(asset))
            .unwrap_or(0u32)
    }

    pub fn set_cache_price_should_panic(env: Env, should_panic: bool) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::CachePriceShouldPanic, &should_panic);
    }

    pub fn set_account_liquidity(env: Env, user: Address, liquidity: u128, shortfall: u128) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::Liquidity(user.clone()), &liquidity);
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::Shortfall(user), &shortfall);
    }

    pub fn account_liquidity(env: Env, user: Address) -> (u128, u128) {
        let liquidity: u128 = env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::Liquidity(user.clone()))
            .unwrap_or(0u128);
        let shortfall: u128 = env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::Shortfall(user))
            .unwrap_or(0u128);
        (liquidity, shortfall)
    }

    pub fn enter_market(env: Env, user: Address, market: Address) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::EnteredMarket(user, market), &true);
    }

    pub fn has_entered_market(env: Env, user: Address, market: Address) -> bool {
        env.storage()
            .persistent()
            .get(&MockPeridottrollerKey::EnteredMarket(user, market))
            .unwrap_or(false)
    }

    pub fn is_borrow_paused(_env: Env, _market: Address) -> bool {
        false
    }

    pub fn track_borrow_market(_env: Env, _user: Address, _market: Address) {}

    pub fn is_deposit_paused(_env: Env, _market: Address) -> bool {
        false
    }

    pub fn is_redeem_paused(_env: Env, _market: Address) -> bool {
        false
    }

    pub fn get_market_cf(env: Env, market: Address) -> u128 {
        env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::MarketCF(market))
            .unwrap_or(1_000_000u128)
    }

    pub fn set_market_cf(env: Env, market: Address, cf: u128) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::MarketCF(market), &cf);
    }

    pub fn get_collateral_excl_usd(
        _env: Env,
        _user: Address,
        _market: Address,
    ) -> u128 {
        0u128
    }

    pub fn get_borrows_excl(
        _env: Env,
        _user: Address,
        _market: Address,
    ) -> u128 {
        0u128
    }

    pub fn hypothetical_liquidity_with_hint(
        _env: Env,
        _user: Address,
        _market: Address,
        _borrow_amount: u128,
        _underlying: Address,
        _hint: Option<MarketLiquidityHint>,
    ) -> (u128, u128) {
        (u128::MAX, 0u128)
    }

    pub fn accrue_user_market(
        _env: Env,
        _user: Address,
        _market: Address,
        _hint: Option<ControllerAccrualHint>,
    ) {
    }

    pub fn liquidate(
        env: Env,
        borrower: Address,
        repay_market: Address,
        collateral_market: Address,
        repay_amount: u128,
        liquidator: Address,
    ) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LastBorrower, &borrower);
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LastRepayMarket, &repay_market);
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LastCollateralMarket, &collateral_market);
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LastRepayAmount, &repay_amount);
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LastLiquidator, &liquidator);
    }

    pub fn get_last_liquidation(env: Env) -> (Address, Address, Address, u128, Address) {
        let borrower: Address = env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::LastBorrower)
            .expect("borrower missing");
        let repay_market: Address = env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::LastRepayMarket)
            .expect("repay market missing");
        let collateral_market: Address = env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::LastCollateralMarket)
            .expect("collateral market missing");
        let repay_amount: u128 = env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::LastRepayAmount)
            .expect("repay amount missing");
        let liquidator: Address = env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::LastLiquidator)
            .expect("liquidator missing");
        (
            borrower,
            repay_market,
            collateral_market,
            repay_amount,
            liquidator,
        )
    }
}

#[contractimpl]
impl MockVault {
    pub fn set_underlying_token(env: Env, token: Address) {
        env.storage()
            .persistent()
            .set(&MockVaultKey::UnderlyingToken, &token);
    }

    pub fn get_underlying_token(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&MockVaultKey::UnderlyingToken)
            .expect("underlying not set")
    }

    pub fn deposit(env: Env, user: Address, amount: u128) {
        let key = MockVaultKey::PTokenBalance(user);
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_add(amount));
    }

    pub fn withdraw(env: Env, user: Address, ptoken_amount: u128) {
        let key = MockVaultKey::PTokenBalance(user.clone());
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        if ptoken_amount > current {
            panic!("insufficient ptoken");
        }
        env.storage()
            .persistent()
            .set(&key, &current.saturating_sub(ptoken_amount));
        let token = Self::get_underlying_token(env.clone());
        MockTokenClient::new(&env, &token).mint(&user, &(ptoken_amount as i128));
    }

    pub fn get_ptoken_balance(env: Env, user: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&MockVaultKey::PTokenBalance(user))
            .unwrap_or(0)
    }

    pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&MockVaultKey::BorrowBalance(user))
            .unwrap_or(0)
    }

    pub fn borrow(env: Env, user: Address, amount: u128) {
        let key = MockVaultKey::BorrowBalance(user);
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_add(amount));
    }

    pub fn repay(env: Env, user: Address, amount: u128) {
        let key = MockVaultKey::BorrowBalance(user);
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_sub(amount.min(current)));
    }
}

fn setup_min() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

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
    usdt_vault.enable_static_rates(&admin);
    xlm_vault.enable_static_rates(&admin);

    let peridottroller_id = env.register(MockPeridottroller, ());
    MockPeridottrollerClient::new(&env, &peridottroller_id)
        .set_price(&usdt_id, &1_000_000u128, &1_000_000u128);
    MockPeridottrollerClient::new(&env, &peridottroller_id)
        .set_price(&xlm_id, &1_000_000u128, &1_000_000u128);
    usdt_vault.set_peridottroller(&peridottroller_id);
    xlm_vault.set_peridottroller(&peridottroller_id);

    let swap_adapter_id = env.register(MockSwapAdapter, ());

    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128, &50_000u128);
    controller.set_market(&admin, &usdt_id, &usdt_vault_id);
    controller.set_market(&admin, &xlm_id, &xlm_vault_id);
    usdt_vault.set_margin_controller(&admin, &Some(controller_id.clone()));
    xlm_vault.set_margin_controller(&admin, &Some(controller_id.clone()));

    usdt.mint(&user, &1_000_000i128);
    usdt.mint(&admin, &1_000_000i128);
    xlm.mint(&admin, &1_000_000i128);
    usdt_vault.deposit(&admin, &500_000u128);
    xlm_vault.deposit(&admin, &500_000u128);

    (env, controller_id, usdt_id, xlm_id, user)
}

fn setup_min_with_vaults(
) -> (Env, Address, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

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
    usdt_vault.enable_static_rates(&admin);
    xlm_vault.enable_static_rates(&admin);

    let peridottroller_id = env.register(MockPeridottroller, ());
    MockPeridottrollerClient::new(&env, &peridottroller_id)
        .set_price(&usdt_id, &1_000_000u128, &1_000_000u128);
    MockPeridottrollerClient::new(&env, &peridottroller_id)
        .set_price(&xlm_id, &1_000_000u128, &1_000_000u128);

    let swap_adapter_id = env.register(MockSwapAdapter, ());

    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128, &50_000u128);
    controller.set_market(&admin, &usdt_id, &usdt_vault_id);
    controller.set_market(&admin, &xlm_id, &xlm_vault_id);
    usdt_vault.set_margin_controller(&admin, &Some(controller_id.clone()));
    xlm_vault.set_margin_controller(&admin, &Some(controller_id.clone()));

    usdt.mint(&user, &1_000_000i128);
    usdt.mint(&admin, &1_000_000i128);
    xlm.mint(&admin, &1_000_000i128);
    usdt_vault.deposit(&admin, &500_000u128);
    xlm_vault.deposit(&admin, &500_000u128);

    (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    )
}

fn setup_short_min() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let usdt_id = env.register(MockToken, ());
    let xlm_id = env.register(MockToken, ());
    let usdt = MockTokenClient::new(&env, &usdt_id);
    let xlm = MockTokenClient::new(&env, &xlm_id);
    usdt.initialize(&"USDT".into_val(&env), &"USDT".into_val(&env), &6u32);
    xlm.initialize(&"XLM".into_val(&env), &"XLM".into_val(&env), &6u32);

    let usdt_vault_id = env.register(MockVault, ());
    let xlm_vault_id = env.register(MockVault, ());
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);
    let xlm_vault = MockVaultClient::new(&env, &xlm_vault_id);
    usdt_vault.set_underlying_token(&usdt_id);
    xlm_vault.set_underlying_token(&xlm_id);

    let peridottroller_id = env.register(MockPeridottroller, ());
    MockPeridottrollerClient::new(&env, &peridottroller_id)
        .set_price(&usdt_id, &1_000_000u128, &1_000_000u128);
    MockPeridottrollerClient::new(&env, &peridottroller_id)
        .set_price(&xlm_id, &1_000_000u128, &1_000_000u128);

    let swap_adapter_id = env.register(MockSwapAdapter, ());

    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128, &50_000u128);
    controller.set_market(&admin, &usdt_id, &usdt_vault_id);
    controller.set_market(&admin, &xlm_id, &xlm_vault_id);
    usdt.mint(&user, &1_000_000i128);

    (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    )
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
    usdt_vault.enable_static_rates(&admin);
    xlm_vault.enable_static_rates(&admin);

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
    usdt_vault.set_margin_controller(&admin, &Some(controller_id.clone()));
    xlm_vault.set_margin_controller(&admin, &Some(controller_id.clone()));

    // Enter markets so peridottroller counts collateral across vaults
    comp.enter_market(&user, &usdt_vault_id);
    comp.enter_market(&user, &xlm_vault_id);

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

fn setup_without_pre_enter_market(
) -> (Env, Address, Address, Address, Address, Address, Address, Address) {
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
    usdt_vault.enable_static_rates(&admin);
    xlm_vault.enable_static_rates(&admin);

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
    usdt_vault.set_margin_controller(&admin, &Some(controller_id.clone()));
    xlm_vault.set_margin_controller(&admin, &Some(controller_id.clone()));

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

fn mock_swaps_chain(
    env: &Env,
    token_in: &Address,
    token_out: &Address,
) -> Vec<(Vec<Address>, BytesN<32>, Address)> {
    let path: Vec<Address> = Vec::from_array(env, [token_in.clone(), token_out.clone()]);
    let pool_id = BytesN::from_array(env, &[1u8; 32]);
    let pool = Address::generate(env);
    Vec::from_array(env, [(path, pool_id, pool)])
}

#[test]
fn open_and_close_long() {
    let (env, controller_id, usdt_id, _xlm_id, user) = setup_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &100u128,
        &2u128,
        &PositionSide::Long,
    );
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Open);

    let swaps_chain_close = mock_swaps_chain(&env, &usdt_id, &usdt_id);
    controller.close_position(
        &user,
        &position_id,
        &swaps_chain_close,
        &200u128,
    );

    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Closed);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice_panics() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let admin = Address::generate(&env);
    let comp = Address::generate(&env);
    let swap = Address::generate(&env);
    controller.initialize(&admin, &comp, &swap, &5u128, &50_000u128);
}

#[test]
fn test_set_market_and_params() {
    let (env, _controller_id, usdt_id, _, _, _, usdt_vault_id, _) = setup();
    let admin = Address::generate(&env);

    // Re-initialize a fresh controller to test set_market and set_params
    let fresh_id = env.register(MarginController, ());
    let fresh = MarginControllerClient::new(&env, &fresh_id);
    let comp = Address::generate(&env);
    let swap = Address::generate(&env);
    fresh.initialize(&admin, &comp, &swap, &3u128, &10_000u128);
    fresh.set_market(&admin, &usdt_id, &usdt_vault_id);

    // Update params
    fresh.set_params(&admin, &5u128, &50_000u128);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_params_non_admin_panics() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let non_admin = Address::generate(&env);
    controller.set_params(&non_admin, &3u128, &10_000u128);
}

#[test]
fn test_set_max_slippage_bps() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let _controller = MarginControllerClient::new(&env, &controller_id);
    let admin = Address::generate(&env);

    let fresh_id = env.register(MarginController, ());
    let fresh = MarginControllerClient::new(&env, &fresh_id);
    let comp = Address::generate(&env);
    let swap = Address::generate(&env);
    fresh.initialize(&admin, &comp, &swap, &3u128, &10_000u128);
    fresh.set_max_slippage_bps(&admin, &25_000u128);
}

#[test]
#[should_panic(expected = "invalid slippage")]
fn test_set_max_slippage_bps_rejects_zero() {
    let (env, _controller_id, _, _, _, _, _, _) = setup();
    let admin = Address::generate(&env);
    let fresh_id = env.register(MarginController, ());
    let fresh = MarginControllerClient::new(&env, &fresh_id);
    let comp = Address::generate(&env);
    let swap = Address::generate(&env);
    fresh.initialize(&admin, &comp, &swap, &3u128, &10_000u128);
    fresh.set_max_slippage_bps(&admin, &0u128);
}

#[test]
fn test_open_position_no_swap() {
    let (env, controller_id, usdt_id, _xlm_id, user) = setup_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    // Use same asset for collateral and debt so deposit+borrow hit the same vault
    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Open);
    assert_eq!(pos.side, PositionSide::Long);
    assert_eq!(pos.owner, user);
}

#[test]
fn test_open_position_no_swap_enters_required_markets() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);

    let _ = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );

    assert!(comp.has_entered_market(&user, &usdt_vault_id));
    assert!(comp.has_entered_market(&user, &xlm_vault_id));
}

#[test]
fn test_open_position_enters_position_market_after_swap() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);

    let swaps_chain = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    let _ = controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain,
        &100u128,
    );

    assert!(comp.has_entered_market(&user, &usdt_vault_id));
    assert!(comp.has_entered_market(&user, &xlm_vault_id));
}

#[test]
fn test_open_position_no_swap_works_without_manual_enter_market() {
    let (env, controller_id, usdt_id, _xlm_id, user, _, _, _) =
        setup_without_pre_enter_market();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Open);
}

#[test]
fn test_locked_ptokens_in_market_tracks_open_position_collateral() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let _position_id = controller.open_position_no_swap_short(
        &user,
        &usdt_id,
        &xlm_id,
        &1_000u128,
        &500u128,
        &2u128,
    );

    let locked = controller.locked_ptokens_in_market(&user, &usdt_vault_id);
    assert_eq!(locked, 1_000u128);
}

#[test]
fn test_open_position_no_swap_short() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap_short(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &50u128,
        &2u128,
    );
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Open);
    assert_eq!(pos.side, PositionSide::Short);
    assert_eq!(pos.owner, user);
}

#[test]
fn test_open_short_position() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let swaps_chain = mock_swaps_chain(&env, &xlm_id, &usdt_id);
    let position_id = controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Short,
        &swaps_chain,
        &200u128,
    );
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Open);
    assert_eq!(pos.side, PositionSide::Short);
}

#[test]
#[should_panic(expected = "bad leverage")]
fn test_open_position_bad_leverage_panics() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let swaps_chain = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &0u128, // leverage=0 invalid
        &PositionSide::Long,
        &swaps_chain,
        &200u128,
    );
}

#[test]
#[should_panic(expected = "bad leverage")]
fn test_open_position_leverage_exceeds_max_panics() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let swaps_chain = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &100u128, // far exceeds max_leverage=5
        &PositionSide::Long,
        &swaps_chain,
        &200u128,
    );
}

#[test]
#[should_panic(expected = "leverage unsupported pre-swap")]
fn test_open_position_no_swap_rejects_leverage_above_pre_swap_cf_bound() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);

    // With CF=50%, pre-swap borrow gate only supports up to 1.5x leverage.
    comp.set_market_cf(&usdt_vault_id, &500_000u128);
    controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &30u128,
        &2u128,
        &PositionSide::Long,
    );
}

#[test]
#[should_panic(expected = "invalid market cf")]
fn test_open_position_no_swap_rejects_invalid_market_cf_scale() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);

    // Defensive check: unexpected CF scale from controller should fail fast.
    comp.set_market_cf(&usdt_vault_id, &1_500_000u128);
    controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &30u128,
        &2u128,
        &PositionSide::Long,
    );
}

#[test]
#[should_panic(expected = "bad collateral")]
fn test_open_position_zero_collateral_panics() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let swaps_chain = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &0u128, // zero collateral
        &2u128,
        &PositionSide::Long,
        &swaps_chain,
        &200u128,
    );
}

#[test]
#[should_panic(expected = "bad swaps")]
fn test_open_position_rejects_mismatched_swap_path() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);

    // Long expects debt->position route usdt -> xlm; this route is usdt -> usdt.
    let swaps_chain = mock_swaps_chain(&env, &usdt_id, &usdt_id);
    controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain,
        &200u128,
    );
}

#[test]
#[should_panic(expected = "bad swaps")]
fn test_open_position_rejects_empty_swap_hop_path() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let empty_path: Vec<Address> = Vec::new(&env);
    let swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)> = Vec::from_array(
        &env,
        [(
            empty_path,
            BytesN::from_array(&env, &[1u8; 32]),
            Address::generate(&env),
        )],
    );
    controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain,
        &200u128,
    );
}

#[test]
#[should_panic(expected = "borrow exceeds leverage")]
fn test_open_position_no_swap_refreshes_oracle_price_before_leverage_check() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, _, _) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);

    // Cached price remains $1.00 from setup, but live oracle has moved down to $0.10.
    // The controller must refresh first; otherwise this borrow would incorrectly pass.
    comp.set_live_price(&xlm_id, &100_000u128);
    controller.open_position_no_swap(
        &user,
        &xlm_id,
        &usdt_id,
        &100u128,
        &30u128,
        &2u128,
        &PositionSide::Long,
    );
}

#[test]
fn test_open_position_no_swap_calls_cache_price_for_assets() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, _, _) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);

    comp.set_live_price(&xlm_id, &1_000_000u128);
    comp.set_live_price(&usdt_id, &1_000_000u128);
    let _ = controller.open_position_no_swap(
        &user,
        &xlm_id,
        &usdt_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );
    assert!(comp.get_cache_price_calls(&xlm_id) > 0);
    assert!(comp.get_cache_price_calls(&usdt_id) > 0);
}

#[test]
fn test_open_position_no_swap_uses_cached_price_when_refresh_traps() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, _, _) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);

    // Simulate refresh path trapping (oracle unavailable). Controller should still
    // proceed using already-cached prices from get_price_usd.
    comp.set_cache_price_should_panic(&true);
    let position_id = controller.open_position_no_swap(
        &user,
        &xlm_id,
        &usdt_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Open);
}

#[test]
#[should_panic(expected = "slippage too high")]
fn test_open_position_rejects_low_slippage_floor() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let swaps_chain = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain,
        &1u128, // below oracle-derived minimum
    );
}

#[test]
#[should_panic(expected = "not owner")]
fn test_close_position_not_owner_panics() {
    let (env, controller_id, usdt_id, _xlm_id, user) = setup_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &100u128,
        &2u128,
        &PositionSide::Long,
    );

    let other_user = Address::generate(&env);
    let swaps_chain_close = mock_swaps_chain(&env, &usdt_id, &usdt_id);
    controller.close_position(
        &other_user,
        &position_id,
        &swaps_chain_close,
        &200u128,
    );
}

#[test]
#[should_panic(expected = "not open")]
fn test_close_position_already_closed_panics() {
    let (env, controller_id, usdt_id, _xlm_id, user) = setup_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &100u128,
        &2u128,
        &PositionSide::Long,
    );

    let swaps_chain_close = mock_swaps_chain(&env, &usdt_id, &usdt_id);
    controller.close_position(
        &user,
        &position_id,
        &swaps_chain_close,
        &200u128,
    );

    // Try closing again
    let swaps_chain_close2 = mock_swaps_chain(&env, &usdt_id, &usdt_id);
    controller.close_position(
        &user,
        &position_id,
        &swaps_chain_close2,
        &200u128,
    );
}

#[test]
#[should_panic(expected = "slippage too high")]
fn test_close_position_rejects_low_slippage_floor() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &100u128,
        &2u128,
        &PositionSide::Long,
    );

    let swaps_chain_close = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.close_position(&user, &position_id, &swaps_chain_close, &1u128);
}

#[test]
fn test_liquidate_position_calls_peridottroller_with_expected_order() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let liquidator = Address::generate(&env);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );
    comp.set_account_liquidity(&user, &0u128, &1u128);

    controller.liquidate_position(&liquidator, &position_id);

    let (borrower, repay_market, collateral_market, repay_amount, captured_liquidator) =
        comp.get_last_liquidation();
    assert_eq!(borrower, user);
    assert_eq!(repay_market, xlm_vault_id);
    assert_eq!(collateral_market, usdt_vault_id);
    assert_eq!(repay_amount, 50u128);
    assert_eq!(captured_liquidator, liquidator);

    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Liquidated);
}

#[test]
fn test_get_position_and_user_positions() {
    let (env, controller_id, usdt_id, _xlm_id, user) = setup_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &100u128,
        &2u128,
        &PositionSide::Long,
    );

    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.owner, user);
    assert_eq!(pos.side, PositionSide::Long);
    assert_eq!(pos.status, PositionStatus::Open);

    let user_positions = controller.get_user_positions(&user);
    assert_eq!(user_positions.len(), 1);
    assert_eq!(user_positions.get(0).unwrap(), position_id);
}

#[test]
fn test_get_health_factor() {
    let (env, controller_id, usdt_id, _xlm_id, user) = setup_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &100u128,
        &2u128,
        &PositionSide::Long,
    );

    let health = controller.get_health_factor(&position_id);
    // With 1:1 prices and 2x leverage, health factor should be > 0
    assert!(health > 0);
}

#[test]
fn test_multiple_positions() {
    let (env, controller_id, usdt_id, _xlm_id, user) = setup_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let id1 = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &100u128,
        &2u128,
        &PositionSide::Long,
    );

    let id2 = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &100u128,
        &2u128,
        &PositionSide::Long,
    );

    assert_ne!(id1, id2);
    let user_positions = controller.get_user_positions(&user);
    assert_eq!(user_positions.len(), 2);
}

#[test]
fn test_get_user_positions_prunes_missing_entries() {
    let (env, controller_id, _usdt_id, _xlm_id, user) = setup_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    env.as_contract(&controller_id, || {
        let mut stale = Vec::new(&env);
        stale.push_back(42u64);
        env.storage()
            .persistent()
            .set(&DataKey::UserPositions(user.clone()), &stale);
    });

    let user_positions = controller.get_user_positions(&user);
    assert_eq!(user_positions.len(), 0);
}

#[test]
fn test_deposit_and_withdraw_collateral() {
    let (env, controller_id, usdt_id, _xlm_id, user, _, usdt_vault_id, _) =
        setup_min_with_vaults();
    let controller = MarginControllerClient::new(&env, &controller_id);

    // Deposit collateral through controller
    controller.deposit_collateral(&user, &usdt_id, &100u128);

    // Check ptoken balance via vault
    let vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    let ptokens = vault.get_ptoken_balance(&user);
    assert!(ptokens > 0);

    // Withdraw collateral
    controller.withdraw_collateral(&user, &usdt_id, &ptokens);
    let ptokens_after = vault.get_ptoken_balance(&user);
    assert_eq!(ptokens_after, 0);
}
