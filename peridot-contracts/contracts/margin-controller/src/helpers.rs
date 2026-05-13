use soroban_sdk::{Address, BytesN, Env, Vec};

use crate::constants::*;
use crate::storage::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionVaults {
    pub collateral_vault: Address,
    pub debt_vault: Address,
    pub position_vault: Address,
}

pub fn next_position_id(env: &Env) -> u64 {
    let mut id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::PositionCounter)
        .expect("position counter missing");
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
    let mut positions = compact_user_positions(env, user);
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
    let positions = compact_user_positions(env, user);
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

pub fn compact_user_positions(env: &Env, user: &Address) -> Vec<u64> {
    let positions: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::UserPositions(user.clone()))
        .unwrap_or(Vec::new(env));
    let mut out = Vec::new(env);
    let mut changed = false;
    for id in positions.iter() {
        let key = DataKey::Position(id);
        if env.storage().persistent().has(&key) {
            bump_position_ttl(env, id);
            out.push_back(id);
        } else {
            changed = true;
        }
    }
    if changed {
        env.storage()
            .persistent()
            .set(&DataKey::UserPositions(user.clone()), &out);
    }
    bump_user_positions_ttl(env, user);
    out
}

pub fn get_debt_shares_total(env: &Env, user: &Address, debt_asset: &Address) -> u128 {
    let key = DataKey::DebtSharesTotal(user.clone(), debt_asset.clone());
    if let Some(total) = env.storage().persistent().get::<_, u128>(&key) {
        bump_debt_shares_ttl(env, user, debt_asset);
        return total;
    }
    let recovered = recover_debt_shares_total_from_positions(env, user, debt_asset);
    if recovered > 0 {
        env.storage().persistent().set(&key, &recovered);
        bump_debt_shares_ttl(env, user, debt_asset);
    }
    recovered
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
    let debt_vault = get_market(env, debt_asset);
    debt_for_shares_in_vault(env, user, debt_asset, &debt_vault, shares)
}

pub fn debt_for_shares_in_vault(
    env: &Env,
    user: &Address,
    debt_asset: &Address,
    debt_vault: &Address,
    shares: u128,
) -> (u128, u128, u128) {
    let total_shares = get_debt_shares_total(env, user, debt_asset);
    if total_shares == 0 || shares == 0 {
        return (0, total_shares, 0);
    }
    let total_debt = ReceiptVaultClient::new(env, debt_vault).get_user_borrow_balance(user);
    let debt_amount = if shares >= total_shares {
        total_debt
    } else {
        let numerator = shares
            .checked_mul(total_debt)
            .expect("debt calculation overflow");
        // Round up so share burn repays enough underlying for the shares removed.
        numerator
            .checked_add(total_shares - 1)
            .expect("debt calculation overflow")
            / total_shares
    };
    (debt_amount, total_shares, total_debt)
}

fn recover_debt_shares_total_from_positions(
    env: &Env,
    user: &Address,
    debt_asset: &Address,
) -> u128 {
    let mut total = 0u128;
    let positions = compact_user_positions(env, user);
    for id in positions.iter() {
        let position: Option<Position> = env.storage().persistent().get(&DataKey::Position(id));
        let Some(position) = position else {
            continue;
        };
        if position.status == PositionStatus::Open
            && position.debt_asset == *debt_asset
            && position.debt_shares > 0
        {
            total = total.saturating_add(position.debt_shares);
        }
    }
    total
}

pub fn set_position_vaults(
    env: &Env,
    position_id: u64,
    collateral_vault: &Address,
    debt_vault: &Address,
    position_vault: &Address,
) {
    env.storage().persistent().set(
        &DataKey::PositionCollateralVault(position_id),
        collateral_vault,
    );
    env.storage()
        .persistent()
        .set(&DataKey::PositionDebtVault(position_id), debt_vault);
    env.storage()
        .persistent()
        .set(&DataKey::PositionPositionVault(position_id), position_vault);
    bump_position_ttl(env, position_id);
}

pub fn set_position_mode(env: &Env, position_id: u64, mode: PositionMode) {
    env.storage()
        .persistent()
        .set(&DataKey::PositionMode(position_id), &mode);
    bump_position_ttl(env, position_id);
}

pub fn get_position_mode(env: &Env, position_id: u64) -> PositionMode {
    let mode: Option<PositionMode> = env
        .storage()
        .persistent()
        .get(&DataKey::PositionMode(position_id));
    bump_position_ttl(env, position_id);
    mode.unwrap_or(PositionMode::Legacy)
}

pub fn get_position_vaults(env: &Env, position_id: u64, position: &Position) -> PositionVaults {
    let collateral_vault: Option<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::PositionCollateralVault(position_id));
    let debt_vault: Option<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::PositionDebtVault(position_id));
    let position_vault: Option<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::PositionPositionVault(position_id));

    // Backward compatibility for pre-snapshot positions created before FIND-064.
    let resolved = PositionVaults {
        collateral_vault: collateral_vault
            .unwrap_or_else(|| get_market(env, &position.collateral_asset)),
        debt_vault: debt_vault.unwrap_or_else(|| get_market(env, &position.debt_asset)),
        position_vault: position_vault
            .unwrap_or_else(|| get_market(env, &position.collateral_asset)),
    };
    bump_position_ttl(env, position_id);
    resolved
}

pub fn clear_position_vaults(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PositionCollateralVault(position_id));
    env.storage()
        .persistent()
        .remove(&DataKey::PositionDebtVault(position_id));
    env.storage()
        .persistent()
        .remove(&DataKey::PositionPositionVault(position_id));
}

pub fn clear_position_mode(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PositionMode(position_id));
}

/// Private helper that panics if position is missing (used internally by contract methods).
pub fn get_position_or_panic(env: &Env, position_id: u64) -> Position {
    bump_position_ttl(env, position_id);
    env.storage()
        .persistent()
        .get(&DataKey::Position(position_id))
        .expect("position missing")
}

pub fn validate_swaps_chain(
    env: &Env,
    swap_adapter: &Address,
    swaps_chain: &Vec<(Vec<Address>, BytesN<32>, Address)>,
    expected_in: &Address,
    expected_out: &Address,
) {
    if swaps_chain.len() == 0 || swaps_chain.len() > MAX_SWAP_PATH_LEN {
        panic!("bad swaps");
    }
    let (first_path, _, _) = swaps_chain.get(0).unwrap();
    if first_path.len() < 2 || first_path.len() > MAX_SWAP_PATH_LEN {
        panic!("bad swaps");
    }
    if first_path.get(0).unwrap() != *expected_in {
        panic!("bad swaps");
    }

    let (last_path, _, _) = swaps_chain.get(swaps_chain.len() - 1).unwrap();
    if last_path.len() < 2 || last_path.len() > MAX_SWAP_PATH_LEN {
        panic!("bad swaps");
    }
    if last_path.get(last_path.len() - 1).unwrap() != *expected_out {
        panic!("bad swaps");
    }

    let adapter = SwapAdapterClient::new(env, swap_adapter);
    let mut current = expected_in.clone();
    for i in 0..swaps_chain.len() {
        let (path, pool_id, pool) = swaps_chain.get(i).unwrap();
        if path.len() < 2 || path.len() > MAX_SWAP_PATH_LEN {
            panic!("bad swaps");
        }
        if pool_id.to_array() == [0u8; 32] {
            panic!("bad swaps");
        }
        if !adapter.is_pool_allowed(&pool) {
            panic!("pool not allowed");
        }
        let hop_in = path.get(0).unwrap();
        if hop_in != current {
            panic!("bad swaps");
        }
        current = path.get(path.len() - 1).unwrap();
    }
    if current != *expected_out {
        panic!("bad swaps");
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
    if persistent.has(&DataKey::MaxSlippageBps) {
        persistent.extend_ttl(&DataKey::MaxSlippageBps, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PositionCounter) {
        persistent.extend_ttl(&DataKey::PositionCounter, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Initialized) {
        persistent.extend_ttl(&DataKey::Initialized, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_pending_upgrade_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::PendingUpgradeHash) {
        persistent.extend_ttl(&DataKey::PendingUpgradeHash, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PendingUpgradeEta) {
        persistent.extend_ttl(&DataKey::PendingUpgradeEta, TTL_THRESHOLD, TTL_EXTEND_TO);
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
    let persistent = env.storage().persistent();
    let key = DataKey::Position(position_id);
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let initial_market_key = DataKey::PositionInitialLockMarket(position_id);
    if persistent.has(&initial_market_key) {
        persistent.extend_ttl(&initial_market_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let initial_ptokens_key = DataKey::PositionInitialLockPtokens(position_id);
    if persistent.has(&initial_ptokens_key) {
        persistent.extend_ttl(&initial_ptokens_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let collateral_vault_key = DataKey::PositionCollateralVault(position_id);
    if persistent.has(&collateral_vault_key) {
        persistent.extend_ttl(&collateral_vault_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let debt_vault_key = DataKey::PositionDebtVault(position_id);
    if persistent.has(&debt_vault_key) {
        persistent.extend_ttl(&debt_vault_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let position_vault_key = DataKey::PositionPositionVault(position_id);
    if persistent.has(&position_vault_key) {
        persistent.extend_ttl(&position_vault_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let mode_key = DataKey::PositionMode(position_id);
    if persistent.has(&mode_key) {
        persistent.extend_ttl(&mode_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn set_position_initial_lock(
    env: &Env,
    position_id: u64,
    market: &Address,
    ptoken_amount: u128,
) {
    env.storage()
        .persistent()
        .set(&DataKey::PositionInitialLockMarket(position_id), market);
    env.storage().persistent().set(
        &DataKey::PositionInitialLockPtokens(position_id),
        &ptoken_amount,
    );
    bump_position_ttl(env, position_id);
}

pub fn get_position_initial_lock(env: &Env, position_id: u64) -> Option<(Address, u128)> {
    let market: Option<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::PositionInitialLockMarket(position_id));
    let Some(market) = market else {
        return None;
    };
    let ptoken_amount: u128 = env
        .storage()
        .persistent()
        .get(&DataKey::PositionInitialLockPtokens(position_id))
        .unwrap_or(0u128);
    bump_position_ttl(env, position_id);
    Some((market, ptoken_amount))
}

pub fn clear_position_initial_lock(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PositionInitialLockMarket(position_id));
    env.storage()
        .persistent()
        .remove(&DataKey::PositionInitialLockPtokens(position_id));
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

pub fn get_margin_balance_ptokens(env: &Env, user: &Address, market: &Address) -> u128 {
    let key = DataKey::MarginBalancePtokens(user.clone(), market.clone());
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    persistent.get(&key).unwrap_or(0u128)
}

pub fn set_margin_balance_ptokens(env: &Env, user: &Address, market: &Address, value: u128) {
    let key = DataKey::MarginBalancePtokens(user.clone(), market.clone());
    env.storage().persistent().set(&key, &value);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}
