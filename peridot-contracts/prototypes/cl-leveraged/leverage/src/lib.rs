#![no_std]
//! PROTOTYPE leveraged PYUSD/USDC CL position with take-profit (LEVERAGE-SPEC.md). Local only;
//! never deployed. `cl-leveraged` is the margin escrow: it owns each position's receipt commands
//! and shares, borrows USDC in the lending market's per-position margin namespace, and repays from
//! the position's own in-kind exit before anything reaches the owner. It is never invoked inside
//! the heavy receipt stages (convert, mint, invest, withdraw); it reads their typed results.
mod types;
#[cfg(test)]
mod test;

pub use types::*;

use cl_lev_interface::{
    Asset, GroupControllerClient, GroupReceiptClient, MarginMarketClient, PairOpenTermsV1, ReflectorClient,
    SwapAdapterClient, BPS, PRICE_SCALE, SCALE_1E6,
};
use cl_lev_interface::stage1::{AmountsV1, CloseModeV1, CloseTermsV1, PhaseV1, ResultV1};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, panic_with_error, symbol_short, token, vec, Address, Env, IntoVal, Symbol,
};

const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND: u32 = 1_000_000;
const RISK_TIMELOCK_SECS: u64 = 86_400;
/// Reflector round length (DESIGN §1.1). A price TP confirm needs a strictly newer round.
const ORACLE_RESOLUTION_SECS: u64 = 300;
const RECEIPT_MAX_DEADLINE: u64 = 3_600;
/// Interest cover for the ledgers between the deficit swap and settlement (cf. margin-controller
/// CLOSE_DEBT_BUFFER_BPS). Unused USDC is returned to the owner.
const SETTLE_DEBT_BUFFER_BPS: u128 = 10;

#[contract]
pub struct LeveragedCl;

// ------------------------------------------------------------------------------------ storage
fn cfg(e: &Env) -> LevConfigV1 {
    e.storage().instance().get(&LevKey::LevV1Cfg).unwrap_or_else(|| panic_with_error!(e, LevError::NotInitialized))
}
fn risk(e: &Env) -> LevRiskV1 {
    e.storage().instance().get(&LevKey::LevV1Risk).unwrap()
}
fn load(e: &Env, pid: u64) -> PositionV1 {
    let k = LevKey::LevV1Pos(pid);
    let p: PositionV1 = e.storage().persistent().get(&k).unwrap_or_else(|| panic_with_error!(e, LevError::BadPhase));
    p
}
fn save(e: &Env, pid: u64, p: &PositionV1) {
    let k = LevKey::LevV1Pos(pid);
    e.storage().persistent().set(&k, p);
    e.storage().persistent().extend_ttl(&k, TTL_THRESHOLD, TTL_EXTEND);
}
fn pending(e: &Env, p: &PositionV1) -> OpenLevV1 {
    match &p.pending {
        PendingV1::Open(t) => t.clone(),
        PendingV1::None => panic_with_error!(e, LevError::BadPhase),
    }
}
fn need(e: &Env, ok: bool, err: LevError) {
    if !ok {
        panic_with_error!(e, err);
    }
}
fn event(e: &Env, name: &str, pid: u64, a: u128, b: u128) {
    #[allow(deprecated)]
    e.events().publish((Symbol::new(e, name),), (pid, a, b));
}

// ------------------------------------------------------------------------------------ auth helpers
/// Pre-authorize one nested `token.transfer(self, to, amount)` invoked by a callee (the receipt,
/// the market or the swap adapter). Equality with a configured address is not authorization.
fn auth_transfers(e: &Env, items: &[(&Address, &Address, u128)]) {
    let mut entries = vec![e];
    for (tok, to, amount) in items.iter() {
        if *amount == 0 {
            continue;
        }
        entries.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: (*tok).clone(),
                fn_name: symbol_short!("transfer"),
                args: (e.current_contract_address(), (*to).clone(), *amount as i128).into_val(e),
            },
            sub_invocations: vec![e],
        }));
    }
    if !entries.is_empty() {
        e.authorize_as_current_contract(entries);
    }
}
fn pay(e: &Env, tok: &Address, to: &Address, amount: u128) {
    if amount > 0 {
        token::Client::new(e, tok).transfer(&e.current_contract_address(), to, &(amount as i128));
    }
}

// ------------------------------------------------------------------------------------ prices
/// Reflector CALI PYUSD/USDC with the group's age guard; optional depeg guard (new risk only).
fn oracle_price(e: &Env, c: &LevConfigV1, r: &LevRiskV1, depeg_guard: bool) -> (u128, u64) {
    let pd = ReflectorClient::new(e, &c.oracle).lastprice(&Asset::Stellar(c.token0.clone()));
    let pd = pd.unwrap_or_else(|| panic_with_error!(e, LevError::OracleStale));
    let now = e.ledger().timestamp();
    need(e, pd.price > 0 && pd.timestamp <= now && now - pd.timestamp <= r.max_oracle_age_secs, LevError::OracleStale);
    let p = pd.price as u128;
    if depeg_guard {
        let d = if p > PRICE_SCALE { p - PRICE_SCALE } else { PRICE_SCALE - p };
        need(e, d * BPS <= PRICE_SCALE * r.max_depeg_bps as u128, LevError::Depeg);
    }
    (p, pd.timestamp)
}
fn fresh(e: &Env, r: &LevRiskV1, ts: u64) -> bool {
    let now = e.ledger().timestamp();
    ts <= now && now - ts <= r.max_ckpt_age_secs
}
/// NAV checkpoint rate (USDC per unit, 1e6) and its timestamp: the stored one if fresh, else an
/// inline permissionless refresh. `None` when the group guards refuse a refresh.
fn nav_rate(e: &Env, c: &LevConfigV1, r: &LevRiskV1) -> Option<(u128, u64)> {
    let rc = GroupReceiptClient::new(e, &c.receipt);
    if let Some(ck) = rc.nav_checkpoint() {
        if fresh(e, r, ck.ts) {
            return Some((ck.rate, ck.ts));
        }
    }
    match rc.try_checkpoint_nav() {
        Ok(Ok(rate)) => Some((rate, e.ledger().timestamp())),
        _ => None,
    }
}
/// Liquidation-only fallback (group DESIGN §2): stored if fresh, else cause-gated inline refresh.
fn fallback_rate(e: &Env, c: &LevConfigV1, r: &LevRiskV1) -> Option<(u128, u64)> {
    let rc = GroupReceiptClient::new(e, &c.receipt);
    if let Some(ck) = rc.fallback_checkpoint() {
        if fresh(e, r, ck.ts) {
            return Some((ck.rate, ck.ts));
        }
    }
    match rc.try_checkpoint_fallback() {
        Ok(Ok(rate)) => Some((rate, e.ledger().timestamp())),
        _ => None,
    }
}
fn value(shares: u128, rate: u128, escrow0: u128, escrow1: u128, p: u128) -> u128 {
    shares * rate / SCALE_1E6 + escrow1 + escrow0 * p / PRICE_SCALE
}
fn debt(e: &Env, c: &LevConfigV1, pid: u64) -> u128 {
    MarginMarketClient::new(e, &c.market).get_margin_borrow_balance(&pid)
}
/// Sleeve CF as the liquidation threshold. 0 is legal (group emergency disable): exits treat it as
/// HF = 0; admission (`cf`) refuses it.
fn cf_raw(e: &Env, c: &LevConfigV1) -> u128 {
    let cf = GroupControllerClient::new(e, &c.controller).get_market_cf(&c.receipt);
    need(e, cf < SCALE_1E6, LevError::NoCollateralFactor);
    cf
}
fn cf(e: &Env, c: &LevConfigV1) -> u128 {
    let cf = cf_raw(e, c);
    need(e, cf > 0, LevError::NoCollateralFactor);
    cf
}
/// Open/add admission: debt <= CF * (1 - b) * value and debt <= owner's max LTV * value.
fn check_ltv(e: &Env, c: &LevConfigV1, r: &LevRiskV1, d: u128, v: u128, owner_max_bps: u32) {
    let x_open = cf(e, c) * (BPS - r.open_buffer_bps as u128) / BPS;
    need(e, d * SCALE_1E6 <= x_open * v, LevError::LtvExceeded);
    need(e, d * BPS <= owner_max_bps as u128 * v, LevError::LtvExceeded);
}
fn equity_tp_ok(e: &Env, r: &LevRiskV1, exit: &ExitPolicyV1, equity: u128, p: u128) {
    if let Some(s) = exit.stop_hf_bps {
        need(e, s as u128 > BPS, LevError::InvalidTerms);
    }
    let gap = BPS + r.tp_min_gap_bps as u128;
    match &exit.take_profit {
        TakeProfitV1::EquityAtLeast(t) => need(e, *t * BPS >= equity * gap && *t > equity, LevError::TakeProfitTooClose),
        TakeProfitV1::PriceAtLeast(t) => need(e, *t * BPS >= p * gap, LevError::TakeProfitTooClose),
        TakeProfitV1::None => {}
    }
}
fn validate_open(e: &Env, r: &LevRiskV1, t: &OpenLevV1) {
    let now = e.ledger().timestamp();
    let horizon = core::cmp::min(r.max_deadline_secs, RECEIPT_MAX_DEADLINE);
    need(e, r.new_risk_enabled, LevError::NewRiskDisabled);
    need(e, t.deadline > now && t.deadline - now <= horizon, LevError::InvalidTerms);
    need(e, t.equity0 > 0 && t.max_ltv_bps as u128 <= BPS && t.swap_dir <= 1 && t.min_shares > 0, LevError::InvalidTerms);
}

// ------------------------------------------------------------------------------------ settlement
/// Settlement amounts at the current debt: (debt, exit incentive, close fee).
fn settle_terms(e: &Env, c: &LevConfigV1, r: &LevRiskV1, pid: u64, p: &PositionV1) -> (u128, u128, u128) {
    let d = debt(e, c, pid);
    let paid_exit = matches!(p.reason, REASON_TAKE_PROFIT | REASON_STOP | REASON_HEALTH);
    let inc = if paid_exit { d * r.exit_incentive_bps as u128 / BPS } else { 0 };
    let fee = if p.reason == REASON_REFUND { 0 } else { d * r.close_fee_bps as u128 / BPS };
    (d, inc, fee)
}
/// Separate light stage: swap escrowed PYUSD for the USDC deficit only, with an oracle-bounded
/// minimum (pool spot never sets the price) and a small interest buffer for the next ledgers.
fn swap_deficit_inner(e: &Env, c: &LevConfigV1, r: &LevRiskV1, pid: u64, p: &mut PositionV1) {
    let (d, inc, fee) = settle_terms(e, c, r, pid, p);
    let want = d + d * SETTLE_DEBT_BUFFER_BPS / BPS + inc + fee;
    need(e, p.escrow1 < want && p.escrow0 > 0, LevError::NoDeficit);
    let (price, _) = oracle_price(e, c, r, false);
    let keep = BPS - r.max_slippage_bps as u128;
    let deficit = want - p.escrow1;
    let mut amount_in = (deficit * PRICE_SCALE * BPS + price * keep - 1) / (price * keep);
    if amount_in > p.escrow0 {
        amount_in = p.escrow0;
    }
    let min_out = amount_in * price / PRICE_SCALE * keep / BPS;
    auth_transfers(e, &[(&c.token0, &c.swap, amount_in)]);
    let me = e.current_contract_address();
    let out = SwapAdapterClient::new(e, &c.swap).swap_pool(&me, &c.pool, &0, &1, &amount_in, &min_out);
    p.escrow0 -= amount_in;
    p.escrow1 += out;
    event(e, "lev_swap", pid, amount_in, out);
}
/// Proceeds-bound settlement (no swap): repay the lender first from the USDC leg, then the close
/// fee, then the exit incentive, then the residual in kind to the fixed owner. If USDC cannot
/// cover the debt while PYUSD remains, it refuses (`SwapRequired`) instead of forgiving debt.
fn settle_inner(e: &Env, c: &LevConfigV1, r: &LevRiskV1, pid: u64, p: &mut PositionV1, keeper: &Address) {
    let me = e.current_contract_address();
    let (d, inc, fee) = settle_terms(e, c, r, pid, p);
    need(e, p.escrow1 >= d || p.escrow0 == 0, LevError::SwapRequired);
    let repay = core::cmp::min(p.escrow1, d);
    if repay > 0 {
        auth_transfers(e, &[(&c.token1, &c.market, repay)]);
        MarginMarketClient::new(e, &c.market).repay_for_margin(&pid, &me, &repay);
        p.escrow1 -= repay;
    }
    if repay < d {
        p.shortfall = d - repay;
        p.phase = LevPhaseV1::Defaulted;
        event(e, "lev_default", pid, p.shortfall, p.escrow0);
        return;
    }
    let fee_paid = core::cmp::min(fee, p.escrow1);
    pay(e, &c.token1, &c.treasury, fee_paid);
    p.escrow1 -= fee_paid;
    let inc_paid = core::cmp::min(inc, p.escrow1);
    pay(e, &c.token1, keeper, inc_paid);
    p.escrow1 -= inc_paid;
    pay(e, &c.token0, &p.owner, p.escrow0);
    pay(e, &c.token1, &p.owner, p.escrow1);
    event(e, "lev_settled", pid, p.escrow0, p.escrow1);
    p.escrow0 = 0;
    p.escrow1 = 0;
    p.phase = if p.reason == REASON_REFUND { LevPhaseV1::Refunded } else { LevPhaseV1::Closed };
}
/// Add-path refund with existing shares: repay what the returned USDC covers, keep the rest as
/// escrowed collateral of the (again) active position.
fn absorb_escrow(e: &Env, c: &LevConfigV1, pid: u64, p: &mut PositionV1) {
    let d = debt(e, c, pid);
    let repay = core::cmp::min(p.escrow1, d);
    if repay > 0 {
        auth_transfers(e, &[(&c.token1, &c.market, repay)]);
        MarginMarketClient::new(e, &c.market).repay_for_margin(&pid, &e.current_contract_address(), &repay);
        p.escrow1 -= repay;
    }
    p.phase = LevPhaseV1::Active;
}
/// Apply a finished/cancelled open command.
fn sync_inner(e: &Env, c: &LevConfigV1, pid: u64, p: &mut PositionV1) {
    let rc = GroupReceiptClient::new(e, &c.receipt);
    let id = p.cmd.unwrap();
    let job = rc.stage1_job(&id).unwrap_or_else(|| panic_with_error!(e, LevError::CommandNotSettled));
    let me = e.current_contract_address();
    match (job.phase, job.result) {
        (PhaseV1::Invested, ResultV1::Invested(r)) => {
            rc.assign_beneficiary(&me, &p.owner, &job.shares, &true);
            p.shares += job.shares;
            pay(e, &c.token0, &p.owner, r.refund0);
            p.phase = LevPhaseV1::Active;
        }
        (PhaseV1::Cancelled, ResultV1::Released(r)) if r.shares > 0 => {
            rc.assign_beneficiary(&me, &p.owner, &r.shares, &true);
            p.shares += r.shares;
            pay(e, &c.token0, &p.owner, r.amounts.token0);
            p.phase = LevPhaseV1::Active;
        }
        (PhaseV1::Cancelled, ResultV1::Released(r)) => {
            p.escrow0 += r.amounts.token0;
            p.escrow1 += r.amounts.token1;
            if p.shares == 0 {
                p.reason = REASON_REFUND;
                p.phase = LevPhaseV1::Settling;
            } else {
                absorb_escrow(e, c, pid, p);
            }
        }
        (PhaseV1::Cancelled, _) => {
            p.reason = REASON_REFUND;
            p.phase = if p.shares == 0 { LevPhaseV1::Settling } else { LevPhaseV1::Active };
        }
        _ => panic_with_error!(e, LevError::CommandNotSettled),
    }
    p.cmd = None;
    p.pending = PendingV1::None;
    p.reserve0 = 0;
    event(e, "lev_synced", pid, p.shares, p.escrow1);
}
fn begin_close(e: &Env, c: &LevConfigV1, r: &LevRiskV1, pid: u64, p: &mut PositionV1, reason: u32) {
    p.reason = reason;
    p.armed = ArmV1::Off;
    if p.shares == 0 {
        p.phase = LevPhaseV1::Settling;
        return;
    }
    let rc = GroupReceiptClient::new(e, &c.receipt);
    let horizon = core::cmp::min(r.max_deadline_secs, RECEIPT_MAX_DEADLINE);
    let terms = CloseTermsV1 {
        mode: CloseModeV1::InKind,
        min_amounts: AmountsV1 { token0: 0, token1: 0 },
        min_underlying: 0,
        deadline: e.ledger().timestamp() + horizon,
        config_revision: rc.stage1_revision(),
    };
    let created = rc.prepare_close(&e.current_contract_address(), &p.shares, &terms);
    p.cmd = Some(created.id);
    p.phase = LevPhaseV1::Closing;
    event(e, "lev_exit", pid, reason as u128, p.shares);
}
fn tp_condition(e: &Env, c: &LevConfigV1, r: &LevRiskV1, pid: u64, p: &PositionV1) -> Option<u64> {
    match p.exit.take_profit.clone() {
        TakeProfitV1::EquityAtLeast(t) => {
            let (rate, ts) = nav_rate(e, c, r)?;
            let price = if p.escrow0 > 0 { oracle_price(e, c, r, false).0 } else { 0 };
            let v = value(p.shares, rate, p.escrow0, p.escrow1, price);
            let d = debt(e, c, pid);
            if v >= d && v - d >= t { Some(ts) } else { None }
        }
        TakeProfitV1::PriceAtLeast(t) => {
            let (price, ts) = oracle_price(e, c, r, false);
            if price >= t { Some(ts) } else { None }
        }
        TakeProfitV1::None => panic_with_error!(e, LevError::NoTakeProfit),
    }
}

fn check_risk(e: &Env, r: &LevRiskV1) {
    let ok = r.open_buffer_bps < 10_000
        && r.max_slippage_bps <= 1_000
        && r.open_fee_bps <= 500
        && r.close_fee_bps <= 500
        && r.exit_incentive_bps <= 1_000
        && r.max_depeg_bps <= 1_000
        && r.max_ckpt_age_secs > 0
        && r.max_oracle_age_secs > 0
        && r.max_deadline_secs > 0
        && r.tp_confirm_window_secs > r.tp_confirm_delay_secs
        && r.tp_confirm_window_secs > ORACLE_RESOLUTION_SECS;
    need(e, ok, LevError::InvalidRisk);
}


#[contractimpl]
impl LeveragedCl {
    // -------------------------------------------------------------------------------- admin
    pub fn initialize(e: Env, cfg: LevConfigV1, risk: LevRiskV1) {
        need(&e, !e.storage().instance().has(&LevKey::LevV1Cfg), LevError::AlreadyInitialized);
        cfg.admin.require_auth();
        check_risk(&e, &risk);
        e.storage().instance().set(&LevKey::LevV1Cfg, &cfg);
        e.storage().instance().set(&LevKey::LevV1Risk, &LevRiskV1 { revision: 0, ..risk });
        e.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
    }
    pub fn propose_risk_v1(e: Env, risk: LevRiskV1) {
        let c = cfg(&e);
        c.admin.require_auth();
        check_risk(&e, &risk);
        let eta = e.ledger().timestamp() + RISK_TIMELOCK_SECS;
        e.storage().instance().set(&LevKey::LevV1PendingRisk, &PendingRiskV1 { risk, eta });
    }
    pub fn execute_risk_v1(e: Env) {
        let c = cfg(&e);
        c.admin.require_auth();
        let pr: PendingRiskV1 = e
            .storage()
            .instance()
            .get(&LevKey::LevV1PendingRisk)
            .unwrap_or_else(|| panic_with_error!(&e, LevError::NoPendingRisk));
        need(&e, e.ledger().timestamp() >= pr.eta, LevError::Timelock);
        let revision = risk(&e).revision + 1;
        e.storage().instance().set(&LevKey::LevV1Risk, &LevRiskV1 { revision, ..pr.risk });
        e.storage().instance().remove(&LevKey::LevV1PendingRisk);
    }
    /// Immediate guardian-style stop of new risk (opens/adds); exits, repayment, settlement unaffected.
    pub fn pause_new_risk_v1(e: Env) {
        let c = cfg(&e);
        c.admin.require_auth();
        let r = risk(&e);
        e.storage().instance().set(&LevKey::LevV1Risk, &LevRiskV1 { new_risk_enabled: false, ..r });
    }
    // -------------------------------------------------------------------------------- owner
    pub fn open_v1(e: Env, owner: Address, t: OpenLevV1) -> u64 {
        owner.require_auth();
        let (c, r) = (cfg(&e), risk(&e));
        validate_open(&e, &r, &t);
        let me = e.current_contract_address();
        if t.borrow1 > 0 {
            need(&e, !GroupControllerClient::new(&e, &c.controller).is_borrow_paused(&c.market), LevError::BorrowPaused);
        }
        let (price, _) = oracle_price(&e, &c, &r, true);
        let pid: u64 = e.storage().instance().get(&LevKey::LevV1Next).unwrap_or(0);
        e.storage().instance().set(&LevKey::LevV1Next, &(pid + 1));
        token::Client::new(&e, &c.token0).transfer(&owner, &me, &((t.equity0 + t.reserve0) as i128));
        if t.borrow1 > 0 {
            MarginMarketClient::new(&e, &c.market).borrow_for_margin_to_controller(&pid, &t.borrow1);
        }
        let fee = t.borrow1 * r.open_fee_bps as u128 / BPS;
        pay(&e, &c.token1, &c.treasury, fee);
        let (escrow0, escrow1) = (t.equity0, t.borrow1 - fee);
        let v = value(0, 0, escrow0, escrow1, price);
        check_ltv(&e, &c, &r, t.borrow1, v, t.max_ltv_bps);
        let equity = v.saturating_sub(t.borrow1);
        equity_tp_ok(&e, &r, &t.exit, equity, price);
        let p = PositionV1 {
            owner,
            phase: LevPhaseV1::PendingFund,
            cmd: None,
            shares: 0,
            escrow0,
            escrow1,
            reserve0: t.reserve0,
            exit: t.exit.clone(),
            pending: PendingV1::Open(t.clone()),
            armed: ArmV1::Off,
            reason: REASON_NONE,
            equity_in: equity,
            shortfall: 0,
        };
        save(&e, pid, &p);
        event(&e, "lev_open", pid, t.equity0, t.borrow1);
        pid
    }
    /// Repeat round on an active position (more equity and/or debt), re-proved on a fresh NAV.
    pub fn add_v1(e: Env, owner: Address, pid: u64, t: OpenLevV1) {
        owner.require_auth();
        let (c, r) = (cfg(&e), risk(&e));
        let mut p = load(&e, pid);
        need(&e, p.owner == owner, LevError::NotOwner);
        need(&e, p.phase == LevPhaseV1::Active, LevError::BadPhase);
        validate_open(&e, &r, &t);
        if t.borrow1 > 0 {
            need(&e, !GroupControllerClient::new(&e, &c.controller).is_borrow_paused(&c.market), LevError::BorrowPaused);
        }
        let (price, _) = oracle_price(&e, &c, &r, true);
        let (rate, _) = nav_rate(&e, &c, &r).unwrap_or_else(|| panic_with_error!(&e, LevError::CheckpointStale));
        let me = e.current_contract_address();
        token::Client::new(&e, &c.token0).transfer(&owner, &me, &((t.equity0 + t.reserve0) as i128));
        if t.borrow1 > 0 {
            MarginMarketClient::new(&e, &c.market).borrow_for_margin_to_controller(&pid, &t.borrow1);
        }
        let fee = t.borrow1 * r.open_fee_bps as u128 / BPS;
        pay(&e, &c.token1, &c.treasury, fee);
        p.escrow0 += t.equity0;
        p.escrow1 += t.borrow1 - fee;
        let d = debt(&e, &c, pid);
        let v = value(p.shares, rate, p.escrow0, p.escrow1, price);
        check_ltv(&e, &c, &r, d, v, t.max_ltv_bps);
        p.reserve0 = t.reserve0;
        p.pending = PendingV1::Open(t.clone());
        p.phase = LevPhaseV1::PendingFund;
        p.armed = ArmV1::Off;
        save(&e, pid, &p);
        event(&e, "lev_add", pid, t.equity0, t.borrow1);
    }
    pub fn set_exit_v1(e: Env, owner: Address, pid: u64, exit: ExitPolicyV1) {
        owner.require_auth();
        let (c, r) = (cfg(&e), risk(&e));
        let mut p = load(&e, pid);
        need(&e, p.owner == owner, LevError::NotOwner);
        need(&e, matches!(p.phase, LevPhaseV1::PendingFund | LevPhaseV1::Opening | LevPhaseV1::Active), LevError::BadPhase);
        let (price, _) = oracle_price(&e, &c, &r, false);
        let equity = if p.phase == LevPhaseV1::Active && p.shares > 0 && matches!(exit.take_profit, TakeProfitV1::EquityAtLeast(_)) {
            let (rate, _) = nav_rate(&e, &c, &r).unwrap_or_else(|| panic_with_error!(&e, LevError::CheckpointStale));
            value(p.shares, rate, p.escrow0, p.escrow1, price).saturating_sub(debt(&e, &c, pid))
        } else {
            p.equity_in
        };
        equity_tp_ok(&e, &r, &exit, equity, price);
        p.exit = exit;
        p.armed = ArmV1::Off;
        save(&e, pid, &p);
    }
    pub fn close_v1(e: Env, owner: Address, pid: u64) {
        owner.require_auth();
        let (c, r) = (cfg(&e), risk(&e));
        let mut p = load(&e, pid);
        need(&e, p.owner == owner, LevError::NotOwner);
        need(&e, p.phase == LevPhaseV1::Active, LevError::BadPhase);
        begin_close(&e, &c, &r, pid, &mut p, REASON_VOLUNTARY);
        save(&e, pid, &p);
    }
    /// Any payer may repay (top-up / deleverage); the payer authorizes the nested market transfer.
    pub fn repay_v1(e: Env, payer: Address, pid: u64, amount: u128) {
        payer.require_auth();
        let c = cfg(&e);
        let p = load(&e, pid);
        need(&e, !matches!(p.phase, LevPhaseV1::Closed | LevPhaseV1::Refunded), LevError::BadPhase);
        MarginMarketClient::new(&e, &c.market).repay_for_margin(&pid, &payer, &amount);
        event(&e, "lev_repay", pid, amount, 0);
    }

    // -------------------------------------------------------------------------------- keeper
    pub fn fund_v1(e: Env, pid: u64) -> u64 {
        let (c, r) = (cfg(&e), risk(&e));
        let mut p = load(&e, pid);
        need(&e, p.phase == LevPhaseV1::PendingFund, LevError::BadPhase);
        let t = pending(&e, &p);
        need(&e, e.ledger().timestamp() <= t.deadline, LevError::Expired);
        need(&e, r.new_risk_enabled, LevError::NewRiskDisabled);
        let rc = GroupReceiptClient::new(&e, &c.receipt);
        let me = e.current_contract_address();
        let terms = PairOpenTermsV1 {
            amount0: p.escrow0,
            amount1: p.escrow1,
            swap_dir: t.swap_dir,
            swap_in: t.swap_in,
            min_swap: t.min_swap,
            min_shares: t.min_shares,
            reserve0: p.reserve0,
            deadline: t.deadline,
            config_revision: rc.stage1_revision(),
        };
        let created = rc.prepare_open_pair(&me, &terms);
        auth_transfers(&e, &[(&c.token0, &c.receipt, p.escrow0 + p.reserve0), (&c.token1, &c.receipt, p.escrow1)]);
        rc.fund_open(&created.id);
        p.cmd = Some(created.id);
        p.escrow0 = 0;
        p.escrow1 = 0;
        p.phase = LevPhaseV1::Opening;
        save(&e, pid, &p);
        event(&e, "lev_funded", pid, created.id as u128, 0);
        created.id
    }
    pub fn sync_open_v1(e: Env, pid: u64) {
        let c = cfg(&e);
        let mut p = load(&e, pid);
        need(&e, p.phase == LevPhaseV1::Opening, LevError::BadPhase);
        sync_inner(&e, &c, pid, &mut p);
        save(&e, pid, &p);
    }
    pub fn arm_exit_v1(e: Env, pid: u64) {
        let (c, r) = (cfg(&e), risk(&e));
        let mut p = load(&e, pid);
        need(&e, p.phase == LevPhaseV1::Active, LevError::BadPhase);
        let ts = tp_condition(&e, &c, &r, pid, &p).unwrap_or_else(|| panic_with_error!(&e, LevError::NoExitCondition));
        p.armed = ArmV1::On(ArmedV1 { ts: e.ledger().timestamp(), ckpt_ts: ts });
        save(&e, pid, &p);
        event(&e, "lev_armed", pid, ts as u128, 0);
    }
    /// Health (protocol), Stop (owner HF floor) or confirmed TakeProfit. Changes state only.
    pub fn trigger_exit_v1(e: Env, pid: u64) -> u32 {
        let (c, r) = (cfg(&e), risk(&e));
        let mut p = load(&e, pid);
        need(&e, p.phase == LevPhaseV1::Active, LevError::BadPhase);
        let d = debt(&e, &c, pid);
        let lt = cf_raw(&e, &c);
        // Health and Stop need a value: NAV if refreshable, else (Health only) the cause-gated fallback.
        let nav = nav_rate(&e, &c, &r);
        let rate = match nav {
            Some((rate, _)) => Some((rate, false)),
            None => fallback_rate(&e, &c, &r).map(|(rate, _)| (rate, true)),
        };
        let mut reason = REASON_NONE;
        if d > 0 && lt == 0 {
            // Group emergency_disable_collateral (CF -> 0): collateral is worth nothing to the
            // lender, so every indebted position may unwind (in kind, own assets; no seizure).
            reason = REASON_HEALTH;
        } else if let Some((rate, on_fallback)) = rate {
            let price = if p.escrow0 > 0 { core::cmp::min(oracle_price(&e, &c, &r, false).0, PRICE_SCALE) } else { 0 };
            let v = value(p.shares, rate, p.escrow0, p.escrow1, price);
            if d > 0 && v * lt < d * SCALE_1E6 {
                reason = REASON_HEALTH;
            } else if !on_fallback && d > 0 && p.exit.stop_hf_bps.map_or(false, |s| v * lt * BPS < d * SCALE_1E6 * s as u128) {
                reason = REASON_STOP;
            }
        }
        // Take-profit is evaluated independently of the Health/Stop value: a PriceAtLeast TP reads
        // CALI only and fires even while the NAV checkpoint is refused (pool out of band).
        if reason == REASON_NONE {
            if let ArmV1::On(a) = p.armed.clone() {
                let now = e.ledger().timestamp();
                need(&e, now >= a.ts + r.tp_confirm_delay_secs, LevError::ConfirmTooEarly);
                need(&e, now <= a.ts + r.tp_confirm_window_secs, LevError::ConfirmWindowPassed);
                let cts = tp_condition(&e, &c, &r, pid, &p).unwrap_or_else(|| panic_with_error!(&e, LevError::NoExitCondition));
                need(&e, cts > a.ckpt_ts, LevError::CheckpointNotNewer);
                reason = REASON_TAKE_PROFIT;
            } else if rate.is_none() {
                panic_with_error!(&e, LevError::CheckpointStale);
            }
        }
        need(&e, reason != REASON_NONE, LevError::NoExitCondition);
        begin_close(&e, &c, &r, pid, &mut p, reason);
        save(&e, pid, &p);
        reason
    }
    /// C2 follow-up: read the paid in-kind exit, release the beneficiary label, -> Settling.
    pub fn collect_v1(e: Env, pid: u64) {
        let c = cfg(&e);
        let mut p = load(&e, pid);
        need(&e, p.phase == LevPhaseV1::Closing, LevError::BadPhase);
        let rc = GroupReceiptClient::new(&e, &c.receipt);
        let job = rc.stage1_job(&p.cmd.unwrap()).unwrap();
        let exit = match (job.phase, job.result) {
            (PhaseV1::Paid, ResultV1::Exit(x)) => x,
            _ => panic_with_error!(&e, LevError::CommandNotSettled),
        };
        rc.assign_beneficiary(&e.current_contract_address(), &p.owner, &p.shares, &false);
        p.shares = 0;
        p.escrow0 += exit.amounts.token0;
        p.escrow1 += exit.amounts.token1;
        p.cmd = None;
        p.phase = LevPhaseV1::Settling;
        save(&e, pid, &p);
        event(&e, "lev_collect", pid, exit.amounts.token0, exit.amounts.token1);
    }
    pub fn swap_deficit_v1(e: Env, pid: u64) {
        let (c, r) = (cfg(&e), risk(&e));
        let mut p = load(&e, pid);
        need(&e, p.phase == LevPhaseV1::Settling, LevError::BadPhase);
        swap_deficit_inner(&e, &c, &r, pid, &mut p);
        save(&e, pid, &p);
    }
    /// The caller signs (it receives the exit incentive on Health/Stop/TakeProfit exits).
    pub fn settle_v1(e: Env, keeper: Address, pid: u64) {
        keeper.require_auth();
        let (c, r) = (cfg(&e), risk(&e));
        let mut p = load(&e, pid);
        need(&e, p.phase == LevPhaseV1::Settling, LevError::BadPhase);
        settle_inner(&e, &c, &r, pid, &mut p, &keeper);
        save(&e, pid, &p);
    }
    /// Owner before the deadline; anyone strictly after. Health exits only after expiry.
    pub fn cancel_v1(e: Env, pid: u64) {
        let c = cfg(&e);
        let mut p = load(&e, pid);
        let now = e.ledger().timestamp();
        match p.phase {
            LevPhaseV1::PendingFund => {
                if now <= pending(&e, &p).deadline {
                    p.owner.require_auth();
                }
                // Return the unused reserve; the escrow settles against the debt.
                let me_reserve = p.reserve0;
                p.reserve0 = 0;
                pay(&e, &c.token0, &p.owner, me_reserve);
                p.pending = PendingV1::None;
                if p.shares == 0 {
                    p.reason = REASON_REFUND;
                    p.phase = LevPhaseV1::Settling;
                } else {
                    absorb_escrow(&e, &c, pid, &mut p);
                }
            }
            LevPhaseV1::Opening => {
                let rc = GroupReceiptClient::new(&e, &c.receipt);
                let job = rc.stage1_job(&p.cmd.unwrap()).unwrap();
                if !matches!(job.phase, PhaseV1::Invested | PhaseV1::Cancelled) {
                    if now <= pending(&e, &p).deadline {
                        p.owner.require_auth();
                    }
                    rc.cancel(&job.id);
                }
                sync_inner(&e, &c, pid, &mut p);
            }
            LevPhaseV1::Closing => {
                let rc = GroupReceiptClient::new(&e, &c.receipt);
                let job = rc.stage1_job(&p.cmd.unwrap()).unwrap();
                let deadline = match &job.terms {
                    cl_lev_interface::stage1::CommandTermsV1::Close(t) => t.deadline,
                    _ => 0,
                };
                if now <= deadline {
                    need(&e, p.reason != REASON_HEALTH, LevError::HealthExitNotCancellable);
                    p.owner.require_auth();
                }
                rc.cancel(&job.id);
                p.cmd = None;
                p.reason = REASON_NONE;
                p.phase = LevPhaseV1::Active;
            }
            _ => panic_with_error!(&e, LevError::BadPhase),
        }
        save(&e, pid, &p);
        event(&e, "lev_cancel", pid, 0, 0);
    }

    // -------------------------------------------------------------------------------- views
    pub fn position_v1(e: Env, pid: u64) -> Option<PositionV1> {
        e.storage().persistent().get(&LevKey::LevV1Pos(pid))
    }
    pub fn config_v1(e: Env) -> LevConfigV1 {
        cfg(&e)
    }
    pub fn risk_v1(e: Env) -> LevRiskV1 {
        risk(&e)
    }
    /// Read-only health on the stored checkpoints (no refresh): NAV if fresh, else fallback if fresh.
    pub fn health_v1(e: Env, pid: u64) -> HealthV1 {
        let (c, r) = (cfg(&e), risk(&e));
        let p = load(&e, pid);
        let rc = GroupReceiptClient::new(&e, &c.receipt);
        let (rate, fb) = match rc.nav_checkpoint().filter(|k| fresh(&e, &r, k.ts)) {
            Some(k) => (k.rate, false),
            None => match rc.fallback_checkpoint().filter(|k| fresh(&e, &r, k.ts)) {
                Some(k) => (k.rate, true),
                None => panic_with_error!(&e, LevError::CheckpointStale),
            },
        };
        let price = if p.escrow0 > 0 { core::cmp::min(oracle_price(&e, &c, &r, false).0, PRICE_SCALE) } else { 0 };
        let v = value(p.shares, rate, p.escrow0, p.escrow1, price);
        let d = debt(&e, &c, pid);
        let hf = if d == 0 { u128::MAX } else { v * cf_raw(&e, &c) / d };
        HealthV1 { value: v, debt: d, hf, fallback: fb }
    }
}
