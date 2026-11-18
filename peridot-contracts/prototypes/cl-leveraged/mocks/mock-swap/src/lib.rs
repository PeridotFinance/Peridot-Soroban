#![no_std]
//! MOCK swap adapter `swap_pool` (contracts/swap-adapter). Executes at a fixture price
//! (token1 per token0, 14 dp) shifted by a test "push" in bps, paying from its own balance.
//! The input transfer is invoked by this adapter, so a contract user must pre-authorize it.
use soroban_sdk::{contract, contracttype, contractimpl, token, Address, Env};

#[contracttype]
enum K {
    T0,
    T1,
    Price,
    PushBps,
}

#[contract]
pub struct MockSwap;

#[contractimpl]
impl MockSwap {
    pub fn mock_init(e: Env, token0: Address, token1: Address, price_1e14: u128) {
        let s = e.storage().instance();
        s.set(&K::T0, &token0);
        s.set(&K::T1, &token1);
        s.set(&K::Price, &price_1e14);
        s.set(&K::PushBps, &0i32);
    }
    /// Test-only: move the executable price away from the oracle by `bps` (negative = worse for 0→1).
    pub fn mock_push(e: Env, bps: i32) {
        e.storage().instance().set(&K::PushBps, &bps);
    }
    pub fn swap_pool(e: Env, user: Address, _pool: Address, in_idx: u32, out_idx: u32, amount_in: u128, amount_out_min: u128) -> u128 {
        user.require_auth();
        let s = e.storage().instance();
        let (t0, t1): (Address, Address) = (s.get(&K::T0).unwrap(), s.get(&K::T1).unwrap());
        let price: u128 = s.get(&K::Price).unwrap();
        let push: i32 = s.get(&K::PushBps).unwrap();
        let p = (price as i128 * (10_000 + push as i128) / 10_000) as u128;
        let (tin, tout, out) = match (in_idx, out_idx) {
            (0, 1) => (t0, t1, amount_in * p / 100_000_000_000_000),
            (1, 0) => (t1, t0, amount_in * 100_000_000_000_000 / p),
            _ => panic!("bad index"),
        };
        assert!(out >= amount_out_min, "min out");
        token::Client::new(&e, &tin).transfer(&user, &e.current_contract_address(), &(amount_in as i128));
        token::Client::new(&e, &tout).transfer(&e.current_contract_address(), &user, &(out as i128));
        out
    }
}
