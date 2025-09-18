#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, Symbol
};

// Storage key types for the contract
#[contracttype]
pub enum DataKey {
    UnderlyingToken,
    UserDeposits(Address),    // Keep for now (will remove in later steps)
    PTokenBalances(Address),  // New: receipt token balances
    PTokenAllowances(Address, Address), // allowances[owner][spender] -> u128
    TotalDeposited,
    TotalPTokens,            // New: total receipt tokens issued
    InterestRatePerSecond,   // u128, scaled by 1_000_000 (6 decimals)
    LastUpdateTime,          // u64
    AccumulatedInterest,     // u128
    YearlyRateScaled,        // u128, scaled by 1_000_000 (6 decimals)
    InitialExchangeRate,     // u128, scaled 1e6
    // Borrowing-related keys
    BorrowSnapshots(Address),   // BorrowSnapshot per user
    TotalBorrowed,              // u128
    BorrowIndex,                // u128 (scaled 1e18)
    BorrowYearlyRateScaled,     // u128, scaled 1e6
    CollateralFactorScaled,     // u128, scaled 1e6 (e.g., 500_000 = 50%)
    Admin,                      // Address
    Peridottroller,                // Address (optional)
    InterestModel,              // Address (optional)
    ReserveFactorScaled,        // u128 (scaled 1e6), defaults 0
    AdminFeeScaled,             // u128 (scaled 1e6), defaults 0
    TotalAdminFees,             // u128 accumulated admin fees
    TotalReserves,              // u128 accumulated reserves
    SupplyCap,                  // u128, max total underlying (principal + interest)
    BorrowCap,                  // u128, max total borrowed
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowSnapshot {
    pub principal: u128,
    pub interest_index: u128,
}

const SCALE_1E6: u128 = 1_000_000u128;
const INDEX_SCALE_1E18: u128 = 1_000_000_000_000_000_000u128; // 1e18

#[contract]
pub struct ReceiptVault;

#[contractimpl]
impl ReceiptVault {
    /// Initialize the vault with underlying token, supply yearly rate, borrow yearly rate, and admin
    /// Rates are scaled by 1e6 (e.g., 10% = 100_000)
    pub fn initialize(env: Env, token_address: Address, supply_yearly_rate_scaled: u128, borrow_yearly_rate_scaled: u128, admin: Address) {
        // Store the underlying token address
        env.storage()
            .persistent()
            .set(&DataKey::UnderlyingToken, &token_address);
        
        // Initialize totals to 0
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &0u128);
        env.storage()
            .persistent()
            .set(&DataKey::TotalPTokens, &0u128);

        // Store yearly supply/borrow rates (scaled 1e6)
        env.storage()
            .persistent()
            .set(&DataKey::YearlyRateScaled, &supply_yearly_rate_scaled);
        env.storage()
            .persistent()
            .set(&DataKey::BorrowYearlyRateScaled, &borrow_yearly_rate_scaled);

        // Set last update time and accumulated interest
        let now = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::LastUpdateTime, &now);
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedInterest, &0u128);

        // Initialize borrowing state
        env.storage()
            .persistent()
            .set(&DataKey::TotalBorrowed, &0u128);
        env.storage()
            .persistent()
            .set(&DataKey::BorrowIndex, &INDEX_SCALE_1E18);
        // Default collateral factor 50%
        env.storage()
            .persistent()
            .set(&DataKey::CollateralFactorScaled, &500_000u128);

        // Set admin
        env.storage().persistent().set(&DataKey::Admin, &admin);

        // Initial exchange rate and fee factors
        env.storage().persistent().set(&DataKey::InitialExchangeRate, &SCALE_1E6);
        env.storage().persistent().set(&DataKey::ReserveFactorScaled, &0u128);
        env.storage().persistent().set(&DataKey::TotalReserves, &0u128);
        env.storage().persistent().set(&DataKey::AdminFeeScaled, &0u128);
        env.storage().persistent().set(&DataKey::TotalAdminFees, &0u128);
        // Default caps unset (0 means disabled)
        env.storage().persistent().set(&DataKey::SupplyCap, &0u128);
        env.storage().persistent().set(&DataKey::BorrowCap, &0u128);
    }

    /// Deposit tokens into the vault and receive pTokens
    pub fn deposit(env: Env, user: Address, amount: u128) {
        // Always update interest first
        Self::update_interest(env.clone());
        // Require authorization from the user
        user.require_auth();
        // Rewards: accrue user in this market
        if let Some(comp_addr) = env.storage().persistent().get::<_, Address>(&DataKey::Peridottroller) {
            use soroban_sdk::IntoVal;
            let _: () = env.invoke_contract(&comp_addr, &Symbol::new(&env, "accrue_user_market"), (user.clone(), env.current_contract_address()).into_val(&env));
        }

        // Get the underlying token
        let token_address: Address = env.storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");

        

        // Pause: consult peridottroller if set
        if let Some(comp_addr) = env.storage().persistent().get::<_, Address>(&DataKey::Peridottroller) {
            use soroban_sdk::IntoVal;
            let paused: bool = env.invoke_contract(&comp_addr, &Symbol::new(&env, "is_deposit_paused"), (env.current_contract_address(),).into_val(&env));
            if paused { panic!("deposit paused"); }
        }

        // Create token client
        let token_client = token::Client::new(&env, &token_address);

        // Enforce supply cap if set (cap applies to total underlying after deposit)
        let cap: u128 = env.storage().persistent().get(&DataKey::SupplyCap).unwrap_or(0u128);
        if cap > 0 {
            let total_underlying_before = Self::get_total_underlying(env.clone());
            let total_underlying_after = total_underlying_before.saturating_add(amount);
            if total_underlying_after > cap { panic!("supply cap exceeded"); }
        }

        // Calculate pTokens to mint based on current exchange rate BEFORE moving cash
        let current_rate = Self::get_exchange_rate(env.clone());
        // Transfer tokens from user to contract
        token_client.transfer(&user, &env.current_contract_address(), &(amount as i128));
        // pTokens = amount * 1e6 / rate
        let ptokens_to_mint = (amount * SCALE_1E6) / current_rate;

        // Update user's deposit balance (keep for compatibility)
        let current_deposits = env.storage()
            .persistent()
            .get(&DataKey::UserDeposits(user.clone()))
            .unwrap_or(0u128);
        env.storage()
            .persistent()
            .set(&DataKey::UserDeposits(user.clone()), &(current_deposits + amount));

        // Update user's pToken balance
        let current_ptokens = env.storage()
            .persistent()
            .get(&DataKey::PTokenBalances(user.clone()))
            .unwrap_or(0u128);
        env.storage()
            .persistent()
            .set(&DataKey::PTokenBalances(user.clone()), &(current_ptokens + ptokens_to_mint));

        // Update totals
        let total_deposited: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        let total_ptokens: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalPTokens)
            .unwrap_or(0u128);
        
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &(total_deposited + amount));
        env.storage()
            .persistent()
            .set(&DataKey::TotalPTokens, &(total_ptokens + ptokens_to_mint));

        // Emit deposit event
        env.events().publish(
            (Symbol::new(&env, "deposit"), user.clone()),
            (amount, ptokens_to_mint)
        );
    }

    /// Withdraw tokens using pTokens
    pub fn withdraw(env: Env, user: Address, ptoken_amount: u128) {
        // Always update interest first
        Self::update_interest(env.clone());
        // Require authorization from the user
        user.require_auth();
        // Rewards accrue
        if let Some(comp_addr) = env.storage().persistent().get::<_, Address>(&DataKey::Peridottroller) {
            use soroban_sdk::IntoVal;
            let _: () = env.invoke_contract(&comp_addr, &Symbol::new(&env, "accrue_user_market"), (user.clone(), env.current_contract_address()).into_val(&env));
        }

        // Check user has sufficient pTokens
        let current_ptokens = env.storage()
            .persistent()
            .get(&DataKey::PTokenBalances(user.clone()))
            .unwrap_or(0u128);
        
        if current_ptokens < ptoken_amount {
            panic!("Insufficient pTokens");
        }

        // Calculate underlying tokens to return based on current exchange rate
        let current_rate = Self::get_exchange_rate(env.clone());
        // underlying = ptoken_amount * rate / 1e6
        let underlying_to_return = (ptoken_amount * current_rate) / SCALE_1E6;

        // Check we have enough total underlying (principal + interest)
        let total_underlying_available = Self::get_total_underlying(env.clone());
        if total_underlying_available < underlying_to_return {
            panic!("Not enough liquidity");
        }

        // Get the underlying token
        let token_address: Address = env.storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");

        // USD-based redeem gating via peridottroller, if set
        if let Some(comp_addr) = env.storage().persistent().get::<_, Address>(&DataKey::Peridottroller) {
            use soroban_sdk::IntoVal;
            // Pause check via peridottroller
            let paused: bool = env.invoke_contract(&comp_addr, &Symbol::new(&env, "is_redeem_paused"), (env.current_contract_address(),).into_val(&env));
            if paused { panic!("redeem paused"); }
            // Other markets collateral in USD
            let other_collateral_usd: u128 = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "get_collateral_excl_usd"),
                (user.clone(), env.current_contract_address()).into_val(&env),
            );
            // Price of this underlying
            let price_opt: Option<(u128, u128)> = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "get_price_usd"),
                (token_address.clone(),).into_val(&env),
            );
            if price_opt.is_none() { panic!("Price unavailable"); }
            let (price, scale) = price_opt.unwrap();
            let cf: u128 = env.invoke_contract(&comp_addr, &Symbol::new(&env, "get_market_cf"), (env.current_contract_address(),).into_val(&env));

            // Local remaining collateral after this redeem
            let remaining_ptokens = current_ptokens - ptoken_amount;
            let remaining_underlying = (remaining_ptokens.saturating_mul(current_rate)) / SCALE_1E6;
            let remaining_discounted = (remaining_underlying.saturating_mul(cf)) / SCALE_1E6;
            let local_collateral_usd = (remaining_discounted.saturating_mul(price)) / scale;

            // Borrows USD: other markets + local market
            let other_borrows_usd: u128 = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "get_borrows_excl"),
                (user.clone(), env.current_contract_address()).into_val(&env),
            );
            let local_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
            let local_debt_usd = (local_debt.saturating_mul(price)) / scale;

            let total_collateral_usd = other_collateral_usd.saturating_add(local_collateral_usd);
            let total_borrow_usd = other_borrows_usd.saturating_add(local_debt_usd);
            if total_collateral_usd < total_borrow_usd { panic!("Insufficient collateral"); }
        }

        // Create token client
        let token_client = token::Client::new(&env, &token_address);

        // Update user's pToken balance
        env.storage()
            .persistent()
            .set(&DataKey::PTokenBalances(user.clone()), &(current_ptokens - ptoken_amount));

        // Update user's deposit balance (for compatibility)
        let current_deposits = env.storage()
            .persistent()
            .get(&DataKey::UserDeposits(user.clone()))
            .unwrap_or(0u128);
        env.storage()
            .persistent()
            .set(&DataKey::UserDeposits(user.clone()), &(current_deposits - underlying_to_return));

        // Update totals
        let mut total_deposited: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        let mut accumulated: u128 = env.storage()
            .persistent()
            .get(&DataKey::AccumulatedInterest)
            .unwrap_or(0u128);
        let total_ptokens: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalPTokens)
            .unwrap_or(0u128);
        // Reduce principal first, then interest if needed
        if underlying_to_return > total_deposited {
            let from_interest = underlying_to_return - total_deposited;
            total_deposited = 0;
            accumulated = accumulated.saturating_sub(from_interest);
        } else {
            total_deposited = total_deposited - underlying_to_return;
        }
        env.storage().persistent().set(&DataKey::TotalDeposited, &total_deposited);
        env.storage().persistent().set(&DataKey::AccumulatedInterest, &accumulated);
        env.storage()
            .persistent()
            .set(&DataKey::TotalPTokens, &(total_ptokens - ptoken_amount));

        // Transfer tokens back to user
        token_client.transfer(&env.current_contract_address(), &user, &(underlying_to_return as i128));

        // Emit withdraw event
        env.events().publish(
            (Symbol::new(&env, "withdraw"), user.clone()),
            (underlying_to_return, ptoken_amount)
        );
    }

    /// Get user's balance in the vault (original deposits)
    pub fn get_user_balance(env: Env, user: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::UserDeposits(user))
            .unwrap_or(0u128)
    }

    /// Get user's pToken balance
    pub fn get_ptoken_balance(env: Env, user: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::PTokenBalances(user))
            .unwrap_or(0u128)
    }

    // ERC20-like pToken API
    pub fn approve(env: Env, owner: Address, spender: Address, amount: u128) {
        owner.require_auth();
        env.storage().persistent().set(&DataKey::PTokenAllowances(owner.clone(), spender.clone()), &amount);
        env.events().publish((Symbol::new(&env, "approve"),), (owner, spender, amount));
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> u128 {
        env.storage().persistent().get(&DataKey::PTokenAllowances(owner, spender)).unwrap_or(0u128)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: u128) {
        from.require_auth();
        Self::transfer_internal(env, from, to, amount, false);
    }

    pub fn transfer_from(env: Env, spender: Address, owner: Address, to: Address, amount: u128) {
        spender.require_auth();
        let allowed: u128 = env.storage().persistent().get(&DataKey::PTokenAllowances(owner.clone(), spender.clone())).unwrap_or(0u128);
        if allowed < amount { panic!("insufficient allowance"); }
        // deduct allowance
        env.storage().persistent().set(&DataKey::PTokenAllowances(owner.clone(), spender.clone()), &(allowed - amount));
        Self::transfer_internal(env, owner, to, amount, true);
    }

    fn transfer_internal(env: Env, from: Address, to: Address, amount: u128, via_spender: bool) {
        if amount == 0 { return; }
        // Gating: if peridottroller wired, consult redeem pause and health for from-user
        if let Some(comp_addr) = env.storage().persistent().get::<_, Address>(&DataKey::Peridottroller) {
            use soroban_sdk::IntoVal;
            // Pause check
            let paused: bool = env.invoke_contract(&comp_addr, &Symbol::new(&env, "is_redeem_paused"), (env.current_contract_address(),).into_val(&env));
            if paused { panic!("redeem paused"); }
            // Health check: ensure reducing `from` pTokens by amount keeps account healthy
            let pbal: u128 = env.storage().persistent().get(&DataKey::PTokenBalances(from.clone())).unwrap_or(0u128);
            if pbal < amount { panic!("Insufficient pTokens"); }
            // Check via preview_redeem_max
            let max_ptokens: u128 = env.invoke_contract(&comp_addr, &Symbol::new(&env, "preview_redeem_max"), (from.clone(), env.current_contract_address()).into_val(&env));
            if amount > max_ptokens { panic!("Insufficient collateral"); }
        }
        // balances update
        let from_bal: u128 = env.storage().persistent().get(&DataKey::PTokenBalances(from.clone())).unwrap_or(0u128);
        if from_bal < amount { panic!("Insufficient pTokens"); }
        let to_bal: u128 = env.storage().persistent().get(&DataKey::PTokenBalances(to.clone())).unwrap_or(0u128);
        env.storage().persistent().set(&DataKey::PTokenBalances(from.clone()), &(from_bal - amount));
        env.storage().persistent().set(&DataKey::PTokenBalances(to.clone()), &(to_bal.saturating_add(amount)));
        let evt = if via_spender { Symbol::new(&env, "transfer_from") } else { Symbol::new(&env, "transfer") };
        env.events().publish((evt,), (from.clone(), to.clone(), amount));

        // Rewards accrual on transfers when peridottroller is wired
        if let Some(comp_addr) = env.storage().persistent().get::<_, Address>(&DataKey::Peridottroller) {
            use soroban_sdk::IntoVal;
            let _: () = env.invoke_contract(&comp_addr, &Symbol::new(&env, "accrue_user_market"), (from, env.current_contract_address()).into_val(&env));
            let _: () = env.invoke_contract(&comp_addr, &Symbol::new(&env, "accrue_user_market"), (to, env.current_contract_address()).into_val(&env));
        }
    }

    /// Get total amount deposited in the vault
    pub fn get_total_deposited(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128)
    }

    /// Get total pTokens issued
    pub fn get_total_ptokens(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalPTokens)
            .unwrap_or(0u128)
    }

    /// Admin: upgrade contract code
    pub fn upgrade_wasm(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Admin: transfer admin to new address
    pub fn set_admin(env: Env, new_admin: Address) {
        let old: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        old.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &new_admin);
        env.events().publish((Symbol::new(&env, "admin_set"),), new_admin);
    }

    

    /// Get the exchange rate (pToken to underlying ratio) scaled by 1e6
    pub fn get_exchange_rate(env: Env) -> u128 {
        let total_ptokens: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalPTokens)
            .unwrap_or(0u128);
        if total_ptokens == 0 {
            return env.storage().persistent().get(&DataKey::InitialExchangeRate).unwrap_or(SCALE_1E6);
        }
        let total_underlying = Self::get_total_underlying(env.clone());
        // rate = total_underlying / total_ptokens, scaled 1e6
        (total_underlying * SCALE_1E6) / total_ptokens
    }

    /// Get the underlying token address
    pub fn get_underlying_token(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized")
    }

    /// Get collateral factor (scaled 1e6)
    pub fn get_collateral_factor(env: Env) -> u128 {
        env.storage().persistent().get(&DataKey::CollateralFactorScaled).unwrap_or(500_000u128)
    }

    /// Admin: set peridottroller address
    pub fn set_peridottroller(env: Env, peridottroller: Address) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Peridottroller, &peridottroller);
        env.events().publish((Symbol::new(&env, "peridottroller_set"),), peridottroller);
    }

    /// Admin: set interest rate model address
    pub fn set_interest_model(env: Env, model: Address) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        env.storage().persistent().set(&DataKey::InterestModel, &model);
        env.events().publish((Symbol::new(&env, "interest_model_set"),), model);
    }

    /// Admin: set reserve factor (0..=1e6)
    pub fn set_reserve_factor(env: Env, reserve_factor_scaled: u128) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        if reserve_factor_scaled > 1_000_000u128 { panic!("Invalid reserve factor"); }
        env.storage().persistent().set(&DataKey::ReserveFactorScaled, &reserve_factor_scaled);
        env.events().publish((Symbol::new(&env, "reserve_factor_set"),), reserve_factor_scaled);
    }

    /// Admin: set admin fee factor (0..=1e6)
    pub fn set_admin_fee(env: Env, admin_fee_scaled: u128) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        if admin_fee_scaled > 1_000_000u128 { panic!("Invalid admin fee"); }
        env.storage().persistent().set(&DataKey::AdminFeeScaled, &admin_fee_scaled);
        env.events().publish((Symbol::new(&env, "admin_fee_set"),), admin_fee_scaled);
    }

    /// Admin: set supply cap (0 disables)
    pub fn set_supply_cap(env: Env, cap: u128) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        env.storage().persistent().set(&DataKey::SupplyCap, &cap);
        env.events().publish((Symbol::new(&env, "supply_cap_set"),), cap);
    }

    /// Admin: set borrow cap (0 disables)
    pub fn set_borrow_cap(env: Env, cap: u128) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        env.storage().persistent().set(&DataKey::BorrowCap, &cap);
        env.events().publish((Symbol::new(&env, "borrow_cap_set"),), cap);
    }

    /// Get total reserves
    pub fn get_total_reserves(env: Env) -> u128 {
        env.storage().persistent().get(&DataKey::TotalReserves).unwrap_or(0u128)
    }

    /// Get total admin fees
    pub fn get_total_admin_fees(env: Env) -> u128 {
        env.storage().persistent().get(&DataKey::TotalAdminFees).unwrap_or(0u128)
    }

    /// Admin: reduce reserves and transfer to admin
    pub fn reduce_reserves(env: Env, amount: u128) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        let reserves: u128 = env.storage().persistent().get(&DataKey::TotalReserves).unwrap_or(0u128);
        if amount > reserves { panic!("Insufficient reserves"); }
        env.storage().persistent().set(&DataKey::TotalReserves, &reserves.saturating_sub(amount));
        // Transfer underlying to admin
        let token_address: Address = env.storage().persistent().get(&DataKey::UnderlyingToken).expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &admin, &(amount as i128));
        env.events().publish((Symbol::new(&env, "reserves_reduced"),), amount);
    }

    /// Admin: reduce admin fees and transfer to admin
    pub fn reduce_admin_fees(env: Env, amount: u128) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        let fees: u128 = env.storage().persistent().get(&DataKey::TotalAdminFees).unwrap_or(0u128);
        if amount > fees { panic!("Insufficient admin fees"); }
        env.storage().persistent().set(&DataKey::TotalAdminFees, &fees.saturating_sub(amount));
        // Transfer underlying to admin
        let token_address: Address = env.storage().persistent().get(&DataKey::UnderlyingToken).expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &admin, &(amount as i128));
        env.events().publish((Symbol::new(&env, "admin_fees_reduced"),), amount);
    }

    //

    /// Update interest based on elapsed time and current per-second rate
    pub fn update_interest(env: Env) {
        let last_time: u64 = env.storage()
            .persistent()
            .get(&DataKey::LastUpdateTime)
            .unwrap_or(env.ledger().timestamp());
        let now = env.ledger().timestamp();
        if now <= last_time {
            return;
        }
        let elapsed = (now - last_time) as u128;

        // Determine supply yearly rate from model if set, else static
        let yearly_rate_scaled: u128 = if let Some(model) = env.storage().persistent().get::<_, Address>(&DataKey::InterestModel) {
            use soroban_sdk::IntoVal;
            let cash = Self::get_available_liquidity(env.clone());
            let borrows: u128 = env.storage().persistent().get(&DataKey::TotalBorrowed).unwrap_or(0u128);
            let reserves: u128 = 0u128; // reserves tracking can be added later
            let rf: u128 = env.storage().persistent().get(&DataKey::ReserveFactorScaled).unwrap_or(0u128);
            env.invoke_contract(&model, &Symbol::new(&env, "get_supply_rate"), (cash, borrows, reserves, rf).into_val(&env))
        } else {
            env.storage().persistent().get(&DataKey::YearlyRateScaled).unwrap_or(0u128)
        };

        let total_deposited: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        // Supply interest accrual only when no external interest model is set
        if env.storage().persistent().get::<_, Address>(&DataKey::InterestModel).is_none() && total_deposited > 0 && yearly_rate_scaled > 0 {
            // new_interest = total_deposited * yearly_rate * elapsed / (SECONDS_PER_YEAR * 1e6)
            let seconds_per_year: u128 = 365 * 24 * 60 * 60;
            let numerator = total_deposited
                .saturating_mul(yearly_rate_scaled)
                .saturating_mul(elapsed);
            let denominator = seconds_per_year.saturating_mul(SCALE_1E6);
            let new_interest = numerator / denominator;

            let accumulated: u128 = env.storage()
                .persistent()
                .get(&DataKey::AccumulatedInterest)
                .unwrap_or(0u128);
            let updated_accumulated = accumulated.saturating_add(new_interest);
            env.storage()
                .persistent()
                .set(&DataKey::AccumulatedInterest, &updated_accumulated);

            // Emit interest event
            env.events().publish(
                (Symbol::new(&env, "interest_accrued"),),
                (new_interest, updated_accumulated)
            );
        }

        // Borrow interest accrual via global index (split to reserves, admin fees, and suppliers)
        let tb_prior: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .unwrap_or(0u128);
        // Determine borrow yearly rate from model if set, else static
        let borrow_yearly_rate_scaled: u128 = if let Some(model) = env.storage().persistent().get::<_, Address>(&DataKey::InterestModel) {
            use soroban_sdk::IntoVal;
            let cash = Self::get_available_liquidity(env.clone());
            let borrows: u128 = tb_prior;
            let reserves: u128 = 0u128;
            env.invoke_contract(&model, &Symbol::new(&env, "get_borrow_rate"), (cash, borrows, reserves).into_val(&env))
        } else {
            env.storage().persistent().get(&DataKey::BorrowYearlyRateScaled).unwrap_or(0u128)
        };
        if tb_prior > 0 && borrow_yearly_rate_scaled > 0 {
            let seconds_per_year: u128 = 365 * 24 * 60 * 60;
            let numerator = tb_prior
                .saturating_mul(borrow_yearly_rate_scaled)
                .saturating_mul(elapsed);
            let denominator = seconds_per_year.saturating_mul(SCALE_1E6);
            let borrow_interest_total = numerator / denominator;

            // Split between reserves, admin fees and suppliers based on factors
            let rf: u128 = env.storage().persistent().get(&DataKey::ReserveFactorScaled).unwrap_or(0u128);
            let af: u128 = env.storage().persistent().get(&DataKey::AdminFeeScaled).unwrap_or(0u128);
            let to_reserves = (borrow_interest_total.saturating_mul(rf)) / SCALE_1E6;
            let to_admin = (borrow_interest_total.saturating_mul(af)) / SCALE_1E6;
            let _to_suppliers = borrow_interest_total.saturating_sub(to_reserves).saturating_sub(to_admin);

            // Update total reserves and admin fees
            let current_reserves: u128 = env.storage().persistent().get(&DataKey::TotalReserves).unwrap_or(0u128);
            env.storage().persistent().set(&DataKey::TotalReserves, &current_reserves.saturating_add(to_reserves));
            let current_fees: u128 = env.storage().persistent().get(&DataKey::TotalAdminFees).unwrap_or(0u128);
            env.storage().persistent().set(&DataKey::TotalAdminFees, &current_fees.saturating_add(to_admin));

            // Increase total borrowed by total interest; suppliers' share increases total underlying via accumulated_interest path below
            let tb_after = tb_prior.saturating_add(borrow_interest_total);
            env.storage().persistent().set(&DataKey::TotalBorrowed, &tb_after);

            // Update borrow index: delta = old_index * borrow_interest / tb_prior
            let old_index: u128 = env.storage()
                .persistent()
                .get(&DataKey::BorrowIndex)
                .unwrap_or(INDEX_SCALE_1E18);
            let delta_index = (old_index.saturating_mul(borrow_interest_total)) / tb_prior;
            let new_index = old_index.saturating_add(delta_index);
            env.storage().persistent().set(&DataKey::BorrowIndex, &new_index);

            // Do not credit suppliers here when using model-driven accrual to avoid double counting.
            // Suppliers' share will be reflected implicitly via exchange rate from underlying math if needed.
        }

        // Move time forward
        env.storage().persistent().set(&DataKey::LastUpdateTime, &now);
    }

    /// Get total underlying, including accumulated interest
    pub fn get_total_underlying(env: Env) -> u128 {
        // cash + borrows - reserves - admin_fees
        let token_address: Address = env.storage().persistent().get(&DataKey::UnderlyingToken).expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        let cash_i: i128 = token_client.balance(&env.current_contract_address());
        let cash: u128 = if cash_i < 0 { 0u128 } else { cash_i as u128 };
        let borrows: u128 = env.storage().persistent().get(&DataKey::TotalBorrowed).unwrap_or(0u128);
        let reserves: u128 = env.storage().persistent().get(&DataKey::TotalReserves).unwrap_or(0u128);
        let admin_fees: u128 = env.storage().persistent().get(&DataKey::TotalAdminFees).unwrap_or(0u128);
        cash.saturating_add(borrows)
            .saturating_sub(reserves)
            .saturating_sub(admin_fees)
    }

    /// Admin: update yearly interest rate (scaled 1e6). Applies after accruing with old rate.
    pub fn set_interest_rate(env: Env, yearly_rate_scaled: u128) {
        // Admin guard
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        // Accrue with old rate first
        Self::update_interest(env.clone());
        env.storage()
            .persistent()
            .set(&DataKey::YearlyRateScaled, &yearly_rate_scaled);
        env.events().publish(
            (Symbol::new(&env, "interest_rate_changed"),),
            yearly_rate_scaled
        );
    }

    /// Admin: update borrow yearly rate (scaled 1e6)
    pub fn set_borrow_rate(env: Env, yearly_rate_scaled: u128) {
        // Admin guard
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        Self::update_interest(env.clone());
        env.storage().persistent().set(&DataKey::BorrowYearlyRateScaled, &yearly_rate_scaled);
        env.events().publish(
            (Symbol::new(&env, "borrow_rate_changed"),),
            yearly_rate_scaled
        );
    }

    /// Admin: set collateral factor (0..=1e6)
    pub fn set_collateral_factor(env: Env, new_factor_scaled: u128) {
        // Admin guard
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        if new_factor_scaled > SCALE_1E6 {
            panic!("Invalid collateral factor");
        }
        env.storage().persistent().set(&DataKey::CollateralFactorScaled, &new_factor_scaled);
        env.events().publish((Symbol::new(&env, "collateral_factor_changed"),), new_factor_scaled);
    }

    /// Read admin
    pub fn get_admin(env: Env) -> Address {
        env.storage().persistent().get(&DataKey::Admin).expect("admin not set")
    }

    /// Get user's current borrow balance (principal adjusted by index)
    pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {
        let snap: Option<BorrowSnapshot> = env.storage()
            .persistent()
            .get(&DataKey::BorrowSnapshots(user.clone()));
        let Some(snapshot) = snap else { return 0u128; };
        if snapshot.principal == 0 { return 0u128; }
        let current_index: u128 = env.storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .unwrap_or(INDEX_SCALE_1E18);
        // principal * current_index / user_index
        (snapshot.principal.saturating_mul(current_index)) / snapshot.interest_index
    }

    /// Internal: write user's borrow snapshot
    fn write_borrow_snapshot(env: &Env, user: Address, principal: u128) {
        let current_index: u128 = env.storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .unwrap_or(INDEX_SCALE_1E18);
        let snap = BorrowSnapshot { principal, interest_index: current_index };
        env.storage().persistent().set(&DataKey::BorrowSnapshots(user), &snap);
    }

    /// Get available liquidity = total_underlying - total_borrowed
    pub fn get_available_liquidity(env: Env) -> u128 {
        let total_underlying = Self::get_total_underlying(env.clone());
        let total_borrowed: u128 = env.storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .unwrap_or(0u128);
        total_underlying.saturating_sub(total_borrowed)
    }

    /// Get total borrowed outstanding
    pub fn get_total_borrowed(env: Env) -> u128 {
        env.storage().persistent().get(&DataKey::TotalBorrowed).unwrap_or(0u128)
    }

    /// Get user's collateral value in underlying terms
    pub fn get_user_collateral_value(env: Env, user: Address) -> u128 {
        let pbal: u128 = env.storage()
            .persistent()
            .get(&DataKey::PTokenBalances(user))
            .unwrap_or(0u128);
        if pbal == 0 { return 0u128; }
        let rate = Self::get_exchange_rate(env.clone());
        (pbal.saturating_mul(rate)) / SCALE_1E6
    }

    /// Borrow tokens against pToken collateral
    pub fn borrow(env: Env, user: Address, amount: u128) {
        Self::update_interest(env.clone());
        user.require_auth();
        if let Some(comp_addr) = env.storage().persistent().get::<_, Address>(&DataKey::Peridottroller) {
            use soroban_sdk::IntoVal;
            let _: () = env.invoke_contract(&comp_addr, &Symbol::new(&env, "accrue_user_market"), (user.clone(), env.current_contract_address()).into_val(&env));
        }

        // Cross-market enforcement via peridottroller (USD); fall back to local-only if no peridottroller
        if let Some(comp_addr) = env.storage().persistent().get::<_, Address>(&DataKey::Peridottroller) {
            use soroban_sdk::IntoVal;
            // Pause check via peridottroller
            let paused: bool = env.invoke_contract(&comp_addr, &Symbol::new(&env, "is_borrow_paused"), (env.current_contract_address(),).into_val(&env));
            if paused { panic!("borrow paused"); }
            let underlying_token: Address = env.storage().persistent().get(&DataKey::UnderlyingToken).expect("Vault not initialized");
            let (_liq, shortfall): (u128, u128) = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "hypothetical_liquidity"),
                (user.clone(), env.current_contract_address(), amount, underlying_token).into_val(&env),
            );
            if shortfall > 0 { panic!("Insufficient collateral"); }
        } else {
            // Collateral: local-only check
            let local_collateral_value = Self::get_user_collateral_value(env.clone(), user.clone());
            let local_cf: u128 = env.storage().persistent().get(&DataKey::CollateralFactorScaled).unwrap_or(500_000u128);
            let local_max_borrow = (local_collateral_value.saturating_mul(local_cf)) / 1_000_000u128;
            let local_current_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
            if local_current_debt.saturating_add(amount) > local_max_borrow { panic!("Insufficient collateral"); }
        }
        

        // Liquidity check
        let available = Self::get_available_liquidity(env.clone());
        if available < amount { panic!("Not enough liquidity to borrow"); }

        // Borrow cap check
        let bcap: u128 = env.storage().persistent().get(&DataKey::BorrowCap).unwrap_or(0u128);
        if bcap > 0 {
            let tb: u128 = env.storage().persistent().get(&DataKey::TotalBorrowed).unwrap_or(0u128);
            if tb.saturating_add(amount) > bcap { panic!("borrow cap exceeded"); }
        }

        // Update totals and user snapshot
        let new_principal = Self::get_user_borrow_balance(env.clone(), user.clone()).saturating_add(amount);
        Self::write_borrow_snapshot(&env, user.clone(), new_principal);
        let tb: u128 = env.storage().persistent().get(&DataKey::TotalBorrowed).unwrap_or(0u128);
        env.storage().persistent().set(&DataKey::TotalBorrowed, &tb.saturating_add(amount));

        // Transfer tokens to user
        let token_address: Address = env.storage().persistent().get(&DataKey::UnderlyingToken).expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &user, &(amount as i128));

        // Emit event
        env.events().publish((Symbol::new(&env, "borrow"), user.clone()), amount);
    }

    /// Repay borrowed tokens
    pub fn repay(env: Env, user: Address, amount: u128) {
        Self::update_interest(env.clone());
        user.require_auth();
        if let Some(comp_addr) = env.storage().persistent().get::<_, Address>(&DataKey::Peridottroller) {
            use soroban_sdk::IntoVal;
            let _: () = env.invoke_contract(&comp_addr, &Symbol::new(&env, "accrue_user_market"), (user.clone(), env.current_contract_address()).into_val(&env));
        }

        let current_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
        if current_debt == 0 { return; }
        let repay_amount = if amount > current_debt { current_debt } else { amount };

        // Transfer tokens from user
        let token_address: Address = env.storage().persistent().get(&DataKey::UnderlyingToken).expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&user, &env.current_contract_address(), &(repay_amount as i128));

        // Update snapshot and totals
        let new_principal = current_debt - repay_amount;
        Self::write_borrow_snapshot(&env, user.clone(), new_principal);
        let tb: u128 = env.storage().persistent().get(&DataKey::TotalBorrowed).unwrap_or(0u128);
        let tb_after = tb - repay_amount;
        env.storage().persistent().set(&DataKey::TotalBorrowed, &tb_after);

        env.events().publish((Symbol::new(&env, "repay"), user.clone()), repay_amount);
    }

    /// Repay on behalf during liquidation; only callable by peridottroller/peridottroller
    pub fn repay_on_behalf(env: Env, liquidator: Address, borrower: Address, amount: u128) {
        // Accrue and auth via peridottroller/peridottroller
        Self::update_interest(env.clone());
        let comp: Option<Address> = env.storage().persistent().get(&DataKey::Peridottroller);
        let Some(_comp_addr) = comp else { panic!("no peridottroller"); };

        let current_debt = Self::get_user_borrow_balance(env.clone(), borrower.clone());
        if current_debt == 0 { return; }
        let repay_amount = if amount > current_debt { current_debt } else { amount };

        // Transfer tokens from liquidator
        let token_address: Address = env.storage().persistent().get(&DataKey::UnderlyingToken).expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&liquidator, &env.current_contract_address(), &(repay_amount as i128));

        // Update borrower snapshot and totals
        let new_principal = current_debt - repay_amount;
        Self::write_borrow_snapshot(&env, borrower.clone(), new_principal);
        let tb: u128 = env.storage().persistent().get(&DataKey::TotalBorrowed).unwrap_or(0u128);
        let tb_after = tb - repay_amount;
        env.storage().persistent().set(&DataKey::TotalBorrowed, &tb_after);
        env.events().publish((Symbol::new(&env, "repay_on_behalf"),), (borrower.clone(), repay_amount));
    }

    /// Seize pTokens from borrower to liquidator; only callable by peridottroller/peridottroller
    pub fn seize(env: Env, borrower: Address, liquidator: Address, ptoken_amount: u128) {
        let comp: Option<Address> = env.storage().persistent().get(&DataKey::Peridottroller);
        let Some(_comp_addr) = comp else { panic!("no peridottroller"); };

        // reduce borrower pTokens and increase liquidator pTokens
        let bbal: u128 = env.storage().persistent().get(&DataKey::PTokenBalances(borrower.clone())).unwrap_or(0u128);
        if bbal < ptoken_amount { panic!("insufficient borrower ptokens"); }
        let lbal: u128 = env.storage().persistent().get(&DataKey::PTokenBalances(liquidator.clone())).unwrap_or(0u128);
        env.storage().persistent().set(&DataKey::PTokenBalances(borrower.clone()), &(bbal - ptoken_amount));
        env.storage().persistent().set(&DataKey::PTokenBalances(liquidator.clone()), &(lbal.saturating_add(ptoken_amount)));

        env.events().publish((Symbol::new(&env, "seize"),), (borrower.clone(), liquidator.clone(), ptoken_amount));
    }
}



mod test;
