use super::*;

struct Fixture {
    env: Env,
    id: Address,
    usdt: Address,
    xlm: Address,
    user: Address,
    oracle: Address,
    uv: Address,
    xv: Address,
}

impl Fixture {
    fn new(open_bps: u128, close_bps: u128) -> Self {
        let (env, id, usdt, xlm, user, oracle, uv, xv) = setup_min_with_vaults();
        env.cost_estimate().disable_resource_limits();
        env.cost_estimate().budget().reset_unlimited();
        seed_perps_matrix_liquidity(&env, &user, &usdt, &xlm, &uv, &xv);
        let f = Self {
            env,
            id,
            usdt,
            xlm,
            user,
            oracle,
            uv,
            xv,
        };
        let c = f.client();
        c.set_open_fee_bps(&c.get_admin(), &open_bps);
        c.set_close_fee_bps(&c.get_admin(), &close_bps);
        f
    }

    fn client(&self) -> MarginControllerClient<'_> {
        MarginControllerClient::new(&self.env, &self.id)
    }

    fn fund_margin(&self, user: &Address, amount: u128) {
        MockTokenClient::new(&self.env, &self.usdt).mint(user, &(amount as i128));
        receipt_vault::ReceiptVaultClient::new(&self.env, &self.uv).deposit(user, &amount);
        self.client()
            .transfer_spot_to_margin(user, &self.usdt, &amount);
    }

    fn begin(&self, amount: u128, leverage: u128, side: PositionSide) -> (u64, Address) {
        let c = self.client();
        let quote = c.preview_open_fees_v3(&amount, &leverage);
        self.fund_margin(&self.user, quote.total_required_ptokens);
        let (pool, pool_id, tokens) = setup_perps_pool(&self.env, &self.usdt, &self.xlm);
        let id = c.begin_open_position_v3(
            &self.user,
            &self.usdt,
            &self.xlm,
            &amount,
            &leverage,
            &side,
            &tokens,
            &pool_id,
            &pool,
            &(amount * leverage),
        );
        (id, pool)
    }

    fn open(&self, amount: u128, leverage: u128, side: PositionSide) -> u64 {
        let (id, _) = self.begin(amount, leverage, side);
        self.client().swap_open_position_v3(&self.user, &id);
        self.client().activate_open_position_v3(&self.user, &id);
        id
    }

    fn swap_close(&self, id: u64, side: &PositionSide) -> u128 {
        let c = self.client();
        c.prepare_close_position_v3(&self.user, &id);
        let pending = c.get_pending_perps_close(&id).unwrap();
        let debt_vault = if *side == PositionSide::Long {
            &self.uv
        } else {
            &self.xv
        };
        let debt = receipt_vault::ReceiptVaultClient::new(&self.env, debt_vault)
            .get_margin_borrow_balance(&id);
        let input = if *side == PositionSide::Long {
            pending.collateral_underlying
        } else {
            debt + debt.div_ceil(1000)
        };
        self.env.cost_estimate().budget().reset_unlimited();
        if *side == PositionSide::Long {
            c.swap_close_position_v3(&self.user, &id, &input);
        } else {
            c.swap_close_short_position_v3(&self.user, &id, &input, &input);
        }
        assert_last_invocation_resources_under(&self.env, 100, 45, 30_000_000);
        input
    }
}

#[test]
fn nonzero_fees_long_short_2_to_5x_amount_matrix() {
    for side in [PositionSide::Long, PositionSide::Short] {
        for leverage in 2..=5 {
            for margin in [1_000_000u128, 100_000_000, 5_000_000_000] {
                let f = Fixture::new(10, 20);
                let c = f.client();
                let alice = Address::generate(&f.env);
                let bob = Address::generate(&f.env);
                f.fund_margin(&alice, 10_000_000);
                f.fund_margin(&bob, 30_000_000);
                let (id, _) = f.begin(margin, leverage, side.clone());
                let open_fee = margin * leverage / 1000;
                assert_eq!(
                    c.get_position_fee_terms(&id).unwrap().open_fee_ptokens,
                    open_fee
                );
                assert_eq!(c.get_claimable_margin_fees(&alice, &f.usdt), 0);
                f.env.cost_estimate().budget().reset_unlimited();
                c.swap_open_position_v3(&f.user, &id);
                assert_last_invocation_resources_under(&f.env, 100, 45, 30_000_000);
                assert_eq!(
                    c.get_position_fee_terms(&id).unwrap().open_fee_ptokens,
                    open_fee
                );
                assert_eq!(c.get_claimable_margin_fees(&alice, &f.usdt), 0);
                assert!(c.try_swap_open_position_v3(&f.user, &id).is_err());
                f.env.cost_estimate().budget().reset_unlimited();
                c.activate_open_position_v3(&f.user, &id);
                assert_last_invocation_resources_under(&f.env, 100, 45, 30_000_000);
                assert_eq!(c.get_position_fee_terms(&id).unwrap().open_fee_ptokens, 0);
                assert_eq!(c.get_undistributed_margin_fees(&f.uv).ptokens, open_fee);
                assert_eq!(c.distribute_margin_fees(&f.uv), open_fee);
                assert_eq!(c.get_claimable_margin_fees(&alice, &f.usdt), open_fee / 4);
                assert_eq!(c.get_claimable_margin_fees(&bob, &f.usdt), open_fee * 3 / 4);
                assert!(c.try_activate_open_position_v3(&f.user, &id).is_err());

                let input = f.swap_close(id, &side);
                let close_fee = input * 20 / BPS_SCALE;
                let user_before = MockTokenClient::new(&f.env, &f.usdt).balance(&f.user);
                f.env.cost_estimate().budget().reset_unlimited();
                c.finish_close_position_v3(&id);
                assert_last_invocation_resources_under(&f.env, 100, 50, 30_000_000);
                assert_eq!(c.get_position(&id), None);
                assert_eq!(c.get_position_fee_terms(&id), None);
                assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, close_fee);
                if side == PositionSide::Long {
                    assert_eq!(
                        c.get_margin_balance_ptokens(&f.user, &f.usdt),
                        margin - close_fee
                    );
                } else {
                    let expected = margin * (leverage + 1) - input - close_fee;
                    let user_after = MockTokenClient::new(&f.env, &f.usdt).balance(&f.user);
                    assert_eq!(user_after - user_before, expected as i128);
                }
                assert!(c.try_finish_close_position_v3(&id).is_err());
                assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, close_fee);

                // Distribution is separate and permissionless. Even an unrelated
                // caller cannot redirect it; newly returned free margin gets no back-pay.
                let index_before = c.get_margin_fee_index(&f.usdt);
                f.env.cost_estimate().budget().reset_unlimited();
                assert_eq!(c.distribute_margin_fees(&f.uv), close_fee);
                assert_last_invocation_resources_under(&f.env, 100, 45, 30_000_000);
                assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 0);
                assert!(c.get_margin_fee_index(&f.usdt) > index_before);
                assert_eq!(c.distribute_margin_fees(&f.uv), 0);
                let earned = c.get_claimable_margin_fees(&alice, &f.usdt);
                assert!(earned >= open_fee / 4);
                assert_eq!(c.claim_margin_fees(&alice, &f.usdt), earned);
                assert_eq!(c.claim_margin_fees(&alice, &f.usdt), 0);
                c.claim_margin_fees(&bob, &f.usdt);
                c.claim_margin_fees(&f.user, &f.usdt);
                let accounted = c.get_margin_balance_ptokens(&alice, &f.usdt)
                    + c.get_margin_balance_ptokens(&bob, &f.usdt)
                    + c.get_margin_balance_ptokens(&f.user, &f.usdt);
                let backing =
                    receipt_vault::ReceiptVaultClient::new(&f.env, &f.uv).get_ptoken_balance(&f.id);
                assert!(accounted <= backing);
                assert!(
                    backing - accounted <= 3,
                    "only distribution dust may remain"
                );
            }
        }
    }
}

#[test]
fn begin_requires_the_extra_fee_without_changing_margin_or_debt_on_failure() {
    for side in [PositionSide::Long, PositionSide::Short] {
        let f = Fixture::new(100, 100);
        let c = f.client();
        f.fund_margin(&f.user, 100_000);
        let (pool, pool_id, tokens) = setup_perps_pool(&f.env, &f.usdt, &f.xlm);
        assert_eq!(
            c.preview_open_fees_v3(&100_000, &5).total_required_ptokens,
            105_000
        );
        assert!(c
            .try_begin_open_position_v3(
                &f.user, &f.usdt, &f.xlm, &100_000, &5, &side, &tokens, &pool_id, &pool, &500_000,
            )
            .is_err());
        assert_eq!(c.get_margin_balance_ptokens(&f.user, &f.usdt), 100_000);
        assert!(c.get_user_positions(&f.user).is_empty());
        for vault in [&f.uv, &f.xv] {
            assert_eq!(
                receipt_vault::ReceiptVaultClient::new(&f.env, vault).get_total_borrowed(),
                0
            );
            let fees = c.get_undistributed_margin_fees(vault);
            assert_eq!((fees.ptokens, fees.underlying), (0, 0));
        }
    }
}

#[test]
fn unswapped_cancel_and_expiry_refund_all_reserved_shares() {
    for expire in [false, true] {
        let f = Fixture::new(100, 100);
        let c = f.client();
        let (id, _) = f.begin(100_000, 5, PositionSide::Long);
        assert_eq!(c.get_margin_balance_ptokens(&f.user, &f.usdt), 0);
        if expire {
            f.env
                .ledger()
                .with_mut(|l| l.timestamp += PENDING_OPEN_TTL_SECS + 1);
            c.expire_pending_open_v3(&id);
        } else {
            c.cancel_pending_open_v3(&f.user, &id);
        }
        assert_eq!(c.get_margin_balance_ptokens(&f.user, &f.usdt), 105_000);
        assert_eq!(c.get_margin_fee_index(&f.usdt), 0);
        assert_eq!(c.get_position_fee_terms(&id), None);
    }
}

#[test]
fn fees_snapshot_rates_and_swap_failure_rolls_back_collection() {
    let f = Fixture::new(100, 100);
    let c = f.client();
    let lp = Address::generate(&f.env);
    f.fund_margin(&lp, 100_000);
    let (id, pool) = f.begin(100_000, 5, PositionSide::Short);
    c.set_open_fee_bps(&c.get_admin(), &500);
    c.set_close_fee_bps(&c.get_admin(), &500);
    MockAquariusPoolClient::new(&f.env, &pool).set_payout_bps(&900_000);
    assert!(c.try_swap_open_position_v3(&f.user, &id).is_err());
    assert_eq!(
        c.get_position_fee_terms(&id).unwrap().open_fee_ptokens,
        5_000
    );
    assert_eq!(c.get_margin_fee_index(&f.usdt), 0);
    MockAquariusPoolClient::new(&f.env, &pool).set_payout_bps(&1_000_000);
    c.swap_open_position_v3(&f.user, &id);
    c.activate_open_position_v3(&f.user, &id);
    c.distribute_margin_fees(&f.uv);
    assert_eq!(c.get_claimable_margin_fees(&lp, &f.usdt), 5_000);
    assert_eq!(c.get_position_fee_terms(&id).unwrap().close_fee_bps, 100);
    let input = f.swap_close(id, &PositionSide::Short);
    c.finish_close_position_v3(&id);
    assert_eq!(
        c.get_undistributed_margin_fees(&f.uv).underlying,
        input / 100
    );
}

#[test]
fn aborted_close_is_not_charged_and_zero_debt_release_has_no_swap_fee() {
    let f = Fixture::new(100, 100);
    let c = f.client();
    let id = f.open(100_000, 5, PositionSide::Long);
    c.prepare_close_position_v3(&f.user, &id);
    c.cancel_close_position_v3(&f.user, &id);
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 0);
    assert_eq!(
        c.get_position_fee_terms(&id).unwrap().close_fee_underlying,
        0
    );
    c.repay_margin_position_v3(&f.user, &id, &u128::MAX);
    c.release_debt_free_position_v3(&f.user, &id);
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 0);
    assert_eq!(c.get_margin_balance_ptokens(&f.user, &f.xlm), 500_000);
}

#[test]
fn legacy_and_zero_fee_positions_stay_free_after_admin_enables_fees() {
    let f = Fixture::new(0, 0);
    let c = f.client();
    let id = f.open(100_000, 5, PositionSide::Long);
    assert_eq!(c.get_position_fee_terms(&id), None);
    c.set_open_fee_bps(&c.get_admin(), &500);
    c.set_close_fee_bps(&c.get_admin(), &500);
    f.swap_close(id, &PositionSide::Long);
    c.finish_close_position_v3(&id);
    assert_eq!(c.get_margin_balance_ptokens(&f.user, &f.usdt), 100_000);
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 0);
}

#[test]
fn fee_math_floors_and_does_not_overflow_at_u128_max() {
    assert_eq!(crate::fees::fee_for_amount(99, 100), 0);
    assert_eq!(crate::fees::fee_for_amount(100, 100), 1);
    assert_eq!(crate::fees::fee_for_amount(u128::MAX, 500), u128::MAX / 20);
}

#[test]
fn fees_without_free_providers_are_backed_orphans() {
    let f = Fixture::new(100, 100);
    let c = f.client();
    let (id, _) = f.begin(100_000, 5, PositionSide::Long);
    c.swap_open_position_v3(&f.user, &id);
    c.activate_open_position_v3(&f.user, &id);
    c.distribute_margin_fees(&f.uv);
    let recipient = Address::generate(&f.env);
    assert_eq!(
        c.sweep_orphan_fees(&c.get_admin(), &f.usdt, &recipient),
        5_000
    );
    assert_eq!(c.sweep_orphan_fees(&c.get_admin(), &f.usdt, &recipient), 0);
    assert_eq!(c.get_margin_balance_ptokens(&recipient, &f.usdt), 5_000);
}

#[test]
fn close_fee_is_capped_to_surplus_in_atomic_and_split_long_closes() {
    for atomic in [false, true] {
        let f = Fixture::new(0, 500);
        let c = f.client();
        let (id, pool) = f.begin(100_000, 5, PositionSide::Long);
        c.swap_open_position_v3(&f.user, &id);
        c.activate_open_position_v3(&f.user, &id);
        MockPeridottrollerClient::new(&f.env, &f.oracle).set_price(&f.xlm, &810_000, &SCALE_1E6);
        let pool = MockAquariusPoolClient::new(&f.env, &pool);
        pool.set_quote_bps(&810_000);
        pool.set_payout_bps(&810_000);
        if atomic {
            c.close_position_v3(&f.user, &id, &405_000);
        } else {
            c.prepare_close_position_v3(&f.user, &id);
            c.swap_close_position_v3(&f.user, &id, &405_000);
            c.finish_close_position_v3(&id);
        }
        assert_eq!(
            receipt_vault::ReceiptVaultClient::new(&f.env, &f.uv).get_margin_borrow_balance(&id),
            0
        );
        assert_eq!(c.get_margin_balance_ptokens(&f.user, &f.usdt), 0);
        assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 5_000);
        assert_eq!(c.distribute_margin_fees(&f.uv), 5_000);
        let lp = Address::generate(&f.env);
        assert_eq!(c.sweep_orphan_fees(&c.get_admin(), &f.usdt, &lp), 5_000);
    }
}

#[test]
fn residual_repayment_never_charges_a_fee_until_debt_is_zero() {
    let f = Fixture::new(10, 100);
    let c = f.client();
    let id = f.open(100_000, 5, PositionSide::Short);
    let debt_vault = receipt_vault::ReceiptVaultClient::new(&f.env, &f.xv);
    debt_vault.set_borrow_rate(&10_000_000);
    let input = f.swap_close(id, &PositionSide::Short);
    let quoted_fee = input / 100;
    f.env.ledger().with_mut(|l| l.timestamp += 7 * 24 * 60 * 60);
    c.finish_close_position_v3(&id);
    assert!(debt_vault.get_margin_borrow_balance(&id) > 0);
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 0);
    assert_eq!(
        c.get_position_fee_terms(&id).unwrap().close_fee_underlying,
        quoted_fee
    );
    c.finish_close_position_v3(&id);
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 0);
    let debt = debt_vault.get_margin_borrow_balance(&id);
    MockTokenClient::new(&f.env, &f.xlm).mint(&f.user, &(debt as i128));
    c.repay_margin_position_v3(&f.user, &id, &debt);
    c.finish_close_position_v3(&id);
    assert_eq!(
        c.get_undistributed_margin_fees(&f.uv).underlying,
        quoted_fee
    );
    assert_eq!(c.get_position(&id), None);
}

#[test]
fn activation_failure_preserves_reserved_fee_and_success_charges_once() {
    let f = Fixture::new(100, 100);
    let c = f.client();
    let lp = Address::generate(&f.env);
    f.fund_margin(&lp, 100_000);
    let (id, _) = f.begin(100_000, 5, PositionSide::Long);
    c.swap_open_position_v3(&f.user, &id);
    let oracle = MockPeridottrollerClient::new(&f.env, &f.oracle);
    oracle.set_price(&f.xlm, &500_000, &SCALE_1E6);
    assert!(c.try_activate_open_position_v3(&f.user, &id).is_err());
    assert_eq!(c.get_claimable_margin_fees(&lp, &f.usdt), 0);
    assert_eq!(
        c.get_position_fee_terms(&id).unwrap().open_fee_ptokens,
        5_000
    );
    assert!(c.try_cancel_pending_open_v3(&f.user, &id).is_err());
    oracle.set_price(&f.xlm, &SCALE_1E6, &SCALE_1E6);
    c.activate_open_position_v3(&f.user, &id);
    c.distribute_margin_fees(&f.uv);
    assert_eq!(c.get_claimable_margin_fees(&lp, &f.usdt), 5_000);
}

#[test]
fn fees_follow_actual_swap_output_not_the_pools_reported_return() {
    let f = Fixture::new(0, 100);
    let c = f.client();
    let (id, pool) = f.begin(100_000, 5, PositionSide::Long);
    c.swap_open_position_v3(&f.user, &id);
    c.activate_open_position_v3(&f.user, &id);
    let pool = MockAquariusPoolClient::new(&f.env, &pool);
    pool.set_reported_bps(&100_000_000);
    f.swap_close(id, &PositionSide::Long);
    c.finish_close_position_v3(&id);
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 5_000);
    assert_eq!(c.get_margin_balance_ptokens(&f.user, &f.usdt), 95_000);
}

#[test]
fn permissionless_fee_conversion_cannot_spend_unreserved_assets_and_retries_atomically() {
    let f = Fixture::new(0, 100);
    let c = f.client();
    let id = f.open(100_000, 5, PositionSide::Short);
    f.swap_close(id, &PositionSide::Short);
    c.finish_close_position_v3(&id);
    let reserved = c.get_undistributed_margin_fees(&f.uv).underlying;
    let token = MockTokenClient::new(&f.env, &f.usdt);
    // Simulate missing backing, then repair it. Failed conversion must not clear
    // the fee inventory or create claims. Extra unrelated cash must not be swept.
    token.burn(&f.id, &(reserved as i128));
    assert!(c.try_distribute_margin_fees(&f.uv).is_err());
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, reserved);
    token.mint(&f.id, &(reserved as i128 + 12345));
    f.env.mock_auths(&[]);
    assert_eq!(c.distribute_margin_fees(&f.uv), reserved);
    assert_eq!(token.balance(&f.id), 12345);
    assert_eq!(c.distribute_margin_fees(&Address::generate(&f.env)), 0);
    assert_eq!(c.distribute_margin_fees(&f.uv), 0);
}

#[test]
fn fee_terms_ttl_tracks_position_and_pending_close_reads() {
    let f = Fixture::new(100, 100);
    let c = f.client();
    let id = f.open(100_000, 5, PositionSide::Long);
    c.prepare_close_position_v3(&f.user, &id);
    let key = DataKey::PerpsFeeTerms(id);
    let ttl = f
        .env
        .as_contract(&f.id, || f.env.storage().persistent().get_ttl(&key));
    f.env.ledger().set_sequence_number(ttl - 10_000);
    c.get_pending_perps_close(&id);
    f.env.as_contract(&f.id, || {
        assert!(f.env.storage().persistent().get_ttl(&key) > TTL_THRESHOLD);
    });
    c.cancel_close_position_v3(&f.user, &id);
    let ttl = f
        .env
        .as_contract(&f.id, || f.env.storage().persistent().get_ttl(&key));
    f.env
        .ledger()
        .set_sequence_number(f.env.ledger().sequence() + ttl - 10_000);
    c.get_position(&id);
    f.env.as_contract(&f.id, || {
        assert!(f.env.storage().persistent().get_ttl(&key) > TTL_THRESHOLD);
    });
}

#[test]
fn fee_terms_public_read_keeps_dependent_position_state_alive() {
    for phase in 0..4 {
        let f = Fixture::new(100, 100);
        let c = f.client();
        let (id, _) = f.begin(100_000, 5, PositionSide::Long);
        if phase >= 1 {
            c.swap_open_position_v3(&f.user, &id);
        }
        if phase >= 2 {
            c.activate_open_position_v3(&f.user, &id);
        }
        if phase == 3 {
            c.prepare_close_position_v3(&f.user, &id);
        }
        let mut keys = std::vec![
            DataKey::PerpsFeeTerms(id),
            DataKey::Position(id),
            DataKey::PositionMode(id),
            DataKey::PositionCollateralVault(id),
            DataKey::PositionDebtVault(id),
            DataKey::PositionPositionVault(id),
        ];
        if phase < 2 {
            keys.push(DataKey::PendingPerpsOpenPosition(id));
            if phase == 1 {
                keys.push(DataKey::PendingPerpsOpenExecution(id));
            }
        } else {
            keys.push(DataKey::PerpsPositionData(id));
            if phase == 3 {
                keys.push(DataKey::PendingPerpsClose(id));
            }
        }
        let ttl = f.env.as_contract(&f.id, || {
            keys.iter()
                .map(|key| f.env.storage().persistent().get_ttl(key))
                .min()
                .unwrap()
        });
        f.env
            .ledger()
            .set_sequence_number(f.env.ledger().sequence() + ttl - 10_000);
        f.env.as_contract(&f.id, || {
            for key in &keys {
                assert!(f.env.storage().persistent().get_ttl(key) < TTL_THRESHOLD);
            }
        });
        assert!(c.get_position_fee_terms(&id).is_some());
        f.env.as_contract(&f.id, || {
            for key in &keys {
                assert!(f.env.storage().persistent().get_ttl(key) > TTL_THRESHOLD);
            }
        });
    }
}

#[test]
fn no_close_fee_is_charged_on_liquidation() {
    let f = Fixture::new(100, 500);
    let c = f.client();
    let (id, pool) = f.begin(100_000, 5, PositionSide::Long);
    c.swap_open_position_v3(&f.user, &id);
    c.activate_open_position_v3(&f.user, &id);
    MockPeridottrollerClient::new(&f.env, &f.oracle).set_price(&f.xlm, &810_000, &SCALE_1E6);
    MockAquariusPoolClient::new(&f.env, &pool).set_quote_bps(&810_000);
    MockAquariusPoolClient::new(&f.env, &pool).set_payout_bps(&810_000);
    let liquidator = Address::generate(&f.env);
    c.begin_liquidation_v3(&liquidator, &id);
    c.swap_liquidation_v3(&liquidator, &id, &405_000);
    c.finish_liquidation_v3(&liquidator, &id);
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 0);
    assert_eq!(c.get_position_fee_terms(&id), None);
}

#[test]
fn fee_enabled_dynamic_rate_stages_do_not_scale_with_sibling_positions() {
    for side in [PositionSide::Long, PositionSide::Short] {
        let f = Fixture::new(10, 20);
        let c = f.client();
        let lp = Address::generate(&f.env);
        f.fund_margin(&lp, 100_000_000);
        for _ in 0..8 {
            f.open(1_000_000, 5, side.clone());
        }
        let model = f.env.register(MockRateModel, ());
        receipt_vault::ReceiptVaultClient::new(&f.env, &f.uv).set_interest_model(&model);
        receipt_vault::ReceiptVaultClient::new(&f.env, &f.xv).set_interest_model(&model);
        let (id, _) = f.begin(100_000_000, 5, side.clone());
        f.env.cost_estimate().budget().reset_unlimited();
        c.swap_open_position_v3(&f.user, &id);
        assert_last_invocation_resources_under(&f.env, 100, 45, 30_000_000);
        f.env.cost_estimate().budget().reset_unlimited();
        c.activate_open_position_v3(&f.user, &id);
        // Fee-time ownership adds the epoch and aggregate-weight keys. Keep a
        // measured bound below the live runner's 200-entry ceiling (not an SDK limit).
        assert_last_invocation_resources_under(&f.env, 105, 45, 30_000_000);
        f.swap_close(id, &side);
        f.env.cost_estimate().budget().reset_unlimited();
        c.finish_close_position_v3(&id);
        assert_last_invocation_resources_under(&f.env, 100, 50, 30_000_000);
        f.env.cost_estimate().budget().reset_unlimited();
        c.distribute_margin_fees(&f.uv);
        assert_last_invocation_resources_under(&f.env, 100, 45, 30_000_000);
        assert_eq!(c.get_user_positions(&f.user).len(), 8);
    }
}

#[test]
fn a_fee_created_sub_ptoken_surplus_is_returned_as_wallet_dust() {
    let f = Fixture::new(0, 500);
    let c = f.client();
    let (id, pool) = f.begin(1_000_000_000, 5, PositionSide::Long);
    c.swap_open_position_v3(&f.user, &id);
    c.activate_open_position_v3(&f.user, &id);
    let vault = receipt_vault::ReceiptVaultClient::new(&f.env, &f.uv);
    vault.set_borrow_rate(&1_000_000);
    f.env.ledger().with_mut(|l| l.timestamp += 60 * 60);
    vault.update_interest();
    let debt = vault.get_margin_borrow_balance(&id);
    assert!(vault.get_exchange_rate() > SCALE_1E6);
    let gross = debt * 20 / 19 + 1;
    let fee = gross / 20;
    assert_eq!(gross - debt - fee, 1);
    // Produce the exact integer output while keeping pool and oracle aligned.
    let position = c.get_position(&id).unwrap();
    let price = gross * SCALE_1E6 / position.collateral_ptokens;
    MockPeridottrollerClient::new(&f.env, &f.oracle).set_price(&f.xlm, &price, &SCALE_1E6);
    let pool_client = MockAquariusPoolClient::new(&f.env, &pool);
    pool_client.set_quote_bps(&price);
    pool_client.set_payout_bps(&price);
    c.prepare_close_position_v3(&f.user, &id);
    // Test the settlement boundary using an already-swapped escrow record;
    // this isolates exact 1-unit dust from the mock pool's integer quote scale.
    MockTokenClient::new(&f.env, &f.usdt).mint(&f.id, &(gross as i128));
    f.env.as_contract(&f.id, || {
        let mut pending = get_pending_perps_close_or_panic(&f.env, id);
        pending.debt_amount = debt;
        pending.received_debt_asset = gross;
        set_pending_perps_close(&f.env, id, &pending);
        crate::fees::set_close_execution_fee(&f.env, id, gross);
    });
    let before = MockTokenClient::new(&f.env, &f.usdt).balance(&f.user);
    c.finish_close_position_v3(&id);
    assert_eq!(
        MockTokenClient::new(&f.env, &f.usdt).balance(&f.user) - before,
        1
    );
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, fee);
    assert_eq!(c.get_position(&id), None);
}

#[test]
fn one_batch_distributes_open_shares_and_converts_close_underlying() {
    let f = Fixture::new(100, 100);
    let c = f.client();
    let lp = Address::generate(&f.env);
    f.fund_margin(&lp, 100_000);
    let id = f.open(100_000, 5, PositionSide::Short);
    let input = f.swap_close(id, &PositionSide::Short);
    c.finish_close_position_v3(&id);
    let pending = c.get_undistributed_margin_fees(&f.uv);
    assert_eq!(pending.ptokens, 5_000);
    assert_eq!(pending.underlying, input / 100);
    assert_eq!(c.get_claimable_margin_fees(&lp, &f.usdt), 0);
    f.env.mock_auths(&[]);
    let total = pending.ptokens + pending.underlying;
    assert_eq!(c.distribute_margin_fees(&f.uv), total);
    assert_eq!(c.get_claimable_margin_fees(&lp, &f.usdt), total);
    assert_eq!(c.distribute_margin_fees(&f.uv), 0);
}

#[test]
fn underlying_dust_defers_the_epoch_without_blocking_principal() {
    let f = Fixture::new(100, 100);
    let c = f.client();
    let lp = Address::generate(&f.env);
    f.fund_margin(&lp, 100_000);
    let id = f.open(1_000_000_000, 5, PositionSide::Long);
    let vault = receipt_vault::ReceiptVaultClient::new(&f.env, &f.uv);
    vault.set_borrow_rate(&1_000_000);
    f.env.ledger().with_mut(|l| l.timestamp += 60 * 60);
    vault.update_interest();
    assert!(vault.get_exchange_rate() > SCALE_1E6);
    let expected = c.get_undistributed_margin_fees(&f.uv).ptokens;
    MockTokenClient::new(&f.env, &f.usdt).mint(&f.id, &1);
    f.env.as_contract(&f.id, || {
        crate::fee_entitlements::reserve(&f.env, &f.uv, 0, 1);
        let key = DataKey::PendingMarginFees(f.uv.clone());
        let mut batch: PendingMarginFees = f.env.storage().persistent().get(&key).unwrap();
        batch.underlying = 1;
        f.env.storage().persistent().set(&key, &batch);
    });
    assert_eq!(c.distribute_margin_fees(&f.uv), 0);
    assert_eq!(
        c.get_undistributed_margin_fees(&f.uv),
        PendingMarginFees {
            underlying: 1,
            ptokens: expected
        }
    );
    assert_eq!(c.distribute_margin_fees(&f.uv), 0);
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 1);
    c.transfer_margin_to_spot(&lp, &f.usdt, &100_000);
    assert_eq!(c.get_margin_balance_ptokens(&lp, &f.usdt), 0);
    assert!(c.get_position(&id).is_some());
}

#[test]
fn late_deposit_cannot_capture_historical_open_or_close_fees() {
    let f = Fixture::new(100, 100);
    let c = f.client();
    let provider = Address::generate(&f.env);
    let late = Address::generate(&f.env);
    f.fund_margin(&provider, 100_000);
    let id = f.open(100_000, 5, PositionSide::Short);
    f.swap_close(id, &PositionSide::Short);
    c.finish_close_position_v3(&id);
    let pending = c.get_undistributed_margin_fees(&f.uv);
    let batch = pending.ptokens + pending.underlying;
    let ledger = f.env.ledger().sequence();
    let timestamp = f.env.ledger().timestamp();
    f.fund_margin(&late, 900_000);
    f.env.mock_auths(&[]);
    assert_eq!(c.distribute_margin_fees(&f.uv), batch);
    assert_eq!(c.get_claimable_margin_fees(&late, &f.usdt), 0);
    assert_eq!(c.get_claimable_margin_fees(&provider, &f.usdt), batch);
    f.env.mock_all_auths();
    assert_eq!(c.claim_margin_fees(&late, &f.usdt), 0);
    c.transfer_margin_to_spot(&late, &f.usdt, &900_000);
    assert_eq!(c.claim_margin_fees(&provider, &f.usdt), batch);
    assert_eq!(c.claim_margin_fees(&provider, &f.usdt), 0);
    assert_eq!(f.env.ledger().sequence(), ledger);
    assert_eq!(f.env.ledger().timestamp(), timestamp);
}

// Backed fees without a trade, for exact ownership/rounding boundary checks.
fn reserve_test_fees(f: &Fixture, ptokens: u128, underlying: u128) {
    MockTokenClient::new(&f.env, &f.usdt).mint(&f.id, &((ptokens + underlying) as i128));
    if ptokens > 0 {
        receipt_vault::ReceiptVaultClient::new(&f.env, &f.uv).deposit(&f.id, &ptokens);
    }
    f.env.as_contract(&f.id, || {
        crate::fee_entitlements::reserve(&f.env, &f.uv, ptokens, underlying);
        let mut pending = crate::fees::pending_margin_fees(&f.env, &f.uv);
        pending.ptokens += ptokens;
        pending.underlying += underlying;
        f.env
            .storage()
            .persistent()
            .set(&DataKey::PendingMarginFees(f.uv.clone()), &pending);
    });
}

#[test]
fn ownership_survives_exit_and_only_future_fees_use_new_weights() {
    let f = Fixture::new(0, 0);
    let c = f.client();
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    f.fund_margin(&alice, 100_000);
    reserve_test_fees(&f, 1_000, 2_000);
    f.fund_margin(&bob, 300_000);
    reserve_test_fees(&f, 4_000, 8_000);
    c.transfer_margin_to_spot(&alice, &f.usdt, &100_000);
    reserve_test_fees(&f, 3_000, 6_000);
    c.transfer_margin_to_spot(&bob, &f.usdt, &300_000);
    assert_eq!(c.distribute_margin_fees(&f.uv), 24_000);
    assert_eq!(c.get_claimable_margin_fees(&alice, &f.usdt), 6_000);
    assert_eq!(c.get_claimable_margin_fees(&bob, &f.usdt), 18_000);
    assert_eq!(c.claim_margin_fees(&alice, &f.usdt), 6_000);
    assert_eq!(c.claim_margin_fees(&bob, &f.usdt), 18_000);
    assert_eq!(c.claim_margin_fees(&alice, &f.usdt), 0);
}

#[test]
fn fee_orphans_are_decided_when_earned_not_when_converted() {
    let f = Fixture::new(0, 0);
    let c = f.client();
    reserve_test_fees(&f, 1_000, 2_000);
    let late = Address::generate(&f.env);
    f.fund_margin(&late, 100_000);
    reserve_test_fees(&f, 500, 1_000);
    assert_eq!(c.distribute_margin_fees(&f.uv), 4_500);
    assert_eq!(c.claim_margin_fees(&late, &f.usdt), 1_500);
    assert_eq!(c.sweep_orphan_fees(&c.get_admin(), &f.usdt, &late), 3_000);
    assert_eq!(c.claim_margin_fees(&late, &f.usdt), 0);
}

#[test]
fn delayed_claim_skips_fifty_batches_with_constant_footprint() {
    let f = Fixture::new(0, 0);
    let c = f.client();
    let alice = Address::generate(&f.env);
    f.fund_margin(&alice, 100_000);
    let mut settlement_footprint = None;
    for _ in 0..50 {
        reserve_test_fees(&f, 1_000, 2_000);
        assert_eq!(c.distribute_margin_fees(&f.uv), 3_000);
        let resources = f.env.cost_estimate().resources();
        let footprint = (
            resources.disk_read_entries + resources.memory_read_entries,
            resources.write_entries,
        );
        if let Some(first) = settlement_footprint {
            assert_eq!(footprint, first, "settlement footprint grew with history");
        } else {
            settlement_footprint = Some(footprint);
        }
    }
    assert_eq!(c.get_claimable_margin_fees(&alice, &f.usdt), 150_000);
    f.env.cost_estimate().budget().reset_unlimited();
    assert_eq!(c.claim_margin_fees(&alice, &f.usdt), 150_000);
    assert_last_invocation_resources_under(&f.env, 30, 30, 10_000_000);
    assert_eq!(c.claim_margin_fees(&alice, &f.usdt), 0);
    assert_eq!(c.distribute_margin_fees(&f.uv), 0);
    f.env.as_contract(&f.id, || {
        let epoch: MarginFeeEpoch = f
            .env
            .storage()
            .persistent()
            .get(&DataKey::MarginFeeEpoch(f.uv.clone()))
            .unwrap();
        assert_eq!(epoch.id, 50, "empty calls must not create history");
    });
}

#[test]
fn pending_vault_snapshots_are_not_controlled_by_the_caller_or_market_rebinding() {
    for side in [PositionSide::Long, PositionSide::Short] {
        let f = Fixture::new(10, 20);
        let c = f.client();
        let (id, _) = f.begin(100_000, 5, side.clone());
        let pending = c.get_pending_perps_open(&id).unwrap();
        let other = Address::generate(&f.env);
        assert!(c.try_swap_open_position_v3(&other, &id).is_err());
        let replacement = f.env.register(ReceiptVault, ());
        receipt_vault::ReceiptVaultClient::new(&f.env, &replacement).initialize(
            &f.usdt,
            &0,
            &0,
            &c.get_admin(),
        );
        assert!(c.try_set_market(&other, &f.usdt, &replacement).is_err());
        // Even an authorized mapping change does not rewrite pending/canonical
        // position addresses. Only begin writes those records, from validated markets.
        c.set_market(&c.get_admin(), &f.usdt, &replacement);
        assert_eq!(c.get_pending_perps_open(&id).unwrap(), pending);
        f.env.as_contract(&f.id, || {
            let position = get_position_record_or_panic(&f.env, id);
            let vaults = get_position_vaults(&f.env, id, &position);
            assert_eq!(vaults.collateral_vault, pending.margin_vault);
            assert_eq!(vaults.debt_vault, pending.debt_vault);
            assert_eq!(vaults.position_vault, pending.position_vault);
        });
        c.swap_open_position_v3(&f.user, &id);
        c.activate_open_position_v3(&f.user, &id);
        assert_eq!(c.get_position(&id).unwrap().status, PositionStatus::Open);
        assert!(
            receipt_vault::ReceiptVaultClient::new(&f.env, &pending.debt_vault)
                .get_margin_borrow_balance(&id)
                > 0
        );
        assert_eq!(
            receipt_vault::ReceiptVaultClient::new(&f.env, &replacement).get_total_borrowed(),
            0
        );
    }
}

#[test]
fn conversion_uses_actual_minted_shares_at_the_later_exchange_rate() {
    let f = Fixture::new(0, 0);
    let c = f.client();
    let alice = Address::generate(&f.env);
    f.fund_margin(&alice, 100_000);
    reserve_test_fees(&f, 1_000, 20_000);
    // Model recognized strategy yield without minting shares. A plain donation
    // is deliberately excluded by ReceiptVault's managed-cash accounting.
    let vault = receipt_vault::ReceiptVaultClient::new(&f.env, &f.uv);
    let total = vault.get_total_underlying();
    MockTokenClient::new(&f.env, &f.usdt).mint(&f.uv, &(total as i128));
    f.env.as_contract(&f.uv, || {
        let key = receipt_vault::DataKey::ManagedCash;
        let cash: u128 = f.env.storage().persistent().get(&key).unwrap();
        f.env.storage().persistent().set(&key, &(cash + total));
    });
    assert_eq!(vault.get_exchange_rate(), 2 * SCALE_1E6);
    c.transfer_margin_to_spot(&alice, &f.usdt, &100_000);
    assert_eq!(c.distribute_margin_fees(&f.uv), 11_000);
    assert_eq!(c.claim_margin_fees(&alice, &f.usdt), 11_000);
}

#[test]
fn old_pending_batches_cannot_be_reinterpreted_with_current_weights() {
    let f = Fixture::new(0, 0);
    let c = f.client();
    f.fund_margin(&f.user, 100_000);
    reserve_test_fees(&f, 1_000, 2_000);
    f.env.as_contract(&f.id, || {
        f.env
            .storage()
            .persistent()
            .remove(&DataKey::MarginFeeEpoch(f.uv.clone()))
    });
    assert!(c.try_distribute_margin_fees(&f.uv).is_err());
    assert_eq!(c.get_undistributed_margin_fees(&f.uv).underlying, 2_000);
    assert_eq!(c.get_claimable_margin_fees(&f.user, &f.usdt), 0);
}

#[test]
fn fee_history_ttl_is_renewed_and_missing_required_history_fails_closed() {
    let f = Fixture::new(0, 0);
    let c = f.client();
    f.fund_margin(&f.user, 100_000);
    reserve_test_fees(&f, 1_000, 2_000);
    // Persist a current-epoch checkpoint; repeated views must not consume it.
    assert_eq!(c.claim_margin_fees(&f.user, &f.usdt), 0);
    c.distribute_margin_fees(&f.uv);
    let keys = [
        DataKey::MarginFeeEpoch(f.uv.clone()),
        DataKey::ClosedMarginFeeEpoch(f.uv.clone(), 0),
        DataKey::UserMarginFeeEpoch(f.user.clone(), f.uv.clone()),
    ];
    let ttl = f
        .env
        .as_contract(&f.id, || f.env.storage().persistent().get_ttl(&keys[0]));
    f.env
        .ledger()
        .set_sequence_number(f.env.ledger().sequence() + ttl - 10_000);
    assert_eq!(c.get_claimable_margin_fees(&f.user, &f.usdt), 3_000);
    assert_eq!(c.get_claimable_margin_fees(&f.user, &f.usdt), 3_000);
    f.env.as_contract(&f.id, || {
        for key in keys {
            assert!(f.env.storage().persistent().get_ttl(&key) > TTL_THRESHOLD);
        }
        f.env
            .storage()
            .persistent()
            .remove(&DataKey::ClosedMarginFeeEpoch(f.uv.clone(), 0));
    });
    assert!(c.try_claim_margin_fees(&f.user, &f.usdt).is_err());
    assert_eq!(c.get_margin_balance_ptokens(&f.user, &f.usdt), 100_000);
}

#[test]
fn repeated_balance_changes_and_claims_never_exceed_fee_backing() {
    let f = Fixture::new(0, 0);
    let c = f.client();
    let users: std::vec::Vec<_> = (0..4).map(|_| Address::generate(&f.env)).collect();
    let mut weights = [0u128; 4];
    let mut earned = [0.0f64; 4];
    let mut claimed = [0u128; 4];
    let mut total_fees = 0u128;
    for round in 0..30usize {
        let entrant = round % 4;
        let deposit = (round as u128 * 13 + 7) % 97 + 1;
        f.fund_margin(&users[entrant], deposit);
        weights[entrant] += deposit;
        let p = round as u128 % 7 + 1;
        let u = round as u128 % 11 + 1;
        reserve_test_fees(&f, p, u);
        total_fees += p + u;
        let total: u128 = weights.iter().sum();
        for i in 0..4 {
            earned[i] += (p + u) as f64 * weights[i] as f64 / total as f64;
        }
        let leaver = (round + 1) % 4;
        if weights[leaver] > 0 {
            c.transfer_margin_to_spot(&users[leaver], &f.usdt, &weights[leaver]);
            weights[leaver] = 0;
        }
        if round % 3 == 2 {
            c.distribute_margin_fees(&f.uv);
            let claimant = (round + 2) % 4;
            let shares = c.claim_margin_fees(&users[claimant], &f.usdt);
            claimed[claimant] += shares;
            weights[claimant] += shares;
            assert_eq!(c.claim_margin_fees(&users[claimant], &f.usdt), 0);
        }
    }
    c.distribute_margin_fees(&f.uv);
    for i in 0..4 {
        let shares = c.claim_margin_fees(&users[i], &f.usdt);
        claimed[i] += shares;
        weights[i] += shares;
        assert!(
            claimed[i] as f64 <= earned[i] + 1e-9,
            "user {i} received historical fees"
        );
        assert!(
            earned[i] - (claimed[i] as f64) < 30.0,
            "unexpected rounding loss"
        );
    }
    assert!(claimed.iter().sum::<u128>() <= total_fees);
    let backing = receipt_vault::ReceiptVaultClient::new(&f.env, &f.uv).get_ptoken_balance(&f.id);
    assert_eq!(
        backing - weights.iter().sum::<u128>(),
        total_fees - claimed.iter().sum::<u128>()
    );
}

#[test]
#[ignore = "requires MARGIN_NEW_WASM"]
fn exact_fee_wasm_long_short_lifecycle_and_resource_limits() {
    let wasm = required_wasm_from_env("MARGIN_NEW_WASM");
    for side in [PositionSide::Long, PositionSide::Short] {
        for leverage in [2, 5] {
            let f = Fixture::new(10, 20);
            f.env.register_at(&f.id, wasm.as_slice(), ());
            let c = f.client();
            let lp = Address::generate(&f.env);
            f.fund_margin(&lp, 100_000_000);
            let (id, _) = f.begin(100_000_000, leverage, side.clone());
            f.env.cost_estimate().budget().reset_unlimited();
            c.swap_open_position_v3(&f.user, &id);
            assert_last_invocation_resources_under(&f.env, 100, 45, 100_000_000);
            f.env.cost_estimate().budget().reset_unlimited();
            c.activate_open_position_v3(&f.user, &id);
            assert_last_invocation_resources_under(&f.env, 100, 45, 100_000_000);
            f.swap_close(id, &side);
            f.env.cost_estimate().budget().reset_unlimited();
            c.finish_close_position_v3(&id);
            assert_last_invocation_resources_under(&f.env, 100, 50, 100_000_000);
            let pending = c.get_undistributed_margin_fees(&f.uv);
            assert!(pending.ptokens > 0 && pending.underlying > 0);
            f.env.cost_estimate().budget().reset_unlimited();
            assert_eq!(
                c.distribute_margin_fees(&f.uv),
                pending.ptokens + pending.underlying
            );
            assert_last_invocation_resources_under(&f.env, 100, 45, 100_000_000);
            assert!(c.claim_margin_fees(&lp, &f.usdt) > 0);
            assert_eq!(c.claim_margin_fees(&lp, &f.usdt), 0);
            assert_eq!(c.get_position(&id), None);
        }
    }
}
