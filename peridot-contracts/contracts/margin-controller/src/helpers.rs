use soroban_sdk::{Address, Env, Vec};

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
    // Position removal keeps this list current during normal operation. Avoid
    // loading and TTL-bumping every sibling position on each new open.
    let mut positions: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::UserPositions(user.clone()))
        .unwrap_or(Vec::new(env));
    if positions.len() >= MAX_USER_POSITIONS {
        panic!("compact positions first");
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
    // transaction footprint. Stale IDs are filtered by read paths and can be
    // removed explicitly through the bounded compaction entrypoint.
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

pub fn compact_user_positions_bounded(env: &Env, user: &Address, limit: u32) -> u32 {
    if limit == 0 || limit > MAX_POSITION_COMPACTION_BATCH {
        panic!("bad compaction limit");
    }
    let key = DataKey::UserPositions(user.clone());
    let cursor_key = DataKey::UserPositionsCompactionCursor(user.clone());
    let persistent = env.storage().persistent();
    let positions: Vec<u64> = persistent.get(&key).unwrap_or(Vec::new(env));
    let len = positions.len();
    if len == 0 {
        persistent.remove(&cursor_key);
        return 0;
    }

    let mut cursor: u32 = persistent.get(&cursor_key).unwrap_or(0u32) % len;
    let scan = limit.min(len);
    let mut stale = Vec::new(env);
    for offset in 0..scan {
        let idx = cursor.saturating_add(offset) % len;
        let id = positions.get(idx).unwrap();
        if !persistent.has(&DataKey::Position(id)) {
            stale.push_back(id);
        }
    }

    let mut out = Vec::new(env);
    for id in positions.iter() {
        let mut remove = false;
        for stale_id in stale.iter() {
            if stale_id == id {
                remove = true;
                break;
            }
        }
        if !remove {
            out.push_back(id);
        }
    }
    if !stale.is_empty() {
        persistent.set(&key, &out);
    }

    if out.is_empty() {
        persistent.remove(&cursor_key);
    } else {
        cursor = if !stale.is_empty() {
            cursor.min(out.len().saturating_sub(1))
        } else {
            cursor.saturating_add(scan) % out.len()
        };
        persistent.set(&cursor_key, &cursor);
        persistent.extend_ttl(&cursor_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    bump_user_positions_ttl(env, user);
    out.len()
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
    let persistent = env.storage().persistent();
    let mode: Option<PositionMode> = persistent.get(&key);
    if let Some(mode) = mode {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        bump_position_record_ttl(env, position_id);
        return mode;
    }

    // PerpsPositionData is only created by V3. It is therefore sufficient to
    // reconstruct a mode key that was independently archived or restored.
    if persistent.has(&DataKey::PerpsPositionData(position_id)) {
        persistent.set(&key, &PositionMode::PerpsV3);
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        bump_position_record_ttl(env, position_id);
        return PositionMode::PerpsV3;
    }

    bump_position_record_ttl(env, position_id);
    PositionMode::Legacy
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

pub fn get_position_risk_vaults(
    env: &Env,
    position_id: u64,
    position: &Position,
) -> (Address, Address) {
    let debt_vault_key = DataKey::PositionDebtVault(position_id);
    let position_vault_key = DataKey::PositionPositionVault(position_id);
    let persistent = env.storage().persistent();
    let debt_vault: Option<Address> = persistent.get(&debt_vault_key);
    let position_vault: Option<Address> = persistent.get(&position_vault_key);

    if debt_vault.is_some() {
        persistent.extend_ttl(&debt_vault_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if position_vault.is_some() {
        persistent.extend_ttl(&position_vault_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    bump_position_record_ttl(env, position_id);

    (
        debt_vault.unwrap_or_else(|| get_market(env, &position.debt_asset)),
        position_vault.unwrap_or_else(|| get_market(env, &position.collateral_asset)),
    )
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

pub fn clear_perps_v3_position_storage(env: &Env, position_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::Position(position_id));
    clear_pending_perps_open_position(env, position_id);
    clear_pending_perps_open_execution(env, position_id);
    clear_perps_position_data(env, position_id);
    clear_pending_perps_close(env, position_id);
    clear_pending_liquidation(env, position_id);
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

pub fn set_pending_perps_close_remainder(env: &Env, position_id: u64, amount: u128) {
    let key = DataKey::PendingPerpsCloseRemainder(position_id);
    if amount == 0 {
        env.storage().persistent().remove(&key);
        return;
    }
    env.storage().persistent().set(&key, &amount);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn get_pending_perps_close_remainder(env: &Env, position_id: u64) -> u128 {
    let key = DataKey::PendingPerpsCloseRemainder(position_id);
    let persistent = env.storage().persistent();
    let amount = persistent.get(&key).unwrap_or(0u128);
    if amount > 0 {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    amount
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
    env.storage()
        .persistent()
        .remove(&DataKey::PendingPerpsCloseRemainder(position_id));
}

pub fn bump_pending_perps_close_ttl(env: &Env, position_id: u64) {
    let persistent = env.storage().persistent();
    let pending_key = DataKey::PendingPerpsClose(position_id);
    if persistent.has(&pending_key) {
        persistent.extend_ttl(&pending_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let remainder_key = DataKey::PendingPerpsCloseRemainder(position_id);
    if persistent.has(&remainder_key) {
        persistent.extend_ttl(&remainder_key, TTL_THRESHOLD, TTL_EXTEND_TO);
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
    let persistent = env.storage().persistent();
    let data = persistent.get(&key);
    if data.is_some() {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        let mode_key = DataKey::PositionMode(position_id);
        if persistent.has(&mode_key) {
            persistent.extend_ttl(&mode_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
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
    let persistent = env.storage().persistent();
    let pending_key = DataKey::PendingLiquidation(position_id);
    let had_pending = persistent.has(&pending_key);
    persistent.remove(&pending_key);
    if had_pending {
        persistent.remove(&DataKey::PendingLiquidationTakeoverAfter(position_id));
        persistent.remove(&DataKey::PendingLiqCollateralUnderlying(position_id));
    }
}

pub fn set_pending_liquidation_takeover_after(env: &Env, position_id: u64, deadline: u64) {
    let key = DataKey::PendingLiquidationTakeoverAfter(position_id);
    env.storage().persistent().set(&key, &deadline);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn get_pending_liquidation_takeover_after(env: &Env, position_id: u64) -> Option<u64> {
    let key = DataKey::PendingLiquidationTakeoverAfter(position_id);
    let persistent = env.storage().persistent();
    let deadline = persistent.get(&key);
    if deadline.is_some() {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    deadline
}

pub fn set_pending_liquidation_collateral_underlying(env: &Env, position_id: u64, amount: u128) {
    let key = DataKey::PendingLiqCollateralUnderlying(position_id);
    let persistent = env.storage().persistent();
    if amount == 0 {
        persistent.remove(&key);
    } else {
        persistent.set(&key, &amount);
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn get_pending_liquidation_collateral_underlying(env: &Env, position_id: u64) -> u128 {
    let key = DataKey::PendingLiqCollateralUnderlying(position_id);
    let persistent = env.storage().persistent();
    let amount = persistent.get(&key).unwrap_or(0u128);
    if amount > 0 {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    amount
}

pub fn set_perps_pair_config(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
    config: &PerpsPairConfig,
) {
    register_perps_pair(env, margin_asset, base_asset, side);
    let key = DataKey::PerpsPairConfig(margin_asset.clone(), base_asset.clone(), side.clone());
    env.storage().instance().set(&key, config);
}

pub fn get_perps_pair_config(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
) -> Option<PerpsPairConfig> {
    let key = DataKey::PerpsPairConfig(margin_asset.clone(), base_asset.clone(), side.clone());
    if let Some(config) = env.storage().instance().get(&key) {
        return Some(config);
    }

    // Lazily migrate configs written by earlier Wasm versions. Instance
    // storage ties protocol policy to the contract lifetime instead of an
    // independently expiring persistent entry.
    if let Some(config) = env.storage().persistent().get(&key) {
        env.storage().instance().set(&key, &config);
        bump_perps_pair_config_ttl(env, margin_asset, base_asset, side);
        return Some(config);
    }
    None
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

pub fn set_perps_pair_execution_config(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
    config: &PerpsPairExecutionConfig,
) {
    register_perps_pair(env, margin_asset, base_asset, side);
    let key =
        DataKey::PerpsPairExecutionConfig(margin_asset.clone(), base_asset.clone(), side.clone());
    env.storage().instance().set(&key, config);
}

pub fn get_perps_pair_execution_config(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
) -> Option<PerpsPairExecutionConfig> {
    let key =
        DataKey::PerpsPairExecutionConfig(margin_asset.clone(), base_asset.clone(), side.clone());
    if let Some(config) = env.storage().instance().get(&key) {
        return Some(config);
    }

    // Preserve configured policy across upgrades by migrating the legacy
    // persistent value on first use.
    let persistent = env.storage().persistent();
    if let Some(config) = persistent.get(&key) {
        env.storage().instance().set(&key, &config);
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        return Some(config);
    }
    None
}

fn register_perps_pair(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
) {
    let registry_key = DataKey::ConfiguredPerpsPairs;
    let instance = env.storage().instance();
    let mut pairs: Vec<(Address, Address, PositionSide)> =
        instance.get(&registry_key).unwrap_or(Vec::new(env));
    let pair = (margin_asset.clone(), base_asset.clone(), side.clone());
    for configured in pairs.iter() {
        if configured == pair {
            return;
        }
    }
    if pairs.len() >= MAX_PERPS_PAIR_CONFIGS {
        panic!("too many perps pairs");
    }
    pairs.push_back(pair);
    instance.set(&registry_key, &pairs);
}

pub fn get_perps_pair_execution_config_or_default(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
) -> PerpsPairExecutionConfig {
    get_perps_pair_execution_config(env, margin_asset, base_asset, side).unwrap_or(
        PerpsPairExecutionConfig {
            max_open_deviation_scaled: DEFAULT_OPEN_POOL_ORACLE_DEVIATION_SCALED,
            open_slippage_scaled: DEFAULT_OPEN_POOL_SLIPPAGE_SCALED,
            close_slippage_scaled: DEFAULT_CLOSE_POOL_SLIPPAGE_SCALED,
            liquidation_slippage_scaled: DEFAULT_LIQUIDATION_POOL_SLIPPAGE_SCALED,
        },
    )
}

pub fn set_perps_pair_exit_config(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
    config: &PerpsPairExitExecutionConfig,
) {
    register_perps_pair(env, margin_asset, base_asset, side);
    let key = DataKey::PerpsPairExitExecutionConfig(
        margin_asset.clone(),
        base_asset.clone(),
        side.clone(),
    );
    env.storage().instance().set(&key, config);
}

pub fn get_perps_pair_exit_config_or_default(
    env: &Env,
    margin_asset: &Address,
    base_asset: &Address,
    side: &PositionSide,
) -> PerpsPairExitExecutionConfig {
    let key = DataKey::PerpsPairExitExecutionConfig(
        margin_asset.clone(),
        base_asset.clone(),
        side.clone(),
    );
    if let Some(config) = env.storage().instance().get(&key) {
        return config;
    }
    PerpsPairExitExecutionConfig {
        max_close_deviation_scaled: DEFAULT_CLOSE_POOL_ORACLE_DEVIATION_SCALED,
        max_liq_deviation_scaled: DEFAULT_LIQUIDATION_POOL_ORACLE_DEVIATION_SCALED,
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

pub fn bump_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn bump_core_ttl(env: &Env) {
    bump_instance_ttl(env);
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

pub fn bump_peridottroller_ttl(env: &Env) {
    bump_instance_ttl(env);
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::Peridottroller) {
        persistent.extend_ttl(&DataKey::Peridottroller, TTL_THRESHOLD, TTL_EXTEND_TO);
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
    let pending_liquidation_key = DataKey::PendingLiquidation(position_id);
    if persistent.has(&pending_liquidation_key) {
        bump_pending_liquidation_ttl(env, position_id);
    }
}

pub fn bump_pending_liquidation_ttl(env: &Env, position_id: u64) {
    let persistent = env.storage().persistent();
    let pending_key = DataKey::PendingLiquidation(position_id);
    if persistent.has(&pending_key) {
        persistent.extend_ttl(&pending_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let takeover_key = DataKey::PendingLiquidationTakeoverAfter(position_id);
    if persistent.has(&takeover_key) {
        persistent.extend_ttl(&takeover_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let collateral_key = DataKey::PendingLiqCollateralUnderlying(position_id);
    if persistent.has(&collateral_key) {
        persistent.extend_ttl(&collateral_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

pub fn bump_user_positions_ttl(env: &Env, user: &Address) {
    let key = DataKey::UserPositions(user.clone());
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
