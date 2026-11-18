//! Bounded historical entitlement settlement. Critical ownership paths append
//! one segment/checkpoint; users settle immutable historical weights separately.
//! Neither liquidations nor emergency exits enumerate beneficiaries or waivers.
use crate::{custody, math::mul_div, rewards, storage};
use soroban_sdk::{contractevent, contracttype, Address, BytesN, Env, Executable};
const SCALE: u128 = 1_000_000_000_000_000_000;
const DELAY: u64 = 86_400;
const PIN: [u8; 32] = [
    0x12, 0xfc, 0xa5, 0xa7, 0xa9, 0x65, 0x77, 0x27, 0x3b, 0x6d, 0x41, 0x84, 0xcf, 0x9c, 0x98, 0x40,
    0x36, 0xcd, 0xa0, 0xe8, 0xf0, 0x59, 0x47, 0x47, 0xe7, 0xb2, 0x93, 0x3d, 0xce, 0xd3, 0x7e, 0xe6,
];
#[contractevent(topics=["reward_attribution_proposed"])]
#[derive(Clone)]
struct AttributionProposed {
    pub segment: u64,
    pub amount: u128,
    pub evidence: BytesN<32>,
    pub execute_after: u64,
}
#[contractevent(topics=["reward_attribution_governed"])]
#[derive(Clone)]
struct AttributionGoverned {
    pub segment: u64,
    pub amount: u128,
    pub evidence: BytesN<32>,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Weight {
    pub revision: u64,
    pub weight: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Interval {
    pub sealed: bool,
    pub first_segment: u64,
    pub segments: u32,
    pub resolved: u32,
    pub expected: u128,
    pub received: u128,
    pub remaining_scaled: u128,
    pub original_scaled: u128,
    pub redistribution_scaled: u128,
    pub waivers: u32,
    pub last_reading: Option<u128>,
    pub last_verified: bool,
    pub direct_governed: bool,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Segment {
    pub interval: u64,
    pub revision: u64,
    pub supply: u128,
    pub owned_supply: u128,
    pub history_complete: bool,
    pub reading_start: Option<u128>,
    pub reading_end: Option<u128>,
    pub expected: Option<u128>,
    pub observed_verified: bool,
    pub governed: bool,
    pub evidence: Option<BytesN<32>>,
    pub waiver_cursor: u32,
    pub waived_weight: u128,
    pub prepared: bool,
    pub credited_original_scaled: u128,
    pub credited_redistribution_scaled: u128,
}
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryTotals {
    pub reserved_scaled: u128,
    pub original_scaled: u128,
    pub redistribution_scaled: u128,
    pub governed_direct_scaled: u128,
}
#[contracttype]
#[derive(Clone)]
struct Waiver {
    holder: Address,
    through: u64,
}
#[contracttype]
#[derive(Clone)]
struct Proposal {
    amount: u128,
    evidence: BytesN<32>,
    after: u64,
}
#[contracttype]
#[derive(Clone)]
enum Key {
    HistoryEnabled,
    HistoryRevision,
    HistoryTracked,
    HistoryCount(Address),
    HistoryWeight(Address, u32),
    HistorySegments,
    HistorySegment(u64),
    HistoryInterval(u64),
    HistoryWaiver(u64, u32),
    HistoryWaiverIndex(u64, Address),
    HistorySettled(u64, Address),
    HistoryProposal(u64),
    HistoryTotals,
}
fn get<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(e: &Env, k: &Key) -> Option<T> {
    if e.storage().persistent().has(k) {
        e.storage().persistent().extend_ttl(k, 500_000, 1_000_000);
    }
    e.storage().persistent().get(k)
}
fn put<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(e: &Env, k: &Key, v: &T) {
    e.storage().persistent().set(k, v);
    e.storage().persistent().extend_ttl(k, 500_000, 1_000_000);
}
fn revision(e: &Env) -> u64 {
    e.storage()
        .instance()
        .get(&Key::HistoryRevision)
        .unwrap_or(0)
}
pub fn count(e: &Env, a: &Address) -> u32 {
    get(e, &Key::HistoryCount(a.clone())).unwrap_or(0)
}
pub fn weight(e: &Env, a: &Address, n: u32) -> Option<Weight> {
    get(e, &Key::HistoryWeight(a.clone(), n))
}
pub fn record_weight(e: &Env, a: &Address) {
    let n = count(e, a);
    let w = custody::effective(e, a);
    let old = if n == 0 {
        0
    } else {
        weight(e, a, n - 1).expect("weight history missing").weight
    };
    if n > 0 && old == w {
        return;
    }
    if n == 0 && w == 0 {
        return;
    }
    let tracked: u128 = e
        .storage()
        .instance()
        .get(&Key::HistoryTracked)
        .unwrap_or(0);
    e.storage().instance().set(
        &Key::HistoryTracked,
        &tracked.checked_sub(old).unwrap().checked_add(w).unwrap(),
    );
    let rev = revision(e).checked_add(1).unwrap();
    e.storage().instance().set(&Key::HistoryRevision, &rev);
    put(
        e,
        &Key::HistoryWeight(a.clone(), n),
        &Weight {
            revision: rev,
            weight: w,
        },
    );
    put(e, &Key::HistoryCount(a.clone()), &n.checked_add(1).unwrap());
}
/// An explicit historical index needs only two reads, independent of holder count.
fn checked_weight(e: &Env, a: &Address, at: u64, n: u32) -> u128 {
    let count = count(e, a);
    if count == 0 {
        assert_eq!(n, 0, "invalid empty checkpoint");
        return 0;
    }
    let w = weight(e, a, n).expect("weight checkpoint missing");
    if w.revision > at {
        assert_eq!(n, 0, "checkpoint after boundary");
        return 0;
    }
    if n + 1 < count {
        assert!(
            weight(e, a, n + 1).unwrap().revision > at,
            "stale checkpoint"
        );
    }
    w.weight
}
/// Only incremental waiver preparation uses this bounded binary search. At most
/// 32 iterations for a u32 checkpoint index; never walks all holders.
fn weight_at(e: &Env, a: &Address, at: u64) -> u128 {
    let mut lo = 0u32;
    let mut hi = count(e, a);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if weight(e, a, mid).unwrap().revision <= at {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == 0 {
        0
    } else {
        weight(e, a, lo - 1).unwrap().weight
    }
}
pub fn enable(e: &Env) {
    crate::require_validation_network(e);
    storage::admin(e).require_auth();
    assert_eq!(
        storage::config(e).pool.executable(),
        Some(Executable::Wasm(BytesN::from_array(e, &PIN))),
        "unverified reward view code"
    );
    e.storage().instance().set(&Key::HistoryEnabled, &true);
}
pub fn verified(e: &Env) -> bool {
    e.storage()
        .instance()
        .get(&Key::HistoryEnabled)
        .unwrap_or(false)
        && storage::config(e).pool.executable()
            == Some(Executable::Wasm(BytesN::from_array(e, &PIN)))
}
pub fn interval(e: &Env, id: u64) -> Option<Interval> {
    get(e, &Key::HistoryInterval(id))
}
pub fn segment(e: &Env, id: u64) -> Option<Segment> {
    get(e, &Key::HistorySegment(id))
}
pub fn totals(e: &Env) -> HistoryTotals {
    e.storage()
        .instance()
        .get(&Key::HistoryTotals)
        .unwrap_or(HistoryTotals {
            reserved_scaled: 0,
            original_scaled: 0,
            redistribution_scaled: 0,
            governed_direct_scaled: 0,
        })
}
fn save_totals(e: &Env, t: &HistoryTotals) {
    e.storage().instance().set(&Key::HistoryTotals, t);
}
fn new_interval(e: &Env) -> Interval {
    Interval {
        sealed: false,
        first_segment: e
            .storage()
            .instance()
            .get(&Key::HistorySegments)
            .unwrap_or(0),
        segments: 0,
        resolved: 0,
        expected: 0,
        received: 0,
        remaining_scaled: 0,
        original_scaled: 0,
        redistribution_scaled: 0,
        waivers: 0,
        last_reading: Some(0),
        last_verified: true,
        direct_governed: false,
    }
}
fn history_complete(e: &Env) -> bool {
    let tracked: u128 = e
        .storage()
        .instance()
        .get(&Key::HistoryTracked)
        .unwrap_or(0);
    tracked
        == storage::state(e)
            .total_shares
            .checked_sub(custody::unassigned(e))
            .unwrap()
}
pub fn boundary_proven(e: &Env, id: u64, reading: Option<u128>, trusted: bool) -> bool {
    let i = interval(e, id).unwrap_or_else(|| new_interval(e));
    trusted
        && history_complete(e)
        && i.last_verified
        && reading
            .and_then(|end| i.last_reading.and_then(|start| end.checked_sub(start)))
            .is_some()
}
/// Snapshot BEFORE ownership changes. A missing observation taints both adjacent
/// segments. A later observation can start a new segment but cannot repair a gap.
pub fn observe(
    e: &Env,
    id: u64,
    reading: Option<u128>,
    trusted: bool,
    waive: Option<Address>,
) -> u64 {
    let mut i = interval(e, id).unwrap_or_else(|| new_interval(e));
    assert!(!i.sealed, "interval sealed");
    let sid: u64 = e
        .storage()
        .instance()
        .get(&Key::HistorySegments)
        .unwrap_or(0);
    let complete = history_complete(e);
    let delta = if trusted && i.last_verified && complete {
        reading.and_then(|end| i.last_reading.and_then(|start| end.checked_sub(start)))
    } else {
        None
    };
    let supply = storage::state(e).total_shares;
    let s = Segment {
        interval: id,
        revision: revision(e),
        supply,
        owned_supply: supply.checked_sub(custody::unassigned(e)).unwrap(),
        history_complete: complete,
        reading_start: i.last_reading,
        reading_end: reading,
        expected: delta,
        observed_verified: delta.is_some(),
        governed: false,
        evidence: None,
        waiver_cursor: 0,
        waived_weight: 0,
        prepared: false,
        credited_original_scaled: 0,
        credited_redistribution_scaled: 0,
    };
    if let Some(n) = delta {
        i.expected = i.expected.checked_add(n).unwrap();
        i.resolved = i.resolved.checked_add(1).unwrap();
    }
    i.last_reading = reading;
    i.last_verified = trusted && reading.is_some();
    i.segments = i.segments.checked_add(1).unwrap();
    if let Some(a) = waive {
        let key = Key::HistoryWaiverIndex(id, a.clone());
        if let Some(n) = get::<u32>(e, &key) {
            put(
                e,
                &Key::HistoryWaiver(id, n),
                &Waiver {
                    holder: a,
                    through: sid,
                },
            );
        } else {
            put(e, &key, &i.waivers);
            put(
                e,
                &Key::HistoryWaiver(id, i.waivers),
                &Waiver {
                    holder: a,
                    through: sid,
                },
            );
            i.waivers = i.waivers.checked_add(1).unwrap();
        }
    }
    put(e, &Key::HistorySegment(sid), &s);
    put(e, &Key::HistoryInterval(id), &i);
    e.storage()
        .instance()
        .set(&Key::HistorySegments, &sid.checked_add(1).unwrap());
    sid
}
pub fn seal(e: &Env, id: u64, reading: Option<u128>, trusted: bool, received: u128) {
    observe(e, id, reading, trusted, None);
    let mut i = interval(e, id).unwrap();
    i.sealed = true;
    i.received = received;
    i.remaining_scaled = received.checked_mul(SCALE).unwrap();
    put(e, &Key::HistoryInterval(id), &i);
    let mut t = totals(e);
    t.reserved_scaled = t.reserved_scaled.checked_add(i.remaining_scaled).unwrap();
    save_totals(e, &t);
}
pub fn prepare(e: &Env, id: u64, limit: u32) -> bool {
    assert!(limit > 0 && limit <= 4, "waiver batch bound");
    let mut s = segment(e, id).expect("segment missing");
    let i = interval(e, s.interval).unwrap();
    assert!(i.sealed, "claim required");
    if s.prepared {
        return true;
    }
    let end = s.waiver_cursor.checked_add(limit).unwrap().min(i.waivers);
    while s.waiver_cursor < end {
        let w: Waiver =
            get(e, &Key::HistoryWaiver(s.interval, s.waiver_cursor)).expect("waiver missing");
        if !s.observed_verified && id <= w.through {
            s.waived_weight = s
                .waived_weight
                .checked_add(weight_at(e, &w.holder, s.revision))
                .unwrap();
        }
        s.waiver_cursor += 1;
    }
    assert!(s.waived_weight <= s.owned_supply, "waiver weights invalid");
    s.prepared = s.waiver_cursor == i.waivers;
    put(e, &Key::HistorySegment(id), &s);
    s.prepared
}
fn waived(e: &Env, id: u64, s: &Segment, a: &Address) -> bool {
    if s.observed_verified {
        return false;
    }
    get::<u32>(e, &Key::HistoryWaiverIndex(s.interval, a.clone()))
        .map(|n| {
            get::<Waiver>(e, &Key::HistoryWaiver(s.interval, n))
                .unwrap()
                .through
                >= id
        })
        .unwrap_or(false)
}
pub fn settle(e: &Env, a: &Address, id: u64, checkpoint: u32) -> u128 {
    let key = Key::HistorySettled(id, a.clone());
    if get::<bool>(e, &key).unwrap_or(false) {
        return 0;
    }
    let mut s = segment(e, id).expect("segment missing");
    let mut i = interval(e, s.interval).unwrap();
    assert!(
        i.sealed && i.resolved == i.segments && !i.direct_governed,
        "interval unresolved"
    );
    assert!(s.history_complete, "historical weights incomplete");
    assert!(s.prepared, "prepare waivers first");
    let w = checked_weight(e, a, s.revision, checkpoint);
    assert!(w <= s.owned_supply, "invalid original weight");
    let available = i.received.min(i.expected).checked_mul(SCALE).unwrap();
    let segment_scaled = if i.expected == 0 {
        0
    } else {
        mul_div(e, available, s.expected.unwrap(), i.expected)
    };
    let (original, redistributed) = if w == 0 || waived(e, id, &s, a) || s.supply == 0 {
        (0, 0)
    } else {
        let original = mul_div(e, segment_scaled, w, s.supply);
        let waiver_amount = mul_div(e, segment_scaled, s.waived_weight, s.supply);
        let eligible = s.owned_supply.checked_sub(s.waived_weight).unwrap();
        let redistributed = if eligible == 0 {
            0
        } else {
            mul_div(e, waiver_amount, w, eligible)
        };
        (original, redistributed)
    };
    let amount = original.checked_add(redistributed).unwrap();
    i.remaining_scaled = i
        .remaining_scaled
        .checked_sub(amount)
        .expect("interval backing short");
    i.original_scaled = i.original_scaled.checked_add(original).unwrap();
    i.redistribution_scaled = i.redistribution_scaled.checked_add(redistributed).unwrap();
    s.credited_original_scaled = s.credited_original_scaled.checked_add(original).unwrap();
    s.credited_redistribution_scaled = s
        .credited_redistribution_scaled
        .checked_add(redistributed)
        .unwrap();
    let mut t = totals(e);
    let before = t.reserved_scaled;
    t.reserved_scaled = t.reserved_scaled.checked_sub(amount).unwrap();
    t.original_scaled = t.original_scaled.checked_add(original).unwrap();
    t.redistribution_scaled = t.redistribution_scaled.checked_add(redistributed).unwrap();
    rewards::credit_history(
        e,
        a,
        amount,
        before,
        t.reserved_scaled,
        s.interval,
        i.remaining_scaled,
    );
    save_totals(e, &t);
    put(e, &Key::HistoryInterval(s.interval), &i);
    put(e, &Key::HistorySegment(id), &s);
    put(e, &key, &true);
    amount
}
pub fn propose(e: &Env, id: u64, amount: u128, evidence: BytesN<32>) {
    storage::admin(e).require_auth();
    assert_ne!(evidence, BytesN::from_array(e, &[0; 32]));
    let s = segment(e, id).expect("segment missing");
    let i = interval(e, s.interval).unwrap();
    assert!(
        i.sealed && s.expected.is_none() && !i.direct_governed,
        "segment already attributed"
    );
    let after = e.ledger().timestamp().checked_add(DELAY).unwrap();
    put(
        e,
        &Key::HistoryProposal(id),
        &Proposal {
            amount,
            evidence: evidence.clone(),
            after,
        },
    );
    AttributionProposed {
        segment: id,
        amount,
        evidence,
        execute_after: after,
    }
    .publish(e);
}
pub fn attribute(e: &Env, id: u64, amount: u128, evidence: BytesN<32>) {
    storage::admin(e).require_auth();
    let p: Proposal = get(e, &Key::HistoryProposal(id)).expect("proposal required");
    assert!(
        p.amount == amount && p.evidence == evidence && e.ledger().timestamp() >= p.after,
        "proposal mismatch or timelock"
    );
    let mut s = segment(e, id).unwrap();
    let mut i = interval(e, s.interval).unwrap();
    assert!(
        i.sealed && s.expected.is_none() && !i.direct_governed,
        "segment already attributed"
    );
    AttributionGoverned {
        segment: id,
        amount,
        evidence: evidence.clone(),
    }
    .publish(e);
    s.expected = Some(amount);
    s.governed = true;
    s.evidence = Some(evidence);
    i.resolved = i.resolved.checked_add(1).unwrap();
    i.expected = i.expected.checked_add(amount).unwrap();
    put(e, &Key::HistorySegment(id), &s);
    put(e, &Key::HistoryInterval(s.interval), &i);
    e.storage().persistent().remove(&Key::HistoryProposal(id));
}
/// Backward-compatible governed per-boundary release. Once used, the interval
/// cannot also pay the automatic/segment route. Evidence remains explicitly governed.
pub fn direct_release(e: &Env, id: u64, a: &Address, amount: u128) -> bool {
    let Some(mut i) = interval(e, id) else {
        return false;
    };
    assert!(
        i.sealed && i.original_scaled == 0 && i.redistribution_scaled == 0,
        "segment settlement already started"
    );
    let scaled = amount.checked_mul(SCALE).unwrap();
    i.remaining_scaled = i
        .remaining_scaled
        .checked_sub(scaled)
        .expect("interval backing short");
    i.direct_governed = true;
    let mut t = totals(e);
    let before = t.reserved_scaled;
    t.reserved_scaled = t.reserved_scaled.checked_sub(scaled).unwrap();
    t.governed_direct_scaled = t.governed_direct_scaled.checked_add(scaled).unwrap();
    rewards::credit_history(
        e,
        a,
        scaled,
        before,
        t.reserved_scaled,
        id,
        i.remaining_scaled,
    );
    save_totals(e, &t);
    put(e, &Key::HistoryInterval(id), &i);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
        IntoVal,
    };
    #[contract]
    struct Fixture;
    #[contractimpl]
    impl Fixture {
        pub fn init(e: Env, admin: Address) {
            e.storage().instance().set(&storage::DataKey::Admin, &admin);
        }
        pub fn propose(e: Env, id: u64, amount: u128, evidence: BytesN<32>) {
            super::propose(&e, id, amount, evidence);
        }
        pub fn attribute(e: Env, id: u64, amount: u128, evidence: BytesN<32>) {
            super::attribute(&e, id, amount, evidence);
        }
    }
    struct F {
        e: Env,
        c: Address,
        a: Address,
        b: Address,
        new: Address,
        admin: Address,
    }
    impl F {
        fn new() -> Self {
            let e = Env::default();
            let admin = Address::generate(&e);
            let a = Address::generate(&e);
            let b = Address::generate(&e);
            let new = Address::generate(&e);
            let c = e.register(Fixture, ());
            FixtureClient::new(&e, &c).init(&admin);
            Self {
                e,
                c,
                a,
                b,
                new,
                admin,
            }
        }
        fn within<T>(&self, f: impl FnOnce() -> T) -> T {
            self.e.as_contract(&self.c, f)
        }
        fn weights(&self, a: u128, b: u128, new: u128) {
            self.within(|| {
                let mut s = storage::state(&self.e);
                s.total_shares = a + b + new;
                storage::set_state(&self.e, &s);
                for (who, w) in [(&self.a, a), (&self.b, b), (&self.new, new)] {
                    storage::set_shares(&self.e, who, w);
                    record_weight(&self.e, who);
                }
            })
        }
        fn observe(&self, r: Option<u128>, trusted: bool, waive: bool) -> u64 {
            self.within(|| {
                super::observe(
                    &self.e,
                    0,
                    r,
                    trusted,
                    if waive { Some(self.a.clone()) } else { None },
                )
            })
        }
        fn seal(&self, r: Option<u128>, trusted: bool, received: u128) {
            self.within(|| {
                super::seal(&self.e, 0, r, trusted, received);
                rewards::test_history_receipt(&self.e, received);
            })
        }
        fn settle(&self, a: &Address, id: u64, n: u32) -> u128 {
            self.within(|| {
                assert!(prepare(&self.e, id, 4));
                super::settle(&self.e, a, id, n)
            })
        }
        fn credit(&self, a: &Address) -> u128 {
            self.within(|| {
                let h = rewards::holder(&self.e, a);
                h.pending * SCALE + h.fraction
            })
        }
        fn conservation(&self) {
            self.within(|| {
                let t = rewards::totals(&self.e);
                let mut credits = 0;
                for a in [&self.a, &self.b, &self.new] {
                    let h = rewards::holder(&self.e, a);
                    credits += h.pending * SCALE + h.fraction;
                }
                assert_eq!(
                    t.received * SCALE,
                    t.paid * SCALE + t.quarantine * SCALE + t.rounding_scaled + credits
                );
            })
        }
        fn govern(&self, id: u64, amount: u128) {
            let evidence = BytesN::from_array(&self.e, &[7; 32]);
            for name in ["propose", "attribute"] {
                self.e.mock_auths(&[MockAuth {
                    address: &self.admin,
                    invoke: &MockAuthInvoke {
                        contract: &self.c,
                        fn_name: name,
                        args: (id, amount, evidence.clone()).into_val(&self.e),
                        sub_invokes: &[],
                    },
                }]);
                let c = FixtureClient::new(&self.e, &self.c);
                if name == "propose" {
                    c.propose(&id, &amount, &evidence);
                    self.e.ledger().with_mut(|l| l.timestamp += DELAY);
                } else {
                    c.attribute(&id, &amount, &evidence);
                }
            }
        }
    }
    #[test]
    fn proportional_shortfall_exited_owner_and_excess_conserve() {
        for received in [80, 100, 120] {
            let f = F::new();
            f.weights(100, 100, 0);
            f.observe(Some(10), true, false);
            f.weights(0, 100, 0);
            f.observe(Some(100), true, false);
            f.seal(Some(100), true, received);
            for id in 0..3 {
                f.settle(&f.a, id, if id == 0 { 0 } else { 1 });
                f.settle(&f.b, id, 0);
            }
            let paid = received.min(100);
            assert_eq!(f.credit(&f.a), paid * SCALE / 20);
            assert_eq!(f.credit(&f.b), paid * SCALE * 19 / 20);
            assert_eq!(f.settle(&f.a, 0, 0), 0);
            f.conservation();
            f.within(|| {
                assert_eq!(
                    rewards::totals(&f.e).quarantine,
                    received.saturating_sub(100)
                )
            });
        }
    }
    #[test]
    fn attributable_tail_goes_to_historical_later_owner_only() {
        let f = F::new();
        f.weights(100, 100, 0);
        f.observe(Some(10), true, false);
        f.weights(0, 100, 0);
        f.observe(Some(100), true, false);
        f.seal(Some(120), true, 120);
        f.weights(0, 0, 100);
        for id in 0..3 {
            f.settle(&f.a, id, if id == 0 { 0 } else { 1 });
            f.settle(&f.b, id, 0);
            f.settle(&f.new, id, 0);
        }
        assert_eq!(f.credit(&f.a), 5 * SCALE);
        assert_eq!(f.credit(&f.b), 115 * SCALE);
        assert_eq!(f.credit(&f.new), 0);
        f.conservation();
    }
    #[test]
    fn missing_boundary_cannot_be_repaired_by_later_cumulative_reading() {
        let f = F::new();
        f.weights(100, 100, 0);
        f.observe(None, false, false);
        f.weights(0, 100, 0);
        f.observe(Some(100), true, false);
        f.seal(Some(120), true, 120);
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.settle(&f.b, 2, 0)));
        assert!(result.is_err());
        f.within(|| {
            let i = interval(&f.e, 0).unwrap();
            assert_eq!(i.resolved, 1);
            assert_eq!(i.segments, 3);
            assert_eq!(rewards::totals(&f.e).quarantine, 120);
        });
        f.conservation();
    }
    #[test]
    fn governed_waiver_redistribution_uses_exited_coowner_and_excludes_newcomer() {
        let f = F::new();
        f.weights(100, 100, 0);
        f.observe(Some(100), false, true);
        f.weights(0, 100, 0);
        f.observe(Some(100), false, false);
        f.weights(0, 0, 100);
        f.seal(Some(100), false, 100);
        for (id, n) in [(0, 100), (1, 0), (2, 0)] {
            f.govern(id, n);
        }
        for id in 0..3 {
            f.settle(&f.a, id, if id == 0 { 0 } else { 1 });
            f.settle(&f.b, id, if id < 2 { 0 } else { 1 });
            f.settle(&f.new, id, 0);
        }
        assert_eq!(f.credit(&f.a), 0);
        assert_eq!(f.credit(&f.b), 100 * SCALE);
        assert_eq!(f.credit(&f.new), 0);
        f.conservation();
        f.within(|| {
            let s = segment(&f.e, 0).unwrap();
            assert!(s.governed && !s.observed_verified);
            assert_eq!(s.credited_original_scaled, 50 * SCALE);
            assert_eq!(s.credited_redistribution_scaled, 50 * SCALE);
        });
    }
    #[test]
    fn partial_waiver_never_erases_later_accrual_and_fractional_backing_conserves() {
        let f = F::new();
        f.weights(200, 200, 0);
        f.observe(Some(3), false, true);
        f.weights(100, 200, 0);
        f.seal(Some(4), false, 4);
        f.govern(0, 3);
        f.govern(1, 1);
        for id in 0..2 {
            f.settle(&f.a, id, if id == 0 { 0 } else { 1 });
            f.settle(&f.b, id, 0);
        }
        assert_eq!(f.credit(&f.a), SCALE / 3);
        assert_eq!(f.credit(&f.b), 3 * SCALE + 2 * SCALE / 3);
        f.conservation();
    }
    #[test]
    fn checkpoint_brackets_and_incremental_preparation_fail_closed() {
        let f = F::new();
        f.weights(200, 200, 0);
        f.observe(Some(100), true, false);
        f.weights(100, 300, 0);
        f.seal(Some(100), true, 100);
        let stale = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.settle(&f.a, 1, 0)));
        assert!(stale.is_err());
        let later = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.settle(&f.a, 0, 1)));
        assert!(later.is_err());
        let batch = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f.within(|| prepare(&f.e, 0, 5))
        }));
        assert!(batch.is_err());
        f.settle(&f.a, 0, 0);
        f.settle(&f.b, 0, 0);
        f.conservation();
    }
    #[test]
    fn multiple_intervals_keep_exact_scaled_backing_separate() {
        let f = F::new();
        f.weights(100, 200, 0);
        for interval in 0..2 {
            f.within(|| {
                super::observe(&f.e, interval, Some(1), false, None);
                super::seal(&f.e, interval, Some(1), false, 1);
                rewards::test_history_receipt(&f.e, 1);
            });
            f.govern(interval * 2, 1);
            f.govern(interval * 2 + 1, 0);
        }
        for id in 0..4 {
            f.settle(&f.a, id, 0);
            f.settle(&f.b, id, 0);
            f.conservation();
        }
        assert_eq!(f.credit(&f.a), 2 * (SCALE / 3));
        assert_eq!(f.credit(&f.b), 2 * (2 * SCALE / 3));
        f.within(|| assert_eq!(totals(&f.e).reserved_scaled, 2));
    }
    #[test]
    fn waiver_preparation_pages_over_fixed_bound_and_preserves_original_revision() {
        let f = F::new();
        let holders: std::vec::Vec<_> = (0..6).map(|_| Address::generate(&f.e)).collect();
        f.within(|| {
            let mut state = storage::state(&f.e);
            state.total_shares = 600;
            storage::set_state(&f.e, &state);
            for a in &holders {
                storage::set_shares(&f.e, a, 100);
                record_weight(&f.e, a);
            }
            for a in &holders {
                super::observe(&f.e, 0, Some(60), false, Some(a.clone()));
            }
            super::seal(&f.e, 0, Some(60), false, 60);
            rewards::test_history_receipt(&f.e, 60);
            assert!(!prepare(&f.e, 0, 4));
            assert_eq!(segment(&f.e, 0).unwrap().waiver_cursor, 4);
            assert!(prepare(&f.e, 0, 4));
            assert_eq!(segment(&f.e, 0).unwrap().waiver_cursor, 6);
            // All six eventually waive their unresolved open-interval reward;
            // their later cutoffs include this earlier immutable segment.
            assert_eq!(segment(&f.e, 0).unwrap().waived_weight, 600);
        });
    }
    #[test]
    fn absent_historical_weights_quarantine_instead_of_using_current_holders() {
        let f = F::new();
        // Explicit legacy/pre-history fixture: existing supply has no snapshots.
        f.within(|| {
            let mut state = storage::state(&f.e);
            state.total_shares = 100;
            storage::set_state(&f.e, &state);
            storage::set_shares(&f.e, &f.a, 100);
        });
        f.observe(Some(100), true, false);
        f.seal(Some(100), true, 100);
        f.within(|| {
            let s = segment(&f.e, 0).unwrap();
            assert!(!s.history_complete);
            assert!(s.expected.is_none());
        });
        f.govern(0, 100);
        f.govern(1, 0);
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.settle(&f.a, 0, 0)));
        assert!(result.is_err());
        f.conservation();
        f.within(|| assert_eq!(rewards::totals(&f.e).quarantine, 100));
    }
}
