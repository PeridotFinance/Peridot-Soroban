use soroban_sdk::{contractevent, Address, Symbol};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultInitialized {
    pub admin: Address,
    pub pool: Address,
    pub underlying: Address,
    pub tick_lower: i32,
    pub tick_upper: i32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Deposited {
    pub from: Address,
    pub underlying_in: u128,
    pub liquidity_minted: u128,
    pub shares_minted: u128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Withdrawn {
    pub to: Address,
    pub shares_burned: u128,
    pub liquidity_burned: u128,
    pub underlying_out: u128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Harvested {
    pub caller: Address,
    pub reward_token: Address,
    pub reward_amount: u128,
    pub underlying_out: u128,
}

/// Emitted when a harvest leg is skipped rather than reverting the whole call.
/// A single unsellable reward token must not block the rest of the harvest.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarvestSkipped {
    pub reward_token: Address,
    pub reward_amount: u128,
    pub reason: Symbol,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalCallFailed {
    pub contract: Address,
    pub function: Symbol,
    pub recoverable: bool,
    pub failure_kind: u32,
}

/// Raised when liquidity was burned but no underlying came back. Emitting
/// instead of panicking keeps withdrawals from being DoS'd by a misbehaving
/// pool, matching `receipt-vault`'s `BoostedRedeemZeroReturn` handling.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedeemZeroReturn {
    pub liquidity_burned: u128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferProposed {
    pub current_admin: Address,
    pub pending_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferred {
    pub previous_admin: Address,
    pub new_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigChanged {
    pub what: Symbol,
    pub old_value: u128,
    pub new_value: u128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PausedSet {
    pub paused: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptVaultBound {
    pub receipt_vault: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimaryRewardTokenSet {
    pub reward_token: Option<Address>,
}
