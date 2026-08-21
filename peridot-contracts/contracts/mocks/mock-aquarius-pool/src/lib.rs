#![no_std]
//! Test double for an Aquarius concentrated-liquidity pool.
//!
//! Models only the surface `aquarius-lp-vault` touches. A full-range
//! concentrated position is mathematically a constant-product position
//! (`r0 = L/sqrt(P)`, `r1 = L*sqrt(P)` implies `L = sqrt(r0*r1)`), so the
//! reserve math here is plain `x*y=k` — which is exactly what makes it a
//! faithful stand-in for the vault's full-range NAV formula.

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, Map, Symbol, Vec, U256,
};

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
#[derive(Clone)]
enum Key {
    Token0,
    Token1,
    Reserve0,
    Reserve1,
    TotalLiquidity,
    Liquidity(Address),
    TickSpacing,
    FeeBps,
    PendingFees0(Address),
    PendingFees1(Address),
    PendingAqua(Address),
    AquaToken,
    GaugeToken,
    PendingGauge(Address),
    FailWithdraw,
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let bits = 128 - n.leading_zeros();
    let mut x = 1u128 << bits.div_ceil(2);
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            break;
        }
        x = y;
    }
    x
}

fn get_u128(env: &Env, key: Key) -> u128 {
    env.storage().persistent().get(&key).unwrap_or(0u128)
}

fn set_u128(env: &Env, key: Key, v: u128) {
    env.storage().persistent().set(&key, &v);
}

fn addr(env: &Env, key: Key) -> Address {
    env.storage().persistent().get(&key).expect("not set")
}

#[contract]
pub struct MockAquariusPool;

#[contractimpl]
impl MockAquariusPool {
    pub fn initialize(env: Env, token0: Address, token1: Address, tick_spacing: i32, fee_bps: u32) {
        env.storage().persistent().set(&Key::Token0, &token0);
        env.storage().persistent().set(&Key::Token1, &token1);
        env.storage()
            .persistent()
            .set(&Key::TickSpacing, &tick_spacing);
        env.storage().persistent().set(&Key::FeeBps, &fee_bps);
        set_u128(&env, Key::Reserve0, 0);
        set_u128(&env, Key::Reserve1, 0);
        set_u128(&env, Key::TotalLiquidity, 0);
    }

    // ── Test controls ─────────────────────────────────────────────────────

    pub fn set_reward_tokens(env: Env, aqua: Address, gauge: Address) {
        env.storage().persistent().set(&Key::AquaToken, &aqua);
        env.storage().persistent().set(&Key::GaugeToken, &gauge);
    }

    pub fn credit_rewards(env: Env, user: Address, aqua: u128, gauge: u128) {
        set_u128(&env, Key::PendingAqua(user.clone()), aqua);
        set_u128(&env, Key::PendingGauge(user), gauge);
    }

    pub fn credit_fees(env: Env, user: Address, fee0: u128, fee1: u128) {
        set_u128(&env, Key::PendingFees0(user.clone()), fee0);
        set_u128(&env, Key::PendingFees1(user), fee1);
    }

    pub fn set_fail_withdraw(env: Env, fail: bool) {
        env.storage().persistent().set(&Key::FailWithdraw, &fail);
    }

    /// Simulates trading profit accruing to the pool without minting shares.
    pub fn donate(env: Env, from: Address, amount0: u128, amount1: u128) {
        let t0 = addr(&env, Key::Token0);
        let t1 = addr(&env, Key::Token1);
        let me = env.current_contract_address();
        if amount0 > 0 {
            token::TokenClient::new(&env, &t0).transfer(&from, &me, &(amount0 as i128));
            set_u128(&env, Key::Reserve0, get_u128(&env, Key::Reserve0) + amount0);
        }
        if amount1 > 0 {
            token::TokenClient::new(&env, &t1).transfer(&from, &me, &(amount1 as i128));
            set_u128(&env, Key::Reserve1, get_u128(&env, Key::Reserve1) + amount1);
        }
    }

    // ── Views ─────────────────────────────────────────────────────────────

    pub fn pool_type(env: Env) -> Symbol {
        Symbol::new(&env, "concentrated")
    }

    pub fn get_tokens(env: Env) -> Vec<Address> {
        let mut v = Vec::new(&env);
        v.push_back(addr(&env, Key::Token0));
        v.push_back(addr(&env, Key::Token1));
        v
    }

    pub fn get_tick_spacing(env: Env) -> i32 {
        env.storage()
            .persistent()
            .get(&Key::TickSpacing)
            .unwrap_or(60)
    }

    pub fn get_reserves(env: Env) -> Vec<u128> {
        let mut v = Vec::new(&env);
        v.push_back(get_u128(&env, Key::Reserve0));
        v.push_back(get_u128(&env, Key::Reserve1));
        v
    }

    pub fn get_slot0(env: Env) -> Slot0 {
        Slot0 {
            sqrt_price_x96: U256::from_u32(&env, 0),
            tick: 0,
        }
    }

    pub fn get_user_position_snapshot(env: Env, user: Address) -> UserPositionSnapshot {
        UserPositionSnapshot {
            ranges: Vec::new(&env),
            raw_liquidity: get_u128(&env, Key::Liquidity(user)),
            weighted_liquidity: 0,
        }
    }

    // ── Liquidity ─────────────────────────────────────────────────────────

    fn quote_deposit(env: &Env, desired: &Vec<u128>) -> (Vec<u128>, u128) {
        let d0 = desired.get(0).unwrap_or(0);
        let d1 = desired.get(1).unwrap_or(0);
        let r0 = get_u128(env, Key::Reserve0);
        let r1 = get_u128(env, Key::Reserve1);
        let total = get_u128(env, Key::TotalLiquidity);

        let mut out = Vec::new(env);
        if d0 == 0 || d1 == 0 {
            // Matches the real contract: a full-range position needs both legs.
            panic!("AllCoinsRequired");
        }
        if total == 0 || r0 == 0 || r1 == 0 {
            out.push_back(d0);
            out.push_back(d1);
            return (out, isqrt(d0.saturating_mul(d1)));
        }
        // Take both legs at the current reserve ratio, capped by what was
        // offered, and refund the rest by simply not taking it.
        let liq_from_0 = d0.saturating_mul(total) / r0;
        let liq_from_1 = d1.saturating_mul(total) / r1;
        let liq = liq_from_0.min(liq_from_1);
        let a0 = liq.saturating_mul(r0) / total;
        let a1 = liq.saturating_mul(r1) / total;
        out.push_back(a0);
        out.push_back(a1);
        (out, liq)
    }

    pub fn estimate_deposit_position(
        env: Env,
        _tick_lower: i32,
        _tick_upper: i32,
        desired_amounts: Vec<u128>,
    ) -> (Vec<u128>, u128) {
        Self::quote_deposit(&env, &desired_amounts)
    }

    pub fn deposit_position(
        env: Env,
        sender: Address,
        _tick_lower: i32,
        _tick_upper: i32,
        desired_amounts: Vec<u128>,
        min_liquidity: u128,
    ) -> (Vec<u128>, u128) {
        // Deliberately no `sender.require_auth()`. The deployed Aquarius pool
        // does not authorize the position call itself — it relies on the token
        // transfers below carrying the sender's authorization. An earlier
        // version of this mock required auth here, which made the vault's
        // auth tree look correct in tests while failing on-chain.
        let (actual, liq) = Self::quote_deposit(&env, &desired_amounts);
        if liq < min_liquidity {
            panic!("OutMinNotSatisfied");
        }
        let a0 = actual.get(0).unwrap_or(0);
        let a1 = actual.get(1).unwrap_or(0);
        let me = env.current_contract_address();
        if a0 > 0 {
            token::TokenClient::new(&env, &addr(&env, Key::Token0)).transfer(
                &sender,
                &me,
                &(a0 as i128),
            );
        }
        if a1 > 0 {
            token::TokenClient::new(&env, &addr(&env, Key::Token1)).transfer(
                &sender,
                &me,
                &(a1 as i128),
            );
        }
        set_u128(&env, Key::Reserve0, get_u128(&env, Key::Reserve0) + a0);
        set_u128(&env, Key::Reserve1, get_u128(&env, Key::Reserve1) + a1);
        set_u128(
            &env,
            Key::TotalLiquidity,
            get_u128(&env, Key::TotalLiquidity) + liq,
        );
        set_u128(
            &env,
            Key::Liquidity(sender.clone()),
            get_u128(&env, Key::Liquidity(sender)) + liq,
        );
        (actual, liq)
    }

    fn quote_withdraw(env: &Env, amount: u128) -> Vec<u128> {
        let total = get_u128(env, Key::TotalLiquidity);
        let mut out = Vec::new(env);
        if total == 0 {
            out.push_back(0);
            out.push_back(0);
            return out;
        }
        out.push_back(amount.saturating_mul(get_u128(env, Key::Reserve0)) / total);
        out.push_back(amount.saturating_mul(get_u128(env, Key::Reserve1)) / total);
        out
    }

    pub fn estimate_withdraw_position(
        env: Env,
        _owner: Address,
        _tick_lower: i32,
        _tick_upper: i32,
        amount: u128,
    ) -> Vec<u128> {
        Self::quote_withdraw(&env, amount)
    }

    pub fn withdraw_position(
        env: Env,
        owner: Address,
        _tick_lower: i32,
        _tick_upper: i32,
        amount: u128,
        min_amounts: Vec<u128>,
    ) -> Vec<u128> {
        owner.require_auth();
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&Key::FailWithdraw)
            .unwrap_or(false)
        {
            panic!("withdraw disabled");
        }
        let held = get_u128(&env, Key::Liquidity(owner.clone()));
        if amount > held {
            panic!("InsufficientLiquidity");
        }
        let amounts = Self::quote_withdraw(&env, amount);
        let a0 = amounts.get(0).unwrap_or(0);
        let a1 = amounts.get(1).unwrap_or(0);
        if a0 < min_amounts.get(0).unwrap_or(0) || a1 < min_amounts.get(1).unwrap_or(0) {
            panic!("OutMinNotSatisfied");
        }
        let me = env.current_contract_address();
        if a0 > 0 {
            token::TokenClient::new(&env, &addr(&env, Key::Token0)).transfer(
                &me,
                &owner,
                &(a0 as i128),
            );
        }
        if a1 > 0 {
            token::TokenClient::new(&env, &addr(&env, Key::Token1)).transfer(
                &me,
                &owner,
                &(a1 as i128),
            );
        }
        set_u128(&env, Key::Reserve0, get_u128(&env, Key::Reserve0) - a0);
        set_u128(&env, Key::Reserve1, get_u128(&env, Key::Reserve1) - a1);
        set_u128(
            &env,
            Key::TotalLiquidity,
            get_u128(&env, Key::TotalLiquidity) - amount,
        );
        set_u128(&env, Key::Liquidity(owner), held - amount);
        amounts
    }

    // ── Swaps ─────────────────────────────────────────────────────────────

    pub fn estimate_swap(env: Env, in_idx: u32, _out_idx: u32, in_amount: u128) -> u128 {
        let (ri, ro) = if in_idx == 0 {
            (get_u128(&env, Key::Reserve0), get_u128(&env, Key::Reserve1))
        } else {
            (get_u128(&env, Key::Reserve1), get_u128(&env, Key::Reserve0))
        };
        if ri == 0 || ro == 0 || in_amount == 0 {
            return 0;
        }
        let fee_bps: u32 = env.storage().persistent().get(&Key::FeeBps).unwrap_or(30);
        let in_after_fee = in_amount.saturating_mul((10_000 - fee_bps) as u128) / 10_000;
        ro.saturating_mul(in_after_fee) / (ri + in_after_fee)
    }

    pub fn swap(
        env: Env,
        user: Address,
        in_idx: u32,
        out_idx: u32,
        in_amount: u128,
        out_min: u128,
    ) -> u128 {
        // No `user.require_auth()` — matches the deployed pool; the inner
        // token transfer is what carries authorization.
        let out = Self::estimate_swap(env.clone(), in_idx, out_idx, in_amount);
        if out < out_min {
            panic!("OutMinNotSatisfied");
        }
        let t_in = if in_idx == 0 {
            Key::Token0
        } else {
            Key::Token1
        };
        let t_out = if out_idx == 0 {
            Key::Token0
        } else {
            Key::Token1
        };
        let me = env.current_contract_address();
        token::TokenClient::new(&env, &addr(&env, t_in)).transfer(&user, &me, &(in_amount as i128));
        token::TokenClient::new(&env, &addr(&env, t_out)).transfer(&me, &user, &(out as i128));
        let (k_in, k_out) = if in_idx == 0 {
            (Key::Reserve0, Key::Reserve1)
        } else {
            (Key::Reserve1, Key::Reserve0)
        };
        let ri = get_u128(&env, k_in.clone());
        set_u128(&env, k_in, ri + in_amount);
        let ro = get_u128(&env, k_out.clone());
        set_u128(&env, k_out, ro.saturating_sub(out));
        out
    }

    // ── Rewards ───────────────────────────────────────────────────────────

    pub fn get_user_reward(env: Env, user: Address) -> u128 {
        get_u128(&env, Key::PendingAqua(user))
    }

    pub fn claim(env: Env, user: Address) -> u128 {
        user.require_auth();
        let amount = get_u128(&env, Key::PendingAqua(user.clone()));
        if amount == 0 {
            return 0;
        }
        set_u128(&env, Key::PendingAqua(user.clone()), 0);
        let aqua = addr(&env, Key::AquaToken);
        token::TokenClient::new(&env, &aqua).transfer(
            &env.current_contract_address(),
            &user,
            &(amount as i128),
        );
        amount
    }

    pub fn get_gauges(env: Env) -> Map<Address, Address> {
        let mut m = Map::new(&env);
        if let Some(g) = env
            .storage()
            .persistent()
            .get::<_, Address>(&Key::GaugeToken)
        {
            m.set(g.clone(), g);
        }
        m
    }

    pub fn gauges_claim(env: Env, user: Address) -> Map<Address, u128> {
        user.require_auth();
        let mut m = Map::new(&env);
        let amount = get_u128(&env, Key::PendingGauge(user.clone()));
        let Some(gauge) = env
            .storage()
            .persistent()
            .get::<_, Address>(&Key::GaugeToken)
        else {
            return m;
        };
        if amount > 0 {
            set_u128(&env, Key::PendingGauge(user.clone()), 0);
            token::TokenClient::new(&env, &gauge).transfer(
                &env.current_contract_address(),
                &user,
                &(amount as i128),
            );
        }
        m.set(gauge, amount);
        m
    }

    pub fn get_all_position_fees(env: Env, owner: Address) -> Vec<u128> {
        let mut v = Vec::new(&env);
        v.push_back(get_u128(&env, Key::PendingFees0(owner.clone())));
        v.push_back(get_u128(&env, Key::PendingFees1(owner)));
        v
    }

    pub fn claim_all_position_fees(env: Env, owner: Address) -> Vec<u128> {
        owner.require_auth();
        let f0 = get_u128(&env, Key::PendingFees0(owner.clone()));
        let f1 = get_u128(&env, Key::PendingFees1(owner.clone()));
        set_u128(&env, Key::PendingFees0(owner.clone()), 0);
        set_u128(&env, Key::PendingFees1(owner.clone()), 0);
        let me = env.current_contract_address();
        if f0 > 0 {
            token::TokenClient::new(&env, &addr(&env, Key::Token0)).transfer(
                &me,
                &owner,
                &(f0 as i128),
            );
            set_u128(
                &env,
                Key::Reserve0,
                get_u128(&env, Key::Reserve0).saturating_sub(f0),
            );
        }
        if f1 > 0 {
            token::TokenClient::new(&env, &addr(&env, Key::Token1)).transfer(
                &me,
                &owner,
                &(f1 as i128),
            );
            set_u128(
                &env,
                Key::Reserve1,
                get_u128(&env, Key::Reserve1).saturating_sub(f1),
            );
        }
        let mut v = Vec::new(&env);
        v.push_back(f0);
        v.push_back(f1);
        v
    }
}
