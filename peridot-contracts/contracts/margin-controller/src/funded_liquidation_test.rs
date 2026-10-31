//! Principal-only native tests. Fixture setup uses the existing mock-auth setup;
//! every funded liquidation clears it and authorizes only the liquidator's exact
//! root/repay/token-transfer tree. These are not compiled pool or LP reward tests.
use super::*;
use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};

struct Fixture {
    env: Env,
    controller: Address,
    debt_token: Address,
    collateral_token: Address,
    debt_vault: Address,
    collateral_vault: Address,
    oracle: Address,
    owner: Address,
    liquidator: Address,
    position_id: u64,
}

impl Fixture {
    fn new() -> Self {
        let (
            env,
            controller,
            debt_token,
            collateral_token,
            owner,
            oracle,
            debt_vault,
            collateral_vault,
        ) = setup_min_with_vaults();
        env.cost_estimate().disable_resource_limits();
        env.cost_estimate().budget().reset_unlimited();
        let client = MarginControllerClient::new(&env, &controller);
        // Test-only 2x and the fixture's default 5% maintenance, not launch params.
        let (position_id, _, _, _) = open_perps_long_with_leverage(
            &env,
            &client,
            &owner,
            &debt_token,
            &collateral_token,
            &debt_vault,
            500u128,
            2u128,
        );
        MockPeridottrollerClient::new(&env, &oracle).set_price(
            &collateral_token,
            &520_000u128,
            &SCALE_1E6,
        );
        let liquidator = Address::generate(&env);
        MockTokenClient::new(&env, &debt_token).mint(&liquidator, &1_000i128);
        // No pool is bound at this address: any attempt to quote/swap must fail.
        env.as_contract(&controller, || {
            let key = DataKey::PerpsPositionData(position_id);
            let mut data: PerpsPositionData = env.storage().persistent().get(&key).unwrap();
            data.pool = Address::generate(&env);
            env.storage().persistent().set(&key, &data);
        });
        env.set_auths(&[]);
        Self {
            env,
            controller,
            debt_token,
            collateral_token,
            debt_vault,
            collateral_vault,
            oracle,
            owner,
            liquidator,
            position_id,
        }
    }

    fn authorize(&self, max_repay: u128, min_seize: u128, include_transfer: bool) {
        let transfer = MockAuthInvoke {
            contract: &self.debt_token,
            fn_name: "transfer",
            args: (
                self.liquidator.clone(),
                self.debt_vault.clone(),
                max_repay as i128,
            )
                .into_val(&self.env),
            sub_invokes: &[],
        };
        let transfers = [transfer];
        let repay = MockAuthInvoke {
            contract: &self.debt_vault,
            fn_name: "repay_full_for_margin",
            args: (self.position_id, self.liquidator.clone(), max_repay).into_val(&self.env),
            sub_invokes: if include_transfer { &transfers } else { &[] },
        };
        self.env.mock_auths(&[MockAuth {
            address: &self.liquidator,
            invoke: &MockAuthInvoke {
                contract: &self.controller,
                fn_name: "liquidate_funded_v3",
                args: (
                    self.liquidator.clone(),
                    self.position_id,
                    max_repay,
                    min_seize,
                )
                    .into_val(&self.env),
                sub_invokes: &[repay],
            },
        }]);
    }

    fn debt(&self) -> u128 {
        receipt_vault::ReceiptVaultClient::new(&self.env, &self.debt_vault)
            .get_margin_borrow_balance(&self.position_id)
    }

    fn funds(&self) -> i128 {
        MockTokenClient::new(&self.env, &self.debt_token).balance(&self.liquidator)
    }
}

#[test]
fn funded_prototype_repays_from_liquidator_without_swap_and_returns_surplus() {
    let f = Fixture::new();
    let client = MarginControllerClient::new(&f.env, &f.controller);
    assert_eq!(f.debt(), 500);
    MockTokenClient::new(&f.env, &f.debt_token).mint(&f.controller, &111i128);
    f.authorize(550, 971, true);
    assert_eq!(
        client.liquidate_funded_v3(&f.liquidator, &f.position_id, &550, &971),
        (500, 971)
    );
    assert_eq!(f.debt(), 0);
    assert_eq!(f.funds(), 500); // 550 paid, 50 refunded.
    assert_eq!(
        MockTokenClient::new(&f.env, &f.debt_token).balance(&f.controller),
        111
    );
    let collateral = receipt_vault::ReceiptVaultClient::new(&f.env, &f.collateral_vault);
    assert_eq!(collateral.get_ptoken_balance(&f.liquidator), 971);
    assert_eq!(
        client.get_margin_balance_ptokens(&f.owner, &f.collateral_token),
        29
    );
    assert_eq!(collateral.get_ptoken_balance(&f.controller), 29);
    assert!(client.get_position(&f.position_id).is_none());
}

#[test]
fn funded_prototype_missing_root_auth_cannot_spend_or_seize() {
    let f = Fixture::new();
    let client = MarginControllerClient::new(&f.env, &f.controller);
    assert!(client
        .try_liquidate_funded_v3(&f.liquidator, &f.position_id, &550, &971)
        .is_err());
    assert_eq!(f.debt(), 500);
    assert_eq!(f.funds(), 1_000);
}

#[test]
fn funded_prototype_caps_seizure_to_position_collateral() {
    let f = Fixture::new();
    MockPeridottrollerClient::new(&f.env, &f.oracle).set_price(
        &f.collateral_token,
        &400_000u128,
        &SCALE_1E6,
    );
    // The liquidator deliberately funds all debt despite the collateral gap.
    // No subsidy or implicit bad-debt absorption makes this economically free.
    f.authorize(550, 1_000, true);
    let client = MarginControllerClient::new(&f.env, &f.controller);
    assert_eq!(
        client.liquidate_funded_v3(&f.liquidator, &f.position_id, &550, &1_000),
        (500, 1_000)
    );
    assert_eq!(f.debt(), 0);
    assert_eq!(f.funds(), 500);
    assert_eq!(
        client.get_margin_balance_ptokens(&f.owner, &f.collateral_token),
        0
    );
}

#[test]
fn funded_prototype_missing_nested_transfer_auth_rolls_back() {
    let f = Fixture::new();
    f.authorize(550, 971, false);
    let client = MarginControllerClient::new(&f.env, &f.controller);
    assert!(client
        .try_liquidate_funded_v3(&f.liquidator, &f.position_id, &550, &971)
        .is_err());
    assert_eq!(f.debt(), 500);
    assert_eq!(f.funds(), 1_000);
    assert!(client.get_position(&f.position_id).is_some());
}

#[test]
fn funded_prototype_failed_fresh_price_does_not_use_positive_cache() {
    let f = Fixture::new();
    MockPeridottrollerClient::new(&f.env, &f.oracle).set_cache_price_should_panic(&true);
    f.authorize(550, 971, true);
    let client = MarginControllerClient::new(&f.env, &f.controller);
    assert!(client
        .try_liquidate_funded_v3(&f.liquidator, &f.position_id, &550, &971)
        .is_err());
    assert_eq!(f.debt(), 500);
    assert_eq!(f.funds(), 1_000);
}

#[test]
fn funded_prototype_seizure_failure_rolls_back_successful_repayment() {
    let f = Fixture::new();
    // Fault injection: ledger claims more pTokens than custody actually holds.
    // Seizure will fail in the real receipt vault after debt repayment succeeds.
    f.env.as_contract(&f.controller, || {
        let key = DataKey::Position(f.position_id);
        let mut position: Position = f.env.storage().persistent().get(&key).unwrap();
        position.collateral_ptokens = 2_000;
        f.env.storage().persistent().set(&key, &position);
    });
    MockPeridottrollerClient::new(&f.env, &f.oracle).set_price(
        &f.collateral_token,
        &260_000u128,
        &SCALE_1E6,
    );
    f.authorize(550, 1_942, true);
    let client = MarginControllerClient::new(&f.env, &f.controller);
    assert!(client
        .try_liquidate_funded_v3(&f.liquidator, &f.position_id, &550, &1_942)
        .is_err());
    assert_eq!(f.debt(), 500);
    assert_eq!(f.funds(), 1_000);
    assert!(client.get_position(&f.position_id).is_some());
    assert_eq!(
        receipt_vault::ReceiptVaultClient::new(&f.env, &f.collateral_vault)
            .get_ptoken_balance(&f.liquidator),
        0
    );
}

#[test]
fn funded_prototype_guards_health_payment_minimum_and_pause() {
    for case in 0..5 {
        let f = Fixture::new();
        let (max, min) = match case {
            0 => (499, 971),
            1 => (550, 972),
            _ => (550, 971),
        };
        if case == 2 {
            MockPeridottrollerClient::new(&f.env, &f.oracle).set_price(
                &f.collateral_token,
                &SCALE_1E6,
                &SCALE_1E6,
            );
        } else if case == 3 {
            MockPeridottrollerClient::new(&f.env, &f.oracle)
                .set_liquidation_paused(&f.collateral_vault, &true);
        } else if case == 4 {
            MockPeridottrollerClient::new(&f.env, &f.oracle).set_price(
                &f.collateral_token,
                &0u128,
                &SCALE_1E6,
            );
        }
        f.authorize(max, min, true);
        let client = MarginControllerClient::new(&f.env, &f.controller);
        assert!(client
            .try_liquidate_funded_v3(&f.liquidator, &f.position_id, &max, &min)
            .is_err());
        assert_eq!(f.debt(), 500);
        assert_eq!(f.funds(), 1_000);
    }
}
