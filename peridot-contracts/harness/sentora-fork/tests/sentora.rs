//! Isolated Sentora-backed lending group on a read-only Mainnet fork (protocol-30 host, soroban-env-host 30.0.1).
//!
//! Real bytes from the fork: Sentora XLM vault CA54LV.. and USDC vault CAHEWH.. (wasm ca6b85a1), their reward escrow
//! CCZSNG.. (f22e4a52), the XLM/USDC SACs with the vaults' live balances, Reflector CAFJZQ.. (price feed), and the CODE
//! of the live cash-group controller (f41e5260) and receipts (5630a701). Fresh controller/receipt instances are
//! registered from that deployed code: a NEW isolated controller that shares nothing with the live cash group.
//! The only workspace WASM is the sentora-strategy prototype (sha256 asserted).
//!
//! Flow, each step measured: lender XLM deposit -> borrower USDC deposit -> XLM borrow -> lender withdraw beyond the
//! idle buffer (exit through real Sentora) -> Reflector round with XLM +20 % -> keeper refresh -> liquidation ->
//! liquidator exit of the seized USDC pTokens (real Sentora) -> permissionless reward claim on the real escrow.
//! User deposit/borrow/withdraw run with explicit (non-mock-all) auth trees: only the user signs; every nested
//! receipt->strategy->Sentora->SAC authorization must come from the contracts. Setup, the Reflector admin's
//! set_price, the Sentora escrow admin's set_rewards_root and the liquidator use mock_all_auths.
//!
//! Regenerate the fork (read-only RPC, no keys; git-ignored):
//!   cd tools && npm ci && node fetch-fork.cjs --out ../.scratch/fork.json
//! Build the strategy first (from peridot-contracts/): stellar contract build --package sentora-strategy
//! Run: cargo test -- --nocapture   (results: .scratch/sentora-fork-results.json)
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke};
use soroban_sdk::xdr::{LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey, Limits, ReadXdr, ToXdr};
use soroban_sdk::{contracttype, token, Address, Bytes, BytesN, Env, IntoVal, Symbol, TryFromVal, Val, Vec};
use std::cell::RefCell;

mod strat {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/sentora_strategy.wasm");
}
const STRAT_SHA: &str = "221eaa03091d31e928083619764eea2b6a741a9bd80c397fc34e133309355954";
const SENTORA_SHA: &str = "ca6b85a13e53353912aeee2d4742830fc58076660dde64fdad4a6afbcca9674f";
const ESCROW_SHA: &str = "f22e4a52893d04b277c79bd19b3ff0b8771e1c0ca396a35d9339dd3a027ae299";

const ADMIN: &str = "GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP"; // baked init admin of the deployed code
const ESCROW: &str = "CCZSNGVMEHRKMMMQGVDHAXT3KSWIMPEPRRTUE2GU3F6TW4XA2Y6EMHN3";
const REFLECTOR: &str = "CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN";
const XLM_T: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const USDC_T: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const S_XLM: &str = "CA54LVHMAY7HGLMVPN4W72XJB4OGKVZBZX26FWN6JD4P3HJFWQUQEHJO";
const S_USDC: &str = "CAHEWHOPPDBQYFMAOLDOXXGUX2BCR7EXP4CWYCRY3NEAJB35YPZMMJFF";
const LIVE_CTRL: &str = "CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK";
const LIVE_RV: &str = "CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F";
const R7: i128 = 10_000_000;
const WEEK: u64 = 7 * 24 * 3600;
const CF: u128 = 700_000;
const IDLE_BPS: u32 = 2_000;

// Internal rule: <= 100 footprint entries / 100M instructions. Live network: 400M instr, 200 disk reads, 200 writes, 400 entries.
const NET_INSTR: i64 = 400_000_000;
const NET_MEM: i64 = 41_943_040;

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
struct RPriceUpdate { mask: Bytes, prices: Vec<i128> }
#[contracttype]
enum SentoraKey { NextDepositId, TotalPrincipal }

fn a(e: &Env, s: &str) -> Address { Address::from_str(e, s) }
fn call<T: TryFromVal<Env, Val>>(e: &Env, c: &Address, f: &str, args: std::vec::Vec<Val>) -> T {
    e.invoke_contract(c, &Symbol::new(e, f), Vec::from_slice(e, &args))
}
fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
fn h32(e: &Env, x: &str) -> soroban_sdk::BytesN<32> {
    let mut b = [0u8; 32];
    for i in 0..32 { b[i] = u8::from_str_radix(&x[2 * i..2 * i + 2], 16).unwrap(); }
    soroban_sdk::BytesN::from_array(e, &b)
}
/// The reviewed Sentora vault and escrow code the adapter pins.
fn pins(e: &Env) -> (soroban_sdk::BytesN<32>, soroban_sdk::BytesN<32>) { (h32(e, SENTORA_SHA), h32(e, ESCROW_SHA)) }
fn dir() -> String { env!("CARGO_MANIFEST_DIR").to_string() }

fn load_fork() -> (LedgerSnapshot, Value) {
    let path = std::env::var("FORK").unwrap_or_else(|_| format!("{}/.scratch/fork.json", dir()));
    let j: Value = serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|_| panic!("{path} unreadable: run tools/fetch-fork.cjs"))).unwrap();
    let mut entries = std::vec::Vec::new();
    for e in j["entries"].as_array().unwrap() {
        let key = LedgerKey::from_xdr_base64(e["key"].as_str().unwrap(), Limits::none()).unwrap();
        let data = LedgerEntryData::from_xdr_base64(e["xdr"].as_str().unwrap(), Limits::none()).unwrap();
        let entry = LedgerEntry { last_modified_ledger_seq: e["lastModified"].as_u64().unwrap() as u32, data, ext: LedgerEntryExt::V0 };
        entries.push((Box::new(key), (Box::new(entry), e["liveUntil"].as_u64().map(|x| x as u32))));
    }
    let nid: [u8; 32] = Sha256::digest(j["network"].as_str().unwrap().as_bytes()).into();
    let snap = LedgerSnapshot {
        protocol_version: j["protocolVersion"].as_u64().unwrap() as u32,
        sequence_number: j["ledger"].as_u64().unwrap() as u32,
        timestamp: j["closeTime"].as_u64().unwrap(),
        network_id: nid, base_reserve: 5_000_000, min_persistent_entry_ttl: 2_073_600, min_temp_entry_ttl: 17_280,
        max_entry_ttl: 3_110_400, ledger_entries: entries,
    };
    (snap, j)
}

/// Deployed WASM bytes for `hash` from the fork's ContractCode entries.
fn code(fj: &Value, hash: &str) -> std::vec::Vec<u8> {
    for e in fj["entries"].as_array().unwrap() {
        if let LedgerEntryData::ContractCode(c) = LedgerEntryData::from_xdr_base64(e["xdr"].as_str().unwrap(), Limits::none()).unwrap() {
            if hex(&c.hash.0) == hash { return c.code.to_vec(); }
        }
    }
    panic!("code {hash} not in fork")
}

fn fund(e: &Env, tok: &Address, holder: &Address, amount: i128) {
    let k = SacKey::Balance(holder.clone());
    e.as_contract(tok, || {
        e.storage().persistent().set(&k, &SacBalance { amount, authorized: true, clawback: false });
        e.storage().persistent().extend_ttl(&k, 100, 2_000_000);
    });
}
fn bal(e: &Env, tok: &Address, who: &Address) -> i128 { token::TokenClient::new(e, tok).balance(who) }

struct M { asset: Address, rv: Address, sentora: Address, strat: Address }

struct Ctx {
    e: Env,
    admin: Address,
    ctrl: Address,
    xlm: M,
    usdc: M,
    base: std::vec::Vec<Option<i128>>,
    with_strategy: bool,
    xs: bool,
    us: bool,
    rows: RefCell<std::vec::Vec<Value>>,
    notes: RefCell<serde_json::Map<String, Value>>,
}

impl Ctx {
    fn s(&self, m: &M) -> strat::Client<'_> { strat::Client::new(&self.e, &m.strat) }
    fn pt(&self, m: &M, who: &Address) -> u128 { call(&self.e, &m.rv, "get_ptoken_balance", vec![who.into_val(&self.e)]) }
    fn debt(&self, m: &M, who: &Address) -> u128 { call(&self.e, &m.rv, "get_user_borrow_balance", vec![who.into_val(&self.e)]) }
    fn note(&self, k: &str, v: Value) { self.notes.borrow_mut().insert(k.into(), v); }
    fn tick(&self) { self.e.ledger().with_mut(|l| { l.timestamp += 5; l.sequence_number += 1; }); }

    /// Resources of the last top-level invocation, checked against the internal and the live network limits.
    fn res(&self, op: &str) -> Value {
        let r = self.e.cost_estimate().resources();
        let keys = r.disk_read_entries + r.memory_read_entries;
        let row = json!({"op": op, "sentoraXlm": self.xs, "sentoraUsdc": self.us, "cpu": r.instructions, "memBytes": r.mem_bytes, "footprintKeys": keys,
            "diskReadEntries": r.disk_read_entries, "memoryReadEntries": r.memory_read_entries, "writeEntries": r.write_entries,
            "diskReadBytes": r.disk_read_bytes, "writeBytes": r.write_bytes, "eventsBytes": r.contract_events_size_bytes,
            "internalFit": keys <= 100 && r.instructions <= 100_000_000,
            "networkFit": r.instructions <= NET_INSTR && keys <= 400 && r.disk_read_entries <= 200 && r.write_entries <= 200 && r.mem_bytes <= NET_MEM});
        println!("{row}");
        self.rows.borrow_mut().push(row.clone());
        row
    }

    /// Strict auth: only `who` authorizes `contract.f(args)` and the given direct sub-invocations.
    fn strict(&self, who: &Address, contract: &Address, f: &str, args: Vec<Val>, subs: &[(&Address, &str, Vec<Val>)]) {
        let s: std::vec::Vec<MockAuthInvoke> = subs.iter().map(|(c, n, x)| MockAuthInvoke { contract: c, fn_name: n, args: x.clone(), sub_invokes: &[] }).collect();
        self.e.mock_auths(&[MockAuth { address: who, invoke: &MockAuthInvoke { contract, fn_name: f, args: args.clone(), sub_invokes: &s } }]);
        let r = self.e.try_invoke_contract::<Val, soroban_sdk::Error>(contract, &Symbol::new(&self.e, f), args);
        assert!(matches!(r, Ok(Ok(_))), "strict {f} failed: {r:?}");
    }

    fn deposit(&self, m: &M, who: &Address, amt: i128) {
        let e = &self.e;
        self.strict(who, &m.rv, "deposit", (who.clone(), amt as u128).into_val(e),
            &[(&m.asset, "transfer", (who.clone(), m.rv.clone(), amt).into_val(e))]);
    }

    /// Next 5-min Reflector round (+`extra` s) with every base price, XLM scaled num/den; refresh the controller cache.
    fn post(&self, num: i128, den: i128, extra: u64) {
        let e = &self.e;
        e.ledger().with_mut(|l| { l.timestamp = ((l.timestamp + extra) / 300 + 1) * 300; l.sequence_number += 1; });
        let r = a(e, REFLECTOR);
        let assets: Vec<RAsset> = call(e, &r, "assets", vec![]);
        let mut prices = Vec::<i128>::new(e);
        let mut mask = vec![0u8; (assets.len() as usize).div_ceil(8)];
        for (i, asset) in assets.iter().enumerate() {
            let Some(Some(p0)) = self.base.get(i) else { continue };
            let mut p = *p0;
            if let RAsset::Other(s) = &asset { if *s == Symbol::new(e, "XLM") { p = p * num / den; } }
            mask[i / 8] |= 1 << (i % 8);
            prices.push_back(p);
        }
        e.mock_all_auths();
        let upd = RPriceUpdate { mask: Bytes::from_slice(e, &mask), prices };
        let _: () = call(e, &r, "set_price", vec![upd.into_val(e), (e.ledger().timestamp() * 1000).into_val(e)]);
        for t in [&self.xlm.asset, &self.usdc.asset] { let _: Option<(u128, u128)> = call(e, &self.ctrl, "cache_price", vec![t.into_val(e)]); }
    }
    fn price(&self, t: &Address) -> (u128, u128) {
        let p: Option<(u128, u128)> = call(&self.e, &self.ctrl, "get_price_usd", vec![t.into_val(&self.e)]);
        p.expect("price")
    }
    fn refresh(&self, tag: &str) {
        if !self.with_strategy { return; }
        self.e.mock_all_auths();
        for (m, n, on) in [(&self.xlm, "XLM", self.xs), (&self.usdc, "USDC", self.us)] {
            if !on { continue; }
            self.tick();
            let _: () = call(&self.e, &m.rv, "refresh_boosted_underlying", vec![]);
            self.res(&format!("refresh_boosted_underlying {n} ({tag})"));
        }
    }
}

fn setup(xs: bool, us: bool) -> Ctx {
    let with_strategy = xs || us;
    let (snap, fj) = load_fork();
    let e = Env::from_ledger_snapshot(snap);
    e.cost_estimate().disable_resource_limits();
    #[allow(deprecated)]
    let pv = e.ledger().protocol_version();
    assert_eq!(pv, 30, "protocol-30 host and snapshot");
    let strat_wasm = std::fs::read(format!("{}/../../target/wasm32v1-none/release/sentora_strategy.wasm", dir())).unwrap();
    assert_eq!(hex(&Sha256::digest(&strat_wasm)), STRAT_SHA, "rebuild with stellar contract build --package sentora-strategy");
    let ih = &fj["instanceHash"];
    assert_eq!(ih[S_XLM], SENTORA_SHA);
    assert_eq!(ih[S_USDC], SENTORA_SHA);
    assert_eq!(ih[ESCROW], ESCROW_SHA);
    let ctrl_code = code(&fj, ih[LIVE_CTRL].as_str().unwrap());
    let rv_code = code(&fj, ih[LIVE_RV].as_str().unwrap());
    e.mock_all_auths();
    let admin = a(&e, ADMIN);
    let ctrl = e.register(ctrl_code.as_slice(), ());
    let _: () = call(&e, &ctrl, "initialize", vec![admin.into_val(&e)]);
    let _: () = call(&e, &ctrl, "set_oracle", vec![a(&e, REFLECTOR).into_val(&e)]);
    let mut ms = std::vec::Vec::new();
    for (t, s, symb) in [(XLM_T, S_XLM, "XLM"), (USDC_T, S_USDC, "USDC")] {
        let asset = a(&e, t);
        let _: () = call(&e, &ctrl, "set_oracle_asset_symbol", vec![asset.into_val(&e), Some(Symbol::new(&e, symb)).into_val(&e)]);
        let rv = e.register(rv_code.as_slice(), ());
        let _: () = call(&e, &rv, "initialize", vec![asset.into_val(&e), 0u128.into_val(&e), 0u128.into_val(&e), admin.into_val(&e)]);
        let _: () = call(&e, &rv, "enable_static_rates", vec![admin.into_val(&e)]);
        let _: () = call(&e, &rv, "set_cash_borrowing_enabled", vec![admin.into_val(&e), true.into_val(&e)]);
        let _: () = call(&e, &rv, "set_idle_cash_buffer_bps", vec![admin.into_val(&e), IDLE_BPS.into_val(&e)]);
        // Fork-only seed funding of the admin's contract-balance stand-in (admin is a G account: XLM uses its account,
        // USDC a trustline, so seed from a generated contract-type "seeder" via transfer to the admin first).
        let seeder = Address::generate(&e);
        fund(&e, &asset, &seeder, R7);
        token::TokenClient::new(&e, &asset).transfer(&seeder, &admin, &R7);
        let _: u128 = call(&e, &rv, "seed_minimum_supply", vec![admin.into_val(&e), (R7 as u128).into_val(&e)]);
        let _: () = call(&e, &ctrl, "add_market", vec![rv.into_val(&e)]);
        let _: () = call(&e, &ctrl, "set_market_cf", vec![rv.into_val(&e), CF.into_val(&e)]);
        let _: () = call(&e, &rv, "set_collateral_factor", vec![CF.into_val(&e)]);
        let _: () = call(&e, &rv, "set_peridottroller", vec![ctrl.into_val(&e)]);
        let sentora = a(&e, s);
        let st = e.register(strat_wasm.as_slice(), (admin.clone(), rv.clone(), asset.clone(), sentora.clone(), a(&e, ESCROW), WEEK, u128::MAX, pins(&e)));
        if (symb == "XLM" && xs) || (symb == "USDC" && us) {
            let _: () = call(&e, &rv, "set_boosted_vault", vec![admin.into_val(&e), st.into_val(&e)]);
        }
        let _: Option<(u128, u128)> = call(&e, &ctrl, "cache_price", vec![asset.into_val(&e)]);
        ms.push(M { asset, rv, sentora, strat: st });
    }
    let usdc = ms.pop().unwrap();
    let xlm = ms.pop().unwrap();
    let r = a(&e, REFLECTOR);
    let assets: Vec<RAsset> = call(&e, &r, "assets", vec![]);
    let base = assets.iter().map(|x| { let p: Option<RPriceData> = call(&e, &r, "lastprice", vec![x.into_val(&e)]); p.map(|p| p.price) }).collect();
    Ctx { e, admin, ctrl, xlm, usdc, base, with_strategy, xs, us, rows: RefCell::new(vec![]), notes: RefCell::new(serde_json::Map::new()) }
}

#[test]
fn deposit_borrow_withdraw_liquidate_claim_on_real_sentora() {
    flow(true, true, "sentora-fork-results.json");
}

/// Recommended shape: Sentora only behind the supply-heavy collateral market (USDC); the borrowed market is plain cash.
#[test]
fn collateral_only_sentora() {
    flow(false, true, "collateral-only-results.json");
}

/// Same isolated group and flow WITHOUT a boosted strategy (plain cash markets, deployed code): the baseline that
/// separates the Sentora/strategy cost from the core controller/receipt cost.
#[test]
fn baseline_same_flow_without_strategy() {
    flow(false, false, "baseline-results.json");
}

fn flow(xs: bool, us: bool, out_name: &str) {
    let c = setup(xs, us);
    let e = &c.e;
    let (lender, borrower, liq) = (Address::generate(e), Address::generate(e), Address::generate(e));
    fund(e, &c.xlm.asset, &lender, 100_000 * R7);
    fund(e, &c.usdc.asset, &borrower, 1_000 * R7);
    fund(e, &c.xlm.asset, &liq, 20_000 * R7);
    let s_xlm0 = bal(e, &c.xlm.asset, &c.xlm.sentora);
    let s_usdc0 = bal(e, &c.usdc.asset, &c.usdc.sentora);
    let next_id: u64 = e.as_contract(&c.usdc.sentora, || e.storage().instance().get(&SentoraKey::NextDepositId).unwrap());
    let tp0: u128 = e.as_contract(&c.usdc.sentora, || e.storage().instance().get(&SentoraKey::TotalPrincipal).unwrap());
    c.note("prices", json!({"xlm": format!("{:?}", c.price(&c.xlm.asset)), "usdc": format!("{:?}", c.price(&c.usdc.asset))}));

    // 1. lender deposits 100k XLM; 80 % lands in the real Sentora XLM vault.
    c.tick();
    c.deposit(&c.xlm, &lender, 100_000 * R7);
    c.res("deposit XLM 100k (strict auth; -> real Sentora when bound)");
    let moved = bal(e, &c.xlm.asset, &c.xlm.sentora) - s_xlm0;
    if xs {
        assert!(moved > 79_999 * R7 && moved <= 80_001 * R7, "XLM into Sentora {moved}");
        assert_eq!(c.s(&c.xlm).get_state().principal as i128, moved);
    } else {
        assert_eq!(moved, 0);
    }

    // 2. borrower deposits 1000 USDC collateral.
    c.tick();
    c.deposit(&c.usdc, &borrower, 1_000 * R7);
    c.res("deposit USDC 1000 (strict auth)");
    let tp1: u128 = e.as_contract(&c.usdc.sentora, || e.storage().instance().get(&SentoraKey::TotalPrincipal).unwrap());
    if us {
        assert_eq!(tp1 - tp0, c.s(&c.usdc).get_state().principal);
        assert!(bal(e, &c.usdc.asset, &c.usdc.sentora) - s_usdc0 > 799 * R7);
    }
    e.mock_all_auths();
    let _: () = call(e, &c.ctrl, "enter_market", vec![borrower.into_val(e), c.usdc.rv.into_val(e)]);
    let _: () = call(e, &c.ctrl, "enter_market", vec![borrower.into_val(e), c.xlm.rv.into_val(e)]);

    // 3. borrow XLM up to ~95 % of the limit; borrowable cash is the receipt's idle buffer only.
    let (px, _) = c.price(&c.xlm.asset);
    let (pxd, pud) = (c.price(&c.xlm.asset).1, c.price(&c.usdc.asset));
    let limit_usd = 1_000u128 * R7 as u128 * pud.0 / pud.1 * CF / 1_000_000; // USD in price scale
    let borrow = (limit_usd * pxd / px) * 95 / 100;
    c.note("borrowXlmRaw", json!(borrow.to_string()));
    c.tick();
    c.strict(&borrower, &c.xlm.rv, "borrow", (borrower.clone(), borrow).into_val(e), &[]);
    c.res("borrow XLM (strict auth; cash from receipt idle buffer)");
    assert_eq!(bal(e, &c.xlm.asset, &borrower) as u128, borrow);

    // 4. lender withdraws 50k XLM pTokens: beyond idle, exits through real Sentora.
    let before = bal(e, &c.xlm.asset, &c.xlm.sentora);
    c.tick();
    let pt: u128 = 50_000 * R7 as u128;
    c.strict(&lender, &c.xlm.rv, "withdraw", (lender.clone(), pt).into_val(e), &[]);
    c.res("withdraw XLM 50k pTokens (strict auth; Sentora exit when bound)");
    assert!(!xs || bal(e, &c.xlm.asset, &c.xlm.sentora) < before);

    // 5. XLM +20 % in a new Reflector round; keeper refresh (health cache > 5 min old after the round).
    c.post(120, 100, 0);
    c.refresh("after price round");
    e.mock_all_auths();
    let liq_res: Val = call(e, &c.ctrl, "account_liquidity", vec![borrower.into_val(e)]);
    let liq_res: soroban_sdk::Vec<u128> = soroban_sdk::Vec::try_from_val(e, &liq_res).unwrap_or_else(|_| soroban_sdk::Vec::new(e));
    c.note("borrowerLiquidityAfterShock(liq,shortfall,_)", json!(format!("{liq_res:?}")));

    // 6. liquidation: repay 40 % of the debt, seize USDC pTokens.
    let debt = c.debt(&c.xlm, &borrower);
    let repay = debt * 40 / 100;
    // repay_on_behalf pulls the repayment with transfer_from: the liquidator approves the XLM receipt first.
    let live = e.ledger().sequence() + 100_000;
    token::TokenClient::new(e, &c.xlm.asset).approve(&liq, &c.xlm.rv, &(20_000 * R7), &live);
    c.tick();
    let _: () = call(e, &c.ctrl, "liquidate", vec![borrower.into_val(e), c.xlm.rv.into_val(e), c.usdc.rv.into_val(e), repay.into_val(e), liq.into_val(e)]);
    c.res("liquidate (repay XLM, seize USDC pTokens; mock_all_auths liquidator)");
    let seized = c.pt(&c.usdc, &liq);
    assert!(seized > 0);
    assert!(c.debt(&c.xlm, &borrower) < debt);

    // 7. liquidator exits the seized pTokens; the receipt pulls USDC from real Sentora.
    let s_before = bal(e, &c.usdc.asset, &c.usdc.sentora);
    c.tick();
    c.strict(&liq, &c.usdc.rv, "withdraw", (liq.clone(), seized).into_val(e), &[]);
    c.res("liquidator withdraw seized USDC pTokens (strict auth; Sentora exit when bound)");
    let got = bal(e, &c.usdc.asset, &liq);
    assert!(got > 0 && (!us || bal(e, &c.usdc.asset, &c.usdc.sentora) < s_before));
    c.note("liquidatorUsdcRaw", json!(got.to_string()));
    c.note("seizedPtokens", json!(seized.to_string()));

    if !us {
        let out = json!({"rows": *c.rows.borrow(), "notes": *c.notes.borrow()});
        std::fs::write(format!("{}/.scratch/{out_name}", dir()), serde_json::to_string_pretty(&out).unwrap()).unwrap();
        return;
    }
    // 8. reward epoch for the strategy's first USDC deposit id on the REAL escrow (escrow admin mocked), claimed
    //    permissionlessly with no signer at all.
    let amount: u128 = 2 * R7 as u128;
    let escrow = a(e, ESCROW);
    let now = e.ledger().timestamp();
    let epoch = now as u32;
    let mut payload = Bytes::from_slice(e, &[0u8]);
    payload.append(&(c.usdc.sentora.clone(), epoch, next_id, c.usdc.strat.clone(), amount).to_xdr(e));
    let root: BytesN<32> = e.crypto().sha256(&payload).into();
    e.mock_all_auths();
    let eb = bal(e, &c.usdc.asset, &escrow);
    fund(e, &c.usdc.asset, &escrow, eb + amount as i128);
    let _: () = call(e, &escrow, "set_rewards_root", vec![c.usdc.asset.into_val(e), c.usdc.sentora.into_val(e), epoch.into_val(e),
        root.into_val(e), amount.into_val(e), 1u32.into_val(e), (now - 1).into_val(e)]);
    e.mock_auths(&[]);
    c.tick();
    let claimed = c.s(&c.usdc).claim_rewards(&epoch, &strat::ClaimParams { amount, deposit_id: next_id, leaf_index: 0, proof: Vec::new(e) });
    c.res("claim_rewards (permissionless, no signer; real escrow)");
    assert_eq!(claimed, amount);
    let st = c.s(&c.usdc).get_state();
    c.note("usdcStrategyAfterClaim", json!(format!("{st:?}")));

    let out = json!({"forkLedger": e.ledger().sequence(), "rows": *c.rows.borrow(), "notes": *c.notes.borrow(),
        "admin": c.admin.to_string().to_string()});
    let s = serde_json::to_string_pretty(&out).unwrap();
    std::fs::write(format!("{}/.scratch/{out_name}", dir()), &s).unwrap();
    for r in c.rows.borrow().iter() { assert!(r["networkFit"].as_bool().unwrap(), "network limit exceeded: {r}"); }
}
