use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{token, Address, BytesN, Env, IntoVal, Symbol, Val, Vec};

use crate::constants::*;
use crate::contract::MarginController;
use crate::helpers::*;
use crate::storage::*;

pub(crate) struct PerpsRiskValues {
    pub(crate) equity: u128,
    pub(crate) maintenance_required: u128,
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
        let min_out_oracle =
            Self::oracle_min_out_from_prices(env, trade_in_price, trade_out_price, trade_amount);
        if amount_with_slippage < min_out_oracle {
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
            Self::swap_open_position_v3_impl(env, user.clone(), position_id);
        }
        Self::activate_open_position_v3_impl(env, user, position_id);
    }

    pub(crate) fn swap_open_position_v3_impl(env: &Env, user: Address, position_id: u64) {
        let (position, pending, vaults) =
            Self::load_pending_perps_open(env, &user, position_id, true);
        if get_pending_perps_open_execution(env, position_id).is_some() {
            panic!("open already swapped");
        }

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

        let position_token = token::TokenClient::new(env, &position.collateral_asset);
        let position_bal_before = position_token.balance(&controller);
        let trade_amount = match pending.side.clone() {
            PositionSide::Long => margin_received.saturating_add(pending.borrow_amount),
            PositionSide::Short => pending.borrow_amount,
        };
        let (token_in, token_out) = match pending.side.clone() {
            PositionSide::Long => (pending.margin_asset.clone(), pending.base_asset.clone()),
            PositionSide::Short => (pending.base_asset.clone(), pending.margin_asset.clone()),
        };
        let reported_received = Self::direct_pool_swap_as_controller(
            env,
            &pending.pool_tokens,
            &pending.pool,
            &token_in,
            &token_out,
            trade_amount,
            pending.min_position_amount,
        );
        if reported_received < pending.min_position_amount {
            panic!("slippage too high");
        }
        let position_bal_after = position_token.balance(&controller);
        let received_from_swap = if position_bal_after <= position_bal_before {
            0u128
        } else {
            (position_bal_after - position_bal_before) as u128
        };
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
        bump_position_ttl(env, position_id);
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
        accrue_user_fee(env, &user, &pending.margin_vault);
        let free = get_margin_balance_ptokens(env, &user, &pending.margin_vault);
        set_margin_balance_ptokens(
            env,
            &user,
            &pending.margin_vault,
            free.saturating_add(pending.margin_ptokens),
        );
        update_total_margin_ptokens(env, &pending.margin_vault, pending.margin_ptokens, true);
        clear_position_storage(env, position_id);
        remove_user_position(env, &user, position_id);
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
        if position.status != PositionStatus::Open {
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

    pub(crate) fn close_position_v3_impl(
        env: &Env,
        user: Address,
        position_id: u64,
        amount_with_slippage: u128,
    ) {
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        let position = get_position_or_panic(env, position_id);
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
        if get_pending_liquidation(env, position_id).is_some() {
            panic!("liquidation pending");
        }
        let mut position = get_position_or_panic(env, position_id);
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        if liquidator == position.owner {
            panic!("self liquidation");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        let perps = get_perps_position_data_or_panic(env, position_id);
        let risk = Self::perps_risk_values(env, position_id, &position, &perps, true, true);
        if risk.equity > risk.maintenance_required {
            panic!("not liquidatable");
        }
        let vaults = get_position_vaults(env, position_id, &position);
        let debt_amount = ReceiptVaultClient::new(env, &vaults.debt_vault)
            .get_margin_borrow_balance(&position_id);
        if debt_amount == 0 {
            panic!("zero debt");
        }
        position.status = PositionStatus::Liquidated;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        bump_position_ttl(env, position_id);
        let pending = PendingLiquidation {
            kind: PendingLiquidationKind::PerpsV3,
            stage: PendingLiquidationStage::Started,
            owner: position.owner,
            liquidator,
            debt_amount,
            repay_amount: 0u128,
            received_debt_asset: 0u128,
            position_seize_ptokens: 0u128,
            position_fee_ptokens: 0u128,
            initial_market: None,
            initial_seize_ptokens: 0u128,
            initial_fee_ptokens: 0u128,
            reserve_recipient: None,
            liquidation_incentive_scaled: perps.liquidation_incentive_scaled,
        };
        set_pending_liquidation(env, position_id, &pending);
    }

    pub(crate) fn swap_liquidation_v3_impl(env: &Env, liquidator: Address, position_id: u64) {
        let mut pending = get_pending_liquidation_or_panic(env, position_id);
        if pending.kind != PendingLiquidationKind::PerpsV3
            || pending.stage != PendingLiquidationStage::Started
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
        if position.owner != pending.owner {
            panic!("pending owner mismatch");
        }
        if get_position_mode(env, position_id) != PositionMode::PerpsV3 {
            panic!("not v3 position");
        }
        let perps = get_perps_position_data_or_panic(env, position_id);
        let vaults = get_position_vaults(env, position_id, &position);
        if position.collateral_ptokens == 0 {
            panic!("no collateral");
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
                1u128,
            )
        };
        if received_debt_asset == 0 {
            panic!("swap output too small");
        }
        position.collateral_ptokens = 0u128;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        bump_position_ttl(env, position_id);
        pending.stage = PendingLiquidationStage::CollateralConverted;
        pending.received_debt_asset = received_debt_asset;
        set_pending_liquidation(env, position_id, &pending);
    }

    pub(crate) fn finish_liquidation_v3_impl(env: &Env, liquidator: Address, position_id: u64) {
        let pending = get_pending_liquidation_or_panic(env, position_id);
        if pending.kind != PendingLiquidationKind::PerpsV3
            || pending.stage != PendingLiquidationStage::CollateralConverted
        {
            panic!("bad liquidation stage");
        }
        if pending.liquidator != liquidator {
            panic!("not liquidator");
        }
        let position = get_position_or_panic(env, position_id);
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
        clear_position_storage(env, position_id);
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
        if !adapter.is_pool_allowed(pool) {
            panic!("pool not allowed");
        }
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
        AquariusPoolClient::new(env, pool).swap(
            &controller,
            &in_idx,
            &out_idx,
            &amount_in,
            &amount_out_min,
        )
    }

    fn raw_ptoken_value_usd_with_update(
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
        let vaults = get_position_vaults(env, position_id, position);
        let debt_vault = ReceiptVaultClient::new(env, &vaults.debt_vault);
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
            &vaults.position_vault,
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
            panic!("zero debt");
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
        let min_out = if liquidator.is_some() {
            1u128
        } else {
            let oracle_min = Self::oracle_min_out(
                env,
                &position.collateral_asset,
                &position.debt_asset,
                collateral_underlying,
            );
            if amount_with_slippage < oracle_min {
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
        clear_position_storage(env, position_id);
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

    fn authorize_controller_margin_repay(
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
