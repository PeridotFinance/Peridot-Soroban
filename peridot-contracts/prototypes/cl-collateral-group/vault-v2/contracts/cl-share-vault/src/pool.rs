#![allow(dead_code)]
//! Client for the Aquarius concentrated-liquidity pool contract.
//!
//! Signatures and struct layouts are transcribed from the deployed mainnet
//! pool spec (`stellar contract info interface`), not from the docs, so the
//! XDR encoding matches exactly.

use soroban_sdk::{contracttype, Address, Env, Map, Symbol, Vec, U256};

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Slot0 {
    pub sqrt_price_x96: U256,
    pub tick: i32,
}

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PositionRange {
    pub tick_lower: i32,
    pub tick_upper: i32,
}

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct UserPositionSnapshot {
    pub ranges: Vec<PositionRange>,
    pub raw_liquidity: u128,
    pub weighted_liquidity: u128,
}

#[contracttype]
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PositionData {
    pub fee_growth_inside_0_last_x128: U256,
    pub fee_growth_inside_1_last_x128: U256,
    pub liquidity: u128,
    pub tokens_owed_0: u128,
    pub tokens_owed_1: u128,
}

#[soroban_sdk::contractclient(name = "ConcentratedPoolClient")]
pub trait ConcentratedPool {
    // ── Position management ────────────────────────────────────────────────
    fn deposit_position(
        env: Env,
        sender: Address,
        tick_lower: i32,
        tick_upper: i32,
        desired_amounts: Vec<u128>,
        min_liquidity: u128,
    ) -> (Vec<u128>, u128);

    fn withdraw_position(
        env: Env,
        owner: Address,
        tick_lower: i32,
        tick_upper: i32,
        amount: u128,
        min_amounts: Vec<u128>,
    ) -> Vec<u128>;

    fn estimate_deposit_position(
        env: Env,
        tick_lower: i32,
        tick_upper: i32,
        desired_amounts: Vec<u128>,
    ) -> (Vec<u128>, u128);

    fn estimate_withdraw_position(
        env: Env,
        owner: Address,
        tick_lower: i32,
        tick_upper: i32,
        amount: u128,
    ) -> Vec<u128>;

    // ── Swaps (used to rebalance one-sided inflows/outflows) ───────────────
    fn swap(
        env: Env,
        user: Address,
        in_idx: u32,
        out_idx: u32,
        in_amount: u128,
        out_min: u128,
    ) -> u128;

    fn estimate_swap(env: Env, in_idx: u32, out_idx: u32, in_amount: u128) -> u128;

    // ── Rewards ───────────────────────────────────────────────────────────
    /// AQUA emissions.
    fn claim(env: Env, user: Address) -> u128;
    fn get_user_reward(env: Env, user: Address) -> u128;
    /// Third-party pool incentives, keyed by reward token.
    fn gauges_claim(env: Env, user: Address) -> Map<Address, u128>;
    fn get_gauges(env: Env) -> Map<Address, Address>;

    // ── State ─────────────────────────────────────────────────────────────
    fn get_slot0(env: Env) -> Slot0;
    fn get_tick_spacing(env: Env) -> i32;
    fn get_tokens(env: Env) -> Vec<Address>;
    fn get_reserves(env: Env) -> Vec<u128>;
    fn pool_type(env: Env) -> Symbol;
    fn get_user_position_snapshot(env: Env, user: Address) -> UserPositionSnapshot;
    fn get_all_position_fees(env: Env, owner: Address) -> Vec<u128>;

    // ── Aquarius kill switches (errors 205 / 206) ─────────────────────────
    // The pool admin can pause deposits and swaps. `withdraw_position`
    // deliberately has none — Aquarius guarantees principal is always
    // recoverable — so there is no corresponding read for it.
    fn get_is_killed_deposit(env: Env) -> bool;
    fn get_is_killed_swap(env: Env) -> bool;
    fn claim_all_position_fees(env: Env, owner: Address) -> Vec<u128>;
}
