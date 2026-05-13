use super::*;
use mock_token::{MockToken, MockTokenClient};
use receipt_vault::ReceiptVault;
use simple_peridottroller::SimplePeridottroller;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, BytesN, Env, IntoVal, Symbol, Vec,
};

fn assert_budget_under(env: &Env, max_cpu: u64, max_mem: u64) {
    let budget = env.cost_estimate().budget();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();
    assert!(cpu <= max_cpu, "cpu cost {cpu} exceeds {max_cpu}");
    assert!(mem <= max_mem, "mem cost {mem} exceeds {max_mem}");
}

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
    AccountLiquidityShouldPanic,
    MarketCF(Address),
    Liquidity(Address),
    Shortfall(Address),
    LastBorrower,
    LastRepayMarket,
    LastCollateralMarket,
    LastRepayAmount,
    LastLiquidator,
    EnteredMarket(Address, Address),
    LiquidateRepayBps,
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

#[contract]
struct MockBadSwapAdapter;

#[contract]
struct MockAuthSwapAdapter;

#[contracttype]
enum MockSwapAdapterKey {
    LastAmountIn,
}

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
        env.storage()
            .persistent()
            .set(&MockSwapAdapterKey::LastAmountIn, &amount);
        let last = swaps_chain.get(swaps_chain.len() - 1).unwrap();
        let (path, _, _) = last;
        let token_out = path.get(path.len() - 1).unwrap();
        MockTokenClient::new(&env, &token_out).mint(&user, &(amount as i128));
        amount
    }

    pub fn get_last_swap_amount_in(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&MockSwapAdapterKey::LastAmountIn)
            .unwrap_or(0u128)
    }
}

#[contractimpl]
impl MockBadSwapAdapter {
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
        let received = amount / 2;
        let last = swaps_chain.get(swaps_chain.len() - 1).unwrap();
        let (path, _, _) = last;
        let token_out = path.get(path.len() - 1).unwrap();
        MockTokenClient::new(&env, &token_out).mint(&user, &(received as i128));
        received
    }
}

#[contractimpl]
impl MockAuthSwapAdapter {
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
        user.require_auth();
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
    MarginBorrow(u64),
    UnderlyingToken,
    MarginController,
    WithdrawPayoutBps,
    MarginInterestIncrement,
    LastMarginPosition,
}

#[contractimpl]
impl MockPeridottroller {
    pub fn set_price(env: Env, asset: Address, price: u128, _scale: u128) {
        env.storage().persistent().set(
            &OracleKey::Price(asset),
            &OraclePrice {
                price: price as i128,
            },
        );
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
        env.storage().persistent().set(
            &OracleKey::Price(asset.clone()),
            &OraclePrice {
                price: price as i128,
            },
        );
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

    pub fn set_liq_panic(env: Env, should_panic: bool) {
        env.storage().persistent().set(
            &MockPeridottrollerKey::AccountLiquidityShouldPanic,
            &should_panic,
        );
    }

    pub fn account_liquidity(env: Env, user: Address) -> (u128, u128) {
        if env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::AccountLiquidityShouldPanic)
            .unwrap_or(false)
        {
            panic!("account liquidity should not be called");
        }
        let liquidity: u128 = env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::Liquidity(user.clone()))
            .unwrap_or(u128::MAX);
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
        env.storage()
            .persistent()
            .get(&MockPeridottrollerKey::MarketCF(market))
            .unwrap_or(1_000_000u128)
    }

    pub fn set_market_cf(env: Env, market: Address, cf: u128) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::MarketCF(market), &cf);
    }

    pub fn set_liquidate_repay_bps(env: Env, bps: u128) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LiquidateRepayBps, &bps);
    }

    pub fn get_collateral_excl_usd(_env: Env, _user: Address, _market: Address) -> u128 {
        0u128
    }

    pub fn get_borrows_excl(_env: Env, _user: Address, _market: Address) -> u128 {
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
        env.storage().persistent().set(
            &MockPeridottrollerKey::LastCollateralMarket,
            &collateral_market,
        );
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LastRepayAmount, &repay_amount);
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LastLiquidator, &liquidator);

        // Apply a configurable mocked liquidation effect so post-call debt reflects progress.
        let repay_bps: u128 = env
            .storage()
            .persistent()
            .get(&MockPeridottrollerKey::LiquidateRepayBps)
            .unwrap_or(1_000_000u128);
        let effective_repay = repay_amount.saturating_mul(repay_bps) / 1_000_000u128;
        if effective_repay > 0 {
            MockVaultClient::new(&env, &repay_market).repay(&borrower, &effective_repay);
        }
    }

    pub fn liquidate_for_margin(
        env: Env,
        _controller: Address,
        borrower: Address,
        repay_market: Address,
        collateral_market: Address,
        repay_amount: u128,
        liquidator: Address,
        _position_shortfall_usd: u128,
        max_seize_ptokens: u128,
    ) -> u128 {
        Self::liquidate(
            env.clone(),
            borrower,
            repay_market,
            collateral_market,
            repay_amount,
            liquidator,
        );
        max_seize_ptokens
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
        let payout_bps: u128 = env
            .storage()
            .persistent()
            .get(&MockVaultKey::WithdrawPayoutBps)
            .unwrap_or(1_000_000u128);
        let payout = ptoken_amount.saturating_mul(payout_bps) / 1_000_000u128;
        MockTokenClient::new(&env, &token).mint(&user, &(payout as i128));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        if amount < 0 {
            panic!("bad amount");
        }
        let amt = amount as u128;
        let from_key = MockVaultKey::PTokenBalance(from.clone());
        let to_key = MockVaultKey::PTokenBalance(to.clone());
        let from_bal: u128 = env.storage().persistent().get(&from_key).unwrap_or(0);
        if from_bal < amt {
            panic!("insufficient ptoken");
        }
        let to_bal: u128 = env.storage().persistent().get(&to_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&from_key, &from_bal.saturating_sub(amt));
        env.storage()
            .persistent()
            .set(&to_key, &to_bal.saturating_add(amt));
    }

    pub fn transfer_from(env: Env, _spender: Address, owner: Address, to: Address, amount: i128) {
        Self::transfer(env, owner, to, amount);
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

    pub fn get_exchange_rate(_env: Env) -> u128 {
        1_000_000u128
    }

    pub fn update_interest(env: Env) {
        let increment: u128 = env
            .storage()
            .persistent()
            .get(&MockVaultKey::MarginInterestIncrement)
            .unwrap_or(0);
        if increment == 0 {
            return;
        }
        if let Some(position_id) = env
            .storage()
            .persistent()
            .get::<_, u64>(&MockVaultKey::LastMarginPosition)
        {
            let key = MockVaultKey::MarginBorrow(position_id);
            let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
            env.storage()
                .persistent()
                .set(&key, &current.saturating_add(increment));
        }
    }

    pub fn borrow(env: Env, user: Address, amount: u128) {
        let key = MockVaultKey::BorrowBalance(user);
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_add(amount));
    }

    pub fn init_margin_borrow_state(_env: Env, _position_id: u64) {}

    pub fn repay(env: Env, user: Address, amount: u128) {
        let key = MockVaultKey::BorrowBalance(user);
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_sub(amount.min(current)));
    }

    pub fn borrow_for_margin(env: Env, position_id: u64, _receiver: Address, amount: u128) {
        let key = MockVaultKey::MarginBorrow(position_id);
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_add(amount));
        env.storage()
            .persistent()
            .set(&MockVaultKey::LastMarginPosition, &position_id);
    }

    pub fn repay_for_margin(env: Env, position_id: u64, _payer: Address, amount: u128) {
        let key = MockVaultKey::MarginBorrow(position_id);
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_sub(amount.min(current)));
    }

    pub fn get_margin_borrow_balance(env: Env, position_id: u64) -> u128 {
        env.storage()
            .persistent()
            .get(&MockVaultKey::MarginBorrow(position_id))
            .unwrap_or(0)
    }

    pub fn set_margin_interest_increment(env: Env, amount: u128) {
        env.storage()
            .persistent()
            .set(&MockVaultKey::MarginInterestIncrement, &amount);
    }

    pub fn set_margin_controller(env: Env, margin_controller: Option<Address>) {
        if let Some(controller) = margin_controller {
            env.storage()
                .persistent()
                .set(&MockVaultKey::MarginController, &controller);
            return;
        }
        env.storage()
            .persistent()
            .remove(&MockVaultKey::MarginController);
    }

    pub fn get_margin_controller(env: Env) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&MockVaultKey::MarginController)
    }

    pub fn begin_margin_withdraw(_env: Env, _margin_controller: Address, _user: Address) {}

    pub fn set_withdraw_payout_bps(env: Env, payout_bps: u128) {
        env.storage()
            .persistent()
            .set(&MockVaultKey::WithdrawPayoutBps, &payout_bps);
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
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &usdt_id,
        &1_000_000u128,
        &1_000_000u128,
    );
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &xlm_id,
        &1_000_000u128,
        &1_000_000u128,
    );
    usdt_vault.set_peridottroller(&peridottroller_id);
    xlm_vault.set_peridottroller(&peridottroller_id);

    let swap_adapter_id = env.register(MockSwapAdapter, ());

    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128);
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

fn setup_min_with_vaults() -> (
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

    let usdt_vault_id = env.register(ReceiptVault, ());
    let xlm_vault_id = env.register(ReceiptVault, ());
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    usdt_vault.initialize(&usdt_id, &0u128, &0u128, &admin);
    xlm_vault.initialize(&xlm_id, &0u128, &0u128, &admin);
    usdt_vault.enable_static_rates(&admin);
    xlm_vault.enable_static_rates(&admin);

    let peridottroller_id = env.register(MockPeridottroller, ());
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &usdt_id,
        &1_000_000u128,
        &1_000_000u128,
    );
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &xlm_id,
        &1_000_000u128,
        &1_000_000u128,
    );

    let swap_adapter_id = env.register(MockSwapAdapter, ());

    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128);
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
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &usdt_id,
        &1_000_000u128,
        &1_000_000u128,
    );
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &xlm_id,
        &1_000_000u128,
        &1_000_000u128,
    );

    let swap_adapter_id = env.register(MockSwapAdapter, ());

    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128);
    controller.set_market(&admin, &usdt_id, &usdt_vault_id);
    controller.set_market(&admin, &xlm_id, &xlm_vault_id);
    usdt_vault.set_margin_controller(&Some(controller_id.clone()));
    xlm_vault.set_margin_controller(&Some(controller_id.clone()));
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
fn setup() -> (
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
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128);
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

#[allow(dead_code)]
fn setup_without_pre_enter_market() -> (
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
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128);
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

/// Functional correctness of open_position_no_swap_v2 with real ReceiptVault +
/// SimplePeridottroller. Resource limits disabled in this test because the path
/// is currently ~114 footprint entries vs mainnet's 100-entry cap (see
/// test_open_position_no_swap_documents_footprint_limit for the legacy path
/// at 144 — V2 is a 21% reduction). Further reductions require either
/// modifying audited core (skip update_interest in early-position state) or
/// splitting the open into more user-signed transactions.
#[test]
fn test_open_position_no_swap_v2_correctness() {
    let (env, controller_id, usdt_id, xlm_id, user, _lender, usdt_vault_id, xlm_vault_id) = setup();
    env.cost_estimate().disable_resource_limits();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);

    // Pre-deposit collateral and move into margin custody (separate user txs).
    usdt_vault.deposit(&user, &100u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &100u128);

    let id = controller.open_position_no_swap_v2(
        &user, &usdt_id, &xlm_id, &100u128, // collateral pTokens (already in margin custody)
        &50u128,  // borrow amount in debt asset
        &2u128,
    );
    let pos = controller.get_position(&id).unwrap();
    assert_eq!(pos.status, PositionStatus::Open);
    assert_eq!(pos.collateral_ptokens, 100u128);
    assert_eq!(pos.debt_shares, 0u128); // V2 uses margin namespace, no debt_shares

    // Verify borrowed funds landed with user via margin namespace
    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    let outstanding = xlm_vault.get_margin_borrow_balance(&id);
    assert_eq!(outstanding, 50u128);
}

#[test]
fn test_open_and_close_position_no_swap_v2() {
    let (env, controller_id, usdt_id, xlm_id, user, _lender, usdt_vault_id, _xlm_vault_id) =
        setup();
    env.cost_estimate().disable_resource_limits();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);

    usdt_vault.deposit(&user, &100u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &100u128);

    let id =
        controller.open_position_no_swap_v2(&user, &usdt_id, &xlm_id, &100u128, &50u128, &2u128);

    // Close: user pays debt from wallet, gets collateral pTokens back to margin balance
    controller.close_position_no_swap_v2(&user, &id);

    let pos = controller.get_position(&id).unwrap();
    assert_eq!(pos.status, PositionStatus::Closed);
    // Collateral pTokens released to user's margin balance for the collateral vault
    let margin_bal = controller.get_margin_balance_ptokens(&user, &usdt_id);
    assert_eq!(margin_bal, 100u128);
}

/// Documented finding: open_position_no_swap exceeds Soroban's per-invocation
/// LEDGER FOOTPRINT limit (100 entries), NOT the CPU budget.
///
/// Profiling against the real ReceiptVault + SimplePeridottroller stack reports:
///   "invocation resource limits are exceeded: total footprint ledger entries: 144 > 100"
///
/// Root cause: open_position_no_swap orchestrates vault.deposit + 2×enter_market
/// + vault.borrow, and vault.borrow internally calls peridottroller.account_liquidity
/// which reads collateral and debt state from EVERY entered market. With 2 markets
/// entered and reward accrual hooks active, the storage footprint balloons.
///
/// Open_position_v2 avoids this by using the margin-namespace borrow_for_margin
/// path which bypasses the per-market reward accrual and uses a simpler position-
/// scoped debt namespace. Recommend deprecating open_position_no_swap or refactoring
/// it to use the borrow_for_margin path internally.
#[test]
#[should_panic(expected = "invocation resource limits are exceeded")]
fn test_open_position_no_swap_documents_footprint_limit() {
    let (env, controller_id, usdt_id, xlm_id, user, _lender, _usdt_vault_id, _xlm_vault_id) =
        setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    env.cost_estimate().budget().reset_unlimited();
    // This call panics with "invocation resource limits are exceeded:
    // total footprint ledger entries: 144 > 100" under the metered host.
    let _id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );
}

#[test]
fn open_and_close_long() {
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
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Open);

    let swaps_chain_close = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.close_position(&user, &position_id, &swaps_chain_close, &200u128);

    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Closed);
}

#[test]
fn test_transfer_spot_and_margin_ptokens() {
    let (env, controller_id, usdt_id, _xlm_id, user, _pid, usdt_vault_id, _xid) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);

    usdt_vault.deposit(&user, &100u128);
    assert_eq!(usdt_vault.get_ptoken_balance(&user), 100u128);

    controller.transfer_spot_to_margin(&user, &usdt_id, &60u128);
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &usdt_id),
        60u128
    );
    assert_eq!(usdt_vault.get_ptoken_balance(&user), 40u128);
    assert_eq!(usdt_vault.get_ptoken_balance(&controller_id), 60u128);

    controller.transfer_margin_to_spot(&user, &usdt_id, &10u128);
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &usdt_id),
        50u128
    );
    assert_eq!(
        controller.get_margin_balance_underlying(&user, &usdt_id),
        50u128
    );
    assert_eq!(usdt_vault.get_ptoken_balance(&user), 50u128);
    assert_eq!(usdt_vault.get_ptoken_balance(&controller_id), 50u128);
}

#[test]
fn test_open_and_close_position_v2_restores_margin_balance() {
    let (env, controller_id, usdt_id, xlm_id, user, _pid, usdt_vault_id, _xlm_vault_id) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);

    usdt_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &200u128);
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &usdt_id),
        200u128
    );

    let swaps_chain_open = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    let position_id = controller.open_position_v2(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain_open,
        &100u128,
    );
    let pos_open = controller.get_position(&position_id).unwrap();
    assert_eq!(pos_open.status, PositionStatus::Open);

    let swaps_chain_close = mock_swaps_chain(&env, &xlm_id, &usdt_id);
    controller.close_position_v2(&user, &position_id, &swaps_chain_close, &100u128);

    let pos_closed = controller.get_position(&position_id).unwrap();
    assert_eq!(pos_closed.status, PositionStatus::Closed);
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &usdt_id),
        200u128
    );
}

#[test]
fn test_close_position_v2_authorizes_controller_swap() {
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
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    peridottroller.set_price(&usdt_id, &1_000_000u128, &1_000_000u128);
    peridottroller.set_price(&xlm_id, &1_000_000u128, &1_000_000u128);

    let swap_adapter_id = env.register(MockAuthSwapAdapter, ());
    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128);
    controller.set_market(&admin, &usdt_id, &usdt_vault_id);
    controller.set_market(&admin, &xlm_id, &xlm_vault_id);
    usdt_vault.set_margin_controller(&Some(controller_id.clone()));
    xlm_vault.set_margin_controller(&Some(controller_id.clone()));

    usdt.mint(&user, &1_000_000i128);
    usdt_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &200u128);

    let swaps_chain_open = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    let position_id = controller.open_position_v2(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain_open,
        &100u128,
    );

    let swaps_chain_close = mock_swaps_chain(&env, &xlm_id, &usdt_id);
    controller.close_position_v2(&user, &position_id, &swaps_chain_close, &100u128);
    let pos_closed = controller.get_position(&position_id).unwrap();
    assert_eq!(pos_closed.status, PositionStatus::Closed);
}

#[test]
fn test_liquidate_position_v2_marks_liquidated() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _xid) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);
    let liquidator = Address::generate(&env);

    usdt_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &200u128);
    let swaps_chain_open = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    let position_id = controller.open_position_v2(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain_open,
        &100u128,
    );

    // Force underwater by dropping position collateral and discounting the
    // initial collateral market.
    peridottroller.set_price(&xlm_id, &400_000u128, &1_000_000u128);
    peridottroller.set_market_cf(&usdt_vault_id, &500_000u128);
    controller.liquidate_position_v2(&liquidator, &position_id);
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Liquidated);
}

#[test]
fn test_liquidate_position_v2_accrues_margin_debt_before_repay() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _xid) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);
    let liquidator = Address::generate(&env);

    usdt_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &200u128);
    let swaps_chain_open = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    let position_id = controller.open_position_v2(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain_open,
        &100u128,
    );

    usdt_vault.set_margin_interest_increment(&5u128);
    peridottroller.set_price(&xlm_id, &400_000u128, &1_000_000u128);
    peridottroller.set_market_cf(&usdt_vault_id, &500_000u128);

    controller.liquidate_position_v2(&liquidator, &position_id);

    assert_eq!(usdt_vault.get_margin_borrow_balance(&position_id), 0u128);
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Liquidated);
}

#[test]
fn test_open_position_v2_budget_short_min() {
    let (env, controller_id, usdt_id, xlm_id, user, _peridottroller_id, usdt_vault_id, _xid) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);

    usdt_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &200u128);
    let swaps_chain_open = mock_swaps_chain(&env, &usdt_id, &xlm_id);

    env.cost_estimate().budget().reset_unlimited();
    let _position_id = controller.open_position_v2(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain_open,
        &100u128,
    );
    assert_budget_under(&env, 8_000_000, 1_500_000);
}

#[test]
fn test_open_position_v2_applies_collateral_factor_to_borrow_sizing() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _xid) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);

    peridottroller.set_market_cf(&usdt_vault_id, &500_000u128);
    usdt_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &200u128);
    let swaps_chain_open = mock_swaps_chain(&env, &usdt_id, &xlm_id);

    let position_id = controller.open_position_v2(
        &user,
        &usdt_id,
        &xlm_id,
        &200u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain_open,
        &100u128,
    );

    // Raw collateral is 200, but with CF=50% the leverage base is 100.
    // 2x therefore borrows 100, not the previous raw-collateral 200.
    assert_eq!(usdt_vault.get_margin_borrow_balance(&position_id), 100u128);
}

#[test]
#[should_panic(expected = "not liquidatable")]
fn test_liquidate_position_v2_counts_initial_locked_collateral() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _xid) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);
    let liquidator = Address::generate(&env);

    usdt_vault.deposit(&user, &100u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &100u128);
    let swaps_chain_open = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    let position_id = controller.open_position_v2(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain_open,
        &100u128,
    );

    // Position-asset collateral drops to 10, but the initial 100 USDT lock
    // still makes combined collateral value exceed the 100 USDT debt.
    peridottroller.set_price(&xlm_id, &100_000u128, &1_000_000u128);
    controller.liquidate_position_v2(&liquidator, &position_id);
}

#[test]
fn test_get_health_factor_v2_counts_initial_locked_collateral() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _xid) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);

    usdt_vault.deposit(&user, &100u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &100u128);
    let swaps_chain_open = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    let position_id = controller.open_position_v2(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain_open,
        &100u128,
    );

    // Position collateral is worth 10, initial locked USDT collateral is worth
    // 100, and debt is 100. HF must therefore be 1.1, not 0.1.
    peridottroller.set_price(&xlm_id, &100_000u128, &1_000_000u128);
    assert_eq!(controller.get_health_factor(&position_id), 1_100_000u128);
}

#[test]
fn test_close_position_v2_budget_short_min() {
    let (env, controller_id, usdt_id, xlm_id, user, _peridottroller_id, usdt_vault_id, _xid) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);

    usdt_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &200u128);
    let swaps_chain_open = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    let position_id = controller.open_position_v2(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain_open,
        &100u128,
    );

    let swaps_chain_close = mock_swaps_chain(&env, &xlm_id, &usdt_id);
    env.cost_estimate().budget().reset_unlimited();
    controller.close_position_v2(&user, &position_id, &swaps_chain_close, &100u128);
    assert_budget_under(&env, 8_500_000, 1_600_000);
}

#[test]
fn test_liquidate_position_v2_budget_short_min() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _xid) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);
    let liquidator = Address::generate(&env);

    usdt_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &200u128);
    let swaps_chain_open = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    let position_id = controller.open_position_v2(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain_open,
        &100u128,
    );
    peridottroller.set_price(&xlm_id, &400_000u128, &1_000_000u128);
    peridottroller.set_market_cf(&usdt_vault_id, &500_000u128);

    env.cost_estimate().budget().reset_unlimited();
    controller.liquidate_position_v2(&liquidator, &position_id);
    assert_budget_under(&env, 8_000_000, 1_500_000);
}

#[test]
fn test_close_position_withdraws_initial_collateral_lock() {
    let (env, controller_id, usdt_id, xlm_id, user, _comp, usdt_vault_id, _xlm_vault_id) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);

    let open_swaps = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    let position_id = controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &open_swaps,
        &100u128,
    );

    // Initial collateral pTokens are minted in the collateral (USDT) vault.
    assert_eq!(usdt_vault.get_ptoken_balance(&user), 100u128);

    let close_swaps = mock_swaps_chain(&env, &xlm_id, &usdt_id);
    controller.close_position(&user, &position_id, &close_swaps, &100u128);

    // Closing now auto-withdraws the initial collateral lock.
    assert_eq!(usdt_vault.get_ptoken_balance(&user), 0u128);
}

#[test]
fn test_close_position_swaps_actual_withdrawn_amount_after_interest_accrual() {
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
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);
    let swap_adapter_id: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::SwapAdapter)
            .expect("swap adapter not set")
    });
    let swap_adapter = MockSwapAdapterClient::new(&env, &swap_adapter_id);

    // Emulate post-snapshot accrual by paying out 110% underlying on withdraw.
    usdt_vault.set_withdraw_payout_bps(&1_100_000u128);

    let position_id = controller
        .open_position_no_swap_short(&user, &usdt_id, &xlm_id, &1_000u128, &500u128, &2u128);
    let position = controller.get_position(&position_id).unwrap();

    let swaps_chain_close = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.close_position(&user, &position_id, &swaps_chain_close, &2_000u128);

    let swapped_amount = swap_adapter.get_last_swap_amount_in();
    assert!(
        swapped_amount > position.collateral_ptokens,
        "swap input should use actual withdrawn underlying, not stale pre-withdraw snapshot"
    );
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice_panics() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let admin = Address::generate(&env);
    let comp = Address::generate(&env);
    let swap = Address::generate(&env);
    controller.initialize(&admin, &comp, &swap, &5u128);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_rejects_when_legacy_instance_initialized_exists() {
    let env = Env::default();
    env.mock_all_auths();
    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);

    env.as_contract(&controller_id, || {
        env.storage().instance().set(&DataKey::Initialized, &true);
    });

    let admin = Address::generate(&env);
    let comp = Address::generate(&env);
    let swap = env.register(MockSwapAdapter, ());
    controller.initialize(&admin, &comp, &swap, &5u128);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_rejects_when_admin_key_exists_without_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);

    env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Admin, &Address::generate(&env));
    });

    let admin = Address::generate(&env);
    let comp = Address::generate(&env);
    let swap = env.register(MockSwapAdapter, ());
    controller.initialize(&admin, &comp, &swap, &5u128);
}

#[test]
fn test_set_market_and_params() {
    let (env, _controller_id, usdt_id, _, _, _, usdt_vault_id, _) = setup();
    let admin = Address::generate(&env);

    // Re-initialize a fresh controller to test set_market and set_params
    let fresh_id = env.register(MarginController, ());
    let fresh = MarginControllerClient::new(&env, &fresh_id);
    let comp = Address::generate(&env);
    let swap = env.register(MockSwapAdapter, ());
    fresh.initialize(&admin, &comp, &swap, &3u128);
    fresh.set_market(&admin, &usdt_id, &usdt_vault_id);

    // Update params
    fresh.set_params(&admin, &5u128);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_params_non_admin_panics() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let non_admin = Address::generate(&env);
    controller.set_params(&non_admin, &3u128);
}

#[test]
fn test_set_max_slippage_bps() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let _controller = MarginControllerClient::new(&env, &controller_id);
    let admin = Address::generate(&env);

    let fresh_id = env.register(MarginController, ());
    let fresh = MarginControllerClient::new(&env, &fresh_id);
    let comp = Address::generate(&env);
    let swap = env.register(MockSwapAdapter, ());
    fresh.initialize(&admin, &comp, &swap, &3u128);
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
    let swap = env.register(MockSwapAdapter, ());
    fresh.initialize(&admin, &comp, &swap, &3u128);
    fresh.set_max_slippage_bps(&admin, &0u128);
}

#[test]
#[should_panic(expected = "invalid swap adapter")]
fn test_set_swap_adapter_rejects_invalid_contract() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let admin: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    });
    let not_adapter = Address::generate(&env);
    controller.set_swap_adapter(&admin, &not_adapter);
}

#[test]
fn test_open_position_no_swap() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
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
#[should_panic(expected = "assets must differ")]
fn test_open_position_no_swap_rejects_same_assets() {
    let (env, controller_id, usdt_id, _xlm_id, user) = setup_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let _ = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );
}

#[test]
#[should_panic(expected = "assets must differ")]
fn test_open_position_rejects_same_collateral_and_base() {
    let (env, controller_id, usdt_id, _xlm_id, user, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let swaps_chain = mock_swaps_chain(&env, &usdt_id, &usdt_id);

    let _ = controller.open_position(
        &user,
        &usdt_id,
        &usdt_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain,
        &100u128,
    );
}

#[test]
fn test_open_position_no_swap_issues_ceil_debt_shares() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        _usdt_vault_id,
        _xlm_vault_id,
    ) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let _ = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &20u128,
        &10u128,
        &2u128,
        &PositionSide::Long,
    );

    // Force a non-integer share/debt ratio before the next borrow:
    // debt_before = 10, shares_before = 3, borrow = 4.
    // New share issuance should be ceil(4*3/10) = 2.
    let debt_key = DataKey::DebtSharesTotal(user.clone(), xlm_id.clone());
    env.as_contract(&controller_id, || {
        env.storage().persistent().set(&debt_key, &3u128);
    });

    let second_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &20u128,
        &4u128,
        &2u128,
        &PositionSide::Long,
    );
    let second = controller.get_position(&second_id).unwrap();
    assert_eq!(second.debt_shares, 2u128);

    let total_shares: u128 = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::DebtSharesTotal(user.clone(), xlm_id.clone()))
            .expect("missing debt shares total")
    });
    assert_eq!(total_shares, 5u128);
}

#[test]
fn test_debt_shares_total_recovers_from_open_positions_when_missing() {
    let (env, controller_id, usdt_id, xlm_id, user, _peridottroller_id, _usdt_vault_id, _xid) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );

    let debt_key = DataKey::DebtSharesTotal(user.clone(), xlm_id.clone());
    env.as_contract(&controller_id, || {
        env.storage().persistent().remove(&debt_key);
    });

    assert_eq!(controller.get_health_factor(&position_id), 2_000_000u128);
    let recovered: u128 = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&debt_key)
            .expect("debt shares not recovered")
    });
    assert_eq!(recovered, 50u128);
}

#[test]
fn test_open_position_no_swap_enters_required_markets() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, xlm_vault_id) =
        setup_short_min();
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
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, xlm_vault_id) =
        setup_short_min();
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
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
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

    let _position_id = controller
        .open_position_no_swap_short(&user, &usdt_id, &xlm_id, &1_000u128, &500u128, &2u128);

    let locked = controller.locked_ptokens_in_market(&user, &usdt_vault_id);
    assert_eq!(locked, 1_000u128);
}

#[test]
fn test_open_position_no_swap_short() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id =
        controller.open_position_no_swap_short(&user, &usdt_id, &xlm_id, &100u128, &50u128, &2u128);
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
        &100u128,
    );
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Open);
    assert_eq!(pos.side, PositionSide::Short);
}

#[test]
#[should_panic(expected = "slippage too high")]
fn test_open_position_rejects_user_slippage_floor_not_met() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let swaps_chain = mock_swaps_chain(&env, &xlm_id, &usdt_id);
    controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Short,
        &swaps_chain,
        &200u128, // higher than realizable output in this setup
    );
}

#[test]
#[should_panic(expected = "margin lock not configured")]
fn test_open_position_rejects_market_without_margin_lock_introspection() {
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
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);
    usdt_vault.set_margin_controller(&None);

    let _ = controller
        .open_position_no_swap_short(&user, &usdt_id, &xlm_id, &1_000u128, &500u128, &2u128);
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
#[should_panic(expected = "borrow exceeds leverage")]
fn test_open_position_no_swap_applies_cf_to_borrow_ceiling() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);

    // Raw collateral value is $100, but with CF=50% the leverage ceiling is based on $50.
    // At leverage=1, a $60 borrow must fail once CF discounting is applied.
    comp.set_market_cf(&usdt_vault_id, &500_000u128);
    controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &60u128,
        &1u128,
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
fn test_open_position_rejects_wrong_swap_input_endpoint() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);

    // Long expects debt->position route usdt -> xlm; this route starts from xlm.
    let swaps_chain = mock_swaps_chain(&env, &xlm_id, &xlm_id);
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
#[should_panic(expected = "bad swaps")]
fn test_close_position_rejects_mismatched_swap_path() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &30u128,
        &2u128,
        &PositionSide::Long,
    );

    // Close path for this position must be usdt -> xlm. usdt -> usdt should fail validation.
    let bad_swaps_chain = mock_swaps_chain(&env, &usdt_id, &usdt_id);
    controller.close_position(&user, &position_id, &bad_swaps_chain, &200u128);
}

#[test]
#[should_panic(expected = "borrow exceeds leverage")]
fn test_open_position_no_swap_refreshes_oracle_price_before_leverage_check() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, _, _) = setup_short_min();
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
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, _, _) = setup_short_min();
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
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, _, _) = setup_short_min();
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
#[should_panic(expected = "slippage too high")]
fn test_open_position_rejects_amm_output_below_oracle_floor() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    // Swap adapter that always returns only 50% of the input amount.
    let bad_swap_adapter_id = env.register(MockBadSwapAdapter, ());
    let admin: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    });
    controller.set_swap_adapter(&admin, &bad_swap_adapter_id);

    let swaps_chain = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.open_position(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &2u128,
        &PositionSide::Long,
        &swaps_chain,
        &100u128, // meets user floor, but not the oracle-derived minimum
    );
}

#[test]
#[should_panic(expected = "not owner")]
fn test_close_position_not_owner_panics() {
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

    let other_user = Address::generate(&env);
    let swaps_chain_close = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.close_position(&other_user, &position_id, &swaps_chain_close, &200u128);
}

#[test]
#[should_panic(expected = "not open")]
fn test_close_position_already_closed_panics() {
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
    controller.close_position(&user, &position_id, &swaps_chain_close, &200u128);

    // Try closing again
    let swaps_chain_close2 = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.close_position(&user, &position_id, &swaps_chain_close2, &200u128);
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
fn test_close_position_allows_underwater_with_wallet_topup() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, xlm_vault_id) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    // Build an underwater position: collateral=100 USDT, debt=150 XLM.
    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &150u128,
        &2u128,
        &PositionSide::Long,
    );

    // Swap output from collateral (100) is not enough to repay debt (150); top up from wallet.
    MockTokenClient::new(&env, &xlm_id).mint(&user, &1_000i128);
    let swaps_chain_close = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.close_position(&user, &position_id, &swaps_chain_close, &1_000u128);

    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Closed);
    assert_eq!(
        MockVaultClient::new(&env, &xlm_vault_id).get_user_borrow_balance(&user),
        0u128
    );
}

#[test]
fn test_close_position_uses_snapshotted_vault_after_market_remap() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_short_min();
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

    let admin: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    });

    let new_usdt_vault_id = env.register(MockVault, ());
    let new_usdt_vault = MockVaultClient::new(&env, &new_usdt_vault_id);
    new_usdt_vault.set_underlying_token(&usdt_id);
    new_usdt_vault.set_margin_controller(&Some(controller_id.clone()));
    controller.set_market(&admin, &usdt_id, &new_usdt_vault_id);

    let swaps_chain_close = mock_swaps_chain(&env, &usdt_id, &xlm_id);
    controller.close_position(&user, &position_id, &swaps_chain_close, &200u128);

    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Closed);
    assert_eq!(
        MockVaultClient::new(&env, &usdt_vault_id).get_ptoken_balance(&user),
        0u128
    );
    assert_eq!(
        MockVaultClient::new(&env, &new_usdt_vault_id).get_ptoken_balance(&user),
        0u128
    );
    assert_eq!(
        MockVaultClient::new(&env, &xlm_vault_id).get_user_borrow_balance(&user),
        0u128
    );
}

#[test]
fn test_locked_ptokens_uses_snapshotted_vault_after_market_remap() {
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

    let _position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );

    let admin: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    });

    let new_usdt_vault_id = env.register(MockVault, ());
    let new_usdt_vault = MockVaultClient::new(&env, &new_usdt_vault_id);
    new_usdt_vault.set_underlying_token(&usdt_id);
    new_usdt_vault.set_margin_controller(&Some(controller_id.clone()));
    controller.set_market(&admin, &usdt_id, &new_usdt_vault_id);

    let locked_old = controller.locked_ptokens_in_market(&user, &usdt_vault_id);
    let locked_new = controller.locked_ptokens_in_market(&user, &new_usdt_vault_id);
    assert_eq!(locked_old, 100u128);
    assert_eq!(locked_new, 0u128);
}

#[test]
fn test_liquidate_position_calls_peridottroller_with_expected_order() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, xlm_vault_id) =
        setup_short_min();
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
    // Make the position insolvent in position-isolated terms (HF < 1.0).
    comp.set_price(&usdt_id, &400_000u128, &1_000_000u128);

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
fn test_liquidate_position_uses_snapshotted_vaults_after_market_remap() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, xlm_vault_id) =
        setup_short_min();
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

    // Make position insolvent.
    comp.set_price(&usdt_id, &400_000u128, &1_000_000u128);

    let admin: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    });
    let new_usdt_vault_id = env.register(MockVault, ());
    let new_usdt_vault = MockVaultClient::new(&env, &new_usdt_vault_id);
    new_usdt_vault.set_underlying_token(&usdt_id);
    new_usdt_vault.set_margin_controller(&Some(controller_id.clone()));
    controller.set_market(&admin, &usdt_id, &new_usdt_vault_id);

    let new_xlm_vault_id = env.register(MockVault, ());
    let new_xlm_vault = MockVaultClient::new(&env, &new_xlm_vault_id);
    new_xlm_vault.set_underlying_token(&xlm_id);
    new_xlm_vault.set_margin_controller(&Some(controller_id.clone()));
    controller.set_market(&admin, &xlm_id, &new_xlm_vault_id);

    controller.liquidate_position(&liquidator, &position_id);

    let (_borrower, repay_market, collateral_market, _repay_amount, _liq) =
        comp.get_last_liquidation();
    assert_eq!(repay_market, xlm_vault_id);
    assert_eq!(collateral_market, usdt_vault_id);

    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Liquidated);
}

#[test]
fn test_liquidate_position_partial_repay_keeps_position_open_and_preserves_other_position_debt() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        _usdt_vault_id,
        _xlm_vault_id,
    ) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let liquidator = Address::generate(&env);

    // User needs collateral asset (XLM) for these long no-swap positions.
    MockTokenClient::new(&env, &xlm_id).mint(&user, &1_000i128);

    // Two positions share the same (user, debt_asset) share pool.
    let pos_a_id = controller.open_position_no_swap(
        &user,
        &xlm_id,
        &usdt_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );
    let pos_b_id = controller.open_position_no_swap(
        &user,
        &xlm_id,
        &usdt_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );

    // Make positions underwater for liquidation eligibility.
    comp.set_price(&xlm_id, &400_000u128, &1_000_000u128);
    comp.set_price(&usdt_id, &1_000_000u128, &1_000_000u128);
    comp.set_account_liquidity(&user, &0u128, &1u128);

    let hf_b_before = controller.get_health_factor(&pos_b_id);
    assert_eq!(hf_b_before, 800_000u128);

    // Simulate close-factor-capped liquidation progress (50% of requested repay).
    comp.set_liquidate_repay_bps(&500_000u128);
    controller.liquidate_position(&liquidator, &pos_a_id);

    let pos_a_after = controller.get_position(&pos_a_id).unwrap();
    assert_eq!(pos_a_after.status, PositionStatus::Open);
    assert_eq!(pos_a_after.debt_shares, 25u128);

    // Position B's debt projection should remain stable (no cross-position contamination).
    let hf_b_after = controller.get_health_factor(&pos_b_id);
    assert_eq!(hf_b_after, hf_b_before);
}

#[test]
fn test_liquidate_position_allows_hf_liquidation_even_with_positive_account_liquidity() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        _usdt_vault_id,
        _xlm_vault_id,
    ) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let liquidator = Address::generate(&env);

    let position_id = controller.open_position_no_swap(
        &user,
        &xlm_id,
        &usdt_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );
    // Position-level insolvency: collateral (XLM) marked down to 40% of initial value.
    comp.set_price(&xlm_id, &400_000u128, &1_000_000u128);
    // Global account liquidity still appears healthy; this should no longer block liquidation.
    comp.set_account_liquidity(&user, &9_940u128, &0u128);

    controller.liquidate_position(&liquidator, &position_id);
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Liquidated);
}

#[test]
fn test_liquidate_position_does_not_call_account_liquidity_gate() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        _usdt_vault_id,
        _xlm_vault_id,
    ) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let comp = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let liquidator = Address::generate(&env);

    let position_id = controller.open_position_no_swap(
        &user,
        &xlm_id,
        &usdt_id,
        &100u128,
        &50u128,
        &2u128,
        &PositionSide::Long,
    );

    // Ensure position is insolvent for position-level liquidation.
    comp.set_price(&xlm_id, &400_000u128, &1_000_000u128);
    // If liquidation still depends on account_liquidity(), this call will panic.
    comp.set_liq_panic(&true);

    controller.liquidate_position(&liquidator, &position_id);
    let pos = controller.get_position(&position_id).unwrap();
    assert_eq!(pos.status, PositionStatus::Liquidated);
}

#[test]
#[should_panic(expected = "not liquidatable")]
fn test_liquidate_position_rejects_healthy_hf_even_if_account_has_shortfall() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        _usdt_vault_id,
        _xlm_vault_id,
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
    // Force account-level shortfall signal while position remains healthy (HF=2.0 at 1:1 prices).
    comp.set_account_liquidity(&user, &0u128, &1u128);
    controller.liquidate_position(&liquidator, &position_id);
}

#[test]
fn test_get_position_and_user_positions() {
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

    let health = controller.get_health_factor(&position_id);
    // With 1:1 prices and 2x leverage, health factor should be > 0
    assert!(health > 0);
}

#[test]
fn test_get_health_factor_applies_collateral_factor() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let peridottroller_id: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Peridottroller)
            .expect("peridottroller not set")
    });
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let usdt_vault_id: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Market(usdt_id.clone()))
            .expect("market not set")
    });

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &60u128,
        &2u128,
        &PositionSide::Long,
    );

    let hf_before = controller.get_health_factor(&position_id);
    assert_eq!(hf_before, 1_666_666u128);

    // Halve collateral factor; health factor must reflect discounted collateral.
    peridottroller.set_market_cf(&usdt_vault_id, &500_000u128);
    let hf_after = controller.get_health_factor(&position_id);
    assert_eq!(hf_after, 833_333u128);
    assert!(hf_after < 1_000_000u128);
}

#[test]
fn test_get_health_factor_invalid_cf_returns_indeterminate() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let peridottroller_id: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Peridottroller)
            .expect("peridottroller not set")
    });
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let usdt_vault_id: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Market(usdt_id.clone()))
            .expect("market not set")
    });

    let position_id = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &60u128,
        &2u128,
        &PositionSide::Long,
    );

    peridottroller.set_market_cf(&usdt_vault_id, &1_000_001u128);
    let hf = controller.get_health_factor(&position_id);
    assert_eq!(hf, u128::MAX);
}

#[test]
fn test_multiple_positions() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let id1 = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &100u128,
        &2u128,
        &PositionSide::Long,
    );

    let id2 = controller.open_position_no_swap(
        &user,
        &usdt_id,
        &xlm_id,
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
    let (env, controller_id, usdt_id, _xlm_id, user, _, usdt_vault_id, _) = setup_min_with_vaults();
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
