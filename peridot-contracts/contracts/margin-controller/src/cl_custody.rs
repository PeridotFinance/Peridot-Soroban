//! Local validation-only custody bridge. Explicit asset registration avoids
//! treating failed custody calls as optional or silently losing ownership.
use crate::{contract::MarginController, fees::get_fee_terms, helpers::*, storage::*};
use soroban_sdk::{Address, Env, IntoVal, Symbol};

impl MarginController {
    pub(crate) fn cl_enabled(env: &Env, asset: &Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::ClShareAsset(asset.clone()))
            .unwrap_or(false)
    }
    pub(crate) fn cl_assign(
        env: &Env,
        asset: &Address,
        owner: &Address,
        amount: u128,
        recovery: bool,
    ) {
        if !Self::cl_enabled(env, asset) {
            return;
        }
        let name = if recovery {
            "custody_recover_assign"
        } else {
            "custody_assign"
        };
        env.invoke_contract::<()>(
            asset,
            &Symbol::new(env, name),
            (env.current_contract_address(), owner.clone(), amount).into_val(env),
        );
    }
    pub(crate) fn cl_prepare(
        env: &Env,
        asset: &Address,
        owner: &Address,
        recipient: &Address,
        amount: u128,
        recovery: bool,
    ) {
        if !Self::cl_enabled(env, asset) {
            return;
        }
        env.invoke_contract::<()>(
            asset,
            &Symbol::new(env, "custody_prepare_transfer"),
            (
                env.current_contract_address(),
                owner.clone(),
                recipient.clone(),
                amount,
                recovery,
            )
                .into_val(env),
        );
    }
    pub(crate) fn cl_receipt_assign(
        env: &Env,
        asset: &Address,
        vault: &Address,
        owner: &Address,
        amount: u128,
    ) {
        if Self::cl_enabled(env, asset) {
            env.invoke_contract::<()>(
                vault,
                &Symbol::new(env, "cl_assign_margin_rewards"),
                (owner.clone(), amount).into_val(env),
            );
        }
    }
    pub(crate) fn cl_receipt_prepare(
        env: &Env,
        asset: &Address,
        vault: &Address,
        owner: &Address,
        ptokens: u128,
        recovery: bool,
    ) {
        if Self::cl_enabled(env, asset) {
            env.invoke_contract::<()>(
                vault,
                &Symbol::new(env, "cl_prepare_margin_rewards"),
                (owner.clone(), ptokens, recovery).into_val(env),
            );
        }
    }
    pub(crate) fn cl_receipt_reassign(
        env: &Env,
        asset: &Address,
        vault: &Address,
        owner: &Address,
        recipient: &Address,
        ptokens: u128,
        recovery: bool,
    ) {
        if Self::cl_enabled(env, asset) {
            env.invoke_contract::<()>(
                vault,
                &Symbol::new(env, "cl_reassign_margin_rewards"),
                (owner.clone(), recipient.clone(), ptokens, recovery).into_val(env),
            );
        }
    }
    pub(crate) fn recover_pending_open_funded_impl(
        env: &Env,
        user: Address,
        position_id: u64,
        max_repay: u128,
    ) -> u128 {
        let position = get_position_record_or_panic(env, position_id);
        assert!(position.owner == user, "not owner");
        assert!(
            position.status == PositionStatus::PendingOpen,
            "not pending open"
        );
        assert!(
            get_position_mode(env, position_id) == PositionMode::PerpsV3,
            "not v3"
        );
        let pending = get_pending_perps_open_position_or_panic(env, position_id);
        assert!(pending.side == PositionSide::Long, "recovery long only");
        let execution = get_pending_perps_open_execution_or_panic(env, position_id);
        // Full external debt settlement precedes recovery. It is never a
        // general reward-claim bypass for a live leveraged position.
        let debt = ReceiptVaultClient::new(env, &pending.debt_vault);
        let paid = debt.repay_full_for_margin(&position_id, &user, &max_repay);
        assert!(
            debt.get_margin_borrow_balance(&position_id) == 0,
            "debt remains"
        );
        let controller = env.current_contract_address();
        Self::cl_prepare(
            env,
            &position.collateral_asset,
            &user,
            &user,
            execution.position_amount,
            true,
        );
        let amount: i128 = execution
            .position_amount
            .try_into()
            .expect("amount overflow");
        Self::authorize_controller_subcall(
            env,
            &position.collateral_asset,
            "transfer",
            (controller.clone(), user.clone(), amount).into_val(env),
        );
        soroban_sdk::token::Client::new(env, &position.collateral_asset).transfer(
            &controller,
            &user,
            &amount,
        );
        let refund = get_fee_terms(env, position_id)
            .map(|f| f.open_fee_ptokens)
            .unwrap_or(0);
        if refund > 0 {
            let free = get_margin_balance_ptokens(env, &user, &pending.margin_vault);
            set_margin_balance_ptokens(
                env,
                &user,
                &pending.margin_vault,
                free.checked_add(refund).expect("refund overflow"),
            );
            update_total_margin_ptokens(env, &pending.margin_vault, refund, true);
        }
        clear_perps_v3_position_storage(env, position_id);
        remove_user_position(env, &user, position_id);
        paid
    }
}
