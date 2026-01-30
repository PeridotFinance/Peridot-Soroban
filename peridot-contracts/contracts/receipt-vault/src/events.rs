use soroban_sdk::{contractevent, Address, Symbol};

/// Mirrors Compound's Mint event: emitted on deposit when pTokens are minted.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mint {
    #[topic]
    pub minter: Address,
    pub mint_amount: u128,
    pub mint_tokens: u128,
}

/// Mirrors Compound's Redeem event: emitted on withdraw when pTokens are burned.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Redeem {
    #[topic]
    pub redeemer: Address,
    pub redeem_amount: u128,
    pub redeem_tokens: u128,
}

/// Mirrors Compound's Borrow event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowEvent {
    #[topic]
    pub borrower: Address,
    pub borrow_amount: u128,
    pub account_borrows: u128,
    pub total_borrows: u128,
}

/// Mirrors Compound's RepayBorrow event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepayBorrow {
    #[topic]
    pub payer: Address,
    #[topic]
    pub borrower: Address,
    pub repay_amount: u128,
    pub account_borrows: u128,
    pub total_borrows: u128,
}

/// Mirrors Compound's AccrueInterest event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccrueInterest {
    pub interest_accumulated: u128,
    pub borrow_index: u128,
    pub total_borrows: u128,
}

/// Mirrors Compound's NewAdmin event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAdmin {
    #[topic]
    pub admin: Address,
}

/// Mirrors Compound's NewMarketInterestRateModel event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewInterestModel {
    #[topic]
    pub model: Address,
}

/// Mirrors Compound's NewReserveFactor event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewReserveFactor {
    pub reserve_factor_mantissa: u128,
}

/// Mirrors Compound's NewAdminFee event (custom extension).
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAdminFee {
    pub admin_fee_mantissa: u128,
}

/// Flash loan premium update (custom extension).
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewFlashLoanFee {
    pub fee_mantissa: u128,
}

/// Mirrors Compound's NewCollateralFactor event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewCollateralFactor {
    pub collateral_factor_mantissa: u128,
}

/// Emits when the manual supply rate is updated.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewSupplyRate {
    pub rate_mantissa: u128,
}

/// Emits when the manual borrow rate is updated.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewManualBorrowRate {
    pub rate_mantissa: u128,
}

/// Mirrors Compound's NewSupplyCap event (custom extension).
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewSupplyCap {
    pub supply_cap: u128,
}

/// Mirrors Compound's NewBorrowCap event (custom extension).
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewBorrowCap {
    pub borrow_cap: u128,
}

/// Mirrors Compound's ReservesReduced event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReservesReduced {
    pub reduce_amount: u128,
    pub total_reserves: u128,
}

/// Mirrors Compound's AdminFeesReduced event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminFeesReduced {
    pub reduce_amount: u128,
    pub total_admin_fees: u128,
}

/// Mirrors Compound's NewPeridottroller (custom) event.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewPeridottroller {
    #[topic]
    pub peridottroller: Address,
}

/// Flash loan execution log.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlashLoan {
    #[topic]
    pub receiver: Address,
    pub amount: u128,
    pub fee_paid: u128,
}

/// Records recoverable vs fatal external contract call failures.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalCallFailed {
    #[topic]
    pub contract: Address,
    #[topic]
    pub function: Symbol,
    pub recoverable: bool,
    pub failure_kind: u32,
}

/// Emits when interest math saturates to avoid panic.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterestOverflow {
    pub amount: u128,
    pub yearly_rate_scaled: u128,
    pub elapsed: u128,
}

/// Logs failed liquidation attempts for monitoring.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidSeizeAttempt {
    #[topic]
    pub borrower: Address,
    #[topic]
    pub liquidator: Address,
    pub requested: u128,
    pub reason: Symbol,
}
