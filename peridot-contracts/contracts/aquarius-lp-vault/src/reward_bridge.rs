//! Native-only receipt-authorized reward bridge. No production activation.
//!
//! Claims export only measured NEW non-pair rewards. Legacy inventory and pool
//! fees need an explicit ownership migration, never an implicit sweep. Exact
//! swaps pull only the receipt's requested amount and return only newly produced
//! settlement tokens; failed calls must be caught by the receipt coordinator to
//! defer the reward leg without blocking an otherwise valid principal exit.
use crate::constants::REWARD_RATE_SCALE;
use crate::contract::auth_entry;
use crate::math::{apply_slippage_floor, mul_div, to_i128};
use crate::storage::{
    bound_receipt_vault, bump_critical_ttl, config, params, primary_reward_token, DataKey,
};
use crate::AquariusLpVault;
use soroban_sdk::{contracttype, token, Address, Env, IntoVal, Map, Symbol, Vec};

const MAX_REWARD_TOKENS: u32 = 4;

#[contracttype]
#[derive(Clone)]
enum BridgeKey {
    UnprovenPrimary,
}

fn receipt(env: &Env) -> Address {
    bump_critical_ttl(env);
    let receipt = bound_receipt_vault(env).expect("receipt not bound");
    receipt.require_auth();
    receipt
}

fn balance(env: &Env, asset: &Address, owner: &Address) -> u128 {
    token::Client::new(env, asset)
        .balance(owner)
        .try_into()
        .expect("negative balance")
}

fn delta(after: u128, before: u128) -> u128 {
    after.checked_sub(before).expect("reward balance decreased")
}

fn call<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    target: &Address,
    name: &str,
) -> T {
    let args: Vec<soroban_sdk::Val> = (env.current_contract_address(),).into_val(env);
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        auth_entry(env, target, name, args.clone(), Vec::new(env))
    ]);
    env.invoke_contract(target, &Symbol::new(env, name), args)
}

pub(crate) struct RewardBridge;

impl RewardBridge {
    /// Independently observable claimable debt, NOT received cash. Used only when
    /// the atomic claim fails. Every gauge must expose a nonnegative `to_claim`
    /// and match the pool registry; unreadable/unknown shapes fail closed.
    pub fn hybrid_reward_quote(env: Env) -> Map<Address, u128> {
        receipt(&env);
        assert!(
            !env.storage()
                .instance()
                .get::<_, bool>(&BridgeKey::UnprovenPrimary)
                .unwrap_or(false),
            "new primary needs actual claim proof"
        );
        Self::quote(env)
    }

    fn quote(env: Env) -> Map<Address, u128> {
        let cfg = config(&env);
        let primary = primary_reward_token(&env).expect("primary reward missing");
        let gauges: Map<Address, Address> =
            env.invoke_contract(&cfg.pool, &Symbol::new(&env, "get_gauges"), Vec::new(&env));
        assert!(gauges.len() <= MAX_REWARD_TOKENS, "too many reward streams");
        let user = env.current_contract_address();
        let raw: u128 = env.invoke_contract(
            &cfg.pool,
            &Symbol::new(&env, "get_user_reward"),
            (user.clone(),).into_val(&env),
        );
        let mut quoted = Map::new(&env);
        quoted.set(primary, raw);
        if !gauges.is_empty() {
            let info: Map<Address, Map<Symbol, i128>> = env.invoke_contract(
                &cfg.pool,
                &Symbol::new(&env, "gauges_get_reward_info"),
                (user,).into_val(&env),
            );
            assert_eq!(info.len(), gauges.len(), "gauge quote registry mismatch");
            for (asset, _) in gauges.iter() {
                let amount: u128 = info
                    .get(asset.clone())
                    .expect("gauge quote missing")
                    .get(Symbol::new(&env, "to_claim"))
                    .expect("claimable gauge amount missing")
                    .try_into()
                    .expect("negative gauge claimable");
                let combined = quoted
                    .get(asset.clone())
                    .unwrap_or(0u128)
                    .checked_add(amount)
                    .expect("reward quote overflow");
                quoted.set(asset, combined);
            }
        }
        assert!(quoted.len() <= MAX_REWARD_TOKENS, "too many reward streams");
        for (asset, _) in quoted.iter() {
            assert!(
                asset != cfg.token0 && asset != cfg.token1,
                "pair reward unsupported"
            );
        }
        quoted
    }

    /// Receipt-coordinated change only after old emissions have stopped and all
    /// claimable inventory was collected. Initial primary remains a bootstrap
    /// trust assumption; rotated primary cannot authorize quoted IOUs unproven.
    pub fn hybrid_rotate_primary(env: Env, expected: Address, next: Address) {
        receipt(&env);
        let cfg = config(&env);
        assert_eq!(primary_reward_token(&env), Some(expected.clone()));
        assert_ne!(next, expected, "primary unchanged");
        assert!(
            next != cfg.token0 && next != cfg.token1,
            "pair reward unsupported"
        );
        assert_eq!(
            crate::storage::state(&env).total_shares,
            0,
            "unwind strategy first"
        );
        let position: crate::pool::UserPositionSnapshot = env.invoke_contract(
            &cfg.pool,
            &Symbol::new(&env, "get_user_position_snapshot"),
            (env.current_contract_address(),).into_val(&env),
        );
        assert_eq!(position.raw_liquidity, 0, "pool liquidity remains");
        assert_eq!(position.weighted_liquidity, 0, "pool reward weight remains");
        let quote = Self::quote(env.clone());
        for (_, amount) in quote.iter() {
            assert_eq!(amount, 0, "old claimable rewards remain");
        }
        let route: Address = env
            .storage()
            .persistent()
            .get(&DataKey::RewardRoute(next.clone()))
            .expect("new reward route missing");
        let floor: u128 = env
            .storage()
            .persistent()
            .get(&DataKey::RewardMinRate(next.clone()))
            .expect("new reward floor missing");
        assert!(floor > 0, "new reward floor zero");
        let tokens: Vec<Address> =
            env.invoke_contract(&route, &Symbol::new(&env, "get_tokens"), Vec::new(&env));
        let underlying = if cfg.underlying_index == 0 {
            cfg.token0
        } else {
            cfg.token1
        };
        assert!(
            tokens.len() == 2 && tokens.contains(next.clone()) && tokens.contains(underlying),
            "invalid new reward route"
        );
        crate::storage::set_primary_reward(&env, &Some(next));
        env.storage()
            .instance()
            .set(&BridgeKey::UnprovenPrimary, &true);
    }

    /// Strict checkpoint: every configured stream must claim successfully before
    /// holder weights can change. Return values are checked, not used as backing.
    pub fn hybrid_claim(env: Env) -> Map<Address, u128> {
        let receipt = receipt(&env);
        let cfg = config(&env);
        let primary = primary_reward_token(&env).expect("primary reward missing");
        let gauges: Map<Address, Address> =
            env.invoke_contract(&cfg.pool, &Symbol::new(&env, "get_gauges"), Vec::new(&env));
        assert!(gauges.len() <= MAX_REWARD_TOKENS, "too many reward streams");
        let mut assets = Vec::new(&env);
        assets.push_back(primary.clone());
        for (asset, _) in gauges.iter() {
            if !assets.contains(asset.clone()) {
                assets.push_back(asset);
            }
        }
        assert!(assets.len() <= MAX_REWARD_TOKENS, "too many reward streams");
        for asset in assets.iter() {
            // Pair emissions overlap principal/fee NAV and need another ledger.
            assert!(
                asset != cfg.token0 && asset != cfg.token1,
                "pair reward unsupported"
            );
        }
        let me = env.current_contract_address();
        let mut before = Map::new(&env);
        for asset in assets.iter() {
            before.set(asset.clone(), balance(&env, &asset, &me));
        }
        let claimed: u128 = call(&env, &cfg.pool, "claim");
        assert_eq!(
            delta(
                balance(&env, &primary, &me),
                before.get(primary.clone()).unwrap()
            ),
            claimed,
            "primary claim mismatch"
        );
        if claimed > 0
            && env
                .storage()
                .instance()
                .get::<_, bool>(&BridgeKey::UnprovenPrimary)
                == Some(true)
        {
            env.storage()
                .instance()
                .set(&BridgeKey::UnprovenPrimary, &false);
        }
        // Snapshot gauge balances AFTER primary claim; the same asset may occur
        // in both streams and must not be counted twice.
        let mut gauge_before = Map::new(&env);
        for asset in assets.iter() {
            gauge_before.set(asset.clone(), balance(&env, &asset, &me));
        }
        let gauge_claims: Map<Address, u128> = call(&env, &cfg.pool, "gauges_claim");
        assert!(
            gauge_claims.len() <= MAX_REWARD_TOKENS,
            "too many claimed streams"
        );
        for (asset, _) in gauge_claims.iter() {
            assert!(gauges.contains_key(asset), "unregistered gauge reward");
        }
        for asset in assets.iter() {
            assert_eq!(
                delta(
                    balance(&env, &asset, &me),
                    gauge_before.get(asset.clone()).unwrap()
                ),
                gauge_claims.get(asset).unwrap_or(0),
                "gauge claim mismatch"
            );
        }
        let mut received = Map::new(&env);
        for asset in assets.iter() {
            let amount = delta(
                balance(&env, &asset, &me),
                before.get(asset.clone()).unwrap(),
            );
            if amount == 0 {
                continue;
            }
            let receipt_before = balance(&env, &asset, &receipt);
            token::Client::new(&env, &asset).transfer(&me, &receipt, &to_i128(amount));
            assert_eq!(
                delta(balance(&env, &asset, &receipt), receipt_before),
                amount,
                "claim transfer mismatch"
            );
            assert_eq!(
                balance(&env, &asset, &me),
                before.get(asset.clone()).unwrap(),
                "claim export overspent"
            );
            received.set(asset, amount);
        }
        received
    }

    /// Exact-input conversion. A failed/guarded route reverts this subcall,
    /// including the input transfer, so the receipt retains its raw-token claim.
    pub fn hybrid_swap(env: Env, reward: Address, amount: u128, minimum: u128) -> u128 {
        let receipt = receipt(&env);
        assert!(amount > 0, "zero reward swap");
        let cfg = config(&env);
        assert!(
            reward != cfg.token0 && reward != cfg.token1,
            "pair reward unsupported"
        );
        let underlying = if cfg.underlying_index == 0 {
            cfg.token0
        } else {
            cfg.token1
        };
        let route_key = DataKey::RewardRoute(reward.clone());
        crate::storage::bump_mapping_ttl(&env, &route_key);
        let route: Address = env
            .storage()
            .persistent()
            .get(&route_key)
            .expect("reward route missing");
        let tokens: Vec<Address> =
            env.invoke_contract(&route, &Symbol::new(&env, "get_tokens"), Vec::new(&env));
        assert_eq!(tokens.len(), 2, "reward route must have two tokens");
        let in_idx = tokens
            .first_index_of(reward.clone())
            .expect("route input missing");
        let out_idx = tokens
            .first_index_of(underlying.clone())
            .expect("route output missing");
        let estimate: u128 = env.invoke_contract(
            &route,
            &Symbol::new(&env, "estimate_swap"),
            (in_idx, out_idx, amount).into_val(&env),
        );
        let rate_key = DataKey::RewardMinRate(reward.clone());
        crate::storage::bump_mapping_ttl(&env, &rate_key);
        let rate: u128 = env.storage().persistent().get(&rate_key).unwrap_or(0);
        assert!(rate > 0, "reward floor missing");
        let floor = mul_div(&env, amount, rate, REWARD_RATE_SCALE);
        assert!(floor > 0 && estimate >= floor, "reward price guard");
        let required = apply_slippage_floor(&env, estimate, params(&env).slippage_bps)
            .max(floor)
            .max(minimum);
        let me = env.current_contract_address();
        let raw_before = balance(&env, &reward, &me);
        let receipt_raw = balance(&env, &reward, &receipt);
        let cash_before = balance(&env, &underlying, &me);
        let receipt_cash = balance(&env, &underlying, &receipt);
        token::Client::new(&env, &reward).transfer(&receipt, &me, &to_i128(amount));
        assert_eq!(
            delta(balance(&env, &reward, &me), raw_before),
            amount,
            "swap input mismatch"
        );
        assert_eq!(
            receipt_raw.checked_sub(balance(&env, &reward, &receipt)),
            Some(amount),
            "input overspent"
        );
        let output = AquariusLpVault::swap_reward(&env, &reward, amount);
        // The legacy whole-balance swap cannot detect replay using leftover
        // inventory. This exact-input wrapper MUST check actual input consumption.
        assert_eq!(
            balance(&env, &reward, &me),
            raw_before,
            "swap consumed wrong amount"
        );
        assert_eq!(
            delta(balance(&env, &underlying, &me), cash_before),
            output,
            "swap output mismatch"
        );
        assert!(output >= required, "reward output below minimum");
        token::Client::new(&env, &underlying).transfer(&me, &receipt, &to_i128(output));
        assert_eq!(
            delta(balance(&env, &underlying, &receipt), receipt_cash),
            output,
            "settlement transfer mismatch"
        );
        assert_eq!(
            balance(&env, &underlying, &me),
            cash_before,
            "settlement overspent"
        );
        output
    }
}
