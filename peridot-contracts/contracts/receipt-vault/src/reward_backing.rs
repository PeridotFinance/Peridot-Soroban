//! Development-only backing library for the future hybrid reward coordinator.
//!
//! Not exported as Soroban entrypoints. The coordinator must checkpoint all reward
//! streams before changing holder weights, allocate units from the epoch ledger,
//! and maintain the pending-reward gate. This module cannot discover pool rewards.
//! No production entrypoint calls it and the default WASM build excludes it.
//!
//! Backing consists of real pTokens held by this ReceiptVault itself. Their value
//! is already included in total pToken supply; reward units are claims on those
//! pTokens, never another claim on ordinary holders' underlying. All share moves
//! are internal TokenBase calls, avoiding ReceiptVault -> helper -> ReceiptVault
//! token-transfer reentrancy. Underlying transfers still use actual token calls.

use crate::{storage::*, ReceiptVault, SCALE_1E6};
use soroban_sdk::{
    contractevent, contracttype, token, vec, Address, Env, IntoVal, Symbol, Vec, U256,
};
use stellar_tokens::fungible::Base as TokenBase;

const TTL_LOW: u32 = 500_000;
const TTL_HIGH: u32 = 1_000_000;

#[contracttype]
#[derive(Clone)]
pub enum BackingKey {
    HybridBackingState,
    HybridBackingOwner(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackingState {
    pub units: u128,
    pub unallocated_units: u128,
    pub ptokens: u128,
    // Maintained by the internal reward-stream coordinator, not arbitrary users.
    pub pending_rewards: u32,
}

#[contractevent(topics = ["reward_backed"])]
pub struct RewardBacked {
    pub underlying: u128,
    pub recycled_underlying: u128,
    pub ptokens: u128,
    pub units: u128,
}

#[contractevent(topics = ["reward_paid"])]
pub struct RewardPaid {
    pub owner: Address,
    pub units: u128,
    pub ptokens: u128,
    pub underlying: u128,
}

/// Cannot be supplied by a contract caller, cloned, persisted or reused. It is
/// consumed by `finish` inside the same invocation that captured it.
pub struct FundingSnapshot {
    token: Address,
    cash: i128,
    managed_cash: u128,
    deposited: u128,
    supply: u128,
    nav: u128,
    initial_rate: u128,
    state: BackingState,
    recycle_only: bool,
}

fn amount(value: u128) -> i128 {
    value.try_into().expect("reward amount exceeds i128")
}

fn mul_div(env: &Env, a: u128, b: u128, denominator: u128) -> u128 {
    assert!(denominator > 0, "reward division by zero");
    U256::from_u128(env, a)
        .mul(&U256::from_u128(env, b))
        .div(&U256::from_u128(env, denominator))
        .to_u128()
        .expect("reward quotient exceeds u128")
}

/// Ordinary principal exits have fail-soft accounting fallback semantics. New
/// reward share pricing instead requires an actually successful positive quote.
/// A failure defers this reward leg, not the existing principal exit path.
fn fresh_nav(env: &Env) -> u128 {
    let nav = ReceiptVault::get_total_underlying(env.clone());
    if let Some(boosted) = env
        .storage()
        .persistent()
        .get::<_, Address>(&DataKey::BoostedVault)
    {
        let shares = token::Client::new(env, &boosted).balance(&env.current_contract_address());
        if shares > 0 {
            let amounts: Vec<i128> = env.invoke_contract(
                &boosted,
                &Symbol::new(env, "get_asset_amounts_per_shares"),
                vec![env, shares.into_val(env)],
            );
            let quote = amounts.get(0).expect("missing reward backing quote");
            assert!(quote > 0, "nonpositive reward backing quote");
            let cached: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::BoostedUnderlyingCached)
                .expect("backing quote not recorded");
            assert_eq!(quote as u128, cached, "inconsistent reward backing quote");
        }
    }
    nav
}

fn put(env: &Env, state: &BackingState) {
    assert!(
        state.unallocated_units <= state.units,
        "unbacked allocation"
    );
    assert_eq!(
        state.units == 0,
        state.ptokens == 0,
        "inconsistent reward backing"
    );
    assert!(
        TokenBase::balance(env, &env.current_contract_address()) >= amount(state.ptokens),
        "reward shares missing"
    );
    env.storage()
        .persistent()
        .set(&BackingKey::HybridBackingState, state);
    env.storage()
        .persistent()
        .extend_ttl(&BackingKey::HybridBackingState, TTL_LOW, TTL_HIGH);
}

pub fn state(env: &Env) -> BackingState {
    let state = env
        .storage()
        .persistent()
        .get(&BackingKey::HybridBackingState)
        .expect("reward backing not initialized");
    env.storage()
        .persistent()
        .extend_ttl(&BackingKey::HybridBackingState, TTL_LOW, TTL_HIGH);
    state
}

/// Internal one-time initialization. Production activation is intentionally absent.
pub fn initialize(env: &Env) {
    ensure_initialized(env);
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin missing");
    admin.require_auth();
    assert!(
        !env.storage()
            .persistent()
            .has(&BackingKey::HybridBackingState),
        "backing already initialized"
    );
    put(
        env,
        &BackingState {
            units: 0,
            unallocated_units: 0,
            ptokens: 0,
            pending_rewards: 0,
        },
    );
}

/// Internal coordinator hook. This is NOT a public permission to clear rewards.
/// The native LP coordinator derives this from all retained streams. Generic
/// test harness setters are stand-ins, not production permissions.
pub fn set_pending_rewards(env: &Env, count: u32) {
    let mut current = state(env);
    current.pending_rewards = count;
    put(env, &current);
}

pub fn owner_units(env: &Env, owner: &Address) -> u128 {
    let key = BackingKey::HybridBackingOwner(owner.clone());
    let value = env.storage().persistent().get(&key).unwrap_or(0);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_LOW, TTL_HIGH);
    }
    value
}

fn set_owner(env: &Env, owner: &Address, value: u128) {
    let key = BackingKey::HybridBackingOwner(owner.clone());
    // Preserve a zero record so restoration never substitutes a fresh liability.
    env.storage().persistent().set(&key, &value);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_LOW, TTL_HIGH);
}

/// Allocate previously minted units after the coordinator validates an account's
/// epoch entitlement. No token or backing can be created by this operation.
pub fn allocate(env: &Env, owner: &Address, units: u128) {
    assert!(units > 0, "zero reward allocation");
    assert_ne!(
        *owner,
        env.current_contract_address(),
        "cannot allocate to backing"
    );
    let mut current = state(env);
    current.unallocated_units = current
        .unallocated_units
        .checked_sub(units)
        .expect("allocation exceeds funded units");
    let balance = owner_units(env, owner)
        .checked_add(units)
        .expect("reward units overflow");
    set_owner(env, owner, balance);
    put(env, &current);
}

/// Capture valuation BEFORE conversion/receipt of fresh settlement assets.
/// Only the future coordinator may decide that those assets are new reward money.
pub fn begin(env: &Env) -> FundingSnapshot {
    begin_mode(env, false)
}

/// Pending rewards may enrich EXISTING units, but never issue new units or pay
/// an owner. The opaque snapshot enforces that restriction again at completion.
pub fn begin_recycled(env: &Env) -> FundingSnapshot {
    begin_mode(env, true)
}

fn begin_mode(env: &Env, recycle_only: bool) -> FundingSnapshot {
    let token = ensure_initialized(env);
    assert!(
        !env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::FlashLoanActive)
            .unwrap_or(false),
        "flash loan active"
    );
    ReceiptVault::update_interest(env.clone());
    let current = state(env);
    if recycle_only {
        assert!(
            current.pending_rewards > 0 && current.units > 0,
            "no backing rewards"
        );
    } else {
        assert_eq!(current.pending_rewards, 0, "unsettled backing rewards");
    }
    put(env, &current);
    let nav = fresh_nav(env);
    let supply = total_ptokens_supply(env);
    assert!(
        (supply == 0 && nav == 0) || (supply > 0 && nav > 0),
        "invalid receipt valuation"
    );
    FundingSnapshot {
        cash: token::Client::new(env, &token).balance(&env.current_contract_address()),
        token,
        managed_cash: env
            .storage()
            .persistent()
            .get(&DataKey::ManagedCash)
            .expect("managed cash missing"),
        deposited: env
            .storage()
            .persistent()
            .get(&DataKey::TotalDeposited)
            .expect("deposits missing"),
        initial_rate: env
            .storage()
            .persistent()
            .get(&DataKey::InitialExchangeRate)
            .expect("initial rate missing"),
        supply,
        nav,
        state: current,
        recycle_only,
    }
}

/// Credit exact newly received settlement cash, mint receipt backing at the
/// pre-receipt NAV, and reinvest with the existing configured idle buffer.
/// `recycled` is yield earned by the OLD reward backing. It raises old unit value
/// before new units are issued; it is not distributed to later reward recipients.
pub fn finish(
    env: &Env,
    snapshot: FundingSnapshot,
    received: u128,
    recycled: u128,
    min_units: u128,
) -> u128 {
    finish_mode(env, snapshot, received, recycled, min_units, true)
}

/// Credit backing atomically but keep converted cash managed and idle. A later
/// ordinary rebalance may deploy it. Separating LP entry from conversion avoids
/// exceeding the transaction footprint; claims are backed immediately either way.
pub fn finish_idle(
    env: &Env,
    snapshot: FundingSnapshot,
    received: u128,
    recycled: u128,
    min_units: u128,
) -> u128 {
    finish_mode(env, snapshot, received, recycled, min_units, false)
}

fn finish_mode(
    env: &Env,
    snapshot: FundingSnapshot,
    received: u128,
    recycled: u128,
    min_units: u128,
    deploy: bool,
) -> u128 {
    assert!(
        received > 0 && recycled <= received,
        "invalid reward receipt"
    );
    if snapshot.recycle_only {
        assert_eq!(
            received, recycled,
            "pending funding must only enrich old units"
        );
    }
    assert_eq!(state(env), snapshot.state, "backing changed during funding");
    assert_eq!(
        total_ptokens_supply(env),
        snapshot.supply,
        "receipt supply changed during funding"
    );
    assert_eq!(
        env.storage()
            .persistent()
            .get::<_, u128>(&DataKey::ManagedCash),
        Some(snapshot.managed_cash),
        "managed cash changed during funding"
    );
    assert_eq!(
        env.storage()
            .persistent()
            .get::<_, u128>(&DataKey::TotalDeposited),
        Some(snapshot.deposited),
        "tracked deposits changed during funding"
    );
    // Incoming untracked cash is excluded from NAV. A conversion that also
    // changes strategy value requires separate reconciliation, not stale pricing.
    assert_eq!(
        fresh_nav(env),
        snapshot.nav,
        "invested value changed during funding"
    );
    let cash_after =
        token::Client::new(env, &snapshot.token).balance(&env.current_contract_address());
    assert_eq!(
        cash_after.checked_sub(snapshot.cash),
        Some(amount(received)),
        "reward transfer mismatch"
    );
    let cap = env
        .storage()
        .persistent()
        .get::<_, u128>(&DataKey::SupplyCap)
        .unwrap_or(0);
    let after_nav = snapshot
        .nav
        .checked_add(received)
        .expect("reward nav overflow");
    assert!(cap == 0 || after_nav <= cap, "supply cap exceeded");
    let minted = if snapshot.supply == 0 {
        mul_div(env, received, SCALE_1E6, snapshot.initial_rate)
    } else {
        mul_div(env, received, snapshot.supply, snapshot.nav)
    };
    assert!(minted > 0, "reward receipt below share dust");
    let mut current = snapshot.state;
    assert!(
        recycled == 0 || current.units > 0,
        "no holders for recycled rewards"
    );
    // Round OLD backing's portion upward; new claims must not capture its dust.
    let new_ptokens = mul_div(env, minted, received - recycled, received);
    let old_ptokens = minted - new_ptokens;
    let prior_backing = current
        .ptokens
        .checked_add(old_ptokens)
        .expect("backing overflow");
    let units = if current.units == 0 {
        new_ptokens
    } else {
        mul_div(env, new_ptokens, current.units, prior_backing)
    };
    assert!(
        received == recycled || units > 0,
        "reward receipt below unit dust"
    );
    assert!(units >= min_units, "reward units below minimum");
    current.ptokens = current
        .ptokens
        .checked_add(minted)
        .expect("backing overflow");
    current.units = current
        .units
        .checked_add(units)
        .expect("reward units overflow");
    current.unallocated_units = current
        .unallocated_units
        .checked_add(units)
        .expect("reward units overflow");
    env.storage().persistent().set(
        &DataKey::ManagedCash,
        &snapshot
            .managed_cash
            .checked_add(received)
            .expect("cash overflow"),
    );
    env.storage().persistent().set(
        &DataKey::TotalDeposited,
        &snapshot
            .deposited
            .checked_add(received)
            .expect("deposits overflow"),
    );
    TokenBase::mint(env, &env.current_contract_address(), amount(minted));
    put(env, &current);
    // Do not deploy pre-existing untracked donations or reserved settlement cash.
    let managed = env
        .storage()
        .persistent()
        .get::<_, u128>(&DataKey::ManagedCash)
        .unwrap();
    if deploy {
        ReceiptVault::deposit_excess_idle_cash(
            env,
            &snapshot.token,
            managed.min(cash_after.try_into().expect("negative cash")),
        );
    }
    RewardBacked {
        underlying: received,
        recycled_underlying: recycled,
        ptokens: minted,
        units,
    }
    .publish(env);
    units
}

/// Convenience funding path for a separately funded settlement source. It must
/// NOT pull cash from the invested strategy: that cash is already in receipt NAV.
pub fn fund(env: &Env, source: &Address, received: u128, recycled: u128, min_units: u128) -> u128 {
    assert_ne!(
        *source,
        env.current_contract_address(),
        "cannot fund from self"
    );
    let boosted: Option<Address> = env.storage().persistent().get(&DataKey::BoostedVault);
    assert!(
        boosted.as_ref() != Some(source),
        "strategy cash already counted"
    );
    source.require_auth();
    let snapshot = begin(env);
    token::Client::new(env, &snapshot.token).transfer(
        source,
        env.current_contract_address(),
        &amount(received),
    );
    finish(env, snapshot, received, recycled, min_units)
}

/// Release only the owner's backed units and redeem through existing receipt
/// withdrawal checks. Parent coordinator MUST checkpoint reward weights first.
pub fn redeem(env: &Env, owner: &Address, units: u128, min_underlying: u128) -> u128 {
    // ReceiptVault::withdraw authenticates this owner below, in this same frame.
    // Requiring the same authorization twice in one frame is invalid on Soroban;
    // any missing authorization rolls back the preceding internal changes too.
    assert_ne!(
        *owner,
        env.current_contract_address(),
        "cannot redeem to backing"
    );
    assert!(units > 0, "zero reward redemption");
    let underlying = ensure_initialized(env);
    let mut current = state(env);
    assert_eq!(current.pending_rewards, 0, "unsettled backing rewards");
    ReceiptVault::update_interest(env.clone());
    fresh_nav(env);
    let remaining = owner_units(env, owner)
        .checked_sub(units)
        .expect("insufficient reward units");
    let shares = if units == current.units {
        current.ptokens
    } else {
        mul_div(env, units, current.ptokens, current.units)
    };
    assert!(shares > 0, "reward redemption below share dust");
    let cash_before = token::Client::new(env, &underlying).balance(owner);
    current.units = current.units.checked_sub(units).expect("units underflow");
    current.ptokens = current
        .ptokens
        .checked_sub(shares)
        .expect("backing underflow");
    set_owner(env, owner, remaining);
    TokenBase::update(
        env,
        Some(&env.current_contract_address()),
        Some(owner),
        amount(shares),
    );
    put(env, &current);
    // Native internal call, NOT an external callback into this contract.
    ReceiptVault::withdraw(env.clone(), owner.clone(), shares);
    let received = token::Client::new(env, &underlying)
        .balance(owner)
        .checked_sub(cash_before)
        .expect("negative reward payout");
    let received: u128 = received.try_into().expect("negative reward payout");
    assert!(received >= min_underlying, "reward payout below minimum");
    RewardPaid {
        owner: owner.clone(),
        units,
        ptokens: shares,
        underlying: received,
    }
    .publish(env);
    received
}
