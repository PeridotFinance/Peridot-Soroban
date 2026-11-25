//! Sentora group, phase 2: USDC + PYUSD + XLM, all three Sentora-backed, with the PYUSD peg oracle as the group's ONE
//! controller oracle, on a read-only Mainnet fork (protocol-30 host).
//!
//! Real bytes from the fork: Sentora XLM/USDC/PYUSD vaults (ca6b85a1) and escrow, the three SACs, Reflector CAFJZQ (USD
//! feed) and Reflector DEX feed CALI2BY (base USDC; same wasm 25fc3060), the Aquarius PYUSD/USDC concentrated pool
//! CAPIOQ (wasm 155a17b9, the one the LP vaults use; listed by the router's get_pools), and the CODE of the live
//! cash-group controller (f41e5260) and receipts (5630a701). Workspace WASM: sentora-strategy and peg-oracle (sha256
//! asserted).
//!
//! Strict auth everywhere it is measured. Contract-type users use `mock_auths` with an explicit tree. The liquidator is
//! a real G ACCOUNT: an account entry is added to the snapshot for an ephemeral, in-test ed25519 key (fixed seed, test
//! only, never used anywhere else), and its calls carry a genuinely signed SorobanAuthorizationEntry (Address
//! credentials, so the nonce entry is counted; source-account credentials would be one entry fewer).
//!
//! Fork: cd tools && node fetch-fork.cjs --out ../.scratch/fork3.json   (read-only RPC, git-ignored)
//! Build (from peridot-contracts/): stellar contract build --package sentora-strategy && stellar contract build --package peg-oracle
//! Run:  cargo test --test group3 -- --nocapture --test-threads=1   (results: .scratch/group3-*.json)
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use soroban_ledger_snapshot::LedgerSnapshot;
use soroban_sdk::testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke};
use soroban_sdk::xdr::{
    self, AccountEntry, AccountEntryExt, AccountId, AlphaNum12, AssetCode12, HashIdPreimage,
    HashIdPreimageSorobanAuthorization, InvokeContractArgs, LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey,
    LedgerKeyAccount, LedgerKeyTrustLine, Limits, ReadXdr, ScAddress, ScMap, ScMapEntry, ScSymbol, ScVal, ScVec,
    SequenceNumber, SorobanAddressCredentials, SorobanAuthorizationEntry, SorobanAuthorizedFunction,
    SorobanAuthorizedInvocation, SorobanCredentials, Thresholds, TrustLineAsset, TrustLineEntry, TrustLineEntryExt,
    WriteXdr,
};
use soroban_sdk::{contracttype, token, Address, Bytes, Env, IntoVal, Symbol, TryFromVal, Val, Vec};
use std::cell::RefCell;
use std::str::FromStr;

mod strat {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/sentora_strategy.wasm");
}
mod peg {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/peg_oracle.wasm");
}
const STRAT_SHA: &str = "221eaa03091d31e928083619764eea2b6a741a9bd80c397fc34e133309355954";
const PEG_SHA: &str = "d0c534637fd31a3ea532752b0adcd0419fd5e9fb76251f97b6a92a7e3858a39e";
const SENTORA_SHA: &str = "ca6b85a13e53353912aeee2d4742830fc58076660dde64fdad4a6afbcca9674f";
const ESCROW_SHA: &str = "f22e4a52893d04b277c79bd19b3ff0b8771e1c0ca396a35d9339dd3a027ae299";
const POOL_SHA: &str = "155a17b9929ffb1f9e84bd6ef5c00a4d613c1ab5f4ad4c502d84515250cc2907";

const ADMIN: &str = "GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP"; // baked init admin of the deployed code
const ESCROW: &str = "CCZSNGVMEHRKMMMQGVDHAXT3KSWIMPEPRRTUE2GU3F6TW4XA2Y6EMHN3";
const REFLECTOR: &str = "CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN";
const REFLECTOR_DEX: &str = "CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M";
const POOL: &str = "CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX";
const STABLE_POOL: &str = "CDMH535JSD224YXPET3B4SJOLXTQQ24GRSCWACGYBKSH2DKFJYWI7SUW";
const XLM_T: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const USDC_T: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const PYUSD_T: &str = "CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2";
const PYUSD_ISSUER: &str = "GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5";
const USDC_ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
const S_XLM: &str = "CA54LVHMAY7HGLMVPN4W72XJB4OGKVZBZX26FWN6JD4P3HJFWQUQEHJO";
const S_USDC: &str = "CAHEWHOPPDBQYFMAOLDOXXGUX2BCR7EXP4CWYCRY3NEAJB35YPZMMJFF";
const S_PYUSD: &str = "CAQRAXBU6G4AAX4BZ7R4WLB62TSVAQFS5ZXJDVXRLAU2NZ2ZTGU5QOYB";
const LIVE_CTRL: &str = "CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK";
const LIVE_RV: &str = "CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F";
const R7: i128 = 10_000_000;
const WEEK: u64 = 7 * 24 * 3600;
const IDLE_BPS: u32 = 2_000;
/// Ephemeral test-only signing seed for the synthetic G-account liquidator.
const LIQ_SEED: [u8; 32] = [42u8; 32];

// Live network limits (Mainnet, protocol 30) and the proposal's 85 % ceiling.
const NET_INSTR: i64 = 400_000_000;
const NET_MEM: i64 = 41_943_040;

#[contracttype]
enum SacKey { Balance(Address) }
#[contracttype]
struct SacBalance { amount: i128, authorized: bool, clawback: bool }
#[contracttype]
#[derive(Clone, PartialEq)]
enum RAsset { Stellar(Address), Other(Symbol) }
#[contracttype]
#[derive(Clone)]
struct RPriceData { price: i128, timestamp: u64 }
#[contracttype]
struct RPriceUpdate { mask: Bytes, prices: Vec<i128> }

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
fn account_id(g: &str) -> AccountId { AccountId::from_str(g).unwrap() }
fn liq_key() -> SigningKey { SigningKey::from_bytes(&LIQ_SEED) }
fn liq_account_id() -> AccountId {
    AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(liq_key().verifying_key().to_bytes())))
}

fn load_fork() -> (LedgerSnapshot, Value) {
    let path = std::env::var("FORK3").unwrap_or_else(|_| format!("{}/.scratch/fork3.json", dir()));
    let j: Value = serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|_| panic!("{path} unreadable: run tools/fetch-fork.cjs"))).unwrap();
    let mut entries = std::vec::Vec::new();
    for e in j["entries"].as_array().unwrap() {
        let key = LedgerKey::from_xdr_base64(e["key"].as_str().unwrap(), Limits::none()).unwrap();
        let data = LedgerEntryData::from_xdr_base64(e["xdr"].as_str().unwrap(), Limits::none()).unwrap();
        let entry = LedgerEntry { last_modified_ledger_seq: e["lastModified"].as_u64().unwrap() as u32, data, ext: LedgerEntryExt::V0 };
        entries.push((Box::new(key), (Box::new(entry), e["liveUntil"].as_u64().map(|x| x as u32))));
    }
    let seq = j["ledger"].as_u64().unwrap() as u32;
    // Synthetic classic entries (test-only): the G liquidator's account (100k XLM, master key = ephemeral test key)
    // and the admin's PYUSD trustline so the PYUSD receipt can take its permanent seed from the admin.
    let liq = AccountEntry {
        account_id: liq_account_id(), balance: 100_000 * R7 as i64, seq_num: SequenceNumber(1 << 32), num_sub_entries: 0,
        inflation_dest: None, flags: 0, home_domain: Default::default(), thresholds: Thresholds([1, 0, 0, 0]),
        signers: Default::default(), ext: AccountEntryExt::V0,
    };
    entries.push((Box::new(LedgerKey::Account(LedgerKeyAccount { account_id: liq_account_id() })),
        (Box::new(LedgerEntry { last_modified_ledger_seq: seq, data: LedgerEntryData::Account(liq), ext: LedgerEntryExt::V0 }), None)));
    let mut code = [0u8; 12];
    code[..5].copy_from_slice(b"PYUSD");
    let tla = TrustLineAsset::CreditAlphanum12(AlphaNum12 { asset_code: AssetCode12(code), issuer: account_id(PYUSD_ISSUER) });
    let mut usdc = [0u8; 4];
    usdc.copy_from_slice(b"USDC");
    let tlu = TrustLineAsset::CreditAlphanum4(xdr::AlphaNum4 { asset_code: xdr::AssetCode4(usdc), issuer: account_id(USDC_ISSUER) });
    // Liquidator trustlines (PYUSD, USDC) so it can exit seized pTokens to its own account.
    for (acct, asset) in [(account_id(ADMIN), tla.clone()), (liq_account_id(), tla), (liq_account_id(), tlu)] {
        let b = if acct == liq_account_id() { 100_000 * R7 as i64 } else { 0 };
        let tl = TrustLineEntry { account_id: acct.clone(), asset: asset.clone(), balance: b, limit: i64::MAX, flags: 1, ext: TrustLineEntryExt::V0 };
        entries.push((Box::new(LedgerKey::Trustline(LedgerKeyTrustLine { account_id: acct, asset })),
            (Box::new(LedgerEntry { last_modified_ledger_seq: seq, data: LedgerEntryData::Trustline(tl), ext: LedgerEntryExt::V0 }), None)));
    }
    let nid: [u8; 32] = Sha256::digest(j["network"].as_str().unwrap().as_bytes()).into();
    let snap = LedgerSnapshot {
        protocol_version: j["protocolVersion"].as_u64().unwrap() as u32, sequence_number: seq,
        timestamp: j["closeTime"].as_u64().unwrap(), network_id: nid, base_reserve: 5_000_000,
        min_persistent_entry_ttl: 2_073_600, min_temp_entry_ttl: 17_280, max_entry_ttl: 3_110_400, ledger_entries: entries,
    };
    (snap, j)
}

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

fn sc(e: &Env, v: Val) -> ScVal { ScVal::try_from_val(e, &v).unwrap() }
fn sc_addr(e: &Env, x: &Address) -> ScAddress {
    match sc(e, x.to_val()) { ScVal::Address(s) => s, _ => unreachable!() }
}

#[derive(Clone)]
#[allow(dead_code)]
struct M { asset: Address, rv: Address, sentora: Address, strat: Address }

#[allow(dead_code)]
struct Ctx {
    e: Env,
    admin: Address,
    ctrl: Address,
    peg: Address,
    added: std::vec::Vec<&'static str>,
    usdc: M,
    pyusd: M,
    xlm: M,
    liq: Address,
    nonce: RefCell<i64>,
    /// Current XLM scale (num, den) and DEX-feed PYUSD/USDC override (bps), re-posted by `repost`.
    scale: RefCell<(i128, i128, Option<i128>)>,
    base: std::vec::Vec<(Address, std::vec::Vec<Option<i128>>)>,
    rows: RefCell<std::vec::Vec<Value>>,
    notes: RefCell<serde_json::Map<String, Value>>,
}

impl Ctx {
    fn pt(&self, m: &M, who: &Address) -> u128 { call(&self.e, &m.rv, "get_ptoken_balance", vec![who.into_val(&self.e)]) }
    fn debt(&self, m: &M, who: &Address) -> u128 { call(&self.e, &m.rv, "get_user_borrow_balance", vec![who.into_val(&self.e)]) }
    fn note(&self, k: &str, v: Value) { self.notes.borrow_mut().insert(k.into(), v); }
    fn tick(&self) { self.e.ledger().with_mut(|l| { l.timestamp += 5; l.sequence_number += 1; }); }

    fn res(&self, op: &str) -> Value {
        let r = self.e.cost_estimate().resources();
        let keys = r.disk_read_entries + r.memory_read_entries;
        let row = json!({"op": op, "cpu": r.instructions, "memBytes": r.mem_bytes, "footprintKeys": keys,
            "diskReadEntries": r.disk_read_entries, "memoryReadEntries": r.memory_read_entries, "writeEntries": r.write_entries,
            "diskReadBytes": r.disk_read_bytes, "writeBytes": r.write_bytes, "eventsBytes": r.contract_events_size_bytes,
            "fits85pctOfLive": keys <= 340 && r.disk_read_entries <= 170 && r.write_entries <= 170 && r.instructions <= 340_000_000,
            "networkFit": r.instructions <= NET_INSTR && keys <= 400 && r.disk_read_entries <= 200 && r.write_entries <= 200 && r.mem_bytes <= NET_MEM});
        println!("{row}");
        self.rows.borrow_mut().push(row.clone());
        row
    }

    fn strict(&self, who: &Address, contract: &Address, f: &str, args: Vec<Val>, subs: &[(&Address, &str, Vec<Val>)]) {
        let s: std::vec::Vec<MockAuthInvoke> = subs.iter().map(|(c, n, x)| MockAuthInvoke { contract: c, fn_name: n, args: x.clone(), sub_invokes: &[] }).collect();
        self.e.mock_auths(&[MockAuth { address: who, invoke: &MockAuthInvoke { contract, fn_name: f, args: args.clone(), sub_invokes: &s } }]);
        let r = self.e.try_invoke_contract::<Val, soroban_sdk::Error>(contract, &Symbol::new(&self.e, f), args);
        assert!(matches!(r, Ok(Ok(_))), "strict {f} failed: {r:?}");
    }

    /// The G-account liquidator signs `contract.f(args)` (and `subs`) with its ed25519 key: real account auth.
    fn signed(&self, contract: &Address, f: &str, args: Vec<Val>, subs: &[(&Address, &str, Vec<Val>)]) -> Result<(), String> {
        let e = &self.e;
        let inv = |c: &Address, n: &str, x: &Vec<Val>, sub: std::vec::Vec<SorobanAuthorizedInvocation>| SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                contract_address: sc_addr(e, c), function_name: ScSymbol(n.try_into().unwrap()),
                args: x.iter().map(|v| sc(e, v)).collect::<std::vec::Vec<_>>().try_into().unwrap() }),
            sub_invocations: sub.try_into().unwrap(),
        };
        let subs_x = subs.iter().map(|(c, n, x)| inv(c, n, x, vec![])).collect();
        let root = inv(contract, f, &args, subs_x);
        let nonce = { let mut n = self.nonce.borrow_mut(); *n += 1; *n };
        let exp = e.ledger().sequence() + 100;
        let nid: [u8; 32] = e.ledger().network_id().to_array();
        let pre = HashIdPreimage::SorobanAuthorization(HashIdPreimageSorobanAuthorization {
            network_id: xdr::Hash(nid), nonce, signature_expiration_ledger: exp, invocation: root.clone() });
        let payload: [u8; 32] = Sha256::digest(pre.to_xdr(Limits::none()).unwrap()).into();
        let sig = liq_key().sign(&payload).to_bytes();
        let pk = liq_key().verifying_key().to_bytes();
        let sigval = ScVal::Vec(Some(ScVec(vec![ScVal::Map(Some(ScMap(vec![
            ScMapEntry { key: ScVal::Symbol(ScSymbol("public_key".try_into().unwrap())), val: ScVal::Bytes(pk.to_vec().try_into().unwrap()) },
            ScMapEntry { key: ScVal::Symbol(ScSymbol("signature".try_into().unwrap())), val: ScVal::Bytes(sig.to_vec().try_into().unwrap()) },
        ].try_into().unwrap())))].try_into().unwrap())));
        let entry = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: sc_addr(e, &self.liq), nonce, signature_expiration_ledger: exp, signature: sigval }),
            root_invocation: root,
        };
        e.set_auths(&[entry]);
        let r = e.try_invoke_contract::<Val, soroban_sdk::Error>(contract, &Symbol::new(e, f), args);
        match r { Ok(Ok(_)) => Ok(()), other => Err(format!("{other:?}")) }
    }

    fn deposit(&self, m: &M, who: &Address, amt: i128) {
        let e = &self.e;
        self.strict(who, &m.rv, "deposit", (who.clone(), amt as u128).into_val(e),
            &[(&m.asset, "transfer", (who.clone(), m.rv.clone(), amt).into_val(e))]);
    }

    /// Next 5-minute round on BOTH Reflectors with base prices, `xlm` scaling (num/den) on the USD feed and `pyusd_bps`
    /// for PYUSD/USDC on the DEX feed. Does NOT refresh any controller cache (cold-price path stays reachable).
    fn post(&self, num: i128, den: i128, pyusd_bps: Option<i128>) {
        *self.scale.borrow_mut() = (num, den, pyusd_bps);
        let e = &self.e;
        e.ledger().with_mut(|l| { l.timestamp = (l.timestamp / 300 + 1) * 300; l.sequence_number += 1; });
        e.mock_all_auths();
        for (oracle, base) in &self.base {
            let assets: Vec<RAsset> = call(e, oracle, "assets", vec![]);
            let mut prices = Vec::<i128>::new(e);
            let mut mask = vec![0u8; (assets.len() as usize).div_ceil(8)];
            for (i, asset) in assets.iter().enumerate() {
                let Some(Some(p0)) = base.get(i) else { continue };
                let mut p = *p0;
                if asset == RAsset::Other(Symbol::new(e, "XLM")) { p = p * num / den; }
                if asset == RAsset::Stellar(self.pyusd.asset.clone()) { if let Some(b) = pyusd_bps { p = 100_000_000_000_000 * b / 10_000; } }
                mask[i / 8] |= 1 << (i % 8);
                prices.push_back(p);
            }
            let upd = RPriceUpdate { mask: Bytes::from_slice(e, &mask), prices };
            let _: () = call(e, oracle, "set_price", vec![upd.into_val(e), (e.ledger().timestamp() * 1000).into_val(e)]);
        }
    }
    /// Two more rounds at the current scale: every controller price cache expires (cold), prices unchanged.
    fn expire(&self) {
        let (n, d, p) = *self.scale.borrow();
        self.post(n, d, p);
        self.post(n, d, p);
        self.e.ledger().with_mut(|l| l.timestamp += 5);
        self.refresh_all("");
    }
    fn warm(&self) {
        self.e.mock_all_auths();
        for m in self.markets() { let _: Option<(u128, u128)> = call(&self.e, &self.ctrl, "cache_price", vec![m.asset.into_val(&self.e)]); }
    }
    fn price(&self, t: &Address) -> Option<(u128, u128)> { call(&self.e, &self.ctrl, "get_price_usd", vec![t.into_val(&self.e)]) }
    fn refresh_all(&self, tag: &str) {
        self.e.mock_all_auths();
        for n in self.added.clone() {
            let m = self.m(n);
            self.tick();
            let _: () = call(&self.e, &m.rv, "refresh_boosted_underlying", vec![]);
            if tag == "measure" { self.res(&format!("refresh_boosted_underlying {n}")); }
            let _ = n;
        }
    }
    fn m(&self, n: &str) -> &M { match n { "USDC" => &self.usdc, "PYUSD" => &self.pyusd, _ => &self.xlm } }
    fn markets(&self) -> std::vec::Vec<&M> { self.added.iter().map(|n| self.m(n)).collect() }
    fn peg_status(&self) -> peg::PegStatus { peg::Client::new(&self.e, &self.peg).peg_status() }
    fn save(&self, name: &str) {
        let out = json!({"rows": *self.rows.borrow(), "notes": *self.notes.borrow()});
        std::fs::write(format!("{}/.scratch/{name}", dir()), serde_json::to_string_pretty(&out).unwrap()).unwrap();
    }
}

/// Refined group (owner decisions 25 Nov): USDC CF 0.90 and PYUSD CF 0.80 borrowable; XLM CF 0.55 collateral-only.
/// Liquidation incentive 1.03, close factor 50 %. Controller oracle = peg oracle. PYUSD band [0.99, 1.00], USDC band
/// [0.98, 1.02] (cash-group setting). Absolute Sentora caps per adapter.
const CFS: &[(&str, u128)] = &[("USDC", 900_000), ("PYUSD", 800_000), ("XLM", 550_000)];
const BORROWABLE: &[&str] = &["USDC", "PYUSD"];
const SENTORA_CAP: &[(&str, i128)] = &[("USDC", 20_000 * R7), ("PYUSD", 5_000 * R7), ("XLM", 100_000 * R7)];
const LI: u128 = 1_030_000;

fn setup(max_markets: u32) -> Ctx {
    let (snap, fj) = load_fork();
    let e = Env::from_ledger_snapshot(snap);
    e.cost_estimate().disable_resource_limits();
    #[allow(deprecated)]
    let pv = e.ledger().protocol_version();
    assert_eq!(pv, 30);
    let rd = |p: &str| std::fs::read(format!("{}/../../target/wasm32v1-none/release/{p}", dir())).unwrap();
    let (strat_wasm, peg_wasm) = (rd("sentora_strategy.wasm"), rd("peg_oracle.wasm"));
    assert_eq!(hex(&Sha256::digest(&strat_wasm)), STRAT_SHA, "rebuild with stellar contract build --package sentora-strategy");
    assert_eq!(hex(&Sha256::digest(&peg_wasm)), PEG_SHA, "rebuild with stellar contract build --package peg-oracle");
    let ih = &fj["instanceHash"];
    for s in [S_XLM, S_USDC, S_PYUSD] { assert_eq!(ih[s], SENTORA_SHA); }
    assert_eq!(ih[POOL], POOL_SHA);
    assert_eq!(ih[REFLECTOR], ih[REFLECTOR_DEX]);
    // The fork reproduces the RPC's own pool quotes (a missing entry would read as empty instead of faulting).
    for p in fj["poolSims"].as_array().unwrap() {
        let (i, o) = (p["inIdx"].as_u64().unwrap() as u32, p["outIdx"].as_u64().unwrap() as u32);
        let amt: u128 = p["amount"].as_str().unwrap().parse().unwrap();
        let pool = a(&e, p["pool"].as_str().unwrap());
        let q: u128 = call(&e, &pool, "estimate_swap", vec![i.into_val(&e), o.into_val(&e), amt.into_val(&e)]);
        assert_eq!(q.to_string(), p["estimate"].as_str().unwrap(), "fork quote differs from RPC for {p}");
    }
    let ctrl_code = code(&fj, ih[LIVE_CTRL].as_str().unwrap());
    let rv_code = code(&fj, ih[LIVE_RV].as_str().unwrap());
    e.mock_all_auths();
    let admin = a(&e, ADMIN);
    let pools = Vec::from_array(&e, [
        peg::PoolRef { pool: a(&e, POOL), pegged_idx: 0, quote_idx: 1 },
        peg::PoolRef { pool: a(&e, STABLE_POOL), pegged_idx: 0, quote_idx: 1 },
    ]);
    let peg_cfg = peg::PegConfig {
        reflector: a(&e, REFLECTOR), dex_feed: a(&e, REFLECTOR_DEX), pegged: a(&e, PYUSD_T), pegged_symbol: Symbol::new(&e, "PYUSD"),
        quote: a(&e, USDC_T), quote_symbol: Symbol::new(&e, "USDC"), pools, probe: 1_000 * R7 as u128, band_bps: 100, max_age_secs: 600,
    };
    let pegc = e.register(peg_wasm.as_slice(), (peg_cfg,));
    let ctrl = e.register(ctrl_code.as_slice(), ());
    let _: () = call(&e, &ctrl, "initialize", vec![admin.into_val(&e)]);
    let _: () = call(&e, &ctrl, "set_oracle", vec![pegc.into_val(&e)]);
    let mut ms = std::vec::Vec::new();
    for (t, s, symb) in [(USDC_T, S_USDC, "USDC"), (PYUSD_T, S_PYUSD, "PYUSD"), (XLM_T, S_XLM, "XLM")] {
        let asset = a(&e, t);
        let rv = e.register(rv_code.as_slice(), ());
        let _: () = call(&e, &rv, "initialize", vec![asset.into_val(&e), 0u128.into_val(&e), 0u128.into_val(&e), admin.into_val(&e)]);
        let _: () = call(&e, &rv, "enable_static_rates", vec![admin.into_val(&e)]);
        let _: () = call(&e, &rv, "set_cash_borrowing_enabled", vec![admin.into_val(&e), BORROWABLE.contains(&symb).into_val(&e)]);
        let _: () = call(&e, &rv, "set_idle_cash_buffer_bps", vec![admin.into_val(&e), IDLE_BPS.into_val(&e)]);
        let seeder = Address::generate(&e);
        fund(&e, &asset, &seeder, R7);
        token::TokenClient::new(&e, &asset).transfer(&seeder, &admin, &R7);
        let _: u128 = call(&e, &rv, "seed_minimum_supply", vec![admin.into_val(&e), (R7 as u128).into_val(&e)]);
        let sentora = a(&e, s);
        let cap = SENTORA_CAP.iter().find(|(n, _)| *n == symb).unwrap().1 as u128;
        let st = e.register(strat_wasm.as_slice(), (admin.clone(), rv.clone(), asset.clone(), sentora.clone(), a(&e, ESCROW), WEEK, cap, pins(&e)));
        let cf = CFS.iter().find(|(n, _)| *n == symb).unwrap().1;
        let _: () = call(&e, &ctrl, "set_oracle_asset_symbol", vec![asset.into_val(&e), Some(Symbol::new(&e, symb)).into_val(&e)]);
        let _: () = call(&e, &ctrl, "add_market", vec![rv.into_val(&e)]);
        let _: () = call(&e, &ctrl, "set_market_cf", vec![rv.into_val(&e), cf.into_val(&e)]);
        let _: () = call(&e, &rv, "set_collateral_factor", vec![cf.into_val(&e)]);
        let _: () = call(&e, &rv, "set_peridottroller", vec![ctrl.into_val(&e)]);
        let _: () = call(&e, &rv, "set_boosted_vault", vec![admin.into_val(&e), st.into_val(&e)]);
        ms.push(M { asset, rv, sentora, strat: st });
    }
    let _: () = call(&e, &ctrl, "set_price_band", vec![a(&e, USDC_T).into_val(&e), Some((980_000u128, 1_020_000u128)).into_val(&e)]);
    let _: () = call(&e, &ctrl, "set_price_band", vec![a(&e, PYUSD_T).into_val(&e), Some((990_000u128, 1_000_000u128)).into_val(&e)]);
    // Timelocked parameters: stage, wait 24 h, apply.
    let _: () = call(&e, &ctrl, "set_liquidation_incentive", vec![LI.into_val(&e)]);
    if max_markets > 2 { let _: () = call(&e, &ctrl, "set_max_user_markets", vec![max_markets.into_val(&e)]); }
    e.ledger().with_mut(|l| { l.timestamp += 86_401; l.sequence_number += 1; });
    let _: () = call(&e, &ctrl, "set_liquidation_incentive", vec![LI.into_val(&e)]);
    if max_markets > 2 { let _: () = call(&e, &ctrl, "set_max_user_markets", vec![max_markets.into_val(&e)]); }
    let li: u128 = call(&e, &ctrl, "get_liquidation_incentive_scaled", vec![]);
    assert_eq!(li, LI);
    let mm: u32 = call(&e, &ctrl, "get_max_user_markets", vec![]);
    assert_eq!(mm, max_markets);
    let xlm = ms.pop().unwrap();
    let pyusd = ms.pop().unwrap();
    let usdc = ms.pop().unwrap();
    let mut base = std::vec::Vec::new();
    for o in [REFLECTOR, REFLECTOR_DEX] {
        let r = a(&e, o);
        let assets: Vec<RAsset> = call(&e, &r, "assets", vec![]);
        let b = assets.iter().map(|x| { let p: Option<RPriceData> = call(&e, &r, "lastprice", vec![x.into_val(&e)]); p.map(|p| p.price) }).collect();
        base.push((r, b));
    }
    let liq = Address::try_from_val(&e, &ScAddress::Account(liq_account_id())).unwrap();
    let c = Ctx { e, admin, ctrl, peg: pegc, added: vec!["USDC", "PYUSD", "XLM"], usdc, pyusd, xlm, liq, nonce: RefCell::new(7_000),
        scale: RefCell::new((1, 1, None)), base, rows: RefCell::new(vec![]), notes: RefCell::new(serde_json::Map::new()) };
    c.post(1, 1, None);
    c.warm();
    let st = c.peg_status();
    assert!(st.priced && st.at_peg && st.missing == 0, "peg oracle at fork ledger: {st:?}");
    c.note("pegStatusAtFork", json!(format!("{st:?}")));
    c.note("config", json!({"cf": format!("{CFS:?}"), "li": LI, "maxUserMarkets": max_markets, "sentoraCaps": format!("{SENTORA_CAP:?}")}));
    c
}

/// Lenders in every market; the borrower deposits `coll`, enters `coll` and `debts`, borrows `frac_bps` of its limit
/// split evenly over `debts`; the LAST borrow runs with cold price caches (peg oracle live in the health check).
fn open(c: &Ctx, coll: &[(&str, i128)], debts: &[&str], frac_bps: u128) -> Address {
    let e = &c.e;
    let lender = Address::generate(e);
    let borrower = Address::generate(e);
    for n in ["USDC", "PYUSD", "XLM"] {
        let m = c.m(n);
        let amt = if n == "XLM" { 200_000 * R7 } else { 10_000 * R7 };
        fund(e, &m.asset, &lender, amt);
        c.tick();
        c.deposit(m, &lender, amt);
    }
    for (n, amt) in coll {
        fund(e, &c.m(n).asset, &borrower, *amt);
        c.tick();
        c.deposit(c.m(n), &borrower, *amt);
        c.res(&format!("deposit {n} (strict; borrower)"));
    }
    e.mock_all_auths();
    let mut entered: std::vec::Vec<&str> = coll.iter().map(|(n, _)| *n).collect();
    for d in debts { if !entered.contains(d) { entered.push(d); } }
    for n in &entered { let _: () = call(e, &c.ctrl, "enter_market", vec![borrower.into_val(e), c.m(n).rv.into_val(e)]); }
    assert_eq!(c.price(&c.pyusd.asset), Some((100_000_000_000_000, 100_000_000_000_000)), "PYUSD is exactly $1");
    let (liq0, _): (u128, u128) = call(e, &c.ctrl, "account_liquidity", vec![borrower.into_val(e)]);
    let share = liq0 * frac_bps / 10_000 / debts.len() as u128;
    for (i, n) in debts.iter().enumerate() {
        let m = c.m(n);
        let last = i + 1 == debts.len();
        let (p, s) = c.price(&m.asset).unwrap();
        if last { c.expire(); }
        let amt = share * s / p;
        c.tick();
        c.strict(&borrower, &m.rv, "borrow", (borrower.clone(), amt).into_val(e), &[]);
        let temp = if last { "COLD prices, oracle live" } else { "warm prices" };
        c.res(&format!("borrow {n} (strict; {} entered; debt #{}; {temp})", entered.len(), i + 1));
    }
    borrower
}

fn liquidate(c: &Ctx, borrower: &Address, repay: &M, coll: &M, tag: &str, cold: bool) -> u128 {
    let e = &c.e;
    let (_, sf): (u128, u128) = call(e, &c.ctrl, "account_liquidity", vec![borrower.into_val(e)]);
    assert!(sf > 0, "borrower must be in shortfall ({tag})");
    if cold { c.expire(); } else { c.warm(); c.refresh_all(""); }
    let amount = c.debt(repay, borrower) * 40 / 100;
    e.mock_all_auths();
    let live = e.ledger().sequence() + 100_000;
    token::TokenClient::new(e, &repay.asset).approve(&c.liq, &repay.rv, &(50_000 * R7), &live);
    c.tick();
    let before = c.pt(coll, &c.liq);
    let args: Vec<Val> = (borrower.clone(), repay.rv.clone(), coll.rv.clone(), amount, c.liq.clone()).into_val(e);
    c.signed(&c.ctrl, "liquidate", args, &[]).unwrap_or_else(|err| panic!("liquidate {tag}: {err}"));
    let temp = if cold { "COLD prices, oracle live" } else { "warm prices" };
    c.res(&format!("liquidate {tag}; {temp} (G liquidator, signed)"));
    let seized = c.pt(coll, &c.liq) - before;
    assert!(seized > 0);
    seized
}

fn liquidator_exit(c: &Ctx, m: &M, seized: u128, n: &str) {
    c.tick();
    let before = bal(&c.e, &m.asset, &c.liq);
    let args: Vec<Val> = (c.liq.clone(), seized).into_val(&c.e);
    c.signed(&m.rv, "withdraw", args, &[]).unwrap_or_else(|err| panic!("liquidator withdraw: {err}"));
    c.res(&format!("G liquidator withdraw seized {n} pTokens (signed; Sentora exit beyond idle)"));
    assert!(bal(&c.e, &m.asset, &c.liq) > before);
}

fn finish(c: &Ctx, name: &str) {
    c.save(name);
    for r in c.rows.borrow().iter() { assert!(r["networkFit"].as_bool().unwrap(), "network limit exceeded: {r}"); }
}

const COLL3: &[(&str, i128)] = &[("XLM", 4_000 * R7), ("USDC", 1_000 * R7), ("PYUSD", 1_000 * R7)];

/// Worst case with MaxUserMarkets = 3: collateral in all three, debt in both stables, XLM -40 %.
#[test]
fn three_markets_xlm_crash_cold_and_warm() {
    for (cold, coll, n) in [(true, "XLM", "cold-xlm"), (false, "XLM", "warm-xlm"), (true, "PYUSD", "cold-pyusd")] {
        let c = setup(3);
        let b = open(&c, COLL3, &["USDC", "PYUSD"], 9_500);
        c.post(60, 100, None);
    c.expire();
        let m = c.m(coll);
        let s = liquidate(&c, &b, &c.usdc, m, &format!("3 markets, repay USDC, seize {coll}"), cold);
        liquidator_exit(&c, m, s, coll);
        finish(&c, &format!("group3-3m-{n}.json"));
    }
}

/// MaxUserMarkets = 2 (deployed default): XLM collateral against USDC debt.
#[test]
fn two_markets_xlm_collateral_cold() {
    let c = setup(2);
    let b = open(&c, &[("XLM", 4_000 * R7), ("USDC", 100 * R7)], &["USDC"], 9_500);
    c.post(60, 100, None);
    c.expire();
    let s = liquidate(&c, &b, &c.usdc, &c.xlm, "2 markets, repay USDC, seize XLM", true);
    liquidator_exit(&c, &c.xlm, s, "XLM");
    finish(&c, "group3-2m-xlm.json");
}

fn whale(c: &Ctx) -> Address {
    let w = Address::generate(&c.e);
    fund(&c.e, &c.pyusd.asset, &w, 5_000_000 * R7);
    fund(&c.e, &c.usdc.asset, &w, 5_000_000 * R7);
    w
}
fn swap(c: &Ctx, pool: &Address, who: &Address, sell_pyusd: bool, amt: u128) -> u128 {
    c.e.mock_all_auths();
    let (i, o) = if sell_pyusd { (0u32, 1u32) } else { (1, 0) };
    let r = c.e.try_invoke_contract::<u128, soroban_sdk::Error>(pool, &Symbol::new(&c.e, "swap"),
        (who.clone(), i, o, amt, 0u128).into_val(&c.e));
    match r { Ok(Ok(v)) => v, _ => 0 }
}
/// The oracle's probe quote of one pool in USD (1e14): bid = USDC per PYUSD sold; ask = USDC per PYUSD bought.
fn quote_usd(c: &Ctx, pool: &Address, bid: bool) -> Option<f64> {
    let e = &c.e;
    let probe: u128 = 1_000 * R7 as u128;
    let (i, o) = if bid { (0u32, 1u32) } else { (1, 0) };
    let r = e.try_invoke_contract::<u128, soroban_sdk::Error>(pool, &Symbol::new(e, "estimate_swap"), (i, o, probe).into_val(e));
    let out = match r { Ok(Ok(v)) if v > 0 => v as f64, _ => return None };
    let q: Option<RPriceData> = call(e, &a(e, REFLECTOR), "lastprice", vec![RAsset::Other(Symbol::new(e, "USDC")).into_val(e)]);
    let usd = q.unwrap().price as f64 / 1e14;
    Some(if bid { out / probe as f64 * usd } else { probe as f64 / out * usd })
}

/// Manipulation cost from REAL pool depth (fork of the deployed pool bytes and state): the trade that pushes each
/// pool's 1,000-unit probe quote past the 100 bps threshold, in both directions, and the cost of that move.
#[test]
fn manipulation_cost_both_pools_both_directions() {
    let mut out = serde_json::Map::new();
    for (name, pool_s, steps) in [("concentrated", POOL, [5_000i128, 10_000, 25_000]), ("stable", STABLE_POOL, [50_000, 250_000, 500_000])] {
        for down in [true, false] {
            let c = setup(2);
            let pool = a(&c.e, pool_s);
            let w = whale(&c);
            let q0 = quote_usd(&c, &pool, down).unwrap();
            let (mut spent, mut got) = (0i128, 0i128);
            let mut tripped = None;
            'outer: for step in steps {
                for _ in 0..40 {
                    let o = swap(&c, &pool, &w, down, (step * R7) as u128);
                    if o == 0 { break 'outer; }
                    spent += step * R7;
                    got += o as i128;
                    let q = quote_usd(&c, &pool, down);
                    let past = match q { None => true, Some(v) => if down { v < 0.99 } else { v > 1.01 } };
                    if past { tripped = Some(q); break 'outer; }
                    if q.map(|v| (v - q0).abs() > 0.002).unwrap_or(true) { continue 'outer; }
                }
            }
            // In-transaction round trip: immediately swap the proceeds back (what a flash manipulator pays).
            let back = swap(&c, &pool, &w, !down, got as u128) as i128;
            let rt_loss = (spent - back) as f64 / R7 as f64;
            let dur_loss = (spent - got) as f64 / R7 as f64; // at par: value given minus value received, if arbs restore
            let key = format!("{name} {}", if down { "down (sell PYUSD; bid < 0.99)" } else { "up (buy PYUSD; ask > 1.01)" });
            out.insert(key, json!({"inputUnits": spent / R7, "outputUnits": got as f64 / R7 as f64, "quoteBefore": q0,
                "quoteAfter": format!("{tripped:?}"), "tripped": tripped.is_some(), "inTxRoundTripLossUsd": rt_loss,
                "durableLossAtParUsd": dur_loss, "pegPrice": format!("{:?}", c.peg_status().price)}));
            // One source alone never moves the oracle price.
            assert!(c.peg_status().at_peg, "single-pool move must not move the median: {name}");
        }
    }
    std::fs::write(format!("{}/.scratch/group3-manipulation.json", dir()), serde_json::to_string_pretty(&out).unwrap()).unwrap();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

/// Real depeg (two sources): the concentrated pool is sold down with real swaps and the round-sampled DEX feed reads
/// 0.97 while the deep stable pool stays at par. The oracle marks PYUSD down to the median; the controller band then
/// blocks borrows and redeem-with-debt, while liquidation proceeds at the marked-down price. Debt-free exits work.
#[test]
fn depeg_marks_down_blocks_borrow_and_liquidates_at_min_price() {
    let c = setup(2);
    let e = &c.e;
    let b = open(&c, &[("PYUSD", 1_000 * R7), ("USDC", 50 * R7)], &["USDC"], 9_800);
    let pool = a(e, POOL);
    let w = whale(&c);
    // Sell down until the 1,000-unit bid is below 0.99 while the pool still quotes (all three sources live).
    for step in [10_000i128, 1_000] {
        for _ in 0..200 {
            let before = quote_usd(&c, &pool, true);
            if before.map(|v| v < 0.99).unwrap_or(true) { break; }
            let o = swap(&c, &pool, &w, true, (step * R7) as u128);
            if o == 0 || quote_usd(&c, &pool, true).is_none() { swap(&c, &pool, &w, false, o); break; }
        }
    }
    assert!(quote_usd(&c, &pool, true).map(|v| v < 0.99).unwrap_or(false), "concentrated pool sold below 0.99 and still quoting");
    assert!(c.peg_status().at_peg, "one source (concentrated pool) alone: still $1");
    assert!(c.price(&c.pyusd.asset).is_some());
    c.post(1, 1, Some(9_700)); // DEX feed round: PYUSD/USDC 0.97
    let st = c.peg_status();
    assert!(st.priced && !st.at_peg, "{st:?}");
    c.note("markedDownStatus", json!(format!("{st:?}")));
    let marked = st.price as u128;
    // Anyone (keeper) can push the mark-down into the controller cache at once.
    e.mock_all_auths();
    let cached: Option<(u128, u128)> = call(e, &c.ctrl, "cache_price", vec![c.pyusd.asset.into_val(e)]);
    assert_eq!(cached.map(|x| x.0), Some(marked));
    assert_eq!(c.price(&c.pyusd.asset), None, "public view: out of band");
    c.refresh_all("");
    c.tick();
    e.mock_auths(&[MockAuth { address: &b, invoke: &MockAuthInvoke { contract: &c.usdc.rv, fn_name: "borrow", args: (b.clone(), R7 as u128).into_val(e), sub_invokes: &[] } }]);
    let r = e.try_invoke_contract::<Val, soroban_sdk::Error>(&c.usdc.rv, &Symbol::new(e, "borrow"), (b.clone(), R7 as u128).into_val(e));
    assert!(!matches!(r, Ok(Ok(_))), "borrow fails closed");
    e.mock_auths(&[MockAuth { address: &b, invoke: &MockAuthInvoke { contract: &c.usdc.rv, fn_name: "withdraw", args: (b.clone(), R7 as u128).into_val(e), sub_invokes: &[] } }]);
    let r = e.try_invoke_contract::<Val, soroban_sdk::Error>(&c.usdc.rv, &Symbol::new(e, "withdraw"), (b.clone(), R7 as u128).into_val(e));
    assert!(!matches!(r, Ok(Ok(_))), "withdraw by an account with debt fails closed");
    let saver = Address::generate(e);
    fund(e, &c.pyusd.asset, &saver, 100 * R7);
    c.tick();
    c.deposit(&c.pyusd, &saver, 100 * R7);
    c.tick();
    let pts = c.pt(&c.pyusd, &saver);
    c.strict(&saver, &c.pyusd.rv, "withdraw", (saver.clone(), pts).into_val(e), &[]);
    // Liquidation at the marked-down price (warm: the mark-down is cached).
    let debt0 = c.debt(&c.usdc, &b);
    let seized = liquidate(&c, &b, &c.usdc, &c.pyusd, "depeg: repay USDC, seize PYUSD at min($1, observed)", false);
    let repaid = debt0 - c.debt(&c.usdc, &b);
    let (pu, su) = (c.base[0].1.iter().flatten().next().copied(), 0);
    let _ = (pu, su);
    let rate: u128 = call(e, &c.pyusd.rv, "get_exchange_rate", vec![]);
    let seized_underlying = seized * rate / 1_000_000;
    let implied = repaid as f64 * 1.03 / seized_underlying as f64; // USDC price ~1.001 ignored: implied PYUSD price
    c.note("liquidationAtMarkdown", json!({"markedPrice": marked as f64 / 1e14, "repaidUsdc": repaid as f64 / R7 as f64,
        "seizedPyusd": seized_underlying as f64 / R7 as f64, "impliedPyusdPrice": implied}));
    assert!(implied < 0.99 && implied > 0.95, "seized at the marked-down price: {implied}");
    // Restore the pool and the feed: $1 again, borrowing resumes.
    for _ in 0..40 { if quote_usd(&c, &pool, true).map(|v| v > 0.998).unwrap_or(false) { break; } swap(&c, &pool, &w, false, (10_000 * R7) as u128); }
    c.post(1, 1, None);
    assert!(c.peg_status().at_peg);
    e.mock_all_auths();
    let p: Option<(u128, u128)> = call(e, &c.ctrl, "cache_price", vec![c.pyusd.asset.into_val(e)]);
    assert_eq!(p.map(|x| x.0), Some(100_000_000_000_000));
    finish(&c, "group3-depeg.json");
}

/// Kill path on the real Sentora bytes: guardian emergency_exit pulls principal out of the real XLM vault and stops
/// investing; a paused Sentora keeps the kill but defers the pull-out until a retry; drain detection is permissionless.
#[test]
fn sentora_kill_path_on_fork() {
    let c = setup(2);
    let e = &c.e;
    let lender = Address::generate(e);
    fund(e, &c.xlm.asset, &lender, 200_000 * R7);
    c.tick();
    c.deposit(&c.xlm, &lender, 200_000 * R7);
    let s = strat::Client::new(e, &c.xlm.strat);
    let st = s.get_state();
    assert_eq!(st.principal as i128, 100_000 * R7, "absolute cap 100k XLM (idle buffer would allow 160k)");
    let guardian = Address::generate(e);
    e.mock_all_auths();
    s.set_guardian(&c.admin, &Some(guardian.clone()));
    // Sentora paused by its admin: the kill sticks, principal stays until the retry.
    let _: () = call(e, &c.xlm.sentora, "set_status", vec![true.into_val(e)]);
    e.mock_auths(&[MockAuth { address: &guardian, invoke: &MockAuthInvoke { contract: &c.xlm.strat, fn_name: "emergency_exit", args: (guardian.clone(),).into_val(e), sub_invokes: &[] } }]);
    s.emergency_exit(&guardian);
    c.res("emergency_exit (guardian, strict; Sentora paused -> kill only)");
    assert!(s.get_state().killed && s.get_state().principal > 0);
    e.mock_all_auths();
    let _: () = call(e, &c.xlm.sentora, "set_status", vec![false.into_val(e)]);
    e.mock_auths(&[MockAuth { address: &guardian, invoke: &MockAuthInvoke { contract: &c.xlm.strat, fn_name: "emergency_exit", args: (guardian.clone(),).into_val(e), sub_invokes: &[] } }]);
    s.emergency_exit(&guardian);
    c.res("emergency_exit (guardian, strict; full pull-out from real Sentora)");
    assert_eq!(s.get_state().principal, 0);
    // New deposits stay idle.
    fund(e, &c.xlm.asset, &lender, 10_000 * R7);
    c.tick();
    c.deposit(&c.xlm, &lender, 10_000 * R7);
    assert_eq!(s.get_state().principal, 0);
    // Permissionless drain trigger on the USDC adapter.
    let ul = Address::generate(e);
    fund(e, &c.usdc.asset, &ul, 10_000 * R7);
    c.tick();
    c.deposit(&c.usdc, &ul, 10_000 * R7);
    let su = strat::Client::new(e, &c.usdc.strat);
    let anyone = Address::generate(e);
    e.mock_all_auths();
    assert!(su.try_exit_if_drained(&anyone).is_err());
    let live = bal(e, &c.usdc.asset, &c.usdc.sentora);
    let principal = su.get_state().principal as i128;
    fund(e, &c.usdc.asset, &c.usdc.sentora, principal / 2); // model a drain: Sentora holds less than our principal
    let _ = live;
    su.exit_if_drained(&anyone);
    c.res("exit_if_drained (permissionless; drained real Sentora USDC vault)");
    let st = su.get_state();
    assert!(st.killed);
    c.note("drainExit", json!(format!("{st:?}")));
    finish(&c, "group3-kill.json");
}

/// MaxUserMarkets = 2 with the PEG PATH COLD: collateral and debt in both USDC and PYUSD; shortfall from a PYUSD CF
/// cut (24 h timelock), so the liquidation's cold price reads run the peg oracle (both pools + DEX feed).
#[test]
fn two_markets_stables_collateral_and_debt_cold_peg_path() { stables_liquidation(true); }

/// Same position shape, warm caches (the keeper / liquidator pre-warm mitigation).
#[test]
fn two_markets_stables_collateral_and_debt_warm() { stables_liquidation(false); }

fn stables_liquidation(cold: bool) {
    let c = setup(2);
    let e = &c.e;
    let b = open(&c, &[("PYUSD", 1_000 * R7), ("USDC", 500 * R7)], &["USDC", "PYUSD"], 9_800);
    e.mock_all_auths();
    let _: () = call(e, &c.ctrl, "set_market_cf", vec![c.pyusd.rv.into_val(e), 700_000u128.into_val(e)]); // staged
    e.ledger().with_mut(|l| { l.timestamp += 86_401; l.sequence_number += 10; });
    let _: () = call(e, &c.ctrl, "set_market_cf", vec![c.pyusd.rv.into_val(e), 700_000u128.into_val(e)]);
    c.expire();
    let s = liquidate(&c, &b, &c.usdc, &c.pyusd, "2 markets stables, repay USDC, seize PYUSD", cold);
    liquidator_exit(&c, &c.pyusd, s, "PYUSD");
    finish(&c, if cold { "group3-2m-stables-cold.json" } else { "group3-2m-stables-warm.json" });
}

/// Withdraw by an account WITH DEBT in both stables (health check with cold prices, peg path live) that needs more
/// cash than the receipt holds, so the exit runs through the adapter and the real Sentora vault.
#[test]
fn two_markets_stables_withdraw_with_debt_cold_through_sentora() {
    let c = setup(2);
    let e = &c.e;
    let b = open(&c, &[("PYUSD", 1_000 * R7), ("USDC", 5_000 * R7)], &["USDC", "PYUSD"], 5_000);
    c.expire();
    let cash = bal(e, &c.usdc.asset, &c.usdc.rv);
    let s_before = bal(e, &c.usdc.asset, &c.usdc.sentora);
    let pt: u128 = 2_000 * R7 as u128;
    assert!((cash as u128) < pt, "withdrawal exceeds receipt cash ({cash})");
    c.tick();
    c.strict(&b, &c.usdc.rv, "withdraw", (b.clone(), pt).into_val(e), &[]);
    c.res("withdraw USDC 2000 by account with debt in both stables (strict; COLD prices; exit via Sentora)");
    assert!(bal(e, &c.usdc.asset, &c.usdc.sentora) < s_before, "the Sentora exit path ran");
    finish(&c, "group3-2m-withdraw-debt.json");
}

/// With the DEX feed stale (a missed Reflector round), crashing the concentrated pool in-transaction must NOT set the
/// PYUSD price: the two remaining sources disagree, so the oracle returns None rather than the one-pool price.
#[test]
fn stale_feed_and_crashed_concentrated_pool_cannot_price_pyusd_on_fork() {
    let c = setup(2);
    let e = &c.e;
    // Advance past the DEX feed's max age, then publish a fresh USDC/USD round only on the USD Reflector.
    e.ledger().with_mut(|l| l.timestamp += 700);
    let usd_only = Ctx { base: vec![c.base[0].clone()], ..Ctx { e: c.e.clone(), admin: c.admin.clone(), ctrl: c.ctrl.clone(), peg: c.peg.clone(),
        added: c.added.clone(), usdc: c.usdc.clone(), pyusd: c.pyusd.clone(), xlm: c.xlm.clone(), liq: c.liq.clone(), nonce: RefCell::new(1),
        scale: RefCell::new((1, 1, None)), base: vec![], rows: RefCell::new(vec![]), notes: RefCell::new(serde_json::Map::new()) } };
    usd_only.post(1, 1, None);
    let st = c.peg_status();
    assert_eq!(st.missing, 4, "DEX feed stale: {st:?}");
    assert!(st.at_peg, "two healthy pools agree: $1");
    let pool = a(e, POOL);
    let w = whale(&c);
    let mut sold = 0i128;
    for _ in 0..40 { let o = swap(&c, &pool, &w, true, (10_000 * R7) as u128); if o == 0 { break; } sold += 10_000; if quote_usd(&c, &pool, true).map(|v| v < 0.99).unwrap_or(true) { break; } }
    let st = c.peg_status();
    assert!(!st.priced, "one crashed pool must not price PYUSD: {st:?}");
    e.mock_all_auths();
    let p: Option<(u128, u128)> = call(e, &c.ctrl, "cache_price", vec![c.pyusd.asset.into_val(e)]);
    assert_eq!(p, None);
    c.note("staleFeedCrashedPool", json!({"pyusdSold": sold, "status": format!("{st:?}")}));
    c.save("group3-stale-feed.json");
}

/// Stable pool: keep selling (and, separately, buying) until the 1,000-unit quote crosses the threshold or the pool
/// refuses the swap, to measure (not bound) the cost.
#[test]
fn stable_pool_manipulation_until_trip_or_exhaustion() {
    let mut out = serde_json::Map::new();
    for down in [true, false] {
        let c = setup(2);
        let pool = a(&c.e, STABLE_POOL);
        let w = whale(&c);
        let (mut spent, mut got) = (0i128, 0i128);
        let mut q = quote_usd(&c, &pool, down);
        let q0 = q;
        let mut step = 250_000i128;
        let mut refused = false;
        for _ in 0..400 {
            if q.map(|v| if down { v < 0.99 } else { v > 1.01 }).unwrap_or(true) { break; }
            let o = swap(&c, &pool, &w, down, (step * R7) as u128);
            if o == 0 { if step > 1_000 { step /= 5; continue; } refused = true; break; }
            spent += step * R7;
            got += o as i128;
            q = quote_usd(&c, &pool, down);
        }
        let back = swap(&c, &pool, &w, !down, got as u128) as i128;
        let key = if down { "stable down (sell PYUSD; bid < 0.99)" } else { "stable up (buy PYUSD; ask > 1.01)" };
        out.insert(key.into(), json!({"inputUnits": spent as f64 / R7 as f64, "outputUnits": got as f64 / R7 as f64,
            "quoteBefore": format!("{q0:?}"), "quoteAfter": format!("{q:?}"), "poolRefusedFurtherSwaps": refused,
            "inTxRoundTripLossUsd": (spent - back) as f64 / R7 as f64, "durableLossAtParUsd": (spent - got) as f64 / R7 as f64}));
    }
    std::fs::write(format!("{}/.scratch/group3-stable-manipulation.json", dir()), serde_json::to_string_pretty(&out).unwrap()).unwrap();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

/// Permissionless upgrade trigger on the REAL Sentora vault: its admin upgrades the vault to other code (here the
/// escrow's deployed bytes, already on chain); anyone can then fire the kill path.
#[test]
fn sentora_upgrade_trigger_on_fork() {
    let c = setup(2);
    let e = &c.e;
    let lender = Address::generate(e);
    fund(e, &c.usdc.asset, &lender, 10_000 * R7);
    c.tick();
    c.deposit(&c.usdc, &lender, 10_000 * R7);
    let s = strat::Client::new(e, &c.usdc.strat);
    let anyone = Address::generate(e);
    e.mock_all_auths();
    assert!(s.try_exit_if_upgraded(&anyone).is_err(), "pinned code");
    let _: () = call(e, &c.usdc.sentora, "upgrade", vec![h32(e, ESCROW_SHA).into_val(e)]);
    s.exit_if_upgraded(&anyone);
    c.res("exit_if_upgraded (permissionless; real Sentora USDC vault upgraded)");
    let st = s.get_state();
    assert!(st.killed);
    c.note("afterUpgradeKill", json!(format!("{st:?}")));
    c.save("group3-upgrade.json");
}
