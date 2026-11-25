//! Custody-only margin (v0.2 WASM) on a read-only Mainnet fork: live controller f41e5260, live receipts 5630a701,
//! live Reflector and the REAL Aquarius XLM/USDC concentrated pool (CBBMQ...BUCV, its instance, code and every entry the
//! RPC footprints of read-only swap simulations touched), protocol-30 host (soroban-env-host 30.0.1).
//! Fidelity checks: fork estimate_swap == recorded Mainnet quotes, and a probe swap == the recorded simulated output.
//! Auth shape: a native probe contract swaps on the real pool under ENFORCING auth with only the root `transfer`
//! entry, only the nested `swap -> transfer` entry, both, or none, and the pass/fail matrix is asserted and written out.
//! User and liquidator calls run with explicit (non-mock-all) auth trees, so missing contract-auth entries fail.
//! Result (fork ledger 65661251, 25 Nov 2026): a contract caller needs the ROOT `transfer` entry; the nested
//! `swap -> transfer` entry alone fails Error(Context, InvalidAction). The RPC's recorded tree for a direct account
//! caller is `swap -> transfer`, which says nothing about contract callers.
//! Regenerate (read-only RPC, no keys; the snapshot is git-ignored evidence, ~620 KB):
//!   cd tools && npm ci && node fetch-fork.cjs --out ../.scratch/fork.json
//! v0.2 margin WASM (sha256 80a082ec..6ea8, 23,062 B; branch HEAD only has the WP0 stubs):
//!   git archive 5ab616e peridot-contracts | tar x -C <dir>   (with ../../openzeppelin-stellar-contracts resolvable)
//!   cd <dir>/peridot-contracts && stellar contract build --package margin-controller --out-dir <out>
//! Run: FORK=$PWD/.scratch/fork.json MARGIN_WASM=<out>/margin_controller.wasm OUT=$PWD/.scratch/result.json \
//!   cargo test -- --nocapture
//! Close sells XLM against a 1% Reflector floor by default; if the real pool sits further below Reflector the test
//! records the gap and the rejection, widens to 5% via set_limits and reports `slippageWidened`.
use sha2::{Digest, Sha256};
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke};
use soroban_sdk::xdr::{LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey, Limits, ReadXdr};
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Bytes, Env, IntoVal, Symbol, TryFromVal, Val, Vec};
type SVec<T> = Vec<T>;
use serde_json::{json, Value};

const CTRL: &str = "CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK";
const ADMIN: &str = "GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP";
const REFLECTOR: &str = "CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN";
const USDC_R: &str = "CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F";
const USDC_T: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const XLM_R: &str = "CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS";
const XLM_T: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const R7: i128 = 10_000_000;

#[contracttype]
enum SacKey { Balance(Address) }
#[contracttype]
struct SacBalance { amount: i128, authorized: bool, clawback: bool }
#[contracttype]
#[derive(Clone)]
enum RAsset { Stellar(Address), Other(Symbol) }
#[contracttype]
#[derive(Clone)]
struct RPriceData { price: i128, timestamp: u64 }
#[contracttype]
struct RPriceUpdate { mask: Bytes, prices: soroban_sdk::Vec<i128> }

// ---- auth probe: swaps `amount` of t_in on the real pool as itself, authorizing exactly the entries `mode` selects
// (bit 0: root `t_in.transfer(me, pool, amount)`; bit 1: `pool.swap(me, ii, oi, amount, 1)` with that transfer nested),
// built exactly as margin v0.2 `swap()` builds them.
#[contract]
pub struct AuthProbe;
#[contractimpl]
impl AuthProbe {
    pub fn run(e: Env, pool: Address, t_in: Address, ii: u32, oi: u32, amount: u128, mode: u32) -> u128 {
        let me = e.current_contract_address();
        let xfer = InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext { contract: t_in.clone(), fn_name: Symbol::new(&e, "transfer"), args: (me.clone(), pool.clone(), amount as i128).into_val(&e) },
            sub_invocations: Vec::new(&e),
        });
        let mut auths = Vec::new(&e);
        if mode & 1 != 0 { auths.push_back(xfer.clone()); }
        if mode & 2 != 0 {
            let mut nested = Vec::new(&e);
            nested.push_back(xfer);
            auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext { contract: pool.clone(), fn_name: Symbol::new(&e, "swap"), args: (me.clone(), ii, oi, amount, 1u128).into_val(&e) },
                sub_invocations: nested,
            }));
        }
        if !auths.is_empty() { e.authorize_as_current_contract(auths); }
        e.invoke_contract(&pool, &Symbol::new(&e, "swap"), (me, ii, oi, amount, 1u128).into_val(&e))
    }
}

fn a(e: &Env, s: &str) -> Address { Address::from_str(e, s) }
fn call<T: TryFromVal<Env, Val>>(e: &Env, c: &Address, f: &str, args: std::vec::Vec<Val>) -> T {
    e.invoke_contract(c, &Symbol::new(e, f), SVec::from_slice(e, &args))
}
fn load_fork(path: &str) -> (LedgerSnapshot, Value) {
    let j: Value = serde_json::from_slice(&std::fs::read(path).expect("FORK unreadable")).unwrap();
    let mut entries = std::vec::Vec::new();
    for e in j["entries"].as_array().unwrap() {
        let key = LedgerKey::from_xdr_base64(e["key"].as_str().unwrap(), Limits::none()).unwrap();
        let data = LedgerEntryData::from_xdr_base64(e["xdr"].as_str().unwrap(), Limits::none()).unwrap();
        let entry = LedgerEntry { last_modified_ledger_seq: e["lastModified"].as_u64().unwrap() as u32, data, ext: LedgerEntryExt::V0 };
        entries.push((Box::new(key), (Box::new(entry), e["liveUntil"].as_u64().map(|x| x as u32))));
    }
    let nid: [u8; 32] = Sha256::digest(j["network"].as_str().unwrap().as_bytes()).into();
    let snap = LedgerSnapshot {
        protocol_version: j["protocolVersion"].as_u64().unwrap() as u32, // Mainnet p30, host p30: no clamp
        sequence_number: j["ledger"].as_u64().unwrap() as u32,
        timestamp: j["closeTime"].as_u64().unwrap(),
        network_id: nid, base_reserve: 5_000_000, min_persistent_entry_ttl: 2_073_600, min_temp_entry_ttl: 17_280,
        max_entry_ttl: 3_110_400, ledger_entries: entries,
    };
    (snap, j)
}
fn fund(e: &Env, tok: &Address, holder: &Address, amount: i128) {
    let k = SacKey::Balance(holder.clone());
    e.as_contract(tok, || {
        e.storage().persistent().set(&k, &SacBalance { amount, authorized: true, clawback: false });
        e.storage().persistent().extend_ttl(&k, 100, 2_000_000);
    });
}
fn bal(e: &Env, tok: &Address, who: &Address) -> i128 { token::TokenClient::new(e, tok).balance(who) }
fn res(e: &Env, op: &str) -> Value {
    let r = e.cost_estimate().resources();
    let keys = r.disk_read_entries + r.memory_read_entries;
    json!({"op": op, "cpu": r.instructions, "memBytes": r.mem_bytes, "diskReadEntries": r.disk_read_entries,
        "memoryReadEntries": r.memory_read_entries, "writeEntries": r.write_entries, "footprintKeys": keys,
        "diskReadBytes": r.disk_read_bytes, "writeBytes": r.write_bytes,
        "mainnetFit": r.instructions <= 400_000_000 && keys <= 400 && r.disk_read_entries <= 200 && r.write_entries <= 200 && r.mem_bytes <= 41_943_040,
        "projectGate100Keys": keys <= 100})
}
/// Post one Reflector round with XLM multiplied by num/den (others unchanged), mocked oracle admin auth.
fn post_xlm(e: &Env, num: i128, den: i128) {
    let r = a(e, REFLECTOR);
    let assets: SVec<RAsset> = call(e, &r, "assets", vec![]);
    let mut prices = SVec::<i128>::new(e);
    let mut mask = std::vec![0u8; (assets.len() as usize + 7) / 8];
    for (i, asset) in assets.iter().enumerate() {
        let pd: Option<RPriceData> = call(e, &r, "lastprice", vec![asset.clone().into_val(e)]);
        let Some(pd) = pd else { continue };
        let mut p = pd.price;
        if let RAsset::Other(s) = &asset { if *s == Symbol::new(e, "XLM") { p = p * num / den; } }
        mask[i / 8] |= 1 << (i % 8);
        prices.push_back(p);
    }
    let ts: u64 = e.ledger().timestamp() * 1000;
    let upd = RPriceUpdate { mask: Bytes::from_slice(e, &mask), prices };
    let _: () = call(e, &r, "set_price", vec![upd.into_val(e), ts.into_val(e)]);
}
fn price(e: &Env, t: &str) -> (u128, u128) {
    let p: Option<(u128, u128)> = call(e, &a(e, CTRL), "get_price_usd", vec![a(e, t).into_val(e)]);
    p.expect("price")
}

fn u(v: &Value) -> u128 { v.as_str().unwrap().parse().unwrap() }
fn try_open(e: &Env, margin: &Address, user: &Address, xlm: &Address, usdc: &Address, coll: u128, borrow: u128) -> Result<u64, String> {
    let open_args = (user.clone(), xlm.clone(), usdc.clone(), coll, borrow, 1u128);
    e.mock_auths(&[MockAuth { address: user, invoke: &MockAuthInvoke {
        contract: margin, fn_name: "open", args: open_args.clone().into_val(e),
        sub_invokes: &[MockAuthInvoke { contract: xlm, fn_name: "transfer", args: (user.clone(), margin.clone(), coll as i128).into_val(e), sub_invokes: &[] }],
    }}]);
    let r = e.try_invoke_contract::<u64, soroban_sdk::Error>(margin, &Symbol::new(e, "open"), open_args.into_val(e));
    match r { Ok(Ok(id)) => Ok(id), Ok(Err(c)) => Err(format!("{c:?}")), Err(x) => Err(format!("{x:?}")) }
}

#[test]
fn custody_margin_on_mainnet_fork() {
    let (snap, fj) = load_fork(&std::env::var("FORK").expect("FORK"));
    let wasm = std::fs::read(std::env::var("MARGIN_WASM").expect("MARGIN_WASM")).unwrap();
    let e = Env::from_ledger_snapshot(snap);
    e.cost_estimate().disable_resource_limits();
    #[allow(deprecated)]
    let host_protocol = e.ledger().protocol_version();
    assert_eq!(host_protocol, 30, "protocol-30 host and snapshot");
    let (ctrl, admin, xlm, usdc, xr, ur) = (a(&e, CTRL), a(&e, ADMIN), a(&e, XLM_T), a(&e, USDC_T), a(&e, XLM_R), a(&e, USDC_R));
    let pool = a(&e, fj["pool"]["address"].as_str().expect("fork has no real pool (old schema?)"));
    let mut rows = std::vec::Vec::new();

    // ---- snapshot fidelity: the real pool on the fork quotes exactly what Mainnet quoted at fetch time
    let toks: SVec<Address> = call(&e, &pool, "get_tokens", vec![]);
    assert_eq!(toks, SVec::from_array(&e, [xlm.clone(), usdc.clone()]));
    let mut quote_checks = std::vec::Vec::new();
    for q in fj["pool"]["after"]["quotes"].as_array().unwrap() {
        let (i, o, amt) = (q["in"].as_u64().unwrap() as u32, q["out"].as_u64().unwrap() as u32, u(&q["amount"]));
        let got: u128 = call(&e, &pool, "estimate_swap", vec![i.into_val(&e), o.into_val(&e), amt.into_val(&e)]);
        quote_checks.push(json!({"in": i, "amount": amt.to_string(), "mainnet": q["estimate"], "fork": got.to_string()}));
        assert_eq!(got, u(&q["estimate"]), "fork estimate_swap differs from Mainnet: snapshot is missing pool state");
    }

    // ---- auth shape of the real pool for a CONTRACT caller, enforcing auth (no mocks)
    let probe = e.register(AuthProbe, ());
    let amt: u128 = 15 * R7 as u128; // USDC in, same size as the margin open's swap
    let sim0 = &fj["pool"]["swapSims"][0];
    assert!(sim0["in"] == 1 && u(&sim0["amount"]) == amt);
    fund(&e, &usdc, &probe, 4 * amt as i128);
    e.set_auths(&[]);
    let mut matrix = serde_json::Map::new();
    let mut first_out: Option<u128> = None;
    for (mode, name) in [(0u32, "none"), (2, "nested swap->transfer only"), (1, "root transfer only"), (3, "both (as margin v0.2)")] {
        let r = e.try_invoke_contract::<u128, soroban_sdk::Error>(&probe, &Symbol::new(&e, "run"),
            (pool.clone(), usdc.clone(), 1u32, 0u32, amt, mode).into_val(&e));
        let v = match &r { Ok(Ok(out)) => { if first_out.is_none() { first_out = Some(*out); } json!({"ok": true, "out": out.to_string()}) }
            Ok(Err(c)) => json!({"ok": false, "err": format!("{c:?}")}), Err(x) => json!({"ok": false, "err": format!("{x:?}")}) };
        if mode == 1 { rows.push(res(&e, "probe: bare pool.swap 15 USDC->XLM (root transfer auth)")); }
        matrix.insert(name.to_string(), v);
    }
    let ok = |k: &str| matrix[k]["ok"].as_bool().unwrap();
    let first_out = first_out.expect("no auth shape swapped");
    assert_eq!(first_out, u(&sim0["out_amount"]), "fork swap output != Mainnet simulated output");
    assert!(!ok("none"), "pool must need the caller's transfer auth");
    assert!(ok("both (as margin v0.2)"));
    let auth_shape = match (ok("root transfer only"), ok("nested swap->transfer only")) {
        (true, false) => "root transfer", (false, true) => "nested swap->transfer", (true, true) => "either", (false, false) => "only both together" };

    // ---- setup (admin, mocked)
    e.mock_all_auths();
    let margin = e.register(wasm.as_slice(), (admin.clone(), ctrl.clone(), admin.clone()));
    let (px, pu) = (price(&e, XLM_T), price(&e, USDC_T));
    // oracle USDC per XLM vs pool (15 USDC quote), in bps
    let pool_xlm_out: u128 = call(&e, &pool, "estimate_swap", vec![1u32.into_val(&e), 0u32.into_val(&e), amt.into_val(&e)]);
    let oracle_xlm_out = amt * pu.0 / pu.1 * px.1 / px.0;
    let gap_bps = (pool_xlm_out as i128 - oracle_xlm_out as i128) * 10_000 / oracle_xlm_out as i128;
    let _: () = call(&e, &margin, "set_market", vec![admin.into_val(&e), xlm.into_val(&e), xr.into_val(&e)]);
    let _: () = call(&e, &margin, "set_market", vec![admin.into_val(&e), usdc.into_val(&e), ur.into_val(&e)]);
    let _: () = call(&e, &margin, "enter_markets", vec![admin.into_val(&e), SVec::from_array(&e, [xlm.clone(), usdc.clone()]).into_val(&e)]);
    let _: () = call(&e, &margin, "set_pool", vec![admin.into_val(&e), pool.into_val(&e)]);
    let _: () = call(&e, &margin, "set_risk", vec![admin.into_val(&e), xlm.into_val(&e), 300_000u128.into_val(&e), 450_000u128.into_val(&e)]);
    let _: () = call(&e, &margin, "set_risk", vec![admin.into_val(&e), usdc.into_val(&e), 400_000u128.into_val(&e), 620_000u128.into_val(&e)]);
    let _: () = call(&e, &margin, "set_caps", vec![admin.into_val(&e), usdc.into_val(&e), (50 * R7 as u128).into_val(&e), (200 * R7 as u128).into_val(&e)]);
    let _: () = call(&e, &margin, "set_caps", vec![admin.into_val(&e), xlm.into_val(&e), (390 * R7 as u128).into_val(&e), (1550 * R7 as u128).into_val(&e)]);
    // paused by default
    let user = Address::generate(&e);
    let liq = Address::generate(&e);
    fund(&e, &xlm, &user, 10_000 * R7);
    fund(&e, &usdc, &user, 1_000 * R7);
    fund(&e, &usdc, &liq, 1_000 * R7);
    let coll: u128 = 300 * R7 as u128;
    let borrow: u128 = 15 * R7 as u128;
    let paused = try_open(&e, &margin, &user, &xlm, &usdc, coll, borrow).is_err();
    e.mock_all_auths();
    let _: () = call(&e, &margin, "set_opens_paused", vec![admin.into_val(&e), false.into_val(&e)]);

    // ---- open long XLM: 300 XLM collateral, borrow 15 USDC, swap on the real pool (strict auth)
    let mut widened: Option<String> = None;
    let id = match try_open(&e, &margin, &user, &xlm, &usdc, coll, borrow) {
        Ok(id) => id,
        Err(err) => {
            // Default 1% oracle floor rejected the real pool's fill: record it, widen to 5% and retry.
            widened = Some(format!("default 1% slippage rejected open ({err}); pool-vs-oracle gap {gap_bps} bps; set_limits 5%"));
            e.mock_all_auths();
            let _: () = call(&e, &margin, "set_limits", vec![admin.into_val(&e), 50_000u128.into_val(&e), 1_050_000u128.into_val(&e)]);
            try_open(&e, &margin, &user, &xlm, &usdc, coll, borrow).expect("open with 5% slippage")
        }
    };
    rows.push(res(&e, "open long XLM (300 XLM + 15 USDC borrow, real-pool swap, 2 deposits)"));
    let ltv0: u128 = call(&e, &margin, "get_position_ltv", vec![id.into_val(&e)]);
    let debt0: u128 = call(&e, &margin, "get_position_debt", vec![id.into_val(&e)]);
    let receipt_hook: Option<Address> = call(&e, &xr, "get_margin_controller", vec![]);

    // ---- close (strict auth): withdraw, real-pool swap XLM->USDC, repay
    let x0 = bal(&e, &usdc, &user);
    let try_close = || {
        e.mock_auths(&[MockAuth { address: &user, invoke: &MockAuthInvoke {
            contract: &margin, fn_name: "close", args: (user.clone(), id, 1u128).into_val(&e), sub_invokes: &[] }}]);
        match e.try_invoke_contract::<(), soroban_sdk::Error>(&margin, &Symbol::new(&e, "close"), (user.clone(), id, 1u128).into_val(&e)) {
            Ok(Ok(())) => Ok(()), Ok(Err(c)) => Err(format!("{c:?}")), Err(x) => Err(format!("{x:?}")) }
    };
    if let Err(err) = try_close() {
        // close sells XLM: the default 1% oracle floor fails when the pool is > 1% below Reflector.
        widened = Some(format!("{}default 1% slippage rejected close ({err}); buy-side (USDC->XLM) pool-vs-Reflector gap {gap_bps} bps, sell side wider by fee + spread; set_limits 5% (fork only)",
            widened.map(|w| w + "; ").unwrap_or_default()));
        e.mock_all_auths();
        let _: () = call(&e, &margin, "set_limits", vec![admin.into_val(&e), 50_000u128.into_val(&e), 1_050_000u128.into_val(&e)]);
        try_close().expect("close with 5% slippage");
    }
    rows.push(res(&e, "close long XLM (withdraw, real-pool swap, repay, surplus + pToken transfer)"));
    let user_xr_ptokens: u128 = call(&e, &xr, "get_ptoken_balance", vec![user.into_val(&e)]);
    let surplus = bal(&e, &usdc, &user) - x0;
    let pooled_after_close: u128 = call(&e, &ur, "get_user_borrow_balance", vec![margin.into_val(&e)]);

    // ---- second open, then XLM -50% on the oracle and liquidate (strict auth). liquidate itself does not swap.
    let id2 = try_open(&e, &margin, &user, &xlm, &usdc, coll, borrow).expect("second open");
    e.mock_all_auths();
    e.ledger().with_mut(|li| { li.timestamp = (li.timestamp / 300 + 1) * 300; li.sequence_number += 60; });
    post_xlm(&e, 50, 100);
    let _: Option<(u128, u128)> = call(&e, &ctrl, "cache_price", vec![xlm.into_val(&e)]);
    let _: Option<(u128, u128)> = call(&e, &ctrl, "cache_price", vec![usdc.into_val(&e)]);
    let px2 = price(&e, XLM_T);
    let liq_ok: bool = call(&e, &margin, "is_liquidatable", vec![id2.into_val(&e)]);
    let debt2: u128 = call(&e, &margin, "get_position_debt", vec![id2.into_val(&e)]);
    e.mock_auths(&[MockAuth { address: &liq, invoke: &MockAuthInvoke {
        contract: &margin, fn_name: "liquidate", args: (liq.clone(), id2).into_val(&e),
        sub_invokes: &[MockAuthInvoke { contract: &usdc, fn_name: "transfer", args: (liq.clone(), margin.clone(), debt2 as i128).into_val(&e), sub_invokes: &[] }],
    }}]);
    let _: () = call(&e, &margin, "liquidate", vec![liq.into_val(&e), id2.into_val(&e)]);
    rows.push(res(&e, "liquidate (repay, seize + remainder pToken transfers; no swap) after XLM -50%"));
    let liq_ptokens: u128 = call(&e, &xr, "get_ptoken_balance", vec![liq.into_val(&e)]);
    let pooled_end: u128 = call(&e, &ur, "get_user_borrow_balance", vec![margin.into_val(&e)]);

    let out = json!({
        "fork": std::env::var("FORK").unwrap(), "forkLedger": fj["ledger"], "forkFetchedAt": fj["fetchedAt"],
        "hostProtocol": host_protocol, "marginWasmSha256": hex(&Sha256::digest(&wasm)),
        "pool": {"address": fj["pool"]["address"], "quoteChecks": quote_checks, "probeSwapOut": first_out.to_string(),
            "mainnetSimOut": sim0["out_amount"], "rpcRecordedAuthForAccountCaller": sim0["recordedAuth"],
            "contractCallerAuthMatrix": Value::Object(matrix), "authShapeUsed": auth_shape, "poolVsOracleGapBps15Usdc": gap_bps,
            "slippageWidened": widened},
        "checks": {"opensPausedAtCreate": paused, "receiptHookStillUnset": receipt_hook.is_none(),
            "openLtv1e6": ltv0, "openDebt": debt0, "closeSurplusUsdc": surplus, "userXlmPTokensAfterClose": user_xr_ptokens,
            "pooledUsdcDebtAfterClose": pooled_after_close, "liquidatableAfterDrop": liq_ok, "liquidatorXlmPTokens": liq_ptokens,
            "pooledUsdcDebtAtEnd": pooled_end, "xlmPriceBefore": format!("{px:?}"), "xlmPriceAfterDrop": format!("{px2:?}")},
        "rows": rows,
    });
    let s = serde_json::to_string_pretty(&out).unwrap();
    println!("{s}");
    if let Ok(p) = std::env::var("OUT") { std::fs::write(p, &s).unwrap(); }
    assert!(paused && receipt_hook.is_none() && liq_ok && liq_ptokens > 0 && user_xr_ptokens > 0 && pooled_after_close == 0 && pooled_end == 0);
}
fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
