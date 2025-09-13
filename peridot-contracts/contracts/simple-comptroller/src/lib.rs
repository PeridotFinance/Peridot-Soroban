#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, Symbol, Vec, IntoVal};


#[contracttype]
pub enum DataKey {
    Admin,
    SupportedMarkets,            // Map<Address, bool>
    UserMarkets(Address),        // Vec<Address>
    Oracle,                      // Address
}

#[contract]
pub struct SimpleComptroller;

#[contractimpl]
impl SimpleComptroller {
    pub fn initialize(env: Env, admin: Address) {
        env.storage().persistent().set(&DataKey::Admin, &admin);
        let markets: Map<Address, bool> = Map::new(&env);
        env.storage().persistent().set(&DataKey::SupportedMarkets, &markets);
    }

    pub fn set_oracle(env: Env, oracle: Address) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Oracle, &oracle);
        env.events().publish((Symbol::new(&env, "oracle_set"),), oracle);
    }

    pub fn get_oracle(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Oracle)
    }

    pub fn add_market(env: Env, market: Address) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        let mut markets: Map<Address, bool> = env.storage().persistent().get(&DataKey::SupportedMarkets).unwrap_or(Map::new(&env));
        markets.set(market.clone(), true);
        env.storage().persistent().set(&DataKey::SupportedMarkets, &markets);
        env.events().publish((Symbol::new(&env, "market_added"),), market);
    }

    pub fn enter_market(env: Env, user: Address, market: Address) {
        user.require_auth();
        let markets: Map<Address, bool> = env.storage().persistent().get(&DataKey::SupportedMarkets).unwrap_or(Map::new(&env));
        if markets.get(market.clone()).unwrap_or(false) == false { panic!("Market not supported"); }
        let mut entered: Vec<Address> = env.storage().persistent().get(&DataKey::UserMarkets(user.clone())).unwrap_or(Vec::new(&env));
        if !entered.contains(market.clone()) {
            entered.push_back(market.clone());
            env.storage().persistent().set(&DataKey::UserMarkets(user.clone()), &entered);
        }
        env.events().publish((Symbol::new(&env, "entered"), user.clone()), market);
    }

    pub fn get_user_markets(env: Env, user: Address) -> Vec<Address> {
        env.storage().persistent().get(&DataKey::UserMarkets(user)).unwrap_or(Vec::new(&env))
    }

    pub fn exit_market(env: Env, user: Address, market: Address) {
        user.require_auth();
        let mut entered: Vec<Address> = env.storage().persistent().get(&DataKey::UserMarkets(user.clone())).unwrap_or(Vec::new(&env));
        // Safety: block exit if user has pTokens or borrow balance in this market
        use soroban_sdk::IntoVal;
        let pbal: u128 = env.invoke_contract(&market, &Symbol::new(&env, "get_ptoken_balance"), (user.clone(),).into_val(&env));
        if pbal > 0 { panic!("Cannot exit with collateral in market"); }
        let debt: u128 = env.invoke_contract(&market, &Symbol::new(&env, "get_user_borrow_balance"), (user.clone(),).into_val(&env));
        if debt > 0 { panic!("Cannot exit with outstanding debt"); }
        if entered.contains(market.clone()) {
            // Remove first occurrence
            let mut new_vec = Vec::new(&env);
            for i in 0..entered.len() {
                let m = entered.get(i).unwrap();
                if m != market { new_vec.push_back(m); }
            }
            env.storage().persistent().set(&DataKey::UserMarkets(user.clone()), &new_vec);
        }
        env.events().publish((Symbol::new(&env, "exited"), user.clone()), market);
    }

    pub fn remove_market(env: Env, market: Address) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        let mut markets: Map<Address, bool> = env.storage().persistent().get(&DataKey::SupportedMarkets).unwrap_or(Map::new(&env));
        markets.remove(market.clone());
        env.storage().persistent().set(&DataKey::SupportedMarkets, &markets);
        env.events().publish((Symbol::new(&env, "market_removed"),), market);
    }

    // Sum collateral across user's entered markets using each market's exchange rate and pToken balance
    pub fn get_user_total_collateral(env: Env, user: Address) -> u128 {
        let mut total: u128 = 0u128;
        let markets = Self::get_user_markets(env.clone(), user.clone());
        for i in 0..markets.len() {
            let m = markets.get(i).unwrap();
            // dynamic client: simply call via env.invoke_contract for portability
            let pbal: u128 = env.invoke_contract(&m, &Symbol::new(&env, "get_ptoken_balance"), (user.clone(),).into_val(&env));
            if pbal > 0 {
                let rate: u128 = env.invoke_contract(&m, &Symbol::new(&env, "get_exchange_rate"), ().into_val(&env));
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
            if m == exclude_market { continue; }
            let pbal: u128 = env.invoke_contract(&m, &Symbol::new(&env, "get_ptoken_balance"), (user.clone(),).into_val(&env));
            if pbal > 0 {
                let rate: u128 = env.invoke_contract(&m, &Symbol::new(&env, "get_exchange_rate"), ().into_val(&env));
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
            let debt: u128 = env.invoke_contract(&m, &Symbol::new(&env, "get_user_borrow_balance"), (user.clone(),).into_val(&env));
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
            if m == exclude_market { continue; }
            let debt: u128 = env.invoke_contract(&m, &Symbol::new(&env, "get_user_borrow_balance"), (user.clone(),).into_val(&env));
            if debt == 0 { continue; }
            let token: Address = env.invoke_contract(&m, &Symbol::new(&env, "get_underlying_token"), ().into_val(&env));
            if let Some(price_usd) = Self::get_price_usd(env.clone(), token) {
                let (price, scale) = price_usd;
                let usd = (debt.saturating_mul(price)) / scale;
                total = total.saturating_add(usd);
            }
        }
        total
    }

    // Sum collateral in USD across markets excluding a specific market
    pub fn get_collateral_excl_usd(env: Env, user: Address, exclude_market: Address) -> u128 {
        let (collateral_usd, _borrows) = Self::sum_positions_usd(env, user, Some(exclude_market));
        collateral_usd
    }

    // Account liquidity in USD across all entered markets: (liquidity, shortfall)
    pub fn account_liquidity(env: Env, user: Address) -> (u128, u128) {
        let (collateral_usd, borrow_usd) = Self::sum_positions_usd(env.clone(), user.clone(), None);
        if collateral_usd >= borrow_usd {
            (collateral_usd - borrow_usd, 0u128)
        } else {
            (0u128, borrow_usd - collateral_usd)
        }
    }

    // Hypothetical liquidity after borrowing `borrow_amount` of `market` underlying
    pub fn hypothetical_liquidity(env: Env, user: Address, market: Address, borrow_amount: u128, underlying: Address) -> (u128, u128) {
        // Exclude current market to avoid re-entry
        let (mut collateral_usd, mut borrow_usd) = Self::sum_positions_usd(env.clone(), user.clone(), Some(market.clone()));
        // Add hypothetical borrow in USD using provided underlying token
        if let Some((price, scale)) = Self::get_price_usd(env.clone(), underlying.clone()) {
            let extra = (borrow_amount.saturating_mul(price)) / scale;
            borrow_usd = borrow_usd.saturating_add(extra);
        }
        if collateral_usd >= borrow_usd {
            (collateral_usd - borrow_usd, 0u128)
        } else {
            (0u128, borrow_usd - collateral_usd)
        }
    }

    fn sum_positions_usd(env: Env, user: Address, exclude_market: Option<Address>) -> (u128, u128) {
        let mut collateral_total: u128 = 0u128;
        let mut borrow_total: u128 = 0u128;
        let markets = Self::get_user_markets(env.clone(), user.clone());
        for i in 0..markets.len() {
            let m = markets.get(i).unwrap();
            if let Some(ex) = exclude_market.clone() { if m == ex { continue; } }

            // Underlying token and price
            use soroban_sdk::IntoVal;
            let token: Address = env.invoke_contract(&m, &Symbol::new(&env, "get_underlying_token"), ().into_val(&env));
            let price_opt = Self::get_price_usd(env.clone(), token.clone());
            if price_opt.is_none() { continue; }
            let (price, scale) = price_opt.unwrap();

            // Collateral: pToken balance * exchange rate * collateral factor * price
            let pbal: u128 = env.invoke_contract(&m, &Symbol::new(&env, "get_ptoken_balance"), (user.clone(),).into_val(&env));
            if pbal > 0 {
                let rate: u128 = env.invoke_contract(&m, &Symbol::new(&env, "get_exchange_rate"), ().into_val(&env));
                let cf: u128 = env.invoke_contract(&m, &Symbol::new(&env, "get_collateral_factor"), ().into_val(&env));
                let underlying_amount = (pbal.saturating_mul(rate)) / 1_000_000u128;
                let discounted = (underlying_amount.saturating_mul(cf)) / 1_000_000u128;
                let usd = (discounted.saturating_mul(price)) / scale;
                collateral_total = collateral_total.saturating_add(usd);
            }

            // Borrows: borrow balance * price
            let debt: u128 = env.invoke_contract(&m, &Symbol::new(&env, "get_user_borrow_balance"), (user.clone(),).into_val(&env));
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
        let Some(oracle_addr) = oracle else { return None; };
        let client = crate::reflector::ReflectorClient::new(&env, &oracle_addr);
        let dec = client.decimals();
        let scale = pow10_u128(dec);
        let asset = crate::reflector::Asset::Stellar(token);
        let pd_opt = client.lastprice(&asset);
        match pd_opt {
            Some(pd) if pd.price >= 0 => {
                // Staleness check per Reflector best practices
                let res = client.resolution() as u64; // seconds
                let now = env.ledger().timestamp();
                // consider stale if older than 2 * resolution
                let max_age = res.saturating_mul(2);
                if pd.timestamp + max_age < now { return None; }
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


