//! Native lifecycle regressions; no live rotation or production ABI.
use super::*;

#[test]
fn rotation_unwinds_principal_and_retains_old_reservations_and_backing() {
    let f = Fixture::new();
    f.receipt().co_register(&f.reward);
    let (next, _) = second_token(&f);
    f.credit(100_000, 0);
    reset(&f);
    completed(f.receipt().co_compound(&f.reward));
    f.credit(110_000, 0);
    reset(&f);
    f.receipt().co_exit(&f.user, &100_000, &1);
    let reserved = stream(&f, &f.reward).reserved;
    let before = state(&f);
    let shares = f.receipt().balance(&f.user);
    f.credit(90_000, 0);
    reset(&f);
    f.receipt().co_prepare_rotate(&800_000);
    reset(&f);
    let managed = f.receipt().co_rotate(&next, &800_000);
    let cost = f.env.cost_estimate().resources();
    std::println!(
        "rotation: {} entries / {} CPU",
        cost.memory_read_entries + cost.write_entries,
        cost.instructions
    );
    assert!(cost.memory_read_entries + cost.write_entries <= 100);
    assert!(managed >= 800_000);
    assert_eq!(f.receipt().balance(&f.user), shares);
    assert_eq!(stream(&f, &f.reward).reserved, reserved);
    assert_eq!(state(&f).units, before.units);
    assert_eq!(state(&f).ptokens, before.ptokens);
    assert_eq!(
        AquariusLpVaultClient::new(&f.env, &f.strategy).balance(&f.receipt),
        0
    );
    assert_eq!(
        MockAquariusPoolClient::new(&f.env, &f.pool)
            .get_user_position_snapshot(&f.strategy)
            .raw_liquidity,
        0
    );
    // Retirement does not consume the old raw reserve or its conversion route.
    reset(&f);
    completed(f.receipt().co_settle(&f.reward, &f.user, &reserved, &1));
    assert_eq!(stream(&f, &f.reward).reserved, 0);
    let old_claim = f.receipt().earned(&f.reward, &f.user);
    let pool = MockAquariusPoolClient::new(&f.env, &f.pool);
    pool.set_reward_tokens(&next, &next);
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_000);
    reset(&f);
    f.receipt().co_deposit(&late, &1_000_000);
    assert_eq!(f.receipt().earned(&f.reward, &late).raw_scaled, 0);
    assert_eq!(f.receipt().earned(&f.reward, &late).units_scaled, 0);
    assert_eq!(f.receipt().earned(&f.reward, &f.user), old_claim);
    token::StellarAssetClient::new(&f.env, &next).mint(&f.pool, &100_000);
    pool.credit_rewards(&f.strategy, &100_000, &0);
    reset(&f);
    f.receipt().co_claim();
    assert!(f.receipt().earned(&next, &late).raw_scaled > 0);
    state(&f);
}

#[test]
fn rotation_unproven_denomination_never_creates_quoted_iou() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    let (next, _) = second_token(&f);
    reset(&f);
    f.receipt().co_rotate(&next, &1);
    // Pool has NOT changed yet. Failed cash validation cannot be interpreted as
    // claimable debt denominated in the new configured token.
    f.credit(100_000, 0);
    reset(&f);
    assert!(f.receipt().try_co_claim().is_err());
    assert_eq!(stream(&f, &next).raw, 0);
    assert_eq!(f.raw(&f.receipt), 0);
    let pool = MockAquariusPoolClient::new(&f.env, &f.pool);
    pool.set_reward_tokens(&next, &next);
    pool.credit_rewards(&f.strategy, &0, &0);
    reset(&f);
    f.receipt().co_claim(); // zero claim is NOT denomination proof
    pool.credit_rewards(&f.strategy, &100_000, &0);
    pool.set_kill_claim(&true);
    reset(&f);
    assert!(f.receipt().try_co_claim().is_err());
    assert_eq!(stream(&f, &next).raw, 0);
    token::StellarAssetClient::new(&f.env, &next).mint(&f.pool, &100_000);
    pool.set_kill_claim(&false);
    reset(&f);
    f.receipt().co_claim();
    assert_eq!(stream(&f, &next).raw, 100_000);
    pool.credit_rewards(&f.strategy, &50_000, &0);
    pool.set_kill_claim(&true);
    reset(&f);
    f.receipt().co_claim(); // now independent quoted debt is permitted
    assert_eq!(stream(&f, &next).raw, 150_000);
    f.env.as_contract(&f.receipt, || {
        assert_eq!(coordinator::owed(&f.env, &next), 50_000)
    });
}

#[test]
fn rotation_debt_bad_floor_failed_unwind_and_uncoordinated_config_roll_back() {
    let f = Fixture::new();
    f.receipt().co_register(&f.reward);
    let (next, route) = second_token(&f);
    let pool = MockAquariusPoolClient::new(&f.env, &f.pool);
    let strategy = AquariusLpVaultClient::new(&f.env, &f.strategy);
    let held = strategy.balance(&f.receipt);
    f.credit(100_000, 0);
    pool.set_kill_claim(&true);
    reset(&f);
    assert!(f.receipt().try_co_rotate(&next, &1).is_err());
    assert_eq!(stream(&f, &f.reward).raw, 0); // even the IOU checkpoint rolled back
    pool.set_kill_claim(&false);
    pool.set_fail_withdraw(&true);
    reset(&f);
    assert!(f.receipt().try_co_prepare_rotate(&1).is_err());
    assert_eq!(f.raw(&f.receipt), 0);
    pool.set_fail_withdraw(&false);
    reset(&f);
    assert!(f.receipt().try_co_prepare_rotate(&2_000_000).is_err());
    assert_eq!(strategy.balance(&f.receipt), held);
    reset(&f);
    f.receipt().co_prepare_rotate(&1);
    pool.set_kill_claim(&true);
    reset(&f);
    assert!(f.receipt().try_co_rotate(&next, &1).is_err());
    assert_eq!(stream(&f, &f.reward).raw, 0);
    pool.set_kill_claim(&false);
    strategy.set_reward_min_rate(&f.admin, &next, &0);
    reset(&f);
    assert!(f.receipt().try_co_rotate(&next, &1).is_err());
    strategy.set_reward_min_rate(&f.admin, &next, &9_000_000);
    strategy.set_reward_route(&f.admin, &next, &Some(f.route.clone()));
    reset(&f);
    assert!(f.receipt().try_co_rotate(&next, &1).is_err());
    strategy.set_reward_route(&f.admin, &next, &Some(route));
    assert_eq!(strategy.balance(&f.receipt), 0);
    assert_eq!(strategy.get_primary_reward_token(), Some(f.reward.clone()));
    strategy.set_primary_reward_token(&f.admin, &Some(next.clone()));
    reset(&f);
    assert!(f
        .receipt()
        .try_co_transfer(&f.user, &Address::generate(&f.env), &1)
        .is_err());
    assert_eq!(f.receipt().balance(&f.user), 1_000_000);
}

#[test]
fn rotation_requires_exact_receipt_admin_authorization() {
    let f = Fixture::new();
    f.receipt().co_register(&f.reward);
    let (next, _) = second_token(&f);
    f.env.mock_auths(&[]);
    assert!(f.receipt().try_co_prepare_rotate(&900_000).is_err());
    assert!(f.receipt().try_co_rotate(&next, &900_000).is_err());
    assert!(AquariusLpVaultClient::new(&f.env, &f.strategy)
        .try_hybrid_rotate_primary(&f.reward, &next)
        .is_err());
    reset(&f);
    f.receipt()
        .mock_auths(&[MockAuth {
            address: &f.receipt_admin,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "co_prepare_rotate",
                args: (900_000u128,).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .co_prepare_rotate(&900_000);
    reset(&f);
    f.receipt()
        .mock_auths(&[MockAuth {
            address: &f.receipt_admin,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "co_rotate",
                args: (next.clone(), 900_000u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .co_rotate(&next, &900_000);
    assert_eq!(
        AquariusLpVaultClient::new(&f.env, &f.strategy).get_primary_reward_token(),
        Some(next)
    );
}

#[test]
fn rotation_gauge_cash_and_failed_primary_claim_do_not_prove_denomination() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    let (next, _) = second_token(&f);
    reset(&f);
    f.receipt().co_rotate(&next, &1);
    let pool = MockAquariusPoolClient::new(&f.env, &f.pool);
    pool.set_reward_tokens(&next, &next);
    token::StellarAssetClient::new(&f.env, &next).mint(&f.pool, &110_000);
    pool.credit_rewards(&f.strategy, &0, &10_000);
    reset(&f);
    f.receipt().co_claim();
    assert_eq!(stream(&f, &next).raw, 10_000);
    pool.credit_rewards(&f.strategy, &100_000, &50_000); // only primary is funded
    reset(&f);
    assert!(f.receipt().try_co_claim().is_err());
    assert_eq!(stream(&f, &next).raw, 10_000);
    assert_eq!(token::Client::new(&f.env, &next).balance(&f.pool), 100_000);
    pool.set_kill_claim(&true);
    reset(&f);
    assert!(f.receipt().try_co_claim().is_err()); // neither gauge nor reverted cash proves primary
    pool.set_kill_claim(&false);
    pool.credit_rewards(&f.strategy, &100_000, &0);
    reset(&f);
    f.receipt().co_claim();
    assert_eq!(stream(&f, &next).raw, 110_000);
    // Previously retired streams can be selected again without resetting history.
    reset(&f);
    f.receipt().co_rotate(&f.reward, &1);
    assert_eq!(stream(&f, &next).raw, 110_000);
    assert!(f.receipt().try_co_register(&next).is_err());
}

#[test]
fn exit_intent_survives_total_reward_outage_and_retries_without_losing_rewards() {
    let f = Fixture::new();
    f.receipt().co_register(&f.reward);
    let (second, _) = second_token(&f);
    credit_two(&f, &second, 100_000, 200_000);
    let pool = MockAquariusPoolClient::new(&f.env, &f.pool);
    pool.set_kill_claim(&true);
    pool.set_fail_reward_quote(&true);
    let before = f.cash(&f.user);
    let nonce = f.receipt().co_request(&f.user, &500_000, &450_000);
    let saved = f.receipt().co_request_get(&f.user);
    reset(&f);
    assert!(f.receipt().try_co_execute(&f.user, &nonce).is_err());
    assert_eq!(f.receipt().co_request_get(&f.user), saved);
    assert_eq!(f.receipt().balance(&f.user), 1_000_000);
    assert_eq!(f.cash(&f.user), before);
    assert_eq!(stream(&f, &f.reward).raw, 0);
    // The user still owns the shares and earns while waiting. No frozen snapshot
    // is used to misattribute future rewards to an exited account.
    credit_two(&f, &second, 120_000, 240_000);
    pool.set_kill_claim(&false);
    pool.set_fail_reward_quote(&false);
    reset(&f);
    let (raw, paid) = f.receipt().co_execute(&f.user, &nonce);
    assert!(paid >= 450_000);
    assert_eq!(raw.get(f.reward.clone()), Some(60_000));
    assert_eq!(raw.get(second), Some(120_000));
    assert_eq!(f.receipt().balance(&f.user), 500_000);
    assert!(!f.receipt().co_request_get(&f.user).unwrap().pending);
    reset(&f);
    assert!(f.receipt().try_co_execute(&f.user, &nonce).is_err());
    state(&f);
}

#[test]
fn exit_intent_minimum_cancel_nonce_and_current_balance_are_enforced() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    assert!(f.receipt().try_co_request(&f.user, &0, &1).is_err());
    assert!(f.receipt().try_co_request(&f.user, &1, &0).is_err());
    assert!(f.receipt().try_co_request(&f.user, &1_000_001, &1).is_err());
    let first = f.receipt().co_request(&f.user, &1_000_000, &2_000_000);
    assert!(f.receipt().try_co_request(&f.user, &1, &1).is_err());
    f.credit(100_000, 0);
    reset(&f);
    assert!(f.receipt().try_co_execute(&f.user, &first).is_err());
    assert_eq!(stream(&f, &f.reward).raw, 0);
    assert!(f.receipt().co_request_get(&f.user).unwrap().pending);
    assert!(f.receipt().try_co_cancel(&f.user, &(first + 1)).is_err());
    f.receipt().co_cancel(&f.user, &first);
    let second = f.receipt().co_request(&f.user, &1_000_000, &1);
    assert_eq!(second, first + 1);
    assert!(f.receipt().try_co_execute(&f.user, &first).is_err());
    let other = Address::generate(&f.env);
    f.receipt().co_transfer(&f.user, &other, &1);
    reset(&f);
    assert!(f.receipt().try_co_execute(&f.user, &second).is_err());
    assert!(f.receipt().co_request_get(&f.user).unwrap().pending);
    assert_eq!(f.receipt().balance(&other), 1);
}

#[test]
fn exit_intent_request_cancel_and_execute_need_exact_owner_auth_and_renew_ttl() {
    let f = Fixture::with_buffer(10_000);
    f.receipt().co_register(&f.reward);
    f.env.mock_auths(&[]);
    assert!(f.receipt().try_co_request(&f.user, &500_000, &1).is_err());
    let first = f
        .receipt()
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "co_request",
                args: (f.user.clone(), 500_000u128, 1u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .co_request(&f.user, &500_000, &1);
    f.env.mock_auths(&[]);
    assert!(f.receipt().try_co_cancel(&f.user, &first).is_err());
    reset(&f);
    assert!(f.receipt().try_co_execute(&f.user, &first).is_err());
    f.env.ledger().set_sequence_number(600_001);
    f.receipt().co_request_get(&f.user);
    f.env.as_contract(&f.receipt, || {
        let key = receipt_core::exit_request::ExitKey::LpExitRequest(f.user.clone());
        assert!(f.env.storage().persistent().get_ttl(&key) > 500_000);
    });
    reset(&f);
    f.receipt()
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "co_execute",
                args: (f.user.clone(), first).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .co_execute(&f.user, &first);
    assert_eq!(f.receipt().balance(&f.user), 500_000);
    f.env.mock_all_auths();
    let next = f.receipt().co_request(&f.user, &1, &1);
    f.env.mock_auths(&[]);
    f.receipt()
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "co_cancel",
                args: (f.user.clone(), next).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .co_cancel(&f.user, &next);
    assert!(!f.receipt().co_request_get(&f.user).unwrap().pending);
}

#[test]
fn rotation_preparation_cannot_be_reused_after_reinvestment_or_skip_tail_rewards() {
    let f = Fixture::new();
    f.receipt().co_register(&f.reward);
    let (next, _) = second_token(&f);
    reset(&f);
    f.receipt().co_prepare_rotate(&900_000);
    let strategy = AquariusLpVaultClient::new(&f.env, &f.strategy);
    assert_eq!(strategy.get_primary_reward_token(), Some(f.reward.clone()));
    f.credit(100_000, 0);
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_000);
    reset(&f);
    f.receipt().co_deposit(&late, &1_000_000);
    assert_eq!(f.receipt().earned(&f.reward, &late).raw_scaled, 0);
    assert!(strategy.balance(&f.receipt) > 0);
    reset(&f);
    assert!(f.receipt().try_co_rotate(&next, &1).is_err());
    assert_eq!(strategy.get_primary_reward_token(), Some(f.reward.clone()));
    reset(&f);
    f.receipt().co_prepare_rotate(&1);
    f.credit(50_000, 0); // pending at completion still needs a FRESH checkpoint
    let before_late = f.receipt().earned(&f.reward, &late).raw_scaled;
    reset(&f);
    f.receipt().co_rotate(&next, &1);
    assert!(f.receipt().earned(&f.reward, &late).raw_scaled > before_late);
    assert_eq!(stream(&f, &f.reward).raw, 150_000);
    assert_eq!(f.raw(&f.receipt), 150_000);
    state(&f);
}
