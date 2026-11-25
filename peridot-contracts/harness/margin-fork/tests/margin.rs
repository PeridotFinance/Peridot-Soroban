//! Pooled margin v2 (WP5) on a read-only Mainnet fork: live controller f41e5260, live receipts 5630a701, live
//! Reflector and the REAL Aquarius XLM/USDC concentrated pool (CBBMQ...BUCV: its instance, code and every entry the RPC
//! footprints of read-only swap simulations touched), protocol-30 host (soroban-env-host 30.0.1).
//! The v2 margin WASM (WP6 build) is imported from the workspace build (sha256 asserted below) and registered fresh on the fork.
//!
//! Tests (each on a fresh Env from the same snapshot):
//! - `fork_fidelity_and_pool_auth`: fork estimate_swap == recorded Mainnet quotes; a native probe swaps on the real pool
//!   under ENFORCING auth with the root `transfer` entry, the nested `swap -> transfer` entry, both or none; WP6: the
//!   pool's estimate_swap_strict_receive round-trips at settle sizes and a real swap of its quoted input delivers the output.
//! - `pilot_book_backstop_exit_and_closes`: $750 seed, a second backer, long + short open concurrently at 3x and 5x,
//!   backstop request -> Cooldown -> 72 h warp -> BackstopHeadroom (book open) -> allowed exit, then close and
//!   close_with_repay on both sides, fees into backstop NAV.
//! - `gaps_liquidations_and_settles`: XLM -20 % / -40 % / +20 % / +40 % oracle gaps; solvent and underwater liquidations on
//!   both sides, bad debt -> backstop, chunked USDC settle and XLM settle (real-pool swap), claims; other positions' debt
//!   unchanged.
//! - `pooled_account_seizure_reconcile`: XLM spike until the live controller sees a shortfall on the pooled account, a
//!   plain cash-controller `liquidate` of it, `reconcile` -> Impaired, and what still works afterwards (claim, swap-close,
//!   close_with_repay, liquidate); WP6: self-sized XLM settles in a keeper loop, then the blocked exits go through.
//! - `heaviest_open_with_bad_debt_and_pending_exit`: WP6: opens in several orders with a backstop exit pending; with USDC +
//!   XLM bad debt unsettled opens are refused (#25); after the settles and after a 72 h warp they are measured again.
//! Measured opens/swap-closes/withdraws follow a +5 s tick so they pay their own receipt accrual.
//! User, backer, liquidator and keeper calls into margin run with explicit (non-mock-all) auth trees; only setup, the
//! Reflector `set_price` (oracle admin) and the external cash liquidator use mock_all_auths.
//! Gaps move the ORACLE only (Reflector round posted with XLM scaled, then controller cache_price); the real pool stays
//! at the fetch-time price, so no swap is attempted against an oracle that disagrees with the pool in the failing direction.
//! The 72 h warp moves the ledger timestamp only (sequence +120) so snapshot entries keep their liveUntil.
//!
//! Regenerate the fork (read-only RPC, no keys; git-ignored evidence, ~620 KB):
//!   cd tools && npm ci && node fetch-fork.cjs --out ../.scratch/fork.json
//! Build the margin WASM first (from peridot-contracts/): stellar contract build --package margin-controller
//! Run: cargo test -- --test-threads=1 --nocapture    (results: .scratch/wp5-<test>.json; FORK=<path> overrides the fork)
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke};
use soroban_sdk::xdr::{LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey, Limits, ReadXdr};
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Bytes, Env, IntoVal, Symbol, TryFromVal, Val, Vec};
use std::cell::RefCell;
type SVec<T> = Vec<T>;

mod mc {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/margin_controller.wasm");
}
const MARGIN_SHA: &str = "b1cb1cb21f02de1b1a9047d31fa60ffc066c2ddedc94c4dff2fcbdb0a7b1766f";

const CTRL: &str = "CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK";
const REFLECTOR: &str = "CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN";
const USDC_R: &str = "CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F";
const USDC_T: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const XLM_R: &str = "CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS";
const XLM_T: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const R7: u128 = 10_000_000;
const E6: u128 = 1_000_000;
const EQ: u128 = 249_000_000; // $24.90 equity per position (pilot cap $25, kept just under)
// Fees (SPEC §3, constructor defaults, set explicitly)
const BONUS_LIQ: u128 = 30_000;
const BONUS_BS: u128 = 10_000;
const CLOSE_FEE: u128 = 1_000;

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

// ---- auth probe (fidelity test): swaps `amount` of t_in on the real pool as itself, authorizing exactly the entries
// `mode` selects (bit 0: root `t_in.transfer(me, pool, amount)`; bit 1: `pool.swap(...)` with that transfer nested).
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

// ---------------------------------------------------------------- fork + generic helpers
fn a(e: &Env, s: &str) -> Address { Address::from_str(e, s) }
fn call<T: TryFromVal<Env, Val>>(e: &Env, c: &Address, f: &str, args: std::vec::Vec<Val>) -> T {
    e.invoke_contract(c, &Symbol::new(e, f), SVec::from_slice(e, &args))
}
fn dir() -> String { env!("CARGO_MANIFEST_DIR").to_string() }
fn fork_path() -> String { std::env::var("FORK").unwrap_or_else(|_| format!("{}/.scratch/fork.json", dir())) }
fn load_fork() -> (LedgerSnapshot, Value) {
    let path = fork_path();
    let j: Value = serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|_| panic!("fork snapshot {path} unreadable: run tools/fetch-fork.cjs"))).unwrap();
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
fn fund(e: &Env, tok: &Address, holder: &Address, amount: i128) {
    let k = SacKey::Balance(holder.clone());
    e.as_contract(tok, || {
        e.storage().persistent().set(&k, &SacBalance { amount, authorized: true, clawback: false });
        e.storage().persistent().extend_ttl(&k, 100, 2_000_000);
    });
}
fn bal(e: &Env, tok: &Address, who: &Address) -> i128 { token::TokenClient::new(e, tok).balance(who) }
fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
fn u(v: &Value) -> u128 { v.as_str().unwrap().parse().unwrap() }
fn usd(amount: u128, p: (u128, u128)) -> u128 { amount * p.0 / p.1 }
fn up(a: u128, b: u128, d: u128) -> u128 { (a * b).div_ceil(d) }

// Owner limits (SPEC §2): margin release <= 130 keys / 200M; liquidation flows <= 85 % of every live limit.
const NET_INSTR: i64 = 400_000_000;
const NET_MEM: i64 = 41_943_040;
const NET_WRITE_BYTES: u32 = 132_096;
const NET_EVENTS: u32 = 16_384;

struct Ctx {
    e: Env,
    fj: Value,
    margin: Address,
    admin: Address,
    ctrl: Address,
    xlm: Address,
    usdc: Address,
    xr: Address,
    ur: Address,
    pool: Address,
    base: std::vec::Vec<Option<i128>>, // Reflector price per `assets()` index at fetch time
    rows: RefCell<std::vec::Vec<Value>>,
    log: RefCell<serde_json::Map<String, Value>>,
}

impl Ctx {
    fn m(&self) -> mc::Client<'_> { mc::Client::new(&self.e, &self.margin) }
    fn note(&self, k: &str, v: Value) { self.log.borrow_mut().insert(k.to_string(), v); }
    fn price(&self, t: &Address) -> (u128, u128) {
        let p: Option<(u128, u128)> = call(&self.e, &self.ctrl, "get_price_usd", vec![t.into_val(&self.e)]);
        p.expect("price")
    }
    fn rate(&self, r: &Address) -> u128 { call(&self.e, r, "get_exchange_rate", vec![]) }
    fn pt_bal(&self, r: &Address, who: &Address) -> u128 { call(&self.e, r, "get_ptoken_balance", vec![who.into_val(&self.e)]) }
    fn receipt_of(&self, asset: &Address) -> Address { if *asset == self.xlm { self.xr.clone() } else { self.ur.clone() } }

    /// One measured row: the last top-level invocation's resources, checked against its owner limit class.
    fn res(&self, op: &str, class: &str) -> Value {
        let r = self.e.cost_estimate().resources();
        let keys = r.disk_read_entries + r.memory_read_entries;
        let (fit, limit) = if class == "release" {
            (keys <= 130 && r.instructions <= 200_000_000 && r.disk_read_entries <= 200 && r.write_entries <= 200
                && r.mem_bytes <= NET_MEM && r.write_bytes <= NET_WRITE_BYTES && r.contract_events_size_bytes <= NET_EVENTS,
                "<=130 keys, <=200M instr, network limits otherwise")
        } else {
            (keys <= 340 && r.disk_read_entries <= 170 && r.write_entries <= 170 && r.instructions <= 340_000_000
                && r.mem_bytes <= NET_MEM * 85 / 100 && r.write_bytes <= 112_281 && r.contract_events_size_bytes <= 13_926,
                "85 % of live limits")
        };
        let row = json!({"op": op, "class": class, "cpu": r.instructions, "memBytes": r.mem_bytes, "footprintKeys": keys,
            "diskReadEntries": r.disk_read_entries, "memoryReadEntries": r.memory_read_entries, "writeEntries": r.write_entries,
            "diskReadBytes": r.disk_read_bytes, "writeBytes": r.write_bytes, "eventsBytes": r.contract_events_size_bytes,
            "withinOwnerLimit": fit, "limit": limit, "networkFit": r.instructions <= NET_INSTR && keys <= 400
                && r.disk_read_entries <= 200 && r.write_entries <= 200 && r.mem_bytes <= NET_MEM});
        self.rows.borrow_mut().push(row.clone());
        row
    }

    /// Strict auth: exactly `who` authorizes margin.`f`(args) with the given direct sub-invocations.
    fn strict<T: TryFromVal<Env, Val>>(&self, who: &Address, f: &str, args: SVec<Val>, subs: &[(&Address, &str, SVec<Val>)]) -> Result<T, String> {
        let s: std::vec::Vec<MockAuthInvoke> = subs.iter().map(|(c, n, x)| MockAuthInvoke { contract: c, fn_name: n, args: x.clone(), sub_invokes: &[] }).collect();
        self.e.mock_auths(&[MockAuth { address: who, invoke: &MockAuthInvoke { contract: &self.margin, fn_name: f, args: args.clone(), sub_invokes: &s } }]);
        match self.e.try_invoke_contract::<T, soroban_sdk::Error>(&self.margin, &Symbol::new(&self.e, f), args) {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(x)) => Err(format!("{x:?}")),
            Err(x) => Err(format!("{x:?}")),
        }
    }

    /// Advance to the next 5-min Reflector round (+`extra` s), post every base price with XLM x num/den, refresh the
    /// controller PriceCache. Sequence moves by `seq` only.
    fn post(&self, num: i128, den: i128, extra: u64, seq: u32) {
        let e = &self.e;
        e.ledger().with_mut(|li| { li.timestamp = ((li.timestamp + extra) / 300 + 1) * 300; li.sequence_number += seq; });
        let r = a(e, REFLECTOR);
        let assets: SVec<RAsset> = call(e, &r, "assets", vec![]);
        let mut prices = SVec::<i128>::new(e);
        let mut mask = std::vec![0u8; (assets.len() as usize).div_ceil(8)];
        for (i, asset) in assets.iter().enumerate() {
            let Some(Some(p0)) = self.base.get(i) else { continue };
            let mut p = *p0;
            if let RAsset::Other(s) = &asset { if *s == Symbol::new(e, "XLM") { p = p * num / den; } }
            mask[i / 8] |= 1 << (i % 8);
            prices.push_back(p);
        }
        e.mock_all_auths();
        let ts: u64 = e.ledger().timestamp() * 1000;
        let upd = RPriceUpdate { mask: Bytes::from_slice(e, &mask), prices };
        let _: () = call(e, &r, "set_price", vec![upd.into_val(e), ts.into_val(e)]);
        let _: Option<(u128, u128)> = call(e, &self.ctrl, "cache_price", vec![self.xlm.into_val(e)]);
        let _: Option<(u128, u128)> = call(e, &self.ctrl, "cache_price", vec![self.usdc.into_val(e)]);
    }

    /// Ledger(asset) == receipt.get_ptoken_balance(margin) for both markets (read from margin instance storage).
    fn ledger_eq(&self, tag: &str) -> Value {
        let mut out = serde_json::Map::new();
        let mut ok = true;
        for (asset, r, name) in [(&self.usdc, &self.ur, "usdc"), (&self.xlm, &self.xr, "xlm")] {
            let l = self.ledger_of(asset);
            let b = self.pt_bal(r, &self.margin);
            ok &= l == b;
            out.insert(name.into(), json!({"ledger": l.to_string(), "balance": b.to_string()}));
        }
        out.insert("equal".into(), json!(ok));
        let v = Value::Object(out);
        self.note(&format!("ledger:{tag}"), v.clone());
        assert!(ok, "Ledger != receipt balance at {tag}: {v}");
        v
    }
    fn ledger_of(&self, asset: &Address) -> u128 {
        self.e.as_contract(&self.margin, || self.e.storage().instance().get(&mc::Key::Ledger(asset.clone())).unwrap_or(0))
    }
    fn nav(&self) -> i128 { self.m().backstop_nav().0 }
    /// Next ledger (+5 s, sequence +1) so the measured call pays its own receipt accrual: views called earlier at the same
    /// timestamp (get_position_debt etc.) already commit update_interest in the test host.
    fn tick(&self) { self.e.ledger().with_mut(|li| { li.timestamp += 5; li.sequence_number += 1; }); }

    fn open(&self, user: &Address, long: bool, lev: u128, eq: u128) -> Result<u64, String> {
        let (px, pu) = (self.price(&self.xlm), self.price(&self.usdc));
        let debt_usd = eq * (lev - E6) / E6 * 995 / 1000;
        let (held, debt, coll, borrow) = if long {
            (&self.xlm, &self.usdc, eq * px.1 / px.0, debt_usd * pu.1 / pu.0)
        } else {
            (&self.usdc, &self.xlm, eq * pu.1 / pu.0, debt_usd * px.1 / px.0)
        };
        let args: SVec<Val> = (user.clone(), held.clone(), debt.clone(), coll, borrow, 1u128, 10 * R7).into_val(&self.e);
        let sub: SVec<Val> = (user.clone(), self.margin.clone(), coll as i128).into_val(&self.e);
        self.strict::<u64>(user, "open", args, &[(held, "transfer", sub)])
    }
    fn close(&self, user: &Address, id: u64) -> Result<(), String> {
        self.strict::<()>(user, "close", (user.clone(), id, 1u128).into_val(&self.e), &[])
    }
    /// Exact strict-auth amount: long pays debt + close fee (usd7 at par) + funding in USDC; short pays the XLM debt.
    fn close_with_repay(&self, user: &Address, id: u64) -> Result<(), String> {
        let p = self.m().get_position(&id).unwrap();
        let d = self.m().get_position_debt(&id);
        let long = p.held_asset == self.xlm;
        let amt = if long { d + up(p.notional_usd, CLOSE_FEE, E6) + self.m().get_position_funding(&id) } else { d };
        let sub: SVec<Val> = (user.clone(), self.margin.clone(), amt as i128).into_val(&self.e);
        self.strict::<()>(user, "close_with_repay", (user.clone(), id).into_val(&self.e), &[(&p.debt_asset, "transfer", sub)])
    }
    /// Replicates liquidate's payment (contract math) for the exact strict-auth transfer. Returns (pay, underwater).
    fn liq_pay(&self, id: u64) -> (u128, bool) {
        let p = self.m().get_position(&id).unwrap();
        let long = p.held_asset == self.xlm;
        let (ph, pd) = (self.price(&p.held_asset), self.price(&p.debt_asset));
        let pu = if long { pd } else { ph };
        let d = self.m().get_position_debt(&id);
        let f = self.m().get_position_funding(&id);
        let rate_h = self.rate(&self.receipt_of(&p.held_asset));
        let h_usd = usd(p.held_ptokens * rate_h / E6, ph);
        let d_usd = usd(d, pd);
        let seize_req = d_usd * (E6 + BONUS_LIQ) / E6;
        if h_usd < seize_req {
            ((h_usd * E6 / (E6 + BONUS_LIQ) * pd.1 / pd.0).min(d), true)
        } else if long {
            let bb = up(d, BONUS_BS, E6);
            let f_eff = f.min((h_usd - seize_req) * pu.1 / pu.0);
            (d + bb + f_eff, false)
        } else {
            (d, false)
        }
    }
    fn liquidate(&self, liq: &Address, id: u64) -> Result<(u128, bool), String> {
        let p = self.m().get_position(&id).unwrap();
        let (pay, under) = self.liq_pay(id);
        let sub: SVec<Val> = (liq.clone(), self.margin.clone(), pay as i128).into_val(&self.e);
        self.strict::<()>(liq, "liquidate", (liq.clone(), id, pay).into_val(&self.e), &[(&p.debt_asset, "transfer", sub)])?;
        Ok((pay, under))
    }
    fn settle(&self, caller: &Address, asset: &Address, max_pt: u128) -> Result<u128, String> {
        self.strict::<u128>(caller, "settle_backstop_debt", (caller.clone(), asset.clone(), max_pt, 1u128).into_val(&self.e), &[])
    }
    fn claim(&self, owner: &Address, asset: &Address) -> Result<u128, String> {
        self.strict::<u128>(owner, "claim", (owner.clone(), asset.clone()).into_val(&self.e), &[])
    }
    fn reconcile(&self, caller: &Address) -> Result<bool, String> {
        self.strict::<bool>(caller, "reconcile", (caller.clone(),).into_val(&self.e), &[])
    }
    /// USDC -> USDC pTokens for `who` (setup helper, mocked auth), returns pTokens minted.
    fn mint_usdc_pt(&self, who: &Address, usd7: u128) -> u128 {
        let pu = self.price(&self.usdc);
        let amt = usd7 * pu.1 / pu.0;
        fund(&self.e, &self.usdc, who, amt as i128 + bal(&self.e, &self.usdc, who));
        self.e.mock_all_auths();
        let before = self.pt_bal(&self.ur, who);
        let _: () = call(&self.e, &self.ur, "deposit", vec![who.into_val(&self.e), amt.into_val(&self.e)]);
        self.pt_bal(&self.ur, who) - before
    }
    fn backstop_deposit(&self, backer: &Address, pt: u128) -> Result<u128, String> {
        let sub: SVec<Val> = (backer.clone(), self.margin.clone(), pt as i128).into_val(&self.e);
        self.strict::<u128>(backer, "backstop_deposit", (backer.clone(), pt, 0u128).into_val(&self.e), &[(&self.ur, "transfer", sub)])
    }
    fn debts(&self, ids: &[u64]) -> std::vec::Vec<u128> { ids.iter().map(|id| self.m().get_position_debt(id)).collect() }

    fn finish(&self, name: &str) -> Value {
        let hook: Option<Address> = call(&self.e, &self.xr, "get_margin_controller", vec![]);
        let hook_u: Option<Address> = call(&self.e, &self.ur, "get_margin_controller", vec![]);
        assert!(hook.is_none() && hook_u.is_none(), "receipt MarginController hook must stay unset");
        let out = json!({"test": name, "fork": fork_path(), "forkLedger": self.fj["ledger"], "forkFetchedAt": self.fj["fetchedAt"],
            "marginWasmSha256": MARGIN_SHA, "receiptHooksUnset": true, "log": Value::Object(self.log.borrow().clone()),
            "rows": Value::Array(self.rows.borrow().clone())});
        let s = serde_json::to_string_pretty(&out).unwrap();
        println!("{s}");
        std::fs::write(format!("{}/.scratch/wp5-{name}.json", dir()), &s).unwrap();
        out
    }
}

/// Fresh fork Env + v2 margin with pilot parameters (SPEC §3/§5) and the $750 USDC-pToken seed by the admin.
fn setup(name: &str) -> Ctx {
    let (snap, fj) = load_fork();
    let e = Env::from_ledger_snapshot(snap);
    e.cost_estimate().disable_resource_limits();
    #[allow(deprecated)]
    let pv = e.ledger().protocol_version();
    assert_eq!(pv, 30, "protocol-30 host and snapshot");
    assert_eq!(hex(&Sha256::digest(mc::WASM)), MARGIN_SHA, "{name}: rebuild margin-controller with stellar contract build");
    let (ctrl, xlm, usdc, xr, ur) = (a(&e, CTRL), a(&e, XLM_T), a(&e, USDC_T), a(&e, XLM_R), a(&e, USDC_R));
    let pool = a(&e, fj["pool"]["address"].as_str().unwrap());
    let admin = Address::generate(&e);
    e.mock_all_auths();
    let margin = e.register(mc::WASM, (admin.clone(), ctrl.clone(), admin.clone(), usdc.clone(), xlm.clone()));
    // base Reflector prices (fetch time), aligned with assets() order
    let r = a(&e, REFLECTOR);
    let assets: SVec<RAsset> = call(&e, &r, "assets", vec![]);
    let mut base = std::vec::Vec::new();
    for asset in assets.iter() {
        let pd: Option<RPriceData> = call(&e, &r, "lastprice", vec![asset.clone().into_val(&e)]);
        base.push(pd.map(|p| p.price));
    }
    let c = Ctx { e, fj, margin, admin, ctrl, xlm, usdc, xr, ur, pool, base, rows: RefCell::new(vec![]), log: RefCell::new(serde_json::Map::new()) };
    let (e, m, ad) = (&c.e, c.m(), &c.admin);
    m.set_market(ad, &c.xlm, &c.xr);
    m.set_market(ad, &c.usdc, &c.ur);
    m.enter_markets(ad, &SVec::from_array(e, [c.xlm.clone(), c.usdc.clone()]));
    m.set_pool(ad, &c.pool);
    m.set_fees(ad, &mc::Fees { open_fee: 1_000, close_fee: CLOSE_FEE, funding_apr: 20_000, bonus_liquidator: BONUS_LIQ, bonus_backstop: BONUS_BS });
    m.set_max_slippage(ad, &15_000);
    m.set_risk(ad, &c.xlm, &5_000_000);
    m.set_risk(ad, &c.usdc, &5_000_000);
    let caps = mc::Caps { per_position_equity_usd: 25 * R7, side_equity_usd: 100 * R7, side_oi_usd: 500 * R7, min_equity_usd: R7 };
    m.set_caps(ad, &c.xlm, &caps);
    m.set_caps(ad, &c.usdc, &caps);
    m.set_headroom(ad, &mc::Headroom { hf_target: 1_250_000, hf_stress: 1_100_000, stress: 200_000, close_reserve_usd: 0,
        coef_long: 3_000_000, coef_short: 2_100_000, coef_largest: 4_600_000 });
    // $750 USDC pTokens seed (fork-only funding of the admin), strict auth incl. the receipt pToken transfer
    let seed_pt = c.mint_usdc_pt(ad, 750 * R7);
    let shares = c.backstop_deposit(ad, seed_pt).expect("seed");
    c.res("backstop_deposit (admin seed $750, dead shares)", "release");
    assert_eq!(shares, seed_pt);
    e.mock_all_auths();
    m.set_opens_paused(ad, &false);
    c.note("seedPtokens", json!(seed_pt.to_string()));
    c.note("required_backstop_usd7_empty_book", json!(m.required_backstop().to_string()));
    c.note("prices_P0", json!({"xlm": format!("{:?}", c.price(&c.xlm)), "usdc": format!("{:?}", c.price(&c.usdc))}));
    c.ledger_eq("after seed");
    c
}

fn trader(c: &Ctx) -> Address {
    let u = Address::generate(&c.e);
    fund(&c.e, &c.xlm, &u, 100_000 * R7 as i128);
    fund(&c.e, &c.usdc, &u, 10_000 * R7 as i128);
    u
}

// ================================================================ tests
#[test]
fn fork_fidelity_and_pool_auth() {
    let (snap, fj) = load_fork();
    let e = Env::from_ledger_snapshot(snap);
    e.cost_estimate().disable_resource_limits();
    let (xlm, usdc) = (a(&e, XLM_T), a(&e, USDC_T));
    let pool = a(&e, fj["pool"]["address"].as_str().unwrap());
    let toks: SVec<Address> = call(&e, &pool, "get_tokens", vec![]);
    assert_eq!(toks, SVec::from_array(&e, [xlm.clone(), usdc.clone()]));
    let mut quote_checks = vec![];
    for q in fj["pool"]["after"]["quotes"].as_array().unwrap() {
        let (i, o, amt) = (q["in"].as_u64().unwrap() as u32, q["out"].as_u64().unwrap() as u32, u(&q["amount"]));
        let got: u128 = call(&e, &pool, "estimate_swap", vec![i.into_val(&e), o.into_val(&e), amt.into_val(&e)]);
        quote_checks.push(json!({"in": i, "amount": amt.to_string(), "mainnet": q["estimate"], "fork": got.to_string()}));
        assert_eq!(got, u(&q["estimate"]), "fork estimate_swap differs from Mainnet: snapshot is missing pool state");
    }
    let probe = e.register(AuthProbe, ());
    let amt: u128 = 15 * R7;
    let sim0 = &fj["pool"]["swapSims"][0];
    assert!(sim0["in"] == 1 && u(&sim0["amount"]) == amt);
    fund(&e, &usdc, &probe, 4 * amt as i128);
    e.set_auths(&[]);
    let mut matrix = serde_json::Map::new();
    let mut first_out = None;
    for (mode, name) in [(0u32, "none"), (2, "nested swap->transfer only"), (1, "root transfer only"), (3, "both (as margin v2 swap())")] {
        let r = e.try_invoke_contract::<u128, soroban_sdk::Error>(&probe, &Symbol::new(&e, "run"), (pool.clone(), usdc.clone(), 1u32, 0u32, amt, mode).into_val(&e));
        let v = match &r {
            Ok(Ok(out)) => { first_out.get_or_insert(*out); json!({"ok": true, "out": out.to_string()}) }
            Ok(Err(c)) => json!({"ok": false, "err": format!("{c:?}")}),
            Err(x) => json!({"ok": false, "err": format!("{x:?}")}),
        };
        matrix.insert(name.into(), v);
    }
    let ok = |k: &str| matrix[k]["ok"].as_bool().unwrap();
    assert_eq!(first_out.expect("no auth shape swapped"), u(&sim0["out_amount"]), "fork swap output != Mainnet simulated output");
    assert!(!ok("none") && ok("both (as margin v2 swap())") && ok("root transfer only") && !ok("nested swap->transfer only"));
    // WP6: settle_backstop_debt sizes its USDC input with the pool's estimate_swap_strict_receive. Round-trip it at settle
    // sizes (estimate_swap of the quoted input must deliver the asked output) and swap one size for real.
    let mut strict = vec![];
    for out in [10 * R7, 100 * R7, 1_000 * R7, 5_000 * R7] {
        let q: u128 = call(&e, &pool, "estimate_swap_strict_receive", vec![1u32.into_val(&e), 0u32.into_val(&e), out.into_val(&e)]);
        let back: u128 = call(&e, &pool, "estimate_swap", vec![1u32.into_val(&e), 0u32.into_val(&e), q.into_val(&e)]);
        strict.push(json!({"outXlm": out.to_string(), "usdcIn": q.to_string(), "estimateSwapOfIn": back.to_string()}));
        assert!(back >= out, "estimate_swap(strict_receive({out})) = {back} < {out}");
    }
    let want = 100 * R7;
    let q: u128 = call(&e, &pool, "estimate_swap_strict_receive", vec![1u32.into_val(&e), 0u32.into_val(&e), want.into_val(&e)]);
    fund(&e, &usdc, &probe, q as i128);
    let got: u128 = e.invoke_contract(&probe, &Symbol::new(&e, "run"), (pool.clone(), usdc.clone(), 1u32, 0u32, q, 3u32).into_val(&e));
    assert!(got >= want, "real swap of the strict-receive input delivered {got} < {want}");
    let out = json!({"test": "fidelity", "forkLedger": fj["ledger"], "forkFetchedAt": fj["fetchedAt"], "quoteChecks": quote_checks,
        "contractCallerAuthMatrix": Value::Object(matrix), "mainnetSimOut": sim0["out_amount"], "strictReceiveRoundTrip": strict,
        "strictReceiveRealSwap": {"wantXlm": want.to_string(), "usdcIn": q.to_string(), "gotXlm": got.to_string()}});
    let s = serde_json::to_string_pretty(&out).unwrap();
    println!("{s}");
    std::fs::write(format!("{}/.scratch/wp5-fidelity.json", dir()), s).unwrap();
}

#[test]
fn pilot_book_backstop_exit_and_closes() {
    let c = setup("pilot");
    let m = c.m();
    let (u, x) = (c.usdc.clone(), c.xlm.clone());
    // second backer (non-seed deposit path)
    let backer = Address::generate(&c.e);
    let b_pt = c.mint_usdc_pt(&backer, 50 * R7);
    let b_shares = c.backstop_deposit(&backer, b_pt).expect("backer deposit");
    c.res("backstop_deposit (second backer $50)", "release");

    // ---- long + short at 3x and 5x, concurrently open
    let t = trader(&c);
    let mut ids = vec![];
    for (long, lev, name) in [(true, 5_000_000u128, "open long 5x"), (false, 5_000_000, "open short 5x"), (true, 3_000_000, "open long 3x"), (false, 3_000_000, "open short 3x")] {
        let nav0 = c.nav();
        c.tick();
        let id = c.open(&t, long, lev, EQ).unwrap_or_else(|err| panic!("{name}: {err}"));
        c.res(&format!("{name} ($24.90 equity, real-pool swap)"), "release");
        let p = m.get_position(&id).unwrap();
        let ltv = m.get_position_ltv(&id);
        let nav1 = c.nav();
        assert!(nav1 > nav0, "{name}: open fee must reach backstop NAV");
        c.note(name, json!({"id": id, "equityUsd7": p.equity_usd.to_string(), "notionalUsd7": p.notional_usd.to_string(),
            "leverage": format!("{:.3}", p.notional_usd as f64 / p.equity_usd as f64), "ltvOpen": ltv.to_string(), "liqLtv": p.liq_ltv.to_string(),
            "debt": m.get_position_debt(&id).to_string(), "feeToBackstopPt": (nav1 - nav0).to_string()}));
        ids.push(id);
    }
    assert!(ids.iter().all(|id| m.get_position(id).unwrap().open));
    let (hf, hfd, hfu, free) = m.pooled_health();
    c.note("pooledHealthAfter4Opens", json!({"hf": hf.to_string(), "hfDown": hfd.to_string(), "hfUp": hfu.to_string(), "freeUsd7": free.to_string(),
        "required": m.required_backstop().to_string()}));
    c.ledger_eq("after 4 opens");

    // ---- backstop exits: request (backer all, admin sized to leave pooled HF ~1.10) -> Cooldown -> 72 h -> blocked/allowed
    c.strict::<()>(&backer, "backstop_request_withdraw", (backer.clone(), b_shares).into_val(&c.e), &[]).expect("request");
    c.res("backstop_request_withdraw", "release");
    let (_, (coll, debt)): (SVec<(Address, u128, u128, u128, u128)>, (u128, u128)) = call(&c.e, &c.ctrl, "portfolio", vec![c.margin.into_val(&c.e)]);
    let w_usd = (coll - debt * 110 / 100) * 4 / 3; // CF-weighted -> USD at USDC CF 0.75
    let pu = c.price(&u);
    let w_pt = w_usd * pu.1 / pu.0 * E6 / c.rate(&c.ur);
    let (nav, shares) = m.backstop_nav();
    let admin_req = (w_pt * shares / nav as u128).min(m.get_backer(&c.admin));
    c.strict::<()>(&c.admin, "backstop_request_withdraw", (c.admin.clone(), admin_req).into_val(&c.e), &[]).expect("admin request");
    let early = c.strict::<u128>(&backer, "backstop_withdraw", (backer.clone(), 0u128).into_val(&c.e), &[]);
    assert!(early.as_ref().is_err_and(|s| s.contains("#26")), "withdraw before 72 h must fail Cooldown: {early:?}");
    let req = m.get_withdraw_req(&backer).unwrap();
    c.e.ledger().with_mut(|li| li.timestamp = req.ready_at + 1);
    c.post(1, 1, 0, 120); // fresh round at P0, timestamp-only warp (sequence +120)
    assert!(c.e.ledger().timestamp() >= req.ready_at && c.e.ledger().timestamp() <= req.expires_at);
    let blocked = c.strict::<u128>(&c.admin, "backstop_withdraw", (c.admin.clone(), 0u128).into_val(&c.e), &[]);
    c.note("backstopWithdrawBlocked", json!({"adminRequestShares": admin_req.to_string(), "result": format!("{blocked:?}")}));
    assert!(blocked.as_ref().is_err_and(|s| s.contains("#22")), "admin exit with the book open must fail BackstopHeadroom: {blocked:?}");
    c.strict::<()>(&c.admin, "backstop_cancel_withdraw", (c.admin.clone(),).into_val(&c.e), &[]).expect("cancel");
    c.tick();
    let got = c.strict::<u128>(&backer, "backstop_withdraw", (backer.clone(), b_pt).into_val(&c.e), &[]).expect("allowed exit");
    c.res("backstop_withdraw (after 72 h, book open, pool check)", "release");
    assert!(got >= b_pt, "backer exits at NAV incl. its share of open fees");
    c.note("backstopWithdrawAllowed", json!({"depositedPt": b_pt.to_string(), "receivedPt": got.to_string(), "cooldownResult": format!("{early:?}")}));
    c.ledger_eq("after backstop exit");

    // ---- closes after 72 h (interest + funding accrued): close long/short (3x), close_with_repay long/short (5x)
    let (l5, s5, l3, s3) = (ids[0], ids[1], ids[2], ids[3]);
    for (id, how, name) in [(l3, "close", "close long 3x"), (s3, "close", "close short 3x"), (l5, "cwr", "close_with_repay long 5x"), (s5, "cwr", "close_with_repay short 5x")] {
        let nav0 = c.nav();
        let f = m.get_position_funding(&id);
        let (xu, xx) = (bal(&c.e, &u, &t), bal(&c.e, &x, &t));
        let (pu0, px0) = (c.pt_bal(&c.ur, &t), c.pt_bal(&c.xr, &t));
        if how == "close" { c.tick(); }
        let r = if how == "close" { c.close(&t, id) } else { c.close_with_repay(&t, id) };
        r.unwrap_or_else(|err| panic!("{name}: {err}"));
        c.res(&format!("{name} (72 h funding)"), "release");
        let nav1 = c.nav();
        assert!(nav1 > nav0, "{name}: close fee + funding must reach backstop NAV");
        assert!(!m.get_position(&id).unwrap().open);
        c.note(name, json!({"fundingUsdc": f.to_string(), "feeAndFundingToBackstopPt": (nav1 - nav0).to_string(),
            "traderUsdcDelta": (bal(&c.e, &u, &t) - xu).to_string(), "traderXlmDelta": (bal(&c.e, &x, &t) - xx).to_string(),
            "traderUsdcPtDelta": (c.pt_bal(&c.ur, &t) as i128 - pu0 as i128).to_string(), "traderXlmPtDelta": (c.pt_bal(&c.xr, &t) as i128 - px0 as i128).to_string()}));
    }
    let pooled_u: u128 = call(&c.e, &c.ur, "get_user_borrow_balance", vec![c.margin.into_val(&c.e)]);
    let pooled_x: u128 = call(&c.e, &c.xr, "get_user_borrow_balance", vec![c.margin.into_val(&c.e)]);
    c.note("pooledDebtAfterAllCloses", json!({"usdc": pooled_u.to_string(), "xlm": pooled_x.to_string()}));
    assert!(pooled_u == 0 && pooled_x == 0, "all position debt repaid");
    c.ledger_eq("after closes");
    c.note("backstopFinalNavPt", json!(c.nav().to_string()));
    c.finish("pilot");
}

#[test]
fn gaps_liquidations_and_settles() {
    let c = setup("gaps");
    let m = c.m();
    let (u, x) = (c.usdc.clone(), c.xlm.clone());
    let t = trader(&c);
    let keeper = Address::generate(&c.e);
    fund(&c.e, &x, &keeper, 1_000_000 * R7 as i128);
    fund(&c.e, &u, &keeper, 100_000 * R7 as i128);
    let o = |long, lev, n: &str| c.open(&t, long, lev, EQ).unwrap_or_else(|err| panic!("{n}: {err}"));
    let (l5, l3, l2) = (o(true, 5_000_000, "L5"), o(true, 3_000_000, "L3"), o(true, 2_000_000, "L2"));
    let (s5a, s5b, s3) = (o(false, 5_000_000, "S5a"), o(false, 5_000_000, "S5b"), o(false, 3_000_000, "S3"));
    for (id, n) in [(l5, "L5"), (l3, "L3"), (l2, "L2"), (s5a, "S5a"), (s5b, "S5b"), (s3, "S3")] {
        let p = m.get_position(&id).unwrap();
        c.note(&format!("open {n}"), json!({"id": id, "liqLtv": p.liq_ltv.to_string(), "ltv": m.get_position_ltv(&id).to_string(),
            "leverage": format!("{:.3}", p.notional_usd as f64 / p.equity_usd as f64)}));
    }
    c.ledger_eq("after 6 opens");
    let mut liq_log = vec![];
    let bs_usdc = || m.get_bs_debt_shares(&u);
    let bs_xlm = || m.get_bs_debt_shares(&x);
    let st = |id| (m.is_liquidatable(&id), m.get_position_ltv(&id));

    // ---- XLM -20 % (1 h later): L5 underwater -> USDC bad debt; others unchanged; chunked USDC settle
    c.post(80, 100, 3_600, 720);
    c.note("-20%: liquidatable/ltv", json!({"L5": format!("{:?}", st(l5)), "L3": format!("{:?}", st(l3)), "L2": format!("{:?}", st(l2))}));
    let others = [l3, l2];
    let d0 = c.debts(&others);
    let nav0 = c.nav();
    let (pay, under) = c.liquidate(&keeper, l5).expect("liquidate L5 at -20%");
    c.res("liquidate long 5x UNDERWATER at XLM -20% (bad debt -> backstop, Impaired check)", "liquidation");
    assert!(under && bs_usdc() > 0, "L5 at -20 % must be underwater");
    let d1 = c.debts(&others);
    for (a0, a1) in d0.iter().zip(&d1) { assert!(a0.abs_diff(*a1) <= 1, "other positions' debt changed after bad debt: {d0:?} -> {d1:?}"); }
    let nav1 = c.nav();
    liq_log.push(json!({"id": l5, "gap": "-20%", "underwater": under, "paid": pay.to_string(), "navBefore": nav0.to_string(), "navAfter": nav1.to_string(),
        "bsDebtSharesUsdc": bs_usdc().to_string(), "otherDebtsBefore": format!("{d0:?}"), "otherDebtsAfter": format!("{d1:?}"), "impaired": m.is_impaired()}));
    // chunk 1: small max_ptokens -> partial; chunk 2: the rest
    let s_before = bs_usdc();
    let r1 = c.settle(&keeper, &u, 5_000_000).expect("settle USDC chunk 1");
    c.res("settle_backstop_debt USDC (chunk, max_ptokens 0.5 pT)", "liquidation");
    let s_mid = bs_usdc();
    assert!(s_mid > 0 && s_mid < s_before, "chunked settle burns part of the shares");
    let r2 = c.settle(&keeper, &u, u128::MAX).expect("settle USDC rest");
    c.res("settle_backstop_debt USDC (full, no swap)", "liquidation");
    assert_eq!(bs_usdc(), 0);
    let d2 = c.debts(&others);
    for (a0, a2) in d0.iter().zip(&d2) { assert!(a0.abs_diff(*a2) <= 1, "other positions' debt changed after settle: {d0:?} -> {d2:?}"); }
    c.note("settleUsdc", json!({"chunk1Repaid": r1.to_string(), "sharesBefore": s_before.to_string(), "sharesMid": s_mid.to_string(),
        "chunk2Repaid": r2.to_string(), "otherDebtsAfterSettle": format!("{d2:?}"), "navAfter": c.nav().to_string()}));
    c.ledger_eq("after -20% liquidation + settle");

    // ---- XLM -40 %: L3 underwater, L2 solvent (owner remainder -> claim)
    c.post(60, 100, 0, 60);
    c.note("-40%: liquidatable/ltv", json!({"L3": format!("{:?}", st(l3)), "L2": format!("{:?}", st(l2))}));
    let (hf, hfd, _, _) = m.pooled_health();
    c.note("-40%: pooledHealth", json!({"hf": hf.to_string(), "hfDown": hfd.to_string()}));
    let d0 = c.debts(&[l2]);
    let (pay, under) = c.liquidate(&keeper, l3).expect("liquidate L3 at -40%");
    c.res("liquidate long 3x UNDERWATER at XLM -40%", "liquidation");
    assert!(under);
    let d1 = c.debts(&[l2]);
    assert!(d0[0].abs_diff(d1[0]) <= 1, "L2 debt changed by L3's bad debt");
    liq_log.push(json!({"id": l3, "gap": "-40%", "underwater": under, "paid": pay.to_string(), "bsDebtSharesUsdc": bs_usdc().to_string(),
        "L2DebtBefore": d0[0].to_string(), "L2DebtAfter": d1[0].to_string()}));
    let nav0 = c.nav();
    let (pay, under) = c.liquidate(&keeper, l2).expect("liquidate L2 at -40%");
    c.res("liquidate long 2x SOLVENT at XLM -40% (USDC bad debt outstanding)", "liquidation");
    assert!(!under);
    let claim_x = m.get_claim(&t, &x);
    liq_log.push(json!({"id": l2, "gap": "-40%", "underwater": under, "paid": pay.to_string(), "backstopNavDeltaPt": (c.nav() - nav0).to_string(),
        "ownerClaimXlmPt": claim_x.to_string(), "keeperXlmPt": c.pt_bal(&c.xr, &keeper).to_string()}));
    let got = c.claim(&t, &x).expect("claim XLM");
    c.res("claim XLM pTokens (owner remainder)", "liquidation");
    assert!(got == claim_x && got > 0);
    let r3 = c.settle(&keeper, &u, u128::MAX).expect("settle USDC (L3)");
    assert_eq!(bs_usdc(), 0);
    c.note("settleUsdcL3", json!({"repaid": r3.to_string()}));
    c.ledger_eq("after -40%");

    // ---- XLM +20 % (vs P0): S5a liquidation; branch asserted, not assumed
    c.post(120, 100, 0, 60);
    c.note("+20%: liquidatable/ltv", json!({"S5a": format!("{:?}", st(s5a)), "S5b": format!("{:?}", st(s5b)), "S3": format!("{:?}", st(s3))}));
    let nav0 = c.nav();
    let xb0 = bs_xlm();
    let (pay, under) = c.liquidate(&keeper, s5a).expect("liquidate S5a at +20%");
    c.res(&format!("liquidate short 5x {} at XLM +20%", if under { "UNDERWATER" } else { "SOLVENT" }), "liquidation");
    assert_eq!(under, bs_xlm() > xb0);
    let claim_u = m.get_claim(&t, &u);
    liq_log.push(json!({"id": s5a, "gap": "+20%", "underwater": under, "paid": pay.to_string(), "backstopNavDeltaPt": (c.nav() - nav0).to_string(),
        "ownerClaimUsdcPt": claim_u.to_string()}));
    if claim_u > 0 {
        let got = c.claim(&t, &u).expect("claim USDC");
        c.res("claim USDC pTokens (owner remainder)", "liquidation");
        assert_eq!(got, claim_u);
    }
    if under {
        c.settle(&keeper, &x, u128::MAX).expect("settle XLM (S5a)");
    }
    c.ledger_eq("after +20%");

    // ---- XLM +40 %: S5b underwater -> XLM bad debt; S3 unchanged; XLM settle swaps backstop USDC on the real pool;
    //      backstop deposits frozen meanwhile.
    c.post(140, 100, 0, 60);
    c.note("+40%: liquidatable/ltv", json!({"S5b": format!("{:?}", st(s5b)), "S3": format!("{:?}", st(s3))}));
    let d0 = c.debts(&[s3]);
    let nav0 = c.nav();
    let (pay, under) = c.liquidate(&keeper, s5b).expect("liquidate S5b at +40%");
    c.res("liquidate short 5x UNDERWATER at XLM +40% (XLM bad debt, oracle NAV)", "liquidation");
    assert!(under && bs_xlm() > 0);
    let d1 = c.debts(&[s3]);
    assert!(d0[0].abs_diff(d1[0]) <= 1, "S3 debt changed by S5b's bad debt");
    liq_log.push(json!({"id": s5b, "gap": "+40%", "underwater": under, "paid": pay.to_string(), "navBefore": nav0.to_string(), "navAfter": c.nav().to_string(),
        "bsDebtSharesXlm": bs_xlm().to_string(), "S3DebtBefore": d0[0].to_string(), "S3DebtAfter": d1[0].to_string(), "impaired": m.is_impaired()}));
    let fz_pt = c.mint_usdc_pt(&keeper, 5 * R7);
    let frozen = c.backstop_deposit(&keeper, fz_pt);
    assert!(frozen.as_ref().is_err_and(|s| s.contains("#25")), "deposit must be frozen while XLM bad debt is outstanding: {frozen:?}");
    let xb = bs_xlm();
    let x_idle0 = bal(&c.e, &x, &c.margin);
    let repaid = c.settle(&keeper, &x, u128::MAX).expect("settle XLM");
    c.res("settle_backstop_debt XLM (withdraw USDC, real-pool swap USDC->XLM, repay)", "liquidation");
    assert_eq!(bs_xlm(), 0);
    let d2 = c.debts(&[s3]);
    assert!(d0[0].abs_diff(d2[0]) <= 1, "S3 debt changed by XLM settle");
    let idle = bal(&c.e, &x, &c.margin) - x_idle0;
    let bs_xpt = m.get_bs_xlm_ptokens();
    assert!(idle <= 2, "WP6: no idle XLM after the settle ({idle} stroops)");
    assert!(bs_xpt > 0 && (bs_xpt as u128) * c.rate(&c.xr) / E6 <= repaid / 500 + 2 * R7, "surplus kept as backstop XLM pT, about the 0.05 % buffer: {bs_xpt}");
    c.note("settleXlm", json!({"sharesBefore": xb.to_string(), "repaidXlm": repaid.to_string(), "idleXlmAfter": idle.to_string(), "bsXlmPtokens": bs_xpt.to_string(),
        "S3DebtAfter": d2[0].to_string(), "navAfter": c.nav().to_string(), "frozenDepositResult": format!("{frozen:?}")}));
    // S3 at +40 %
    let (pay, under) = c.liquidate(&keeper, s3).expect("liquidate S3 at +40%");
    c.res(&format!("liquidate short 3x {} at XLM +40%", if under { "UNDERWATER" } else { "SOLVENT" }), "liquidation");
    liq_log.push(json!({"id": s3, "gap": "+40%", "underwater": under, "paid": pay.to_string()}));
    let claim_s3 = m.get_claim(&t, &u);
    if claim_s3 > 0 {
        let got = c.claim(&t, &u).expect("claim USDC S3");
        c.res("claim USDC pTokens (owner remainder)", "liquidation");
        assert_eq!(got, claim_s3);
    }
    c.note("claimUsdcS3", json!(claim_s3.to_string()));
    if under { c.settle(&keeper, &x, u128::MAX).expect("settle XLM (S3)"); }
    c.note("liquidations", Value::Array(liq_log));
    let rec = c.reconcile(&keeper).expect("reconcile");
    c.res("reconcile (no deficit, returns false)", "liquidation");
    assert!(!rec && !m.is_impaired());
    let pooled_u: u128 = call(&c.e, &c.ur, "get_user_borrow_balance", vec![c.margin.into_val(&c.e)]);
    let pooled_x: u128 = call(&c.e, &c.xr, "get_user_borrow_balance", vec![c.margin.into_val(&c.e)]);
    c.note("end", json!({"pooledDebtUsdc": pooled_u.to_string(), "pooledDebtXlm": pooled_x.to_string(), "nav": c.nav().to_string(),
        "backstop": format!("{:?}", m.get_backstop())}));
    assert!(pooled_u == 0 && pooled_x == 0, "every position closed and backstop debt settled");
    c.ledger_eq("end");
    c.finish("gaps");
}

#[test]
fn pooled_account_seizure_reconcile() {
    let c = setup("seizure");
    let m = c.m();
    let (u, x) = (c.usdc.clone(), c.xlm.clone());
    let t = trader(&c);
    let s5 = c.open(&t, false, 5_000_000, EQ).expect("S5");
    let s3 = c.open(&t, false, 3_000_000, EQ).expect("S3");
    let l3 = c.open(&t, true, 3_000_000, EQ).expect("L3");
    let l2 = c.open(&t, true, 2_000_000, EQ).expect("L2");
    // L2 liquidated solvent at XLM -40 %: its owner remainder stays as an unclaimed XLM-pT claim through the spike
    let keeper = Address::generate(&c.e);
    fund(&c.e, &x, &keeper, 1_000_000 * R7 as i128);
    fund(&c.e, &u, &keeper, 100_000 * R7 as i128);
    c.post(60, 100, 0, 60);
    assert!(!c.liquidate(&keeper, l2).expect("L2 -40%").1);
    assert!(m.get_claim(&t, &x) > 0);
    c.ledger_eq("before spike");
    // keeper offline: XLM spikes until the live controller sees a shortfall on the pooled account
    let mut k_hit = 0;
    for k in [300i128, 500, 700, 900, 1200] {
        c.post(k, 100, 0, 60);
        let (_, short): (u128, u128) = call(&c.e, &c.ctrl, "account_liquidity", vec![c.margin.into_val(&c.e)]);
        if short > 0 { k_hit = k; break; }
    }
    assert!(k_hit > 0, "no pooled shortfall reached");
    let (liq0, short0): (u128, u128) = call(&c.e, &c.ctrl, "account_liquidity", vec![c.margin.into_val(&c.e)]);
    let (hf, _, _, _) = m.pooled_health();
    c.note("spike", json!({"xlmMultiplePct": k_hit, "pooledHf": hf.to_string(), "liquidity": liq0.to_string(), "shortfall": short0.to_string()}));
    // plain cash-controller liquidation of the pooled account (repay XLM debt, seize USDC pTokens)
    let ext = Address::generate(&c.e);
    fund(&c.e, &x, &ext, 1_000_000 * R7 as i128);
    let pooled_x: u128 = call(&c.e, &c.xr, "get_user_borrow_balance", vec![c.margin.into_val(&c.e)]);
    let repay = pooled_x / 4;
    let ur_before = c.pt_bal(&c.ur, &c.margin);
    c.e.mock_all_auths();
    // the receipt pulls the repayment with transfer_from (SAC AllowanceError #9 without this approval)
    let live_until = c.e.ledger().sequence() + 10_000;
    token::TokenClient::new(&c.e, &x).approve(&ext, &c.xr, &(1_000_000 * R7 as i128), &live_until);
    let r = c.e.try_invoke_contract::<(), soroban_sdk::Error>(&c.ctrl, &Symbol::new(&c.e, "liquidate"),
        (c.margin.clone(), c.xr.clone(), c.ur.clone(), repay, ext.clone()).into_val(&c.e));
    let seized = ur_before - c.pt_bal(&c.ur, &c.margin);
    c.note("cashLiquidation", json!({"result": format!("{r:?}"), "repayXlm": repay.to_string(), "seizedUsdcPt": seized.to_string(),
        "extUsdcPt": c.pt_bal(&c.ur, &ext).to_string()}));
    assert!(matches!(r, Ok(Ok(()))) && seized > 0, "cash-controller liquidation of the pooled account failed: {r:?}");
    let newly = c.reconcile(&keeper).expect("reconcile");
    c.res("reconcile (deficit -> newly Impaired, opens paused, event)", "liquidation");
    assert!(newly && m.is_impaired() && m.opens_paused());
    let again = c.reconcile(&keeper).expect("reconcile again");
    assert!(!again);
    let (nav_r, _) = m.backstop_nav();
    let (hf_r, _, _, _) = m.pooled_health();
    c.note("atReconcile", json!({"ledgerUsdc": c.ledger_of(&u).to_string(), "balanceUsdc": c.pt_bal(&c.ur, &c.margin).to_string(),
        "ledgerXlm": c.ledger_of(&x).to_string(), "balanceXlm": c.pt_bal(&c.xr, &c.margin).to_string(), "backstopNavPt": nav_r.to_string(), "pooledHf": hf_r.to_string()}));
    // ---- what still works with pooled HF < 1 (right after the seizure; INTERFACE known gaps)
    let open_try = c.open(&t, true, 2_000_000, 50_000_000);
    // Traced (WP6): claim traps in the XLM receipt `transfer` (margin -> owner) and swap-close in the XLM receipt `withdraw`
    // of the pTokens to sell (before any swap or repay), both in the receipt's controller liquidity check
    // (get_collateral_excl_usd + get_borrows_excl, XLM price and CF -> UnreachableCodeReached).
    let claim_hf_lt1 = c.claim(&t, &x); // L2's solvent-liquidation remainder (XLM pT)
    c.tick();
    let close_l3_hf_lt1 = c.close(&t, l3); // swap-close long: withdraws XLM pT before the repay
    let cwr_l3 = c.close_with_repay(&t, l3);
    fund(&c.e, &x, &keeper, 1_000_000 * R7 as i128);
    let liq_s5 = c.liquidate(&keeper, s5); // pays in first: works
    let (_, short_s5): (u128, u128) = call(&c.e, &c.ctrl, "account_liquidity", vec![c.margin.into_val(&c.e)]);
    let view_s5 = m.settle_max_ptokens(&x);
    c.tick();
    let settle_s5 = c.settle(&keeper, &x, u128::MAX); // XLM bad debt now; PooledShortfall (#35) if still short
    if settle_s5.is_ok() { c.res("settle_backstop_debt XLM (self-sized, right after S5 liquidation, Impaired)", "liquidation"); }
    let cwr_s3 = c.close_with_repay(&t, s3);
    let (_, short_mid): (u128, u128) = call(&c.e, &c.ctrl, "account_liquidity", vec![c.margin.into_val(&c.e)]);
    let (nav_mid, _) = m.backstop_nav();
    let (hf_mid, _, _, _) = m.pooled_health();
    let bs_x0 = m.get_bs_debt_shares(&x);
    // L3's close_with_repay failed above: traced (WP6) to the XLM receipt `transfer` of L3's XLM pTokens to the owner,
    // AFTER the repay; the receipt's controller liquidity check (get_collateral_excl_usd + get_borrows_excl, XLM price, CF)
    // traps UnreachableCodeReached because the pooled account's other debt needs that collateral. Retried here once the
    // keeper has liquidated S5, settled (self-sized) and S3 has repaid.
    let cwr_l3_retry = c.close_with_repay(&t, l3);
    // ---- WP6 keeper loop: self-sized XLM settles (each withdraws only what the pooled liquidity allows) until zero
    let mut chunks = vec![];
    let mut first = true;
    while m.get_bs_debt_shares(&x) > 0 {
        let view = m.settle_max_ptokens(&x);
        c.tick();
        let r = c.settle(&keeper, &x, u128::MAX).unwrap_or_else(|err| panic!("self-sized XLM settle {}: {err}", chunks.len()));
        if first { c.res("settle_backstop_debt XLM (self-sized keeper loop, Impaired, book open)", "liquidation"); first = false; }
        chunks.push(json!({"viewPt": view.to_string(), "repaidXlm": r.to_string(), "sharesLeft": m.get_bs_debt_shares(&x).to_string()}));
        assert!(chunks.len() < 40, "self-sized settles do not converge: {chunks:?}");
    }
    let view_after: u128 = m.settle_max_ptokens(&x);
    // after the bad debt is settled the trapped exits go through
    let cwr_l3_after = if cwr_l3_retry.is_ok() { Ok(()) } else { c.close_with_repay(&t, l3) };
    let (_, short1): (u128, u128) = call(&c.e, &c.ctrl, "account_liquidity", vec![c.margin.into_val(&c.e)]);
    let claim_after = if claim_hf_lt1.is_ok() { Ok(0) } else { c.claim(&t, &x) };
    c.note("afterImpaired", json!({"ledgerUsdc": c.ledger_of(&u).to_string(), "balanceUsdc": c.pt_bal(&c.ur, &c.margin).to_string(),
        "reconcileAgain": again, "openWhileImpaired": format!("{open_try:?}"), "claimXlmHfLt1": format!("{claim_hf_lt1:?}"),
        "swapCloseLongL3HfLt1": format!("{close_l3_hf_lt1:?}"), "closeWithRepayShortS3": format!("{cwr_s3:?}"),
        "closeWithRepayLongL3": format!("{cwr_l3:?}"), "liquidateS5": format!("{liq_s5:?}"), "shortfallAfterS5Liq": short_s5.to_string(),
        "settleMaxPtokensAfterS5Liq": view_s5.to_string(), "settleXlmAfterS5Liq": format!("{settle_s5:?}"), "shortfallAfterS3S5": short_mid.to_string(),
        "navAfterS5LiqPt": nav_mid.to_string(), "pooledHfBeforeRetry": hf_mid.to_string(), "bsDebtSharesXlmBeforeSettle": bs_x0.to_string(),
        "closeWithRepayLongL3Retry": format!("{cwr_l3_retry:?}"), "selfSizedSettles": chunks, "settleMaxPtokensAfter": view_after.to_string(),
        "closeWithRepayLongL3AfterSettles": format!("{cwr_l3_after:?}"), "claimXlmAfterSettles": format!("{claim_after:?}"),
        "bsXlmPtokens": m.get_bs_xlm_ptokens().to_string(), "shortfallAfter": short1.to_string()}));
    assert!(open_try.is_err_and(|s| s.contains("#24")), "open must fail Impaired");
    assert!(cwr_s3.is_ok() && liq_s5.is_ok(), "pay-in-first exits work under pooled HF < 1");
    assert!(cwr_l3_after.is_ok(), "L3 close_with_repay after the self-sized settles: {cwr_l3_after:?}");
    assert!(claim_hf_lt1.is_ok() || claim_after.is_ok(), "claim after the settles");
    c.finish("seizure");
}

/// WP6 open states. Opens are measured in several orders, all with a backstop exit pending: first on a side, after the
/// opposite side, second on the same side; then, with USDC and XLM bad debt unsettled, opens are REFUSED (#25); after
/// the keeper's settles (backstop XLM pTokens held), and again after a 72 h warp. pool_check reads the NAV once.
#[test]
fn heaviest_open_with_bad_debt_and_pending_exit() {
    let c = setup("heavy");
    let m = c.m();
    let (u, x) = (c.usdc.clone(), c.xlm.clone());
    let backer = Address::generate(&c.e);
    let b_pt = c.mint_usdc_pt(&backer, 50 * R7);
    let b_shares = c.backstop_deposit(&backer, b_pt).expect("backer deposit");
    c.strict::<()>(&backer, "backstop_request_withdraw", (backer.clone(), b_shares).into_val(&c.e), &[]).expect("request");
    let t = trader(&c);
    let keeper = Address::generate(&c.e);
    fund(&c.e, &x, &keeper, 1_000_000 * R7 as i128);
    fund(&c.e, &u, &keeper, 100_000 * R7 as i128);
    let mut o = |long: bool, lev: u128, name: &str| -> u64 {
        c.tick();
        let id = c.open(&t, long, lev, EQ).unwrap_or_else(|err| panic!("{name}: {err}"));
        c.res(name, "release");
        id
    };
    let l5 = o(true, 5_000_000, "open long 5x (first long, exit pending)");
    let s5 = o(false, 5_000_000, "open short 5x (first short, after a long, exit pending)");
    let _l3 = o(true, 3_000_000, "open long 3x (second long, both sides open, exit pending)");
    let _s3 = o(false, 3_000_000, "open short 3x (second short, both sides open, exit pending)");
    c.post(80, 100, 600, 120);
    assert!(c.liquidate(&keeper, l5).expect("L5 -20%").1);
    c.post(140, 100, 0, 60);
    assert!(c.liquidate(&keeper, s5).expect("S5 +40%").1);
    c.post(1, 1, 0, 60);
    assert!(m.get_bs_debt_shares(&u) > 0 && m.get_bs_debt_shares(&x) > 0 && m.get_backstop().pending_shares > 0);
    // WP6 1a: opens refused while backstop bad debt is unsettled
    let mut refused = serde_json::Map::new();
    for long in [true, false] {
        c.tick();
        let r = c.open(&t, long, 5_000_000, EQ);
        assert!(r.as_ref().is_err_and(|s| s.contains("#25")), "open with bad debt outstanding must fail BackstopDebtOutstanding: {r:?}");
        refused.insert(if long { "long".into() } else { "short".into() }, json!(format!("{r:?}")));
    }
    c.note("opensWithBadDebt", Value::Object(refused));
    c.tick();
    let _ = c.settle(&keeper, &x, u128::MAX).expect("settle XLM");
    c.res("settle_backstop_debt XLM (ticked, book open, exit pending)", "liquidation");
    c.tick();
    let _ = c.settle(&keeper, &u, u128::MAX).expect("settle USDC");
    c.res("settle_backstop_debt USDC (ticked, book open)", "liquidation");
    assert!(m.get_bs_debt_shares(&u) == 0 && m.get_bs_debt_shares(&x) == 0 && m.get_bs_xlm_ptokens() > 0);
    let mut o = |long: bool, lev: u128, name: &str| -> u64 {
        c.tick();
        let id = c.open(&t, long, lev, EQ).unwrap_or_else(|err| panic!("{name}: {err}"));
        c.res(name, "release");
        id
    };
    o(true, 5_000_000, "open long 5x after settles (both sides open, exit pending, backstop XLM pT held)");
    o(false, 5_000_000, "open short 5x after settles (both sides open, exit pending)");
    // 72 h later (interest accrued on both receipts, exit request inside its window)
    let req = m.get_withdraw_req(&backer).unwrap();
    c.e.ledger().with_mut(|li| li.timestamp = req.ready_at + 1);
    c.post(1, 1, 0, 120);
    let mut o = |long: bool, lev: u128, name: &str| -> u64 {
        c.tick();
        let id = c.open(&t, long, lev, EQ).unwrap_or_else(|err| panic!("{name}: {err}"));
        c.res(name, "release");
        id
    };
    o(true, 3_000_000, "open long 3x after 72 h (3 longs + 3 shorts open, exit pending)");
    o(false, 3_000_000, "open short 3x after 72 h (3 longs + 3 shorts open, exit pending)");
    c.ledger_eq("heavy end");
    c.finish("heavy");
}
