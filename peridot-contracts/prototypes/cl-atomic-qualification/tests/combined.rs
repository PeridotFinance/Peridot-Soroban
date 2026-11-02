//! Targeted atomic qualification on actual compiled contracts / pinned Aquarius.
//! Setup uses bounded captured-network acceptance limits, never unlimited meters.
//! Pool User corruption is controlled dependency-fault injection, not live state.
use super::*;
use soroban_sdk::Map;

const SCALE: u128 = 1_000_000_000_000_000_000;
const MARGIN: u128 = 25_000_000;
const MAX_REPAY: u128 = 27_500_000;
#[contracttype]
#[derive(Clone)]
enum FaultKey {
    User(Address),
}

// A fresh transaction host must not inherit SDK objects allocated by hundreds
// of prior fixture calls. This replaces ONLY the test Env, never ledger state,
// limits, contract bytecode or production stages. External auth is freshly
// scoped by the next root's mock_auths; no contract authorization is bypassed.
fn rebase(old: F, owners: std::vec::Vec<Address>) -> (F, std::vec::Vec<Address>) {
    use sha2::{Digest, Sha256};
    // Callers have already recorded any assessed root before entering here.
    // This is a finite, existing-profile reset at a completed-root boundary.
    crate::cl_resource_limits::reset(&old.e);
    let sdk_snapshot = old.e.to_snapshot();
    let snapshot = sdk_snapshot.ledger.clone();
    let before_hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&snapshot).unwrap())
    );
    let names: std::vec::Vec<_> = owners.iter().map(|a| a.to_string().to_string()).collect();
    let roles: std::vec::Vec<_> = [
        "v3",
        "facade",
        "vault",
        "aquarius",
        "cash_receipt",
        "share_receipt",
        "adapter",
        "lending_controller",
        "oracle",
        "plane",
        "underlying",
        "paired",
        "reward",
    ]
    .iter()
    .map(|role| {
        (
            *role,
            crate::resource_profile::contract_address(role).expect("fixture role registered"),
        )
    })
    .collect();
    let binding = old.binding.to_array();
    // Preserve the test generator as well: restarting it would replay consumed
    // mock-authorization nonces retained in the exact ledger snapshot. The SDK
    // loads only ledger + generators here, never previous native auth handlers.
    let mut e = Env::from_snapshot(sdk_snapshot);
    e.set_config(soroban_sdk::testutils::EnvTestConfig {
        capture_snapshot_at_drop: false,
        ..Default::default()
    });
    crate::cl_resource_limits::configure(&e);
    let address = |a: &Address| Address::from_str(&e, &a.to_string().to_string());
    let f = F {
        admin: address(&old.admin),
        user: address(&old.user),
        under: address(&old.under),
        paired: address(&old.paired),
        reward: address(&old.reward),
        pool: address(&old.pool),
        share: address(&old.share),
        facade: address(&old.facade),
        margin: address(&old.margin),
        cash: address(&old.cash),
        receipt: address(&old.receipt),
        binding: soroban_sdk::BytesN::from_array(&e, &binding),
        oracle: address(&old.oracle),
        controller: address(&old.controller),
        e,
    };
    let owners = names.iter().map(|s| Address::from_str(&f.e, s)).collect();
    let role_addresses: std::vec::Vec<_> = roles
        .iter()
        .map(|(name, a)| (*name, Address::from_str(&f.e, a)))
        .collect();
    let references: std::vec::Vec<_> = role_addresses.iter().map(|(name, a)| (*name, a)).collect();
    crate::resource_profile::register_contracts(&f.e, &references);
    let after = f.e.to_ledger_snapshot();
    assert_eq!(
        snapshot, after,
        "rebase must preserve exact ledger data, TTLs and ledger configuration"
    );
    let after_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&after).unwrap()));
    assert_eq!(before_hash, after_hash);
    std::println!("ATOMIC_QUALIFICATION_REBASE before_snapshot_sha256={before_hash} after_snapshot_sha256={after_hash} entries={} ledger={} timestamp={} ledger_state_unchanged=true sdk_generators_preserved=true native_auth_registry_not_carried=true contract_optimization=false",snapshot.ledger_entries.len(),snapshot.sequence_number,snapshot.timestamp);
    crate::cl_resource_limits::reset(&f.e);
    (f, owners)
}

// Ledger snapshots deliberately exclude the SDK's synthetic address generator.
// Avoid reusing an existing identity when allocating a post-rebase liquidator.
fn fresh_liquidator(f: &F, owners: &[Address]) -> Address {
    let e = &f.e;
    let mut known: std::vec::Vec<std::string::String> = owners
        .iter()
        .chain([
            &f.admin,
            &f.user,
            &f.under,
            &f.paired,
            &f.reward,
            &f.pool,
            &f.share,
            &f.facade,
            &f.margin,
            &f.cash,
            &f.receipt,
            &f.oracle,
            &f.controller,
        ])
        .map(|a| a.to_string().to_string())
        .collect();
    for role in [
        "v3",
        "facade",
        "vault",
        "aquarius",
        "cash_receipt",
        "share_receipt",
        "adapter",
        "lending_controller",
        "oracle",
        "plane",
        "underlying",
        "paired",
        "reward",
    ] {
        known.push(
            crate::resource_profile::contract_address(role).expect("fixture role registered"),
        );
    }
    let snapshot = e.to_ledger_snapshot();
    for attempts in 1..=1024 {
        let candidate = Address::generate(e);
        let key = candidate.to_string().to_string();
        let address: soroban_sdk::xdr::ScAddress = (&candidate).into();
        let existing_contract = snapshot.ledger_entries.iter().any(|(key, _)| {
            matches!(&**key, soroban_sdk::xdr::LedgerKey::ContractData(data) if data.contract == address)
        });
        if !known.contains(&key) && !existing_contract {
            assert!(!owners.contains(&candidate) && candidate != f.admin);
            std::println!("ATOMIC_QUALIFICATION_FRESH_LIQUIDATOR attempts={attempts} identity={key} distinct_from_owners_admin_contracts_roles=true native_auth_collision=false");
            return candidate;
        }
    }
    panic!("bounded synthetic liquidator address allocation exhausted");
}

fn field<T: soroban_sdk::TryFromVal<Env, Val>>(e: &Env, m: &Map<Symbol, Val>, k: &str) -> T {
    T::try_from_val(e, &m.get(Symbol::new(e, k)).unwrap())
        .ok()
        .unwrap()
}

// Even query-looking methods can settle rewards or extend TTLs. Never perform
// assertion/index/NAV queries on the state that a subsequent assessed call sees.
fn inspect<T: soroban_sdk::TryFromVal<Env, Val>>(
    f: &F,
    address: &Address,
    name: &str,
    args: Vec<Val>,
) -> T {
    use soroban_sdk::xdr::{FromXdr, ToXdr};
    let encoded: std::vec::Vec<u8> = args.to_xdr(&f.e).iter().collect();
    let target = address.to_string().to_string();
    let mut clone = Env::from_ledger_snapshot(f.e.to_ledger_snapshot());
    clone.set_config(soroban_sdk::testutils::EnvTestConfig {
        capture_snapshot_at_drop: false,
        ..Default::default()
    });
    crate::cl_resource_limits::reset(&clone);
    let target = Address::from_str(&clone, &target);
    if address == &f.cash && name == "get_margin_borrow_balance" {
        clone.invoke_contract::<()>(
            &target,
            &Symbol::new(&clone, "update_interest"),
            ().into_val(&clone),
        );
        crate::cl_resource_limits::reset(&clone);
    }
    let args = Vec::<Val>::from_xdr(&clone, &Bytes::from_slice(&clone, &encoded)).unwrap();
    let value: Val = clone.invoke_contract(&target, &Symbol::new(&clone, name), args);
    let encoded: std::vec::Vec<u8> = value.to_xdr(&clone).iter().collect();
    let value = Val::from_xdr(&f.e, &Bytes::from_slice(&f.e, &encoded)).unwrap();
    T::try_from_val(&f.e, &value).ok().unwrap()
}
fn balance(f: &F, token: &Address, owner: &Address) -> i128 {
    inspect(f, token, "balance", (owner.clone(),).into_val(&f.e))
}
fn holder(f: &F, owner: &Address) -> (u128, u128) {
    let h: RewardHolder = inspect(
        f,
        &f.share,
        "reward_account",
        (owner.clone(),).into_val(&f.e),
    );
    (h.pending, h.fraction)
}
fn totals(f: &F) -> RewardTotals {
    inspect(f, &f.share, "reward_totals", ().into_val(&f.e))
}
fn advance(f: &F, timestamp: u64) {
    funded_tests::advance_rewards(&f.e, timestamp);
}
fn enable_measurement(f: &F, selected: &str) {
    std::env::set_var("CL_RESOURCE_PROFILE", selected);
    crate::cl_resource_limits::configure(&f.e);
    MEASURE.with(|v| v.set(true));
}
fn begin_for(f: &F, owner: &Address) -> u64 {
    let e = &f.e;
    let transfer = MockAuthInvoke {
        contract: &f.cash,
        fn_name: "transfer",
        args: (owner.clone(), f.margin.clone(), MARGIN as i128).into_val(e),
        sub_invokes: &[],
    };
    auth::<()>(
        e,
        owner,
        &f.margin,
        "transfer_spot_to_margin",
        (owner.clone(), f.under.clone(), MARGIN).into_val(e),
        &[transfer],
    );
    auth(
        e,
        owner,
        &f.margin,
        "begin_open_position_v3",
        (
            owner.clone(),
            f.under.clone(),
            f.share.clone(),
            MARGIN,
            2u128,
            Side::Long,
            soroban_sdk::vec![e, f.under.clone(), f.share.clone()],
            f.binding.clone(),
            f.facade.clone(),
            48_510_000u128,
        )
            .into_val(e),
        &[],
    )
}
fn seed_history(f: &F) -> std::vec::Vec<Address> {
    let e = &f.e;
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
    for _ in 1..64 {
        let owner = Address::generate(e);
        auth::<()>(
            e,
            &f.user,
            &f.share,
            "transfer",
            (f.user.clone(), owner.clone(), minted / 64).into_val(e),
            &[],
        );
        owners.push(owner);
    }
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
    owners
}
// Eight DIFFERENT owners, not eight positions belonging to one owner. Position
// size keeps all deposits below the fixture's existing 1B deployment bound.
fn fixture(
    active: bool,
    dynamic_interest: bool,
) -> (
    F,
    std::vec::Vec<Address>,
    std::vec::Vec<u64>,
    std::string::String,
) {
    let selected = std::env::var("CL_RESOURCE_PROFILE").unwrap();
    let setup =
        std::env::var("CL_ATOMIC_SETUP_PROFILE").expect("bounded captured setup profile required");
    let p: serde_json::Value = serde_json::from_slice(&std::fs::read(&setup).unwrap()).unwrap();
    assert_eq!(p["name"], "captured-network-diagnostic");
    assert_eq!(p["mode"], "acceptance");
    std::env::set_var("CL_RESOURCE_PROFILE", setup);
    let f = F::new();
    MEASURE.with(|v| v.set(false));
    let e = &f.e;
    // TEST ONLY: exercise the configured reserve path during compiled opens.
    // This synthetic market permission is not a deployment configuration.
    auth::<()>(
        e,
        &f.admin,
        &f.cash,
        "set_min_cash_reserve_bps",
        (f.admin.clone(), 1_000u32).into_val(e),
        &[],
    );
    auth::<()>(
        e,
        &f.admin,
        &f.cash,
        "set_cash_borrowing_enabled",
        (f.admin.clone(), true).into_val(e),
        &[],
    );
    assert_eq!(
        call::<Option<u32>>(e, &f.cash, "get_min_cash_reserve_bps", ().into_val(e)),
        Some(1_000)
    );
    assert!(call::<bool>(
        e,
        &f.cash,
        "get_cash_borrowing_enabled",
        ().into_val(e)
    ));
    std::println!("ATOMIC_QUALIFICATION_TEST_ONLY_CASH_POLICY reserve_bps=1000 borrowing_enabled=true production_configuration_unchanged=true");
    let owners = seed_history(&f);
    let mut ids = std::vec::Vec::new();
    for owner in &owners[1..9] {
        auth::<()>(
            e,
            &f.user,
            &f.under,
            "transfer",
            (f.user.clone(), owner.clone(), 500_000_000i128).into_val(e),
            &[],
        );
        let transfer = MockAuthInvoke {
            contract: &f.under,
            fn_name: "transfer",
            args: (owner.clone(), f.cash.clone(), 100_000_000i128).into_val(e),
            sub_invokes: &[],
        };
        auth::<()>(
            e,
            owner,
            &f.cash,
            "deposit",
            (owner.clone(), 100_000_000u128).into_val(e),
            &[transfer],
        );
        let id = begin_for(&f, owner);
        auth::<()>(
            e,
            owner,
            &f.margin,
            "swap_open_position_v3",
            (owner.clone(), id).into_val(e),
            &[],
        );
        if active {
            auth::<()>(
                e,
                owner,
                &f.margin,
                "activate_open_position_v3",
                (owner.clone(), id).into_val(e),
                &[],
            );
        }
        ids.push(id);
    }
    assert_eq!(ids.len(), 8);
    if dynamic_interest {
        let model = wasm(e, "jump_rate_model");
        auth::<()>(
            e,
            &f.admin,
            &model,
            "initialize",
            (
                50_000u128,
                200_000u128,
                1_000_000u128,
                800_000u128,
                f.admin.clone(),
            )
                .into_val(e),
            &[],
        );
        auth::<()>(
            e,
            &f.admin,
            &f.cash,
            "set_interest_model",
            (model,).into_val(e),
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
    auth::<()>(
        e,
        &f.user,
        &f.reward,
        "transfer",
        (f.user.clone(), f.pool.clone(), 100_000_000i128).into_val(e),
        &[],
    );
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "set_rewards_config",
        (f.admin.clone(), 10_000u64, 10_000u128).into_val(e),
        &[],
    );
    advance(&f, 1100);
    assert!(call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    let checkpoints: u32 = owners
        .iter()
        .map(|o| {
            inspect::<u32>(
                &f,
                &f.share,
                "reward_history_count",
                (o.clone(),).into_val(e),
            )
        })
        .sum();
    assert!(checkpoints >= 383);
    for owner in &owners[1..9] {
        assert!(holder(&f, owner).0 > 0);
    }
    std::println!("ATOMIC_QUALIFICATION_SETUP owners=64 independent_borrowers=8 additional_real_transfers=128 history_checkpoints={checkpoints} margin_each={MARGIN} prior_position_state={} dynamic_cash_interest={dynamic_interest} captured_limits_enforced=true setup_measurements_suppressed=true production_policy_unchanged=true",if active{"active"}else{"swapped_pending"});
    let (f, owners) = rebase(f, owners);
    (f, owners, ids, selected)
}
fn fault(f: &F) -> Val {
    let key = FaultKey::User(f.share.clone());
    f.e.as_contract(&f.pool, || {
        let old =
            f.e.storage()
                .persistent()
                .get(&key)
                .expect("actual pool User record");
        f.e.storage().persistent().set(&key, &123u32);
        old
    })
}
fn restore(f: &F, saved: Val) {
    f.e.as_contract(&f.pool, || {
        f.e.storage()
            .persistent()
            .set(&FaultKey::User(f.share.clone()), &saved)
    });
}
fn prove_both_calls_fail(f: &F) {
    let e = &f.e;
    e.mock_auths(&[]);
    #[cfg(feature = "transaction-evidence")]
    if MEASURE.with(|m| m.get()) {
        crate::transaction_evidence::capture_rejection_before(
            e,
            &f.pool,
            "get_user_reward",
            &(f.share.clone(),).into_val(e),
            None,
            true,
            "atomic_qualification_pending_view_fault",
        );
    }
    crate::cl_resource_limits::reset(e);
    assert!(e
        .try_invoke_contract::<u128, soroban_sdk::Error>(
            &f.pool,
            &Symbol::new(e, "get_user_reward"),
            (f.share.clone(),).into_val(e)
        )
        .is_err());
    measure(e, "atomic_qualification_pending_view_fault", true);
    assert!(!call::<bool>(
        e,
        &f.share,
        "checkpoint_rewards",
        ().into_val(e)
    ));
    measure(e, "atomic_qualification_claim_fault", true);
}
fn funded<T: soroban_sdk::TryFromVal<Env, Val>>(
    f: &F,
    owner: &Address,
    id: u64,
    name: &str,
    extra: bool,
    max: u128,
) -> T {
    let e = &f.e;
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (owner.clone(), f.cash.clone(), max as i128).into_val(e),
        sub_invokes: &[],
    };
    let transfers = [transfer];
    let repay = MockAuthInvoke {
        contract: &f.cash,
        fn_name: if name == "repay_margin_position_v3" {
            "repay_for_margin"
        } else {
            "repay_full_for_margin"
        },
        args: (id, owner.clone(), max).into_val(e),
        sub_invokes: &transfers,
    };
    let args = if extra {
        (owner.clone(), id, max, 1u128).into_val(e)
    } else {
        (owner.clone(), id, max).into_val(e)
    };
    auth(e, owner, &f.margin, name, args, &[repay])
}
fn check_boundaries(f: &F, cursor: &mut u64, intervals: &mut std::vec::Vec<u64>) {
    let e = &f.e;
    let start = *cursor;
    let mut current = None;
    loop {
        let b: Option<Map<Symbol, Val>> =
            inspect(f, &f.share, "reward_boundary", (*cursor,).into_val(e));
        let Some(b) = b else { break };
        assert!(!field::<bool>(e, &b, "waived"));
        assert_eq!(field::<Option<u128>>(e, &b, "reading"), None);
        let id = field::<u64>(e, &b, "interval");
        if let Some(prior) = current {
            assert_eq!(prior, id);
        } else {
            current = Some(id);
        }
        *cursor += 1;
        assert!(*cursor <= 96, "bounded fixture history");
    }
    assert!(*cursor > start);
    let id = current.unwrap();
    assert!(!intervals.contains(&id));
    intervals.push(id);
}
fn verify_quarantine(f: &F, intervals: &[u64]) {
    let e = &f.e;
    let mut reserved = 0;
    for id in intervals {
        let i: Map<Symbol, Val> = inspect(f, &f.share, "reward_interval", (*id,).into_val(e));
        assert!(field::<bool>(e, &i, "sealed"));
        assert!(field::<u32>(e, &i, "resolved") < field::<u32>(e, &i, "segments"));
        assert_eq!(field::<u32>(e, &i, "waivers"), 0);
        assert_eq!(field::<u128>(e, &i, "original_scaled"), 0);
        assert_eq!(field::<u128>(e, &i, "redistribution_scaled"), 0);
        let received = field::<u128>(e, &i, "received");
        assert!(received > 0);
        assert_eq!(field::<u128>(e, &i, "remaining_scaled"), received * SCALE);
        reserved += received;
    }
    assert!(totals(f).quarantine >= reserved);
}
fn conserve(f: &F, owners: &[Address], extra: Option<&Address>, label: &str) {
    let mut pending = 0;
    let mut fractions = 0;
    for owner in owners.iter().chain(extra.into_iter()) {
        let h = holder(f, owner);
        pending += h.0;
        fractions += h.1;
    }
    let t = totals(f);
    assert_eq!(
        t.received * SCALE,
        (t.paid + t.quarantine + pending) * SCALE + fractions + t.rounding_scaled
    );
    assert_eq!(balance(f, &f.reward, &f.share) as u128, t.received - t.paid);
    for custodian in [&f.margin, &f.receipt, &f.facade] {
        assert_eq!(holder(f, custodian), (0, 0));
    }
    std::println!("ATOMIC_QUALIFICATION_CONSERVATION label={label} received={} paid={} pending={pending} fractions={fractions} quarantine={} rounding={} exact_scaled=true no_waiver=true",t.received,t.paid,t.quarantine,t.rounding_scaled);
}
fn debt_zero(f: &F, id: u64) {
    assert_eq!(
        inspect::<u128>(
            f,
            &f.cash,
            "get_margin_borrow_balance",
            (id,).into_val(&f.e)
        ),
        0
    );
}
fn no_custody(f: &F, owner: &Address) {
    for c in [&f.margin, &f.receipt] {
        assert_eq!(
            inspect::<u128>(
                f,
                &f.share,
                "custody_allocation",
                (c.clone(), owner.clone()).into_val(&f.e)
            ),
            0
        );
    }
}
fn repay_pending(f: &F, owner: &Address, id: u64) {
    let paid: u128 = funded(
        f,
        owner,
        id,
        "recover_pending_open_funded",
        false,
        MAX_REPAY,
    );
    measure(&f.e, "atomic_qualification_pending_recovery", true);
    assert!(paid > MARGIN, "full recovery must repay accrued interest");
    debt_zero(f, id);
    no_custody(f, owner);
}

#[test]
fn compiled_atomic_qualification_swap_close_64_owners_128_changes_8_unresolved_5pct_trade() {
    let (f, owners, ids, selected) = fixture(false, true);
    let e = &f.e;
    let mut cursor = 0;
    let mut intervals = std::vec::Vec::new();
    for (n, id) in ids.iter().enumerate() {
        advance(&f, 1200 + n as u64 * 80);
        let saved = fault(&f);
        prove_both_calls_fail(&f);
        repay_pending(&f, &owners[n + 1], *id);
        check_boundaries(&f, &mut cursor, &mut intervals);
        restore(&f, saved);
        assert!(call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
    }
    assert_eq!(intervals.len(), 8);
    verify_quarantine(&f, &intervals);
    assert!(call::<bool>(
        e,
        &f.share,
        "refresh_position_amounts",
        ().into_val(e)
    ));
    assert!(call::<u128>(e, &f.share, "refresh_nav_root", ().into_val(e)) > 0);
    // Retain one independent aged real debt during the assessed open, rather
    // than accidentally measuring an empty dynamic-interest market.
    let aged_owner = &owners[9];
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), aged_owner.clone(), 500_000_000i128).into_val(e),
        &[],
    );
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (aged_owner.clone(), f.cash.clone(), 100_000_000i128).into_val(e),
        sub_invokes: &[],
    };
    auth::<()>(
        e,
        aged_owner,
        &f.cash,
        "deposit",
        (aged_owner.clone(), 100_000_000u128).into_val(e),
        &[transfer],
    );
    let (f, owners) = rebase(f, owners);
    let e = &f.e;
    let aged_owner = &owners[9];
    enable_measurement(&f, &selected);
    let aged_id = begin_for(&f, aged_owner);
    auth::<()>(
        e,
        aged_owner,
        &f.margin,
        "swap_open_position_v3",
        (aged_owner.clone(), aged_id).into_val(e),
        &[],
    );
    measure(e, "atomic_qualification_aged_swap_open", true);
    // The next independent root's argument allocation must not spend the
    // previous root's meter. Its complete measurements were recorded above.
    crate::cl_resource_limits::reset(e);
    auth::<()>(
        e,
        aged_owner,
        &f.margin,
        "activate_open_position_v3",
        (aged_owner.clone(), aged_id).into_val(e),
        &[],
    );
    measure(e, "atomic_qualification_aged_activate", true);
    crate::cl_resource_limits::reset(e);
    MEASURE.with(|m| m.set(false));
    std::env::set_var(
        "CL_RESOURCE_PROFILE",
        std::env::var("CL_ATOMIC_SETUP_PROFILE").unwrap(),
    );
    crate::cl_resource_limits::configure(e);
    advance(&f, 1900);
    assert!(
        inspect::<u128>(
            &f,
            &f.cash,
            "get_margin_borrow_balance",
            (aged_id,).into_val(e)
        ) > MARGIN
    );
    // Fill the assessed borrower's actual position vector to its supported
    // maximum without inventing 63 debt-funded swaps. These are ordinary live
    // pending opens with locked margin; they have no actual debt or venue assets.
    f.deposit_cash(2_000_000_000);
    let prefix: std::vec::Vec<u64> = (0..63).map(|_| begin_for(&f, &f.user)).collect();
    for id in &prefix {
        debt_zero(&f, *id);
    }
    assert_eq!(
        inspect::<Vec<u64>>(
            &f,
            &f.margin,
            "get_user_positions",
            (f.user.clone(),).into_val(e)
        )
        .len(),
        63
    );
    std::println!("ATOMIC_QUALIFICATION_POSITION_PREFIX pending_begin_only=63 zero_actual_debt=true real_locked_margin=1575000000 existing_position_cap=64");
    advance(&f, 2000);
    let (f, owners) = rebase(f, owners);
    let e = &f.e;
    let before = totals(&f);
    enable_measurement(&f, &selected);
    let trade = 5_000_000_000u128;
    let quote: u128 = inspect(
        &f,
        &f.pool,
        "estimate_swap",
        (0u32, 1u32, trade).into_val(e),
    );
    let transfer = MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.pool.clone(), trade as i128).into_val(e),
        sub_invokes: &[],
    };
    let got: u128 = auth(
        e,
        &f.user,
        &f.pool,
        "swap",
        (f.user.clone(), 0u32, 1u32, trade, quote).into_val(e),
        &[transfer],
    );
    measure(e, "atomic_qualification_5pct_pool_trade", true);
    assert_eq!(got, quote);
    assert!(call::<bool>(
        e,
        &f.share,
        "refresh_position_amounts",
        ().into_val(e)
    ));
    measure(e, "atomic_qualification_refresh_position", true);
    assert!(call::<u128>(e, &f.share, "refresh_nav_root", ().into_val(e)) > 0);
    measure(e, "atomic_qualification_refresh_nav", true);
    let id = begin(&f);
    measure(e, "atomic_qualification_begin", true);
    assert_eq!(
        inspect::<Vec<u64>>(
            &f,
            &f.margin,
            "get_user_positions",
            (f.user.clone(),).into_val(e)
        )
        .len(),
        64
    );
    advance(&f, 2005);
    let (f, owners) = rebase(f, owners);
    let e = &f.e;
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "swap_open_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    measure(e, "atomic_qualification_swap_open", true);
    assert!(
        totals(&f).received > before.received,
        "swap must claim nonzero new rewards"
    );
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "activate_open_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    measure(e, "atomic_qualification_activate", true);
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "prepare_close_position_v3",
        (f.user.clone(), id).into_val(e),
        &[],
    );
    measure(e, "atomic_qualification_prepare_close", true);
    let shares = balance(&f, &f.share, &f.margin);
    let quote: u128 = inspect(
        &f,
        &f.facade,
        "estimate_swap",
        (1u32, 0u32, shares as u128).into_val(e),
    );
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "swap_close_position_v3",
        (f.user.clone(), id, quote).into_val(e),
        &[],
    );
    measure(e, "atomic_qualification_swap_close", true);
    call::<()>(e, &f.margin, "finish_close_position_v3", (id,).into_val(e));
    measure(e, "atomic_qualification_finish_close", true);
    debt_zero(&f, id);
    assert_eq!(balance(&f, &f.share, &f.facade), 0);
    assert_eq!(
        inspect::<Vec<u64>>(
            &f,
            &f.margin,
            "get_user_positions",
            (f.user.clone(),).into_val(e)
        )
        .len(),
        63
    );
    verify_quarantine(&f, &intervals);
    conserve(&f, &owners, None, "stressed_open_close");
}

#[test]
fn compiled_atomic_qualification_8_independent_liquidations_repeated_both_call_outages() {
    let (f, owners, ids, selected) = fixture(true, true);
    let e = &f.e;
    let liquidator = fresh_liquidator(&f, &owners);
    auth::<()>(
        e,
        &f.user,
        &f.under,
        "transfer",
        (f.user.clone(), liquidator.clone(), 300_000_000i128).into_val(e),
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
    let booked: std::vec::Vec<_> = owners[1..9].iter().map(|o| holder(&f, o)).collect();
    let mut cursor = 0;
    let mut intervals = std::vec::Vec::new();
    enable_measurement(&f, &selected);
    for (n, id) in ids.iter().enumerate() {
        advance(&f, 1200 + n as u64 * 80);
        let saved = fault(&f);
        prove_both_calls_fail(&f);
        let cash = balance(&f, &f.under, &liquidator);
        let (paid, seized): (u128, u128) =
            funded(&f, &liquidator, *id, "liquidate_funded_v3", true, MAX_REPAY);
        measure(e, "atomic_qualification_funded_liquidation", true);
        assert!(
            paid > MARGIN && seized > 0,
            "funded liquidation must repay accrued interest"
        );
        assert_eq!(cash - balance(&f, &f.under, &liquidator), paid as i128);
        debt_zero(&f, *id);
        no_custody(&f, &owners[n + 1]);
        assert_eq!(holder(&f, &owners[n + 1]), booked[n]);
        check_boundaries(&f, &mut cursor, &mut intervals);
        restore(&f, saved);
        assert!(call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
        measure(e, "atomic_qualification_late_claim", true);
        verify_quarantine(&f, &intervals);
        conserve(
            &f,
            &owners,
            Some(&liquidator),
            "independent_liquidation_history",
        );
    }
    assert_eq!(intervals.len(), 8);
    for (owner, old) in owners[1..9].iter().zip(booked) {
        assert_eq!(holder(&f, owner), old);
    }
}

#[test]
fn compiled_atomic_qualification_8_independent_pending_recoveries_repeated_both_call_outages() {
    let (f, owners, ids, selected) = fixture(false, true);
    let e = &f.e;
    let booked: std::vec::Vec<_> = owners[1..9].iter().map(|o| holder(&f, o)).collect();
    let mut cursor = 0;
    let mut intervals = std::vec::Vec::new();
    enable_measurement(&f, &selected);
    for (n, id) in ids.iter().enumerate() {
        advance(&f, 1200 + n as u64 * 80);
        let saved = fault(&f);
        prove_both_calls_fail(&f);
        let before = balance(&f, &f.share, &owners[n + 1]);
        repay_pending(&f, &owners[n + 1], *id);
        assert!(balance(&f, &f.share, &owners[n + 1]) > before);
        assert_eq!(holder(&f, &owners[n + 1]), booked[n]);
        check_boundaries(&f, &mut cursor, &mut intervals);
        restore(&f, saved);
        assert!(call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
        measure(e, "atomic_qualification_late_claim", true);
        verify_quarantine(&f, &intervals);
        conserve(&f, &owners, None, "independent_pending_history");
    }
    assert_eq!(intervals.len(), 8);
    assert_eq!(balance(&f, &f.share, &f.margin), 0);
}

#[test]
fn compiled_atomic_qualification_8_independent_repay_release_and_exited_entitlements() {
    // This existing repay_for_margin route caps its request before accrual.
    // Keep zero/static interest explicit here; dynamic repay/release is NOT
    // qualified by this test. Other three questions use the compiled rate model.
    let (f, owners, ids, selected) = fixture(true, false);
    let e = &f.e;
    let booked: std::vec::Vec<_> = owners[1..9].iter().map(|o| holder(&f, o)).collect();
    let mut cursor = 0;
    let mut intervals = std::vec::Vec::new();
    enable_measurement(&f, &selected);
    for (n, id) in ids.iter().enumerate() {
        advance(&f, 1200 + n as u64 * 80);
        let saved = fault(&f);
        prove_both_calls_fail(&f);
        let debt: u128 = inspect(&f, &f.cash, "get_margin_borrow_balance", (*id,).into_val(e));
        funded::<()>(
            &f,
            &owners[n + 1],
            *id,
            "repay_margin_position_v3",
            false,
            debt,
        );
        measure(e, "atomic_qualification_full_repay", true);
        auth::<()>(
            e,
            &owners[n + 1],
            &f.margin,
            "release_debt_free_position_v3",
            (owners[n + 1].clone(), *id).into_val(e),
            &[],
        );
        measure(e, "atomic_qualification_repay_release", true);
        debt_zero(&f, *id);
        assert_eq!(holder(&f, &owners[n + 1]), booked[n]);
        // Release first credits free receipt-pToken margin; actual recovery
        // ownership boundary is the existing margin-to-spot transfer below.
        let ptokens: u128 = inspect(
            &f,
            &f.margin,
            "get_margin_balance_ptokens",
            (owners[n + 1].clone(), f.share.clone()).into_val(e),
        );
        assert!(ptokens > 0);
        auth::<()>(
            e,
            &owners[n + 1],
            &f.margin,
            "transfer_margin_to_spot",
            (owners[n + 1].clone(), f.share.clone(), ptokens).into_val(e),
            &[],
        );
        measure(e, "atomic_qualification_margin_to_spot_recovery", true);
        check_boundaries(&f, &mut cursor, &mut intervals);
        restore(&f, saved);
        assert!(call::<bool>(
            e,
            &f.share,
            "checkpoint_rewards",
            ().into_val(e)
        ));
        measure(e, "atomic_qualification_late_claim", true);
        auth::<()>(
            e,
            &owners[n + 1],
            &f.receipt,
            "withdraw",
            (owners[n + 1].clone(), ptokens).into_val(e),
            &[],
        );
        measure(
            e,
            "atomic_qualification_receipt_exit_after_dependency_restore",
            true,
        );
        no_custody(&f, &owners[n + 1]);
        verify_quarantine(&f, &intervals);
    }
    assert_eq!(intervals.len(), 8);
    // Actual pool withdrawals need intact User storage. Physical exits below use
    // supported claim kill, distinct from both-call fault during custody release.
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "kill_claim",
        (f.admin.clone(),).into_val(e),
        &[],
    );
    for (n, owner) in owners[1..9].iter().enumerate() {
        let shares = balance(&f, &f.share, owner);
        assert!(shares > 0);
        let principal: Vec<u128> = auth(
            e,
            owner,
            &f.share,
            "emergency_redeem",
            (owner.clone(), shares as u128, false).into_val(e),
            &[],
        );
        measure(e, "atomic_qualification_principal_exit", true);
        assert!(principal.get(0).unwrap() + principal.get(1).unwrap() > 0);
        assert_eq!(balance(&f, &f.share, owner), 0);
        assert_eq!(holder(&f, owner), booked[n]);
        let paid: u128 = auth(
            e,
            owner,
            &f.share,
            "claim_rewards",
            (owner.clone(),).into_val(e),
            &[],
        );
        measure(e, "atomic_qualification_exited_entitlement_claim", true);
        assert_eq!(
            paid, booked[n].0,
            "principal exit must retain booked rewards without waiver"
        );
    }
    verify_quarantine(&f, &intervals);
    conserve(&f, &owners, None, "exited_beneficiaries");
}
