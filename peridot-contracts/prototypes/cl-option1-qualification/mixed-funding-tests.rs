//! Companion only: positive own proceeds AND external deficit in one root.
//! Actual compiled contracts, controlled synthetic assets/prices/accounts.
use super::*;
use soroban_sdk::xdr::{
    ContractEventBody, LedgerEntryData, LedgerKey, Limits, ScError, ScVal, WriteXdr,
};
use soroban_sdk::{testutils::Events, TryFromVal};
const MIXED: &str = "settle_stalled_funded_v3";
const V3: &str = "ad9fbb7669ff126ff54e8d9b021d610b9313cb5f3dc595df6e9d7dabc3d7d482";

fn mixed_guard() {
    use sha2::{Digest, Sha256};
    let build = std::path::PathBuf::from(std::env::var("CL_ATOMIC_BUILD_DIR").unwrap());
    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(build.join("build-manifest.json")).unwrap()).unwrap();
    assert_eq!(m["mixed_funding_companion"], true);
    assert_eq!(m["margin_build_feature"], "funded-liquidation-validation");
    assert_eq!(m["wasm"]["margin_controller.wasm"]["sha256"], V3);
    assert_eq!(m["release_approval"], false);
    assert_eq!(
        m["mixed_test_sha256"],
        format!(
            "{:x}",
            Sha256::digest(include_bytes!("mixed-funding-tests.rs"))
        )
    );
    for name in [
        "margin_controller.wasm",
        "receipt_vault.wasm",
        "simple_peridottroller.wasm",
    ] {
        assert_eq!(
            m["wasm"][name]["sha256"],
            format!(
                "{:x}",
                Sha256::digest(std::fs::read(build.join("artifacts").join(name)).unwrap())
            )
        );
    }
}
fn mixed_auth(
    e: &Env,
    margin: &Address,
    cash: &Address,
    under: &Address,
    payer: &Address,
    id: u64,
    max: u128,
) {
    let transfers = [MockAuthInvoke {
        contract: under,
        fn_name: "transfer",
        args: (payer.clone(), cash.clone(), max as i128).into_val(e),
        sub_invokes: &[],
    }];
    let repay = [MockAuthInvoke {
        contract: cash,
        fn_name: "repay_full_for_margin",
        args: (id, payer.clone(), max).into_val(e),
        sub_invokes: &transfers,
    }];
    e.mock_auths(&[MockAuth {
        address: payer,
        invoke: &MockAuthInvoke {
            contract: margin,
            fn_name: MIXED,
            args: (payer.clone(), id, max, 0u128, 0u128).into_val(e),
            sub_invokes: &repay,
        },
    }]);
}
fn fee_records(f: &F) -> std::collections::BTreeMap<std::vec::Vec<u8>, std::vec::Vec<u8>> {
    let contract: soroban_sdk::xdr::ScAddress = (&f.margin).into();
    let mut result = std::collections::BTreeMap::new();
    for (k, (v, _)) in f.e.to_ledger_snapshot().ledger_entries {
        let LedgerKey::ContractData(d) = k.as_ref() else {
            continue;
        };
        if d.contract != contract {
            continue;
        }
        let ScVal::Vec(Some(vs)) = &d.key else {
            continue;
        };
        let Some(ScVal::Symbol(tag)) = vs.first() else {
            continue;
        };
        let name = tag.0.to_vec();
        if [
            b"MarginFeeEpoch".as_slice(),
            b"ClosedMarginFeeEpoch",
            b"UserMarginFeeEpoch",
            b"PendingMarginFees",
            b"UserMarginFeeAccrued",
            b"MarginFeeIndex",
            b"UserMarginFeeIndex",
            b"MarginFeeOrphan",
            b"MarginFeeRemainder",
        ]
        .contains(&name.as_slice())
        {
            assert!(matches!(v.data, LedgerEntryData::ContractData(_)));
            result.insert(
                k.to_xdr(Limits::none()).unwrap(),
                v.data.to_xdr(Limits::none()).unwrap(),
            );
        }
    }
    assert!(!result.is_empty());
    result
}
fn preview_fee_claim(f: &F, provider: &Address) -> (u128, u128) {
    // Verification only on a separate clone. Actual reserved balances remain
    // undistributed during the measured settlement; real distribution is tested
    // afterwards and measured separately, not hidden outside the root.
    let mut clone = Env::from_snapshot(f.e.to_snapshot());
    clone.set_config(soroban_sdk::testutils::EnvTestConfig {
        capture_snapshot_at_drop: false,
        ..Default::default()
    });
    crate::cl_resource_limits::configure(&clone);
    let a = |old: &Address| Address::from_str(&clone, &old.to_string().to_string());
    let margin = a(&f.margin);
    let cash = a(&f.cash);
    let asset = a(&f.under);
    let who = a(provider);
    clone.mock_auths(&[]);
    crate::cl_resource_limits::reset(&clone);
    let distributed: u128 = clone.invoke_contract(
        &margin,
        &Symbol::new(&clone, "distribute_margin_fees"),
        (cash,).into_val(&clone),
    );
    crate::cl_resource_limits::reset(&clone);
    let claim: u128 = clone.invoke_contract(
        &margin,
        &Symbol::new(&clone, "get_claimable_margin_fees"),
        (who, asset).into_val(&clone),
    );
    (distributed, claim)
}
fn reject_insufficient(f: &F, payer: &Address, id: u64, max: u128) {
    let e = &f.e;
    let args = (payer.clone(), id, max, 0u128, 0u128).into_val(e);
    mixed_auth(e, &f.margin, &f.cash, &f.under, payer, id, max);
    crate::cl_resource_limits::reset(e);
    let before = e.to_ledger_snapshot();
    #[cfg(feature = "transaction-evidence")]
    crate::transaction_evidence::capture_rejection_before(
        e,
        &f.margin,
        MIXED,
        &args,
        Some(payer),
        false,
        "mixed_insufficient_max",
    );
    crate::cl_resource_limits::reset(e);
    let result =
        e.try_invoke_contract::<Val, soroban_sdk::Error>(&f.margin, &Symbol::new(e, MIXED), args);
    assert!(result.is_err());
    let events = e.host().get_diagnostic_events().unwrap();
    assert!(events.0.iter().any(|event| {
        let ContractEventBody::V0(b) = &event.event.body;
        b.topics.len() == 3
            && b.topics[0] == ScVal::Symbol("fn_call".try_into().unwrap())
            && b.topics[2] == ScVal::Symbol(MIXED.try_into().unwrap())
    }));
    assert!(
        !events.0.iter().any(|event| {
            let ContractEventBody::V0(b) = &event.event.body;
            b.topics
                .iter()
                .any(|v| matches!(v, ScVal::Error(ScError::Auth(_))))
        }),
        "business rejection must not be an authorization failure"
    );
    assert_eq!(
        before,
        e.to_ledger_snapshot(),
        "complete mixed-payment rejection rollback including TTLs"
    );
    measure(e, "option1_mixed_insufficient_max_rollback", true);
}
fn reject_mainnet_clone(f: &F, payer: &Address, id: u64, max: u128) {
    use sha2::{Digest, Sha256};
    let mut clone = Env::from_snapshot(f.e.to_snapshot());
    clone.set_config(soroban_sdk::testutils::EnvTestConfig {
        capture_snapshot_at_drop: false,
        ..Default::default()
    });
    crate::cl_resource_limits::configure(&clone);
    clone.ledger().with_mut(|l| {
        l.network_id = Sha256::digest(b"Public Global Stellar Network ; September 2015").into()
    });
    let a = |old: &Address| Address::from_str(&clone, &old.to_string().to_string());
    let margin = a(&f.margin);
    let cash = a(&f.cash);
    let under = a(&f.under);
    let who = a(payer);
    mixed_auth(&clone, &margin, &cash, &under, &who, id, max);
    let args = (who.clone(), id, max, 0u128, 0u128).into_val(&clone);
    crate::cl_resource_limits::reset(&clone);
    let before = clone.to_ledger_snapshot();
    #[cfg(feature = "transaction-evidence")]
    crate::transaction_evidence::capture_rejection_before(
        &clone,
        &margin,
        MIXED,
        &args,
        Some(&who),
        false,
        "mixed_mainnet_validation_disabled",
    );
    crate::cl_resource_limits::reset(&clone);
    assert!(clone
        .try_invoke_contract::<Val, soroban_sdk::Error>(&margin, &Symbol::new(&clone, MIXED), args)
        .is_err());
    let events = clone.host().get_diagnostic_events().unwrap();
    let calls: std::vec::Vec<_> = events
        .0
        .iter()
        .filter_map(|event| {
            let ContractEventBody::V0(b) = &event.event.body;
            (b.topics.len() == 3 && b.topics[0] == ScVal::Symbol("fn_call".try_into().unwrap()))
                .then(|| b.topics[2].clone())
        })
        .collect();
    assert_eq!(
        calls,
        std::vec![ScVal::Symbol(MIXED.try_into().unwrap())],
        "guard rejects at root before nested financial/auth calls"
    );
    assert_eq!(before, clone.to_ledger_snapshot());
    measure(&clone, "option1_mixed_mainnet_validation_reject", true);
}
fn payment_order(f: &F, payer: &Address, cursor: usize, used: u128, needed: u128, max: u128) {
    let e = &f.e;
    let address = |a: &Address| ScVal::try_from_val(e, &a.to_val()).unwrap();
    let controller = address(&f.margin);
    let cash = address(&f.cash);
    let who = address(payer);
    let value = |n: u128| {
        let val: Val = (n as i128).into_val(e);
        ScVal::try_from_val(e, &val).unwrap()
    };
    let token_id: soroban_sdk::xdr::ScAddress = (&f.under).into();
    let soroban_sdk::xdr::ScAddress::Contract(token_id) = token_id else {
        unreachable!()
    };
    let all = e.events().all();
    let mut order = std::vec::Vec::new();
    for event in all.events().iter().skip(cursor) {
        if event.contract_id.as_ref() != Some(&token_id) {
            continue;
        }
        let ContractEventBody::V0(body) = &event.body;
        if body.topics.len() < 3 || body.topics[0] != ScVal::Symbol("transfer".try_into().unwrap())
        {
            continue;
        }
        if body.topics[1] == controller && body.topics[2] == cash && body.data == value(used) {
            order.push("own_proceeds");
        }
        if body.topics[1] == who && body.topics[2] == cash && body.data == value(max) {
            order.push("authorized_maximum");
        }
        if body.topics[1] == cash && body.topics[2] == who && body.data == value(max - needed) {
            order.push("exact_refund");
        }
    }
    assert_eq!(
        order,
        std::vec!["own_proceeds", "authorized_maximum", "exact_refund"]
    );
}

#[test]
fn compiled_option1_mixed_proceeds_and_external_deficit_preserve_reserved_fees() {
    mixed_guard();
    let (f, owners, ids, selected) = fixture(true, true);
    let e = &f.e;
    let owner = &owners[1];
    let id = ids[0];
    let provider = &owners[2];
    let fee_payer = &owners[3];
    // Reserve a nonzero fee exclusively through real public operations. At
    // charge time this provider is the only free cash-margin beneficiary.
    for (who, n) in [(provider, 10_000_000u128), (fee_payer, 500_000u128)] {
        let transfer = MockAuthInvoke {
            contract: &f.cash,
            fn_name: "transfer",
            args: (who.clone(), f.margin.clone(), n as i128).into_val(e),
            sub_invokes: &[],
        };
        auth::<()>(
            e,
            who,
            &f.margin,
            "transfer_spot_to_margin",
            (who.clone(), f.under.clone(), n).into_val(e),
            &[transfer],
        );
    }
    auth::<()>(
        e,
        &f.admin,
        &f.margin,
        "set_open_fee_bps",
        (f.admin.clone(), 100u128).into_val(e),
        &[],
    );
    let fee_id = begin_for(&f, fee_payer);
    auth::<()>(
        e,
        fee_payer,
        &f.margin,
        "swap_open_position_v3",
        (fee_payer.clone(), fee_id).into_val(e),
        &[],
    );
    auth::<()>(
        e,
        fee_payer,
        &f.margin,
        "activate_open_position_v3",
        (fee_payer.clone(), fee_id).into_val(e),
        &[],
    );
    let reserved: Map<Symbol, Val> = inspect(
        &f,
        &f.margin,
        "get_undistributed_margin_fees",
        (f.cash.clone(),).into_val(e),
    );
    assert_eq!(field::<u128>(e, &reserved, "ptokens"), 500_000);
    assert_eq!(field::<u128>(e, &reserved, "underlying"), 0);
    assert_eq!(preview_fee_claim(&f, provider), (500_000, 500_000));
    // Convert actual Closing collateral, then age debt WITHOUT finish_close.
    assert!(call::<u128>(e, &f.share, "refresh_nav_root", ().into_val(e)) > 0);
    auth::<()>(
        e,
        owner,
        &f.margin,
        "prepare_close_position_v3",
        (owner.clone(), id).into_val(e),
        &[],
    );
    let pending: Option<Map<Symbol, Val>> =
        inspect(&f, &f.margin, "get_pending_perps_close", (id,).into_val(e));
    let held: u128 = field(e, &pending.unwrap(), "collateral_underlying");
    let quote: u128 = inspect(
        &f,
        &f.facade,
        "estimate_swap",
        (1u32, 0u32, held).into_val(e),
    );
    auth::<()>(
        e,
        owner,
        &f.margin,
        "swap_close_position_v3",
        (owner.clone(), id, quote).into_val(e),
        &[],
    );
    let pending: Option<Map<Symbol, Val>> =
        inspect(&f, &f.margin, "get_pending_perps_close", (id,).into_val(e));
    let pending = pending.unwrap();
    let expiry: u64 = field(e, &pending, "expires_at");
    let proceeds: u128 = field(e, &pending, "received_debt_asset");
    assert!(proceeds > 0);
    // Deliberately artificial interest horizon, not elapsed-ledger archive proof.
    advance(&f, expiry + 30 * 365 * 24 * 60 * 60);
    for asset in [&f.under, &f.share] {
        auth::<()>(
            e,
            &f.admin,
            &f.oracle,
            "set_test_price",
            (
                f.admin.clone(),
                Asset::Stellar(asset.clone()),
                100_000_000_000_000i128,
            )
                .into_val(e),
            &[],
        );
    }
    let debt: u128 = inspect(&f, &f.cash, "get_margin_borrow_balance", (id,).into_val(e));
    assert!(debt > proceeds);
    let needed = debt - proceeds;
    let maximum = needed.checked_add(5_000_000).unwrap();
    assert!(maximum < 300_000_000);
    let payer = fresh_liquidator(&f, &owners);
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), payer.clone(), 300_000_000i128).into_val(e),
        &[],
    );
    auth::<()>(
        e,
        &f.admin,
        &f.cash,
        "set_cash_borrowing_enabled",
        (f.admin.clone(), false).into_val(e),
        &[],
    );
    let booked = holder(&f, owner);
    assert!(booked.0 > 0);
    let saved = fault(&f);
    enable_measurement(&f, &selected);
    prove_both_calls_fail(&f);
    let claims = fee_records(&f);
    let backing: u128 = inspect(
        &f,
        &f.cash,
        "get_ptoken_balance",
        (f.margin.clone(),).into_val(e),
    );
    assert!(backing >= 500_000);
    assert_eq!(preview_fee_claim(&f, provider), (500_000, 500_000));
    let snapshot = e.to_ledger_snapshot();
    let report = std::path::PathBuf::from(std::env::var("CL_RESOURCE_REPORT").unwrap());
    let snapshot_path = report
        .parent()
        .unwrap()
        .join("mixed-before-settlement.json");
    assert!(!snapshot_path.exists());
    std::fs::write(snapshot_path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    reject_insufficient(&f, &payer, id, needed - 1);
    reject_mainnet_clone(&f, &payer, id, maximum);
    assert_eq!(fee_records(&f), claims);
    assert_eq!(holder(&f, owner), booked);
    let payer_before = balance(&f, &f.under, &payer);
    let cash_before = balance(&f, &f.under, &f.cash);
    let held_before = balance(&f, &f.under, &f.margin);
    mixed_auth(e, &f.margin, &f.cash, &f.under, &payer, id, maximum);
    let args = (payer.clone(), id, maximum, 0u128, 0u128).into_val(e);
    let cursor = e.events().all().events().len();
    #[cfg(feature = "transaction-evidence")]
    crate::transaction_evidence::capture_before(e, &f.margin, MIXED, &args, Some(&payer));
    crate::cl_resource_limits::reset(e);
    let result: (u128, u128, u128, u128) =
        e.invoke_contract(&f.margin, &Symbol::new(e, MIXED), args);
    measure(
        e,
        "option1_mixed_own_proceeds_external_full_repayment",
        true,
    );
    assert_eq!(result, (proceeds, needed, 0, 0));
    payment_order(&f, &payer, cursor, proceeds, needed, maximum);
    assert_eq!(payer_before - balance(&f, &f.under, &payer), needed as i128);
    assert_eq!(balance(&f, &f.under, &f.cash) - cash_before, debt as i128);
    assert_eq!(
        held_before - balance(&f, &f.under, &f.margin),
        proceeds as i128
    );
    debt_zero(&f, id);
    assert!(
        inspect::<Option<Map<Symbol, Val>>>(&f, &f.margin, "get_position", (id,).into_val(e))
            .is_none()
    );
    assert!(inspect::<Option<Map<Symbol, Val>>>(
        &f,
        &f.margin,
        "get_pending_perps_close",
        (id,).into_val(e)
    )
    .is_none());
    assert_eq!(
        fee_records(&f),
        claims,
        "reserved fee entitlement records survive untouched"
    );
    assert_eq!(
        inspect::<Map<Symbol, Val>>(
            &f,
            &f.margin,
            "get_undistributed_margin_fees",
            (f.cash.clone(),).into_val(e)
        ),
        reserved
    );
    assert_eq!(
        inspect::<u128>(
            &f,
            &f.cash,
            "get_ptoken_balance",
            (f.margin.clone(),).into_val(e)
        ),
        backing,
        "physical fee pToken backing unchanged"
    );
    assert_eq!(preview_fee_claim(&f, provider), (500_000, 500_000));
    assert_eq!(holder(&f, owner), booked);
    conserve(
        &f,
        &owners,
        Some(&payer),
        "mixed_before_actual_fee_distribution",
    );
    // Actual permissionless distribution and beneficiary claim are separate,
    // measured roots. They are not required work omitted from settlement.
    assert_eq!(
        call::<u128>(
            e,
            &f.margin,
            "distribute_margin_fees",
            (f.cash.clone(),).into_val(e)
        ),
        500_000
    );
    measure(e, "option1_mixed_reserved_fee_distribution", true);
    let free: u128 = inspect(
        &f,
        &f.margin,
        "get_margin_balance_ptokens",
        (provider.clone(), f.under.clone()).into_val(e),
    );
    assert_eq!(
        auth::<u128>(
            e,
            provider,
            &f.margin,
            "claim_margin_fees",
            (provider.clone(), f.under.clone()).into_val(e),
            &[]
        ),
        500_000
    );
    measure(e, "option1_mixed_reserved_fee_claim", true);
    assert_eq!(
        inspect::<u128>(
            &f,
            &f.margin,
            "get_margin_balance_ptokens",
            (provider.clone(), f.under.clone()).into_val(e)
        ),
        free + 500_000
    );
    assert_eq!(
        inspect::<u128>(
            &f,
            &f.cash,
            "get_ptoken_balance",
            (f.margin.clone(),).into_val(e)
        ),
        backing
    );
    assert_eq!(
        auth::<u128>(
            e,
            provider,
            &f.margin,
            "claim_margin_fees",
            (provider.clone(), f.under.clone()).into_val(e),
            &[]
        ),
        0
    );
    measure(e, "option1_mixed_reserved_fee_repeat_zero", true);
    restore(&f, saved);
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    measure(e, "option1_mixed_late_reward_quarantine", true);
    conserve(
        &f,
        &owners,
        Some(&payer),
        "mixed_after_actual_fee_claim_and_late_rewards",
    );
    std::println!("MIXED_FUNDED_RESULT id={id} own_proceeds={proceeds} external={needed} maximum={maximum} refund={} debt={debt} fee_reserved=500000 fee_claimed=500000 payer_distinct=true prior_finish_close_called=false",maximum-needed);
}
