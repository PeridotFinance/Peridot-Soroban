use soroban_sdk::{contractevent, Address};

use crate::storage::{PositionMode, PositionStatus};

#[contractevent(topics = ["admin_transfer_proposed"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferProposed {
    #[topic]
    pub current_admin: Address,
    #[topic]
    pub pending_admin: Address,
}

#[contractevent(topics = ["admin_transferred"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferred {
    #[topic]
    pub previous_admin: Address,
    #[topic]
    pub new_admin: Address,
}

#[contractevent(topics = ["position_created"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionCreated {
    #[topic]
    pub owner: Address,
    #[topic]
    pub position_id: u64,
    pub mode: PositionMode,
    pub status: PositionStatus,
}

#[contractevent(topics = ["position_removed"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionRemoved {
    #[topic]
    pub owner: Address,
    #[topic]
    pub position_id: u64,
    pub removed_at: u64,
}

#[contractevent(topics = ["liquidation_started"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidationStarted {
    #[topic]
    pub position_id: u64,
    #[topic]
    pub liquidator: Address,
    pub owner: Address,
    pub debt_amount: u128,
    pub takeover_after: u64,
}

#[contractevent(topics = ["liquidation_swapped"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidationSwapped {
    #[topic]
    pub position_id: u64,
    #[topic]
    pub liquidator: Address,
    pub collateral_underlying: u128,
    pub received_debt_asset: u128,
    pub takeover_after: u64,
}

#[contractevent(topics = ["liquidation_finished"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidationFinished {
    #[topic]
    pub position_id: u64,
    #[topic]
    pub liquidator: Address,
    pub owner: Address,
    pub repaid: u128,
    pub bad_debt: u128,
    pub incentive: u128,
}
