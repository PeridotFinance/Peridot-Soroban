use soroban_sdk::{contractevent, Address, Symbol};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleUpdated {
    #[topic]
    pub oracle: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminUpdated {
    #[topic]
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloseFactorUpdated {
    pub close_factor_mantissa: u128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidationIncentiveUpdated {
    pub incentive_mantissa: u128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketCollateralFactorUpdated {
    #[topic]
    pub market: Address,
    pub cf_mantissa: u128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidationFeeUpdated {
    pub fee_mantissa: u128,
}

#[contractevent(topics = ["oracle_max_age_multiplier"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleMaxAgeMultiplierUpdated {
    pub multiplier: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveRecipientUpdated {
    #[topic]
    pub recipient: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleAssetSymbolMapped {
    #[topic]
    pub token: Address,
    pub symbol: Option<Symbol>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeridotTokenSet {
    #[topic]
    pub token: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PauseGuardianUpdated {
    #[topic]
    pub guardian: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowPauseUpdated {
    #[topic]
    pub market: Address,
    pub paused: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedeemPauseUpdated {
    #[topic]
    pub market: Address,
    pub paused: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidationPauseUpdated {
    #[topic]
    pub market: Address,
    pub paused: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositPauseUpdated {
    #[topic]
    pub market: Address,
    pub paused: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketAdded {
    #[topic]
    pub market: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketRemoved {
    #[topic]
    pub market: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FallbackPriceUpdated {
    #[topic]
    pub token: Address,
    pub price: Option<u128>,
    pub scale: Option<u128>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketEntered {
    #[topic]
    pub account: Address,
    #[topic]
    pub market: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketExited {
    #[topic]
    pub account: Address,
    #[topic]
    pub market: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiquidateBorrow {
    #[topic]
    pub liquidator: Address,
    #[topic]
    pub borrower: Address,
    #[topic]
    pub repay_market: Address,
    pub collateral_market: Address,
    pub repay_amount: u128,
    pub seize_tokens: u128,
}
