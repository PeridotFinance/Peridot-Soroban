//! Local validation-only custody bridge. Explicit asset registration avoids
//! treating failed custody calls as optional or silently losing ownership.
use crate::{contract::MarginController, storage::*};
use soroban_sdk::{Address, Env, IntoVal, Symbol};

impl MarginController {
    pub(crate) fn cl_enabled(env: &Env, asset: &Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::ClShareAsset(asset.clone()))
            .unwrap_or(false)
    }
    pub(crate) fn cl_assign(
        env: &Env,
        asset: &Address,
        owner: &Address,
        amount: u128,
        recovery: bool,
    ) {
        if !Self::cl_enabled(env, asset) {
            return;
        }
        let name = if recovery {
            "custody_recover_assign"
        } else {
            "custody_assign"
        };
        env.invoke_contract::<()>(
            asset,
            &Symbol::new(env, name),
            (env.current_contract_address(), owner.clone(), amount).into_val(env),
        );
    }
    pub(crate) fn cl_prepare(
        env: &Env,
        asset: &Address,
        owner: &Address,
        recipient: &Address,
        amount: u128,
        recovery: bool,
    ) {
        if !Self::cl_enabled(env, asset) {
            return;
        }
        env.invoke_contract::<()>(
            asset,
            &Symbol::new(env, "custody_prepare_transfer"),
            (
                env.current_contract_address(),
                owner.clone(),
                recipient.clone(),
                amount,
                recovery,
            )
                .into_val(env),
        );
    }
    pub(crate) fn cl_receipt_assign(
        env: &Env,
        asset: &Address,
        vault: &Address,
        owner: &Address,
        amount: u128,
    ) {
        if Self::cl_enabled(env, asset) {
            env.invoke_contract::<()>(
                vault,
                &Symbol::new(env, "cl_assign_margin_rewards"),
                (owner.clone(), amount).into_val(env),
            );
        }
    }
    pub(crate) fn cl_receipt_prepare(
        env: &Env,
        asset: &Address,
        vault: &Address,
        owner: &Address,
        ptokens: u128,
        recovery: bool,
    ) {
        if Self::cl_enabled(env, asset) {
            env.invoke_contract::<()>(
                vault,
                &Symbol::new(env, "cl_prepare_margin_rewards"),
                (owner.clone(), ptokens, recovery).into_val(env),
            );
        }
    }
    pub(crate) fn cl_receipt_reassign(
        env: &Env,
        asset: &Address,
        vault: &Address,
        owner: &Address,
        recipient: &Address,
        ptokens: u128,
        recovery: bool,
    ) {
        if Self::cl_enabled(env, asset) {
            env.invoke_contract::<()>(
                vault,
                &Symbol::new(env, "cl_reassign_margin_rewards"),
                (owner.clone(), recipient.clone(), ptokens, recovery).into_val(env),
            );
        }
    }
}
