use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, token, Address, BytesN, Env, IntoVal, String, Symbol, Val, Vec,
};

use crate::constants::*;
use crate::events::*;
use crate::math::*;
use crate::oracle::{Asset as OracleAsset, PriceData};
use crate::pool::{ConcentratedPoolClient, Slot0};
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
pub struct ClShareVault;

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
pub(crate) fn auth_entry(
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
impl ClShareVault {
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
        crate::accounting::balance(env, token)
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
        crate::stage_storage::set_composition_dirty(env,false);
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
        assert!(!crate::stage_storage::composition_dirty(env),"unfinalised composition is not a NAV quote");
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

    fn collect_fees(env: &Env) {
        if Self::position_liquidity(env) == 0 {
            return;
        }
        let pool = Self::pool(env);
        let me = env.current_contract_address();
        let _: Vec<u128> = try_call(env, &pool, "claim_all_position_fees", (me,))
            .unwrap_or_else(|_| panic!("fee checkpoint failed"));
        assert!(
            Self::refresh_position_amounts_inner(env),
            "principal snapshot unavailable"
        );
    }

    /// Extract exactly the exiting share fraction. Aquarius pays all outstanding
    /// fees on a partial withdrawal, so checkpoint them BEFORE measuring idle.
    /// No oracle or reward call is needed here; emergency exits return both legs.
    fn extract_fraction(env: &Env, shares: u128) -> (u128, u128, u128) {
        let supply = Self::total_shares(env);
        assert!(shares > 0 && shares <= supply, "invalid share fraction");
        let liquidity = Self::position_liquidity(env);
        if liquidity > 0 {
            let _: Vec<u128> = try_call(
                env,
                &Self::pool(env),
                "claim_all_position_fees",
                (env.current_contract_address(),),
            )
            .unwrap_or_else(|_| panic!("fee checkpoint unavailable"));
        }
        let t0 = Self::token(env, 0);
        let t1 = Self::token(env, 1);
        let idle0 = Self::balance_of_token(env, &t0);
        let idle1 = Self::balance_of_token(env, &t1);
        let burn = mul_div(env, liquidity, shares, supply);
        if burn > 0 {
            let (lo, hi) = Self::ticks(env);
            let _: Vec<u128> = try_call(
                env,
                &Self::pool(env),
                "withdraw_position",
                (
                    env.current_contract_address(),
                    lo,
                    hi,
                    burn,
                    soroban_sdk::vec![env, 0u128, 0u128],
                ),
            )
            .unwrap_or_else(|_| panic!("principal withdrawal failed"));
        }
        let principal0 = Self::balance_of_token(env, &t0).checked_sub(idle0).unwrap();
        let principal1 = Self::balance_of_token(env, &t1).checked_sub(idle1).unwrap();
        if let Some(mut snapshot) = position_amounts(env) {
            snapshot.liquidity = liquidity - burn;
            snapshot.amount0 = mul_div(env, snapshot.amount0, liquidity - burn, liquidity.max(1));
            snapshot.amount1 = mul_div(env, snapshot.amount1, liquidity - burn, liquidity.max(1));
            set_position_amounts(env, &snapshot);
        } else {
            Self::set_position_liquidity(env, liquidity - burn);
        }
        (
            principal0
                .checked_add(mul_div(env, idle0, shares, supply))
                .unwrap(),
            principal1
                .checked_add(mul_div(env, idle1, shares, supply))
                .unwrap(),
            burn,
        )
    }

    fn total_shares(env: &Env) -> u128 {
        state(env).total_shares
    }

    fn shares_held(env: &Env, owner: &Address) -> u128 {
        shares_of(env, owner)
    }

    fn mint_shares(env: &Env, to: &Address, amount: u128) {
        set_shares(env, to, shares_of(env, to).checked_add(amount).unwrap());
        crate::custody::received(env, to, amount);
        let mut st = state(env);
        st.total_shares = st.total_shares.checked_add(amount).unwrap();
        set_state(env, &st);
    }

    fn burn_shares(env: &Env, from: &Address, amount: u128) {
        assert!(crate::stage_storage::available(env,from)>=amount,"shares locked by command");
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
        // Only the admin can change the policy. Apply a different aligned width
        // without waiting for the OLD range's edge; rebalance_due still enforces
        // the cooldown and rebalance retains every price/accounting guard.
        if i64::from(current_upper) - i64::from(current_lower)
            != i64::from(desired_upper) - i64::from(desired_lower)
        {
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

    /// Converts only the excess side of an unwind into the exact composition
    /// quoted for the new range. A spacing-aligned range is generally not
    /// centered on the live tick, so a 50/50 value split can leave more than
    /// the allowed idle amount even when the range itself is correct.
    fn balance_pair_for_centered_range(
        env: &Env,
        root: u128,
        spot_tick: i32,
        max_divergence_bps: u32,
    ) -> u128 {
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
        let u_idx = Self::underlying_index(env);
        let o_idx = 1 - u_idx;

        // Probe the exact active-range ratio with an equal-value pair. The
        // quote is read-only and happens in the same transaction as the swap
        // and deposit, so no intervening ledger can change the target. Bound
        // the untrusted result by what was offered before using its ratio.
        let probe_u = total_value / 2;
        let probe_o = mul_div_ceil(env, total_value - probe_u, NAV_RATIO_SCALE, r_scaled);
        if probe_u == 0 || probe_o == 0 {
            panic!("rebalance pair too small");
        }
        let mut desired: Vec<u128> = Vec::new(env);
        if u_idx == 0 {
            desired.push_back(probe_u);
            desired.push_back(probe_o);
        } else {
            desired.push_back(probe_o);
            desired.push_back(probe_u);
        }
        let pool = Self::pool(env);
        let (tick_lower, tick_upper) = Self::ticks(env);
        let (quoted, quoted_liquidity): (Vec<u128>, u128) = match try_call(
            env,
            &pool,
            "estimate_deposit_position",
            (tick_lower, tick_upper, desired.clone()),
        ) {
            Ok(v) => v,
            Err(ref e) => {
                emit_call_failure(env, &pool, e, true);
                panic!("rebalance range quote unavailable");
            }
        };
        if quoted.len() != 2 || quoted_liquidity == 0 {
            panic!("rebalance range quote invalid");
        }
        let quoted0 = quoted.get(0).unwrap_or(0);
        let quoted1 = quoted.get(1).unwrap_or(0);
        if quoted0 == 0
            || quoted1 == 0
            || quoted0 > desired.get(0).unwrap_or(0)
            || quoted1 > desired.get(1).unwrap_or(0)
        {
            panic!("rebalance range quote invalid");
        }
        let (quoted_u, quoted_o) = if u_idx == 0 {
            (quoted0, quoted1)
        } else {
            (quoted1, quoted0)
        };
        let quoted_o_value = mul_div(env, quoted_o, r_scaled, NAV_RATIO_SCALE);
        if quoted_o_value == 0 {
            panic!("rebalance range quote invalid");
        }
        let quoted_value = quoted_u.saturating_add(quoted_o_value);
        let quoted_u_share_bps = quoted_u
            .checked_mul(BPS_DENOM as u128)
            .expect("rebalance range quote overflow")
            / quoted_value;

        // A centered constant-liquidity range has value on each side in
        // proportion to its tick distance from spot (up to small exponential
        // curvature). Bind the external quote to that independently derived
        // geometry before it can determine swap size. The swap itself still
        // has the tighter two-sided oracle price guard below.
        let tick_width = (tick_upper - tick_lower) as u128;
        let token0_tick_span = (tick_upper - spot_tick) as u128;
        let expected_token0_share_bps = token0_tick_span * BPS_DENOM as u128 / tick_width;
        let expected_u_share_bps = if u_idx == 0 {
            expected_token0_share_bps
        } else {
            (BPS_DENOM as u128).saturating_sub(expected_token0_share_bps)
        };
        if quoted_u_share_bps.abs_diff(expected_u_share_bps)
            > MAX_REBALANCE_QUOTE_SHARE_DEVIATION_BPS
        {
            panic!("rebalance range quote disagrees with tick geometry");
        }
        let target_u_value = mul_div(env, total_value, quoted_u, quoted_value);
        let target_o_value = total_value.saturating_sub(target_u_value);

        let mut price_checked = false;
        if amount_u.saturating_sub(target_u_value) >= MIN_DEPLOY_AMOUNT {
            let excess = amount_u - target_u_value;
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
        } else if other_value.saturating_sub(target_o_value) >= MIN_DEPLOY_AMOUNT {
            let excess_value = other_value - target_o_value;
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
        String::from_str(&env, "Peridot Peridot CL Shares")
    }

    pub fn symbol(env: Env) -> String {
        String::from_str(&env, "pCL")
    }

    /// Underlying redeemable for `vault_shares`, as a single-element vector.
    ///
    /// `receipt-vault` reads index 0 as *the* underlying amount and sizes its
    /// `min_amounts_out` vector by this vector's length, so the single-asset
    /// shape here is load-bearing.
    pub fn get_asset_amounts_per_shares(env: Env, vault_shares: i128) -> Vec<i128> {
        if env.storage().instance().has(&StageVaultKey::StgV1Receipt) {return Self::stage_share_value(&env,vault_shares);}
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
        pilot_require_legacy_financial(&env);

        bump_critical_ttl(&env);
        Self::require_not_paused(&env);

        // Fresh share vault accepts direct holders and the configured facade.
        from.require_auth();
        crate::accounting::begin(&env);
        crate::rewards::normal(&env, &from);
        Self::collect_fees(&env);

        let amount = to_u128(amounts_desired.get(0).unwrap_or(0));
        assert!(amount >= MIN_DEPLOY_AMOUNT, "deposit below dust minimum");
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
        assert!(Self::nav_root(&env) > 0, "NAV unavailable");
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

        let shares = if supply == 0 {
            assert_eq!(nav_before, 0, "unowned NAV");
            value_added
        } else {
            assert!(nav_before > 0, "insolvent share supply");
            mul_div(&env, value_added, supply, nav_before)
        };
        if shares == 0 {
            panic!("deposit too small to mint shares");
        }
        Self::mint_shares(&env, &from, shares);
        crate::rewards::reset_debt(&env, &from);
        crate::accounting::end(&env);

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
    /// The holder authorizes the exact withdrawal, including its output floor.
    pub fn withdraw(
        env: Env,
        withdraw_shares: i128,
        min_amounts_out: Vec<i128>,
        from: Address,
    ) -> Vec<i128> {
        pilot_require_legacy_financial(&env);

        bump_critical_ttl(&env);
        from.require_auth();
        crate::accounting::begin(&env);
        crate::rewards::normal(&env, &from);

        crate::custody::require_direct_holder(&env, &from);
        let shares = to_u128(withdraw_shares);
        let mut out: Vec<i128> = Vec::new(&env);
        assert!(shares > 0, "zero burn");
        let supply = Self::total_shares(&env);
        if supply == 0 {
            panic!("no shares outstanding");
        }

        let min_out = to_u128(min_amounts_out.get(0).unwrap_or(0));
        let exit_root = Self::exit_nav_root(&env, min_out);

        assert!(shares <= shares_of(&env, &from), "insufficient shares");
        let underlying = Self::underlying(&env);
        let liquidity_before = Self::position_liquidity(&env);
        let (amount0, amount1, _) = Self::extract_fraction(&env, shares);
        let (principal, paired) = if Self::underlying_index(&env) == 0 {
            (amount0, amount1)
        } else {
            (amount1, amount0)
        };
        let output = if paired == 0 {
            0
        } else {
            let before = Self::balance_of_token(&env, &Self::other_token(&env));
            let output = Self::swap_exact_in(
                &env,
                Self::other_index(&env),
                Self::underlying_index(&env),
                paired,
                Some(SwapGuard {
                    max_divergence_bps: params(&env).max_pool_divergence_bps,
                    two_sided: false,
                    exit_root: Some(exit_root),
                }),
                Some(before),
            );
            assert!(
                output > 0,
                "paired conversion unavailable; shares preserved"
            );
            output
        };
        let payout = principal.checked_add(output).unwrap();
        assert!(payout >= min_out, "withdraw below minimum");
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
            to: from.clone(),
            shares_burned: shares_to_burn,
            liquidity_burned: liquidity_before.saturating_sub(Self::position_liquidity(&env)),
            underlying_out: payout,
        }
        .publish(&env);

        crate::rewards::reset_debt(&env, &from);
        crate::accounting::end(&env);
        out.push_back(to_i128(payout));
        out
    }

    // ─────────────────────────────────────────────────────────────────────
    // Single-asset convenience entrypoints
    // ─────────────────────────────────────────────────────────────────────

    /// Single-sided deposit wrapper; the vault handles the paired asset split.
    pub fn deposit_underlying(env: Env, from: Address, amount: i128, min_shares: i128) -> i128 {
        pilot_require_legacy_financial(&env);

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
        pilot_require_legacy_financial(&env);

        let mut mins: Vec<i128> = Vec::new(&env);
        mins.push_back(min_underlying_out);
        let out = Self::withdraw(env, shares, mins, from);
        out.get(0).unwrap_or(0)
    }

    /// Validation-only, pinned bytecode view support. Default is unresolved.
    pub fn enable_verified_reward_view(env: Env) { crate::reward_history::enable(&env); }
    pub fn reward_history_count(env: Env, holder: Address) -> u32 { crate::reward_history::count(&env,&holder) }
    pub fn reward_weight(env: Env, holder: Address, checkpoint: u32) -> Option<crate::reward_history::Weight> { crate::reward_history::weight(&env,&holder,checkpoint) }
    pub fn reward_interval(env: Env, id: u64) -> Option<crate::reward_history::Interval> { crate::reward_history::interval(&env,id) }
    pub fn reward_segment(env: Env, id: u64) -> Option<crate::reward_history::Segment> { crate::reward_history::segment(&env,id) }
    pub fn reward_history_totals(env: Env) -> crate::reward_history::HistoryTotals { crate::reward_history::totals(&env) }
    pub fn prepare_reward_segment(env: Env, id: u64, limit: u32) -> bool { crate::reward_history::prepare(&env,id,limit) }
    pub fn settle_reward_segment(env: Env, holder: Address, id: u64, checkpoint: u32) -> u128 { crate::reward_history::settle(&env,&holder,id,checkpoint) }
    pub fn propose_reward_segment(env: Env, id: u64, amount: u128, evidence: BytesN<32>) { crate::reward_history::propose(&env,id,amount,evidence); }
    pub fn attribute_reward_segment(env: Env, id: u64, amount: u128, evidence: BytesN<32>) { crate::reward_history::attribute(&env,id,amount,evidence); }
    pub fn checkpoint_rewards(env: Env) -> bool {
        crate::rewards::checkpoint(&env)
    }
    pub fn reward_account(env: Env, holder: Address) -> crate::rewards::Holder {
        crate::rewards::settle(&env, &holder);
        crate::rewards::holder(&env, &holder)
    }
    pub fn reward_totals(env: Env) -> crate::rewards::Totals {
        crate::rewards::totals(&env)
    }
    pub fn reward_boundary(env: Env, id: u64) -> Option<crate::rewards::Boundary> {
        crate::rewards::boundary(&env, id)
    }
    pub fn claim_rewards(env: Env, holder: Address) -> u128 {
        crate::rewards::claim(&env, &holder)
    }
    pub fn propose_reward_attribution(env: Env, id: u64, amount: u128, evidence: BytesN<32>) {
        crate::rewards::propose_attribute(&env, id, amount, evidence)
    }
    pub fn attribute_rewards(env: Env, id: u64, amount: u128, evidence: BytesN<32>) {
        crate::rewards::attribute(&env, id, amount, evidence)
    }
    /// Direct holder's unencumbered shares only. No swap or NAV dependency.
    pub fn emergency_redeem(
        env: Env,
        holder: Address,
        shares: u128,
        accept_reward_waiver: bool,
    ) -> Vec<u128> {
        assert!(!pilot_bound(&env),"receipt custody: shares do not move; use the staged routes");

        holder.require_auth();
        crate::custody::require_direct_holder(&env, &holder);
        crate::accounting::begin(&env);
        crate::rewards::recovery(&env, &holder, false, accept_reward_waiver);
        assert!(
            shares > 0 && shares <= shares_of(&env, &holder),
            "invalid shares"
        );
        let (amount0, amount1, _) = Self::extract_fraction(&env, shares);
        let mut out = Vec::new(&env);
        for (token_address, amount) in [
            (Self::token(&env, 0), amount0),
            (Self::token(&env, 1), amount1),
        ] {
            if amount > 0 {
                token::Client::new(&env, &token_address).transfer(
                    &env.current_contract_address(),
                    &holder,
                    &to_i128(amount),
                );
            }
            out.push_back(amount);
        }
        Self::burn_shares(&env, &holder, shares);
        crate::rewards::reset_debt(&env, &holder);
        crate::accounting::end(&env);
        out
    }
    /// Conservative oracle-NAV quote; deposit pause is directional.
    pub fn quote(env: Env, deposit: bool, amount: u128) -> u128 {
        let p = params(&env);
        if (deposit && p.paused)
            || env
                .ledger()
                .timestamp()
                .saturating_sub(state(&env).last_nav_root_at)
                > p.nav_root_max_age
            || Self::nav_root(&env) == 0
        {
            return 0;
        }
        let pool = ConcentratedPoolClient::new(&env, &Self::pool(&env));
        if pool.try_get_is_killed_swap().ok().and_then(Result::ok) != Some(false) {
            return 0;
        }
        if deposit && pool.try_get_is_killed_deposit().ok().and_then(Result::ok) != Some(false) {
            return 0;
        }
        let nav = Self::total_underlying_full(&env);
        let supply = state(&env).total_shares;
        let gross = if supply == 0 {
            amount
        } else if nav == 0 {
            return 0;
        } else if deposit {
            mul_div(&env, amount, supply, nav)
        } else {
            mul_div(&env, amount, nav, supply)
        };
        Self::receipt_quote_value(&env, gross)
    }

    /// Transferable shares; registered custody moves preserve beneficial rewards.
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        assert!(!pilot_bound(&env),"receipt custody: shares do not move; use the staged routes");

        from.require_auth();
        Self::transfer_shares(&env, &from, &to, amount);
    }

    /// SEP-41 spender authorization replaces the owner signature only; reward
    /// checkpointing and all share/custody invariants use the same move routine.
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        assert!(!pilot_bound(&env),"receipt custody: shares do not move; use the staged routes");

        spender.require_auth();
        crate::allowances::spend(&env, &from, &spender, amount);
        Self::transfer_shares(&env, &from, &to, amount);
    }

    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        crate::allowances::allowance(&env, &from, &spender)
    }

    pub fn approve(env: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        assert!(!pilot_bound(&env),"receipt custody: shares do not move; use the staged routes");

        from.require_auth();
        crate::allowances::approve(&env, &from, &spender, amount, expiration_ledger);
        ShareApproval { from, spender, amount, expiration_ledger }.publish(&env);
    }

    fn transfer_shares(env: &Env, from: &Address, to: &Address, amount: i128) {
        assert!(amount >= 0, "negative transfer");
        if amount == 0 {
            ShareTransfer { from: from.clone(), to: to.clone(), amount }.publish(env);
            return;
        }
        let amount = to_u128(amount);
        assert!(crate::stage_storage::available(env,from)>=amount,"shares locked by command");
        assert!(shares_of(env, from) >= amount, "insufficient shares");
        // Self-transfers neither consume custody allocations nor change ownership.
        if from == to {
            crate::rewards::normal(env, from);
            ShareTransfer { from: from.clone(), to: to.clone(), amount: to_i128(amount) }.publish(env);
            return;
        }
        let owner = match crate::custody::outgoing(env, from, to, amount) {
            Some(owner) => owner,
            None => {
                crate::rewards::normal(env, from);
                crate::rewards::settle(env, to);
                from.clone()
            }
        };
        set_shares(env, from, shares_of(env, from) - amount);
        set_shares(env, to, shares_of(env, to).checked_add(amount).unwrap());
        crate::custody::received(env, to, amount);
        crate::rewards::reset_debt(env, &owner);
        crate::rewards::reset_debt(env, to);
        ShareTransfer { from: from.clone(), to: to.clone(), amount: to_i128(amount) }.publish(env);
    }

    pub fn register_custodian(env: Env, custodian: Address) {
        assert!(!pilot_public_network(&env),"minimal pilot has no custody registry");

        pilot_require_unbound_configuration(&env);

        crate::custody::register(&env, &custodian);
    }
    pub fn is_custodian(env: Env, custodian: Address) -> bool {
        crate::custody::registered(&env, &custodian)
    }
    pub fn custody_allocation(env: Env, custodian: Address, owner: Address) -> u128 {
        crate::custody::allocation(&env, &custodian, &owner)
    }
    pub fn custody_assign(env: Env, custodian: Address, owner: Address, amount: u128) {
        assert!(!pilot_bound(&env),"receipt custody: shares do not move; use the staged routes");

        crate::custody::assign(&env, &custodian, &owner, amount, false);
    }
    pub fn custody_recover_assign(env: Env, custodian: Address, owner: Address, amount: u128) {
        assert!(!pilot_bound(&env),"receipt custody: shares do not move; use the staged routes");

        crate::custody::assign(&env, &custodian, &owner, amount, true);
    }
    pub fn custody_reassign(
        env: Env,
        custodian: Address,
        from_owner: Address,
        to_owner: Address,
        amount: u128,
    ) {
        assert!(!pilot_bound(&env),"receipt custody: shares do not move; use the staged routes");

        crate::custody::reassign(&env, &custodian, &from_owner, &to_owner, amount, false);
    }
    pub fn custody_recover_reassign(
        env: Env,
        custodian: Address,
        from_owner: Address,
        to_owner: Address,
        amount: u128,
    ) {
        assert!(!pilot_bound(&env),"receipt custody: shares do not move; use the staged routes");

        crate::custody::reassign(&env, &custodian, &from_owner, &to_owner, amount, true);
    }
    /// Registered contracts must enforce their position/receipt recovery lifecycle.
    pub fn custody_prepare_transfer(
        env: Env,
        custodian: Address,
        owner: Address,
        recipient: Address,
        amount: u128,
        recovery: bool,
    ) {
        assert!(!pilot_bound(&env),"receipt custody: shares do not move; use the staged routes");

        crate::custody::prepare(&env, &custodian, &owner, &recipient, amount, recovery);
    }

    // ─────────────────────────────────────────────────────────────────────
    // Harvest
    // ─────────────────────────────────────────────────────────────────────

    /// Sells `amount` of `reward_token` for underlying through its configured
    /// route pool. Returns 0 (and emits) rather than reverting, so one
    /// unsellable reward cannot block the rest of the harvest.
    pub(crate) fn swap_reward(env: &Env, reward_token: &Address, amount: u128) -> u128 {
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
        // Legacy harvest/sweep callers pass the complete reward-token balance,
        // so replaying the transfer fails inside SEP-41. The development-only
        // exact-input bridge instead verifies actual input consumption itself;
        // its partial-balance calls cannot rely on that legacy assumption.
        Self::balance_of_token(env, &underlying).saturating_sub(underlying_before)
    }

    /// Permissionless: claim the configured primary reward, third-party gauge
    /// incentives and accrued swap fees, convert everything to underlying,
    /// and redeploy.
    ///
    /// Rate-limited because each call moves the share price; without a cooldown
    /// it could be used to grind rounding in the depositor's favour.
    pub fn harvest(env: Env, caller: Address) -> i128 {
        let _ = (env, caller);
        panic!("pooled reward conversion disabled in CL shares");
    }

    /// Sells any configured reward held by the vault. Kept separate from
    /// `harvest` so a route can be exercised even when claiming is paused.
    pub fn sweep_reward(env: Env, caller: Address, reward_token: Address) -> i128 {
        let _ = (env, caller);
        panic!("pooled reward conversion disabled in CL shares");
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
        pilot_require_pinned_admin(&env,&admin);
        if pilot_public_network(&env) {
            assert!(cfg!(feature="cl-pyusd-vault-b"),"pilot vault Mainnet release feature missing");
            assert_eq!(pool,Address::from_str(&env,"CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX"),"pilot pool");
            assert_eq!(oracle,Address::from_str(&env,"CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M"),"pilot primary oracle");
            assert_eq!(underlying_index,0,"pilot PYUSD NAV index");
        }

        crate::require_pilot_release_network(&env);
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
        // ISOLATED MEASUREMENT FIXTURE ONLY: accepted economics band.
        assert_eq!(tick_spacing,20);
        assert!(slot.tick>=-20 && slot.tick<20);
        let (tick_lower,tick_upper)=(-20,20);

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
        assert!(!pilot_public_network(&env),"pilot Ledger authority and code are fixed");

        pilot_require_unbound_configuration(&env);

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
        assert!(!pilot_public_network(&env),"pilot Ledger authority and code are fixed");

        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
        // v2 upgrade gate: the reversible admin pause stays available while bound.
        // It blocks new deposits/investment and swaps only; in-kind close, emergency
        // exit and seizure sync do not read it. The guardian pause stays one-way.
        pilot_require_pinned_admin(&env,&admin_addr);

        Self::require_admin(&env, &admin_addr);
        let mut p = params(&env);
        p.paused = paused;
        set_params(&env, &p);
        PausedSet { paused }.publish(&env);
    }

    pub fn set_oracle(env: Env, admin_addr: Address, oracle: Address) {
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

        assert!(
            state(&env).total_shares == 0 && primary_reward_token(&env).is_none(),
            "reward token immutable once set"
        );
        if let Some(ref token) = reward_token {
            assert!(
                *token != Self::underlying(&env) && *token != Self::other_token(&env),
                "reward must be separate from principal"
            );
        }
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
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
        pilot_require_unbound_configuration(&env);

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
    /// only the excess side into the exact token ratio quoted for the new
    /// spacing-aligned range, avoiding a wasteful sell-and-buy round trip. The
    /// call reverts atomically unless a new position is minted and at least
    /// 95% of the pre-deposit pair value is put back to work.
    pub fn rebalance(env: Env, caller: Address) -> bool {
        pilot_require_legacy_financial(&env);

        let _ = env;
        panic!("maintenance not yet validated for transferable shares");
    }

    /// Reconciles locally tracked liquidity against the pool's own view.
    ///
    /// Permissionless: it can only ever replace the local number with the
    /// pool's authoritative one, so there is nothing to gain from calling it.
    pub fn sync_liquidity(env: Env) -> u128 {
        let _ = env;
        panic!("maintenance not yet validated for transferable shares");
    }

    /// Deploys any idle underlying. Permissionless — it only ever moves the
    /// vault's own cash into the position it is configured for.
    pub fn deploy(env: Env) -> u128 {
        pilot_require_legacy_financial(&env);

        let _ = env;
        panic!("maintenance not yet validated for transferable shares");
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
        if crate::stage_storage::composition_dirty(&env) {return None;}
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

    // v2: upgradeable while bound, on Mainnet only by the pinned Ledger admin (set_admin
    // stays frozen there). 24 h timelock; cancel; execution requires the admin pause.
    pub fn propose_upgrade_wasm(env: Env, admin_addr: Address, new_wasm_hash: BytesN<32>) {
        pilot_require_pinned_admin(&env,&admin_addr);
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
        env.events().publish((Symbol::new(&env,"upgrade_proposed"),),(new_wasm_hash,eta));
    }

    pub fn cancel_upgrade(env: Env, admin_addr: Address) {
        pilot_require_pinned_admin(&env,&admin_addr);
        Self::require_admin(&env, &admin_addr);
        let h: BytesN<32> = env.storage().persistent().get(&DataKey::PendingUpgradeHash).expect("pending upgrade not set");
        env.storage().persistent().remove(&DataKey::PendingUpgradeHash);
        env.storage().persistent().remove(&DataKey::PendingUpgradeEta);
        env.events().publish((Symbol::new(&env,"upgrade_cancelled"),),h);
    }

    pub fn get_pending_upgrade(env: Env) -> Option<(BytesN<32>, u64)> {
        let h: Option<BytesN<32>> = env.storage().persistent().get(&DataKey::PendingUpgradeHash);
        h.map(|h| (h, env.storage().persistent().get(&DataKey::PendingUpgradeEta).unwrap()))
    }

    pub fn upgrade_wasm(env: Env, admin_addr: Address, new_wasm_hash: BytesN<32>) {
        pilot_require_pinned_admin(&env,&admin_addr);
        Self::require_admin(&env, &admin_addr);
        assert!(params(&env).paused, "pause vault before upgrade");
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
        env.events().publish((Symbol::new(&env,"upgraded"),),new_wasm_hash.clone());
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

// ISOLATED 1x prototype. No production ABI adoption.
#[soroban_sdk::contracttype]
#[derive(Clone, Debug)]
pub struct StageJob {
    pub owner: Address,
    pub kind: u32, // 0 open; 1 close
    pub phase: u32, // open 0 prepared,1 funded,2 converted,3 minted,4 terminal; close 10 locked,11 mixture,12 terminal
    pub deadline: u64,
    pub a: u128,
    pub b: u128,
    pub reserve: u128,
    pub swap_in: u128,
    pub min_swap: u128,
    pub min_shares: u128,
    pub shares: u128,
}

// Appended to isolated cl-share-vault/contract.rs only.
#[soroban_sdk::contracttype]
#[derive(Clone)]
enum StageVaultKey { StgV1Receipt, StgV1Allocation(u64), StgV1Guardian, StgV1DepositPause, StgV1InvestGuard, StgV1InvestKeeper, StgV1Journal(u64), StgV1Exit(u64) }
#[soroban_sdk::contracttype]
#[derive(Clone,Debug)]
pub struct StageAllocation {
    pub supply:u128,pub shares:u128,pub liquidity_burn:u128,
    pub fee0:u128,pub fee1:u128,pub principal0:u128,pub principal1:u128,
    pub allocated0:u128,pub allocated1:u128,pub retained0:u128,pub retained1:u128,
}
#[soroban_sdk::contracttype]
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct StageInvestmentGuard {
    pub observed_ledger:u32,
    pub healthy_count:u32,
    pub inside:bool,
    pub observed_tick:i32,
}
#[soroban_sdk::contracttype]
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct StageInvestmentJournal {
    pub owner:Address,pub shares:u128,pub liquidity_before:u128,pub liquidity_added:u128,
    pub liquidity_after:u128,pub spent0:u128,pub spent1:u128,pub original_reserve:u128,
    pub cost:u128,pub refund:u128,pub execution_ledger:u32,pub settled:bool,
}
#[soroban_sdk::contracttype]
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct StageEmergencyExit {
    pub owner:Address,pub shares:u128,pub out0:u128,pub out1:u128,
    pub waived:bool,pub proven:bool,pub reading:Option<u128>,pub extract_ledger:u32,pub paid:bool,
}
#[soroban_sdk::contracttype]
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct StageOpenQuote {
    pub valid:bool,pub admissible:bool,pub deposits_paused:bool,pub composition_dirty:bool,pub oracle_fresh:bool,pub in_band:bool,
    pub guard_ready:bool,pub ledger:u32,pub timestamp:u64,pub tick:i32,
    pub swap_in:u128,pub swap_out:u128,pub min_swap:u128,pub shares:u128,pub min_shares:u128,
    pub invest0:u128,pub invest1:u128,pub min_reserve:u128,pub reserve:u128,
    pub vault_slippage_bps:u32,pub vault_divergence_bps:u32,
}
#[soroban_sdk::contracttype]
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct StageCloseQuote {
    pub valid:bool,pub ledger:u32,pub timestamp:u64,pub tick:i32,
    pub shares:u128,pub out0:u128,pub out1:u128,pub swap_out:u128,pub min_swap:u128,pub total:u128,
    pub vault_slippage_bps:u32,pub vault_divergence_bps:u32,
}
fn stage_raw(e:&Env,t:&Address)->u128 {to_u128(token::Client::new(e,t).balance(&e.current_contract_address()))}
fn stage_job(e:&Env,r:&Address,_id:u64,phase:u32,j:StageJob)->StageJob {
    assert_eq!(*r,e.storage().instance().get::<_,Address>(&StageVaultKey::StgV1Receipt).expect("stage unbound"));r.require_auth();
    assert_eq!(j.phase,phase);j
}
fn stage_pay(e:&Env,t:&Address,to:&Address,n:u128) {if n>0 {token::Client::new(e,t).transfer(&e.current_contract_address(),to,&to_i128(n));}}
#[contractimpl]
impl ClShareVault {
    pub fn stage_bind(e:Env,admin_addr:Address,receipt:Address) {
        pilot_require_pinned_admin(&e,&admin_addr);
        if pilot_public_network(&e) {
            let c=config(&e);
            assert_eq!(c.pool,Address::from_str(&e,"CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX"));
            assert_eq!(c.oracle,Address::from_str(&e,"CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M"));
            assert_eq!(c.token0,Address::from_str(&e,"CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2"));
            assert_eq!(c.token1,Address::from_str(&e,"CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"));
            assert_eq!((c.dec0,c.dec1,c.underlying_index,c.tick_lower,c.tick_upper),(7,7,0,-20,20));
            let peer_admin:Address=e.invoke_contract(&receipt,&Symbol::new(&e,"get_admin"),().into_val(&e));
            assert_eq!(peer_admin,admin_addr,"pilot receipt admin");
        }

        Self::require_admin(&e,&admin_addr);crate::require_pilot_release_network(&e);
        assert!(state(&e).total_shares==0 && !e.storage().instance().has(&StageVaultKey::StgV1Receipt));
        assert_eq!(Self::ticks(&e),(-20,20),"pilot fixed parity band");
        assert_eq!(config(&e).underlying_index,0);assert_eq!(config(&e).dec0,config(&e).dec1);
        // Topology B: the bound receipt is the vault's only custodian. Every share is
        // minted into its custody for a recorded beneficiary and can leave only by burn.
        crate::custody::stage_register(&e,&receipt);
        e.storage().instance().set(&StageVaultKey::StgV1Receipt,&receipt);
    }
    pub fn stage_receipt(e:Env)->Address {e.storage().instance().get(&StageVaultKey::StgV1Receipt).expect("stage unbound")}
    pub fn stage_funding_asset(e:Env)->Address {let _=Self::stage_receipt(e.clone());Self::token(&e,0)}
    pub fn stage_beneficial(e:Env,owner:Address)->u128 {crate::custody::beneficial(&e,&owner)}
    pub fn stage_locked(e:Env,owner:Address)->u128 {crate::stage_storage::locked(&e,&owner)}
    pub fn stage_available(e:Env,owner:Address)->u128 {crate::stage_storage::available(&e,&owner)}
    pub fn stage_reserved(e:Env)->Vec<u128> {soroban_sdk::vec![&e,crate::stage_storage::reserved(&e,&Self::token(&e,0)),crate::stage_storage::reserved(&e,&Self::token(&e,1))]}
    // Custody mint/burn. The physical share balance sits at the receipt; the reward
    // weight and the staged claim sit with the beneficiary. No path moves a share.
    fn stage_mint_custody(e:&Env,r:&Address,owner:&Address,n:u128) {
        set_shares(e,r,shares_of(e,r).checked_add(n).unwrap());crate::custody::stage_credit(e,owner,n);
        let mut st=state(e);st.total_shares=st.total_shares.checked_add(n).unwrap();set_state(e,&st);
    }
    fn stage_burn_custody(e:&Env,r:&Address,owner:&Address,n:u128) {
        assert!(crate::stage_storage::available(e,owner)>=n,"shares locked by command");
        set_shares(e,r,shares_of(e,r).checked_sub(n).expect("custody short"));crate::custody::stage_debit(e,owner,n);
        let mut st=state(e);st.total_shares=st.total_shares.checked_sub(n).unwrap();set_state(e,&st);
    }
    // Minimal pilot: initially unpaused only after Ledger-only configuration.
    // Guardian pause is one-way. No keeper/admin resume exists in these bytes;
    // reopening deposits requires the later capped release and upgrade review.
    pub fn stage_configure_guardian(e:Env,admin_addr:Address,guardian:Address) {
        Self::require_admin(&e,&admin_addr);assert_eq!(guardian,admin_addr,"Ledger holds guardian and admin");
        assert!(e.storage().instance().has(&StageVaultKey::StgV1Receipt));
        assert!(!e.storage().instance().has(&StageVaultKey::StgV1Guardian),"guardian already configured");
        assert_eq!(state(&e).total_shares,0);
        assert_eq!(crate::stage_storage::reserved(&e,&Self::token(&e,0)),0);
        assert_eq!(crate::stage_storage::reserved(&e,&Self::token(&e,1)),0);
        e.storage().instance().set(&StageVaultKey::StgV1Guardian,&guardian);
        e.storage().instance().set(&StageVaultKey::StgV1DepositPause,&false);
    }
    pub fn stage_guardian(e:Env)->Address {e.storage().instance().get(&StageVaultKey::StgV1Guardian).expect("guardian not configured")}
    pub fn stage_deposits_paused(e:Env)->bool {
        !e.storage().instance().has(&StageVaultKey::StgV1Guardian) || e.storage().instance().get(&StageVaultKey::StgV1DepositPause).unwrap_or(true)
    }
    pub fn stage_pause_deposits(e:Env,guardian:Address) {
        assert_eq!(guardian,Self::stage_guardian(e.clone()));guardian.require_auth();
        e.storage().instance().set(&StageVaultKey::StgV1DepositPause,&true);
    }
    fn stage_admit_deposit(e:&Env) {
        assert!(!crate::stage_storage::composition_dirty(e),"refresh investment composition before new deposits");
        assert!(!Self::stage_deposits_paused(e.clone()),"guardian deposit pause");
        Self::require_not_paused(e);
        let root=Self::stage_root(e);Self::stage_band(e,root);
        let c=config(e);let age=params(e).oracle_max_age_mult;
        let p0=Self::read_price(e,&c.oracle,&c.token0,age).expect("fresh PYUSD price required");
        let p1=Self::read_price(e,&c.oracle,&c.token1,age).expect("fresh USDC price required");
        let slot=ConcentratedPoolClient::new(e,&c.pool).get_slot0();
        assert!(slot.tick>=-20 && slot.tick<20,"outside fixed pilot range");
        let u=|n|soroban_sdk::U256::from_u128(e,n);let scale=1_000_000_000_000_000_000u128;
        let sqrt=slot.sqrt_price_x96.mul(&u(scale)).div(&u(1u128<<96));
        let observed=sqrt.mul(&sqrt).mul(&u(p1)).mul(&u(scale));let reference=u(p0).mul(&u(scale)).mul(&u(scale));
        assert!(observed>=reference.mul(&u(998_002_098_460_885_076)) && observed<=reference.mul(&u(1_002_001_901_140_484_655)),"depeg deposit edge");
    }
    fn stage_fixed_invest_band(e:&Env,slot:&Slot0)->bool {
        if slot.tick < -20 || slot.tick >= 20 {return false;}
        let u=|n|soroban_sdk::U256::from_u128(e,n);
        let sqrt=slot.sqrt_price_x96.mul(&u(1_000_000_000_000_000_000)).div(&u(1u128<<96));
        // Reject inconsistent/outlier sqrt values before squaring. No oracle,
        // caller-provided reference or mutable center participates in this path.
        if sqrt < u(999_000_549_780_071_480) || sqrt > u(1_001_000_450_120_021_002) {return false;}
        let price=sqrt.mul(&sqrt);let one=u(1_000_000_000_000_000_000).mul(&u(1_000_000_000_000_000_000));
        price.mul(&u(10_000))>=one.mul(&u(9_980)) && price.mul(&u(10_000))<=one.mul(&u(10_020))
    }
    pub fn stage_set_invest_keeper(e:Env,admin_addr:Address,keeper:Address) {
        Self::require_admin(&e,&admin_addr);
        assert!(e.storage().instance().has(&StageVaultKey::StgV1Receipt));
        assert!(!e.storage().instance().has(&StageVaultKey::StgV1InvestKeeper));
        assert_eq!(Self::total_shares(&e),0);
        assert_eq!(crate::stage_storage::reserved(&e,&Self::token(&e,0)),0);
        assert_eq!(crate::stage_storage::reserved(&e,&Self::token(&e,1)),0);
        e.storage().instance().set(&StageVaultKey::StgV1InvestKeeper,&keeper);
    }
    pub fn stage_investment_keeper(e:Env)->Address {e.storage().instance().get(&StageVaultKey::StgV1InvestKeeper).expect("keeper not configured")}
    /// Authenticated keeper observations are separate measured transactions. Three
    /// successive samples do not establish continuous within-ledger stability.
    pub fn stage_checkpoint_investment(e:Env,keeper:Address)->StageInvestmentGuard {
        assert_eq!(keeper,Self::stage_investment_keeper(e.clone()));keeper.require_auth();
        assert!(e.storage().instance().has(&StageVaultKey::StgV1Receipt),"stage unbound");
        let ledger=e.ledger().sequence();
        let previous:Option<StageInvestmentGuard>=e.storage().instance().get(&StageVaultKey::StgV1InvestGuard);
        if let Some(ref g)=previous {assert!(ledger>g.observed_ledger,"one guard observation per ledger");}
        let slot=ConcentratedPoolClient::new(&e,&Self::pool(&e)).get_slot0();
        let inside=Self::stage_fixed_invest_band(&e,&slot);
        let count=if !inside {0} else {match previous {
            Some(g) if g.inside && ledger==g.observed_ledger.checked_add(1).unwrap()=>g.healthy_count.checked_add(1).unwrap().min(3),
            _=>1,
        }};
        let observed=StageInvestmentGuard{observed_ledger:ledger,healthy_count:count,inside,observed_tick:slot.tick};
        e.storage().instance().set(&StageVaultKey::StgV1InvestGuard,&observed);
        e.events().publish((Symbol::new(&e,"investguard"),),observed.clone());observed
    }
    pub fn stage_investment_guard(e:Env)->Option<StageInvestmentGuard> {e.storage().instance().get(&StageVaultKey::StgV1InvestGuard)}
    fn stage_require_investment_guard(e:&Env,slot:&Slot0) {
        let g:StageInvestmentGuard=e.storage().instance().get(&StageVaultKey::StgV1InvestGuard).expect("investment guard missing");
        let now=e.ledger().sequence();
        assert!(g.inside && g.healthy_count==3 && g.observed_ledger<now && now-g.observed_ledger<=3,"investment guard immature or stale");
        assert!(Self::stage_fixed_invest_band(e,slot),"fixed USD investment band");
    }
    fn stage_root(e:&Env)->u128 {
        let c=config(e);let prm=params(e);
        let pu=Self::read_price(e,&c.oracle,&c.token0,prm.oracle_max_age_mult).expect("fresh token0 price required");
        let po=Self::read_price(e,&c.oracle,&c.token1,prm.oracle_max_age_mult).expect("fresh token1 price required");
        let root=isqrt(mul_div(e,po,NAV_RATIO_SCALE,pu));assert!(root>0);
        let mut st=state(e);st.last_nav_root=root;st.last_nav_root_at=e.ledger().timestamp();set_state(e,&st);root
    }
    fn stage_band(e:&Env,root:u128) {
        let slot=ConcentratedPoolClient::new(e,&Self::pool(e)).get_slot0();
        let q=soroban_sdk::U256::from_u128(e,1u128<<96);
        let s=slot.sqrt_price_x96.mul(&soroban_sdk::U256::from_u128(e,1_000_000_000_000_000_000)).div(&q).to_u128().unwrap();
        let fair=mul_div(e,1_000_000_000_000_000_000,1_000_000_000,root);
        let b=params(e).max_pool_divergence_bps;assert!(b>0 && b<=500);
        let spot=s.checked_mul(s).unwrap();let fair_price=fair.checked_mul(fair).unwrap();
        let lower=apply_slippage_floor(e,fair_price,b);let upper=mul_div(e,fair_price,10_000+b as u128,10_000);
        assert!(spot>=lower && spot<=upper,"fresh pool/oracle band");
    }
    // Independent oracle-price CL value, not a current-spot mark that hides
    // price-reversion cost. Fixed wide-range sqrt constants are rounded inward.
    fn stage_fair_lp(e:&Env,l:u128,root:u128)->u128 {
        let q=1_000_000_000_000_000_000u128;
        let lo=999_000_549_780_071_480u128;let hi=1_001_000_450_120_021_002u128;
        let s=mul_div(e,q,1_000_000_000,root).max(lo).min(hi);
        let u=|n|soroban_sdk::U256::from_u128(e,n);
        let a=u(l).mul(&u(q)).mul(&u(hi-s)).div(&u(s).mul(&u(hi))).to_u128().unwrap();
        let b=mul_div(e,l,s-lo,q);
        a.checked_add(Self::other_amount_value_at_root(e,b,root)).unwrap()
    }
    fn stage_fair_nav(e:&Env,root:u128)->u128 {
        let l=Self::position_liquidity(e);
        let fees=if l>0 {ConcentratedPoolClient::new(e,&Self::pool(e)).get_all_position_fees(&e.current_contract_address())}else{soroban_sdk::vec![e,0,0]};
        Self::stage_fair_lp(e,l,root).checked_add(Self::balance_of_token(e,&Self::token(e,0))).unwrap()
            .checked_add(Self::other_amount_value_at_root(e,Self::balance_of_token(e,&Self::token(e,1)),root)).unwrap()
            .checked_add(fees.get(0).unwrap()).unwrap().checked_add(Self::other_amount_value_at_root(e,fees.get(1).unwrap(),root)).unwrap()
    }
    // Current estimate_withdraw_position includes accrued LP fees (part-a proof).
    // Call only after a fresh position snapshot; do not add pending fees twice.
    fn stage_actual_nav(e:&Env,root:u128)->u128 {
        Self::position_value_at_root(e,root,false)
            .checked_add(Self::balance_of_token(e,&Self::token(e,0))).unwrap()
            .checked_add(Self::other_amount_value_at_root(e,Self::balance_of_token(e,&Self::token(e,1)),root)).unwrap()
    }
    pub fn stage_check_open(e:Env,r:Address,id:u64,j:StageJob) {
        let j=stage_job(&e,&r,id,0,j);Self::stage_admit_deposit(&e);
        // Existing max_deploy acts only as a per-command raw-PYUSD bound here;
        // this minimal release does NOT enforce an aggregate backing cap.
        assert!(j.kind==0 && j.a>0 && j.b==0 && params(&e).max_deploy>0 && j.a<=params(&e).max_deploy);
    }
    pub fn stage_accept_funds(e:Env,r:Address,id:u64,j:StageJob) {
        let j=stage_job(&e,&r,id,0,j);j.owner.require_auth();Self::stage_admit_deposit(&e);
        assert!(j.kind==0 && j.a>0 && j.b==0 && j.a<=params(&e).max_deploy);
        crate::accounting::begin(&e);let t=Self::token(&e,0);let before=stage_raw(&e,&t);
        let n=j.a.checked_add(j.reserve).unwrap();
        token::Client::new(&e,&t).transfer(&j.owner,&e.current_contract_address(),&to_i128(n));
        assert_eq!(stage_raw(&e,&t)-before,n);crate::stage_storage::adjust(&e,&t,n,0);crate::accounting::end(&e);
    }
    fn stage_swap(e:&Env,i:u32,n:u128,min:u128)->u128 {
        let cfg=config(e);Self::require_not_paused(e);let root=Self::stage_root(e);Self::stage_band(e,root);
        let pool=ConcentratedPoolClient::new(e,&cfg.pool);assert!(!pool.get_is_killed_swap());
        let estimate=pool.estimate_swap(&i,&(1-i),&n);
        Self::require_quote_near_oracle(e,i,n,estimate,Some(root),params(e).max_pool_divergence_bps,true);
        let floor=apply_slippage_floor(e,estimate,Self::slippage_bps(e)).max(min);assert!(estimate>=floor && floor>0);
        let t=Self::token(e,i);let other=Self::token(e,1-i);let before=stage_raw(e,&t);let out_before=stage_raw(e,&other);
        e.authorize_as_current_contract(soroban_sdk::vec![e,auth_entry(e,&t,"transfer",(e.current_contract_address(),cfg.pool.clone(),to_i128(n)).into_val(e),Vec::new(e))]);
        let out=pool.swap(&e.current_contract_address(),&i,&(1-i),&n,&floor);
        assert_eq!(before-stage_raw(e,&t),n);assert_eq!(stage_raw(e,&other)-out_before,out);assert!(out>=floor);out
    }
    pub fn stage_convert_open(e:Env,r:Address,id:u64,j:StageJob)->u128 {
        let j=stage_job(&e,&r,id,1,j);crate::accounting::begin(&e);
        assert!(j.kind==0 && j.b==0 && j.swap_in>0 && j.swap_in<j.a);
        let out=Self::stage_swap(&e,0,j.swap_in,j.min_swap);
        crate::stage_storage::adjust(&e,&Self::token(&e,0),0,j.swap_in);
        crate::stage_storage::adjust(&e,&Self::token(&e,1),out,0);crate::accounting::end(&e);out
    }
    pub fn stage_cash_mint(e:Env,r:Address,id:u64,j:StageJob)->u128 {
        let j=stage_job(&e,&r,id,2,j);Self::require_not_paused(&e);crate::custody::require_direct_holder(&e,&j.owner);
        crate::accounting::begin(&e);crate::rewards::normal(&e,&j.owner);
        let root=Self::stage_root(&e);Self::stage_band(&e,root);Self::collect_fees(&e);
        let before=Self::stage_actual_nav(&e,root);let supply=Self::total_shares(&e);
        let added=j.a.checked_add(Self::other_amount_value_at_root(&e,j.b,root)).unwrap();
        // Aggregate cap deliberately absent from the minimal pilot release.
        let shares=if supply==0 {assert_eq!(before,0);added}else{mul_div(&e,added,supply,before)};
        assert!(shares>0 && shares>=j.min_shares);
        crate::stage_storage::adjust(&e,&Self::token(&e,0),0,j.a);
        crate::stage_storage::adjust(&e,&Self::token(&e,1),0,j.b);
        Self::stage_mint_custody(&e,&r,&j.owner,shares);crate::stage_storage::lock(&e,&j.owner,shares);
        crate::rewards::reset_debt(&e,&j.owner);crate::accounting::end(&e);shares
    }
    // Current155a bytecode component proofs compare these exact Q96 integer
    // totals with fee-subtracted venue withdrawal quotes. Difference of rounded
    // totals is required; principal(deltaL) alone loses existing-position carry.
    pub fn stage_composition_dirty(e:Env)->bool {crate::stage_storage::composition_dirty(&e)}
    fn stage_local_principal(e:&Env,l:u128,slot:&Slot0)->(u128,u128) {
        let u=|n|soroban_sdk::U256::from_u128(e,n);let q=u(1u128<<96);
        let lo=u(79_148_977_909_814_923_576_066_331_265);
        let hi=u(79_307_426_338_960_776_842_885_539_845);
        let x=if slot.sqrt_price_x96<lo {lo.clone()} else if slot.sqrt_price_x96>hi {hi.clone()} else {slot.sqrt_price_x96.clone()};
        // Divide the product by hi with quotient/remainder decomposition to
        // avoid a 128+96+96-bit intermediate. All intermediates fit U256.
        let lq=u(l).mul(&q);let whole=lq.div(&hi);let rem=lq.sub(&whole.mul(&hi));let d=hi.sub(&x);
        let numerator=whole.mul(&d).add(&rem.mul(&d).div(&hi));
        (numerator.div(&x).to_u128().unwrap(),u(l).mul(&x.sub(&lo)).div(&q).to_u128().unwrap())
    }
    pub fn stage_invest_cash(e:Env,r:Address,id:u64,j:StageJob)->u128 {
        let j=stage_job(&e,&r,id,3,j);Self::require_not_paused(&e);crate::accounting::begin(&e);
        assert_eq!(Self::ticks(&e),(-20,20));
        let p=ConcentratedPoolClient::new(&e,&Self::pool(&e));
        let slot=p.get_slot0();Self::stage_require_investment_guard(&e,&slot);
        assert!(!p.get_is_killed_deposit());
        let root=1_000_000_000u128;
        let a=Self::balance_of_token(&e,&Self::token(&e,0));let b=Self::balance_of_token(&e,&Self::token(&e,1));
        assert!(a>=j.a && b>=j.b,"command amount no longer available; cancel or retry");
        let liquidity_before=Self::position_liquidity(&e);
        let fair_before=Self::stage_fair_lp(&e,liquidity_before,root);
        let actual_before=Self::stage_local_principal(&e,liquidity_before,&slot);
        let result=Self::deposit_balances(&e,j.a,j.b,a,b);assert!(result.minted>0,"investment unavailable");
        let liquidity_after=Self::position_liquidity(&e);
        assert_eq!(liquidity_after,liquidity_before.checked_add(result.minted).unwrap());
        let spent0=a.checked_sub(result.idle_underlying).unwrap();let spent1=b.checked_sub(result.idle_other).unwrap();
        let spent=spent0.checked_add(spent1).unwrap();
        let fair_gain=Self::stage_fair_lp(&e,liquidity_after,root).checked_sub(fair_before).unwrap();
        let actual_after=Self::stage_local_principal(&e,liquidity_after,&slot);
        let actual_gain=actual_after.0.checked_sub(actual_before.0).unwrap().checked_add(actual_after.1.checked_sub(actual_before.1).unwrap()).unwrap();
        assert!(fair_gain<=spent.checked_add(4).unwrap(),"unexplained investment gain");
        let cost=spent.saturating_sub(fair_gain).max(spent.saturating_sub(actual_gain));
        assert!(cost<=j.reserve,"execution reserve insufficient");
        crate::stage_storage::adjust(&e,&Self::token(&e,0),0,cost);
        assert!(fair_gain.checked_add(cost).unwrap()>=spent);
        assert!(actual_gain.checked_add(cost).unwrap()>=spent);
        // The old fee claim is unchanged by current pinned deposit bytecode.
        // Never publish the helper's spent-amount cache as actual composition.
        crate::stage_storage::set_composition_dirty(&e,true);
        let k=StageVaultKey::StgV1Journal(id);assert!(!e.storage().persistent().has(&k));
        let journal=StageInvestmentJournal{owner:j.owner,shares:j.shares,liquidity_before,liquidity_added:result.minted,
            liquidity_after,spent0,spent1,original_reserve:j.reserve,cost,
            refund:j.reserve-cost,execution_ledger:e.ledger().sequence(),settled:false};
        e.storage().persistent().set(&k,&journal);e.storage().persistent().extend_ttl(&k,500_000,1_000_000);
        e.events().publish((Symbol::new(&e,"invested"),id),journal);
        crate::accounting::end(&e);result.minted
    }
    // Normal finalisation includes this measured refresh. Permissionless phase5
    // recovery deliberately skips it and settles immutable owner claims only.
    pub fn stage_refresh_finalise(e:Env,r:Address,id:u64,j:StageJob) {
        let _=stage_job(&e,&r,id,5,j);crate::accounting::begin(&e);
        assert!(Self::refresh_position_amounts_inner(&e),"current composition unavailable; use in-kind recovery");
        crate::accounting::end(&e);
    }
    pub fn stage_investment_journal(e:Env,id:u64)->StageInvestmentJournal {
        let k=StageVaultKey::StgV1Journal(id);
        let j=e.storage().persistent().get(&k).expect("unknown investment journal");
        e.storage().persistent().extend_ttl(&k,500_000,1_000_000);j
    }
    // No oracle, keeper, pool, deadline or guardian dependency. Investment has
    // already settled its loss and refreshed its composition atomically.
    pub fn stage_finalise_cash(e:Env,r:Address,id:u64,j:StageJob) {
        let j=stage_job(&e,&r,id,5,j);crate::accounting::begin(&e);
        let mut recorded=Self::stage_investment_journal(e.clone(),id);
        assert!(!recorded.settled && recorded.owner==j.owner && recorded.shares==j.shares);
        assert_eq!(recorded.original_reserve,j.reserve);
        assert_eq!(recorded.cost.checked_add(recorded.refund).unwrap(),j.reserve);
        crate::stage_storage::adjust(&e,&Self::token(&e,0),0,recorded.refund);
        stage_pay(&e,&Self::token(&e,0),&j.owner,recorded.refund);
        crate::stage_storage::unlock(&e,&j.owner,j.shares);
        recorded.settled=true;let k=StageVaultKey::StgV1Journal(id);
        e.storage().persistent().set(&k,&recorded);e.storage().persistent().extend_ttl(&k,500_000,1_000_000);
        e.events().publish((Symbol::new(&e,"finalised"),id),(j.owner,recorded.refund,j.shares));
        crate::accounting::end(&e);
    }
    pub fn stage_lock_close(e:Env,r:Address,id:u64,j:StageJob) {
        let j=stage_job(&e,&r,id,10,j);crate::custody::require_direct_holder(&e,&j.owner);
        assert!(j.kind==1 && j.shares>0 && j.shares<=crate::stage_storage::available(&e,&j.owner));
        crate::stage_storage::lock(&e,&j.owner,j.shares);
    }
    // StgV2: settle-only seizure reassign, called by the bound receipt's permissionless
    // sync of a queued seizure. No venue claim, no pool or oracle call, no share moves.
    pub fn stage_seize_reassign(e:Env,r:Address,from:Address,to:Address,n:u128)->bool {
        assert_eq!(r,Self::stage_receipt(e.clone()));r.require_auth();
        let opened=crate::custody::stage_seize_reassign(&e,&from,&to,n);
        e.events().publish((Symbol::new(&e,"seizesync"),),(from,to,n,opened));opened
    }
    pub fn stage_last_claim(e:Env)->Option<u64> {crate::stage_storage::last_claim(&e)}
    /// StgV2 fallback input: booked idle [token0, token1] (command liabilities and unbooked
    /// donations excluded, as in NAV). No pool, oracle or reward call.
    pub fn stage_idle(e:Env)->Vec<u128> {soroban_sdk::vec![&e,Self::balance_of_token(&e,&Self::token(&e,0)),Self::balance_of_token(&e,&Self::token(&e,1))]}
    pub fn stage_allocation(e:Env,id:u64)->StageAllocation {e.storage().persistent().get(&StageVaultKey::StgV1Allocation(id)).unwrap()}
    // Shared by the claim-first close and the no-claim emergency extraction; the caller
    // has already performed its reward boundary for `owner`. Fees are allocated at the
    // OLD supply and the exit is fully reserved before any share is burned.
    fn stage_extract(e:&Env,r:&Address,id:u64,owner:&Address,shares:u128,was_locked:bool)->(u128,u128,StageAllocation,u128) {
        let supply=Self::total_shares(e);assert!(shares>0 && shares<=supply && shares<=crate::custody::beneficial(e,owner));
        let l=Self::position_liquidity(e);let burn=mul_div(e,l,shares,supply);
        let c=config(e);let me=e.current_contract_address();let pool=ConcentratedPoolClient::new(e,&c.pool);
        let fees=if l>0 {pool.get_all_position_fees(&me)}else{soroban_sdk::vec![e,0u128,0u128]};
        let idle0=Self::balance_of_token(e,&c.token0);let idle1=Self::balance_of_token(e,&c.token1);
        let f0=fees.get(0).unwrap();let f1=fees.get(1).unwrap();
        let a0=mul_div(e,idle0.checked_add(f0).unwrap(),shares,supply);
        let a1=mul_div(e,idle1.checked_add(f1).unwrap(),shares,supply);
        let retained0=idle0+f0-a0;let retained1=idle1+f1-a1;
        let before0=stage_raw(e,&c.token0);let before1=stage_raw(e,&c.token1);
        let received=if burn>0 {
            pool.withdraw_position(&me,&c.tick_lower,&c.tick_upper,&burn,&soroban_sdk::vec![e,0u128,0u128])
        } else if l>0 {pool.claim_all_position_fees(&me)}else{soroban_sdk::vec![e,0u128,0u128]};
        assert_eq!(stage_raw(e,&c.token0)-before0,received.get(0).unwrap());
        assert_eq!(stage_raw(e,&c.token1)-before1,received.get(1).unwrap());
        let p0=received.get(0).unwrap().checked_sub(f0).expect("exact fee0 reconciliation");
        let p1=received.get(1).unwrap().checked_sub(f1).expect("exact fee1 reconciliation");
        let out0=p0.checked_add(a0).unwrap();let out1=p1.checked_add(a1).unwrap();
        crate::stage_storage::adjust(e,&c.token0,out0,0);crate::stage_storage::adjust(e,&c.token1,out1,0);
        // NAMED RULE: INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN.
        // Fees are allocated against OLD supply, exit liabilities fully funded,
        // and incumbent backing checked before any vault shares are burned.
        assert_eq!(Self::balance_of_token(e,&c.token0),retained0,"INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN token0");
        assert_eq!(Self::balance_of_token(e,&c.token1),retained1,"INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN token1");
        let allocation=StageAllocation{supply,shares,liquidity_burn:burn,fee0:f0,fee1:f1,principal0:p0,principal1:p1,allocated0:a0,allocated1:a1,retained0,retained1};
        let k=StageVaultKey::StgV1Allocation(id);e.storage().persistent().set(&k,&allocation);e.storage().persistent().extend_ttl(&k,500_000,1_000_000);
        e.events().publish((Symbol::new(e,"feealloc"),id),allocation.clone());
        if was_locked {crate::stage_storage::unlock(e,owner,shares);}
        Self::stage_burn_custody(e,r,owner,shares);
        e.events().publish((Symbol::new(e,"stageburn"),id),(shares,supply,Self::total_shares(e)));
        crate::rewards::reset_debt(e,owner);Self::set_position_liquidity(e,l-burn);
        (out0,out1,allocation,l-burn)
    }
    pub fn stage_withdraw_kind(e:Env,r:Address,id:u64,j:StageJob)->Vec<u128> {
        let j=stage_job(&e,&r,id,10,j);crate::accounting::begin(&e);
        crate::custody::require_direct_holder(&e,&j.owner);crate::rewards::normal(&e,&j.owner);
        let (out0,out1,allocation,_)=Self::stage_extract(&e,&r,id,&j.owner,j.shares,true);
        if j.min_swap==0 {
            // Direct in-kind close completes in this transaction, not an
            // omitted prerequisite/payout transaction.
            let c=config(&e);
            crate::stage_storage::adjust(&e,&c.token0,0,out0);crate::stage_storage::adjust(&e,&c.token1,0,out1);
            stage_pay(&e,&c.token0,&j.owner,out0);stage_pay(&e,&c.token1,&j.owner,out1);
            assert_eq!(Self::balance_of_token(&e,&c.token0),allocation.retained0);
            assert_eq!(Self::balance_of_token(&e,&c.token1),allocation.retained1);
        }
        assert!(Self::refresh_position_amounts_inner(&e));crate::accounting::end(&e);
        soroban_sdk::vec![&e,out0,out1]
    }
    // EMERGENCY EXIT, step 1 of 2. No venue reward claim. One pending-view read
    // for the reward boundary, the fee VIEW, and at most ONE venue extraction:
    // withdraw the exiting liquidity fraction, or collect only fees if that
    // fraction rounds to zero. Never combine a fee collection and withdrawal.
    // The mixture stays in the vault as a
    // liability to the owner; step 2 pays it. Requires the holder's own authorization
    // because the waiver flag is the holder's.
    pub fn stage_emergency_extract(e:Env,r:Address,id:u64,j:StageJob,accept_reward_waiver:bool)->Vec<u128> {
        let j=stage_job(&e,&r,id,20,j);assert_eq!(j.kind,2);j.owner.require_auth();crate::accounting::begin(&e);
        crate::custody::require_direct_holder(&e,&j.owner);
        // Command-held shares (phase 3, 5, 10) are not available; reject before any venue call.
        assert!(j.shares>0 && j.shares<=crate::stage_storage::available(&e,&j.owner),"shares locked by command");
        let (reading,proven,waived)=crate::rewards::boundary_no_claim(&e,&j.owner,accept_reward_waiver);
        let k=StageVaultKey::StgV1Exit(id);assert!(!e.storage().persistent().has(&k),"duplicate emergency exit");
        let (out0,out1,_,left)=Self::stage_extract(&e,&r,id,&j.owner,j.shares,false);
        // No venue composition read here: the cached composition is not a NAV quote
        // until the next venue read. An emptied position needs none.
        if left>0 {crate::stage_storage::set_composition_dirty(&e,true);} else {Self::record_position(&e,0,0,0);}
        let record=StageEmergencyExit{owner:j.owner.clone(),shares:j.shares,out0,out1,waived,proven,reading,extract_ledger:e.ledger().sequence(),paid:false};
        e.storage().persistent().set(&k,&record);e.storage().persistent().extend_ttl(&k,500_000,1_000_000);
        e.events().publish((Symbol::new(&e,"emergencyout"),id),record);
        crate::accounting::end(&e);soroban_sdk::vec![&e,out0,out1]
    }
    pub fn stage_emergency_record(e:Env,id:u64)->StageEmergencyExit {
        let k=StageVaultKey::StgV1Exit(id);let x=e.storage().persistent().get(&k).expect("unknown emergency exit");
        e.storage().persistent().extend_ttl(&k,500_000,1_000_000);x
    }
    // EMERGENCY EXIT, step 2 of 2. Pays the vault's OWN record once, to its recorded
    // owner. No venue, oracle, reward, keeper, deadline or pause dependency.
    pub fn stage_emergency_pay(e:Env,r:Address,id:u64,j:StageJob)->Vec<u128> {
        let j=stage_job(&e,&r,id,21,j);assert_eq!(j.kind,2);crate::accounting::begin(&e);
        let mut x=Self::stage_emergency_record(e.clone(),id);
        assert!(!x.paid && x.owner==j.owner && x.shares==j.shares,"emergency exit already paid or not this owner's");
        let c=config(&e);
        crate::stage_storage::adjust(&e,&c.token0,0,x.out0);crate::stage_storage::adjust(&e,&c.token1,0,x.out1);
        stage_pay(&e,&c.token0,&x.owner,x.out0);stage_pay(&e,&c.token1,&x.owner,x.out1);
        x.paid=true;let k=StageVaultKey::StgV1Exit(id);e.storage().persistent().set(&k,&x);e.storage().persistent().extend_ttl(&k,500_000,1_000_000);
        e.events().publish((Symbol::new(&e,"emergencypaid"),id),(x.owner.clone(),x.out0,x.out1));
        crate::accounting::end(&e);soroban_sdk::vec![&e,x.out0,x.out1]
    }
    // ---- read-only valuation and quote views (ABI.md sections 5 and 6) ----
    // Fresh primary prices or nothing. Writes no state.
    fn stage_primary(e:&Env)->Option<(u128,u128,u128)> {
        let c=config(e);let age=params(e).oracle_max_age_mult;
        let pu=Self::read_price(e,&c.oracle,&c.token0,age)?;let po=Self::read_price(e,&c.oracle,&c.token1,age)?;
        if pu>1_000_000_000_000_000_000_000_000_000_000 || po>1_000_000_000_000_000_000_000_000_000_000 {return None;}
        let root=isqrt(try_mul_div(e,po,NAV_RATIO_SCALE,pu)?);if root==0 {None} else {Some((pu,po,root))}
    }
    // Live fee-inclusive NAV in funding-asset units: booked idle plus the venue's
    // CURRENT quote of the whole position. Never the cached composition.
    fn stage_live_nav_at(e:&Env,root:u128)->Option<u128> {
        let c=config(e);let l=Self::position_liquidity(e);
        let (q0,q1)=if l>0 {
            let v:Vec<u128>=try_call(e,&c.pool,"estimate_withdraw_position",(e.current_contract_address(),c.tick_lower,c.tick_upper,l)).ok()?;
            if v.len()!=2 {return None;}
            (v.get(0)?,v.get(1)?)
        } else {(0,0)};
        let a=Self::balance_of_token(e,&c.token0).checked_add(q0)?;let b=Self::balance_of_token(e,&c.token1).checked_add(q1)?;
        a.checked_add(Self::other_amount_value_at_root(e,b,root))
    }
    pub(crate) fn stage_share_value(e:&Env,vault_shares:i128)->Vec<i128> {
        let supply=Self::total_shares(e);
        assert!(vault_shares>=0 && supply>0 && to_u128(vault_shares)<=supply,"staged valuation: invalid shares");
        let (_,_,root)=Self::stage_primary(e).expect("staged valuation: primary price unavailable");
        let nav=Self::stage_live_nav_at(e,root).expect("staged valuation: venue quote unavailable");
        soroban_sdk::vec![e,to_i128(mul_div(e,to_u128(vault_shares),nav,supply))]
    }
    // Non-trapping mirror of the admission band in stage_admit_deposit + stage_band.
    fn stage_admission_band(e:&Env,slot:&Slot0,pu:u128,po:u128,root:u128)->bool {
        if slot.tick < -20 || slot.tick >= 20 {return false;}
        let u=|n|soroban_sdk::U256::from_u128(e,n);let scale=1_000_000_000_000_000_000u128;
        let sqrt=slot.sqrt_price_x96.mul(&u(scale)).div(&u(1u128<<96));
        if sqrt>u(2*scale) {return false;}
        let observed=sqrt.mul(&sqrt).mul(&u(po)).mul(&u(scale));let reference=u(pu).mul(&u(scale)).mul(&u(scale));
        if !(observed>=reference.mul(&u(998_002_098_460_885_076)) && observed<=reference.mul(&u(1_002_001_901_140_484_655))) {return false;}
        let b=params(e).max_pool_divergence_bps;if b==0 || b>500 {return false;}
        let s=sqrt.to_u128().unwrap();let fair=mul_div(e,scale,1_000_000_000,root);
        let (Some(spot),Some(fair_price))=(s.checked_mul(s),fair.checked_mul(fair)) else {return false;};
        spot>=apply_slippage_floor(e,fair_price,b) && spot<=mul_div(e,fair_price,10_000+b as u128,10_000)
    }
    fn stage_floor(e:&Env,n:u128,tolerance_bps:u32)->u128 {mul_div(e,n,(10_000-tolerance_bps) as u128,10_000).max(1)}
    /// Read-only open quote. Tolerance and reserve margin are the caller's; the contract
    /// has no default for either. `valid=false` instead of a trap. See ABI.md section 6.
    pub fn stage_quote_open(e:Env,amount:u128,tolerance_bps:u32,reserve_margin_bps:u32)->StageOpenQuote {
        let c=config(&e);let prm=params(&e);
        let mut q=StageOpenQuote{valid:false,admissible:false,deposits_paused:Self::stage_deposits_paused(e.clone())||prm.paused,
            composition_dirty:crate::stage_storage::composition_dirty(&e),oracle_fresh:false,in_band:false,guard_ready:false,
            ledger:e.ledger().sequence(),timestamp:e.ledger().timestamp(),tick:0,swap_in:0,swap_out:0,min_swap:0,shares:0,min_shares:0,
            invest0:0,invest1:0,min_reserve:0,reserve:0,vault_slippage_bps:prm.slippage_bps,vault_divergence_bps:prm.max_pool_divergence_bps};
        let Ok(slot)=try_call::<Slot0,_>(&e,&c.pool,"get_slot0",()) else {return q;};
        q.tick=slot.tick;
        if let Some(g)=Self::stage_investment_guard(e.clone()) {
            q.guard_ready=g.inside && g.healthy_count==3 && g.observed_ledger<q.ledger && q.ledger-g.observed_ledger<=3 && Self::stage_fixed_invest_band(&e,&slot);
        }
        let Some((pu,po,root))=Self::stage_primary(&e) else {return q;};
        q.oracle_fresh=true;q.in_band=Self::stage_admission_band(&e,&slot,pu,po,root);
        q.admissible=!q.deposits_paused && !q.composition_dirty && q.in_band && prm.max_deploy>0 && amount>0 && amount<=prm.max_deploy;
        if amount==0 || amount>u64::MAX as u128 || tolerance_bps>9_999 || reserve_margin_bps>1_000_000 {return q;}
        // Conversion size matching the position's token ratio at the current pool price.
        let u=|n|soroban_sdk::U256::from_u128(&e,n);let x96=u(1u128<<96);
        let lo=u(79_148_977_909_814_923_576_066_331_265);let hi=u(79_307_426_338_960_776_842_885_539_845);
        let x=if slot.sqrt_price_x96<lo {lo.clone()} else if slot.sqrt_price_x96>hi {hi.clone()} else {slot.sqrt_price_x96.clone()};
        let (p0,p1)=Self::stage_local_principal(&e,1_000_000_000_000_000_000_000_000,&slot);
        let v1=u(p1).mul(&x96).div(&x).mul(&x96).div(&x).to_u128().unwrap();
        if p0.checked_add(v1).unwrap()==0 {return q;}
        q.swap_in=mul_div(&e,amount,v1,p0+v1);
        if q.swap_in==0 || q.swap_in>=amount {return q;}
        let Ok(out)=try_call::<u128,_>(&e,&c.pool,"estimate_swap",(0u32,1u32,q.swap_in)) else {return q;};
        if out==0 {return q;}
        q.swap_out=out;q.min_swap=Self::stage_floor(&e,out,tolerance_bps);
        let added=(amount-q.swap_in).checked_add(Self::other_amount_value_at_root(&e,out,root)).unwrap();
        let supply=Self::total_shares(&e);
        let Some(nav)=Self::stage_live_nav_at(&e,root) else {return q;};
        q.shares=if supply==0 {if nav!=0 {return q;} added} else {if nav==0 {return q;} mul_div(&e,added,supply,nav)};
        if q.shares==0 {return q;}
        q.min_shares=Self::stage_floor(&e,q.shares,tolerance_bps);
        // Reserve the investment would require after this conversion. The entrant's own
        // swap moves the pool, so the cost is evaluated at the PREDICTED post-conversion
        // price x1 = x0 - swap_out*2^96/active_liquidity (exact within one liquidity
        // segment), with the same integer arithmetic as stage_invest_cash. No margin.
        // The pinned pilot range is [-20,20]. With spacing 20 its only possible
        // interior liquidity boundary is tick 0. A crossing makes the one-segment
        // prediction invalid; refuse that quote instead of understating reserve.
        let Ok(spacing)=try_call::<i32,_>(&e,&c.pool,"get_tick_spacing",()) else {return q;};
        if spacing!=20 {return q;}
        let Ok(active)=try_call::<u128,_>(&e,&c.pool,"get_active_liquidity",()) else {return q;};
        if active==0 {return q;}
        {
            // Round the boundary output DOWN and reject equality. Inverting
            // the rounded output with floor(out*Q96/L) could otherwise miss
            // a crossing by one raw unit at tick 0.
            let boundary=if slot.sqrt_price_x96>=x96 {x96.clone()}else{lo.clone()};
            if slot.sqrt_price_x96<=boundary {return q;}
            let boundary_out=u(active).mul(&slot.sqrt_price_x96.sub(&boundary)).div(&x96);
            if u(out)>=boundary_out {return q;}
        }
        let step=u(out).mul(&x96).div(&u(active));if step>=slot.sqrt_price_x96 {return q;}
        let x1=slot.sqrt_price_x96.sub(&step);
        let after_slot=Slot0{sqrt_price_x96:x1.clone(),tick:0};
        if x1<=lo || x1>=hi || !Self::stage_fixed_invest_band(&e,&after_slot) {return q;}
        let (a0,a1)=(amount-q.swap_in,out);
        let l0=u(a0).mul(&x1).div(&x96).mul(&hi).div(&hi.sub(&x1));let l1=u(a1).mul(&x96).div(&x1.sub(&lo));
        let Some(minted)=(if l0<l1 {l0} else {l1}).to_u128() else {return q;};
        let before=Self::position_liquidity(&e);let Some(after)=before.checked_add(minted) else {return q;};
        if minted==0 {return q;}
        let b=Self::stage_local_principal(&e,before,&after_slot);let a=Self::stage_local_principal(&e,after,&after_slot);
        let actual_gain=a.0.saturating_sub(b.0)+a.1.saturating_sub(b.1);
        // The venue rounds what it takes up; one unit per leg bounds that.
        q.invest0=(a.0.saturating_sub(b.0)+1).min(a0);q.invest1=(a.1.saturating_sub(b.1)+1).min(a1);
        let parity=1_000_000_000u128;let spent=q.invest0+q.invest1;
        let fair_gain=Self::stage_fair_lp(&e,after,parity).saturating_sub(Self::stage_fair_lp(&e,before,parity));
        q.min_reserve=spent.saturating_sub(fair_gain).max(spent.saturating_sub(actual_gain));
        q.reserve=q.min_reserve+mul_div_ceil(&e,q.min_reserve,reserve_margin_bps as u128,10_000);
        q.valid=true;q
    }
    /// Read-only close quote. `min_swap` is the floor on the swap output of the paired
    /// leg only, which is what stage_convert_exit enforces. See ABI.md section 6.
    pub fn stage_quote_close(e:Env,shares:u128,tolerance_bps:u32)->StageCloseQuote {
        let c=config(&e);let prm=params(&e);let me=e.current_contract_address();
        let mut q=StageCloseQuote{valid:false,ledger:e.ledger().sequence(),timestamp:e.ledger().timestamp(),tick:0,shares,out0:0,out1:0,
            swap_out:0,min_swap:0,total:0,vault_slippage_bps:prm.slippage_bps,vault_divergence_bps:prm.max_pool_divergence_bps};
        let supply=Self::total_shares(&e);
        if shares==0 || shares>supply || tolerance_bps>9_999 {return q;}
        let l=Self::position_liquidity(&e);let burn=mul_div(&e,l,shares,supply);
        let (mut f0,mut f1,mut p0,mut p1)=(0u128,0u128,0u128,0u128);
        if l>0 {
            let Ok(slot)=try_call::<Slot0,_>(&e,&c.pool,"get_slot0",()) else {return q;};q.tick=slot.tick;
            let Ok(fees)=try_call::<Vec<u128>,_>(&e,&c.pool,"get_all_position_fees",(me.clone(),)) else {return q;};
            if fees.len()!=2 {return q;}
            f0=fees.get(0).unwrap();f1=fees.get(1).unwrap();
            if burn>0 {
                let Ok(est)=try_call::<Vec<u128>,_>(&e,&c.pool,"estimate_withdraw_position",(me,c.tick_lower,c.tick_upper,burn)) else {return q;};
                if est.len()!=2 {return q;}
                let (Some(a),Some(b))=(est.get(0).unwrap().checked_sub(f0),est.get(1).unwrap().checked_sub(f1)) else {return q;};
                p0=a;p1=b;
            }
        }
        q.out0=p0+mul_div(&e,Self::balance_of_token(&e,&c.token0)+f0,shares,supply);
        q.out1=p1+mul_div(&e,Self::balance_of_token(&e,&c.token1)+f1,shares,supply);
        if q.out1>0 {
            let Ok(out)=try_call::<u128,_>(&e,&c.pool,"estimate_swap",(1u32,0u32,q.out1)) else {return q;};
            q.swap_out=out;if out>0 {q.min_swap=Self::stage_floor(&e,out,tolerance_bps);}
        }
        q.total=q.out0+q.swap_out;q.valid=true;q
    }
    /// Permissionless bounded keepalive for the overlay's persistent records.
    pub fn stage_bump(e:Env,ids:Vec<u64>,owners:Vec<Address>) {
        assert!(ids.len()<=8 && owners.len()<=8,"bounded keepalive");
        for id in ids.iter() {for k in [StageVaultKey::StgV1Journal(id),StageVaultKey::StgV1Allocation(id),StageVaultKey::StgV1Exit(id)] {
            if e.storage().persistent().has(&k) {e.storage().persistent().extend_ttl(&k,500_000,1_000_000);}}}
        for o in owners.iter() {let _=crate::stage_storage::locked(&e,&o);let _=crate::custody::beneficial(&e,&o);}
    }
    pub fn stage_convert_exit(e:Env,r:Address,id:u64,j:StageJob)->u128 {
        let j=stage_job(&e,&r,id,11,j);crate::accounting::begin(&e);
        let converted=if j.b>0 {Self::stage_swap(&e,1,j.b,j.min_swap)}else{0};
        crate::stage_storage::adjust(&e,&Self::token(&e,0),0,j.a);
        crate::stage_storage::adjust(&e,&Self::token(&e,1),0,j.b);
        let total=j.a.checked_add(converted).unwrap();stage_pay(&e,&Self::token(&e,0),&j.owner,total);
        crate::accounting::end(&e);total
    }
    pub fn stage_cancel_kind(e:Env,r:Address,id:u64,j:StageJob) {
        let bound:Address=e.storage().instance().get(&StageVaultKey::StgV1Receipt).unwrap();assert_eq!(bound,r);r.require_auth();
        let _ = id; // Receipt passes its stored command under real contract authorization; no reentry callback.
        crate::accounting::begin(&e);
        let (a,b)=match j.phase {
            0=>(0,0),1|2=>(j.a.checked_add(j.reserve).unwrap(),j.b),
            3=>{crate::stage_storage::unlock(&e,&j.owner,j.shares);(j.reserve,0)},
            10=>{crate::stage_storage::unlock(&e,&j.owner,j.shares);(0,0)},
            11=>(j.a,j.b),_=>panic!("terminal command"),
        };
        for (t,n) in [(Self::token(&e,0),a),(Self::token(&e,1),b)] {
            crate::stage_storage::adjust(&e,&t,0,n);stage_pay(&e,&t,&j.owner,n);
        }
        crate::accounting::end(&e);
    }
}

// PILOT_RELEASE_GUARDS_V1: generated only in the isolated workspace.
fn pilot_public_network(e:&Env)->bool {
    let public=e.crypto().sha256(&soroban_sdk::Bytes::from_slice(e,b"Public Global Stellar Network ; September 2015"));
    e.ledger().network_id()==public.to_bytes()
}
fn pilot_bound(e:&Env)->bool {e.storage().instance().has(&StageVaultKey::StgV1Receipt)}
fn pilot_require_legacy_financial(e:&Env) {
    assert!(!pilot_public_network(e) && !pilot_bound(e),"minimal pilot requires staged financial route");
}
fn pilot_require_unbound_configuration(e:&Env) {
    assert!(!pilot_bound(e),"pilot binding freezes legacy configuration");
}
fn pilot_require_pinned_admin(e:&Env,admin:&Address) {
    if pilot_public_network(e) {
        assert_eq!(*admin,Address::from_str(e,"GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP"),"pilot Ledger admin required");
    }
}

// Test-only v2' build (`--features upgrade-marker`): proves a state-preserving upgrade.
#[cfg(feature="upgrade-marker")]
#[contractimpl]
impl ClShareVault {
    pub fn upgrade_marker(_e:Env)->u32 {2}
}
