//! Local per-transaction measurement of the cl-leveraged stages (LEVERAGE-SPEC.md §7).
//!
//! Every row is ONE top-level invocation in its own `Env`, built from the previous row's ledger
//! snapshot (+1 ledger, +6 s). The snapshot source records every ledger key the host asks for, so
//! "keys" is the unique footprint (absent keys that get created included). "entries" is the SDK
//! total used by the project limits: memory reads + disk reads + writes. CPU is the host 29.0.1
//! default cost model (the spike measured the live model at +0.2M..+0.6M per row; not applied).
//! Auth: `mock_all_auths` in recording mode (nonces created and counted), as in the group harness.
//! Contract identities are fresh C-addresses; real G-account users add an account and trustline
//! disk read each (spike: +1/+1). No network, no keys, no signing.
//!
//! Usage: cargo run --release -- <wasm_dir> <out_dir>
use serde_json::{json, Value};
use soroban_env_host::HostError;
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::{
    contracttype,
    testutils::{Address as _, EnvTestConfig, Ledger, SnapshotSource, SnapshotSourceInput},
    token::StellarAssetClient,
    xdr::{LedgerEntry, LedgerKey, Limits, ReadXdr, WriteXdr},
    Address, Env, IntoVal, String as SStr, Symbol, Val, Vec as SVec,
};
use std::{cell::RefCell, collections::{BTreeMap, BTreeSet}, rc::Rc};

// ------------------------------------------------------------------ wire types (match leverage/src/types.rs)
#[contracttype]
#[derive(Clone)]
pub struct LevConfigV1 {
    pub admin: Address,
    pub receipt: Address,
    pub controller: Address,
    pub market: Address,
    pub swap: Address,
    pub pool: Address,
    pub oracle: Address,
    pub token0: Address,
    pub token1: Address,
    pub treasury: Address,
}
#[contracttype]
#[derive(Clone)]
pub struct LevRiskV1 {
    pub revision: u64,
    pub new_risk_enabled: bool,
    pub open_buffer_bps: u32,
    pub max_ckpt_age_secs: u64,
    pub max_oracle_age_secs: u64,
    pub max_depeg_bps: u32,
    pub max_slippage_bps: u32,
    pub open_fee_bps: u32,
    pub close_fee_bps: u32,
    pub exit_incentive_bps: u32,
    pub tp_min_gap_bps: u32,
    pub tp_confirm_delay_secs: u64,
    pub tp_confirm_window_secs: u64,
    pub max_deadline_secs: u64,
}
#[contracttype]
#[derive(Clone)]
pub enum TakeProfitV1 {
    None,
    EquityAtLeast(u128),
    PriceAtLeast(u128),
}
#[contracttype]
#[derive(Clone)]
pub struct ExitPolicyV1 {
    pub take_profit: TakeProfitV1,
    pub stop_hf_bps: Option<u32>,
}
#[contracttype]
#[derive(Clone)]
pub struct OpenLevV1 {
    pub equity0: u128,
    pub borrow1: u128,
    pub swap_dir: u32,
    pub swap_in: u128,
    pub min_swap: u128,
    pub min_shares: u128,
    pub reserve0: u128,
    pub deadline: u64,
    pub max_ltv_bps: u32,
    pub exit: ExitPolicyV1,
}
#[contracttype]
#[derive(Clone)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

const U: u128 = 10_000_000;
const P1: i128 = 100_000_000_000_000;

// ------------------------------------------------------------------ recording snapshot source
struct Src {
    snap: Rc<LedgerSnapshot>,
    touched: Rc<RefCell<BTreeSet<String>>>,
}
impl SnapshotSource for Src {
    fn get(&self, key: &Rc<LedgerKey>) -> Result<Option<(Rc<LedgerEntry>, Option<u32>)>, HostError> {
        self.touched.borrow_mut().insert(key.to_xdr_base64(Limits::none()).unwrap());
        (&*self.snap).get(key)
    }
}

#[derive(Clone)]
#[allow(dead_code)]
struct Ids {
    t0: String,
    t1: String,
    rc: String,
    ctrl: String,
    mkt: String,
    orc: String,
    swp: String,
    lev: String,
    owner: String,
    keeper: String,
    labels: BTreeMap<String, String>,
}
fn a(e: &Env, s: &str) -> Address {
    Address::from_str(e, s)
}
fn astr(x: &Address) -> String {
    let s: SStr = x.to_string();
    let mut b = vec![0u8; s.len() as usize];
    s.copy_into_slice(&mut b);
    String::from_utf8(b).unwrap()
}

type Args = Rc<dyn Fn(&Env, &Ids) -> SVec<Val>>;
struct Call {
    label: &'static str,
    target: fn(&Ids) -> String,
    f: &'static str,
    args: Args,
    measured: bool,
    dt: u64,
    expect_fail: bool,
    group_dependent: &'static str,
}
fn c(label: &'static str, target: fn(&Ids) -> String, f: &'static str, args: impl Fn(&Env, &Ids) -> SVec<Val> + 'static) -> Call {
    Call { label, target, f, args: Rc::new(args), measured: true, dt: 0, expect_fail: false, group_dependent: "" }
}
impl Call {
    fn setup(mut self) -> Self {
        self.measured = false;
        self
    }
    fn after(mut self, dt: u64) -> Self {
        self.dt = dt;
        self
    }
    fn dep(mut self, d: &'static str) -> Self {
        self.group_dependent = d;
        self
    }
}

fn step_env(snap: &LedgerSnapshot) -> (Env, Rc<RefCell<BTreeSet<String>>>) {
    let snap = Rc::new(snap.clone());
    let touched = Rc::new(RefCell::new(BTreeSet::new()));
    let src = Src { snap: snap.clone(), touched: touched.clone() };
    let mut env = Env::from_ledger_snapshot(SnapshotSourceInput { source: Rc::new(src), ledger_info: Some(snap.ledger_info()), snapshot: Some(snap.clone()) });
    env.set_config(EnvTestConfig { capture_snapshot_at_drop: false });
    env.cost_estimate().disable_resource_limits();
    env.cost_estimate().budget().reset_limits(2_000_000_000, 1_073_741_824);
    env.mock_all_auths();
    (env, touched)
}

fn owner_of(k: &LedgerKey, ids: &Ids) -> String {
    match k {
        LedgerKey::ContractData(d) => {
            let s = d.contract.to_string();
            ids.labels.get(&s).cloned().unwrap_or(s)
        }
        LedgerKey::ContractCode(_) => "code".into(),
        LedgerKey::Account(_) => "account".into(),
        LedgerKey::Trustline(_) => "trustline".into(),
        _ => "other".into(),
    }
}

fn run(call: &Call, snap: &LedgerSnapshot, ids: &Ids) -> (bool, Value, LedgerSnapshot) {
    let (env, touched) = step_env(snap);
    let args = (call.args)(&env, ids);
    let target = a(&env, &(call.target)(ids));
    let res = env.try_invoke_contract::<Val, soroban_sdk::Error>(&target, &Symbol::new(&env, call.f), args);
    let r = env.cost_estimate().resources();
    let ok = matches!(res, Ok(Ok(_)));
    let mut by: BTreeMap<String, u32> = BTreeMap::new();
    let keys = touched.borrow();
    for k in keys.iter() {
        let lk = LedgerKey::from_xdr_base64(k, Limits::none()).unwrap();
        *by.entry(owner_of(&lk, ids)).or_default() += 1;
    }
    let sdk = r.memory_read_entries + r.disk_read_entries + r.write_entries;
    let auth: Vec<String> = env.auths().iter().map(|(x, _)| ids.labels.get(&astr(x)).cloned().unwrap_or_else(|| astr(x))).collect();
    let row = json!({
        "label": call.label, "call": format!("{}.{}", ids.labels.get(&(call.target)(ids)).unwrap(), call.f), "ok": ok,
        "expect_fail": call.expect_fail, "group_dependent": call.group_dependent,
        "err": if ok { Value::Null } else { json!(format!("{res:?}")) },
        "cpu": r.instructions, "mem": r.mem_bytes, "mem_reads": r.memory_read_entries, "disk_reads": r.disk_read_entries,
        "writes": r.write_entries, "sdk_entries": sdk, "unique_keys": keys.len(), "disk_read_bytes": r.disk_read_bytes,
        "write_bytes": r.write_bytes, "events_bytes": r.contract_events_size_bytes, "auth": auth, "keys_by_owner": by,
        "ledger": env.ledger().sequence(), "ts": env.ledger().timestamp(),
    });
    let mut next = env.to_ledger_snapshot();
    // Mocked-auth nonces belong to one transaction; a real signed tx uses a fresh nonce.
    next.ledger_entries.retain(|(k, _)| !matches!(&**k, LedgerKey::ContractData(d) if matches!(d.key, soroban_sdk::xdr::ScVal::LedgerKeyNonce(_))));
    next.sequence_number += 1;
    next.timestamp += 6;
    (ok, row, next)
}

fn seq(name: &str, base: &LedgerSnapshot, calls: Vec<Call>, ids: &Ids, rows: &mut Vec<Value>) -> LedgerSnapshot {
    let mut snap = base.clone();
    for call in calls.iter() {
        snap.timestamp += call.dt;
        snap.sequence_number += (call.dt / 5) as u32;
        let (ok, mut row, next) = run(call, &snap, ids);
        let pass = ok != call.expect_fail;
        eprintln!("{name:>10} {:<52} ok={ok} cpu={:>11} entries={:>3} keys={:>3} writes={:>2}", call.label, row["cpu"], row["sdk_entries"], row["unique_keys"], row["writes"]);
        if !pass {
            panic!("sequence {name}: step {} unexpected outcome: {}", call.label, row["err"]);
        }
        row["sequence"] = json!(name);
        if call.measured {
            rows.push(row);
        }
        if ok {
            snap = next;
        }
    }
    snap
}

// ------------------------------------------------------------------ setup
fn setup(wasm_dir: &str) -> (LedgerSnapshot, Ids) {
    let e = Env::default();
    e.ledger().with_mut(|l| {
        l.timestamp = 1_800_000_000;
        l.sequence_number = 1_000;
        l.protocol_version = 29;
        l.min_persistent_entry_ttl = 2_073_600;
        l.min_temp_entry_ttl = 17_280;
        l.max_entry_ttl = 3_110_400;
    });
    e.mock_all_auths();
    let w = |n: &str| std::fs::read(format!("{wasm_dir}/{n}.wasm")).unwrap_or_else(|_| panic!("missing {n}.wasm"));
    let admin = Address::generate(&e);
    let owner = Address::generate(&e);
    let keeper = Address::generate(&e);
    let treasury = Address::generate(&e);
    let pool = Address::generate(&e);
    let t0 = e.register_stellar_asset_contract_v2(admin.clone()).address();
    let t1 = e.register_stellar_asset_contract_v2(admin.clone()).address();
    let rc = e.register(w("cl_lev_mock_receipt").as_slice(), ());
    let ctrl = e.register(w("cl_lev_mock_controller").as_slice(), ());
    let mkt = e.register(w("cl_lev_mock_market").as_slice(), ());
    let orc = e.register(w("cl_lev_mock_oracle").as_slice(), ());
    let swp = e.register(w("cl_lev_mock_swap").as_slice(), ());
    let lev = e.register(w("cl_leveraged").as_slice(), ());
    let inv = |c: &Address, f: &str, args: SVec<Val>| {
        e.invoke_contract::<Val>(c, &Symbol::new(&e, f), args);
    };
    inv(&rc, "mock_init", (t0.clone(), t1.clone(), orc.clone()).into_val(&e));
    inv(&ctrl, "mock_set_cf", (rc.clone(), 500_000u128).into_val(&e));
    inv(&mkt, "mock_init", (t1.clone(), ctrl.clone(), lev.clone(), 2_536_783_358u128).into_val(&e));
    inv(&swp, "mock_init", (t0.clone(), t1.clone(), P1 as u128).into_val(&e));
    inv(&orc, "mock_set", (Asset::Stellar(t0.clone()), P1, 1_800_000_000u64).into_val(&e));
    for (t, to, n) in [(&t0, &rc, 100_000 * U), (&t1, &rc, 100_000 * U), (&t0, &swp, 100_000 * U), (&t1, &swp, 100_000 * U), (&t1, &mkt, 100_000 * U), (&t0, &owner, 10_000 * U), (&t1, &owner, 10_000 * U)] {
        StellarAssetClient::new(&e, t).mint(to, &(n as i128));
    }
    let cfg = LevConfigV1 { admin: admin.clone(), receipt: rc.clone(), controller: ctrl.clone(), market: mkt.clone(), swap: swp.clone(), pool, oracle: orc.clone(), token0: t0.clone(), token1: t1.clone(), treasury: treasury.clone() };
    let risk = LevRiskV1 { revision: 0, new_risk_enabled: true, open_buffer_bps: 200, max_ckpt_age_secs: 60, max_oracle_age_secs: 600, max_depeg_bps: 100, max_slippage_bps: 100, open_fee_bps: 10, close_fee_bps: 10, exit_incentive_bps: 50, tp_min_gap_bps: 50, tp_confirm_delay_secs: 30, tp_confirm_window_secs: 600, max_deadline_secs: 3_600 };
    inv(&lev, "initialize", (cfg, risk).into_val(&e));
    let mut labels = BTreeMap::new();
    for (x, n) in [(&t0, "PYUSD-SAC"), (&t1, "USDC-SAC"), (&rc, "receipt(mock)"), (&ctrl, "controller(mock)"), (&mkt, "market(mock)"), (&orc, "oracle(mock)"), (&swp, "swap(mock)"), (&lev, "cl-leveraged"), (&owner, "owner"), (&keeper, "keeper"), (&treasury, "treasury"), (&admin, "admin")] {
        labels.insert(astr(x), n.to_string());
    }
    let ids = Ids { t0: astr(&t0), t1: astr(&t1), rc: astr(&rc), ctrl: astr(&ctrl), mkt: astr(&mkt), orc: astr(&orc), swp: astr(&swp), lev: astr(&lev), owner: astr(&owner), keeper: astr(&keeper), labels };
    let mut snap = e.to_ledger_snapshot();
    snap.sequence_number += 1;
    snap.timestamp += 6;
    (snap, ids)
}

// ------------------------------------------------------------------ calls
fn lev(i: &Ids) -> String { i.lev.clone() }
fn rcp(i: &Ids) -> String { i.rc.clone() }
fn orc(i: &Ids) -> String { i.orc.clone() }
fn ctl(i: &Ids) -> String { i.ctrl.clone() }

fn terms(e: &Env, equity: u128, borrow: u128) -> OpenLevV1 {
    let (dir, swap_in) = if equity >= borrow { (0, (equity - borrow) / 2) } else { (1, (borrow - equity) / 2) };
    OpenLevV1 { equity0: equity, borrow1: borrow, swap_dir: dir, swap_in, min_swap: swap_in * 99 / 100, min_shares: 1, reserve0: U / 10, deadline: e.ledger().timestamp() + 1_800, max_ltv_bps: 9_000, exit: ExitPolicyV1 { take_profit: TakeProfitV1::None, stop_hf_bps: None } }
}
fn oracle_now() -> Call {
    c("oracle refresh (setup)", orc, "mock_set", |e, i| (Asset::Stellar(a(e, &i.t0)), P1, e.ledger().timestamp()).into_val(e)).setup()
}
fn open(label: &'static str) -> Call {
    c(label, lev, "open_v1", |e, i| (a(e, &i.owner), terms(e, 1_000 * U, 900 * U)).into_val(e)).dep("market borrow (X4), controller pause (G3)")
}
fn pid(n: u64) -> impl Fn(&Env, &Ids) -> SVec<Val> {
    move |e, _| (n,).into_val(e)
}
fn cmd(n: u64) -> impl Fn(&Env, &Ids) -> SVec<Val> {
    move |e, _| (n,).into_val(e)
}
fn settle(label: &'static str, p: u64) -> Call {
    c(label, lev, "settle_v1", move |e, i| (a(e, &i.keeper), p).into_val(e)).dep("market repay (X4), receipt job/beneficiary (X2/X3)")
}
fn knob(label: &'static str, f: &'static str, args: impl Fn(&Env, &Ids) -> SVec<Val> + 'static) -> Call {
    c(label, rcp, f, args).setup()
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let (wasm_dir, out_dir) = (&argv[1], &argv[2]);
    let (s0, ids) = setup(wasm_dir);
    let mut rows = Vec::new();
    let r = &mut rows;

    // ---- A: open journey (first position) -> take-profit (inline refresh) -> close, no swap
    let s_open = seq("A", &s0, vec![oracle_now(), open("open_v1 (first position)")], &ids, r);
    let s_active = seq("A", &s_open, vec![
        c("fund_v1 (pair open + fund, nested transfers)", lev, "fund_v1", pid(0)).dep("receipt prepare_open_pair (X1) + fund_open"),
        c("R convert_open (MOCK)", rcp, "convert_open", cmd(0)).dep("MOCK: cite doc27 O3 / pilot stage_convert"),
        c("R mint_cash (MOCK)", rcp, "mint_cash", cmd(0)).dep("MOCK: cite doc27 O4 / pilot stage_mint"),
        c("R invest_cash (MOCK)", rcp, "invest_cash", cmd(0)).dep("MOCK: cite doc27 O5 / pilot stage_invest+finalise"),
        c("sync_open_v1 (activate, assign beneficiary)", lev, "sync_open_v1", pid(0)).dep("receipt job result (X2), beneficiary (X3)"),
    ], &ids, r);
    let s_tp = seq("A", &s_active, vec![
        c("R checkpoint_nav (MOCK, L1a)", rcp, "checkpoint_nav", |e, _| SVec::new(e)).dep("MOCK: cite group L1a 18 entries"),
        c("set_exit_v1 (equity TP, fresh stored NAV)", lev, "set_exit_v1", |e, i| (a(e, &i.owner), 0u64, ExitPolicyV1 { take_profit: TakeProfitV1::EquityAtLeast(1_030 * U), stop_hf_bps: Some(10_200) }).into_val(e)).dep("NAV view (G1)"),
        knob("vault fee gain +2% (setup)", "mock_loss", |e, _| (-200i32,).into_val(e)),
    ], &ids, r);
    let s_armed = seq("A", &s_tp, vec![
        oracle_now().after(61),
        c("arm_exit_v1 (stale NAV -> inline checkpoint_nav)", lev, "arm_exit_v1", pid(0)).dep("inline group L1a (G1)"),
    ], &ids, r);
    let s_closing = seq("A", &s_armed, vec![
        oracle_now().after(61),
        c("trigger_exit_v1 TP (inline refresh) + prepare_close", lev, "trigger_exit_v1", pid(0)).dep("inline L1a (G1) + receipt prepare_close (doc27 C1)"),
        c("R withdraw_close (MOCK)", rcp, "withdraw_close", cmd(1)).dep("MOCK: cite doc27 C2a 78 entries"),
    ], &ids, r);
    seq("A", &s_closing, vec![settle("settle_v1 TP, USDC leg covers debt (no swap)", 0)], &ids, r);

    // ---- B: warm variants from the armed state (stored NAV fresh)
    seq("B", &s_armed, vec![
        oracle_now().after(55),
        c("R checkpoint_nav (MOCK)", rcp, "checkpoint_nav", |e, _| SVec::new(e)).setup(),
        c("trigger_exit_v1 TP (fresh stored NAV) + prepare_close", lev, "trigger_exit_v1", pid(0)).dep("NAV view (G1) + receipt prepare_close"),
    ], &ids, r);
    seq("B", &s_tp, vec![
        oracle_now().after(61),
        c("R checkpoint_nav (MOCK)", rcp, "checkpoint_nav", |e, _| SVec::new(e)).setup(),
        c("arm_exit_v1 (fresh stored NAV)", lev, "arm_exit_v1", pid(0)).dep("NAV view (G1)"),
    ], &ids, r);
    seq("B", &s_active, vec![
        c("R checkpoint_nav (MOCK)", rcp, "checkpoint_nav", |e, _| SVec::new(e)).setup(),
        c("health_v1 view", lev, "health_v1", pid(0)).dep("NAV view (G1), market debt (X4)"),
        c("open_v1 (second position, same owner)", lev, "open_v1", |e, i| (a(e, &i.owner), terms(e, 1_000 * U, 900 * U)).into_val(e)).dep("market borrow (X4), controller pause (G3)"),
    ], &ids, r);
    seq("B", &s_active, vec![
        c("R checkpoint_nav (MOCK)", rcp, "checkpoint_nav", |e, _| SVec::new(e)).setup(),
        c("add_v1 (fresh NAV, borrow more)", lev, "add_v1", |e, i| (a(e, &i.owner), 0u64, terms(e, 100 * U, 50 * U)).into_val(e)).dep("market borrow (X4), NAV view (G1)"),
        c("repay_v1 (external payer)", lev, "repay_v1", |e, i| (a(e, &i.owner), 0u64, 10 * U).into_val(e)).dep("market repay (X4)"),
    ], &ids, r);
    seq("B", &s_active, vec![c("close_v1 (owner, voluntary) + prepare_close", lev, "close_v1", |e, i| (a(e, &i.owner), 0u64).into_val(e)).dep("receipt prepare_close (doc27 C1)")], &ids, r);

    // ---- H: health exit (inline refresh), then deficit swap on settlement
    let s_loss = seq("H", &s_active, vec![knob("venue loss 6% (setup)", "mock_loss", |e, _| (600i32,).into_val(e)), oracle_now().after(61)], &ids, r);
    let s_hclose = seq("H", &s_loss, vec![
        c("trigger_exit_v1 Health (inline refresh) + prepare_close", lev, "trigger_exit_v1", pid(0)).dep("inline L1a (G1), CF (G3), market debt (X4)"),
        c("R withdraw_close (MOCK)", rcp, "withdraw_close", cmd(1)).setup(),
    ], &ids, r);
    seq("H", &s_hclose, vec![settle("settle_v1 Health, deficit swap PYUSD->USDC", 0)], &ids, r);
    seq("H", &s_loss, vec![
        c("R checkpoint_nav (MOCK)", rcp, "checkpoint_nav", |e, _| SVec::new(e)).setup(),
        c("trigger_exit_v1 Health (fresh stored NAV) + prepare_close", lev, "trigger_exit_v1", pid(0)).dep("NAV view (G1)"),
    ], &ids, r);
    // fallback: NAV refused (pool out of band), fallback cause exists
    let s_fb = seq("F", &s_active, vec![knob("venue loss 4% (setup)", "mock_loss", |e, _| (400i32,).into_val(e)), knob("pool out of band (setup)", "mock_nav_fail", |e, _| (true,).into_val(e)), oracle_now().after(61)], &ids, r);
    seq("F", &s_fb, vec![c("trigger_exit_v1 Health on fallback (NAV try fails, inline fallback)", lev, "trigger_exit_v1", pid(0)).dep("inline L1a try + L1b (G2)")], &ids, r);
    seq("F", &s_fb, vec![
        c("R checkpoint_fallback (MOCK, L1b)", rcp, "checkpoint_fallback", |e, _| SVec::new(e)).dep("MOCK: cite group L1b 13 entries"),
        c("trigger_exit_v1 Health on fallback (stored fallback, NAV try fails)", lev, "trigger_exit_v1", pid(0)).dep("inline L1a try (G1) + fallback view (G2)"),
    ], &ids, r);

    // ---- R: refunds
    seq("R", &s_open, vec![c("cancel_v1 PendingFund by owner (inline refund + swap)", lev, "cancel_v1", pid(0)).dep("market repay (X4)")], &ids, r);
    let s_funded = seq("R", &s_open, vec![c("fund_v1", lev, "fund_v1", pid(0)).setup(), knob("convert guard fails (setup)", "mock_fail", |e, _| (1u32,).into_val(e)), oracle_now().after(1_801)], &ids, r);
    let s_settling = seq("R", &s_funded, vec![c("cancel_v1 Opening after expiry (receipt cancel + sync)", lev, "cancel_v1", pid(0)).dep("receipt cancel/job (doc27 cancel, X2)")], &ids, r);
    seq("R", &s_settling, vec![settle("settle_v1 refund (swap for fee+interest deficit)", 0)], &ids, r);

    let _ = ctl;
    let out = json!({
        "schema": "cl-leveraged-rows-v1", "host": "soroban-env-host 29.0.1 (default cost model)", "sdk_contracts": "soroban-sdk 25.3.1",
        "entries_definition": "memory reads + disk reads + writes", "keys_definition": "unique ledger keys requested from the snapshot source",
        "limits_internal": {"cpu": 100_000_000u64, "keeper_cpu_stop": 97_000_000u64, "mem": 41_943_040u64, "entries": 100, "keys": 100, "writes": 50},
        "rows": rows,
    });
    std::fs::create_dir_all(out_dir).unwrap();
    std::fs::write(format!("{out_dir}/results.json"), serde_json::to_string_pretty(&out).unwrap()).unwrap();
    let mut md = String::from("| # | Seq | Row | CPU | Mem | Entries (m+d+w) | Keys | Writes | W bytes | Depends on |\n|---|---|---|---:|---:|---|---:|---:|---:|---|\n");
    for (n, x) in out["rows"].as_array().unwrap().iter().enumerate() {
        md += &format!("| {} | {} | {}{} | {} | {} | **{}** ({}+{}+{}) | {} | {} | {} | {} |\n", n + 1, x["sequence"].as_str().unwrap(), x["label"].as_str().unwrap(),
            if x["ok"].as_bool().unwrap() { "" } else { " — REVERTED (expected)" }, x["cpu"], x["mem"], x["sdk_entries"], x["mem_reads"], x["disk_reads"], x["writes"], x["unique_keys"], x["writes"], x["write_bytes"], x["group_dependent"].as_str().unwrap());
    }
    std::fs::write(format!("{out_dir}/rows.md"), md).unwrap();
    eprintln!("wrote {out_dir}/results.json and rows.md");
}
