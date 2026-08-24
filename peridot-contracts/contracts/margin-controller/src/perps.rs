use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{token, Address, BytesN, Env, IntoVal, Symbol, Val, Vec};

use crate::constants::*;
use crate::contract::MarginController;
use crate::events::{
    CloseResidual, LiquidationFinished, LiquidationStarted, LiquidationSwapped,
    PositionCollateralAdded, PositionPoolReplaced, PositionReleased,
};
use crate::helpers::*;
use crate::storage::*;

pub(crate) struct PerpsRiskValues {
    pub(crate) equity: u128,
    pub(crate) maintenance_required: u128,
    pub(crate) debt_amount: u128,
}

impl MarginController {
    pub(crate) fn begin_open_position_v3_impl(
        env: &Env,
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
        if margin_asset == base_asset {
            panic!("assets must differ");
        }
        if margin_ptokens == 0 {
            panic!("bad collateral");
        }
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        let config =
            get_perps_pair_config_or_default(env, &margin_asset, &base_asset, &side.clone());
        if leverage < 1 || leverage > config.max_leverage {
            panic!("bad leverage");
        }
        Self::validate_perps_config(&config);

        let margin_vault = get_market(env, &margin_asset);
        let base_vault = get_market(env, &base_asset);
        let (debt_asset, position_asset, debt_vault, position_vault) = match side.clone() {
            PositionSide::Long => (
                margin_asset.clone(),
                base_asset.clone(),
                margin_vault.clone(),
                base_vault.clone(),
            ),
            PositionSide::Short => (
                base_asset.clone(),
                margin_asset.clone(),
                base_vault.clone(),
                margin_vault.clone(),
            ),
        };
        Self::assert_market_supported(env, &margin_vault);
        Self::assert_market_supported(env, &base_vault);
        Self::assert_market_not_borrow_paused(env, &debt_vault);
        Self::assert_margin_lock_configured(env, &margin_vault);
        Self::assert_margin_lock_configured(env, &base_vault);

        let (expected_in, expected_out) = match side.clone() {
            PositionSide::Long => (&margin_asset, &base_asset),
            PositionSide::Short => (&base_asset, &margin_asset),
        };
        Self::validate_direct_pool(
            env,
            &pool_tokens,
            &pool_id,
            &pool,
            expected_in,
            expected_out,
        );

        let free_margin = get_margin_balance_ptokens(env, &user, &margin_vault);
        if free_margin < margin_ptokens {
            panic!("insufficient margin balance");
        }
        accrue_user_fee(env, &user, &margin_vault);
        set_margin_balance_ptokens(
            env,
            &user,
            &margin_vault,
            free_margin.saturating_sub(margin_ptokens),
        );
        update_total_margin_ptokens(env, &margin_vault, margin_ptokens, false);

        let margin_rate = ReceiptVaultClient::new(env, &margin_vault).get_exchange_rate();
        if margin_rate == 0 {
            panic!("invalid exchange rate");
        }
        let margin_amount = margin_ptokens
            .checked_mul(margin_rate)
            .expect("valuation overflow")
            / SCALE_1E6;
        if margin_amount == 0 {
            panic!("margin too small");
        }
        let margin_price = get_price_usd(env, &margin_asset);
        let base_price = get_price_usd(env, &base_asset);
        let margin_value = Self::value_from_amount(margin_amount, margin_price);
        let notional_value = margin_value
            .checked_mul(leverage)
            .expect("valuation overflow");
        if notional_value <= margin_value {
            panic!("zero borrow");
        }
        let debt_price = match side.clone() {
            PositionSide::Long => margin_price,
            PositionSide::Short => base_price,
        };
        let borrow_value = match side.clone() {
            PositionSide::Long => notional_value.saturating_sub(margin_value),
            PositionSide::Short => notional_value,
        };
        let borrow_amount = Self::amount_from_value_floor(borrow_value, debt_price);
        if borrow_amount == 0 {
            panic!("borrow too small");
        }
        let trade_amount = match side.clone() {
            PositionSide::Long => margin_amount.saturating_add(borrow_amount),
            PositionSide::Short => borrow_amount,
        };
        let (trade_in_price, trade_out_price) = match side.clone() {
            PositionSide::Long => (margin_price, base_price),
            PositionSide::Short => (base_price, margin_price),
        };
        let execution_config =
            get_perps_pair_execution_config_or_default(env, &margin_asset, &base_asset, &side);
        let pool_quote = Self::estimate_direct_pool_swap(
            env,
            &pool_tokens,
            &pool,
            expected_in,
            expected_out,
            trade_amount,
        );
        let oracle_expected =
            Self::oracle_expected_out_from_prices(trade_in_price, trade_out_price, trade_amount);
        Self::assert_pool_quote_in_oracle_band(
            pool_quote,
            oracle_expected,
            execution_config.max_open_deviation_scaled,
        );
        let pool_min = Self::pool_quote_min_out(pool_quote, execution_config.open_slippage_scaled);
        if amount_with_slippage < pool_min {
            panic!("slippage too high");
        }

        let id = next_position_id(env);
        let now = env.ledger().timestamp();
        let position = Position {
            owner: user.clone(),
            side: side.clone(),
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
        set_position_mode(env, id, PositionMode::PerpsV3);
        set_position_vaults(env, id, &margin_vault, &debt_vault, &position_vault);
        let pending = PendingPerpsOpenPosition {
            owner: user.clone(),
            margin_asset,
            base_asset,
            side,
            margin_vault,
            debt_vault: debt_vault.clone(),
            position_vault,
            margin_ptokens,
            margin_amount,
            notional_value,
            borrow_amount,
            min_position_amount: amount_with_slippage,
            pool_tokens,
            pool_id,
            pool,
            expires_at: now.saturating_add(PENDING_OPEN_TTL_SECS),
        };
        set_pending_perps_open_position(env, id, &pending);
        push_user_position(env, &user, id);
        ReceiptVaultClient::new(env, &debt_vault).init_margin_borrow_state(&id);
        id
    }

    pub(crate) fn execute_open_position_v3_impl(env: &Env, user: Address, position_id: u64) {
        if get_pending_perps_open_execution(env, position_id).is_none() {
            // A full swap+activation touches more than Soroban's 100-entry
            // transaction footprint with the production vault/oracle stack.
            // Keep this legacy convenience entrypoint as an idempotent
            // finalizer, but require the budget-safe swap stage first.
            panic!("open swap required");
        }
        Self::activate_open_position_v3_impl(env, user, position_id);
    }

    pub(crate) fn swap_open_position_v3_impl(env: &Env, user: Address, position_id: u64) {
        let (_position, pending, vaults) =
            Self::load_pending_perps_open(env, &user, position_id, true);
        if get_pending_perps_open_execution(env, position_id).is_some() {
            panic!("open already swapped");
        }
        let (token_in, token_out) = match pending.side.clone() {
            PositionSide::Long => (pending.margin_asset.clone(), pending.base_asset.clone()),
            PositionSide::Short => (pending.base_asset.clone(), pending.margin_asset.clone()),
        };
        Self::validate_direct_pool(
            env,
            &pending.pool_tokens,
            &pending.pool_id,
            &pending.pool,
            &token_in,
            &token_out,
        );

        let controller = env.current_contract_address();
        let margin_token = token::TokenClient::new(env, &pending.margin_asset);
        let margin_bal_before = margin_token.balance(&controller);
        let margin_vault_client = ReceiptVaultClient::new(env, &pending.margin_vault);
        Self::authorize_controller_subcall(
            env,
            &pending.margin_vault,
            "withdraw",
            (controller.clone(), pending.margin_ptokens).into_val(env),
        );
        Self::begin_margin_withdraw_if_supported(
            env,
            &pending.margin_vault,
            &controller,
            &controller,
            pending.margin_ptokens,
        );
        margin_vault_client.withdraw(&controller, &pending.margin_ptokens);
        let margin_bal_after = margin_token.balance(&controller);
        let margin_received = if margin_bal_after <= margin_bal_before {
            0u128
        } else {
            (margin_bal_after - margin_bal_before) as u128
        };
        if margin_received == 0 {
            panic!("margin withdraw failed");
        }

        let debt_vault_client = ReceiptVaultClient::new(env, &vaults.debt_vault);
        Self::authorize_controller_subcall(
            env,
            &vaults.debt_vault,
            "borrow_for_margin_to_controller",
            (position_id, pending.borrow_amount).into_val(env),
        );
        debt_vault_client.borrow_for_margin_to_controller(&position_id, &pending.borrow_amount);

        let trade_amount = match pending.side.clone() {
            PositionSide::Long => margin_received.saturating_add(pending.borrow_amount),
            PositionSide::Short => pending.borrow_amount,
        };
        let received_from_swap = Self::direct_pool_swap_as_controller(
            env,
            &pending.pool_tokens,
            &pending.pool,
            &token_in,
            &token_out,
            trade_amount,
            pending.min_position_amount,
        );
        if received_from_swap < pending.min_position_amount {
            panic!("slippage too high");
        }
        let position_amount = match pending.side.clone() {
            PositionSide::Long => received_from_swap,
            PositionSide::Short => margin_received.saturating_add(received_from_swap),
        };
        if position_amount == 0 {
            panic!("swap failed");
        }

        set_pending_perps_open_execution(
            env,
            position_id,
            &PendingPerpsOpenExecution {
                margin_received,
                position_amount,
            },
        );
    }

    pub(crate) fn activate_open_position_v3_impl(env: &Env, user: Address, position_id: u64) {
        let execution = get_pending_perps_open_execution_or_panic(env, position_id);
        let (mut position, pending, vaults) =
            Self::load_pending_perps_open(env, &user, position_id, false);
        Self::activate_open_position_v3_from_execution(
            env,
            position_id,
            &mut position,
            &pending,
            &vaults,
            &execution,
            true,
        );
    }

    fn activate_open_position_v3_from_execution(
        env: &Env,
        position_id: u64,
        position: &mut Position,
        pending: &PendingPerpsOpenPosition,
        vaults: &PositionVaults,
        execution: &PendingPerpsOpenExecution,
        enforce_open_health: bool,
    ) -> (Position, PerpsPositionData) {
        if execution.margin_received == 0 || execution.position_amount == 0 {
            panic!("pending execution missing");
        }

        let controller = env.current_contract_address();
        let position_vault_client = ReceiptVaultClient::new(env, &vaults.position_vault);
        let p_before = position_vault_client.get_ptoken_balance(&controller);
        Self::authorize_controller_vault_deposit(
            env,
            &vaults.position_vault,
            &position.collateral_asset,
            &controller,
            execution.position_amount,
        );
        position_vault_client.deposit(&controller, &execution.position_amount);
        let p_after = position_vault_client.get_ptoken_balance(&controller);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta == 0 {
            panic!("no collateral minted");
        }
        let debt_vault_client = ReceiptVaultClient::new(env, &vaults.debt_vault);
        let debt_amount = debt_vault_client.get_margin_borrow_balance(&position_id);
        if debt_amount == 0 {
            panic!("zero debt");
        }
        let config = get_perps_pair_config_or_default(
            env,
            &pending.margin_asset,
            &pending.base_asset,
            &pending.side,
        );
        let perps = PerpsPositionData {
            margin_asset: pending.margin_asset.clone(),
            base_asset: pending.base_asset.clone(),
            side: pending.side.clone(),
            margin_amount: execution.margin_received,
            notional_value: pending.notional_value,
            maintenance_margin_scaled: config.maintenance_margin_scaled,
            liquidation_incentive_scaled: config.liquidation_incentive_scaled,
            pool_tokens: pending.pool_tokens.clone(),
            pool_id: pending.pool_id.clone(),
            pool: pending.pool.clone(),
        };
        let entry_denominator = match pending.side.clone() {
            PositionSide::Long => execution.position_amount,
            PositionSide::Short => pending.borrow_amount,
        };
        position.collateral_ptokens = p_delta;
        position.entry_price_scaled =
            Self::entry_price_scaled(pending.notional_value, entry_denominator);
        position.status = PositionStatus::Open;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), position);
        set_perps_position_data(env, position_id, &perps);

        if enforce_open_health {
            let risk = Self::perps_risk_values(env, position_id, position, &perps, true, true);
            if risk.equity <= risk.maintenance_required {
                panic!("insufficient margin");
            }
        }

        let activated = position.clone();
        clear_pending_perps_open_position(env, position_id);
        clear_pending_perps_open_execution(env, position_id);
        bump_position_record_ttl(env, position_id);
        (activated, perps)
    }

    fn load_pending_perps_open(
        env: &Env,
        user: &Address,
        position_id: u64,
        enforce_expiry: bool,
    ) -> (Position, PendingPerpsOpenPosition, PositionVaults) {
        let position = get_position_or_panic(env, position_id);
        if position.owner != *user {
            panic!("not owner");
        }
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let pending = get_pending_perps_open_position_or_panic(env, position_id);
        if pending.owner != *user {
            panic!("not owner");
        }
        if enforce_expiry && env.ledger().timestamp() > pending.expires_at {
            panic!("pending open expired");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        if vaults.collateral_vault != pending.margin_vault
            || vaults.debt_vault != pending.debt_vault
            || vaults.position_vault != pending.position_vault
        {
            panic!("pending vault mismatch");
        }
        (position, pending, vaults)
    }

    pub(crate) fn cancel_pending_open_v3_impl(env: &Env, user: Address, position_id: u64) {
        let position = get_position_or_panic(env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let pending = get_pending_perps_open_position_or_panic(env, position_id);
        let debt_vault_client = ReceiptVaultClient::new(env, &pending.debt_vault);
        if debt_vault_client.get_margin_borrow_balance(&position_id) != 0 {
            panic!("pending debt exists");
        }
        Self::release_unswapped_pending_open(env, position_id, &position, &pending);
    }

    pub(crate) fn expire_pending_open_v3_impl(env: &Env, position_id: u64) {
        let position = get_position_or_panic(env, position_id);
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let pending = get_pending_perps_open_position_or_panic(env, position_id);
        if env.ledger().timestamp() <= pending.expires_at {
            panic!("pending open live");
        }
        if get_pending_perps_open_execution(env, position_id).is_some() {
            panic!("pending execution exists");
        }
        if ReceiptVaultClient::new(env, &pending.debt_vault).get_margin_borrow_balance(&position_id)
            != 0
        {
            panic!("pending debt exists");
        }
        Self::release_unswapped_pending_open(env, position_id, &position, &pending);
    }

    pub(crate) fn resolve_pending_open_v3_impl(env: &Env, position_id: u64) {
        let mut position = get_position_or_panic(env, position_id);
        if position.status != PositionStatus::PendingOpen {
            panic!("not pending open");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let pending = get_pending_perps_open_position_or_panic(env, position_id);
        if env.ledger().timestamp() <= pending.expires_at {
            panic!("pending open live");
        }
        let execution = get_pending_perps_open_execution_or_panic(env, position_id);
        let vaults = get_position_vaults(env, position_id, &position);
        Self::activate_open_position_v3_from_execution(
            env,
            position_id,
            &mut position,
            &pending,
            &vaults,
            &execution,
            true,
        );
    }

    fn release_unswapped_pending_open(
        env: &Env,
        position_id: u64,
        position: &Position,
        pending: &PendingPerpsOpenPosition,
    ) {
        accrue_user_fee(env, &position.owner, &pending.margin_vault);
        let free = get_margin_balance_ptokens(env, &position.owner, &pending.margin_vault);
        set_margin_balance_ptokens(
            env,
            &position.owner,
            &pending.margin_vault,
            free.saturating_add(pending.margin_ptokens),
        );
        update_total_margin_ptokens(env, &pending.margin_vault, pending.margin_ptokens, true);
        clear_perps_v3_position_storage(env, position_id);
        remove_user_position(env, &position.owner, position_id);
    }

    pub(crate) fn repay_margin_position_v3_impl(
        env: &Env,
        user: Address,
        position_id: u64,
        amount: u128,
    ) {
        if amount == 0 {
            panic!("bad amount");
        }
        let position = get_position_or_panic(env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open && position.status != PositionStatus::Closing {
            panic!("not open");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        ReceiptVaultClient::new(env, &vaults.debt_vault).repay_for_margin(
            &position_id,
            &user,
            &amount,
        );
    }

    pub(crate) fn add_position_collateral_v3_impl(
        env: &Env,
        user: Address,
        position_id: u64,
        position_ptokens: u128,
    ) {
        if position_ptokens == 0 {
            panic!("bad ptokens");
        }
        let mut position = get_position_or_panic(env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let _ = get_perps_position_data_or_panic(env, position_id);
        if get_pending_perps_close(env, position_id).is_some() {
            panic!("close pending");
        }

        let vaults = get_position_vaults(env, position_id, &position);
        accrue_user_fee(env, &user, &vaults.position_vault);
        let free_margin = get_margin_balance_ptokens(env, &user, &vaults.position_vault);
        if free_margin < position_ptokens {
            panic!("insufficient margin balance");
        }
        let collateral_ptokens = position
            .collateral_ptokens
            .checked_add(position_ptokens)
            .expect("position collateral overflow");

        set_margin_balance_ptokens(
            env,
            &user,
            &vaults.position_vault,
            free_margin - position_ptokens,
        );
        update_total_margin_ptokens(env, &vaults.position_vault, position_ptokens, false);
        position.collateral_ptokens = collateral_ptokens;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        bump_position_ttl(env, position_id);
        PositionCollateralAdded {
            owner: user,
            position_id,
            ptoken_amount: position_ptokens,
            collateral_ptokens,
        }
        .publish(env);
    }

    pub(crate) fn release_debt_free_position_v3_impl(env: &Env, user: Address, position_id: u64) {
        let position = get_position_record_or_panic(env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        if position.status != PositionStatus::Open && position.status != PositionStatus::Closing {
            panic!("not releasable");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        let debt_vault = ReceiptVaultClient::new(env, &vaults.debt_vault);
        debt_vault.update_interest();
        if debt_vault.get_margin_borrow_balance(&position_id) != 0 {
            panic!("debt remains");
        }
        match position.status {
            PositionStatus::Open => {
                Self::release_open_position_ptokens(env, position_id, &position, &vaults)
            }
            PositionStatus::Closing => {
                let pending = get_pending_perps_close_or_panic(env, position_id);
                Self::release_closing_position_assets(
                    env,
                    position_id,
                    &position,
                    &vaults,
                    &pending,
                );
            }
            _ => panic!("not releasable"),
        }
    }

    pub(crate) fn replace_position_pool_v3_impl(
        env: &Env,
        position_id: u64,
        pool_tokens: Vec<Address>,
        pool_id: BytesN<32>,
        pool: Address,
    ) {
        let position = get_position_record_or_panic(env, position_id);
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        if position.status != PositionStatus::Open
            && position.status != PositionStatus::Closing
            && position.status != PositionStatus::Liquidated
        {
            panic!("position route not replaceable");
        }

        let adapter = SwapAdapterClient::new(env, &get_swap_adapter(env));
        let mut perps = get_perps_position_data_or_panic(env, position_id);
        if adapter.is_pool_binding_allowed(&perps.pool_id, &perps.pool) {
            panic!("pool still allowed");
        }
        Self::validate_direct_pool(
            env,
            &pool_tokens,
            &pool_id,
            &pool,
            &position.collateral_asset,
            &position.debt_asset,
        );

        let previous_pool = perps.pool.clone();
        perps.pool_tokens = pool_tokens;
        perps.pool_id = pool_id.clone();
        perps.pool = pool.clone();
        set_perps_position_data(env, position_id, &perps);
        PositionPoolReplaced {
            position_id,
            previous_pool,
            new_pool: pool,
            new_pool_id: pool_id,
        }
        .publish(env);
    }

    pub(crate) fn close_position_v3_impl(
        env: &Env,
        user: Address,
        position_id: u64,
        amount_with_slippage: u128,
    ) {
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        if get_pending_perps_close(env, position_id).is_some() {
            panic!("close pending");
        }
        let position = get_position_record_or_panic(env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let perps = get_perps_position_data_or_panic(env, position_id);
        if perps.side == PositionSide::Short {
            panic!("use split short close");
        }
        Self::close_perps_position(
            env,
            &user,
            position_id,
            &position,
            &perps,
            amount_with_slippage,
            None,
        );
    }

    pub(crate) fn begin_close_position_v3_impl(env: &Env, user: Address, position_id: u64) {
        if get_pending_perps_close(env, position_id).is_some() {
            panic!("close pending");
        }
        let position = get_position_record_or_panic(env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let _ = get_perps_position_data_or_panic(env, position_id);
        // Closing is permitted at any health. Swap/finish refresh debt, while
        // any competing liquidation performs its own current health check.

        set_pending_perps_close(
            env,
            position_id,
            &PendingPerpsClose {
                owner: user,
                collateral_underlying: 0u128,
                debt_amount: 0u128,
                received_debt_asset: 0u128,
                expires_at: env
                    .ledger()
                    .timestamp()
                    .saturating_add(PENDING_CLOSE_TTL_SECS),
                prepared_ledger: env.ledger().sequence(),
            },
        );
        bump_position_record_ttl(env, position_id);
    }

    pub(crate) fn withdraw_close_position_v3_impl(env: &Env, user: Address, position_id: u64) {
        let mut position = get_position_record_or_panic(env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let mut pending = get_pending_perps_close_or_panic(env, position_id);
        if pending.owner != user {
            panic!("close owner mismatch");
        }
        if pending.collateral_underlying > 0 || pending.received_debt_asset > 0 {
            panic!("close already withdrawn");
        }
        if env.ledger().timestamp() > pending.expires_at {
            panic!("close preparation expired");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        let collateral_underlying = Self::withdraw_position_underlying(
            env,
            &vaults.position_vault,
            &position.collateral_asset,
            position.collateral_ptokens,
        );
        if collateral_underlying == 0 {
            panic!("no collateral");
        }

        position.status = PositionStatus::Closing;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        pending.collateral_underlying = collateral_underlying;
        pending.expires_at = env
            .ledger()
            .timestamp()
            .saturating_add(PENDING_CLOSE_TTL_SECS);
        set_pending_perps_close(env, position_id, &pending);
        bump_position_record_ttl(env, position_id);
    }

    pub(crate) fn swap_close_position_v3_impl(
        env: &Env,
        user: Address,
        position_id: u64,
        amount_with_slippage: u128,
    ) {
        Self::swap_close_position_v3_with_input_impl(
            env,
            user,
            position_id,
            None,
            amount_with_slippage,
        );
    }

    pub(crate) fn swap_close_short_position_v3_impl(
        env: &Env,
        user: Address,
        position_id: u64,
        swap_amount_in: u128,
        min_debt_out: u128,
    ) {
        Self::swap_close_position_v3_with_input_impl(
            env,
            user,
            position_id,
            Some(swap_amount_in),
            min_debt_out,
        );
    }

    fn swap_close_position_v3_with_input_impl(
        env: &Env,
        user: Address,
        position_id: u64,
        short_swap_amount_in: Option<u128>,
        amount_with_slippage: u128,
    ) {
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        let position = get_position_record_or_panic(env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Closing {
            panic!("not closing");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let mut pending = get_pending_perps_close_or_panic(env, position_id);
        if pending.owner != user {
            panic!("close owner mismatch");
        }
        if pending.received_debt_asset > 0 || pending.debt_amount > 0 {
            panic!("close already swapped");
        }
        if env.ledger().timestamp() > pending.expires_at {
            panic!("pending close expired");
        }
        let perps = get_perps_position_data_or_panic(env, position_id);
        if position.collateral_asset != position.debt_asset {
            Self::validate_direct_pool(
                env,
                &perps.pool_tokens,
                &perps.pool_id,
                &perps.pool,
                &position.collateral_asset,
                &position.debt_asset,
            );
        }
        let swap_amount_in = match (perps.side.clone(), short_swap_amount_in) {
            (PositionSide::Long, None) => pending.collateral_underlying,
            (PositionSide::Long, Some(_)) => panic!("not short position"),
            (PositionSide::Short, None) => panic!("use short close"),
            (PositionSide::Short, Some(amount)) => {
                if amount == 0 || amount > pending.collateral_underlying {
                    panic!("bad swap amount");
                }
                amount
            }
        };
        let vaults = get_position_vaults(env, position_id, &position);
        let debt_vault_client = ReceiptVaultClient::new(env, &vaults.debt_vault);
        debt_vault_client.update_interest();
        let debt_amount = debt_vault_client.get_margin_borrow_balance(&position_id);
        if debt_amount == 0 {
            Self::release_closing_position_assets(env, position_id, &position, &vaults, &pending);
            return;
        }
        let debt_buffer = Self::ceil_div(
            debt_amount
                .checked_mul(CLOSE_DEBT_BUFFER_BPS)
                .expect("close debt buffer overflow"),
            BPS_SCALE,
        );
        let buffered_debt = debt_amount
            .checked_add(debt_buffer)
            .expect("close debt buffer overflow");
        if amount_with_slippage < buffered_debt {
            panic!("debt floor too low");
        }
        let execution_config = get_perps_pair_execution_config_or_default(
            env,
            &perps.margin_asset,
            &perps.base_asset,
            &perps.side,
        );
        let exit_config = get_perps_pair_exit_config_or_default(
            env,
            &perps.margin_asset,
            &perps.base_asset,
            &perps.side,
        );
        let pool_min = if position.collateral_asset == position.debt_asset {
            swap_amount_in
        } else {
            let quote = Self::estimate_direct_pool_swap(
                env,
                &perps.pool_tokens,
                &perps.pool,
                &position.collateral_asset,
                &position.debt_asset,
                swap_amount_in,
            );
            Self::assert_pool_quote_above_oracle_floor(
                env,
                quote,
                &position.collateral_asset,
                &position.debt_asset,
                swap_amount_in,
                exit_config.max_close_deviation_scaled,
            );
            Self::pool_quote_min_out(quote, execution_config.close_slippage_scaled)
        };
        if amount_with_slippage < pool_min {
            panic!("slippage too high");
        }
        let received_debt_asset = if position.collateral_asset == position.debt_asset {
            swap_amount_in
        } else {
            Self::direct_pool_swap_as_controller(
                env,
                &perps.pool_tokens,
                &perps.pool,
                &position.collateral_asset,
                &position.debt_asset,
                swap_amount_in,
                amount_with_slippage,
            )
        };
        if received_debt_asset < buffered_debt {
            panic!("debt remains");
        }

        pending.debt_amount = debt_amount;
        pending.received_debt_asset = received_debt_asset;
        pending.expires_at = env
            .ledger()
            .timestamp()
            .saturating_add(PENDING_CLOSE_TTL_SECS);
        set_pending_perps_close_remainder(
            env,
            position_id,
            pending.collateral_underlying.saturating_sub(swap_amount_in),
        );
        set_pending_perps_close(env, position_id, &pending);
    }

    pub(crate) fn finish_close_position_v3_impl(env: &Env, position_id: u64) {
        let position = get_position_record_or_panic(env, position_id);
        if position.status != PositionStatus::Closing {
            panic!("not closing");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let mut pending = get_pending_perps_close_or_panic(env, position_id);
        if pending.owner != position.owner {
            panic!("close owner mismatch");
        }
        if pending.received_debt_asset == 0 && pending.debt_amount == 0 {
            panic!("close not swapped");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        let debt_vault_client = ReceiptVaultClient::new(env, &vaults.debt_vault);
        debt_vault_client.update_interest();
        let debt_amount = debt_vault_client.get_margin_borrow_balance(&position_id);
        let repay_amount = pending.received_debt_asset.min(debt_amount);
        let mut repaid = 0u128;
        if repay_amount > 0 {
            repaid = Self::repay_margin_from_controller(
                env,
                &vaults.debt_vault,
                position_id,
                repay_amount,
            );
        }
        pending.received_debt_asset = pending.received_debt_asset.saturating_sub(repaid);
        let remaining_debt = debt_vault_client.get_margin_borrow_balance(&position_id);
        set_pending_perps_close(env, position_id, &pending);
        if remaining_debt > 0 {
            CloseResidual {
                position_id,
                owner: position.owner,
                repaid,
                remaining_debt,
                takeover_after: pending.expires_at,
            }
            .publish(env);
            return;
        }
        Self::release_closing_position_assets(env, position_id, &position, &vaults, &pending);
    }

    pub(crate) fn cancel_close_position_v3_impl(env: &Env, user: Address, position_id: u64) {
        let position = get_position_record_or_panic(env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        Self::restore_pending_close_position(env, position_id, position);
    }

    pub(crate) fn expire_close_position_v3_impl(env: &Env, position_id: u64) {
        let position = get_position_record_or_panic(env, position_id);
        let pending = get_pending_perps_close_or_panic(env, position_id);
        if env.ledger().timestamp() <= pending.expires_at {
            panic!("pending close active");
        }
        Self::restore_pending_close_position(env, position_id, position);
    }

    fn restore_pending_close_position(env: &Env, position_id: u64, mut position: Position) {
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let pending = get_pending_perps_close_or_panic(env, position_id);
        if pending.owner != position.owner {
            panic!("close owner mismatch");
        }
        if pending.received_debt_asset > 0 || pending.debt_amount > 0 {
            panic!("close already swapped");
        }
        if pending.collateral_underlying == 0 {
            if position.status != PositionStatus::Open {
                panic!("bad close state");
            }
            clear_pending_perps_close(env, position_id);
            bump_position_record_ttl(env, position_id);
            return;
        }
        if position.status != PositionStatus::Closing {
            panic!("not closing");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        let controller = env.current_contract_address();
        let vault_client = ReceiptVaultClient::new(env, &vaults.position_vault);
        let p_before = vault_client.get_ptoken_balance(&controller);
        Self::authorize_controller_vault_deposit(
            env,
            &vaults.position_vault,
            &position.collateral_asset,
            &controller,
            pending.collateral_underlying,
        );
        vault_client.deposit(&controller, &pending.collateral_underlying);
        let p_after = vault_client.get_ptoken_balance(&controller);
        let restored_ptokens = p_after.saturating_sub(p_before);
        if restored_ptokens == 0 {
            panic!("close restore failed");
        }
        position.collateral_ptokens = restored_ptokens;
        position.status = PositionStatus::Open;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        clear_pending_perps_close(env, position_id);
        bump_position_record_ttl(env, position_id);
    }

    pub(crate) fn liquidate_position_v3_impl(env: &Env, liquidator: Address, position_id: u64) {
        let position = get_position_or_panic(env, position_id);
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        if liquidator == position.owner {
            panic!("self liquidation");
        }
        if position.status == PositionStatus::PendingOpen {
            Self::liquidate_pending_open_position_v3_impl(env, liquidator, position_id, position);
            return;
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        let perps = get_perps_position_data_or_panic(env, position_id);
        let risk = Self::perps_risk_values(env, position_id, &position, &perps, true, true);
        if risk.equity > risk.maintenance_required {
            panic!("not liquidatable");
        }
        // Liquidated status blocks any prepared close from progressing. Its
        // record is removed with the rest of the position during settlement.
        Self::close_perps_position(
            env,
            &position.owner,
            position_id,
            &position,
            &perps,
            1u128,
            Some(liquidator),
        );
    }

    pub(crate) fn begin_liquidation_v3_impl(env: &Env, liquidator: Address, position_id: u64) {
        let mut position = get_position_record_or_panic(env, position_id);
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        if position.status == PositionStatus::Liquidated {
            panic!("liquidation pending");
        }
        if liquidator == position.owner {
            panic!("self liquidation");
        }
        let perps = get_perps_position_data_or_panic(env, position_id);
        let risk = Self::perps_risk_values(env, position_id, &position, &perps, true, true);
        let debt_amount = risk.debt_amount;
        if debt_amount == 0 {
            panic!("zero debt");
        }
        let mut liquidation_incentive_scaled = perps.liquidation_incentive_scaled;
        let mut converted_debt_asset = 0u128;
        let mut controller_collateral_underlying = 0u128;
        let liquidation_stage = if position.status == PositionStatus::Open {
            if risk.equity > risk.maintenance_required {
                panic!("not liquidatable");
            }
            // Settlement cleanup removes any prepared close record. Avoid
            // pulling it into this budget-critical stage.
            PendingLiquidationStage::Started
        } else if position.status == PositionStatus::Closing {
            let closing = get_pending_perps_close_or_panic(env, position_id);
            if closing.owner != position.owner {
                panic!("close owner mismatch");
            }
            let stalled = env.ledger().timestamp() > closing.expires_at;
            let residual_recovery =
                closing.debt_amount > 0 && closing.received_debt_asset < debt_amount && stalled;
            // Timeout makes normal close completion permissionless. A healthy
            // close can enter this recovery path only when current debt has
            // actually outrun held debt-asset proceeds, and that recovery pays
            // no liquidation incentive.
            if risk.equity > risk.maintenance_required && !residual_recovery {
                panic!("not liquidatable");
            }
            if residual_recovery {
                liquidation_incentive_scaled = 0u128;
            }
            if closing.debt_amount > 0 {
                converted_debt_asset = closing.received_debt_asset;
                controller_collateral_underlying =
                    get_pending_perps_close_remainder(env, position_id);
            } else {
                controller_collateral_underlying = closing.collateral_underlying;
            }
            position.collateral_ptokens = 0u128;
            clear_pending_perps_close(env, position_id);
            if controller_collateral_underlying == 0 {
                PendingLiquidationStage::CollateralConverted
            } else {
                PendingLiquidationStage::Started
            }
        } else {
            panic!("not liquidatable state");
        };
        position.status = PositionStatus::Liquidated;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        bump_position_record_ttl(env, position_id);
        let takeover_after = env
            .ledger()
            .timestamp()
            .saturating_add(PENDING_LIQUIDATION_TTL_SECS);
        let pending = PendingLiquidation {
            kind: PendingLiquidationKind::PerpsV3,
            stage: liquidation_stage,
            owner: position.owner,
            liquidator,
            debt_amount,
            // Retained for V2 schema compatibility. New V3 deadlines use an
            // append-only storage key.
            repay_amount: 0u128,
            received_debt_asset: converted_debt_asset,
            position_seize_ptokens: 0u128,
            position_fee_ptokens: 0u128,
            initial_market: None,
            initial_seize_ptokens: 0u128,
            initial_fee_ptokens: 0u128,
            reserve_recipient: None,
            liquidation_incentive_scaled,
        };
        set_pending_liquidation(env, position_id, &pending);
        if controller_collateral_underlying > 0 {
            set_pending_liquidation_collateral_underlying(
                env,
                position_id,
                controller_collateral_underlying,
            );
        }
        set_pending_liquidation_takeover_after(env, position_id, takeover_after);
        LiquidationStarted {
            position_id,
            liquidator: pending.liquidator.clone(),
            owner: pending.owner.clone(),
            debt_amount,
            takeover_after,
        }
        .publish(env);
    }

    pub(crate) fn swap_liquidation_v3_impl(
        env: &Env,
        liquidator: Address,
        position_id: u64,
        amount_with_slippage: u128,
    ) {
        let mut pending = get_pending_liquidation_or_panic(env, position_id);
        if pending.kind != PendingLiquidationKind::PerpsV3
            || pending.stage != PendingLiquidationStage::Started
        {
            panic!("bad liquidation stage");
        }
        let now = env.ledger().timestamp();
        let expires_at = get_pending_liquidation_takeover_after(env, position_id)
            .unwrap_or_else(|| pending.repay_amount.min(u64::MAX as u128) as u64);
        if pending.liquidator != liquidator && now <= expires_at {
            panic!("not liquidator");
        }
        pending.liquidator = liquidator.clone();
        let takeover_after = now.saturating_add(PENDING_LIQUIDATION_TTL_SECS);
        set_pending_liquidation_takeover_after(env, position_id, takeover_after);
        let mut position = get_position_record_or_panic(env, position_id);
        if position.status != PositionStatus::Liquidated {
            panic!("not liquidating");
        }
        if position.owner != pending.owner {
            panic!("pending owner mismatch");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let perps = get_perps_position_data_or_panic(env, position_id);
        if position.collateral_asset != position.debt_asset {
            Self::validate_direct_pool(
                env,
                &perps.pool_tokens,
                &perps.pool_id,
                &perps.pool,
                &position.collateral_asset,
                &position.debt_asset,
            );
        }
        let vaults = get_position_vaults(env, position_id, &position);
        let held_underlying = get_pending_liquidation_collateral_underlying(env, position_id);
        let collateral_underlying = if held_underlying > 0 {
            held_underlying
        } else {
            if position.collateral_ptokens == 0 {
                panic!("no collateral");
            }
            Self::withdraw_position_underlying(
                env,
                &vaults.position_vault,
                &position.collateral_asset,
                position.collateral_ptokens,
            )
        };
        if collateral_underlying == 0 {
            panic!("no collateral");
        }
        let received_from_swap = if position.collateral_asset == position.debt_asset {
            collateral_underlying
        } else {
            if amount_with_slippage == 0 {
                panic!("bad slippage");
            }
            let execution_config = get_perps_pair_execution_config_or_default(
                env,
                &perps.margin_asset,
                &perps.base_asset,
                &perps.side,
            );
            let exit_config = get_perps_pair_exit_config_or_default(
                env,
                &perps.margin_asset,
                &perps.base_asset,
                &perps.side,
            );
            let quote = Self::estimate_direct_pool_swap(
                env,
                &perps.pool_tokens,
                &perps.pool,
                &position.collateral_asset,
                &position.debt_asset,
                collateral_underlying,
            );
            Self::assert_pool_quote_above_oracle_floor(
                env,
                quote,
                &position.collateral_asset,
                &position.debt_asset,
                collateral_underlying,
                exit_config.max_liq_deviation_scaled,
            );
            let pool_min =
                Self::pool_quote_min_out(quote, execution_config.liquidation_slippage_scaled);
            if amount_with_slippage < pool_min {
                panic!("slippage too high");
            }
            Self::direct_pool_swap_as_controller(
                env,
                &perps.pool_tokens,
                &perps.pool,
                &position.collateral_asset,
                &position.debt_asset,
                collateral_underlying,
                amount_with_slippage,
            )
        };
        if received_from_swap == 0 {
            panic!("swap output too small");
        }
        let received_debt_asset = pending
            .received_debt_asset
            .checked_add(received_from_swap)
            .expect("liquidation proceeds overflow");
        position.collateral_ptokens = 0u128;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        bump_position_record_ttl(env, position_id);
        pending.stage = PendingLiquidationStage::CollateralConverted;
        pending.received_debt_asset = received_debt_asset;
        set_pending_liquidation_collateral_underlying(env, position_id, 0u128);
        set_pending_liquidation(env, position_id, &pending);
        LiquidationSwapped {
            position_id,
            liquidator: pending.liquidator.clone(),
            collateral_underlying,
            received_debt_asset,
            takeover_after,
        }
        .publish(env);
    }

    pub(crate) fn finish_liquidation_v3_impl(env: &Env, liquidator: Address, position_id: u64) {
        let pending = get_pending_liquidation_or_panic(env, position_id);
        if pending.kind != PendingLiquidationKind::PerpsV3
            || pending.stage != PendingLiquidationStage::CollateralConverted
        {
            panic!("bad liquidation stage");
        }
        let expires_at = get_pending_liquidation_takeover_after(env, position_id)
            .unwrap_or_else(|| pending.repay_amount.min(u64::MAX as u128) as u64);
        if pending.liquidator != liquidator && env.ledger().timestamp() <= expires_at {
            panic!("not liquidator");
        }
        let position = get_position_record_or_panic(env, position_id);
        if position.status != PositionStatus::Liquidated {
            panic!("not liquidating");
        }
        if position.owner != pending.owner {
            panic!("pending owner mismatch");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        let debt_vault_client = ReceiptVaultClient::new(env, &vaults.debt_vault);
        debt_vault_client.update_interest();
        let debt_before = debt_vault_client.get_margin_borrow_balance(&position_id);
        let repay_amount = pending.received_debt_asset.min(debt_before);
        let repaid =
            Self::repay_margin_from_controller(env, &vaults.debt_vault, position_id, repay_amount);
        let remaining_debt = debt_vault_client.get_margin_borrow_balance(&position_id);
        if remaining_debt > 0 {
            Self::authorize_controller_subcall(
                env,
                &vaults.debt_vault,
                "absorb_margin_bad_debt",
                (position_id,).into_val(env),
            );
            debt_vault_client.absorb_margin_bad_debt(&position_id);
        }

        let mut surplus = pending.received_debt_asset.saturating_sub(repaid);
        let incentive = surplus.min(
            pending
                .debt_amount
                .checked_mul(pending.liquidation_incentive_scaled)
                .expect("liq overflow")
                / SCALE_1E6,
        );
        if incentive > 0 {
            Self::transfer_controller_underlying(env, &position.debt_asset, &liquidator, incentive);
            surplus = surplus.saturating_sub(incentive);
        }
        Self::credit_margin_underlying(env, &position.owner, &vaults.debt_vault, surplus);
        LiquidationFinished {
            position_id,
            liquidator: liquidator.clone(),
            owner: position.owner.clone(),
            repaid,
            bad_debt: remaining_debt,
            incentive,
        }
        .publish(env);
        clear_perps_v3_position_storage(env, position_id);
        remove_user_position(env, &position.owner, position_id);
    }

    fn liquidate_pending_open_position_v3_impl(
        env: &Env,
        liquidator: Address,
        position_id: u64,
        mut position: Position,
    ) {
        let execution = get_pending_perps_open_execution_or_panic(env, position_id);
        let pending = get_pending_perps_open_position_or_panic(env, position_id);
        if pending.owner != position.owner {
            panic!("not owner");
        }
        if env.ledger().timestamp() <= pending.expires_at {
            panic!("pending open active");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        if vaults.collateral_vault != pending.margin_vault
            || vaults.debt_vault != pending.debt_vault
            || vaults.position_vault != pending.position_vault
        {
            panic!("pending vault mismatch");
        }
        let (activated_position, perps) = Self::activate_open_position_v3_from_execution(
            env,
            position_id,
            &mut position,
            &pending,
            &vaults,
            &execution,
            false,
        );
        let risk =
            Self::perps_risk_values(env, position_id, &activated_position, &perps, true, true);
        if risk.equity > risk.maintenance_required {
            panic!("not liquidatable");
        }
        Self::close_perps_position(
            env,
            &activated_position.owner,
            position_id,
            &activated_position,
            &perps,
            1u128,
            Some(liquidator),
        );
    }

    pub(crate) fn get_perps_health_factor(
        env: &Env,
        position_id: u64,
        position: &Position,
    ) -> u128 {
        let perps = get_perps_position_data_or_panic(env, position_id);
        let risk = Self::perps_risk_values(env, position_id, position, &perps, true, true);
        if risk.maintenance_required == 0 {
            return u128::MAX;
        }
        risk.equity.checked_mul(SCALE_1E6).expect("health overflow") / risk.maintenance_required
    }

    pub(crate) fn preview_liquidation_v3_impl(
        env: &Env,
        position_id: u64,
    ) -> PerpsLiquidationQuote {
        let position = get_position_record_or_panic(env, position_id);
        if position.status != PositionStatus::Open && position.status != PositionStatus::Liquidated
        {
            panic!("not liquidatable state");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let perps = get_perps_position_data_or_panic(env, position_id);
        let vaults = get_position_vaults(env, position_id, &position);
        let position_vault = ReceiptVaultClient::new(env, &vaults.position_vault);
        let exchange_rate = position_vault.get_exchange_rate();
        if exchange_rate == 0 {
            panic!("invalid exchange rate");
        }
        let held_underlying = if position.status == PositionStatus::Liquidated {
            get_pending_liquidation_collateral_underlying(env, position_id)
        } else {
            0u128
        };
        let collateral_underlying = if held_underlying > 0 {
            held_underlying
        } else {
            position
                .collateral_ptokens
                .checked_mul(exchange_rate)
                .expect("valuation overflow")
                / SCALE_1E6
        };
        if collateral_underlying == 0 {
            panic!("no collateral");
        }
        let debt_amount = ReceiptVaultClient::new(env, &vaults.debt_vault)
            .get_margin_borrow_balance(&position_id);
        let oracle_min_out = Self::oracle_min_out(
            env,
            &position.collateral_asset,
            &position.debt_asset,
            collateral_underlying,
        );
        let pool_estimated_out = if position.collateral_asset == position.debt_asset {
            collateral_underlying
        } else {
            let (in_idx, out_idx) = Self::direct_pool_indices(
                &perps.pool_tokens,
                &position.collateral_asset,
                &position.debt_asset,
            );
            AquariusPoolClient::new(env, &perps.pool).estimate_swap(
                &in_idx,
                &out_idx,
                &collateral_underlying,
            )
        };
        PerpsLiquidationQuote {
            collateral_underlying,
            debt_amount,
            oracle_min_out,
            pool_estimated_out,
        }
    }

    fn validate_perps_config(config: &PerpsPairConfig) {
        if config.max_leverage < 1 || config.max_leverage > MAX_LEVERAGE_CAP {
            panic!("invalid leverage");
        }
        if config.maintenance_margin_scaled == 0
            || config.maintenance_margin_scaled > MAX_PERPS_MAINTENANCE_MARGIN_SCALED
        {
            panic!("invalid maintenance margin");
        }
        if config.liquidation_incentive_scaled > MAX_PERPS_LIQUIDATION_INCENTIVE_SCALED {
            panic!("invalid liquidation incentive");
        }
    }

    fn value_from_amount(amount: u128, price: (u128, u128)) -> u128 {
        if price.0 == 0 || price.1 == 0 {
            panic!("invalid price");
        }
        amount.checked_mul(price.0).expect("valuation overflow") / price.1
    }

    fn amount_from_value_floor(value: u128, price: (u128, u128)) -> u128 {
        if price.0 == 0 || price.1 == 0 {
            panic!("invalid price");
        }
        value.checked_mul(price.1).expect("valuation overflow") / price.0
    }

    fn validate_direct_pool(
        env: &Env,
        pool_tokens: &Vec<Address>,
        pool_id: &BytesN<32>,
        pool: &Address,
        expected_in: &Address,
        expected_out: &Address,
    ) {
        if pool_tokens.len() != 2 {
            panic!("bad swaps");
        }
        if pool_id.to_array() == [0u8; 32] {
            panic!("bad swaps");
        }
        let _ = Self::direct_pool_indices(pool_tokens, expected_in, expected_out);
        let adapter = SwapAdapterClient::new(env, &get_swap_adapter(env));
        // SwapAdapter's binding read also checks the pool-level kill switch.
        if !adapter.is_pool_binding_allowed(pool_id, pool) {
            panic!("pool binding not allowed");
        }
    }

    fn direct_pool_indices(
        pool_tokens: &Vec<Address>,
        token_in: &Address,
        token_out: &Address,
    ) -> (u32, u32) {
        if pool_tokens.len() != 2 {
            panic!("bad swaps");
        }
        let token_0 = pool_tokens.get(0).unwrap();
        let token_1 = pool_tokens.get(1).unwrap();
        if token_0 == *token_in && token_1 == *token_out {
            (0u32, 1u32)
        } else if token_1 == *token_in && token_0 == *token_out {
            (1u32, 0u32)
        } else {
            panic!("bad swaps");
        }
    }

    fn direct_pool_swap_as_controller(
        env: &Env,
        pool_tokens: &Vec<Address>,
        pool: &Address,
        token_in: &Address,
        token_out: &Address,
        amount_in: u128,
        amount_out_min: u128,
    ) -> u128 {
        if amount_in == 0 || amount_out_min == 0 {
            panic!("bad swap amount");
        }
        let (in_idx, out_idx) = Self::direct_pool_indices(pool_tokens, token_in, token_out);
        let controller = env.current_contract_address();
        let output_token = token::TokenClient::new(env, token_out);
        let output_before = output_token.balance(&controller);
        Self::authorize_controller_pool_swap(
            env,
            pool,
            token_in,
            &controller,
            in_idx,
            out_idx,
            amount_in,
            amount_out_min,
        );
        let _reported = AquariusPoolClient::new(env, pool).swap(
            &controller,
            &in_idx,
            &out_idx,
            &amount_in,
            &amount_out_min,
        );
        let output_after = output_token.balance(&controller);
        let received = if output_after <= output_before {
            0u128
        } else {
            (output_after - output_before) as u128
        };
        if received < amount_out_min {
            panic!("slippage too high");
        }
        received
    }

    fn estimate_direct_pool_swap(
        env: &Env,
        pool_tokens: &Vec<Address>,
        pool: &Address,
        token_in: &Address,
        token_out: &Address,
        amount_in: u128,
    ) -> u128 {
        if amount_in == 0 {
            panic!("bad swap amount");
        }
        let (in_idx, out_idx) = Self::direct_pool_indices(pool_tokens, token_in, token_out);
        let quote = AquariusPoolClient::new(env, pool).estimate_swap(&in_idx, &out_idx, &amount_in);
        if quote == 0 {
            panic!("swap quote unavailable");
        }
        quote
    }

    fn pool_quote_min_out(pool_quote: u128, slippage_scaled: u128) -> u128 {
        if pool_quote == 0 || slippage_scaled > SCALE_1E6 {
            panic!("invalid pool quote");
        }
        let min_out = pool_quote
            .checked_mul(SCALE_1E6.saturating_sub(slippage_scaled))
            .expect("pool quote overflow")
            / SCALE_1E6;
        if min_out == 0 {
            panic!("swap amount too small");
        }
        min_out
    }

    fn assert_pool_quote_in_oracle_band(
        pool_quote: u128,
        oracle_expected: u128,
        max_deviation_scaled: u128,
    ) {
        if pool_quote == 0
            || oracle_expected == 0
            || max_deviation_scaled > MAX_POOL_EXECUTION_DEVIATION_SCALED
        {
            panic!("invalid execution price");
        }
        let lower = oracle_expected
            .checked_mul(SCALE_1E6.saturating_sub(max_deviation_scaled))
            .expect("execution price overflow")
            / SCALE_1E6;
        let upper = Self::ceil_div(
            oracle_expected
                .checked_mul(
                    SCALE_1E6
                        .checked_add(max_deviation_scaled)
                        .expect("execution price overflow"),
                )
                .expect("execution price overflow"),
            SCALE_1E6,
        );
        if pool_quote < lower || pool_quote > upper {
            panic!("pool oracle divergence");
        }
    }

    fn assert_pool_quote_above_oracle_floor(
        env: &Env,
        pool_quote: u128,
        token_in: &Address,
        token_out: &Address,
        amount_in: u128,
        max_deviation_scaled: u128,
    ) {
        let oracle_expected = Self::oracle_expected_out_from_prices(
            get_price_usd(env, token_in),
            get_price_usd(env, token_out),
            amount_in,
        );
        if pool_quote == 0
            || oracle_expected == 0
            || max_deviation_scaled > MAX_POOL_EXECUTION_DEVIATION_SCALED
        {
            panic!("invalid execution price");
        }
        let lower = oracle_expected
            .checked_mul(SCALE_1E6.saturating_sub(max_deviation_scaled))
            .expect("execution price overflow")
            / SCALE_1E6;
        if pool_quote < lower {
            panic!("pool oracle divergence");
        }
    }

    fn raw_ptoken_value_usd_with_update(
        env: &Env,
        vault: &Address,
        asset: &Address,
        ptokens: u128,
        update_interest: bool,
        refresh_price: bool,
    ) -> u128 {
        if ptokens == 0 {
            return 0;
        }
        let vault_client = ReceiptVaultClient::new(env, vault);
        let price = if refresh_price {
            get_price_usd(env, asset)
        } else {
            get_price_usd_cache_first(env, asset)
        };
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
        Self::value_from_amount(underlying, price)
    }

    fn perps_risk_values(
        env: &Env,
        position_id: u64,
        position: &Position,
        perps: &PerpsPositionData,
        update_interest: bool,
        refresh_price: bool,
    ) -> PerpsRiskValues {
        let (debt_vault_address, position_vault) =
            get_position_risk_vaults(env, position_id, position);
        let debt_vault = ReceiptVaultClient::new(env, &debt_vault_address);
        if update_interest {
            debt_vault.update_interest();
        }
        let debt_amount = debt_vault.get_margin_borrow_balance(&position_id);
        let debt_price = if refresh_price {
            get_price_usd(env, &position.debt_asset)
        } else {
            get_price_usd_cache_first(env, &position.debt_asset)
        };
        let debt_value = Self::value_from_amount(debt_amount, debt_price);
        let collateral_value = Self::raw_ptoken_value_usd_with_update(
            env,
            &position_vault,
            &position.collateral_asset,
            position.collateral_ptokens,
            false,
            refresh_price,
        );
        let current_notional = match perps.side {
            PositionSide::Long => collateral_value,
            PositionSide::Short => debt_value,
        };
        let maintenance_required = current_notional
            .checked_mul(perps.maintenance_margin_scaled)
            .expect("health overflow")
            / SCALE_1E6;
        PerpsRiskValues {
            equity: collateral_value.saturating_sub(debt_value),
            maintenance_required,
            debt_amount,
        }
    }

    fn withdraw_position_underlying(
        env: &Env,
        vault: &Address,
        token: &Address,
        ptokens: u128,
    ) -> u128 {
        if ptokens == 0 {
            return 0;
        }
        let controller = env.current_contract_address();
        let token_client = token::TokenClient::new(env, token);
        let bal_before = token_client.balance(&controller);
        let vault_client = ReceiptVaultClient::new(env, vault);
        Self::authorize_controller_subcall(
            env,
            vault,
            "withdraw",
            (controller.clone(), ptokens).into_val(env),
        );
        Self::begin_margin_withdraw_if_supported(env, vault, &controller, &controller, ptokens);
        vault_client.withdraw(&controller, &ptokens);
        let bal_after = token_client.balance(&controller);
        if bal_after <= bal_before {
            0u128
        } else {
            (bal_after - bal_before) as u128
        }
    }

    fn repay_margin_from_controller(
        env: &Env,
        debt_vault: &Address,
        position_id: u64,
        amount: u128,
    ) -> u128 {
        if amount == 0 {
            return 0u128;
        }
        let controller = env.current_contract_address();
        let debt_vault_client = ReceiptVaultClient::new(env, debt_vault);
        let underlying = debt_vault_client.get_underlying_token();
        let token_client = token::TokenClient::new(env, &underlying);
        let before = token_client.balance(&controller);
        Self::authorize_controller_margin_repay(
            env,
            debt_vault,
            &underlying,
            &controller,
            position_id,
            amount,
        );
        debt_vault_client.repay_for_margin(&position_id, &controller, &amount);
        let after = token_client.balance(&controller);
        if before <= after {
            0u128
        } else {
            (before - after) as u128
        }
    }

    fn credit_margin_underlying(env: &Env, user: &Address, vault: &Address, amount: u128) {
        if amount == 0 {
            return;
        }
        let controller = env.current_contract_address();
        let vault_client = ReceiptVaultClient::new(env, vault);
        let p_before = vault_client.get_ptoken_balance(&controller);
        let underlying = vault_client.get_underlying_token();
        Self::authorize_controller_vault_deposit(env, vault, &underlying, &controller, amount);
        vault_client.deposit(&controller, &amount);
        let p_after = vault_client.get_ptoken_balance(&controller);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta > 0 {
            accrue_user_fee(env, user, vault);
            let free = get_margin_balance_ptokens(env, user, vault);
            set_margin_balance_ptokens(env, user, vault, free.saturating_add(p_delta));
            update_total_margin_ptokens(env, vault, p_delta, true);
        }
    }

    fn release_open_position_ptokens(
        env: &Env,
        position_id: u64,
        position: &Position,
        vaults: &PositionVaults,
    ) {
        if position.collateral_ptokens == 0 {
            panic!("no collateral");
        }
        accrue_user_fee(env, &position.owner, &vaults.position_vault);
        let free = get_margin_balance_ptokens(env, &position.owner, &vaults.position_vault);
        let released = free
            .checked_add(position.collateral_ptokens)
            .expect("margin balance overflow");
        set_margin_balance_ptokens(env, &position.owner, &vaults.position_vault, released);
        update_total_margin_ptokens(
            env,
            &vaults.position_vault,
            position.collateral_ptokens,
            true,
        );
        PositionReleased {
            owner: position.owner.clone(),
            position_id,
        }
        .publish(env);
        clear_perps_v3_position_storage(env, position_id);
        remove_user_position(env, &position.owner, position_id);
    }

    fn release_closing_position_assets(
        env: &Env,
        position_id: u64,
        position: &Position,
        vaults: &PositionVaults,
        pending: &PendingPerpsClose,
    ) {
        let was_swapped = pending.debt_amount > 0;
        if was_swapped {
            if position.side == PositionSide::Short {
                Self::transfer_controller_underlying(
                    env,
                    &position.debt_asset,
                    &position.owner,
                    pending.received_debt_asset,
                );
            } else {
                Self::credit_margin_underlying(
                    env,
                    &position.owner,
                    &vaults.debt_vault,
                    pending.received_debt_asset,
                );
            }
            Self::transfer_controller_underlying(
                env,
                &position.collateral_asset,
                &position.owner,
                get_pending_perps_close_remainder(env, position_id),
            );
        } else {
            Self::credit_margin_underlying(
                env,
                &position.owner,
                &vaults.position_vault,
                pending.collateral_underlying,
            );
        }
        PositionReleased {
            owner: position.owner.clone(),
            position_id,
        }
        .publish(env);
        clear_pending_perps_close(env, position_id);
        clear_perps_v3_position_storage(env, position_id);
        remove_user_position(env, &position.owner, position_id);
    }

    fn close_perps_position(
        env: &Env,
        user: &Address,
        position_id: u64,
        position: &Position,
        perps: &PerpsPositionData,
        amount_with_slippage: u128,
        liquidator: Option<Address>,
    ) {
        let vaults = get_position_vaults(env, position_id, position);
        let debt_vault_client = ReceiptVaultClient::new(env, &vaults.debt_vault);
        debt_vault_client.update_interest();
        let debt_amount = debt_vault_client.get_margin_borrow_balance(&position_id);
        if debt_amount == 0 {
            if liquidator.is_some() {
                panic!("zero debt");
            }
            Self::release_open_position_ptokens(env, position_id, position, &vaults);
            return;
        }
        if position.collateral_asset != position.debt_asset {
            Self::validate_direct_pool(
                env,
                &perps.pool_tokens,
                &perps.pool_id,
                &perps.pool,
                &position.collateral_asset,
                &position.debt_asset,
            );
        }
        let collateral_underlying = Self::withdraw_position_underlying(
            env,
            &vaults.position_vault,
            &position.collateral_asset,
            position.collateral_ptokens,
        );
        if collateral_underlying == 0 {
            panic!("no collateral");
        }
        let execution_config = get_perps_pair_execution_config_or_default(
            env,
            &perps.margin_asset,
            &perps.base_asset,
            &perps.side,
        );
        let exit_config = get_perps_pair_exit_config_or_default(
            env,
            &perps.margin_asset,
            &perps.base_asset,
            &perps.side,
        );
        let pool_min = if position.collateral_asset == position.debt_asset {
            collateral_underlying
        } else {
            let quote = Self::estimate_direct_pool_swap(
                env,
                &perps.pool_tokens,
                &perps.pool,
                &position.collateral_asset,
                &position.debt_asset,
                collateral_underlying,
            );
            Self::assert_pool_quote_above_oracle_floor(
                env,
                quote,
                &position.collateral_asset,
                &position.debt_asset,
                collateral_underlying,
                if liquidator.is_some() {
                    exit_config.max_liq_deviation_scaled
                } else {
                    exit_config.max_close_deviation_scaled
                },
            );
            let slippage = if liquidator.is_some() {
                execution_config.liquidation_slippage_scaled
            } else {
                execution_config.close_slippage_scaled
            };
            Self::pool_quote_min_out(quote, slippage)
        };
        let min_out = if liquidator.is_some() {
            pool_min
        } else {
            if amount_with_slippage < pool_min || amount_with_slippage < debt_amount {
                panic!("slippage too high");
            }
            amount_with_slippage
        };
        let received_debt_asset = if position.collateral_asset == position.debt_asset {
            collateral_underlying
        } else {
            Self::direct_pool_swap_as_controller(
                env,
                &perps.pool_tokens,
                &perps.pool,
                &position.collateral_asset,
                &position.debt_asset,
                collateral_underlying,
                min_out,
            )
        };
        let repay_amount = received_debt_asset.min(debt_amount);
        Self::repay_margin_from_controller(env, &vaults.debt_vault, position_id, repay_amount);
        let remaining_debt = debt_vault_client.get_margin_borrow_balance(&position_id);
        if remaining_debt > 0 {
            if liquidator.is_none() {
                panic!("debt remains");
            }
            Self::authorize_controller_subcall(
                env,
                &vaults.debt_vault,
                "absorb_margin_bad_debt",
                (position_id,).into_val(env),
            );
            debt_vault_client.absorb_margin_bad_debt(&position_id);
        }
        let mut surplus = received_debt_asset.saturating_sub(repay_amount);
        if let Some(liquidator_addr) = liquidator {
            let incentive = surplus.min(
                debt_amount
                    .checked_mul(perps.liquidation_incentive_scaled)
                    .expect("liq overflow")
                    / SCALE_1E6,
            );
            if incentive > 0 {
                let controller = env.current_contract_address();
                let incentive_i128: i128 = incentive.try_into().expect("amount too large");
                Self::authorize_controller_subcall(
                    env,
                    &position.debt_asset,
                    "transfer",
                    (controller.clone(), liquidator_addr.clone(), incentive_i128).into_val(env),
                );
                token::TokenClient::new(env, &position.debt_asset).transfer(
                    &controller,
                    &liquidator_addr,
                    &incentive_i128,
                );
                surplus = surplus.saturating_sub(incentive);
            }
        }
        Self::credit_margin_underlying(env, user, &vaults.debt_vault, surplus);
        clear_perps_v3_position_storage(env, position_id);
        remove_user_position(env, user, position_id);
    }

    fn authorize_controller_vault_deposit(
        env: &Env,
        vault: &Address,
        token: &Address,
        controller: &Address,
        amount: u128,
    ) {
        let amount_i128: i128 = amount.try_into().expect("amount too large");
        let transfer_args: Vec<Val> =
            (controller.clone(), vault.clone(), amount_i128).into_val(env);
        let mut nested = Vec::new(env);
        nested.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: transfer_args.clone(),
            },
            sub_invocations: Vec::new(env),
        }));
        let mut auths = Vec::new(env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: transfer_args,
            },
            sub_invocations: Vec::new(env),
        }));
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: vault.clone(),
                fn_name: Symbol::new(env, "deposit"),
                args: (controller.clone(), amount).into_val(env),
            },
            sub_invocations: nested,
        }));
        env.authorize_as_current_contract(auths);
    }

    pub(crate) fn authorize_controller_margin_repay(
        env: &Env,
        vault: &Address,
        token: &Address,
        controller: &Address,
        position_id: u64,
        amount: u128,
    ) {
        let amount_i128: i128 = amount.try_into().expect("amount too large");
        let transfer_args: Vec<Val> =
            (controller.clone(), vault.clone(), amount_i128).into_val(env);
        let mut nested = Vec::new(env);
        nested.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: transfer_args.clone(),
            },
            sub_invocations: Vec::new(env),
        }));
        let repay_args = (position_id, controller.clone(), amount).into_val(env);
        let mut auths = Vec::new(env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: transfer_args,
            },
            sub_invocations: Vec::new(env),
        }));
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: vault.clone(),
                fn_name: Symbol::new(env, "repay_for_margin"),
                args: repay_args,
            },
            sub_invocations: nested,
        }));
        env.authorize_as_current_contract(auths);
    }

    fn authorize_controller_pool_swap(
        env: &Env,
        pool: &Address,
        token_in: &Address,
        controller: &Address,
        in_idx: u32,
        out_idx: u32,
        amount_in: u128,
        amount_out_min: u128,
    ) {
        let amount_i128: i128 = amount_in.try_into().expect("amount too large");
        let mut nested = Vec::new(env);
        nested.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token_in.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: (controller.clone(), pool.clone(), amount_i128).into_val(env),
            },
            sub_invocations: Vec::new(env),
        }));
        let mut auths = Vec::new(env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: token_in.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: (controller.clone(), pool.clone(), amount_i128).into_val(env),
            },
            sub_invocations: Vec::new(env),
        }));
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: pool.clone(),
                fn_name: Symbol::new(env, "swap"),
                args: (
                    controller.clone(),
                    in_idx,
                    out_idx,
                    amount_in,
                    amount_out_min,
                )
                    .into_val(env),
            },
            sub_invocations: nested,
        }));
        env.authorize_as_current_contract(auths);
    }
}
