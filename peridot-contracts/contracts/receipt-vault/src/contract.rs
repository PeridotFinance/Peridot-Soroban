use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, token, Address, Bytes, Env, IntoVal, MuxedAddress, String, Symbol, Val,
    Vec,
};
use stellar_tokens::fungible::burnable::emit_burn;
use stellar_tokens::fungible::Base as TokenBase;

use crate::constants::*;
use crate::events::*;
use crate::helpers::*;
use crate::storage::*;

#[contract]
pub struct ReceiptVault;

const BOOSTED_CACHE_MAX_AGE_SECS: u64 = 60 * 60;
const BPS_SCALE: u128 = 10_000u128;
const BOOSTED_MODEL_CASH_TOLERANCE_BPS: u128 = 500u128; // 5%
const DEBT_STATE_VERSION_V1: u32 = 1u32;

#[contractimpl]
impl ReceiptVault {
    fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        a
    }

    fn checked_mul_div_u128(a: u128, b: u128, denom: u128) -> u128 {
        if denom == 0 {
            panic!("division by zero");
        }
        // Reduce before multiplying to avoid overflow in intermediate products.
        let mut left = a;
        let mut right = b;
        let mut d = denom;

        let g1 = Self::gcd_u128(left, d);
        left /= g1;
        d /= g1;
        let g2 = Self::gcd_u128(right, d);
        right /= g2;
        d /= g2;

        left.checked_mul(right)
            .expect("borrow index delta overflow")
            / d
    }

    fn cached_boosted_underlying(env: &Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::BoostedUnderlyingCached)
            .unwrap_or(0u128)
    }

    fn estimate_boosted_underlying_from_accounting(env: &Env) -> u128 {
        let storage = env.storage().persistent();
        let total_deposited: u128 = storage.get(&DataKey::TotalDeposited).unwrap_or(0u128);
        let total_reserves: u128 = storage.get(&DataKey::TotalReserves).unwrap_or(0u128);
        let total_admin_fees: u128 = storage.get(&DataKey::TotalAdminFees).unwrap_or(0u128);
        let total_borrowed: u128 = storage.get(&DataKey::TotalBorrowed).unwrap_or(0u128);
        let tracked_cash = Self::get_managed_cash(env);

        total_deposited
            .saturating_add(total_reserves)
            .saturating_add(total_admin_fees)
            .saturating_sub(total_borrowed)
            .saturating_sub(tracked_cash)
    }

    fn get_boosted_underlying(env: &Env) -> u128 {
        if let Some(boosted) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::BoostedVault)
        {
            let shares_i =
                token::TokenClient::new(env, &boosted).balance(&env.current_contract_address());
            if shares_i > 0 {
                match try_call_contract::<Vec<i128>, _>(
                    env,
                    &boosted,
                    "get_asset_amounts_per_shares",
                    (shares_i,),
                ) {
                    Ok(amounts) => {
                        let amt_i = amounts.get(0).unwrap_or(0);
                        if amt_i <= 0 {
                            let cached: u128 = env
                                .storage()
                                .persistent()
                                .get(&DataKey::BoostedUnderlyingCached)
                                .unwrap_or(0u128);
                            let estimated = Self::estimate_boosted_underlying_from_accounting(env);
                            return cached.max(estimated);
                        }
                        let boosted_underlying = amt_i as u128;
                        env.storage()
                            .persistent()
                            .set(&DataKey::BoostedUnderlyingCached, &boosted_underlying);
                        env.storage().persistent().set(
                            &DataKey::BoostedUnderlyingUpdatedAt,
                            &env.ledger().timestamp(),
                        );
                        boosted_underlying
                    }
                    Err(err) => {
                        emit_external_call_failure(env, &boosted, &err, true);
                        let now = env.ledger().timestamp();
                        let cached: Option<u128> = env
                            .storage()
                            .persistent()
                            .get(&DataKey::BoostedUnderlyingCached);
                        let updated_at: Option<u64> = env
                            .storage()
                            .persistent()
                            .get(&DataKey::BoostedUnderlyingUpdatedAt);
                        if let (Some(cached), Some(updated_at)) = (cached, updated_at) {
                            if now.saturating_sub(updated_at) <= BOOSTED_CACHE_MAX_AGE_SECS {
                                return cached;
                            }
                            // When cache is stale and external reads fail, avoid dropping to an
                            // accounting estimate that can be materially lower than the last
                            // observed boosted value.
                            let estimated = Self::estimate_boosted_underlying_from_accounting(env);
                            return cached.max(estimated);
                        }
                        Self::estimate_boosted_underlying_from_accounting(env)
                    }
                }
            } else {
                0u128
            }
        } else {
            0u128
        }
    }

    fn derive_managed_cash(env: &Env) -> u128 {
        let storage = env.storage().persistent();
        let total_deposited: u128 = storage.get(&DataKey::TotalDeposited).unwrap_or(0u128);
        let total_reserves: u128 = storage.get(&DataKey::TotalReserves).unwrap_or(0u128);
        let total_admin_fees: u128 = storage.get(&DataKey::TotalAdminFees).unwrap_or(0u128);
        let total_borrowed: u128 = storage.get(&DataKey::TotalBorrowed).unwrap_or(0u128);
        let cached_boosted = Self::cached_boosted_underlying(env);
        total_deposited
            .saturating_add(total_reserves)
            .saturating_add(total_admin_fees)
            .saturating_sub(total_borrowed)
            .saturating_sub(cached_boosted)
    }

    fn current_live_cash(env: &Env, token_address: &Address) -> u128 {
        let cash_i = token_balance(env, token_address, &env.current_contract_address());
        if cash_i < 0 {
            0u128
        } else {
            cash_i as u128
        }
    }

    fn idle_cash_buffer_bps(env: &Env) -> u32 {
        let value: Option<u32> = env.storage().persistent().get(&DataKey::IdleCashBufferBps);
        if value.is_some() {
            bump_idle_cash_buffer_ttl(env);
        }
        value.unwrap_or(0u32)
    }

    fn deposit_into_boosted(env: &Env, token_address: &Address, amount: u128) -> u128 {
        if amount == 0 {
            return 0u128;
        }
        let Some(boosted) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::BoostedVault)
        else {
            return 0u128;
        };
        let available_cash = Self::current_live_cash(env, token_address);
        let deploy_amount = amount.min(available_cash);
        if deploy_amount == 0 {
            return 0u128;
        }

        let deploy_i128 = to_i128(deploy_amount);
        let mut amounts_desired: Vec<i128> = Vec::new(env);
        let mut amounts_min: Vec<i128> = Vec::new(env);
        amounts_desired.push_back(deploy_i128);
        amounts_min.push_back(deploy_i128);
        let args: Vec<Val> = (
            amounts_desired.clone(),
            amounts_min.clone(),
            env.current_contract_address(),
            true,
        )
            .into_val(env);
        let mut auths = Vec::new(env);
        let mut sub_invocations: Vec<InvokerContractAuthEntry> = Vec::new(env);
        let transfer_args: Vec<Val> =
            (env.current_contract_address(), boosted.clone(), deploy_i128).into_val(env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token_address.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: transfer_args.clone(),
            },
            sub_invocations: Vec::new(env),
        }));
        sub_invocations.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token_address.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: transfer_args,
            },
            sub_invocations: Vec::new(env),
        }));
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: boosted.clone(),
                fn_name: Symbol::new(env, "deposit"),
                args,
            },
            sub_invocations,
        }));
        env.authorize_as_current_contract(auths);

        let cash_before_boost = Self::current_live_cash(env, token_address);
        let _: Val = env.invoke_contract(
            &boosted,
            &Symbol::new(env, "deposit"),
            (
                amounts_desired,
                amounts_min,
                env.current_contract_address(),
                true,
            )
                .into_val(env),
        );
        let cash_after_boost = Self::current_live_cash(env, token_address);
        let moved = cash_before_boost.saturating_sub(cash_after_boost);
        if moved > 0 {
            Self::sub_managed_cash(env, moved);
        }
        moved
    }

    /// Redeem from boosted vault to satisfy a live-cash requirement.
    fn redeem_from_boosted(env: &Env, token_address: &Address, needed_cash: u128) {
        if needed_cash == 0 {
            return;
        }
        let Some(boosted) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::BoostedVault)
        else {
            return;
        };

        let share_balance_i =
            token::TokenClient::new(env, &boosted).balance(&env.current_contract_address());
        if share_balance_i <= 0 {
            return;
        }
        let share_balance = share_balance_i as u128;

        let total_shares_i: i128 = call_contract_or_panic(env, &boosted, "total_supply", ());
        if total_shares_i <= 0 {
            return;
        }
        let total_shares = total_shares_i as u128;
        let total_amounts: Vec<i128> = call_contract_or_panic(
            env,
            &boosted,
            "get_asset_amounts_per_shares",
            (total_shares_i,),
        );
        let total_underlying_i = total_amounts.get(0).unwrap_or(0);
        if total_underlying_i <= 0 {
            return;
        }
        let total_underlying = total_underlying_i as u128;

        // Add a tiny buffer for share rounding so downstream payout paths are
        // less brittle to 1-unit quote/withdraw drift in boosted vault math.
        let target_cash = needed_cash.saturating_add(1);
        let numerator = target_cash.checked_mul(total_shares).unwrap_or(u128::MAX);
        let mut shares_to_withdraw = numerator / total_underlying;
        if numerator % total_underlying != 0 {
            shares_to_withdraw = shares_to_withdraw.saturating_add(1);
        }
        if shares_to_withdraw == 0 {
            shares_to_withdraw = 1;
        }
        if shares_to_withdraw > share_balance {
            shares_to_withdraw = share_balance;
        }

        let mut min_amounts_out: Vec<i128> = Vec::new(env);
        min_amounts_out.push_back(to_i128(needed_cash.saturating_sub(1)));
        let args: Vec<Val> = (
            to_i128(shares_to_withdraw),
            min_amounts_out.clone(),
            env.current_contract_address(),
        )
            .into_val(env);
        let mut auths = Vec::new(env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: boosted.clone(),
                fn_name: Symbol::new(env, "withdraw"),
                args,
            },
            sub_invocations: Vec::new(env),
        }));
        env.authorize_as_current_contract(auths);

        let cash_before = Self::current_live_cash(env, token_address);
        let _: Val = env.invoke_contract(
            &boosted,
            &Symbol::new(env, "withdraw"),
            (
                to_i128(shares_to_withdraw),
                min_amounts_out,
                env.current_contract_address(),
            )
                .into_val(env),
        );
        let cash_after = Self::current_live_cash(env, token_address);
        let received = cash_after.saturating_sub(cash_before);
        if received > 0 {
            Self::add_managed_cash(env, received);
        }
    }

    /// Ensure live cash can satisfy an immediate payout/borrow.
    fn ensure_liquid_cash(env: &Env, token_address: &Address, required_cash: u128) {
        let live_cash = Self::current_live_cash(env, token_address);
        if live_cash >= required_cash {
            return;
        }
        let needed = required_cash - live_cash;
        Self::redeem_from_boosted(env, token_address, needed);
    }

    fn get_managed_cash(env: &Env) -> u128 {
        if let Some(cash) = env.storage().persistent().get(&DataKey::ManagedCash) {
            cash
        } else {
            let cash = Self::derive_managed_cash(env);
            Self::set_managed_cash(env, cash);
            cash
        }
    }

    fn set_managed_cash(env: &Env, amount: u128) {
        env.storage()
            .persistent()
            .set(&DataKey::ManagedCash, &amount);
    }

    fn add_managed_cash(env: &Env, amount: u128) {
        let cash = Self::get_managed_cash(env);
        Self::set_managed_cash(env, cash.saturating_add(amount));
    }

    fn sub_managed_cash(env: &Env, amount: u128) {
        let cash = Self::get_managed_cash(env);
        if cash <= amount {
            Self::set_managed_cash(env, 0u128);
            return;
        }
        Self::set_managed_cash(env, cash - amount);
    }

    fn ensure_not_in_flash_loan(env: &Env) {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::FlashLoanActive)
            .unwrap_or(false)
        {
            panic!("operation blocked during flash loan");
        }
    }

    fn ensure_user_borrow_flag(env: &Env, user: &Address) {
        let persistent = env.storage().persistent();
        let has_snapshot = persistent
            .get::<_, BorrowSnapshot>(&DataKey::BorrowSnapshots(user.clone()))
            .is_some();
        if has_snapshot {
            persistent.set(&DataKey::HasBorrowed(user.clone()), &true);
        } else if persistent
            .get::<_, bool>(&DataKey::HasBorrowed(user.clone()))
            .is_none()
            && ptoken_balance(env, user) == 0
        {
            // Only initialize false flags for accounts without collateral.
            // This avoids masking missing debt state for collateralized users.
            persistent.set(&DataKey::HasBorrowed(user.clone()), &false);
        }
        bump_user_borrow_state_ttl(env, user);
    }

    fn ensure_margin_position_borrow_flag(env: &Env, position_id: u64) {
        let persistent = env.storage().persistent();
        let has_snapshot = persistent
            .get::<_, BorrowSnapshot>(&DataKey::MarginBorrowSnapshots(position_id))
            .is_some();
        if has_snapshot {
            persistent.set(&DataKey::MarginHasBorrowed(position_id), &true);
        } else if persistent
            .get::<_, bool>(&DataKey::MarginHasBorrowed(position_id))
            .is_none()
        {
            panic!("margin borrow state missing");
        }
        bump_margin_borrow_state_ttl(env, position_id);
    }

    fn require_margin_controller_auth(env: &Env) -> Address {
        let configured: Address = env
            .storage()
            .persistent()
            .get(&DataKey::MarginController)
            .expect("margin controller not set");
        configured.require_auth();
        configured
    }

    fn require_margin_position_owner(
        env: &Env,
        margin_controller: &Address,
        position_id: u64,
    ) -> Address {
        call_contract_or_panic(
            env,
            margin_controller,
            "get_margin_position_owner",
            (position_id, env.current_contract_address()),
        )
    }

    fn consume_margin_withdraw_bypass(env: &Env, user: &Address) -> bool {
        let key = DataKey::MarginWithdrawBypass(user.clone());
        let enabled = env.storage().persistent().get(&key).unwrap_or(false);
        if enabled {
            env.storage().persistent().remove(&key);
        }
        enabled
    }

    fn enforce_margin_lock(
        env: &Env,
        user: &Address,
        current_ptokens: u128,
        ptoken_reduction: u128,
    ) {
        if ptoken_reduction == 0 {
            return;
        }
        if let Some(margin_controller) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::MarginController)
        {
            let locked_ptokens: u128 = call_contract_or_panic(
                env,
                &margin_controller,
                "locked_ptokens_in_market",
                (user.clone(), env.current_contract_address()),
            );
            let remaining_ptokens = current_ptokens.saturating_sub(ptoken_reduction);
            if remaining_ptokens < locked_ptokens {
                panic!("collateral locked");
            }
        }
    }

    fn accrue_user_rewards(
        env: &Env,
        user: &Address,
        hint: ControllerAccrualHint,
        operation: &str,
    ) {
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            if let Err(err) = try_call_contract::<(), _>(
                env,
                &comp_addr,
                "accrue_user_market",
                (user.clone(), env.current_contract_address(), Some(hint)),
            ) {
                emit_external_call_failure(env, &comp_addr, &err, false);
                RewardAccrualFailed {
                    controller: comp_addr,
                    user: user.clone(),
                    operation: Symbol::new(env, operation),
                    failure_kind: err.kind.as_code(),
                }
                .publish(env);
                panic!("reward accrual failed");
            }
        }
    }

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
            || storage.has(&DataKey::Admin)
            || storage.has(&DataKey::UnderlyingToken)
            || TokenBase::total_supply(&env) > 0
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
        if supply_yearly_rate_scaled > borrow_yearly_rate_scaled {
            panic!("invalid rate relationship");
        }
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
            .set(&DataKey::ManagedCash, &0u128);

        // Store yearly supply/borrow rates (scaled 1e6)
        env.storage()
            .persistent()
            .set(&DataKey::YearlyRateScaled, &supply_yearly_rate_scaled);
        env.storage()
            .persistent()
            .set(&DataKey::BorrowYearlyRateScaled, &borrow_yearly_rate_scaled);
        // Borrowing is gated until either an interest model is configured or
        // admin explicitly enables static-rate mode.
        env.storage().persistent().set(&DataKey::RatesReady, &false);

        // Set last update time and accumulated interest
        let now = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::LastUpdateTime, &now);
        env.storage()
            .persistent()
            .set(&DataKey::DebtStateVersion, &DEBT_STATE_VERSION_V1);
        env.storage()
            .persistent()
            .set(&DataKey::DebtStateMigratedAt, &now);
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

        let metadata = env.current_contract_address().to_string();
        TokenBase::set_metadata(&env, PTOKEN_DECIMALS, metadata.clone(), metadata);
        bump_core_ttl(&env);
        bump_borrow_state_ttl(&env);
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
        let old_boosted: Option<Address> = env.storage().persistent().get(&DataKey::BoostedVault);
        if let Some(comp_addr) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            // Enforce one-to-one boosted-vault ownership across markets when a
            // shared controller is configured.
            let _: () = call_contract_or_panic(
                &env,
                &comp_addr,
                "bind_boosted_vault",
                (
                    env.current_contract_address(),
                    old_boosted.clone(),
                    Some(boosted_vault.clone()),
                ),
            );
        }
        env.storage()
            .persistent()
            .set(&DataKey::BoostedVault, &boosted_vault);
        env.storage()
            .persistent()
            .remove(&DataKey::BoostedUnderlyingCached);
        env.storage()
            .persistent()
            .remove(&DataKey::BoostedUnderlyingUpdatedAt);
        BoostedVaultSet {
            old_vault: old_boosted,
            new_vault: Some(boosted_vault),
        }
        .publish(&env);
    }

    /// View: get boosted vault (if set)
    pub fn get_boosted_vault(env: Env) -> Option<Address> {
        let _ = ensure_initialized(&env);
        env.storage().persistent().get(&DataKey::BoostedVault)
    }

    /// Admin: set target idle cash buffer in basis points (0..=10_000).
    pub fn set_idle_cash_buffer_bps(env: Env, admin: Address, idle_cash_buffer_bps: u32) {
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
        if idle_cash_buffer_bps > BPS_SCALE as u32 {
            panic!("invalid idle cash buffer");
        }
        if idle_cash_buffer_bps == 0 {
            env.storage()
                .persistent()
                .remove(&DataKey::IdleCashBufferBps);
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::IdleCashBufferBps, &idle_cash_buffer_bps);
            bump_idle_cash_buffer_ttl(&env);
        }
        NewIdleCashBuffer {
            idle_cash_buffer_bps,
        }
        .publish(&env);
    }

    /// View: get target idle cash buffer in basis points.
    pub fn get_idle_cash_buffer_bps(env: Env) -> u32 {
        let _ = ensure_initialized(&env);
        Self::idle_cash_buffer_bps(&env)
    }

    /// Admin: move excess live cash into boosted vault to match target buffer.
    pub fn rebalance_idle_cash(env: Env, admin: Address) {
        let token_address = ensure_initialized(&env);
        let stored: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if stored != admin {
            panic!("not admin");
        }
        admin.require_auth();

        let live_cash = Self::current_live_cash(&env, &token_address);
        if live_cash == 0 {
            return;
        }
        let bps = Self::idle_cash_buffer_bps(&env) as u128;
        let total_underlying = Self::get_total_underlying(env.clone());
        let desired_idle = total_underlying.saturating_mul(bps) / BPS_SCALE;
        if live_cash > desired_idle {
            let excess = live_cash - desired_idle;
            let _ = Self::deposit_into_boosted(&env, &token_address, excess);
        }
    }

    /// Deposit tokens into the vault and receive pTokens
    pub fn deposit(env: Env, user: Address, amount: u128) {
        let token_address = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        // Always update interest first
        Self::update_interest(env.clone());
        // Require authorization from the user
        Self::ensure_user_borrow_flag(&env, &user);
        ensure_user_auth(&env, &user);
        // Rewards: accrue user in this market and fail closed on error.
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
        Self::accrue_user_rewards(&env, &user, hint, "deposit");

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
        let cash_before = Self::current_live_cash(&env, &token_address);

        // Enforce supply cap if set (cap applies to total underlying after deposit)
        let cap: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::SupplyCap)
            .unwrap_or(0u128);
        let total_underlying_before = if cap > 0 {
            Some(Self::get_total_underlying(env.clone()))
        } else {
            None
        };

        // Calculate pTokens to mint based on current exchange rate BEFORE moving cash
        let current_rate = Self::get_exchange_rate(env.clone());
        let amount_i128 = to_i128(amount);
        token_client.transfer(&user, &env.current_contract_address(), &amount_i128);
        let cash_after = Self::current_live_cash(&env, &token_address);
        let received_cash = cash_after.saturating_sub(cash_before);
        if received_cash == 0 {
            panic!("amount below minimum");
        }
        if cap > 0 {
            let total_underlying_after = total_underlying_before
                .unwrap_or(0u128)
                .saturating_add(received_cash);
            if total_underlying_after > cap {
                panic!("supply cap exceeded");
            }
        }

        let scaled_amount = received_cash
            .checked_mul(SCALE_1E6)
            .expect("ptoken calculation overflow");
        let ptokens_to_mint = scaled_amount / current_rate;
        if ptokens_to_mint == 0 {
            panic!("amount below minimum");
        }
        Self::add_managed_cash(&env, received_cash);

        let deploy_amount = if received_cash == 0 {
            0u128
        } else {
            let idle_bps = Self::idle_cash_buffer_bps(&env) as u128;
            if idle_bps == 0 {
                received_cash
            } else {
                let total_underlying_after = Self::get_total_underlying(env.clone());
                let desired_idle = total_underlying_after.saturating_mul(idle_bps) / BPS_SCALE;
                let excess_live_cash = cash_after.saturating_sub(desired_idle);
                received_cash.min(excess_live_cash)
            }
        };
        let _ = Self::deposit_into_boosted(&env, &token_address, deploy_amount);

        // Mint pTokens and update totals
        TokenBase::mint(&env, &user, to_i128(ptokens_to_mint));
        let total_deposited: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .expect("total deposited missing");
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &(total_deposited + received_cash));

        // Emit Compound-style Mint event
        Mint {
            minter: user.clone(),
            mint_amount: received_cash,
            mint_tokens: ptokens_to_mint,
        }
        .publish(&env);
    }

    /// Withdraw tokens using pTokens
    pub fn withdraw(env: Env, user: Address, ptoken_amount: u128) {
        let token_address = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        user.require_auth();
        Self::ensure_user_borrow_flag(&env, &user);
        // Always update interest first
        Self::update_interest(env.clone());
        let current_ptokens = ptoken_balance(&env, &user);
        // Rewards accrue and fail closed on error.
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
        Self::accrue_user_rewards(&env, &user, hint, "withdraw");

        // Check user has sufficient pTokens
        if current_ptokens < ptoken_amount {
            panic!("Insufficient pTokens");
        }
        if !Self::consume_margin_withdraw_bypass(&env, &user) {
            Self::enforce_margin_lock(&env, &user, current_ptokens, ptoken_amount);
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

        let total_ptokens_after = total_ptokens_before
            .checked_sub(ptoken_amount)
            .expect("ptoken supply underflow");
        if total_ptokens_after == 0 {
            let total_borrowed: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
            if total_borrowed > 0 {
                panic!("cannot zero supply with outstanding borrows");
            }

            // Prevent a zero-supply state with residual value that would let the
            // next depositor bootstrap at an unfair initial exchange rate.
            let total_underlying_before = Self::get_total_underlying(env.clone());
            if total_underlying_before > underlying_to_return {
                panic!("cannot zero supply with residual assets");
            }
        }

        // Create token client
        let token_client = token::Client::new(&env, &token_address);

        let burn_i128 = to_i128(ptoken_amount);
        // Burn pTokens without implicit auth (already required above)
        TokenBase::update(&env, Some(&user), None, burn_i128);
        emit_burn(&env, &user, burn_i128);
        // Update totals
        let total_deposited: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .unwrap_or(0u128);
        // AccumulatedInterest is deprecated from supplier accounting; withdrawals
        // only adjust tracked deposits.
        let total_deposited_after = total_deposited.saturating_sub(underlying_to_return);
        env.storage()
            .persistent()
            .set(&DataKey::TotalDeposited, &total_deposited_after);

        // Pull from boosted vault on demand so user withdrawals are backed by live cash.
        Self::ensure_liquid_cash(&env, &token_address, underlying_to_return);

        let cash_after_boost = Self::current_live_cash(&env, &token_address);
        if cash_after_boost < underlying_to_return {
            panic!("withdraw liquidity shortfall");
        }

        // Transfer tokens back to user
        let underlying_i128 = to_i128(underlying_to_return);
        let cash_before_withdraw = Self::current_live_cash(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &user, &underlying_i128);
        let cash_after_withdraw = Self::current_live_cash(&env, &token_address);
        Self::sub_managed_cash(
            &env,
            cash_before_withdraw.saturating_sub(cash_after_withdraw),
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
    pub fn approve(
        env: Env,
        owner: Address,
        spender: Address,
        amount: i128,
        live_until_ledger: u32,
    ) {
        owner.require_auth();
        if amount < 0 {
            panic!("bad amount");
        }
        TokenBase::approve(&env, &owner, &spender, amount, live_until_ledger);
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        TokenBase::allowance(&env, &owner, &spender)
    }

    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        if amount < 0 {
            panic!("bad amount");
        }
        Self::transfer_internal(env, from, to.address(), amount as u128, None);
    }

    pub fn transfer_from(env: Env, spender: Address, owner: Address, to: Address, amount: i128) {
        if amount < 0 {
            panic!("bad amount");
        }
        Self::transfer_internal(env, owner, to, amount as u128, Some(spender));
    }

    pub fn balance(env: Env, account: Address) -> i128 {
        let _ = ensure_initialized(&env);
        TokenBase::balance(&env, &account)
    }

    pub fn total_supply(env: Env) -> i128 {
        let _ = ensure_initialized(&env);
        TokenBase::total_supply(&env)
    }

    pub fn decimals(env: Env) -> u32 {
        let _ = ensure_initialized(&env);
        TokenBase::decimals(&env)
    }

    pub fn name(env: Env) -> String {
        let _ = ensure_initialized(&env);
        TokenBase::name(&env)
    }

    pub fn symbol(env: Env) -> String {
        let _ = ensure_initialized(&env);
        TokenBase::symbol(&env)
    }

    fn transfer_internal(
        env: Env,
        from: Address,
        to: Address,
        amount: u128,
        spender: Option<Address>,
    ) {
        ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        if amount == 0 {
            return;
        }
        // Ensure collateral checks use the latest debt/index state.
        Self::update_interest(env.clone());
        Self::ensure_user_borrow_flag(&env, &from);
        Self::ensure_user_borrow_flag(&env, &to);
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
        Self::enforce_margin_lock(&env, &from, from_bal, amount);

        match spender {
            Some(spender_addr) => {
                TokenBase::transfer_from(&env, &spender_addr, &from, &to, to_i128(amount));
            }
            None => {
                TokenBase::transfer(&env, &from, &to, to_i128(amount));
            }
        }

        // Rewards accrual on transfers when peridottroller is wired.
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
        Self::accrue_user_rewards(&env, &from, from_hint, "transfer");
        let to_hint = ControllerAccrualHint {
            total_ptokens: Some(total_ptokens_now),
            total_borrowed: Some(total_borrowed_now),
            user_ptokens: Some(ptoken_balance(&env, &to)),
            user_borrowed: Some(Self::get_user_borrow_balance(env.clone(), to.clone())),
        };
        Self::accrue_user_rewards(&env, &to, to_hint, "transfer");
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

    /// Admin: stage a timelocked contract upgrade.
    pub fn propose_upgrade_wasm(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        let _ = ensure_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        let execute_after = env
            .ledger()
            .timestamp()
            .saturating_add(UPGRADE_TIMELOCK_SECS);
        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgradeHash, &new_wasm_hash);
        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgradeEta, &execute_after);
        bump_pending_upgrade_ttl(&env);
    }

    /// Admin: execute a staged upgrade once timelock has elapsed.
    pub fn upgrade_wasm(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        let _ = ensure_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        bump_pending_upgrade_ttl(&env);
        let pending_hash: soroban_sdk::BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgradeHash)
            .expect("pending upgrade not set");
        let execute_after: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgradeEta)
            .expect("pending upgrade eta not set");
        if pending_hash != new_wasm_hash {
            panic!("upgrade hash mismatch");
        }
        if env.ledger().timestamp() < execute_after {
            panic!("upgrade timelocked");
        }

        // If wired to a controller, require all market operations paused pre-upgrade.
        if let Some(peridottroller) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Peridottroller)
        {
            let market = env.current_contract_address();
            let deposit_paused: bool = call_contract_or_panic::<bool, _>(
                &env,
                &peridottroller,
                "is_deposit_paused",
                (market.clone(),),
            );
            let redeem_paused: bool = call_contract_or_panic::<bool, _>(
                &env,
                &peridottroller,
                "is_redeem_paused",
                (market.clone(),),
            );
            let borrow_paused: bool = call_contract_or_panic::<bool, _>(
                &env,
                &peridottroller,
                "is_borrow_paused",
                (market,),
            );
            if !(deposit_paused && redeem_paused && borrow_paused) {
                panic!("market not paused for upgrade");
            }
        }

        // Re-baseline interest state before swapping logic to avoid accrual discontinuity.
        Self::update_interest(env.clone());
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeHash);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeEta);
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
        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &new_admin);
        PendingAdmin { admin: new_admin }.publish(&env);
    }

    pub fn accept_admin(env: Env) {
        let _ = ensure_initialized(&env);
        let new_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .expect("pending admin not set");
        new_admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &new_admin);
        env.storage().persistent().remove(&DataKey::PendingAdmin);
        NewAdmin { admin: new_admin }.publish(&env);
    }

    /// Get the exchange rate (pToken to underlying ratio) scaled by 1e6
    pub fn get_exchange_rate(env: Env) -> u128 {
        let _ = ensure_initialized(&env);
        let total_ptokens = total_ptokens_supply(&env);
        if total_ptokens == 0 {
            let total_underlying = Self::get_total_underlying(env.clone());
            if total_underlying > 0 {
                panic!("non-empty vault at zero supply");
            }
            return env
                .storage()
                .persistent()
                .get(&DataKey::InitialExchangeRate)
                .unwrap_or(SCALE_1E6);
        }
        let total_underlying = Self::get_total_underlying(env.clone());
        if total_underlying == 0 {
            panic!("invalid underlying state");
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
        let existing_boosted: Option<Address> =
            env.storage().persistent().get(&DataKey::BoostedVault);
        if existing_boosted.is_some() {
            let _: () = call_contract_or_panic(
                &env,
                &peridottroller,
                "bind_boosted_vault",
                (
                    env.current_contract_address(),
                    Option::<Address>::None,
                    existing_boosted.clone(),
                ),
            );
        }
        env.storage()
            .persistent()
            .set(&DataKey::Peridottroller, &peridottroller.clone());
        NewPeridottroller { peridottroller }.publish(&env);
    }

    /// Admin: set or clear margin controller address used for collateral lock checks.
    pub fn set_margin_controller(env: Env, admin: Address, margin_controller: Option<Address>) {
        let _ = ensure_initialized(&env);
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if admin != stored_admin {
            panic!("not admin");
        }
        admin.require_auth();

        if let Some(controller) = margin_controller {
            let _: u128 = call_contract_or_panic(
                &env,
                &controller,
                "locked_ptokens_in_market",
                (
                    env.current_contract_address(),
                    env.current_contract_address(),
                ),
            );
            env.storage()
                .persistent()
                .set(&DataKey::MarginController, &controller);
            return;
        }
        env.storage()
            .persistent()
            .remove(&DataKey::MarginController);
    }

    pub fn get_margin_controller(env: Env) -> Option<Address> {
        let _ = ensure_initialized(&env);
        env.storage().persistent().get(&DataKey::MarginController)
    }

    pub fn begin_margin_withdraw(env: Env, margin_controller: Address, user: Address) {
        let _ = ensure_initialized(&env);
        let configured: Address = env
            .storage()
            .persistent()
            .get(&DataKey::MarginController)
            .expect("margin controller not set");
        if margin_controller != configured {
            panic!("not margin controller");
        }
        margin_controller.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::MarginWithdrawBypass(user), &true);
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
        env.storage().persistent().set(&DataKey::RatesReady, &true);
        bump_rates_ready_ttl(&env);
        NewInterestModel { model }.publish(&env);
    }

    /// Admin: explicitly enable static-rate mode when no external model is used.
    pub fn enable_static_rates(env: Env, admin: Address) {
        let _ = ensure_initialized(&env);
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if stored_admin != admin {
            panic!("not admin");
        }
        admin.require_auth();
        let supply_rate: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::YearlyRateScaled)
            .expect("supply rate missing");
        let borrow_rate: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowYearlyRateScaled)
            .expect("borrow rate missing");
        if supply_rate > borrow_rate {
            panic!("invalid rate relationship");
        }
        env.storage().persistent().set(&DataKey::RatesReady, &true);
        bump_rates_ready_ttl(&env);
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
        let storage = env.storage().persistent();
        if cap == 0 {
            // Disable principal tracking when cap is disabled to avoid stale state.
            storage.remove(&DataKey::TotalBorrowPrincipal);
        } else if storage
            .get::<_, u128>(&DataKey::TotalBorrowPrincipal)
            .is_none()
        {
            let total_borrowed: u128 = storage
                .get(&DataKey::TotalBorrowed)
                .expect("total borrowed missing");
            storage.set(&DataKey::TotalBorrowPrincipal, &total_borrowed);
        }
        storage.set(&DataKey::BorrowCap, &cap);
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
        let cash_before = Self::current_live_cash(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &admin, &amount_i128);
        let cash_after = Self::current_live_cash(&env, &token_address);
        Self::sub_managed_cash(&env, cash_before.saturating_sub(cash_after));
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
        let cash_before = Self::current_live_cash(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &admin, &amount_i128);
        let cash_after = Self::current_live_cash(&env, &token_address);
        Self::sub_managed_cash(&env, cash_before.saturating_sub(cash_after));
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
        bump_core_ttl(&env);
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
        let token_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .expect("underlying not set");
        // Borrow interest accrual via global index (split to reserves, admin fees, and suppliers)
        let tb_prior: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .expect("total borrowed missing");

        // Snapshot gross cash once so rate queries use raw liquidity inputs and
        // reserves are subtracted only inside the rate model.
        //
        // For rate-model inputs, cap boosted cash only when the reported value is
        // implausibly above an internal baseline (cached/accounting). This avoids
        // trusting extreme external quotes while preserving legitimate yield growth.
        // If baseline is unavailable while borrows are outstanding, fail-safe by
        // ignoring boosted cash for this accrual tick.
        let cached_before = Self::cached_boosted_underlying(&env);
        let boosted_reported = Self::get_boosted_underlying(&env);
        let boosted_accounting = Self::estimate_boosted_underlying_from_accounting(&env);
        let boosted_baseline = cached_before.max(boosted_accounting);
        let boosted_cap = if boosted_baseline == 0 {
            if tb_prior > 0 {
                0
            } else {
                boosted_reported
            }
        } else {
            boosted_baseline.saturating_add(
                (boosted_baseline.saturating_mul(BOOSTED_MODEL_CASH_TOLERANCE_BPS)) / BPS_SCALE,
            )
        };
        let boosted_for_model = boosted_reported.min(boosted_cap);
        let model_cash =
            Self::current_live_cash(&env, &token_address).saturating_add(boosted_for_model);

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

        let mut interest_accumulated_event: u128 = 0u128;
        let mut event_borrow_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        let mut event_total_borrows: u128 = tb_prior;
        let mut advance_last_update = tb_prior == 0;
        // Determine borrow yearly rate from model if set, else static
        let borrow_yearly_rate_scaled: u128 = if let Some(model) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::InterestModel)
        {
            let borrows: u128 = tb_prior;
            match try_call_contract(
                &env,
                &model,
                "get_borrow_rate",
                (model_cash, borrows, pooled_reserves),
            ) {
                Ok(rate) => rate,
                Err(err) => {
                    emit_external_call_failure(&env, &model, &err, true);
                    env.storage()
                        .persistent()
                        .get(&DataKey::BorrowYearlyRateScaled)
                        .expect("borrow yearly rate missing")
                }
            }
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
            if borrow_interest_total > 0 {
                advance_last_update = true;
            }

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

            // Increase total borrowed by total interest; supplier yield is
            // reflected through exchange-rate math via the borrow growth path.
            let tb_after = tb_prior.saturating_add(borrow_interest_total);
            env.storage()
                .persistent()
                .set(&DataKey::TotalBorrowed, &tb_after);
            event_total_borrows = tb_after;

            // Update borrow index with checked math (no saturating overflow).
            // delta = old_index * borrow_interest / tb_prior
            let old_index: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::BorrowIndex)
                .expect("borrow index missing");
            let delta_index =
                Self::checked_mul_div_u128(old_index, borrow_interest_total, tb_prior);
            let new_index = old_index
                .checked_add(delta_index)
                .expect("borrow index overflow");
            env.storage()
                .persistent()
                .set(&DataKey::BorrowIndex, &new_index);
            event_borrow_index = new_index;
        }
        if tb_prior > 0 && borrow_yearly_rate_scaled == 0 {
            advance_last_update = true;
        }

        AccrueInterest {
            interest_accumulated: interest_accumulated_event,
            borrow_index: event_borrow_index,
            total_borrows: event_total_borrows,
        }
        .publish(&env);

        // Move time forward only when accrual inputs cannot produce future interest
        // (no debt or zero rate) or this update accrued a non-zero amount.
        if advance_last_update {
            env.storage()
                .persistent()
                .set(&DataKey::LastUpdateTime, &now);
        }
    }

    /// Admin-only recovery for missing core state after TTL expiry.
    /// Sets missing rate/index/time fields to safe defaults.
    pub fn recover_state(
        env: Env,
        admin: Address,
        supply_yearly_rate_scaled: u128,
        borrow_yearly_rate_scaled: u128,
        total_borrowed: u128,
    ) {
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if stored_admin != admin {
            panic!("not admin");
        }
        admin.require_auth();
        if supply_yearly_rate_scaled > MAX_YEARLY_RATE_SCALED {
            panic!("invalid supply rate");
        }
        if borrow_yearly_rate_scaled > MAX_YEARLY_RATE_SCALED {
            panic!("invalid borrow rate");
        }
        if supply_yearly_rate_scaled > borrow_yearly_rate_scaled {
            panic!("invalid rate relationship");
        }
        let storage = env.storage().persistent();
        if !storage
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            storage.set(&DataKey::Initialized, &true);
        }
        if storage.get::<_, u128>(&DataKey::YearlyRateScaled).is_none() {
            storage.set(&DataKey::YearlyRateScaled, &supply_yearly_rate_scaled);
        }
        if storage
            .get::<_, u128>(&DataKey::BorrowYearlyRateScaled)
            .is_none()
        {
            storage.set(&DataKey::BorrowYearlyRateScaled, &borrow_yearly_rate_scaled);
        }
        if storage.get::<_, u128>(&DataKey::BorrowIndex).is_none() {
            storage.set(&DataKey::BorrowIndex, &INDEX_SCALE_1E18);
        }
        if storage.get::<_, u128>(&DataKey::TotalBorrowed).is_none() {
            storage.set(&DataKey::TotalBorrowed, &total_borrowed);
        }
        let borrow_cap: u128 = storage.get(&DataKey::BorrowCap).unwrap_or(0u128);
        if borrow_cap > 0
            && storage
                .get::<_, u128>(&DataKey::TotalBorrowPrincipal)
                .is_none()
        {
            // If borrow caps are enabled on an upgraded deployment, seed the
            // principal tracker from current borrows.
            storage.set(&DataKey::TotalBorrowPrincipal, &total_borrowed);
        }
        if storage.get::<_, u128>(&DataKey::TotalDeposited).is_none() {
            storage.set(&DataKey::TotalDeposited, &0u128);
        }
        if storage.get::<_, u128>(&DataKey::ManagedCash).is_none() {
            // Migration path for pre-upgrade deployments: initialize managed cash
            // from live vault balance to avoid circular dependency with boosted
            // underlying fallback paths.
            storage.set(&DataKey::ManagedCash, &Self::derive_managed_cash(&env));
        }
        if storage
            .get::<_, u128>(&DataKey::AccumulatedInterest)
            .is_none()
        {
            storage.set(&DataKey::AccumulatedInterest, &0u128);
        }
        if storage.get::<_, u64>(&DataKey::LastUpdateTime).is_none() {
            storage.set(&DataKey::LastUpdateTime, &env.ledger().timestamp());
        }
        if storage.get::<_, bool>(&DataKey::RatesReady).is_none() {
            let has_model = storage.get::<_, Address>(&DataKey::InterestModel).is_some();
            storage.set(&DataKey::RatesReady, &has_model);
        }
        bump_rates_ready_ttl(&env);
        bump_core_ttl(&env);
        bump_borrow_state_ttl(&env);
    }

    /// Get total underlying
    pub fn get_total_underlying(env: Env) -> u128 {
        // managed_cash + boosted_underlying + borrows - reserves - admin_fees
        let cash = Self::get_managed_cash(&env);
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
        let boosted_underlying = Self::get_boosted_underlying(&env);
        cash.saturating_add(boosted_underlying)
            .saturating_add(borrows)
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
        let borrow_rate_scaled: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowYearlyRateScaled)
            .expect("borrow rate missing");
        if yearly_rate_scaled > borrow_rate_scaled {
            panic!("invalid rate relationship");
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
        let supply_rate_scaled: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::YearlyRateScaled)
            .expect("supply rate missing");
        if supply_rate_scaled > yearly_rate_scaled {
            panic!("invalid rate relationship");
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
        let persistent = env.storage().persistent();
        let has_borrowed: Option<bool> = persistent.get(&DataKey::HasBorrowed(user.clone()));
        bump_user_borrow_state_ttl(&env, &user);
        let snap: Option<BorrowSnapshot> = persistent.get(&DataKey::BorrowSnapshots(user.clone()));
        let snapshot = if let Some(snapshot) = snap {
            snapshot
        } else if has_borrowed.unwrap_or(false)
            && persistent.has(&DataKey::BorrowPrincipal(user.clone()))
        {
            let principal: u128 = persistent
                .get(&DataKey::BorrowPrincipal(user.clone()))
                .expect("canonical borrow principal missing");
            let current_index: u128 = persistent
                .get(&DataKey::BorrowIndex)
                .expect("borrow index missing");
            let rebuilt = BorrowSnapshot {
                principal,
                interest_index: current_index,
            };
            persistent.set(&DataKey::BorrowSnapshots(user.clone()), &rebuilt);
            persistent.set(&DataKey::HasBorrowed(user.clone()), &(principal > 0));
            bump_user_borrow_state_ttl(&env, &user);
            rebuilt
        } else {
            if has_borrowed.unwrap_or(false) {
                panic!("borrow snapshot missing");
            }
            // Fail closed for collateralized accounts with missing borrow state.
            if has_borrowed.is_none() && ptoken_balance(&env, &user) > 0 {
                panic!("borrow state missing");
            }
            return 0u128;
        };
        if snapshot.principal == 0 {
            return 0u128;
        }
        if snapshot.interest_index == 0 {
            panic!("invalid borrower index");
        }
        let current_index: u128 = persistent
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        // principal * current_index / user_index
        (snapshot.principal.saturating_mul(current_index)) / snapshot.interest_index
    }

    /// Get current borrow balance for a margin position namespace.
    pub fn get_margin_borrow_balance(env: Env, position_id: u64) -> u128 {
        let _ = ensure_initialized(&env);
        let persistent = env.storage().persistent();
        let has_borrowed: Option<bool> = persistent.get(&DataKey::MarginHasBorrowed(position_id));
        bump_margin_borrow_state_ttl(&env, position_id);
        let snap: Option<BorrowSnapshot> =
            persistent.get(&DataKey::MarginBorrowSnapshots(position_id));
        let snapshot = if let Some(snapshot) = snap {
            snapshot
        } else if has_borrowed.unwrap_or(false)
            && persistent.has(&DataKey::MarginBorrowPrincipal(position_id))
        {
            let principal: u128 = persistent
                .get(&DataKey::MarginBorrowPrincipal(position_id))
                .expect("canonical margin principal missing");
            let current_index: u128 = persistent
                .get(&DataKey::BorrowIndex)
                .expect("borrow index missing");
            let rebuilt = BorrowSnapshot {
                principal,
                interest_index: current_index,
            };
            persistent.set(&DataKey::MarginBorrowSnapshots(position_id), &rebuilt);
            persistent.set(&DataKey::MarginHasBorrowed(position_id), &(principal > 0));
            bump_margin_borrow_state_ttl(&env, position_id);
            rebuilt
        } else {
            if has_borrowed.unwrap_or(false) {
                panic!("margin borrow snapshot missing");
            }
            if has_borrowed.is_none() {
                panic!("margin borrow state missing");
            }
            return 0u128;
        };
        if snapshot.principal == 0 {
            return 0u128;
        }
        if snapshot.interest_index == 0 {
            panic!("invalid margin borrower index");
        }
        let current_index: u128 = persistent
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        (snapshot.principal.saturating_mul(current_index)) / snapshot.interest_index
    }

    /// Permissionless TTL extension for position-scoped margin borrow state.
    pub fn bump_margin_borrow_ttl(env: Env, position_id: u64) {
        let _ = ensure_initialized(&env);
        bump_margin_borrow_state_ttl(&env, position_id);
    }

    /// Permissionless recovery path for missing user borrow snapshots.
    /// Rebuilds the snapshot from canonical principal stored in-vault.
    pub fn recover_borrow_snapshot(env: Env, user: Address) {
        let _ = ensure_initialized(&env);
        let persistent = env.storage().persistent();
        if persistent
            .get::<_, BorrowSnapshot>(&DataKey::BorrowSnapshots(user.clone()))
            .is_some()
        {
            panic!("borrow snapshot exists");
        }
        let principal: u128 = persistent
            .get(&DataKey::BorrowPrincipal(user.clone()))
            .expect("canonical borrow principal missing");
        let current_index: u128 = persistent
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        let snapshot = BorrowSnapshot {
            principal,
            interest_index: current_index,
        };
        persistent.set(&DataKey::BorrowSnapshots(user.clone()), &snapshot);
        persistent.set(&DataKey::HasBorrowed(user.clone()), &(principal > 0));
        bump_user_borrow_state_ttl(&env, &user);
    }

    /// Permissionless recovery path for missing margin borrow snapshots.
    /// Rebuilds the snapshot from canonical principal stored in-vault.
    pub fn recover_margin_snapshot(env: Env, position_id: u64) {
        let _ = ensure_initialized(&env);
        let persistent = env.storage().persistent();
        if persistent
            .get::<_, BorrowSnapshot>(&DataKey::MarginBorrowSnapshots(position_id))
            .is_some()
        {
            panic!("margin borrow snapshot exists");
        }
        let principal: u128 = persistent
            .get(&DataKey::MarginBorrowPrincipal(position_id))
            .expect("canonical margin principal missing");
        let current_index: u128 = persistent
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        let snapshot = BorrowSnapshot {
            principal,
            interest_index: current_index,
        };
        persistent.set(&DataKey::MarginBorrowSnapshots(position_id), &snapshot);
        persistent.set(&DataKey::MarginHasBorrowed(position_id), &(principal > 0));
        bump_margin_borrow_state_ttl(&env, position_id);
    }

    /// Permissionless migration/keepalive path for user borrow state.
    /// Seeds canonical principal mirrors from existing snapshots and bumps TTL.
    pub fn migrate_borrow_state_batch(env: Env, users: Vec<Address>) {
        let _ = ensure_initialized(&env);
        let persistent = env.storage().persistent();
        for i in 0..users.len() {
            let user = users.get(i).unwrap();
            if let Some(snapshot) =
                persistent.get::<_, BorrowSnapshot>(&DataKey::BorrowSnapshots(user.clone()))
            {
                persistent.set(&DataKey::BorrowPrincipal(user.clone()), &snapshot.principal);
            }
            bump_user_borrow_state_ttl(&env, &user);
        }
        persistent.set(&DataKey::DebtStateVersion, &DEBT_STATE_VERSION_V1);
        persistent.set(&DataKey::DebtStateMigratedAt, &env.ledger().timestamp());
        bump_core_ttl(&env);
    }

    /// Permissionless migration/keepalive path for margin borrow state.
    pub fn migrate_margin_state_batch(env: Env, position_ids: Vec<u64>) {
        let _ = ensure_initialized(&env);
        let persistent = env.storage().persistent();
        for i in 0..position_ids.len() {
            let position_id = position_ids.get(i).unwrap();
            if let Some(snapshot) =
                persistent.get::<_, BorrowSnapshot>(&DataKey::MarginBorrowSnapshots(position_id))
            {
                persistent.set(
                    &DataKey::MarginBorrowPrincipal(position_id),
                    &snapshot.principal,
                );
            }
            bump_margin_borrow_state_ttl(&env, position_id);
        }
        persistent.set(&DataKey::DebtStateVersion, &DEBT_STATE_VERSION_V1);
        persistent.set(&DataKey::DebtStateMigratedAt, &env.ledger().timestamp());
        bump_core_ttl(&env);
    }

    /// Admin recovery path for a missing/expired margin-position borrow snapshot.
    pub fn recover_margin_borrow_snapshot(
        env: Env,
        admin: Address,
        position_id: u64,
        principal: u128,
        interest_index: u128,
    ) {
        let _ = ensure_initialized(&env);
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if stored_admin != admin {
            panic!("not admin");
        }
        admin.require_auth();
        if interest_index == 0 {
            panic!("invalid borrow index");
        }
        let snap = BorrowSnapshot {
            principal,
            interest_index,
        };
        env.storage()
            .persistent()
            .set(&DataKey::MarginBorrowSnapshots(position_id), &snap);
        env.storage()
            .persistent()
            .set(&DataKey::MarginHasBorrowed(position_id), &(principal > 0));
        env.storage()
            .persistent()
            .set(&DataKey::MarginBorrowPrincipal(position_id), &principal);
        bump_margin_borrow_state_ttl(&env, position_id);
    }

    /// Permissionless TTL extension for per-user borrow state.
    /// Keepers can call this periodically for active borrowers.
    pub fn bump_user_borrow_ttl(env: Env, user: Address) {
        let _ = ensure_initialized(&env);
        bump_user_borrow_state_ttl(&env, &user);
    }

    /// Admin recovery path for a missing/expired borrower snapshot.
    /// Intended for keeper-assisted repair after TTL expiry.
    pub fn recover_user_borrow_snapshot(
        env: Env,
        admin: Address,
        user: Address,
        principal: u128,
        interest_index: u128,
    ) {
        let _ = ensure_initialized(&env);
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        if stored_admin != admin {
            panic!("not admin");
        }
        admin.require_auth();
        if interest_index == 0 {
            panic!("invalid borrow index");
        }
        let snap = BorrowSnapshot {
            principal,
            interest_index,
        };
        env.storage()
            .persistent()
            .set(&DataKey::BorrowSnapshots(user.clone()), &snap);
        env.storage()
            .persistent()
            .set(&DataKey::HasBorrowed(user.clone()), &(principal > 0));
        env.storage()
            .persistent()
            .set(&DataKey::BorrowPrincipal(user.clone()), &principal);
        bump_user_borrow_state_ttl(&env, &user);
    }

    /// Internal: write user's borrow snapshot
    fn write_borrow_snapshot(env: &Env, user: Address, principal: u128) {
        let current_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        let snap = BorrowSnapshot {
            principal,
            interest_index: current_index,
        };
        env.storage()
            .persistent()
            .set(&DataKey::BorrowSnapshots(user.clone()), &snap);
        env.storage()
            .persistent()
            .set(&DataKey::HasBorrowed(user.clone()), &(principal > 0));
        bump_user_borrow_state_ttl(env, &user);
    }

    fn write_margin_borrow_snapshot(env: &Env, position_id: u64, principal: u128) {
        let current_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        let snap = BorrowSnapshot {
            principal,
            interest_index: current_index,
        };
        env.storage()
            .persistent()
            .set(&DataKey::MarginBorrowSnapshots(position_id), &snap);
        env.storage()
            .persistent()
            .set(&DataKey::MarginHasBorrowed(position_id), &(principal > 0));
        bump_margin_borrow_state_ttl(env, position_id);
    }

    /// Repayment amount applied to principal (interest-only repayment does not reduce principal).
    fn principal_component_of_repay(
        env: &Env,
        user: &Address,
        current_debt: u128,
        repay_amount: u128,
    ) -> u128 {
        let snapshot = env
            .storage()
            .persistent()
            .get::<_, BorrowSnapshot>(&DataKey::BorrowSnapshots(user.clone()));
        let Some(snapshot) = snapshot else {
            return 0u128;
        };
        let accrued_interest = current_debt.saturating_sub(snapshot.principal);
        repay_amount
            .saturating_sub(accrued_interest)
            .min(snapshot.principal)
    }

    fn principal_component_of_margin_repay(
        env: &Env,
        position_id: u64,
        current_debt: u128,
        repay_amount: u128,
    ) -> u128 {
        let snapshot = env
            .storage()
            .persistent()
            .get::<_, BorrowSnapshot>(&DataKey::MarginBorrowSnapshots(position_id));
        let Some(snapshot) = snapshot else {
            return 0u128;
        };
        let accrued_interest = current_debt.saturating_sub(snapshot.principal);
        repay_amount
            .saturating_sub(accrued_interest)
            .min(snapshot.principal)
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
        Self::ensure_not_in_flash_loan(&env);
        Self::ensure_user_borrow_flag(&env, &user);
        Self::update_interest(env.clone());
        let storage = env.storage().persistent();
        bump_rates_ready_ttl(&env);
        let rates_ready = storage
            .get::<_, bool>(&DataKey::RatesReady)
            .unwrap_or_else(|| storage.get::<_, Address>(&DataKey::InterestModel).is_some());
        if !rates_ready {
            panic!("rates not configured");
        }
        ensure_user_auth(&env, &user);
        let mut user_ptokens_before: u128 = 0;
        let mut user_borrow_before: u128 = 0;
        let mut exchange_rate: u128 = 0;
        if let Some(_comp_addr) = env
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
            Self::accrue_user_rewards(&env, &user, hint, "borrow");
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
            let principal_total: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowPrincipal)
                .unwrap_or_else(|| {
                    env.storage()
                        .persistent()
                        .get(&DataKey::TotalBorrowed)
                        .expect("total borrowed missing")
                });
            if principal_total.saturating_add(amount) > bcap {
                panic!("borrow cap exceeded");
            }
        }

        // Pull liquidity from boosted vault only when managed cash indicates a shortfall.
        // This avoids extra token-balance reads on the common non-boosted path.
        let managed_cash = Self::get_managed_cash(&env);
        if managed_cash < amount {
            Self::ensure_liquid_cash(&env, &token_address, amount);
            let cash_for_borrow = Self::current_live_cash(&env, &token_address);
            if cash_for_borrow < amount {
                panic!("borrow liquidity shortfall");
            }
        }

        // Update totals and user snapshot
        let new_principal =
            Self::get_user_borrow_balance(env.clone(), user.clone()).saturating_add(amount);
        Self::write_borrow_snapshot(&env, user.clone(), new_principal);

        if bcap > 0 {
            let total_principal_before: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowPrincipal)
                .unwrap_or_else(|| {
                    env.storage()
                        .persistent()
                        .get(&DataKey::TotalBorrowed)
                        .expect("total borrowed missing")
                });
            env.storage().persistent().set(
                &DataKey::TotalBorrowPrincipal,
                &total_principal_before.saturating_add(amount),
            );
        }

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
        let cash_before = Self::current_live_cash(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &user, &amount_i128);
        let cash_after = Self::current_live_cash(&env, &token_address);
        Self::sub_managed_cash(&env, cash_before.saturating_sub(cash_after));

        // Emit event
        BorrowEvent {
            borrower: user.clone(),
            borrow_amount: amount,
            account_borrows: new_principal,
            total_borrows,
        }
        .publish(&env);
    }

    /// Borrow into a margin position namespace.
    /// Callable only by the configured margin controller.
    pub fn init_margin_borrow_state(env: Env, position_id: u64) {
        let _ = ensure_initialized(&env);
        let _margin_controller = Self::require_margin_controller_auth(&env);
        let has_snapshot = env
            .storage()
            .persistent()
            .get::<_, BorrowSnapshot>(&DataKey::MarginBorrowSnapshots(position_id))
            .is_some();
        if has_snapshot {
            let snapshot: BorrowSnapshot = env
                .storage()
                .persistent()
                .get(&DataKey::MarginBorrowSnapshots(position_id))
                .expect("margin borrow snapshot missing");
            env.storage().persistent().set(
                &DataKey::MarginHasBorrowed(position_id),
                &(snapshot.principal > 0),
            );
            bump_margin_borrow_state_ttl(&env, position_id);
            return;
        }
        let has_flag = env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::MarginHasBorrowed(position_id))
            .is_some();
        if !has_flag {
            env.storage()
                .persistent()
                .set(&DataKey::MarginHasBorrowed(position_id), &false);
        }
        bump_margin_borrow_state_ttl(&env, position_id);
    }

    /// Borrow into a margin position namespace.
    /// Callable only by the configured margin controller.
    pub fn borrow_for_margin(env: Env, position_id: u64, receiver: Address, amount: u128) {
        let token_address = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        Self::update_interest(env.clone());
        bump_rates_ready_ttl(&env);
        let storage = env.storage().persistent();
        let rates_ready = storage
            .get::<_, bool>(&DataKey::RatesReady)
            .unwrap_or_else(|| storage.get::<_, Address>(&DataKey::InterestModel).is_some());
        if !rates_ready {
            panic!("rates not configured");
        }
        let margin_controller = Self::require_margin_controller_auth(&env);
        let owner = Self::require_margin_position_owner(&env, &margin_controller, position_id);
        receiver.require_auth();
        if receiver != owner {
            panic!("receiver must be position owner");
        }
        Self::ensure_margin_position_borrow_flag(&env, position_id);
        if amount == 0 {
            panic!("bad amount");
        }

        let available = Self::get_available_liquidity(env.clone());
        if available < amount {
            panic!("Not enough liquidity to borrow");
        }

        let bcap: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowCap)
            .unwrap_or(0u128);
        if bcap > 0 {
            let principal_total: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowPrincipal)
                .unwrap_or_else(|| {
                    env.storage()
                        .persistent()
                        .get(&DataKey::TotalBorrowed)
                        .expect("total borrowed missing")
                });
            if principal_total.saturating_add(amount) > bcap {
                panic!("borrow cap exceeded");
            }
        }

        let managed_cash = Self::get_managed_cash(&env);
        if managed_cash < amount {
            Self::ensure_liquid_cash(&env, &token_address, amount);
            let cash_for_borrow = Self::current_live_cash(&env, &token_address);
            if cash_for_borrow < amount {
                panic!("borrow liquidity shortfall");
            }
        }

        let current = Self::get_margin_borrow_balance(env.clone(), position_id);
        let new_principal = current.saturating_add(amount);
        Self::write_margin_borrow_snapshot(&env, position_id, new_principal);

        if bcap > 0 {
            let total_principal_before: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowPrincipal)
                .unwrap_or_else(|| {
                    env.storage()
                        .persistent()
                        .get(&DataKey::TotalBorrowed)
                        .expect("total borrowed missing")
                });
            env.storage().persistent().set(
                &DataKey::TotalBorrowPrincipal,
                &total_principal_before.saturating_add(amount),
            );
        }

        let tb: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .expect("total borrowed missing");
        let total_borrows = tb.saturating_add(amount);
        env.storage()
            .persistent()
            .set(&DataKey::TotalBorrowed, &total_borrows);

        let token_client = token::Client::new(&env, &token_address);
        let amount_i128 = to_i128(amount);
        let cash_before = Self::current_live_cash(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &receiver, &amount_i128);
        let cash_after = Self::current_live_cash(&env, &token_address);
        Self::sub_managed_cash(&env, cash_before.saturating_sub(cash_after));
    }

    /// Repay borrowed tokens
    pub fn repay(env: Env, user: Address, amount: u128) {
        let token_address = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        Self::ensure_user_borrow_flag(&env, &user);
        // Compute a deterministic repay cap from pre-accrual state so auth entries
        // do not depend on time-elapsed interest updates between simulation and execution.
        let debt_before_accrual = Self::get_user_borrow_balance(env.clone(), user.clone());
        let planned_repay = if amount > debt_before_accrual {
            debt_before_accrual
        } else {
            amount
        };
        Self::update_interest(env.clone());
        ensure_user_auth(&env, &user);
        let current_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
        if let Some(_comp_addr) = env
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
            Self::accrue_user_rewards(&env, &user, hint, "repay");
        }

        if current_debt == 0 {
            return;
        }
        let repay_amount = if planned_repay > current_debt {
            current_debt
        } else {
            planned_repay
        };
        let principal_repay_user =
            Self::principal_component_of_repay(&env, &user, current_debt, repay_amount);

        // Transfer tokens from user
        let token_client = token::Client::new(&env, &token_address);
        let repay_i128 = to_i128(repay_amount);
        let cash_before = Self::current_live_cash(&env, &token_address);
        token_client.transfer(&user, &env.current_contract_address(), &repay_i128);
        let cash_after = Self::current_live_cash(&env, &token_address);
        Self::add_managed_cash(&env, cash_after.saturating_sub(cash_before));

        // Update snapshot and totals
        let new_principal = current_debt - repay_amount;
        Self::write_borrow_snapshot(&env, user.clone(), new_principal);

        let bcap: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowCap)
            .unwrap_or(0u128);
        if bcap > 0 {
            let total_principal_before: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowPrincipal)
                .unwrap_or_else(|| {
                    env.storage()
                        .persistent()
                        .get(&DataKey::TotalBorrowed)
                        .expect("total borrowed missing")
                });
            let principal_repay_global = principal_repay_user.min(total_principal_before);
            let total_principal_after = total_principal_before - principal_repay_global;
            env.storage()
                .persistent()
                .set(&DataKey::TotalBorrowPrincipal, &total_principal_after);
        }

        let tb: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .expect("total borrowed missing");
        let tb_after = tb
            .checked_sub(repay_amount)
            .expect("repay exceeds total borrowed");
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

    /// Repay debt tracked in a margin position namespace.
    /// Callable only by the configured margin controller.
    pub fn repay_for_margin(env: Env, position_id: u64, payer: Address, amount: u128) {
        let token_address = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        let margin_controller = Self::require_margin_controller_auth(&env);
        let _owner = Self::require_margin_position_owner(&env, &margin_controller, position_id);
        Self::ensure_margin_position_borrow_flag(&env, position_id);
        let debt_before_accrual = Self::get_margin_borrow_balance(env.clone(), position_id);
        let planned_repay = if amount > debt_before_accrual {
            debt_before_accrual
        } else {
            amount
        };
        Self::update_interest(env.clone());
        let current_debt = Self::get_margin_borrow_balance(env.clone(), position_id);
        if current_debt == 0 {
            return;
        }
        let repay_amount = if planned_repay > current_debt {
            current_debt
        } else {
            planned_repay
        };
        let principal_repay_position = Self::principal_component_of_margin_repay(
            &env,
            position_id,
            current_debt,
            repay_amount,
        );

        payer.require_auth();
        let token_client = token::Client::new(&env, &token_address);
        let repay_i128 = to_i128(repay_amount);
        let cash_before = Self::current_live_cash(&env, &token_address);
        token_client.transfer(&payer, &env.current_contract_address(), &repay_i128);
        let cash_after = Self::current_live_cash(&env, &token_address);
        Self::add_managed_cash(&env, cash_after.saturating_sub(cash_before));

        let new_principal = current_debt - repay_amount;
        Self::write_margin_borrow_snapshot(&env, position_id, new_principal);

        let bcap: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowCap)
            .unwrap_or(0u128);
        if bcap > 0 {
            let total_principal_before: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowPrincipal)
                .unwrap_or_else(|| {
                    env.storage()
                        .persistent()
                        .get(&DataKey::TotalBorrowed)
                        .expect("total borrowed missing")
                });
            let principal_repay_global = principal_repay_position.min(total_principal_before);
            let total_principal_after = total_principal_before - principal_repay_global;
            env.storage()
                .persistent()
                .set(&DataKey::TotalBorrowPrincipal, &total_principal_after);
        }

        let tb: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .expect("total borrowed missing");
        let tb_after = tb
            .checked_sub(repay_amount)
            .expect("repay exceeds total borrowed");
        env.storage()
            .persistent()
            .set(&DataKey::TotalBorrowed, &tb_after);
    }

    /// Execute a flash loan to `receiver`. Receiver must return `amount + fee` within the callback.
    pub fn flash_loan(env: Env, receiver: Address, amount: u128, data: Bytes) {
        if amount == 0 {
            panic!("invalid flash amount");
        }
        let token_address = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
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

        // Pull from boosted vault on demand so flash loans are backed by live cash.
        // Do this before taking the pre-loan balance snapshot used for repayment checks.
        Self::ensure_liquid_cash(&env, &token_address, amount);
        let cash_for_flash = Self::current_live_cash(&env, &token_address);
        if cash_for_flash < amount {
            panic!("flash loan liquidity shortfall");
        }

        let fee_scaled: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::FlashLoanFeeScaled)
            .unwrap_or(0u128);
        let fee_numerator = amount.saturating_mul(fee_scaled);
        let fee = if fee_numerator == 0 {
            0u128
        } else {
            fee_numerator.saturating_sub(1) / SCALE_1E6 + 1
        };

        let token_client = token::Client::new(&env, &token_address);

        let balance_before_i: i128 = token_client.balance(&env.current_contract_address());
        if balance_before_i < 0 {
            panic!("invalid cash state");
        }
        let balance_before = balance_before_i as u128;

        // Receiver must explicitly authorize being targeted as a flash loan callback.
        receiver.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::FlashLoanActive, &true);
        Self::sub_managed_cash(&env, amount);
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
        let returned = balance_after.saturating_sub(balance_before.saturating_sub(amount));
        if returned > 0 {
            Self::add_managed_cash(&env, returned);
        }
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
        env.storage().persistent().remove(&DataKey::FlashLoanActive);

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
        Self::ensure_not_in_flash_loan(&env);
        Self::ensure_user_borrow_flag(&env, &borrower);
        // Compute a deterministic repay cap from pre-accrual state so auth entries
        // do not depend on time-elapsed interest updates between simulation and execution.
        let debt_before_accrual = Self::get_user_borrow_balance(env.clone(), borrower.clone());
        let planned_repay = if amount > debt_before_accrual {
            debt_before_accrual
        } else {
            amount
        };
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
        let repay_amount = if planned_repay > current_debt {
            current_debt
        } else {
            planned_repay
        };
        let principal_repay_user =
            Self::principal_component_of_repay(&env, &borrower, current_debt, repay_amount);

        // Pull repayment from liquidator allowance. This avoids requiring liquidator
        // sub-invocation auth entries that depend on dynamic repay amounts.
        let token_client = token::Client::new(&env, &token_address);
        let repay_i128 = to_i128(repay_amount);
        let cash_before = Self::current_live_cash(&env, &token_address);
        token_client.transfer_from(
            &env.current_contract_address(),
            &liquidator,
            &env.current_contract_address(),
            &repay_i128,
        );
        let cash_after = Self::current_live_cash(&env, &token_address);
        Self::add_managed_cash(&env, cash_after.saturating_sub(cash_before));

        // Update borrower snapshot and totals
        let new_principal = current_debt - repay_amount;
        Self::write_borrow_snapshot(&env, borrower.clone(), new_principal);

        let bcap: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowCap)
            .unwrap_or(0u128);
        if bcap > 0 {
            let total_principal_before: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalBorrowPrincipal)
                .unwrap_or_else(|| {
                    env.storage()
                        .persistent()
                        .get(&DataKey::TotalBorrowed)
                        .expect("total borrowed missing")
                });
            let principal_repay_global = principal_repay_user.min(total_principal_before);
            let total_principal_after = total_principal_before - principal_repay_global;
            env.storage()
                .persistent()
                .set(&DataKey::TotalBorrowPrincipal, &total_principal_after);
        }

        let tb: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .expect("total borrowed missing");
        let tb_after = tb
            .checked_sub(repay_amount)
            .expect("repay exceeds total borrowed");
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
        // Do not block liquidations based on redeem previews. A precomputed
        // shortfall already proves insolvency at liquidation initiation.
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
