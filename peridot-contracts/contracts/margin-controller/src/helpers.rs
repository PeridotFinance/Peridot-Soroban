use soroban_sdk::{Address, BytesN, Env, Vec};

use crate::constants::*;
use crate::storage::*;

pub fn next_position_id(env: &Env) -> u64 {
    let mut id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::PositionCounter)
        .unwrap_or(0u64);
    if id == u64::MAX {
        panic!("position id overflow");
    }
    id = id.saturating_add(1);
    env.storage()
        .persistent()
        .set(&DataKey::PositionCounter, &id);
    id
}

pub fn push_user_position(env: &Env, user: &Address, id: u64) {
    let mut positions: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::UserPositions(user.clone()))
        .unwrap_or(Vec::new(env));
    if positions.len() >= MAX_USER_POSITIONS {
        panic!("too many positions");
    }
    positions.push_back(id);
    env.storage()
        .persistent()
        .set(&DataKey::UserPositions(user.clone()), &positions);
    bump_user_positions_ttl(env, user);
}

pub fn remove_user_position(env: &Env, user: &Address, id: u64) {
    let positions: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::UserPositions(user.clone()))
        .unwrap_or(Vec::new(env));
    let mut out = Vec::new(env);
    for p in positions.iter() {
        if p != id {
            out.push_back(p);
        }
    }
    env.storage()
        .persistent()
        .set(&DataKey::UserPositions(user.clone()), &out);
    bump_user_positions_ttl(env, user);
}

pub fn get_debt_shares_total(env: &Env, user: &Address, debt_asset: &Address) -> u128 {
    bump_debt_shares_ttl(env, user, debt_asset);
    env.storage()
        .persistent()
        .get(&DataKey::DebtSharesTotal(
            user.clone(),
            debt_asset.clone(),
        ))
        .unwrap_or(0u128)
}

pub fn set_debt_shares_total(env: &Env, user: &Address, debt_asset: &Address, value: u128) {
    env.storage().persistent().set(
        &DataKey::DebtSharesTotal(user.clone(), debt_asset.clone()),
        &value,
    );
    bump_debt_shares_ttl(env, user, debt_asset);
}

pub fn debt_for_shares(
    env: &Env,
    user: &Address,
    debt_asset: &Address,
    shares: u128,
) -> (u128, u128, u128) {
    let total_shares = get_debt_shares_total(env, user, debt_asset);
    if total_shares == 0 || shares == 0 {
        return (0, total_shares, 0);
    }
    let debt_vault = get_market(env, debt_asset);
    let total_debt = ReceiptVaultClient::new(env, &debt_vault).get_user_borrow_balance(user);
    let debt_amount = shares.saturating_mul(total_debt) / total_shares;
    (debt_amount, total_shares, total_debt)
}

/// Private helper that panics if position is missing (used internally by contract methods).
pub fn get_position_or_panic(env: &Env, position_id: u64) -> Position {
    bump_position_ttl(env, position_id);
    env.storage()
        .persistent()
        .get(&DataKey::Position(position_id))
        .expect("position missing")
}

pub fn validate_swaps_chain(swaps_chain: &Vec<(Vec<Address>, BytesN<32>, Address)>) {
    if swaps_chain.len() == 0 || swaps_chain.len() > MAX_SWAP_PATH_LEN {
        panic!("bad swaps");
    }
    for i in 0..swaps_chain.len() {
        let (path, _, _) = swaps_chain.get(i).unwrap();
        if path.len() == 0 || path.len() > MAX_SWAP_PATH_LEN {
            panic!("bad swaps");
        }
    }
}

pub fn bump_core_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::Admin) {
        persistent.extend_ttl(&DataKey::Admin, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Peridottroller) {
        persistent.extend_ttl(&DataKey::Peridottroller, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::SwapAdapter) {
        persistent.extend_ttl(&DataKey::SwapAdapter, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::MaxLeverage) {
        persistent.extend_ttl(&DataKey::MaxLeverage, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::LiquidationBonus) {
        persistent.extend_ttl(&DataKey::LiquidationBonus, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PositionCounter) {
        persistent.extend_ttl(&DataKey::PositionCounter, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if env.storage().instance().has(&DataKey::Initialized) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_market_ttl(env: &Env, asset: &Address) {
    let key = DataKey::Market(asset.clone());
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_position_ttl(env: &Env, position_id: u64) {
    let key = DataKey::Position(position_id);
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_user_positions_ttl(env: &Env, user: &Address) {
    let key = DataKey::UserPositions(user.clone());
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_debt_shares_ttl(env: &Env, user: &Address, debt_asset: &Address) {
    let key = DataKey::DebtSharesTotal(user.clone(), debt_asset.clone());
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}
