//! Development-only persistent reward ownership. No public contract entrypoints.
//!
//! The coordinator MUST checkpoint pool claims before every pToken mutation and
//! supply actual pre-mutation balances as weights. These internal functions are
//! not user permissions. Token registration, recycled backing yield, production
//! hooks and legacy migration remain gated outside this storage layer.
use crate::reward_backing as backing;
use soroban_sdk::{contracttype, token, Address, Env, Vec, U256};

pub const SCALE: u128 = 1_000_000_000_000;
const TTL_LOW: u32 = 500_000;
const TTL_HIGH: u32 = 1_000_000;

#[contracttype]
#[derive(Clone)]
pub enum LedgerKey {
    HybridStreams,
    HybridStream(Address),
    HybridAccount(Address, Address),
    HybridEpoch(Address, u64),
    HybridRegistryInitialized,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    pub epoch: u64,
    pub total_weight: u128,
    pub raw_index: u128,
    pub units_index: u128,
    pub raw: u128,
    pub reserved: u128,
    pub units: u128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Account {
    pub weight: u128,
    pub epoch: u64,
    pub raw_index: u128,
    pub raw_scaled: u128,
    pub units_scaled: u128,
    pub reserved: u128,
}

#[contracttype]
#[derive(Clone)]
pub struct ClosedEpoch {
    pub raw_index: u128,
    pub rate: u128,
    pub units_index: u128,
}

fn add(a: u128, b: u128) -> u128 {
    a.checked_add(b).expect("reward ledger overflow")
}
fn sub(a: u128, b: u128) -> u128 {
    a.checked_sub(b).expect("reward ledger underflow")
}
fn mul(a: u128, b: u128) -> u128 {
    a.checked_mul(b).expect("reward index overflow")
}
fn ratio(env: &Env, a: u128, b: u128, d: u128) -> u128 {
    assert!(d > 0, "zero reward denominator");
    U256::from_u128(env, a)
        .mul(&U256::from_u128(env, b))
        .div(&U256::from_u128(env, d))
        .to_u128()
        .expect("reward ratio overflow")
}
fn bump(env: &Env, key: &LedgerKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_LOW, TTL_HIGH);
}
fn put_stream(env: &Env, asset: &Address, value: &Stream) {
    let key = LedgerKey::HybridStream(asset.clone());
    env.storage().persistent().set(&key, value);
    bump(env, &key);
}
fn put_account(env: &Env, asset: &Address, owner: &Address, value: &Account) {
    let key = LedgerKey::HybridAccount(asset.clone(), owner.clone());
    // Never remove zero-weight accounts: they can retain fractions/deferred claims.
    env.storage().persistent().set(&key, value);
    bump(env, &key);
}
fn cash(env: &Env, asset: &Address) -> u128 {
    token::Client::new(env, asset)
        .balance(&env.current_contract_address())
        .try_into()
        .expect("negative reward cash")
}

/// Trusted coordinator registration; no owner iteration and no removal/reuse of
/// streams with history. A later stream can initialize against existing weights.
pub fn register(env: &Env, asset: &Address, total_weight: u128) {
    let key = LedgerKey::HybridStream(asset.clone());
    assert!(
        !env.storage().persistent().has(&key),
        "reward stream already registered"
    );
    let mut assets: Vec<Address> = match env.storage().persistent().get(&LedgerKey::HybridStreams) {
        Some(assets) => assets,
        None => {
            assert!(
                !env.storage()
                    .instance()
                    .get::<_, bool>(&LedgerKey::HybridRegistryInitialized)
                    .unwrap_or(false),
                "reward registry requires restoration"
            );
            Vec::new(env)
        }
    };
    assert!(
        !assets.contains(asset.clone()),
        "missing registered reward state"
    );
    assert!(assets.len() < 4, "reward stream limit reached");
    assets.push_back(asset.clone());
    env.storage()
        .instance()
        .set(&LedgerKey::HybridRegistryInitialized, &true);
    env.storage()
        .persistent()
        .set(&LedgerKey::HybridStreams, &assets);
    bump(env, &LedgerKey::HybridStreams);
    put_stream(
        env,
        asset,
        &Stream {
            epoch: 0,
            total_weight,
            raw_index: 0,
            units_index: 0,
            raw: 0,
            reserved: 0,
            units: 0,
        },
    );
}
pub fn stream(env: &Env, asset: &Address) -> Stream {
    let key = LedgerKey::HybridStream(asset.clone());
    let value = env
        .storage()
        .persistent()
        .get(&key)
        .expect("reward stream missing");
    bump(env, &key);
    value
}

/// Retained registry, including retired streams with historical liabilities.
pub fn assets(env: &Env) -> Vec<Address> {
    let key = LedgerKey::HybridStreams;
    let assets = env
        .storage()
        .persistent()
        .get(&key)
        .expect("reward registry missing");
    bump(env, &key);
    assets
}

/// O(1) historical lookup even across many conversions. The supplied weight is
/// read by the coordinator from real pTokens, never accepted from a user argument.
pub fn checkpoint(env: &Env, asset: &Address, owner: &Address, weight: u128) -> Account {
    assert_ne!(
        *owner,
        env.current_contract_address(),
        "escrow is not an ordinary reward holder"
    );
    let s = stream(env, asset);
    assert!(weight <= s.total_weight, "holder exceeds reward weight");
    let key = LedgerKey::HybridAccount(asset.clone(), owner.clone());
    let mut a: Account = env.storage().persistent().get(&key).unwrap_or(Account {
        weight,
        epoch: 0,
        raw_index: 0,
        raw_scaled: 0,
        units_scaled: 0,
        reserved: 0,
    });
    assert_eq!(a.weight, weight, "missed holder weight checkpoint");
    assert!(a.epoch <= s.epoch, "invalid account epoch");
    if a.epoch == s.epoch {
        a.raw_scaled = add(a.raw_scaled, mul(weight, sub(s.raw_index, a.raw_index)));
    } else {
        let key = LedgerKey::HybridEpoch(asset.clone(), a.epoch);
        let closed: ClosedEpoch = env
            .storage()
            .persistent()
            .get(&key)
            .expect("reward epoch missing");
        bump(env, &key);
        let raw = add(
            a.raw_scaled,
            mul(weight, sub(closed.raw_index, a.raw_index)),
        );
        let first = ratio(env, raw, closed.rate, SCALE);
        let later = mul(weight, sub(s.units_index, closed.units_index));
        a.units_scaled = add(a.units_scaled, add(first, later));
        a.raw_scaled = mul(weight, s.raw_index);
    }
    a.epoch = s.epoch;
    a.raw_index = s.raw_index;
    put_account(env, asset, owner, &a);
    a
}

/// Invoke around the real pToken mutation, AFTER fresh pool reward recognition.
pub fn change_weight(env: &Env, asset: &Address, owner: &Address, before: u128, after: u128) {
    let mut a = checkpoint(env, asset, owner, before);
    let mut s = stream(env, asset);
    s.total_weight = add(sub(s.total_weight, before), after);
    a.weight = after;
    put_account(env, asset, owner, &a);
    put_stream(env, asset, &s);
}

pub struct ReceiveSnapshot {
    asset: Address,
    balance: u128,
    stream: Stream,
}
pub fn begin_receive(env: &Env, asset: &Address) -> ReceiveSnapshot {
    ReceiveSnapshot {
        asset: asset.clone(),
        balance: cash(env, asset),
        stream: stream(env, asset),
    }
}
/// Attribute only the newly received balance delta. Pre-existing donations and
/// previously reserved rewards cannot be recognized for a second time.
pub fn finish_receive(env: &Env, snapshot: ReceiveSnapshot, reported: u128) {
    finish_receive_partitioned(env, snapshot, reported, reported);
}

/// The coordinator independently records the remainder as old escrow's rewards.
/// Verify the WHOLE cash delta while indexing only the ordinary-holder portion.
pub fn finish_receive_partitioned(
    env: &Env,
    snapshot: ReceiveSnapshot,
    reported: u128,
    ordinary: u128,
) {
    let mut s = stream(env, &snapshot.asset);
    assert_eq!(s, snapshot.stream, "ledger changed during claim");
    let received = sub(cash(env, &snapshot.asset), snapshot.balance);
    assert_eq!(received, reported, "reported reward receipt mismatch");
    assert!(ordinary <= received, "invalid reward partition");
    index_ordinary(env, &snapshot.asset, &mut s, ordinary);
}

/// Internal coordinator-only recognition of independently quoted pool debt.
/// No cash or backing units are minted. The coordinator must retain a matching
/// per-token receivable and prohibit conversion/payout until it is collected.
pub fn finish_unfunded(env: &Env, snapshot: ReceiveSnapshot, ordinary: u128) {
    let mut s = stream(env, &snapshot.asset);
    assert_eq!(s, snapshot.stream, "ledger changed during observation");
    assert_eq!(
        cash(env, &snapshot.asset),
        snapshot.balance,
        "failed claim changed cash"
    );
    index_ordinary(env, &snapshot.asset, &mut s, ordinary);
}

fn index_ordinary(env: &Env, asset: &Address, s: &mut Stream, ordinary: u128) {
    if ordinary == 0 {
        return;
    }
    assert!(
        s.total_weight > 0,
        "rewards without holders require migration"
    );
    s.raw_index = add(s.raw_index, ratio(env, ordinary, SCALE, s.total_weight));
    s.raw = add(s.raw, ordinary);
    put_stream(env, asset, s);
}

pub struct ConversionSnapshot {
    asset: Address,
    balance: u128,
    stream: Stream,
    units_before: u128,
    unallocated_before: u128,
}
impl ConversionSnapshot {
    pub fn raw_amount(&self) -> u128 {
        self.stream.raw
    }
}
pub fn begin_conversion(env: &Env, asset: &Address) -> ConversionSnapshot {
    let s = stream(env, asset);
    assert!(s.raw > 0, "no unreserved rewards to convert");
    let b = backing::state(env);
    ConversionSnapshot {
        asset: asset.clone(),
        balance: cash(env, asset),
        stream: s,
        units_before: b.units,
        unallocated_before: b.unallocated_units,
    }
}
/// Close only after actual exact-input consumption AND new real backing units.
/// The coordinator settles all recycled-backing streams before issuing units.
pub fn finish_conversion(env: &Env, snapshot: ConversionSnapshot, minted_units: u128) {
    let mut s = stream(env, &snapshot.asset);
    assert_eq!(s, snapshot.stream, "ledger changed during conversion");
    assert_eq!(
        sub(snapshot.balance, cash(env, &snapshot.asset)),
        s.raw,
        "conversion consumed wrong rewards"
    );
    let b = backing::state(env);
    assert_eq!(
        sub(b.units, snapshot.units_before),
        minted_units,
        "unbacked reward units"
    );
    assert_eq!(
        sub(b.unallocated_units, snapshot.unallocated_before),
        minted_units,
        "units already allocated"
    );
    let rate = ratio(env, minted_units, SCALE, s.raw);
    assert!(
        minted_units > 0 && rate > 0,
        "conversion below reward index dust"
    );
    s.units_index = add(s.units_index, ratio(env, s.raw_index, rate, SCALE));
    let key = LedgerKey::HybridEpoch(snapshot.asset.clone(), s.epoch);
    assert!(
        !env.storage().persistent().has(&key),
        "epoch already closed"
    );
    env.storage().persistent().set(
        &key,
        &ClosedEpoch {
            raw_index: s.raw_index,
            rate,
            units_index: s.units_index,
        },
    );
    bump(env, &key);
    s.epoch = s.epoch.checked_add(1).expect("reward epoch overflow");
    s.raw_index = 0;
    s.raw = 0;
    s.units = add(s.units, minted_units);
    assert!(
        cash(env, &snapshot.asset) >= s.reserved,
        "reserved rewards consumed"
    );
    put_stream(env, &snapshot.asset, &s);
}

/// Crystallize ONLY this owner's proportional whole raw tokens for an exit.
/// Fractions and previously compounded units remain with the same owner.
pub fn reserve(env: &Env, asset: &Address, owner: &Address, weight: u128, exiting: u128) -> u128 {
    assert!(
        exiting > 0 && exiting <= weight,
        "invalid reward exit weight"
    );
    let mut a = checkpoint(env, asset, owner, weight);
    let mut s = stream(env, asset);
    let raw = ratio(env, a.raw_scaled, exiting, weight) / SCALE;
    a.raw_scaled = sub(a.raw_scaled, mul(raw, SCALE));
    a.reserved = add(a.reserved, raw);
    s.raw = sub(s.raw, raw);
    s.reserved = add(s.reserved, raw);
    put_account(env, asset, owner, &a);
    put_stream(env, asset, &s);
    raw
}

pub struct ReservedSnapshot {
    asset: Address,
    owner: Address,
    account: Account,
    stream: Stream,
    balance: u128,
    amount: u128,
}
pub fn begin_reserved(
    env: &Env,
    asset: &Address,
    owner: &Address,
    weight: u128,
    amount: u128,
) -> ReservedSnapshot {
    let a = checkpoint(env, asset, owner, weight);
    assert!(
        amount > 0 && amount <= a.reserved,
        "insufficient reserved rewards"
    );
    ReservedSnapshot {
        asset: asset.clone(),
        owner: owner.clone(),
        account: a,
        stream: stream(env, asset),
        balance: cash(env, asset),
        amount,
    }
}
/// Call only after successful conversion AND owner payout in the same invocation.
pub fn finish_reserved(env: &Env, snapshot: ReservedSnapshot) {
    let mut s = stream(env, &snapshot.asset);
    assert_eq!(s, snapshot.stream, "ledger changed during reserved payout");
    let mut a = checkpoint(
        env,
        &snapshot.asset,
        &snapshot.owner,
        snapshot.account.weight,
    );
    assert_eq!(
        a, snapshot.account,
        "account changed during reserved payout"
    );
    assert_eq!(
        sub(snapshot.balance, cash(env, &snapshot.asset)),
        snapshot.amount,
        "reserved swap mismatch"
    );
    a.reserved = sub(a.reserved, snapshot.amount);
    s.reserved = sub(s.reserved, snapshot.amount);
    put_account(env, &snapshot.asset, &snapshot.owner, &a);
    put_stream(env, &snapshot.asset, &s);
}

/// Move only ledger-earned units into the real backing's owner claim record.
pub fn allocate(env: &Env, asset: &Address, owner: &Address, weight: u128) -> u128 {
    let mut a = checkpoint(env, asset, owner, weight);
    let amount = a.units_scaled / SCALE;
    if amount == 0 {
        return 0;
    }
    let mut s = stream(env, asset);
    a.units_scaled = sub(a.units_scaled, mul(amount, SCALE));
    s.units = sub(s.units, amount);
    backing::allocate(env, owner, amount);
    put_account(env, asset, owner, &a);
    put_stream(env, asset, &s);
    amount
}
