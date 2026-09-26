use soroban_sdk::{Address, Env, U256};

use crate::constants::{MARGIN_FEE_PRECISION, TTL_EXTEND_TO, TTL_THRESHOLD};
use crate::helpers::{get_margin_balance_ptokens, get_total_margin_ptokens};
use crate::storage::*;

fn add(a: u128, b: u128) -> u128 {
    a.checked_add(b).expect("margin fee overflow")
}

fn sub(a: u128, b: u128) -> u128 {
    a.checked_sub(b).expect("fee index regressed")
}

fn mul_div(env: &Env, a: u128, b: u128, denominator: u128) -> u128 {
    if a == 0 || b == 0 {
        return 0;
    }
    U256::from_u128(env, a)
        .mul(&U256::from_u128(env, b))
        .div(&U256::from_u128(env, denominator))
        .to_u128()
        .expect("margin fee overflow")
}

fn bump(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn current(env: &Env, vault: &Address) -> Option<MarginFeeEpoch> {
    let key = DataKey::MarginFeeEpoch(vault.clone());
    let state = env.storage().persistent().get(&key);
    if state.is_some() {
        bump(env, &key);
    }
    state
}

pub(crate) fn settled_index(env: &Env, vault: &Address) -> u128 {
    current(env, vault).map(|s| s.settled_index).unwrap_or(0)
}

/// Called before adding to PendingMarginFees. Eligibility is fixed at fee charge,
/// not at the later conversion. No remainder is reassigned to future depositors.
pub(crate) fn reserve(env: &Env, vault: &Address, ptokens: u128, underlying: u128) {
    let mut state = current(env, vault).unwrap_or_else(|| {
        let pending = crate::fees::pending_margin_fees(env, vault);
        if pending.ptokens != 0 || pending.underlying != 0 {
            // Historical weights cannot be reconstructed from an old fee batch.
            panic!("settle legacy fee batch before upgrade");
        }
        MarginFeeEpoch::default()
    });
    let total = get_total_margin_ptokens(env, vault);
    if total == 0 {
        state.orphan_ptokens = add(state.orphan_ptokens, ptokens);
        state.orphan_underlying = add(state.orphan_underlying, underlying);
    } else {
        state.ptoken_index = add(
            state.ptoken_index,
            mul_div(env, ptokens, MARGIN_FEE_PRECISION, total),
        );
        state.underlying_index = add(
            state.underlying_index,
            mul_div(env, underlying, MARGIN_FEE_PRECISION, total),
        );
    }
    let key = DataKey::MarginFeeEpoch(vault.clone());
    env.storage().persistent().set(&key, &state);
    bump(env, &key);
}

/// Seal a converted batch using the actual minted share delta. Prefix indices let
/// an untouched account skip arbitrarily many completed batches in constant work.
pub(crate) fn settle(env: &Env, vault: &Address, underlying: u128, minted: u128) {
    let state = current(env, vault).expect("fee entitlement state missing");
    let settled_index = add(
        state.settled_index,
        add(
            state.ptoken_index,
            mul_div(env, state.underlying_index, minted, underlying),
        ),
    );
    let closed = ClosedMarginFeeEpoch {
        ptoken_index: state.ptoken_index,
        underlying_index: state.underlying_index,
        settled_index,
        underlying,
        minted_ptokens: minted,
    };
    let key = DataKey::ClosedMarginFeeEpoch(vault.clone(), state.id);
    env.storage().persistent().set(&key, &closed);
    bump(env, &key);
    let orphan = add(
        state.orphan_ptokens,
        mul_div(env, state.orphan_underlying, minted, underlying),
    );
    if orphan > 0 {
        let key = DataKey::MarginFeeOrphan(vault.clone());
        let previous = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &add(previous, orphan));
        bump(env, &key);
    }
    let key = DataKey::MarginFeeEpoch(vault.clone());
    env.storage().persistent().set(
        &key,
        &MarginFeeEpoch {
            id: state.id.checked_add(1).expect("fee epoch overflow"),
            settled_index,
            ..MarginFeeEpoch::default()
        },
    );
    bump(env, &key);
}

fn checkpoint_values(
    env: &Env,
    user: &Address,
    vault: &Address,
) -> Option<(UserMarginFeeEpoch, u128)> {
    let state = current(env, vault)?;
    let key = DataKey::UserMarginFeeEpoch(user.clone(), vault.clone());
    let mut account: UserMarginFeeEpoch = env.storage().persistent().get(&key).unwrap_or_default();
    if env.storage().persistent().has(&key) {
        bump(env, &key);
    }
    let balance = get_margin_balance_ptokens(env, user, vault);
    let mut claim = 0;
    if account.id < state.id {
        let key = DataKey::ClosedMarginFeeEpoch(vault.clone(), account.id);
        let closed: ClosedMarginFeeEpoch = env
            .storage()
            .persistent()
            .get(&key)
            .expect("fee epoch history missing");
        bump(env, &key);
        let ptokens = add(
            account.ptokens,
            mul_div(
                env,
                balance,
                sub(closed.ptoken_index, account.ptoken_index),
                MARGIN_FEE_PRECISION,
            ),
        );
        let underlying = add(
            account.underlying,
            mul_div(
                env,
                balance,
                sub(closed.underlying_index, account.underlying_index),
                MARGIN_FEE_PRECISION,
            ),
        );
        claim = add(
            ptokens,
            mul_div(env, underlying, closed.minted_ptokens, closed.underlying),
        );
        // The balance cannot have changed across skipped epochs: every mutation
        // checkpoints first. Only the first epoch needs its conversion record.
        claim = add(
            claim,
            mul_div(
                env,
                balance,
                sub(state.settled_index, closed.settled_index),
                MARGIN_FEE_PRECISION,
            ),
        );
        account = UserMarginFeeEpoch {
            id: state.id,
            ..UserMarginFeeEpoch::default()
        };
    }
    if account.id != state.id {
        panic!("fee epoch regressed");
    }
    account.ptokens = add(
        account.ptokens,
        mul_div(
            env,
            balance,
            sub(state.ptoken_index, account.ptoken_index),
            MARGIN_FEE_PRECISION,
        ),
    );
    account.underlying = add(
        account.underlying,
        mul_div(
            env,
            balance,
            sub(state.underlying_index, account.underlying_index),
            MARGIN_FEE_PRECISION,
        ),
    );
    account.ptoken_index = state.ptoken_index;
    account.underlying_index = state.underlying_index;
    Some((account, claim))
}

pub(crate) fn claimable(env: &Env, user: &Address, vault: &Address) -> u128 {
    checkpoint_values(env, user, vault)
        .map(|(_, claim)| claim)
        .unwrap_or(0)
}

/// Called by accrue_user_fee BEFORE every free-margin balance change. Reserved
/// entitlements survive withdrawing principal, even before conversion succeeds.
pub(crate) fn checkpoint(env: &Env, user: &Address, vault: &Address) {
    let Some((account, claim)) = checkpoint_values(env, user, vault) else {
        return;
    };
    let key = DataKey::UserMarginFeeEpoch(user.clone(), vault.clone());
    env.storage().persistent().set(&key, &account);
    bump(env, &key);
    if claim > 0 {
        let key = DataKey::UserMarginFeeAccrued(user.clone(), vault.clone());
        let previous = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &add(previous, claim));
        bump(env, &key);
    }
}
