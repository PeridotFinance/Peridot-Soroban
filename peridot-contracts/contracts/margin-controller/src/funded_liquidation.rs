//! Adapted from PR74 checkpoint 8375723a, with explicit local WASM validation.
//! No pool/facade/swap/strategy redemption or flash loan. Default production
//! builds omit this route; validation builds reject Mainnet. Principal pToken
//! delivery alone does not clear the LP beneficial-owner reward activation gate.
use soroban_sdk::{contractevent, Address, Bytes, Env, IntoVal};

use crate::constants::SCALE_1E6;
use crate::contract::MarginController;
use crate::helpers::*;
use crate::storage::*;

#[contractevent(topics = ["funded_liquidation"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundedLiquidationSettled {
    #[topic]
    pub position_id: u64,
    #[topic]
    pub liquidator: Address,
    pub owner: Address,
    pub repaid: u128,
    pub seized_ptokens: u128,
    pub returned_ptokens: u128,
}

pub(crate) fn require_validation_network(env: &Env) {
    let public = env.crypto().sha256(&Bytes::from_slice(
        env,
        b"Public Global Stellar Network ; September 2015",
    ));
    assert_ne!(
        env.ledger().network_id(),
        public.to_bytes(),
        "funded validation: Mainnet disabled"
    );
}

fn value(amount: u128, price: (u128, u128)) -> u128 {
    assert!(price.0 > 0 && price.1 > 0, "invalid price");
    amount.checked_mul(price.0).expect("valuation overflow") / price.1
}

impl MarginController {
    pub(crate) fn liquidate_funded_v3_impl(
        env: &Env,
        liquidator: Address,
        position_id: u64,
        max_repay: u128,
        min_seize_ptokens: u128,
    ) -> (u128, u128) {
        assert!(
            max_repay > 0 && min_seize_ptokens > 0,
            "invalid liquidation bounds"
        );
        let position = get_position_record_or_panic(env, position_id);
        assert!(
            get_position_mode(env, position_id) == PositionMode::PerpsV3,
            "not v3 position"
        );
        assert!(position.status == PositionStatus::Open, "not open");
        assert!(liquidator != position.owner, "self liquidation");
        assert!(position.collateral_ptokens > 0, "no collateral");
        let perps = get_perps_position_data_or_panic(env, position_id);
        let vaults = get_position_vaults(env, position_id, &position);
        Self::assert_margin_lock_configured(env, &vaults.position_vault);
        Self::assert_margin_lock_configured(env, &vaults.debt_vault);
        let controller_policy = get_peridottroller(env);
        assert!(
            !controller_policy.is_liquidation_paused(&vaults.position_vault)
                && !controller_policy.is_liquidation_paused(&vaults.debt_vault),
            "liquidation paused"
        );

        let debt_vault = ReceiptVaultClient::new(env, &vaults.debt_vault);
        let position_vault = ReceiptVaultClient::new(env, &vaults.position_vault);
        debt_vault.update_interest();
        position_vault.update_interest();
        let debt = debt_vault.get_margin_borrow_balance(&position_id);
        assert!(debt > 0, "zero debt");
        assert!(debt <= max_repay, "max repay too small");
        // A failed fresh read must not fall back to a cached valuation. Share NAV
        // validity still needs its full product integration before LP activation.
        let debt_price = refresh_price_usd(env, &position.debt_asset);
        let collateral_price = refresh_price_usd(env, &position.collateral_asset);
        let exchange_rate = position_vault.get_exchange_rate();
        assert!(exchange_rate > 0, "invalid exchange rate");
        let collateral_underlying = position
            .collateral_ptokens
            .checked_mul(exchange_rate)
            .expect("collateral overflow")
            / SCALE_1E6;
        let debt_value = value(debt, debt_price);
        let collateral_value = value(collateral_underlying, collateral_price);
        let notional = match perps.side {
            PositionSide::Long => collateral_value,
            PositionSide::Short => debt_value,
        };
        let maintenance = notional
            .checked_mul(perps.maintenance_margin_scaled)
            .expect("maintenance overflow")
            / SCALE_1E6;
        assert!(
            collateral_value.saturating_sub(debt_value) <= maintenance,
            "not liquidatable"
        );

        let seize_value = debt_value
            .checked_mul(
                SCALE_1E6
                    .checked_add(perps.liquidation_incentive_scaled)
                    .expect("incentive overflow"),
            )
            .expect("seizure overflow")
            / SCALE_1E6;
        // Round seizure down at every conversion; never round against the
        // borrower. The liquidator accepts the exact minimum in the auth tree.
        let seize_underlying = seize_value
            .checked_mul(collateral_price.1)
            .expect("seizure overflow")
            / collateral_price.0;
        let seized = (seize_underlying
            .checked_mul(SCALE_1E6)
            .expect("seizure overflow")
            / exchange_rate)
            .min(position.collateral_ptokens);
        assert!(seized >= min_seize_ptokens, "seizure below minimum");
        let returned = position.collateral_ptokens - seized;

        // The liquidator authorizes a fixed transfer of max_repay to the debt
        // vault. That vault refunds the surplus, so interest drift never changes
        // the signed transfer amount. No controller cash is used.
        let paid = debt_vault.repay_full_for_margin(&position_id, &liquidator, &max_repay);
        assert!(paid == debt, "unexpected repayment");
        assert!(
            debt_vault.get_margin_borrow_balance(&position_id) == 0,
            "debt remains"
        );

        let controller = env.current_contract_address();
        let amount: i128 = seized.try_into().expect("seizure too large");
        let before = position_vault.get_ptoken_balance(&liquidator);
        #[cfg(feature = "funded-liquidation-validation")]
        Self::cl_receipt_reassign(
            env,
            &position.collateral_asset,
            &vaults.position_vault,
            &position.owner,
            &liquidator,
            seized,
            true,
        );
        Self::begin_margin_withdraw_if_supported(
            env,
            &vaults.position_vault,
            &controller,
            &liquidator,
            seized,
        );
        Self::authorize_controller_subcall(
            env,
            &vaults.position_vault,
            "transfer",
            (controller.clone(), liquidator.clone(), amount).into_val(env),
        );
        position_vault.transfer(&controller, &liquidator, &amount);
        assert!(
            position_vault
                .get_ptoken_balance(&liquidator)
                .checked_sub(before)
                .expect("seizure balance decreased")
                == seized,
            "seizure transfer shortfall"
        );

        // All residual collateral belongs to the borrower, including any amount
        // above debt+bonus. Other positions and pre-existing free margin stay put.
        if returned > 0 {
            accrue_user_fee(env, &position.owner, &vaults.position_vault);
            let free = get_margin_balance_ptokens(env, &position.owner, &vaults.position_vault);
            set_margin_balance_ptokens(
                env,
                &position.owner,
                &vaults.position_vault,
                free.checked_add(returned).expect("margin balance overflow"),
            );
            update_total_margin_ptokens(env, &vaults.position_vault, returned, true);
        }
        FundedLiquidationSettled {
            position_id,
            liquidator,
            owner: position.owner.clone(),
            repaid: paid,
            seized_ptokens: seized,
            returned_ptokens: returned,
        }
        .publish(env);
        clear_perps_v3_position_storage(env, position_id);
        remove_user_position(env, &position.owner, position_id);
        (paid, seized)
    }
}
