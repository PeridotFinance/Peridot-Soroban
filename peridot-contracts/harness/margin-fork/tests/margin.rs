//! Custody-only margin on a read-only Mainnet fork (live controller f41e5260, live receipts 5630a701,
//! live Reflector), protocol-29 host. The margin contract is the built WASM. The Aquarius pool is a
//! native mock with the deployed pool's auth shape (swap does NOT require_auth; the input transfer does).
//! User and liquidator calls run with explicit (non-mock-all) auth trees, so missing contract-auth
//! entries fail. Env: FORK=<fork.json> MARGIN_WASM=<margin_controller.wasm> OUT=<results.json>
use sha2::{Digest, Sha256};
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke};
use soroban_sdk::xdr::{LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey, Limits, ReadXdr};
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

// ---- mock Aquarius pool: [XLM, USDC]; out = in * num/den (USDC per XLM in 1e7) less 0.1% fee
#[contracttype]
enum PK { Tokens, Num, Den }
#[contract]
pub struct MockPool;
#[contractimpl]
impl MockPool {
    pub fn init(e: Env, tokens: Vec<Address>, num: i128, den: i128) {
        e.storage().instance().set(&PK::Tokens, &tokens);
        e.storage().instance().set(&PK::Num, &num);
        e.storage().instance().set(&PK::Den, &den);
    }
    pub fn set_rate(e: Env, num: i128, den: i128) {
        e.storage().instance().set(&PK::Num, &num);
        e.storage().instance().set(&PK::Den, &den);
    }
    pub fn get_tokens(e: Env) -> Vec<Address> { e.storage().instance().get(&PK::Tokens).unwrap() }
    pub fn swap(e: Env, user: Address, in_idx: u32, out_idx: u32, amount_in: u128, out_min: u128) -> u128 {
        let t: Vec<Address> = e.storage().instance().get(&PK::Tokens).unwrap();
        let (n, d): (i128, i128) = (e.storage().instance().get(&PK::Num).unwrap(), e.storage().instance().get(&PK::Den).unwrap());
        let ai = amount_in as i128;
        let raw = if in_idx == 0 { ai * n / d } else { ai * d / n };
        let out = raw * 999 / 1000;
        assert!(out as u128 >= out_min, "pool slippage");
        let me = e.current_contract_address();
        token::TokenClient::new(&e, &t.get(in_idx).unwrap()).transfer(&user, &me, &ai);
        token::TokenClient::new(&e, &t.get(out_idx).unwrap()).transfer(&me, &user, &out);
        out as u128
    }
}

fn a(e: &Env, s: &str) -> Address { Address::from_str(e, s) }
fn call<T: TryFromVal<Env, Val>>(e: &Env, c: &Address, f: &str, args: std::vec::Vec<Val>) -> T {
    e.invoke_contract(c, &Symbol::new(e, f), SVec::from_slice(e, &args))
}
fn load_fork(path: &str) -> LedgerSnapshot {
    let j: Value = serde_json::from_slice(&std::fs::read(path).expect("FORK unreadable")).unwrap();
    let mut entries = std::vec::Vec::new();
    for e in j["entries"].as_array().unwrap() {
        let key = LedgerKey::from_xdr_base64(e["key"].as_str().unwrap(), Limits::none()).unwrap();
        let data = LedgerEntryData::from_xdr_base64(e["xdr"].as_str().unwrap(), Limits::none()).unwrap();
        let entry = LedgerEntry { last_modified_ledger_seq: e["lastModified"].as_u64().unwrap() as u32, data, ext: LedgerEntryExt::V0 };
        entries.push((Box::new(key), (Box::new(entry), e["liveUntil"].as_u64().map(|x| x as u32))));
    }
    let nid: [u8; 32] = Sha256::digest(j["network"].as_str().unwrap().as_bytes()).into();
    LedgerSnapshot {
        protocol_version: (j["protocolVersion"].as_u64().unwrap() as u32).min(29), // Mainnet is p30; host here is p29
        sequence_number: j["ledger"].as_u64().unwrap() as u32,
        timestamp: j["closeTime"].as_u64().unwrap(),
        network_id: nid, base_reserve: 5_000_000, min_persistent_entry_ttl: 2_073_600, min_temp_entry_ttl: 17_280,
        max_entry_ttl: 3_110_400, ledger_entries: entries,
    }
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

#[test]
fn custody_margin_on_mainnet_fork() {
    let snap = load_fork(&std::env::var("FORK").expect("FORK"));
    let wasm = std::fs::read(std::env::var("MARGIN_WASM").expect("MARGIN_WASM")).unwrap();
    let e = Env::from_ledger_snapshot(snap);
    e.cost_estimate().disable_resource_limits();
    let (ctrl, admin, xlm, usdc, xr, ur) = (a(&e, CTRL), a(&e, ADMIN), a(&e, XLM_T), a(&e, USDC_T), a(&e, XLM_R), a(&e, USDC_R));
    let mut rows = std::vec::Vec::new();

    // ---- setup (admin, mocked)
    e.mock_all_auths();
    let margin = e.register(wasm.as_slice(), (admin.clone(), ctrl.clone(), admin.clone()));
    let (px, pu) = (price(&e, XLM_T), price(&e, USDC_T));
    // USDC per XLM, 1e7: (px.0/px.1) / (pu.0/pu.1)
    let num = (px.0 * pu.1 * 10_000_000 / px.1 / pu.0) as i128;
    let pool = e.register(MockPool, ());
    let _: () = call(&e, &pool, "init", vec![SVec::from_array(&e, [xlm.clone(), usdc.clone()]).into_val(&e), num.into_val(&e), 10_000_000i128.into_val(&e)]);
    fund(&e, &xlm, &pool, 1_000_000 * R7);
    fund(&e, &usdc, &pool, 1_000_000 * R7);
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
    let paused = e.try_invoke_contract::<u64, soroban_sdk::Error>(&margin, &Symbol::new(&e, "open"),
        SVec::from_slice(&e, &[user.into_val(&e), xlm.into_val(&e), usdc.into_val(&e), (100 * R7 as u128).into_val(&e), (5 * R7 as u128).into_val(&e), 1u128.into_val(&e)])).is_err();
    let _: () = call(&e, &margin, "set_opens_paused", vec![admin.into_val(&e), false.into_val(&e)]);

    // ---- open long XLM: 300 XLM collateral, borrow 15 USDC (strict auth)
    let coll: u128 = 300 * R7 as u128;
    let borrow: u128 = 15 * R7 as u128;
    let open_args = (user.clone(), xlm.clone(), usdc.clone(), coll, borrow, 1u128);
    e.mock_auths(&[MockAuth { address: &user, invoke: &MockAuthInvoke {
        contract: &margin, fn_name: "open", args: open_args.clone().into_val(&e),
        sub_invokes: &[MockAuthInvoke { contract: &xlm, fn_name: "transfer", args: (user.clone(), margin.clone(), coll as i128).into_val(&e), sub_invokes: &[] }],
    }}]);
    let id: u64 = call(&e, &margin, "open", vec![user.into_val(&e), xlm.into_val(&e), usdc.into_val(&e), coll.into_val(&e), borrow.into_val(&e), 1u128.into_val(&e)]);
    rows.push(res(&e, "open long XLM (300 XLM + 15 USDC borrow, swap, 2 deposits)"));
    let ltv0: u128 = call(&e, &margin, "get_position_ltv", vec![id.into_val(&e)]);
    let debt0: u128 = call(&e, &margin, "get_position_debt", vec![id.into_val(&e)]);
    let receipt_hook: Option<Address> = call(&e, &xr, "get_margin_controller", vec![]);

    // ---- close (strict auth)
    let x0 = bal(&e, &usdc, &user);
    e.mock_auths(&[MockAuth { address: &user, invoke: &MockAuthInvoke {
        contract: &margin, fn_name: "close", args: (user.clone(), id, 1u128).into_val(&e), sub_invokes: &[] }}]);
    let _: () = call(&e, &margin, "close", vec![user.into_val(&e), id.into_val(&e), 1u128.into_val(&e)]);
    rows.push(res(&e, "close long XLM (withdraw, swap, repay, surplus + pToken transfer)"));
    let user_xr_ptokens: u128 = call(&e, &xr, "get_ptoken_balance", vec![user.into_val(&e)]);
    let surplus = bal(&e, &usdc, &user) - x0;
    let pooled_after_close: u128 = call(&e, &ur, "get_user_borrow_balance", vec![margin.into_val(&e)]);

    // ---- second open, then XLM -40% and liquidate (strict auth)
    e.mock_auths(&[MockAuth { address: &user, invoke: &MockAuthInvoke {
        contract: &margin, fn_name: "open", args: open_args.clone().into_val(&e),
        sub_invokes: &[MockAuthInvoke { contract: &xlm, fn_name: "transfer", args: (user.clone(), margin.clone(), coll as i128).into_val(&e), sub_invokes: &[] }],
    }}]);
    let id2: u64 = call(&e, &margin, "open", vec![user.into_val(&e), xlm.into_val(&e), usdc.into_val(&e), coll.into_val(&e), borrow.into_val(&e), 1u128.into_val(&e)]);
    e.mock_all_auths();
    e.ledger().with_mut(|li| { li.timestamp = (li.timestamp / 300 + 1) * 300; li.sequence_number += 60; });
    post_xlm(&e, 50, 100);
    let _: Option<(u128, u128)> = call(&e, &ctrl, "cache_price", vec![xlm.into_val(&e)]);
    let _: Option<(u128, u128)> = call(&e, &ctrl, "cache_price", vec![usdc.into_val(&e)]);
    let px2 = price(&e, XLM_T);
    let _: () = call(&e, &pool, "set_rate", vec![(num * 50 / 100).into_val(&e), 10_000_000i128.into_val(&e)]);
    let liq_ok: bool = call(&e, &margin, "is_liquidatable", vec![id2.into_val(&e)]);
    let debt2: u128 = call(&e, &margin, "get_position_debt", vec![id2.into_val(&e)]);
    let ltv2: u128 = call(&e, &margin, "get_position_ltv", vec![id2.into_val(&e)]);
    println!("ltv2={ltv2} px2={px2:?} px={px:?}");
    e.mock_auths(&[MockAuth { address: &liq, invoke: &MockAuthInvoke {
        contract: &margin, fn_name: "liquidate", args: (liq.clone(), id2).into_val(&e),
        sub_invokes: &[MockAuthInvoke { contract: &usdc, fn_name: "transfer", args: (liq.clone(), margin.clone(), debt2 as i128).into_val(&e), sub_invokes: &[] }],
    }}]);
    let _: () = call(&e, &margin, "liquidate", vec![liq.into_val(&e), id2.into_val(&e)]);
    rows.push(res(&e, "liquidate (repay, seize + remainder pToken transfers) after XLM -50%"));
    let liq_ptokens: u128 = call(&e, &xr, "get_ptoken_balance", vec![liq.into_val(&e)]);
    let pooled_end: u128 = call(&e, &ur, "get_user_borrow_balance", vec![margin.into_val(&e)]);

    let out = json!({
        "fork": std::env::var("FORK").unwrap(), "marginWasmSha256": hex(&Sha256::digest(&wasm)),
        "checks": {"opensPausedAtCreate": paused, "receiptHookStillUnset": receipt_hook.is_none(),
            "openLtv1e6": ltv0, "openDebt": debt0, "closeSurplusUsdc": surplus, "userXlmPTokensAfterClose": user_xr_ptokens,
            "pooledUsdcDebtAfterClose": pooled_after_close, "liquidatableAfterDrop": liq_ok, "liquidatorXlmPTokens": liq_ptokens,
            "pooledUsdcDebtAtEnd": pooled_end, "xlmPriceBefore": format!("{px:?}"), "xlmPriceAfterDrop": format!("{px2:?}")},
        "rows": rows,
    });
    let s = serde_json::to_string_pretty(&out).unwrap();
    println!("{s}");
    if let Ok(p) = std::env::var("OUT") { std::fs::write(p, &s).unwrap(); }
    assert!(paused && receipt_hook.is_none() && liq_ok && liq_ptokens > 0 && user_xr_ptokens > 0);
}
fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
