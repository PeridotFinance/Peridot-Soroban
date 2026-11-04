//! Real Aquarius receipts with explicitly governed synthetic shortfall assumptions.
//! This is an accounting fault scenario, NOT automatically verified venue shortfall.
use super::*;
use soroban_sdk::{BytesN, Map};
const SCALE: u128 = 1_000_000_000_000_000_000;
fn get<T: soroban_sdk::TryFromVal<Env, Val>>(e: &Env, m: &Map<Symbol, Val>, k: &str) -> T {
    T::try_from_val(e, &m.get(Symbol::new(e, k)).unwrap())
        .ok()
        .unwrap()
}
fn map(f: &F, method: &str, id: u64) -> Map<Symbol, Val> {
    call(&f.e, &f.share, method, (id,).into_val(&f.e))
}
fn time(e: &Env, t: u64) {
    e.ledger().with_mut(|l| {
        l.timestamp = t;
        l.sequence_number += 1;
    });
}
fn version(f: &F, a: &Address, rev: u64) -> (u32, u128) {
    let e = &f.e;
    let n: u32 = call(
        e,
        &f.share,
        "reward_history_count",
        (a.clone(),).into_val(e),
    );
    let mut selected = (0, 0);
    for k in 0..n {
        let w: Map<Symbol, Val> = call(e, &f.share, "reward_weight", (a.clone(), k).into_val(e));
        if get::<u64>(e, &w, "revision") <= rev {
            selected = (k, get(e, &w, "weight"));
        }
    }
    selected
}
#[test]
fn compiled_governed_shortfall_proportional_exited_custody_owner() {
    let f = F::new();
    let e = &f.e;
    let id = funded_tests::open(&f);
    let newcomer = Address::generate(e);
    funded_tests::schedule_rewards(&f);
    time(e, 1100);
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "kill_claim",
        (f.admin.clone(),).into_val(e),
        &[],
    );
    let at_boundary: u128 = call(
        e,
        &f.pool,
        "get_user_reward",
        (f.share.clone(),).into_val(e),
    );
    assert!(at_boundary > 0);
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), newcomer.clone(), 200_000_000i128).into_val(e),
        &[],
    );
    auth::<()>(
        e,
        &f.admin,
        &f.oracle,
        "set_test_price",
        (
            f.admin.clone(),
            Asset::Stellar(f.share.clone()),
            45_000_000_000_000i128,
        )
            .into_val(e),
        &[],
    );
    let t = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (newcomer.clone(), f.cash.clone(), 110_000_000i128).into_val(e),
        sub_invokes: &[],
    };
    let ts = [t];
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "repay_full_for_margin",
        args: (id, newcomer.clone(), 110_000_000u128).into_val(e),
        sub_invokes: &ts,
    };
    let _: (u128, u128) = auth(
        e,
        &newcomer,
        &f.margin,
        "liquidate_funded_v3",
        (newcomer.clone(), id, 110_000_000u128, 1u128).into_val(e),
        &[repay],
    );
    let boundary = map(&f, "reward_boundary", 0);
    assert!(!get::<bool>(e, &boundary, "waived"));
    assert_eq!(
        get::<Option<u128>>(e, &boundary, "reading"),
        Some(at_boundary)
    );
    let interval: u64 = get(e, &boundary, "interval");
    time(e, 1150);
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "unkill_claim",
        (f.admin.clone(),).into_val(e),
        &[],
    );
    let pending: u128 = call(
        e,
        &f.pool,
        "get_user_reward",
        (f.share.clone(),).into_val(e),
    );
    let before = token::Client::new(e, &f.reward).balance(&f.share);
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    let actual = token::Client::new(e, &f.reward).balance(&f.share) - before;
    assert_eq!(actual as u128, pending);
    let i = map(&f, "reward_interval", interval);
    let first: u64 = get(e, &i, "first_segment");
    let count: u32 = get(e, &i, "segments");
    let received: u128 = get(e, &i, "received");
    assert_eq!(received, pending);
    let mut declarations = std::vec::Vec::new();
    let mut expected = 0u128;
    for sid in first..first + count as u64 {
        let s = map(&f, "reward_segment", sid);
        assert!(!get::<bool>(e, &s, "observed_verified"));
        let start: Option<u128> = get(e, &s, "reading_start");
        let end: Option<u128> = get(e, &s, "reading_end");
        let observed = end.unwrap() - start.unwrap();
        let declared = observed * 2;
        expected += declared;
        declarations.push((sid, observed, declared));
    }
    assert_eq!(expected, received * 2);
    let evidence=serde_json::to_string(&serde_json::json!({"classification":"governed-synthetic-expected-table-shortfall-arithmetic-fault-test","automatic_venue_shortfall_proof":false,"pool_hash":"12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6","actual_boundary_reading":at_boundary,"actual_final_pending":pending,"actual_claim":actual,"synthetic_expected_total":expected,"segments_sid_actual_delta_synthetic_expected":declarations})).unwrap();
    let digest = BytesN::<32>::try_from(
        e.crypto()
            .sha256(&Bytes::from_slice(e, evidence.as_bytes()))
            .to_bytes(),
    )
    .unwrap();
    std::println!("GOVERNED_SHORTFALL_PUBLISHED_EVIDENCE digest={digest:?} json={evidence}");
    if let Ok(dir) = std::env::var("CL_GOVERNED_EVIDENCE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            format!("{dir}/synthetic-shortfall-{interval}.json"),
            &evidence,
        )
        .unwrap();
    }
    for (sid, _, declared) in &declarations {
        auth::<()>(
            e,
            &f.admin,
            &f.share,
            "propose_reward_segment",
            (*sid, *declared, digest.clone()).into_val(e),
            &[],
        );
    }
    time(e, 87551);
    for (sid, _, declared) in &declarations {
        auth::<()>(
            e,
            &f.admin,
            &f.share,
            "attribute_reward_segment",
            (*sid, *declared, digest.clone()).into_val(e),
            &[],
        );
    }
    let mut total_original = 0u128;
    let mut credits = [0u128; 2];
    let owners = [f.user.clone(), newcomer.clone()];
    for (sid, _, declared) in declarations {
        assert!(call::<bool>(
            e,
            &f.share,
            "prepare_reward_segment",
            (sid, 4u32).into_val(e)
        ));
        let s = map(&f, "reward_segment", sid);
        assert!(get::<bool>(e, &s, "governed"));
        assert!(!get::<bool>(e, &s, "observed_verified"));
        let rev: u64 = get(e, &s, "revision");
        let supply: u128 = get(e, &s, "supply");
        let segment_allocation = (received * SCALE) * declared / expected;
        for (n, a) in owners.iter().enumerate() {
            let (k, w) = version(&f, a, rev);
            let calculated = if supply == 0 {
                0
            } else {
                segment_allocation * w / supply
            };
            let credited: u128 = call(
                e,
                &f.share,
                "settle_reward_segment",
                (a.clone(), sid, k).into_val(e),
            );
            measure(e, "governed_proportional_shortfall", true);
            assert_eq!(credited, calculated);
            credits[n] += credited;
            total_original += credited;
            assert_eq!(
                call::<u128>(
                    e,
                    &f.share,
                    "settle_reward_segment",
                    (a.clone(), sid, k).into_val(e)
                ),
                0
            );
            if sid == first && n == 1 {
                assert_eq!(w, 0);
                assert_eq!(
                    credited, 0,
                    "newcomer cannot receive original interval entitlement"
                );
            }
        }
    }
    assert!(credits[0] > 0 && credits[1] > 0);
    let i = map(&f, "reward_interval", interval);
    assert_eq!(get::<u128>(e, &i, "original_scaled"), total_original);
    assert_eq!(get::<u128>(e, &i, "redistribution_scaled"), 0);
    assert!(total_original <= received * SCALE);
    let t: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    let mut pending_h = 0u128;
    let mut fraction = 0u128;
    for a in &owners {
        let h: RewardHolder = call(e, &f.share, "reward_account", (a.clone(),).into_val(e));
        pending_h += h.pending;
        fraction += h.fraction;
    }
    assert_eq!(
        t.received * SCALE,
        (t.paid + t.quarantine + pending_h) * SCALE + fraction + t.rounding_scaled
    );
    assert_eq!(
        token::Client::new(e, &f.reward).balance(&f.share) as u128,
        t.received - t.paid
    );
    std::println!("GOVERNED_SHORTFALL_CONSERVATION actual_received={received} declared_synthetic_expected={expected} old_owner_scaled={} newcomer_later_scaled={} remaining_scaled={} automatic_venue_proof=false",credits[0],credits[1],get::<u128>(e,&i,"remaining_scaled"));
}
