#![no_std]
use soroban_sdk::{contract, contractevent, contractimpl, contracttype, Env};

const SCALE_1E6: u128 = 1_000_000u128;

#[contracttype]
pub enum DataKey {
    BaseRatePerYear,       // u128 scaled 1e6
    MultiplierPerYear,     // u128 scaled 1e6
    JumpMultiplierPerYear, // u128 scaled 1e6
    Kink,                  // u128 scaled 1e6
}

#[contract]
pub struct JumpRateModel;

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelInitialized {
    pub base_rate: u128,
    pub multiplier: u128,
    pub jump_multiplier: u128,
    pub kink: u128,
}

#[contractimpl]
impl JumpRateModel {
    pub fn initialize(env: Env, base: u128, multiplier: u128, jump: u128, kink: u128) {
        env.storage()
            .persistent()
            .set(&DataKey::BaseRatePerYear, &base);
        env.storage()
            .persistent()
            .set(&DataKey::MultiplierPerYear, &multiplier);
        env.storage()
            .persistent()
            .set(&DataKey::JumpMultiplierPerYear, &jump);
        env.storage().persistent().set(&DataKey::Kink, &kink);
        ModelInitialized {
            base_rate: base,
            multiplier,
            jump_multiplier: jump,
            kink,
        }
        .publish(&env);
    }

    pub fn get_borrow_rate(env: Env, cash: u128, borrows: u128, reserves: u128) -> u128 {
        let util = Self::utilization(cash, borrows, reserves);
        let base: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::BaseRatePerYear)
            .unwrap_or(0);
        let mult: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::MultiplierPerYear)
            .unwrap_or(0);
        let jump: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::JumpMultiplierPerYear)
            .unwrap_or(0);
        let kink: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::Kink)
            .unwrap_or(SCALE_1E6 * 8 / 10);
        if util <= kink {
            base.saturating_add(util.saturating_mul(mult) / SCALE_1E6)
        } else {
            let normal = base.saturating_add(kink.saturating_mul(mult) / SCALE_1E6);
            let excess = util - kink;
            normal.saturating_add(excess.saturating_mul(jump) / SCALE_1E6)
        }
    }

    pub fn get_supply_rate(
        env: Env,
        cash: u128,
        borrows: u128,
        reserves: u128,
        reserve_factor: u128,
    ) -> u128 {
        let one_minus_rf = SCALE_1E6.saturating_sub(reserve_factor);
        let borrow_rate = Self::get_borrow_rate(env.clone(), cash, borrows, reserves);
        let rate_to_pool = borrow_rate.saturating_mul(one_minus_rf) / SCALE_1E6;
        let util = Self::utilization(cash, borrows, reserves);
        util.saturating_mul(rate_to_pool) / SCALE_1E6
    }

    fn utilization(cash: u128, borrows: u128, reserves: u128) -> u128 {
        if borrows == 0 {
            return 0;
        }
        let denom = cash.saturating_add(borrows).saturating_sub(reserves);
        if denom == 0 {
            return 0;
        }
        borrows.saturating_mul(SCALE_1E6) / denom
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn model_rates() {
        let env = Env::default();
        let id = env.register(JumpRateModel, ());
        let client = JumpRateModelClient::new(&env, &id);
        client.initialize(&20_000u128, &180_000u128, &4_000_000u128, &800_000u128);
        let br_low = client.get_borrow_rate(&1_000u128, &100u128, &0u128);
        let br_high = client.get_borrow_rate(&10u128, &1_000u128, &0u128);
        assert!(br_high > br_low);
        let sr = client.get_supply_rate(&1_000u128, &500u128, &0u128, &100_000u128);
        assert!(sr > 0);
    }
}
