//! Native-only all-stream Aquarius claim accounting shared by the lending and
//! lean research engines. No exported ABI or production activation. Pool IOUs
//! never enter principal NAV and cannot fund reward conversions until collected.
use crate::{reward_backing as backing, reward_ledger as ledger, DataKey, ReceiptVault};
use soroban_sdk::{contracttype, token, Address, Env, Map, Symbol, Vec, U256};

#[contracttype]
#[derive(Clone)]
pub enum CoordinatorKey {
    RecycledRaw(Address),
    PoolOwed(Address),
    PinnedPrimary,
}

fn add(a: u128, b: u128) -> u128 {
    a.checked_add(b).expect("coordinator overflow")
}
fn ratio(env: &Env, a: u128, b: u128, d: u128) -> u128 {
    assert!(d > 0, "rewards without supply require migration");
    U256::from_u128(env, a)
        .mul(&U256::from_u128(env, b))
        .div(&U256::from_u128(env, d))
        .to_u128()
        .expect("coordinator ratio overflow")
}
pub(crate) fn strategy(env: &Env) -> Address {
    ReceiptVault::get_boosted_vault(env.clone()).expect("LP strategy missing")
}
pub(crate) fn primary(env: &Env) -> Address {
    env.invoke_contract::<Option<Address>>(
        &strategy(env),
        &Symbol::new(env, "get_primary_reward_token"),
        Vec::new(env),
    )
    .expect("primary reward missing")
}
pub(crate) fn require_admin(env: &Env) {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin missing");
    admin.require_auth();
}
pub(crate) fn weight(env: &Env, owner: &Address) -> u128 {
    ReceiptVault::balance(env.clone(), owner.clone())
        .try_into()
        .expect("negative shares")
}
pub(crate) fn cash(env: &Env, asset: &Address) -> u128 {
    token::Client::new(env, asset)
        .balance(&env.current_contract_address())
        .try_into()
        .expect("negative reward cash")
}
pub(crate) fn put_recycled(env: &Env, asset: &Address, raw: u128) {
    let key = CoordinatorKey::RecycledRaw(asset.clone());
    env.storage().persistent().set(&key, &raw);
    env.storage()
        .persistent()
        .extend_ttl(&key, 500_000, 1_000_000);
}
pub fn recycled(env: &Env, asset: &Address) -> u128 {
    let key = CoordinatorKey::RecycledRaw(asset.clone());
    let value = env
        .storage()
        .persistent()
        .get(&key)
        .expect("recycled history missing");
    env.storage()
        .persistent()
        .extend_ttl(&key, 500_000, 1_000_000);
    value
}

fn put_owed(env: &Env, asset: &Address, value: u128) {
    let key = CoordinatorKey::PoolOwed(asset.clone());
    env.storage().persistent().set(&key, &value);
    env.storage()
        .persistent()
        .extend_ttl(&key, 500_000, 1_000_000);
}
pub fn owed(env: &Env, asset: &Address) -> u128 {
    let key = CoordinatorKey::PoolOwed(asset.clone());
    let value = env
        .storage()
        .persistent()
        .get(&key)
        .expect("pool receivable missing");
    env.storage()
        .persistent()
        .extend_ttl(&key, 500_000, 1_000_000);
    value
}

/// Native admin registration only; retained streams can never be removed/reset.
/// Register new assets before a claim, so untracked donations cannot be indexed.
pub fn register(env: &Env, asset: &Address) {
    require_admin(env);
    assert_ne!(
        *asset,
        ReceiptVault::get_underlying_token(env.clone()),
        "settlement reward unsupported"
    );
    let ordinary = ReceiptVault::get_total_ptokens(env.clone())
        .checked_sub(backing::state(env).ptokens)
        .expect("invalid escrow supply");
    assert!(
        !env.storage()
            .persistent()
            .has(&CoordinatorKey::RecycledRaw(asset.clone())),
        "recycled history exists"
    );
    // Instance pin cannot expire independently of the receipt. Never recreate it
    // after ledger initialization; legacy activation/restoration needs migration.
    if !env.storage().instance().has(&CoordinatorKey::PinnedPrimary) {
        assert!(
            !env.storage()
                .instance()
                .has(&ledger::LedgerKey::HybridRegistryInitialized),
            "primary pin requires restoration"
        );
        let configured = primary(env);
        assert_eq!(*asset, configured, "register primary first");
        env.storage()
            .instance()
            .set(&CoordinatorKey::PinnedPrimary, &configured);
    }
    ledger::register(env, asset, ordinary);
    put_recycled(env, asset, 0);
    put_owed(env, asset, 0);
    check(env);
}

fn pending(env: &Env) -> u32 {
    ledger::assets(env)
        .iter()
        .filter(|asset| recycled(env, asset) > 0)
        .count() as u32
}
pub(crate) fn sync_pending(env: &Env) {
    backing::set_pending_rewards(env, pending(env));
}

/// Bounded custody/receivable checks; receivables are NOT cash or receipt NAV.
/// No holder/history enumeration. Per-token payout gates require owed == 0.
pub fn check(env: &Env) {
    let b = backing::state(env);
    let ordinary = ReceiptVault::get_total_ptokens(env.clone())
        .checked_sub(b.ptokens)
        .expect("invalid escrow supply");
    assert_eq!(
        weight(env, &env.current_contract_address()),
        b.ptokens,
        "untracked escrow shares"
    );
    let assets = ledger::assets(env);
    assert!(
        !assets.is_empty() && assets.len() <= 4,
        "invalid reward registry"
    );
    let mut count = 0;
    let mut unallocated = 0;
    for asset in assets.iter() {
        let s = ledger::stream(env, &asset);
        unallocated = add(unallocated, s.units);
        assert_eq!(s.total_weight, ordinary, "uncheckpointed receipt mutation");
        let r = recycled(env, &asset);
        assert!(r == 0 || b.units > 0, "orphan backing rewards");
        assert!(
            add(cash(env, &asset), owed(env, &asset)) >= add(add(s.raw, s.reserved), r),
            "unbacked raw rewards"
        );
        count += u32::from(r > 0);
    }
    assert_eq!(b.pending_rewards, count, "inconsistent pending streams");
    assert_eq!(
        b.unallocated_units, unallocated,
        "unattributed backing units"
    );
}

/// Attempt every pool claim atomically before changing shares. On failure, use
/// independent complete reward observations, retaining debt separately from cash.
/// Subsequent collection indexes only the increase above already-recorded debt.
/// Unknown tokens/declining debt revert; never reset/reuse old token history.
pub fn claim(env: &Env) {
    check(env);
    let pinned: Address = env
        .storage()
        .instance()
        .get(&CoordinatorKey::PinnedPrimary)
        .expect("primary pin missing");
    assert_eq!(primary(env), pinned, "uncoordinated primary rotation");
    let assets = ledger::assets(env);
    let b = backing::state(env);
    let supply = ReceiptVault::get_total_ptokens(env.clone());
    let ordinary_weight = supply
        .checked_sub(b.ptokens)
        .expect("invalid escrow supply");
    let mut snapshots: [Option<ledger::ReceiveSnapshot>; 4] = [None, None, None, None];
    for (i, asset) in assets.iter().enumerate() {
        snapshots[i] = Some(ledger::begin_receive(env, &asset));
    }
    let attempt = env.try_invoke_contract::<Map<Address, u128>, soroban_sdk::InvokeError>(
        &strategy(env),
        &Symbol::new(env, "hybrid_claim"),
        Vec::new(env),
    );
    let (received, funded): (Map<Address, u128>, bool) = match attempt {
        Ok(Ok(received)) => (received, true),
        _ => (
            env.invoke_contract(
                &strategy(env),
                &Symbol::new(env, "hybrid_reward_quote"),
                Vec::new(env),
            ),
            false,
        ),
    };
    assert_eq!(
        ReceiptVault::get_total_ptokens(env.clone()),
        supply,
        "supply changed during claim"
    );
    assert_eq!(backing::state(env), b, "backing changed during claim");
    for (asset, _) in received.iter() {
        assert!(assets.contains(asset), "unregistered reward token");
    }
    for (i, asset) in assets.iter().enumerate() {
        let observed = received.get(asset.clone()).unwrap_or(0);
        let raw = observed
            .checked_sub(owed(env, &asset))
            .expect("pool receivable decreased without collection");
        // Round the split toward OLD backing; never allocate its dust to newcomers.
        let ordinary = if raw == 0 {
            0
        } else {
            ratio(env, raw, ordinary_weight, supply)
        };
        if funded {
            ledger::finish_receive_partitioned(
                env,
                snapshots[i].take().unwrap(),
                observed,
                ordinary,
            );
            put_owed(env, &asset, 0);
        } else {
            ledger::finish_unfunded(env, snapshots[i].take().unwrap(), ordinary);
            put_owed(env, &asset, observed);
        }
        put_recycled(env, &asset, add(recycled(env, &asset), raw - ordinary));
    }
    sync_pending(env);
    check(env);
}
