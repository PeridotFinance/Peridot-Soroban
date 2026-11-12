//! Expiring SEP-41 approvals do not change share or reward ownership.
//! Temporary entries cannot resurrect an expired approval after restoration.
use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
enum Key {
    ShareAllowance(Address, Address),
}
#[contracttype]
#[derive(Clone)]
struct Approval {
    amount: i128,
    expiration_ledger: u32,
}

pub fn allowance(e: &Env, from: &Address, spender: &Address) -> i128 {
    read(e, from, spender).map(|a| a.amount).unwrap_or(0)
}
fn read(e: &Env, from: &Address, spender: &Address) -> Option<Approval> {
    e.storage()
        .temporary()
        .get::<_, Approval>(&Key::ShareAllowance(from.clone(), spender.clone()))
        .filter(|a| a.expiration_ledger >= e.ledger().sequence())
}
pub fn approve(e: &Env, from: &Address, spender: &Address, amount: i128, expiration_ledger: u32) {
    assert!(amount >= 0, "negative allowance");
    let key = Key::ShareAllowance(from.clone(), spender.clone());
    if amount == 0 {
        e.storage().temporary().remove(&key);
        return;
    }
    // Custodian movements require the direct, exact lifecycle authorization;
    // a general spender approval must not become a recovery-transfer bypass.
    assert!(
        !crate::custody::registered(e, from),
        "custody allowance forbidden"
    );
    let ttl = expiration_ledger
        .checked_sub(e.ledger().sequence())
        .expect("allowance already expired");
    assert!(ttl <= e.storage().max_ttl(), "allowance expiry exceeds TTL");
    e.storage().temporary().set(
        &key,
        &Approval {
            amount,
            expiration_ledger,
        },
    );
    e.storage().temporary().extend_ttl(&key, ttl, ttl);
}
pub fn spend(e: &Env, from: &Address, spender: &Address, amount: i128) {
    assert!(amount >= 0, "negative transfer");
    assert!(
        !crate::custody::registered(e, from),
        "delegated custody forbidden"
    );
    if amount == 0 {
        return;
    }
    let mut approval = read(e, from, spender).expect("allowance unavailable");
    approval.amount = approval
        .amount
        .checked_sub(amount)
        .expect("allowance overflow");
    assert!(approval.amount >= 0, "insufficient allowance");
    let key = Key::ShareAllowance(from.clone(), spender.clone());
    if approval.amount == 0 {
        e.storage().temporary().remove(&key);
    } else {
        // Preserve the original expiration. Successful spending never renews it.
        e.storage().temporary().set(&key, &approval);
    }
}
