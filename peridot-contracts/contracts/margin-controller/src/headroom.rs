//! WP3: pool check, required backstop size, reconcile / Impaired. Signatures frozen in WP0.
use crate::common::*;
use crate::types::*;
use crate::{MarginController, MarginControllerArgs, MarginControllerClient};
use soroban_sdk::{contractimpl, Address, Env};

/// SPEC §4 checks 1-3 on the post-trade pooled state (both receipts' update_interest first,
/// then controller.portfolio(me)). Check::Open also requires backstop::value_usd >=
/// required_backstop_usd (else BackstopTooSmall) and excludes backstop::pending_pt from USDC
/// collateral. Panics PoolCheck (Open) / BackstopHeadroom (BackstopExit).
pub(crate) fn pool_check(_env: &Env, _check: Check) {
    unimplemented!("WP3")
}
/// B_req = coef_long*long equity + coef_short*short equity + coef_largest*max(per_position caps), usd7.
pub(crate) fn required_backstop_usd(_env: &Env) -> u128 {
    unimplemented!("WP3")
}
/// Panics Impaired if the Impaired flag is set.
pub(crate) fn require_not_impaired(_env: &Env) {
    unimplemented!("WP3")
}

#[contractimpl]
impl MarginController {
    pub fn set_headroom(_env: Env, _admin: Address, _h: Headroom) {
        unimplemented!("WP3")
    }
    pub fn get_headroom(_env: Env) -> Headroom {
        unimplemented!("WP3")
    }
    /// (hf_spot, hf_down, hf_up, free_usd) 1e6 / usd7; u128::MAX HF when no debt.
    pub fn pooled_health(_env: Env) -> (u128, u128, u128, u128) {
        unimplemented!("WP3")
    }
    pub fn required_backstop(_env: Env) -> u128 {
        unimplemented!("WP3")
    }
    /// Permissionless: compare receipt.get_ptoken_balance(me) with Ledger(asset) for USDC and
    /// XLM; on any deficit set Impaired, pause opens, emit margin_impaired. Returns true if impaired.
    pub fn reconcile(_env: Env, _caller: Address) -> bool {
        unimplemented!("WP3")
    }
    /// Book a charge (pTokens) against the backstop and Ledger; clears Impaired only if
    /// Ledger <= balance for both markets afterwards.
    pub fn admin_resolve_impairment(
        _env: Env,
        _admin: Address,
        _usdc_pt_charge: u128,
        _xlm_pt_charge: u128,
    ) {
        unimplemented!("WP3")
    }
    pub fn is_impaired(_env: Env) -> bool {
        unimplemented!("WP3")
    }
}
