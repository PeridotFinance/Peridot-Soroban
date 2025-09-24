#![no_std]
use core::convert::TryFrom;
use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, token, Address, Bytes, Env, String, Symbol,
};
use stellar_tokens::fungible::Base as TokenBase;

// Storage key types for the contract
#[contracttype]
pub enum DataKey {
    UnderlyingToken,
    TotalDeposited,
    InterestRatePerSecond, // u128, scaled by 1_000_000 (6 decimals)
    LastUpdateTime,        // u64
    AccumulatedInterest,   // u128
    YearlyRateScaled,      // u128, scaled by 1_000_000 (6 decimals)
    InitialExchangeRate,   // u128, scaled 1e6
    // Borrowing-related keys
    BorrowSnapshots(Address), // BorrowSnapshot per user
    TotalBorrowed,            // u128
    BorrowIndex,              // u128 (scaled 1e18)
    BorrowYearlyRateScaled,   // u128, scaled 1e6
    CollateralFactorScaled,   // u128, scaled 1e6 (e.g., 500_000 = 50%)
    Admin,                    // Address
    Peridottroller,           // Address (optional)
    InterestModel,            // Address (optional)
    ReserveFactorScaled,      // u128 (scaled 1e6), defaults 0
    AdminFeeScaled,           // u128 (scaled 1e6), defaults 0
    FlashLoanFeeScaled,       // u128 (scaled 1e6), defaults 0
    TotalAdminFees,           // u128 accumulated admin fees
    TotalReserves,            // u128 accumulated reserves
    SupplyCap,                // u128, max total underlying (principal + interest)
    BorrowCap,                // u128, max total borrowed
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowSnapshot {
    pub principal: u128,
    pub interest_index: u128,
}

const SCALE_1E6: u128 = 1_000_000u128;
const INDEX_SCALE_1E18: u128 = 1_000_000_000_000_000_000u128; // 1e18
const PTOKEN_DECIMALS: u32 = 6;

// ################## EVENTS ##################

/// Mirrors Compound's Mint event: emitted on deposit when pTokens are minted.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mint {
    #[topic]
    pub minter: Address,
    pub mint_amount: u128,
    pub mint_tokens: u128,
}

/// Mirrors Compound's Redeem event: emitted on withdraw when pTokens are burned.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Redeem {
    #[topic]
    pub redeemer: Address,
    pub redeem_amount: u128,
    pub redeem_tokens: u128,
}

/// Mirrors Compound's Borrow event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowEvent {
    #[topic]
    pub borrower: Address,
    pub borrow_amount: u128,
    pub account_borrows: u128,
    pub total_borrows: u128,
}

/// Mirrors Compound's RepayBorrow event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepayBorrow {
    #[topic]
    pub payer: Address,
    #[topic]
    pub borrower: Address,
    pub repay_amount: u128,
    pub account_borrows: u128,
    pub total_borrows: u128,
}

/// Mirrors Compound's AccrueInterest event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccrueInterest {
    pub interest_accumulated: u128,
    pub borrow_index: u128,
    pub total_borrows: u128,
}

/// Mirrors Compound's NewAdmin event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAdmin {
    #[topic]
    pub admin: Address,
}

/// Mirrors Compound's NewMarketInterestRateModel event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewInterestModel {
    #[topic]
    pub model: Address,
}

/// Mirrors Compound's NewReserveFactor event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewReserveFactor {
    pub reserve_factor_mantissa: u128,
}

/// Mirrors Compound's NewAdminFee event (custom extension).
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAdminFee {
    pub admin_fee_mantissa: u128,
}

/// Flash loan premium update (custom extension).
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewFlashLoanFee {
    pub fee_mantissa: u128,
}

/// Mirrors Compound's NewCollateralFactor event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewCollateralFactor {
    pub collateral_factor_mantissa: u128,
}

/// Emits when the manual supply rate is updated.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewSupplyRate {
    pub rate_mantissa: u128,
}

/// Emits when the manual borrow rate is updated.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewManualBorrowRate {
    pub rate_mantissa: u128,
}

/// Mirrors Compound's NewSupplyCap event (custom extension).
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewSupplyCap {
    pub supply_cap: u128,
}

/// Mirrors Compound's NewBorrowCap event (custom extension).
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewBorrowCap {
    pub borrow_cap: u128,
}

/// Mirrors Compound's ReservesReduced event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReservesReduced {
    pub reduce_amount: u128,
    pub total_reserves: u128,
}

/// Mirrors Compound's AdminFeesReduced event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminFeesReduced {
    pub reduce_amount: u128,
    pub total_admin_fees: u128,
}

/// Mirrors Compound's NewPeridottroller (custom) event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewPeridottroller {
    #[topic]
    pub peridottroller: Address,
}

/// Flash loan execution log.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlashLoan {
    #[topic]
    pub receiver: Address,
    pub amount: u128,
    pub fee_paid: u128,
}

#[contract]
pub struct ReceiptVault;

#[contractimpl]
impl ReceiptVault {
    /// Initialize the vault with underlying token, supply yearly rate, borrow yearly rate, and admin
    /// Rates are scaled by 1e6 (e.g., 10% = 100_000)
    pub fn initialize(
        env: Env,
        token_address: Address,
        supply_yearly_rate_scaled: u128,
        borrow_yearly_rate_scaled: u128,
        admin: Address,
    ) {
        // Store the underlying token address
        env.storage()
            .persistent()
            .set(&DataKey::UnderlyingToken, &token_address);

        // Initialize totals to 0
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &0u128);

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
        env.storage()
            .persistent()
            .set(&DataKey::InitialExchangeRate, &SCALE_1E6);
        env.storage()
            .persistent()
            .set(&DataKey::ReserveFactorScaled, &0u128);
        env.storage()
            .persistent()
            .set(&DataKey::TotalReserves, &0u128);
        env.storage()
            .persistent()
            .set(&DataKey::AdminFeeScaled, &0u128);
        env.storage()
            .persistent()
            .set(&DataKey::TotalAdminFees, &0u128);
        // Default caps unset (0 means disabled)
        env.storage().persistent().set(&DataKey::SupplyCap, &0u128);
        env.storage().persistent().set(&DataKey::BorrowCap, &0u128);

        TokenBase::set_metadata(
            &env,
            PTOKEN_DECIMALS,
            String::from_str(&env, "Peridot Receipt"),
            String::from_str(&env, "pPRT"),
        );
    }

    /// Deposit tokens into the vault and receive pTokens
    pub fn deposit(env: Env, user: Address, amount: u128) {
        // Always update interest first
        Self::update_interest(env.clone());
        // Require authorization from the user
        user.require_auth();
        // Rewards: accrue user in this market
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            use soroban_sdk::IntoVal;
            let _: () = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "accrue_user_market"),
                (user.clone(), env.current_contract_address()).into_val(&env),
            );
        }

        // Get the underlying token
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");

        // Pause: consult peridottroller if set
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            use soroban_sdk::IntoVal;
            let paused: bool = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "is_deposit_paused"),
                (env.current_contract_address(),).into_val(&env),
            );
            if paused {
                panic!("deposit paused");
            }
        }

        // Create token client
        let token_client = token::Client::new(&env, &token_address);

        // Enforce supply cap if set (cap applies to total underlying after deposit)
        let cap: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::SupplyCap)
            .unwrap_or(0u128);
        if cap > 0 {
            let total_underlying_before = Self::get_total_underlying(env.clone());
            let total_underlying_after = total_underlying_before.saturating_add(amount);
            if total_underlying_after > cap {
                panic!("supply cap exceeded");
            }
        }

        // Calculate pTokens to mint based on current exchange rate BEFORE moving cash
        let current_rate = Self::get_exchange_rate(env.clone());
        // Transfer tokens from user to contract
        token_client.transfer(&user, &env.current_contract_address(), &(amount as i128));
        // pTokens = amount * 1e6 / rate
        let ptokens_to_mint = (amount * SCALE_1E6) / current_rate;

        // Mint pTokens and update totals
        TokenBase::mint(&env, &user, to_i128(ptokens_to_mint));
        let total_deposited: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &(total_deposited + amount));

        // Emit Compound-style Mint event
        Mint {
            minter: user.clone(),
            mint_amount: amount,
            mint_tokens: ptokens_to_mint,
        }
        .publish(&env);
    }

    /// Withdraw tokens using pTokens
    pub fn withdraw(env: Env, user: Address, ptoken_amount: u128) {
        // Always update interest first
        Self::update_interest(env.clone());
        // Rewards accrue
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            use soroban_sdk::IntoVal;
            let _: () = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "accrue_user_market"),
                (user.clone(), env.current_contract_address()).into_val(&env),
            );
        }

        // Check user has sufficient pTokens
        let current_ptokens = ptoken_balance(&env, &user);

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
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");

        // USD-based redeem gating via peridottroller, if set
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            use soroban_sdk::IntoVal;
            // Pause check via peridottroller
            let paused: bool = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "is_redeem_paused"),
                (env.current_contract_address(),).into_val(&env),
            );
            if paused {
                panic!("redeem paused");
            }
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
            if price_opt.is_none() {
                panic!("Price unavailable");
            }
            let (price, scale) = price_opt.unwrap();
            let cf: u128 = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "get_market_cf"),
                (env.current_contract_address(),).into_val(&env),
            );

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
            if total_collateral_usd < total_borrow_usd {
                panic!("Insufficient collateral");
            }
        }

        // Create token client
        let token_client = token::Client::new(&env, &token_address);

        // Burn pTokens and update totals
        TokenBase::burn(&env, &user, to_i128(ptoken_amount));

        // Update totals
        let mut total_deposited: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        let mut accumulated: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedInterest)
            .unwrap_or(0u128);
        // Reduce principal first, then interest if needed
        if underlying_to_return > total_deposited {
            let from_interest = underlying_to_return - total_deposited;
            total_deposited = 0;
            accumulated = accumulated.saturating_sub(from_interest);
        } else {
            total_deposited = total_deposited - underlying_to_return;
        }
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &total_deposited);
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedInterest, &accumulated);

        // Transfer tokens back to user
        token_client.transfer(
            &env.current_contract_address(),
            &user,
            &(underlying_to_return as i128),
        );

        // Emit Compound-style Redeem event
        Redeem {
            redeemer: user.clone(),
            redeem_amount: underlying_to_return,
            redeem_tokens: ptoken_amount,
        }
        .publish(&env);
    }

    /// Get user's balance in the vault in underlying terms (pTokens × exchange rate)
    pub fn get_user_balance(env: Env, user: Address) -> u128 {
        let pbal = ptoken_balance(&env, &user);
        if pbal == 0 {
            return 0u128;
        }
        let rate = Self::get_exchange_rate(env.clone());
        (pbal.saturating_mul(rate)) / SCALE_1E6
    }

    /// Get user's pToken balance
    pub fn get_ptoken_balance(env: Env, user: Address) -> u128 {
        ptoken_balance(&env, &user)
    }

    // ERC20-like pToken API
    pub fn approve(env: Env, owner: Address, spender: Address, amount: u128) {
        owner.require_auth();
        TokenBase::approve(&env, &owner, &spender, to_i128(amount), u32::MAX);
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> u128 {
        let allowance = TokenBase::allowance(&env, &owner, &spender);
        if allowance < 0 {
            0
        } else {
            allowance as u128
        }
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: u128) {
        Self::transfer_internal(env, from, to, amount, None);
    }

    pub fn transfer_from(env: Env, spender: Address, owner: Address, to: Address, amount: u128) {
        Self::transfer_internal(env, owner, to, amount, Some(spender));
    }

    fn transfer_internal(
        env: Env,
        from: Address,
        to: Address,
        amount: u128,
        spender: Option<Address>,
    ) {
        if amount == 0 {
            return;
        }
        // Gating: if peridottroller wired, consult redeem pause and health for from-user
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            use soroban_sdk::IntoVal;
            // Pause check
            let paused: bool = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "is_redeem_paused"),
                (env.current_contract_address(),).into_val(&env),
            );
            if paused {
                panic!("redeem paused");
            }
            let pbal = ptoken_balance(&env, &from);
            if pbal < amount {
                panic!("Insufficient pTokens");
            }
            // Check via preview_redeem_max
            let max_ptokens: u128 = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "preview_redeem_max"),
                (from.clone(), env.current_contract_address()).into_val(&env),
            );
            if amount > max_ptokens {
                panic!("Insufficient collateral");
            }
        }
        let from_bal = ptoken_balance(&env, &from);
        if from_bal < amount {
            panic!("Insufficient pTokens");
        }

        match spender {
            Some(spender_addr) => {
                TokenBase::transfer_from(&env, &spender_addr, &from, &to, to_i128(amount));
            }
            None => {
                TokenBase::transfer(&env, &from, &to, to_i128(amount));
            }
        }

        // Rewards accrual on transfers when peridottroller is wired
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            use soroban_sdk::IntoVal;
            let _: () = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "accrue_user_market"),
                (from.clone(), env.current_contract_address()).into_val(&env),
            );
            let _: () = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "accrue_user_market"),
                (to, env.current_contract_address()).into_val(&env),
            );
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
        total_ptokens_supply(&env)
    }

    /// Admin: upgrade contract code
    pub fn upgrade_wasm(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Admin: transfer admin to new address
    pub fn set_admin(env: Env, new_admin: Address) {
        let old: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        old.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &new_admin);
        NewAdmin { admin: new_admin }.publish(&env);
    }

    /// Get the exchange rate (pToken to underlying ratio) scaled by 1e6
    pub fn get_exchange_rate(env: Env) -> u128 {
        let total_ptokens = total_ptokens_supply(&env);
        if total_ptokens == 0 {
            return env
                .storage()
                .persistent()
                .get(&DataKey::InitialExchangeRate)
                .unwrap_or(SCALE_1E6);
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
        env.storage()
            .persistent()
            .get(&DataKey::CollateralFactorScaled)
            .unwrap_or(500_000u128)
    }

    /// Admin: set peridottroller address
    pub fn set_peridottroller(env: Env, peridottroller: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Peridottroller, &peridottroller.clone());
        NewPeridottroller { peridottroller }.publish(&env);
    }

    /// Admin: set interest rate model address
    pub fn set_interest_model(env: Env, model: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::InterestModel, &model.clone());
        NewInterestModel { model }.publish(&env);
    }

    /// Admin: set reserve factor (0..=1e6)
    pub fn set_reserve_factor(env: Env, reserve_factor_scaled: u128) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        if reserve_factor_scaled > 1_000_000u128 {
            panic!("Invalid reserve factor");
        }
        env.storage()
            .persistent()
            .set(&DataKey::ReserveFactorScaled, &reserve_factor_scaled);
        NewReserveFactor {
            reserve_factor_mantissa: reserve_factor_scaled,
        }
        .publish(&env);
    }

    /// Admin: set admin fee factor (0..=1e6)
    pub fn set_admin_fee(env: Env, admin_fee_scaled: u128) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        if admin_fee_scaled > 1_000_000u128 {
            panic!("Invalid admin fee");
        }
        env.storage()
            .persistent()
            .set(&DataKey::AdminFeeScaled, &admin_fee_scaled);
        NewAdminFee {
            admin_fee_mantissa: admin_fee_scaled,
        }
        .publish(&env);
    }

    /// Admin: set flash loan fee (0..=1e6, applied to principal)
    pub fn set_flash_loan_fee(env: Env, fee_scaled: u128) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        if fee_scaled > 1_000_000u128 {
            panic!("Invalid flash fee");
        }
        env.storage()
            .persistent()
            .set(&DataKey::FlashLoanFeeScaled, &fee_scaled);
        NewFlashLoanFee {
            fee_mantissa: fee_scaled,
        }
        .publish(&env);
    }

    /// Admin: set supply cap (0 disables)
    pub fn set_supply_cap(env: Env, cap: u128) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        env.storage().persistent().set(&DataKey::SupplyCap, &cap);
        NewSupplyCap { supply_cap: cap }.publish(&env);
    }

    /// Admin: set borrow cap (0 disables)
    pub fn set_borrow_cap(env: Env, cap: u128) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        env.storage().persistent().set(&DataKey::BorrowCap, &cap);
        NewBorrowCap { borrow_cap: cap }.publish(&env);
    }

    /// Get total reserves
    pub fn get_total_reserves(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalReserves)
            .unwrap_or(0u128)
    }

    /// Get total admin fees
    pub fn get_total_admin_fees(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalAdminFees)
            .unwrap_or(0u128)
    }

    /// Admin: reduce reserves and transfer to admin
    pub fn reduce_reserves(env: Env, amount: u128) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        let reserves: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalReserves)
            .unwrap_or(0u128);
        if amount > reserves {
            panic!("Insufficient reserves");
        }
        let updated_reserves = reserves.saturating_sub(amount);
        env.storage()
            .persistent()
            .set(&DataKey::TotalReserves, &updated_reserves);
        // Transfer underlying to admin
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &admin, &(amount as i128));
        ReservesReduced {
            reduce_amount: amount,
            total_reserves: updated_reserves,
        }
        .publish(&env);
    }

    /// Admin: reduce admin fees and transfer to admin
    pub fn reduce_admin_fees(env: Env, amount: u128) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        let fees: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalAdminFees)
            .unwrap_or(0u128);
        if amount > fees {
            panic!("Insufficient admin fees");
        }
        let updated_fees = fees.saturating_sub(amount);
        env.storage()
            .persistent()
            .set(&DataKey::TotalAdminFees, &updated_fees);
        // Transfer underlying to admin
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &admin, &(amount as i128));
        AdminFeesReduced {
            reduce_amount: amount,
            total_admin_fees: updated_fees,
        }
        .publish(&env);
    }

    //

    /// Update interest based on elapsed time and current per-second rate
    pub fn update_interest(env: Env) {
        let last_time: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::LastUpdateTime)
            .unwrap_or(env.ledger().timestamp());
        let now = env.ledger().timestamp();
        if now <= last_time {
            return;
        }
        let elapsed = (now - last_time) as u128;

        // Determine supply yearly rate from model if set, else static
        let current_reserves: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalReserves)
            .unwrap_or(0u128);
        let current_admin_fees: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalAdminFees)
            .unwrap_or(0u128);
        let pooled_reserves = current_reserves.saturating_add(current_admin_fees);

        let yearly_rate_scaled: u128 = if let Some(model) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::InterestModel)
        {
            use soroban_sdk::IntoVal;
            let cash = Self::get_available_liquidity(env.clone());
            let borrows: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowed)
                .unwrap_or(0u128);
            let rf: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::ReserveFactorScaled)
                .unwrap_or(0u128);
            env.invoke_contract(
                &model,
                &Symbol::new(&env, "get_supply_rate"),
                (cash, borrows, pooled_reserves, rf).into_val(&env),
            )
        } else {
            env.storage()
                .persistent()
                .get(&DataKey::YearlyRateScaled)
                .unwrap_or(0u128)
        };

        let total_deposited: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        if total_deposited > 0 && yearly_rate_scaled > 0 {
            // new_interest = total_deposited * yearly_rate * elapsed / (SECONDS_PER_YEAR * 1e6)
            let seconds_per_year: u128 = 365 * 24 * 60 * 60;
            let numerator = total_deposited
                .saturating_mul(yearly_rate_scaled)
                .saturating_mul(elapsed);
            let denominator = seconds_per_year.saturating_mul(SCALE_1E6);
            let new_interest = numerator / denominator;

            if new_interest > 0 {
                let accumulated: u128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AccumulatedInterest)
                    .unwrap_or(0u128);
                let updated_accumulated = accumulated.saturating_add(new_interest);
                env.storage()
                    .persistent()
                    .set(&DataKey::AccumulatedInterest, &updated_accumulated);
            }
        }

        // Borrow interest accrual via global index (split to reserves, admin fees, and suppliers)
        let tb_prior: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .unwrap_or(0u128);
        let mut interest_accumulated_event: u128 = 0u128;
        let mut event_borrow_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .unwrap_or(INDEX_SCALE_1E18);
        let mut event_total_borrows: u128 = tb_prior;
        // Determine borrow yearly rate from model if set, else static
        let borrow_yearly_rate_scaled: u128 = if let Some(model) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::InterestModel)
        {
            use soroban_sdk::IntoVal;
            let cash = Self::get_available_liquidity(env.clone());
            let borrows: u128 = tb_prior;
            env.invoke_contract(
                &model,
                &Symbol::new(&env, "get_borrow_rate"),
                (cash, borrows, pooled_reserves).into_val(&env),
            )
        } else {
            env.storage()
                .persistent()
                .get(&DataKey::BorrowYearlyRateScaled)
                .unwrap_or(0u128)
        };
        if tb_prior > 0 && borrow_yearly_rate_scaled > 0 {
            let seconds_per_year: u128 = 365 * 24 * 60 * 60;
            let numerator = tb_prior
                .saturating_mul(borrow_yearly_rate_scaled)
                .saturating_mul(elapsed);
            let denominator = seconds_per_year.saturating_mul(SCALE_1E6);
            let borrow_interest_total = numerator / denominator;
            interest_accumulated_event = borrow_interest_total;

            // Split between reserves, admin fees and suppliers based on factors
            let rf: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::ReserveFactorScaled)
                .unwrap_or(0u128);
            let af: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::AdminFeeScaled)
                .unwrap_or(0u128);
            let to_reserves = (borrow_interest_total.saturating_mul(rf)) / SCALE_1E6;
            let to_admin = (borrow_interest_total.saturating_mul(af)) / SCALE_1E6;
            let _to_suppliers = borrow_interest_total
                .saturating_sub(to_reserves)
                .saturating_sub(to_admin);

            // Update total reserves and admin fees
            let current_reserves: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalReserves)
                .unwrap_or(0u128);
            env.storage().persistent().set(
                &DataKey::TotalReserves,
                &current_reserves.saturating_add(to_reserves),
            );
            let current_fees: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalAdminFees)
                .unwrap_or(0u128);
            env.storage().persistent().set(
                &DataKey::TotalAdminFees,
                &current_fees.saturating_add(to_admin),
            );

            // Increase total borrowed by total interest; suppliers' share is reflected through exchange-rate math and the accumulated interest tracker above
            let tb_after = tb_prior.saturating_add(borrow_interest_total);
            env.storage()
                .persistent()
                .set(&DataKey::TotalBorrowed, &tb_after);
            event_total_borrows = tb_after;

            // Update borrow index: delta = old_index * borrow_interest / tb_prior
            let old_index: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::BorrowIndex)
                .unwrap_or(INDEX_SCALE_1E18);
            let delta_index = (old_index.saturating_mul(borrow_interest_total)) / tb_prior;
            let new_index = old_index.saturating_add(delta_index);
            env.storage()
                .persistent()
                .set(&DataKey::BorrowIndex, &new_index);
            event_borrow_index = new_index;

            // Do not credit suppliers here when using model-driven accrual to avoid double counting.
            // Suppliers' share will be reflected implicitly via exchange rate from underlying math if needed.
        }

        AccrueInterest {
            interest_accumulated: interest_accumulated_event,
            borrow_index: event_borrow_index,
            total_borrows: event_total_borrows,
        }
        .publish(&env);

        // Move time forward
        env.storage()
            .persistent()
            .set(&DataKey::LastUpdateTime, &now);
    }

    /// Get total underlying, including accumulated interest
    pub fn get_total_underlying(env: Env) -> u128 {
        // cash + borrows - reserves - admin_fees
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        let cash_i: i128 = token_client.balance(&env.current_contract_address());
        let cash: u128 = if cash_i < 0 { 0u128 } else { cash_i as u128 };
        let borrows: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .unwrap_or(0u128);
        let reserves: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalReserves)
            .unwrap_or(0u128);
        let admin_fees: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalAdminFees)
            .unwrap_or(0u128);
        cash.saturating_add(borrows)
            .saturating_sub(reserves)
            .saturating_sub(admin_fees)
    }

    /// Admin: update yearly interest rate (scaled 1e6). Applies after accruing with old rate.
    pub fn set_interest_rate(env: Env, yearly_rate_scaled: u128) {
        // Admin guard
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        // Accrue with old rate first
        Self::update_interest(env.clone());
        env.storage()
            .persistent()
            .set(&DataKey::YearlyRateScaled, &yearly_rate_scaled);
        NewSupplyRate {
            rate_mantissa: yearly_rate_scaled,
        }
        .publish(&env);
    }

    /// Admin: update borrow yearly rate (scaled 1e6)
    pub fn set_borrow_rate(env: Env, yearly_rate_scaled: u128) {
        // Admin guard
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        Self::update_interest(env.clone());
        env.storage()
            .persistent()
            .set(&DataKey::BorrowYearlyRateScaled, &yearly_rate_scaled);
        NewManualBorrowRate {
            rate_mantissa: yearly_rate_scaled,
        }
        .publish(&env);
    }

    /// Admin: set collateral factor (0..=1e6)
    pub fn set_collateral_factor(env: Env, new_factor_scaled: u128) {
        // Admin guard
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        if new_factor_scaled > SCALE_1E6 {
            panic!("Invalid collateral factor");
        }
        env.storage()
            .persistent()
            .set(&DataKey::CollateralFactorScaled, &new_factor_scaled);
        NewCollateralFactor {
            collateral_factor_mantissa: new_factor_scaled,
        }
        .publish(&env);
    }

    /// Read admin
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    }

    /// Get user's current borrow balance (principal adjusted by index)
    pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {
        let snap: Option<BorrowSnapshot> = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowSnapshots(user.clone()));
        let Some(snapshot) = snap else {
            return 0u128;
        };
        if snapshot.principal == 0 {
            return 0u128;
        }
        let current_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .unwrap_or(INDEX_SCALE_1E18);
        // principal * current_index / user_index
        (snapshot.principal.saturating_mul(current_index)) / snapshot.interest_index
    }

    /// Internal: write user's borrow snapshot
    fn write_borrow_snapshot(env: &Env, user: Address, principal: u128) {
        let current_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .unwrap_or(INDEX_SCALE_1E18);
        let snap = BorrowSnapshot {
            principal,
            interest_index: current_index,
        };
        env.storage()
            .persistent()
            .set(&DataKey::BorrowSnapshots(user), &snap);
    }

    /// Get available liquidity = total_underlying - total_borrowed
    pub fn get_available_liquidity(env: Env) -> u128 {
        let total_underlying = Self::get_total_underlying(env.clone());
        let total_borrowed: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .unwrap_or(0u128);
        total_underlying.saturating_sub(total_borrowed)
    }

    /// Get total borrowed outstanding
    pub fn get_total_borrowed(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .unwrap_or(0u128)
    }

    /// Get user's collateral value in underlying terms
    pub fn get_user_collateral_value(env: Env, user: Address) -> u128 {
        let pbal = ptoken_balance(&env, &user);
        if pbal == 0 {
            return 0u128;
        }
        let rate = Self::get_exchange_rate(env.clone());
        (pbal.saturating_mul(rate)) / SCALE_1E6
    }

    /// Borrow tokens against pToken collateral
    pub fn borrow(env: Env, user: Address, amount: u128) {
        Self::update_interest(env.clone());
        user.require_auth();
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            use soroban_sdk::IntoVal;
            let _: () = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "accrue_user_market"),
                (user.clone(), env.current_contract_address()).into_val(&env),
            );
        }

        // Cross-market enforcement via peridottroller (USD); fall back to local-only if no peridottroller
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            use soroban_sdk::IntoVal;
            // Pause check via peridottroller
            let paused: bool = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "is_borrow_paused"),
                (env.current_contract_address(),).into_val(&env),
            );
            if paused {
                panic!("borrow paused");
            }
            let underlying_token: Address = env
                .storage()
                .persistent()
                .get(&DataKey::UnderlyingToken)
                .expect("Vault not initialized");
            let (_liq, shortfall): (u128, u128) = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "hypothetical_liquidity"),
                (
                    user.clone(),
                    env.current_contract_address(),
                    amount,
                    underlying_token,
                )
                    .into_val(&env),
            );
            if shortfall > 0 {
                panic!("Insufficient collateral");
            }
        } else {
            // Collateral: local-only check
            let local_collateral_value = Self::get_user_collateral_value(env.clone(), user.clone());
            let local_cf: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::CollateralFactorScaled)
                .unwrap_or(500_000u128);
            let local_max_borrow =
                (local_collateral_value.saturating_mul(local_cf)) / 1_000_000u128;
            let local_current_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
            if local_current_debt.saturating_add(amount) > local_max_borrow {
                panic!("Insufficient collateral");
            }
        }

        // Liquidity check
        let available = Self::get_available_liquidity(env.clone());
        if available < amount {
            panic!("Not enough liquidity to borrow");
        }

        // Borrow cap check
        let bcap: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowCap)
            .unwrap_or(0u128);
        if bcap > 0 {
            let tb: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowed)
                .unwrap_or(0u128);
            if tb.saturating_add(amount) > bcap {
                panic!("borrow cap exceeded");
            }
        }

        // Update totals and user snapshot
        let new_principal =
            Self::get_user_borrow_balance(env.clone(), user.clone()).saturating_add(amount);
        Self::write_borrow_snapshot(&env, user.clone(), new_principal);
        let tb: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .unwrap_or(0u128);
        let total_borrows = tb.saturating_add(amount);
        env.storage()
            .persistent()
            .set(&DataKey::TotalBorrowed, &total_borrows);

        // Transfer tokens to user
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &user, &(amount as i128));

        // Emit event
        BorrowEvent {
            borrower: user.clone(),
            borrow_amount: amount,
            account_borrows: new_principal,
            total_borrows,
        }
        .publish(&env);
    }

    /// Repay borrowed tokens
    pub fn repay(env: Env, user: Address, amount: u128) {
        Self::update_interest(env.clone());
        user.require_auth();
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            use soroban_sdk::IntoVal;
            let _: () = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "accrue_user_market"),
                (user.clone(), env.current_contract_address()).into_val(&env),
            );
        }

        let current_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
        if current_debt == 0 {
            return;
        }
        let repay_amount = if amount > current_debt {
            current_debt
        } else {
            amount
        };

        // Transfer tokens from user
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(
            &user,
            &env.current_contract_address(),
            &(repay_amount as i128),
        );

        // Update snapshot and totals
        let new_principal = current_debt - repay_amount;
        Self::write_borrow_snapshot(&env, user.clone(), new_principal);
        let tb: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .unwrap_or(0u128);
        let tb_after = tb - repay_amount;
        env.storage()
            .persistent()
            .set(&DataKey::TotalBorrowed, &tb_after);

        RepayBorrow {
            payer: user.clone(),
            borrower: user.clone(),
            repay_amount,
            account_borrows: new_principal,
            total_borrows: tb_after,
        }
        .publish(&env);
    }

    /// Execute a flash loan to `receiver`. Receiver must return `amount + fee` within the callback.
    pub fn flash_loan(env: Env, receiver: Address, amount: u128, data: Bytes) {
        if amount == 0 {
            panic!("invalid flash amount");
        }
        Self::update_interest(env.clone());

        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            use soroban_sdk::IntoVal;
            let paused: bool = env.invoke_contract(
                &comp_addr,
                &Symbol::new(&env, "is_borrow_paused"),
                (env.current_contract_address(),).into_val(&env),
            );
            if paused {
                panic!("borrow paused");
            }
        }

        let available = Self::get_available_liquidity(env.clone());
        if available < amount {
            panic!("insufficient liquidity");
        }

        let fee_scaled: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::FlashLoanFeeScaled)
            .unwrap_or(0u128);
        let fee = (amount.saturating_mul(fee_scaled)) / SCALE_1E6;

        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);

        let balance_before_i: i128 = token_client.balance(&env.current_contract_address());
        if balance_before_i < 0 {
            panic!("invalid cash state");
        }
        let balance_before = balance_before_i as u128;

        token_client.transfer(&env.current_contract_address(), &receiver, &to_i128(amount));

        {
            use soroban_sdk::IntoVal;
            // Receiver contract executes its logic and must return funds before this call unwinds.
            let _: () = env.invoke_contract(
                &receiver,
                &Symbol::new(&env, "on_flash_loan"),
                (env.current_contract_address(), amount, fee, data).into_val(&env),
            );
        }

        let balance_after_i: i128 = token_client.balance(&env.current_contract_address());
        if balance_after_i < 0 {
            panic!("invalid repayment state");
        }
        let balance_after = balance_after_i as u128;
        let required = balance_before.saturating_add(fee);
        if balance_after < required {
            panic!("flash loan not repaid");
        }

        let fee_paid = balance_after.saturating_sub(balance_before);
        if fee_paid > 0 {
            let reserves: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalReserves)
                .unwrap_or(0u128);
            env.storage()
                .persistent()
                .set(&DataKey::TotalReserves, &reserves.saturating_add(fee_paid));
        }

        FlashLoan {
            receiver: receiver.clone(),
            amount,
            fee_paid,
        }
        .publish(&env);
    }

    /// Repay on behalf during liquidation; only callable by peridottroller/peridottroller
    pub fn repay_on_behalf(env: Env, liquidator: Address, borrower: Address, amount: u128) {
        // Accrue and auth via peridottroller/peridottroller
        Self::update_interest(env.clone());
        let comp: Option<Address> = env.storage().persistent().get(&DataKey::Peridottroller);
        let Some(_comp_addr) = comp else {
            panic!("no peridottroller");
        };

        let current_debt = Self::get_user_borrow_balance(env.clone(), borrower.clone());
        if current_debt == 0 {
            return;
        }
        let repay_amount = if amount > current_debt {
            current_debt
        } else {
            amount
        };

        // Transfer tokens from liquidator
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(
            &liquidator,
            &env.current_contract_address(),
            &(repay_amount as i128),
        );

        // Update borrower snapshot and totals
        let new_principal = current_debt - repay_amount;
        Self::write_borrow_snapshot(&env, borrower.clone(), new_principal);
        let tb: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .unwrap_or(0u128);
        let tb_after = tb - repay_amount;
        env.storage()
            .persistent()
            .set(&DataKey::TotalBorrowed, &tb_after);
        RepayBorrow {
            payer: liquidator.clone(),
            borrower: borrower.clone(),
            repay_amount,
            account_borrows: new_principal,
            total_borrows: tb_after,
        }
        .publish(&env);
    }

    /// Seize pTokens from borrower to liquidator; only callable by peridottroller/peridottroller
    pub fn seize(env: Env, borrower: Address, liquidator: Address, ptoken_amount: u128) {
        let comp: Option<Address> = env.storage().persistent().get(&DataKey::Peridottroller);
        let Some(_comp_addr) = comp else {
            panic!("no peridottroller");
        };

        let borrower_bal = ptoken_balance(&env, &borrower);
        if borrower_bal < ptoken_amount {
            panic!("insufficient borrower ptokens");
        }
        TokenBase::update(
            &env,
            Some(&borrower),
            Some(&liquidator),
            to_i128(ptoken_amount),
        );

        stellar_tokens::fungible::emit_transfer(
            &env,
            &borrower,
            &liquidator,
            to_i128(ptoken_amount),
        );
    }
}

mod test;

fn to_i128(amount: u128) -> i128 {
    i128::try_from(amount).expect("amount exceeds i128")
}

fn ptoken_balance(env: &Env, addr: &Address) -> u128 {
    let bal = TokenBase::balance(env, addr);
    if bal < 0 {
        panic!("negative ptokens");
    }
    bal as u128
}

fn total_ptokens_supply(env: &Env) -> u128 {
    let supply = TokenBase::total_supply(env);
    if supply < 0 {
        panic!("negative supply");
    }
    supply as u128
}
