//! Native V3 and real receipt principal-settlement tests. Oracle/token/pool are
//! synthetic fixtures. All setup and settlement account auth is scoped; no
//! controller or vault invoker authorization is mocked. This is not venue proof.
use super::*;
use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};

fn auth_call<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    account: &Address,
    contract: &Address,
    name: &str,
    args: Vec<soroban_sdk::Val>,
    children: &[MockAuthInvoke],
) -> T {
    env.mock_auths(&[MockAuth {
        address: account,
        invoke: &MockAuthInvoke {
            contract,
            fn_name: name,
            args: args.clone(),
            sub_invokes: children,
        },
    }]);
    env.invoke_contract(contract, &Symbol::new(env, name), args)
}

fn measure<T>(env: &Env, label: &str, succeeds: bool, invoke: impl FnOnce() -> T) -> T {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(invoke));
    let r = env.cost_estimate().resources();
    let line = std::format!(
        concat!(
            "{{\"flow\":\"{label}\",\"ok\":{ok},\"instructions\":{},\"memory_bytes\":{},",
            "\"read_entries\":{},\"write_entries\":{},\"read_bytes\":{},\"write_bytes\":{},",
            "\"disk_read_entries\":{},\"contract_events_size_bytes\":{},",
            "\"persistent_rent_ledger_bytes\":{},\"persistent_entry_rent_bumps\":{},",
            "\"temporary_rent_ledger_bytes\":{},\"temporary_entry_rent_bumps\":{},",
            "\"scope\":\"native-funded-synthetic-prestate\"}}\n"
        ),
        r.instructions,
        r.mem_bytes,
        r.memory_read_entries,
        r.write_entries,
        r.disk_read_bytes,
        r.write_bytes,
        r.disk_read_entries,
        r.contract_events_size_bytes,
        r.persistent_rent_ledger_bytes,
        r.persistent_entry_rent_bumps,
        r.temporary_rent_ledger_bytes,
        r.temporary_entry_rent_bumps,
        label = label,
        ok = result.is_ok() && succeeds
    );
    std::println!("{line}");
    if let Ok(path) = std::env::var("CL_RESOURCE_REPORT") {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap()
            .write_all(line.as_bytes())
            .unwrap();
    }
    match result {
        Ok(v) => v,
        Err(e) => std::panic::resume_unwind(e),
    }
}

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
        let env = Env::default();
        env.ledger().with_mut(|l| l.timestamp = 1);
        env.cost_estimate().budget().reset_unlimited();
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let debt_token = env.register(MockToken, ());
        let collateral_token = env.register(MockToken, ());
        for (t, name) in [(&debt_token, "DEBT"), (&collateral_token, "COLL")] {
            MockTokenClient::new(&env, t).initialize(
                &name.into_val(&env),
                &name.into_val(&env),
                &7,
            );
            MockTokenClient::new(&env, t).mint(&admin, &1_000_000);
        }
        MockTokenClient::new(&env, &debt_token).mint(&owner, &1_000_000);
        let debt_vault = env.register(ReceiptVault, ());
        let collateral_vault = env.register(ReceiptVault, ());
        for (v, t) in [
            (&debt_vault, &debt_token),
            (&collateral_vault, &collateral_token),
        ] {
            auth_call::<()>(
                &env,
                &admin,
                v,
                "initialize",
                (t.clone(), 0u128, 0u128, admin.clone()).into_val(&env),
                &[],
            );
            auth_call::<()>(
                &env,
                &admin,
                v,
                "enable_static_rates",
                (admin.clone(),).into_val(&env),
                &[],
            );
        }
        let oracle = env.register(MockPeridottroller, ());
        for t in [&debt_token, &collateral_token] {
            MockPeridottrollerClient::new(&env, &oracle).set_price(t, &SCALE_1E6, &SCALE_1E6);
        }
        let adapter = env.register(MockSwapAdapter, ());
        let controller = env.register(MarginController, ());
        auth_call::<()>(
            &env,
            &admin,
            &controller,
            "initialize",
            (admin.clone(), oracle.clone(), adapter, 2u128).into_val(&env),
            &[],
        );
        for (v, t) in [
            (&debt_vault, &debt_token),
            (&collateral_vault, &collateral_token),
        ] {
            auth_call::<()>(
                &env,
                &admin,
                &controller,
                "set_market",
                (admin.clone(), t.clone(), v.clone()).into_val(&env),
                &[],
            );
            auth_call::<()>(
                &env,
                &admin,
                v,
                "set_margin_controller",
                (admin.clone(), Some(controller.clone())).into_val(&env),
                &[],
            );
            let transfer = MockAuthInvoke {
                contract: t,
                fn_name: "transfer",
                args: (admin.clone(), v.clone(), 500_000i128).into_val(&env),
                sub_invokes: &[],
            };
            auth_call::<()>(
                &env,
                &admin,
                v,
                "deposit",
                (admin.clone(), 500_000u128).into_val(&env),
                &[transfer],
            );
        }
        let transfer = MockAuthInvoke {
            contract: &debt_token,
            fn_name: "transfer",
            args: (owner.clone(), debt_vault.clone(), 500i128).into_val(&env),
            sub_invokes: &[],
        };
        auth_call::<()>(
            &env,
            &owner,
            &debt_vault,
            "deposit",
            (owner.clone(), 500u128).into_val(&env),
            &[transfer],
        );
        let transfer = MockAuthInvoke {
            contract: &debt_vault,
            fn_name: "transfer",
            args: (owner.clone(), controller.clone(), 500i128).into_val(&env),
            sub_invokes: &[],
        };
        auth_call::<()>(
            &env,
            &owner,
            &controller,
            "transfer_spot_to_margin",
            (owner.clone(), debt_token.clone(), 500u128).into_val(&env),
            &[transfer],
        );
        // Synthetic active-position prestate: native begin currently exceeds
        // the SDK's 100-entry gate (136 entries); the compiled open harness is
        // separate. Do not disable limits or present this as an open proof.
        let position_id = 1u64;
        auth_call::<()>(
            &env,
            &admin,
            &collateral_vault,
            "transfer",
            (admin.clone(), controller.clone(), 1_000i128).into_val(&env),
            &[],
        );
        env.as_contract(&debt_vault, || {
            use receipt_vault::DataKey as R;
            let p = env.storage().persistent();
            let index: u128 = p.get(&R::BorrowIndex).unwrap();
            p.set(
                &R::MarginBorrowSnapshots(position_id),
                &receipt_vault::BorrowSnapshot {
                    principal: 500,
                    interest_index: index,
                },
            );
            p.set(&R::MarginBorrowPrincipal(position_id), &500u128);
            p.set(&R::MarginHasBorrowed(position_id), &true);
            p.set(&R::TotalBorrowed, &500u128);
            p.set(&R::TotalBorrowPrincipal, &500u128);
            let cash: u128 = p.get(&R::ManagedCash).unwrap();
            p.set(&R::ManagedCash, &(cash - 500));
        });
        env.as_contract(&controller, || {
            let p = env.storage().persistent();
            p.set(
                &DataKey::Position(position_id),
                &Position {
                    owner: owner.clone(),
                    side: PositionSide::Long,
                    collateral_asset: collateral_token.clone(),
                    debt_asset: debt_token.clone(),
                    collateral_ptokens: 1_000,
                    debt_shares: 0,
                    entry_price_scaled: SCALE_1E6,
                    opened_at: 1,
                    status: PositionStatus::Open,
                },
            );
            p.set(&DataKey::PositionMode(position_id), &PositionMode::PerpsV3);
            p.set(&DataKey::PositionCollateralVault(position_id), &debt_vault);
            p.set(&DataKey::PositionDebtVault(position_id), &debt_vault);
            p.set(
                &DataKey::PositionPositionVault(position_id),
                &collateral_vault,
            );
            p.set(
                &DataKey::PerpsPositionData(position_id),
                &PerpsPositionData {
                    margin_asset: debt_token.clone(),
                    base_asset: collateral_token.clone(),
                    side: PositionSide::Long,
                    margin_amount: 500,
                    notional_value: 1_000,
                    maintenance_margin_scaled: 50_000,
                    liquidation_incentive_scaled: 10_000,
                    pool_tokens: Vec::from_array(
                        &env,
                        [debt_token.clone(), collateral_token.clone()],
                    ),
                    pool_id: BytesN::from_array(&env, &[9; 32]),
                    pool: Address::generate(&env),
                },
            );
        });
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
    let result = measure(&f.env, "funded_settlement", true, || {
        client.liquidate_funded_v3(&f.liquidator, &f.position_id, &550, &971)
    });
    assert_eq!(result, (500, 971));
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
    assert!(measure(&f.env, "funded_missing_root_auth", false, || client
        .try_liquidate_funded_v3(&f.liquidator, &f.position_id, &550, &971))
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
        measure(&f.env, "funded_capped_settlement", true, || client
            .liquidate_funded_v3(
                &f.liquidator,
                &f.position_id,
                &550,
                &1_000
            )),
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
    assert!(
        measure(&f.env, "funded_missing_nested_auth", false, || client
            .try_liquidate_funded_v3(
                &f.liquidator,
                &f.position_id,
                &550,
                &971
            ))
        .is_err()
    );
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
    assert!(measure(&f.env, "funded_failed_price", false, || client
        .try_liquidate_funded_v3(&f.liquidator, &f.position_id, &550, &971))
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
    assert!(measure(&f.env, "funded_seizure_rollback", false, || client
        .try_liquidate_funded_v3(&f.liquidator, &f.position_id, &550, &1_942))
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
        assert!(measure(&f.env, "funded_guard_rejection", false, || client
            .try_liquidate_funded_v3(&f.liquidator, &f.position_id, &max, &min))
        .is_err());
        assert_eq!(f.debt(), 500);
        assert_eq!(f.funds(), 1_000);
    }
}

#[test]
fn funded_prototype_rejects_mainnet_validation_network() {
    let f = Fixture::new();
    let network = f
        .env
        .crypto()
        .sha256(&Bytes::from_slice(
            &f.env,
            b"Public Global Stellar Network ; September 2015",
        ))
        .to_array();
    f.env.ledger().with_mut(|l| l.network_id = network);
    f.authorize(550, 971, true);
    assert!(measure(&f.env, "funded_mainnet_rejection", false, || {
        MarginControllerClient::new(&f.env, &f.controller).try_liquidate_funded_v3(
            &f.liquidator,
            &f.position_id,
            &550,
            &971,
        )
    })
    .is_err());
    assert_eq!(f.debt(), 500);
    assert_eq!(f.funds(), 1_000);
}

#[cfg(feature = "funded-liquidation-validation")]
#[test]
fn funded_prototype_pending_recovery_rejects_wrong_owner_and_live_position() {
    for wrong_owner in [true, false] {
        let f = Fixture::new();
        let caller = if wrong_owner {
            f.liquidator.clone()
        } else {
            f.owner.clone()
        };
        f.env.mock_auths(&[MockAuth {
            address: &caller,
            invoke: &MockAuthInvoke {
                contract: &f.controller,
                fn_name: "recover_pending_open_funded",
                args: (caller.clone(), f.position_id, 550u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }]);
        let client = MarginControllerClient::new(&f.env, &f.controller);
        assert!(
            measure(&f.env, "funded_recovery_lifecycle_rejection", false, || {
                client.try_recover_pending_open_funded(&caller, &f.position_id, &550)
            })
            .is_err()
        );
        assert_eq!(f.debt(), 500);
        assert_eq!(f.funds(), 1_000);
    }
}
