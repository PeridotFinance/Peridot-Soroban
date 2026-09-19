//! Native-only LOCAL migration diagnostics, never an activation permit.
//!
//! Even a successful local check says nothing about archived per-owner debt,
//! external controller membership/incentives, strategy custody, or old emissions.
//! There is deliberately no activation method, admin attestation, storage write,
//! or public contract ABI. See ../MIGRATION.md for the remaining release gates.

use crate::{storage::LpKey, DataKey};
use soroban_sdk::{token, Address, Env, String};
use stellar_tokens::fungible::{Base as TokenBase, FungibleStorageKey};

/// Exact local accounting observations. Optional values remain unknown, NOT zero.
/// Token getters may renew their own TTL; this module never rewrites accounting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSnapshot {
    pub receipt: Address,
    pub asset: Address,
    pub admin: Address,
    pub strategy: Option<Address>,
    pub controller: Option<Address>,
    pub margin_controller: Option<Address>,
    pub supply: i128,
    pub escrow: i128,
    pub actual_cash: i128,
    pub managed_cash: u128,
    pub deposited: u128,
    pub initial_rate: u128,
    pub supply_cap: u128,
    pub idle_buffer: Option<u32>,
    pub borrowed: u128,
    pub principal: Option<u128>,
    pub bad_debt: Option<u128>,
    pub reserves: Option<u128>,
    pub admin_fees: Option<u128>,
    pub decimals: u32,
    pub name: String,
    pub symbol: String,
    pub lp_version: Option<u32>,
    pub pending_admin: Option<Address>,
    pub flash_active: Option<bool>,
}

/// Read in the receipt's own storage context. Missing mandatory state or token
/// metadata traps; do not catch that failure and fabricate a replacement value.
pub fn snapshot(env: &Env) -> LocalSnapshot {
    let p = env.storage().persistent();
    assert_eq!(p.get::<_, bool>(&DataKey::Initialized), Some(true));
    let receipt = env.current_contract_address();
    let asset: Address = p
        .get(&DataKey::UnderlyingToken)
        .expect("underlying missing");
    LocalSnapshot {
        actual_cash: token::Client::new(env, &asset).balance(&receipt),
        supply: env
            .storage()
            .instance()
            .get(&FungibleStorageKey::TotalSupply)
            .expect("total supply missing"),
        escrow: TokenBase::balance(env, &receipt),
        receipt,
        asset,
        admin: p.get(&DataKey::Admin).expect("admin missing"),
        strategy: p.get(&DataKey::BoostedVault),
        controller: p.get(&DataKey::Peridottroller),
        margin_controller: p.get(&DataKey::MarginController),
        managed_cash: p.get(&DataKey::ManagedCash).expect("managed cash missing"),
        deposited: p.get(&DataKey::TotalDeposited).expect("deposited missing"),
        initial_rate: p.get(&DataKey::InitialExchangeRate).expect("rate missing"),
        supply_cap: p.get(&DataKey::SupplyCap).expect("cap missing"),
        idle_buffer: p.get(&DataKey::IdleCashBufferBps),
        borrowed: p.get(&DataKey::TotalBorrowed).expect("borrowed missing"),
        principal: p.get(&DataKey::TotalBorrowPrincipal),
        bad_debt: p.get(&DataKey::TotalBadDebt),
        reserves: p.get(&DataKey::TotalReserves),
        admin_fees: p.get(&DataKey::TotalAdminFees),
        decimals: TokenBase::decimals(env),
        name: TokenBase::name(env),
        symbol: TokenBase::symbol(env),
        lp_version: env.storage().instance().get(&LpKey::LpReceiptVersion),
        pending_admin: p.get(&DataKey::PendingAdmin),
        flash_active: p.get(&DataKey::FlashLoanActive),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalBlocker {
    AlreadyActivated,
    UnknownAccounting,
    DebtOrFees,
    InvalidBacking,
    ExistingEscrow,
    AdministrativeTransition,
    ExternalController,
}

/// Necessary LOCAL conditions only. `Ok(())` MUST NOT authorize an upgrade or
/// clear any external gate. In particular absent controller keys are not proof
/// of absence from an external controller's registry (or from archive).
pub fn check_local(snapshot: &LocalSnapshot) -> Result<(), LocalBlocker> {
    use LocalBlocker::*;
    if snapshot.lp_version.is_some() {
        return Err(AlreadyActivated);
    }
    if snapshot.principal.is_none()
        || snapshot.bad_debt.is_none()
        || snapshot.reserves.is_none()
        || snapshot.admin_fees.is_none()
        || snapshot.idle_buffer.is_none()
    {
        return Err(UnknownAccounting);
    }
    if snapshot.borrowed != 0
        || snapshot.principal != Some(0)
        || snapshot.bad_debt != Some(0)
        || snapshot.reserves != Some(0)
        || snapshot.admin_fees != Some(0)
    {
        return Err(DebtOrFees);
    }
    if snapshot.supply < 0
        || snapshot.actual_cash < 0
        || snapshot.managed_cash > snapshot.actual_cash as u128
        || snapshot.initial_rate == 0
        || snapshot.idle_buffer.unwrap() > 10_000
    {
        return Err(InvalidBacking);
    }
    if snapshot.escrow != 0 {
        return Err(ExistingEscrow);
    }
    if snapshot.pending_admin.is_some() || snapshot.flash_active == Some(true) {
        return Err(AdministrativeTransition);
    }
    if snapshot.controller.is_some() || snapshot.margin_controller.is_some() {
        return Err(ExternalController);
    }
    Ok(())
}

/// Compare a same-ledger LOCAL shadow cutover. Only the activation marker may
/// differ. This does not compare owner entries, allowances, strategy state or
/// reward history: the rehearsal and release checklist cover those separately.
pub fn assert_local_preserved(before: &LocalSnapshot, after: &LocalSnapshot) {
    assert_eq!(before.lp_version, None, "not legacy state");
    assert_eq!(after.lp_version, Some(1), "not LP state");
    let mut expected = before.clone();
    expected.lp_version = Some(1);
    assert_eq!(&expected, after, "local cutover changed accounting");
}
