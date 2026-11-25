//! WP1: backstop module. Signatures frozen in WP0; bodies owned by WP1.
use crate::common::*;
use crate::{MarginController, MarginControllerArgs, MarginControllerClient};
use crate::types::*;
use soroban_sdk::{contractimpl, Address, Env};

// ---- crate-internal API used by WP2 / WP3
/// Credit `usdc_pt` USDC pTokens (already held by this contract and already added to
/// Ledger(USDC) by the caller) to the backstop as fee income.
pub(crate) fn credit_pt(_env: &Env, _usdc_pt: u128) { unimplemented!("WP1") }
/// Move `shares` debt shares of `debt_asset` from a position (caller already removed them
/// from DebtShares) into BsDebtShares(debt_asset).
pub(crate) fn assign_bad_debt(_env: &Env, _debt_asset: &Address, _shares: u128) { unimplemented!("WP1") }
/// Net backstop USDC pTokens: Bs.ptokens - USDC value of BsDebtShares(USDC), rounded against
/// the pool. Negative if exhausted. Panics BackstopDebtOutstanding if BsDebtShares(XLM) > 0
/// only when `strict` (deposit/withdraw); otherwise values XLM bad debt at the oracle.
pub(crate) fn nav_pt(_env: &Env, _strict: bool) -> i128 { unimplemented!("WP1") }
/// USDC pTokens equivalent of Bs.pending_shares at the current NAV (>= 0).
pub(crate) fn pending_pt(_env: &Env) -> u128 { unimplemented!("WP1") }
/// Backstop value in usd7 (nav_pt * exchange rate * USDC price); 0 if negative.
pub(crate) fn value_usd(_env: &Env) -> u128 { unimplemented!("WP1") }

#[contractimpl]
impl MarginController {
    pub fn set_backstop_params(_env: Env, _admin: Address, _p: BackstopParams) { unimplemented!("WP1") }
    /// First deposit must be by the admin (treasury seed) and mints BS_DEAD_SHARES to the contract.
    pub fn backstop_deposit(_env: Env, _backer: Address, _ptokens: u128, _min_shares: u128) -> u128 { unimplemented!("WP1") }
    pub fn backstop_request_withdraw(_env: Env, _backer: Address, _shares: u128) { unimplemented!("WP1") }
    pub fn backstop_cancel_withdraw(_env: Env, _backer: Address) { unimplemented!("WP1") }
    /// After ready_at and before expires_at; pays at execution-time NAV; then
    /// headroom::pool_check(Check::BackstopExit). Returns USDC pTokens paid.
    pub fn backstop_withdraw(_env: Env, _backer: Address, _min_ptokens: u128) -> u128 { unimplemented!("WP1") }
    /// Permissionless: repay backstop-owned bad debt from backstop USDC pTokens (XLM: swap).
    /// Returns debt repaid (raw debt units).
    pub fn settle_backstop_debt(_env: Env, _caller: Address, _debt_asset: Address, _max_ptokens: u128, _min_out: u128) -> u128 { unimplemented!("WP1") }
    pub fn get_backstop(_env: Env) -> Backstop { unimplemented!("WP1") }
    /// (nav_pt, shares)
    pub fn backstop_nav(_env: Env) -> (i128, u128) { unimplemented!("WP1") }
    pub fn get_backer(_env: Env, _backer: Address) -> u128 { unimplemented!("WP1") }
    pub fn get_withdraw_req(_env: Env, _backer: Address) -> Option<WithdrawReq> { unimplemented!("WP1") }
    pub fn get_bs_debt_shares(_env: Env, _debt_asset: Address) -> u128 { unimplemented!("WP1") }
}
