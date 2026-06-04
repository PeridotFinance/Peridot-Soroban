use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
#[cfg(not(test))]
use soroban_sdk::String;
use soroban_sdk::{
    contract, contractimpl, token, Address, BytesN, Env, IntoVal, InvokeError, Symbol, Val, Vec,
};

use crate::constants::*;
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
            .set(&DataKey::MaxSlippageBps, &DEFAULT_MAX_SLIPPAGE_BPS);
        env.storage()
            .persistent()
            .set(&DataKey::PositionCounter, &0u64);
        env.storage().persistent().set(&DataKey::Initialized, &true);
        bump_core_ttl(&env);
    }

    pub fn set_market(env: Env, admin: Address, asset: Address, vault: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::Market(asset.clone()), &vault);
        bump_market_ttl(&env, &asset);
    }

    pub fn set_peridottroller(env: Env, admin: Address, peridottroller: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::Peridottroller, &peridottroller);
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

    pub fn set_max_slippage_bps(env: Env, admin: Address, max_slippage_bps: u128) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if max_slippage_bps == 0 || max_slippage_bps > MAX_SLIPPAGE_BPS_CAP {
            panic!("invalid slippage");
        }
        env.storage()
            .persistent()
            .set(&DataKey::MaxSlippageBps, &max_slippage_bps);
    }

    pub fn set_swap_adapter(env: Env, admin: Address, swap_adapter: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        Self::assert_valid_swap_adapter(&env, &swap_adapter);
        env.storage()
            .persistent()
            .set(&DataKey::SwapAdapter, &swap_adapter);
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
        let accrued: u128 = env
            .storage()
            .persistent()
            .get(&accrued_key)
            .unwrap_or(0);
        if accrued == 0 {
            return 0;
        }
        // Move accrued pTokens into user's free margin balance.
        // (accrue_user_fee above already advanced UserMarginFeeIndex to current fee_index,
        //  so this balance increase won't cause back-pay on the next interaction.)
        let free = get_margin_balance_ptokens(&env, &user, &vault);
        set_margin_balance_ptokens(&env, &user, &vault, free.saturating_add(accrued));
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
            delta.saturating_mul(user_bal) / MARGIN_FEE_PRECISION
        } else {
            0
        };
        let accrued_key = DataKey::UserMarginFeeAccrued(user.clone(), vault.clone());
        let accrued: u128 = env
            .storage()
            .persistent()
            .get(&accrued_key)
            .unwrap_or(0);
        accrued.saturating_add(pending)
    }

    pub fn get_margin_fee_index(env: Env, asset: Address) -> u128 {
        bump_core_ttl(&env);
        let vault = get_market(&env, &asset);
        get_margin_fee_index(&env, &vault)
    }

    /// Admin function: move orphaned fee pTokens (collected when pool was empty)
    /// into `recipient`'s free margin balance. Returns the amount swept.
    pub fn sweep_orphan_fees(
        env: Env,
        admin: Address,
        asset: Address,
        recipient: Address,
    ) -> u128 {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        let vault = get_market(&env, &asset);
        let orphan_key = DataKey::MarginFeeOrphan(vault.clone());
        let orphan: u128 = env
            .storage()
            .persistent()
            .get(&orphan_key)
            .unwrap_or(0);
        if orphan == 0 {
            return 0;
        }
        accrue_user_fee(&env, &recipient, &vault);
        let free = get_margin_balance_ptokens(&env, &recipient, &vault);
        set_margin_balance_ptokens(&env, &recipient, &vault, free.saturating_add(orphan));
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
            .saturating_mul(leverage)
            .saturating_mul(open_fee_bps)
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
        let debt_price = get_price_usd(&env, &debt_asset);
        if coll_price.0 == 0 || coll_price.1 == 0 {
            panic!("invalid collateral price");
        }
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let coll_rate = ReceiptVaultClient::new(&env, &collateral_vault).get_exchange_rate();
        let collateral_underlying = collateral_ptokens.saturating_mul(coll_rate) / SCALE_1E6;
        let collateral_value = collateral_underlying.saturating_mul(coll_price.0) / coll_price.1;
        let collateral_cf = get_peridottroller(&env).get_market_cf(&collateral_vault);
        if collateral_cf > SCALE_1E6 {
            panic!("invalid market cf");
        }
        let discounted_collateral_value =
            collateral_value.saturating_mul(collateral_cf) / SCALE_1E6;
        let target_value = discounted_collateral_value.saturating_mul(leverage);
        let borrow_value = target_value.saturating_sub(discounted_collateral_value);
        if borrow_value == 0 {
            panic!("zero borrow");
        }
        let borrow_amount = borrow_value.saturating_mul(debt_price.1) / debt_price.0;
        if borrow_amount == 0 {
            panic!("borrow too small");
        }
        let min_out_oracle =
            Self::oracle_min_out(&env, &debt_asset, &position_asset, borrow_amount);
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
        let received = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
            &user,
            &swaps_chain,
            &debt_asset,
            &borrow_amount,
            &amount_with_slippage,
        );
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
        ReceiptVaultClient::new(&env, &position_vault).transfer(&user, &controller, &p_delta_i128);

        let position_cf = get_peridottroller(&env).get_market_cf(&position_vault);
        if position_cf > SCALE_1E6 {
            panic!("invalid market cf");
        }
        let position_collateral_value =
            Self::discounted_ptoken_value_usd(&env, &position_vault, p_delta);
        let combined_collateral_value =
            discounted_collateral_value.saturating_add(position_collateral_value);
        let debt_value = borrow_amount.saturating_mul(debt_price.0) / debt_price.1;
        let min_open_collateral_value =
            debt_value.saturating_mul(DEFAULT_MARGIN_MIN_OPEN_HF_SCALED) / SCALE_1E6;
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
        let collateral_underlying = collateral_ptokens.saturating_mul(coll_rate) / SCALE_1E6;
        if collateral_underlying == 0 {
            panic!("zero collateral");
        }
        let collateral_value =
            collateral_underlying.saturating_mul(collateral_price.0) / collateral_price.1;
        let borrow_value = borrow_amount.saturating_mul(debt_price.0) / debt_price.1;
        let discounted_collateral_value =
            collateral_value.saturating_mul(collateral_cf) / SCALE_1E6;
        let min_open_collateral_value =
            borrow_value.saturating_mul(DEFAULT_MARGIN_MIN_OPEN_HF_SCALED) / SCALE_1E6;
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
        let mut position = get_position_or_panic(&env, position_id);
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
            update_total_margin_ptokens(&env, &vaults.position_vault, position.collateral_ptokens, true);
        }

        position.status = PositionStatus::Closed;
        position.collateral_ptokens = 0u128;
        position.debt_shares = 0u128;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        clear_position_initial_lock(&env, position_id);
        clear_position_vaults(&env, position_id);
        clear_position_mode(&env, position_id);
        remove_user_position(&env, &user, position_id);
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
        let mut position = get_position_or_panic(&env, position_id);
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

            received = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
                &user,
                &swaps_chain,
                &position.collateral_asset,
                &collateral_underlying,
                &amount_with_slippage,
            );
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
            let p_before = debt_vault_client.get_ptoken_balance(&controller);
            let deposit_args: Vec<Val> = (controller.clone(), surplus).into_val(&env);
            Self::authorize_controller_subcall(&env, &vaults.debt_vault, "deposit", deposit_args);
            debt_vault_client.deposit(&controller, &surplus);
            let p_after = debt_vault_client.get_ptoken_balance(&controller);
            let p_delta = p_after.saturating_sub(p_before);
            if p_delta > 0 {
                let close_fee_ptokens = p_delta.saturating_mul(close_fee_bps) / BPS_SCALE;
                let user_ptokens = p_delta.saturating_sub(close_fee_ptokens);
                if user_ptokens > 0 {
                    // Accrue pending fees with OLD balance before increasing it.
                    accrue_user_fee(&env, &user, &vaults.debt_vault);
                    let free = get_margin_balance_ptokens(&env, &user, &vaults.debt_vault);
                    set_margin_balance_ptokens(
                        &env,
                        &user,
                        &vaults.debt_vault,
                        free.saturating_add(user_ptokens),
                    );
                    update_total_margin_ptokens(&env, &vaults.debt_vault, user_ptokens, true);
                }
                // Distribute close fee to LP holders of the debt vault.
                collect_margin_fee(&env, &vaults.debt_vault, close_fee_ptokens);
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

        position.status = PositionStatus::Closed;
        position.collateral_ptokens = 0u128;
        position.debt_shares = 0u128;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        clear_position_initial_lock(&env, position_id);
        clear_position_vaults(&env, position_id);
        clear_position_mode(&env, position_id);
        remove_user_position(&env, &user, position_id);
    }

    /// Legacy margin V1 is intentionally disabled. Use `liquidate_position_v2`.
    pub fn liquidate_position(env: Env, liquidator: Address, position_id: u64) {
        let _ = (env, liquidator, position_id);
        panic!("legacy margin disabled");
    }

    pub fn liquidate_position_v2(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        let mut position = get_position_or_panic(&env, position_id);
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        if liquidator == position.owner {
            panic!("self liquidation");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
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
        let collateral_value =
            Self::combined_v2_collateral_value_usd(&env, position_id, &vaults, &position);
        let debt_value = debt_amount.saturating_mul(debt_price.0) / debt_price.1;
        if collateral_value >= debt_value {
            panic!("not liquidatable");
        }

        let raw_collateral_value =
            Self::combined_v2_raw_collateral_value_usd(&env, position_id, &vaults, &position);
        if raw_collateral_value == 0 {
            panic!("no collateral");
        }
        let close_factor_repay =
            debt_amount.saturating_mul(DEFAULT_MARGIN_CLOSE_FACTOR_SCALED) / SCALE_1E6;
        let max_repay_by_close_factor = close_factor_repay.max(1).min(debt_amount);
        let max_repay_value_by_collateral =
            raw_collateral_value.saturating_mul(SCALE_1E6) / DEFAULT_MARGIN_LIQ_BONUS_SCALED;
        let max_repay_by_collateral =
            max_repay_value_by_collateral.saturating_mul(debt_price.1) / debt_price.0;
        if max_repay_by_collateral == 0 {
            panic!("collateral too small");
        }
        let repay_amount = max_repay_by_close_factor
            .min(max_repay_by_collateral)
            .min(debt_amount);

        debt_vault.repay_for_margin(&position_id, &liquidator, &repay_amount);
        let remaining_debt = debt_vault.get_margin_borrow_balance(&position_id);
        if remaining_debt >= debt_amount {
            panic!("repay failed");
        }

        let repaid_value = repay_amount.saturating_mul(debt_price.0) / debt_price.1;
        if repaid_value == 0 {
            panic!("repay too small");
        }
        let mut remaining_seize_value =
            repaid_value.saturating_mul(DEFAULT_MARGIN_LIQ_BONUS_SCALED) / SCALE_1E6;
        let seize_ptokens = Self::ptokens_for_raw_value_ceil(
            &env,
            &vaults.position_vault,
            position.collateral_ptokens,
            remaining_seize_value,
        );
        if seize_ptokens > 0 {
            let controller = env.current_contract_address();
            let seize_i128: i128 = seize_ptokens.try_into().expect("amount too large");
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
            position.collateral_ptokens = position.collateral_ptokens.saturating_sub(seize_ptokens);
            let seized_value =
                Self::raw_ptoken_value_usd(&env, &vaults.position_vault, seize_ptokens);
            remaining_seize_value = remaining_seize_value.saturating_sub(seized_value);
        }

        if let Some((initial_market, initial_ptokens)) =
            get_position_initial_lock(&env, position_id)
        {
            let initial_seize_ptokens = Self::ptokens_for_raw_value_ceil(
                &env,
                &initial_market,
                initial_ptokens,
                remaining_seize_value,
            );
            if initial_seize_ptokens > 0 {
                let controller = env.current_contract_address();
                let amt_i128: i128 = initial_seize_ptokens.try_into().expect("amount too large");
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

        if remaining_debt > 0 {
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
            update_total_margin_ptokens(&env, &vaults.position_vault, position.collateral_ptokens, true);
        }
        position.status = PositionStatus::Liquidated;
        position.collateral_ptokens = 0u128;
        position.debt_shares = 0u128;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        clear_position_vaults(&env, position_id);
        clear_position_mode(&env, position_id);
        remove_user_position(&env, &position.owner, position_id);
    }

    pub fn locked_ptokens_in_market(env: Env, user: Address, market: Address) -> u128 {
        bump_core_ttl(&env);
        let position_ids = compact_user_positions(&env, &user);
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
            let mode = get_position_mode(&env, position_id);
            if mode == PositionMode::MarginV2 {
                continue;
            }

            let vaults = get_position_vaults(&env, position_id, &position);
            if vaults.position_vault == market {
                total_locked = total_locked.saturating_add(position.collateral_ptokens);
            }

            if let Some((initial_market, initial_ptokens)) =
                get_position_initial_lock(&env, position_id)
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
        env.storage()
            .persistent()
            .get(&DataKey::Position(position_id))
    }

    /// ReceiptVault defense-in-depth callback.
    /// Returns the owner for an open MarginV2 position and validates debt-vault binding.
    pub fn get_margin_position_owner(env: Env, position_id: u64, debt_vault: Address) -> Address {
        bump_core_ttl(&env);
        let position = get_position_or_panic(&env, position_id);
        if position.status != PositionStatus::Open {
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
        compact_user_positions(&env, &user)
    }

    pub fn get_health_factor(env: Env, position_id: u64) -> u128 {
        bump_core_ttl(&env);
        let position = get_position_or_panic(&env, position_id);
        let vaults = get_position_vaults(&env, position_id, &position);
        let mode = get_position_mode(&env, position_id);
        let debt_amount = if mode == PositionMode::MarginV2 {
            ReceiptVaultClient::new(&env, &vaults.debt_vault)
                .get_margin_borrow_balance(&position_id)
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
        let debt_value = debt_amount.saturating_mul(debt_price.0) / debt_price.1;
        let mut collateral_value = Self::discounted_ptoken_value_usd(
            &env,
            &vaults.position_vault,
            position.collateral_ptokens,
        );
        if mode == PositionMode::MarginV2 {
            if let Some((initial_market, initial_ptokens)) =
                get_position_initial_lock(&env, position_id)
            {
                collateral_value = collateral_value.saturating_add(
                    Self::discounted_ptoken_value_usd(&env, &initial_market, initial_ptokens),
                );
            }
        }
        if collateral_value == 0 {
            return 0;
        }
        if collateral_value == u128::MAX {
            return u128::MAX;
        }
        collateral_value.saturating_mul(SCALE_1E6) / debt_value
    }

    fn oracle_min_out(env: &Env, token_in: &Address, token_out: &Address, amount_in: u128) -> u128 {
        let in_price = get_price_usd(env, token_in);
        let out_price = get_price_usd(env, token_out);
        if in_price.0 == 0 || in_price.1 == 0 || out_price.0 == 0 || out_price.1 == 0 {
            panic!("invalid price");
        }
        let in_value_usd = amount_in.saturating_mul(in_price.0) / in_price.1;
        let expected_out = in_value_usd.saturating_mul(out_price.1) / out_price.0;
        if expected_out == 0 {
            panic!("swap amount too small");
        }
        let max_slippage_bps = get_max_slippage_bps(env);
        expected_out.saturating_mul(SCALE_1E6.saturating_sub(max_slippage_bps)) / SCALE_1E6
    }

    fn discounted_ptoken_value_usd(env: &Env, vault: &Address, ptokens: u128) -> u128 {
        if ptokens == 0 {
            return 0;
        }
        let vault_client = ReceiptVaultClient::new(env, vault);
        let asset = vault_client.get_underlying_token();
        let price = get_price_usd(env, &asset);
        if price.0 == 0 || price.1 == 0 {
            panic!("invalid collateral price");
        }
        let cf = get_peridottroller(env).get_market_cf(vault).min(SCALE_1E6);
        let exchange_rate = vault_client.get_exchange_rate();
        if exchange_rate == 0 {
            panic!("invalid exchange rate");
        }
        let underlying = ptokens.saturating_mul(exchange_rate) / SCALE_1E6;
        let raw_value = underlying.saturating_mul(price.0) / price.1;
        raw_value.saturating_mul(cf) / SCALE_1E6
    }

    fn combined_v2_collateral_value_usd(
        env: &Env,
        position_id: u64,
        vaults: &PositionVaults,
        position: &Position,
    ) -> u128 {
        let mut value = Self::discounted_ptoken_value_usd(
            env,
            &vaults.position_vault,
            position.collateral_ptokens,
        );
        if let Some((initial_market, initial_ptokens)) = get_position_initial_lock(env, position_id)
        {
            value = value.saturating_add(Self::discounted_ptoken_value_usd(
                env,
                &initial_market,
                initial_ptokens,
            ));
        }
        value
    }

    fn combined_v2_raw_collateral_value_usd(
        env: &Env,
        position_id: u64,
        vaults: &PositionVaults,
        position: &Position,
    ) -> u128 {
        let mut value =
            Self::raw_ptoken_value_usd(env, &vaults.position_vault, position.collateral_ptokens);
        if let Some((initial_market, initial_ptokens)) = get_position_initial_lock(env, position_id)
        {
            value = value.saturating_add(Self::raw_ptoken_value_usd(
                env,
                &initial_market,
                initial_ptokens,
            ));
        }
        value
    }

    fn raw_ptoken_value_usd(env: &Env, vault: &Address, ptokens: u128) -> u128 {
        if ptokens == 0 {
            return 0;
        }
        let vault_client = ReceiptVaultClient::new(env, vault);
        let asset = vault_client.get_underlying_token();
        let price = get_price_usd(env, &asset);
        if price.0 == 0 || price.1 == 0 {
            panic!("invalid collateral price");
        }
        let exchange_rate = vault_client.get_exchange_rate();
        if exchange_rate == 0 {
            panic!("invalid exchange rate");
        }
        let underlying = ptokens.saturating_mul(exchange_rate) / SCALE_1E6;
        underlying.saturating_mul(price.0) / price.1
    }

    fn entry_price_scaled(numerator: u128, denominator: u128) -> u128 {
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

    fn ptokens_for_raw_value_ceil(
        env: &Env,
        vault: &Address,
        available_ptokens: u128,
        target_raw_value: u128,
    ) -> u128 {
        if target_raw_value == 0 || available_ptokens == 0 {
            return 0;
        }
        let available_value = Self::raw_ptoken_value_usd(env, vault, available_ptokens);
        if target_raw_value >= available_value {
            return available_ptokens;
        }
        let vault_client = ReceiptVaultClient::new(env, vault);
        let asset = vault_client.get_underlying_token();
        let price = get_price_usd(env, &asset);
        if price.0 == 0 || price.1 == 0 {
            panic!("invalid collateral price");
        }
        let exchange_rate = vault_client.get_exchange_rate();
        if exchange_rate == 0 {
            panic!("invalid exchange rate");
        }
        let underlying = Self::ceil_div(target_raw_value.saturating_mul(price.1), price.0);
        let ptokens = Self::ceil_div(underlying.saturating_mul(SCALE_1E6), exchange_rate);
        ptokens.min(available_ptokens).max(1)
    }

    fn assert_margin_lock_configured(env: &Env, vault: &Address) {
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

    fn begin_margin_withdraw_if_supported(
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

    fn authorize_controller_subcall(env: &Env, contract: &Address, fn_name: &str, args: Vec<Val>) {
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
