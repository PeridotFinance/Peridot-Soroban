use soroban_sdk::{Address, BytesN, Env, Vec};

use crate::constants::*;
use crate::events::{PositionCreated, PositionRemoved};
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
    let position: Position = env
        .storage()
        .persistent()
        .get(&DataKey::Position(id))
        .expect("position missing");
    PositionCreated {
        owner: user.clone(),
        position_id: id,
        mode: get_position_mode_no_bump(env, id),
        status: position.status,
    }
    .publish(env);
}

pub fn remove_user_position(env: &Env, user: &Address, id: u64) {
    // Closing one position must not pull every other position's storage into the
    // transaction footprint. Stale IDs are filtered by read paths and by the
    // bounded compaction performed before adding a new position.
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
    PositionRemoved {
        owner: user.clone(),
        position_id: id,
        removed_at: env.ledger().timestamp(),
    }
    .publish(env);
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

pub fn read_user_positions(env: &Env, user: &Address) -> Vec<u64> {
    let positions: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::UserPositions(user.clone()))
        .unwrap_or(Vec::new(env));
    let mut out = Vec::new(env);
    for id in positions.iter() {
        if env.storage().persistent().has(&DataKey::Position(id)) {
            out.push_back(id);
        }
    }
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
    let key = DataKey::PositionMode(position_id);
    let mode: Option<PositionMode> = env.storage().persistent().get(&key);
    if mode.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    bump_position_record_ttl(env, position_id);
    mode.unwrap_or(PositionMode::Legacy)
}

pub fn get_position_mode_no_bump(env: &Env, position_id: u64) -> PositionMode {
    env.storage()
        .persistent()
        .get(&DataKey::PositionMode(position_id))
        .unwrap_or(PositionMode::Legacy)
}

pub fn get_position_vaults(env: &Env, position_id: u64, position: &Position) -> PositionVaults {
    let collateral_vault_key = DataKey::PositionCollateralVault(position_id);
    let debt_vault_key = DataKey::PositionDebtVault(position_id);
    let position_vault_key = DataKey::PositionPositionVault(position_id);
    let collateral_vault: Option<Address> = env.storage().persistent().get(&collateral_vault_key);
    let debt_vault: Option<Address> = env.storage().persistent().get(&debt_vault_key);
    let position_vault: Option<Address> = env.storage().persistent().get(&position_vault_key);

    let persistent = env.storage().persistent();
    if collateral_vault.is_some() {
        persistent.extend_ttl(&collateral_vault_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if debt_vault.is_some() {
        persistent.extend_ttl(&debt_vault_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if position_vault.is_some() {
        persistent.extend_ttl(&position_vault_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    // Backward compatibility for pre-snapshot positions created before FIND-064.
    let resolved = PositionVaults {
        collateral_vault: collateral_vault
            .unwrap_or_else(|| get_market(env, &position.collateral_asset)),
        debt_vault: debt_vault.unwrap_or_else(|| get_market(env, &position.debt_asset)),
        position_vault: position_vault
            .unwrap_or_else(|| get_market(env, &position.collateral_asset)),
    };
    bump_position_record_ttl(env, position_id);
    resolved
}

pub fn get_position_vaults_no_bump(
    env: &Env,
    position_id: u64,
    position: &Position,
) -> PositionVaults {
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

    PositionVaults {
        collateral_vault: collateral_vault
            .unwrap_or_else(|| get_market(env, &position.collateral_asset)),
        debt_vault: debt_vault.unwrap_or_else(|| get_market(env, &position.debt_asset)),
        position_vault: position_vault
            .unwrap_or_else(|| get_market(env, &position.collateral_asset)),
    }
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

pub fn clear_position_storage(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::Position(position_id));
    clear_pending_open_position(env, position_id);
    clear_pending_open_supplied_collateral(env, position_id);
    clear_pending_perps_open_position(env, position_id);
    clear_pending_perps_open_execution(env, position_id);
    clear_perps_position_data(env, position_id);
    clear_pending_liquidation(env, position_id);
    clear_position_initial_lock(env, position_id);
    clear_position_vaults(env, position_id);
    clear_position_mode(env, position_id);
}

/// Private helper that panics if position is missing (used internally by contract methods).
pub fn get_position_or_panic(env: &Env, position_id: u64) -> Position {
    bump_position_ttl(env, position_id);
    env.storage()
        .persistent()
        .get(&DataKey::Position(position_id))
        .expect("position missing")
}

pub fn get_position_record_or_panic(env: &Env, position_id: u64) -> Position {
    bump_position_record_ttl(env, position_id);
    env.storage()
        .persistent()
        .get(&DataKey::Position(position_id))
        .expect("position missing")
}

pub fn bump_position_record_ttl(env: &Env, position_id: u64) {
    let key = DataKey::Position(position_id);
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn set_pending_open_position(env: &Env, position_id: u64, pending: &PendingOpenPosition) {
    env.storage()
        .persistent()
        .set(&DataKey::PendingOpenPosition(position_id), pending);
    bump_position_ttl(env, position_id);
}

pub fn get_pending_open_position(env: &Env, position_id: u64) -> Option<PendingOpenPosition> {
    let pending = env
        .storage()
        .persistent()
        .get(&DataKey::PendingOpenPosition(position_id));
    if pending.is_some() {
        bump_position_ttl(env, position_id);
    }
    pending
}

pub fn get_pending_open_position_or_panic(env: &Env, position_id: u64) -> PendingOpenPosition {
    get_pending_open_position(env, position_id).expect("pending open missing")
}

pub fn set_pending_perps_open_position(
    env: &Env,
    position_id: u64,
    pending: &PendingPerpsOpenPosition,
) {
    env.storage()
        .persistent()
        .set(&DataKey::PendingPerpsOpenPosition(position_id), pending);
    bump_position_ttl(env, position_id);
}

pub fn get_pending_perps_open_position(
    env: &Env,
    position_id: u64,
) -> Option<PendingPerpsOpenPosition> {
    let pending = env
        .storage()
        .persistent()
        .get(&DataKey::PendingPerpsOpenPosition(position_id));
    if pending.is_some() {
        bump_position_ttl(env, position_id);
    }
    pending
}

pub fn get_pending_perps_open_position_or_panic(
    env: &Env,
    position_id: u64,
) -> PendingPerpsOpenPosition {
    get_pending_perps_open_position(env, position_id).expect("pending perps open missing")
}

pub fn clear_pending_perps_open_position(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PendingPerpsOpenPosition(position_id));
}

pub fn set_pending_perps_open_execution(
    env: &Env,
    position_id: u64,
    execution: &PendingPerpsOpenExecution,
) {
    env.storage()
        .persistent()
        .set(&DataKey::PendingPerpsOpenExecution(position_id), execution);
    bump_position_ttl(env, position_id);
}

pub fn get_pending_perps_open_execution(
    env: &Env,
    position_id: u64,
) -> Option<PendingPerpsOpenExecution> {
    let execution = env
        .storage()
        .persistent()
        .get(&DataKey::PendingPerpsOpenExecution(position_id));
    if execution.is_some() {
        bump_position_ttl(env, position_id);
    }
    execution
}

pub fn get_pending_perps_open_execution_or_panic(
    env: &Env,
    position_id: u64,
) -> PendingPerpsOpenExecution {
    get_pending_perps_open_execution(env, position_id).expect("pending perps execution missing")
}

pub fn clear_pending_perps_open_execution(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PendingPerpsOpenExecution(position_id));
}

pub fn set_pending_perps_close(env: &Env, position_id: u64, pending: &PendingPerpsClose) {
    env.storage()
        .persistent()
        .set(&DataKey::PendingPerpsClose(position_id), pending);
    bump_position_record_ttl(env, position_id);
    bump_pending_perps_close_ttl(env, position_id);
}

pub fn get_pending_perps_close(env: &Env, position_id: u64) -> Option<PendingPerpsClose> {
    let pending = env
        .storage()
        .persistent()
        .get(&DataKey::PendingPerpsClose(position_id));
    if pending.is_some() {
        bump_position_record_ttl(env, position_id);
        bump_pending_perps_close_ttl(env, position_id);
    }
    pending
}

pub fn get_pending_perps_close_or_panic(env: &Env, position_id: u64) -> PendingPerpsClose {
    get_pending_perps_close(env, position_id).expect("pending perps close missing")
}

pub fn clear_pending_perps_close(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PendingPerpsClose(position_id));
}

pub fn bump_pending_perps_close_ttl(env: &Env, position_id: u64) {
    let key = DataKey::PendingPerpsClose(position_id);
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn set_perps_position_data(env: &Env, position_id: u64, data: &PerpsPositionData) {
    env.storage()
        .persistent()
        .set(&DataKey::PerpsPositionData(position_id), data);
    bump_position_ttl(env, position_id);
}

pub fn get_perps_position_data(env: &Env, position_id: u64) -> Option<PerpsPositionData> {
    let key = DataKey::PerpsPositionData(position_id);
    let data = env.storage().persistent().get(&key);
    if data.is_some() {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        bump_position_record_ttl(env, position_id);
    }
    data
}

pub fn get_perps_position_data_or_panic(env: &Env, position_id: u64) -> PerpsPositionData {
    get_perps_position_data(env, position_id).expect("perps position missing")
}

pub fn clear_perps_position_data(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PerpsPositionData(position_id));
}

pub fn set_pending_liquidation(env: &Env, position_id: u64, pending: &PendingLiquidation) {
    env.storage()
        .persistent()
        .set(&DataKey::PendingLiquidation(position_id), pending);
    bump_position_record_ttl(env, position_id);
    bump_pending_liquidation_ttl(env, position_id);
}

pub fn get_pending_liquidation(env: &Env, position_id: u64) -> Option<PendingLiquidation> {
    let pending = env
        .storage()
        .persistent()
        .get(&DataKey::PendingLiquidation(position_id));
    if pending.is_some() {
        bump_position_record_ttl(env, position_id);
        bump_pending_liquidation_ttl(env, position_id);
    }
    pending
}

pub fn get_pending_liquidation_or_panic(env: &Env, position_id: u64) -> PendingLiquidation {
    get_pending_liquidation(env, position_id).expect("pending liquidation missing")
}

pub fn clear_pending_liquidation(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PendingLiquidation(position_id));
}

pub fn set_perps_pair_config(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
    config: &PerpsPairConfig,
) {
    env.storage().persistent().set(
        &DataKey::PerpsPairConfig(margin_asset.clone(), base_asset.clone(), side.clone()),
        config,
    );
    bump_perps_pair_config_ttl(env, margin_asset, base_asset, side);
}

pub fn get_perps_pair_config(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
) -> Option<PerpsPairConfig> {
    let config = env.storage().persistent().get(&DataKey::PerpsPairConfig(
        margin_asset.clone(),
        base_asset.clone(),
        side.clone(),
    ));
    if config.is_some() {
        bump_perps_pair_config_ttl(env, margin_asset, base_asset, side);
    }
    config
}

pub fn get_perps_pair_config_or_default(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
) -> PerpsPairConfig {
    get_perps_pair_config(env, margin_asset, base_asset, side).unwrap_or(PerpsPairConfig {
        max_leverage: MAX_LEVERAGE_CAP,
        maintenance_margin_scaled: DEFAULT_PERPS_MAINTENANCE_MARGIN_SCALED,
        liquidation_incentive_scaled: DEFAULT_PERPS_LIQUIDATION_INCENTIVE_SCALED,
    })
}

pub fn clear_pending_open_position(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PendingOpenPosition(position_id));
}

pub fn set_pending_open_supplied_collateral(
    env: &Env,
    position_id: u64,
    ptokens: u128,
    position_amount: u128,
) {
    env.storage()
        .persistent()
        .set(&DataKey::PendingOpenSuppliedPtokens(position_id), &ptokens);
    env.storage().persistent().set(
        &DataKey::PendingOpenSuppliedAmount(position_id),
        &position_amount,
    );
    bump_position_ttl(env, position_id);
}

pub fn get_pending_open_supplied_collateral(env: &Env, position_id: u64) -> Option<(u128, u128)> {
    let ptokens: Option<u128> = env
        .storage()
        .persistent()
        .get(&DataKey::PendingOpenSuppliedPtokens(position_id));
    let Some(ptokens) = ptokens else {
        return None;
    };
    let position_amount: u128 = env
        .storage()
        .persistent()
        .get(&DataKey::PendingOpenSuppliedAmount(position_id))
        .unwrap_or(0u128);
    bump_position_ttl(env, position_id);
    Some((ptokens, position_amount))
}

pub fn get_pending_open_supplied_collateral_or_panic(env: &Env, position_id: u64) -> (u128, u128) {
    get_pending_open_supplied_collateral(env, position_id).expect("pending collateral missing")
}

pub fn clear_pending_open_supplied_collateral(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PendingOpenSuppliedPtokens(position_id));
    env.storage()
        .persistent()
        .remove(&DataKey::PendingOpenSuppliedAmount(position_id));
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
    if first_path.len() != 2 {
        panic!("bad swaps");
    }

    let (last_path, _, _) = swaps_chain.get(swaps_chain.len() - 1).unwrap();
    if last_path.len() != 2 {
        panic!("bad swaps");
    }

    let adapter = SwapAdapterClient::new(env, swap_adapter);
    let mut current = expected_in.clone();
    for i in 0..swaps_chain.len() {
        let (path, pool_id, pool) = swaps_chain.get(i).unwrap();
        if path.len() != 2 {
            panic!("bad swaps");
        }
        if pool_id.to_array() == [0u8; 32] {
            panic!("bad swaps");
        }
        if !adapter.is_pool_binding_allowed(&pool_id, &pool) {
            panic!("pool binding not allowed");
        }
        current = infer_two_token_hop_output(&path, &current);
    }
    if current != *expected_out {
        panic!("bad swaps");
    }
}

fn infer_two_token_hop_output(pool_tokens: &Vec<Address>, current_token: &Address) -> Address {
    let token_0 = pool_tokens.get(0).unwrap();
    let token_1 = pool_tokens.get(1).unwrap();
    if token_0 == current_token.clone() {
        token_1
    } else if token_1 == current_token.clone() {
        token_0
    } else {
        panic!("bad swaps");
    }
}

fn bump_total_margin_ptokens_ttl(env: &Env, vault: &Address) {
    let key = DataKey::TotalMarginPtokens(vault.clone());
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn get_total_margin_ptokens(env: &Env, vault: &Address) -> u128 {
    let key = DataKey::TotalMarginPtokens(vault.clone());
    let persistent = env.storage().persistent();
    bump_total_margin_ptokens_ttl(env, vault);
    persistent.get(&key).unwrap_or(0u128)
}

pub fn update_total_margin_ptokens(env: &Env, vault: &Address, amount: u128, add: bool) {
    if amount == 0 {
        return;
    }
    let key = DataKey::TotalMarginPtokens(vault.clone());
    let current = get_total_margin_ptokens(env, vault);
    let new_val = if add {
        current.checked_add(amount).expect("margin total overflow")
    } else {
        current.checked_sub(amount).expect("margin total underflow")
    };
    env.storage().persistent().set(&key, &new_val);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn get_margin_fee_index(env: &Env, vault: &Address) -> u128 {
    let key = DataKey::MarginFeeIndex(vault.clone());
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    persistent.get(&key).unwrap_or(0u128)
}

pub fn get_user_margin_fee_index(env: &Env, user: &Address, vault: &Address) -> u128 {
    let key = DataKey::UserMarginFeeIndex(user.clone(), vault.clone());
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    persistent.get(&key).unwrap_or(0u128)
}

pub fn compute_margin_fee_pending(delta: u128, user_bal: u128) -> u128 {
    if delta == 0 || user_bal == 0 {
        return 0;
    }
    delta.checked_mul(user_bal).expect("margin fee overflow") / MARGIN_FEE_PRECISION
}

/// Snapshot pending fee earnings into `UserMarginFeeAccrued`, then advance
/// the user's fee-index snapshot to the current global index.
/// Must be called BEFORE any `MarginBalancePtokens` change for `(user, vault)`.
pub fn accrue_user_fee(env: &Env, user: &Address, vault: &Address) {
    let fee_index = get_margin_fee_index(env, vault);
    let user_index = get_user_margin_fee_index(env, user, vault);
    let delta = fee_index.saturating_sub(user_index);
    if delta > 0 {
        let user_bal = get_margin_balance_ptokens(env, user, vault);
        if user_bal > 0 {
            let pending = compute_margin_fee_pending(delta, user_bal);
            if pending > 0 {
                let accrued_key = DataKey::UserMarginFeeAccrued(user.clone(), vault.clone());
                let accrued: u128 = env.storage().persistent().get(&accrued_key).unwrap_or(0);
                let new_accrued = accrued.checked_add(pending).expect("margin fee overflow");
                env.storage().persistent().set(&accrued_key, &new_accrued);
                env.storage()
                    .persistent()
                    .extend_ttl(&accrued_key, TTL_THRESHOLD, TTL_EXTEND_TO);
            }
        }
    }
    let user_index_key = DataKey::UserMarginFeeIndex(user.clone(), vault.clone());
    env.storage().persistent().set(&user_index_key, &fee_index);
    env.storage()
        .persistent()
        .extend_ttl(&user_index_key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Distribute `fee_ptokens` to the current LP pool of `vault`.
/// If the pool is empty, accumulates to `MarginFeeOrphan` for admin sweep.
pub fn collect_margin_fee(env: &Env, vault: &Address, fee_ptokens: u128) {
    if fee_ptokens == 0 {
        return;
    }
    let total = get_total_margin_ptokens(env, vault);
    if total == 0 {
        let orphan_key = DataKey::MarginFeeOrphan(vault.clone());
        let orphan: u128 = env.storage().persistent().get(&orphan_key).unwrap_or(0);
        let new_orphan = orphan
            .checked_add(fee_ptokens)
            .expect("margin fee overflow");
        env.storage().persistent().set(&orphan_key, &new_orphan);
        env.storage()
            .persistent()
            .extend_ttl(&orphan_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        return;
    }
    // Carry forward the sub-unit remainder from prior calls so that fees too small to
    // move the index at the current pool size are not silently dropped (they stay
    // deducted from user margin balances, so dropping them would strand pTokens in the
    // controller and short LP fee revenue). The remainder accumulates in 1e18-scaled
    // numerator units until it is large enough to bump the index by >= 1.
    let remainder_key = DataKey::MarginFeeRemainder(vault.clone());
    let remainder: u128 = env.storage().persistent().get(&remainder_key).unwrap_or(0);
    // Fail closed on overflow rather than silently dropping the carried remainder, matching the
    // checked-arithmetic posture used across open/health/liquidation valuation. A position large
    // enough to overflow this product is expected to revert earlier at open valuation, so this
    // panic is unlikely to be reachable in practice; we fail closed regardless.
    let numerator = fee_ptokens
        .checked_mul(MARGIN_FEE_PRECISION)
        .and_then(|v| v.checked_add(remainder))
        .expect("margin fee overflow");
    let delta = numerator / total;
    let new_remainder = numerator % total;
    env.storage()
        .persistent()
        .set(&remainder_key, &new_remainder);
    env.storage()
        .persistent()
        .extend_ttl(&remainder_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    if delta == 0 {
        return;
    }
    let index_key = DataKey::MarginFeeIndex(vault.clone());
    let current: u128 = env.storage().persistent().get(&index_key).unwrap_or(0);
    let new_index = current.checked_add(delta).expect("margin fee overflow");
    env.storage().persistent().set(&index_key, &new_index);
    env.storage()
        .persistent()
        .extend_ttl(&index_key, TTL_THRESHOLD, TTL_EXTEND_TO);
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
    if persistent.has(&DataKey::MaxSlippageScaled) {
        persistent.extend_ttl(&DataKey::MaxSlippageScaled, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PositionCounter) {
        persistent.extend_ttl(&DataKey::PositionCounter, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Initialized) {
        persistent.extend_ttl(&DataKey::Initialized, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::OpenFeeBps) {
        persistent.extend_ttl(&DataKey::OpenFeeBps, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::CloseFeeBps) {
        persistent.extend_ttl(&DataKey::CloseFeeBps, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PendingPeridottroller) {
        persistent.extend_ttl(
            &DataKey::PendingPeridottroller,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }
    if persistent.has(&DataKey::PendingPeridottrollerEta) {
        persistent.extend_ttl(
            &DataKey::PendingPeridottrollerEta,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }
    if persistent.has(&DataKey::PendingSwapAdapter) {
        persistent.extend_ttl(&DataKey::PendingSwapAdapter, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PendingSwapAdapterEta) {
        persistent.extend_ttl(
            &DataKey::PendingSwapAdapterEta,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }
}

pub fn bump_pending_admin_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::PendingAdmin) {
        persistent.extend_ttl(&DataKey::PendingAdmin, TTL_THRESHOLD, TTL_EXTEND_TO);
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

pub fn bump_perps_pair_config_ttl(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
) {
    let key = DataKey::PerpsPairConfig(margin_asset.clone(), base_asset.clone(), side.clone());
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
    let pending_key = DataKey::PendingOpenPosition(position_id);
    if persistent.has(&pending_key) {
        persistent.extend_ttl(&pending_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let pending_perps_key = DataKey::PendingPerpsOpenPosition(position_id);
    if persistent.has(&pending_perps_key) {
        persistent.extend_ttl(&pending_perps_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let pending_perps_execution_key = DataKey::PendingPerpsOpenExecution(position_id);
    if persistent.has(&pending_perps_execution_key) {
        persistent.extend_ttl(&pending_perps_execution_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let perps_position_key = DataKey::PerpsPositionData(position_id);
    if persistent.has(&perps_position_key) {
        persistent.extend_ttl(&perps_position_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let pending_supplied_ptokens_key = DataKey::PendingOpenSuppliedPtokens(position_id);
    if persistent.has(&pending_supplied_ptokens_key) {
        persistent.extend_ttl(&pending_supplied_ptokens_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let pending_supplied_amount_key = DataKey::PendingOpenSuppliedAmount(position_id);
    if persistent.has(&pending_supplied_amount_key) {
        persistent.extend_ttl(&pending_supplied_amount_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let pending_liquidation_key = DataKey::PendingLiquidation(position_id);
    if persistent.has(&pending_liquidation_key) {
        persistent.extend_ttl(&pending_liquidation_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_pending_liquidation_ttl(env: &Env, position_id: u64) {
    let key = DataKey::PendingLiquidation(position_id);
    let persistent = env.storage().persistent();
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
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

pub fn get_position_initial_lock_no_bump(env: &Env, position_id: u64) -> Option<(Address, u128)> {
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
        bump_total_margin_ptokens_ttl(env, market);
    }
    persistent.get(&key).unwrap_or(0u128)
}

pub fn set_margin_balance_ptokens(env: &Env, user: &Address, market: &Address, value: u128) {
    let key = DataKey::MarginBalancePtokens(user.clone(), market.clone());
    env.storage().persistent().set(&key, &value);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    bump_total_margin_ptokens_ttl(env, market);
}
