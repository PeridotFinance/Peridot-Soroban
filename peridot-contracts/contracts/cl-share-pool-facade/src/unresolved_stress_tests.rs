//! Three unresolved intervals using pinned Aquarius bytecode with controlled
//! corruption of its User record. This is dependency-fault injection, not live state.
use super::*;
use soroban_sdk::Map;
const SCALE: u128 = 1_000_000_000_000_000_000;
#[contracttype]
#[derive(Clone)]
enum PoolFaultKey {
    User(Address),
}
fn get<T: soroban_sdk::TryFromVal<Env, Val>>(e: &Env, m: &Map<Symbol, Val>, k: &str) -> T {
    T::try_from_val(e, &m.get(Symbol::new(e, k)).unwrap())
        .ok()
        .unwrap()
}
fn interval(f: &F, id: u64) -> Map<Symbol, Val> {
    call(&f.e, &f.share, "reward_interval", (id,).into_val(&f.e))
}
#[test]
fn compiled_multiple_unresolved_intervals_after_funded_exits() {
    let f = F::new();
    let e = &f.e;
    let positions = [
        funded_tests::open(&f),
        funded_tests::open(&f),
        funded_tests::open(&f),
    ];
    let liquidator = Address::generate(e);
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), liquidator.clone(), 600_000_000i128).into_val(e),
        &[],
    );
    funded_tests::schedule_rewards(&f);
    funded_tests::advance_rewards(e, 1100);
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    let booked: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    assert!(booked.pending > 0);
    auth::<()>(
        e,
        &f.admin,
        &f.share,
        "enable_verified_reward_view",
        ().into_val(e),
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
    let mut boundary_cursor = 0u64;
    let mut intervals = std::vec::Vec::new();
    for (n, id) in positions.iter().enumerate() {
        funded_tests::advance_rewards(e, 1200 + n as u64 * 100);
        let key = PoolFaultKey::User(f.share.clone());
        let saved: Val = e.as_contract(&f.pool, || {
            let v = e
                .storage()
                .persistent()
                .get(&key)
                .expect("actual pool user record");
            e.storage().persistent().set(&key, &123u32);
            v
        });
        rejection(
            e,
            &f.pool,
            "get_user_reward",
            (f.share.clone(),).into_val(e),
            "unresolved_stress_pending_view_fault",
            false,
        );
        assert!(!call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
        let transfer = MockAuthInvoke {
            contract: &f.under,
            fn_name: "transfer",
            args: (liquidator.clone(), f.cash.clone(), 110_000_000i128).into_val(e),
            sub_invokes: &[],
        };
        let ts = [transfer];
        let repay = MockAuthInvoke {
            contract: &f.cash,
            fn_name: "repay_full_for_margin",
            args: (*id, liquidator.clone(), 110_000_000u128).into_val(e),
            sub_invokes: &ts,
        };
        let paid: (u128, u128) = auth(
            e,
            &liquidator,
            &f.margin,
            "liquidate_funded_v3",
            (liquidator.clone(), *id, 110_000_000u128, 1u128).into_val(e),
            &[repay],
        );
        measure(e, "historical_unresolved_intervals", true);
        assert!(paid.0 > 0 && paid.1 > 0);
        assert_eq!(
            call::<u128>(e, &f.cash, "get_margin_borrow_balance", (*id,).into_val(e)),
            0
        );
        let after: RewardHolder =
            call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
        assert_eq!(after.pending, booked.pending);
        assert_eq!(after.fraction, booked.fraction);
        let first_boundary = boundary_cursor;
        let mut current = None;
        loop {
            let b: Option<Map<Symbol, Val>> = call(
                e,
                &f.share,
                "reward_boundary",
                (boundary_cursor,).into_val(e),
            );
            let Some(b) = b else { break };
            assert!(!get::<bool>(e, &b, "waived"));
            assert_eq!(get::<Option<u128>>(e, &b, "reading"), None);
            let i = get::<u64>(e, &b, "interval");
            if let Some(previous) = current {
                assert_eq!(previous, i)
            } else {
                current = Some(i)
            };
            boundary_cursor += 1;
            assert!(boundary_cursor < 32, "bounded test fixture");
        }
        assert!(boundary_cursor > first_boundary);
        let current = current.unwrap();
        assert!(!intervals.contains(&current));
        intervals.push(current);
        e.as_contract(&f.pool, || e.storage().persistent().set(&key, &saved));
        let later: u128 = call(
            e,
            &f.pool,
            "get_user_reward",
            (f.share.clone(),).into_val(e),
        );
        assert!(later > 0);
        assert!(call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
        measure(e, "historical_unresolved_late_claim", true);
        // Later cumulative values must never fill any earlier missing boundary.
        for old_id in &intervals {
            let i = interval(&f, *old_id);
            assert!(get::<bool>(e, &i, "sealed"));
            assert!(get::<u32>(e, &i, "resolved") < get::<u32>(e, &i, "segments"));
            assert_eq!(get::<u32>(e, &i, "waivers"), 0);
            assert_eq!(get::<u128>(e, &i, "original_scaled"), 0);
            assert_eq!(get::<u128>(e, &i, "redistribution_scaled"), 0);
            assert_eq!(
                get::<u128>(e, &i, "remaining_scaled"),
                get::<u128>(e, &i, "received") * SCALE
            );
        }
    }
    assert_eq!(intervals.len(), 3);
    assert_eq!(
        call::<u128>(
            e,
            &f.share,
            "custody_allocation",
            (f.receipt.clone(), f.user.clone()).into_val(e)
        ),
        0
    );
    assert_eq!(
        call::<u128>(
            e,
            &f.share,
            "custody_allocation",
            (f.margin.clone(), f.user.clone()).into_val(e)
        ),
        0
    );
    let mut quarantined = 0u128;
    for id in &intervals {
        let i = interval(&f, *id);
        quarantined += get::<u128>(e, &i, "received");
        let sid = get::<u64>(e, &i, "first_segment");
        assert!(call::<bool>(
            e,
            &f.share,
            "prepare_reward_segment",
            (sid, 4u32).into_val(e)
        ));
        let before = e.to_ledger_snapshot().ledger_entries;
        rejection(
            e,
            &f.share,
            "settle_reward_segment",
            (f.user.clone(), sid, 0u32).into_val(e),
            "historical_unresolved_settlement_rejected",
            false,
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
    }
    let history: u32 = call(
        e,
        &f.share,
        "reward_history_count",
        (f.user.clone(),).into_val(e),
    );
    assert!(history > 1);
    let mut historically_owned = false;
    for n in 0..history {
        let w: Map<Symbol, Val> = call(
            e,
            &f.share,
            "reward_weight",
            (f.user.clone(), n).into_val(e),
        );
        historically_owned |= get::<u128>(e, &w, "weight") > 0;
    }
    assert!(historically_owned);
    let original: RewardHolder = call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    let receiver: RewardHolder = call(
        e,
        &f.share,
        "reward_account",
        (liquidator.clone(),).into_val(e),
    );
    assert_eq!(original.pending, booked.pending);
    assert_eq!(original.fraction, booked.fraction);
    let t: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    assert_eq!(t.quarantine, quarantined);
    assert!(quarantined > 0);
    assert_eq!(
        t.received * SCALE,
        (t.paid + t.quarantine + original.pending + receiver.pending) * SCALE
            + original.fraction
            + receiver.fraction
            + t.rounding_scaled
    );
    assert_eq!(
        token::Client::new(e, &f.reward).balance(&f.share) as u128,
        t.received - t.paid
    );
    std::println!("UNRESOLVED_HISTORY_STRESS intervals=3 boundaries={boundary_cursor} exited_borrower_booked={} quarantine={quarantined} missing_boundary_not_reconstructed=true waiver=false exact_scaled_conservation=true dependency_fault_injection=true",original.pending);
}
