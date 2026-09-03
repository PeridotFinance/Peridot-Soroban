use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, token, Address, BytesN, Env, IntoVal, Map, String, Symbol, Val, Vec,
};

use crate::constants::*;
use crate::events::*;
use crate::math::*;
use crate::oracle::{Asset as OracleAsset, PriceData};
use crate::pool::{ConcentratedPoolClient, Slot0, UserPositionSnapshot};
use crate::storage::*;

pub const DEFAULT_INIT_ADMIN: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";

struct SwapGuard {
    max_divergence_bps: u32,
    two_sided: bool,
    exit_root: Option<u128>,
}

struct DepositResult {
    minted: u128,
    idle_underlying: u128,
    idle_other: u128,
}

impl DepositResult {
    fn unchanged(idle_underlying: u128, idle_other: u128) -> Self {
        Self {
            minted: 0,
            idle_underlying,
            idle_other,
        }
    }
}

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

    fn is_legacy_full_range(env: &Env) -> bool {
        let (lower, upper) = Self::ticks(env);
        lower <= -800_000 && upper >= 800_000
    }

    fn slippage_bps(env: &Env) -> u32 {
        params(env).slippage_bps
    }

    fn receipt_vault(env: &Env) -> Address {
        bound_receipt_vault(env).expect("receipt vault not bound")
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

    /// Liquidity units this vault holds in its single active position.
    ///
    /// Tracked locally rather than read from the pool on every call: the value
    /// is known exactly from `deposit_position` / `withdraw_position` return
    /// values, and keeping it local means the hot NAV path costs no
    /// cross-contract call and cannot be bricked by a pool TTL expiry.
    /// `sync_liquidity()` reconciles it against the pool on demand.
    fn position_liquidity(env: &Env) -> u128 {
        if let Some(snapshot) = position_amounts(env) {
            return snapshot.liquidity;
        }
        state(env).position_liquidity
    }

    fn set_position_liquidity(env: &Env, value: u128) {
        if let Some(mut snapshot) = position_amounts(env) {
            snapshot.liquidity = value;
            set_position_amounts(env, &snapshot);
            return;
        }
        let mut st = state(env);
        st.position_liquidity = value;
        set_state(env, &st);
    }

    fn position_amounts_value_at_root(env: &Env, amounts: &PositionAmounts, root: u128) -> u128 {
        let cfg = config(env);
        let (underlying_amount, other_amount) = if cfg.underlying_index == 0 {
            (amounts.amount0, amounts.amount1)
        } else {
            (amounts.amount1, amounts.amount0)
        };
        if root == 0 {
            return 0;
        }
        let r_scaled = root.checked_mul(root).expect("nav root overflow");
        underlying_amount.saturating_add(mul_div(env, other_amount, r_scaled, NAV_RATIO_SCALE))
    }

    fn record_position(env: &Env, liquidity: u128, amount0: u128, amount1: u128) {
        Self::record_position_at(env, liquidity, amount0, amount1, env.ledger().timestamp());
    }

    fn record_position_at(
        env: &Env,
        liquidity: u128,
        amount0: u128,
        amount1: u128,
        updated_at: u64,
    ) {
        set_position_amounts(
            env,
            &PositionAmounts {
                liquidity,
                amount0,
                amount1,
                updated_at,
            },
        );
    }

    /// Refreshes the actual token composition of the active concentrated
    /// position. This is deliberately keeper-side: pulling the pool estimate
    /// into ReceiptVault's quote path would exceed Soroban's footprint limit.
    fn refresh_position_amounts_inner(env: &Env) -> bool {
        let liquidity = Self::position_liquidity(env);
        if liquidity == 0 {
            Self::record_position(env, 0, 0, 0);
            return true;
        }
        let pool = Self::pool(env);
        let me = env.current_contract_address();
        let (tick_lower, tick_upper) = Self::ticks(env);
        let estimate: Vec<u128> = match try_call::<Vec<u128>, _>(
            env,
            &pool,
            "estimate_withdraw_position",
            (me, tick_lower, tick_upper, liquidity),
        ) {
            Ok(v) => v,
            Err(ref e) => {
                emit_call_failure(env, &pool, e, true);
                return false;
            }
        };
        if estimate.len() != 2 {
            return false;
        }
        Self::record_position(
            env,
            liquidity,
            estimate.get(0).unwrap_or(0),
            estimate.get(1).unwrap_or(0),
        );
        true
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
        // Once the hard stale bound is crossed, public quote/user paths fail
        // soft without even touching the oracle. Besides being conservative,
        // this keeps a dead Reflector out of the ReceiptVault withdrawal
        // footprint. `refresh_nav_root` calls the inner function directly, so
        // a keeper can still recover the cache as soon as the feed returns.
        if cached.last_nav_root > 0
            && prm.nav_root_max_stale != 0
            && now.saturating_sub(cached.last_nav_root_at) > prm.nav_root_max_stale
        {
            return 0;
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
                let ratio = try_mul_div(env, po, NAV_RATIO_SCALE, pu).and_then(|ratio| {
                    try_mul_div(
                        env,
                        ratio,
                        pow10(Self::decimals_at(&cfg, u_idx)),
                        pow10(Self::decimals_at(&cfg, o_idx)),
                    )
                });
                if let Some(ratio) = ratio {
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
        }
        // Oracle unavailable: fall back to the last good root rather than
        // reporting a zero NAV, which would wipe out the backing market's
        // exchange rate. The fallback is bounded — past `nav_root_max_stale`
        // the value is too old to price borrowing or minting against, so
        // public quote paths return zero and let `receipt-vault` degrade to
        // its own cached/accounting value. Never panic here: this function is
        // reachable from supplier quote and withdrawal paths.
        let st = state(env);
        let age = env.ledger().timestamp().saturating_sub(st.last_nav_root_at);
        let max_stale = params(env).nav_root_max_stale;
        if max_stale != 0 && age > max_stale {
            return 0;
        }
        st.last_nav_root
    }

    fn position_value_at_root(env: &Env, root: u128, allow_stale_exit: bool) -> u128 {
        let liq = Self::position_liquidity(env);
        if liq == 0 || root == 0 {
            return 0;
        }
        if let Some(amounts) = position_amounts(env) {
            let max_stale = params(env).nav_root_max_stale;
            if !allow_stale_exit
                && max_stale != 0
                && env.ledger().timestamp().saturating_sub(amounts.updated_at) > max_stale
            {
                // Concentrated NAV depends on the position's current token
                // composition. Never keep valuing an indefinitely stale mix;
                // fail soft so the ReceiptVault can use its bounded cache for
                // exits while blocking new deposits against obsolete state.
                return 0;
            }
            return Self::position_amounts_value_at_root(env, &amounts, root);
        }
        if !Self::is_legacy_full_range(env) {
            // Never apply the full-range formula to a concentrated position.
            // A missing keeper snapshot fails soft to zero so ReceiptVault can
            // use its bounded cache for exits without authorizing new minting.
            return 0;
        }
        mul_div(env, liq, root, NAV_ROOT_SCALE)
            .checked_mul(2)
            .expect("nav overflow")
    }

    fn other_idle_value_at_root(env: &Env, root: u128) -> u128 {
        let other_balance = Self::balance_of_token(env, &Self::other_token(env));
        Self::other_amount_value_at_root(env, other_balance, root)
    }

    fn other_amount_value_at_root(env: &Env, other_amount: u128, root: u128) -> u128 {
        if other_amount == 0 || root == 0 {
            return 0;
        }
        let r_scaled = root.checked_mul(root).expect("nav root overflow");
        mul_div(env, other_amount, r_scaled, NAV_RATIO_SCALE)
    }

    /// Value of the active position, denominated in the underlying token.
    ///
    /// Concentrated positions use a keeper-refreshed token-amount snapshot and
    /// value both legs at the independent oracle ratio. Legacy full-range
    /// deployments retain their exact `2 * L * sqrt(P)` fallback only until
    /// their first snapshot/migration.
    ///
    /// Unclaimed swap fees are deliberately excluded, which understates NAV.
    /// Erring low is the safe direction for a lending market; `harvest()`
    /// folds the fees back into the position where they do get counted.
    fn position_value(env: &Env) -> u128 {
        let root = Self::nav_root(env);
        Self::position_value_at_root(env, root, false)
    }

    /// Value of an idle paired-token balance, expressed in the underlying.
    ///
    /// `nav_root` is `sqrt(R) * 1e9` where one raw underlying buys `1/R` raw
    /// other, so one raw other is worth `R` raw underlying.
    fn other_idle_value(env: &Env) -> u128 {
        let root = Self::nav_root(env);
        Self::other_idle_value_at_root(env, root)
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

    fn total_underlying_full_from_balances(
        env: &Env,
        idle_underlying: u128,
        idle_other: u128,
    ) -> u128 {
        let root = Self::nav_root(env);
        idle_underlying
            .saturating_add(Self::position_value_at_root(env, root, false))
            .saturating_add(Self::other_amount_value_at_root(env, idle_other, root))
    }

    /// Exit-only valuation. Once the public NAV has exceeded its stale bound,
    /// a caller must supply a non-zero underlying floor before the last known
    /// ratio can be used to size an unwind. The pool quote is still checked
    /// against that ratio and the final payout must meet the caller's floor.
    fn exit_nav_root(env: &Env, min_underlying_out: u128) -> u128 {
        let current = Self::nav_root(env);
        if current > 0 {
            return current;
        }
        if min_underlying_out == 0 {
            panic!("stale nav exit requires minimum");
        }
        let cached = state(env).last_nav_root;
        if cached == 0 {
            panic!("no cached nav for exit");
        }
        cached
    }

    fn total_underlying_full_at_root(env: &Env, root: u128) -> u128 {
        Self::balance_of_token(env, &Self::underlying(env))
            .saturating_add(Self::position_value_at_root(env, root, true))
            .saturating_add(Self::other_idle_value_at_root(env, root))
    }

    /// Conservative liquidation value exposed to the backing ReceiptVault.
    ///
    /// `total_underlying` is an oracle-valued NAV. Redeeming an LP position,
    /// however, settles the paired leg through the pool and can realize less
    /// underlying when pool spot is away from the oracle or execution moves
    /// after the quote. If the gross NAV were exposed here, ReceiptVault would
    /// redeem too few shares and an otherwise solvent borrow/withdraw could
    /// fail its live-cash post-check by the swap fee alone.
    ///
    /// A concentrated position can be entirely in the paired token near or
    /// outside an edge, so its conservative exit factor is `1 - divergence`
    /// rather than the milder full-range square-root factor. Apply that and
    /// execution slippage so the socket reports realizable rather than gross
    /// NAV. A quote beyond the divergence bound is refused on both entry and
    /// exit; the withdrawal reverts atomically and preserves shares.
    fn receipt_quote_value(env: &Env, gross: u128) -> u128 {
        if gross == 0 {
            return 0;
        }
        let prm = params(env);
        let bps = BPS_DENOM as u128;
        let divergence = (prm.max_pool_divergence_bps as u128).min(bps - 1);
        let slippage = (prm.slippage_bps as u128).min(bps - 1);
        let divergence_factor = bps - divergence;
        let execution_factor = bps - slippage;
        let combined_factor = mul_div(env, divergence_factor, execution_factor, bps);
        mul_div(env, gross, combined_factor, bps)
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
    fn require_quote_near_oracle(
        env: &Env,
        in_idx: u32,
        in_amount: u128,
        quoted_out: u128,
        exit_root: Option<u128>,
        max_div: u32,
        two_sided: bool,
    ) {
        if max_div == 0 {
            return;
        }
        let current_root = Self::nav_root(env);
        let root = if current_root > 0 {
            current_root
        } else {
            exit_root.unwrap_or(0)
        };
        if root == 0 {
            panic!("nav unavailable for swap");
        }
        let r_scaled = root.checked_mul(root).expect("nav root overflow"); // R * 1e18
        let fair_out = if in_idx == Self::underlying_index(env) {
            // underlying -> other: out = in / R
            mul_div(env, in_amount, NAV_RATIO_SCALE, r_scaled)
        } else {
            // other -> underlying: out = in * R
            mul_div(env, in_amount, r_scaled, NAV_RATIO_SCALE)
        };
        if fair_out == 0 {
            return;
        }
        if quoted_out < apply_slippage_floor(env, fair_out, max_div) {
            panic!("pool price diverged from oracle");
        }
        if two_sided {
            let upper = mul_div(
                env,
                fair_out,
                BPS_DENOM as u128 + max_div as u128,
                BPS_DENOM as u128,
            );
            if quoted_out > upper {
                panic!("pool price diverged from oracle");
            }
        }
    }

    fn desired_range(env: &Env) -> Option<(i32, i32, i32, i32)> {
        let policy = range_params(env);
        if !policy.enabled || policy.half_width_ticks == 0 {
            return None;
        }
        let pool = Self::pool(env);
        let tick_spacing: i32 = match try_call(env, &pool, "get_tick_spacing", ()) {
            Ok(v) => v,
            Err(ref e) => {
                emit_call_failure(env, &pool, e, true);
                return None;
            }
        };
        let slot: Slot0 = match try_call(env, &pool, "get_slot0", ()) {
            Ok(v) => v,
            Err(ref e) => {
                emit_call_failure(env, &pool, e, true);
                return None;
            }
        };
        let (lower, upper) = centered_range(
            slot.tick,
            tick_spacing,
            policy.half_width_ticks,
            MAX_TICK_ABS,
        );
        Some((slot.tick, tick_spacing, lower, upper))
    }

    fn range_triggered(env: &Env, spot_tick: i32, desired_lower: i32, desired_upper: i32) -> bool {
        let (current_lower, current_upper) = Self::ticks(env);
        if current_lower == desired_lower && current_upper == desired_upper {
            return false;
        }
        if Self::is_legacy_full_range(env) {
            return true;
        }
        let margin = i32::try_from(range_params(env).rebalance_margin_ticks)
            .expect("rebalance margin too large");
        spot_tick <= current_lower.saturating_add(margin)
            || spot_tick >= current_upper.saturating_sub(margin)
    }

    fn rebalance_due(env: &Env, spot_tick: i32, desired_lower: i32, desired_upper: i32) -> bool {
        let policy = range_params(env);
        let last = range_state(env).last_rebalance_at;
        if last != 0 && env.ledger().timestamp() < last.saturating_add(policy.rebalance_cooldown) {
            return false;
        }
        Self::range_triggered(env, spot_tick, desired_lower, desired_upper)
    }

    /// Burns the complete old position without swapping either returned leg.
    /// Used only by `rebalance`, where keeping the pair intact avoids a costly
    /// sell-and-buy round trip before creating the new centered position.
    fn withdraw_all_for_rebalance(env: &Env) -> Option<u128> {
        let liquidity = Self::position_liquidity(env);
        if liquidity == 0 {
            Self::record_position(env, 0, 0, 0);
            return Some(0);
        }
        let pool = Self::pool(env);
        let me = env.current_contract_address();
        let (tick_lower, tick_upper) = Self::ticks(env);
        let estimate: Vec<u128> = match try_call::<Vec<u128>, _>(
            env,
            &pool,
            "estimate_withdraw_position",
            (me.clone(), tick_lower, tick_upper, liquidity),
        ) {
            Ok(v) if v.len() == 2 => v,
            Ok(_) => return None,
            Err(ref e) => {
                emit_call_failure(env, &pool, e, true);
                return None;
            }
        };
        let mut mins = Vec::new(env);
        mins.push_back(apply_slippage_floor(
            env,
            estimate.get(0).unwrap_or(0),
            Self::slippage_bps(env),
        ));
        mins.push_back(apply_slippage_floor(
            env,
            estimate.get(1).unwrap_or(0),
            Self::slippage_bps(env),
        ));

        let args: Vec<Val> =
            (me.clone(), tick_lower, tick_upper, liquidity, mins.clone()).into_val(env);
        let mut auths = Vec::new(env);
        auths.push_back(auth_entry(
            env,
            &pool,
            "withdraw_position",
            args,
            Vec::new(env),
        ));
        let token0 = Self::token(env, 0);
        let token1 = Self::token(env, 1);
        let before0 = Self::balance_of_token(env, &token0);
        let before1 = Self::balance_of_token(env, &token1);
        env.authorize_as_current_contract(auths);
        let result: Result<Vec<u128>, CallError> = try_call(
            env,
            &pool,
            "withdraw_position",
            (me, tick_lower, tick_upper, liquidity, mins.clone()),
        );
        if let Err(ref e) = result {
            emit_call_failure(env, &pool, e, true);
            return None;
        }
        let got0 = Self::balance_of_token(env, &token0).saturating_sub(before0);
        let got1 = Self::balance_of_token(env, &token1).saturating_sub(before1);
        if got0 < mins.get(0).unwrap_or(0) || got1 < mins.get(1).unwrap_or(0) {
            panic!("pool withdrawal below minimum");
        }
        Self::record_position(env, 0, 0, 0);
        Some(liquidity)
    }

    /// Converts only the excess side of an unwind so a range centered at the
    /// current price receives approximately equal value on both legs. At an
    /// old range edge the withdrawn position can be almost entirely one token;
    /// offering that pair unchanged would leave most capital idle.
    fn balance_pair_for_centered_range(env: &Env, root: u128, max_divergence_bps: u32) -> u128 {
        if root == 0 {
            panic!("nav unavailable for rebalance");
        }
        let underlying = Self::underlying(env);
        let other = Self::other_token(env);
        let amount_u = Self::balance_of_token(env, &underlying);
        let amount_o = Self::balance_of_token(env, &other);
        let r_scaled = root.checked_mul(root).expect("nav root overflow");
        let other_value = mul_div(env, amount_o, r_scaled, NAV_RATIO_SCALE);
        let total_value = amount_u.saturating_add(other_value);
        let target_value = total_value / 2;
        let u_idx = Self::underlying_index(env);
        let o_idx = 1 - u_idx;

        let mut price_checked = false;
        if amount_u.saturating_sub(target_value) >= MIN_DEPLOY_AMOUNT {
            let excess = amount_u - target_value;
            let received = Self::swap_exact_in(
                env,
                u_idx,
                o_idx,
                excess,
                Some(SwapGuard {
                    max_divergence_bps,
                    two_sided: true,
                    exit_root: Some(root),
                }),
                Some(amount_u),
            );
            if received == 0 {
                panic!("rebalance balancing swap failed");
            }
            price_checked = true;
        } else if other_value.saturating_sub(target_value) >= MIN_DEPLOY_AMOUNT {
            let excess_value = other_value - target_value;
            let excess_other =
                mul_div_ceil(env, excess_value, NAV_RATIO_SCALE, r_scaled).min(amount_o);
            let received = Self::swap_exact_in(
                env,
                o_idx,
                u_idx,
                excess_other,
                Some(SwapGuard {
                    max_divergence_bps,
                    two_sided: true,
                    exit_root: Some(root),
                }),
                Some(amount_o),
            );
            if received == 0 {
                panic!("rebalance balancing swap failed");
            }
            price_checked = true;
        }
        if !price_checked && total_value > 0 {
            // A perfectly balanced unwind needs no trade, but the pool tick
            // still determines the new range. One read-only quote keeps that
            // centering decision under the same two-sided oracle bound.
            let probe = pow10(Self::decimals_at(&config(env), u_idx));
            let quoted: u128 = try_call(
                env,
                &Self::pool(env),
                "estimate_swap",
                (u_idx, o_idx, probe),
            )
            .unwrap_or_else(|_| panic!("rebalance price unavailable"));
            if quoted == 0 {
                panic!("rebalance price unavailable");
            }
            Self::require_quote_near_oracle(
                env,
                u_idx,
                probe,
                quoted,
                Some(root),
                max_divergence_bps,
                true,
            );
        }
        total_value
    }

    /// Swaps `in_amount` of token `in_idx` for the other token.
    ///
    /// Returns the amount received, or `0` if the pool refused. Never panics on
    /// a pool-side failure: Aquarius can pause swaps (error 206) and a revert
    /// here would propagate into the backing market's deposit or withdrawal.
    ///
    /// `max_divergence_bps` is set for both position entry and the paired leg
    /// of position exit. A dislocated exit therefore reverts atomically,
    /// preserving the supplier's shares until the pool returns within bounds
    /// or governance deliberately changes the bound. Reward conversions keep
    /// it disabled because their token pair may not have this vault's oracle
    /// mapping.
    fn swap_exact_in(
        env: &Env,
        in_idx: u32,
        out_idx: u32,
        in_amount: u128,
        guard: Option<SwapGuard>,
        input_balance_before: Option<u128>,
    ) -> u128 {
        if in_amount == 0 {
            return 0;
        }
        let pool = Self::pool(env);
        let in_token = Self::token(env, in_idx);
        let out_token = Self::token(env, out_idx);

        let estimated: u128 =
            match try_call(env, &pool, "estimate_swap", (in_idx, out_idx, in_amount)) {
                Ok(v) => v,
                Err(ref e) => {
                    emit_call_failure(env, &pool, e, true);
                    return 0;
                }
            };
        if estimated == 0 {
            return 0;
        }
        if let Some(guard) = guard {
            Self::require_quote_near_oracle(
                env,
                in_idx,
                in_amount,
                estimated,
                guard.exit_root,
                guard.max_divergence_bps,
                guard.two_sided,
            );
        }
        let out_min = apply_slippage_floor(env, estimated, Self::slippage_bps(env));

        let me = env.current_contract_address();
        let transfer_args: Vec<Val> = (me.clone(), pool.clone(), to_i128(in_amount)).into_val(env);

        // The Aquarius pool does not call `user.require_auth()` on `swap` — it
        // relies on the token transfer's own auth. So the transfer entry must
        // sit at the ROOT of the authorization list; nested under a pool-call
        // entry it is unreachable and the transfer fails with
        // Auth(InvalidAction). Verified on testnet against the deployed pool.
        let mut auths = Vec::new(env);
        auths.push_back(auth_entry(
            env,
            &in_token,
            "transfer",
            transfer_args,
            Vec::new(env),
        ));

        // Balance first: `authorize_as_current_contract` only covers the next
        // contract call, so an intervening read discards the authorization.
        // The deployed pool requires a root transfer entry. Since invoker auth
        // entries are not consumable, also cap the transaction-atomic input
        // balance delta whenever the vault has enough input for a replay to
        // succeed. Full-balance swaps need no extra read: a repeated transfer
        // necessarily fails inside the token contract and rolls the pool call
        // back. Callers pass the balance they already read before entering.
        let out_before = Self::balance_of_token(env, &out_token);
        env.authorize_as_current_contract(auths);
        let res: Result<u128, CallError> = try_call(
            env,
            &pool,
            "swap",
            (me, in_idx, out_idx, in_amount, out_min),
        );
        if let Err(ref e) = res {
            emit_call_failure(env, &pool, e, true);
            return 0;
        }
        if let Some(in_before) = input_balance_before {
            if in_before >= in_amount.saturating_mul(2) {
                let in_after = Self::balance_of_token(env, &in_token);
                if in_before.saturating_sub(in_after) > in_amount {
                    panic!("pool exceeded swap authorization");
                }
            }
        }
        let received = Self::balance_of_token(env, &out_token).saturating_sub(out_before);
        if received < out_min {
            panic!("pool swap output below minimum");
        }
        received
    }

    /// Deposits the offered pair balances into the currently configured range.
    /// The pool receives authority for no more than its exact two-entry quote;
    /// the returned spend vector is bounded by that quote before it updates
    /// the position cache.
    fn deposit_balances(
        env: &Env,
        amount_u: u128,
        amount_o: u128,
        balance_u_before: u128,
        balance_o_before: u128,
    ) -> DepositResult {
        if amount_u == 0 || amount_o == 0 {
            return DepositResult::unchanged(balance_u_before, balance_o_before);
        }
        let pool = Self::pool(env);
        let u_idx = Self::underlying_index(env);
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
        // both happen inside this transaction. The returned vector is then
        // reconciled to both actual token balance deltas before accounting is
        // updated; the caller reuses those post-deposit balances so the check
        // does not increase the overall live-migration footprint.
        // Aquarius can pause deposits and swaps at will (errors 205/206) — those
        // are their kill switches on their contract, and nothing on our side
        // can prevent that. What we *can* do is refuse to propagate it: a
        // paused pool must leave the cash sitting idle here, not revert the
        // user's deposit into the market above. `receipt-vault` invokes our
        // `deposit` directly (contract.rs:330), so a panic here would take the
        // whole market deposit down with it.
        let quote: Result<(Vec<u128>, u128), CallError> = try_call(
            env,
            &pool,
            "estimate_deposit_position",
            (tick_lower, tick_upper, desired.clone()),
        );
        let (actual, est_liquidity) = match quote {
            Ok(v) => v,
            Err(ref e) => {
                emit_call_failure(env, &pool, e, true);
                return DepositResult::unchanged(balance_u_before, balance_o_before);
            }
        };
        if actual.len() != 2 {
            return DepositResult::unchanged(balance_u_before, balance_o_before);
        }
        let actual0 = actual.get(0).unwrap_or(0);
        let actual1 = actual.get(1).unwrap_or(0);
        let desired0 = desired.get(0).unwrap_or(0);
        let desired1 = desired.get(1).unwrap_or(0);
        // The authorization below is the maximum authority the untrusted pool
        // receives. Never let its estimate enlarge that authority beyond the
        // two amounts this vault deliberately offered.
        if actual0 == 0 || actual1 == 0 || actual0 > desired0 || actual1 > desired1 {
            return DepositResult::unchanged(balance_u_before, balance_o_before);
        }
        if est_liquidity == 0 {
            return DepositResult::unchanged(balance_u_before, balance_o_before);
        }
        let min_liquidity = apply_slippage_floor(env, est_liquidity, Self::slippage_bps(env));

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
        let (balance0_before, balance1_before) = if u_idx == 0 {
            (balance_u_before, balance_o_before)
        } else {
            (balance_o_before, balance_u_before)
        };
        env.authorize_as_current_contract(auths);

        let deposited: Result<(Vec<u128>, u128), CallError> = try_call(
            env,
            &pool,
            "deposit_position",
            (
                me.clone(),
                tick_lower,
                tick_upper,
                actual.clone(),
                min_liquidity,
            ),
        );
        let (reported_spent, minted) = match deposited {
            Ok(v) => v,
            Err(ref e) => {
                // Deposits paused, or the pool rejected us. Leave the cash idle;
                // the next `deploy()` or deposit will retry once it reopens.
                emit_call_failure(env, &pool, e, true);
                return DepositResult::unchanged(balance_u_before, balance_o_before);
            }
        };
        if reported_spent.len() != 2 {
            panic!("pool returned invalid spend vector");
        }
        let reported0 = reported_spent.get(0).unwrap_or(0);
        let reported1 = reported_spent.get(1).unwrap_or(0);
        if reported0 > actual0 || reported1 > actual1 {
            panic!("pool exceeded deposit authorization");
        }
        let balance0_after = Self::balance_of_token(env, &Self::token(env, 0));
        let balance1_after = Self::balance_of_token(env, &Self::token(env, 1));
        let spent0 = balance0_before.saturating_sub(balance0_after);
        let spent1 = balance1_before.saturating_sub(balance1_after);
        if spent0 > actual0 || spent1 > actual1 {
            panic!("pool exceeded deposit authorization");
        }
        if spent0 != reported0 || spent1 != reported1 {
            panic!("pool deposit accounting mismatch");
        }
        if minted > 0 && (spent0 == 0 || spent1 == 0) {
            panic!("position minted without both assets");
        }
        if minted == 0 && (spent0 > 0 || spent1 > 0) {
            panic!("pool retained assets without minting liquidity");
        }
        if minted > 0 {
            let liquidity_before = Self::position_liquidity(env);
            let liquidity_after = liquidity_before.saturating_add(minted);
            // Preserve the exact legacy full-range fallback if this was a
            // top-up before its first snapshot; every concentrated position
            // always has an explicit amount cache.
            if position_amounts(env).is_none() && Self::is_legacy_full_range(env) {
                Self::set_position_liquidity(env, liquidity_after);
            } else {
                let current = position_amounts(env).unwrap_or(PositionAmounts {
                    liquidity: 0,
                    amount0: 0,
                    amount1: 0,
                    updated_at: env.ledger().timestamp(),
                });
                let updated_at = if current.liquidity == 0 {
                    env.ledger().timestamp()
                } else {
                    // Adding a quoted deposit to an older composition does
                    // not re-observe the existing position. Only a pool read
                    // may renew this timestamp.
                    current.updated_at
                };
                Self::record_position_at(
                    env,
                    liquidity_after,
                    current.amount0.saturating_add(spent0),
                    current.amount1.saturating_add(spent1),
                    updated_at,
                );
            }
        }
        let (idle_underlying, idle_other) = if u_idx == 0 {
            (balance0_after, balance1_after)
        } else {
            (balance1_after, balance0_after)
        };
        DepositResult {
            minted,
            idle_underlying,
            idle_other,
        }
    }

    /// Deploys idle underlying into the active concentrated position.
    ///
    /// Single-asset inflow is split in half; any paired residue already held
    /// by the vault is offered too. Aquarius chooses the exact range ratio and
    /// leaves the non-limiting residue idle and owned by existing shares.
    fn deploy_idle(env: &Env) -> Option<DepositResult> {
        let underlying = Self::underlying(env);
        let idle = Self::balance_of_token(env, &underlying);
        if idle < MIN_DEPLOY_AMOUNT {
            return None;
        }

        let max_deploy = params(env).max_deploy;
        let deployable = if max_deploy == 0 {
            idle
        } else {
            let deployed = Self::position_value(env);
            if deployed >= max_deploy {
                return None;
            }
            idle.min(max_deploy - deployed)
        };
        if deployable < MIN_DEPLOY_AMOUNT {
            return None;
        }

        let pool_addr = Self::pool(env);
        let deposits_killed: bool =
            try_call(env, &pool_addr, "get_is_killed_deposit", ()).unwrap_or(false);
        if deposits_killed {
            return None;
        }

        let u_idx = Self::underlying_index(env);
        let o_idx = 1 - u_idx;
        let half = deployable / 2;
        if half == 0 {
            return None;
        }
        Self::swap_exact_in(
            env,
            u_idx,
            o_idx,
            half,
            Some(SwapGuard {
                max_divergence_bps: params(env).max_pool_divergence_bps,
                two_sided: false,
                exit_root: None,
            }),
            Some(idle),
        );

        let balance_u = Self::balance_of_token(env, &underlying);
        let balance_o = Self::balance_of_token(env, &Self::other_token(env));
        let amount_u = balance_u.min(deployable - half);
        Some(Self::deposit_balances(
            env, amount_u, balance_o, balance_u, balance_o,
        ))
    }

    /// Raises `needed` underlying, cheapest source first.
    ///
    /// Idle paired token is sold before touching the LP position: it is part of
    /// NAV (so shares have a claim on it) but cannot be paid out directly, and
    /// selling it is cheaper than burning liquidity. Doing this also clears the
    /// residue a deposit swap leaves behind.
    /// Returns `false` if the LP redemption leg failed outright, so the caller
    /// can tell "the pool is down" apart from "exiting simply costs fees".
    fn raise_underlying(env: &Env, needed: u128, exit_root: u128) -> bool {
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
            return Self::redeem_for(env, needed.saturating_sub(have), exit_root);
        }
        // No position left: the paired balance is all there is to sell.
        if Self::other_idle_value_at_root(env, exit_root) >= MIN_DEPLOY_AMOUNT {
            let other_balance = Self::balance_of_token(env, &Self::other_token(env));
            Self::swap_exact_in(
                env,
                Self::other_index(env),
                Self::underlying_index(env),
                other_balance,
                Some(SwapGuard {
                    max_divergence_bps: params(env).max_pool_divergence_bps,
                    two_sided: false,
                    exit_root: Some(exit_root),
                }),
                None,
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
    fn redeem_for(env: &Env, needed: u128, exit_root: u128) -> bool {
        if needed == 0 {
            return true;
        }
        let liq = Self::position_liquidity(env);
        if liq == 0 {
            return false;
        }
        let position_value = Self::position_value_at_root(env, exit_root, true);
        if position_value == 0 {
            return false;
        }

        // Round up so a rounding shortfall does not leave the caller one unit
        // short and force a second round trip.
        let mut burn = mul_div_ceil(env, needed.min(position_value), liq, position_value);
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
            Ok(_) => {}
            Err(ref e) => {
                emit_call_failure(env, &pool, e, false);
                return false;
            }
        }

        let under_got = Self::balance_of_token(env, &underlying).saturating_sub(under_before);
        let other_got = Self::balance_of_token(env, &other).saturating_sub(other_before);
        let (amount0_got, amount1_got) = if Self::underlying_index(env) == 0 {
            (under_got, other_got)
        } else {
            (other_got, under_got)
        };
        let liquidity_after = liq.saturating_sub(burn);
        if burn == liq {
            Self::record_position(env, 0, 0, 0);
        } else if let Some(current) = position_amounts(env) {
            // Actual withdrawal deltas make the cached amounts more
            // conservative, but they do not reveal the complete remaining
            // composition. Preserve the original observation time.
            Self::record_position_at(
                env,
                liquidity_after,
                current.amount0.saturating_sub(amount0_got),
                current.amount1.saturating_sub(amount1_got),
                current.updated_at,
            );
        } else {
            Self::set_position_liquidity(env, liquidity_after);
        }

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
                Some(SwapGuard {
                    max_divergence_bps: params(env).max_pool_divergence_bps,
                    two_sided: false,
                    exit_root: Some(exit_root),
                }),
                None,
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
        // Do not let a few idle raw units masquerade as the full strategy
        // value when the LP position itself cannot be priced. ReceiptVault
        // interprets any positive quote as authoritative and would otherwise
        // replace its useful cache with dust during an oracle outage.
        if Self::position_liquidity(&env) > 0 && Self::nav_root(&env) == 0 {
            out.push_back(0i128);
            return out;
        }
        let gross_nav = Self::total_underlying(&env);
        let gross_claim = mul_div(&env, shares, gross_nav, supply);
        out.push_back(to_i128(Self::receipt_quote_value(&env, gross_claim)));
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

        // The ReceiptVault's supply cap is the product-level capacity limit.
        // Letting arbitrary accounts mint pAQLP shares here would bypass that
        // cap and dilute the yield of market suppliers, so capital may enter
        // through the one bound market only.
        if from != Self::receipt_vault(&env) {
            panic!("only receipt vault may deposit");
        }

        let amount = to_u128(amounts_desired.get(0).unwrap_or(0));
        if amount == 0 {
            return 0;
        }
        let min_amount = to_u128(amounts_min.get(0).unwrap_or(0));
        if amount < min_amount {
            panic!("deposit below minimum");
        }

        if Self::position_liquidity(&env) > 0 && !Self::is_legacy_full_range(&env) {
            let snapshot = position_amounts(&env).expect("position snapshot unavailable");
            let max_stale = params(&env).nav_root_max_stale;
            if max_stale != 0
                && env.ledger().timestamp().saturating_sub(snapshot.updated_at) > max_stale
            {
                panic!("position snapshot stale");
            }
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

        let deployment = if invest {
            Self::deploy_idle(&env)
        } else {
            None
        };
        let minted_liquidity = deployment.as_ref().map_or(0, |result| result.minted);
        let nav_after = match deployment.as_ref() {
            Some(result) => Self::total_underlying_full_from_balances(
                &env,
                result.idle_underlying,
                result.idle_other,
            ),
            None => Self::total_underlying_full(&env),
        };
        let value_added = nav_after.saturating_sub(nav_before);
        if value_added == 0 {
            panic!("deposit added no value");
        }

        let shares = if supply == 0 || nav_before == 0 {
            value_added
        } else {
            mul_div(&env, value_added, supply, nav_before)
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

        let min_out = to_u128(min_amounts_out.get(0).unwrap_or(0));
        let exit_root = Self::exit_nav_root(&env, min_out);

        // Value the same asset set deposits do. `redeem_for` sells the vault's
        // *entire* paired balance including residue, so pricing the exit
        // against a residue-excluding NAV would hand a partial exiter less
        // than their pro-rata claim and leave the difference to whoever stays.
        // Costs no extra ledger entry on any path that actually redeems —
        // `redeem_for` already has the paired token in footprint.
        let owed = mul_div(
            &env,
            shares,
            Self::total_underlying_full_at_root(&env, exit_root),
            supply,
        );
        let underlying = Self::underlying(&env);
        let liquidity_before = Self::position_liquidity(&env);

        let redeemed_ok = Self::raise_underlying(&env, owed, exit_root);

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
        } else if from == Self::receipt_vault(&env) {
            // ReceiptVault is the sole permitted strategy-share holder. Its
            // non-zero floor may be a few raw units above this cached NAV's
            // pro-rata result because it deliberately rounds strategy shares
            // up and can size a stale-oracle exit from its independent cache.
            // Moving that tiny difference into the same market's idle cash
            // does not transfer value between users; it keeps the protected
            // exit live while the final available/minimum checks still bound
            // the transfer.
            owed.max(min_out).min(available)
        } else {
            owed.min(available)
        };

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
    // Single-asset convenience entrypoints
    // ─────────────────────────────────────────────────────────────────────

    /// Single-sided deposit wrapper for the bound ReceiptVault. The vault
    /// handles the split; direct user calls are rejected by `deposit`.
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

    /// Strategy shares are deliberately non-transferable. Users hold the
    /// bound ReceiptVault's pTokens instead; keeping this selector as a
    /// fail-closed stub preserves ABI compatibility without allowing dust
    /// holders to create unbounded persistent share entries.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let _ = (env, from, to, amount);
        panic!("strategy shares are non-transferable");
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
        let route_key = DataKey::RewardRoute(reward_token.clone());
        bump_mapping_ttl(env, &route_key);
        let route: Option<Address> = env.storage().persistent().get(&route_key);
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
        let rate_key = DataKey::RewardMinRate(reward_token.clone());
        bump_mapping_ttl(env, &rate_key);
        let min_rate_scaled: u128 = env.storage().persistent().get(&rate_key).unwrap_or(0);
        if min_rate_scaled == 0 {
            HarvestSkipped {
                reward_token: reward_token.clone(),
                reward_amount: amount,
                reason: Symbol::new(env, "no_guard"),
            }
            .publish(env);
            return 0;
        }
        let configured_floor = mul_div(env, amount, min_rate_scaled, REWARD_RATE_SCALE);
        if configured_floor == 0 || estimated < configured_floor {
            HarvestSkipped {
                reward_token: reward_token.clone(),
                reward_amount: amount,
                reason: Symbol::new(env, "price_guard"),
            }
            .publish(env);
            return 0;
        }
        let out_min =
            apply_slippage_floor(env, estimated, Self::slippage_bps(env)).max(configured_floor);
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
        let underlying_before = Self::balance_of_token(env, &underlying);
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
        // `amount` is the vault's complete reward-token balance. Replaying the
        // exact transfer therefore fails inside SEP-41 and reverts the route
        // invocation without another cross-contract balance read here.
        Self::balance_of_token(env, &underlying).saturating_sub(underlying_before)
    }

    /// Permissionless: claim the configured primary reward, third-party gauge
    /// incentives and accrued swap fees, convert everything to underlying,
    /// and redeploy.
    ///
    /// Rate-limited because each call moves the share price; without a cooldown
    /// it could be used to grind rounding in the depositor's favour.
    pub fn harvest(env: Env, caller: Address) -> i128 {
        caller.require_auth();
        bump_critical_ttl(&env);

        let now = env.ledger().timestamp();
        let st = state(&env);
        let cooldown = params(&env).harvest_cooldown;
        if st.last_harvest != 0 && now < st.last_harvest.saturating_add(cooldown) {
            panic!("harvest on cooldown");
        }

        let pool = Self::pool(&env);
        let me = env.current_contract_address();
        let underlying = Self::underlying(&env);
        let other = Self::other_token(&env);
        let before_underlying = Self::balance_of_token(&env, &underlying);
        let mut did_work = false;

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
        match try_call::<Vec<u128>, _>(&env, &pool, "claim_all_position_fees", (me.clone(),)) {
            Ok(amounts) => {
                for amount in amounts.iter() {
                    if amount > 0 {
                        did_work = true;
                    }
                }
            }
            Err(ref e) => emit_call_failure(&env, &pool, e, true),
        }

        // 2. Primary Aquarius emissions (AQUA at launch).
        let claim_args: Vec<Val> = (me.clone(),).into_val(&env);
        let mut auths = Vec::new(&env);
        auths.push_back(auth_entry(&env, &pool, "claim", claim_args, Vec::new(&env)));
        env.authorize_as_current_contract(auths);
        match try_call::<u128, _>(&env, &pool, "claim", (me.clone(),)) {
            Ok(amount) => {
                if amount > 0 {
                    did_work = true;
                }
            }
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
        for (_, amount) in gauge_rewards.iter() {
            if amount > 0 {
                did_work = true;
            }
        }

        // 4. Sell every reward token that is not already part of the pair.
        //    Balances are read directly so donations and partially-claimed
        //    rewards are swept too.
        let mut reward_tokens: Vec<Address> = Vec::new(&env);
        // `claim()` returns only an amount, not the token address. Explicitly
        // include the configured primary token so AQUA is compounded even on
        // pools with no third-party gauges (both launch pools currently have
        // an empty gauge map).
        if let Some(primary) = primary_reward_token(&env) {
            if primary != underlying && primary != other {
                reward_tokens.push_back(primary);
            }
        }
        for (tok, _) in gauge_rewards.iter() {
            if tok != underlying && tok != other && !reward_tokens.contains(tok.clone()) {
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
            if got > 0 {
                did_work = true;
            }
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
            let got = Self::swap_exact_in(
                &env,
                Self::other_index(&env),
                Self::underlying_index(&env),
                other_balance,
                None,
                None,
            );
            if got > 0 {
                did_work = true;
            }
        }
        let deployment = Self::deploy_idle(&env);
        if deployment.as_ref().is_some_and(|result| result.minted > 0) {
            did_work = true;
        }

        if did_work {
            // Re-read after deployment: `deploy_idle` updates the same packed
            // state entry with new position liquidity, so writing the snapshot
            // taken at the start would clobber that accounting change.
            let mut final_state = state(&env);
            final_state.last_harvest = now;
            set_state(&env, &final_state);
        }

        let underlying_after = deployment.as_ref().map_or_else(
            || Self::balance_of_token(&env, &underlying),
            |r| r.idle_underlying,
        );
        let gained = underlying_after.saturating_sub(before_underlying);
        to_i128(gained)
    }

    /// Sells any configured reward held by the vault. Kept separate from
    /// `harvest` so a route can be exercised even when claiming is paused.
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

        // Range management relies on the deployed concentrated-pool ABI.
        // Refuse anything else rather than silently opening the wrong product.
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
        if dec0 > MAX_TOKEN_DECIMALS || dec1 > MAX_TOKEN_DECIMALS {
            panic!("token decimals too large");
        }

        let tick_spacing = pool_client.get_tick_spacing();
        let slot = pool_client.get_slot0();
        let (tick_lower, tick_upper) = centered_range(
            slot.tick,
            tick_spacing,
            DEFAULT_HALF_WIDTH_TICKS,
            MAX_TICK_ABS,
        );

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
        Self::record_position(&env, 0, 0, 0);
        set_range_params(
            &env,
            &RangeParams {
                half_width_ticks: DEFAULT_HALF_WIDTH_TICKS,
                rebalance_margin_ticks: DEFAULT_REBALANCE_MARGIN_TICKS,
                rebalance_cooldown: DEFAULT_REBALANCE_COOLDOWN_SECS,
                max_rebalance_divergence_bps: DEFAULT_MAX_REBALANCE_DIVERGENCE_BPS,
                enabled: true,
            },
        );
        set_range_state(
            &env,
            &RangeState {
                last_rebalance_at: 0,
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
        // Drop the cached ratio: it was derived from the *previous* oracle, and
        // leaving it valid would let the vault keep quoting the old feed's
        // valuation for the whole cache window — long enough to borrow against
        // a price the replacement oracle would reject.
        Self::invalidate_nav_cache(&env);
    }

    /// Permanently binds the sole ReceiptVault allowed to deposit.
    ///
    /// The candidate must already point back at this Aquarius vault and must
    /// settle in the same underlying token. This closes both the supply-cap
    /// bypass and an admin typo that would otherwise strand the deployment.
    pub fn set_receipt_vault(env: Env, admin_addr: Address, receipt_vault: Address) {
        Self::require_admin(&env, &admin_addr);
        let cfg = config(&env);
        if bound_receipt_vault(&env).is_some() {
            panic!("receipt vault already bound");
        }
        let attached: Option<Address> = try_call(&env, &receipt_vault, "get_boosted_vault", ())
            .unwrap_or_else(|_| panic!("invalid receipt vault"));
        if attached != Some(env.current_contract_address()) {
            panic!("receipt vault does not point to this vault");
        }
        let market_underlying: Address = try_call(&env, &receipt_vault, "get_underlying_token", ())
            .unwrap_or_else(|_| panic!("invalid receipt vault"));
        let expected_underlying = Self::token_at(&cfg, cfg.underlying_index);
        if market_underlying != expected_underlying {
            panic!("receipt vault underlying mismatch");
        }
        set_bound_receipt_vault(&env, &receipt_vault);
        ReceiptVaultBound { receipt_vault }.publish(&env);
    }

    /// Changes the token transferred by Aquarius' primary `claim()` path.
    /// Reward conversion routes remain independently configurable per token.
    pub fn set_primary_reward_token(env: Env, admin_addr: Address, reward_token: Option<Address>) {
        Self::require_admin(&env, &admin_addr);
        set_primary_reward(&env, &reward_token);
        PrimaryRewardTokenSet { reward_token }.publish(&env);
    }

    /// Forces the next valuation to re-read the oracle.
    fn invalidate_nav_cache(env: &Env) {
        let mut st = state(env);
        st.last_nav_root = 0;
        st.last_nav_root_at = 0;
        set_state(env, &st);
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
        let key = DataKey::OracleSymbol(token);
        match symbol {
            Some(sym) => {
                env.storage().persistent().set(&key, &sym);
                bump_mapping_ttl(&env, &key);
            }
            None => env.storage().persistent().remove(&key),
        }
        // Changing which asset the feed is queried for changes the price, so
        // the cached ratio is just as invalid as it is after `set_oracle` —
        // it was derived from the previous encoding. Same reasoning, same fix.
        Self::invalidate_nav_cache(&env);
    }

    /// Permissionless TTL renewal for the configured mappings.
    ///
    /// Both are persistent keys that user traffic may not touch often enough to
    /// keep alive on its own, and an archived symbol override stops the vault
    /// pricing at all — which blocks supplier exits, not just deposits.
    pub fn bump_config_mapping_ttl(env: Env, token: Address) {
        bump_mapping_ttl(&env, &DataKey::OracleSymbol(token.clone()));
        bump_mapping_ttl(&env, &DataKey::RewardRoute(token.clone()));
        bump_mapping_ttl(&env, &DataKey::RewardMinRate(token));
    }

    /// Registers the Aquarius pool used to sell a reward token for underlying.
    pub fn set_reward_route(
        env: Env,
        admin_addr: Address,
        reward_token: Address,
        route: Option<Address>,
    ) {
        Self::require_admin(&env, &admin_addr);
        let key = DataKey::RewardRoute(reward_token);
        match route {
            Some(pool) => {
                env.storage().persistent().set(&key, &pool);
                bump_mapping_ttl(&env, &key);
            }
            None => env.storage().persistent().remove(&key),
        }
    }

    /// Sets the minimum raw underlying returned per raw reward unit, scaled by
    /// `REWARD_RATE_SCALE` (1e7). A zero value removes the guard and therefore
    /// disables selling that reward until governance installs a new floor.
    ///
    /// The guard is intentionally independent of the route: changing a pool
    /// must not silently weaken the fair-value floor. Keepers should alert and
    /// leave rewards idle when a legitimate market move crosses the floor;
    /// governance can then review and update it.
    pub fn set_reward_min_rate(
        env: Env,
        admin_addr: Address,
        reward_token: Address,
        min_rate_scaled: u128,
    ) {
        Self::require_admin(&env, &admin_addr);
        let key = DataKey::RewardMinRate(reward_token);
        if min_rate_scaled == 0 {
            env.storage().persistent().remove(&key);
        } else {
            env.storage().persistent().set(&key, &min_rate_scaled);
            bump_mapping_ttl(&env, &key);
        }
    }

    /// Forces a fresh oracle read of the NAV price ratio.
    ///
    /// Permissionless, and the mirror of `receipt-vault`'s
    /// `refresh_boosted_underlying`: keepers call it so user transactions get a
    /// current value without paying the oracle's footprint cost themselves.
    pub fn refresh_nav_root(env: Env) -> u128 {
        bump_critical_ttl(&env);
        let root = Self::refresh_nav_root_inner(&env);
        if root > 0 {
            let _ = Self::refresh_position_amounts_inner(&env);
        }
        root
    }

    /// Permissionless position-composition refresh for keepers and recovery.
    /// Returns false without replacing the last good snapshot if Aquarius is
    /// temporarily unavailable or returns a malformed amount vector.
    pub fn refresh_position_amounts(env: Env) -> bool {
        bump_critical_ttl(&env);
        Self::refresh_position_amounts_inner(&env)
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

    /// Installs the actively managed concentrated-range policy.
    ///
    /// Changing policy does not move funds. The permissionless keeper path
    /// performs the migration only after its independent price guard passes.
    /// This separation keeps governance changes reviewable and lets existing
    /// Mainnet full-range deployments fail closed immediately after upgrade.
    pub fn set_range_policy(
        env: Env,
        admin_addr: Address,
        half_width_ticks: u32,
        rebalance_margin_ticks: u32,
        rebalance_cooldown: u64,
        max_rebalance_divergence_bps: u32,
        enabled: bool,
    ) {
        Self::require_admin(&env, &admin_addr);
        let pool = Self::pool(&env);
        let tick_spacing: i32 = try_call(&env, &pool, "get_tick_spacing", ())
            .unwrap_or_else(|_| panic!("tick spacing unavailable"));
        let spacing = u32::try_from(tick_spacing).expect("invalid tick spacing");
        if half_width_ticks < spacing.saturating_mul(2) || half_width_ticks > MAX_HALF_WIDTH_TICKS {
            panic!("invalid range width");
        }
        let aligned_half = half_width_ticks.div_ceil(spacing).saturating_mul(spacing);
        if rebalance_margin_ticks == 0 || rebalance_margin_ticks >= aligned_half {
            panic!("invalid rebalance margin");
        }
        if rebalance_cooldown < MIN_REBALANCE_COOLDOWN_SECS {
            panic!("rebalance cooldown too short");
        }
        if max_rebalance_divergence_bps == 0
            || max_rebalance_divergence_bps > MAX_REBALANCE_DIVERGENCE_BPS
        {
            panic!("invalid rebalance divergence");
        }
        let value = RangeParams {
            half_width_ticks,
            rebalance_margin_ticks,
            rebalance_cooldown,
            max_rebalance_divergence_bps,
            enabled,
        };
        set_range_params(&env, &value);
        RangePolicySet {
            enabled,
            half_width_ticks,
            rebalance_margin_ticks,
            rebalance_cooldown,
            max_rebalance_divergence_bps,
        }
        .publish(&env);
    }

    /// Cheap simulation/read used by keepers to avoid submitting no-op
    /// rebalance transactions every polling cycle.
    pub fn needs_rebalance(env: Env) -> bool {
        let Some((spot_tick, _, lower, upper)) = Self::desired_range(&env) else {
            return false;
        };
        Self::rebalance_due(&env, spot_tick, lower, upper)
    }

    /// Permissionlessly recenters the vault's one active position.
    ///
    /// The old position is withdrawn in full. A single guarded swap converts
    /// only the excess side into the approximately equal-value pair required
    /// by a centered range, avoiding a wasteful sell-and-buy round trip. The
    /// call reverts atomically unless a new position is minted and at least
    /// 95% of the pre-deposit pair value is put back to work.
    pub fn rebalance(env: Env, caller: Address) -> bool {
        caller.require_auth();
        bump_critical_ttl(&env);
        Self::require_not_paused(&env);
        let Some((spot_tick, _, new_lower, new_upper)) = Self::desired_range(&env) else {
            return false;
        };
        if !Self::rebalance_due(&env, spot_tick, new_lower, new_upper) {
            return false;
        }
        let policy = range_params(&env);
        let root = Self::nav_root(&env);
        if root == 0 {
            panic!("nav unavailable for rebalance");
        }

        let pool = Self::pool(&env);
        match try_call::<bool, _>(&env, &pool, "get_is_killed_deposit", ()) {
            Ok(false) => {}
            Ok(true) => return false,
            Err(ref e) => {
                emit_call_failure(&env, &pool, e, true);
                return false;
            }
        }

        let (old_lower, old_upper) = Self::ticks(&env);
        let Some(liquidity_burned) = Self::withdraw_all_for_rebalance(&env) else {
            return false;
        };

        let mut cfg = config(&env);
        cfg.tick_lower = new_lower;
        cfg.tick_upper = new_upper;
        set_config(&env, &cfg);

        let pair_value =
            Self::balance_pair_for_centered_range(&env, root, policy.max_rebalance_divergence_bps);
        let amount_u = Self::balance_of_token(&env, &Self::underlying(&env));
        let amount_o = Self::balance_of_token(&env, &Self::other_token(&env));
        let deposit = Self::deposit_balances(&env, amount_u, amount_o, amount_u, amount_o);
        let liquidity_minted = deposit.minted;
        if liquidity_burned > 0 && liquidity_minted == 0 {
            panic!("rebalance deposit failed");
        }
        let idle_u = deposit.idle_underlying;
        let idle_o = deposit.idle_other;
        let r_scaled = root.checked_mul(root).expect("nav root overflow");
        let idle_value = idle_u.saturating_add(mul_div(&env, idle_o, r_scaled, NAV_RATIO_SCALE));
        if pair_value > 0
            && mul_div(&env, idle_value, BPS_DENOM as u128, pair_value) > MAX_REBALANCE_IDLE_BPS
        {
            panic!("rebalance left excess idle value");
        }

        let now = env.ledger().timestamp();
        set_range_state(
            &env,
            &RangeState {
                last_rebalance_at: now,
            },
        );
        PositionRebalanced {
            caller,
            old_tick_lower: old_lower,
            old_tick_upper: old_upper,
            new_tick_lower: new_lower,
            new_tick_upper: new_upper,
            liquidity_burned,
            liquidity_minted,
        }
        .publish(&env);
        true
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
        if snapshot.raw_liquidity > 0 {
            let (expected_lower, expected_upper) = Self::ticks(&env);
            if snapshot.ranges.len() != 1 {
                return Self::position_liquidity(&env);
            }
            let active = snapshot.ranges.get(0).unwrap();
            if active.tick_lower != expected_lower || active.tick_upper != expected_upper {
                return Self::position_liquidity(&env);
            }
        }
        Self::set_position_liquidity(&env, snapshot.raw_liquidity);
        let _ = Self::refresh_position_amounts_inner(&env);
        snapshot.raw_liquidity
    }

    /// Deploys any idle underlying. Permissionless — it only ever moves the
    /// vault's own cash into the position it is configured for.
    pub fn deploy(env: Env) -> u128 {
        bump_critical_ttl(&env);
        Self::require_not_paused(&env);
        Self::deploy_idle(&env).map_or(0, |result| result.minted)
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

    pub fn get_position_amounts(env: Env) -> Option<PositionAmounts> {
        position_amounts(&env)
    }

    pub fn get_range_policy(env: Env) -> RangeParams {
        range_params(&env)
    }

    pub fn get_last_rebalance_at(env: Env) -> u64 {
        range_state(&env).last_rebalance_at
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

    /// Timestamp of the last successful oracle read. `0` means the cache has
    /// been invalidated and the next valuation must re-read the feed.
    pub fn get_last_nav_root_at(env: Env) -> u64 {
        state(&env).last_nav_root_at
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

    pub fn get_reward_min_rate(env: Env, reward_token: Address) -> u128 {
        env.storage()
            .persistent()
            .get(&DataKey::RewardMinRate(reward_token))
            .unwrap_or(0)
    }

    pub fn get_receipt_vault(env: Env) -> Option<Address> {
        bound_receipt_vault(&env)
    }

    pub fn get_primary_reward_token(env: Env) -> Option<Address> {
        primary_reward_token(&env)
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
