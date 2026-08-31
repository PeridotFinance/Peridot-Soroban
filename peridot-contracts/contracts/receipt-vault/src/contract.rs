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

#[cfg(all(feature = "test-default-admin", target_arch = "wasm32"))]
compile_error!("receipt-vault test-default-admin must not be enabled for Wasm builds");

#[contract]
pub struct ReceiptVault;

pub const DEFAULT_INIT_ADMIN: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";
const DISABLED_BOOSTED_VAULT: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const BOOSTED_CACHE_MAX_AGE_SECS: u64 = 60 * 60;
// Account-health checks must not synchronously load a boosted strategy. Keep
// this much tighter than the general accounting fallback because a borrow may
// rely on the cached value as collateral.
const BOOSTED_HEALTH_CACHE_MAX_AGE_SECS: u64 = 5 * 60;
const BPS_SCALE: u128 = 10_000u128;
const BOOSTED_MODEL_CASH_TOLERANCE_BPS: u128 = 500u128; // 5%
const BOOSTED_REDEMPTION_QUOTE_FLOOR_BPS: u128 = 9_000u128; // max 10% downward jump
const MAX_BOOSTED_ASSETS: u32 = 16;
const DEBT_STATE_VERSION_V1: u32 = 1u32;

#[contractimpl]
impl ReceiptVault {
    fn ensure_fee_factors_within_cap(reserve_factor_scaled: u128, admin_fee_scaled: u128) {
        if reserve_factor_scaled > SCALE_1E6
            || admin_fee_scaled > SCALE_1E6
            || reserve_factor_scaled.saturating_add(admin_fee_scaled) > SCALE_1E6
        {
            panic!("fee factors exceed 100%");
        }
    }

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

    fn record_boosted_health_cache(env: &Env, value: u128) {
        let cache = BoostedHealthCache {
            underlying: value,
            updated_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::BoostedHealthCache, &cache);
    }

    fn invalidate_boosted_health_cache(env: &Env) {
        env.storage()
            .persistent()
            .remove(&DataKey::BoostedHealthCache);
    }

    /// Keep an existing live-quote cache aligned with exact asset movements
    /// without extending its freshness window. Routine deposits/redemptions
    /// must not let users delete the cache or make an old quote look new.
    fn adjust_existing_boosted_health_cache(env: &Env, amount: u128, add: bool) {
        let persistent = env.storage().persistent();
        let Some(mut cache) = persistent.get::<_, BoostedHealthCache>(&DataKey::BoostedHealthCache)
        else {
            return;
        };
        cache.underlying = if add {
            cache
                .underlying
                .checked_add(amount)
                .expect("boosted health cache overflow")
        } else {
            cache.underlying.saturating_sub(amount)
        };
        persistent.set(&DataKey::BoostedHealthCache, &cache);
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

    fn boosted_underlying_redemption_baseline(env: &Env) -> u128 {
        let cached = Self::cached_boosted_underlying(env);
        let estimated = Self::estimate_boosted_underlying_from_accounting(env);
        if estimated > 0 {
            // Book accounting is independent of a manipulable live strategy
            // quote. An inflated or dust-poisoned cache must not determine how
            // many strategy shares a fixed cash request unwinds.
            estimated
        } else {
            cached
        }
    }

    fn validate_boosted_asset_count(asset_count: u32) -> u32 {
        if asset_count == 0 || asset_count > MAX_BOOSTED_ASSETS {
            panic!("invalid boosted asset count");
        }
        asset_count
    }

    fn stored_boosted_asset_count(env: &Env) -> Option<u32> {
        env.storage()
            .persistent()
            .get::<_, u32>(&DataKey::BoostedAssetCount)
            .map(Self::validate_boosted_asset_count)
    }

    fn record_boosted_asset_count(env: &Env, asset_count: u32) -> u32 {
        let asset_count = Self::validate_boosted_asset_count(asset_count);
        if let Some(expected) = Self::stored_boosted_asset_count(env) {
            if expected != asset_count {
                panic!("boosted asset count changed");
            }
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::BoostedAssetCount, &asset_count);
        }
        asset_count
    }

    /// Recovers the output-vector shape for a legacy or archived binding
    /// without depending on a live NAV quote. DefIndex-compatible strategies,
    /// including AquariusLpVault, return their asset-shaped zero vector before
    /// performing price-sensitive work when asked to quote zero shares.
    fn probe_boosted_asset_count(env: &Env, boosted: &Address) -> Option<u32> {
        match try_call_contract::<Vec<i128>, _>(
            env,
            boosted,
            "get_asset_amounts_per_shares",
            (0i128,),
        ) {
            Ok(amounts) if amounts.len() > 0 => {
                Some(Self::record_boosted_asset_count(env, amounts.len()))
            }
            Ok(_) => None,
            Err(ref err) => {
                emit_external_call_failure(env, boosted, err, true);
                None
            }
        }
    }

    fn get_boosted_underlying(env: &Env) -> u128 {
        let boosted_key = DataKey::BoostedVault;
        if let Some(boosted) = env.storage().persistent().get::<_, Address>(&boosted_key) {
            bump_boosted_vault_ttl(env);
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
                        if amounts.len() > 0 {
                            Self::record_boosted_asset_count(env, amounts.len());
                        }
                        let amt_i = amounts.get(0).unwrap_or(0);
                        if amt_i <= 0 {
                            Self::invalidate_boosted_health_cache(env);
                            let cached: u128 = env
                                .storage()
                                .persistent()
                                .get(&DataKey::BoostedUnderlyingCached)
                                .unwrap_or(0u128);
                            let estimated = Self::estimate_boosted_underlying_from_accounting(env);
                            return cached.max(estimated);
                        }
                        // Live NAV always marks the market up or down. Never
                        // replace a real loss with a higher accounting value.
                        let boosted_underlying = amt_i as u128;
                        env.storage()
                            .persistent()
                            .set(&DataKey::BoostedUnderlyingCached, &boosted_underlying);
                        env.storage().persistent().set(
                            &DataKey::BoostedUnderlyingUpdatedAt,
                            &env.ledger().timestamp(),
                        );
                        Self::record_boosted_health_cache(env, boosted_underlying);
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
                env.storage()
                    .persistent()
                    .set(&DataKey::BoostedUnderlyingCached, &0u128);
                env.storage().persistent().set(
                    &DataKey::BoostedUnderlyingUpdatedAt,
                    &env.ledger().timestamp(),
                );
                Self::record_boosted_health_cache(env, 0u128);
                0u128
            }
        } else {
            0u128
        }
    }

    fn get_boosted_underlying_for_account_health(env: &Env) -> u128 {
        let persistent = env.storage().persistent();
        let boosted_key = DataKey::BoostedVault;
        if persistent.get::<_, Address>(&boosted_key).is_none() {
            return 0u128;
        }

        let cache: BoostedHealthCache = persistent
            .get(&DataKey::BoostedHealthCache)
            .expect("boosted health cache missing");
        if env.ledger().timestamp().saturating_sub(cache.updated_at)
            > BOOSTED_HEALTH_CACHE_MAX_AGE_SECS
        {
            panic!("boosted health cache stale");
        }
        bump_boosted_health_cache_ttl(env);
        cache.underlying
    }

    fn get_total_underlying_for_account_health(env: &Env) -> u128 {
        bump_account_snapshot_valuation_ttl(env);
        let cash = Self::get_managed_cash(env);
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
        let boosted_underlying = Self::get_boosted_underlying_for_account_health(env);
        cash.saturating_add(boosted_underlying)
            .saturating_add(borrows)
            .saturating_sub(reserves)
            .saturating_sub(admin_fees)
    }

    fn get_exchange_rate_for_account_health(env: &Env) -> u128 {
        let total_ptokens = total_ptokens_supply(env);
        let total_underlying = Self::get_total_underlying_for_account_health(env);
        if total_ptokens == 0 {
            if total_underlying > 0 {
                panic!("non-empty vault at zero supply");
            }
            return env
                .storage()
                .persistent()
                .get(&DataKey::InitialExchangeRate)
                .unwrap_or(SCALE_1E6);
        }
        if total_underlying == 0 {
            panic!("invalid underlying state");
        }
        total_underlying
            .checked_mul(SCALE_1E6)
            .expect("exchange rate overflow")
            / total_ptokens
    }

    fn get_available_liquidity_for_borrow(env: &Env) -> u128 {
        let total_underlying = Self::get_total_underlying_for_account_health(env);
        let total_borrowed: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .expect("total borrowed missing");
        total_underlying.saturating_sub(total_borrowed)
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

    fn require_exact_repay_received(received: u128, expected: u128) {
        if received != expected {
            panic!("repay transfer shortfall");
        }
    }

    fn sync_user_borrow_state_for_ptoken_read(env: &Env, user: &Address, ptoken_balance: u128) {
        if ptoken_balance == 0 {
            return;
        }
        let persistent = env.storage().persistent();
        let flag_key = DataKey::HasBorrowed(user.clone());
        match persistent.get::<_, bool>(&flag_key) {
            Some(false) => {
                bump_has_borrowed_ttl(env, user);
                return;
            }
            Some(true) => {
                bump_user_borrow_state_ttl(env, user);
                return;
            }
            None => {}
        }

        let has_snapshot = persistent.has(&DataKey::BorrowSnapshots(user.clone()));
        let principal: u128 = persistent
            .get(&DataKey::BorrowPrincipal(user.clone()))
            .unwrap_or(0u128);
        if has_snapshot || principal > 0 {
            persistent.set(&flag_key, &true);
            bump_user_borrow_state_ttl(env, user);
        }
    }

    fn total_borrowed_for_state_repair(env: &Env) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .unwrap_or(0u128)
    }

    fn has_user_borrow_state(env: &Env, user: &Address) -> bool {
        let persistent = env.storage().persistent();
        persistent.has(&DataKey::HasBorrowed(user.clone()))
            || persistent.has(&DataKey::BorrowSnapshots(user.clone()))
            || persistent.has(&DataKey::BorrowPrincipal(user.clone()))
    }

    fn mark_user_not_borrowed_if_state_missing(env: &Env, user: &Address) {
        if !Self::has_user_borrow_state(env, user) {
            env.storage()
                .persistent()
                .set(&DataKey::HasBorrowed(user.clone()), &false);
        }
        bump_user_borrow_live_ttl(env, user);
    }

    fn read_ptoken_balance_with_borrow_ttl(env: &Env, user: &Address) -> u128 {
        let balance = ptoken_balance(env, user);
        Self::sync_user_borrow_state_for_ptoken_read(env, user, balance);
        balance
    }

    fn user_borrow_principal(env: &Env, user: &Address) -> u128 {
        let persistent = env.storage().persistent();
        let principal_key = DataKey::BorrowPrincipal(user.clone());
        if let Some(principal) = persistent.get::<_, u128>(&principal_key) {
            bump_borrow_principal_ttl(env, user);
            return principal;
        }
        if let Some(snapshot) =
            persistent.get::<_, BorrowSnapshot>(&DataKey::BorrowSnapshots(user.clone()))
        {
            if snapshot.principal == 0
                && persistent.get::<_, bool>(&DataKey::HasBorrowed(user.clone())) != Some(true)
            {
                return 0u128;
            }
            panic!("borrow principal missing");
        }
        0u128
    }

    fn margin_borrow_principal(env: &Env, position_id: u64) -> u128 {
        let persistent = env.storage().persistent();
        let principal_key = DataKey::MarginBorrowPrincipal(position_id);
        if let Some(principal) = persistent.get::<_, u128>(&principal_key) {
            bump_margin_borrow_principal_ttl(env, position_id);
            return principal;
        }
        if let Some(snapshot) =
            persistent.get::<_, BorrowSnapshot>(&DataKey::MarginBorrowSnapshots(position_id))
        {
            if snapshot.principal == 0
                && persistent.get::<_, bool>(&DataKey::MarginHasBorrowed(position_id)) != Some(true)
            {
                return 0u128;
            }
            panic!("margin borrow principal missing");
        }
        0u128
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
        if deploy_amount < MIN_BOOSTED_DEPLOY_AMOUNT {
            return 0u128;
        }

        let deploy_i128 = to_i128(deploy_amount);
        let mut amounts_desired: Vec<i128> = Vec::new(env);
        let mut amounts_min: Vec<i128> = Vec::new(env);
        amounts_desired.push_back(deploy_i128);
        amounts_min.push_back(deploy_i128);
        let transfer_args: Vec<Val> =
            (env.current_contract_address(), boosted.clone(), deploy_i128).into_val(env);
        let mut auths = Vec::new(env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token_address.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: transfer_args,
            },
            sub_invocations: Vec::new(env),
        }));

        let cash_before_boost = Self::current_live_cash(env, token_address);
        env.authorize_as_current_contract(auths);
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
        if moved > deploy_amount {
            panic!("boosted vault exceeded deposit authorization");
        }
        if moved > 0 {
            Self::sub_managed_cash(env, moved);
            let cached = Self::cached_boosted_underlying(env);
            let updated_cached = cached.checked_add(moved).expect("boosted cache overflow");
            env.storage()
                .persistent()
                .set(&DataKey::BoostedUnderlyingCached, &updated_cached);
            env.storage().persistent().set(
                &DataKey::BoostedUnderlyingUpdatedAt,
                &env.ledger().timestamp(),
            );
            Self::adjust_existing_boosted_health_cache(env, moved, true);
        }
        moved
    }

    fn boosted_shares_for_cash(
        target_cash: u128,
        total_shares: u128,
        total_underlying: u128,
        share_balance: u128,
    ) -> u128 {
        if total_underlying == 0 || share_balance == 0 {
            return 0;
        }
        // Add a tiny buffer for share rounding so downstream payout paths are
        // less brittle to 1-unit quote/withdraw drift in boosted vault math.
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
        shares_to_withdraw
    }

    fn try_boosted_redemption(
        env: &Env,
        boosted: &Address,
        token_address: &Address,
        shares_to_withdraw: u128,
        asset_count: u32,
        needed_cash: u128,
        recoverable_failure: bool,
    ) -> (bool, u128) {
        if shares_to_withdraw == 0 {
            return (false, 0);
        }
        // Build min_amounts_out with the same number of elements as the boosted vault's
        // asset list (one per managed asset). The underlying leg carries the
        // cash requirement as a non-zero floor. That lets a fail-soft boosted
        // strategy use a cached exit ratio during an oracle outage without
        // trusting the pool's quote unconditionally; the whole transaction
        // reverts if the strategy cannot return the requested cash.
        let mut min_amounts_out: Vec<i128> = Vec::new(env);
        for idx in 0..asset_count {
            if idx == 0 {
                min_amounts_out.push_back(to_i128(needed_cash));
            } else {
                min_amounts_out.push_back(0i128);
            }
        }
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
        // Use try_call_contract so a boosted-vault failure surfaces as
        // "withdraw liquidity shortfall" in the caller rather than an opaque panic.
        let result: Result<Val, _> = try_call_contract(
            env,
            &boosted,
            "withdraw",
            (
                to_i128(shares_to_withdraw),
                min_amounts_out,
                env.current_contract_address(),
            ),
        );
        if let Err(ref e) = result {
            emit_external_call_failure(env, boosted, e, recoverable_failure);
        }
        let cash_after = Self::current_live_cash(env, token_address);
        let received = cash_after.saturating_sub(cash_before);
        // Post-withdraw invariant: if shares were redeemed but nothing came back, the boosted
        // vault either charged a 100% fee or malfunctioned. Emit an event so monitors can detect
        // this without panicking (a panic here would DoS all withdrawals).
        if received == 0 && shares_to_withdraw > 0 && result.is_ok() {
            BoostedRedeemZeroReturn {
                shares_redeemed: shares_to_withdraw,
            }
            .publish(env);
        }
        (result.is_ok(), received)
    }

    /// Redeem from boosted vault to satisfy a live-cash requirement.
    fn redeem_from_boosted(env: &Env, token_address: &Address, needed_cash: u128) {
        if needed_cash == 0 {
            return;
        }
        if needed_cash > i128::MAX as u128 {
            panic!("cash requirement exceeds token range");
        }
        let Some(boosted) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::BoostedVault)
        else {
            return;
        };
        // The count is part of the protected-exit schema. Renew it whenever a
        // redemption touches the strategy rather than relying on a keeper to
        // arrive before an otherwise healthy user withdrawal.
        bump_boosted_vault_ttl(env);

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
        let stored_asset_count = Self::stored_boosted_asset_count(env);
        let quote = try_call_contract::<Vec<i128>, _>(
            env,
            &boosted,
            "get_asset_amounts_per_shares",
            (total_shares_i,),
        );
        let (mut total_amounts, quoted_asset_count) = match quote {
            Ok(amounts) if amounts.len() > 0 => {
                let count = Self::record_boosted_asset_count(env, amounts.len());
                (amounts, Some(count))
            }
            Ok(amounts) => (amounts, None),
            Err(ref err) => {
                emit_external_call_failure(env, &boosted, err, true);
                (Vec::new(env), None)
            }
        };
        let asset_count = quoted_asset_count
            .or(stored_asset_count)
            .or_else(|| Self::probe_boosted_asset_count(env, &boosted));
        let Some(asset_count) = asset_count else {
            // Never guess a one-entry vector: a multi-asset strategy would
            // reject it and obscure the actual liquidity outage. Leave the
            // state untouched so the caller reports its standard shortfall;
            // the admin setter remains available if the strategy cannot even
            // answer the zero-share shape probe.
            return;
        };
        if total_amounts.len() == 0 {
            total_amounts.push_back(0i128);
        }
        let quoted_underlying_i = total_amounts.get(0).unwrap_or(0);
        let fallback_underlying = Self::boosted_underlying_redemption_baseline(env);
        let mut retry_underlying: Option<u128> = None;
        let total_underlying = if quoted_underlying_i > 0 {
            let quoted = quoted_underlying_i as u128;
            // A dust-positive live quote can otherwise make this market unwind
            // all strategy shares to satisfy a small cash request. Start from
            // the independent book/cache baseline when the quote drops more
            // than 10%. If that bounded redemption cannot meet the same
            // non-zero output floor, retry once using the lower live quote.
            // This distinguishes a real loss from a lying quote without ever
            // accepting less cash than the caller requested.
            let quote_floor =
                fallback_underlying.saturating_mul(BOOSTED_REDEMPTION_QUOTE_FLOOR_BPS) / BPS_SCALE;
            if fallback_underlying > 0 && quoted < quote_floor {
                retry_underlying = Some(quoted);
                fallback_underlying
            } else {
                quoted
            }
        } else {
            // A boosted strategy may deliberately fail soft when its live NAV
            // source is unavailable. Size the redemption from the last known
            // value/accounting estimate instead of turning that read outage
            // into a market-wide withdrawal panic.
            fallback_underlying
        };
        if total_underlying == 0 {
            return;
        }

        let target_cash = needed_cash.saturating_add(1);
        let shares_to_withdraw = Self::boosted_shares_for_cash(
            target_cash,
            total_shares,
            total_underlying,
            share_balance,
        );
        let retry_shares = retry_underlying
            .map(|live_underlying| {
                Self::boosted_shares_for_cash(
                    target_cash,
                    total_shares,
                    live_underlying,
                    share_balance,
                )
            })
            .filter(|shares| *shares > shares_to_withdraw);

        let (succeeded, mut received) = Self::try_boosted_redemption(
            env,
            &boosted,
            token_address,
            shares_to_withdraw,
            asset_count,
            needed_cash,
            retry_shares.is_some(),
        );
        if !succeeded {
            if let Some(shares) = retry_shares {
                let (_, retry_received) = Self::try_boosted_redemption(
                    env,
                    &boosted,
                    token_address,
                    shares,
                    asset_count,
                    needed_cash,
                    false,
                );
                received = retry_received;
            }
        }

        if received > 0 {
            Self::add_managed_cash(env, received);
            let cached = Self::cached_boosted_underlying(env);
            let updated_cached = cached.saturating_sub(received);
            env.storage()
                .persistent()
                .set(&DataKey::BoostedUnderlyingCached, &updated_cached);
            env.storage().persistent().set(
                &DataKey::BoostedUnderlyingUpdatedAt,
                &env.ledger().timestamp(),
            );
            Self::adjust_existing_boosted_health_cache(env, received, false);
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

    fn compute_flash_loan_fee(amount: u128, fee_scaled: u128) -> u128 {
        let fee_numerator = amount.saturating_mul(fee_scaled);
        if fee_numerator == 0 {
            0u128
        } else {
            fee_numerator.saturating_sub(1) / SCALE_1E6 + 1
        }
    }

    fn ensure_user_borrow_flag(env: &Env, user: &Address) {
        let persistent = env.storage().persistent();
        let has_snapshot = persistent
            .get::<_, BorrowSnapshot>(&DataKey::BorrowSnapshots(user.clone()))
            .is_some();
        let has_borrowed = persistent.get::<_, bool>(&DataKey::HasBorrowed(user.clone()));
        if has_snapshot {
            persistent.set(&DataKey::HasBorrowed(user.clone()), &true);
        } else if has_borrowed != Some(false)
            && persistent
                .get::<_, u128>(&DataKey::BorrowPrincipal(user.clone()))
                .unwrap_or(0)
                > 0
        {
            persistent.set(&DataKey::HasBorrowed(user.clone()), &true);
        } else if has_borrowed.is_none() && ptoken_balance(env, user) == 0 {
            persistent.set(&DataKey::HasBorrowed(user.clone()), &false);
        }
        bump_user_borrow_live_ttl(env, user);
    }

    fn ensure_margin_position_borrow_flag(env: &Env, position_id: u64) {
        let persistent = env.storage().persistent();
        let has_snapshot = persistent
            .get::<_, BorrowSnapshot>(&DataKey::MarginBorrowSnapshots(position_id))
            .is_some();
        let has_borrowed = persistent.get::<_, bool>(&DataKey::MarginHasBorrowed(position_id));
        if has_snapshot {
            persistent.set(&DataKey::MarginHasBorrowed(position_id), &true);
        } else if has_borrowed != Some(false)
            && persistent
                .get::<_, u128>(&DataKey::MarginBorrowPrincipal(position_id))
                .unwrap_or(0)
                > 0
        {
            persistent.set(&DataKey::MarginHasBorrowed(position_id), &true);
        } else if has_borrowed.is_none() {
            panic!("margin borrow state missing");
        }
        bump_margin_borrow_live_ttl(env, position_id);
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

    fn consume_margin_withdraw_bypass(
        env: &Env,
        user: &Address,
        recipient: &Address,
        ptoken_amount: u128,
    ) -> bool {
        let key = DataKey::MarginWithdrawBypassV2(user.clone());
        let Some(scope) = env
            .storage()
            .persistent()
            .get::<_, MarginWithdrawBypassScope>(&key)
        else {
            return false;
        };
        // Validate scope BEFORE removing the entry. Removing first would let any
        // mismatched caller (e.g. an approved spender doing transfer_from on the
        // owner's pTokens) burn a still-valid bypass and grief the owner's margin
        // flow. Only consume the one-shot bypass when it actually matches.
        if scope.ledger_sequence != env.ledger().sequence()
            || scope.recipient != *recipient
            || ptoken_amount > scope.max_ptokens
        {
            return false;
        }
        env.storage().persistent().remove(&key);
        true
    }

    fn consume_margin_transfer_bypass(
        env: &Env,
        user: &Address,
        to: &Address,
        ptoken_amount: u128,
    ) -> bool {
        Self::consume_margin_withdraw_bypass(env, user, to, ptoken_amount)
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
        assert_expected_admin(&env, &admin);
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
        let disabled = Address::from_string(&String::from_str(&env, DISABLED_BOOSTED_VAULT));
        let new_boosted = if boosted_vault == disabled {
            None
        } else {
            Some(boosted_vault.clone())
        };

        // Discover and persist the ABI-required min_amounts_out length while
        // the strategy is deliberately being bound. A later NAV/quote outage
        // must not collapse a multi-asset strategy to a guessed one-element
        // vector and brick protected redemptions.
        let asset_count = new_boosted.as_ref().map(|vault| {
            let amounts: Vec<i128> =
                call_contract_or_panic(&env, vault, "get_asset_amounts_per_shares", (0i128,));
            Self::validate_boosted_asset_count(amounts.len())
        });
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
                    new_boosted.clone(),
                ),
            );
        }
        if let (Some(vault), Some(count)) = (new_boosted.clone(), asset_count) {
            env.storage()
                .persistent()
                .set(&DataKey::BoostedVault, &vault);
            env.storage()
                .persistent()
                .set(&DataKey::BoostedAssetCount, &count);
        } else {
            env.storage().persistent().remove(&DataKey::BoostedVault);
            env.storage()
                .persistent()
                .remove(&DataKey::BoostedAssetCount);
        }
        env.storage()
            .persistent()
            .remove(&DataKey::BoostedUnderlyingCached);
        env.storage()
            .persistent()
            .remove(&DataKey::BoostedUnderlyingUpdatedAt);
        Self::invalidate_boosted_health_cache(&env);
        BoostedVaultSet {
            old_vault: old_boosted,
            new_vault: new_boosted,
        }
        .publish(&env);
    }

    /// View: get boosted vault (if set)
    pub fn get_boosted_vault(env: Env) -> Option<Address> {
        let _ = ensure_initialized(&env);
        env.storage().persistent().get(&DataKey::BoostedVault)
    }

    /// View the persisted boosted-vault output-vector length.
    pub fn get_boosted_asset_count(env: Env) -> Option<u32> {
        let _ = ensure_initialized(&env);
        Self::stored_boosted_asset_count(&env)
    }

    /// Admin migration/recovery for markets bound before BoostedAssetCount
    /// existed. The expected address prevents a stale operator command from
    /// configuring a replacement strategy accidentally.
    pub fn set_boosted_asset_count(
        env: Env,
        admin: Address,
        boosted_vault: Address,
        asset_count: u32,
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
        let stored_vault: Address = env
            .storage()
            .persistent()
            .get(&DataKey::BoostedVault)
            .expect("boosted vault not set");
        if stored_vault != boosted_vault {
            panic!("boosted vault mismatch");
        }
        let asset_count = Self::validate_boosted_asset_count(asset_count);
        env.storage()
            .persistent()
            .set(&DataKey::BoostedAssetCount, &asset_count);
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

    /// Pull the specified amount of underlying from the boosted vault into idle cash.
    /// Call this before a large withdraw to pre-fund idle cash so the withdrawal
    /// transaction itself does not need to call the boosted vault (avoiding budget limits).
    /// Restricted to admin to prevent griefing: a permissionless version lets any caller
    /// force a full DeFindex redemption, disrupting yield for all depositors.
    pub fn prepare_liquidity(env: Env, amount: u128) {
        let token_address = ensure_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        Self::ensure_liquid_cash(&env, &token_address, amount);
    }

    /// Permissionless: refresh the cached boosted-underlying value from live DeFindex data.
    /// Call this at least every five minutes and after each rebalance. Account-health
    /// checks fail closed when the cache is stale so they never need to load DeFindex's
    /// full footprint inside a borrow transaction.
    pub fn refresh_boosted_underlying(env: Env) {
        let _ = Self::get_boosted_underlying(&env);
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
        Self::deposit_excess_idle_cash(&env, &token_address, live_cash);
    }

    fn deposit_excess_idle_cash(env: &Env, token_address: &Address, live_cash: u128) {
        let bps = Self::idle_cash_buffer_bps(env) as u128;
        let total_underlying = Self::get_total_underlying(env.clone());
        let desired_idle = total_underlying.saturating_mul(bps) / BPS_SCALE;
        if live_cash > desired_idle {
            let excess = live_cash - desired_idle;
            let _ = Self::deposit_into_boosted(env, token_address, excess);
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
        if !Self::consume_margin_withdraw_bypass(&env, &user, &user, ptoken_amount) {
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
        let _ = ensure_initialized(&env);
        let pbal = Self::read_ptoken_balance_with_borrow_ttl(&env, &user);
        if pbal == 0 {
            return 0u128;
        }
        let rate = Self::get_exchange_rate(env.clone());
        (pbal.saturating_mul(rate)) / SCALE_1E6
    }

    /// Get user's pToken balance
    pub fn get_ptoken_balance(env: Env, user: Address) -> u128 {
        let _ = ensure_initialized(&env);
        Self::read_ptoken_balance_with_borrow_ttl(&env, &user)
    }

    /// Return (ptoken_balance, borrow_balance, exchange_rate, underlying_token) in one call.
    /// Peridottroller uses this in account-health loops to replace four cross-contract reads
    /// with one. Boosted markets use a fresh keeper-maintained NAV cache here; stale or missing
    /// cache state fails closed instead of synchronously loading a strategy footprint.
    pub fn get_account_snapshot(env: Env, user: Address) -> (u128, u128, u128, Address) {
        // Do not call the broad `ensure_initialized` here. This endpoint runs once
        // per entered market inside controller health checks, and loading every
        // unrelated vault config key makes otherwise-valid three-market borrows
        // exceed Soroban's ledger-footprint limit.
        let token = ensure_initialized_for_snapshot(&env);
        let pbal = ptoken_balance(&env, &user);
        let debt = Self::get_user_borrow_balance_internal(&env, &user, Some(pbal));
        let rate = Self::get_exchange_rate_for_account_health(&env);
        (pbal, debt, rate, token)
    }

    // ERC20-like pToken API
    pub fn approve(
        env: Env,
        owner: Address,
        spender: Address,
        amount: i128,
        live_until_ledger: u32,
    ) {
        if amount < 0 {
            panic!("bad amount");
        }
        // owner.require_auth() is enforced by TokenBase::approve internally.
        // Soroban treats a duplicate require_auth() in the same frame as an error
        // (ExistingValue), so we rely on the library call rather than duplicating it.
        // The guarantee is verified by test_ptoken_approve_rejects_without_owner_auth.
        TokenBase::approve(&env, &owner, &spender, amount, live_until_ledger);
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        TokenBase::allowance(&env, &owner, &spender)
    }

    pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        if amount < 0 {
            panic!("bad amount");
        }
        Self::transfer_internal(env, from, to, amount as u128, None);
    }

    pub fn transfer_from(env: Env, spender: Address, owner: Address, to: Address, amount: i128) {
        if amount < 0 {
            panic!("bad amount");
        }
        Self::transfer_internal(
            env,
            owner,
            MuxedAddress::from(to),
            amount as u128,
            Some(spender),
        );
    }

    pub fn balance(env: Env, account: Address) -> i128 {
        let _ = ensure_initialized(&env);
        let balance = TokenBase::balance(&env, &account);
        if balance > 0 {
            Self::sync_user_borrow_state_for_ptoken_read(&env, &account, balance as u128);
        }
        balance
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
        to: MuxedAddress,
        amount: u128,
        spender: Option<Address>,
    ) {
        let to_address = to.address();
        let token_address = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        if amount == 0 {
            return;
        }
        // Ensure collateral checks use the latest debt/index state.
        Self::update_interest(env.clone());
        Self::ensure_user_borrow_flag(&env, &from);
        // Margin custody flow: only the configured margin controller may receive
        // pTokens via the one-shot bypass. Collateral health checks still run;
        // the bypass only skips margin-lock accounting for controller custody moves.
        let bypass = Self::consume_margin_transfer_bypass(&env, &from, &to_address, amount);
        let to_had_borrow_state = Self::has_user_borrow_state(&env, &to_address);
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
            let local_debt = Self::get_user_borrow_balance(env.clone(), from.clone());
            let other_borrows_usd: u128 = call_contract_or_panic(
                &env,
                &comp_addr,
                "get_borrows_excl",
                (from.clone(), env.current_contract_address()),
            );
            if local_debt > 0 || other_borrows_usd > 0 {
                let other_collateral_usd: u128 = call_contract_or_panic(
                    &env,
                    &comp_addr,
                    "get_collateral_excl_usd",
                    (from.clone(), env.current_contract_address()),
                );
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

                let current_rate = Self::get_exchange_rate(env.clone());
                let remaining_ptokens = pbal - amount;
                let remaining_underlying =
                    (remaining_ptokens.saturating_mul(current_rate)) / SCALE_1E6;
                let remaining_discounted = (remaining_underlying.saturating_mul(cf)) / SCALE_1E6;
                let local_collateral_usd = (remaining_discounted.saturating_mul(price)) / scale;
                let local_debt_usd = (local_debt.saturating_mul(price)) / scale;
                let total_collateral_usd =
                    other_collateral_usd.saturating_add(local_collateral_usd);
                let total_borrow_usd = other_borrows_usd.saturating_add(local_debt_usd);
                if total_collateral_usd < total_borrow_usd {
                    panic!("Insufficient collateral");
                }
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
        if !bypass {
            Self::enforce_margin_lock(&env, &from, from_bal, amount);
        }

        match spender {
            Some(spender_addr) => {
                TokenBase::transfer_from(&env, &spender_addr, &from, &to_address, to_i128(amount));
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
        let to_ptokens_now = ptoken_balance(&env, &to_address);
        if !to_had_borrow_state {
            if to_ptokens_now == amount || total_borrowed_now == 0 || bypass {
                Self::mark_user_not_borrowed_if_state_missing(&env, &to_address);
            } else {
                panic!("recipient borrow state missing");
            }
        }
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
            user_ptokens: Some(to_ptokens_now),
            user_borrowed: Some(Self::get_user_borrow_balance(
                env.clone(),
                to_address.clone(),
            )),
        };
        Self::accrue_user_rewards(&env, &to_address, to_hint, "transfer");
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
        Self::get_exchange_rate_internal(&env)
    }

    fn get_exchange_rate_internal(env: &Env) -> u128 {
        bump_account_snapshot_valuation_ttl(env);
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

    /// Permissionless keepalive for global vault configuration and interest state.
    /// Account-health snapshots deliberately use a narrow TTL path to stay within
    /// Soroban's transaction footprint, so operators should call this periodically.
    pub fn bump_ttl(env: Env) {
        let _ = ensure_initialized(&env);
        // BoostedAssetCount stays off the ordinary ensure_initialized hot path:
        // one extra absent-key read is enough to push boundary lending calls
        // past Soroban's 100-entry footprint limit. The explicit keeper path
        // can afford to renew this boosted-only key deliberately.
        bump_boosted_vault_ttl(&env);
        bump_boosted_health_cache_ttl(&env);
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

    pub fn begin_margin_withdraw(
        env: Env,
        margin_controller: Address,
        user: Address,
        recipient: Address,
        max_ptokens: u128,
    ) {
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
        if max_ptokens == 0 {
            panic!("bad amount");
        }
        env.storage().persistent().set(
            &DataKey::MarginWithdrawBypassV2(user),
            &MarginWithdrawBypassScope {
                recipient,
                max_ptokens,
                ledger_sequence: env.ledger().sequence(),
            },
        );
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
        let admin_fee_scaled: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::AdminFeeScaled)
            .unwrap_or(0u128);
        Self::ensure_fee_factors_within_cap(reserve_factor_scaled, admin_fee_scaled);
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
        let reserve_factor_scaled: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::ReserveFactorScaled)
            .unwrap_or(0u128);
        Self::ensure_fee_factors_within_cap(reserve_factor_scaled, admin_fee_scaled);
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

    /// View: preview flash-loan fee for `amount` using the same rounding as `flash_loan`.
    pub fn preview_flash_loan_fee(env: Env, amount: u128) -> u128 {
        let _ = ensure_initialized(&env);
        let fee_scaled: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::FlashLoanFeeScaled)
            .unwrap_or(0u128);
        Self::compute_flash_loan_fee(amount, fee_scaled)
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

    pub fn get_total_bad_debt(env: Env) -> u128 {
        let _ = ensure_initialized(&env);
        env.storage()
            .persistent()
            .get(&DataKey::TotalBadDebt)
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
        // Simulation can happen in the same ledger as the previous accrual and
        // execution in the next one. Always run the full read/call path so the
        // prepared footprint covers the branch that execution may take.
        let elapsed = now.saturating_sub(last_time) as u128;
        // Borrow interest accrual via global index (split to reserves, admin fees, and suppliers)
        let tb_prior: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .expect("total borrowed missing");

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
            let token_address: Address = env
                .storage()
                .persistent()
                .get(&DataKey::UnderlyingToken)
                .expect("underlying not set");
            // Dynamic rate queries use gross live cash. Cap boosted cash only
            // when the external/accounting report is implausibly above its
            // internal baseline.
            let cached_before = Self::cached_boosted_underlying(&env);
            let updated_at: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::BoostedUnderlyingUpdatedAt)
                .unwrap_or(0);
            let cache_age = env.ledger().timestamp().saturating_sub(updated_at);
            let boosted_reported = if cache_age > BOOSTED_CACHE_MAX_AGE_SECS {
                Self::estimate_boosted_underlying_from_accounting(&env)
            } else {
                cached_before
            };
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
        if tb_prior > 0 {
            let borrow_interest_total = if elapsed == 0 || borrow_yearly_rate_scaled == 0 {
                0u128
            } else {
                checked_interest_product(&env, tb_prior, borrow_yearly_rate_scaled, elapsed)
            };
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
            Self::ensure_fee_factors_within_cap(rf, af);
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
        let next_last_update = if advance_last_update { now } else { last_time };
        env.storage()
            .persistent()
            .set(&DataKey::LastUpdateTime, &next_last_update);
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
        Self::get_user_borrow_balance_internal(&env, &user, None)
    }

    fn get_user_borrow_balance_internal(
        env: &Env,
        user: &Address,
        ptoken_balance_hint: Option<u128>,
    ) -> u128 {
        let persistent = env.storage().persistent();
        let has_borrowed: Option<bool> = persistent.get(&DataKey::HasBorrowed(user.clone()));
        bump_user_borrow_live_ttl(env, user);
        let snap: Option<BorrowSnapshot> = persistent.get(&DataKey::BorrowSnapshots(user.clone()));
        let snapshot = if let Some(snapshot) = snap {
            snapshot
        } else {
            if has_borrowed.unwrap_or(false) {
                panic!("borrow snapshot missing");
            }
            if has_borrowed.is_none() {
                let principal_key = DataKey::BorrowPrincipal(user.clone());
                let principal_opt: Option<u128> = persistent.get(&principal_key);
                if let Some(principal) = principal_opt {
                    bump_borrow_principal_ttl(env, user);
                    if principal > 0 {
                        panic!("borrow state missing");
                    }
                } else {
                    let pbal = ptoken_balance_hint.unwrap_or_else(|| ptoken_balance(env, user));
                    if pbal > 0 && Self::total_borrowed_for_state_repair(env) > 0 {
                        panic!("borrow state missing");
                    }
                }
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
        bump_borrow_index_ttl(env);
        // principal * current_index / user_index
        Self::checked_mul_div_u128(snapshot.principal, current_index, snapshot.interest_index)
    }

    /// Get current borrow balance for a margin position namespace.
    pub fn get_margin_borrow_balance(env: Env, position_id: u64) -> u128 {
        let _ = ensure_initialized(&env);
        let persistent = env.storage().persistent();
        let has_borrowed: Option<bool> = persistent.get(&DataKey::MarginHasBorrowed(position_id));
        bump_margin_borrow_live_ttl(&env, position_id);
        let snap: Option<BorrowSnapshot> =
            persistent.get(&DataKey::MarginBorrowSnapshots(position_id));
        let snapshot = if let Some(snapshot) = snap {
            snapshot
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
        Self::checked_mul_div_u128(snapshot.principal, current_index, snapshot.interest_index)
    }

    /// Permissionless TTL extension for position-scoped margin borrow state.
    pub fn bump_margin_borrow_ttl(env: Env, position_id: u64) {
        let _ = ensure_initialized(&env);
        bump_margin_borrow_state_ttl(&env, position_id);
    }

    /// Permissionless snapshot recovery is intentionally disabled: rebuilding
    /// without the historical borrow index can erase accrued interest.
    pub fn recover_borrow_snapshot(env: Env, user: Address) {
        let _ = ensure_initialized(&env);
        let _ = user;
        panic!("admin recovery required");
    }

    /// Permissionless snapshot recovery is intentionally disabled: rebuilding
    /// without the historical borrow index can erase accrued interest.
    pub fn recover_margin_snapshot(env: Env, position_id: u64) {
        let _ = ensure_initialized(&env);
        let _ = position_id;
        panic!("admin recovery required");
    }

    /// Permissionless migration/keepalive path for user borrow state.
    /// Existing mirrors are canonical true principal; snapshots may include
    /// capitalized interest, so this path must not create a missing mirror.
    pub fn migrate_borrow_state_batch(env: Env, users: Vec<Address>) {
        let _ = ensure_initialized(&env);
        let persistent = env.storage().persistent();
        for i in 0..users.len() {
            let user = users.get(i).unwrap();
            if let Some(snapshot) =
                persistent.get::<_, BorrowSnapshot>(&DataKey::BorrowSnapshots(user.clone()))
            {
                let principal_key = DataKey::BorrowPrincipal(user.clone());
                // Existing mirrors are canonical true principal; a debt snapshot may include
                // capitalized interest, so permissionless migration must never increase it.
                if let Some(existing) = persistent.get::<_, u128>(&principal_key) {
                    let principal = existing.min(snapshot.principal);
                    persistent.set(&principal_key, &principal);
                }
            }
            bump_user_borrow_state_ttl(&env, &user);
        }
        persistent.set(&DataKey::DebtStateVersion, &DEBT_STATE_VERSION_V1);
        persistent.set(&DataKey::DebtStateMigratedAt, &env.ledger().timestamp());
        bump_debt_state_marker_ttl(&env);
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
                let principal_key = DataKey::MarginBorrowPrincipal(position_id);
                // Existing mirrors are canonical true principal; a debt snapshot may include
                // capitalized interest, so permissionless migration must never increase it.
                if let Some(existing) = persistent.get::<_, u128>(&principal_key) {
                    let principal = existing.min(snapshot.principal);
                    persistent.set(&principal_key, &principal);
                }
            }
            bump_margin_borrow_state_ttl(&env, position_id);
        }
        persistent.set(&DataKey::DebtStateVersion, &DEBT_STATE_VERSION_V1);
        persistent.set(&DataKey::DebtStateMigratedAt, &env.ledger().timestamp());
        bump_debt_state_marker_ttl(&env);
        bump_core_ttl(&env);
    }

    /// Admin-only migration for a legacy borrower whose debt snapshot exists
    /// but whose true-principal mirror was never written. The principal must be
    /// reconstructed from trusted historical accounting; snapshot principal is
    /// not a safe default because it may include capitalized interest.
    pub fn recover_user_borrow_principal(env: Env, admin: Address, user: Address, principal: u128) {
        let _ = ensure_initialized(&env);
        let persistent = env.storage().persistent();
        let stored_admin: Address = persistent.get(&DataKey::Admin).expect("admin not set");
        if stored_admin != admin {
            panic!("not admin");
        }
        admin.require_auth();

        let principal_key = DataKey::BorrowPrincipal(user.clone());
        if persistent.has(&principal_key) {
            panic!("borrow principal already set");
        }
        let snapshot: BorrowSnapshot = persistent
            .get(&DataKey::BorrowSnapshots(user.clone()))
            .expect("borrow snapshot missing");
        if principal == 0 || snapshot.interest_index == 0 || principal > snapshot.principal {
            panic!("invalid borrow principal");
        }
        persistent.set(&principal_key, &principal);
        persistent.set(&DataKey::HasBorrowed(user.clone()), &true);
        bump_user_borrow_state_ttl(&env, &user);
    }

    /// Admin-only mirror recovery for legacy margin-position debt.
    pub fn recover_margin_borrow_principal(
        env: Env,
        admin: Address,
        position_id: u64,
        principal: u128,
    ) {
        let _ = ensure_initialized(&env);
        let persistent = env.storage().persistent();
        let stored_admin: Address = persistent.get(&DataKey::Admin).expect("admin not set");
        if stored_admin != admin {
            panic!("not admin");
        }
        admin.require_auth();

        let principal_key = DataKey::MarginBorrowPrincipal(position_id);
        if persistent.has(&principal_key) {
            panic!("margin borrow principal already set");
        }
        let snapshot: BorrowSnapshot = persistent
            .get(&DataKey::MarginBorrowSnapshots(position_id))
            .expect("margin borrow snapshot missing");
        if principal == 0 || snapshot.interest_index == 0 || principal > snapshot.principal {
            panic!("invalid margin borrow principal");
        }
        persistent.set(&principal_key, &principal);
        persistent.set(&DataKey::MarginHasBorrowed(position_id), &true);
        bump_margin_borrow_state_ttl(&env, position_id);
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
        if principal > 0 {
            env.storage()
                .persistent()
                .set(&DataKey::HasBorrowed(user.clone()), &true);
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::HasBorrowed(user.clone()), &false);
        }
        env.storage()
            .persistent()
            .set(&DataKey::BorrowPrincipal(user.clone()), &principal);
        bump_user_borrow_state_ttl(&env, &user);
    }

    /// Internal: write user's borrow snapshot and true principal mirror.
    fn write_borrow_snapshot_with_principal(
        env: &Env,
        user: Address,
        debt_principal: u128,
        borrow_principal: u128,
    ) {
        let current_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        let snap = BorrowSnapshot {
            principal: debt_principal,
            interest_index: current_index,
        };
        env.storage()
            .persistent()
            .set(&DataKey::BorrowSnapshots(user.clone()), &snap);
        if debt_principal > 0 {
            env.storage()
                .persistent()
                .set(&DataKey::HasBorrowed(user.clone()), &true);
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::HasBorrowed(user.clone()), &false);
        }
        env.storage()
            .persistent()
            .set(&DataKey::BorrowPrincipal(user.clone()), &borrow_principal);
        bump_user_borrow_state_ttl(env, &user);
    }

    fn write_margin_borrow_snapshot_with_principal(
        env: &Env,
        position_id: u64,
        debt_principal: u128,
        borrow_principal: u128,
    ) {
        let current_index: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex)
            .expect("borrow index missing");
        let snap = BorrowSnapshot {
            principal: debt_principal,
            interest_index: current_index,
        };
        env.storage()
            .persistent()
            .set(&DataKey::MarginBorrowSnapshots(position_id), &snap);
        env.storage().persistent().set(
            &DataKey::MarginHasBorrowed(position_id),
            &(debt_principal > 0),
        );
        env.storage().persistent().set(
            &DataKey::MarginBorrowPrincipal(position_id),
            &borrow_principal,
        );
        bump_margin_borrow_state_ttl(env, position_id);
    }

    /// Repayment amount applied to principal (interest-only repayment does not reduce principal).
    fn principal_component_of_repay(
        env: &Env,
        user: &Address,
        current_debt: u128,
        repay_amount: u128,
    ) -> u128 {
        let principal = Self::user_borrow_principal(env, user);
        let accrued_interest = current_debt.saturating_sub(principal);
        repay_amount.saturating_sub(accrued_interest).min(principal)
    }

    fn principal_component_of_margin_repay(
        env: &Env,
        position_id: u64,
        current_debt: u128,
        repay_amount: u128,
    ) -> u128 {
        let principal = Self::margin_borrow_principal(env, position_id);
        let accrued_interest = current_debt.saturating_sub(principal);
        repay_amount.saturating_sub(accrued_interest).min(principal)
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
            exchange_rate = Self::get_exchange_rate_for_account_health(&env);
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
        let available = Self::get_available_liquidity_for_borrow(&env);
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

        // Update totals, debt snapshot, and true-principal mirror.
        let current_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
        let current_borrow_principal = Self::user_borrow_principal(&env, &user);
        let new_debt_principal = current_debt.saturating_add(amount);
        let new_borrow_principal = current_borrow_principal.saturating_add(amount);
        Self::write_borrow_snapshot_with_principal(
            &env,
            user.clone(),
            new_debt_principal,
            new_borrow_principal,
        );

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
            account_borrows: new_debt_principal,
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
        // receiver.require_auth() is the real authorization gate: the user must
        // have signed an auth entry for this exact (position_id, receiver, amount)
        // call. The previous owner cross-check callback was redundant defensive coding
        // and triggered Soroban's re-entry guard when called from within an
        // open_position flow on the controller.
        if receiver != margin_controller {
            receiver.require_auth();
        }
        Self::ensure_margin_position_borrow_flag(&env, position_id);
        if amount == 0 {
            panic!("bad amount");
        }
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

        let available = Self::get_available_liquidity_for_borrow(&env);
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
        let current_borrow_principal = Self::margin_borrow_principal(&env, position_id);
        let new_debt_principal = current.saturating_add(amount);
        let new_borrow_principal = current_borrow_principal.saturating_add(amount);
        Self::write_margin_borrow_snapshot_with_principal(
            &env,
            position_id,
            new_debt_principal,
            new_borrow_principal,
        );

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

    /// Borrow into a margin position namespace and send funds to the configured
    /// margin controller. This is used by controller-custodied margin positions.
    pub fn borrow_for_margin_to_controller(env: Env, position_id: u64, amount: u128) {
        let receiver: Address = env
            .storage()
            .persistent()
            .get(&DataKey::MarginController)
            .expect("margin controller not set");
        Self::borrow_for_margin(env, position_id, receiver, amount);
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
        let received = cash_after.saturating_sub(cash_before);
        Self::require_exact_repay_received(received, repay_amount);
        Self::add_managed_cash(&env, received);

        // Update snapshot and totals
        let new_principal = current_debt - repay_amount;
        let borrow_principal_after =
            Self::user_borrow_principal(&env, &user).saturating_sub(principal_repay_user);
        Self::write_borrow_snapshot_with_principal(
            &env,
            user.clone(),
            new_principal,
            borrow_principal_after,
        );

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

    /// Repay the user's full post-accrual debt in a single call.
    /// Avoids the dust left by `repay(amount)` when interest accrues between
    /// simulation and execution: the repay cap is fixed pre-accrual, so a
    /// user-supplied amount equal to the displayed debt always leaves a few
    /// units behind. `repay_max` recomputes the debt after accrual and pays
    /// exactly that, leaving the borrow at zero.
    pub fn repay_max(env: Env, user: Address) {
        let _ = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        Self::ensure_user_borrow_flag(&env, &user);
        Self::update_interest(env.clone());
        let current_debt = Self::get_user_borrow_balance(env.clone(), user.clone());
        if current_debt == 0 {
            return;
        }
        Self::repay(env, user, current_debt);
    }

    /// Repay debt tracked in a margin position namespace.
    /// Callable only by the configured margin controller.
    pub fn repay_for_margin(env: Env, position_id: u64, payer: Address, amount: u128) {
        let token_address = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        let _margin_controller = Self::require_margin_controller_auth(&env);
        // payer.require_auth() (later in this fn) is the real authorization gate.
        // The previous owner cross-check (callback to controller) triggered
        // Soroban's re-entry guard when called from a controller settlement.
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
        let received = cash_after.saturating_sub(cash_before);
        Self::require_exact_repay_received(received, repay_amount);
        Self::add_managed_cash(&env, received);

        let new_principal = current_debt - repay_amount;
        let borrow_principal_after = Self::margin_borrow_principal(&env, position_id)
            .saturating_sub(principal_repay_position);
        Self::write_margin_borrow_snapshot_with_principal(
            &env,
            position_id,
            new_principal,
            borrow_principal_after,
        );

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

    /// Repay a margin position's full post-accrual debt with a fixed user-authorized
    /// maximum payment. Any overpay is refunded, avoiding exact-auth drift when
    /// interest changes between simulation and execution.
    pub fn repay_full_for_margin(
        env: Env,
        position_id: u64,
        payer: Address,
        max_amount: u128,
    ) -> u128 {
        let token_address = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        let _margin_controller = Self::require_margin_controller_auth(&env);
        Self::ensure_margin_position_borrow_flag(&env, position_id);
        Self::update_interest(env.clone());
        let current_debt = Self::get_margin_borrow_balance(env.clone(), position_id);
        if current_debt == 0 {
            return 0u128;
        }
        if max_amount < current_debt {
            panic!("max repay too small");
        }

        payer.require_auth();
        let token_client = token::Client::new(&env, &token_address);
        let max_i128 = to_i128(max_amount);
        let cash_before = Self::current_live_cash(&env, &token_address);
        token_client.transfer(&payer, &env.current_contract_address(), &max_i128);
        let cash_after = Self::current_live_cash(&env, &token_address);
        let received = cash_after.saturating_sub(cash_before);
        if received < current_debt {
            panic!("repay transfer shortfall");
        }
        Self::add_managed_cash(&env, received);

        let refund = received.saturating_sub(current_debt);
        if refund > 0 {
            let refund_i128 = to_i128(refund);
            let refund_cash_before = Self::current_live_cash(&env, &token_address);
            token_client.transfer(&env.current_contract_address(), &payer, &refund_i128);
            let refund_cash_after = Self::current_live_cash(&env, &token_address);
            Self::sub_managed_cash(&env, refund_cash_before.saturating_sub(refund_cash_after));
        }

        let principal_repay_position = Self::principal_component_of_margin_repay(
            &env,
            position_id,
            current_debt,
            current_debt,
        );
        Self::write_margin_borrow_snapshot_with_principal(&env, position_id, 0u128, 0u128);

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
            env.storage().persistent().set(
                &DataKey::TotalBorrowPrincipal,
                &(total_principal_before - principal_repay_global),
            );
        }

        let tb: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalBorrowed)
            .expect("total borrowed missing");
        let tb_after = tb
            .checked_sub(current_debt)
            .expect("repay exceeds total borrowed");
        env.storage()
            .persistent()
            .set(&DataKey::TotalBorrowed, &tb_after);
        current_debt
    }

    /// Absorb remaining margin debt after collateral has been exhausted. Reserves
    /// absorb losses first; any remainder is explicitly recorded as bad debt and
    /// socialized through the exchange rate by reducing TotalBorrowed.
    pub fn absorb_margin_bad_debt(env: Env, position_id: u64) -> u128 {
        let _ = ensure_initialized(&env);
        Self::ensure_not_in_flash_loan(&env);
        let _margin_controller = Self::require_margin_controller_auth(&env);
        Self::ensure_margin_position_borrow_flag(&env, position_id);
        Self::update_interest(env.clone());
        let current_debt = Self::get_margin_borrow_balance(env.clone(), position_id);
        if current_debt == 0 {
            return 0u128;
        }

        let principal_repay_position = Self::principal_component_of_margin_repay(
            &env,
            position_id,
            current_debt,
            current_debt,
        );
        Self::write_margin_borrow_snapshot_with_principal(&env, position_id, 0u128, 0u128);

        let storage = env.storage().persistent();
        let total_borrowed: u128 = storage
            .get(&DataKey::TotalBorrowed)
            .expect("total borrowed missing");
        let total_borrowed_after = total_borrowed
            .checked_sub(current_debt)
            .expect("bad debt exceeds total borrowed");
        storage.set(&DataKey::TotalBorrowed, &total_borrowed_after);

        let bcap: u128 = storage.get(&DataKey::BorrowCap).unwrap_or(0u128);
        if bcap > 0 {
            let total_principal_before: u128 = storage
                .get(&DataKey::TotalBorrowPrincipal)
                .unwrap_or(total_borrowed);
            let principal_repay_global = principal_repay_position.min(total_principal_before);
            storage.set(
                &DataKey::TotalBorrowPrincipal,
                &(total_principal_before - principal_repay_global),
            );
        }

        let reserves: u128 = storage.get(&DataKey::TotalReserves).unwrap_or(0u128);
        let reserves_used = reserves.min(current_debt);
        let reserves_after = reserves - reserves_used;
        storage.set(&DataKey::TotalReserves, &reserves_after);

        let bad_debt = current_debt - reserves_used;
        if bad_debt > 0 {
            let total_bad_debt: u128 = storage.get(&DataKey::TotalBadDebt).unwrap_or(0u128);
            storage.set(
                &DataKey::TotalBadDebt,
                &total_bad_debt.saturating_add(bad_debt),
            );
        }

        MarginBadDebtAbsorbed {
            position_id,
            debt_amount: current_debt,
            reserves_used,
            bad_debt,
            total_borrows: total_borrowed_after,
            total_reserves: reserves_after,
        }
        .publish(&env);
        current_debt
    }

    /// Execute a flash loan to `receiver`. Receiver must return `amount + fee` within the callback.
    pub fn flash_loan(env: Env, initiator: Address, receiver: Address, amount: u128, data: Bytes) {
        if amount == 0 {
            panic!("invalid flash amount");
        }
        initiator.require_auth();
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
        let fee = Self::compute_flash_loan_fee(amount, fee_scaled);

        let token_client = token::Client::new(&env, &token_address);

        let balance_before_i: i128 = token_client.balance(&env.current_contract_address());
        if balance_before_i < 0 {
            panic!("invalid cash state");
        }
        let balance_before = balance_before_i as u128;

        env.storage()
            .persistent()
            .set(&DataKey::FlashLoanActive, &true);
        Self::sub_managed_cash(&env, amount);
        token_client.transfer(&env.current_contract_address(), &receiver, &to_i128(amount));

        // Intentionally no `receiver.require_auth()`: contract receivers cannot satisfy
        // account-style auth here, and self-initiated callbacks hit Soroban's re-entry guard.
        // Consent is by implementing this callback; repayment is enforced by the balance check.
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
        let live_cash = Self::current_live_cash(&env, &token_address);
        if live_cash > 0 {
            Self::deposit_excess_idle_cash(&env, &token_address, live_cash);
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
        let received = cash_after.saturating_sub(cash_before);
        Self::require_exact_repay_received(received, repay_amount);
        Self::add_managed_cash(&env, received);

        // Update borrower snapshot and totals
        let new_principal = current_debt - repay_amount;
        let borrow_principal_after =
            Self::user_borrow_principal(&env, &borrower).saturating_sub(principal_repay_user);
        Self::write_borrow_snapshot_with_principal(
            &env,
            borrower.clone(),
            new_principal,
            borrow_principal_after,
        );

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
                stellar_tokens::fungible::emit_transfer(
                    &env, &borrower, &recipient, None, fee_i128,
                );
                Self::mark_user_not_borrowed_if_state_missing(&env, &recipient);
                remaining = remaining.saturating_sub(seize_ctx.fee_ptokens);
            }
        }
        if remaining > 0 {
            TokenBase::update(&env, Some(&borrower), Some(&liquidator), to_i128(remaining));
            stellar_tokens::fungible::emit_transfer(
                &env,
                &borrower,
                &liquidator,
                None,
                to_i128(remaining),
            );
            Self::mark_user_not_borrowed_if_state_missing(&env, &liquidator);
        }
    }
}

fn assert_expected_admin(env: &Env, admin: &Address) {
    if let Some(expected_admin_str) = expected_admin_config() {
        assert_admin_matches_config(env, admin, expected_admin_str);
    }
}

fn assert_admin_matches_config(env: &Env, admin: &Address, expected_admin_str: &str) {
    let expected_admin = Address::from_string(&String::from_str(env, expected_admin_str));
    if admin != &expected_admin {
        panic!("unexpected admin");
    }
}

#[cfg(any(
    test,
    all(
        feature = "test-default-admin",
        debug_assertions,
        not(target_arch = "wasm32")
    )
))]
fn expected_admin_config() -> Option<&'static str> {
    option_env!("RECEIPT_VAULT_INIT_ADMIN")
}

#[cfg(not(any(
    test,
    all(
        feature = "test-default-admin",
        debug_assertions,
        not(target_arch = "wasm32")
    )
)))]
fn expected_admin_config() -> Option<&'static str> {
    Some(env!("RECEIPT_VAULT_INIT_ADMIN"))
}

#[cfg(test)]
mod init_admin_guard_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    #[should_panic(expected = "unexpected admin")]
    fn expected_admin_guard_rejects_mismatch() {
        let env = Env::default();
        let attacker = Address::generate(&env);

        assert_admin_matches_config(&env, &attacker, DEFAULT_INIT_ADMIN);
    }

    #[test]
    fn expected_admin_guard_accepts_configured_admin() {
        let env = Env::default();
        let admin = Address::from_string(&String::from_str(&env, DEFAULT_INIT_ADMIN));

        assert_admin_matches_config(&env, &admin, DEFAULT_INIT_ADMIN);
    }
}
