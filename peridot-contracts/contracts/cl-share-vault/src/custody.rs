//! Beneficial reward ownership while actual shares sit in immutable custodians.
//! Only a registered custody contract can label its own holdings. Its entrypoints
//! must validate the receipt/position lifecycle before preparing a recovery move.
//! Share-token authorization is still required on the subsequent exact transfer.
use crate::{
    rewards,
    storage::{admin, shares_of},
};
use soroban_sdk::{contracttype, Address, Env, Executable};
#[contracttype]
#[derive(Clone)]
enum Key {
    Registered(Address),
    Allocation(Address, Address),
    Total(Address),
    Beneficial(Address),
    Unassigned,
    Transfer(Address),
}
#[contracttype]
#[derive(Clone)]
struct Transfer {
    owner: Address,
    recipient: Address,
    amount: u128,
    ledger: u32,
    recovery: bool,
}
fn read(e: &Env, k: &Key) -> u128 {
    bump(e, k);
    e.storage().persistent().get(k).unwrap_or(0)
}
fn write(e: &Env, k: &Key, n: u128) {
    if n == 0 {
        e.storage().persistent().remove(k);
    } else {
        e.storage().persistent().set(k, &n);
        bump(e, k);
    }
}
fn bump(e: &Env, k: &Key) {
    if e.storage().persistent().has(k) {
        e.storage().persistent().extend_ttl(k, 500_000, 1_000_000);
    }
}
pub fn registered(e: &Env, c: &Address) -> bool {
    bump(e, &Key::Registered(c.clone()));
    e.storage()
        .persistent()
        .get(&Key::Registered(c.clone()))
        .unwrap_or(false)
}
pub fn register(e: &Env, c: &Address) {
    admin(e).require_auth();
    // Recovery privileges require deployed contract code enforcing a lifecycle.
    // A G-account (even if funded) and a SAC cannot provide that policy.
    assert!(
        matches!(c.executable(), Some(Executable::Wasm(_))),
        "custodian must be WASM"
    );
    assert!(!registered(e, c), "custodian fixed");
    assert_eq!(shares_of(e, c), 0, "custodian funded");
    assert_eq!(
        read(e, &Key::Beneficial(c.clone())),
        0,
        "custodian beneficiary"
    );
    e.storage()
        .persistent()
        .set(&Key::Registered(c.clone()), &true);
    bump(e, &Key::Registered(c.clone()));
}
pub fn allocation(e: &Env, c: &Address, owner: &Address) -> u128 {
    read(e, &Key::Allocation(c.clone(), owner.clone()))
}
pub fn effective(e: &Env, a: &Address) -> u128 {
    let direct = if registered(e, a) { 0 } else { shares_of(e, a) };
    direct
        .checked_add(read(e, &Key::Beneficial(a.clone())))
        .expect("weight overflow")
}
pub fn unassigned(e: &Env) -> u128 {
    e.storage().instance().get(&Key::Unassigned).unwrap_or(0)
}
fn change_unassigned(e: &Env, n: u128, add: bool) {
    let old = unassigned(e);
    e.storage().instance().set(
        &Key::Unassigned,
        &if add {
            old.checked_add(n).unwrap()
        } else {
            old.checked_sub(n).unwrap()
        },
    );
}
fn authorize(e: &Env, c: &Address) {
    assert!(registered(e, c), "unknown custodian");
    c.require_auth();
}
fn add(e: &Env, c: &Address, owner: &Address, n: u128) {
    assert!(!registered(e, owner), "nested custody");
    let k = Key::Allocation(c.clone(), owner.clone());
    write(e, &k, read(e, &k).checked_add(n).unwrap());
    let k = Key::Total(c.clone());
    write(e, &k, read(e, &k).checked_add(n).unwrap());
    let k = Key::Beneficial(owner.clone());
    write(e, &k, read(e, &k).checked_add(n).unwrap());
}
fn remove(e: &Env, c: &Address, owner: &Address, n: u128) {
    let k = Key::Allocation(c.clone(), owner.clone());
    write(e, &k, read(e, &k).checked_sub(n).expect("allocation short"));
    let k = Key::Total(c.clone());
    write(e, &k, read(e, &k).checked_sub(n).unwrap());
    let k = Key::Beneficial(owner.clone());
    write(e, &k, read(e, &k).checked_sub(n).unwrap());
}
/// Called for each physical mint/transfer arriving at a registered custodian.
pub fn received(e: &Env, to: &Address, n: u128) {
    if registered(e, to) {
        change_unassigned(e, n, true);
    }
}
pub fn assign(e: &Env, c: &Address, owner: &Address, n: u128, recovery: bool) {
    authorize(e, c);
    assert!(n > 0, "zero allocation");
    assert!(
        n <= shares_of(e, c)
            .checked_sub(read(e, &Key::Total(c.clone())))
            .unwrap(),
        "backing short"
    );
    if recovery {
        rewards::recovery(e, owner, true, false);
    } else {
        rewards::normal(e, owner);
    }
    add(e, c, owner, n);
    change_unassigned(e, n, false);
    rewards::reset_debt(e, owner);
}
pub fn reassign(e: &Env, c: &Address, from: &Address, to: &Address, n: u128, recovery: bool) {
    authorize(e, c);
    assert!(n > 0 && n <= allocation(e, c, from), "invalid allocation");
    if recovery {
        rewards::recovery(e, from, true, false);
        rewards::recovery(e, to, true, false);
    } else {
        rewards::normal(e, from);
        rewards::settle(e, to);
    }
    remove(e, c, from, n);
    add(e, c, to, n);
    rewards::reset_debt(e, from);
    rewards::reset_debt(e, to);
}
pub fn prepare(e: &Env, c: &Address, owner: &Address, to: &Address, n: u128, recovery: bool) {
    authorize(e, c);
    assert!(n > 0 && n <= allocation(e, c, owner), "invalid allocation");
    let k = Key::Transfer(c.clone());
    if let Some(t) = e.storage().temporary().get::<_, Transfer>(&k) {
        assert!(t.ledger < e.ledger().sequence(), "context exists");
    }
    e.storage().temporary().set(
        &k,
        &Transfer {
            owner: owner.clone(),
            recipient: to.clone(),
            amount: n,
            ledger: e.ledger().sequence(),
            recovery,
        },
    );
}
/// Called by token transfer after its real from.require_auth(). Returns the
/// original beneficial owner, preserving all old accrued rewards with that owner.
pub fn outgoing(e: &Env, c: &Address, to: &Address, n: u128) -> Option<Address> {
    if !registered(e, c) {
        return None;
    }
    let k = Key::Transfer(c.clone());
    let t: Transfer = e.storage().temporary().get(&k).expect("context required");
    assert!(
        t.ledger == e.ledger().sequence() && t.recipient == *to && t.amount == n,
        "context mismatch"
    );
    e.storage().temporary().remove(&k);
    if t.recovery {
        rewards::recovery(e, &t.owner, true, false);
        rewards::recovery(e, to, true, false);
    } else {
        rewards::normal(e, &t.owner);
        rewards::settle(e, to);
    }
    remove(e, c, &t.owner, n);
    Some(t.owner)
}
/// Physical redemption by a custodian is forbidden: it must release to the
/// authorised recipient first so principal/reward ownership cannot be conflated.
pub fn require_direct_holder(e: &Env, a: &Address) {
    assert!(!registered(e, a), "release custody first");
}
