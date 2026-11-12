//! Owner-funded recovery of a swapped pending V3 position. Full debt repayment
//! and return of the entire position asset are atomic, independent of borrowing
//! pauses and reserve headroom. No pending position can escape with live debt.
//! Long returns purchased base; Short returns original margin plus sale proceeds.
use crate::{contract::MarginController, fees::get_fee_terms, helpers::*, storage::*};
use soroban_sdk::{Address, Env, IntoVal};

impl MarginController {
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
        // A validation controller must never be downgraded into a token-only
        // recovery that omits persisted CL beneficiary/custody hooks.
        #[cfg(not(feature = "funded-liquidation-validation"))]
        assert!(
            !env.storage()
                .instance()
                .get::<_, bool>(&DataKey::ClShareAsset(position.collateral_asset.clone()))
                .unwrap_or(false),
            "CL custody requires validation implementation"
        );
        let pending = get_pending_perps_open_position_or_panic(env, position_id);
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
        #[cfg(feature = "funded-liquidation-validation")]
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
            // Refunded reserved shares were not free-margin liquidity during
            // intervening fee epochs. Settle the old weight before adding them.
            accrue_user_fee(env, &user, &pending.margin_vault);
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
