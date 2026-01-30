#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Vec};

#[soroban_sdk::contractclient(name = "AquariusRouterClient")]
pub trait AquariusRouter {
    fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        token_in: Address,
        in_amount: u128,
        out_min: u128,
    ) -> u128;
}

#[contracttype]
pub enum DataKey {
    Admin,
    Router,
}

#[contract]
pub struct SwapAdapter;

#[contractimpl]
impl SwapAdapter {
    pub fn initialize(env: Env, admin: Address, router: Address) {
        if env.storage().persistent().get::<_, Address>(&DataKey::Admin).is_some() {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::Router, &router);
    }

    pub fn set_router(env: Env, admin: Address, router: Address) {
        require_admin(&env, &admin);
        env.storage().persistent().set(&DataKey::Router, &router);
    }

    pub fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        token_in: Address,
        in_amount: u128,
        out_min: u128,
    ) -> u128 {
        let router: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Router)
            .expect("router not set");
        AquariusRouterClient::new(&env, &router).swap_chained(
            &user,
            &swaps_chain,
            &token_in,
            &in_amount,
            &out_min,
        )
    }
}

fn require_admin(env: &Env, admin: &Address) {
    let stored: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");
    if stored != *admin {
        panic!("not admin");
    }
    admin.require_auth();
}
