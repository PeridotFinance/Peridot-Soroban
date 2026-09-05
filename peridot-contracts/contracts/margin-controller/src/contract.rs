use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
#[cfg(not(test))]
use soroban_sdk::String;
use soroban_sdk::{
    contract, contractimpl, token, Address, BytesN, Env, IntoVal, InvokeError, Symbol, Val, Vec,
};

use crate::constants::*;
use crate::events::{AdminTransferProposed, AdminTransferred};
use crate::helpers::*;
use crate::storage::*;

#[contract]
pub struct MarginController;

#[contractimpl]
impl MarginController {
    pub fn initialize(
        env: Env,
        admin: Address,
        peridottroller: Address,
        swap_adapter: Address,
        max_leverage: u128,
    ) {
        let persistent = env.storage().persistent();
        let already_initialized = persistent.has(&DataKey::Initialized)
            || persistent.has(&DataKey::Admin)
            || env.storage().instance().has(&DataKey::Initialized);
        if already_initialized {
            panic!("already initialized");
        }
        assert_expected_admin(&env, &admin);
        admin.require_auth();
        if max_leverage < 1 || max_leverage > MAX_LEVERAGE_CAP {
            panic!("invalid leverage");
        }
        Self::assert_valid_swap_adapter(&env, &swap_adapter);
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::Peridottroller, &peridottroller);
        env.storage()
            .persistent()
            .set(&DataKey::SwapAdapter, &swap_adapter);
        env.storage()
            .persistent()
            .set(&DataKey::MaxLeverage, &max_leverage);
        env.storage()
            .persistent()
            .set(&DataKey::MaxSlippageScaled, &DEFAULT_MAX_SLIPPAGE_SCALED);
        env.storage()
            .persistent()
            .set(&DataKey::PositionCounter, &0u64);
        env.storage().persistent().set(&DataKey::Initialized, &true);
        bump_core_ttl(&env);
    }

    pub fn get_admin(env: Env) -> Address {
        bump_core_ttl(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    }

    pub fn set_admin(env: Env, admin: Address, new_admin: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if admin == new_admin {
            panic!("admin unchanged");
        }
        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &new_admin);
        bump_pending_admin_ttl(&env);
        AdminTransferProposed {
            current_admin: admin,
            pending_admin: new_admin,
        }
        .publish(&env);
    }

    pub fn accept_admin(env: Env) {
        bump_core_ttl(&env);
        bump_pending_admin_ttl(&env);
        let new_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .expect("pending admin not set");
        new_admin.require_auth();
        let previous_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        env.storage().persistent().set(&DataKey::Admin, &new_admin);
        env.storage().persistent().remove(&DataKey::PendingAdmin);
        bump_core_ttl(&env);
        AdminTransferred {
            previous_admin,
            new_admin,
        }
        .publish(&env);
    }

    pub fn set_market(env: Env, admin: Address, asset: Address, vault: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        // Bind the mapping to the vault's actual underlying. Pricing uses the
        // user-supplied asset address while rates/borrows use the mapped vault;
        // a mismatch would value positions against the wrong oracle asset and
        // could enable undercollateralized borrows. Fail fast on misconfiguration.
        let vault_underlying = ReceiptVaultClient::new(&env, &vault).get_underlying_token();
        if vault_underlying != asset {
            panic!("vault underlying mismatch");
        }
        let decimals = token::TokenClient::new(&env, &asset).decimals();
        if decimals != REQUIRED_UNDERLYING_DECIMALS {
            panic!("unsupported token decimals");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Market(asset.clone()), &vault);
        bump_market_ttl(&env, &asset);
    }

    /// Propose a peridottroller replacement. The change is timelocked because
    /// this address controls prices, policy gates, and liquidation parameters.
    pub fn set_peridottroller(env: Env, admin: Address, peridottroller: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::PendingPeridottroller, &peridottroller);
        env.storage().persistent().set(
            &DataKey::PendingPeridottrollerEta,
            &env.ledger()
                .timestamp()
                .saturating_add(UPGRADE_TIMELOCK_SECS),
        );
    }

    pub fn execute_peridottroller_update(env: Env, admin: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        let pending: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingPeridottroller)
            .expect("pending peridottroller not set");
        let eta: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PendingPeridottrollerEta)
            .expect("pending peridottroller eta not set");
        if env.ledger().timestamp() < eta {
            panic!("config timelocked");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Peridottroller, &pending);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingPeridottroller);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingPeridottrollerEta);
    }

    pub fn set_params(env: Env, admin: Address, max_leverage: u128) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if max_leverage < 1 || max_leverage > MAX_LEVERAGE_CAP {
            panic!("invalid leverage");
        }
        env.storage()
            .persistent()
            .set(&DataKey::MaxLeverage, &max_leverage);
    }

    pub fn set_perps_pair_config(
        env: Env,
        admin: Address,
        margin_asset: Address,
        base_asset: Address,
        side: PositionSide,
        max_leverage: u128,
        maintenance_margin_scaled: u128,
        liquidation_incentive_scaled: u128,
    ) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if margin_asset == base_asset {
            panic!("assets must differ");
        }
        if max_leverage < 1 || max_leverage > MAX_LEVERAGE_CAP {
            panic!("invalid leverage");
        }
        if maintenance_margin_scaled == 0
            || maintenance_margin_scaled > MAX_PERPS_MAINTENANCE_MARGIN_SCALED
        {
            panic!("invalid maintenance margin");
        }
        if liquidation_incentive_scaled > MAX_PERPS_LIQUIDATION_INCENTIVE_SCALED {
            panic!("invalid liquidation incentive");
        }
        let _ = get_market(&env, &margin_asset);
        let _ = get_market(&env, &base_asset);
        let config = PerpsPairConfig {
            max_leverage,
            maintenance_margin_scaled,
            liquidation_incentive_scaled,
        };
        crate::helpers::set_perps_pair_config(&env, &margin_asset, &base_asset, &side, &config);
    }

    pub fn get_perps_pair_config(
        env: Env,
        margin_asset: Address,
        base_asset: Address,
        side: PositionSide,
    ) -> Option<PerpsPairConfig> {
        bump_core_ttl(&env);
        crate::helpers::get_perps_pair_config(&env, &margin_asset, &base_asset, &side)
    }

    pub fn set_perps_pair_execution_config(
        env: Env,
        admin: Address,
        margin_asset: Address,
        base_asset: Address,
        side: PositionSide,
        config: PerpsPairExecutionConfig,
    ) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if margin_asset == base_asset {
            panic!("assets must differ");
        }
        if config.max_open_deviation_scaled > MAX_POOL_EXECUTION_DEVIATION_SCALED
            || config.open_slippage_scaled > MAX_POOL_EXECUTION_DEVIATION_SCALED
            || config.close_slippage_scaled > MAX_POOL_EXECUTION_DEVIATION_SCALED
            || config.liquidation_slippage_scaled > MAX_POOL_EXECUTION_DEVIATION_SCALED
        {
            panic!("invalid execution config");
        }
        let _ = get_market(&env, &margin_asset);
        let _ = get_market(&env, &base_asset);
        crate::helpers::set_perps_pair_execution_config(
            &env,
            &margin_asset,
            &base_asset,
            &side,
            &config,
        );
    }

    pub fn get_perps_pair_execution_config(
        env: Env,
        margin_asset: Address,
        base_asset: Address,
        side: PositionSide,
    ) -> PerpsPairExecutionConfig {
        bump_core_ttl(&env);
        crate::helpers::get_perps_pair_execution_config_or_default(
            &env,
            &margin_asset,
            &base_asset,
            &side,
        )
    }

    pub fn set_perps_pair_exit_config(
        env: Env,
        admin: Address,
        margin_asset: Address,
        base_asset: Address,
        side: PositionSide,
        config: PerpsPairExitExecutionConfig,
    ) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if margin_asset == base_asset {
            panic!("assets must differ");
        }
        if config.max_close_deviation_scaled > MAX_POOL_EXECUTION_DEVIATION_SCALED
            || config.max_liq_deviation_scaled > MAX_POOL_EXECUTION_DEVIATION_SCALED
        {
            panic!("invalid exit execution config");
        }
        let _ = get_market(&env, &margin_asset);
        let _ = get_market(&env, &base_asset);
        crate::helpers::set_perps_pair_exit_config(
            &env,
            &margin_asset,
            &base_asset,
            &side,
            &config,
        );
    }

    pub fn get_perps_pair_exit_config(
        env: Env,
        margin_asset: Address,
        base_asset: Address,
        side: PositionSide,
    ) -> PerpsPairExitExecutionConfig {
        bump_core_ttl(&env);
        crate::helpers::get_perps_pair_exit_config_or_default(
            &env,
            &margin_asset,
            &base_asset,
            &side,
        )
    }

    pub fn set_max_slippage_scaled(env: Env, admin: Address, max_slippage_scaled: u128) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if max_slippage_scaled == 0 || max_slippage_scaled > MAX_SLIPPAGE_SCALED_CAP {
            panic!("invalid slippage");
        }
        env.storage()
            .persistent()
            .set(&DataKey::MaxSlippageScaled, &max_slippage_scaled);
    }

    /// Propose a swap-adapter replacement. Existing routed positions depend on
    /// this trust boundary, so replacement is timelocked like WASM upgrades.
    pub fn set_swap_adapter(env: Env, admin: Address, swap_adapter: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        Self::assert_valid_swap_adapter(&env, &swap_adapter);
        env.storage()
            .persistent()
            .set(&DataKey::PendingSwapAdapter, &swap_adapter);
        env.storage().persistent().set(
            &DataKey::PendingSwapAdapterEta,
            &env.ledger()
                .timestamp()
                .saturating_add(UPGRADE_TIMELOCK_SECS),
        );
    }

    pub fn execute_swap_adapter_update(env: Env, admin: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        let pending: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingSwapAdapter)
            .expect("pending swap adapter not set");
        let eta: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PendingSwapAdapterEta)
            .expect("pending swap adapter eta not set");
        if env.ledger().timestamp() < eta {
            panic!("config timelocked");
        }
        Self::assert_valid_swap_adapter(&env, &pending);
        env.storage()
            .persistent()
            .set(&DataKey::SwapAdapter, &pending);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingSwapAdapter);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingSwapAdapterEta);
    }

    pub fn set_open_fee_bps(env: Env, admin: Address, fee_bps: u128) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if fee_bps > MAX_BASIS_FEE_BPS {
            panic!("fee too high");
        }
        env.storage()
            .persistent()
            .set(&DataKey::OpenFeeBps, &fee_bps);
    }

    pub fn set_close_fee_bps(env: Env, admin: Address, fee_bps: u128) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if fee_bps > MAX_BASIS_FEE_BPS {
            panic!("fee too high");
        }
        env.storage()
            .persistent()
            .set(&DataKey::CloseFeeBps, &fee_bps);
    }

    /// Accrue any pending fees and transfer claimable pTokens into the
    /// caller's free margin balance. Returns the number of pTokens claimed.
    pub fn claim_margin_fees(env: Env, user: Address, asset: Address) -> u128 {
        bump_core_ttl(&env);
        user.require_auth();
        let vault = get_market(&env, &asset);
        accrue_user_fee(&env, &user, &vault);
        let accrued_key = DataKey::UserMarginFeeAccrued(user.clone(), vault.clone());
        let accrued: u128 = env.storage().persistent().get(&accrued_key).unwrap_or(0);
        if accrued == 0 {
            return 0;
        }
        // Move accrued pTokens into user's free margin balance.
        // (accrue_user_fee above already advanced UserMarginFeeIndex to current fee_index,
        //  so this balance increase won't cause back-pay on the next interaction.)
        let free = get_margin_balance_ptokens(&env, &user, &vault);
        let new_free = free.checked_add(accrued).expect("margin fee overflow");
        set_margin_balance_ptokens(&env, &user, &vault, new_free);
        update_total_margin_ptokens(&env, &vault, accrued, true);
        env.storage().persistent().set(&accrued_key, &0u128);
        accrued
    }

    /// Returns total pTokens claimable by `user` for market `asset`,
    /// including earnings not yet snapshotted.
    pub fn get_claimable_margin_fees(env: Env, user: Address, asset: Address) -> u128 {
        bump_core_ttl(&env);
        let vault = get_market(&env, &asset);
        let fee_index = get_margin_fee_index(&env, &vault);
        let user_index = get_user_margin_fee_index(&env, &user, &vault);
        let delta = fee_index.saturating_sub(user_index);
        let pending = if delta > 0 {
            let user_bal = get_margin_balance_ptokens(&env, &user, &vault);
            compute_margin_fee_pending(delta, user_bal)
        } else {
            0
        };
        let accrued_key = DataKey::UserMarginFeeAccrued(user.clone(), vault.clone());
        let accrued: u128 = env.storage().persistent().get(&accrued_key).unwrap_or(0);
        accrued.checked_add(pending).expect("margin fee overflow")
    }

    pub fn get_margin_fee_index(env: Env, asset: Address) -> u128 {
        bump_core_ttl(&env);
        let vault = get_market(&env, &asset);
        get_margin_fee_index(&env, &vault)
    }

    /// Admin function: move orphaned fee pTokens (collected when pool was empty)
    /// into `recipient`'s free margin balance. Returns the amount swept.
    pub fn sweep_orphan_fees(env: Env, admin: Address, asset: Address, recipient: Address) -> u128 {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        let vault = get_market(&env, &asset);
        let orphan_key = DataKey::MarginFeeOrphan(vault.clone());
        let orphan: u128 = env.storage().persistent().get(&orphan_key).unwrap_or(0);
        if orphan == 0 {
            return 0;
        }
        accrue_user_fee(&env, &recipient, &vault);
        let free = get_margin_balance_ptokens(&env, &recipient, &vault);
        let new_free = free.checked_add(orphan).expect("margin fee overflow");
        set_margin_balance_ptokens(&env, &recipient, &vault, new_free);
        update_total_margin_ptokens(&env, &vault, orphan, true);
        env.storage().persistent().set(&orphan_key, &0u128);
        orphan
    }

    pub fn deposit_collateral(env: Env, user: Address, asset: Address, amount: u128) {
        bump_core_ttl(&env);
        user.require_auth();
        let vault = get_market(&env, &asset);
        ReceiptVaultClient::new(&env, &vault).deposit(&user, &amount);
    }

    pub fn withdraw_collateral(env: Env, user: Address, asset: Address, ptoken_amount: u128) {
        bump_core_ttl(&env);
        user.require_auth();
        let vault = get_market(&env, &asset);
        let current_ptokens = ReceiptVaultClient::new(&env, &vault).get_ptoken_balance(&user);
        if current_ptokens < ptoken_amount {
            panic!("Insufficient pTokens");
        }
        let locked = Self::locked_ptokens_in_market(env.clone(), user.clone(), vault.clone());
        let remaining = current_ptokens.saturating_sub(ptoken_amount);
        if remaining < locked {
            panic!("collateral locked");
        }
        let vault_client = ReceiptVaultClient::new(&env, &vault);
        Self::begin_margin_withdraw_if_supported(&env, &vault, &user, &user, ptoken_amount);
        vault_client.withdraw(&user, &ptoken_amount);
    }

    /// Move spot pTokens into margin custody.
    pub fn transfer_spot_to_margin(env: Env, user: Address, asset: Address, ptoken_amount: u128) {
        bump_core_ttl(&env);
        user.require_auth();
        if ptoken_amount == 0 {
            panic!("bad amount");
        }
        let vault = get_market(&env, &asset);
        Self::assert_margin_lock_configured(&env, &vault);
        let controller = env.current_contract_address();
        let amount_i128: i128 = ptoken_amount.try_into().expect("amount too large");
        // Set the vault's margin-withdraw bypass for the user so the vault's transfer
        // skips the controller-side preview_redeem_max (which would re-enter the vault).
        Self::begin_margin_withdraw_if_supported(&env, &vault, &user, &controller, ptoken_amount);
        ReceiptVaultClient::new(&env, &vault).transfer(&user, &controller, &amount_i128);
        // Accrue pending fees with OLD balance before increasing it.
        accrue_user_fee(&env, &user, &vault);
        let current = get_margin_balance_ptokens(&env, &user, &vault);
        set_margin_balance_ptokens(&env, &user, &vault, current.saturating_add(ptoken_amount));
        update_total_margin_ptokens(&env, &vault, ptoken_amount, true);
    }

    /// Move pTokens from margin custody back to spot wallet.
    pub fn transfer_margin_to_spot(env: Env, user: Address, asset: Address, ptoken_amount: u128) {
        bump_core_ttl(&env);
        user.require_auth();
        if ptoken_amount == 0 {
            panic!("bad amount");
        }
        let vault = get_market(&env, &asset);
        let current = get_margin_balance_ptokens(&env, &user, &vault);
        if current < ptoken_amount {
            panic!("insufficient margin balance");
        }
        // Accrue pending fees with OLD balance before decreasing it.
        accrue_user_fee(&env, &user, &vault);
        set_margin_balance_ptokens(&env, &user, &vault, current.saturating_sub(ptoken_amount));
        update_total_margin_ptokens(&env, &vault, ptoken_amount, false);
        let controller = env.current_contract_address();
        let amount_i128: i128 = ptoken_amount.try_into().expect("amount too large");
        // Set the vault's margin-withdraw bypass for the controller (the `from`
        // address) so the vault's transfer skips preview_redeem_max re-entry.
        Self::begin_margin_withdraw_if_supported(&env, &vault, &controller, &user, ptoken_amount);
        let transfer_args: Vec<Val> =
            (controller.clone(), user.clone(), amount_i128).into_val(&env);
        Self::authorize_controller_subcall(&env, &vault, "transfer", transfer_args);
        ReceiptVaultClient::new(&env, &vault).transfer(&controller, &user, &amount_i128);
    }

    pub fn get_margin_balance_ptokens(env: Env, user: Address, asset: Address) -> u128 {
        bump_core_ttl(&env);
        let vault = get_market(&env, &asset);
        get_margin_balance_ptokens(&env, &user, &vault)
    }

    pub fn get_margin_balance_underlying(env: Env, user: Address, asset: Address) -> u128 {
        bump_core_ttl(&env);
        let vault = get_market(&env, &asset);
        let pbal = get_margin_balance_ptokens(&env, &user, &vault);
        if pbal == 0 {
            return 0;
        }
        let rate = ReceiptVaultClient::new(&env, &vault).get_exchange_rate();
        pbal.saturating_mul(rate) / SCALE_1E6
    }

    pub fn propose_upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
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

    pub fn upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        bump_pending_upgrade_ttl(&env);
        let pending_hash: BytesN<32> = env
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
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeHash);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeEta);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn begin_open_position_v3(
        env: Env,
        user: Address,
        margin_asset: Address,
        base_asset: Address,
        margin_ptokens: u128,
        leverage: u128,
        side: PositionSide,
        pool_tokens: Vec<Address>,
        pool_id: BytesN<32>,
        pool: Address,
        amount_with_slippage: u128,
    ) -> u64 {
        bump_core_ttl(&env);
        user.require_auth();
        Self::begin_open_position_v3_impl(
            &env,
            user,
            margin_asset,
            base_asset,
            margin_ptokens,
            leverage,
            side,
            pool_tokens,
            pool_id,
            pool,
            amount_with_slippage,
        )
    }

    pub fn execute_open_position_v3(env: Env, user: Address, position_id: u64) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::execute_open_position_v3_impl(&env, user, position_id);
    }

    pub fn swap_open_position_v3(env: Env, user: Address, position_id: u64) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::swap_open_position_v3_impl(&env, user, position_id);
    }

    pub fn activate_open_position_v3(env: Env, user: Address, position_id: u64) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::activate_open_position_v3_impl(&env, user, position_id);
    }

    pub fn cancel_pending_open_v3(env: Env, user: Address, position_id: u64) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::cancel_pending_open_v3_impl(&env, user, position_id);
    }

    pub fn expire_pending_open_v3(env: Env, position_id: u64) {
        bump_core_ttl(&env);
        Self::expire_pending_open_v3_impl(&env, position_id);
    }

    pub fn resolve_pending_open_v3(env: Env, position_id: u64) {
        bump_core_ttl(&env);
        Self::resolve_pending_open_v3_impl(&env, position_id);
    }

    pub fn get_pending_perps_open(env: Env, position_id: u64) -> Option<PendingPerpsOpenPosition> {
        bump_core_ttl(&env);
        get_pending_perps_open_position(&env, position_id)
    }

    pub fn get_pending_perps_open_execution(
        env: Env,
        position_id: u64,
    ) -> Option<PendingPerpsOpenExecution> {
        bump_core_ttl(&env);
        get_pending_perps_open_execution(&env, position_id)
    }

    pub fn get_pending_perps_close(env: Env, position_id: u64) -> Option<PendingPerpsClose> {
        bump_core_ttl(&env);
        get_pending_perps_close(&env, position_id)
    }

    pub fn preview_liquidation_v3(env: Env, position_id: u64) -> PerpsLiquidationQuote {
        bump_core_ttl(&env);
        Self::preview_liquidation_v3_impl(&env, position_id)
    }

    pub fn get_perps_position(env: Env, position_id: u64) -> Option<PerpsPositionData> {
        bump_core_ttl(&env);
        get_perps_position_data(&env, position_id)
    }

    /// Replaces a disabled stored pool route for an existing V3 position.
    ///
    /// This recovery path is intentionally unavailable while the old route is
    /// still enabled. Normal route changes must happen through a new position.
    pub fn replace_position_pool_v3(
        env: Env,
        admin: Address,
        position_id: u64,
        pool_tokens: Vec<Address>,
        pool_id: BytesN<32>,
        pool: Address,
    ) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        Self::replace_position_pool_v3_impl(&env, position_id, pool_tokens, pool_id, pool);
    }

    pub fn get_pending_liquidation(env: Env, position_id: u64) -> Option<PendingLiquidation> {
        bump_core_ttl(&env);
        get_pending_liquidation(&env, position_id)
    }

    pub fn get_liquidation_takeover_after(env: Env, position_id: u64) -> Option<u64> {
        bump_core_ttl(&env);
        crate::helpers::get_pending_liquidation_takeover_after(&env, position_id)
    }

    /// Adds already-deposited position pTokens to a pending V3 open.
    pub fn add_position_collateral_v3(
        env: Env,
        user: Address,
        position_id: u64,
        position_ptokens: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::add_position_collateral_v3_impl(&env, user, position_id, position_ptokens);
    }

    pub fn repay_margin_position_v3(env: Env, user: Address, position_id: u64, amount: u128) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::repay_margin_position_v3_impl(&env, user, position_id, amount);
    }

    pub fn release_debt_free_position_v3(env: Env, user: Address, position_id: u64) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::release_debt_free_position_v3_impl(&env, user, position_id);
    }

    pub fn close_position_v3(
        env: Env,
        user: Address,
        position_id: u64,
        amount_with_slippage: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::close_position_v3_impl(&env, user, position_id, amount_with_slippage);
    }

    pub fn begin_close_position_v3(env: Env, user: Address, position_id: u64) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::begin_close_position_v3_impl(&env, user, position_id);
    }

    pub fn prepare_close_position_v3(env: Env, user: Address, position_id: u64) {
        // The close preparation touches and renews its position/vault state
        // directly. Avoid the global TTL walk so begin+withdraw fits one tx.
        user.require_auth();
        Self::begin_close_position_v3_impl(&env, user.clone(), position_id);
        Self::withdraw_close_position_v3_impl(&env, user, position_id);
    }

    pub fn withdraw_close_position_v3(env: Env, user: Address, position_id: u64) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::withdraw_close_position_v3_impl(&env, user, position_id);
    }

    pub fn swap_close_position_v3(
        env: Env,
        user: Address,
        position_id: u64,
        amount_with_slippage: u128,
    ) {
        // `begin` and `withdraw` already renewed core state. Avoid loading the
        // full core TTL set again inside this footprint-sensitive close stage.
        user.require_auth();
        Self::swap_close_position_v3_impl(&env, user, position_id, amount_with_slippage);
    }

    /// Close a Short without converting the user's entire quote-asset margin.
    ///
    /// `swap_amount_in` is the pool-quoted amount of position collateral needed
    /// to buy back the debt asset. `min_debt_out` must cover the current debt;
    /// any unspent position collateral is returned to free margin on settlement.
    pub fn swap_close_short_position_v3(
        env: Env,
        user: Address,
        position_id: u64,
        swap_amount_in: u128,
        min_debt_out: u128,
    ) {
        // This can only follow a live pending close created by the core-bumping
        // begin/withdraw stages, so another global TTL walk is redundant.
        user.require_auth();
        Self::swap_close_short_position_v3_impl(
            &env,
            user,
            position_id,
            swap_amount_in,
            min_debt_out,
        );
    }

    pub fn finish_close_position_v3(env: Env, position_id: u64) {
        // The short-lived pending close anchors this staged operation. Skipping
        // the global TTL walk keeps repayment plus margin credit under 100 keys.
        Self::finish_close_position_v3_impl(&env, position_id);
    }

    pub fn cancel_close_position_v3(env: Env, user: Address, position_id: u64) {
        bump_core_ttl(&env);
        user.require_auth();
        Self::cancel_close_position_v3_impl(&env, user, position_id);
    }

    pub fn expire_close_position_v3(env: Env, position_id: u64) {
        bump_core_ttl(&env);
        Self::expire_close_position_v3_impl(&env, position_id);
    }

    pub fn liquidate_position_v3(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        Self::liquidate_position_v3_impl(&env, liquidator, position_id);
    }

    pub fn begin_liquidation_v3(env: Env, liquidator: Address, position_id: u64) {
        bump_instance_ttl(&env);
        liquidator.require_auth();
        Self::begin_liquidation_v3_impl(&env, liquidator, position_id);
    }

    pub fn swap_liquidation_v3(
        env: Env,
        liquidator: Address,
        position_id: u64,
        amount_with_slippage: u128,
    ) {
        bump_instance_ttl(&env);
        liquidator.require_auth();
        Self::swap_liquidation_v3_impl(&env, liquidator, position_id, amount_with_slippage);
    }

    pub fn finish_liquidation_v3(env: Env, liquidator: Address, position_id: u64) {
        bump_instance_ttl(&env);
        liquidator.require_auth();
        Self::finish_liquidation_v3_impl(&env, liquidator, position_id);
    }

    pub fn locked_ptokens_in_market(env: Env, user: Address, market: Address) -> u128 {
        bump_core_ttl(&env);
        let position_ids = read_user_positions(&env, &user);
        let mut total_locked = 0u128;
        for position_id in position_ids.iter() {
            let position: Option<Position> = env
                .storage()
                .persistent()
                .get(&DataKey::Position(position_id));
            let Some(position) = position else {
                continue;
            };
            if position.status != PositionStatus::Open {
                continue;
            }

            let vaults = get_position_vaults_no_bump(&env, position_id, &position);
            if vaults.position_vault == market {
                total_locked = total_locked.saturating_add(position.collateral_ptokens);
            }
        }
        total_locked
    }

    pub fn get_position(env: Env, position_id: u64) -> Option<Position> {
        bump_core_ttl(&env);
        bump_position_ttl(&env, position_id);
        let position: Option<Position> = env
            .storage()
            .persistent()
            .get(&DataKey::Position(position_id));
        position
    }

    pub fn get_user_positions(env: Env, user: Address) -> Vec<u64> {
        bump_core_ttl(&env);
        read_user_positions(&env, &user)
    }

    pub fn compact_user_positions(env: Env, user: Address, limit: u32) -> u32 {
        bump_core_ttl(&env);
        compact_user_positions_bounded(&env, &user, limit)
    }

    pub fn get_position_counter(env: Env) -> u64 {
        bump_core_ttl(&env);
        env.storage()
            .persistent()
            .get(&DataKey::PositionCounter)
            .expect("position counter missing")
    }

    pub fn get_health_factor(env: Env, position_id: u64) -> u128 {
        bump_core_ttl(&env);
        let position = get_position_or_panic(&env, position_id);
        if get_position_mode(&env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        Self::get_perps_health_factor(&env, position_id, &position)
    }

    pub(crate) fn oracle_min_out(
        env: &Env,
        token_in: &Address,
        token_out: &Address,
        amount_in: u128,
    ) -> u128 {
        let in_price = get_price_usd(env, token_in);
        let out_price = get_price_usd(env, token_out);
        Self::oracle_min_out_from_prices(env, in_price, out_price, amount_in)
    }

    pub(crate) fn oracle_min_out_from_prices(
        env: &Env,
        in_price: (u128, u128),
        out_price: (u128, u128),
        amount_in: u128,
    ) -> u128 {
        Self::oracle_out_from_prices(
            in_price,
            out_price,
            amount_in,
            SCALE_1E6.saturating_sub(get_max_slippage_scaled(env)),
        )
    }

    pub(crate) fn oracle_expected_out_from_prices(
        in_price: (u128, u128),
        out_price: (u128, u128),
        amount_in: u128,
    ) -> u128 {
        Self::oracle_out_from_prices(in_price, out_price, amount_in, SCALE_1E6)
    }

    fn oracle_out_from_prices(
        in_price: (u128, u128),
        out_price: (u128, u128),
        amount_in: u128,
        output_factor_scaled: u128,
    ) -> u128 {
        if in_price.0 == 0 || in_price.1 == 0 || out_price.0 == 0 || out_price.1 == 0 {
            panic!("invalid price");
        }
        let mut n_amount = amount_in;
        let mut n_in_price = in_price.0;
        let mut n_out_scale = out_price.1;
        let mut n_slippage = output_factor_scaled;
        let mut d_in_scale = in_price.1;
        let mut d_out_price = out_price.0;
        let mut d_scale = SCALE_1E6;

        Self::cancel_factor(&mut n_amount, &mut d_in_scale);
        Self::cancel_factor(&mut n_amount, &mut d_out_price);
        Self::cancel_factor(&mut n_amount, &mut d_scale);
        Self::cancel_factor(&mut n_in_price, &mut d_in_scale);
        Self::cancel_factor(&mut n_in_price, &mut d_out_price);
        Self::cancel_factor(&mut n_in_price, &mut d_scale);
        Self::cancel_factor(&mut n_out_scale, &mut d_in_scale);
        Self::cancel_factor(&mut n_out_scale, &mut d_out_price);
        Self::cancel_factor(&mut n_out_scale, &mut d_scale);
        Self::cancel_factor(&mut n_slippage, &mut d_in_scale);
        Self::cancel_factor(&mut n_slippage, &mut d_out_price);
        Self::cancel_factor(&mut n_slippage, &mut d_scale);

        let numerator = n_amount
            .checked_mul(n_in_price)
            .and_then(|v| v.checked_mul(n_out_scale))
            .and_then(|v| v.checked_mul(n_slippage))
            .expect("oracle min overflow");
        let denominator = d_in_scale
            .checked_mul(d_out_price)
            .and_then(|v| v.checked_mul(d_scale))
            .expect("oracle min overflow");
        let min_out = Self::ceil_div(numerator, denominator);
        if min_out == 0 {
            panic!("swap amount too small");
        }
        min_out
    }

    fn cancel_factor(numerator: &mut u128, denominator: &mut u128) {
        let divisor = Self::gcd(*numerator, *denominator);
        if divisor > 1 {
            *numerator /= divisor;
            *denominator /= divisor;
        }
    }

    fn gcd(mut a: u128, mut b: u128) -> u128 {
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        a
    }

    pub(crate) fn transfer_controller_underlying(
        env: &Env,
        token: &Address,
        recipient: &Address,
        amount: u128,
    ) {
        if amount == 0 {
            return;
        }
        let controller = env.current_contract_address();
        let amount_i128: i128 = amount.try_into().expect("amount too large");
        Self::authorize_controller_subcall(
            env,
            token,
            "transfer",
            (controller.clone(), recipient.clone(), amount_i128).into_val(env),
        );
        token::TokenClient::new(env, token).transfer(&controller, recipient, &amount_i128);
    }

    pub(crate) fn entry_price_scaled(numerator: u128, denominator: u128) -> u128 {
        if denominator == 0 {
            panic!("entry price division by zero");
        }
        numerator
            .checked_mul(SCALE_1E6)
            .expect("entry price overflow")
            / denominator
    }

    pub(crate) fn ceil_div(numerator: u128, denominator: u128) -> u128 {
        if denominator == 0 {
            panic!("division by zero");
        }
        if numerator == 0 {
            0
        } else {
            numerator.saturating_sub(1) / denominator + 1
        }
    }

    pub(crate) fn assert_margin_lock_configured(env: &Env, vault: &Address) {
        let configured = env.try_invoke_contract::<Option<Address>, InvokeError>(
            vault,
            &Symbol::new(env, "get_margin_controller"),
            ().into_val(env),
        );
        match configured {
            Ok(Ok(Some(controller))) if controller == env.current_contract_address() => {}
            Ok(Ok(_)) => panic!("margin lock not configured"),
            Err(_) => panic!("margin lock not configured"),
            Ok(Err(_)) => panic!("margin lock not configured"),
        }
    }

    pub(crate) fn assert_market_supported(env: &Env, vault: &Address) {
        if !get_peridottroller(env).is_market_supported(vault) {
            panic!("market not supported");
        }
    }

    pub(crate) fn assert_market_not_borrow_paused(env: &Env, vault: &Address) {
        if get_peridottroller(env).is_borrow_paused(vault) {
            panic!("borrow paused");
        }
    }

    fn assert_valid_swap_adapter(env: &Env, swap_adapter: &Address) {
        match env.try_invoke_contract::<bool, InvokeError>(
            swap_adapter,
            &Symbol::new(env, "is_pool_allowed"),
            (env.current_contract_address(),).into_val(env),
        ) {
            Ok(Ok(true)) => {}
            _ => panic!("invalid swap adapter"),
        }
    }

    pub(crate) fn begin_margin_withdraw_if_supported(
        env: &Env,
        vault: &Address,
        user: &Address,
        recipient: &Address,
        max_ptokens: u128,
    ) {
        let controller = env.current_contract_address();
        let begin_args: Vec<Val> = (
            controller.clone(),
            user.clone(),
            recipient.clone(),
            max_ptokens,
        )
            .into_val(env);
        Self::authorize_controller_subcall(env, vault, "begin_margin_withdraw", begin_args);
        let _ = env.try_invoke_contract::<(), InvokeError>(
            vault,
            &Symbol::new(env, "begin_margin_withdraw"),
            (controller, user.clone(), recipient.clone(), max_ptokens).into_val(env),
        );
    }

    pub(crate) fn authorize_controller_subcall(
        env: &Env,
        contract: &Address,
        fn_name: &str,
        args: Vec<Val>,
    ) {
        let mut auths = Vec::new(env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: contract.clone(),
                fn_name: Symbol::new(env, fn_name),
                args,
            },
            sub_invocations: Vec::new(env),
        }));
        env.authorize_as_current_contract(auths);
    }

    // No-swap borrow path performs health checks before any additional collateral is added.
    // This caps leverage to what the initial collateral can support on its own.
}

fn assert_expected_admin(env: &Env, admin: &Address) {
    #[cfg(test)]
    {
        let _ = env;
        let _ = admin;
        return;
    }
    #[cfg(not(test))]
    {
        let expected_admin_str = option_env!("MARGIN_CONTROLLER_INIT_ADMIN")
            .expect("MARGIN_CONTROLLER_INIT_ADMIN not set");
        let expected_admin = Address::from_string(&String::from_str(env, expected_admin_str));
        if admin != &expected_admin {
            panic!("unexpected admin");
        }
    }
}
