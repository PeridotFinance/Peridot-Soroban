//! Claim-first index. Missing boundaries are quarantined, never inferred from time.
use crate::{
    math::mul_div,
    storage::{config, primary_reward_token, state},
};
use soroban_sdk::{
    contracterror, contracttype, panic_with_error, token, Address, BytesN, Env, IntoVal, Symbol,
};
const SCALE: u128 = 1_000_000_000_000_000_000;
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RewardError {
    ClaimUnavailable = 801,
    RewardWaiverRequired = 802,
    MissingRewardToken = 803,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Holder {
    pub debt: u128,
    pub pending: u128,
    pub fraction: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Boundary {
    pub holder: Address,
    pub shares: u128,
    pub index: u128,
    pub reading: Option<u128>,
    pub timestamp: u64,
    pub interval: u64,
    pub waived: bool,
    // Pending is an estimate until equality is proven against pinned pool code.
    pub proven: bool,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Totals {
    pub received: u128,
    pub paid: u128,
    pub quarantine: u128,
    pub rounding_scaled: u128,
}
#[contracttype]
#[derive(Clone)]
enum Key {
    Index,
    Holder(Address),
    Count,
    Boundary(u64),
    Interval,
    Open,
    Bucket(u64),
    Totals,
    Evidence(u64, Address),
    Proposal(u64),
}
#[contracttype]
#[derive(Clone)]
struct Attribution {
    amount: u128,
    evidence: BytesN<32>,
    execute_after: u64,
}
const ATTRIBUTION_DELAY_SECONDS: u64 = 86_400;
fn quarantine(e: &Env, interval: u64, amount: u128) {
    let key = Key::Bucket(interval);
    let old: u128 = e.storage().persistent().get(&key).unwrap_or(0);
    e.storage()
        .persistent()
        .set(&key, &old.checked_add(amount).unwrap());
    e.storage()
        .persistent()
        .extend_ttl(&key, 500_000, 1_000_000);
}
fn index(e: &Env) -> u128 {
    e.storage().instance().get(&Key::Index).unwrap_or(0)
}
pub fn holder(e: &Env, a: &Address) -> Holder {
    let k = Key::Holder(a.clone());
    if e.storage().persistent().has(&k) {
        e.storage().persistent().extend_ttl(&k, 500_000, 1_000_000);
    }
    e.storage().persistent().get(&k).unwrap_or(Holder {
        debt: 0,
        pending: 0,
        fraction: 0,
    })
}
fn put(e: &Env, a: &Address, h: &Holder) {
    let k = Key::Holder(a.clone());
    e.storage().persistent().set(&k, h);
    e.storage().persistent().extend_ttl(&k, 500_000, 1_000_000);
}
pub fn totals(e: &Env) -> Totals {
    e.storage().instance().get(&Key::Totals).unwrap_or(Totals {
        received: 0,
        paid: 0,
        quarantine: 0,
        rounding_scaled: 0,
    })
}
fn save(e: &Env, t: &Totals) {
    e.storage().instance().set(&Key::Totals, t);
}
pub fn settle(e: &Env, a: &Address) {
    let mut h = holder(e, a);
    let earned = crate::custody::effective(e, a)
        .checked_mul(index(e))
        .unwrap();
    let scaled = earned
        .checked_sub(h.debt)
        .unwrap()
        .checked_add(h.fraction)
        .unwrap();
    h.pending = h.pending.checked_add(scaled / SCALE).unwrap();
    h.fraction = scaled % SCALE;
    h.debt = earned;
    put(e, a, &h);
}
pub fn reset_debt(e: &Env, a: &Address) {
    let mut h = holder(e, a);
    h.debt = crate::custody::effective(e, a)
        .checked_mul(index(e))
        .unwrap();
    put(e, a, &h);
    crate::reward_history::record_weight(e, a);
}
fn reward(e: &Env) -> Address {
    primary_reward_token(e).unwrap_or_else(|| panic_with_error!(e, RewardError::MissingRewardToken))
}
pub fn checkpoint(e: &Env) -> bool {
    let interval: u64 = e.storage().instance().get(&Key::Interval).unwrap_or(0);
    let open: bool = e.storage().instance().get(&Key::Open).unwrap_or(false);
    let pending = if open { pending_reading(e) } else { None };
    let trusted = open && crate::reward_history::verified(e);
    let r = reward(e);
    let me = e.current_contract_address();
    let tok = token::Client::new(e, &r);
    let before = tok.balance(&me);
    let result = e.try_invoke_contract::<u128, soroban_sdk::InvokeError>(
        &config(e).pool,
        &Symbol::new(e, "claim"),
        (me.clone(),).into_val(e),
    );
    if !matches!(result, Ok(Ok(_))) {
        return false;
    }
    let delta: u128 = tok
        .balance(&me)
        .checked_sub(before)
        .unwrap()
        .try_into()
        .unwrap();
    let mut t = totals(e);
    t.received = t.received.checked_add(delta).unwrap();
    if open || state(e).total_shares == 0 {
        if open {
            crate::reward_history::seal(e, interval, pending, trusted, delta);
        }
        quarantine(e, interval, delta);
        t.quarantine = t.quarantine.checked_add(delta).unwrap();
        // End at an actual claim. A later pending read cannot repair old boundaries.
        e.storage()
            .instance()
            .set(&Key::Interval, &interval.checked_add(1).unwrap());
        e.storage().instance().set(&Key::Open, &false);
    } else {
        let supply = state(e).total_shares;
        let increment = mul_div(e, delta, SCALE, supply);
        e.storage()
            .instance()
            .set(&Key::Index, &index(e).checked_add(increment).unwrap());
        // Physical shares in custody do not acquire a beneficiary until the
        // custodian assigns them. This includes unsolicited donated dust.
        let unowned = increment
            .checked_mul(crate::custody::unassigned(e))
            .unwrap();
        let unowned_whole = unowned / SCALE;
        if unowned_whole > 0 {
            quarantine(e, interval, unowned_whole);
        }
        t.quarantine = t.quarantine.checked_add(unowned_whole).unwrap();
        // Neither division dust nor unowned fractional rewards move to later owners.
        t.rounding_scaled = t
            .rounding_scaled
            .checked_add(delta.checked_mul(SCALE).unwrap() - increment.checked_mul(supply).unwrap())
            .unwrap()
            .checked_add(unowned % SCALE)
            .unwrap();
    }
    save(e, &t);
    true
}
pub fn normal(e: &Env, a: &Address) {
    if !checkpoint(e) {
        panic_with_error!(e, RewardError::ClaimUnavailable);
    }
    settle(e, a);
}
pub fn recovery(e: &Env, a: &Address, forced: bool, waiver: bool) {
    if checkpoint(e) {
        settle(e, a);
        return;
    }
    let reading = pending_reading(e);
    let interval: u64 = e.storage().instance().get(&Key::Interval).unwrap_or(0);
    let trusted = crate::reward_history::verified(e);
    let proven = crate::reward_history::boundary_proven(e, interval, reading, trusted);
    if !forced && !waiver && !proven {
        panic_with_error!(e, RewardError::RewardWaiverRequired);
    }
    let waived = !forced && waiver && !proven;
    crate::reward_history::observe(
        e,
        interval,
        reading,
        trusted,
        if waived { Some(a.clone()) } else { None },
    );
    settle(e, a);
    let id: u64 = e.storage().instance().get(&Key::Count).unwrap_or(0);
    let b = Boundary {
        holder: a.clone(),
        shares: crate::custody::effective(e, a),
        index: index(e),
        reading,
        timestamp: e.ledger().timestamp(),
        interval: e.storage().instance().get(&Key::Interval).unwrap_or(0),
        waived,
        proven,
    };
    e.storage().persistent().set(&Key::Boundary(id), &b);
    e.storage()
        .persistent()
        .extend_ttl(&Key::Boundary(id), 500_000, 1_000_000);
    e.storage()
        .instance()
        .set(&Key::Count, &id.checked_add(1).unwrap());
    e.storage().instance().set(&Key::Open, &true);
}
pub fn boundary(e: &Env, id: u64) -> Option<Boundary> {
    if e.storage().persistent().has(&Key::Boundary(id)) {
        e.storage()
            .persistent()
            .extend_ttl(&Key::Boundary(id), 500_000, 1_000_000);
    }
    e.storage().persistent().get(&Key::Boundary(id))
}
pub fn claim(e: &Env, a: &Address) -> u128 {
    a.require_auth();
    // A failed upstream claim must not freeze already-booked personal rewards.
    let _ = checkpoint(e);
    settle(e, a);
    let mut h = holder(e, a);
    let amount = h.pending;
    h.pending = 0;
    put(e, a, &h);
    let mut t = totals(e);
    t.paid = t.paid.checked_add(amount).unwrap();
    assert!(t.paid + t.quarantine <= t.received);
    save(e, &t);
    if amount > 0 {
        token::Client::new(e, &reward(e)).transfer(
            &e.current_contract_address(),
            a,
            &amount.try_into().unwrap(),
        );
    }
    amount
}
// Governance cannot turn a late estimate into proof. An evidence digest is
// persisted for independently reviewed attribution; no automatic release exists.
pub fn propose_attribute(e: &Env, id: u64, amount: u128, evidence: BytesN<32>) {
    crate::storage::admin(e).require_auth();
    assert!(amount > 0, "zero attribution");
    assert_ne!(evidence, BytesN::from_array(e, &[0; 32]));
    let b = boundary(e, id).expect("unknown boundary");
    assert!(!b.waived && b.shares > 0, "ineligible boundary");
    assert!(
        !e.storage().persistent().has(&Key::Evidence(id, b.holder)),
        "already attributed"
    );
    let proposal = Attribution {
        amount,
        evidence,
        execute_after: e
            .ledger()
            .timestamp()
            .checked_add(ATTRIBUTION_DELAY_SECONDS)
            .unwrap(),
    };
    e.storage().persistent().set(&Key::Proposal(id), &proposal);
    e.storage()
        .persistent()
        .extend_ttl(&Key::Proposal(id), 500_000, 1_000_000);
}
pub fn attribute(e: &Env, id: u64, amount: u128, evidence: BytesN<32>) {
    crate::storage::admin(e).require_auth();
    assert_ne!(evidence, BytesN::from_array(e, &[0; 32]));
    let b = boundary(e, id).expect("unknown boundary");
    assert!(!b.waived && b.shares > 0, "ineligible boundary");
    let proposal: Attribution = e
        .storage()
        .persistent()
        .get(&Key::Proposal(id))
        .expect("proposal required");
    assert!(
        proposal.amount == amount && proposal.evidence == evidence,
        "proposal mismatch"
    );
    assert!(
        e.ledger().timestamp() >= proposal.execute_after,
        "proposal timelock"
    );
    let k = Key::Evidence(id, b.holder.clone());
    assert!(!e.storage().persistent().has(&k), "already attributed");
    let bucket: u128 = e
        .storage()
        .persistent()
        .get(&Key::Bucket(b.interval))
        .expect("claim not reconciled");
    assert!(amount <= bucket);
    e.storage()
        .persistent()
        .set(&Key::Bucket(b.interval), &(bucket - amount));
    if !crate::reward_history::direct_release(e, b.interval, &b.holder, amount) {
        let mut h = holder(e, &b.holder);
        h.pending = h.pending.checked_add(amount).unwrap();
        put(e, &b.holder, &h);
        let mut t = totals(e);
        t.quarantine -= amount;
        save(e, &t);
    }
    e.storage().persistent().remove(&Key::Proposal(id));
    e.storage().persistent().set(&k, &evidence);
    e.storage().persistent().extend_ttl(&k, 500_000, 1_000_000);
}

fn pending_reading(e: &Env) -> Option<u128> {
    e.try_invoke_contract::<u128, soroban_sdk::InvokeError>(
        &config(e).pool,
        &Symbol::new(e, "get_user_reward"),
        (e.current_contract_address(),).into_val(e),
    )
    .ok()
    .and_then(Result::ok)
}
/// Keep fractional historical backing explicit, separate from entitlement credit.
pub(crate) fn credit_history(
    e: &Env,
    a: &Address,
    scaled: u128,
    reserve_before: u128,
    reserve_after: u128,
    interval: u64,
    remaining: u128,
) {
    let mut h = holder(e, a);
    let value = h.fraction.checked_add(scaled).unwrap();
    h.pending = h.pending.checked_add(value / SCALE).unwrap();
    h.fraction = value % SCALE;
    put(e, a, &h);
    let mut t = totals(e);
    t.quarantine = t
        .quarantine
        .checked_sub(reserve_before / SCALE)
        .unwrap()
        .checked_add(reserve_after / SCALE)
        .unwrap();
    t.rounding_scaled = t
        .rounding_scaled
        .checked_sub(reserve_before % SCALE)
        .unwrap()
        .checked_add(reserve_after % SCALE)
        .unwrap();
    save(e, &t);
    e.storage()
        .persistent()
        .set(&Key::Bucket(interval), &(remaining / SCALE));
}

#[cfg(test)]
pub(crate) fn test_history_receipt(e: &Env, amount: u128) {
    let mut t = totals(e);
    t.received = t.received.checked_add(amount).unwrap();
    t.quarantine = t.quarantine.checked_add(amount).unwrap();
    save(e, &t);
}
