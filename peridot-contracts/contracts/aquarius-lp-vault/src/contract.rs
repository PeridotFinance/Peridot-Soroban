use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, token, Address, BytesN, Env, IntoVal, Map, String, Symbol, Val, Vec,
};

use crate::constants::*;
use crate::events::*;
use crate::math::*;
use crate::oracle::{Asset as OracleAsset, PriceData};
use crate::pool::{ConcentratedPoolClient, UserPositionSnapshot};
use crate::storage::*;

pub const DEFAULT_INIT_ADMIN: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";

#[contract]
pub struct AquariusLpVault;

#[cfg(all(feature = "test-default-admin", target_arch = "wasm32"))]
compile_error!("aquarius-lp-vault test-default-admin must not be enabled for Wasm builds");

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallErrorKind {
    ContractRevert,
    HostError,
}

impl CallErrorKind {
    fn as_code(&self) -> u32 {
        match self {
            CallErrorKind::ContractRevert => 0,
            CallErrorKind::HostError => 1,
        }
    }
}

struct CallError {
    function: Symbol,
    kind: CallErrorKind,
}

fn try_call<T, A>(env: &Env, contract: &Address, func: &str, args: A) -> Result<T, CallError>
where
    T: soroban_sdk::TryFromVal<Env, Val>,
    A: IntoVal<Env, Vec<Val>>,
{
    use soroban_sdk::InvokeError;
    let symbol = Symbol::new(env, func);
    let args_val: Vec<Val> = args.into_val(env);
    match env.try_invoke_contract::<T, InvokeError>(contract, &symbol, args_val) {
        Ok(Ok(val)) => Ok(val),
        Ok(Err(_)) => Err(CallError {
            function: symbol,
            kind: CallErrorKind::ContractRevert,
        }),
        Err(_) => Err(CallError {
            function: symbol,
            kind: CallErrorKind::HostError,
        }),
    }
}

fn emit_call_failure(env: &Env, contract: &Address, err: &CallError, recoverable: bool) {
    ExternalCallFailed {
        contract: contract.clone(),
        function: err.function.clone(),
        recoverable,
        failure_kind: err.kind.as_code(),
    }
    .publish(env);
}

fn pow10(exp: u32) -> u128 {
    let mut out: u128 = 1;
    let mut i = 0u32;
    while i < exp {
        out = out.checked_mul(10).expect("decimals overflow");
        i += 1;
    }
    out
}

/// Builds an `InvokerContractAuthEntry` for a nested call this contract makes.
fn auth_entry(
    env: &Env,
    contract: &Address,
    fn_name: &str,
    args: Vec<Val>,
    sub: Vec<InvokerContractAuthEntry>,
) -> InvokerContractAuthEntry {
    InvokerContractAuthEntry::Contract(SubContractInvocation {
        context: ContractContext {
            contract: contract.clone(),
            fn_name: Symbol::new(env, fn_name),
            args,
        },
        sub_invocations: sub,
    })
}

#[contractimpl]
impl AquariusLpVault {
    // ─────────────────────────────────────────────────────────────────────
    // Wiring accessors
    // ─────────────────────────────────────────────────────────────────────

    fn pool(env: &Env) -> Address {
        config(env).pool
    }

    fn token_at(cfg: &Config, index: u32) -> Address {
        if index == 0 {
            cfg.token0.clone()
        } else {
            cfg.token1.clone()
        }
    }

    fn token(env: &Env, index: u32) -> Address {
        Self::token_at(&config(env), index)
    }

    fn underlying_index(env: &Env) -> u32 {
        config(env).underlying_index
    }

    fn other_index(env: &Env) -> u32 {
        1 - config(env).underlying_index
    }

    fn underlying(env: &Env) -> Address {
        let cfg = config(env);
        Self::token_at(&cfg, cfg.underlying_index)
    }

    fn other_token(env: &Env) -> Address {
        let cfg = config(env);
        Self::token_at(&cfg, 1 - cfg.underlying_index)
    }

    fn decimals_at(cfg: &Config, index: u32) -> u32 {
        if index == 0 {
            cfg.dec0
        } else {
            cfg.dec1
        }
    }

    fn ticks(env: &Env) -> (i32, i32) {
        let cfg = config(env);
        (cfg.tick_lower, cfg.tick_upper)
    }

    fn slippage_bps(env: &Env) -> u32 {
        params(env).slippage_bps
    }

    fn require_not_paused(env: &Env) {
        if params(env).paused {
            panic!("vault paused");
        }
    }

    fn balance_of_token(env: &Env, token: &Address) -> u128 {
        to_u128(token::TokenClient::new(env, token).balance(&env.current_contract_address()))
    }

    // ─────────────────────────────────────────────────────────────────────
    // NAV
    // ─────────────────────────────────────────────────────────────────────

    /// Liquidity units this vault holds in its single full-range position.
    ///
    /// Tracked locally rather than read from the pool on every call: the value
    /// is known exactly from `deposit_position` / `withdraw_position` return
    /// values, and keeping it local means the hot NAV path costs no
    /// cross-contract call and cannot be bricked by a pool TTL expiry.
    /// `sync_liquidity()` reconciles it against the pool on demand.
    fn position_liquidity(env: &Env) -> u128 {
        state(env).position_liquidity
    }

    fn set_position_liquidity(env: &Env, value: u128) {
        let mut st = state(env);
        st.position_liquidity = value;
        set_state(env, &st);
    }

    /// Reads a Reflector price, returning `None` on any failure or staleness.
    fn read_price(env: &Env, oracle: &Address, token: &Address, max_age_mult: u64) -> Option<u128> {
        let asset = match oracle_symbol(env, token) {
            Some(sym) => OracleAsset::Other(sym),
            None => OracleAsset::Stellar(token.clone()),
        };
        let pd: Option<PriceData> = match try_call(env, oracle, "lastprice", (asset,)) {
            Ok(v) => v,
            Err(e) => {
                emit_call_failure(env, oracle, &e, true);
                return None;
            }
        };
        let pd = pd?;
        if pd.price <= 0 {
            return None;
        }
        let resolution: u32 = match try_call(env, oracle, "resolution", ()) {
            Ok(v) => v,
            Err(e) => {
                emit_call_failure(env, oracle, &e, true);
                return None;
            }
        };
        let now = env.ledger().timestamp();
        if pd.timestamp > now {
            return None;
        }
        let max_age = (resolution as u64).saturating_mul(max_age_mult);
        if pd.timestamp.saturating_add(max_age) < now {
            return None;
        }
        Some(pd.price as u128)
    }

    /// `sqrt(other_price / underlying_price)`, scaled by 1e9.
    ///
    /// This is the manipulation-resistant half of the NAV. Deriving the price
    /// from Reflector rather than from `get_slot0` means an attacker who swings
    /// the pool's spot price cannot move this vault's reported value in either
    /// direction — they can only lose money to arbitrage.
    fn nav_root(env: &Env) -> u128 {
        let now = env.ledger().timestamp();
        let cached = state(env);
        let prm = params(env);
        // Serve from cache when fresh. Beyond saving an oracle round trip this
        // keeps the Reflector entries out of the ReceiptVault withdraw
        // footprint, which is what makes that path fit in a transaction.
        if cached.last_nav_root > 0
            && now.saturating_sub(cached.last_nav_root_at) <= prm.nav_root_max_age
        {
            return cached.last_nav_root;
        }
        Self::refresh_nav_root_inner(env)
    }

    fn refresh_nav_root_inner(env: &Env) -> u128 {
        let cfg = config(env);
        let max_age_mult = params(env).oracle_max_age_mult;
        let u_idx = cfg.underlying_index;
        let o_idx = 1 - u_idx;
        let p_under =
            Self::read_price(env, &cfg.oracle, &Self::token_at(&cfg, u_idx), max_age_mult);
        let p_other =
            Self::read_price(env, &cfg.oracle, &Self::token_at(&cfg, o_idx), max_age_mult);

        if let (Some(pu), Some(po)) = (p_under, p_other) {
            if pu > 0 && po > 0 {
                // ratio = (p_other / p_under) * 10^d_under / 10^d_other
                let ratio = mul_div(po, NAV_RATIO_SCALE, pu);
                let ratio = mul_div(
                    ratio,
                    pow10(Self::decimals_at(&cfg, u_idx)),
                    pow10(Self::decimals_at(&cfg, o_idx)),
                );
                let root = isqrt(ratio);
                if root > 0 {
                    let mut st = state(env);
                    st.last_nav_root = root;
                    st.last_nav_root_at = env.ledger().timestamp();
                    set_state(env, &st);
                    return root;
                }
            }
        }
        // Oracle unavailable: fall back to the last good root rather than
        // reporting a zero NAV, which would wipe out the backing market's
        // exchange rate. The fallback is bounded — past `nav_root_max_stale`
        // the value is too old to price borrowing or minting against, so we
        // stop serving it. `receipt-vault` wraps this read in `try_call` and
        // degrades to its own cached value, which is the intended behaviour.
        let st = state(env);
        let age = env.ledger().timestamp().saturating_sub(st.last_nav_root_at);
        let max_stale = params(env).nav_root_max_stale;
        if max_stale != 0 && age > max_stale {
            panic!("nav root too stale");
        }
        st.last_nav_root
    }

    /// Value of the full-range position, denominated in the underlying token.
    ///
    /// For a position spanning the entire tick range, `amount0 = L / sqrt(P)`
    /// and `amount1 = L * sqrt(P)`, so the total expressed in token0 is
    /// `2L / sqrt(P)` (and `2L * sqrt(P)` in token1). Both collapse to
    /// `2 * L * sqrt(other_price / underlying_price)`.
    ///
    /// Unclaimed swap fees are deliberately excluded, which understates NAV.
    /// Erring low is the safe direction for a lending market; `harvest()`
    /// folds the fees back into the position where they do get counted.
    fn position_value(env: &Env) -> u128 {
        let liq = Self::position_liquidity(env);
        if liq == 0 {
            return 0;
        }
        let root = Self::nav_root(env);
        if root == 0 {
            panic!("nav unavailable");
        }
        mul_div(liq, root, NAV_ROOT_SCALE)
            .checked_mul(2)
            .expect("nav overflow")
    }

    /// Value of an idle paired-token balance, expressed in the underlying.
    ///
    /// `nav_root` is `sqrt(R) * 1e9` where one raw underlying buys `1/R` raw
    /// other, so one raw other is worth `R` raw underlying.
    fn other_idle_value(env: &Env) -> u128 {
        let other_balance = Self::balance_of_token(env, &Self::other_token(env));
        if other_balance == 0 {
            return 0;
        }
        let root = Self::nav_root(env);
        if root == 0 {
            return 0;
        }
        let r_scaled = root.checked_mul(root).expect("nav root overflow");
        mul_div(other_balance, r_scaled, NAV_RATIO_SCALE)
    }

    /// Total underlying backing all outstanding shares: idle underlying plus
    /// the LP position.
    ///
    /// Deliberately excludes any idle paired-token balance. That leg is
    /// normally dust left over from a deposit swap, and reading its balance
    /// drags the paired token contract into the footprint of every caller —
    /// including the backing market's withdraw path, which pushed that
    /// transaction over its resource limits. Omitting it *understates* NAV,
    /// which is the safe direction: holders are never credited value the vault
    /// does not hold.
    fn total_underlying(env: &Env) -> u128 {
        let idle = Self::balance_of_token(env, &Self::underlying(env));
        idle.saturating_add(Self::position_value(env))
    }

    /// NAV including the idle paired-token balance.
    ///
    /// Used only for the before/after snapshots that price a deposit. The
    /// residue has to be counted there: `deploy_idle` folds the vault's whole
    /// paired balance into liquidity, so pricing a new depositor against a NAV
    /// that ignored it would mint them shares for residue belonging to the
    /// existing holders. On that path the balance read is free — `deploy_idle`
    /// already has the paired token in its footprint.
    fn total_underlying_full(env: &Env) -> u128 {
        Self::total_underlying(env).saturating_add(Self::other_idle_value(env))
    }

    fn total_shares(env: &Env) -> u128 {
        state(env).total_shares
    }

    fn shares_held(env: &Env, owner: &Address) -> u128 {
        shares_of(env, owner)
    }

    fn mint_shares(env: &Env, to: &Address, amount: u128) {
        set_shares(env, to, shares_of(env, to).saturating_add(amount));
        let mut st = state(env);
        st.total_shares = st.total_shares.saturating_add(amount);
        set_state(env, &st);
    }

    fn burn_shares(env: &Env, from: &Address, amount: u128) {
        let prev = shares_of(env, from);
        if prev < amount {
            panic!("insufficient shares");
        }
        set_shares(env, from, prev - amount);
        let mut st = state(env);
        st.total_shares = st.total_shares.saturating_sub(amount);
        set_state(env, &st);
    }

    // ─────────────────────────────────────────────────────────────────────
    // Pool interaction
    // ─────────────────────────────────────────────────────────────────────

    /// Rejects a swap whose pool quote is materially worse than the
    /// oracle-implied fair rate.
    ///
    /// `apply_slippage_floor` is derived from the pool's own `estimate_swap`,
    /// so it only catches movement between quote and execution — it cannot
    /// tell that the pool is mispriced to begin with. Without this check a
    /// deposit into a dislocated pool silently realises the gap: measured on
    /// testnet, a ~7% pool/oracle divergence cost 4.65% of the deposit.
    ///
    /// `nav_root` is `sqrt(R) * 1e9` where `R` is the raw-unit price of the
    /// underlying in the paired token, so one raw underlying buys `1/R` raw
    /// other and vice versa.
    fn require_quote_near_oracle(env: &Env, in_idx: u32, in_amount: u128, quoted_out: u128) {
        let max_div = params(env).max_pool_divergence_bps;
        if max_div == 0 {
            return;
        }
        let root = Self::nav_root(env);
        if root == 0 {
            // No usable oracle reading: fall through to the slippage floor
            // rather than blocking the vault entirely.
            return;
        }
        let r_scaled = root.checked_mul(root).expect("nav root overflow"); // R * 1e18
        let fair_out = if in_idx == Self::underlying_index(env) {
            // underlying -> other: out = in / R
            mul_div(in_amount, NAV_RATIO_SCALE, r_scaled)
        } else {
            // other -> underlying: out = in * R
            mul_div(in_amount, r_scaled, NAV_RATIO_SCALE)
        };
        if fair_out == 0 {
            return;
        }
        if quoted_out < apply_slippage_floor(fair_out, max_div) {
            panic!("pool price diverged from oracle");
        }
    }

    /// Swaps `in_amount` of token `in_idx` for the other token, guarded by the
    /// configured slippage floor. Returns the amount received.
    ///
    /// Aquarius guarantees exactly `in_amount` is consumed, so authorizing a
    /// transfer of precisely that amount is always correct.
    fn swap_exact_in(env: &Env, in_idx: u32, out_idx: u32, in_amount: u128) -> u128 {
        if in_amount == 0 {
            return 0;
        }
        let pool = Self::pool(env);
        let pool_client = ConcentratedPoolClient::new(env, &pool);
        let in_token = Self::token(env, in_idx);
        let out_token = Self::token(env, out_idx);

        let estimated = pool_client.estimate_swap(&in_idx, &out_idx, &in_amount);
        Self::require_quote_near_oracle(env, in_idx, in_amount, estimated);
        let out_min = apply_slippage_floor(estimated, Self::slippage_bps(env));

        let me = env.current_contract_address();
        let transfer_args: Vec<Val> = (me.clone(), pool.clone(), to_i128(in_amount)).into_val(env);
        // The Aquarius pool does not call `user.require_auth()` — it relies on
        // the token transfer's own auth. So the transfer entry must sit at the
        // ROOT of the authorization list: nested under a pool-call entry it is
        // unreachable, because that parent context is never matched and the
        // transfer then fails with Auth(InvalidAction). This is the same shape
        // `receipt-vault` uses for the DeFindex vault (contract.rs:311-327),
        // and it was confirmed on testnet against the deployed pool.
        let mut auths = Vec::new(env);
        auths.push_back(auth_entry(
            env,
            &in_token,
            "transfer",
            transfer_args,
            Vec::new(env),
        ));

        // Read balances *before* authorizing. `authorize_as_current_contract`
        // only covers the next contract call, so an intervening call — even a
        // read-only `balance` — discards the authorization and the pool's
        // nested `transfer` then fails with Auth(InvalidAction). Verified on
        // testnet against the real Aquarius pool.
        let before = Self::balance_of_token(env, &out_token);
        env.authorize_as_current_contract(auths);
        pool_client.swap(&me, &in_idx, &out_idx, &in_amount, &out_min);
        Self::balance_of_token(env, &out_token).saturating_sub(before)
    }

    /// Deploys idle underlying into the full-range position.
    ///
    /// Splits the input in half, swaps one half into the paired token, then
    /// deposits both legs. `estimate_deposit_position` is called first so the
    /// authorized transfer amounts match exactly what the pool will pull.
    fn deploy_idle(env: &Env) -> u128 {
        let underlying = Self::underlying(env);
        let idle = Self::balance_of_token(env, &underlying);
        if idle < MIN_DEPLOY_AMOUNT {
            return 0;
        }

        // Respect the TVL cap: this vault's share of a small pool is what sets
        // its realised APR, so over-deploying quietly destroys the yield it
        // exists to capture.
        let max_deploy = params(env).max_deploy;
        let deployable = if max_deploy == 0 {
            idle
        } else {
            let deployed = Self::position_value(env);
            if deployed >= max_deploy {
                return 0;
            }
            idle.min(max_deploy - deployed)
        };
        if deployable < MIN_DEPLOY_AMOUNT {
            return 0;
        }

        let u_idx = Self::underlying_index(env);
        let o_idx = 1 - u_idx;

        let half = deployable / 2;
        if half == 0 {
            return 0;
        }
        Self::swap_exact_in(env, u_idx, o_idx, half);

        let amount_u = Self::balance_of_token(env, &underlying).min(deployable - half);
        let amount_o = Self::balance_of_token(env, &Self::other_token(env));
        if amount_u == 0 || amount_o == 0 {
            return 0;
        }

        let pool = Self::pool(env);
        let pool_client = ConcentratedPoolClient::new(env, &pool);
        let (tick_lower, tick_upper) = Self::ticks(env);

        let mut desired: Vec<u128> = Vec::new(env);
        if u_idx == 0 {
            desired.push_back(amount_u);
            desired.push_back(amount_o);
        } else {
            desired.push_back(amount_o);
            desired.push_back(amount_u);
        }

        // Ask the pool what it would actually take, then authorize exactly
        // that. Price cannot move between the estimate and the call because
        // both happen inside this transaction.
        let (actual, est_liquidity) =
            pool_client.estimate_deposit_position(&tick_lower, &tick_upper, &desired);
        if est_liquidity == 0 {
            return 0;
        }
        let min_liquidity = apply_slippage_floor(est_liquidity, Self::slippage_bps(env));

        let me = env.current_contract_address();
        // The Aquarius pool does not call `user.require_auth()` — it relies on
        // the token transfer's own auth. So the transfer entry must sit at the
        // ROOT of the authorization list: nested under a pool-call entry it is
        // unreachable, because that parent context is never matched and the
        // transfer then fails with Auth(InvalidAction). This is the same shape
        // `receipt-vault` uses for the DeFindex vault (contract.rs:311-327),
        // and it was confirmed on testnet against the deployed pool.
        let mut auths = Vec::new(env);
        for i in 0..actual.len() {
            let amt = actual.get(i).unwrap_or(0);
            if amt == 0 {
                continue;
            }
            let tok = Self::token(env, i);
            let args: Vec<Val> = (me.clone(), pool.clone(), to_i128(amt)).into_val(env);
            auths.push_back(auth_entry(env, &tok, "transfer", args, Vec::new(env)));
        }
        env.authorize_as_current_contract(auths);

        let (_spent, minted) =
            pool_client.deposit_position(&me, &tick_lower, &tick_upper, &actual, &min_liquidity);
        if minted > 0 {
            Self::set_position_liquidity(env, Self::position_liquidity(env).saturating_add(minted));
        }
        minted
    }

    /// Raises `needed` underlying, cheapest source first.
    ///
    /// Idle paired token is sold before touching the LP position: it is part of
    /// NAV (so shares have a claim on it) but cannot be paid out directly, and
    /// selling it is cheaper than burning liquidity. Doing this also clears the
    /// residue a deposit swap leaves behind.
    /// Returns `false` if the LP redemption leg failed outright, so the caller
    /// can tell "the pool is down" apart from "exiting simply costs fees".
    fn raise_underlying(env: &Env, needed: u128) -> bool {
        let underlying = Self::underlying(env);
        if Self::balance_of_token(env, &underlying) >= needed {
            return true;
        }
        let have = Self::balance_of_token(env, &underlying);
        // `redeem_for` sells the vault's entire paired balance in one swap, so
        // idle residue is folded in there rather than swapped separately —
        // a second swap on the exit path costs both fees and the transaction
        // budget the backing market needs.
        if Self::position_liquidity(env) > 0 {
            return Self::redeem_for(env, needed.saturating_sub(have));
        }
        // No position left: the paired balance is all there is to sell.
        if Self::other_idle_value(env) >= MIN_DEPLOY_AMOUNT {
            let other_balance = Self::balance_of_token(env, &Self::other_token(env));
            Self::swap_exact_in(
                env,
                Self::other_index(env),
                Self::underlying_index(env),
                other_balance,
            );
            return true;
        }
        false
    }

    /// Burns enough liquidity to raise `needed` underlying into idle balance.
    ///
    /// Mirrors `receipt-vault::ensure_liquid_cash`: best-effort, never panics
    /// on a pool failure, because a hard revert here would freeze every
    /// withdrawal in the market above.
    fn redeem_for(env: &Env, needed: u128) -> bool {
        if needed == 0 {
            return true;
        }
        let liq = Self::position_liquidity(env);
        if liq == 0 {
            return false;
        }
        let position_value = Self::position_value(env);
        if position_value == 0 {
            return false;
        }

        // Round up so a rounding shortfall does not leave the caller one unit
        // short and force a second round trip.
        let mut burn = mul_div_ceil(needed.min(position_value), liq, position_value);
        if burn == 0 {
            burn = 1;
        }
        if burn > liq {
            burn = liq;
        }

        let pool = Self::pool(env);
        let (tick_lower, tick_upper) = Self::ticks(env);
        let me = env.current_contract_address();
        let underlying = Self::underlying(env);
        let other = Self::other_token(env);

        let mut min_amounts: Vec<u128> = Vec::new(env);
        min_amounts.push_back(0u128);
        min_amounts.push_back(0u128);

        let args: Vec<Val> = (
            me.clone(),
            tick_lower,
            tick_upper,
            burn,
            min_amounts.clone(),
        )
            .into_val(env);
        let mut auths = Vec::new(env);
        auths.push_back(auth_entry(
            env,
            &pool,
            "withdraw_position",
            args,
            Vec::new(env),
        ));

        // Read balances *before* authorizing. `authorize_as_current_contract`
        // only covers the next contract call, so an intervening call — even a
        // read-only `balance` — discards the authorization and the pool's
        // nested `transfer` then fails with Auth(InvalidAction). Verified on
        // testnet against the real Aquarius pool.
        let under_before = Self::balance_of_token(env, &underlying);
        let other_before = Self::balance_of_token(env, &other);
        env.authorize_as_current_contract(auths);

        let result: Result<Vec<u128>, CallError> = try_call(
            env,
            &pool,
            "withdraw_position",
            (me.clone(), tick_lower, tick_upper, burn, min_amounts),
        );
        match result {
            Ok(_) => {
                Self::set_position_liquidity(env, liq.saturating_sub(burn));
            }
            Err(ref e) => {
                emit_call_failure(env, &pool, e, false);
                return false;
            }
        }

        let under_got = Self::balance_of_token(env, &underlying).saturating_sub(under_before);
        let other_got = Self::balance_of_token(env, &other).saturating_sub(other_before);

        // Convert the paired leg back to underlying so the caller only ever
        // sees the single asset the market accounts in. Sells the *entire*
        // balance, not just what this redemption produced, so residue left by
        // earlier deposit swaps is cleared in the same swap.
        let other_total = Self::balance_of_token(env, &other);
        if other_total > 0 {
            Self::swap_exact_in(
                env,
                Self::other_index(env),
                Self::underlying_index(env),
                other_total,
            );
        }

        if under_got == 0 && other_got == 0 {
            RedeemZeroReturn {
                liquidity_burned: burn,
            }
            .publish(env);
            return false;
        }
        true
    }

    // ─────────────────────────────────────────────────────────────────────
    // DeFindex-compatible surface
    //
    // These five functions are the entire contract that `receipt-vault`'s
    // boosted-vault socket depends on (see receipt-vault/src/contract.rs:89,
    // :291, :359). Signatures are transcribed from the deployed Peridot
    // DeFindex vault spec so this contract is a drop-in for
    // `set_boosted_vault` with no change to the audited market code.
    // ─────────────────────────────────────────────────────────────────────

    /// Share balance. Read by `receipt-vault` through a SEP-41 `TokenClient`.
    ///
    /// Renews the holder's TTL on the way past. `Shares(owner)` is persistent
    /// state that is otherwise only bumped on write, so a market that holds a
    /// position without depositing or withdrawing for the entry lifetime would
    /// see its balance archived to zero while `total_shares` still counted it —
    /// and could then never redeem. `receipt-vault` reads this on every
    /// boosted-value refresh, so renewing here keeps live claims alive.
    pub fn balance(env: Env, id: Address) -> i128 {
        bump_share_ttl(&env, &id);
        to_i128(Self::shares_held(&env, &id))
    }

    /// Permissionless TTL renewal for a share holder, for keepers that do not
    /// want to pay for a full `balance` read path.
    pub fn bump_shares_ttl(env: Env, owner: Address) {
        bump_share_ttl(&env, &owner);
    }

    /// Total share supply.
    pub fn total_supply(env: Env) -> i128 {
        to_i128(Self::total_shares(&env))
    }

    pub fn decimals(_env: Env) -> u32 {
        SHARE_DECIMALS
    }

    pub fn name(env: Env) -> String {
        String::from_str(&env, "Peridot Aquarius LP Vault")
    }

    pub fn symbol(env: Env) -> String {
        String::from_str(&env, "pAQLP")
    }

    /// Underlying redeemable for `vault_shares`, as a single-element vector.
    ///
    /// `receipt-vault` reads index 0 as *the* underlying amount and sizes its
    /// `min_amounts_out` vector by this vector's length, so the single-asset
    /// shape here is load-bearing.
    pub fn get_asset_amounts_per_shares(env: Env, vault_shares: i128) -> Vec<i128> {
        bump_critical_ttl(&env);
        let mut out: Vec<i128> = Vec::new(&env);
        let shares = to_u128(vault_shares);
        let supply = Self::total_shares(&env);
        if shares == 0 || supply == 0 {
            out.push_back(0i128);
            return out;
        }
        let nav = Self::total_underlying(&env);
        out.push_back(to_i128(mul_div(shares, nav, supply)));
        out
    }

    /// Accepts underlying and mints shares.
    ///
    /// Deliberately does **not** call `from.require_auth()`: `receipt-vault`
    /// pre-authorizes only the inner token transfer (contract.rs:311-327), and
    /// that transfer's own auth requirement is what makes this safe — a caller
    /// cannot pull funds from an address that has not signed.
    pub fn deposit(
        env: Env,
        amounts_desired: Vec<i128>,
        amounts_min: Vec<i128>,
        from: Address,
        invest: bool,
    ) -> i128 {
        bump_critical_ttl(&env);
        Self::require_not_paused(&env);

        let amount = to_u128(amounts_desired.get(0).unwrap_or(0));
        if amount == 0 {
            return 0;
        }
        let min_amount = to_u128(amounts_min.get(0).unwrap_or(0));
        if amount < min_amount {
            panic!("deposit below minimum");
        }

        // NAV is sampled before the incoming funds land, and again after they
        // have been deployed. Minting against the *net* value added means the
        // depositor pays their own entry cost (the swap fee on converting half
        // the deposit into the paired token) instead of socialising it onto
        // the holders who were already here.
        let nav_before = Self::total_underlying_full(&env);
        let supply = Self::total_shares(&env);

        let underlying = Self::underlying(&env);
        token::TokenClient::new(&env, &underlying).transfer(
            &from,
            env.current_contract_address(),
            &to_i128(amount),
        );

        let minted_liquidity = if invest { Self::deploy_idle(&env) } else { 0 };

        let value_added = Self::total_underlying_full(&env).saturating_sub(nav_before);
        if value_added == 0 {
            panic!("deposit added no value");
        }

        let shares = if supply == 0 || nav_before == 0 {
            value_added
        } else {
            mul_div(value_added, supply, nav_before)
        };
        if shares == 0 {
            panic!("deposit too small to mint shares");
        }
        Self::mint_shares(&env, &from, shares);

        Deposited {
            from,
            underlying_in: amount,
            liquidity_minted: minted_liquidity,
            shares_minted: shares,
        }
        .publish(&env);

        to_i128(shares)
    }

    /// Burns shares and returns underlying.
    ///
    /// Unlike `deposit`, this *does* require auth from `from`, because
    /// `receipt-vault` authorizes this exact invocation before calling it
    /// (contract.rs:420-437). Requiring it closes the forced-redemption
    /// griefing vector that the bare DeFindex shape leaves open.
    pub fn withdraw(
        env: Env,
        withdraw_shares: i128,
        min_amounts_out: Vec<i128>,
        from: Address,
    ) -> Vec<i128> {
        bump_critical_ttl(&env);
        from.require_auth();

        let shares = to_u128(withdraw_shares);
        let mut out: Vec<i128> = Vec::new(&env);
        if shares == 0 {
            out.push_back(0i128);
            return out;
        }
        let supply = Self::total_shares(&env);
        if supply == 0 {
            panic!("no shares outstanding");
        }

        let owed = mul_div(shares, Self::total_underlying(&env), supply);
        let underlying = Self::underlying(&env);

        let redeemed_ok = Self::raise_underlying(&env, owed);

        // Pay what was actually realised, never more. Over-redemption stays as
        // idle and accrues to the remaining holders rather than being paid out
        // to whoever happened to exit during a price dislocation.
        //
        // Unless this is the last holder leaving: there is nobody left to
        // accrue to, so anything held back would sit unowned until the next
        // depositor captured it. Observed on testnet — a full exit stranded
        // 10.3 XLM behind a zero share supply.
        let available = Self::balance_of_token(&env, &underlying);
        let payout = if shares == supply {
            available
        } else {
            owed.min(available)
        };

        let min_out = to_u128(min_amounts_out.get(0).unwrap_or(0));
        if payout < min_out {
            panic!("withdraw below minimum");
        }

        // A shortfall because exiting costs swap fees is the exiting holder's
        // own cost — burn their full stake, exactly as a depositor pays their
        // own entry cost. A shortfall because the pool refused to release
        // liquidity is different: burning there would destroy an unredeemed
        // claim permanently, so refuse and let them retry once it recovers.
        if !redeemed_ok && payout < owed {
            panic!("could not raise underlying; shares preserved");
        }
        let shares_to_burn = shares;

        let liquidity_before = Self::position_liquidity(&env);
        Self::burn_shares(&env, &from, shares_to_burn);
        if payout > 0 {
            token::TokenClient::new(&env, &underlying).transfer(
                &env.current_contract_address(),
                &from,
                &to_i128(payout),
            );
        }

        Withdrawn {
            to: from,
            shares_burned: shares_to_burn,
            liquidity_burned: liquidity_before.saturating_sub(Self::position_liquidity(&env)),
            underlying_out: payout,
        }
        .publish(&env);

        out.push_back(to_i128(payout));
        out
    }

    // ─────────────────────────────────────────────────────────────────────
    // Direct user entrypoints
    // ─────────────────────────────────────────────────────────────────────

    /// Single-sided deposit for end users. The vault handles the split.
    pub fn deposit_underlying(env: Env, from: Address, amount: i128, min_shares: i128) -> i128 {
        from.require_auth();
        let mut desired: Vec<i128> = Vec::new(&env);
        desired.push_back(amount);
        let mut mins: Vec<i128> = Vec::new(&env);
        mins.push_back(0i128);
        let shares = Self::deposit(env, desired, mins, from, true);
        if shares < min_shares {
            panic!("shares below minimum");
        }
        shares
    }

    /// Redeem shares for underlying. Thin wrapper so users are not forced to
    /// construct the DeFindex-shaped vectors.
    pub fn redeem(env: Env, from: Address, shares: i128, min_underlying_out: i128) -> i128 {
        let mut mins: Vec<i128> = Vec::new(&env);
        mins.push_back(min_underlying_out);
        let out = Self::withdraw(env, shares, mins, from);
        out.get(0).unwrap_or(0)
    }

    /// Transfers vault shares. Makes the position portable without forcing a
    /// round trip through the pool.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        bump_critical_ttl(&env);
        let amount_u = to_u128(amount);
        if amount_u == 0 {
            return;
        }
        Self::burn_shares(&env, &from, amount_u);
        Self::mint_shares(&env, &to, amount_u);
    }

    // ─────────────────────────────────────────────────────────────────────
    // Harvest
    // ─────────────────────────────────────────────────────────────────────

    /// Sells `amount` of `reward_token` for underlying through its configured
    /// route pool. Returns 0 (and emits) rather than reverting, so one
    /// unsellable reward cannot block the rest of the harvest.
    fn swap_reward(env: &Env, reward_token: &Address, amount: u128) -> u128 {
        let underlying = Self::underlying(env);
        if *reward_token == underlying {
            return amount;
        }
        let route: Option<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RewardRoute(reward_token.clone()));
        let Some(route_pool) = route else {
            HarvestSkipped {
                reward_token: reward_token.clone(),
                reward_amount: amount,
                reason: Symbol::new(env, "no_route"),
            }
            .publish(env);
            return 0;
        };

        let tokens: Vec<Address> = match try_call(env, &route_pool, "get_tokens", ()) {
            Ok(v) => v,
            Err(e) => {
                emit_call_failure(env, &route_pool, &e, true);
                return 0;
            }
        };
        let mut in_idx: Option<u32> = None;
        let mut out_idx: Option<u32> = None;
        for i in 0..tokens.len() {
            let t = tokens.get(i).unwrap();
            if t == *reward_token {
                in_idx = Some(i);
            } else if t == underlying {
                out_idx = Some(i);
            }
        }
        let (Some(in_idx), Some(out_idx)) = (in_idx, out_idx) else {
            HarvestSkipped {
                reward_token: reward_token.clone(),
                reward_amount: amount,
                reason: Symbol::new(env, "bad_route"),
            }
            .publish(env);
            return 0;
        };

        let estimated: u128 =
            match try_call(env, &route_pool, "estimate_swap", (in_idx, out_idx, amount)) {
                Ok(v) => v,
                Err(e) => {
                    emit_call_failure(env, &route_pool, &e, true);
                    return 0;
                }
            };
        let out_min = apply_slippage_floor(estimated, Self::slippage_bps(env));
        if out_min == 0 {
            HarvestSkipped {
                reward_token: reward_token.clone(),
                reward_amount: amount,
                reason: Symbol::new(env, "no_output"),
            }
            .publish(env);
            return 0;
        }

        let me = env.current_contract_address();
        let transfer_args: Vec<Val> =
            (me.clone(), route_pool.clone(), to_i128(amount)).into_val(env);
        let mut auths = Vec::new(env);
        auths.push_back(auth_entry(
            env,
            reward_token,
            "transfer",
            transfer_args,
            Vec::new(env),
        ));

        // Read balances *before* authorizing. `authorize_as_current_contract`
        // only covers the next contract call, so an intervening call — even a
        // read-only `balance` — discards the authorization and the pool's
        // nested `transfer` then fails with Auth(InvalidAction). Verified on
        // testnet against the real Aquarius pool.
        let before = Self::balance_of_token(env, &underlying);
        env.authorize_as_current_contract(auths);
        let res: Result<u128, CallError> = try_call(
            env,
            &route_pool,
            "swap",
            (me, in_idx, out_idx, amount, out_min),
        );
        if let Err(ref e) = res {
            emit_call_failure(env, &route_pool, e, true);
            return 0;
        }
        Self::balance_of_token(env, &underlying).saturating_sub(before)
    }

    /// Permissionless: claim AQUA emissions, third-party gauge incentives and
    /// accrued swap fees, convert everything to underlying, and redeploy.
    ///
    /// Rate-limited because each call moves the share price; without a cooldown
    /// it could be used to grind rounding in the depositor's favour.
    pub fn harvest(env: Env, caller: Address) -> i128 {
        caller.require_auth();
        bump_critical_ttl(&env);

        let now = env.ledger().timestamp();
        let mut st = state(&env);
        let cooldown = params(&env).harvest_cooldown;
        if st.last_harvest != 0 && now < st.last_harvest.saturating_add(cooldown) {
            panic!("harvest on cooldown");
        }
        st.last_harvest = now;
        set_state(&env, &st);

        let pool = Self::pool(&env);
        let me = env.current_contract_address();
        let underlying = Self::underlying(&env);
        let other = Self::other_token(&env);
        let before_underlying = Self::balance_of_token(&env, &underlying);

        // 1. Swap fees accrued by the position, paid in the pair's own tokens.
        let fee_args: Vec<Val> = (me.clone(),).into_val(&env);
        let mut auths = Vec::new(&env);
        auths.push_back(auth_entry(
            &env,
            &pool,
            "claim_all_position_fees",
            fee_args,
            Vec::new(&env),
        ));
        env.authorize_as_current_contract(auths);
        if let Err(ref e) =
            try_call::<Vec<u128>, _>(&env, &pool, "claim_all_position_fees", (me.clone(),))
        {
            emit_call_failure(&env, &pool, e, true);
        }

        // 2. AQUA emissions.
        let claim_args: Vec<Val> = (me.clone(),).into_val(&env);
        let mut auths = Vec::new(&env);
        auths.push_back(auth_entry(&env, &pool, "claim", claim_args, Vec::new(&env)));
        env.authorize_as_current_contract(auths);
        match try_call::<u128, _>(&env, &pool, "claim", (me.clone(),)) {
            Ok(_) => {}
            Err(ref e) => emit_call_failure(&env, &pool, e, true),
        }

        // 3. Third-party pool incentives (gauges), keyed by reward token.
        let gauge_args: Vec<Val> = (me.clone(),).into_val(&env);
        let mut auths = Vec::new(&env);
        auths.push_back(auth_entry(
            &env,
            &pool,
            "gauges_claim",
            gauge_args,
            Vec::new(&env),
        ));
        env.authorize_as_current_contract(auths);
        let gauge_rewards: Map<Address, u128> =
            match try_call(&env, &pool, "gauges_claim", (me.clone(),)) {
                Ok(v) => v,
                Err(ref e) => {
                    emit_call_failure(&env, &pool, e, true);
                    Map::new(&env)
                }
            };

        // 4. Sell every reward token that is not already part of the pair.
        //    Balances are read directly so donations and partially-claimed
        //    rewards are swept too.
        let mut reward_tokens: Vec<Address> = Vec::new(&env);
        for (tok, _) in gauge_rewards.iter() {
            if tok != underlying && tok != other {
                reward_tokens.push_back(tok);
            }
        }
        for i in 0..reward_tokens.len() {
            let tok = reward_tokens.get(i).unwrap();
            let bal = Self::balance_of_token(&env, &tok);
            if bal == 0 {
                continue;
            }
            let got = Self::swap_reward(&env, &tok, bal);
            Harvested {
                caller: caller.clone(),
                reward_token: tok,
                reward_amount: bal,
                underlying_out: got,
            }
            .publish(&env);
        }

        // 5. Fold the paired-token leg back in, then redeploy everything.
        let other_balance = Self::balance_of_token(&env, &other);
        if other_balance > 0 {
            Self::swap_exact_in(
                &env,
                Self::other_index(&env),
                Self::underlying_index(&env),
                other_balance,
            );
        }
        Self::deploy_idle(&env);

        let gained = Self::balance_of_token(&env, &underlying).saturating_sub(before_underlying);
        to_i128(gained)
    }

    /// Sells any AQUA held by the vault. Kept separate from `harvest` so the
    /// AQUA route can be exercised even when the pool's claim path is paused.
    pub fn sweep_reward(env: Env, caller: Address, reward_token: Address) -> i128 {
        caller.require_auth();
        bump_critical_ttl(&env);
        let underlying = Self::underlying(&env);
        let other = Self::other_token(&env);
        if reward_token == underlying || reward_token == other {
            panic!("cannot sweep pair token");
        }
        let bal = Self::balance_of_token(&env, &reward_token);
        if bal == 0 {
            return 0;
        }
        let got = Self::swap_reward(&env, &reward_token, bal);
        Harvested {
            caller,
            reward_token,
            reward_amount: bal,
            underlying_out: got,
        }
        .publish(&env);
        to_i128(got)
    }

    // ─────────────────────────────────────────────────────────────────────
    // Initialization and administration
    // ─────────────────────────────────────────────────────────────────────

    /// Wires the vault to a concentrated Aquarius pool.
    ///
    /// Pool metadata (tokens, decimals, tick spacing) is read from the pool
    /// itself rather than passed in, so a typo cannot silently point the vault
    /// at the wrong asset.
    pub fn initialize(
        env: Env,
        admin: Address,
        pool: Address,
        underlying_index: u32,
        oracle: Address,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        if underlying_index > 1 {
            panic!("underlying index out of range");
        }
        let expected = Address::from_string(&String::from_str(&env, expected_admin_config()));
        if admin != expected {
            panic!("unexpected admin");
        }
        admin.require_auth();

        let pool_client = ConcentratedPoolClient::new(&env, &pool);

        // The full-range NAV formula (2L/sqrt(P)) only holds for a tick-based
        // pool. Refuse anything else rather than silently mispricing.
        let pool_type = pool_client.pool_type();
        if pool_type != Symbol::new(&env, "concentrated") {
            panic!("pool is not concentrated");
        }

        let tokens = pool_client.get_tokens();
        if tokens.len() != 2 {
            panic!("pool must have exactly two tokens");
        }
        let token0 = tokens.get(0).unwrap();
        let token1 = tokens.get(1).unwrap();
        let dec0 = token::TokenClient::new(&env, &token0).decimals();
        let dec1 = token::TokenClient::new(&env, &token1).decimals();

        let tick_spacing = pool_client.get_tick_spacing();
        let (tick_lower, tick_upper) = full_range_bounds(tick_spacing, MAX_TICK_ABS);

        set_config(
            &env,
            &Config {
                pool: pool.clone(),
                token0: token0.clone(),
                token1: token1.clone(),
                dec0,
                dec1,
                underlying_index,
                tick_lower,
                tick_upper,
                oracle,
            },
        );
        set_params(
            &env,
            &Params {
                max_deploy: 0,
                slippage_bps: DEFAULT_SLIPPAGE_BPS,
                harvest_cooldown: DEFAULT_HARVEST_COOLDOWN_SECS,
                oracle_max_age_mult: DEFAULT_ORACLE_MAX_AGE_MULT,
                nav_root_max_age: DEFAULT_NAV_ROOT_MAX_AGE_SECS,
                max_pool_divergence_bps: DEFAULT_MAX_POOL_DIVERGENCE_BPS,
                nav_root_max_stale: DEFAULT_NAV_ROOT_MAX_STALE_SECS,
                paused: false,
            },
        );
        set_state(
            &env,
            &State {
                total_shares: 0,
                position_liquidity: 0,
                last_nav_root: 0,
                last_nav_root_at: 0,
                last_harvest: 0,
            },
        );
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        bump_critical_ttl(&env);

        let underlying = if underlying_index == 0 {
            token0
        } else {
            token1
        };
        VaultInitialized {
            admin,
            pool,
            underlying,
            tick_lower,
            tick_upper,
        }
        .publish(&env);
    }

    pub fn get_admin(env: Env) -> Address {
        bump_critical_ttl(&env);
        admin(&env)
    }

    pub fn set_admin(env: Env, admin_addr: Address, new_admin: Address) {
        Self::require_admin(&env, &admin_addr);
        if admin_addr == new_admin {
            panic!("admin unchanged");
        }
        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &new_admin);
        bump_pending_admin_ttl(&env);
        AdminTransferProposed {
            current_admin: admin_addr,
            pending_admin: new_admin,
        }
        .publish(&env);
    }

    pub fn accept_admin(env: Env) {
        bump_critical_ttl(&env);
        let pending: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .expect("no pending admin");
        pending.require_auth();
        let previous = admin(&env);
        env.storage().instance().set(&DataKey::Admin, &pending);
        env.storage().persistent().remove(&DataKey::PendingAdmin);
        AdminTransferred {
            previous_admin: previous,
            new_admin: pending,
        }
        .publish(&env);
    }

    /// Caps how much underlying may sit in the pool.
    ///
    /// The realised APR of an LP position is the headline rate scaled by
    /// `pool_tvl / (pool_tvl + deployed)`. On a thin pool an uncapped vault
    /// dilutes itself to near-zero yield, so this is a yield control, not just
    /// a risk control. `0` disables the cap.
    pub fn set_max_deploy(env: Env, admin_addr: Address, max_deploy: u128) {
        Self::require_admin(&env, &admin_addr);
        let mut p = params(&env);
        let old = p.max_deploy;
        p.max_deploy = max_deploy;
        set_params(&env, &p);
        ConfigChanged {
            what: Symbol::new(&env, "max_deploy"),
            old_value: old,
            new_value: max_deploy,
        }
        .publish(&env);
    }

    pub fn set_slippage_bps(env: Env, admin_addr: Address, bps: u32) {
        Self::require_admin(&env, &admin_addr);
        if bps > MAX_SLIPPAGE_BPS {
            panic!("slippage above cap");
        }
        let mut p = params(&env);
        let old = p.slippage_bps;
        p.slippage_bps = bps;
        set_params(&env, &p);
        ConfigChanged {
            what: Symbol::new(&env, "slippage_bps"),
            old_value: old as u128,
            new_value: bps as u128,
        }
        .publish(&env);
    }

    pub fn set_harvest_cooldown(env: Env, admin_addr: Address, seconds: u64) {
        Self::require_admin(&env, &admin_addr);
        let mut p = params(&env);
        let old = p.harvest_cooldown;
        p.harvest_cooldown = seconds;
        set_params(&env, &p);
        ConfigChanged {
            what: Symbol::new(&env, "harvest_cd"),
            old_value: old as u128,
            new_value: seconds as u128,
        }
        .publish(&env);
    }

    /// Pauses new deposits. Withdrawals are deliberately never pausable: the
    /// market above must always be able to pull its cash back.
    pub fn set_paused(env: Env, admin_addr: Address, paused: bool) {
        Self::require_admin(&env, &admin_addr);
        let mut p = params(&env);
        p.paused = paused;
        set_params(&env, &p);
        PausedSet { paused }.publish(&env);
    }

    pub fn set_oracle(env: Env, admin_addr: Address, oracle: Address) {
        Self::require_admin(&env, &admin_addr);
        let mut cfg = config(&env);
        cfg.oracle = oracle;
        set_config(&env, &cfg);
    }

    pub fn set_oracle_max_age_mult(env: Env, admin_addr: Address, k: u64) {
        Self::require_admin(&env, &admin_addr);
        if k == 0 {
            panic!("multiplier must be positive");
        }
        let mut p = params(&env);
        p.oracle_max_age_mult = k;
        set_params(&env, &p);
    }

    /// Overrides the Reflector asset encoding for a token, for feeds published
    /// under a symbol rather than a contract address.
    pub fn set_oracle_symbol(
        env: Env,
        admin_addr: Address,
        token: Address,
        symbol: Option<Symbol>,
    ) {
        Self::require_admin(&env, &admin_addr);
        match symbol {
            Some(sym) => env
                .storage()
                .persistent()
                .set(&DataKey::OracleSymbol(token), &sym),
            None => env
                .storage()
                .persistent()
                .remove(&DataKey::OracleSymbol(token)),
        }
    }

    /// Registers the Aquarius pool used to sell a reward token for underlying.
    pub fn set_reward_route(
        env: Env,
        admin_addr: Address,
        reward_token: Address,
        route: Option<Address>,
    ) {
        Self::require_admin(&env, &admin_addr);
        match route {
            Some(pool) => env
                .storage()
                .persistent()
                .set(&DataKey::RewardRoute(reward_token), &pool),
            None => env
                .storage()
                .persistent()
                .remove(&DataKey::RewardRoute(reward_token)),
        }
    }

    /// Forces a fresh oracle read of the NAV price ratio.
    ///
    /// Permissionless, and the mirror of `receipt-vault`'s
    /// `refresh_boosted_underlying`: keepers call it so user transactions get a
    /// current value without paying the oracle's footprint cost themselves.
    pub fn refresh_nav_root(env: Env) -> u128 {
        bump_critical_ttl(&env);
        Self::refresh_nav_root_inner(&env)
    }

    pub fn set_max_pool_divergence_bps(env: Env, admin_addr: Address, bps: u32) {
        Self::require_admin(&env, &admin_addr);
        if bps >= BPS_DENOM {
            panic!("divergence must be below 100%");
        }
        let mut p = params(&env);
        let old = p.max_pool_divergence_bps;
        p.max_pool_divergence_bps = bps;
        set_params(&env, &p);
        ConfigChanged {
            what: Symbol::new(&env, "max_divergence"),
            old_value: old as u128,
            new_value: bps as u128,
        }
        .publish(&env);
    }

    pub fn set_nav_root_max_stale(env: Env, admin_addr: Address, seconds: u64) {
        Self::require_admin(&env, &admin_addr);
        let mut p = params(&env);
        let old = p.nav_root_max_stale;
        p.nav_root_max_stale = seconds;
        set_params(&env, &p);
        ConfigChanged {
            what: Symbol::new(&env, "nav_max_stale"),
            old_value: old as u128,
            new_value: seconds as u128,
        }
        .publish(&env);
    }

    pub fn set_nav_root_max_age(env: Env, admin_addr: Address, seconds: u64) {
        Self::require_admin(&env, &admin_addr);
        let mut p = params(&env);
        let old = p.nav_root_max_age;
        p.nav_root_max_age = seconds;
        set_params(&env, &p);
        ConfigChanged {
            what: Symbol::new(&env, "nav_root_age"),
            old_value: old as u128,
            new_value: seconds as u128,
        }
        .publish(&env);
    }

    /// Reconciles locally tracked liquidity against the pool's own view.
    ///
    /// Permissionless: it can only ever replace the local number with the
    /// pool's authoritative one, so there is nothing to gain from calling it.
    pub fn sync_liquidity(env: Env) -> u128 {
        bump_critical_ttl(&env);
        let pool = Self::pool(&env);
        let me = env.current_contract_address();
        let snapshot: UserPositionSnapshot =
            match try_call(&env, &pool, "get_user_position_snapshot", (me,)) {
                Ok(v) => v,
                Err(ref e) => {
                    emit_call_failure(&env, &pool, e, true);
                    return Self::position_liquidity(&env);
                }
            };
        Self::set_position_liquidity(&env, snapshot.raw_liquidity);
        snapshot.raw_liquidity
    }

    /// Deploys any idle underlying. Permissionless — it only ever moves the
    /// vault's own cash into the position it is configured for.
    pub fn deploy(env: Env) -> u128 {
        bump_critical_ttl(&env);
        Self::require_not_paused(&env);
        Self::deploy_idle(&env)
    }

    // ─────────────────────────────────────────────────────────────────────
    // Views
    // ─────────────────────────────────────────────────────────────────────

    pub fn get_pool(env: Env) -> Address {
        Self::pool(&env)
    }

    pub fn get_underlying(env: Env) -> Address {
        Self::underlying(&env)
    }

    pub fn get_tokens(env: Env) -> Vec<Address> {
        let mut out = Vec::new(&env);
        out.push_back(Self::token(&env, 0));
        out.push_back(Self::token(&env, 1));
        out
    }

    pub fn get_ticks(env: Env) -> (i32, i32) {
        Self::ticks(&env)
    }

    pub fn get_total_underlying(env: Env) -> i128 {
        to_i128(Self::total_underlying(&env))
    }

    pub fn get_position_liquidity(env: Env) -> u128 {
        Self::position_liquidity(&env)
    }

    /// Idle paired-token balance, valued in the underlying at the oracle rate.
    pub fn get_other_idle_value(env: Env) -> i128 {
        to_i128(Self::other_idle_value(&env))
    }

    pub fn get_idle(env: Env) -> i128 {
        to_i128(Self::balance_of_token(&env, &Self::underlying(&env)))
    }

    pub fn get_max_deploy(env: Env) -> u128 {
        params(&env).max_deploy
    }

    pub fn get_slippage_bps(env: Env) -> u32 {
        Self::slippage_bps(&env)
    }

    pub fn is_paused(env: Env) -> bool {
        params(&env).paused
    }

    pub fn get_last_harvest(env: Env) -> u64 {
        state(&env).last_harvest
    }

    pub fn get_config(env: Env) -> Config {
        config(&env)
    }

    pub fn get_params(env: Env) -> Params {
        params(&env)
    }

    pub fn get_reward_route(env: Env, reward_token: Address) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::RewardRoute(reward_token))
    }

    // ─────────────────────────────────────────────────────────────────────
    // Upgrade (timelocked, same shape as the other Peridot contracts)
    // ─────────────────────────────────────────────────────────────────────

    pub fn propose_upgrade_wasm(env: Env, admin_addr: Address, new_wasm_hash: BytesN<32>) {
        Self::require_admin(&env, &admin_addr);
        let eta = env
            .ledger()
            .timestamp()
            .saturating_add(UPGRADE_TIMELOCK_SECS);
        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgradeHash, &new_wasm_hash);
        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgradeEta, &eta);
        bump_pending_upgrade_ttl(&env);
    }

    pub fn upgrade_wasm(env: Env, admin_addr: Address, new_wasm_hash: BytesN<32>) {
        Self::require_admin(&env, &admin_addr);
        bump_pending_upgrade_ttl(&env);
        let pending: BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgradeHash)
            .expect("pending upgrade not set");
        let eta: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgradeEta)
            .expect("pending upgrade eta not set");
        if pending != new_wasm_hash {
            panic!("upgrade hash mismatch");
        }
        if env.ledger().timestamp() < eta {
            panic!("upgrade timelocked");
        }
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeHash);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeEta);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    fn require_admin(env: &Env, who: &Address) {
        if admin(env) != *who {
            panic!("not admin");
        }
        bump_critical_ttl(env);
        who.require_auth();
    }
}

fn expected_admin_config() -> &'static str {
    if cfg!(any(test, feature = "test-default-admin")) {
        option_env!("AQUARIUS_LP_VAULT_INIT_ADMIN").unwrap_or(DEFAULT_INIT_ADMIN)
    } else {
        option_env!("AQUARIUS_LP_VAULT_INIT_ADMIN")
            .expect("AQUARIUS_LP_VAULT_INIT_ADMIN must be set at build time")
    }
}
