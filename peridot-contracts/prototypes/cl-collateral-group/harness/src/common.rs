// CL collateral GROUP measurements, 18 Nov 2026. LOCAL host-only. Copied from the item-S spike
// harness (prototypes/cl-collateral-spike/harness, unchanged) and extended: time advance, expected
// reverts, the patched group controller and the group sleeve double.
// No network, no signing, no submission. Reads fetched live Mainnet entries from
// $SPIKE_STATE_DIR/state.json. Each top-level call runs in its OWN Env built from the
// snapshot left by the previous call, so every row is counted like a separate transaction.
// Real deployed bytes: CL vault e6638030 (live instance + live storage), staged receipt
// 140dcaab (live instance), Aquarius pool + Reflector + SACs (live), controller 847894a6 and
// cash/lending market 4229b7ef (FRESH local instances of the deployed code), JRM 4f57459d
// (release artifact, fresh instance). Spike-only double: the sleeve market WASM.
// Auth: mock_all_auths everywhere (recording mode; no signature verification).
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use soroban_env_host::HostError;
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig, SnapshotSource, SnapshotSourceInput},
    xdr::{ContractEventBody, LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey, Limits, ReadXdr, ScVal, WriteXdr},
    Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};
use std::{cell::RefCell, collections::{BTreeMap, BTreeSet}, format, rc::Rc, string::{String, ToString}, vec::Vec as StdVec};

const R: &str = "CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI";
const V: &str = "CD6N2EQJN47SYLSIYUK3YVBKD4YISATZ2CZZJQZCXKNNVB4HVR6AWL63";
const O: &str = "GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP";
const KEEPER: &str = "GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4";
const ORACLE: &str = "CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M";
const PYUSD: &str = "CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2";
const USDC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const TREASURY: &str = "GDM5XP4ACJPXPYP5G2UYGB6WC3G2MUBCYI2ZUCYWVB5CL2JWGCFWNWA3";
const POOL: &str = "CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX";

thread_local! {
    static CREATED: RefCell<BTreeSet<String>> = RefCell::new(BTreeSet::new());
    static UNKNOWN: RefCell<BTreeSet<String>> = RefCell::new(BTreeSet::new());
    static ABSENT: RefCell<BTreeSet<String>> = RefCell::new(BTreeSet::new());
}

struct Src { snap: Rc<LedgerSnapshot>, touched: Rc<RefCell<BTreeSet<String>>>, archived: Rc<RefCell<BTreeSet<String>>>, seq: u32 }
impl SnapshotSource for Src {
    fn get(&self, key: &Rc<LedgerKey>) -> Result<Option<(Rc<LedgerEntry>, Option<u32>)>, HostError> {
        let x = key.to_xdr_base64(Limits::none()).unwrap();
        self.touched.borrow_mut().insert(x.clone());
        let r = (&*self.snap).get(key)?;
        if let Some((_, Some(lu))) = &r { if *lu < self.seq { self.archived.borrow_mut().insert(x.clone()); } }
        if r.is_none() && !ABSENT.with(|a| a.borrow().contains(&x)) && !CREATED.with(|c| c.borrow().contains(&x)) {
            UNKNOWN.with(|u| u.borrow_mut().insert(x));
        }
        Ok(r)
    }
}

fn sdir() -> String { std::env::var("SPIKE_STATE_DIR").expect("SPIKE_STATE_DIR") }
fn wdir() -> String { std::env::var("SPIKE_WASM_DIR").expect("SPIKE_WASM_DIR") }
fn wasm(name: &str) -> StdVec<u8> { std::fs::read(format!("{}/{name}", wdir())).unwrap_or_else(|_| panic!("missing wasm {name}")) }

fn base_snapshot() -> (LedgerSnapshot, Value) {
    let st: Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/state.json", sdir())).unwrap()).unwrap();
    let mut entries = StdVec::new();
    for (k, e) in st["entries"].as_object().unwrap() {
        let key = LedgerKey::from_xdr_base64(k, Limits::none()).unwrap();
        let data = LedgerEntryData::from_xdr_base64(e["xdr"].as_str().unwrap(), Limits::none()).unwrap();
        let entry = LedgerEntry { last_modified_ledger_seq: e["lastModified"].as_u64().unwrap_or(0) as u32, data, ext: LedgerEntryExt::V0 };
        entries.push((std::boxed::Box::new(key), (std::boxed::Box::new(entry), e["liveUntil"].as_u64().map(|x| x as u32))));
    }
    ABSENT.with(|a| { let mut a = a.borrow_mut(); a.clear(); for k in st["absent"].as_object().unwrap().keys() { a.insert(k.clone()); } });
    let snap = LedgerSnapshot {
        protocol_version: st["protocolVersion"].as_u64().unwrap() as u32,
        sequence_number: st["ledger"].as_u64().unwrap() as u32,
        timestamp: st["closeTime"].as_u64().unwrap(),
        network_id: Sha256::digest(b"Public Global Stellar Network ; September 2015").into(),
        base_reserve: st["baseReserve"].as_u64().unwrap() as u32,
        min_persistent_entry_ttl: st["minPersistentTtl"].as_u64().unwrap() as u32,
        min_temp_entry_ttl: st["minTemporaryTtl"].as_u64().unwrap() as u32,
        max_entry_ttl: st["maxEntryTtl"].as_u64().unwrap() as u32,
        ledger_entries: entries,
    };
    (snap, st)
}


/// (cost type, live_const - host_const, live_linear - host_linear) for every CPU coefficient that
/// differs between the pinned host 29.0.1 defaults and the LIVE network cost params in state.json.
fn cpu_deltas() -> StdVec<(soroban_sdk::xdr::ContractCostType, i64, i64)> {
    use soroban_sdk::xdr::{ConfigSettingEntry, ContractCostType};
    thread_local! { static D: RefCell<Option<StdVec<(ContractCostType, i64, i64)>>> = RefCell::new(None); }
    if let Some(d) = D.with(|d| d.borrow().clone()) { return d; }
    let st: Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/state.json", sdir())).unwrap()).unwrap();
    let mut cpu = None;
    for e in st["config"]["entries"].as_array().unwrap() {
        if let LedgerEntryData::ConfigSetting(ConfigSettingEntry::ContractCostParamsCpuInstructions(p)) = LedgerEntryData::from_xdr_base64(e["xdr"].as_str().unwrap(), Limits::none()).unwrap() { cpu = Some(p); }
    }
    let cpu = cpu.unwrap(); let env = Env::default(); let debug = format!("{:?}", env.host().budget_cloned());
    let mut out = StdVec::new();
    for ty in ContractCostType::variants() {
        let name = format!("{ty:?}");
        let line = debug.lines().find(|l| l.split_whitespace().next() == Some(name.as_str())).unwrap();
        let cols: StdVec<_> = line.split_whitespace().collect();
        let n: StdVec<i64> = cols[cols.len() - 4..].iter().map(|x| x.parse().unwrap()).collect();
        let c = cpu.0.get(ty as usize).unwrap();
        if c.const_term != n[0] || c.linear_term != n[1] { out.push((ty, c.const_term - n[0], c.linear_term - n[1])); }
    }
    D.with(|d| *d.borrow_mut() = Some(out.clone())); out
}

struct Step { env: Env, touched: Rc<RefCell<BTreeSet<String>>>, archived: Rc<RefCell<BTreeSet<String>>> }
pub(crate) fn step_env(snap: &LedgerSnapshot) -> Step {
    let snap = Rc::new(snap.clone());
    let (touched, archived) = (Rc::new(RefCell::new(BTreeSet::new())), Rc::new(RefCell::new(BTreeSet::new())));
    let src = Src { snap: snap.clone(), touched: touched.clone(), archived: archived.clone(), seq: snap.sequence_number };
    let mut env = Env::from_ledger_snapshot(SnapshotSourceInput { source: Rc::new(src), ledger_info: Some(snap.ledger_info()), snapshot: Some(snap.clone()) });
    env.set_config(EnvTestConfig { capture_snapshot_at_drop: false });
    // Measurement mode: host limits raised so over-limit rows still report figures; compared afterwards.
    env.cost_estimate().disable_resource_limits();
    { let mut b = env.cost_estimate().budget(); b.reset_limits(2_000_000_000, 1_073_741_824); }
    env.mock_all_auths();
    Step { env, touched, archived }
}
fn a(env: &Env, s: &str) -> Address { Address::from_str(env, s) }
fn astr(x: &Address) -> String { let s = x.to_string(); let mut b = [0u8; 56]; s.copy_into_slice(&mut b); std::str::from_utf8(&b).unwrap().to_string() }
fn scv(v: &ScVal) -> Value {
    match v {
        ScVal::U128(p) => json!((((p.hi as u128) << 64) | p.lo as u128).to_string()),
        ScVal::I128(p) => json!((((p.hi as i128) << 64) | p.lo as i128).to_string()),
        ScVal::U64(n) => json!(n.to_string()), ScVal::I64(n) => json!(n.to_string()), ScVal::U32(n) => json!(n), ScVal::I32(n) => json!(n), ScVal::Bool(b) => json!(b),
        ScVal::Void => Value::Null,
        ScVal::Symbol(s) => json!(s.0.to_utf8_string_lossy()),
        ScVal::Vec(Some(xs)) => Value::Array(xs.iter().map(scv).collect()),
        ScVal::Map(Some(m)) => { let mut o = serde_json::Map::new(); for e in m.iter() { o.insert(match &e.key { ScVal::Symbol(s) => s.0.to_utf8_string_lossy(), k => format!("{k:?}") }, scv(&e.val)); } Value::Object(o) }
        ScVal::Address(x) => json!(x.to_string()),
        o => json!(format!("{o:?}")),
    }
}
fn sc(env: &Env, v: &Val) -> Value { scv(&ScVal::try_from_val(env, v).unwrap()) }
fn tv(env: &Env, c: &str, f: &str, args: Vec<Val>) -> Value {
    match env.try_invoke_contract::<Val, soroban_sdk::Error>(&a(env, c), &Symbol::new(env, f), args) { Ok(Ok(v)) => sc(env, &v), e => {
        let d: StdVec<String> = env.host().get_events().unwrap().0.iter().rev().take(6).map(|x| format!("{:?}", x.event.body).chars().take(300).collect()).collect();
        json!({"unavailable": format!("{e:?}"), "call": format!("{c}.{f}"), "diag": d}) } }
}
fn u(v: &Value) -> u128 { v.as_str().map(|x| x.parse().unwrap()).or_else(|| v.as_u64().map(|x| x as u128)).unwrap_or_else(|| panic!("not a number: {v}")) }

#[derive(Clone)]
struct Call { label: String, contract: String, f: String, args: std::rc::Rc<dyn Fn(&Env) -> Vec<Val>>, measured: bool, dt: u64, expect_fail: bool }
fn call(label: &str, contract: &str, f: &str, args: impl Fn(&Env) -> Vec<Val> + 'static, measured: bool) -> Call {
    Call { label: label.into(), contract: contract.into(), f: f.into(), args: std::rc::Rc::new(args), measured, dt: 0, expect_fail: false }
}
impl Call {
    /// Advance the ledger clock by `dt` extra seconds before this call (on top of 6 s per step).
    fn after(mut self, dt: u64) -> Self { self.dt = dt; self }
    /// This call is expected to revert; the sequence then counts as passed.
    fn fails(mut self) -> Self { self.expect_fail = true; self }
}

/// Run one top-level call as its own transaction. Returns (ok, row, next snapshot).
fn run(c: &Call, snap: &LedgerSnapshot) -> (bool, Value, LedgerSnapshot) {
    std::eprintln!("STEP {} {}.{}", c.label, c.contract, c.f);
    let s = step_env(snap);
    let env = &s.env;
    let args_v = (c.args)(env);
    let res = env.try_invoke_contract::<Val, soroban_sdk::Error>(&a(env, &c.contract), &Symbol::new(env, &c.f), args_v);
    let r = env.cost_estimate().resources();
    // Addresses whose authorization the top-level call required (recording mode). Empty = permissionless.
    let auth_addrs: StdVec<String> = env.auths().iter().map(|(ad, _)| astr(ad)).collect();
    let (ok, out) = match &res { Ok(Ok(v)) => (true, sc(env, v)), Ok(Err(e)) => (false, json!(format!("conversion error {e:?}"))), Err(e) => (false, json!(format!("{e:?}"))) };
    let sdk_entries = r.memory_read_entries + r.disk_read_entries + r.write_entries;
    // Re-price the metered work with the LIVE network CPU coefficients (host 29.0.1 defaults differ).
    let mut cpu_adj: i64 = 0; let mut adj_detail = serde_json::Map::new();
    for (ty, dc, dl) in cpu_deltas() {
        let t = env.cost_estimate().budget().tracker(ty);
        let d = (t.iterations as i64) * dc + ((t.inputs.unwrap_or(0) as i64) * dl >> 7);
        if t.iterations > 0 { adj_detail.insert(format!("{ty:?}"), json!({"iterations": t.iterations, "delta_cpu": d})); }
        cpu_adj += d;
    }
    let cpu_live = (r.instructions as i64 + cpu_adj) as u64;
    let touched: StdVec<String> = s.touched.borrow().iter().cloned().collect();
    let unique = touched.len();
    let pool_touched = touched.iter().any(|k| {
        LedgerKey::from_xdr_base64(k, Limits::none()).map(|lk| matches!(&lk, LedgerKey::ContractData(d) if d.contract.to_string() == POOL)).unwrap_or(false)
    });
    // per-contract key counts (what dominated)
    let mut by: BTreeMap<String, u32> = BTreeMap::new();
    for k in &touched {
        let lk = LedgerKey::from_xdr_base64(k, Limits::none()).unwrap();
        let n = match &lk { LedgerKey::ContractData(d) => d.contract.to_string(), LedgerKey::ContractCode(_) => "contractCode".into(), LedgerKey::Account(_) => "account".into(), LedgerKey::Trustline(_) => "trustline".into(), o => format!("{:?}", core::mem::discriminant(o)) };
        *by.entry(n).or_default() += 1;
    }
    let mut events = StdVec::new();
    let mut diag = StdVec::new();
    for e in env.host().get_events().unwrap().0.iter() {
        if let ContractEventBody::V0(b) = &e.event.body {
            if !e.failed_call && e.event.type_ == soroban_sdk::xdr::ContractEventType::Contract {
                events.push(json!({"topics": b.topics.iter().map(scv).collect::<StdVec<_>>()}));
            }
            if !ok && e.event.type_ == soroban_sdk::xdr::ContractEventType::Diagnostic { diag.push(format!("{:?} {:?} {:?}", e.event.contract_id, b.topics, b.data).chars().take(600).collect::<String>()); }
        }
    }
    let diag: StdVec<String> = diag.into_iter().rev().take(12).collect();
    let row = json!({
        "label": c.label, "call": format!("{}.{}", c.contract, c.f), "measured": c.measured, "expect_fail": c.expect_fail, "dt_extra": c.dt, "ledger": env.ledger().sequence(), "timestamp": env.ledger().timestamp(),
        "ok": ok, "result": out, "auth_required": auth_addrs,
        "cpu": cpu_live, "cpu_host_default_model": r.instructions, "cpu_live_model_adjustment": cpu_adj, "cpu_adjustment_detail": adj_detail, "mem": r.mem_bytes,
        "mem_reads": r.memory_read_entries, "disk_reads": r.disk_read_entries, "writes": r.write_entries,
        "sdk_entries": sdk_entries, "unique_keys": unique,
        "disk_read_bytes": r.disk_read_bytes, "write_bytes": r.write_bytes, "events_bytes": r.contract_events_size_bytes,
        "archived_keys_touched": s.archived.borrow().len(), "pool_touched": pool_touched, "keys_by_owner": by,
        "events": events.len(), "event_topics": events.iter().take(12).cloned().collect::<StdVec<_>>(), "diagnostics_if_failed": diag,
    });
    let mut next = env.to_ledger_snapshot();
    strip_nonces(&mut next);
    for (k, _) in next.ledger_entries.iter() { let x = k.to_xdr_base64(Limits::none()).unwrap(); CREATED.with(|c| c.borrow_mut().insert(x)); }
    next.sequence_number += 1; next.timestamp += 6;
    (ok, row, next)
}
/// Mocked-auth nonces are per-transaction; a real signed tx uses a fresh nonce each time.
fn strip_nonces(s: &mut LedgerSnapshot) {
    s.ledger_entries.retain(|(k, _)| !matches!(&**k, LedgerKey::ContractData(d) if matches!(d.key, ScVal::LedgerKeyNonce(_))));
}
/// Cold controller price cache: remove the controller's PriceCache entries (equivalent to no
/// pricing transaction in the last k*resolution = 600 s; the stale entry case reads the same key).
fn drop_price_cache(s: &LedgerSnapshot, ctrl: &str) -> LedgerSnapshot {
    let mut t = s.clone();
    t.ledger_entries.retain(|(k, _)| !matches!(&**k, LedgerKey::ContractData(d) if d.contract.to_string() == ctrl
        && matches!(&d.key, ScVal::Vec(Some(v)) if matches!(v.first(), Some(ScVal::Symbol(sym)) if sym.0.to_utf8_string_lossy() == "PriceCache"))));
    t
}
fn read(snap: &LedgerSnapshot, f: impl Fn(&Env) -> Value) -> Value { let s = step_env(snap); f(&s.env) }

/// Execute a sequence; stop at first failure.
fn seq(name: &str, base: &LedgerSnapshot, calls: &[Call]) -> (Value, LedgerSnapshot, bool) {
    let mut snap = base.clone(); let mut rows = StdVec::new(); let mut all = true;
    for c in calls {
        if c.dt > 0 { snap.timestamp += c.dt; }
        let (ok, row, next) = run(c, &snap); rows.push(row);
        if c.expect_fail { all = all && !ok; break; }
        if !ok { all = false; break; }
        snap = next;
    }
    (json!({"sequence": name, "ok": all, "rows": rows}), snap, all)
}

struct Addrs { ctrl: String, usdc_m: String, pyusd_m: String, jrm: String, sleeve: String, supplier: String, liquidator: String }

fn deploy(snap: &LedgerSnapshot, log: &mut StdVec<Value>) -> (LedgerSnapshot, Addrs) {
    let s = step_env(snap); let env = &s.env;
    let o = a(env, O);
    // Explicit test capital (issuer auth host-mocked; not a claim about issuer signatures).
    let pyusd = soroban_sdk::token::StellarAssetClient::new(env, &a(env, PYUSD));
    let usdc = soroban_sdk::token::StellarAssetClient::new(env, &a(env, USDC));
    pyusd.mint(&o, &30_000_000_000i128); // 3,000 PYUSD
    usdc.mint(&o, &1_000_000_000i128);   // 100 USDC (seeds)
    let supplier = Address::generate(env); let liquidator = a(env, TREASURY); // G account with a live USDC trustline
    usdc.mint(&supplier, &50_000_000_000i128);   // 5,000 USDC lender liquidity
    usdc.mint(&liquidator, &20_000_000_000i128); // 2,000 USDC liquidator budget
    let ctrl = env.register(wasm("controller.wasm").as_slice(), ());
    let usdc_m = env.register(wasm("market.wasm").as_slice(), ());
    let pyusd_m = env.register(wasm("market.wasm").as_slice(), ());
    let jrm = env.register(wasm("jrm.wasm").as_slice(), ());
    let sleeve = env.register(wasm("sleeve.wasm").as_slice(), ());
    let inv = |c: &Address, f: &str, args: Vec<Val>| { let r = env.try_invoke_contract::<Val, soroban_sdk::Error>(c, &Symbol::new(env, f), args); assert!(matches!(r, Ok(Ok(_))), "setup {f} failed: {r:?}"); };
    inv(&ctrl, "initialize", (o.clone(),).into_val(env));
    inv(&ctrl, "set_oracle", (a(env, ORACLE),).into_val(env));
    inv(&jrm, "initialize", (20_000u128, 180_000u128, 4_000_000u128, 800_000u128, o.clone()).into_val(env));
    for (m, tok, borrow) in [(&usdc_m, USDC, true), (&pyusd_m, PYUSD, false)] {
        inv(m, "initialize", (a(env, tok), 0u128, 0u128, o.clone()).into_val(env));
        inv(m, "set_min_cash_reserve_bps", (o.clone(), if borrow { 1_000u32 } else { 10_000u32 }).into_val(env));
        if borrow { inv(m, "set_cash_borrowing_enabled", (o.clone(), true).into_val(env)); inv(m, "set_interest_model", (jrm.clone(),).into_val(env)); }
        inv(m, "seed_minimum_supply", (o.clone(), 10_000u128).into_val(env));
        inv(&ctrl, "add_market", (m.clone(),).into_val(env));
        inv(m, "set_peridottroller", (ctrl.clone(),).into_val(env));
    }
    // mode 1 (live), source 1 (via real staged receipt), max checkpoint age 60 s, no seize proxy
    inv(&sleeve, "init", (ctrl.clone(), a(env, V), a(env, R), a(env, PYUSD), 1u32, 1u32, 60u64, false).into_val(env));
    inv(&ctrl, "add_market", (sleeve.clone(),).into_val(env));
    inv(&ctrl, "set_market_cf", (sleeve.clone(), 500_000u128).into_val(env));
    inv(&ctrl, "set_market_cf", (pyusd_m.clone(), 500_000u128).into_val(env));
    inv(&usdc_m, "deposit", (supplier.clone(), 50_000_000_000u128).into_val(env));
    let ad = Addrs { ctrl: astr(&ctrl), usdc_m: astr(&usdc_m), pyusd_m: astr(&pyusd_m), jrm: astr(&jrm), sleeve: astr(&sleeve), supplier: astr(&supplier), liquidator: astr(&liquidator) };
    log.push(json!({"deploy": {"ctrl": ad.ctrl, "usdc_market": ad.usdc_m, "pyusd_cash_market": ad.pyusd_m, "jrm": ad.jrm, "sleeve": ad.sleeve, "supplier": ad.supplier, "liquidator": ad.liquidator}}));
    let mut next = env.to_ledger_snapshot();
    strip_nonces(&mut next);
    for (k, _) in next.ledger_entries.iter() { let x = k.to_xdr_base64(Limits::none()).unwrap(); CREATED.with(|c| c.borrow_mut().insert(x)); }
    next.sequence_number += 1; next.timestamp += 6;
    (next, ad)
}

fn i128v(n: i128) -> i128 { n }

/// Drive one real open -> convert -> mint -> 3 keeper checkpoints -> invest -> finalise on the
/// LIVE receipt/vault instances so the vault holds a genuine pool position (real pool bytes).
fn invest(snap: &LedgerSnapshot, log: &mut StdVec<Value>) -> Option<LedgerSnapshot> {
    let liq = u(&read(snap, |env| tv(env, V, "get_position_liquidity", Vec::new(env))));
    if liq > 0 { log.push(json!({"invest": "live vault already invested at capture", "liquidity": liq.to_string()})); return Some(snap.clone()); }
    let next_id = u(&read(snap, |env| tv(env, R, "stage_state", Vec::new(env)))["next_id"]) as u64;
    // Continue the owner's live, unfinished open command if one exists; otherwise open a new one.
    let mut calls: StdVec<Call> = StdVec::new();
    let mut id = next_id;
    let mut phase: u64 = 99;
    if next_id > 0 {
        let j = read(snap, move |env| tv(env, R, "stage_job", (next_id - 1,).into_val(env)));
        log.push(json!({"latest_live_job": j, "id": next_id - 1}));
        if j["kind"] == json!(0) { if let Some(p) = j["phase"].as_u64() { if p < 4 || p == 5 { id = next_id - 1; phase = p; } } }
    }
    if phase == 99 {
        let q = read(snap, |env| tv(env, V, "stage_quote_open", (10_000_000_000u128, 20u32, 5000u32).into_val(env)));
        log.push(json!({"open_quote": q}));
        let (swap_in, min_swap, min_shares, reserve) = (u(&q["swap_in"]), u(&q["min_swap"]), u(&q["min_shares"]), u(&q["reserve"]));
        let deadline = snap.timestamp + 3000;
        calls.push(call("open", R, "stage_open", move |e| (a(e, O), 10_000_000_000u128, swap_in, min_swap, min_shares, reserve, deadline).into_val(e), false));
        phase = 1;
    }
    log.push(json!({"invest_continues_job": id, "from_phase": phase}));
    if phase <= 1 { calls.push(call("convert", R, "stage_convert", move |e| (id,).into_val(e), false)); }
    if phase <= 2 { calls.push(call("mint", R, "stage_mint", move |e| (id,).into_val(e), false)); }
    if phase <= 3 {
        for g in ["guard1", "guard2", "guard3"] { calls.push(call(g, V, "stage_checkpoint_investment", |e| (a(e, KEEPER),).into_val(e), false)); }
        calls.push(call("invest", R, "stage_invest", move |e| (id,).into_val(e), false));
    }
    calls.push(call("finalise", R, "stage_finalise", move |e| (id,).into_val(e), false));
    let (rows, next, ok) = seq("invest-flow (setup, live receipt+vault+pool)", snap, &calls);
    log.push(rows);
    if ok { Some(next) } else { None }
}

fn views(snap: &LedgerSnapshot, ad: &Addrs) -> Value {
    let sl = ad.sleeve.clone(); let ctl = ad.ctrl.clone();
    read(snap, move |env| json!({
        "vault_liquidity": tv(env, V, "get_position_liquidity", Vec::new(env)),
        "vault_available_owner": tv(env, V, "stage_available", (a(env, O),).into_val(env)),
        "receipt_stage_account_owner": tv(env, R, "stage_account", (a(env, O),).into_val(env)),
        "sleeve_snapshot_owner": tv(env, &sl, "get_account_snapshot", (a(env, O),).into_val(env)),
        "ctrl_price_cache_pyusd": tv(env, &ctl, "get_price_usd", (a(env, PYUSD),).into_val(env)),
        "ctrl_price_cache_usdc": tv(env, &ctl, "get_price_usd", (a(env, USDC),).into_val(env)),
        "vault_quote_close_1e9": tv(env, V, "stage_quote_close", (1_000_000_000u128, 20u32).into_val(env)),
    }))
}

fn write_misses() -> usize {
    let all: StdVec<String> = UNKNOWN.with(|u| u.borrow().iter().cloned().collect());
    std::fs::write(format!("{}/misses.json", sdir()), serde_json::to_string(&all).unwrap()).unwrap();
    all.len()
}


/// Compare the pinned host's default cost coefficients with the LIVE captured cost params.
#[test]
fn cost_model_vs_live() {
    use soroban_sdk::xdr::{ConfigSettingEntry, ContractCostType};
    let (_, st) = base_snapshot();
    let mut cpu = None; let mut mem = None;
    for e in st["config"]["entries"].as_array().unwrap() {
        let d = LedgerEntryData::from_xdr_base64(e["xdr"].as_str().unwrap(), Limits::none()).unwrap();
        if let LedgerEntryData::ConfigSetting(cs) = d { match cs {
            ConfigSettingEntry::ContractCostParamsCpuInstructions(p) => cpu = Some(p),
            ConfigSettingEntry::ContractCostParamsMemoryBytes(p) => mem = Some(p), _ => {} } }
    }
    let (cpu, mem) = (cpu.unwrap(), mem.unwrap());
    let env = Env::default();
    let debug = format!("{:?}", env.host().budget_cloned());
    let mut mism = StdVec::new(); let variants = ContractCostType::variants();
    for ty in variants {
        let i = ty as usize; let name = format!("{ty:?}");
        let line = debug.lines().find(|l| l.split_whitespace().next() == Some(name.as_str())).expect("debug format");
        let cols: StdVec<_> = line.split_whitespace().collect();
        let nums: StdVec<i64> = cols[cols.len() - 4..].iter().map(|x| x.parse().unwrap()).collect();
        let (c, m) = (cpu.0.get(i), mem.0.get(i));
        let live = [c.map(|x| x.const_term).unwrap_or(-1), c.map(|x| x.linear_term).unwrap_or(-1), m.map(|x| x.const_term).unwrap_or(-1), m.map(|x| x.linear_term).unwrap_or(-1)];
        if nums != live { mism.push(json!({"type": name, "host": nums, "live": live})); }
    }
    std::println!("COST_MODEL live_cpu_entries={} live_mem_entries={} host_types={} mismatches={}", cpu.0.len(), mem.0.len(), variants.len(), serde_json::to_string(&mism).unwrap());
}
