//! Native-only LP coordinator. No exported ABI or legacy activation path.
//!
//! All ordinary share mutations checkpoint every retained stream. Fresh emissions
//! are split using pre-mutation pTokens, including the escrow pTokens. Escrow's
//! raw inventory belongs to existing backing units and must be fully converted
//! across ALL streams before issuing or redeeming units. Principal exits reserve
//! ordinary raw claims and do not require a reward conversion.
//!
//! Failed claims may use independently observable per-token pool receivables.
//! Unobservable/decreasing reward quotes still fail closed. Proportional exits
//! avoid receipt NAV quotes but retain strategy guards and a user payout minimum.
//! Pair emissions, fees, migration, restoration and full compiled-stack budgets
//! remain release gates; none of this module is a production ABI.
use crate::{reward_backing as backing, reward_ledger as ledger, DataKey, ReceiptVault};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contractevent, contracttype, token, Address, Env, IntoVal, Map, Symbol, Vec, U256,
};

#[contracttype]
#[derive(Clone)]
pub enum CoordinatorKey {
    RecycledRaw(Address),
    PoolOwed(Address),
    PinnedPrimary,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Nothing,
    Deferred,
    Completed(u128),
}

#[contractevent(topics = ["lp_primary_rotate"])]
pub struct PrimaryRotated {
    pub previous: Address,
    pub next: Address,
    pub managed_cash: u128,
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
fn strategy(env: &Env) -> Address {
    ReceiptVault::get_boosted_vault(env.clone()).expect("LP strategy missing")
}
fn primary(env: &Env) -> Address {
    env.invoke_contract::<Option<Address>>(
        &strategy(env),
        &Symbol::new(env, "get_primary_reward_token"),
        Vec::new(env),
    )
    .expect("primary reward missing")
}
fn require_admin(env: &Env) {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin missing");
    admin.require_auth();
}
fn weight(env: &Env, owner: &Address) -> u128 {
    ReceiptVault::balance(env.clone(), owner.clone())
        .try_into()
        .expect("negative shares")
}
fn cash(env: &Env, asset: &Address) -> u128 {
    token::Client::new(env, asset)
        .balance(&env.current_contract_address())
        .try_into()
        .expect("negative reward cash")
}
fn put_recycled(env: &Env, asset: &Address, raw: u128) {
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
fn sync_pending(env: &Env) {
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

/// Custody-only preparation, staged separately to bound real pool-WASM costs.
/// Keeps the old denomination and every ownership weight. It is NOT permission
/// to skip fresh claims on subsequent mutations or proof that a rotation is ready.
pub fn prepare_rotation(env: &Env, minimum_cash: u128) -> u128 {
    require_admin(env);
    check(env);
    assert_eq!(
        env.storage()
            .instance()
            .get::<_, Address>(&CoordinatorKey::PinnedPrimary),
        Some(primary(env)),
        "uncoordinated primary rotation"
    );
    let supply = ReceiptVault::get_total_ptokens(env.clone());
    let before = backing::state(env);
    let managed = ReceiptVault::unwind_for_rotation(env, minimum_cash);
    assert_eq!(ReceiptVault::get_total_ptokens(env.clone()), supply);
    assert_eq!(backing::state(env), before);
    check(env);
    managed
}

/// Native admin-only, atomic denomination change after custody preparation.
/// Recheck actual zero strategy shares; fresh claims include the pool withdrawal.
/// No outstanding IOU can be re-denominated.
/// Registered old streams, reservations, fractions and backing units survive.
/// The bridge disables observation fallback until real new-primary cash proves
/// its denomination. Governance must coordinate the external pool transition;
/// this function cannot change Aquarius's own reward configuration.
pub fn rotate_primary(env: &Env, next: &Address, minimum_cash: u128) -> u128 {
    require_admin(env);
    let old = primary(env);
    assert_ne!(old, *next, "primary unchanged");
    assert!(
        ledger::assets(env).contains(next.clone()),
        "register new primary first"
    );
    check(env);
    assert_eq!(
        env.storage()
            .instance()
            .get::<_, Address>(&CoordinatorKey::PinnedPrimary),
        Some(old.clone()),
        "uncoordinated primary rotation"
    );
    let supply = ReceiptVault::get_total_ptokens(env.clone());
    let old_backing = backing::state(env);
    assert_eq!(
        token::Client::new(env, &strategy(env)).balance(&env.current_contract_address()),
        0,
        "prepare rotation custody first"
    );
    // Zero strategy shares: this just validates managed cash and the admin floor.
    let managed = ReceiptVault::unwind_for_rotation(env, minimum_cash);
    claim(env); // same ownership weights, includes rewards checkpointed by pool exit
    for asset in ledger::assets(env).iter() {
        assert_eq!(owed(env, &asset), 0, "collect withdrawal rewards first");
    }
    env.invoke_contract::<()>(
        &strategy(env),
        &Symbol::new(env, "hybrid_rotate_primary"),
        (old.clone(), next.clone()).into_val(env),
    );
    assert_eq!(primary(env), *next, "primary switch mismatch");
    env.storage()
        .instance()
        .set(&CoordinatorKey::PinnedPrimary, next);
    assert_eq!(ReceiptVault::get_total_ptokens(env.clone()), supply);
    let after = backing::state(env);
    assert_eq!(
        (after.units, after.ptokens, after.unallocated_units),
        (
            old_backing.units,
            old_backing.ptokens,
            old_backing.unallocated_units
        )
    );
    check(env);
    PrimaryRotated {
        previous: old,
        next: next.clone(),
        managed_cash: managed,
    }
    .publish(env);
    managed
}

fn swap(env: &Env, reward: &Address, raw: u128, minimum: u128) -> Option<u128> {
    let strategy = strategy(env);
    let amount: i128 = raw.try_into().expect("reward amount exceeds i128");
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: reward.clone(),
                fn_name: Symbol::new(env, "transfer"),
                args: (env.current_contract_address(), strategy.clone(), amount).into_val(env),
            },
            sub_invocations: Vec::new(env),
        })
    ]);
    match env.try_invoke_contract::<u128, soroban_sdk::InvokeError>(
        &strategy,
        &Symbol::new(env, "hybrid_swap"),
        (reward.clone(), raw, minimum).into_val(env),
    ) {
        Ok(Ok(output)) => Some(output),
        _ => None,
    }
}

/// Called only after this invocation's fresh all-stream checkpoint. Successful
/// earlier streams may remain settled when a later route defers. Units do not
/// change until EVERY stream's pending inventory is zero; retries cannot replay.
fn settle_recycled(env: &Env) -> bool {
    for asset in ledger::assets(env).iter() {
        if settle_one_recycled(env, &asset) == Outcome::Deferred {
            return false;
        }
    }
    check(env);
    true
}

fn settle_one_recycled(env: &Env, asset: &Address) -> Outcome {
    let raw = recycled(env, asset);
    if raw == 0 {
        return Outcome::Nothing;
    }
    if owed(env, asset) > 0 {
        return Outcome::Deferred;
    }
    let before = cash(env, asset);
    let snapshot = backing::begin_recycled(env);
    let Some(output) = swap(env, asset, raw, 1) else {
        return Outcome::Deferred;
    };
    assert_eq!(
        before.checked_sub(cash(env, asset)),
        Some(raw),
        "recycled swap mismatch"
    );
    assert_eq!(backing::finish_idle(env, snapshot, output, output, 0), 0);
    put_recycled(env, asset, 0);
    sync_pending(env);
    Outcome::Completed(output)
}

/// Permissionless preparation can settle one stream without issuing/redeeming
/// units. Later calls MUST still checkpoint new emissions; this is not permission
/// to skip them or a guarantee that continuously accruing multi-route calls fit.
pub fn recycle(env: &Env, asset: &Address) -> Outcome {
    claim(env);
    let result = settle_one_recycled(env, asset);
    check(env);
    result
}

pub fn compound(env: &Env, asset: &Address) -> Outcome {
    claim(env);
    if !settle_recycled(env) {
        return Outcome::Deferred;
    }
    if ledger::stream(env, asset).raw == 0 {
        return Outcome::Nothing;
    }
    if owed(env, asset) > 0 {
        return Outcome::Deferred;
    }
    let conversion = ledger::begin_conversion(env, asset);
    let snapshot = backing::begin(env);
    let Some(output) = swap(env, asset, conversion.raw_amount(), 1) else {
        return Outcome::Deferred;
    };
    let units = backing::finish_idle(env, snapshot, output, 0, 1);
    ledger::finish_conversion(env, conversion, units);
    check(env);
    Outcome::Completed(units)
}

pub fn deposit(env: &Env, owner: &Address, amount: u128) {
    claim(env);
    let before = weight(env, owner);
    for asset in ledger::assets(env).iter() {
        ledger::checkpoint(env, &asset, owner, before);
    }
    ReceiptVault::deposit(env.clone(), owner.clone(), amount);
    for asset in ledger::assets(env).iter() {
        ledger::change_weight(env, &asset, owner, before, weight(env, owner));
    }
    check(env);
}

pub fn transfer(env: &Env, from: &Address, to: &Address, amount: i128) {
    claim(env);
    let before = weight(env, from);
    let to_before = weight(env, to);
    for asset in ledger::assets(env).iter() {
        ledger::checkpoint(env, &asset, from, before);
        ledger::checkpoint(env, &asset, to, to_before);
    }
    ReceiptVault::transfer(env.clone(), from.clone(), to.clone().into(), amount);
    for asset in ledger::assets(env).iter() {
        ledger::change_weight(env, &asset, from, before, weight(env, from));
        if from != to {
            ledger::change_weight(env, &asset, to, to_before, weight(env, to));
        }
    }
    check(env);
}

/// Quote-priced principal redemption does not invoke any reward route. A failed
/// claim needs observable debt; an unreadable reward quote still fails closed.
pub fn withdraw(env: &Env, owner: &Address, shares: u128) -> Map<Address, u128> {
    claim(env);
    let before = weight(env, owner);
    let mut reserved = Map::new(env);
    for asset in ledger::assets(env).iter() {
        reserved.set(
            asset.clone(),
            ledger::reserve(env, &asset, owner, before, shares),
        );
    }
    ReceiptVault::withdraw(env.clone(), owner.clone(), shares);
    for asset in ledger::assets(env).iter() {
        ledger::change_weight(env, &asset, owner, before, weight(env, owner));
    }
    check(env);
    reserved
}

/// Principal exits remain possible during a paused/unfunded reward claim when
/// ALL outstanding rewards can still be independently observed and attributed.
/// If those observations fail, do not guess historical entitlement or skip it.
pub fn withdraw_proportional(
    env: &Env,
    owner: &Address,
    shares: u128,
    minimum: u128,
) -> (Map<Address, u128>, u128) {
    claim(env);
    let before = weight(env, owner);
    let mut reserved = Map::new(env);
    for asset in ledger::assets(env).iter() {
        reserved.set(
            asset.clone(),
            ledger::reserve(env, &asset, owner, before, shares),
        );
    }
    let payout = ReceiptVault::withdraw_proportional(env.clone(), owner.clone(), shares, minimum);
    for asset in ledger::assets(env).iter() {
        ledger::change_weight(env, &asset, owner, before, weight(env, owner));
    }
    check(env);
    (reserved, payout)
}

pub fn redeem(env: &Env, owner: &Address, minimum: u128) -> Outcome {
    claim(env);
    if !settle_recycled(env) {
        return Outcome::Deferred;
    }
    for asset in ledger::assets(env).iter() {
        ledger::allocate(env, &asset, owner, weight(env, owner));
    }
    let units = backing::owner_units(env, owner);
    if units == 0 {
        return Outcome::Nothing;
    }
    // The backing withdrawal authenticates owner once in this same call frame.
    let paid = backing::redeem(env, owner, units, minimum);
    check(env);
    Outcome::Completed(paid)
}

pub fn settle_reserved(
    env: &Env,
    asset: &Address,
    owner: &Address,
    raw: u128,
    minimum: u128,
) -> Outcome {
    owner.require_auth();
    check(env);
    if owed(env, asset) > 0 {
        claim(env);
        if owed(env, asset) > 0 {
            return Outcome::Deferred;
        }
    }
    let snapshot = ledger::begin_reserved(env, asset, owner, weight(env, owner), raw);
    let Some(output) = swap(env, asset, raw, minimum) else {
        return Outcome::Deferred;
    };
    let underlying = ReceiptVault::get_underlying_token(env.clone());
    let token = token::Client::new(env, &underlying);
    let before = token.balance(owner);
    let amount: i128 = output.try_into().expect("settlement exceeds i128");
    token.transfer(&env.current_contract_address(), owner, &amount);
    assert_eq!(
        token.balance(owner).checked_sub(before),
        Some(amount),
        "settlement payout mismatch"
    );
    assert!(output >= minimum, "settlement below minimum");
    ledger::finish_reserved(env, snapshot);
    check(env);
    Outcome::Completed(output)
}
