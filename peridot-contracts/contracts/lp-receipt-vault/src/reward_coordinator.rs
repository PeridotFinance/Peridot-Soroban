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
use crate::{reward_backing as backing, reward_ledger as ledger, ReceiptVault};
use soroban_sdk::{contractevent, token, Address, Env, IntoVal, Map, Symbol};

pub use crate::reward_claims::{check, claim, owed, recycled, register, CoordinatorKey};
use crate::reward_claims::{primary, require_admin, strategy, weight};
pub use crate::reward_settlement::{compound, recycle, redeem, settle_reserved, Outcome};

#[contractevent(topics = ["lp_primary_rotate"])]
pub struct PrimaryRotated {
    pub previous: Address,
    pub next: Address,
    pub managed_cash: u128,
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
