//! Native-only durable exit intent for unobservable reward outages.
//!
//! NO tokens move and NO ownership weights change when a request is recorded.
//! Shares are not locked: users may cancel, transfer, or withdraw by another safe
//! path. Execution needs fresh owner authorization, sufficient current shares,
//! a successful all-stream checkpoint and the stored minimum. It is not an
//! immediate-exit bypass or a promise that recovery will become available.
use crate::{reward_coordinator, ReceiptVault};
use soroban_sdk::{contractevent, contracttype, Address, Env, Map};

#[contracttype]
#[derive(Clone)]
pub enum ExitKey {
    LpExitRequest(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExitRecord {
    pub nonce: u64,
    pub pending: bool,
    pub shares: u128,
    pub minimum: u128,
}

#[contractevent(topics = ["lp_exit_request"])]
pub struct ExitRequested {
    pub owner: Address,
    pub nonce: u64,
    pub shares: u128,
    pub minimum: u128,
}

#[contractevent(topics = ["lp_exit_cancel"])]
pub struct ExitCancelled {
    pub owner: Address,
    pub nonce: u64,
}

#[contractevent(topics = ["lp_exit_complete"])]
pub struct ExitCompleted {
    pub owner: Address,
    pub nonce: u64,
    pub underlying: u128,
}

pub fn get(env: &Env, owner: &Address) -> Option<ExitRecord> {
    crate::storage::ensure_initialized(env);
    let key = ExitKey::LpExitRequest(owner.clone());
    let record = env.storage().persistent().get(&key);
    if record.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, 500_000, 1_000_000);
    }
    record
}

fn put(env: &Env, owner: &Address, record: &ExitRecord) {
    let key = ExitKey::LpExitRequest(owner.clone());
    // Retain consumed records/nonces; never delete and recycle a request ID.
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, 500_000, 1_000_000);
}

pub fn request(env: &Env, owner: &Address, shares: u128, minimum: u128) -> u64 {
    owner.require_auth();
    assert_ne!(
        *owner,
        env.current_contract_address(),
        "escrow cannot request exit"
    );
    let balance: u128 = ReceiptVault::balance(env.clone(), owner.clone())
        .try_into()
        .expect("negative shares");
    assert!(shares > 0 && shares <= balance, "invalid requested shares");
    assert!(minimum > 0, "exit minimum required");
    let prior = get(env, owner).unwrap_or(ExitRecord {
        nonce: 0,
        pending: false,
        shares: 0,
        minimum: 0,
    });
    assert!(!prior.pending, "cancel existing request first");
    let nonce = prior.nonce.checked_add(1).expect("exit nonce overflow");
    put(
        env,
        owner,
        &ExitRecord {
            nonce,
            pending: true,
            shares,
            minimum,
        },
    );
    ExitRequested {
        owner: owner.clone(),
        nonce,
        shares,
        minimum,
    }
    .publish(env);
    nonce
}

pub fn cancel(env: &Env, owner: &Address, nonce: u64) {
    owner.require_auth();
    let mut record = get(env, owner).expect("exit request missing");
    assert!(
        record.nonce == nonce && record.pending,
        "exit request mismatch"
    );
    record.pending = false;
    put(env, owner, &record);
    ExitCancelled {
        owner: owner.clone(),
        nonce,
    }
    .publish(env);
}

pub fn execute(env: &Env, owner: &Address, nonce: u64) -> (Map<Address, u128>, u128) {
    let mut record = get(env, owner).expect("exit request missing");
    assert_eq!(record.nonce, nonce, "exit request mismatch");
    assert!(record.pending, "exit request consumed");
    // The principal core authenticates owner once in this call frame. Any
    // failed checkpoint/redemption/minimum/auth rolls back, preserving intent.
    let result =
        reward_coordinator::withdraw_proportional(env, owner, record.shares, record.minimum);
    record.pending = false;
    put(env, owner, &record);
    ExitCompleted {
        owner: owner.clone(),
        nonce,
        underlying: result.1,
    }
    .publish(env);
    result
}
