#![no_std]
use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, Address, Env, IntoVal, Map, Symbol, Vec,
};

#[contracttype]
pub enum DataKey {
    Admin,
    PauseGuardian,              // Address (optional)
    SupportedMarkets,           // Map<Address, bool>
    UserMarkets(Address),       // Vec<Address>
    Oracle,                     // Address
    CloseFactorScaled,          // u128 scaled 1e6
    LiquidationIncentiveScaled, // u128 scaled 1e6
    ReserveRecipient,           // Address for liquidation fee pTokens
    PauseBorrow,                // Map<Address, bool>
    PauseRedeem,                // Map<Address, bool>
    PauseLiquidation,           // Map<Address, bool>
    PauseDeposit,               // Map<Address, bool>
    LiquidationFeeScaled,       // u128 scaled 1e6, portion to reserves
    OracleMaxAgeMultiplier,     // u64 multiplier of resolution (default 2)
    OracleAssetSymbol(Address), // Optional Reflector symbol override
    // Collateral factors per market (scaled 1e6)
    MarketCF(Address),
    // Rewards
    PeridotToken,
    SupplySpeed(Address),
    BorrowSpeed(Address),
    SupplyIndex(Address),
    BorrowIndex(Address),
    SupplyIndexTime(Address),
    BorrowIndexTime(Address),
    UserSupplyIndex(Address, Address),
    UserBorrowIndex(Address, Address),
    Accrued(Address),
}

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

#[contract]
pub struct SimplePeridottroller;

#[contractimpl]
impl SimplePeridottroller {
    pub fn initialize(env: Env, admin: Address) {
        if env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Admin)
            .is_some()
        {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        let markets: Map<Address, bool> = Map::new(&env);
        env.storage()
            .persistent()
            .set(&DataKey::SupportedMarkets, &markets);
        // Defaults: 50% close factor, 1.08x liquidation incentive
        env.storage()
            .persistent()
            .set(&DataKey::CloseFactorScaled, &500_000u128);
        env.storage()
            .persistent()
            .set(&DataKey::LiquidationIncentiveScaled, &1_080_000u128);
        // Initialize pause maps
        env.storage()
            .persistent()
            .set(&DataKey::PauseBorrow, &Map::<Address, bool>::new(&env));
        env.storage()
            .persistent()
            .set(&DataKey::PauseRedeem, &Map::<Address, bool>::new(&env));
        env.storage()
            .persistent()
            .set(&DataKey::PauseLiquidation, &Map::<Address, bool>::new(&env));
        env.storage()
            .persistent()
            .set(&DataKey::PauseDeposit, &Map::<Address, bool>::new(&env));
        env.storage()
            .persistent()
            .set(&DataKey::LiquidationFeeScaled, &0u128);
        env.storage()
            .persistent()
            .set(&DataKey::OracleMaxAgeMultiplier, &2u64);
    }

    pub fn set_oracle(env: Env, oracle: Address) {
        require_admin(env.clone());
        env.storage().persistent().set(&DataKey::Oracle, &oracle);
        OracleUpdated {
            oracle: oracle.clone(),
        }
        .publish(&env);
    }

    // Admin transfer
    pub fn set_admin(env: Env, new_admin: Address) {
        require_admin(env.clone());
        env.storage().persistent().set(&DataKey::Admin, &new_admin);
        AdminUpdated {
            admin: new_admin.clone(),
        }
        .publish(&env);
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set")
    }

    pub fn get_oracle(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Oracle)
    }

    pub fn upgrade_wasm(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(env.clone());
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // Admin parameters
    pub fn set_close_factor(env: Env, close_factor_scaled: u128) {
        require_admin(env.clone());
        if close_factor_scaled > 1_000_000u128 {
            panic!("invalid close factor");
        }
        env.storage()
            .persistent()
            .set(&DataKey::CloseFactorScaled, &close_factor_scaled);
        CloseFactorUpdated {
            close_factor_mantissa: close_factor_scaled,
        }
        .publish(&env);
    }

    pub fn set_liquidation_incentive(env: Env, li_scaled: u128) {
        require_admin(env.clone());
        if li_scaled < 1_000_000u128 {
            panic!("invalid incentive");
        }
        env.storage()
            .persistent()
            .set(&DataKey::LiquidationIncentiveScaled, &li_scaled);
        LiquidationIncentiveUpdated {
            incentive_mantissa: li_scaled,
        }
        .publish(&env);
    }

    // Market collateral factor admin setter/getter
    pub fn set_market_cf(env: Env, market: Address, cf_scaled: u128) {
        require_admin(env.clone());
        if cf_scaled > 1_000_000u128 {
            panic!("invalid collateral factor");
        }
        env.storage()
            .persistent()
            .set(&DataKey::MarketCF(market.clone()), &cf_scaled);
        MarketCollateralFactorUpdated {
            market: market.clone(),
            cf_mantissa: cf_scaled,
        }
        .publish(&env);
    }

    pub fn get_market_cf(env: Env, market: Address) -> u128 {
        // Default to 50% unless explicitly set (Compound v2 default)
        env.storage()
            .persistent()
            .get(&DataKey::MarketCF(market))
            .unwrap_or(500_000u128)
    }

    pub fn set_liquidation_fee(env: Env, fee_scaled: u128) {
        require_admin(env.clone());
        if fee_scaled > 1_000_000u128 {
            panic!("invalid fee");
        }
        env.storage()
            .persistent()
            .set(&DataKey::LiquidationFeeScaled, &fee_scaled);
        LiquidationFeeUpdated {
            fee_mantissa: fee_scaled,
        }
        .publish(&env);
    }

    pub fn set_oracle_max_age_multiplier(env: Env, k: u64) {
        require_admin(env.clone());
        if k == 0 {
            panic!("invalid max age mult");
        }
        env.storage()
            .persistent()
            .set(&DataKey::OracleMaxAgeMultiplier, &k);
        OracleMaxAgeMultiplierUpdated { multiplier: k }.publish(&env);
    }

    pub fn set_oracle_asset_symbol(env: Env, token: Address, symbol: Option<Symbol>) {
        require_admin(env.clone());
        match symbol.clone() {
            Some(sym) => env
                .storage()
                .persistent()
                .set(&DataKey::OracleAssetSymbol(token.clone()), &sym),
            None => env
                .storage()
                .persistent()
                .remove(&DataKey::OracleAssetSymbol(token.clone())),
        }
        OracleAssetSymbolMapped { token, symbol }.publish(&env);
    }

    pub fn set_reserve_recipient(env: Env, recipient: Address) {
        require_admin(env.clone());
        env.storage()
            .persistent()
            .set(&DataKey::ReserveRecipient, &recipient);
        ReserveRecipientUpdated { recipient }.publish(&env);
    }

    // Rewards admin
    pub fn set_peridot_token(env: Env, token: Address) {
        require_admin(env.clone());
        env.storage()
            .persistent()
            .set(&DataKey::PeridotToken, &token);
        PeridotTokenSet { token }.publish(&env);
    }

    pub fn set_supply_speed(env: Env, market: Address, speed_per_sec: u128) {
        require_admin(env.clone());
        let now = env.ledger().timestamp();
        // initialize index/time on first set
        let exists: Option<u128> = env
            .storage()
            .persistent()
            .get(&DataKey::SupplyIndex(market.clone()));
        if exists.is_none() {
            env.storage()
                .persistent()
                .set(&DataKey::SupplyIndex(market.clone()), &INDEX_SCALE_1E18);
        }
        env.storage()
            .persistent()
            .set(&DataKey::SupplyIndexTime(market.clone()), &now);
        env.storage()
            .persistent()
            .set(&DataKey::SupplySpeed(market), &speed_per_sec);
    }

    pub fn set_borrow_speed(env: Env, market: Address, speed_per_sec: u128) {
        require_admin(env.clone());
        let now = env.ledger().timestamp();
        let exists: Option<u128> = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex(market.clone()));
        if exists.is_none() {
            env.storage()
                .persistent()
                .set(&DataKey::BorrowIndex(market.clone()), &INDEX_SCALE_1E18);
        }
        env.storage()
            .persistent()
            .set(&DataKey::BorrowIndexTime(market.clone()), &now);
        env.storage()
            .persistent()
            .set(&DataKey::BorrowSpeed(market), &speed_per_sec);
    }

    pub fn set_pause_guardian(env: Env, guardian: Address) {
        require_admin(env.clone());
        env.storage()
            .persistent()
            .set(&DataKey::PauseGuardian, &guardian);
        PauseGuardianUpdated { guardian }.publish(&env);
    }

    // Pause controls
    pub fn set_pause_borrow(env: Env, market: Address, paused: bool) {
        require_admin(env.clone());
        let mut m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseBorrow)
            .unwrap_or(Map::new(&env));
        m.set(market.clone(), paused);
        env.storage().persistent().set(&DataKey::PauseBorrow, &m);
        BorrowPauseUpdated {
            market: market.clone(),
            paused,
        }
        .publish(&env);
    }
    pub fn is_borrow_paused(env: Env, market: Address) -> bool {
        let m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseBorrow)
            .unwrap_or(Map::new(&env));
        m.get(market).unwrap_or(false)
    }

    pub fn set_pause_redeem(env: Env, market: Address, paused: bool) {
        require_admin(env.clone());
        let mut m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseRedeem)
            .unwrap_or(Map::new(&env));
        m.set(market.clone(), paused);
        env.storage().persistent().set(&DataKey::PauseRedeem, &m);
        RedeemPauseUpdated {
            market: market.clone(),
            paused,
        }
        .publish(&env);
    }
    pub fn is_redeem_paused(env: Env, market: Address) -> bool {
        let m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseRedeem)
            .unwrap_or(Map::new(&env));
        m.get(market).unwrap_or(false)
    }

    pub fn set_pause_liquidation(env: Env, market: Address, paused: bool) {
        require_admin(env.clone());
        let mut m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseLiquidation)
            .unwrap_or(Map::new(&env));
        m.set(market.clone(), paused);
        env.storage()
            .persistent()
            .set(&DataKey::PauseLiquidation, &m);
        LiquidationPauseUpdated {
            market: market.clone(),
            paused,
        }
        .publish(&env);
    }

    pub fn set_pause_deposit(env: Env, market: Address, paused: bool) {
        require_admin(env.clone());
        let mut m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseDeposit)
            .unwrap_or(Map::new(&env));
        m.set(market.clone(), paused);
        env.storage().persistent().set(&DataKey::PauseDeposit, &m);
        DepositPauseUpdated {
            market: market.clone(),
            paused,
        }
        .publish(&env);
    }

    // Guardian variants
    pub fn pause_borrow_g(env: Env, guardian: Address, market: Address, paused: bool) {
        let stored: Option<Address> = env.storage().persistent().get(&DataKey::PauseGuardian);
        let Some(g) = stored else {
            panic!("no guardian");
        };
        if g != guardian {
            panic!("invalid guardian");
        }
        guardian.require_auth();
        let mut m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseBorrow)
            .unwrap_or(Map::new(&env));
        m.set(market.clone(), paused);
        env.storage().persistent().set(&DataKey::PauseBorrow, &m);
        BorrowPauseUpdated {
            market: market.clone(),
            paused,
        }
        .publish(&env);
    }

    pub fn pause_redeem_g(env: Env, guardian: Address, market: Address, paused: bool) {
        let stored: Option<Address> = env.storage().persistent().get(&DataKey::PauseGuardian);
        let Some(g) = stored else {
            panic!("no guardian");
        };
        if g != guardian {
            panic!("invalid guardian");
        }
        guardian.require_auth();
        let mut m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseRedeem)
            .unwrap_or(Map::new(&env));
        m.set(market.clone(), paused);
        env.storage().persistent().set(&DataKey::PauseRedeem, &m);
        RedeemPauseUpdated {
            market: market.clone(),
            paused,
        }
        .publish(&env);
    }

    pub fn pause_liquidation_g(env: Env, guardian: Address, market: Address, paused: bool) {
        let stored: Option<Address> = env.storage().persistent().get(&DataKey::PauseGuardian);
        let Some(g) = stored else {
            panic!("no guardian");
        };
        if g != guardian {
            panic!("invalid guardian");
        }
        guardian.require_auth();
        let mut m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseLiquidation)
            .unwrap_or(Map::new(&env));
        m.set(market.clone(), paused);
        env.storage()
            .persistent()
            .set(&DataKey::PauseLiquidation, &m);
        LiquidationPauseUpdated {
            market: market.clone(),
            paused,
        }
        .publish(&env);
    }

    pub fn pause_deposit_g(env: Env, guardian: Address, market: Address, paused: bool) {
        let stored: Option<Address> = env.storage().persistent().get(&DataKey::PauseGuardian);
        let Some(g) = stored else {
            panic!("no guardian");
        };
        if g != guardian {
            panic!("invalid guardian");
        }
        guardian.require_auth();
        let mut m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseDeposit)
            .unwrap_or(Map::new(&env));
        m.set(market.clone(), paused);
        env.storage().persistent().set(&DataKey::PauseDeposit, &m);
        DepositPauseUpdated {
            market: market.clone(),
            paused,
        }
        .publish(&env);
    }
    pub fn is_liquidation_paused(env: Env, market: Address) -> bool {
        let m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseLiquidation)
            .unwrap_or(Map::new(&env));
        m.get(market).unwrap_or(false)
    }

    pub fn is_deposit_paused(env: Env, market: Address) -> bool {
        let m: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PauseDeposit)
            .unwrap_or(Map::new(&env));
        m.get(market).unwrap_or(false)
    }

    pub fn add_market(env: Env, market: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        let mut markets: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::SupportedMarkets)
            .unwrap_or(Map::new(&env));
        markets.set(market.clone(), true);
        env.storage()
            .persistent()
            .set(&DataKey::SupportedMarkets, &markets);
        MarketAdded { market }.publish(&env);
    }

    pub fn enter_market(env: Env, user: Address, market: Address) {
        user.require_auth();
        let markets: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::SupportedMarkets)
            .unwrap_or(Map::new(&env));
        if markets.get(market.clone()).unwrap_or(false) == false {
            panic!("Market not supported");
        }
        let mut entered: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::UserMarkets(user.clone()))
            .unwrap_or(Vec::new(&env));
        if !entered.contains(market.clone()) {
            entered.push_back(market.clone());
            env.storage()
                .persistent()
                .set(&DataKey::UserMarkets(user.clone()), &entered);
        }
        MarketEntered {
            account: user.clone(),
            market: market.clone(),
        }
        .publish(&env);
    }

    pub fn get_user_markets(env: Env, user: Address) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::UserMarkets(user))
            .unwrap_or(Vec::new(&env))
    }

    pub fn exit_market(env: Env, user: Address, market: Address) {
        user.require_auth();
        let entered: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::UserMarkets(user.clone()))
            .unwrap_or(Vec::new(&env));
        // Safety: block exit if user has pTokens or borrow balance in this market
        use soroban_sdk::IntoVal;
        let pbal: u128 = env.invoke_contract(
            &market,
            &Symbol::new(&env, "get_ptoken_balance"),
            (user.clone(),).into_val(&env),
        );
        if pbal > 0 {
            panic!("Cannot exit with collateral in market");
        }
        let debt: u128 = env.invoke_contract(
            &market,
            &Symbol::new(&env, "get_user_borrow_balance"),
            (user.clone(),).into_val(&env),
        );
        if debt > 0 {
            panic!("Cannot exit with outstanding debt");
        }
        if entered.contains(market.clone()) {
            // Remove first occurrence
            let mut new_vec = Vec::new(&env);
            for i in 0..entered.len() {
                let m = entered.get(i).unwrap();
                if m != market {
                    new_vec.push_back(m);
                }
            }
            env.storage()
                .persistent()
                .set(&DataKey::UserMarkets(user.clone()), &new_vec);
        }
        MarketExited {
            account: user.clone(),
            market,
        }
        .publish(&env);
    }

    pub fn remove_market(env: Env, market: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        admin.require_auth();
        let mut markets: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::SupportedMarkets)
            .unwrap_or(Map::new(&env));
        markets.remove(market.clone());
        env.storage()
            .persistent()
            .set(&DataKey::SupportedMarkets, &markets);
        MarketRemoved { market }.publish(&env);
    }

    // Sum collateral across user's entered markets using each market's exchange rate and pToken balance
    pub fn get_user_total_collateral(env: Env, user: Address) -> u128 {
        let mut total: u128 = 0u128;
        let markets = Self::get_user_markets(env.clone(), user.clone());
        for i in 0..markets.len() {
            let m = markets.get(i).unwrap();
            // dynamic client: simply call via env.invoke_contract for portability
            let pbal: u128 = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_ptoken_balance"),
                (user.clone(),).into_val(&env),
            );
            if pbal > 0 {
                let rate: u128 = env.invoke_contract(
                    &m,
                    &Symbol::new(&env, "get_exchange_rate"),
                    ().into_val(&env),
                );
                total = total.saturating_add((pbal.saturating_mul(rate)) / 1_000_000u128);
            }
        }
        total
    }

    // Sum collateral across markets excluding a specific market (to avoid re-entry from that market)
    pub fn get_collateral_excl(env: Env, user: Address, exclude_market: Address) -> u128 {
        let mut total: u128 = 0u128;
        let markets = Self::get_user_markets(env.clone(), user.clone());
        for i in 0..markets.len() {
            let m = markets.get(i).unwrap();
            if m == exclude_market {
                continue;
            }
            let pbal: u128 = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_ptoken_balance"),
                (user.clone(),).into_val(&env),
            );
            if pbal > 0 {
                let rate: u128 = env.invoke_contract(
                    &m,
                    &Symbol::new(&env, "get_exchange_rate"),
                    ().into_val(&env),
                );
                total = total.saturating_add((pbal.saturating_mul(rate)) / 1_000_000u128);
            }
        }
        total
    }

    // Sum borrows across user's entered markets
    pub fn get_user_total_borrows(env: Env, user: Address) -> u128 {
        let mut total: u128 = 0u128;
        let markets = Self::get_user_markets(env.clone(), user.clone());
        for i in 0..markets.len() {
            let m = markets.get(i).unwrap();
            let debt: u128 = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_user_borrow_balance"),
                (user.clone(),).into_val(&env),
            );
            total = total.saturating_add(debt);
        }
        total
    }

    // Sum borrows in USD across markets excluding a specific market
    pub fn get_borrows_excl(env: Env, user: Address, exclude_market: Address) -> u128 {
        let mut total: u128 = 0u128;
        let markets = Self::get_user_markets(env.clone(), user.clone());
        for i in 0..markets.len() {
            let m = markets.get(i).unwrap();
            if m == exclude_market {
                continue;
            }
            let debt: u128 = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_user_borrow_balance"),
                (user.clone(),).into_val(&env),
            );
            if debt == 0 {
                continue;
            }
            let token: Address = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_underlying_token"),
                ().into_val(&env),
            );
            let (price, scale) =
                Self::get_price_usd(env.clone(), token).expect("price unavailable");
            if price == 0 {
                panic!("price zero");
            }
            let usd = (debt.saturating_mul(price)) / scale;
            total = total.saturating_add(usd);
        }
        total
    }

    // Sum collateral in USD across markets excluding a specific market
    pub fn get_collateral_excl_usd(env: Env, user: Address, exclude_market: Address) -> u128 {
        let (_collateral_usd, _borrows) = Self::sum_positions_usd(env, user, Some(exclude_market));
        _collateral_usd
    }

    // Account liquidity in USD across all entered markets: (liquidity, shortfall)
    pub fn account_liquidity(env: Env, user: Address) -> (u128, u128) {
        let (_collateral_usd, borrow_usd) =
            Self::sum_positions_usd(env.clone(), user.clone(), None);
        if _collateral_usd >= borrow_usd {
            (_collateral_usd - borrow_usd, 0u128)
        } else {
            (0u128, borrow_usd - _collateral_usd)
        }
    }

    // Hypothetical liquidity after borrowing `borrow_amount` of `market` underlying
    pub fn hypothetical_liquidity(
        env: Env,
        user: Address,
        market: Address,
        borrow_amount: u128,
        underlying: Address,
    ) -> (u128, u128) {
        // Exclude current market to avoid re-entry from that market during borrow path
        let (collateral_usd, mut borrow_usd) =
            Self::sum_positions_usd(env.clone(), user.clone(), Some(market.clone()));
        // Add hypothetical borrow in USD using provided underlying token
        if let Some((price, scale)) = Self::get_price_usd(env.clone(), underlying.clone()) {
            if price == 0 {
                panic!("price zero");
            }
            let extra = (borrow_amount.saturating_mul(price)) / scale;
            borrow_usd = borrow_usd.saturating_add(extra);
        }
        if collateral_usd >= borrow_usd {
            (collateral_usd - borrow_usd, 0u128)
        } else {
            (0u128, borrow_usd - collateral_usd)
        }
    }

    // Preview the maximum additional borrow in underlying units for a given market
    pub fn preview_borrow_max(env: Env, user: Address, market: Address) -> u128 {
        // Account-level cushion in USD
        let (liquidity_usd, _shortfall) = Self::account_liquidity(env.clone(), user.clone());
        if liquidity_usd == 0 {
            return 0u128;
        }
        // Price of the market underlying
        use soroban_sdk::IntoVal;
        let underlying: Address = env.invoke_contract(
            &market,
            &Symbol::new(&env, "get_underlying_token"),
            ().into_val(&env),
        );
        let Some((price, scale)) = Self::get_price_usd(env.clone(), underlying.clone()) else {
            return 0u128;
        };
        if price == 0 {
            panic!("price zero");
        }
        // Convert USD cushion to underlying
        let by_collateral = (liquidity_usd.saturating_mul(scale)) / price;
        // Clamp by market available liquidity
        let available: u128 = env.invoke_contract(
            &market,
            &Symbol::new(&env, "get_available_liquidity"),
            ().into_val(&env),
        );
        if by_collateral < available {
            by_collateral
        } else {
            available
        }
    }

    // Preview the maximum redeemable pTokens from a given market without creating shortfall
    pub fn preview_redeem_max(env: Env, user: Address, market: Address) -> u128 {
        use soroban_sdk::IntoVal;
        // Totals in USD
        let (_collateral_usd, borrow_usd) =
            Self::sum_positions_usd(env.clone(), user.clone(), None);
        if borrow_usd == 0 {
            // no debt => can redeem all pTokens (subject to liquidity)
            let pbal: u128 = env.invoke_contract(
                &market,
                &Symbol::new(&env, "get_ptoken_balance"),
                (user.clone(),).into_val(&env),
            );
            // also clamp by available liquidity
            let rate: u128 = env.invoke_contract(
                &market,
                &Symbol::new(&env, "get_exchange_rate"),
                ().into_val(&env),
            );
            let available: u128 = env.invoke_contract(
                &market,
                &Symbol::new(&env, "get_available_liquidity"),
                ().into_val(&env),
            );
            let max_ptokens_by_liq = (available.saturating_mul(1_000_000u128)) / rate;
            return if pbal < max_ptokens_by_liq {
                pbal
            } else {
                max_ptokens_by_liq
            };
        }
        // Collateral in USD excluding current market
        let other_collateral_usd =
            Self::get_collateral_excl_usd(env.clone(), user.clone(), market.clone());
        // Required local discounted collateral in USD after redeem
        let required_local_discounted_usd = if borrow_usd > other_collateral_usd {
            borrow_usd - other_collateral_usd
        } else {
            0u128
        };
        // Local balances and params
        let pbal: u128 = env.invoke_contract(
            &market,
            &Symbol::new(&env, "get_ptoken_balance"),
            (user.clone(),).into_val(&env),
        );
        if pbal == 0 {
            return 0u128;
        }
        let rate: u128 = env.invoke_contract(
            &market,
            &Symbol::new(&env, "get_exchange_rate"),
            ().into_val(&env),
        );
        let cf: u128 = Self::get_market_cf(env.clone(), market.clone());
        let underlying_token: Address = env.invoke_contract(
            &market,
            &Symbol::new(&env, "get_underlying_token"),
            ().into_val(&env),
        );
        let Some((price, scale)) = Self::get_price_usd(env.clone(), underlying_token) else {
            return 0u128;
        };
        // Current local underlying
        let underlying_local = (pbal.saturating_mul(rate)) / 1_000_000u128;
        // Compute max redeemable underlying so that remaining discounted >= required
        // remaining_discounted_usd = (underlying_local - x) * cf/1e6 * price/scale >= required
        // => x <= underlying_local - required * 1e6 * scale / (cf * price)
        if cf == 0 || price == 0 {
            return 0u128;
        }
        let numerator = required_local_discounted_usd
            .saturating_mul(1_000_000u128)
            .saturating_mul(scale);
        let denom = cf.saturating_mul(price);
        let min_remaining_underlying = if denom == 0 {
            underlying_local
        } else {
            numerator / denom
        };
        let max_redeem_underlying_by_health = if underlying_local > min_remaining_underlying {
            underlying_local - min_remaining_underlying
        } else {
            0u128
        };
        // Also limited by market available liquidity
        let available_underlying: u128 = env.invoke_contract(
            &market,
            &Symbol::new(&env, "get_available_liquidity"),
            ().into_val(&env),
        );
        let clamped_underlying = if max_redeem_underlying_by_health < available_underlying {
            max_redeem_underlying_by_health
        } else {
            available_underlying
        };
        // Convert to pTokens: p = underlying * 1e6 / rate
        (clamped_underlying.saturating_mul(1_000_000u128)) / rate
    }

    // Preview maximum repay amount for a borrower on a given market (close factor cap)
    pub fn preview_repay_cap(env: Env, borrower: Address, repay_market: Address) -> u128 {
        use soroban_sdk::IntoVal;
        let close_factor: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::CloseFactorScaled)
            .unwrap_or(500_000u128);
        let debt: u128 = env.invoke_contract(
            &repay_market,
            &Symbol::new(&env, "get_user_borrow_balance"),
            (borrower,).into_val(&env),
        );
        (debt.saturating_mul(close_factor)) / 1_000_000u128
    }

    // Preview pTokens to seize for a given repay_amount
    pub fn preview_seize_ptokens(
        env: Env,
        repay_market: Address,
        collateral_market: Address,
        repay_amount: u128,
    ) -> u128 {
        use soroban_sdk::IntoVal;
        // tokens
        let borrow_token: Address = env.invoke_contract(
            &repay_market,
            &Symbol::new(&env, "get_underlying_token"),
            ().into_val(&env),
        );
        let coll_token: Address = env.invoke_contract(
            &collateral_market,
            &Symbol::new(&env, "get_underlying_token"),
            ().into_val(&env),
        );
        // prices
        let (pb, sb) = Self::get_price_usd(env.clone(), borrow_token).expect("price borrow");
        let (pc, sc) = Self::get_price_usd(env.clone(), coll_token).expect("price collat");
        if pb == 0 || pc == 0 {
            panic!("price zero");
        }
        let repay_usd = (repay_amount.saturating_mul(pb)) / sb;
        let li_scaled: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::LiquidationIncentiveScaled)
            .unwrap_or(1_080_000u128);
        let seize_underlying_usd = (repay_usd.saturating_mul(li_scaled)) / 1_000_000u128;
        let seize_underlying = (seize_underlying_usd.saturating_mul(sc)) / pc;
        let rate: u128 = env.invoke_contract(
            &collateral_market,
            &Symbol::new(&env, "get_exchange_rate"),
            ().into_val(&env),
        );
        (seize_underlying.saturating_mul(1_000_000u128)) / rate
    }

    // Liquidation entrypoint: liquidator repays on behalf and seizes collateral pTokens
    pub fn liquidate(
        env: Env,
        borrower: Address,
        repay_market: Address,
        collateral_market: Address,
        repay_amount: u128,
        liquidator: Address,
    ) {
        // top-level auth for liquidator, to allow token transfer from liquidator in nested calls
        liquidator.require_auth();
        if repay_market == collateral_market {
            panic!("invalid markets");
        }
        // Check pause flags
        if Self::is_liquidation_paused(env.clone(), repay_market.clone())
            || Self::is_liquidation_paused(env.clone(), collateral_market.clone())
        {
            panic!("liquidation paused");
        }
        // ensure borrower is undercollateralized
        let (_liq, shortfall) = Self::account_liquidity(env.clone(), borrower.clone());
        if shortfall == 0 {
            panic!("no shortfall");
        }

        // params
        let close_factor: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::CloseFactorScaled)
            .unwrap_or(500_000u128);
        let li_scaled: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::LiquidationIncentiveScaled)
            .unwrap_or(1_080_000u128);

        use soroban_sdk::IntoVal;
        // get borrower debt
        let debt: u128 = env.invoke_contract(
            &repay_market,
            &Symbol::new(&env, "get_user_borrow_balance"),
            (borrower.clone(),).into_val(&env),
        );
        if debt == 0 {
            panic!("no debt");
        }
        let max_repay = (debt.saturating_mul(close_factor)) / 1_000_000u128;
        let repay = if repay_amount > max_repay {
            max_repay
        } else {
            repay_amount
        };
        if repay == 0 {
            panic!("repay too small");
        }

        // tokens and prices
        let borrow_token: Address = env.invoke_contract(
            &repay_market,
            &Symbol::new(&env, "get_underlying_token"),
            ().into_val(&env),
        );
        let coll_token: Address = env.invoke_contract(
            &collateral_market,
            &Symbol::new(&env, "get_underlying_token"),
            ().into_val(&env),
        );
        let (pb, sb) =
            Self::get_price_usd(env.clone(), borrow_token.clone()).expect("price borrow");
        let (pc, sc) = Self::get_price_usd(env.clone(), coll_token.clone()).expect("price collat");
        if pb == 0 || pc == 0 {
            panic!("price zero");
        }
        let repay_usd = (repay.saturating_mul(pb)) / sb;
        let seize_underlying_usd = (repay_usd.saturating_mul(li_scaled)) / 1_000_000u128;
        let seize_underlying = (seize_underlying_usd.saturating_mul(sc)) / pc;
        let rate: u128 = env.invoke_contract(
            &collateral_market,
            &Symbol::new(&env, "get_exchange_rate"),
            ().into_val(&env),
        );
        let mut seize_ptokens = (seize_underlying.saturating_mul(1_000_000u128)) / rate;

        // perform repay on behalf and seize
        let _: () = env.invoke_contract(
            &repay_market,
            &Symbol::new(&env, "repay_on_behalf"),
            (liquidator.clone(), borrower.clone(), repay).into_val(&env),
        );
        // Clamp seize to available borrower pTokens to avoid over-seize panics
        let borrower_pbal: u128 = env.invoke_contract(
            &collateral_market,
            &Symbol::new(&env, "get_ptoken_balance"),
            (borrower.clone(),).into_val(&env),
        );
        if seize_ptokens > borrower_pbal {
            seize_ptokens = borrower_pbal;
        }
        // Route fee to reserve recipient if configured
        let liq_fee: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::LiquidationFeeScaled)
            .unwrap_or(0u128);
        if liq_fee > 0 {
            let fee_ptokens = (seize_ptokens.saturating_mul(liq_fee)) / 1_000_000u128;
            if fee_ptokens > 0 {
                if let Some(recipient) = env
                    .storage()
                    .persistent()
                    .get::<_, Address>(&DataKey::ReserveRecipient)
                {
                    let _: () = env.invoke_contract(
                        &collateral_market,
                        &Symbol::new(&env, "seize"),
                        (borrower.clone(), recipient, fee_ptokens).into_val(&env),
                    );
                }
            }
            let remainder = seize_ptokens
                .saturating_sub((seize_ptokens.saturating_mul(liq_fee)) / 1_000_000u128);
            if remainder > 0 {
                let _: () = env.invoke_contract(
                    &collateral_market,
                    &Symbol::new(&env, "seize"),
                    (borrower.clone(), liquidator.clone(), remainder).into_val(&env),
                );
            }
        } else {
            let _: () = env.invoke_contract(
                &collateral_market,
                &Symbol::new(&env, "seize"),
                (borrower.clone(), liquidator.clone(), seize_ptokens).into_val(&env),
            );
        }

        LiquidateBorrow {
            liquidator,
            borrower,
            repay_market,
            collateral_market,
            repay_amount: repay,
            seize_tokens: seize_ptokens,
        }
        .publish(&env);
    }

    // Claim accrued rewards and mint PERI to user
    pub fn claim(env: Env, user: Address) {
        // Accrue and distribute on all entered markets
        let markets = Self::get_user_markets(env.clone(), user.clone());
        for i in 0..markets.len() {
            let m = markets.get(i).unwrap();
            Self::accrue_market(env.clone(), m.clone());
            // distribute supply
            Self::distribute_supply(env.clone(), user.clone(), m.clone());
            // distribute borrow
            Self::distribute_borrow(env.clone(), user.clone(), m.clone());
        }
        let accrued: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::Accrued(user.clone()))
            .unwrap_or(0u128);
        if accrued == 0 {
            return;
        }
        if let Some(token) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::PeridotToken)
        {
            use soroban_sdk::IntoVal;
            let amt: i128 = if accrued > i128::MAX as u128 {
                i128::MAX
            } else {
                accrued as i128
            };
            let _: () = env.invoke_contract(
                &token,
                &Symbol::new(&env, "mint"),
                (user.clone(), amt).into_val(&env),
            );
        }
        env.storage()
            .persistent()
            .set(&DataKey::Accrued(user), &0u128);
    }

    pub fn get_accrued(env: Env, user: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::Accrued(user))
            .unwrap_or(0u128)
    }

    // Public: accrue indexes for a market and distribute to a single user (no mint)
    pub fn accrue_user_market(env: Env, user: Address, market: Address) {
        Self::accrue_market(env.clone(), market.clone());
        Self::distribute_supply(env.clone(), user.clone(), market.clone());
        Self::distribute_borrow(env, user, market);
    }

    fn sum_positions_usd(env: Env, user: Address, exclude_market: Option<Address>) -> (u128, u128) {
        let mut collateral_total: u128 = 0u128;
        let mut borrow_total: u128 = 0u128;
        let markets = Self::get_user_markets(env.clone(), user.clone());
        for i in 0..markets.len() {
            let m = markets.get(i).unwrap();
            if let Some(ex) = exclude_market.clone() {
                if m == ex {
                    continue;
                }
            }

            // Underlying token and price
            use soroban_sdk::IntoVal;
            let token: Address = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_underlying_token"),
                ().into_val(&env),
            );
            let pbal: u128 = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_ptoken_balance"),
                (user.clone(),).into_val(&env),
            );
            let debt: u128 = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_user_borrow_balance"),
                (user.clone(),).into_val(&env),
            );
            if pbal == 0 && debt == 0 {
                continue;
            }
            let (price, scale) =
                Self::get_price_usd(env.clone(), token.clone()).expect("price unavailable");
            if price == 0 {
                panic!("price zero");
            }

            // Collateral: pToken balance * exchange rate * collateral factor * price
            if pbal > 0 {
                let rate: u128 = env.invoke_contract(
                    &m,
                    &Symbol::new(&env, "get_exchange_rate"),
                    ().into_val(&env),
                );
                let cf: u128 = Self::get_market_cf(env.clone(), m.clone());
                let underlying_amount = (pbal.saturating_mul(rate)) / 1_000_000u128;
                let discounted = (underlying_amount.saturating_mul(cf)) / 1_000_000u128;
                let usd = (discounted.saturating_mul(price)) / scale;
                collateral_total = collateral_total.saturating_add(usd);
            }

            // Borrows: borrow balance * price
            if debt > 0 {
                let usd = (debt.saturating_mul(price)) / scale;
                borrow_total = borrow_total.saturating_add(usd);
            }
        }
        (collateral_total, borrow_total)
    }

    // Note: we avoid calling back into the current market during hypothetical checks to prevent re-entry

    // Price quotation via Reflector oracle (returns (price, 10^decimals)) if oracle set
    pub fn get_price_usd(env: Env, token: Address) -> Option<(u128, u128)> {
        let oracle: Option<Address> = env.storage().persistent().get(&DataKey::Oracle);
        let Some(oracle_addr) = oracle else {
            return None;
        };
        let client = crate::reflector::ReflectorClient::new(&env, &oracle_addr);
        let dec = client.decimals();
        let scale = pow10_u128(dec);
        let asset = match env
            .storage()
            .persistent()
            .get::<_, Symbol>(&DataKey::OracleAssetSymbol(token.clone()))
        {
            Some(sym) => crate::reflector::Asset::Other(sym),
            None => crate::reflector::Asset::Stellar(token),
        };
        let pd_opt = client.lastprice(&asset);
        match pd_opt {
            Some(pd) if pd.price >= 0 => {
                // Staleness check per Reflector best practices
                let res = client.resolution() as u64; // seconds
                let now = env.ledger().timestamp();
                // consider stale if older than k * resolution (configurable)
                let k: u64 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::OracleMaxAgeMultiplier)
                    .unwrap_or(2u64);
                let max_age = res.saturating_mul(k);
                if pd.timestamp + max_age < now {
                    return None;
                }
                Some((pd.price as u128, scale))
            }
            _ => None,
        }
    }
}

// Reflector oracle client interface
mod reflector;

fn pow10_u128(decimals: u32) -> u128 {
    // conservative pow that avoids overflow for reasonable decimals (<= 20)
    let mut result: u128 = 1;
    let mut i = 0u32;
    while i < decimals {
        result = result.saturating_mul(10);
        i += 1;
    }
    result
}

mod test;
fn require_admin(env: Env) {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");
    admin.require_auth();
}

// Rewards internals
const INDEX_SCALE_1E18: u128 = 1_000_000_000_000_000_000u128;

impl SimplePeridottroller {
    fn accrue_market(env: Env, market: Address) {
        let now = env.ledger().timestamp();
        // supply
        let last_s: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::SupplyIndexTime(market.clone()))
            .unwrap_or(now);
        let dt_s = now.saturating_sub(last_s);
        if dt_s > 0 {
            let speed: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::SupplySpeed(market.clone()))
                .unwrap_or(0u128);
            if speed > 0 {
                use soroban_sdk::IntoVal;
                let total_ptokens: u128 = env.invoke_contract(
                    &market,
                    &Symbol::new(&env, "get_total_ptokens"),
                    ().into_val(&env),
                );
                if total_ptokens > 0 {
                    let mut idx: u128 = env
                        .storage()
                        .persistent()
                        .get(&DataKey::SupplyIndex(market.clone()))
                        .unwrap_or(INDEX_SCALE_1E18);
                    let delta = ((speed.saturating_mul(dt_s as u128))
                        .saturating_mul(INDEX_SCALE_1E18))
                        / total_ptokens;
                    idx = idx.saturating_add(delta);
                    env.storage()
                        .persistent()
                        .set(&DataKey::SupplyIndex(market.clone()), &idx);
                }
            }
            env.storage()
                .persistent()
                .set(&DataKey::SupplyIndexTime(market.clone()), &now);
        }
        // borrow
        let last_b: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndexTime(market.clone()))
            .unwrap_or(now);
        let dt_b = now.saturating_sub(last_b);
        if dt_b > 0 {
            let speed: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::BorrowSpeed(market.clone()))
                .unwrap_or(0u128);
            if speed > 0 {
                use soroban_sdk::IntoVal;
                let total_borrowed: u128 = env.invoke_contract(
                    &market,
                    &Symbol::new(&env, "get_total_borrowed"),
                    ().into_val(&env),
                );
                if total_borrowed > 0 {
                    let mut idx: u128 = env
                        .storage()
                        .persistent()
                        .get(&DataKey::BorrowIndex(market.clone()))
                        .unwrap_or(INDEX_SCALE_1E18);
                    let delta = ((speed.saturating_mul(dt_b as u128))
                        .saturating_mul(INDEX_SCALE_1E18))
                        / total_borrowed;
                    idx = idx.saturating_add(delta);
                    env.storage()
                        .persistent()
                        .set(&DataKey::BorrowIndex(market.clone()), &idx);
                }
            }
            env.storage()
                .persistent()
                .set(&DataKey::BorrowIndexTime(market.clone()), &now);
        }
    }

    fn distribute_supply(env: Env, user: Address, market: Address) {
        use soroban_sdk::IntoVal;
        let idx: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::SupplyIndex(market.clone()))
            .unwrap_or(INDEX_SCALE_1E18);
        let uidx: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::UserSupplyIndex(user.clone(), market.clone()))
            .unwrap_or(INDEX_SCALE_1E18);
        if idx == uidx {
            return;
        }
        let pbal: u128 = env.invoke_contract(
            &market,
            &Symbol::new(&env, "get_ptoken_balance"),
            (user.clone(),).into_val(&env),
        );
        if pbal > 0 {
            let delta_index = idx.saturating_sub(uidx);
            let add = (pbal.saturating_mul(delta_index)) / INDEX_SCALE_1E18;
            let acc: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::Accrued(user.clone()))
                .unwrap_or(0u128);
            env.storage()
                .persistent()
                .set(&DataKey::Accrued(user.clone()), &acc.saturating_add(add));
        }
        env.storage()
            .persistent()
            .set(&DataKey::UserSupplyIndex(user, market), &idx);
    }

    fn distribute_borrow(env: Env, user: Address, market: Address) {
        use soroban_sdk::IntoVal;
        let idx: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowIndex(market.clone()))
            .unwrap_or(INDEX_SCALE_1E18);
        let uidx: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::UserBorrowIndex(user.clone(), market.clone()))
            .unwrap_or(INDEX_SCALE_1E18);
        if idx == uidx {
            return;
        }
        let debt: u128 = env.invoke_contract(
            &market,
            &Symbol::new(&env, "get_user_borrow_balance"),
            (user.clone(),).into_val(&env),
        );
        if debt > 0 {
            let delta_index = idx.saturating_sub(uidx);
            let add = (debt.saturating_mul(delta_index)) / INDEX_SCALE_1E18;
            let acc: u128 = env
                .storage()
                .persistent()
                .get(&DataKey::Accrued(user.clone()))
                .unwrap_or(0u128);
            env.storage()
                .persistent()
                .set(&DataKey::Accrued(user.clone()), &acc.saturating_add(add));
        }
        env.storage()
            .persistent()
            .set(&DataKey::UserBorrowIndex(user, market), &idx);
    }

    // UX: allow claiming for many users at once (permissionless)
    pub fn claim_all(env: Env, users: Vec<Address>) {
        for i in 0..users.len() {
            let u = users.get(i).unwrap();
            Self::claim(env.clone(), u);
        }
    }

    // UX: user-authenticated convenience for claiming own rewards
    pub fn claim_self(env: Env, user: Address) {
        user.require_auth();
        Self::claim(env, user);
    }

    // UX: portfolio view summarizing per-market balances and USD totals
    // Returns (per_market: Vec<(market, ptoken_balance, debt, collateral_usd, borrow_usd)>, totals: (collateral_usd, borrow_usd))
    pub fn portfolio(
        env: Env,
        user: Address,
    ) -> (Vec<(Address, u128, u128, u128, u128)>, (u128, u128)) {
        let mut rows: Vec<(Address, u128, u128, u128, u128)> = Vec::new(&env);
        let mut coll_total: u128 = 0u128;
        let mut debt_total: u128 = 0u128;
        let markets = Self::get_user_markets(env.clone(), user.clone());
        for i in 0..markets.len() {
            let m = markets.get(i).unwrap();
            use soroban_sdk::IntoVal;
            let pbal: u128 = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_ptoken_balance"),
                (user.clone(),).into_val(&env),
            );
            let debt: u128 = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_user_borrow_balance"),
                (user.clone(),).into_val(&env),
            );
            // defaults
            let mut coll_usd: u128 = 0u128;
            let mut debt_usd: u128 = 0u128;
            // price & rate
            let token: Address = env.invoke_contract(
                &m,
                &Symbol::new(&env, "get_underlying_token"),
                ().into_val(&env),
            );
            if pbal > 0 || debt > 0 {
                let (price, scale) =
                    Self::get_price_usd(env.clone(), token).expect("price unavailable");
                if price == 0 {
                    panic!("price zero");
                }
                if pbal > 0 {
                    let rate: u128 = env.invoke_contract(
                        &m,
                        &Symbol::new(&env, "get_exchange_rate"),
                        ().into_val(&env),
                    );
                    let cf: u128 = Self::get_market_cf(env.clone(), m.clone());
                    let underlying = (pbal.saturating_mul(rate)) / 1_000_000u128;
                    let discounted = (underlying.saturating_mul(cf)) / 1_000_000u128;
                    coll_usd = (discounted.saturating_mul(price)) / scale;
                }
                if debt > 0 {
                    debt_usd = (debt.saturating_mul(price)) / scale;
                }
            }
            rows.push_back((m, pbal, debt, coll_usd, debt_usd));
            coll_total = coll_total.saturating_add(coll_usd);
            debt_total = debt_total.saturating_add(debt_usd);
        }
        (rows, (coll_total, debt_total))
    }
}
