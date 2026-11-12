//! Separate production/default V3 build with actual receipt markets and actual
//! Aquarius bytecode in controlled ordinary-SAC state. No CL custody, facade or
//! funded-liquidation promotion. All policy/borrowing/MM settings are TEST ONLY.
use super::*;
use soroban_sdk::TryFromVal;

#[contracttype]
#[derive(Clone)]
enum ValidationMarker {
    ClShareAsset(Address),
}

fn fixture() -> F {
    MEASURE.with(|m| m.set(false));
    let mut e = Env::default();
    // Optional file snapshot at Drop reuses an exhausted execution meter and
    // can double-panic after a policy rejection. Resource/auth checks stay on.
    e.set_config(soroban_sdk::testutils::EnvTestConfig {
        capture_snapshot_at_drop: false,
        ..Default::default()
    });
    e.ledger().with_mut(|l| {
        l.timestamp = 1000;
        l.sequence_number = 100;
    });
    crate::cl_resource_limits::configure(&e);
    // Explicit synthetic contract-admin fixture, not a production identity.
    let admin = Address::generate(&e);
    assert_eq!(
        admin,
        Address::from_str(
            &e,
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"
        )
    );
    let user = Address::generate(&e);
    let a_token = e.register_stellar_asset_contract_v2(admin.clone());
    let b_token = e.register_stellar_asset_contract_v2(admin.clone());
    a_token
        .issuer()
        .set_flag(soroban_sdk::testutils::IssuerFlags::RevocableFlag);
    b_token
        .issuer()
        .set_flag(soroban_sdk::testutils::IssuerFlags::RevocableFlag);
    let a = a_token.address();
    let b = b_token.address();
    let (under, paired) = if a < b { (a, b) } else { (b, a) };
    let reward = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let boost = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let bytes = std::fs::read(std::env::var("AQUARIUS_CONCENTRATED_WASM").unwrap()).unwrap();
    let hash = e.crypto().sha256(&Bytes::from_slice(&e, &bytes)).to_array();
    let hex: std::string::String = hash.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        "12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6"
    );
    let pool = e.register(bytes.as_slice(), ());
    let plane = wasm(&e, "cl_integration_fixtures");
    let roles = (
        admin.clone(),
        admin.clone(),
        admin.clone(),
        admin.clone(),
        soroban_sdk::vec![&e, admin.clone()],
        admin.clone(),
    );
    auth::<()>(
        &e,
        &admin,
        &pool,
        "initialize_all",
        (
            admin.clone(),
            roles,
            admin.clone(),
            soroban_sdk::vec![&e, under.clone(), paired.clone()],
            10u32,
            20i32,
            0u32,
            (reward.clone(), boost, plane.clone()),
            plane.clone(),
        )
            .into_val(&e),
        &[],
    );
    for t in [&under, &paired, &reward] {
        auth::<()>(
            &e,
            &admin,
            t,
            "mint",
            (user.clone(), 2_000_000_000_000i128).into_val(&e),
            &[],
        );
    }
    let p0 = MockAuthInvoke {
        contract: &under,
        fn_name: "transfer",
        args: (user.clone(), pool.clone(), 100_000_000_000i128).into_val(&e),
        sub_invokes: &[],
    };
    let p1 = MockAuthInvoke {
        contract: &paired,
        fn_name: "transfer",
        args: (user.clone(), pool.clone(), 100_000_000_000i128).into_val(&e),
        sub_invokes: &[],
    };
    // Pinned CL pool requires sender auth at deposit plus both transfers.
    let desired = soroban_sdk::vec![&e, 100_000_000_000u128, 100_000_000_000u128];
    let (actual, _): (Vec<u128>, u128) = call(
        &e,
        &pool,
        "estimate_deposit_position",
        (-2000i32, 2000i32, desired.clone()).into_val(&e),
    );
    let mut p0 = p0;
    let mut p1 = p1;
    p0.args = (user.clone(), pool.clone(), amount(actual.get(0).unwrap())).into_val(&e);
    p1.args = (user.clone(), pool.clone(), amount(actual.get(1).unwrap())).into_val(&e);
    let _: (Vec<u128>, u128) = auth(
        &e,
        &user,
        &pool,
        "deposit_position",
        (user.clone(), -2000i32, 2000i32, actual, 0u128).into_val(&e),
        &[p0, p1],
    );
    let oracle = wasm(&e, "cl_integration_fixtures");
    let share = paired.clone();
    let facade = pool.clone();
    let default_wasm = std::fs::read(
        std::env::var("CL_DEFAULT_MARGIN_WASM")
            .expect("separate default controller artifact required"),
    )
    .unwrap();
    let margin = e.register(default_wasm.as_slice(), ());
    let cash = wasm(&e, "receipt_vault");
    let receipt = wasm(&e, "receipt_vault");
    let controller = wasm(&e, "simple_peridottroller");
    let adapter = wasm(&e, "swap_adapter");
    auth::<()>(
        &e,
        &admin,
        &controller,
        "initialize",
        (admin.clone(),).into_val(&e),
        &[],
    );
    auth::<()>(
        &e,
        &admin,
        &controller,
        "set_oracle",
        (oracle.clone(),).into_val(&e),
        &[],
    );
    for (v, t, cf) in [
        (&cash, &under, 600_000u128),
        (&receipt, &share, 600_000u128),
    ] {
        auth::<()>(
            &e,
            &admin,
            v,
            "initialize",
            (t.clone(), 0u128, 0u128, admin.clone()).into_val(&e),
            &[],
        );
        auth::<()>(
            &e,
            &admin,
            v,
            "enable_static_rates",
            (admin.clone(),).into_val(&e),
            &[],
        );
        auth::<()>(
            &e,
            &admin,
            v,
            "set_peridottroller",
            (controller.clone(),).into_val(&e),
            &[],
        );
        auth::<()>(
            &e,
            &admin,
            &controller,
            "add_market",
            (v.clone(),).into_val(&e),
            &[],
        );
        auth::<()>(
            &e,
            &admin,
            &controller,
            "set_market_cf",
            (v.clone(), cf).into_val(&e),
            &[],
        );
        auth::<()>(
            &e,
            &admin,
            v,
            "set_collateral_factor",
            (cf,).into_val(&e),
            &[],
        );
        let _: Option<(u128, u128)> =
            call(&e, &controller, "cache_price", (t.clone(),).into_val(&e));
    }
    auth::<()>(
        &e,
        &admin,
        &adapter,
        "initialize",
        (admin.clone(), pool.clone()).into_val(&e),
        &[],
    );
    auth::<()>(
        &e,
        &admin,
        &adapter,
        "set_pool_allowed",
        (admin.clone(), margin.clone(), true).into_val(&e),
        &[],
    );
    auth::<()>(
        &e,
        &admin,
        &controller,
        "set_margin_liquidation_ctrl",
        (margin.clone(), true).into_val(&e),
        &[],
    );
    auth::<()>(
        &e,
        &admin,
        &margin,
        "initialize",
        (admin.clone(), controller.clone(), adapter.clone(), 2u128).into_val(&e),
        &[],
    );
    for (v, t) in [(&cash, &under), (&receipt, &share)] {
        auth::<()>(
            &e,
            &admin,
            &margin,
            "set_market",
            (admin.clone(), t.clone(), v.clone()).into_val(&e),
            &[],
        );
        auth::<()>(
            &e,
            &admin,
            v,
            "set_margin_controller",
            (admin.clone(), Some(margin.clone())).into_val(&e),
            &[],
        );
    }
    let binding = soroban_sdk::BytesN::from_array(&e, &[17; 32]);
    auth::<()>(
        &e,
        &admin,
        &adapter,
        "set_pool_allowed",
        (admin.clone(), facade.clone(), true).into_val(&e),
        &[],
    );
    auth::<()>(
        &e,
        &admin,
        &adapter,
        "set_pool_binding",
        (admin.clone(), binding.clone(), facade.clone(), true).into_val(&e),
        &[],
    );
    crate::resource_profile::register_contracts(
        &e,
        &[
            ("v3", &margin),
            ("aquarius", &pool),
            ("cash_receipt", &cash),
            ("share_receipt", &receipt),
            ("adapter", &adapter),
            ("lending_controller", &controller),
            ("oracle", &oracle),
            ("plane", &plane),
            ("underlying", &under),
            ("paired", &paired),
            ("reward", &reward),
        ],
    );
    let f = F {
        e,
        admin,
        user,
        under,
        paired,
        reward,
        pool,
        share,
        facade,
        margin,
        cash,
        receipt,
        binding,
        oracle,
        controller,
    };
    f.deposit_cash(1_000_000_000);
    // Test-only maintenance margin: 25%; it is NOT an approved deployment value.
    let _: () = auth(
        &f.e,
        &f.admin,
        &f.margin,
        "set_perps_pair_config",
        (
            f.admin.clone(),
            f.under.clone(),
            f.share.clone(),
            Side::Long,
            2u128,
            250_000u128,
            80_000u128,
        )
            .into_val(&f.e),
        &[],
    );
    MEASURE.with(|m| m.set(true));
    f
}

fn policy(f: &F, bps: u32, enabled: bool) {
    auth::<()>(
        &f.e,
        &f.admin,
        &f.cash,
        "set_min_cash_reserve_bps",
        (f.admin.clone(), bps).into_val(&f.e),
        &[],
    );
    auth::<()>(
        &f.e,
        &f.admin,
        &f.cash,
        "set_cash_borrowing_enabled",
        (f.admin.clone(), enabled).into_val(&f.e),
        &[],
    );
}
fn step(f: &F, id: u64, name: &str) {
    auth::<()>(
        &f.e,
        &f.user,
        &f.margin,
        name,
        (f.user.clone(), id).into_val(&f.e),
        &[],
    );
    measure(&f.e, &format!("default_{name}"), true);
}
fn absent(f: &F, id: u64) {
    let value: Val = call(&f.e, &f.margin, "get_position", (id,).into_val(&f.e));
    assert_eq!(
        soroban_sdk::xdr::ScVal::try_from_val(&f.e, &value).unwrap(),
        soroban_sdk::xdr::ScVal::Void
    );
}
fn reject(
    f: &F,
    who: &Address,
    name: &str,
    args: Vec<Val>,
    subs: &[MockAuthInvoke],
    label: &str,
    frame: &str,
) {
    let e = &f.e;
    let permissionless = name == "resolve_pending_open_v3";
    if permissionless {
        e.mock_auths(&[]);
    } else {
        e.mock_auths(&[MockAuth {
            address: who,
            invoke: &MockAuthInvoke {
                contract: &f.margin,
                fn_name: name,
                args: args.clone(),
                sub_invokes: subs,
            },
        }]);
    }
    let before = e.to_ledger_snapshot().ledger_entries;
    #[cfg(feature = "transaction-evidence")]
    if frame == "Error(Auth" {
        // Recording-mode unsigned replay cannot prove missing nested external
        // authorization. Record only its positive execution costs; the actual
        // scoped SDK invocation below proves the auth rejection and rollback.
        crate::transaction_evidence::capture_before(e, &f.margin, name, &args, Some(who));
    } else {
        crate::transaction_evidence::capture_rejection_before(
            e,
            &f.margin,
            name,
            &args,
            if permissionless { None } else { Some(who) },
            permissionless,
            label,
        );
    }
    crate::cl_resource_limits::reset(e);
    assert!(
        e.try_invoke_contract::<Val, soroban_sdk::Error>(&f.margin, &Symbol::new(e, name), args)
            .is_err(),
        "{label}"
    );
    let events = format!("{:?}", e.host().get_diagnostic_events().unwrap());
    assert!(
        events.contains(frame),
        "{label}: expected {frame}: {events}"
    );
    if frame != "Error(Auth" {
        assert!(
            !events.contains("Error(Auth"),
            "unexpected auth rejection: {label}"
        );
    }
    measure(e, label, true);
    assert_eq!(
        e.to_ledger_snapshot().ledger_entries,
        before,
        "{label}: full rollback"
    );
    std::println!(
        "DEFAULT_RESERVE_ROLLBACK {label} complete_ledger_equal=true expected_frame={frame}"
    );
}
fn recover(f: &F, id: u64, debt_vault: &Address, debt_asset: &Address, collateral: &Address) {
    let e = &f.e;
    let max = 210_000_000u128;
    let transfer = MockAuthInvoke {
        contract: debt_asset,
        fn_name: "transfer",
        args: (f.user.clone(), debt_vault.clone(), max as i128).into_val(e),
        sub_invokes: &[],
    };
    let transfers = [transfer];
    let repay = MockAuthInvoke {
        contract: debt_vault,
        fn_name: "repay_full_for_margin",
        args: (id, f.user.clone(), max).into_val(e),
        sub_invokes: &transfers,
    };
    let held = token::Client::new(e, collateral).balance(&f.margin);
    let wallet = token::Client::new(e, collateral).balance(&f.user);
    let paid: u128 = auth(
        e,
        &f.user,
        &f.margin,
        "recover_pending_open_funded",
        (f.user.clone(), id, max).into_val(e),
        &[repay],
    );
    measure(e, "default_pending_funded_recovery", true);
    assert!(paid > 0);
    assert_eq!(
        call::<u128>(
            e,
            debt_vault,
            "get_margin_borrow_balance",
            (id,).into_val(e)
        ),
        0
    );
    assert_eq!(token::Client::new(e, collateral).balance(&f.margin), 0);
    assert_eq!(
        token::Client::new(e, collateral).balance(&f.user),
        wallet + held
    );
    absent(f, id);
}

#[test]
fn compiled_default_reserve_active_repay_release() {
    let f = fixture();
    let e = &f.e;
    policy(&f, 6000, true);
    let id = begin(&f);
    step(&f, id, "swap_open_position_v3");
    step(&f, id, "activate_open_position_v3");
    call::<()>(e, &f.cash, "assert_borrowing_reserve", ().into_val(e));
    policy(&f, 10000, false);
    assert_eq!(
        call::<u128>(e, &f.cash, "get_borrowable_cash", ().into_val(e)),
        0
    );
    let debt: u128 = call(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e));
    let transfers = [MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.cash.clone(), debt as i128).into_val(e),
        sub_invokes: &[],
    }];
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_for_margin",
        args: (id, f.user.clone(), debt).into_val(e),
        sub_invokes: &transfers,
    };
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "repay_margin_position_v3",
        (f.user.clone(), id, debt).into_val(e),
        &[repay],
    );
    measure(e, "default_active_full_repayment_paused_shortfall", true);
    step(&f, id, "release_debt_free_position_v3");
    let free: u128 = call(
        e,
        &f.margin,
        "get_margin_balance_ptokens",
        (f.user.clone(), f.share.clone()).into_val(e),
    );
    assert!(free > 0);
    assert_eq!(
        token::Client::new(e, &f.receipt).balance(&f.margin),
        free as i128
    );
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    absent(&f, id);
}

#[test]
fn compiled_default_reserve_post_cost_finalizers_rollback_recover() {
    for name in [
        "activate_open_position_v3",
        "execute_open_position_v3",
        "resolve_pending_open_v3",
    ] {
        for enabled in [false, true] {
            let f = fixture();
            let e = &f.e;
            policy(&f, 6000, true);
            auth::<()>(
                e,
                &f.admin,
                &f.margin,
                "set_open_fee_bps",
                (f.admin.clone(), 10u128).into_val(e),
                &[],
            );
            let extra = [MockAuthInvoke {
                contract: &f.cash,
                fn_name: "transfer",
                args: (f.user.clone(), f.margin.clone(), 200_000i128).into_val(e),
                sub_invokes: &[],
            }];
            auth::<()>(
                e,
                &f.user,
                &f.margin,
                "transfer_spot_to_margin",
                (f.user.clone(), f.under.clone(), 200_000u128).into_val(e),
                &extra,
            );
            let id = begin(&f);
            step(&f, id, "swap_open_position_v3");
            policy(&f, 10000, enabled);
            let free_before: u128 = call(
                e,
                &f.margin,
                "get_margin_balance_ptokens",
                (f.user.clone(), f.under.clone()).into_val(e),
            );
            let frame = if enabled {
                "StringM(assert_borrowing_reserve)"
            } else {
                "get_cash_borrowing_enabled"
            };
            let args = if name == "resolve_pending_open_v3" {
                e.ledger().with_mut(|l| {
                    l.timestamp = 3000;
                    l.sequence_number += 400;
                });
                (id,).into_val(e)
            } else {
                (f.user.clone(), id).into_val(e)
            };
            reject(
                &f,
                &f.user,
                name,
                args,
                &[],
                "default_post_cost_reserve_reject",
                frame,
            );
            recover(&f, id, &f.cash, &f.under, &f.share);
            assert_eq!(
                call::<u128>(
                    e,
                    &f.margin,
                    "get_margin_balance_ptokens",
                    (f.user.clone(), f.under.clone()).into_val(e)
                ),
                free_before + 200_000,
                "reserved open-fee refund"
            );
            reject(
                &f,
                &f.user,
                "recover_pending_open_funded",
                (f.user.clone(), id, 210_000_000u128).into_val(e),
                &[],
                "default_pending_recovery_repeat_reject",
                "StringM(recover_pending_open_funded)",
            );
        }
    }
}

#[test]
fn compiled_default_pending_recovery_adversarial() {
    let f = fixture();
    let e = &f.e;
    policy(&f, 6000, true);
    let id = begin(&f);
    step(&f, id, "swap_open_position_v3");
    policy(&f, 10000, false);
    let other = Address::generate(e);
    reject(
        &f,
        &other,
        "recover_pending_open_funded",
        (other.clone(), id, 210_000_000u128).into_val(e),
        &[],
        "default_pending_wrong_owner_reject",
        "StringM(recover_pending_open_funded)",
    );
    reject(
        &f,
        &f.user,
        "recover_pending_open_funded",
        (f.user.clone(), id, 1u128).into_val(e),
        &[],
        "default_pending_insufficient_max_reject",
        "StringM(repay_full_for_margin)",
    );
    reject(
        &f,
        &f.user,
        "recover_pending_open_funded",
        (f.user.clone(), id, 210_000_000u128).into_val(e),
        &[],
        "default_pending_missing_nested_auth_reject",
        "Error(Auth",
    );
    // Persisted validation custody cannot be downgraded into a hook-free exit.
    e.as_contract(&f.margin, || {
        e.storage()
            .instance()
            .set(&ValidationMarker::ClShareAsset(f.share.clone()), &true)
    });
    reject(
        &f,
        &f.user,
        "recover_pending_open_funded",
        (f.user.clone(), id, 210_000_000u128).into_val(e),
        &[],
        "default_validation_custody_downgrade_reject",
        "StringM(recover_pending_open_funded)",
    );
    e.as_contract(&f.margin, || {
        e.storage()
            .instance()
            .remove(&ValidationMarker::ClShareAsset(f.share.clone()))
    });
    let balance = token::Client::new(e, &f.under).balance(&f.user);
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), other.clone(), balance - 1).into_val(e),
        &[],
    );
    let funding_transfers = [MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.cash.clone(), 210_000_000i128).into_val(e),
        sub_invokes: &[],
    }];
    let funding_repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_full_for_margin",
        args: (id, f.user.clone(), 210_000_000u128).into_val(e),
        sub_invokes: &funding_transfers,
    };
    reject(
        &f,
        &f.user,
        "recover_pending_open_funded",
        (f.user.clone(), id, 210_000_000u128).into_val(e),
        &[funding_repay],
        "default_pending_insufficient_wallet_rollback",
        "balance",
    );
    auth::<()>(
        e,
        &other,
        &f.under,
        "transfer",
        (other.clone(), f.user.clone(), balance - 1).into_val(e),
        &[],
    );
    // Actual SAC rejects the final collateral transfer after the debt repayment.
    auth::<()>(
        e,
        &f.admin,
        &f.share,
        "set_authorized",
        (f.user.clone(), false).into_val(e),
        &[],
    );
    let transfers = [MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.cash.clone(), 210_000_000i128).into_val(e),
        sub_invokes: &[],
    }];
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_full_for_margin",
        args: (id, f.user.clone(), 210_000_000u128).into_val(e),
        sub_invokes: &transfers,
    };
    reject(
        &f,
        &f.user,
        "recover_pending_open_funded",
        (f.user.clone(), id, 210_000_000u128).into_val(e),
        &[repay],
        "default_pending_collateral_transfer_failure_rollback",
        "balance is deauthorized",
    );
    auth::<()>(
        e,
        &f.admin,
        &f.share,
        "set_authorized",
        (f.user.clone(), true).into_val(e),
        &[],
    );
    recover(&f, id, &f.cash, &f.under, &f.share);
}

#[test]
fn compiled_default_short_pending_recovery_paused_shortfall() {
    let f = fixture();
    let e = &f.e;
    let transfers = [MockAuthInvoke {
        contract: &f.share,
        fn_name: "transfer",
        args: (f.user.clone(), f.receipt.clone(), 1_000_000_000i128).into_val(e),
        sub_invokes: &[],
    }];
    auth::<()>(
        e,
        &f.user,
        &f.receipt,
        "deposit",
        (f.user.clone(), 1_000_000_000u128).into_val(e),
        &transfers,
    );
    for (name, args) in [
        (
            "set_min_cash_reserve_bps",
            (f.admin.clone(), 6000u32).into_val(e),
        ),
        (
            "set_cash_borrowing_enabled",
            (f.admin.clone(), true).into_val(e),
        ),
    ] {
        auth::<()>(e, &f.admin, &f.receipt, name, args, &[]);
    }
    auth::<()>(
        e,
        &f.admin,
        &f.margin,
        "set_perps_pair_config",
        (
            f.admin.clone(),
            f.under.clone(),
            f.share.clone(),
            Side::Short,
            2u128,
            250_000u128,
            80_000u128,
        )
            .into_val(e),
        &[],
    );
    let transfers = [MockAuthInvoke {
        contract: &f.cash,
        fn_name: "transfer",
        args: (f.user.clone(), f.margin.clone(), 100_000_000i128).into_val(e),
        sub_invokes: &[],
    }];
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "transfer_spot_to_margin",
        (f.user.clone(), f.under.clone(), 100_000_000u128).into_val(e),
        &transfers,
    );
    let id: u64 = auth(
        e,
        &f.user,
        &f.margin,
        "begin_open_position_v3",
        (
            f.user.clone(),
            f.under.clone(),
            f.share.clone(),
            100_000_000u128,
            2u128,
            Side::Short,
            soroban_sdk::vec![e, f.under.clone(), f.share.clone()],
            f.binding.clone(),
            f.pool.clone(),
            194_040_000u128,
        )
            .into_val(e),
        &[],
    );
    step(&f, id, "swap_open_position_v3");
    auth::<()>(
        e,
        &f.admin,
        &f.receipt,
        "set_min_cash_reserve_bps",
        (f.admin.clone(), 10000u32).into_val(e),
        &[],
    );
    auth::<()>(
        e,
        &f.admin,
        &f.receipt,
        "set_cash_borrowing_enabled",
        (f.admin.clone(), false).into_val(e),
        &[],
    );
    recover(&f, id, &f.receipt, &f.share, &f.under);
}

#[test]
fn compiled_default_missing_receipt_abi_fails_closed() {
    let f = fixture();
    let e = &f.e;
    // ABI-capable, explicitly unconfigured None retains legacy behavior.
    assert_eq!(
        call::<Option<u32>>(e, &f.cash, "get_min_cash_reserve_bps", ().into_val(e)),
        None
    );
    let id = begin(&f);
    step(&f, id, "swap_open_position_v3");
    // Controlled historical executable substitution, never a deployment action:
    // receipt-vault source at leveraged-fix 47858c1 predates the reserve ABI.
    let legacy = std::fs::read(
        std::env::var("CL_LEGACY_RECEIPT_WASM").expect("historical receipt artifact required"),
    )
    .unwrap();
    let hash = e.deployer().upload_contract_wasm(legacy.as_slice());
    e.as_contract(&f.cash, || e.deployer().update_current_contract_wasm(hash));
    reject(
        &f,
        &f.user,
        "activate_open_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
        "default_missing_reserve_abi_reject",
        "get_min_cash_reserve_bps",
    );
    // The exact baseline already supports repay_full_for_margin: recovery
    // remains available even though its reserve ABI is absent.
    recover(&f, id, &f.cash, &f.under, &f.share);
}

#[test]
fn compiled_default_pending_fee_refund_preserves_historical_fee_owner() {
    let f = fixture();
    let e = &f.e;
    policy(&f, 6000, true);
    auth::<()>(
        e,
        &f.admin,
        &f.margin,
        "set_open_fee_bps",
        (f.admin.clone(), 10u128).into_val(e),
        &[],
    );
    let bob = Address::generate(e);
    auth::<()>(
        e,
        &f.user,
        &f.cash,
        "transfer",
        (f.user.clone(), bob.clone(), 100_000_000i128).into_val(e),
        &[],
    );
    let bob_transfer = [MockAuthInvoke {
        contract: &f.cash,
        fn_name: "transfer",
        args: (bob.clone(), f.margin.clone(), 100_000_000i128).into_val(e),
        sub_invokes: &[],
    }];
    auth::<()>(
        e,
        &bob,
        &f.margin,
        "transfer_spot_to_margin",
        (bob.clone(), f.under.clone(), 100_000_000u128).into_val(e),
        &bob_transfer,
    );
    let extra_fee = || {
        let sub = [MockAuthInvoke {
            contract: &f.cash,
            fn_name: "transfer",
            args: (f.user.clone(), f.margin.clone(), 200_000i128).into_val(e),
            sub_invokes: &[],
        }];
        auth::<()>(
            e,
            &f.user,
            &f.margin,
            "transfer_spot_to_margin",
            (f.user.clone(), f.under.clone(), 200_000u128).into_val(e),
            &sub,
        );
    };
    extra_fee();
    let pending = begin(&f);
    step(&f, pending, "swap_open_position_v3");
    assert_eq!(
        call::<u128>(
            e,
            &f.margin,
            "get_margin_balance_ptokens",
            (f.user.clone(), f.under.clone()).into_val(e)
        ),
        0
    );
    // A different position completes and charges its fee while the first
    // position's 200k fee remains reserved and ineligible for fee earnings.
    extra_fee();
    let active = begin(&f);
    step(&f, active, "swap_open_position_v3");
    step(&f, active, "activate_open_position_v3");
    assert_eq!(
        call::<u128>(
            e,
            &f.margin,
            "distribute_margin_fees",
            (f.cash.clone(),).into_val(e)
        ),
        200_000
    );
    measure(e, "default_intervening_fee_epoch_distribution", true);
    assert_eq!(
        call::<u128>(
            e,
            &f.margin,
            "get_claimable_margin_fees",
            (f.user.clone(), f.under.clone()).into_val(e)
        ),
        0
    );
    assert_eq!(
        call::<u128>(
            e,
            &f.margin,
            "get_claimable_margin_fees",
            (bob.clone(), f.under.clone()).into_val(e)
        ),
        200_000
    );
    policy(&f, 10000, false);
    recover(&f, pending, &f.cash, &f.under, &f.share);
    assert_eq!(
        call::<u128>(
            e,
            &f.margin,
            "get_margin_balance_ptokens",
            (f.user.clone(), f.under.clone()).into_val(e)
        ),
        200_000
    );
    let alice_fee: u128 = auth(
        e,
        &f.user,
        &f.margin,
        "claim_margin_fees",
        (f.user.clone(), f.under.clone()).into_val(e),
        &[],
    );
    assert_eq!(
        alice_fee, 0,
        "refunded fee shares must not earn an earlier epoch"
    );
    let bob_fee: u128 = auth(
        e,
        &bob,
        &f.margin,
        "claim_margin_fees",
        (bob.clone(), f.under.clone()).into_val(e),
        &[],
    );
    assert_eq!(
        bob_fee, 200_000,
        "original free-margin provider keeps full entitlement"
    );
    assert_eq!(
        auth::<u128>(
            e,
            &bob,
            &f.margin,
            "claim_margin_fees",
            (bob.clone(), f.under.clone()).into_val(e),
            &[]
        ),
        0
    );
    std::println!("DEFAULT_PENDING_FEE_CONSERVATION reserved_refund=200000 historical_fee=200000 alice_fee=0 bob_fee=200000 repeat=0");
}
