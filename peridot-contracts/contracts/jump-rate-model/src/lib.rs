#![no_std]
use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, Address, BytesN, Env, String,
};

const SCALE_1E6: u128 = 1_000_000u128;
pub const DEFAULT_INIT_ADMIN: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";
const TTL_THRESHOLD: u32 = 100_000_000;
const TTL_EXTEND_TO: u32 = 200_000_000;

#[contracttype]
pub enum DataKey {
    BaseRatePerYear,       // u128 scaled 1e6
    MultiplierPerYear,     // u128 scaled 1e6
    JumpMultiplierPerYear, // u128 scaled 1e6
    Kink,                  // u128 scaled 1e6
    Admin,                 // Address
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
    pub fn initialize(
        env: Env,
        base: u128,
        multiplier: u128,
        jump: u128,
        kink: u128,
        admin: Address,
    ) {
        if env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Admin)
            .is_some()
        {
            panic!("already initialized");
        }
        assert_expected_admin(&env, &admin);
        if kink > SCALE_1E6 {
            panic!("invalid kink");
        }
        if multiplier > 10_000_000u128 || jump > 10_000_000u128 {
            panic!("invalid rate params");
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
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
        bump_ttl(&env);
        ModelInitialized {
            base_rate: base,
            multiplier,
            jump_multiplier: jump,
            kink,
        }
        .publish(&env);
    }

    pub fn get_borrow_rate(env: Env, cash: u128, borrows: u128, reserves: u128) -> u128 {
        ensure_initialized(&env);
        bump_ttl(&env);
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
        ensure_initialized(&env);
        bump_ttl(&env);
        let one_minus_rf = SCALE_1E6.saturating_sub(reserve_factor);
        let borrow_rate = Self::get_borrow_rate(env.clone(), cash, borrows, reserves);
        let rate_to_pool = borrow_rate.saturating_mul(one_minus_rf) / SCALE_1E6;
        let util = Self::utilization(cash, borrows, reserves);
        util.saturating_mul(rate_to_pool) / SCALE_1E6
    }

    pub fn upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        require_admin(&env, &admin);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
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

fn ensure_initialized(env: &Env) {
    if env
        .storage()
        .persistent()
        .get::<_, Address>(&DataKey::Admin)
        .is_none()
    {
        panic!("model not initialized");
    }
}

fn assert_expected_admin(_env: &Env, _admin: &Address) {
    #[cfg(not(test))]
    {
        let expected_admin_str =
            option_env!("JUMP_RATE_MODEL_INIT_ADMIN").unwrap_or(DEFAULT_INIT_ADMIN);
        let expected_admin = Address::from_string(&String::from_str(_env, expected_admin_str));
        if _admin != &expected_admin {
            panic!("unexpected admin");
        }
    }
}

fn require_admin(env: &Env, admin: &Address) {
    let stored: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");
    bump_ttl(env);
    if stored != *admin {
        panic!("not admin");
    }
    admin.require_auth();
}

fn bump_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::Admin) {
        persistent.extend_ttl(&DataKey::Admin, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::BaseRatePerYear) {
        persistent.extend_ttl(&DataKey::BaseRatePerYear, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::MultiplierPerYear) {
        persistent.extend_ttl(&DataKey::MultiplierPerYear, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::JumpMultiplierPerYear) {
        persistent.extend_ttl(&DataKey::JumpMultiplierPerYear, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Kink) {
        persistent.extend_ttl(&DataKey::Kink, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn model_rates() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::from_string(&String::from_str(&env, DEFAULT_INIT_ADMIN));
        let id = env.register(JumpRateModel, ());
        let client = JumpRateModelClient::new(&env, &id);
        client.initialize(
            &20_000u128,
            &180_000u128,
            &4_000_000u128,
            &800_000u128,
            &admin,
        );
        let br_low = client.get_borrow_rate(&1_000u128, &100u128, &0u128);
        let br_high = client.get_borrow_rate(&10u128, &1_000u128, &0u128);
        assert!(br_high > br_low);
        let sr = client.get_supply_rate(&1_000u128, &500u128, &0u128, &100_000u128);
        assert!(sr > 0);
    }
}
