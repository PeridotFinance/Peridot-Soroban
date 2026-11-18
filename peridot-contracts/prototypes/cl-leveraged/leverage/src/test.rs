//! Unit tests against native mocks of the group receipt/vault, controller, USDC market, oracle and
//! swap adapter. Owner calls use SPECIFIC mocked authorizations (`mock_auths`), never
//! `mock_all_auths`, so `cl-leveraged`'s own contract authorizations (direct invoker and
//! `authorize_as_current_contract` for nested transfers) are enforced by the host for real.
//! All risk values are TEST FIXTURES, not proposals.
extern crate std;
use crate::*;
use cl_lev_mock_controller::{MockController, MockControllerClient};
use cl_lev_mock_market::{MockMarket, MockMarketClient};
use cl_lev_mock_oracle::{MockOracle, MockOracleClient};
use cl_lev_mock_receipt::{MockReceipt, MockReceiptClient, FAIL_CONVERT, FAIL_FUND, FAIL_INVEST};
use cl_lev_mock_swap::{MockSwap, MockSwapClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    token::{StellarAssetClient, TokenClient},
    Address, Env, IntoVal, Val, Vec as SVec,
};

const U: u128 = 10_000_000; // 1 token, 7 decimals
const P1: u128 = PRICE_SCALE; // 1.0 PYUSD/USDC

struct F {
    e: Env,
    lev: LeveragedClClient<'static>,
    rc: MockReceiptClient<'static>,
    ctrl: MockControllerClient<'static>,
    mkt: MockMarketClient<'static>,
    orc: MockOracleClient<'static>,
    swp: MockSwapClient<'static>,
    t0: Address,
    t1: Address,
    owner: Address,
    keeper: Address,
    treasury: Address,
    admin: Address,
}

fn fixture_risk() -> LevRiskV1 {
    LevRiskV1 {
        revision: 0,
        new_risk_enabled: true,
        open_buffer_bps: 200,
        max_ckpt_age_secs: 60,
        max_oracle_age_secs: 600,
        max_depeg_bps: 100,
        max_slippage_bps: 100,
        open_fee_bps: 10,
        close_fee_bps: 10,
        exit_incentive_bps: 50,
        tp_min_gap_bps: 50,
        tp_confirm_delay_secs: 30,
        tp_confirm_window_secs: 600,
        max_deadline_secs: 3_600,
    }
}

fn setup() -> F {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    e.mock_all_auths(); // setup only; cleared below
    let admin = Address::generate(&e);
    let owner = Address::generate(&e);
    let keeper = Address::generate(&e);
    let treasury = Address::generate(&e);
    let t0 = e.register_stellar_asset_contract_v2(admin.clone()).address();
    let t1 = e.register_stellar_asset_contract_v2(admin.clone()).address();
    let rc_id = e.register(MockReceipt, ());
    let ctrl_id = e.register(MockController, ());
    let mkt_id = e.register(MockMarket, ());
    let orc_id = e.register(MockOracle, ());
    let swp_id = e.register(MockSwap, ());
    let lev_id = e.register(LeveragedCl, ());
    let pool = Address::generate(&e);
    let rc = MockReceiptClient::new(&e, &rc_id);
    let ctrl = MockControllerClient::new(&e, &ctrl_id);
    let mkt = MockMarketClient::new(&e, &mkt_id);
    let orc = MockOracleClient::new(&e, &orc_id);
    let swp = MockSwapClient::new(&e, &swp_id);
    rc.mock_init(&t0, &t1, &orc_id);
    ctrl.mock_set_cf(&rc_id, &500_000); // TEST CF 50%
    // ~8% APR expressed per second at 1e18: 0.08 / 31_536_000 * 1e18.
    mkt.mock_init(&t1, &ctrl_id, &lev_id, &2_536_783_358);
    swp.mock_init(&t0, &t1, &P1);
    orc.mock_set(&Asset::Stellar(t0.clone()), &(P1 as i128), &1_000);
    for (t, to, n) in [(&t0, &rc_id, 100_000 * U), (&t1, &rc_id, 100_000 * U), (&t0, &swp_id, 100_000 * U), (&t1, &swp_id, 100_000 * U), (&t1, &mkt_id, 100_000 * U), (&t0, &owner, 10_000 * U), (&t1, &owner, 10_000 * U)] {
        StellarAssetClient::new(&e, t).mint(to, &(n as i128));
    }
    let lev = LeveragedClClient::new(&e, &lev_id);
    let cfg = LevConfigV1 {
        admin: admin.clone(),
        receipt: rc_id,
        controller: ctrl_id,
        market: mkt_id,
        swap: swp_id,
        pool,
        oracle: orc_id,
        token0: t0.clone(),
        token1: t1.clone(),
        treasury: treasury.clone(),
    };
    lev.initialize(&cfg, &fixture_risk());
    e.set_auths(&[]);
    F { e, lev, rc, ctrl, mkt, orc, swp, t0, t1, owner, keeper, treasury, admin }
}

impl F {
    fn now(&self) -> u64 {
        self.e.ledger().timestamp()
    }
    /// Advance time and keep the oracle fresh at `price`.
    fn tick(&self, dt: u64, price: u128) {
        self.e.ledger().with_mut(|l| {
            l.timestamp += dt;
            l.sequence_number += (dt / 5) as u32 + 1;
        });
        self.orc.mock_set(&Asset::Stellar(self.t0.clone()), &(price as i128), &self.now());
    }
    fn bal(&self, t: &Address, a: &Address) -> u128 {
        TokenClient::new(&self.e, t).balance(a) as u128
    }
    fn terms(&self, equity: u128, borrow: u128) -> OpenLevV1 {
        // Pair funding at p = 1: swap half of the excess leg.
        let (dir, swap_in) = if equity >= borrow { (0, (equity - borrow) / 2) } else { (1, (borrow - equity) / 2) };
        OpenLevV1 {
            equity0: equity,
            borrow1: borrow,
            swap_dir: dir,
            swap_in,
            min_swap: swap_in * 99 / 100,
            min_shares: 1,
            reserve0: U / 10,
            deadline: self.now() + 1_800,
            max_ltv_bps: 9_000,
            exit: ExitPolicyV1 { take_profit: TakeProfitV1::None, stop_hf_bps: None },
        }
    }
    /// Owner-signed open with the exact nested PYUSD transfer in the authorization tree.
    fn open(&self, t: &OpenLevV1) -> u64 {
        let args: SVec<Val> = (self.owner.clone(), t.clone()).into_val(&self.e);
        let xfer: SVec<Val> = (self.owner.clone(), self.lev.address.clone(), (t.equity0 + t.reserve0) as i128).into_val(&self.e);
        self.e.mock_auths(&[MockAuth {
            address: &self.owner,
            invoke: &MockAuthInvoke {
                contract: &self.lev.address,
                fn_name: "open_v1",
                args,
                sub_invokes: &[MockAuthInvoke { contract: &self.t0, fn_name: "transfer", args: xfer, sub_invokes: &[] }],
            },
        }]);
        self.lev.open_v1(&self.owner, t)
    }
    fn as_owner(&self, f: &str, args: SVec<Val>) {
        self.e.mock_auths(&[MockAuth {
            address: &self.owner,
            invoke: &MockAuthInvoke { contract: &self.lev.address, fn_name: f, args, sub_invokes: &[] },
        }]);
    }
    fn as_keeper_settle(&self, pid: u64) {
        let args: SVec<Val> = (self.keeper.clone(), pid).into_val(&self.e);
        self.e.mock_auths(&[MockAuth {
            address: &self.keeper,
            invoke: &MockAuthInvoke { contract: &self.lev.address, fn_name: "settle_v1", args, sub_invokes: &[] },
        }]);
        self.lev.settle_v1(&self.keeper, &pid);
    }
    /// Full open journey; returns pid.
    fn opened(&self, equity: u128, borrow: u128) -> u64 {
        let pid = self.open(&self.terms(equity, borrow));
        let cmd = self.lev.fund_v1(&pid);
        self.tick(6, P1);
        self.rc.convert_open(&cmd);
        self.rc.mint_cash(&cmd);
        self.rc.invest_cash(&cmd);
        self.lev.sync_open_v1(&pid);
        pid
    }
    fn pos(&self, pid: u64) -> PositionV1 {
        self.lev.position_v1(&pid).unwrap()
    }
}

fn lev_err<T: core::fmt::Debug>(r: Result<T, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>, want: LevError) {
    match r {
        Err(Ok(err)) => assert_eq!(err, soroban_sdk::Error::from_contract_error(want as u32), "wrong error"),
        other => panic!("expected {want:?}, got {other:?}"),
    }
}

// ------------------------------------------------------------------------------------ open
#[test]
fn open_reaches_target_leverage_in_one_pass() {
    let f = setup();
    let t = f.terms(1_000 * U, 900 * U);
    let pid = f.open(&t);
    let p = f.pos(pid);
    assert_eq!(p.phase, LevPhaseV1::PendingFund);
    assert_eq!(f.mkt.get_margin_borrow_balance(&pid), 900 * U);
    assert_eq!(f.bal(&f.t1, &f.treasury), 900 * U * 10 / 10_000, "open fee on borrowed notional");
    assert_eq!(p.escrow1, 900 * U - 9 * U / 10);
    let cmd = f.lev.fund_v1(&pid);
    assert_eq!(f.pos(pid).phase, LevPhaseV1::Opening);
    assert_eq!(f.bal(&f.t0, &f.lev.address), 0, "escrow moved into the command");
    f.tick(6, P1);
    f.rc.convert_open(&cmd);
    f.rc.mint_cash(&cmd);
    f.rc.invest_cash(&cmd);
    let before = f.bal(&f.t0, &f.owner);
    f.lev.sync_open_v1(&pid);
    let p = f.pos(pid);
    assert_eq!(p.phase, LevPhaseV1::Active);
    assert!(p.shares > 0);
    assert_eq!(f.rc.units(&f.lev.address), p.shares, "custodied by the margin escrow");
    assert_eq!(f.rc.beneficial(&f.owner), p.shares, "owner keeps AQUA beneficiary rights");
    assert_eq!(f.bal(&f.t0, &f.owner) - before, U / 10, "execution reserve refunded");
    f.rc.checkpoint_nav();
    let h = f.lev.health_v1(&pid);
    // value ~1899.1, debt ~900, CF 50% -> HF ~1.055
    assert!(h.hf > 1_000_000 && h.hf < 1_100_000, "hf {}", h.hf);
}

#[test]
fn open_rejects_above_max_leverage_and_owner_cap() {
    let f = setup();
    // x_open = 50% * (1 - 2%) = 49%: D <= 0.49 (E + D) -> D <= 960.78 for E = 1000.
    let t = f.terms(1_000 * U, 970 * U);
    let args: SVec<Val> = (f.owner.clone(), t.clone()).into_val(&f.e);
    let xfer: SVec<Val> = (f.owner.clone(), f.lev.address.clone(), (t.equity0 + t.reserve0) as i128).into_val(&f.e);
    f.e.mock_auths(&[MockAuth { address: &f.owner, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: "open_v1", args, sub_invokes: &[MockAuthInvoke { contract: &f.t0, fn_name: "transfer", args: xfer, sub_invokes: &[] }] } }]);
    lev_err(f.lev.try_open_v1(&f.owner, &t), LevError::LtvExceeded);
    let mut t2 = f.terms(1_000 * U, 900 * U);
    t2.max_ltv_bps = 4_000;
    let args: SVec<Val> = (f.owner.clone(), t2.clone()).into_val(&f.e);
    let xfer: SVec<Val> = (f.owner.clone(), f.lev.address.clone(), (t2.equity0 + t2.reserve0) as i128).into_val(&f.e);
    f.e.mock_auths(&[MockAuth { address: &f.owner, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: "open_v1", args, sub_invokes: &[MockAuthInvoke { contract: &f.t0, fn_name: "transfer", args: xfer, sub_invokes: &[] }] } }]);
    lev_err(f.lev.try_open_v1(&f.owner, &t2), LevError::LtvExceeded);
    assert_eq!(f.mkt.get_margin_borrow_balance(&0), 0, "rejected open leaves no debt");
}

#[test]
fn open_blocked_by_pause_depeg_and_disabled_risk() {
    let f = setup();
    f.ctrl.mock_set_paused(&f.mkt.address, &true);
    let t = f.terms(1_000 * U, 900 * U);
    let args: SVec<Val> = (f.owner.clone(), t.clone()).into_val(&f.e);
    f.e.mock_auths(&[MockAuth { address: &f.owner, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: "open_v1", args: args.clone(), sub_invokes: &[] } }]);
    lev_err(f.lev.try_open_v1(&f.owner, &t), LevError::BorrowPaused);
    f.ctrl.mock_set_paused(&f.mkt.address, &false);
    f.tick(1, P1 * 98 / 100); // 200 bps depeg > 100 bps guard
    let t = f.terms(1_000 * U, 900 * U);
    let args: SVec<Val> = (f.owner.clone(), t.clone()).into_val(&f.e);
    f.e.mock_auths(&[MockAuth { address: &f.owner, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: "open_v1", args, sub_invokes: &[] } }]);
    lev_err(f.lev.try_open_v1(&f.owner, &t), LevError::Depeg);
    f.tick(1, P1);
    f.e.mock_auths(&[MockAuth { address: &f.admin, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: "pause_new_risk_v1", args: SVec::new(&f.e), sub_invokes: &[] } }]);
    f.lev.pause_new_risk_v1();
    let t = f.terms(1_000 * U, 900 * U);
    let args: SVec<Val> = (f.owner.clone(), t.clone()).into_val(&f.e);
    f.e.mock_auths(&[MockAuth { address: &f.owner, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: "open_v1", args, sub_invokes: &[] } }]);
    lev_err(f.lev.try_open_v1(&f.owner, &t), LevError::NewRiskDisabled);
}

// ------------------------------------------------------------------------------------ take-profit
#[test]
fn open_take_profit_close() {
    let f = setup();
    let pid = f.opened(1_000 * U, 900 * U);
    f.rc.checkpoint_nav();
    let equity0 = f.lev.health_v1(&pid);
    let equity0 = equity0.value - equity0.debt;
    // TP inside the minimum gap is refused (pool-push granularity, spec §5).
    let close = ExitPolicyV1 { take_profit: (TakeProfitV1::EquityAtLeast(equity0 + equity0 / 1_000)), stop_hf_bps: None };
    f.as_owner("set_exit_v1", (f.owner.clone(), pid, close.clone()).into_val(&f.e));
    lev_err(f.lev.try_set_exit_v1(&f.owner, &pid, &close), LevError::TakeProfitTooClose);
    let target = equity0 * 102 / 100;
    let exit = ExitPolicyV1 { take_profit: (TakeProfitV1::EquityAtLeast(target)), stop_hf_bps: None };
    f.as_owner("set_exit_v1", (f.owner.clone(), pid, exit.clone()).into_val(&f.e));
    f.lev.set_exit_v1(&f.owner, &pid, &exit);
    // Not yet in profit: arming fails.
    lev_err(f.lev.try_arm_exit_v1(&pid), LevError::NoExitCondition);
    // Fees accrue to the vault: +2% backing -> equity +~3.8% at ~1.9x.
    f.rc.mock_loss(&-200);
    f.tick(61, P1); // stored checkpoint stale -> arm refreshes inline
    f.lev.arm_exit_v1(&pid);
    lev_err(f.lev.try_trigger_exit_v1(&pid), LevError::ConfirmTooEarly);
    f.tick(31, P1); // within 60 s: same checkpoint as arming
    lev_err(f.lev.try_trigger_exit_v1(&pid), LevError::CheckpointNotNewer);
    f.tick(31, P1); // stale again -> inline refresh gives a later checkpoint
    assert_eq!(f.lev.trigger_exit_v1(&pid), REASON_TAKE_PROFIT);
    let p = f.pos(pid);
    assert_eq!(p.phase, LevPhaseV1::Closing);
    assert_eq!(f.rc.locked(&f.lev.address), p.shares);
    // Keeper runs the unchanged receipt withdrawal; it pays the margin escrow, not the owner.
    let (o0, o1) = (f.bal(&f.t0, &f.owner), f.bal(&f.t1, &f.owner));
    f.rc.withdraw_close(&p.cmd.unwrap());
    assert_eq!((f.bal(&f.t0, &f.owner), f.bal(&f.t1, &f.owner)), (o0, o1));
    let debt = f.mkt.get_margin_borrow_balance(&pid);
    f.as_keeper_settle(pid);
    let p = f.pos(pid);
    assert_eq!(p.phase, LevPhaseV1::Closed);
    assert_eq!(f.mkt.get_margin_borrow_balance(&pid), 0);
    assert_eq!(f.bal(&f.t1, &f.keeper), debt * 50 / 10_000, "exit incentive");
    assert_eq!(f.rc.beneficial(&f.owner), 0);
    let got = (f.bal(&f.t0, &f.owner) - o0) + (f.bal(&f.t1, &f.owner) - o1);
    assert!(got >= target - target / 100, "owner receives ~TP equity in kind: {got} vs {target}");
    assert_eq!(f.bal(&f.t0, &f.lev.address) + f.bal(&f.t1, &f.lev.address), 0, "no residue in escrow");
    lev_err(f.lev.try_trigger_exit_v1(&pid), LevError::BadPhase);
}

#[test]
fn price_take_profit_uses_oracle_only() {
    let f = setup();
    let pid = f.opened(1_000 * U, 900 * U);
    let exit = ExitPolicyV1 { take_profit: (TakeProfitV1::PriceAtLeast(P1 * 1_006 / 1_000)), stop_hf_bps: None };
    f.as_owner("set_exit_v1", (f.owner.clone(), pid, exit.clone()).into_val(&f.e));
    f.lev.set_exit_v1(&f.owner, &pid, &exit);
    // A pool push moves the adapter price, not the oracle: no trigger.
    f.swp.mock_push(&500);
    lev_err(f.lev.try_arm_exit_v1(&pid), LevError::NoExitCondition);
    f.tick(10, P1 * 1_007 / 1_000);
    f.lev.arm_exit_v1(&pid);
    f.tick(40, P1 * 1_007 / 1_000);
    f.rc.checkpoint_nav();
    assert_eq!(f.lev.trigger_exit_v1(&pid), REASON_TAKE_PROFIT);
}

// ------------------------------------------------------------------------------------ health exit
#[test]
fn health_exit_unwinds_and_repays_with_deficit_swap() {
    let f = setup();
    let pid = f.opened(1_000 * U, 900 * U);
    f.tick(61, P1);
    lev_err(f.lev.try_trigger_exit_v1(&pid), LevError::NoExitCondition);
    f.rc.mock_loss(&600); // value ~1785 -> HF < 1 at CF 50%
    f.tick(61, P1);
    assert_eq!(f.lev.trigger_exit_v1(&pid), REASON_HEALTH);
    // The owner cannot cancel a health exit before its deadline.
    f.as_owner("cancel_v1", (pid,).into_val(&f.e));
    lev_err(f.lev.try_cancel_v1(&pid), LevError::HealthExitNotCancellable);
    f.rc.withdraw_close(&f.pos(pid).cmd.unwrap());
    let swap_before = f.bal(&f.t0, &f.swp.address);
    f.as_keeper_settle(pid);
    assert!(f.bal(&f.t0, &f.swp.address) > swap_before, "USDC leg short: deficit swapped from PYUSD");
    assert_eq!(f.pos(pid).phase, LevPhaseV1::Closed);
    assert_eq!(f.mkt.get_margin_borrow_balance(&pid), 0);
}

#[test]
fn health_exit_on_fallback_when_nav_unavailable() {
    let f = setup();
    let pid = f.opened(1_000 * U, 900 * U);
    f.rc.mock_loss(&400); // NAV value ~1823: healthy on NAV
    f.tick(61, P1);
    lev_err(f.lev.try_trigger_exit_v1(&pid), LevError::NoExitCondition);
    f.rc.mock_nav_fail(&true); // pool out of band: NAV refresh refused, fallback cause exists
    f.tick(61, P1);
    assert_eq!(f.lev.trigger_exit_v1(&pid), REASON_HEALTH, "fallback (~98% of NAV) is below the threshold");
    assert!(f.rc.fallback_checkpoint().is_some());
}

#[test]
fn stop_triggers_before_protocol_health() {
    let f = setup();
    let pid = f.opened(1_000 * U, 900 * U);
    let exit = ExitPolicyV1 { take_profit: TakeProfitV1::None, stop_hf_bps: Some(10_400) };
    f.as_owner("set_exit_v1", (f.owner.clone(), pid, exit.clone()).into_val(&f.e));
    f.lev.set_exit_v1(&f.owner, &pid, &exit);
    f.rc.mock_loss(&200); // HF ~1.034: above 1, below the 1.04 stop
    f.tick(61, P1);
    assert_eq!(f.lev.trigger_exit_v1(&pid), REASON_STOP);
    // Owner may cancel their own stop exit.
    f.as_owner("cancel_v1", (pid,).into_val(&f.e));
    f.lev.cancel_v1(&pid);
    assert_eq!(f.pos(pid).phase, LevPhaseV1::Active);
    assert_eq!(f.rc.locked(&f.lev.address), 0);
}

#[test]
fn bad_debt_is_recorded_not_hidden() {
    let f = setup();
    let pid = f.opened(1_000 * U, 900 * U);
    f.rc.mock_loss(&6_000);
    f.tick(61, P1);
    assert_eq!(f.lev.trigger_exit_v1(&pid), REASON_HEALTH);
    f.rc.withdraw_close(&f.pos(pid).cmd.unwrap());
    f.as_keeper_settle(pid);
    let p = f.pos(pid);
    assert_eq!(p.phase, LevPhaseV1::Defaulted);
    assert!(p.shortfall > 0);
    assert_eq!(f.mkt.get_margin_borrow_balance(&pid), p.shortfall);
    assert_eq!(f.bal(&f.t1, &f.keeper), 0, "lender before keeper");
}

// ------------------------------------------------------------------------------------ refunds
#[test]
fn refund_when_fund_rejected() {
    let f = setup();
    let o0 = f.bal(&f.t0, &f.owner);
    let pid = f.open(&f.terms(1_000 * U, 900 * U));
    f.rc.mock_fail(&FAIL_FUND);
    assert!(f.lev.try_fund_v1(&pid).is_err());
    assert_eq!(f.pos(pid).phase, LevPhaseV1::PendingFund, "failed fund rolls back");
    // A stranger cannot cancel before the deadline.
    assert!(f.lev.try_cancel_v1(&pid).is_err());
    f.as_owner("cancel_v1", (pid,).into_val(&f.e));
    f.lev.cancel_v1(&pid);
    let p = f.pos(pid);
    assert_eq!(p.phase, LevPhaseV1::Refunded);
    assert_eq!(f.mkt.get_margin_borrow_balance(&pid), 0);
    let lost = o0 - f.bal(&f.t0, &f.owner);
    // Lost = open fee (0.9 USDC) + accrued interest, settled from PYUSD via the deficit swap.
    assert!(lost < 2 * U, "refund within fee + slippage: lost {lost}");
}

#[test]
fn refund_on_failed_convert_after_expiry() {
    let f = setup();
    let pid = f.open(&f.terms(1_000 * U, 900 * U));
    let cmd = f.lev.fund_v1(&pid);
    f.rc.mock_fail(&FAIL_CONVERT);
    assert!(f.rc.try_convert_open(&cmd).is_err());
    assert!(f.lev.try_cancel_v1(&pid).is_err(), "before the deadline only the owner cancels");
    f.tick(1_801, P1);
    f.lev.cancel_v1(&pid); // anyone, strictly after the deadline
    let p = f.pos(pid);
    assert_eq!(p.phase, LevPhaseV1::Settling);
    assert_eq!(p.reason, REASON_REFUND);
    f.as_keeper_settle(pid);
    let p = f.pos(pid);
    assert_eq!(p.phase, LevPhaseV1::Refunded);
    assert_eq!(f.mkt.get_margin_borrow_balance(&pid), 0);
    assert_eq!(f.bal(&f.t1, &f.keeper), 0, "no exit incentive on refunds");
}

#[test]
fn failed_invest_after_mint_releases_shares_not_cash() {
    let f = setup();
    let pid = f.open(&f.terms(1_000 * U, 900 * U));
    let cmd = f.lev.fund_v1(&pid);
    f.rc.convert_open(&cmd);
    f.rc.mint_cash(&cmd);
    f.rc.mock_fail(&FAIL_INVEST);
    assert!(f.rc.try_invest_cash(&cmd).is_err());
    f.tick(1_801, P1);
    f.lev.cancel_v1(&pid);
    let p = f.pos(pid);
    assert_eq!(p.phase, LevPhaseV1::Active, "minted shares become the position");
    assert!(p.shares > 0 && p.escrow0 == 0 && p.escrow1 == 0, "no second cash refund");
    assert_eq!(f.rc.locked(&f.lev.address), 0);
}

#[test]
fn add_round_increases_shares_and_debt() {
    let f = setup();
    let pid = f.opened(1_000 * U, 500 * U);
    let s0 = f.pos(pid).shares;
    f.tick(61, P1);
    let t = f.terms(100 * U, 300 * U);
    let args: SVec<Val> = (f.owner.clone(), pid, t.clone()).into_val(&f.e);
    let xfer: SVec<Val> = (f.owner.clone(), f.lev.address.clone(), (t.equity0 + t.reserve0) as i128).into_val(&f.e);
    f.e.mock_auths(&[MockAuth { address: &f.owner, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: "add_v1", args, sub_invokes: &[MockAuthInvoke { contract: &f.t0, fn_name: "transfer", args: xfer, sub_invokes: &[] }] } }]);
    f.lev.add_v1(&f.owner, &pid, &t);
    let cmd = f.lev.fund_v1(&pid);
    f.rc.convert_open(&cmd);
    f.rc.mint_cash(&cmd);
    f.rc.invest_cash(&cmd);
    f.lev.sync_open_v1(&pid);
    assert!(f.pos(pid).shares > s0);
    assert!(f.mkt.get_margin_borrow_balance(&pid) >= 800 * U);
}

// ------------------------------------------------------------------------------------ auth
#[test]
fn auth_owner_entry_points_reject_others() {
    let f = setup();
    let t = f.terms(1_000 * U, 900 * U);
    assert!(f.lev.try_open_v1(&f.owner, &t).is_err(), "open needs the owner's signature");
    let pid = f.opened(1_000 * U, 900 * U);
    let attacker = Address::generate(&f.e);
    let exit = ExitPolicyV1 { take_profit: TakeProfitV1::None, stop_hf_bps: Some(20_000) };
    f.e.mock_auths(&[MockAuth { address: &attacker, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: "set_exit_v1", args: (attacker.clone(), pid, exit.clone()).into_val(&f.e), sub_invokes: &[] } }]);
    lev_err(f.lev.try_set_exit_v1(&attacker, &pid, &exit), LevError::NotOwner);
    f.e.mock_auths(&[MockAuth { address: &attacker, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: "close_v1", args: (attacker.clone(), pid).into_val(&f.e), sub_invokes: &[] } }]);
    lev_err(f.lev.try_close_v1(&attacker, &pid), LevError::NotOwner);
    assert!(f.lev.try_close_v1(&f.owner, &pid).is_err(), "owner close needs a signature");
    // Nobody can drive the receipt directly on the escrow's shares.
    let ct = CloseTermsV1 { mode: CloseModeV1::InKind, min_amounts: AmountsV1 { token0: 0, token1: 0 }, min_underlying: 0, deadline: f.now() + 100, config_revision: 0 };
    assert!(f.rc.try_prepare_close(&f.lev.address, &1, &ct).is_err());
    assert!(f.rc.try_assign_beneficiary(&f.lev.address, &attacker, &1, &true).is_err());
    // settle needs the keeper's own signature (incentive recipient).
    assert!(f.lev.try_settle_v1(&f.keeper, &pid).is_err());
    // Admin-only and one-time configuration.
    let r = fixture_risk();
    assert!(f.lev.try_propose_risk_v1(&r).is_err());
    let cfg = f.lev.config_v1();
    f.e.mock_auths(&[MockAuth { address: &f.admin, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: "initialize", args: (cfg.clone(), r.clone()).into_val(&f.e), sub_invokes: &[] } }]);
    lev_err(f.lev.try_initialize(&cfg, &r), LevError::AlreadyInitialized);
}

#[test]
fn risk_changes_are_timelocked() {
    let f = setup();
    let mut r = fixture_risk();
    r.open_buffer_bps = 500;
    let auth = |name: &'static str, args: SVec<Val>| {
        f.e.mock_auths(&[MockAuth { address: &f.admin, invoke: &MockAuthInvoke { contract: &f.lev.address, fn_name: name, args, sub_invokes: &[] } }]);
    };
    auth("propose_risk_v1", (r.clone(),).into_val(&f.e));
    f.lev.propose_risk_v1(&r);
    auth("execute_risk_v1", SVec::new(&f.e));
    lev_err(f.lev.try_execute_risk_v1(), LevError::Timelock);
    f.tick(86_400, P1);
    auth("execute_risk_v1", SVec::new(&f.e));
    f.lev.execute_risk_v1();
    let now = f.lev.risk_v1();
    assert_eq!((now.open_buffer_bps, now.revision), (500, 1));
}
