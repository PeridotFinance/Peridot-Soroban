use super::*;
use mock_token::{MockToken, MockTokenClient};
use receipt_vault::ReceiptVault;
use simple_peridottroller::SimplePeridottroller;
use soroban_sdk::events::Event as _;
use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Events as _;
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

fn assert_last_invocation_resources_under(
    env: &Env,
    max_ledger_entries: u32,
    max_write_entries: u32,
    max_instructions: i64,
) {
    let resources = env.cost_estimate().resources();
    let ledger_entries = resources
        .disk_read_entries
        .saturating_add(resources.memory_read_entries);
    assert!(
        ledger_entries <= max_ledger_entries,
        "ledger entries {ledger_entries} exceed {max_ledger_entries}: {resources:?}"
    );
    assert!(
        resources.write_entries <= max_write_entries,
        "write entries {} exceed {max_write_entries}: {resources:?}",
        resources.write_entries
    );
    assert!(
        resources.instructions <= max_instructions,
        "instructions {} exceed {max_instructions}: {resources:?}",
        resources.instructions
    );
}

#[contract]
struct MockOracle;

#[contract]
struct MockRateModel;

#[contractimpl]
impl MockRateModel {
    pub fn get_supply_rate(
        _env: Env,
        _cash: u128,
        _borrows: u128,
        _reserves: u128,
        _reserve_factor: u128,
    ) -> u128 {
        0u128
    }

    pub fn get_borrow_rate(_env: Env, _cash: u128, _borrows: u128, _reserves: u128) -> u128 {
        100_000u128
    }
}

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
    BorrowPaused(Address),
    LiquidationPaused(Address),
    CloseFactor,
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

#[contract]
struct MockAquariusPool;

#[contracttype]
enum MockSwapAdapterKey {
    LastAmountIn,
    PoolAllowed(Address),
}

#[contracttype]
enum MockAquariusPoolKey {
    LastAmountIn,
    LastUser,
    QuoteBps,
    PayoutBps,
    ReportedBps,
}

#[contractimpl]
impl MockSwapAdapter {
    pub fn is_pool_allowed(env: Env, pool: Address) -> bool {
        env.storage()
            .persistent()
            .get(&MockSwapAdapterKey::PoolAllowed(pool))
            .unwrap_or(true)
    }

    pub fn is_pool_binding_allowed(env: Env, _pool_id: BytesN<32>, pool: Address) -> bool {
        Self::is_pool_allowed(env, pool)
    }

    pub fn set_pool_allowed(env: Env, pool: Address, allowed: bool) {
        env.storage()
            .persistent()
            .set(&MockSwapAdapterKey::PoolAllowed(pool), &allowed);
    }

    pub fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        token_in: Address,
        amount: u128,
        _amount_with_slippage: u128,
    ) -> u128 {
        env.storage()
            .persistent()
            .set(&MockSwapAdapterKey::LastAmountIn, &amount);
        let amount_i128: i128 = amount.try_into().expect("amount too large");
        let token_in_client = MockTokenClient::new(&env, &token_in);
        if token_in_client.balance(&user) >= amount_i128 {
            token_in_client.transfer(&user, &env.current_contract_address(), &amount_i128);
        }
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

    pub fn is_pool_binding_allowed(_env: Env, _pool_id: BytesN<32>, _pool: Address) -> bool {
        true
    }

    pub fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        token_in: Address,
        amount: u128,
        _amount_with_slippage: u128,
    ) -> u128 {
        let received = amount / 2;
        let amount_i128: i128 = amount.try_into().expect("amount too large");
        let token_in_client = MockTokenClient::new(&env, &token_in);
        if token_in_client.balance(&user) >= amount_i128 {
            token_in_client.transfer(&user, &env.current_contract_address(), &amount_i128);
        }
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

    pub fn is_pool_binding_allowed(_env: Env, _pool_id: BytesN<32>, _pool: Address) -> bool {
        true
    }

    pub fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        token_in: Address,
        amount: u128,
        _amount_with_slippage: u128,
    ) -> u128 {
        user.require_auth();
        let amount_i128: i128 = amount.try_into().expect("amount too large");
        let token_in_client = MockTokenClient::new(&env, &token_in);
        if token_in_client.balance(&user) >= amount_i128 {
            token_in_client.transfer(&user, &env.current_contract_address(), &amount_i128);
        }
        let last = swaps_chain.get(swaps_chain.len() - 1).unwrap();
        let (path, _, _) = last;
        let token_out = path.get(path.len() - 1).unwrap();
        MockTokenClient::new(&env, &token_out).mint(&user, &(amount as i128));
        amount
    }
}

#[contractimpl]
impl MockAquariusPool {
    pub fn estimate_swap(env: Env, _in_idx: u32, _out_idx: u32, amount_in: u128) -> u128 {
        let quote_bps: u128 = env
            .storage()
            .persistent()
            .get(&MockAquariusPoolKey::QuoteBps)
            .unwrap_or(1_000_000u128);
        amount_in.saturating_mul(quote_bps) / 1_000_000u128
    }

    pub fn swap(
        env: Env,
        user: Address,
        in_idx: u32,
        out_idx: u32,
        amount_in: u128,
        _amount_out_min: u128,
    ) -> u128 {
        env.storage()
            .persistent()
            .set(&MockAquariusPoolKey::LastAmountIn, &amount_in);
        env.storage()
            .persistent()
            .set(&MockAquariusPoolKey::LastUser, &user);
        let payout_bps: u128 = env
            .storage()
            .persistent()
            .get(&MockAquariusPoolKey::PayoutBps)
            .unwrap_or(1_000_000u128);
        let amount_out = amount_in.saturating_mul(payout_bps) / 1_000_000u128;
        if amount_out < _amount_out_min {
            panic!("slippage too high");
        }
        let amount_i128: i128 = amount_in.try_into().expect("amount too large");
        let amount_out_i128: i128 = amount_out.try_into().expect("amount too large");
        let (token_in, token_out) = if in_idx == 0 && out_idx == 1 {
            let token_in: Address = env
                .storage()
                .persistent()
                .get(&Symbol::new(&env, "t0"))
                .unwrap();
            let token_out: Address = env
                .storage()
                .persistent()
                .get(&Symbol::new(&env, "t1"))
                .unwrap();
            (token_in, token_out)
        } else if in_idx == 1 && out_idx == 0 {
            let token_in: Address = env
                .storage()
                .persistent()
                .get(&Symbol::new(&env, "t1"))
                .unwrap();
            let token_out: Address = env
                .storage()
                .persistent()
                .get(&Symbol::new(&env, "t0"))
                .unwrap();
            (token_in, token_out)
        } else {
            panic!("bad idx");
        };
        let token_in_client = MockTokenClient::new(&env, &token_in);
        if token_in_client.balance(&user) >= amount_i128 {
            token_in_client.transfer(&user, &env.current_contract_address(), &amount_i128);
        }
        MockTokenClient::new(&env, &token_out).mint(&user, &amount_out_i128);
        let reported_bps: u128 = env
            .storage()
            .persistent()
            .get(&MockAquariusPoolKey::ReportedBps)
            .unwrap_or(payout_bps);
        amount_in.saturating_mul(reported_bps) / 1_000_000u128
    }

    pub fn set_tokens(env: Env, token_0: Address, token_1: Address) {
        env.storage()
            .persistent()
            .set(&Symbol::new(&env, "t0"), &token_0);
        env.storage()
            .persistent()
            .set(&Symbol::new(&env, "t1"), &token_1);
    }

    pub fn get_last_swap_amount_in(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&MockAquariusPoolKey::LastAmountIn)
            .unwrap_or(0u128)
    }

    pub fn get_last_user(env: Env) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&MockAquariusPoolKey::LastUser)
    }

    pub fn set_payout_bps(env: Env, payout_bps: u128) {
        env.storage()
            .persistent()
            .set(&MockAquariusPoolKey::PayoutBps, &payout_bps);
    }

    pub fn set_quote_bps(env: Env, quote_bps: u128) {
        env.storage()
            .persistent()
            .set(&MockAquariusPoolKey::QuoteBps, &quote_bps);
    }

    pub fn set_reported_bps(env: Env, reported_bps: u128) {
        env.storage()
            .persistent()
            .set(&MockAquariusPoolKey::ReportedBps, &reported_bps);
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
            &OracleKey::Price(asset.clone()),
            &OraclePrice {
                price: price as i128,
            },
        );
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LivePrice(asset), &price);
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

    pub fn is_borrow_paused(env: Env, market: Address) -> bool {
        env.storage()
            .persistent()
            .get(&MockPeridottrollerKey::BorrowPaused(market))
            .unwrap_or(false)
    }

    pub fn is_market_supported(_env: Env, _market: Address) -> bool {
        true
    }

    pub fn is_liquidation_paused(env: Env, market: Address) -> bool {
        env.storage()
            .persistent()
            .get(&MockPeridottrollerKey::LiquidationPaused(market))
            .unwrap_or(false)
    }

    pub fn get_close_factor_scaled(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&MockPeridottrollerKey::CloseFactor)
            .unwrap_or(500_000u128)
    }

    pub fn get_liquidation_incentive_scaled(_env: Env) -> u128 {
        1_080_000u128
    }

    pub fn get_liquidation_fee_scaled(_env: Env) -> u128 {
        0u128
    }

    pub fn get_reserve_recipient(_env: Env) -> Option<Address> {
        None
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

    pub fn set_borrow_paused(env: Env, market: Address, paused: bool) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::BorrowPaused(market), &paused);
    }

    pub fn set_liquidation_paused(env: Env, market: Address, paused: bool) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::LiquidationPaused(market), &paused);
    }

    pub fn set_close_factor_scaled(env: Env, close_factor: u128) {
        env.storage()
            .persistent()
            .set(&MockPeridottrollerKey::CloseFactor, &close_factor);
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

    pub fn borrow_for_margin_to_controller(env: Env, position_id: u64, amount: u128) {
        let controller: Address = env
            .storage()
            .persistent()
            .get(&MockVaultKey::MarginController)
            .expect("margin controller missing");
        Self::borrow_for_margin(env.clone(), position_id, controller.clone(), amount);
        let token = Self::get_underlying_token(env.clone());
        MockTokenClient::new(&env, &token).mint(&controller, &(amount as i128));
    }

    pub fn repay_for_margin(env: Env, position_id: u64, _payer: Address, amount: u128) {
        let key = MockVaultKey::MarginBorrow(position_id);
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &current.saturating_sub(amount.min(current)));
    }

    pub fn repay_full_for_margin(
        env: Env,
        position_id: u64,
        _payer: Address,
        max_amount: u128,
    ) -> u128 {
        let key = MockVaultKey::MarginBorrow(position_id);
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        if max_amount < current {
            panic!("max repay too small");
        }
        env.storage().persistent().set(&key, &0u128);
        current
    }

    pub fn absorb_margin_bad_debt(env: Env, position_id: u64) -> u128 {
        let key = MockVaultKey::MarginBorrow(position_id);
        let current: u128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &0u128);
        current
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

    pub fn begin_margin_withdraw(
        _env: Env,
        _margin_controller: Address,
        _user: Address,
        _recipient: Address,
        _max_ptokens: u128,
    ) {
    }

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
    usdt.initialize(&"USDT".into_val(&env), &"USDT".into_val(&env), &7u32);
    xlm.initialize(&"XLM".into_val(&env), &"XLM".into_val(&env), &7u32);

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
    usdt.initialize(&"USDT".into_val(&env), &"USDT".into_val(&env), &7u32);
    xlm.initialize(&"XLM".into_val(&env), &"XLM".into_val(&env), &7u32);

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
    usdt.initialize(&"USDT".into_val(&env), &"USDT".into_val(&env), &7u32);
    xlm.initialize(&"XLM".into_val(&env), &"XLM".into_val(&env), &7u32);

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
    usdt.initialize(&"USDT".into_val(&env), &"USDT".into_val(&env), &7u32);
    xlm.initialize(&"XLM".into_val(&env), &"XLM".into_val(&env), &7u32);

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

#[test]
fn test_admin_transfer_requires_pending_admin_acceptance() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let admin = controller.get_admin();
    let new_admin = Address::generate(&env);

    controller.set_admin(&admin, &new_admin);
    assert_eq!(controller.get_admin(), admin);

    controller.accept_admin();
    assert_eq!(controller.get_admin(), new_admin.clone());
    controller.set_params(&new_admin, &4u128);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_previous_admin_loses_access_after_transfer() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let admin = controller.get_admin();
    let new_admin = Address::generate(&env);

    controller.set_admin(&admin, &new_admin);
    controller.accept_admin();
    controller.set_params(&admin, &4u128);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_admin_transfer_rejects_non_admin_proposer() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let non_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    controller.set_admin(&non_admin, &new_admin);
}

#[test]
#[should_panic]
fn test_accept_admin_requires_pending_admin_auth() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let admin = controller.get_admin();
    let new_admin = Address::generate(&env);

    controller.set_admin(&admin, &new_admin);
    env.set_auths(&[]);
    controller.accept_admin();
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
    usdt.initialize(&"USDT".into_val(&env), &"USDT".into_val(&env), &7u32);
    xlm.initialize(&"XLM".into_val(&env), &"XLM".into_val(&env), &7u32);

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

fn setup_perps_pool(
    env: &Env,
    usdt_id: &Address,
    xlm_id: &Address,
) -> (Address, BytesN<32>, Vec<Address>) {
    let pool = env.register(MockAquariusPool, ());
    MockAquariusPoolClient::new(env, &pool).set_tokens(usdt_id, xlm_id);
    let binding_id = BytesN::from_array(env, &[9u8; 32]);
    let pool_tokens = Vec::from_array(env, [usdt_id.clone(), xlm_id.clone()]);
    (pool, binding_id, pool_tokens)
}

fn controller_swap_adapter(env: &Env, controller_id: &Address) -> Address {
    env.as_contract(controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::SwapAdapter)
            .expect("swap adapter missing")
    })
}

fn begin_and_swap_perps_long_10x(
    env: &Env,
    controller: &MarginControllerClient,
    user: &Address,
    usdt_id: &Address,
    xlm_id: &Address,
    usdt_vault_id: &Address,
    margin_ptokens: u128,
) -> (u64, Address, BytesN<32>, Vec<Address>) {
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(env, usdt_vault_id);
    usdt_vault.deposit(user, &margin_ptokens);
    controller.transfer_spot_to_margin(user, usdt_id, &margin_ptokens);

    let (pool, pool_id, pool_tokens) = setup_perps_pool(env, usdt_id, xlm_id);
    let position_id = controller.begin_open_position_v3(
        user,
        usdt_id,
        xlm_id,
        &margin_ptokens,
        &10u128,
        &PositionSide::Long,
        &pool_tokens,
        &pool_id,
        &pool,
        &(margin_ptokens.saturating_mul(10)),
    );
    controller.swap_open_position_v3(user, &position_id);
    (position_id, pool, pool_id, pool_tokens)
}

fn open_perps_long_10x(
    env: &Env,
    controller: &MarginControllerClient,
    user: &Address,
    usdt_id: &Address,
    xlm_id: &Address,
    usdt_vault_id: &Address,
    margin_ptokens: u128,
) -> (u64, Address, BytesN<32>, Vec<Address>) {
    let (position_id, pool, pool_id, pool_tokens) = begin_and_swap_perps_long_10x(
        env,
        controller,
        user,
        usdt_id,
        xlm_id,
        usdt_vault_id,
        margin_ptokens,
    );
    controller.activate_open_position_v3(user, &position_id);
    (position_id, pool, pool_id, pool_tokens)
}

fn open_perps_long_with_leverage(
    env: &Env,
    controller: &MarginControllerClient,
    user: &Address,
    usdt_id: &Address,
    xlm_id: &Address,
    usdt_vault_id: &Address,
    margin_ptokens: u128,
    leverage: u128,
) -> (u64, Address, BytesN<32>, Vec<Address>) {
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(env, usdt_vault_id);
    usdt_vault.deposit(user, &margin_ptokens);
    controller.transfer_spot_to_margin(user, usdt_id, &margin_ptokens);

    let (pool, pool_id, pool_tokens) = setup_perps_pool(env, usdt_id, xlm_id);
    let notional = margin_ptokens
        .checked_mul(leverage)
        .expect("notional overflow");
    let position_id = controller.begin_open_position_v3(
        user,
        usdt_id,
        xlm_id,
        &margin_ptokens,
        &leverage,
        &PositionSide::Long,
        &pool_tokens,
        &pool_id,
        &pool,
        &notional,
    );
    controller.swap_open_position_v3(user, &position_id);
    controller.activate_open_position_v3(user, &position_id);
    (position_id, pool, pool_id, pool_tokens)
}

fn open_perps_short_10x(
    env: &Env,
    controller: &MarginControllerClient,
    user: &Address,
    usdt_id: &Address,
    xlm_id: &Address,
    usdt_vault_id: &Address,
    margin_ptokens: u128,
) -> (u64, Address, BytesN<32>, Vec<Address>) {
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(env, usdt_vault_id);
    usdt_vault.deposit(user, &margin_ptokens);
    controller.transfer_spot_to_margin(user, usdt_id, &margin_ptokens);

    let (pool, pool_id, pool_tokens) = setup_perps_pool(env, usdt_id, xlm_id);
    let position_id = controller.begin_open_position_v3(
        user,
        usdt_id,
        xlm_id,
        &margin_ptokens,
        &10u128,
        &PositionSide::Short,
        &pool_tokens,
        &pool_id,
        &pool,
        &margin_ptokens.saturating_mul(10),
    );
    controller.swap_open_position_v3(user, &position_id);
    controller.activate_open_position_v3(user, &position_id);
    (position_id, pool, pool_id, pool_tokens)
}

#[test]
fn test_open_position_v3_long_uses_full_notional_from_controller_custody() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let (position_id, pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    let position = controller.get_position(&position_id).unwrap();
    assert_eq!(position.status, PositionStatus::Open);
    assert_eq!(position.side, PositionSide::Long);
    assert_eq!(position.collateral_asset, xlm_id);
    assert_eq!(position.debt_asset, usdt_id);
    assert_eq!(position.collateral_ptokens, 5_000u128);
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &position.debt_asset),
        0u128
    );

    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    assert_eq!(
        usdt_vault.get_margin_borrow_balance(&position_id),
        4_500u128
    );
    assert_eq!(
        xlm_vault.get_ptoken_balance(&controller_id),
        position.collateral_ptokens
    );
    assert_eq!(controller.get_health_factor(&position_id), 2_000_000u128);

    let pool_client = MockAquariusPoolClient::new(&env, &pool);
    assert_eq!(pool_client.get_last_swap_amount_in(), 5_000u128);
    assert_eq!(pool_client.get_last_user().unwrap(), controller_id);
}

#[test]
fn test_open_position_v3_split_records_swap_before_activation() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    usdt_vault.deposit(&user, &500u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &500u128);
    let (pool, pool_id, pool_tokens) = setup_perps_pool(&env, &usdt_id, &xlm_id);

    let position_id = controller.begin_open_position_v3(
        &user,
        &usdt_id,
        &xlm_id,
        &500u128,
        &10u128,
        &PositionSide::Long,
        &pool_tokens,
        &pool_id,
        &pool,
        &5_000u128,
    );
    controller.swap_open_position_v3(&user, &position_id);

    let execution = controller
        .get_pending_perps_open_execution(&position_id)
        .unwrap();
    assert_eq!(execution.margin_received, 500u128);
    assert_eq!(execution.position_amount, 5_000u128);
    assert!(controller.get_pending_perps_open(&position_id).is_some());
    assert_eq!(
        usdt_vault.get_margin_borrow_balance(&position_id),
        4_500u128
    );
    assert_eq!(xlm_vault.get_ptoken_balance(&controller_id), 0u128);

    controller.activate_open_position_v3(&user, &position_id);

    let position = controller.get_position(&position_id).unwrap();
    assert_eq!(position.status, PositionStatus::Open);
    assert_eq!(position.collateral_ptokens, 5_000u128);
    assert!(controller
        .get_pending_perps_open_execution(&position_id)
        .is_none());
    assert!(controller.get_pending_perps_open(&position_id).is_none());
    assert_eq!(
        xlm_vault.get_ptoken_balance(&controller_id),
        position.collateral_ptokens
    );
}

#[test]
fn test_execute_open_position_v3_finishes_already_swapped_pending() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let (position_id, _pool, _pool_id, _pool_tokens) = begin_and_swap_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    controller.execute_open_position_v3(&user, &position_id);

    let position = controller.get_position(&position_id).unwrap();
    assert_eq!(position.status, PositionStatus::Open);
    assert!(controller
        .get_pending_perps_open_execution(&position_id)
        .is_none());
}

#[test]
#[should_panic(expected = "open swap required")]
fn test_execute_open_position_v3_requires_completed_swap() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    usdt_vault.deposit(&user, &500u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &500u128);
    let (pool, pool_id, pool_tokens) = setup_perps_pool(&env, &usdt_id, &xlm_id);
    let position_id = controller.begin_open_position_v3(
        &user,
        &usdt_id,
        &xlm_id,
        &500u128,
        &5u128,
        &PositionSide::Long,
        &pool_tokens,
        &pool_id,
        &pool,
        &2_500u128,
    );
    controller.execute_open_position_v3(&user, &position_id);
}

#[test]
fn test_activate_open_position_v3_allows_swapped_pending_after_expiry() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let (position_id, _pool, _pool_id, _pool_tokens) = begin_and_swap_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    let pending = controller.get_pending_perps_open(&position_id).unwrap();
    env.ledger()
        .with_mut(|l| l.timestamp = pending.expires_at.saturating_add(1));

    controller.activate_open_position_v3(&user, &position_id);

    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Open
    );
    assert!(controller.get_pending_perps_open(&position_id).is_none());
}

#[test]
fn test_activate_open_position_v3_rejects_under_maintenance_after_price_move() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let (position_id, _pool, _pool_id, _pool_tokens) = begin_and_swap_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &xlm_id,
        &800_000u128,
        &1_000_000u128,
    );

    assert!(controller
        .try_activate_open_position_v3(&user, &position_id)
        .is_err());

    let position = controller.get_position(&position_id).unwrap();
    assert_eq!(position.status, PositionStatus::PendingOpen);
    assert!(controller
        .get_pending_perps_open_execution(&position_id)
        .is_some());
    assert!(controller.get_pending_perps_open(&position_id).is_some());

    let liquidator = Address::generate(&env);
    assert!(controller
        .try_liquidate_position_v3(&liquidator, &position_id)
        .is_err());

    let pending = controller.get_pending_perps_open(&position_id).unwrap();
    env.ledger()
        .with_mut(|l| l.timestamp = pending.expires_at.saturating_add(1));
    controller.liquidate_position_v3(&liquidator, &position_id);

    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    assert_eq!(usdt_vault.get_margin_borrow_balance(&position_id), 0u128);
    assert!(controller.get_position(&position_id).is_none());
    assert!(controller.get_pending_perps_open(&position_id).is_none());
    assert!(controller
        .get_pending_perps_open_execution(&position_id)
        .is_none());
}

#[test]
fn test_activate_open_position_v3_accrues_interest_before_health_check() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);

    let (position_id, _pool, _pool_id, _pool_tokens) = begin_and_swap_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    assert_eq!(
        usdt_vault.get_margin_borrow_balance(&position_id),
        4_500u128
    );
    usdt_vault.set_borrow_rate(&10_000_000u128);
    env.ledger()
        .with_mut(|l| l.timestamp = l.timestamp.saturating_add(365 * 24 * 60 * 60));

    assert!(controller
        .try_activate_open_position_v3(&user, &position_id)
        .is_err());

    let position = controller.get_position(&position_id).unwrap();
    assert_eq!(position.status, PositionStatus::PendingOpen);
    assert!(controller
        .get_pending_perps_open_execution(&position_id)
        .is_some());

    usdt_vault.update_interest();
    assert!(usdt_vault.get_margin_borrow_balance(&position_id) > 4_500u128);
}

#[test]
fn test_repay_position_v3_reduces_debt_and_improves_health() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    controller.repay_margin_position_v3(&user, &position_id, &500u128);
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    assert_eq!(
        usdt_vault.get_margin_borrow_balance(&position_id),
        4_000u128
    );
    assert_eq!(controller.get_health_factor(&position_id), 4_000_000u128);
}

#[test]
fn test_add_position_collateral_v3_allocates_free_margin_and_improves_health() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    MockTokenClient::new(&env, &xlm_id).mint(&user, &200i128);
    xlm_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &xlm_id, &200u128);

    let health_before = controller.get_health_factor(&position_id);
    let controller_ptokens_before = xlm_vault.get_ptoken_balance(&controller_id);
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &xlm_id),
        200u128
    );

    env.as_contract(&controller_id, || {
        let key = DataKey::UserPositions(user.clone());
        let mut positions: Vec<u64> = env.storage().persistent().get(&key).unwrap();
        for offset in 1..MAX_USER_POSITIONS {
            positions.push_back(position_id.saturating_add(offset as u64));
        }
        env.storage().persistent().set(&key, &positions);
    });

    env.cost_estimate().budget().reset_unlimited();
    controller.add_position_collateral_v3(&user, &position_id, &75u128);
    assert_budget_under(&env, 4_000_000, 800_000);
    assert_last_invocation_resources_under(&env, 50, 20, 10_000_000);

    env.cost_estimate().budget().reset_unlimited();
    controller.add_position_collateral_v3(&user, &position_id, &125u128);
    assert_budget_under(&env, 4_000_000, 800_000);
    assert_last_invocation_resources_under(&env, 50, 20, 10_000_000);
    let expected_event = PositionCollateralAdded {
        owner: user.clone(),
        position_id,
        ptoken_amount: 125u128,
        collateral_ptokens: 5_200u128,
    };
    assert_eq!(
        env.events()
            .all()
            .filter_by_contract(&controller_id)
            .events(),
        &[expected_event.to_xdr(&env, &controller_id)]
    );

    let position = controller.get_position(&position_id).unwrap();
    assert_eq!(position.collateral_ptokens, 5_200u128);
    assert_eq!(controller.get_margin_balance_ptokens(&user, &xlm_id), 0u128);
    assert_eq!(
        xlm_vault.get_ptoken_balance(&controller_id),
        controller_ptokens_before
    );
    assert!(controller.get_health_factor(&position_id) > health_before);
}

#[test]
fn test_add_position_collateral_v3_rejects_invalid_requests() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    assert!(controller
        .try_add_position_collateral_v3(&user, &position_id, &0u128)
        .is_err());
    assert!(controller
        .try_add_position_collateral_v3(&user, &position_id, &1u128)
        .is_err());
    assert!(controller
        .try_add_position_collateral_v3(&Address::generate(&env), &position_id, &1u128)
        .is_err());

    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    MockTokenClient::new(&env, &xlm_id).mint(&user, &100i128);
    xlm_vault.deposit(&user, &100u128);
    controller.transfer_spot_to_margin(&user, &xlm_id, &100u128);
    controller.begin_close_position_v3(&user, &position_id);
    assert!(controller
        .try_add_position_collateral_v3(&user, &position_id, &100u128)
        .is_err());
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &xlm_id),
        100u128
    );
    assert_eq!(
        controller
            .get_position(&position_id)
            .unwrap()
            .collateral_ptokens,
        5_000u128
    );
}

#[test]
fn test_added_position_collateral_is_preserved_by_close_recovery() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    MockTokenClient::new(&env, &xlm_id).mint(&user, &100i128);
    xlm_vault.deposit(&user, &100u128);
    controller.transfer_spot_to_margin(&user, &xlm_id, &100u128);
    controller.add_position_collateral_v3(&user, &position_id, &100u128);

    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    assert_eq!(
        controller
            .get_pending_perps_close(&position_id)
            .unwrap()
            .collateral_underlying,
        5_100u128
    );

    controller.cancel_close_position_v3(&user, &position_id);
    let restored = controller.get_position(&position_id).unwrap();
    assert_eq!(restored.status, PositionStatus::Open);
    assert_eq!(restored.collateral_ptokens, 5_100u128);
}

#[test]
fn test_added_position_collateral_is_included_in_liquidation_quote() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, xlm_vault_id) =
        setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    MockTokenClient::new(&env, &xlm_id).mint(&user, &100i128);
    xlm_vault.deposit(&user, &100u128);
    controller.transfer_spot_to_margin(&user, &xlm_id, &100u128);
    controller.add_position_collateral_v3(&user, &position_id, &100u128);

    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &xlm_id,
        &900_000u128,
        &1_000_000u128,
    );
    controller.begin_liquidation_v3(&Address::generate(&env), &position_id);
    let quote = controller.preview_liquidation_v3(&position_id);
    assert_eq!(quote.collateral_underlying, 5_100u128);
    assert_eq!(quote.debt_amount, 4_500u128);
}

#[test]
fn test_open_position_v3_short_borrows_base_and_custodies_quote_collateral() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    usdt_vault.deposit(&user, &500u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &500u128);
    let (pool, pool_id, pool_tokens) = setup_perps_pool(&env, &usdt_id, &xlm_id);

    let position_id = controller.begin_open_position_v3(
        &user,
        &usdt_id,
        &xlm_id,
        &500u128,
        &10u128,
        &PositionSide::Short,
        &pool_tokens,
        &pool_id,
        &pool,
        &5_000u128,
    );
    controller.swap_open_position_v3(&user, &position_id);
    controller.activate_open_position_v3(&user, &position_id);

    let position = controller.get_position(&position_id).unwrap();
    assert_eq!(position.status, PositionStatus::Open);
    assert_eq!(position.side, PositionSide::Short);
    assert_eq!(position.collateral_asset, usdt_id);
    assert_eq!(position.debt_asset, xlm_id);
    assert_eq!(position.collateral_ptokens, 5_500u128);
    assert_eq!(controller.get_health_factor(&position_id), 2_000_000u128);

    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    assert_eq!(xlm_vault.get_margin_borrow_balance(&position_id), 5_000u128);
    assert_eq!(
        usdt_vault.get_ptoken_balance(&controller_id),
        position.collateral_ptokens
    );
    let pool_client = MockAquariusPoolClient::new(&env, &pool);
    assert_eq!(pool_client.get_last_swap_amount_in(), 5_000u128);
    assert_eq!(pool_client.get_last_user().unwrap(), controller_id);
}

#[test]
fn test_split_short_close_returns_unspent_quote_to_wallet() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, pool, _pool_id, _pool_tokens) = open_perps_short_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    assert_eq!(
        controller
            .get_pending_perps_close(&position_id)
            .unwrap()
            .collateral_underlying,
        5_500u128
    );

    env.cost_estimate().budget().reset_unlimited();
    controller.swap_close_short_position_v3(&user, &position_id, &5_005u128, &5_005u128);
    assert_budget_under(&env, 10_000_000, 2_000_000);
    assert_last_invocation_resources_under(&env, 98, 35, 20_000_000);
    assert_eq!(
        MockAquariusPoolClient::new(&env, &pool).get_last_swap_amount_in(),
        5_005u128
    );
    let (remainder, remainder_ttl) = env.as_contract(&controller_id, || {
        let key = DataKey::PendingPerpsCloseRemainder(position_id);
        (
            env.storage().persistent().get::<_, u128>(&key).unwrap(),
            env.storage().persistent().get_ttl(&key),
        )
    });
    assert_eq!(remainder, 495u128);
    assert!(remainder_ttl > TTL_THRESHOLD);

    let usdt_before_finish = MockTokenClient::new(&env, &usdt_id).balance(&user);
    let xlm_before_finish = MockTokenClient::new(&env, &xlm_id).balance(&user);
    env.cost_estimate().budget().reset_unlimited();
    controller.finish_close_position_v3(&position_id);
    assert_budget_under(&env, 15_000_000, 3_200_000);
    assert_last_invocation_resources_under(&env, 98, 45, 25_000_000);

    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    assert_eq!(xlm_vault.get_margin_borrow_balance(&position_id), 0u128);
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &usdt_id),
        0u128
    );
    assert_eq!(controller.get_margin_balance_ptokens(&user, &xlm_id), 0u128);
    assert_eq!(
        MockTokenClient::new(&env, &usdt_id).balance(&user),
        usdt_before_finish + 495i128
    );
    assert_eq!(
        MockTokenClient::new(&env, &xlm_id).balance(&user),
        xlm_before_finish + 5i128
    );
    assert!(controller.get_position(&position_id).is_none());
    assert!(env.as_contract(&controller_id, || env
        .storage()
        .persistent()
        .get::<_, u128>(&DataKey::PendingPerpsCloseRemainder(position_id))
        .is_none()));
}

#[test]
fn test_split_short_close_buffer_covers_delayed_max_rate_interest() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_short_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    let xlm_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    xlm_vault.set_borrow_rate(&10_000_000u128);

    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    controller.swap_close_short_position_v3(&user, &position_id, &5_005u128, &5_005u128);

    let usdt_before_finish = MockTokenClient::new(&env, &usdt_id).balance(&user);
    let xlm_before_finish = MockTokenClient::new(&env, &xlm_id).balance(&user);
    env.ledger()
        .with_mut(|ledger| ledger.timestamp = ledger.timestamp.saturating_add(30 * 60));
    controller.finish_close_position_v3(&position_id);

    assert_eq!(xlm_vault.get_margin_borrow_balance(&position_id), 0u128);
    assert!(controller.get_position(&position_id).is_none());
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &usdt_id),
        0u128
    );
    assert_eq!(
        MockTokenClient::new(&env, &usdt_id).balance(&user),
        usdt_before_finish + 495i128
    );
    assert!(MockTokenClient::new(&env, &xlm_id).balance(&user) > xlm_before_finish);
}

#[test]
fn test_short_close_rejects_full_swap_and_unsafe_partial_quotes() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, pool, _pool_id, _pool_tokens) = open_perps_short_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    assert!(controller
        .try_close_position_v3(&user, &position_id, &5_500u128)
        .is_err());
    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    assert!(controller
        .try_swap_close_position_v3(&user, &position_id, &5_500u128)
        .is_err());
    assert!(controller
        .try_swap_close_short_position_v3(&user, &position_id, &5_501u128, &5_000u128)
        .is_err());
    assert!(controller
        .try_swap_close_short_position_v3(&user, &position_id, &5_000u128, &5_000u128)
        .is_err());

    MockAquariusPoolClient::new(&env, &pool).set_payout_bps(&800_000u128);
    assert!(controller
        .try_swap_close_short_position_v3(&user, &position_id, &5_005u128, &5_005u128)
        .is_err());
    MockAquariusPoolClient::new(&env, &pool).set_payout_bps(&1_000_000u128);
    controller.swap_close_short_position_v3(&user, &position_id, &5_005u128, &5_005u128);
    controller.finish_close_position_v3(&position_id);
    assert!(controller.get_position(&position_id).is_none());
}

#[test]
fn test_partial_short_close_entrypoint_rejects_long_positions() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    assert!(controller
        .try_swap_close_short_position_v3(&user, &position_id, &4_500u128, &4_500u128)
        .is_err());
    controller.swap_close_position_v3(&user, &position_id, &4_750u128);
    controller.finish_close_position_v3(&position_id);
    assert!(controller.get_position(&position_id).is_none());
}

#[test]
fn test_liquidate_position_v3_uses_maintenance_margin_not_lending_cf() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    peridottroller.set_market_cf(&usdt_vault_id, &100_000u128);
    assert_eq!(controller.get_health_factor(&position_id), 2_000_000u128);
    assert!(controller
        .try_liquidate_position_v3(&Address::generate(&env), &position_id)
        .is_err());

    peridottroller.set_price(&xlm_id, &940_000u128, &1_000_000u128);
    let liquidator = Address::generate(&env);
    controller.liquidate_position_v3(&liquidator, &position_id);
    assert!(controller.get_position(&position_id).is_none());
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    assert_eq!(usdt_vault.get_margin_borrow_balance(&position_id), 0u128);
    assert!(MockTokenClient::new(&env, &usdt_id).balance(&liquidator) > 0i128);
}

#[test]
fn test_split_liquidate_position_v3_completes_across_stages() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, xlm_vault_id) =
        setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    // Production vaults use a dynamic rate model. Keep this budget assertion on
    // that path so a same-ledger simulation includes the model and token reads
    // needed if execution lands in the next ledger.
    let rate_model_id = env.register(MockRateModel, ());
    receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id).set_interest_model(&rate_model_id);

    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    peridottroller.set_price(&xlm_id, &940_000u128, &1_000_000u128);
    let liquidator = Address::generate(&env);

    env.cost_estimate().budget().reset_unlimited();
    controller.begin_liquidation_v3(&liquidator, &position_id);
    assert_last_invocation_resources_under(&env, 95, 35, 20_000_000);
    let expected_started = LiquidationStarted {
        position_id,
        liquidator: liquidator.clone(),
        owner: user.clone(),
        debt_amount: 4_500u128,
        takeover_after: env
            .ledger()
            .timestamp()
            .saturating_add(PENDING_LIQUIDATION_TTL_SECS),
    };
    assert_eq!(
        env.events()
            .all()
            .filter_by_contract(&controller_id)
            .events(),
        &[expected_started.to_xdr(&env, &controller_id)]
    );
    let pending = controller.get_pending_liquidation(&position_id).unwrap();
    assert_eq!(pending.kind, PendingLiquidationKind::PerpsV3);
    assert_eq!(pending.stage, PendingLiquidationStage::Started);
    let expires_at = controller
        .get_liquidation_takeover_after(&position_id)
        .unwrap();
    assert!(expires_at > env.ledger().timestamp());
    let pending_ttl = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::PendingLiquidation(position_id))
    });
    assert!(pending_ttl > TTL_THRESHOLD);
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Liquidated
    );

    assert!(controller
        .try_swap_liquidation_v3(&liquidator, &position_id, &1u128)
        .is_err());
    let position = controller.get_position(&position_id).unwrap();
    let position_rate =
        receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id).get_exchange_rate();
    let liquidation_min_out = position.collateral_ptokens.saturating_mul(position_rate) / SCALE_1E6;
    let quote = controller.preview_liquidation_v3(&position_id);
    assert_eq!(quote.collateral_underlying, 5_000u128);
    assert_eq!(quote.debt_amount, 4_500u128);
    assert_eq!(quote.oracle_min_out, 4_465u128);
    assert_eq!(quote.pool_estimated_out, 5_000u128);
    MockAquariusPoolClient::new(&env, &pool).set_payout_bps(&800_000u128);
    assert!(controller
        .try_swap_liquidation_v3(&liquidator, &position_id, &liquidation_min_out)
        .is_err());
    MockAquariusPoolClient::new(&env, &pool).set_payout_bps(&1_000_000u128);
    env.cost_estimate().budget().reset_unlimited();
    controller.swap_liquidation_v3(&liquidator, &position_id, &liquidation_min_out);
    assert_last_invocation_resources_under(&env, 100, 35, 20_000_000);
    let expected_swapped = LiquidationSwapped {
        position_id,
        liquidator: liquidator.clone(),
        collateral_underlying: 5_000u128,
        received_debt_asset: 5_000u128,
        takeover_after: env
            .ledger()
            .timestamp()
            .saturating_add(PENDING_LIQUIDATION_TTL_SECS),
    };
    assert_eq!(
        env.events()
            .all()
            .filter_by_contract(&controller_id)
            .events(),
        &[expected_swapped.to_xdr(&env, &controller_id)]
    );
    let pending = controller.get_pending_liquidation(&position_id).unwrap();
    assert_eq!(pending.stage, PendingLiquidationStage::CollateralConverted);
    assert!(pending.received_debt_asset > 0u128);
    assert_eq!(
        controller
            .get_position(&position_id)
            .unwrap()
            .collateral_ptokens,
        0u128
    );

    env.cost_estimate().budget().reset_unlimited();
    controller.finish_liquidation_v3(&liquidator, &position_id);
    assert_last_invocation_resources_under(&env, 100, 45, 25_000_000);
    let expected_finished = LiquidationFinished {
        position_id,
        liquidator: liquidator.clone(),
        owner: user.clone(),
        repaid: 4_500u128,
        bad_debt: 0u128,
        incentive: 45u128,
    };
    let expected_removed = PositionRemoved {
        owner: user.clone(),
        position_id,
        removed_at: env.ledger().timestamp(),
    };
    assert_eq!(
        env.events()
            .all()
            .filter_by_contract(&controller_id)
            .events(),
        &[
            expected_finished.to_xdr(&env, &controller_id),
            expected_removed.to_xdr(&env, &controller_id),
        ]
    );
    assert!(controller.get_pending_liquidation(&position_id).is_none());
    assert!(controller.get_position(&position_id).is_none());
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    assert_eq!(usdt_vault.get_margin_borrow_balance(&position_id), 0u128);
    assert!(MockTokenClient::new(&env, &usdt_id).balance(&liquidator) > 0i128);
}

#[test]
fn test_position_lifecycle_events_and_counter_support_indexer_bootstrap() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    usdt_vault.deposit(&user, &500u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &500u128);
    let (pool, pool_id, pool_tokens) = setup_perps_pool(&env, &usdt_id, &xlm_id);

    let position_id = controller.begin_open_position_v3(
        &user,
        &usdt_id,
        &xlm_id,
        &500u128,
        &10u128,
        &PositionSide::Long,
        &pool_tokens,
        &pool_id,
        &pool,
        &5_000u128,
    );
    let expected_created = PositionCreated {
        owner: user.clone(),
        position_id,
        mode: PositionMode::PerpsV3,
        status: PositionStatus::PendingOpen,
    };
    assert_eq!(
        env.events()
            .all()
            .filter_by_contract(&controller_id)
            .events(),
        &[expected_created.to_xdr(&env, &controller_id)]
    );
    assert_eq!(controller.get_position_counter(), position_id);

    controller.cancel_pending_open_v3(&user, &position_id);
    let expected_removed = PositionRemoved {
        owner: user,
        position_id,
        removed_at: env.ledger().timestamp(),
    };
    assert_eq!(
        env.events()
            .all()
            .filter_by_contract(&controller_id)
            .events(),
        &[expected_removed.to_xdr(&env, &controller_id)]
    );
}

#[test]
fn test_get_position_bumps_pending_liquidation_ttl() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _xlm_vid) =
        setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    peridottroller.set_price(&xlm_id, &940_000u128, &1_000_000u128);
    let liquidator = Address::generate(&env);
    controller.begin_liquidation_v3(&liquidator, &position_id);

    let position_key = DataKey::Position(position_id);
    let pending_key = DataKey::PendingLiquidation(position_id);
    let initial_pending_ttl = env.as_contract(&controller_id, || {
        env.storage().persistent().get_ttl(&pending_key)
    });
    env.ledger()
        .set_sequence_number(initial_pending_ttl.saturating_sub(10_000));

    let pending_ttl_before = env.as_contract(&controller_id, || {
        env.storage().persistent().get_ttl(&pending_key)
    });
    assert!(
        pending_ttl_before < TTL_THRESHOLD,
        "test setup expected pending liquidation TTL below bump threshold, got {pending_ttl_before}"
    );

    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Liquidated
    );

    env.as_contract(&controller_id, || {
        let position_ttl_after = env.storage().persistent().get_ttl(&position_key);
        let pending_ttl_after = env.storage().persistent().get_ttl(&pending_key);
        assert!(
            position_ttl_after > TTL_THRESHOLD,
            "expected bumped position TTL, got {position_ttl_after}"
        );
        assert!(
            pending_ttl_after > TTL_THRESHOLD,
            "expected bumped pending liquidation TTL, got {pending_ttl_after}"
        );
    });
}

#[test]
fn test_get_perps_position_bumps_position_mode_ttl() {
    let (env, controller_id, usdt_id, xlm_id, user, _peridottroller_id, usdt_vault_id, _xlm_vid) =
        setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    let mode_key = DataKey::PositionMode(position_id);
    let initial_mode_ttl = env.as_contract(&controller_id, || {
        env.storage().persistent().get_ttl(&mode_key)
    });
    env.ledger()
        .set_sequence_number(initial_mode_ttl.saturating_sub(10_000));

    let mode_ttl_before = env.as_contract(&controller_id, || {
        env.storage().persistent().get_ttl(&mode_key)
    });
    assert!(
        mode_ttl_before < TTL_THRESHOLD,
        "test setup expected position mode TTL below bump threshold, got {mode_ttl_before}"
    );

    assert!(controller.get_perps_position(&position_id).is_some());

    let mode_ttl_after = env.as_contract(&controller_id, || {
        env.storage().persistent().get_ttl(&mode_key)
    });
    assert!(
        mode_ttl_after > TTL_THRESHOLD,
        "expected bumped position mode TTL, got {mode_ttl_after}"
    );
}

#[test]
fn test_begin_liquidation_v3_recovers_missing_mode_before_mutation() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _xlm_vid) =
        setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &xlm_id,
        &940_000u128,
        &1_000_000u128,
    );
    env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::PositionMode(position_id));
    });

    let liquidator = Address::generate(&env);
    controller.begin_liquidation_v3(&liquidator, &position_id);
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Liquidated
    );
    assert!(controller.get_pending_liquidation(&position_id).is_some());
    let mode = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get::<_, PositionMode>(&DataKey::PositionMode(position_id))
    });
    assert_eq!(mode, Some(PositionMode::PerpsV3));
}

#[test]
fn test_split_liquidate_position_v3_can_be_taken_over_after_timeout() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        _usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &_usdt_vault_id,
        500u128,
    );

    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    peridottroller.set_price(&xlm_id, &940_000u128, &1_000_000u128);
    let first_liquidator = Address::generate(&env);
    let takeover_liquidator = Address::generate(&env);

    controller.begin_liquidation_v3(&first_liquidator, &position_id);
    let expires_at = controller
        .get_liquidation_takeover_after(&position_id)
        .unwrap();
    assert!(controller
        .try_swap_liquidation_v3(&takeover_liquidator, &position_id, &1u128)
        .is_err());

    env.ledger().set_timestamp(expires_at.saturating_add(1));
    let position = controller.get_position(&position_id).unwrap();
    let position_rate =
        receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id).get_exchange_rate();
    let liquidation_min_out = position.collateral_ptokens.saturating_mul(position_rate) / SCALE_1E6;
    controller.swap_liquidation_v3(&takeover_liquidator, &position_id, &liquidation_min_out);
    let pending = controller.get_pending_liquidation(&position_id).unwrap();
    assert_eq!(pending.liquidator, takeover_liquidator);
    assert_eq!(pending.stage, PendingLiquidationStage::CollateralConverted);

    controller.finish_liquidation_v3(&takeover_liquidator, &position_id);
    assert!(controller.get_position(&position_id).is_none());
    assert!(MockTokenClient::new(&env, &usdt_id).balance(&takeover_liquidator) > 0i128);
    assert_eq!(
        MockTokenClient::new(&env, &usdt_id).balance(&first_liquidator),
        0i128
    );
}

#[test]
fn test_split_liquidate_position_v3_missing_expiry_key_is_upgrade_compatible() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        _usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &_usdt_vault_id,
        500u128,
    );

    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    peridottroller.set_price(&xlm_id, &940_000u128, &1_000_000u128);
    let first_liquidator = Address::generate(&env);
    let takeover_liquidator = Address::generate(&env);
    controller.begin_liquidation_v3(&first_liquidator, &position_id);

    let legacy_expires_at = controller
        .get_liquidation_takeover_after(&position_id)
        .unwrap();
    env.as_contract(&controller_id, || {
        let mut pending = env
            .storage()
            .persistent()
            .get::<_, PendingLiquidation>(&DataKey::PendingLiquidation(position_id))
            .unwrap();
        pending.repay_amount = legacy_expires_at as u128;
        env.storage()
            .persistent()
            .set(&DataKey::PendingLiquidation(position_id), &pending);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingLiquidationTakeoverAfter(position_id));
    });

    let pending = controller.get_pending_liquidation(&position_id).unwrap();
    assert_eq!(pending.liquidator, first_liquidator);
    let expires_at = pending.repay_amount.min(u64::MAX as u128) as u64;
    assert_eq!(expires_at, legacy_expires_at);
    assert!(controller
        .get_liquidation_takeover_after(&position_id)
        .is_none());
    assert!(controller
        .try_swap_liquidation_v3(&takeover_liquidator, &position_id, &1u128)
        .is_err());
    env.ledger().set_timestamp(expires_at.saturating_add(1));

    let position = controller.get_position(&position_id).unwrap();
    let position_rate =
        receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id).get_exchange_rate();
    let liquidation_min_out = position.collateral_ptokens.saturating_mul(position_rate) / SCALE_1E6;
    controller.swap_liquidation_v3(&takeover_liquidator, &position_id, &liquidation_min_out);
    let pending = controller.get_pending_liquidation(&position_id).unwrap();
    assert_eq!(pending.liquidator, takeover_liquidator);
    let expires_at = controller
        .get_liquidation_takeover_after(&position_id)
        .unwrap();
    assert!(expires_at > env.ledger().timestamp());
}

#[test]
fn test_owner_close_position_v3_reverts_when_collateral_cannot_repay_debt() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    peridottroller.set_price(&xlm_id, &800_000u128, &1_000_000u128);
    MockAquariusPoolClient::new(&env, &pool).set_payout_bps(&800_000u128);

    assert!(controller
        .try_close_position_v3(&user, &position_id, &3_800u128)
        .is_err());

    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    assert_eq!(
        usdt_vault.get_margin_borrow_balance(&position_id),
        4_500u128
    );
    assert!(controller.get_position(&position_id).is_some());
}

#[test]
fn test_perps_v3_stress_repeated_open_close_liquidate_and_pending_recovery() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);

    let mut position_ids: Vec<u64> = Vec::new(&env);
    for _ in 0..8 {
        let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
            &env,
            &controller,
            &user,
            &usdt_id,
            &xlm_id,
            &usdt_vault_id,
            500u128,
        );
        assert_eq!(
            controller.get_position(&position_id).unwrap().status,
            PositionStatus::Open
        );
        assert_eq!(
            usdt_vault.get_margin_borrow_balance(&position_id),
            4_500u128
        );
        position_ids.push_back(position_id);
    }

    for i in 0..4u32 {
        let position_id = position_ids.get(i).unwrap();
        controller.repay_margin_position_v3(&user, &position_id, &100u128);
        assert_eq!(
            usdt_vault.get_margin_borrow_balance(&position_id),
            4_400u128
        );
        controller.close_position_v3(&user, &position_id, &4_750u128);
        assert!(controller.get_position(&position_id).is_none());
        assert_eq!(usdt_vault.get_margin_borrow_balance(&position_id), 0u128);
    }

    peridottroller.set_price(&xlm_id, &940_000u128, &1_000_000u128);
    for i in 4u32..position_ids.len() {
        let position_id = position_ids.get(i).unwrap();
        let liquidator = Address::generate(&env);
        controller.liquidate_position_v3(&liquidator, &position_id);
        assert!(controller.get_position(&position_id).is_none());
        assert_eq!(usdt_vault.get_margin_borrow_balance(&position_id), 0u128);
    }

    peridottroller.set_price(&xlm_id, &1_000_000u128, &1_000_000u128);
    let (pending_id, _pool, _pool_id, _pool_tokens) = begin_and_swap_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    peridottroller.set_price(&xlm_id, &800_000u128, &1_000_000u128);
    assert!(controller
        .try_activate_open_position_v3(&user, &pending_id)
        .is_err());
    let active_pending_liquidator = Address::generate(&env);
    assert!(controller
        .try_liquidate_position_v3(&active_pending_liquidator, &pending_id)
        .is_err());

    let pending = controller.get_pending_perps_open(&pending_id).unwrap();
    env.ledger()
        .with_mut(|l| l.timestamp = pending.expires_at.saturating_add(1));
    let liquidator = Address::generate(&env);
    controller.liquidate_position_v3(&liquidator, &pending_id);
    assert!(controller.get_position(&pending_id).is_none());
    assert!(controller.get_pending_perps_open(&pending_id).is_none());
    assert!(controller
        .get_pending_perps_open_execution(&pending_id)
        .is_none());
    assert_eq!(usdt_vault.get_margin_borrow_balance(&pending_id), 0u128);
    assert_eq!(controller.get_user_positions(&user).len(), 0u32);
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
fn test_swap_open_position_v3_budget_short_min() {
    let (env, controller_id, usdt_id, xlm_id, user, _pid, usdt_vault_id, _xlm_vault_id) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);

    usdt_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &200u128);
    let (pool, pool_id, pool_tokens) = setup_perps_pool(&env, &usdt_id, &xlm_id);
    let position_id = controller.begin_open_position_v3(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &10u128,
        &PositionSide::Long,
        &pool_tokens,
        &pool_id,
        &pool,
        &1_000u128,
    );

    env.cost_estimate().budget().reset_unlimited();
    controller.swap_open_position_v3(&user, &position_id);
    assert_budget_under(&env, 8_000_000, 1_600_000);
    assert_last_invocation_resources_under(&env, 90, 40, 22_000_000);
}

#[test]
fn test_activate_open_position_v3_budget_short_min() {
    let (env, controller_id, usdt_id, xlm_id, user, _pid, usdt_vault_id, _xlm_vault_id) =
        setup_short_min();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);

    usdt_vault.deposit(&user, &200u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &200u128);
    let (pool, pool_id, pool_tokens) = setup_perps_pool(&env, &usdt_id, &xlm_id);
    let position_id = controller.begin_open_position_v3(
        &user,
        &usdt_id,
        &xlm_id,
        &100u128,
        &10u128,
        &PositionSide::Long,
        &pool_tokens,
        &pool_id,
        &pool,
        &1_000u128,
    );
    controller.swap_open_position_v3(&user, &position_id);

    env.cost_estimate().budget().reset_unlimited();
    controller.activate_open_position_v3(&user, &position_id);
    assert_budget_under(&env, 8_000_000, 1_600_000);
    assert_last_invocation_resources_under(&env, 90, 40, 22_000_000);
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
#[should_panic(expected = "unsupported token decimals")]
fn test_set_market_rejects_unsupported_token_decimals() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1);

    let admin = Address::generate(&env);
    let token_id = env.register(MockToken, ());
    MockTokenClient::new(&env, &token_id).initialize(
        &"BAD".into_val(&env),
        &"BAD".into_val(&env),
        &6u32,
    );
    let vault_id = env.register(MockVault, ());
    MockVaultClient::new(&env, &vault_id).set_underlying_token(&token_id);

    let comp = env.register(MockPeridottroller, ());
    let swap = env.register(MockSwapAdapter, ());
    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.initialize(&admin, &comp, &swap, &3u128);
    controller.set_market(&admin, &token_id, &vault_id);
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
fn test_set_max_slippage_scaled() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let _controller = MarginControllerClient::new(&env, &controller_id);
    let admin = Address::generate(&env);

    let fresh_id = env.register(MarginController, ());
    let fresh = MarginControllerClient::new(&env, &fresh_id);
    let comp = Address::generate(&env);
    let swap = env.register(MockSwapAdapter, ());
    fresh.initialize(&admin, &comp, &swap, &3u128);
    fresh.set_max_slippage_scaled(&admin, &25_000u128);
}

#[test]
#[should_panic(expected = "invalid slippage")]
fn test_set_max_slippage_scaled_rejects_zero() {
    let (env, _controller_id, _, _, _, _, _, _) = setup();
    let admin = Address::generate(&env);
    let fresh_id = env.register(MarginController, ());
    let fresh = MarginControllerClient::new(&env, &fresh_id);
    let comp = Address::generate(&env);
    let swap = env.register(MockSwapAdapter, ());
    fresh.initialize(&admin, &comp, &swap, &3u128);
    fresh.set_max_slippage_scaled(&admin, &0u128);
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
#[should_panic(expected = "config timelocked")]
fn test_execute_peridottroller_update_rejects_before_timelock() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let admin: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    });
    let new_peridottroller = Address::generate(&env);

    controller.set_peridottroller(&admin, &new_peridottroller);
    controller.execute_peridottroller_update(&admin);
}

#[test]
fn test_execute_config_updates_after_timelock() {
    let (env, controller_id, _, _, _, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let admin: Address = env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    });
    let new_peridottroller = Address::generate(&env);
    let new_swap_adapter = env.register(MockSwapAdapter, ());

    controller.set_peridottroller(&admin, &new_peridottroller);
    controller.set_swap_adapter(&admin, &new_swap_adapter);
    env.ledger()
        .with_mut(|l| l.timestamp = 1u64.saturating_add(UPGRADE_TIMELOCK_SECS));

    env.cost_estimate().budget().reset_unlimited();
    controller.execute_peridottroller_update(&admin);
    controller.execute_swap_adapter_update(&admin);
    assert_budget_under(&env, 3_000_000, 600_000);

    let (stored_peridottroller, stored_swap): (Address, Address) =
        env.as_contract(&controller_id, || {
            (
                env.storage()
                    .persistent()
                    .get(&DataKey::Peridottroller)
                    .expect("peridottroller not set"),
                env.storage()
                    .persistent()
                    .get(&DataKey::SwapAdapter)
                    .expect("swap adapter not set"),
            )
        });
    assert_eq!(stored_peridottroller, new_peridottroller);
    assert_eq!(stored_swap, new_swap_adapter);
}

#[test]
fn test_get_user_positions_filters_missing_entries() {
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
fn test_close_position_v3_budget_does_not_scale_with_unrelated_positions() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    env.as_contract(&controller_id, || {
        let template: Position = env
            .storage()
            .persistent()
            .get(&DataKey::Position(position_id))
            .unwrap();
        let perps: PerpsPositionData = env
            .storage()
            .persistent()
            .get(&DataKey::PerpsPositionData(position_id))
            .unwrap();
        let collateral_vault: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PositionCollateralVault(position_id))
            .unwrap();
        let debt_vault: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PositionDebtVault(position_id))
            .unwrap();
        let position_vault: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PositionPositionVault(position_id))
            .unwrap();
        let mut positions: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::UserPositions(user.clone()))
            .unwrap();

        for offset in 1..MAX_USER_POSITIONS {
            let id = position_id.saturating_add(offset as u64);
            positions.push_back(id);
            env.storage()
                .persistent()
                .set(&DataKey::Position(id), &template);
            env.storage()
                .persistent()
                .set(&DataKey::PositionMode(id), &PositionMode::PerpsV3);
            env.storage()
                .persistent()
                .set(&DataKey::PositionCollateralVault(id), &collateral_vault);
            env.storage()
                .persistent()
                .set(&DataKey::PositionDebtVault(id), &debt_vault);
            env.storage()
                .persistent()
                .set(&DataKey::PositionPositionVault(id), &position_vault);
            env.storage()
                .persistent()
                .set(&DataKey::PerpsPositionData(id), &perps);
        }
        env.storage()
            .persistent()
            .set(&DataKey::UserPositions(user.clone()), &positions);
    });

    env.cost_estimate().budget().reset_unlimited();
    controller.begin_close_position_v3(&user, &position_id);
    assert_budget_under(&env, 10_000_000, 2_000_000);
    assert_last_invocation_resources_under(&env, 100, 30, 16_000_000);
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Open
    );
    assert_eq!(
        controller
            .get_pending_perps_close(&position_id)
            .unwrap()
            .collateral_underlying,
        0u128
    );

    env.cost_estimate().budget().reset_unlimited();
    controller.withdraw_close_position_v3(&user, &position_id);
    assert_budget_under(&env, 10_000_000, 2_000_000);
    assert_last_invocation_resources_under(&env, 90, 30, 16_000_000);
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Closing
    );
    assert!(controller.get_pending_perps_close(&position_id).is_some());

    env.cost_estimate().budget().reset_unlimited();
    controller.swap_close_position_v3(&user, &position_id, &4_750u128);
    assert_budget_under(&env, 10_000_000, 2_000_000);
    assert_last_invocation_resources_under(&env, 90, 25, 16_000_000);

    env.cost_estimate().budget().reset_unlimited();
    controller.finish_close_position_v3(&position_id);
    assert_budget_under(&env, 15_000_000, 3_200_000);
    assert_last_invocation_resources_under(&env, 90, 40, 20_000_000);

    let remaining = controller.get_user_positions(&user);
    assert_eq!(remaining.len(), MAX_USER_POSITIONS - 1);
    assert!(!remaining.contains(position_id));
}

#[test]
fn test_cancel_close_position_v3_restores_open_position() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    let original_ptokens = controller
        .get_position(&position_id)
        .unwrap()
        .collateral_ptokens;

    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    controller.cancel_close_position_v3(&user, &position_id);

    let restored = controller.get_position(&position_id).unwrap();
    assert_eq!(restored.status, PositionStatus::Open);
    assert_eq!(restored.collateral_ptokens, original_ptokens);
    assert!(controller.get_pending_perps_close(&position_id).is_none());
}

#[test]
fn test_expire_close_position_v3_restores_abandoned_preswap_close() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    let pending = controller.get_pending_perps_close(&position_id).unwrap();
    env.ledger()
        .with_mut(|ledger| ledger.timestamp = pending.expires_at.saturating_add(1));
    let liquidator = Address::generate(&env);
    assert!(controller
        .try_begin_liquidation_v3(&liquidator, &position_id)
        .is_err());
    controller.expire_close_position_v3(&position_id);

    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Open
    );
    assert!(controller.get_pending_perps_close(&position_id).is_none());
}

#[test]
fn test_underwater_split_close_can_be_cancelled_after_swap_reverts() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &xlm_id,
        &800_000u128,
        &1_000_000u128,
    );
    MockAquariusPoolClient::new(&env, &pool).set_payout_bps(&800_000u128);
    assert!(controller
        .try_swap_close_position_v3(&user, &position_id, &3_800u128)
        .is_err());
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Closing
    );
    assert_eq!(
        controller
            .get_pending_perps_close(&position_id)
            .unwrap()
            .received_debt_asset,
        0u128
    );

    controller.cancel_close_position_v3(&user, &position_id);
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Open
    );
}

#[test]
fn test_underwater_owner_close_can_be_superseded_by_liquidation() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &xlm_id,
        &800_000u128,
        &1_000_000u128,
    );

    controller.begin_close_position_v3(&user, &position_id);
    assert!(controller.get_pending_perps_close(&position_id).is_some());
    controller.withdraw_close_position_v3(&user, &position_id);
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Closing
    );
    let liquidator = Address::generate(&env);
    controller.begin_liquidation_v3(&liquidator, &position_id);
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Liquidated
    );
    assert!(controller.get_pending_perps_close(&position_id).is_none());
    assert!(controller.get_pending_liquidation(&position_id).is_some());
}

#[test]
fn test_liquidation_supersedes_unfunded_close_preparation() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    controller.begin_close_position_v3(&user, &position_id);
    assert!(controller.get_pending_perps_close(&position_id).is_some());

    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &xlm_id,
        &800_000u128,
        &1_000_000u128,
    );
    let liquidator = Address::generate(&env);
    controller.begin_liquidation_v3(&liquidator, &position_id);

    assert!(controller.get_pending_perps_close(&position_id).is_some());
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Liquidated
    );
    let quote = controller.preview_liquidation_v3(&position_id);
    controller.swap_liquidation_v3(&liquidator, &position_id, &quote.pool_estimated_out);
    controller.finish_liquidation_v3(&liquidator, &position_id);
    assert!(controller.get_position(&position_id).is_none());
    assert!(controller.get_pending_perps_close(&position_id).is_none());
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

// ─── Margin-fee helpers ───────────────────────────────────────────────────────

/// Returns (env, admin, controller_id, usdt_id, xlm_id, usdt_vault_id, xlm_vault_id)
/// using MockVault so tests control exchange rate / payout exactly.
fn setup_for_fees() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1);

    let admin = Address::generate(&env);

    let usdt_id = env.register(MockToken, ());
    let xlm_id = env.register(MockToken, ());
    MockTokenClient::new(&env, &usdt_id).initialize(
        &"USDT".into_val(&env),
        &"USDT".into_val(&env),
        &7u32,
    );
    MockTokenClient::new(&env, &xlm_id).initialize(
        &"XLM".into_val(&env),
        &"XLM".into_val(&env),
        &7u32,
    );

    let usdt_vault_id = env.register(MockVault, ());
    let xlm_vault_id = env.register(MockVault, ());
    MockVaultClient::new(&env, &usdt_vault_id).set_underlying_token(&usdt_id);
    MockVaultClient::new(&env, &xlm_vault_id).set_underlying_token(&xlm_id);

    let peridottroller_id = env.register(MockPeridottroller, ());
    let peridottroller = MockPeridottrollerClient::new(&env, &peridottroller_id);
    peridottroller.set_price(&usdt_id, &1_000_000u128, &1_000_000u128);
    peridottroller.set_price(&xlm_id, &1_000_000u128, &1_000_000u128);

    let swap_adapter_id = env.register(MockSwapAdapter, ());

    let controller_id = env.register(MarginController, ());
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.initialize(&admin, &peridottroller_id, &swap_adapter_id, &5u128);
    controller.set_market(&admin, &usdt_id, &usdt_vault_id);
    controller.set_market(&admin, &xlm_id, &xlm_vault_id);
    MockVaultClient::new(&env, &usdt_vault_id).set_margin_controller(&Some(controller_id.clone()));
    MockVaultClient::new(&env, &xlm_vault_id).set_margin_controller(&Some(controller_id.clone()));

    (
        env,
        admin,
        controller_id,
        usdt_id,
        xlm_id,
        usdt_vault_id,
        xlm_vault_id,
    )
}

// ─── Fee admin / caps ─────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn test_set_open_fee_bps_non_admin_panics() {
    let (env, _admin, controller_id, _usdt_id, _xlm_id, _usdt_vid, _xlm_vid) = setup_for_fees();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let not_admin = Address::generate(&env);
    controller.set_open_fee_bps(&not_admin, &100u128);
}

#[test]
#[should_panic]
fn test_set_open_fee_bps_over_cap_panics() {
    let (env, admin, controller_id, _usdt_id, _xlm_id, _usdt_vid, _xlm_vid) = setup_for_fees();
    let controller = MarginControllerClient::new(&env, &controller_id);
    // MAX_BASIS_FEE_BPS = 500; 501 should panic.
    controller.set_open_fee_bps(&admin, &501u128);
}

#[test]
#[should_panic]
fn test_set_close_fee_bps_over_cap_panics() {
    let (env, admin, controller_id, _usdt_id, _xlm_id, _usdt_vid, _xlm_vid) = setup_for_fees();
    let controller = MarginControllerClient::new(&env, &controller_id);
    controller.set_close_fee_bps(&admin, &501u128);
}

// ─── Open fee: deduction + LP distribution ────────────────────────────────────

#[test]
#[should_panic(expected = "margin fee overflow")]
fn test_get_claimable_margin_fees_reverts_on_pending_overflow() {
    let (env, _admin, controller_id, usdt_id, _xlm_id, usdt_vault_id, _xlm_vid) = setup_for_fees();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let user = Address::generate(&env);

    env.as_contract(&controller_id, || {
        env.storage().persistent().set(
            &DataKey::MarginBalancePtokens(user.clone(), usdt_vault_id.clone()),
            &2u128,
        );
        env.storage()
            .persistent()
            .set(&DataKey::TotalMarginPtokens(usdt_vault_id.clone()), &2u128);
        env.storage()
            .persistent()
            .set(&DataKey::MarginFeeIndex(usdt_vault_id.clone()), &u128::MAX);
    });

    controller.get_claimable_margin_fees(&user, &usdt_id);
}

#[test]
#[should_panic(expected = "margin fee overflow")]
fn test_claim_margin_fees_reverts_on_pending_overflow() {
    let (env, _admin, controller_id, usdt_id, _xlm_id, usdt_vault_id, _xlm_vid) = setup_for_fees();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let user = Address::generate(&env);

    env.as_contract(&controller_id, || {
        env.storage().persistent().set(
            &DataKey::MarginBalancePtokens(user.clone(), usdt_vault_id.clone()),
            &2u128,
        );
        env.storage()
            .persistent()
            .set(&DataKey::TotalMarginPtokens(usdt_vault_id.clone()), &2u128);
        env.storage()
            .persistent()
            .set(&DataKey::MarginFeeIndex(usdt_vault_id.clone()), &u128::MAX);
    });

    controller.claim_margin_fees(&user, &usdt_id);
}

// ─── Close fee: deduction from surplus ────────────────────────────────────────

// ─── Total pToken tracking ────────────────────────────────────────────────────

#[test]
fn test_margin_balance_read_bumps_total_margin_ptokens_ttl() {
    let (env, _admin, controller_id, usdt_id, _xlm_id, usdt_vault_id, _xlm_vid) = setup_for_fees();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = MockVaultClient::new(&env, &usdt_vault_id);
    let user = Address::generate(&env);

    usdt_vault.deposit(&user, &1_000u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &1_000u128);

    let total_key = DataKey::TotalMarginPtokens(usdt_vault_id.clone());
    let balance_key = DataKey::MarginBalancePtokens(user.clone(), usdt_vault_id.clone());
    let initial_total_ttl = env.as_contract(&controller_id, || {
        env.storage().persistent().get_ttl(&total_key)
    });
    env.ledger()
        .set_sequence_number(initial_total_ttl.saturating_sub(10_000));

    let total_ttl_before = env.as_contract(&controller_id, || {
        env.storage().persistent().get_ttl(&total_key)
    });
    assert!(
        total_ttl_before < 500_000,
        "test setup expected total TTL below bump threshold, got {total_ttl_before}"
    );

    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &usdt_id),
        1_000u128
    );

    env.as_contract(&controller_id, || {
        let total_ttl_after = env.storage().persistent().get_ttl(&total_key);
        let balance_ttl_after = env.storage().persistent().get_ttl(&balance_key);
        assert!(
            total_ttl_after > 500_000,
            "expected bumped total margin TTL, got {total_ttl_after}"
        );
        assert!(
            balance_ttl_after > 500_000,
            "expected bumped user margin balance TTL, got {balance_ttl_after}"
        );
    });
}

// ─── Orphan fee + sweep ───────────────────────────────────────────────────────

// ─── Accrual ordering: no back-pay on new deposit ────────────────────────────

#[test]
fn test_per_pair_execution_config_rejects_open_pool_oracle_divergence() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let admin = controller.get_admin();
    let defaults =
        controller.get_perps_pair_execution_config(&usdt_id, &xlm_id, &PositionSide::Long);
    assert_eq!(
        defaults.max_open_deviation_scaled,
        DEFAULT_OPEN_POOL_ORACLE_DEVIATION_SCALED
    );

    receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id).deposit(&user, &500u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &500u128);
    let (pool, pool_id, pool_tokens) = setup_perps_pool(&env, &usdt_id, &xlm_id);
    MockAquariusPoolClient::new(&env, &pool).set_quote_bps(&800_000u128);

    assert!(controller
        .try_begin_open_position_v3(
            &user,
            &usdt_id,
            &xlm_id,
            &500u128,
            &10u128,
            &PositionSide::Long,
            &pool_tokens,
            &pool_id,
            &pool,
            &3_800u128,
        )
        .is_err());
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &usdt_id),
        500u128
    );

    controller.set_perps_pair_execution_config(
        &admin,
        &usdt_id,
        &xlm_id,
        &PositionSide::Long,
        &PerpsPairExecutionConfig {
            max_open_deviation_scaled: 250_000u128,
            open_slippage_scaled: 50_000u128,
            close_slippage_scaled: 50_000u128,
            liquidation_slippage_scaled: 100_000u128,
        },
    );
    let configured =
        controller.get_perps_pair_execution_config(&usdt_id, &xlm_id, &PositionSide::Long);
    assert_eq!(configured.max_open_deviation_scaled, 250_000u128);
    assert_eq!(configured.open_slippage_scaled, 50_000u128);

    let position_id = controller.begin_open_position_v3(
        &user,
        &usdt_id,
        &xlm_id,
        &500u128,
        &10u128,
        &PositionSide::Long,
        &pool_tokens,
        &pool_id,
        &pool,
        &3_800u128,
    );
    assert!(controller.get_pending_perps_open(&position_id).is_some());
    controller.cancel_pending_open_v3(&user, &position_id);
    assert!(controller.get_position(&position_id).is_none());
}

#[test]
fn test_per_pair_configs_lazy_migrate_to_instance_storage() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        _user,
        _peridottroller_id,
        _usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let side = PositionSide::Long;
    let risk_key = DataKey::PerpsPairConfig(usdt_id.clone(), xlm_id.clone(), side.clone());
    let execution_key =
        DataKey::PerpsPairExecutionConfig(usdt_id.clone(), xlm_id.clone(), side.clone());
    let risk = PerpsPairConfig {
        max_leverage: 5u128,
        maintenance_margin_scaled: 75_000u128,
        liquidation_incentive_scaled: 20_000u128,
    };
    let execution = PerpsPairExecutionConfig {
        max_open_deviation_scaled: 25_000u128,
        open_slippage_scaled: 20_000u128,
        close_slippage_scaled: 30_000u128,
        liquidation_slippage_scaled: 40_000u128,
    };

    env.as_contract(&controller_id, || {
        env.storage().persistent().set(&risk_key, &risk);
        env.storage().persistent().set(&execution_key, &execution);
    });

    assert_eq!(
        controller
            .get_perps_pair_config(&usdt_id, &xlm_id, &side)
            .unwrap(),
        risk
    );
    assert_eq!(
        controller.get_perps_pair_execution_config(&usdt_id, &xlm_id, &side),
        execution
    );

    env.as_contract(&controller_id, || {
        assert_eq!(env.storage().instance().get(&risk_key), Some(risk.clone()));
        assert_eq!(
            env.storage().instance().get(&execution_key),
            Some(execution.clone())
        );
        env.storage().persistent().remove(&risk_key);
        env.storage().persistent().remove(&execution_key);
    });

    assert_eq!(
        controller
            .get_perps_pair_config(&usdt_id, &xlm_id, &side)
            .unwrap(),
        risk
    );
    assert_eq!(
        controller.get_perps_pair_execution_config(&usdt_id, &xlm_id, &side),
        execution
    );
}

#[test]
#[should_panic(expected = "too many perps pairs")]
fn test_perps_pair_instance_storage_enforces_pair_limit() {
    let env = Env::default();
    let controller_id = env.register(MarginController, ());
    let margin_asset = Address::generate(&env);
    let risk = PerpsPairConfig {
        max_leverage: 5,
        maintenance_margin_scaled: 50_000,
        liquidation_incentive_scaled: 10_000,
    };
    let execution = PerpsPairExecutionConfig {
        max_open_deviation_scaled: 100_000,
        open_slippage_scaled: 50_000,
        close_slippage_scaled: 50_000,
        liquidation_slippage_scaled: 100_000,
    };

    env.as_contract(&controller_id, || {
        for _ in 0..MAX_PERPS_PAIR_CONFIGS {
            let base_asset = Address::generate(&env);
            crate::helpers::set_perps_pair_config(
                &env,
                &margin_asset,
                &base_asset,
                &PositionSide::Long,
                &risk,
            );
            // A second policy for the same pair-side must not consume another slot.
            crate::helpers::set_perps_pair_execution_config(
                &env,
                &margin_asset,
                &base_asset,
                &PositionSide::Long,
                &execution,
            );
        }
        crate::helpers::set_perps_pair_config(
            &env,
            &margin_asset,
            &Address::generate(&env),
            &PositionSide::Long,
            &risk,
        );
    });
}

#[test]
fn test_v3_close_uses_pool_quote_when_pool_is_below_oracle() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, pool, _pool_id, _pool_tokens) = open_perps_long_with_leverage(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
        3u128,
    );
    let pool_client = MockAquariusPoolClient::new(&env, &pool);
    pool_client.set_quote_bps(&800_000u128);
    pool_client.set_payout_bps(&800_000u128);

    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    // 1,500 collateral quotes to 1,200; the 5% pool floor is 1,140.
    // The former oracle floor would have required 1,425 and blocked the close.
    controller.swap_close_position_v3(&user, &position_id, &1_140u128);
    controller.finish_close_position_v3(&position_id);

    assert!(controller.get_position(&position_id).is_none());
    assert_eq!(
        receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id)
            .get_margin_borrow_balance(&position_id),
        0u128
    );
}

#[test]
fn test_v3_open_rechecks_pool_kill_switch_before_borrowing() {
    let (env, controller_id, usdt_id, xlm_id, user, _, usdt_vault_id, _) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let usdt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    usdt_vault.deposit(&user, &500u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &500u128);
    let (pool, pool_id, pool_tokens) = setup_perps_pool(&env, &usdt_id, &xlm_id);

    let position_id = controller.begin_open_position_v3(
        &user,
        &usdt_id,
        &xlm_id,
        &500u128,
        &3u128,
        &PositionSide::Long,
        &pool_tokens,
        &pool_id,
        &pool,
        &1_500u128,
    );
    let adapter = MockSwapAdapterClient::new(&env, &controller_swap_adapter(&env, &controller_id));
    adapter.set_pool_allowed(&pool, &false);

    assert!(controller
        .try_swap_open_position_v3(&user, &position_id)
        .is_err());
    assert_eq!(usdt_vault.get_margin_borrow_balance(&position_id), 0u128);
    controller.cancel_pending_open_v3(&user, &position_id);
    assert!(controller.get_position(&position_id).is_none());
}

#[test]
fn test_admin_can_replace_disabled_position_pool_and_close() {
    let (env, controller_id, usdt_id, xlm_id, user, _, usdt_vault_id, _) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, old_pool, _, _) = open_perps_long_with_leverage(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
        3u128,
    );
    let (new_pool, new_pool_id, new_pool_tokens) = setup_perps_pool(&env, &usdt_id, &xlm_id);
    let admin = controller.get_admin();

    assert!(controller
        .try_replace_position_pool_v3(
            &admin,
            &position_id,
            &new_pool_tokens,
            &new_pool_id,
            &new_pool,
        )
        .is_err());

    let adapter = MockSwapAdapterClient::new(&env, &controller_swap_adapter(&env, &controller_id));
    adapter.set_pool_allowed(&old_pool, &false);
    assert!(controller
        .try_close_position_v3(&user, &position_id, &1_425u128)
        .is_err());
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Open
    );

    controller.replace_position_pool_v3(
        &admin,
        &position_id,
        &new_pool_tokens,
        &new_pool_id,
        &new_pool,
    );
    assert_eq!(
        controller.get_perps_position(&position_id).unwrap().pool,
        new_pool
    );
    controller.close_position_v3(&user, &position_id, &1_425u128);
    assert!(controller.get_position(&position_id).is_none());
}

#[test]
fn test_v3_liquidation_uses_pool_quote_and_absorbs_only_real_shortfall() {
    let (env, controller_id, usdt_id, xlm_id, user, peridottroller_id, usdt_vault_id, _) =
        setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    MockPeridottrollerClient::new(&env, &peridottroller_id).set_price(
        &xlm_id,
        &940_000u128,
        &1_000_000u128,
    );
    let pool_client = MockAquariusPoolClient::new(&env, &pool);
    pool_client.set_quote_bps(&800_000u128);
    pool_client.set_payout_bps(&800_000u128);
    // A pool return value is advisory. Settlement must use the 4,000 tokens
    // actually received rather than this deliberately inflated report.
    pool_client.set_reported_bps(&1_600_000u128);
    let liquidator = Address::generate(&env);

    controller.begin_liquidation_v3(&liquidator, &position_id);
    let adapter = MockSwapAdapterClient::new(&env, &controller_swap_adapter(&env, &controller_id));
    adapter.set_pool_allowed(&pool, &false);
    assert!(controller
        .try_swap_liquidation_v3(&liquidator, &position_id, &3_600u128)
        .is_err());

    let (replacement_pool, replacement_pool_id, replacement_pool_tokens) =
        setup_perps_pool(&env, &usdt_id, &xlm_id);
    let replacement_pool_client = MockAquariusPoolClient::new(&env, &replacement_pool);
    replacement_pool_client.set_quote_bps(&800_000u128);
    replacement_pool_client.set_payout_bps(&800_000u128);
    replacement_pool_client.set_reported_bps(&1_600_000u128);
    controller.replace_position_pool_v3(
        &controller.get_admin(),
        &position_id,
        &replacement_pool_tokens,
        &replacement_pool_id,
        &replacement_pool,
    );
    // 5,000 collateral quotes to 4,000; the 10% liquidation floor is 3,600.
    controller.swap_liquidation_v3(&liquidator, &position_id, &3_600u128);
    controller.finish_liquidation_v3(&liquidator, &position_id);

    let debt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    assert!(controller.get_position(&position_id).is_none());
    assert_eq!(debt_vault.get_margin_borrow_balance(&position_id), 0u128);
    assert_eq!(debt_vault.get_total_bad_debt(), 500u128);
}

#[test]
fn test_release_debt_free_open_v3_returns_ptokens_to_margin() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    let collateral_ptokens = controller
        .get_position(&position_id)
        .unwrap()
        .collateral_ptokens;
    let debt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    let debt = debt_vault.get_margin_borrow_balance(&position_id);
    controller.repay_margin_position_v3(&user, &position_id, &debt);
    assert_eq!(debt_vault.get_margin_borrow_balance(&position_id), 0u128);

    controller.release_debt_free_position_v3(&user, &position_id);
    assert!(controller.get_position(&position_id).is_none());
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &xlm_id),
        collateral_ptokens
    );
}

#[test]
fn test_release_debt_free_withdrawn_v3_redeposits_collateral() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_with_leverage(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
        3u128,
    );
    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    let debt_vault = receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id);
    let debt = debt_vault.get_margin_borrow_balance(&position_id);
    controller.repay_margin_position_v3(&user, &position_id, &debt);

    controller.release_debt_free_position_v3(&user, &position_id);
    assert!(controller.get_position(&position_id).is_none());
    assert!(controller.get_margin_balance_ptokens(&user, &xlm_id) > 0u128);
}

#[test]
fn test_finish_close_v3_commits_partial_repay_then_owner_can_top_up() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_short_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    let debt_vault = receipt_vault::ReceiptVaultClient::new(&env, &xlm_vault_id);
    debt_vault.set_borrow_rate(&10_000_000u128);
    controller.begin_close_position_v3(&user, &position_id);
    controller.withdraw_close_position_v3(&user, &position_id);
    controller.swap_close_short_position_v3(&user, &position_id, &5_005u128, &5_005u128);

    env.ledger()
        .with_mut(|ledger| ledger.timestamp = ledger.timestamp.saturating_add(7 * 24 * 60 * 60));
    controller.finish_close_position_v3(&position_id);
    let remaining = debt_vault.get_margin_borrow_balance(&position_id);
    assert!(remaining > 0u128);
    let pending = controller.get_pending_perps_close(&position_id).unwrap();
    assert_eq!(pending.received_debt_asset, 0u128);
    assert!(pending.debt_amount > 0u128);
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Closing
    );
    assert!(controller
        .try_expire_close_position_v3(&position_id)
        .is_err());

    MockTokenClient::new(&env, &xlm_id).mint(&user, &(remaining as i128));
    controller.repay_margin_position_v3(&user, &position_id, &remaining);
    controller.finish_close_position_v3(&position_id);
    assert!(controller.get_position(&position_id).is_none());
    assert_eq!(debt_vault.get_margin_borrow_balance(&position_id), 0u128);
}

#[test]
fn test_permissionless_v3_pending_open_expiry_and_resolution() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    receipt_vault::ReceiptVaultClient::new(&env, &usdt_vault_id).deposit(&user, &500u128);
    controller.transfer_spot_to_margin(&user, &usdt_id, &500u128);
    let (pool, pool_id, pool_tokens) = setup_perps_pool(&env, &usdt_id, &xlm_id);
    let pending_id = controller.begin_open_position_v3(
        &user,
        &usdt_id,
        &xlm_id,
        &500u128,
        &10u128,
        &PositionSide::Long,
        &pool_tokens,
        &pool_id,
        &pool,
        &5_000u128,
    );
    assert!(controller.try_expire_pending_open_v3(&pending_id).is_err());
    let expires_at = controller
        .get_pending_perps_open(&pending_id)
        .unwrap()
        .expires_at;
    env.ledger().set_timestamp(expires_at.saturating_add(1));
    controller.expire_pending_open_v3(&pending_id);
    assert!(controller.get_position(&pending_id).is_none());
    assert_eq!(
        controller.get_margin_balance_ptokens(&user, &usdt_id),
        500u128
    );

    let (swapped_id, _pool, _pool_id, _pool_tokens) = begin_and_swap_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );
    assert!(controller.try_resolve_pending_open_v3(&swapped_id).is_err());
    let expires_at = controller
        .get_pending_perps_open(&swapped_id)
        .unwrap()
        .expires_at;
    env.ledger().set_timestamp(expires_at.saturating_add(1));
    controller.resolve_pending_open_v3(&swapped_id);
    assert_eq!(
        controller.get_position(&swapped_id).unwrap().status,
        PositionStatus::Open
    );
}

#[test]
fn test_bounded_user_position_compaction_removes_only_scanned_stale_ids() {
    let (env, controller_id, usdt_id, xlm_id, user, _, _, _) = setup();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let mut ids = Vec::new(&env);
    for id in 1u64..=12u64 {
        ids.push_back(id);
    }
    let template = Position {
        owner: user.clone(),
        side: PositionSide::Long,
        collateral_asset: xlm_id.clone(),
        debt_asset: usdt_id.clone(),
        collateral_ptokens: 1u128,
        debt_shares: 0u128,
        entry_price_scaled: SCALE_1E6,
        opened_at: 0u64,
        status: PositionStatus::Open,
    };
    env.as_contract(&controller_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::UserPositions(user.clone()), &ids);
        for id in [2u64, 4u64, 6u64, 8u64, 10u64, 12u64] {
            env.storage()
                .persistent()
                .set(&DataKey::Position(id), &template);
        }
    });

    assert_eq!(controller.compact_user_positions(&user, &8u32), 8u32);
    assert_last_invocation_resources_under(&env, 60, 25, 10_000_000);
    assert_eq!(controller.compact_user_positions(&user, &8u32), 6u32);
    assert_eq!(
        controller.get_user_positions(&user),
        Vec::from_array(&env, [2u64, 4u64, 6u64, 8u64, 10u64, 12u64])
    );
}

#[test]
fn test_prepare_close_position_v3_fuses_begin_and_withdraw_under_budget() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_long_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    env.cost_estimate().budget().reset_unlimited();
    controller.prepare_close_position_v3(&user, &position_id);
    assert_last_invocation_resources_under(&env, 100, 45, 25_000_000);
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Closing
    );
    assert!(
        controller
            .get_pending_perps_close(&position_id)
            .unwrap()
            .collateral_underlying
            > 0u128
    );
}

#[test]
fn test_prepare_close_short_position_v3_fuses_begin_and_withdraw_under_budget() {
    let (
        env,
        controller_id,
        usdt_id,
        xlm_id,
        user,
        _peridottroller_id,
        usdt_vault_id,
        _xlm_vault_id,
    ) = setup_min_with_vaults();
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_unlimited();
    let controller = MarginControllerClient::new(&env, &controller_id);
    let (position_id, _pool, _pool_id, _pool_tokens) = open_perps_short_10x(
        &env,
        &controller,
        &user,
        &usdt_id,
        &xlm_id,
        &usdt_vault_id,
        500u128,
    );

    env.cost_estimate().budget().reset_unlimited();
    controller.prepare_close_position_v3(&user, &position_id);
    assert_last_invocation_resources_under(&env, 100, 45, 25_000_000);
    assert_eq!(
        controller.get_position(&position_id).unwrap().status,
        PositionStatus::Closing
    );
    assert!(
        controller
            .get_pending_perps_close(&position_id)
            .unwrap()
            .collateral_underlying
            > 0u128
    );
}
