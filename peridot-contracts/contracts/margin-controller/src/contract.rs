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

/// Snapshot of a vault's pricing inputs, fetched once via cross-contract calls
/// and reused for repeated valuation math within a single entrypoint. Used by
/// `liquidate_position_v2` to avoid re-reading price/rate/CF (each `get_price_usd`
/// hits the oracle) ~5x per liquidation, which otherwise exceeds the CPU budget.
struct VaultValCtx {
    price_num: u128,
    price_den: u128,
    rate: u128,
    cf: u128,
}

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

    /// Legacy margin V1 is intentionally disabled. Use Margin V2 entrypoints.
    pub fn open_position(
        env: Env,
        user: Address,
        collateral_asset: Address,
        base_asset: Address,
        collateral_amount: u128,
        leverage: u128,
        side: PositionSide,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        amount_with_slippage: u128,
    ) -> u64 {
        let _ = (
            env,
            user,
            collateral_asset,
            base_asset,
            collateral_amount,
            leverage,
            side,
            swaps_chain,
            amount_with_slippage,
        );
        panic!("legacy margin disabled");
    }

    /// Margin V2 open path that consumes pTokens from margin balance and borrows
    /// against a position-scoped debt namespace in the vault.
    pub fn open_position_v2(
        env: Env,
        user: Address,
        collateral_asset: Address,
        base_asset: Address,
        collateral_ptokens: u128,
        leverage: u128,
        side: PositionSide,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        amount_with_slippage: u128,
    ) -> u64 {
        bump_core_ttl(&env);
        user.require_auth();
        let max_leverage = get_max_leverage(&env);
        if leverage < 1 || leverage > max_leverage {
            panic!("bad leverage");
        }
        if collateral_ptokens == 0 {
            panic!("bad collateral");
        }
        if collateral_asset == base_asset {
            panic!("assets must differ");
        }
        let (debt_asset, position_asset) = match side {
            PositionSide::Long => (collateral_asset.clone(), base_asset.clone()),
            PositionSide::Short => (base_asset.clone(), collateral_asset.clone()),
        };
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        let swap_adapter = get_swap_adapter(&env);
        validate_swaps_chain(
            &env,
            &swap_adapter,
            &swaps_chain,
            &debt_asset,
            &position_asset,
        );

        let collateral_vault = get_market(&env, &collateral_asset);
        let debt_vault = get_market(&env, &debt_asset);
        let position_vault = get_market(&env, &position_asset);
        Self::assert_market_supported(&env, &collateral_vault);
        Self::assert_market_supported(&env, &debt_vault);
        Self::assert_market_supported(&env, &position_vault);
        Self::assert_market_not_borrow_paused(&env, &debt_vault);
        Self::assert_margin_lock_configured(&env, &collateral_vault);
        Self::assert_margin_lock_configured(&env, &debt_vault);
        Self::assert_margin_lock_configured(&env, &position_vault);

        let open_fee_bps: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::OpenFeeBps)
            .unwrap_or(0);
        // Fee scales with leverage: higher leverage = higher cost to LP providers.
        let open_fee_ptokens = collateral_ptokens
            .checked_mul(leverage)
            .expect("fee overflow")
            .checked_mul(open_fee_bps)
            .expect("fee overflow")
            / BPS_SCALE;
        let total_deduct = collateral_ptokens.saturating_add(open_fee_ptokens);
        let free_collateral = get_margin_balance_ptokens(&env, &user, &collateral_vault);
        if free_collateral < total_deduct {
            panic!("insufficient margin balance");
        }
        // Accrue pending fees with OLD balance before decreasing it.
        accrue_user_fee(&env, &user, &collateral_vault);
        set_margin_balance_ptokens(
            &env,
            &user,
            &collateral_vault,
            free_collateral.saturating_sub(total_deduct),
        );
        update_total_margin_ptokens(&env, &collateral_vault, total_deduct, false);

        let coll_price = get_price_usd(&env, &collateral_asset);
        let debt_price = if debt_asset == collateral_asset {
            coll_price
        } else {
            get_price_usd(&env, &debt_asset)
        };
        if coll_price.0 == 0 || coll_price.1 == 0 {
            panic!("invalid collateral price");
        }
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let coll_rate = ReceiptVaultClient::new(&env, &collateral_vault).get_exchange_rate();
        let collateral_underlying = collateral_ptokens
            .checked_mul(coll_rate)
            .expect("valuation overflow")
            / SCALE_1E6;
        let collateral_value = collateral_underlying
            .checked_mul(coll_price.0)
            .expect("valuation overflow")
            / coll_price.1;
        let collateral_cf = get_peridottroller(&env).get_market_cf(&collateral_vault);
        if collateral_cf > SCALE_1E6 {
            panic!("invalid market cf");
        }
        let discounted_collateral_value = collateral_value
            .checked_mul(collateral_cf)
            .expect("valuation overflow")
            / SCALE_1E6;
        let target_value = discounted_collateral_value
            .checked_mul(leverage)
            .expect("valuation overflow");
        let borrow_value = target_value.saturating_sub(discounted_collateral_value);
        if borrow_value == 0 {
            panic!("zero borrow");
        }
        let borrow_amount = borrow_value
            .checked_mul(debt_price.1)
            .expect("valuation overflow")
            / debt_price.0;
        if borrow_amount == 0 {
            panic!("borrow too small");
        }
        let position_price = if position_asset == collateral_asset {
            coll_price
        } else if position_asset == debt_asset {
            debt_price
        } else {
            get_price_usd(&env, &position_asset)
        };
        let min_out_oracle =
            Self::oracle_min_out_from_prices(&env, debt_price, position_price, borrow_amount);
        if amount_with_slippage < min_out_oracle {
            panic!("slippage too high");
        }

        let id = next_position_id(&env);
        let mut position = Position {
            owner: user.clone(),
            side,
            collateral_asset: position_asset.clone(),
            debt_asset: debt_asset.clone(),
            collateral_ptokens: 0u128,
            debt_shares: 0u128,
            entry_price_scaled: 0u128,
            opened_at: env.ledger().timestamp(),
            status: PositionStatus::Open,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Position(id), &position);
        set_position_mode(&env, id, PositionMode::MarginV2);
        set_position_vaults(&env, id, &collateral_vault, &debt_vault, &position_vault);
        set_position_initial_lock(&env, id, &collateral_vault, collateral_ptokens);
        bump_position_ttl(&env, id);

        let debt_vault_client = ReceiptVaultClient::new(&env, &debt_vault);
        debt_vault_client.init_margin_borrow_state(&id);
        debt_vault_client.borrow_for_margin(&id, &user, &borrow_amount);
        let position_token = token::TokenClient::new(&env, &position_asset);
        let position_bal_before = position_token.balance(&user);
        let _reported_received = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
            &user,
            &swaps_chain,
            &debt_asset,
            &borrow_amount,
            &amount_with_slippage,
        );
        let position_bal_after = position_token.balance(&user);
        let received = if position_bal_after <= position_bal_before {
            0u128
        } else {
            (position_bal_after - position_bal_before) as u128
        };
        if received < min_out_oracle {
            panic!("slippage too high");
        }
        if received == 0 {
            panic!("swap failed");
        }

        let p_before = ReceiptVaultClient::new(&env, &position_vault).get_ptoken_balance(&user);
        ReceiptVaultClient::new(&env, &position_vault).deposit(&user, &received);
        let p_after = ReceiptVaultClient::new(&env, &position_vault).get_ptoken_balance(&user);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta == 0 {
            panic!("no collateral minted");
        }
        let controller = env.current_contract_address();
        let p_delta_i128: i128 = p_delta.try_into().expect("amount too large");
        Self::begin_margin_withdraw_if_supported(
            &env,
            &position_vault,
            &user,
            &controller,
            p_delta,
        );
        ReceiptVaultClient::new(&env, &position_vault).transfer(&user, &controller, &p_delta_i128);

        let position_cf = get_peridottroller(&env).get_market_cf(&position_vault);
        if position_cf > SCALE_1E6 {
            panic!("invalid market cf");
        }
        let position_collateral_value =
            Self::discounted_ptoken_value_usd(&env, &position_vault, p_delta);
        let combined_collateral_value = discounted_collateral_value
            .checked_add(position_collateral_value)
            .expect("valuation overflow");
        let debt_value = borrow_amount
            .checked_mul(debt_price.0)
            .expect("valuation overflow")
            / debt_price.1;
        let min_open_collateral_value = debt_value
            .checked_mul(DEFAULT_MARGIN_MIN_OPEN_HF_SCALED)
            .expect("valuation overflow")
            / SCALE_1E6;
        if combined_collateral_value < min_open_collateral_value {
            panic!("insufficient collateral");
        }

        let entry_price_scaled = Self::entry_price_scaled(borrow_amount, received);
        position.collateral_ptokens = p_delta;
        position.entry_price_scaled = entry_price_scaled;
        env.storage()
            .persistent()
            .set(&DataKey::Position(id), &position);
        bump_position_ttl(&env, id);
        // Distribute open fee to LP holders of the collateral vault.
        collect_margin_fee(&env, &collateral_vault, open_fee_ptokens);
        push_user_position(&env, &user, id);
        id
    }

    /// Budget-friendly V2 open flow, step 1.
    ///
    /// This locks the user's margin pTokens and creates a pending position, but
    /// does not create margin debt yet. The debt is opened only during a
    /// finalization call that atomically places collateral into controller
    /// custody, preventing the borrowed asset from being withdrawn during the
    /// pending window.
    pub fn begin_open_position_v2(
        env: Env,
        user: Address,
        collateral_asset: Address,
        base_asset: Address,
        collateral_ptokens: u128,
        leverage: u128,
        side: PositionSide,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        amount_with_slippage: u128,
    ) -> u64 {
        bump_core_ttl(&env);
        user.require_auth();
        let max_leverage = get_max_leverage(&env);
        if leverage < 1 || leverage > max_leverage {
            panic!("bad leverage");
        }
        if collateral_ptokens == 0 {
            panic!("bad collateral");
        }
        if collateral_asset == base_asset {
            panic!("assets must differ");
        }
        let (debt_asset, position_asset) = match side.clone() {
            PositionSide::Long => (collateral_asset.clone(), base_asset.clone()),
            PositionSide::Short => (base_asset.clone(), collateral_asset.clone()),
        };
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        let swap_adapter = get_swap_adapter(&env);
        validate_swaps_chain(
            &env,
            &swap_adapter,
            &swaps_chain,
            &debt_asset,
            &position_asset,
        );

        let collateral_vault = get_market(&env, &collateral_asset);
        let debt_vault = get_market(&env, &debt_asset);
        let position_vault = get_market(&env, &position_asset);
        Self::assert_market_supported(&env, &collateral_vault);
        Self::assert_market_supported(&env, &debt_vault);
        Self::assert_market_supported(&env, &position_vault);
        Self::assert_market_not_borrow_paused(&env, &debt_vault);
        Self::assert_margin_lock_configured(&env, &collateral_vault);
        Self::assert_margin_lock_configured(&env, &debt_vault);
        Self::assert_margin_lock_configured(&env, &position_vault);

        let open_fee_bps: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::OpenFeeBps)
            .unwrap_or(0);
        let open_fee_ptokens = collateral_ptokens
            .checked_mul(leverage)
            .expect("fee overflow")
            .checked_mul(open_fee_bps)
            .expect("fee overflow")
            / BPS_SCALE;
        let total_deduct = collateral_ptokens.saturating_add(open_fee_ptokens);
        let free_collateral = get_margin_balance_ptokens(&env, &user, &collateral_vault);
        if free_collateral < total_deduct {
            panic!("insufficient margin balance");
        }
        accrue_user_fee(&env, &user, &collateral_vault);
        set_margin_balance_ptokens(
            &env,
            &user,
            &collateral_vault,
            free_collateral.saturating_sub(total_deduct),
        );
        update_total_margin_ptokens(&env, &collateral_vault, total_deduct, false);

        let coll_price = get_price_usd(&env, &collateral_asset);
        let debt_price = if debt_asset == collateral_asset {
            coll_price
        } else {
            get_price_usd(&env, &debt_asset)
        };
        if coll_price.0 == 0 || coll_price.1 == 0 {
            panic!("invalid collateral price");
        }
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let coll_rate = ReceiptVaultClient::new(&env, &collateral_vault).get_exchange_rate();
        let collateral_underlying = collateral_ptokens
            .checked_mul(coll_rate)
            .expect("valuation overflow")
            / SCALE_1E6;
        let collateral_value = collateral_underlying
            .checked_mul(coll_price.0)
            .expect("valuation overflow")
            / coll_price.1;
        let collateral_cf = get_peridottroller(&env).get_market_cf(&collateral_vault);
        if collateral_cf > SCALE_1E6 {
            panic!("invalid market cf");
        }
        let discounted_collateral_value = collateral_value
            .checked_mul(collateral_cf)
            .expect("valuation overflow")
            / SCALE_1E6;
        let target_value = discounted_collateral_value
            .checked_mul(leverage)
            .expect("valuation overflow");
        let borrow_value = target_value.saturating_sub(discounted_collateral_value);
        if borrow_value == 0 {
            panic!("zero borrow");
        }
        let borrow_amount = borrow_value
            .checked_mul(debt_price.1)
            .expect("valuation overflow")
            / debt_price.0;
        if borrow_amount == 0 {
            panic!("borrow too small");
        }
        let position_price = if position_asset == collateral_asset {
            coll_price
        } else if position_asset == debt_asset {
            debt_price
        } else {
            get_price_usd(&env, &position_asset)
        };
        let min_out_oracle =
            Self::oracle_min_out_from_prices(&env, debt_price, position_price, borrow_amount);
        if amount_with_slippage < min_out_oracle {
            panic!("slippage too high");
        }

        let id = next_position_id(&env);
        let now = env.ledger().timestamp();
        let position = Position {
            owner: user.clone(),
            side,
            collateral_asset: position_asset.clone(),
            debt_asset: debt_asset.clone(),
            collateral_ptokens: 0u128,
            debt_shares: 0u128,
            entry_price_scaled: 0u128,
            opened_at: now,
            status: PositionStatus::PendingOpen,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Position(id), &position);
        set_position_mode(&env, id, PositionMode::MarginV2);
        set_position_vaults(&env, id, &collateral_vault, &debt_vault, &position_vault);
        set_position_initial_lock(&env, id, &collateral_vault, collateral_ptokens);
        let pending = PendingOpenPosition {
            owner: user.clone(),
            collateral_asset,
            debt_asset: debt_asset.clone(),
            position_asset,
            collateral_vault,
            debt_vault: debt_vault.clone(),
            position_vault,
            collateral_ptokens,
            open_fee_ptokens,
            borrow_amount,
            min_position_amount: amount_with_slippage,
            expires_at: now.saturating_add(PENDING_OPEN_TTL_SECS),
        };
        set_pending_open_position(&env, id, &pending);
        push_user_position(&env, &user, id);
        ReceiptVaultClient::new(&env, &debt_vault).init_margin_borrow_state(&id);
        id
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

    pub fn get_pending_liquidation(env: Env, position_id: u64) -> Option<PendingLiquidation> {
        bump_core_ttl(&env);
        get_pending_liquidation(&env, position_id)
    }

    /// Budget-friendly V2 open flow, step 2 for routed swaps.
    ///
    /// The margin debt is created and swapped in this call. If the swap,
    /// deposit, or health check fails, the whole transaction rolls back and the
    /// pending position remains debt-free.
    pub fn finalize_open_swap_v2(
        env: Env,
        user: Address,
        position_id: u64,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        amount_with_slippage: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        let mut position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let pending = get_pending_open_position_or_panic(&env, position_id);
        if pending.owner != user {
            panic!("not owner");
        }
        if env.ledger().timestamp() > pending.expires_at {
            panic!("pending open expired");
        }
        if amount_with_slippage < pending.min_position_amount {
            panic!("slippage too high");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        if vaults.collateral_vault != pending.collateral_vault
            || vaults.debt_vault != pending.debt_vault
            || vaults.position_vault != pending.position_vault
        {
            panic!("pending vault mismatch");
        }
        let swap_adapter = get_swap_adapter(&env);
        validate_swaps_chain(
            &env,
            &swap_adapter,
            &swaps_chain,
            &pending.debt_asset,
            &pending.position_asset,
        );

        let debt_token = token::TokenClient::new(&env, &pending.debt_asset);
        let debt_bal_before = debt_token.balance(&user);
        let debt_vault_client = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        debt_vault_client.init_margin_borrow_state(&position_id);
        debt_vault_client.borrow_for_margin(&position_id, &user, &pending.borrow_amount);

        let position_token = token::TokenClient::new(&env, &pending.position_asset);
        let position_bal_before = position_token.balance(&user);
        let _reported_received = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
            &user,
            &swaps_chain,
            &pending.debt_asset,
            &pending.borrow_amount,
            &amount_with_slippage,
        );
        let position_bal_after = position_token.balance(&user);
        let received = if position_bal_after <= position_bal_before {
            0u128
        } else {
            (position_bal_after - position_bal_before) as u128
        };
        if received < amount_with_slippage {
            panic!("slippage too high");
        }
        if received == 0 {
            panic!("swap failed");
        }
        let debt_bal_after = debt_token.balance(&user);
        if debt_bal_after > debt_bal_before {
            panic!("borrow not spent");
        }

        let p_before =
            ReceiptVaultClient::new(&env, &vaults.position_vault).get_ptoken_balance(&user);
        ReceiptVaultClient::new(&env, &vaults.position_vault).deposit(&user, &received);
        let p_after =
            ReceiptVaultClient::new(&env, &vaults.position_vault).get_ptoken_balance(&user);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta == 0 {
            panic!("no collateral minted");
        }
        let controller = env.current_contract_address();
        let p_delta_i128: i128 = p_delta.try_into().expect("amount too large");
        Self::begin_margin_withdraw_if_supported(
            &env,
            &vaults.position_vault,
            &user,
            &controller,
            p_delta,
        );
        ReceiptVaultClient::new(&env, &vaults.position_vault).transfer(
            &user,
            &controller,
            &p_delta_i128,
        );

        debt_vault_client.update_interest();
        let debt_amount = debt_vault_client.get_margin_borrow_balance(&position_id);
        if debt_amount == 0 {
            panic!("zero debt");
        }
        Self::finalize_pending_open_collateral(
            &env,
            &mut position,
            position_id,
            &pending,
            &vaults,
            p_delta,
            received,
            debt_amount,
        );
    }

    /// Budget-friendly V2 open flow, step 2.
    ///
    /// `position_amount` is user-supplied position asset. This function deposits
    /// that amount into the position vault, moves the resulting pTokens into
    /// controller custody, then creates the margin debt only after the
    /// collateral has passed the open-health check.
    pub fn finalize_open_position_v2(
        env: Env,
        user: Address,
        position_id: u64,
        position_amount: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        if position_amount == 0 {
            panic!("bad amount");
        }
        let mut position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let pending = get_pending_open_position_or_panic(&env, position_id);
        if pending.owner != user {
            panic!("not owner");
        }
        if env.ledger().timestamp() > pending.expires_at {
            panic!("pending open expired");
        }
        if position_amount < pending.min_position_amount {
            panic!("slippage too high");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        if vaults.collateral_vault != pending.collateral_vault
            || vaults.debt_vault != pending.debt_vault
            || vaults.position_vault != pending.position_vault
        {
            panic!("pending vault mismatch");
        }

        let p_before =
            ReceiptVaultClient::new(&env, &vaults.position_vault).get_ptoken_balance(&user);
        ReceiptVaultClient::new(&env, &vaults.position_vault).deposit(&user, &position_amount);
        let p_after =
            ReceiptVaultClient::new(&env, &vaults.position_vault).get_ptoken_balance(&user);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta == 0 {
            panic!("no collateral minted");
        }
        let controller = env.current_contract_address();
        let p_delta_i128: i128 = p_delta.try_into().expect("amount too large");
        Self::begin_margin_withdraw_if_supported(
            &env,
            &vaults.position_vault,
            &user,
            &controller,
            p_delta,
        );
        ReceiptVaultClient::new(&env, &vaults.position_vault).transfer(
            &user,
            &controller,
            &p_delta_i128,
        );

        let debt_amount = pending.borrow_amount;
        let debt_vault_client = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        debt_vault_client.init_margin_borrow_state(&position_id);
        debt_vault_client.borrow_for_margin(&position_id, &user, &debt_amount);
        let actual_debt = debt_vault_client.get_margin_borrow_balance(&position_id);
        if actual_debt == 0 {
            panic!("zero debt");
        }
        Self::finalize_pending_open_collateral(
            &env,
            &mut position,
            position_id,
            &pending,
            &vaults,
            p_delta,
            position_amount,
            actual_debt,
        );
    }

    /// Budget-friendly V2 open flow, final step for live routed opens.
    ///
    /// The user first deposits position asset into the position vault, then
    /// passes the pToken amount minted here. The margin debt is created only
    /// after those pTokens are moved into controller custody and pass the
    /// open-health check.
    pub fn finalize_open_ptokens_v2(
        env: Env,
        user: Address,
        position_id: u64,
        position_ptokens: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        if position_ptokens == 0 {
            panic!("bad ptokens");
        }
        let mut position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let pending = get_pending_open_position_or_panic(&env, position_id);
        if pending.owner != user {
            panic!("not owner");
        }
        if env.ledger().timestamp() > pending.expires_at {
            panic!("pending open expired");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        if vaults.collateral_vault != pending.collateral_vault
            || vaults.debt_vault != pending.debt_vault
            || vaults.position_vault != pending.position_vault
        {
            panic!("pending vault mismatch");
        }

        let position_vault_client = ReceiptVaultClient::new(&env, &vaults.position_vault);
        let user_ptokens = position_vault_client.get_ptoken_balance(&user);
        if user_ptokens < position_ptokens {
            panic!("insufficient ptokens");
        }
        let position_rate = position_vault_client.get_exchange_rate();
        let position_amount = position_ptokens
            .checked_mul(position_rate)
            .expect("valuation overflow")
            / SCALE_1E6;
        if position_amount == 0 {
            panic!("zero collateral");
        }
        if position_amount < pending.min_position_amount {
            panic!("slippage too high");
        }

        let controller = env.current_contract_address();
        let p_delta_i128: i128 = position_ptokens.try_into().expect("amount too large");
        Self::begin_margin_withdraw_if_supported(
            &env,
            &vaults.position_vault,
            &user,
            &controller,
            position_ptokens,
        );
        position_vault_client.transfer(&user, &controller, &p_delta_i128);

        let debt_amount = pending.borrow_amount;
        let debt_vault_client = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        debt_vault_client.init_margin_borrow_state(&position_id);
        debt_vault_client.borrow_for_margin(&position_id, &user, &debt_amount);
        Self::finalize_pending_open_collateral(
            &env,
            &mut position,
            position_id,
            &pending,
            &vaults,
            position_ptokens,
            position_amount,
            debt_amount,
        );
    }

    /// Split V2 open flow, step 2a.
    ///
    /// The user has already obtained and deposited the position asset, then
    /// supplies the resulting pTokens to controller custody. No margin debt is
    /// created here, so this step can be retried/cancelled without leaving
    /// borrowed funds outside the position.
    pub fn supply_open_ptokens_v2(
        env: Env,
        user: Address,
        position_id: u64,
        position_ptokens: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        if position_ptokens == 0 {
            panic!("bad ptokens");
        }
        let position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        if get_pending_open_supplied_collateral(&env, position_id).is_some() {
            panic!("collateral already supplied");
        }
        let pending = get_pending_open_position_or_panic(&env, position_id);
        if pending.owner != user {
            panic!("not owner");
        }
        if env.ledger().timestamp() > pending.expires_at {
            panic!("pending open expired");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        if vaults.collateral_vault != pending.collateral_vault
            || vaults.debt_vault != pending.debt_vault
            || vaults.position_vault != pending.position_vault
        {
            panic!("pending vault mismatch");
        }

        let position_vault_client = ReceiptVaultClient::new(&env, &vaults.position_vault);
        let user_ptokens = position_vault_client.get_ptoken_balance(&user);
        if user_ptokens < position_ptokens {
            panic!("insufficient ptokens");
        }
        let position_rate = position_vault_client.get_exchange_rate();
        if position_rate == 0 {
            panic!("invalid exchange rate");
        }
        let position_amount = position_ptokens
            .checked_mul(position_rate)
            .expect("valuation overflow")
            / SCALE_1E6;
        if position_amount == 0 {
            panic!("zero collateral");
        }
        if position_amount < pending.min_position_amount {
            panic!("slippage too high");
        }

        let controller = env.current_contract_address();
        let p_delta_i128: i128 = position_ptokens.try_into().expect("amount too large");
        Self::begin_margin_withdraw_if_supported(
            &env,
            &vaults.position_vault,
            &user,
            &controller,
            position_ptokens,
        );
        position_vault_client.transfer(&user, &controller, &p_delta_i128);
        set_pending_open_supplied_collateral(&env, position_id, position_ptokens, position_amount);
    }

    /// Split V2 open flow, step 2b.
    ///
    /// Activates a pending position after `supply_open_ptokens_v2` has moved
    /// position collateral into custody. The trusted oracle is still used for
    /// health and borrow-limit checks; the stored supplied amount is used only
    /// for the position entry price.
    pub fn activate_open_position_v2(env: Env, user: Address, position_id: u64) {
        bump_core_ttl(&env);
        user.require_auth();
        let mut position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let pending = get_pending_open_position_or_panic(&env, position_id);
        if pending.owner != user {
            panic!("not owner");
        }
        if env.ledger().timestamp() > pending.expires_at {
            panic!("pending open expired");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        if vaults.collateral_vault != pending.collateral_vault
            || vaults.debt_vault != pending.debt_vault
            || vaults.position_vault != pending.position_vault
        {
            panic!("pending vault mismatch");
        }
        let (position_ptokens, position_amount) =
            get_pending_open_supplied_collateral_or_panic(&env, position_id);
        if position_ptokens == 0 || position_amount == 0 {
            panic!("pending collateral missing");
        }

        let debt_amount = pending.borrow_amount;
        let debt_vault_client = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        debt_vault_client.init_margin_borrow_state(&position_id);
        debt_vault_client.borrow_for_margin(&position_id, &user, &debt_amount);
        let actual_debt = debt_vault_client.get_margin_borrow_balance(&position_id);
        if actual_debt == 0 {
            panic!("zero debt");
        }
        Self::finalize_pending_open_collateral(
            &env,
            &mut position,
            position_id,
            &pending,
            &vaults,
            position_ptokens,
            position_amount,
            actual_debt,
        );
    }

    /// Cancels a pending split-open position by repaying the outstanding margin
    /// debt from the user's wallet, if this is a legacy pending created before
    /// debt-at-finalize, and returning the locked margin pTokens.
    pub fn cancel_pending_open_v2(
        env: Env,
        user: Address,
        position_id: u64,
        max_repay_amount: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        let position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let pending = get_pending_open_position_or_panic(&env, position_id);
        let vaults = get_position_vaults(&env, position_id, &position);
        let debt_vault_client = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        if debt_vault_client.get_margin_borrow_balance(&position_id) > 0 {
            let repaid =
                debt_vault_client.repay_full_for_margin(&position_id, &user, &max_repay_amount);
            if repaid > 0 && debt_vault_client.get_margin_borrow_balance(&position_id) != 0 {
                panic!("debt remains");
            }
        }

        Self::release_pending_open_supplied_collateral(&env, &user, position_id, &vaults);
        Self::release_pending_open_lock(&env, &user, position_id, pending.open_fee_ptokens);

        clear_position_storage(&env, position_id);
        remove_user_position(&env, &user, position_id);
    }

    /// Permissionless cleanup for stale debt-free pending opens.
    pub fn expire_pending_open_v2(env: Env, position_id: u64) {
        bump_core_ttl(&env);
        let position = get_position_or_panic(&env, position_id);
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let pending = get_pending_open_position_or_panic(&env, position_id);
        if env.ledger().timestamp() <= pending.expires_at {
            panic!("pending open live");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        if ReceiptVaultClient::new(&env, &vaults.debt_vault).get_margin_borrow_balance(&position_id)
            != 0
        {
            panic!("pending debt exists");
        }

        Self::release_pending_open_supplied_collateral(&env, &position.owner, position_id, &vaults);
        Self::release_pending_open_lock(
            &env,
            &position.owner,
            position_id,
            pending.open_fee_ptokens,
        );
        clear_position_storage(&env, position_id);
        remove_user_position(&env, &position.owner, position_id);
    }

    pub fn get_pending_open(env: Env, position_id: u64) -> Option<PendingOpenPosition> {
        bump_core_ttl(&env);
        get_pending_open_position(&env, position_id)
    }

    pub fn get_pending_open_supplied(env: Env, position_id: u64) -> Option<(u128, u128)> {
        bump_core_ttl(&env);
        get_pending_open_supplied_collateral(&env, position_id)
    }

    /// V2 no-swap open: consumes pTokens from the user's existing margin
    /// balance (deposit + `transfer_spot_to_margin` must have happened in
    /// prior transactions) and borrows the debt asset directly to the user
    /// via the position-scoped margin namespace. No DEX swap involved.
    ///
    /// Uses `borrow_for_margin` instead of `vault.borrow`, which skips the
    /// vault's `controller.account_liquidity` walk and per-market reward
    /// accrual. Footprint stays under Soroban's 100-entry per-tx limit.
    ///
    /// Open flow (3 user transactions):
    /// 1. `vault.deposit(user, collateral_amount)` (or `controller.deposit_collateral`)
    /// 2. `controller.transfer_spot_to_margin(user, collateral_asset, ptokens)`
    /// 3. `controller.open_position_no_swap_v2(...)`
    ///
    /// Close with `close_position_no_swap_v2`. Liquidate with
    /// `liquidate_position_v2` (collateral lives in `position.collateral_ptokens`).
    pub fn open_position_no_swap_v2(
        env: Env,
        user: Address,
        collateral_asset: Address,
        debt_asset: Address,
        collateral_ptokens: u128,
        borrow_amount: u128,
        leverage: u128,
    ) -> u64 {
        bump_core_ttl(&env);
        user.require_auth();

        if leverage != 1 {
            panic!("no-swap leverage disabled");
        }
        if collateral_asset == debt_asset {
            panic!("assets must differ");
        }
        if collateral_ptokens == 0 || borrow_amount == 0 {
            panic!("bad amounts");
        }

        let collateral_vault = get_market(&env, &collateral_asset);
        let debt_vault = get_market(&env, &debt_asset);
        Self::assert_market_supported(&env, &collateral_vault);
        Self::assert_market_supported(&env, &debt_vault);
        Self::assert_market_not_borrow_paused(&env, &debt_vault);
        // Margin-lock wiring is asserted at deploy time and re-verified by the
        // vault's `init_margin_borrow_state` / `borrow_for_margin` via
        // `require_margin_controller_auth`. Skipping redundant client-side
        // checks here to stay under the 100-entry per-tx footprint cap.

        // Consume the user's pre-deposited margin custody balance.
        let free_collateral = get_margin_balance_ptokens(&env, &user, &collateral_vault);
        if free_collateral < collateral_ptokens {
            panic!("insufficient margin balance");
        }
        // Accrue pending fees with OLD balance before decreasing it (no open fee for no-swap).
        accrue_user_fee(&env, &user, &collateral_vault);
        set_margin_balance_ptokens(
            &env,
            &user,
            &collateral_vault,
            free_collateral.saturating_sub(collateral_ptokens),
        );
        update_total_margin_ptokens(&env, &collateral_vault, collateral_ptokens, false);

        // No-swap V2 sends borrowed funds to the user instead of re-collateralizing
        // them, so leverage is not a meaningful multiplier here. Enforce LTV
        // directly with the same open-time health buffer used by swap V2.
        let collateral_cf = get_peridottroller(&env).get_market_cf(&collateral_vault);
        if collateral_cf > SCALE_1E6 {
            panic!("invalid market cf");
        }
        let collateral_price = get_price_usd(&env, &collateral_asset);
        let debt_price = get_price_usd(&env, &debt_asset);
        if collateral_price.0 == 0 || collateral_price.1 == 0 {
            panic!("invalid collateral price");
        }
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let coll_rate = ReceiptVaultClient::new(&env, &collateral_vault).get_exchange_rate();
        let collateral_underlying = collateral_ptokens
            .checked_mul(coll_rate)
            .expect("valuation overflow")
            / SCALE_1E6;
        if collateral_underlying == 0 {
            panic!("zero collateral");
        }
        let collateral_value = collateral_underlying
            .checked_mul(collateral_price.0)
            .expect("valuation overflow")
            / collateral_price.1;
        let borrow_value = borrow_amount
            .checked_mul(debt_price.0)
            .expect("valuation overflow")
            / debt_price.1;
        if borrow_value == 0 {
            panic!("borrow too small");
        }
        let discounted_collateral_value = collateral_value
            .checked_mul(collateral_cf)
            .expect("valuation overflow")
            / SCALE_1E6;
        let min_open_collateral_value = borrow_value
            .checked_mul(DEFAULT_MARGIN_MIN_OPEN_HF_SCALED)
            .expect("valuation overflow")
            / SCALE_1E6;
        if discounted_collateral_value < min_open_collateral_value {
            panic!("insufficient collateral");
        }

        let id = next_position_id(&env);
        set_position_mode(&env, id, PositionMode::MarginV2);
        // collateral_vault == position_vault for no-swap V2: the deposit IS the
        // position collateral. liquidate_position_v2 reads position.collateral_ptokens
        // and seizes from position_vault custody.
        set_position_vaults(&env, id, &collateral_vault, &debt_vault, &collateral_vault);

        let debt_vault_client = ReceiptVaultClient::new(&env, &debt_vault);
        debt_vault_client.init_margin_borrow_state(&id);
        debt_vault_client.borrow_for_margin(&id, &user, &borrow_amount);

        let entry_price_scaled = Self::entry_price_scaled(borrow_amount, collateral_underlying);

        let position = Position {
            owner: user.clone(),
            side: PositionSide::Long,
            collateral_asset,
            debt_asset,
            collateral_ptokens,
            debt_shares: 0u128,
            entry_price_scaled,
            opened_at: env.ledger().timestamp(),
            status: PositionStatus::Open,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Position(id), &position);
        bump_position_ttl(&env, id);
        push_user_position(&env, &user, id);
        id
    }

    /// Close a no-swap V2 position. The user repays the outstanding margin debt
    /// directly from their wallet (no swap) and the controller releases the
    /// collateral pTokens back to the user's margin balance, where they can be
    /// withdrawn to spot via `transfer_margin_to_spot`.
    pub fn close_position_no_swap_v2(env: Env, user: Address, position_id: u64) {
        bump_core_ttl(&env);
        user.require_auth();
        let position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        // Guard against using this entrypoint for swap-style V2 positions
        // (where collateral_vault != position_vault). Those must use close_position_v2.
        if vaults.collateral_vault != vaults.position_vault {
            panic!("use close_position_v2");
        }
        if get_position_initial_lock(&env, position_id).is_some() {
            panic!("use close_position_v2");
        }

        let debt_vault_client = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        // Accrue interest before reading debt so we pay the post-accrual amount
        // and avoid dust. repay_for_margin's internal pre-accrual cap is sized
        // against the same updated state, leaving the position at exactly 0.
        debt_vault_client.update_interest();
        let debt_amount = debt_vault_client.get_margin_borrow_balance(&position_id);
        if debt_amount > 0 {
            // User pays directly from wallet. payer.require_auth() inside
            // repay_for_margin is satisfied by the user's outer auth tree.
            debt_vault_client.repay_for_margin(&position_id, &user, &debt_amount);
        }

        // Release collateral pTokens (in controller custody) back to user's
        // margin balance map. They can be moved to spot with transfer_margin_to_spot.
        if position.collateral_ptokens > 0 {
            accrue_user_fee(&env, &user, &vaults.position_vault);
            let free = get_margin_balance_ptokens(&env, &user, &vaults.position_vault);
            set_margin_balance_ptokens(
                &env,
                &user,
                &vaults.position_vault,
                free.saturating_add(position.collateral_ptokens),
            );
            update_total_margin_ptokens(
                &env,
                &vaults.position_vault,
                position.collateral_ptokens,
                true,
            );
        }

        clear_position_storage(&env, position_id);
        remove_user_position(&env, &user, position_id);
    }

    /// Emergency V2 close that does not use oracle prices, DEX routing, or
    /// position-collateral swaps. The user repays the full margin debt from
    /// wallet with a fixed maximum payment, then all controller-held pToken
    /// collateral is returned to the user's margin balance.
    pub fn close_position_v2_repay_only(
        env: Env,
        user: Address,
        position_id: u64,
        max_repay_amount: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        let position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        let debt_vault_client = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        let repaid =
            debt_vault_client.repay_full_for_margin(&position_id, &user, &max_repay_amount);
        if repaid > 0 && debt_vault_client.get_margin_borrow_balance(&position_id) != 0 {
            panic!("debt remains");
        }

        if position.collateral_ptokens > 0 {
            accrue_user_fee(&env, &user, &vaults.position_vault);
            let free = get_margin_balance_ptokens(&env, &user, &vaults.position_vault);
            set_margin_balance_ptokens(
                &env,
                &user,
                &vaults.position_vault,
                free.saturating_add(position.collateral_ptokens),
            );
            update_total_margin_ptokens(
                &env,
                &vaults.position_vault,
                position.collateral_ptokens,
                true,
            );
        }
        if let Some((initial_market, initial_ptokens)) =
            get_position_initial_lock(&env, position_id)
        {
            if initial_ptokens > 0 {
                accrue_user_fee(&env, &user, &initial_market);
                let free = get_margin_balance_ptokens(&env, &user, &initial_market);
                set_margin_balance_ptokens(
                    &env,
                    &user,
                    &initial_market,
                    free.saturating_add(initial_ptokens),
                );
                update_total_margin_ptokens(&env, &initial_market, initial_ptokens, true);
            }
        }

        clear_position_storage(&env, position_id);
        remove_user_position(&env, &user, position_id);
    }

    /// Allocate free margin pTokens in the position asset to an open V3 position.
    ///
    /// Deposit and move the position asset into margin custody first with
    /// `deposit_collateral` and `transfer_spot_to_margin`. This accounting-only
    /// step avoids an oracle read, swap, or token transfer.
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
        bump_core_ttl(&env);
        user.require_auth();
        Self::swap_close_position_v3_impl(&env, user, position_id, amount_with_slippage);
    }

    pub fn finish_close_position_v3(env: Env, position_id: u64) {
        bump_core_ttl(&env);
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

    /// Legacy margin V1 is intentionally disabled. Use `open_position_no_swap_v2`.
    pub fn open_position_no_swap(
        env: Env,
        user: Address,
        collateral_asset: Address,
        debt_asset: Address,
        collateral_amount: u128,
        borrow_amount: u128,
        leverage: u128,
        side: PositionSide,
    ) -> u64 {
        let _ = (
            env,
            user,
            collateral_asset,
            debt_asset,
            collateral_amount,
            borrow_amount,
            leverage,
            side,
        );
        panic!("legacy margin disabled");
    }

    /// Legacy margin V1 is intentionally disabled. Use Margin V2 entrypoints.
    pub fn open_position_no_swap_short(
        env: Env,
        user: Address,
        collateral_asset: Address,
        debt_asset: Address,
        collateral_amount: u128,
        borrow_amount: u128,
        leverage: u128,
    ) -> u64 {
        let _ = (
            env,
            user,
            collateral_asset,
            debt_asset,
            collateral_amount,
            borrow_amount,
            leverage,
        );
        panic!("legacy margin disabled");
    }

    /// Legacy margin V1 is intentionally disabled. Use `close_position_v2`.
    pub fn close_position(
        env: Env,
        user: Address,
        position_id: u64,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        amount_with_slippage: u128,
    ) {
        let _ = (env, user, position_id, swaps_chain, amount_with_slippage);
        panic!("legacy margin disabled");
    }

    pub fn close_position_v2(
        env: Env,
        user: Address,
        position_id: u64,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        amount_with_slippage: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        let position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        let swap_adapter = get_swap_adapter(&env);
        validate_swaps_chain(
            &env,
            &swap_adapter,
            &swaps_chain,
            &position.collateral_asset,
            &position.debt_asset,
        );

        let debt_vault_client = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        debt_vault_client.update_interest();
        let debt_amount = debt_vault_client.get_margin_borrow_balance(&position_id);
        if debt_amount == 0 {
            panic!("zero debt");
        }

        let controller = env.current_contract_address();
        let debt_underlying =
            ReceiptVaultClient::new(&env, &vaults.debt_vault).get_underlying_token();
        if debt_underlying != position.debt_asset {
            panic!("debt asset mismatch");
        }
        let debt_token = token::TokenClient::new(&env, &debt_underlying);

        let underlying_token =
            ReceiptVaultClient::new(&env, &vaults.position_vault).get_underlying_token();
        if underlying_token != position.collateral_asset {
            panic!("collateral asset mismatch");
        }
        let token_client = token::TokenClient::new(&env, &underlying_token);
        let bal_before = token_client.balance(&controller);
        let vault_client = ReceiptVaultClient::new(&env, &vaults.position_vault);
        Self::begin_margin_withdraw_if_supported(
            &env,
            &vaults.position_vault,
            &controller,
            &controller,
            position.collateral_ptokens,
        );
        let withdraw_args: Vec<Val> =
            (controller.clone(), position.collateral_ptokens).into_val(&env);
        Self::authorize_controller_subcall(&env, &vaults.position_vault, "withdraw", withdraw_args);
        vault_client.withdraw(&controller, &position.collateral_ptokens);
        let bal_after = token_client.balance(&controller);
        let collateral_underlying = if bal_after <= bal_before {
            0u128
        } else {
            (bal_after - bal_before) as u128
        };
        let mut received = 0u128;
        if collateral_underlying > 0 {
            let min_out_oracle = Self::oracle_min_out(
                &env,
                &position.collateral_asset,
                &position.debt_asset,
                collateral_underlying,
            );
            if amount_with_slippage < min_out_oracle {
                panic!("slippage too high");
            }
            // Move withdrawn collateral back to the signer and execute the
            // swap as the signer. This avoids impossible controller auth for
            // router-internal token transfers.
            let collateral_i128: i128 = collateral_underlying.try_into().expect("amount too large");
            let collateral_transfer_args: Vec<Val> =
                (controller.clone(), user.clone(), collateral_i128).into_val(&env);
            Self::authorize_controller_subcall(
                &env,
                &underlying_token,
                "transfer",
                collateral_transfer_args,
            );
            token_client.transfer(&controller, &user, &collateral_i128);

            let debt_bal_before = debt_token.balance(&user);
            let _reported_received = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
                &user,
                &swaps_chain,
                &position.collateral_asset,
                &collateral_underlying,
                &amount_with_slippage,
            );
            let debt_bal_after = debt_token.balance(&user);
            received = if debt_bal_after <= debt_bal_before {
                0u128
            } else {
                (debt_bal_after - debt_bal_before) as u128
            };
            if received < min_out_oracle {
                panic!("slippage too high");
            }
        }
        let settlement_amount = received.max(debt_amount);
        let settlement_i128: i128 = settlement_amount.try_into().expect("amount too large");
        debt_token.transfer(&user, &controller, &settlement_i128);

        let repay_for_margin_args: Vec<Val> =
            (position_id, controller.clone(), debt_amount).into_val(&env);
        Self::authorize_controller_subcall(
            &env,
            &vaults.debt_vault,
            "repay_for_margin",
            repay_for_margin_args,
        );
        debt_vault_client.repay_for_margin(&position_id, &controller, &debt_amount);
        if debt_vault_client.get_margin_borrow_balance(&position_id) != 0 {
            panic!("debt remains");
        }

        let close_fee_bps: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::CloseFeeBps)
            .unwrap_or(0);
        let surplus = received.saturating_sub(debt_amount);
        if surplus > 0 {
            let debt_rate = debt_vault_client.get_exchange_rate();
            if debt_rate == 0 {
                panic!("invalid exchange rate");
            }
            let mut close_fee_underlying =
                surplus.checked_mul(close_fee_bps).expect("fee overflow") / BPS_SCALE;
            let mut user_surplus_underlying = surplus.saturating_sub(close_fee_underlying);
            if close_fee_underlying > 0
                && close_fee_underlying.saturating_mul(SCALE_1E6) / debt_rate == 0
            {
                user_surplus_underlying =
                    user_surplus_underlying.saturating_add(close_fee_underlying);
                close_fee_underlying = 0;
            }

            if user_surplus_underlying > 0 {
                let preview_ptokens = user_surplus_underlying.saturating_mul(SCALE_1E6) / debt_rate;
                if preview_ptokens == 0 {
                    let surplus_i128: i128 = user_surplus_underlying
                        .try_into()
                        .expect("amount too large");
                    let refund_args: Vec<Val> =
                        (controller.clone(), user.clone(), surplus_i128).into_val(&env);
                    Self::authorize_controller_subcall(
                        &env,
                        &debt_underlying,
                        "transfer",
                        refund_args,
                    );
                    debt_token.transfer(&controller, &user, &surplus_i128);
                } else {
                    let p_before = debt_vault_client.get_ptoken_balance(&controller);
                    let deposit_args: Vec<Val> =
                        (controller.clone(), user_surplus_underlying).into_val(&env);
                    Self::authorize_controller_subcall(
                        &env,
                        &vaults.debt_vault,
                        "deposit",
                        deposit_args,
                    );
                    debt_vault_client.deposit(&controller, &user_surplus_underlying);
                    let p_after = debt_vault_client.get_ptoken_balance(&controller);
                    let p_delta = p_after.saturating_sub(p_before);
                    if p_delta > 0 {
                        // Accrue pending fees with OLD balance before increasing it.
                        accrue_user_fee(&env, &user, &vaults.debt_vault);
                        let free = get_margin_balance_ptokens(&env, &user, &vaults.debt_vault);
                        set_margin_balance_ptokens(
                            &env,
                            &user,
                            &vaults.debt_vault,
                            free.saturating_add(p_delta),
                        );
                        update_total_margin_ptokens(&env, &vaults.debt_vault, p_delta, true);
                    }
                }
            }
            if close_fee_underlying > 0 {
                let p_before = debt_vault_client.get_ptoken_balance(&controller);
                let deposit_args: Vec<Val> =
                    (controller.clone(), close_fee_underlying).into_val(&env);
                Self::authorize_controller_subcall(
                    &env,
                    &vaults.debt_vault,
                    "deposit",
                    deposit_args,
                );
                debt_vault_client.deposit(&controller, &close_fee_underlying);
                let p_after = debt_vault_client.get_ptoken_balance(&controller);
                let fee_ptokens = p_after.saturating_sub(p_before);
                collect_margin_fee(&env, &vaults.debt_vault, fee_ptokens);
            }
        }
        if let Some((initial_market, initial_ptokens)) =
            get_position_initial_lock(&env, position_id)
        {
            if initial_ptokens > 0 {
                accrue_user_fee(&env, &user, &initial_market);
                let free = get_margin_balance_ptokens(&env, &user, &initial_market);
                set_margin_balance_ptokens(
                    &env,
                    &user,
                    &initial_market,
                    free.saturating_add(initial_ptokens),
                );
                update_total_margin_ptokens(&env, &initial_market, initial_ptokens, true);
            }
        }

        clear_position_storage(&env, position_id);
        remove_user_position(&env, &user, position_id);
    }

    /// Legacy margin V1 is intentionally disabled. Use `liquidate_position_v2`.
    pub fn liquidate_position(env: Env, liquidator: Address, position_id: u64) {
        let _ = (env, liquidator, position_id);
        panic!("legacy margin disabled");
    }

    pub fn liquidate_position_v3(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        Self::liquidate_position_v3_impl(&env, liquidator, position_id);
    }

    pub fn begin_liquidation_v3(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        Self::begin_liquidation_v3_impl(&env, liquidator, position_id);
    }

    pub fn swap_liquidation_v3(
        env: Env,
        liquidator: Address,
        position_id: u64,
        amount_with_slippage: u128,
    ) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        Self::swap_liquidation_v3_impl(&env, liquidator, position_id, amount_with_slippage);
    }

    pub fn finish_liquidation_v3(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        Self::finish_liquidation_v3_impl(&env, liquidator, position_id);
    }

    pub fn begin_liquidation_v2(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        Self::begin_liquidation_v2_impl(&env, liquidator, position_id);
    }

    pub fn finish_liquidation_v2(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        Self::finish_liquidation_v2_impl(&env, liquidator, position_id);
    }

    fn begin_liquidation_v2_impl(env: &Env, liquidator: Address, position_id: u64) {
        if get_pending_liquidation(env, position_id).is_some() {
            panic!("liquidation pending");
        }
        let mut position = get_position_or_panic(env, position_id);
        if position.status != PositionStatus::Open && position.status != PositionStatus::PendingOpen
        {
            panic!("not open");
        }
        if get_position_mode(env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        if liquidator == position.owner {
            panic!("self liquidation");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        Self::assert_liquidation_allowed(env, &vaults.debt_vault, &vaults.position_vault);
        let debt_vault = ReceiptVaultClient::new(env, &vaults.debt_vault);
        debt_vault.update_interest();
        let debt_amount = debt_vault.get_margin_borrow_balance(&position_id);
        if debt_amount == 0 {
            panic!("zero debt");
        }

        let debt_price = get_price_usd(env, &position.debt_asset);
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let initial_lock = get_position_initial_lock(env, position_id);
        if let Some((initial_market, _)) = &initial_lock {
            Self::assert_liquidation_allowed(env, &vaults.debt_vault, initial_market);
        }
        let pos_ctx = Self::vault_val_ctx_with_update(
            env,
            &vaults.position_vault,
            vaults.position_vault != vaults.debt_vault,
            true,
        );
        let init_ctx = initial_lock.as_ref().map(|(market, _)| {
            Self::vault_val_ctx_with_update(
                env,
                market,
                *market != vaults.debt_vault && *market != vaults.position_vault,
                true,
            )
        });
        let peridottroller = get_peridottroller(env);
        let close_factor_scaled = peridottroller.get_close_factor_scaled();
        if close_factor_scaled == 0 || close_factor_scaled > SCALE_1E6 {
            panic!("invalid close factor");
        }
        let liquidation_incentive_scaled = peridottroller.get_liquidation_incentive_scaled();
        if liquidation_incentive_scaled < SCALE_1E6 {
            panic!("invalid liquidation incentive");
        }
        let liquidation_fee_scaled = peridottroller.get_liquidation_fee_scaled();
        if liquidation_fee_scaled > SCALE_1E6 {
            panic!("invalid liquidation fee");
        }
        let reserve_recipient = if liquidation_fee_scaled > 0 {
            Some(
                peridottroller
                    .get_reserve_recipient()
                    .expect("reserve recipient missing"),
            )
        } else {
            None
        };

        let mut collateral_value =
            Self::ctx_discounted_value(&pos_ctx, position.collateral_ptokens);
        if let (Some((_, initial_ptokens)), Some(ctx)) = (&initial_lock, &init_ctx) {
            collateral_value =
                collateral_value.saturating_add(Self::ctx_discounted_value(ctx, *initial_ptokens));
        }
        let debt_value =
            debt_amount.checked_mul(debt_price.0).expect("liq overflow") / debt_price.1;
        if collateral_value >= debt_value {
            panic!("not liquidatable");
        }
        if position.status == PositionStatus::PendingOpen {
            if let Some(mut pending) = get_pending_open_position(env, position_id) {
                if pending.open_fee_ptokens > 0 {
                    collect_margin_fee(env, &pending.collateral_vault, pending.open_fee_ptokens);
                    pending.open_fee_ptokens = 0;
                    set_pending_open_position(env, position_id, &pending);
                }
            }
        }

        let mut raw_collateral_value = Self::ctx_raw_value(&pos_ctx, position.collateral_ptokens);
        if let (Some((_, initial_ptokens)), Some(ctx)) = (&initial_lock, &init_ctx) {
            raw_collateral_value =
                raw_collateral_value.saturating_add(Self::ctx_raw_value(ctx, *initial_ptokens));
        }
        let has_collateral_ptokens = position.collateral_ptokens > 0
            || initial_lock
                .as_ref()
                .map(|(_, ptokens)| *ptokens > 0)
                .unwrap_or(false);
        if !has_collateral_ptokens {
            panic!("no collateral");
        }
        let mut close_factor_repay = debt_amount
            .checked_mul(close_factor_scaled)
            .expect("liq overflow")
            / SCALE_1E6;
        if close_factor_repay == 0 {
            if debt_amount <= 1 {
                close_factor_repay = 1;
            } else {
                panic!("repay too small");
            }
        }
        let max_repay_by_close_factor = close_factor_repay.min(debt_amount);
        let max_repay_value_by_collateral = raw_collateral_value
            .checked_mul(SCALE_1E6)
            .expect("liq overflow")
            / liquidation_incentive_scaled;
        let mut max_repay_by_collateral = max_repay_value_by_collateral
            .checked_mul(debt_price.1)
            .expect("liq overflow")
            / debt_price.0;
        if max_repay_by_collateral == 0 {
            max_repay_by_collateral = 1;
        }
        let repay_amount = max_repay_by_close_factor
            .min(max_repay_by_collateral)
            .min(debt_amount);

        debt_vault.repay_for_margin(&position_id, &liquidator, &repay_amount);
        let remaining_debt = debt_vault.get_margin_borrow_balance(&position_id);
        if remaining_debt >= debt_amount {
            panic!("repay failed");
        }

        let repaid_value = Self::ceil_div(
            repay_amount
                .checked_mul(debt_price.0)
                .expect("liq overflow"),
            debt_price.1,
        );
        if repaid_value == 0 {
            panic!("repay too small");
        }
        let mut remaining_seize_value = repaid_value
            .checked_mul(liquidation_incentive_scaled)
            .expect("liq overflow")
            / SCALE_1E6;
        let position_seize_ptokens = Self::ctx_ptokens_for_raw_value_ceil(
            &pos_ctx,
            position.collateral_ptokens,
            remaining_seize_value,
        );
        let mut total_seized_ptokens = 0u128;
        let position_fee_ptokens = if position_seize_ptokens > 0 {
            total_seized_ptokens = total_seized_ptokens.saturating_add(position_seize_ptokens);
            let seized_value = Self::ctx_raw_value(&pos_ctx, position_seize_ptokens);
            remaining_seize_value = remaining_seize_value.saturating_sub(seized_value);
            position_seize_ptokens
                .checked_mul(liquidation_fee_scaled)
                .expect("liq overflow")
                / SCALE_1E6
        } else {
            0u128
        };

        let mut initial_market = None::<Address>;
        let mut initial_seize_ptokens = 0u128;
        let mut initial_fee_ptokens = 0u128;
        if let (Some((market, initial_ptokens)), Some(ctx)) = (&initial_lock, &init_ctx) {
            initial_market = Some(market.clone());
            initial_seize_ptokens =
                Self::ctx_ptokens_for_raw_value_ceil(ctx, *initial_ptokens, remaining_seize_value);
            if initial_seize_ptokens > 0 {
                total_seized_ptokens = total_seized_ptokens.saturating_add(initial_seize_ptokens);
                initial_fee_ptokens = initial_seize_ptokens
                    .checked_mul(liquidation_fee_scaled)
                    .expect("liq overflow")
                    / SCALE_1E6;
            }
        }
        if total_seized_ptokens == 0 {
            panic!("seize too small");
        }

        position.status = PositionStatus::Liquidated;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        bump_position_ttl(env, position_id);
        let pending = PendingLiquidation {
            kind: PendingLiquidationKind::MarginV2,
            stage: PendingLiquidationStage::Repaid,
            owner: position.owner,
            liquidator,
            debt_amount,
            repay_amount,
            received_debt_asset: 0u128,
            position_seize_ptokens,
            position_fee_ptokens,
            initial_market,
            initial_seize_ptokens,
            initial_fee_ptokens,
            reserve_recipient,
            liquidation_incentive_scaled,
        };
        set_pending_liquidation(env, position_id, &pending);
    }

    fn finish_liquidation_v2_impl(env: &Env, liquidator: Address, position_id: u64) {
        let pending = get_pending_liquidation_or_panic(env, position_id);
        if pending.kind != PendingLiquidationKind::MarginV2
            || pending.stage != PendingLiquidationStage::Repaid
        {
            panic!("bad liquidation stage");
        }
        if pending.liquidator != liquidator {
            panic!("not liquidator");
        }
        let mut position = get_position_or_panic(env, position_id);
        if position.status != PositionStatus::Liquidated {
            panic!("not liquidating");
        }
        if get_position_mode(env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        if position.owner != pending.owner {
            panic!("pending owner mismatch");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        if position.collateral_ptokens < pending.position_seize_ptokens {
            panic!("pending collateral mismatch");
        }

        let reserve_recipient = pending.reserve_recipient.clone();
        if pending.position_fee_ptokens > 0 {
            let recipient = reserve_recipient
                .as_ref()
                .expect("reserve recipient missing")
                .clone();
            Self::transfer_controller_ptokens(
                env,
                &vaults.position_vault,
                &recipient,
                pending.position_fee_ptokens,
            );
        }
        let position_liquidator_ptokens = pending
            .position_seize_ptokens
            .saturating_sub(pending.position_fee_ptokens);
        Self::transfer_controller_ptokens(
            env,
            &vaults.position_vault,
            &liquidator,
            position_liquidator_ptokens,
        );
        position.collateral_ptokens = position
            .collateral_ptokens
            .saturating_sub(pending.position_seize_ptokens);

        let mut initial_remaining = None::<(Address, u128)>;
        if let Some(initial_market) = pending.initial_market.clone() {
            let (stored_initial_market, initial_ptokens) =
                get_position_initial_lock(env, position_id).expect("initial lock missing");
            if stored_initial_market != initial_market
                || initial_ptokens < pending.initial_seize_ptokens
            {
                panic!("pending initial mismatch");
            }
            if pending.initial_fee_ptokens > 0 {
                let recipient = reserve_recipient
                    .as_ref()
                    .expect("reserve recipient missing")
                    .clone();
                Self::transfer_controller_ptokens(
                    env,
                    &initial_market,
                    &recipient,
                    pending.initial_fee_ptokens,
                );
            }
            let initial_liquidator_ptokens = pending
                .initial_seize_ptokens
                .saturating_sub(pending.initial_fee_ptokens);
            Self::transfer_controller_ptokens(
                env,
                &initial_market,
                &liquidator,
                initial_liquidator_ptokens,
            );
            initial_remaining = Some((
                initial_market,
                initial_ptokens.saturating_sub(pending.initial_seize_ptokens),
            ));
        }

        let debt_vault = ReceiptVaultClient::new(env, &vaults.debt_vault);
        let remaining_debt = debt_vault.get_margin_borrow_balance(&position_id);
        if let Some((initial_market, remaining)) = initial_remaining.clone() {
            if remaining_debt == 0 && remaining > 0 {
                Self::credit_margin_ptokens(env, &position.owner, &initial_market, remaining);
                clear_position_initial_lock(env, position_id);
            } else if remaining_debt > 0 && remaining > 0 {
                set_position_initial_lock(env, position_id, &initial_market, remaining);
            } else {
                clear_position_initial_lock(env, position_id);
            }
        }

        let has_initial_collateral = get_position_initial_lock(env, position_id)
            .map(|(_, ptokens)| ptokens > 0)
            .unwrap_or(false);
        if remaining_debt > 0 {
            if position.collateral_ptokens == 0 && !has_initial_collateral {
                Self::authorize_controller_subcall(
                    env,
                    &vaults.debt_vault,
                    "absorb_margin_bad_debt",
                    (position_id,).into_val(env),
                );
                debt_vault.absorb_margin_bad_debt(&position_id);
                clear_position_storage(env, position_id);
                remove_user_position(env, &position.owner, position_id);
                return;
            }
            position.status = PositionStatus::Open;
            env.storage()
                .persistent()
                .set(&DataKey::Position(position_id), &position);
            bump_position_ttl(env, position_id);
            clear_pending_liquidation(env, position_id);
            return;
        }

        if position.collateral_ptokens > 0 {
            Self::credit_margin_ptokens(
                env,
                &position.owner,
                &vaults.position_vault,
                position.collateral_ptokens,
            );
        }
        clear_position_storage(env, position_id);
        remove_user_position(env, &position.owner, position_id);
    }

    pub fn liquidate_position_v2(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        let mut position = get_position_or_panic(&env, position_id);
        if get_position_mode(&env, position_id) == PositionMode::PerpsV3 {
            Self::liquidate_position_v3_impl(&env, liquidator, position_id);
            return;
        }
        if position.status != PositionStatus::Open && position.status != PositionStatus::PendingOpen
        {
            panic!("not open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        if liquidator == position.owner {
            panic!("self liquidation");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        Self::assert_liquidation_allowed(&env, &vaults.debt_vault, &vaults.position_vault);
        let debt_vault = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        debt_vault.update_interest();
        let debt_amount = debt_vault.get_margin_borrow_balance(&position_id);
        if debt_amount == 0 {
            panic!("zero debt");
        }

        let debt_price = get_price_usd(&env, &position.debt_asset);
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        // Fetch each collateral vault's pricing inputs once and reuse them for all
        // valuation math below. Re-reading via the cross-contract helpers (each
        // get_price_usd hits the oracle) ~5x per liquidation exceeds the CPU budget.
        let initial_lock = get_position_initial_lock(&env, position_id);
        if let Some((initial_market, _)) = &initial_lock {
            Self::assert_liquidation_allowed(&env, &vaults.debt_vault, initial_market);
        }
        let pos_ctx = Self::vault_val_ctx_with_update(
            &env,
            &vaults.position_vault,
            vaults.position_vault != vaults.debt_vault,
            true,
        );
        let init_ctx = initial_lock.as_ref().map(|(market, _)| {
            Self::vault_val_ctx_with_update(
                &env,
                market,
                *market != vaults.debt_vault && *market != vaults.position_vault,
                true,
            )
        });
        let peridottroller = get_peridottroller(&env);
        let close_factor_scaled = peridottroller.get_close_factor_scaled();
        if close_factor_scaled == 0 || close_factor_scaled > SCALE_1E6 {
            panic!("invalid close factor");
        }
        let liquidation_incentive_scaled = peridottroller.get_liquidation_incentive_scaled();
        if liquidation_incentive_scaled < SCALE_1E6 {
            panic!("invalid liquidation incentive");
        }
        let liquidation_fee_scaled = peridottroller.get_liquidation_fee_scaled();
        if liquidation_fee_scaled > SCALE_1E6 {
            panic!("invalid liquidation fee");
        }
        let reserve_recipient = if liquidation_fee_scaled > 0 {
            Some(
                peridottroller
                    .get_reserve_recipient()
                    .expect("reserve recipient missing"),
            )
        } else {
            None
        };

        let mut collateral_value =
            Self::ctx_discounted_value(&pos_ctx, position.collateral_ptokens);
        if let (Some((_, initial_ptokens)), Some(ctx)) = (&initial_lock, &init_ctx) {
            collateral_value =
                collateral_value.saturating_add(Self::ctx_discounted_value(ctx, *initial_ptokens));
        }
        // Checked math for the liquidation value/seize computation: saturating to
        // u128::MAX on overflow could understate debt or over-inflate the seize.
        // Liquidatability gate is borrower-safe: collateral and debt values
        // both floor their final division. Ceil is reserved for post-gate
        // repay/seize sizing where dust conservatism is intentional.
        let debt_value =
            debt_amount.checked_mul(debt_price.0).expect("liq overflow") / debt_price.1;
        if collateral_value >= debt_value {
            panic!("not liquidatable");
        }
        if position.status == PositionStatus::PendingOpen {
            if let Some(mut pending) = get_pending_open_position(&env, position_id) {
                if pending.open_fee_ptokens > 0 {
                    collect_margin_fee(&env, &pending.collateral_vault, pending.open_fee_ptokens);
                    pending.open_fee_ptokens = 0;
                    set_pending_open_position(&env, position_id, &pending);
                }
            }
        }

        let mut raw_collateral_value = Self::ctx_raw_value(&pos_ctx, position.collateral_ptokens);
        if let (Some((_, initial_ptokens)), Some(ctx)) = (&initial_lock, &init_ctx) {
            raw_collateral_value =
                raw_collateral_value.saturating_add(Self::ctx_raw_value(ctx, *initial_ptokens));
        }
        let has_collateral_ptokens = position.collateral_ptokens > 0
            || initial_lock
                .as_ref()
                .map(|(_, ptokens)| *ptokens > 0)
                .unwrap_or(false);
        if !has_collateral_ptokens {
            panic!("no collateral");
        }
        let mut close_factor_repay = debt_amount
            .checked_mul(close_factor_scaled)
            .expect("liq overflow")
            / SCALE_1E6;
        if close_factor_repay == 0 {
            if debt_amount <= 1 {
                // True dust debt must still be liquidatable under fractional
                // close factors; otherwise a 1-unit debt is permanently stuck.
                close_factor_repay = 1;
            } else {
                panic!("repay too small");
            }
        }
        let max_repay_by_close_factor = close_factor_repay.min(debt_amount);
        let max_repay_value_by_collateral = raw_collateral_value
            .checked_mul(SCALE_1E6)
            .expect("liq overflow")
            / liquidation_incentive_scaled;
        let mut max_repay_by_collateral = max_repay_value_by_collateral
            .checked_mul(debt_price.1)
            .expect("liq overflow")
            / debt_price.0;
        if max_repay_by_collateral == 0 {
            // If there is any pToken collateral, allow a 1-unit repay and seize
            // the dust collateral instead of making the position unliquidatable.
            max_repay_by_collateral = 1;
        }
        let repay_amount = max_repay_by_close_factor
            .min(max_repay_by_collateral)
            .min(debt_amount);

        debt_vault.repay_for_margin(&position_id, &liquidator, &repay_amount);
        let remaining_debt = debt_vault.get_margin_borrow_balance(&position_id);
        if remaining_debt >= debt_amount {
            panic!("repay failed");
        }

        let repaid_value = Self::ceil_div(
            repay_amount
                .checked_mul(debt_price.0)
                .expect("liq overflow"),
            debt_price.1,
        );
        if repaid_value == 0 {
            panic!("repay too small");
        }
        let mut remaining_seize_value = repaid_value
            .checked_mul(liquidation_incentive_scaled)
            .expect("liq overflow")
            / SCALE_1E6;
        let seize_ptokens = Self::ctx_ptokens_for_raw_value_ceil(
            &pos_ctx,
            position.collateral_ptokens,
            remaining_seize_value,
        );
        let mut total_seized_ptokens = 0u128;
        if seize_ptokens > 0 {
            total_seized_ptokens = total_seized_ptokens.saturating_add(seize_ptokens);
            let controller = env.current_contract_address();
            let fee_ptokens = seize_ptokens
                .checked_mul(liquidation_fee_scaled)
                .expect("liq overflow")
                / SCALE_1E6;
            let liquidator_ptokens = seize_ptokens.saturating_sub(fee_ptokens);
            if fee_ptokens > 0 {
                let recipient = reserve_recipient
                    .as_ref()
                    .expect("reserve recipient missing")
                    .clone();
                let fee_i128: i128 = fee_ptokens.try_into().expect("amount too large");
                Self::begin_margin_withdraw_if_supported(
                    &env,
                    &vaults.position_vault,
                    &controller,
                    &recipient,
                    fee_ptokens,
                );
                let transfer_args: Vec<Val> =
                    (controller.clone(), recipient.clone(), fee_i128).into_val(&env);
                Self::authorize_controller_subcall(
                    &env,
                    &vaults.position_vault,
                    "transfer",
                    transfer_args,
                );
                ReceiptVaultClient::new(&env, &vaults.position_vault).transfer(
                    &controller,
                    &recipient,
                    &fee_i128,
                );
            }
            if liquidator_ptokens > 0 {
                let seize_i128: i128 = liquidator_ptokens.try_into().expect("amount too large");
                // Arm the vault's margin bypass so the seize transfer out of controller
                // custody skips enforce_margin_lock, which would otherwise call back into
                // locked_ptokens_in_market on this controller mid-liquidation (re-entry trap).
                Self::begin_margin_withdraw_if_supported(
                    &env,
                    &vaults.position_vault,
                    &controller,
                    &liquidator,
                    liquidator_ptokens,
                );
                let transfer_args: Vec<Val> =
                    (controller.clone(), liquidator.clone(), seize_i128).into_val(&env);
                Self::authorize_controller_subcall(
                    &env,
                    &vaults.position_vault,
                    "transfer",
                    transfer_args,
                );
                ReceiptVaultClient::new(&env, &vaults.position_vault).transfer(
                    &controller,
                    &liquidator,
                    &seize_i128,
                );
            }
            position.collateral_ptokens = position.collateral_ptokens.saturating_sub(seize_ptokens);
            let seized_value = Self::ctx_raw_value(&pos_ctx, seize_ptokens);
            remaining_seize_value = remaining_seize_value.saturating_sub(seized_value);
        }

        if let (Some((initial_market, initial_ptokens)), Some(ctx)) =
            (initial_lock.clone(), init_ctx.as_ref())
        {
            let initial_seize_ptokens =
                Self::ctx_ptokens_for_raw_value_ceil(ctx, initial_ptokens, remaining_seize_value);
            if initial_seize_ptokens > 0 {
                total_seized_ptokens = total_seized_ptokens.saturating_add(initial_seize_ptokens);
                let controller = env.current_contract_address();
                let fee_ptokens = initial_seize_ptokens
                    .checked_mul(liquidation_fee_scaled)
                    .expect("liq overflow")
                    / SCALE_1E6;
                let liquidator_ptokens = initial_seize_ptokens.saturating_sub(fee_ptokens);
                if fee_ptokens > 0 {
                    let recipient = reserve_recipient
                        .as_ref()
                        .expect("reserve recipient missing")
                        .clone();
                    let fee_i128: i128 = fee_ptokens.try_into().expect("amount too large");
                    Self::begin_margin_withdraw_if_supported(
                        &env,
                        &initial_market,
                        &controller,
                        &recipient,
                        fee_ptokens,
                    );
                    let transfer_args: Vec<Val> =
                        (controller.clone(), recipient.clone(), fee_i128).into_val(&env);
                    Self::authorize_controller_subcall(
                        &env,
                        &initial_market,
                        "transfer",
                        transfer_args,
                    );
                    ReceiptVaultClient::new(&env, &initial_market).transfer(
                        &controller,
                        &recipient,
                        &fee_i128,
                    );
                }
                if liquidator_ptokens > 0 {
                    let amt_i128: i128 = liquidator_ptokens.try_into().expect("amount too large");
                    // Same bypass arming as the position-collateral seize above, to avoid
                    // the re-entry trap on the initial-lock collateral transfer.
                    Self::begin_margin_withdraw_if_supported(
                        &env,
                        &initial_market,
                        &controller,
                        &liquidator,
                        liquidator_ptokens,
                    );
                    let transfer_args: Vec<Val> =
                        (controller.clone(), liquidator.clone(), amt_i128).into_val(&env);
                    Self::authorize_controller_subcall(
                        &env,
                        &initial_market,
                        "transfer",
                        transfer_args,
                    );
                    ReceiptVaultClient::new(&env, &initial_market).transfer(
                        &controller,
                        &liquidator,
                        &amt_i128,
                    );
                }
            }
            let initial_remaining = initial_ptokens.saturating_sub(initial_seize_ptokens);
            if remaining_debt == 0 && initial_remaining > 0 {
                accrue_user_fee(&env, &position.owner, &initial_market);
                let free = get_margin_balance_ptokens(&env, &position.owner, &initial_market);
                set_margin_balance_ptokens(
                    &env,
                    &position.owner,
                    &initial_market,
                    free.saturating_add(initial_remaining),
                );
                update_total_margin_ptokens(&env, &initial_market, initial_remaining, true);
            }
            if remaining_debt > 0 && initial_remaining > 0 {
                set_position_initial_lock(&env, position_id, &initial_market, initial_remaining);
            } else {
                clear_position_initial_lock(&env, position_id);
            }
        }

        if total_seized_ptokens == 0 {
            panic!("seize too small");
        }

        if remaining_debt > 0 {
            let has_initial_collateral = get_position_initial_lock(&env, position_id)
                .map(|(_, ptokens)| ptokens > 0)
                .unwrap_or(false);
            if position.collateral_ptokens == 0 && !has_initial_collateral {
                let absorb_args: Vec<Val> = (position_id,).into_val(&env);
                Self::authorize_controller_subcall(
                    &env,
                    &vaults.debt_vault,
                    "absorb_margin_bad_debt",
                    absorb_args,
                );
                debt_vault.absorb_margin_bad_debt(&position_id);
                clear_position_storage(&env, position_id);
                remove_user_position(&env, &position.owner, position_id);
                return;
            }
            env.storage()
                .persistent()
                .set(&DataKey::Position(position_id), &position);
            bump_position_ttl(&env, position_id);
            return;
        }

        if position.collateral_ptokens > 0 {
            accrue_user_fee(&env, &position.owner, &vaults.position_vault);
            let free = get_margin_balance_ptokens(&env, &position.owner, &vaults.position_vault);
            set_margin_balance_ptokens(
                &env,
                &position.owner,
                &vaults.position_vault,
                free.saturating_add(position.collateral_ptokens),
            );
            update_total_margin_ptokens(
                &env,
                &vaults.position_vault,
                position.collateral_ptokens,
                true,
            );
        }
        clear_position_storage(&env, position_id);
        remove_user_position(&env, &position.owner, position_id);
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
            let mode = get_position_mode_no_bump(&env, position_id);
            if mode == PositionMode::MarginV2 {
                continue;
            }

            let vaults = get_position_vaults_no_bump(&env, position_id, &position);
            if vaults.position_vault == market {
                total_locked = total_locked.saturating_add(position.collateral_ptokens);
            }

            if let Some((initial_market, initial_ptokens)) =
                get_position_initial_lock_no_bump(&env, position_id)
            {
                if initial_market == market {
                    total_locked = total_locked.saturating_add(initial_ptokens);
                }
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
        if position
            .as_ref()
            .map(|value| value.status == PositionStatus::Closing)
            .unwrap_or(false)
        {
            bump_pending_perps_close_ttl(&env, position_id);
        }
        position
    }

    /// ReceiptVault defense-in-depth callback.
    /// Returns the owner for an active MarginV2 position and validates debt-vault binding.
    pub fn get_margin_position_owner(env: Env, position_id: u64, debt_vault: Address) -> Address {
        bump_core_ttl(&env);
        let position = get_position_or_panic(&env, position_id);
        if position.status != PositionStatus::Open && position.status != PositionStatus::PendingOpen
        {
            panic!("position not open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        if vaults.debt_vault != debt_vault {
            panic!("wrong debt vault");
        }
        position.owner
    }

    pub fn get_user_positions(env: Env, user: Address) -> Vec<u64> {
        bump_core_ttl(&env);
        read_user_positions(&env, &user)
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
        let vaults = get_position_vaults(&env, position_id, &position);
        let mode = get_position_mode(&env, position_id);
        if mode == PositionMode::PerpsV3 {
            return Self::get_perps_health_factor(&env, position_id, &position);
        }
        let debt_amount = if mode == PositionMode::MarginV2 {
            let debt_vault = ReceiptVaultClient::new(&env, &vaults.debt_vault);
            debt_vault.update_interest();
            debt_vault.get_margin_borrow_balance(&position_id)
        } else {
            let (debt, _total_shares, _total_debt) = debt_for_shares_in_vault(
                &env,
                &position.owner,
                &position.debt_asset,
                &vaults.debt_vault,
                position.debt_shares,
            );
            debt
        };
        if debt_amount == 0 {
            return u128::MAX;
        }
        let debt_price = get_price_usd(&env, &position.debt_asset);
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let debt_value = debt_amount
            .checked_mul(debt_price.0)
            .expect("health overflow")
            / debt_price.1;
        if debt_value == 0 {
            panic!("debt value too small");
        }
        let mut collateral_value = Self::discounted_ptoken_value_usd_with_update(
            &env,
            &vaults.position_vault,
            position.collateral_ptokens,
            mode != PositionMode::MarginV2 || vaults.position_vault != vaults.debt_vault,
            true,
        );
        if mode == PositionMode::MarginV2 {
            if let Some((initial_market, initial_ptokens)) =
                get_position_initial_lock(&env, position_id)
            {
                collateral_value = collateral_value
                    .checked_add(Self::discounted_ptoken_value_usd_with_update(
                        &env,
                        &initial_market,
                        initial_ptokens,
                        initial_market != vaults.debt_vault
                            && initial_market != vaults.position_vault,
                        true,
                    ))
                    .expect("health overflow");
            }
        }
        if collateral_value == 0 {
            return 0;
        }
        collateral_value
            .checked_mul(SCALE_1E6)
            .expect("health overflow")
            / debt_value
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
        if in_price.0 == 0 || in_price.1 == 0 || out_price.0 == 0 || out_price.1 == 0 {
            panic!("invalid price");
        }
        let max_slippage_scaled = get_max_slippage_scaled(env);
        let mut n_amount = amount_in;
        let mut n_in_price = in_price.0;
        let mut n_out_scale = out_price.1;
        let mut n_slippage = SCALE_1E6.saturating_sub(max_slippage_scaled);
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

    pub(crate) fn transfer_controller_ptokens(
        env: &Env,
        vault: &Address,
        recipient: &Address,
        ptokens: u128,
    ) {
        if ptokens == 0 {
            return;
        }
        let controller = env.current_contract_address();
        let amount_i128: i128 = ptokens.try_into().expect("amount too large");
        Self::begin_margin_withdraw_if_supported(env, vault, &controller, recipient, ptokens);
        let transfer_args: Vec<Val> =
            (controller.clone(), recipient.clone(), amount_i128).into_val(env);
        Self::authorize_controller_subcall(env, vault, "transfer", transfer_args);
        ReceiptVaultClient::new(env, vault).transfer(&controller, recipient, &amount_i128);
    }

    pub(crate) fn credit_margin_ptokens(env: &Env, user: &Address, vault: &Address, ptokens: u128) {
        if ptokens == 0 {
            return;
        }
        accrue_user_fee(env, user, vault);
        let free = get_margin_balance_ptokens(env, user, vault);
        set_margin_balance_ptokens(env, user, vault, free.saturating_add(ptokens));
        update_total_margin_ptokens(env, vault, ptokens, true);
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

    /// Fetch a vault's pricing inputs once (underlying asset, oracle price,
    /// exchange rate, collateral factor) for reuse across valuation math.
    fn vault_val_ctx_with_update(
        env: &Env,
        vault: &Address,
        update_interest: bool,
        refresh_price: bool,
    ) -> VaultValCtx {
        let vault_client = ReceiptVaultClient::new(env, vault);
        let asset = vault_client.get_underlying_token();
        let price = if refresh_price {
            get_price_usd(env, &asset)
        } else {
            get_price_usd_cache_first(env, &asset)
        };
        if price.0 == 0 || price.1 == 0 {
            panic!("invalid collateral price");
        }
        if update_interest {
            vault_client.update_interest();
        }
        let rate = vault_client.get_exchange_rate();
        if rate == 0 {
            panic!("invalid exchange rate");
        }
        let cf = get_peridottroller(env).get_market_cf(vault).min(SCALE_1E6);
        VaultValCtx {
            price_num: price.0,
            price_den: price.1,
            rate,
            cf,
        }
    }

    /// Raw (undiscounted) USD value of pTokens using a cached context.
    /// Checked math: overflow fails closed rather than saturating to u128::MAX,
    /// which in the liquidation seize math could over-inflate the seized amount.
    fn ctx_raw_value(ctx: &VaultValCtx, ptokens: u128) -> u128 {
        if ptokens == 0 {
            return 0;
        }
        let underlying = ptokens.checked_mul(ctx.rate).expect("liq overflow") / SCALE_1E6;
        underlying.checked_mul(ctx.price_num).expect("liq overflow") / ctx.price_den
    }

    /// Collateral-factor-discounted USD value of pTokens using a cached context.
    fn ctx_discounted_value(ctx: &VaultValCtx, ptokens: u128) -> u128 {
        Self::ctx_raw_value(ctx, ptokens)
            .checked_mul(ctx.cf)
            .expect("liq overflow")
            / SCALE_1E6
    }

    /// pTokens needed to cover `target_raw_value`, rounded up, using a cached context.
    fn ctx_ptokens_for_raw_value_ceil(
        ctx: &VaultValCtx,
        available_ptokens: u128,
        target_raw_value: u128,
    ) -> u128 {
        if target_raw_value == 0 || available_ptokens == 0 {
            return 0;
        }
        let available_value = Self::ctx_raw_value(ctx, available_ptokens);
        if target_raw_value >= available_value {
            return available_ptokens;
        }
        let min_unit_value = Self::ctx_raw_value(ctx, 1);
        if min_unit_value > 0 && target_raw_value < min_unit_value {
            // Dust liquidation target: seize one smallest pToken unit rather
            // than reverting and leaving an unhealthy position stuck forever.
            return 1u128.min(available_ptokens);
        }
        let underlying = Self::ceil_div(
            target_raw_value
                .checked_mul(ctx.price_den)
                .expect("liq overflow"),
            ctx.price_num,
        );
        let ptokens = Self::ceil_div(
            underlying.checked_mul(SCALE_1E6).expect("liq overflow"),
            ctx.rate,
        );
        ptokens.min(available_ptokens)
    }

    fn discounted_ptoken_value_usd(env: &Env, vault: &Address, ptokens: u128) -> u128 {
        Self::discounted_ptoken_value_usd_with_update(env, vault, ptokens, true, true)
    }

    fn assert_pending_open_health(
        env: &Env,
        position_id: u64,
        debt_asset: &Address,
        position_vault: &Address,
        collateral_ptokens: u128,
        debt_amount: u128,
    ) {
        let debt_price = refresh_price_usd(env, debt_asset);
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let debt_value = debt_amount
            .checked_mul(debt_price.0)
            .expect("valuation overflow")
            / debt_price.1;
        let combined_collateral_value = if let Some((initial_market, initial_ptokens)) =
            get_position_initial_lock(env, position_id)
        {
            if initial_market == *position_vault {
                Self::discounted_ptoken_value_usd_refreshed(
                    env,
                    position_vault,
                    collateral_ptokens.saturating_add(initial_ptokens),
                    false,
                )
            } else {
                Self::discounted_ptoken_value_usd_refreshed(
                    env,
                    position_vault,
                    collateral_ptokens,
                    false,
                )
                .checked_add(Self::discounted_ptoken_value_usd_refreshed(
                    env,
                    &initial_market,
                    initial_ptokens,
                    false,
                ))
                .expect("valuation overflow")
            }
        } else {
            Self::discounted_ptoken_value_usd_refreshed(
                env,
                position_vault,
                collateral_ptokens,
                false,
            )
        };
        let min_open_collateral_value = debt_value
            .checked_mul(DEFAULT_MARGIN_MIN_OPEN_HF_SCALED)
            .expect("valuation overflow")
            / SCALE_1E6;
        if combined_collateral_value < min_open_collateral_value {
            panic!("insufficient collateral");
        }
    }

    fn discounted_ptoken_value_usd_refreshed(
        env: &Env,
        vault: &Address,
        ptokens: u128,
        update_interest: bool,
    ) -> u128 {
        if ptokens == 0 {
            return 0;
        }
        let vault_client = ReceiptVaultClient::new(env, vault);
        let asset = vault_client.get_underlying_token();
        let price = refresh_price_usd(env, &asset);
        if price.0 == 0 || price.1 == 0 {
            panic!("invalid collateral price");
        }
        let cf = get_peridottroller(env).get_market_cf(vault).min(SCALE_1E6);
        if update_interest {
            vault_client.update_interest();
        }
        let exchange_rate = vault_client.get_exchange_rate();
        if exchange_rate == 0 {
            panic!("invalid exchange rate");
        }
        let underlying = ptokens
            .checked_mul(exchange_rate)
            .expect("valuation overflow")
            / SCALE_1E6;
        let raw_value = underlying.checked_mul(price.0).expect("valuation overflow") / price.1;
        raw_value.checked_mul(cf).expect("valuation overflow") / SCALE_1E6
    }

    fn finalize_pending_open_collateral(
        env: &Env,
        position: &mut Position,
        position_id: u64,
        pending: &PendingOpenPosition,
        vaults: &PositionVaults,
        collateral_ptokens: u128,
        position_amount: u128,
        debt_amount: u128,
    ) {
        Self::assert_pending_open_health(
            env,
            position_id,
            &position.debt_asset,
            &vaults.position_vault,
            collateral_ptokens,
            debt_amount,
        );
        position.collateral_ptokens = collateral_ptokens;
        position.entry_price_scaled = Self::entry_price_scaled(debt_amount, position_amount);
        position.status = PositionStatus::Open;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), position);
        clear_pending_open_position(env, position_id);
        clear_pending_open_supplied_collateral(env, position_id);
        bump_position_ttl(env, position_id);
        collect_margin_fee(env, &vaults.collateral_vault, pending.open_fee_ptokens);
    }

    fn release_pending_open_supplied_collateral(
        env: &Env,
        user: &Address,
        position_id: u64,
        vaults: &PositionVaults,
    ) {
        let Some((position_ptokens, _position_amount)) =
            get_pending_open_supplied_collateral(env, position_id)
        else {
            return;
        };
        if position_ptokens > 0 {
            accrue_user_fee(env, user, &vaults.position_vault);
            let free = get_margin_balance_ptokens(env, user, &vaults.position_vault);
            set_margin_balance_ptokens(
                env,
                user,
                &vaults.position_vault,
                free.saturating_add(position_ptokens),
            );
            update_total_margin_ptokens(env, &vaults.position_vault, position_ptokens, true);
        }
        clear_pending_open_supplied_collateral(env, position_id);
    }

    fn release_pending_open_lock(
        env: &Env,
        user: &Address,
        position_id: u64,
        open_fee_ptokens: u128,
    ) {
        if let Some((initial_market, initial_ptokens)) = get_position_initial_lock(env, position_id)
        {
            let release_ptokens = initial_ptokens.saturating_add(open_fee_ptokens);
            if release_ptokens > 0 {
                accrue_user_fee(env, user, &initial_market);
                let free = get_margin_balance_ptokens(env, user, &initial_market);
                set_margin_balance_ptokens(
                    env,
                    user,
                    &initial_market,
                    free.saturating_add(release_ptokens),
                );
                update_total_margin_ptokens(env, &initial_market, release_ptokens, true);
            }
        }
    }

    fn discounted_ptoken_value_usd_with_update(
        env: &Env,
        vault: &Address,
        ptokens: u128,
        update_interest: bool,
        refresh_price: bool,
    ) -> u128 {
        if ptokens == 0 {
            return 0;
        }
        let vault_client = ReceiptVaultClient::new(env, vault);
        let asset = vault_client.get_underlying_token();
        let price = if refresh_price {
            get_price_usd(env, &asset)
        } else {
            get_price_usd_cache_first(env, &asset)
        };
        if price.0 == 0 || price.1 == 0 {
            panic!("invalid collateral price");
        }
        let cf = get_peridottroller(env).get_market_cf(vault).min(SCALE_1E6);
        if update_interest {
            vault_client.update_interest();
        }
        let exchange_rate = vault_client.get_exchange_rate();
        if exchange_rate == 0 {
            panic!("invalid exchange rate");
        }
        let underlying = ptokens
            .checked_mul(exchange_rate)
            .expect("valuation overflow")
            / SCALE_1E6;
        let raw_value = underlying.checked_mul(price.0).expect("valuation overflow") / price.1;
        raw_value.checked_mul(cf).expect("valuation overflow") / SCALE_1E6
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

    fn ceil_div(numerator: u128, denominator: u128) -> u128 {
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

    fn assert_liquidation_allowed(env: &Env, repay_vault: &Address, collateral_vault: &Address) {
        let peridottroller = get_peridottroller(env);
        if !peridottroller.is_market_supported(repay_vault)
            || !peridottroller.is_market_supported(collateral_vault)
        {
            panic!("market not supported");
        }
        if peridottroller.is_liquidation_paused(repay_vault)
            || peridottroller.is_liquidation_paused(collateral_vault)
        {
            panic!("liquidation paused");
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
