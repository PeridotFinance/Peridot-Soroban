use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, token, Address, Bytes, Env, IntoVal, String, Symbol, Val, Vec,
};
use stellar_tokens::fungible::burnable::emit_burn;
use stellar_tokens::fungible::Base as TokenBase;

use crate::constants::*;
use crate::events::*;
use crate::helpers::*;
use crate::storage::*;

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
        let storage = env.storage().persistent();
        if storage
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            panic!("already initialized");
        }
        storage.set(&DataKey::Initialized, &true);
        #[cfg(test)]
        {
            if let Some((caller, _)) = env.auths().first() {
                if caller != &admin {
                    panic!("initializer mismatch");
                }
            }
        }
        admin.require_auth();
        if supply_yearly_rate_scaled > MAX_YEARLY_RATE_SCALED {
            panic!("invalid supply rate");
        }
        if borrow_yearly_rate_scaled > MAX_YEARLY_RATE_SCALED {
            panic!("invalid borrow rate");
        }
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

    /// Admin: set boosted vault address (DeFindex).
    pub fn set_boosted_vault(env: Env, admin: Address, boosted_vault: Address) {
        let _ = ensure_initialized(&env);
        let stored: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if stored != admin {
            panic!("not admin");
        }
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::BoostedVault, &boosted_vault);
    }

    /// View: get boosted vault (if set)
    pub fn get_boosted_vault(env: Env) -> Option<Address> {
        let _ = ensure_initialized(&env);
        env.storage().persistent().get(&DataKey::BoostedVault)
    }

    /// Deposit tokens into the vault and receive pTokens
    pub fn deposit(env: Env, user: Address, amount: u128) {
        let token_address = ensure_initialized(&env);
        // Always update interest first
        Self::update_interest(env.clone());
        // Require authorization from the user
        ensure_user_auth(&env, &user);
        // Rewards: accrue user in this market
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            let total_ptokens_before = total_ptokens_supply(&env);
            let total_borrowed_before: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
            let user_ptokens_before = ptoken_balance(&env, &user);
            let user_borrow_before = Self::get_user_borrow_balance(env.clone(), user.clone());
            let hint = ControllerAccrualHint {
                total_ptokens: Some(total_ptokens_before),
                total_borrowed: Some(total_borrowed_before),
                user_ptokens: Some(user_ptokens_before),
                user_borrowed: Some(user_borrow_before),
            };
            if let Err(err) = try_call_contract::<(), _>(
                &env,
                &comp_addr,
                "accrue_user_market",
                (user.clone(), env.current_contract_address(), Some(hint)),
            ) {
                emit_external_call_failure(&env, &comp_addr, &err, true);
            }
        }

        // Get the underlying token
        // Pause: consult peridottroller if set
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            let paused: bool = call_contract_or_panic(
                &env,
                &comp_addr,
                "is_deposit_paused",
                (env.current_contract_address(),),
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
        let scaled_amount = amount
            .checked_mul(SCALE_1E6)
            .expect("ptoken calculation overflow");
        let ptokens_to_mint = scaled_amount / current_rate;
        if ptokens_to_mint == 0 {
            panic!("amount below minimum");
        }
        // Transfer tokens from user to contract
        let amount_i128 = to_i128(amount);
        token_client.transfer(&user, &env.current_contract_address(), &amount_i128);

        // If boosted, deposit into DeFindex vault (single-asset)
        if let Some(boosted) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::BoostedVault)
        {
            let mut amounts_desired: Vec<i128> = Vec::new(&env);
            let mut amounts_min: Vec<i128> = Vec::new(&env);
            amounts_desired.push_back(amount_i128);
            amounts_min.push_back(amount_i128);
            let args: Vec<Val> = (
                amounts_desired.clone(),
                amounts_min.clone(),
                env.current_contract_address(),
                true,
            )
                .into_val(&env);
            let mut auths = Vec::new(&env);
            let mut sub_invocations: Vec<InvokerContractAuthEntry> = Vec::new(&env);
            let transfer_args: Vec<Val> = (
                env.current_contract_address(),
                boosted.clone(),
                amount_i128,
            )
                .into_val(&env);
            // Root transfer auth (in case vault expects a flat auth entry)
            auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: token_address.clone(),
                    fn_name: Symbol::new(&env, "transfer"),
                    args: transfer_args.clone(),
                },
                sub_invocations: Vec::new(&env),
            }));
            sub_invocations.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: token_address.clone(),
                    fn_name: Symbol::new(&env, "transfer"),
                    args: transfer_args,
                },
                sub_invocations: Vec::new(&env),
            }));
            auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: boosted.clone(),
                    fn_name: Symbol::new(&env, "deposit"),
                    args,
                },
                sub_invocations,
            }));
            env.authorize_as_current_contract(auths);
            let _: Val = env.invoke_contract(
                &boosted,
                &Symbol::new(&env, "deposit"),
                (
                    amounts_desired,
                    amounts_min,
                    env.current_contract_address(),
                    true,
                )
                    .into_val(&env),
            );
        }

        // Mint pTokens and update totals
        TokenBase::mint(&env, &user, to_i128(ptokens_to_mint));
        let total_deposited: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .expect("total deposited missing");
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
        let token_address = ensure_initialized(&env);
        user.require_auth();
        // Always update interest first
        Self::update_interest(env.clone());
        let current_ptokens = ptoken_balance(&env, &user);
        // Rewards accrue
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            let total_ptokens_before = total_ptokens_supply(&env);
            let total_borrowed_before: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
            let user_borrow_before = Self::get_user_borrow_balance(env.clone(), user.clone());
            let hint = ControllerAccrualHint {
                total_ptokens: Some(total_ptokens_before),
                total_borrowed: Some(total_borrowed_before),
                user_ptokens: Some(current_ptokens),
                user_borrowed: Some(user_borrow_before),
            };
            if let Err(err) = try_call_contract::<(), _>(
                &env,
                &comp_addr,
                "accrue_user_market",
                (user.clone(), env.current_contract_address(), Some(hint)),
            ) {
                emit_external_call_failure(&env, &comp_addr, &err, true);
            }
        }

        // Check user has sufficient pTokens
        if current_ptokens < ptoken_amount {
            panic!("Insufficient pTokens");
        }

        // Calculate underlying tokens to return based on current exchange rate
        let current_rate = Self::get_exchange_rate(env.clone());
        // underlying = ptoken_amount * rate / 1e6
        // SECURITY: Use checked_mul to prevent silent overflow in release builds
        let underlying_to_return = ptoken_amount
            .checked_mul(current_rate)
            .expect("withdraw calculation overflow")
            / SCALE_1E6;

        // Check we have enough liquid underlying (cash)
        let available_underlying = Self::get_available_liquidity(env.clone());
        if available_underlying < underlying_to_return {
            panic!("Not enough liquidity");
        }

        // USD-based redeem gating via peridottroller, if set; otherwise local-only check
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            // Pause check via peridottroller
            let paused: bool = call_contract_or_panic(
                &env,
                &comp_addr,
                "is_redeem_paused",
                (env.current_contract_address(),),
            );
            if paused {
                panic!("redeem paused");
            }
            let local_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
            let other_borrows_usd: u128 = call_contract_or_panic(
                &env,
                &comp_addr,
                "get_borrows_excl",
                (user.clone(), env.current_contract_address()),
            );
            if local_debt > 0 || other_borrows_usd > 0 {
                // Other markets collateral in USD
                let other_collateral_usd: u128 = call_contract_or_panic(
                    &env,
                    &comp_addr,
                    "get_collateral_excl_usd",
                    (user.clone(), env.current_contract_address()),
                );
                // Price of this underlying
                let price_opt: Option<(u128, u128)> = call_contract_or_panic(
                    &env,
                    &comp_addr,
                    "get_price_usd",
                    (token_address.clone(),),
                );
                if price_opt.is_none() {
                    panic!("Price unavailable");
                }
                let (price, scale) = price_opt.unwrap();
                let cf: u128 = call_contract_or_panic(
                    &env,
                    &comp_addr,
                    "get_market_cf",
                    (env.current_contract_address(),),
                );

                // Local remaining collateral after this redeem
                let remaining_ptokens = current_ptokens - ptoken_amount;
                let remaining_underlying =
                    (remaining_ptokens.saturating_mul(current_rate)) / SCALE_1E6;
                let remaining_discounted = (remaining_underlying.saturating_mul(cf)) / SCALE_1E6;
                let local_collateral_usd = (remaining_discounted.saturating_mul(price)) / scale;

                // Borrows USD: other markets + local market
                let other_borrows_usd: u128 = call_contract_or_panic(
                    &env,
                    &comp_addr,
                    "get_borrows_excl",
                    (user.clone(), env.current_contract_address()),
                );
                let local_debt_usd = (local_debt.saturating_mul(price)) / scale;

                let total_collateral_usd =
                    other_collateral_usd.saturating_add(local_collateral_usd);
                let total_borrow_usd = other_borrows_usd.saturating_add(local_debt_usd);
                if total_collateral_usd < total_borrow_usd {
                    panic!("Insufficient collateral");
                }
            }
        } else {
            // SECURITY: Local-only collateral check when no Peridottroller is configured.
            // Without this, users could withdraw all collateral while having outstanding debt,
            // creating undercollateralized positions and bad debt.
            let local_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
            if local_debt > 0 {
                // Compute remaining collateral after this withdrawal
                let remaining_ptokens = current_ptokens - ptoken_amount;
                let remaining_underlying =
                    (remaining_ptokens.saturating_mul(current_rate)) / SCALE_1E6;
                let local_cf: u128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::CollateralFactorScaled)
                    .unwrap_or(500_000u128);
                // remaining_max_borrow = remaining_underlying * CF / 1e6
                let remaining_max_borrow =
                    (remaining_underlying.saturating_mul(local_cf)) / SCALE_1E6;
                // User's debt must not exceed their remaining borrowing capacity
                if local_debt > remaining_max_borrow {
                    panic!("Insufficient collateral");
                }
            }
        }

        // Create token client
        let token_client = token::Client::new(&env, &token_address);

        let burn_i128 = to_i128(ptoken_amount);
        // Burn pTokens without implicit auth (already required above)
        TokenBase::update(&env, Some(&user), None, burn_i128);
        emit_burn(&env, &user, burn_i128);
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
                .expect("accumulated interest missing");
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

        // If boosted and cash is insufficient, withdraw from DeFindex vault
        if let Some(boosted) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::BoostedVault)
        {
            let cash_i: i128 =
                token_balance(&env, &token_address, &env.current_contract_address());
            let cash: u128 = if cash_i < 0 { 0u128 } else { cash_i as u128 };
            if cash < underlying_to_return {
                    let total_shares_i: i128 =
                        call_contract_or_panic(&env, &boosted, "total_supply", ());
                if total_shares_i > 0 {
                    let total_shares = total_shares_i as u128;
                    let total_amounts: Vec<i128> = call_contract_or_panic(
                        &env,
                        &boosted,
                        "get_asset_amounts_per_shares",
                        (total_shares as i128,),
                    );
                    let total_underlying_i = total_amounts.get(0).unwrap_or(0);
                    if total_underlying_i > 0 {
                        let total_underlying = total_underlying_i as u128;
                        let mut shares_to_withdraw =
                            underlying_to_return.saturating_mul(total_shares) / total_underlying;
                        if underlying_to_return.saturating_mul(total_shares) % total_underlying
                            != 0
                        {
                            shares_to_withdraw = shares_to_withdraw.saturating_add(1);
                        }
                        let mut min_amounts_out: Vec<i128> = Vec::new(&env);
                        min_amounts_out.push_back(0);
                        let args: Vec<Val> = (
                            shares_to_withdraw as i128,
                            min_amounts_out.clone(),
                            env.current_contract_address(),
                        )
                            .into_val(&env);
                        let mut auths = Vec::new(&env);
                        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
                            context: ContractContext {
                                contract: boosted.clone(),
                                fn_name: Symbol::new(&env, "withdraw"),
                                args,
                            },
                            sub_invocations: Vec::new(&env),
                        }));
                        env.authorize_as_current_contract(auths);
                        let _: Val = env.invoke_contract(
                            &boosted,
                            &Symbol::new(&env, "withdraw"),
                            (
                                shares_to_withdraw as i128,
                                min_amounts_out,
                                env.current_contract_address(),
                            )
                                .into_val(&env),
                        );
                    }
                }
            }
        }

        // Transfer tokens back to user
        let underlying_i128 = to_i128(underlying_to_return);
        token_client.transfer(&env.current_contract_address(), &user, &underlying_i128);

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
        ensure_initialized(&env);
        if amount == 0 {
            return;
        }
        // Gating: if peridottroller wired, consult redeem pause and health for from-user
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            // Pause check
            let paused: bool = call_contract_or_panic(
                &env,
                &comp_addr,
                "is_redeem_paused",
                (env.current_contract_address(),),
            );
            if paused {
                panic!("redeem paused");
            }
            let pbal = ptoken_balance(&env, &from);
            if pbal < amount {
                panic!("Insufficient pTokens");
            }
            // Check via preview_redeem_max
            let max_ptokens: u128 = call_contract_or_panic(
                &env,
                &comp_addr,
                "preview_redeem_max",
                (from.clone(), env.current_contract_address()),
            );
            if amount > max_ptokens {
                panic!("Insufficient collateral");
            }
        } else {
            // Local-only collateral check when Peridottroller is not configured.
            // Prevents users with debt from transferring away collateral pTokens.
            let local_debt = Self::get_user_borrow_balance(env.clone(), from.clone());
            if local_debt > 0 {
                let current_rate = Self::get_exchange_rate(env.clone());
                let current_ptokens = ptoken_balance(&env, &from);
                if current_ptokens < amount {
                    panic!("Insufficient pTokens");
                }
                let remaining_ptokens = current_ptokens - amount;
                let remaining_underlying =
                    (remaining_ptokens.saturating_mul(current_rate)) / SCALE_1E6;
                let local_cf: u128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::CollateralFactorScaled)
                    .unwrap_or(500_000u128);
                let remaining_max_borrow =
                    (remaining_underlying.saturating_mul(local_cf)) / SCALE_1E6;
                if local_debt > remaining_max_borrow {
                    panic!("Insufficient collateral");
                }
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
            let total_ptokens_now = total_ptokens_supply(&env);
            let total_borrowed_now: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
            let from_hint = ControllerAccrualHint {
                total_ptokens: Some(total_ptokens_now),
                total_borrowed: Some(total_borrowed_now),
                user_ptokens: Some(ptoken_balance(&env, &from)),
                user_borrowed: Some(Self::get_user_borrow_balance(env.clone(), from.clone())),
            };
            if let Err(err) = try_call_contract::<(), _>(
                &env,
                &comp_addr,
                "accrue_user_market",
                (
                    from.clone(),
                    env.current_contract_address(),
                    Some(from_hint),
                ),
            ) {
                emit_external_call_failure(&env, &comp_addr, &err, true);
            }
            let to_hint = ControllerAccrualHint {
                total_ptokens: Some(total_ptokens_now),
                total_borrowed: Some(total_borrowed_now),
                user_ptokens: Some(ptoken_balance(&env, &to)),
                user_borrowed: Some(Self::get_user_borrow_balance(env.clone(), to.clone())),
            };
            if let Err(err) = try_call_contract::<(), _>(
                &env,
                &comp_addr,
                "accrue_user_market",
                (to, env.current_contract_address(), Some(to_hint)),
            ) {
                emit_external_call_failure(&env, &comp_addr, &err, true);
            }
        }
    }

    /// Get total amount deposited in the vault
    pub fn get_total_deposited(env: Env) -> u128 {
        let _ = ensure_initialized(&env);
        env.storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128)
    }

    /// Get total pTokens issued
    pub fn get_total_ptokens(env: Env) -> u128 {
        let _ = ensure_initialized(&env);
        total_ptokens_supply(&env)
    }

    /// Admin: upgrade contract code
    pub fn upgrade_wasm(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        let _ = ensure_initialized(&env);
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
        let _ = ensure_initialized(&env);
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
        let _ = ensure_initialized(&env);
        let total_ptokens = total_ptokens_supply(&env);
        if total_ptokens == 0 {
            return env
                .storage()
                .persistent()
                .get(&DataKey::InitialExchangeRate)
                .unwrap_or(SCALE_1E6);
        }
        let total_underlying = Self::get_total_underlying(env.clone());
        if total_underlying == 0 {
            // Fall back to initial rate to avoid halting operations; downstream liquidity
            // checks still protect withdrawals when cash is exhausted.
            return env
                .storage()
                .persistent()
                .get(&DataKey::InitialExchangeRate)
                .unwrap_or(SCALE_1E6);
        }
        // rate = total_underlying / total_ptokens, scaled 1e6
        let scaled_underlying = total_underlying
            .checked_mul(SCALE_1E6)
            .expect("exchange rate overflow");
        scaled_underlying / total_ptokens
    }

    /// Get the underlying token address
    pub fn get_underlying_token(env: Env) -> Address {
        let _ = ensure_initialized(&env);
        env.storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized")
    }

    /// Get collateral factor (scaled 1e6)
    pub fn get_collateral_factor(env: Env) -> u128 {
        let _ = ensure_initialized(&env);
        env.storage()
            .persistent()
            .get(&DataKey::CollateralFactorScaled)
            .unwrap_or(500_000u128)
    }

    /// Admin: set peridottroller address
    pub fn set_peridottroller(env: Env, peridottroller: Address) {
        let _ = ensure_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("Vault not initialized");
        let _: bool = call_contract_or_panic::<bool, _>(
            &env,
            &peridottroller,
            "is_deposit_paused",
            (env.current_contract_address(),),
        );
        let _: bool = call_contract_or_panic::<bool, _>(
            &env,
            &peridottroller,
            "is_redeem_paused",
            (env.current_contract_address(),),
        );
        let _: bool = call_contract_or_panic::<bool, _>(
            &env,
            &peridottroller,
            "is_borrow_paused",
            (env.current_contract_address(),),
        );
        let _: () = call_contract_or_panic::<(), _>(
            &env,
            &peridottroller,
            "accrue_user_market",
            (
                env.current_contract_address(),
                env.current_contract_address(),
                Option::<ControllerAccrualHint>::None,
            ),
        );
        let _: u128 = call_contract_or_panic::<u128, _>(
            &env,
            &peridottroller,
            "get_market_cf",
            (env.current_contract_address(),),
        );
        let _: u128 = call_contract_or_panic::<u128, _>(
            &env,
            &peridottroller,
            "get_collateral_excl_usd",
            (
                env.current_contract_address(),
                env.current_contract_address(),
            ),
        );
        let _: u128 = call_contract_or_panic::<u128, _>(
            &env,
            &peridottroller,
            "get_borrows_excl",
            (
                env.current_contract_address(),
                env.current_contract_address(),
            ),
        );
        let _price_check: Option<(u128, u128)> =
            call_contract_or_panic(&env, &peridottroller, "get_price_usd", (token_address,));
        env.storage()
            .persistent()
            .set(&DataKey::Peridottroller, &peridottroller.clone());
        NewPeridottroller { peridottroller }.publish(&env);
    }

    /// Admin: set interest rate model address
    pub fn set_interest_model(env: Env, model: Address) {
        let _ = ensure_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        // Basic interface check to ensure the target contract exposes the expected entrypoints
        let _ = call_contract_or_panic::<u128, _>(
            &env,
            &model,
            "get_borrow_rate",
            (0u128, 0u128, 0u128),
        );
        let _ = call_contract_or_panic::<u128, _>(
            &env,
            &model,
            "get_supply_rate",
            (0u128, 0u128, 0u128, 0u128),
        );
        env.storage()
            .persistent()
            .set(&DataKey::InterestModel, &model.clone());
        NewInterestModel { model }.publish(&env);
    }

    /// Admin: set reserve factor (0..=1e6)
    pub fn set_reserve_factor(env: Env, reserve_factor_scaled: u128) {
        let _ = ensure_initialized(&env);
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
        let _ = ensure_initialized(&env);
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
        let _ = ensure_initialized(&env);
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
        let _ = ensure_initialized(&env);
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
        let _ = ensure_initialized(&env);
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
        let _ = ensure_initialized(&env);
        env.storage()
            .persistent()
            .get(&DataKey::TotalReserves)
            .unwrap_or(0u128)
    }

    /// Get total admin fees
    pub fn get_total_admin_fees(env: Env) -> u128 {
        let _ = ensure_initialized(&env);
        env.storage()
            .persistent()
            .get(&DataKey::TotalAdminFees)
            .unwrap_or(0u128)
    }

    /// Admin: reduce reserves and transfer to admin
    pub fn reduce_reserves(env: Env, amount: u128) {
        let _ = ensure_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        let token_address = ensure_initialized(&env);
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
        let token_client = token::Client::new(&env, &token_address);
        let amount_i128 = to_i128(amount);
        token_client.transfer(&env.current_contract_address(), &admin, &amount_i128);
        ReservesReduced {
            reduce_amount: amount,
            total_reserves: updated_reserves,
        }
        .publish(&env);
    }

    /// Admin: reduce admin fees and transfer to admin
    pub fn reduce_admin_fees(env: Env, amount: u128) {
        let _ = ensure_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        let token_address = ensure_initialized(&env);
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
        let token_client = token::Client::new(&env, &token_address);
        let amount_i128 = to_i128(amount);
        token_client.transfer(&env.current_contract_address(), &admin, &amount_i128);
        AdminFeesReduced {
            reduce_amount: amount,
            total_admin_fees: updated_fees,
        }
        .publish(&env);
    }

    //

    /// Update interest based on elapsed time and current per-second rate
    pub fn update_interest(env: Env) {
        if env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::UnderlyingToken)
            .is_none()
        {
            return;
        }
        bump_borrow_state_ttl(&env);
        let last_time: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::LastUpdateTime)
            .expect("last update missing");
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
            let cash = Self::get_available_liquidity(env.clone());
            let borrows: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
            let rf: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::ReserveFactorScaled)
                .unwrap_or(0u128);
            call_contract_or_panic(
                &env,
                &model,
                "get_supply_rate",
                (cash, borrows, pooled_reserves, rf),
            )
        } else {
            env.storage()
                .persistent()
                .get(&DataKey::YearlyRateScaled)
                .expect("yearly rate missing")
        };
        if yearly_rate_scaled > MAX_YEARLY_RATE_SCALED {
            panic!("interest rate out of bounds");
        }

        let total_deposited: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        if total_deposited > 0 && yearly_rate_scaled > 0 {
            // new_interest = total_deposited * yearly_rate * elapsed / (SECONDS_PER_YEAR * 1e6)
            let new_interest =
                checked_interest_product(&env, total_deposited, yearly_rate_scaled, elapsed);

            if new_interest > 0 {
                let accumulated: u128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AccumulatedInterest)
                    .expect("accumulated interest missing");
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
            .expect("total borrowed missing");
        let mut interest_accumulated_event: u128 = 0u128;
        let mut event_borrow_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        let mut event_total_borrows: u128 = tb_prior;
        // Determine borrow yearly rate from model if set, else static
        let borrow_yearly_rate_scaled: u128 = if let Some(model) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::InterestModel)
        {
            let cash = Self::get_available_liquidity(env.clone());
            let borrows: u128 = tb_prior;
            call_contract_or_panic(
                &env,
                &model,
                "get_borrow_rate",
                (cash, borrows, pooled_reserves),
            )
        } else {
            env.storage()
                .persistent()
                .get(&DataKey::BorrowYearlyRateScaled)
                .expect("borrow yearly rate missing")
        };
        if borrow_yearly_rate_scaled > MAX_YEARLY_RATE_SCALED {
            panic!("interest rate out of bounds");
        }
        if tb_prior > 0 && borrow_yearly_rate_scaled > 0 {
            let borrow_interest_total =
                checked_interest_product(&env, tb_prior, borrow_yearly_rate_scaled, elapsed);
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
                .expect("borrow index missing");
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
        let cash_i: i128 = token_balance(&env, &token_address, &env.current_contract_address());
        let cash: u128 = if cash_i < 0 { 0u128 } else { cash_i as u128 };
        let borrows: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .expect("total borrowed missing");
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
        let accumulated_interest: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedInterest)
            .expect("accumulated interest missing");
        let boosted_underlying = if let Some(boosted) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::BoostedVault)
        {
            let shares_i = token::TokenClient::new(&env, &boosted)
                .balance(&env.current_contract_address());
            if shares_i > 0 {
                let amounts: Vec<i128> = call_contract_or_panic(
                    &env,
                    &boosted,
                    "get_asset_amounts_per_shares",
                    (shares_i,),
                );
                let amt_i = amounts.get(0).unwrap_or(0);
                if amt_i > 0 {
                    amt_i as u128
                } else {
                    0u128
                }
            } else {
                0u128
            }
        } else {
            0u128
        };
        cash.saturating_add(boosted_underlying)
            .saturating_add(borrows)
            .saturating_add(accumulated_interest)
            .saturating_sub(reserves)
            .saturating_sub(admin_fees)
    }

    /// Admin: update yearly interest rate (scaled 1e6). Applies after accruing with old rate.
    pub fn set_interest_rate(env: Env, yearly_rate_scaled: u128) {
        let _ = ensure_initialized(&env);
        // Admin guard
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        if yearly_rate_scaled > MAX_YEARLY_RATE_SCALED {
            panic!("invalid supply rate");
        }
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
        let _ = ensure_initialized(&env);
        // Admin guard
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        if yearly_rate_scaled > MAX_YEARLY_RATE_SCALED {
            panic!("invalid borrow rate");
        }
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
        let _ = ensure_initialized(&env);
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
        let _ = ensure_initialized(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    }

    /// Get user's current borrow balance (principal adjusted by index)
    pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {
        let _ = ensure_initialized(&env);
        let has_borrowed: bool = env
            .storage()
            .persistent()
            .get(&DataKey::HasBorrowed(user.clone()))
            .unwrap_or(false);
        bump_borrow_snapshot_ttl(&env, &user);
        bump_has_borrowed_ttl(&env, &user);
        let snap: Option<BorrowSnapshot> = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowSnapshots(user.clone()));
        let Some(snapshot) = snap else {
            if has_borrowed {
                panic!("borrow snapshot missing");
            }
            return 0u128;
        };
        if snapshot.principal == 0 {
            return 0u128;
        }
        let current_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        // principal * current_index / user_index
        (snapshot.principal.saturating_mul(current_index)) / snapshot.interest_index
    }

    /// Internal: write user's borrow snapshot
    fn write_borrow_snapshot(env: &Env, user: Address, principal: u128) {
        let current_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        if principal == 0 {
            env.storage()
                .persistent()
                .remove(&DataKey::BorrowSnapshots(user.clone()));
            env.storage()
                .persistent()
                .remove(&DataKey::HasBorrowed(user.clone()));
            bump_has_borrowed_ttl(env, &user);
            return;
        }
        let snap = BorrowSnapshot {
            principal,
            interest_index: current_index,
        };
        env.storage()
            .persistent()
            .set(&DataKey::BorrowSnapshots(user.clone()), &snap);
        env.storage()
            .persistent()
            .set(&DataKey::HasBorrowed(user.clone()), &true);
        bump_borrow_snapshot_ttl(env, &user);
        bump_has_borrowed_ttl(env, &user);
    }

    /// Get available liquidity = total_underlying - total_borrowed
    pub fn get_available_liquidity(env: Env) -> u128 {
        let total_underlying = Self::get_total_underlying(env.clone());
        let total_borrowed: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
        total_underlying.saturating_sub(total_borrowed)
    }

    /// Get total borrowed outstanding
    pub fn get_total_borrowed(env: Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing")
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
        let token_address = ensure_initialized(&env);
        Self::update_interest(env.clone());
        ensure_user_auth(&env, &user);
        let mut user_ptokens_before: u128 = 0;
        let mut user_borrow_before: u128 = 0;
        let mut exchange_rate: u128 = 0;
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            let total_ptokens_before = total_ptokens_supply(&env);
            let total_borrowed_before: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
            user_ptokens_before = ptoken_balance(&env, &user);
            user_borrow_before = Self::get_user_borrow_balance(env.clone(), user.clone());
            exchange_rate = Self::get_exchange_rate(env.clone());
            let hint = ControllerAccrualHint {
                total_ptokens: Some(total_ptokens_before),
                total_borrowed: Some(total_borrowed_before),
                user_ptokens: Some(user_ptokens_before),
                user_borrowed: Some(user_borrow_before),
            };
            if let Err(err) = try_call_contract::<(), _>(
                &env,
                &comp_addr,
                "accrue_user_market",
                (user.clone(), env.current_contract_address(), Some(hint)),
            ) {
                emit_external_call_failure(&env, &comp_addr, &err, true);
            }
        }

        // Cross-market enforcement via peridottroller (USD); fall back to local-only if no peridottroller
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            // Pause check via peridottroller
            let paused: bool = call_contract_or_panic(
                &env,
                &comp_addr,
                "is_borrow_paused",
                (env.current_contract_address(),),
            );
            if paused {
                panic!("borrow paused");
            }
            let liq_hint = MarketLiquidityHint {
                ptoken_balance: user_ptokens_before,
                user_borrowed: user_borrow_before,
                exchange_rate,
            };
            let (_liq, shortfall): (u128, u128) = call_contract_or_panic(
                &env,
                &comp_addr,
                "hypothetical_liquidity_with_hint",
                (
                    user.clone(),
                    env.current_contract_address(),
                    amount,
                    token_address.clone(),
                    liq_hint,
                ),
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
                .expect("total borrowed missing");
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
                .expect("total borrowed missing");
        let total_borrows = tb.saturating_add(amount);
        env.storage()
            .persistent()
            .set(&DataKey::TotalBorrowed, &total_borrows);

        // Transfer tokens to user
        let token_client = token::Client::new(&env, &token_address);
        let amount_i128 = to_i128(amount);
        token_client.transfer(&env.current_contract_address(), &user, &amount_i128);

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
        let token_address = ensure_initialized(&env);
        Self::update_interest(env.clone());
        ensure_user_auth(&env, &user);
        let current_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            let total_ptokens_before = total_ptokens_supply(&env);
            let total_borrowed_before: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
            let user_ptokens_before = ptoken_balance(&env, &user);
            let hint = ControllerAccrualHint {
                total_ptokens: Some(total_ptokens_before),
                total_borrowed: Some(total_borrowed_before),
                user_ptokens: Some(user_ptokens_before),
                user_borrowed: Some(current_debt),
            };
            if let Err(err) = try_call_contract::<(), _>(
                &env,
                &comp_addr,
                "accrue_user_market",
                (user.clone(), env.current_contract_address(), Some(hint)),
            ) {
                emit_external_call_failure(&env, &comp_addr, &err, true);
            }
        }

        if current_debt == 0 {
            return;
        }
        let repay_amount = if amount > current_debt {
            current_debt
        } else {
            amount
        };

        // Transfer tokens from user
        let token_client = token::Client::new(&env, &token_address);
        let repay_i128 = to_i128(repay_amount);
        token_client.transfer(&user, &env.current_contract_address(), &repay_i128);

        // Update snapshot and totals
        let new_principal = current_debt - repay_amount;
        Self::write_borrow_snapshot(&env, user.clone(), new_principal);
        let tb: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
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
        let token_address = ensure_initialized(&env);
        Self::update_interest(env.clone());

        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            let paused: bool = call_contract_or_panic(
                &env,
                &comp_addr,
                "is_borrow_paused",
                (env.current_contract_address(),),
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

        let token_client = token::Client::new(&env, &token_address);

        let balance_before_i: i128 = token_client.balance(&env.current_contract_address());
        if balance_before_i < 0 {
            panic!("invalid cash state");
        }
        let balance_before = balance_before_i as u128;

        token_client.transfer(&env.current_contract_address(), &receiver, &to_i128(amount));

        // Receiver contract executes its logic and must return funds before this call unwinds.
        call_contract_or_panic::<(), _>(
            &env,
            &receiver,
            "on_flash_loan",
            (env.current_contract_address(), amount, fee, data.clone()),
        );

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
        let token_address = ensure_initialized(&env);
        // Accrue and auth via peridottroller or allowlisted liquidator
        Self::update_interest(env.clone());
        let comp: Option<Address> = env.storage().persistent().get(&DataKey::Peridottroller);
        let Some(comp_addr) = comp else {
            panic!("no peridottroller");
        };
        comp_addr.require_auth();

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
        liquidator.require_auth();
        let token_client = token::Client::new(&env, &token_address);
        let repay_i128 = to_i128(repay_amount);
        token_client.transfer(&liquidator, &env.current_contract_address(), &repay_i128);

        // Update borrower snapshot and totals
        let new_principal = current_debt - repay_amount;
        Self::write_borrow_snapshot(&env, borrower.clone(), new_principal);
        let tb: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
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
    pub fn seize(
        env: Env,
        borrower: Address,
        liquidator: Address,
        ptoken_amount: u128,
        ctx: Option<SeizeContext>,
    ) {
        let comp: Option<Address> = env.storage().persistent().get(&DataKey::Peridottroller);
        let Some(comp_addr) = comp else {
            abort_seize(&env, &borrower, &liquidator, ptoken_amount, "no_comp");
        };
        comp_addr.require_auth();
        if ptoken_amount == 0 {
            abort_seize(&env, &borrower, &liquidator, ptoken_amount, "zero_amt");
        }
        if ctx.is_none() {
            abort_seize(&env, &borrower, &liquidator, ptoken_amount, "missing_ctx");
        }
        let seize_ctx = ctx.unwrap();
        if seize_ctx.seize_ptokens != ptoken_amount {
            abort_seize(&env, &borrower, &liquidator, ptoken_amount, "ctx_mismatch");
        }
        if seize_ctx.fee_ptokens > ptoken_amount {
            abort_seize(&env, &borrower, &liquidator, ptoken_amount, "fee_gt_total");
        }
        if seize_ctx.fee_ptokens > 0 && seize_ctx.fee_recipient.is_none() {
            abort_seize(
                &env,
                &borrower,
                &liquidator,
                ptoken_amount,
                "fee_missing_recipient",
            );
        }
        if seize_ctx.shortfall == 0 {
            abort_seize(&env, &borrower, &liquidator, ptoken_amount, "solvent");
        }
        if seize_ctx.max_redeem_ptokens >= ptoken_amount {
            abort_seize(&env, &borrower, &liquidator, ptoken_amount, "voluntary");
        }
        if seize_ctx.expires_at < env.ledger().timestamp() {
            abort_seize(&env, &borrower, &liquidator, ptoken_amount, "stale_ctx");
        }
        let borrower_bal = ptoken_balance(&env, &borrower);
        if borrower_bal < ptoken_amount {
            abort_seize(&env, &borrower, &liquidator, ptoken_amount, "insufficient");
        }
        let mut remaining = ptoken_amount;
        if seize_ctx.fee_ptokens > 0 {
            if let Some(recipient) = seize_ctx.fee_recipient {
                let fee_i128 = to_i128(seize_ctx.fee_ptokens);
                TokenBase::update(&env, Some(&borrower), Some(&recipient), fee_i128);
                stellar_tokens::fungible::emit_transfer(&env, &borrower, &recipient, fee_i128);
                remaining = remaining.saturating_sub(seize_ctx.fee_ptokens);
            }
        }
        if remaining > 0 {
            TokenBase::update(&env, Some(&borrower), Some(&liquidator), to_i128(remaining));
            stellar_tokens::fungible::emit_transfer(
                &env,
                &borrower,
                &liquidator,
                to_i128(remaining),
            );
        }
    }
}
