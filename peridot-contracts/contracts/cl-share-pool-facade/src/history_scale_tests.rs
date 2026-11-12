//! Larger histories on pinned Aquarius bytecode and the selected runtime.
//! State construction uses real compiled entrypoints and enforced invocation
//! limits. Its report suppression is explicitly fixture setup, not assessed
//! transaction evidence. Every assessed mutation below is fully captured.
use super::*;
use soroban_sdk::{BytesN, Map};
const SCALE: u128 = 1_000_000_000_000_000_000;

#[contracttype]
#[derive(Clone)]
enum FaultKey {
    User(Address),
}

fn get<T: soroban_sdk::TryFromVal<Env, Val>>(e: &Env, m: &Map<Symbol, Val>, key: &str) -> T {
    T::try_from_val(e, &m.get(Symbol::new(e, key)).unwrap())
        .ok()
        .unwrap()
}
fn interval(f: &F, id: u64) -> Map<Symbol, Val> {
    call(&f.e, &f.share, "reward_interval", (id,).into_val(&f.e))
}
fn segment(f: &F, id: u64) -> Map<Symbol, Val> {
    call(&f.e, &f.share, "reward_segment", (id,).into_val(&f.e))
}
// Query-looking APIs can settle rewards or extend TTLs. Assertion/index reads
// therefore run on a disposable ledger clone, never warming or settling the
// assessed state. Cross-environment values are transferred through XDR.
fn inspect<T: soroban_sdk::TryFromVal<Env, Val>>(f: &F, name: &str, args: Vec<Val>) -> T {
    use soroban_sdk::xdr::{FromXdr, ToXdr};
    let encoded_args: std::vec::Vec<u8> = args.to_xdr(&f.e).iter().collect();
    let address = f.share.to_string().to_string();
    let mut clone = Env::from_ledger_snapshot(f.e.to_ledger_snapshot());
    clone.set_config(soroban_sdk::testutils::EnvTestConfig {
        capture_snapshot_at_drop: false,
        ..Default::default()
    });
    crate::cl_resource_limits::reset(&clone);
    let target = Address::from_str(&clone, &address);
    let args = Vec::<Val>::from_xdr(&clone, &Bytes::from_slice(&clone, &encoded_args)).unwrap();
    let result: Val = clone.invoke_contract(&target, &Symbol::new(&clone, name), args);
    let encoded: std::vec::Vec<u8> = result.to_xdr(&clone).iter().collect();
    let result = Val::from_xdr(&f.e, &Bytes::from_slice(&f.e, &encoded)).unwrap();
    T::try_from_val(&f.e, &result).ok().unwrap()
}
fn index(f: &F, owner: &Address, revision: u64) -> u32 {
    let e = &f.e;
    let count: u32 = inspect(f, "reward_history_count", (owner.clone(),).into_val(e));
    let mut low = 0;
    let mut high = count;
    while low < high {
        let n = low + (high - low) / 2;
        let w: Map<Symbol, Val> = inspect(f, "reward_weight", (owner.clone(), n).into_val(e));
        if get::<u64>(e, &w, "revision") <= revision {
            low = n + 1;
        } else {
            high = n;
        }
    }
    low.saturating_sub(1)
}
fn settle(f: &F, interval_id: u64, owners: &[Address], label: &str) -> std::vec::Vec<u128> {
    let e = &f.e;
    let i = interval(f, interval_id);
    let first: u64 = get(e, &i, "first_segment");
    let count: u32 = get(e, &i, "segments");
    let mut credited = std::vec![0u128; owners.len()];
    for sid in first..first + count as u64 {
        assert!(call::<bool>(
            e,
            &f.share,
            "prepare_reward_segment",
            (sid, 4u32).into_val(e)
        ));
        measure(e, "history_scale_prepare", true);
        let s = segment(f, sid);
        let revision = get(e, &s, "revision");
        for (n, owner) in owners.iter().enumerate() {
            let checkpoint = index(f, owner, revision);
            let amount: u128 = call(
                e,
                &f.share,
                "settle_reward_segment",
                (owner.clone(), sid, checkpoint).into_val(e),
            );
            measure(e, label, true);
            credited[n] += amount;
            assert_eq!(
                call::<u128>(
                    e,
                    &f.share,
                    "settle_reward_segment",
                    (owner.clone(), sid, checkpoint).into_val(e)
                ),
                0
            );
            measure(e, "history_scale_repeat_idempotent", true);
        }
    }
    credited
}
fn conserve(f: &F, owners: &[Address], label: &str) {
    let e = &f.e;
    let t: RewardTotals = inspect(f, "reward_totals", ().into_val(e));
    let mut pending = 0;
    let mut fractions = 0;
    for owner in owners {
        let h: RewardHolder = inspect(f, "reward_account", (owner.clone(),).into_val(e));
        pending += h.pending;
        fractions += h.fraction;
    }
    assert_eq!(
        t.received * SCALE,
        (t.paid + t.quarantine + pending) * SCALE + fractions + t.rounding_scaled
    );
    assert_eq!(
        token::Client::new(e, &f.reward).balance(&f.share) as u128,
        t.received - t.paid
    );
    std::println!("HISTORY_SCALE_CONSERVATION label={label} received={} paid={} pending={pending} fractions={fractions} quarantine={} rounding={} exact_scaled=true", t.received, t.paid, t.quarantine, t.rounding_scaled);
}
fn toggle(f: &F, killed: bool) {
    auth::<()>(
        &f.e,
        &f.admin,
        &f.pool,
        if killed { "kill_claim" } else { "unkill_claim" },
        (f.admin.clone(),).into_val(&f.e),
        &[],
    );
}

#[test]
fn compiled_history_scale_64_owners_128_changes_8_outages() {
    let f = F::new();
    let e = &f.e;
    MEASURE.with(|m| m.set(false));
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.share.clone(), 100_000_000i128).into_val(e),
        sub_invokes: &[],
    };
    let minted: i128 = auth(
        e,
        &f.user,
        &f.share,
        "deposit_underlying",
        (f.user.clone(), 100_000_000i128, 1i128).into_val(e),
        &[transfer],
    );
    let mut owners = std::vec![f.user.clone()];
    let unit = minted / 64;
    assert!(unit > 128);
    for _ in 1..64 {
        let owner = Address::generate(e);
        auth::<()>(
            e,
            &f.user,
            &f.share,
            "transfer",
            (f.user.clone(), owner.clone(), unit).into_val(e),
            &[],
        );
        owners.push(owner);
    }
    // 128 additional actual ownership mutations create a deep immutable history
    // for two beneficiaries. No raw vault storage is fabricated.
    for _ in 0..64 {
        auth::<()>(
            e,
            &owners[0],
            &f.share,
            "transfer",
            (owners[0].clone(), owners[1].clone(), 1i128).into_val(e),
            &[],
        );
        auth::<()>(
            e,
            &owners[1],
            &f.share,
            "transfer",
            (owners[1].clone(), owners[0].clone(), 1i128).into_val(e),
            &[],
        );
    }
    auth::<()>(
        e,
        &f.admin,
        &f.share,
        "enable_verified_reward_view",
        ().into_val(e),
        &[],
    );
    funded_tests::schedule_rewards(&f);
    let mut checkpoints = 0u32;
    for owner in &owners {
        checkpoints += inspect::<u32>(&f, "reward_history_count", (owner.clone(),).into_val(e));
    }
    assert!(checkpoints >= 383);
    MEASURE.with(|m| m.set(true));
    std::println!("HISTORY_SCALE_FIXTURE owners=64 additional_ownership_mutations=128 checkpoints={checkpoints} setup_real_compiled=true setup_limits_enforced=true setup_transaction_evidence=false");
    let mut intervals = std::vec::Vec::new();
    for n in 0..8u64 {
        funded_tests::advance_rewards(e, 1100 + n * 100);
        toggle(&f, true);
        let owner = &owners[n as usize + 1];
        let shares = token::Client::new(e, &f.share).balance(owner);
        let _: Vec<u128> = auth(
            e,
            owner,
            &f.share,
            "emergency_redeem",
            (owner.clone(), shares as u128, false).into_val(e),
            &[],
        );
        measure(e, "history_scale_64_owner_exit", true);
        assert_eq!(token::Client::new(e, &f.share).balance(owner), 0);
        let b: Map<Symbol, Val> = call(e, &f.share, "reward_boundary", (n,).into_val(e));
        assert!(!get::<bool>(e, &b, "waived"));
        assert!(get::<Option<u128>>(e, &b, "reading").is_some());
        intervals.push(get::<u64>(e, &b, "interval"));
        funded_tests::advance_rewards(e, 1110 + n * 100);
        toggle(&f, false);
        assert!(call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
        measure(e, "history_scale_8_outages_claim", true);
    }
    // All 64 beneficiaries settle the earliest interval, including eight who
    // have since exited. Later intervals are settled incrementally for their
    // respective exited beneficiary; the remaining backing stays reserved.
    let first_credits = settle(&f, intervals[0], &owners, "history_scale_64_owner_settle");
    assert!(first_credits.iter().all(|n| *n > 0));
    for n in 1..8 {
        let got = settle(
            &f,
            intervals[n],
            &[owners[n + 1].clone()],
            "history_scale_later_exited_settle",
        );
        assert!(got[0] > 0);
    }
    conserve(&f, &owners, "64_owners_128_changes_8_outages");
    std::println!("HISTORY_SCALE_RESULT owners=64 exited=8 intervals=8 earliest_interval_all_owners_settled=true later_intervals_incremental=true unresolved_boundaries=0 remaining_reservation_not_unresolved=true");
}

#[test]
fn compiled_history_scale_8_unresolved_custody_outages_governed_one() {
    let f = F::new();
    let e = &f.e;
    MEASURE.with(|m| m.set(false));
    // Cash supply must cover both the margin contributions and borrowed debt
    // for eight positions. This is controlled setup, not production borrowing.
    f.deposit_cash(2_000_000_000);
    let positions: std::vec::Vec<_> = (0..8).map(|_| funded_tests::open(&f)).collect();
    let liquidator = Address::generate(e);
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), liquidator.clone(), 1_200_000_000i128).into_val(e),
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
    let booked: RewardHolder = inspect(&f, "reward_account", (f.user.clone(),).into_val(e));
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
    MEASURE.with(|m| m.set(true));
    let mut cursor = 0u64;
    let mut intervals = std::vec::Vec::new();
    let mut first_external_pending = 0u128;
    for (n, id) in positions.iter().enumerate() {
        funded_tests::advance_rewards(e, 1200 + n as u64 * 80);
        let external_pending: u128 = call(
            e,
            &f.pool,
            "get_user_reward",
            (f.share.clone(),).into_val(e),
        );
        if n == 0 {
            first_external_pending = external_pending;
        }
        let key = FaultKey::User(f.share.clone());
        let saved: Val = e.as_contract(&f.pool, || {
            let saved = e.storage().persistent().get(&key).unwrap();
            e.storage().persistent().set(&key, &123u32);
            saved
        });
        rejection(
            e,
            &f.pool,
            "get_user_reward",
            (f.share.clone(),).into_val(e),
            "history_scale_pending_fault",
            false,
        );
        assert!(!call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
        measure(e, "history_scale_claim_fault", true);
        let transfers = [MockAuthInvoke {
            contract: &f.under,
            fn_name: "transfer",
            args: (liquidator.clone(), f.cash.clone(), 110_000_000i128).into_val(e),
            sub_invokes: &[],
        }];
        let repay = MockAuthInvoke {
            contract: &f.cash,
            fn_name: "repay_full_for_margin",
            args: (*id, liquidator.clone(), 110_000_000u128).into_val(e),
            sub_invokes: &transfers,
        };
        let (paid, seized): (u128, u128) = auth(
            e,
            &liquidator,
            &f.margin,
            "liquidate_funded_v3",
            (liquidator.clone(), *id, 110_000_000u128, 1u128).into_val(e),
            &[repay],
        );
        measure(e, "history_scale_8_funded_fault_exit", true);
        assert!(paid > 0 && seized > 0);
        assert_eq!(
            call::<u128>(e, &f.cash, "get_margin_borrow_balance", (*id,).into_val(e)),
            0
        );
        let mut current = None;
        loop {
            let b: Option<Map<Symbol, Val>> =
                call(e, &f.share, "reward_boundary", (cursor,).into_val(e));
            let Some(b) = b else { break };
            assert!(!get::<bool>(e, &b, "waived"));
            assert_eq!(get::<Option<u128>>(e, &b, "reading"), None);
            let iid = get::<u64>(e, &b, "interval");
            if let Some(prior) = current {
                assert_eq!(prior, iid);
            } else {
                current = Some(iid);
            }
            cursor += 1;
            assert!(cursor <= 32);
        }
        let iid = current.unwrap();
        assert!(!intervals.contains(&iid));
        intervals.push(iid);
        e.as_contract(&f.pool, || e.storage().persistent().set(&key, &saved));
        // The fault lasts one fixed synthetic timestamp. Before/after values are
        // controlled replay evidence only, never an observed missing boundary.
        assert_eq!(
            call::<u128>(
                e,
                &f.pool,
                "get_user_reward",
                (f.share.clone(),).into_val(e)
            ),
            external_pending
        );
        assert!(call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
        measure(e, "history_scale_8_unresolved_claim", true);
        let i = interval(&f, iid);
        assert_eq!(get::<u128>(e, &i, "received"), external_pending);
        assert!(get::<u32>(e, &i, "resolved") < get::<u32>(e, &i, "segments"));
        let after: RewardHolder = inspect(&f, "reward_account", (f.user.clone(),).into_val(e));
        assert_eq!(
            (after.pending, after.fraction),
            (booked.pending, booked.fraction)
        );
    }
    assert_eq!(intervals.len(), 8);
    assert_eq!(cursor, 16);
    for custodian in [&f.receipt, &f.margin] {
        assert_eq!(
            call::<u128>(
                e,
                &f.share,
                "custody_allocation",
                (custodian.clone(), f.user.clone()).into_val(e)
            ),
            0
        );
    }
    conserve(
        &f,
        &[f.user.clone(), liquidator.clone()],
        "8_unresolved_before_governance",
    );
    // Resolve only interval zero using separately published controlled-state
    // evidence. Later cumulative readings alone cannot justify any other gap.
    let first = interval(&f, intervals[0]);
    let sid: u64 = get(e, &first, "first_segment");
    let count: u32 = get(e, &first, "segments");
    let evidence = serde_json::to_string_pretty(&serde_json::json!({"classification":"governed-controlled-state-replay-NOT-automatic-or-live-boundary-evidence", "pool_hash":"12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6", "interval":intervals[0], "timestamp":1200, "external_before_after_pending":first_external_pending, "received":get::<u128>(e,&first,"received"), "first_segment":sid, "segments":count, "allocation":"first segment receives observed pre-fault accrual; zero elapsed timestamp and no LP mutation make remaining segment accrual zero in this controlled replay", "missing_in_contract_boundaries":2, "other_intervals":"no governed evidence provided; remain quarantined"})).unwrap();
    let digest: BytesN<32> = e
        .crypto()
        .sha256(&Bytes::from_slice(e, evidence.as_bytes()))
        .to_bytes()
        .into();
    if let Ok(dir) = std::env::var("CL_GOVERNED_EVIDENCE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            format!("{dir}/history-scale-interval-{}.json", intervals[0]),
            &evidence,
        )
        .unwrap();
    }
    std::println!("HISTORY_SCALE_GOVERNED_EVIDENCE digest={digest:?} json={evidence}");
    for s in sid..sid + count as u64 {
        auth::<()>(
            e,
            &f.admin,
            &f.share,
            "propose_reward_segment",
            (
                s,
                if s == sid {
                    first_external_pending
                } else {
                    0u128
                },
                digest.clone(),
            )
                .into_val(e),
            &[],
        );
    }
    funded_tests::advance_rewards(e, 88_161);
    for s in sid..sid + count as u64 {
        auth::<()>(
            e,
            &f.admin,
            &f.share,
            "attribute_reward_segment",
            (
                s,
                if s == sid {
                    first_external_pending
                } else {
                    0u128
                },
                digest.clone(),
            )
                .into_val(e),
            &[],
        );
        measure(e, "history_scale_governed_attribute", true);
    }
    let credits = settle(
        &f,
        intervals[0],
        &[f.user.clone(), liquidator.clone()],
        "history_scale_governed_exited_settle",
    );
    assert_eq!(credits, std::vec![first_external_pending * SCALE, 0]);
    let s = segment(&f, sid);
    assert!(get::<bool>(e, &s, "governed"));
    assert!(!get::<bool>(e, &s, "observed_verified"));
    let mut remaining = 0;
    for iid in &intervals[1..] {
        let i = interval(&f, *iid);
        let s: u64 = get(e, &i, "first_segment");
        remaining += get::<u128>(e, &i, "received");
        assert!(get::<u32>(e, &i, "resolved") < get::<u32>(e, &i, "segments"));
        assert_eq!(
            get::<u128>(e, &i, "remaining_scaled"),
            get::<u128>(e, &i, "received") * SCALE
        );
        assert!(call::<bool>(
            e,
            &f.share,
            "prepare_reward_segment",
            (s, 4u32).into_val(e)
        ));
        let before = e.to_ledger_snapshot().ledger_entries;
        rejection(
            e,
            &f.share,
            "settle_reward_segment",
            (f.user.clone(), s, 0u32).into_val(e),
            "history_scale_missing_evidence_rejected",
            false,
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
    }
    let totals: RewardTotals = inspect(&f, "reward_totals", ().into_val(e));
    assert_eq!(totals.quarantine, remaining);
    conserve(
        &f,
        &[f.user.clone(), liquidator.clone()],
        "governed_one_seven_quarantined",
    );
    std::println!("HISTORY_SCALE_UNRESOLVED_RESULT intervals=8 missing_boundaries=16 governed_intervals=1 still_unresolved=7 quarantine={remaining} forced_waiver=false exited_custody_preserved=true");
}
