// ---------------------------------------------------------------------------------------------
// FRESH-STACK MAINNET EMULATION (option A, 21 Nov 2026). Included into lib.rs after real.rs.
// The ordered fresh-stack ceremony on a capture of LIVE Mainnet state refreshed at the run ledger
// (target/clg/fresh/state, `state-driver.cjs refresh`), with the PUBLIC network id. The real.rs
// rows used the Testnet id and non-Mainnet builds; these rows exercise every Mainnet gate of the
// FROZEN bytes (vault v2 72286249, receipt StgV2 with stg-mainnet-accepted + stg2-mainnet-accepted,
// group controller debe1a36, cash market 4229b7ef). This is an EMULATION, not an RPC simulation:
// the RPC cannot simulate a call on a contract whose code is not yet on chain. Every step is
// simulated live by the ceremony immediately before it is signed. mock_all_auths; no signing.
// Contract creation happens in the setup step (fresh instances at fixed local addresses; the
// live create cost comes from mainnet-sims.cjs). Each later call is its own transaction.
// ---------------------------------------------------------------------------------------------

const CAFJ: &str = "CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN";
const AQUA: &str = "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK";

struct Fresh { v: String, r: String, c: String, m: String, supplier: String }

fn fresh_create(snap: &LedgerSnapshot, log: &mut StdVec<Value>) -> (LedgerSnapshot, Fresh) {
    let s = step_env(snap); let env = &s.env;
    let caddr = |tag: u8| -> Address {
        let sc = ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(soroban_sdk::xdr::ContractId(soroban_sdk::xdr::Hash([tag; 32]))));
        Address::try_from_val(env, &sc).unwrap()
    };
    let v = env.register_at(&caddr(0xe1), wasm("vault-v2.wasm").as_slice(), ());
    let r = env.register_at(&caddr(0xe2), wasm("receipt-v2.wasm").as_slice(), ());
    let c = env.register_at(&caddr(0xe3), wasm("ctrl-group.wasm").as_slice(), ());
    let m = env.register_at(&caddr(0xe4), wasm("market.wasm").as_slice(), ());
    // Explicit test capital (SAC admin auth host-mocked): owner PYUSD for the first command and the
    // floor seed, owner USDC for the market seed, a supplier's USDC liquidity.
    let o = a(env, O);
    soroban_sdk::token::StellarAssetClient::new(env, &a(env, PYUSD)).mint(&o, &20_000_000_000i128);
    let usdc = soroban_sdk::token::StellarAssetClient::new(env, &a(env, USDC));
    usdc.mint(&o, &100_000_000i128);
    let supplier = Address::generate(env); usdc.mint(&supplier, &10_000_000_000i128);
    let f = Fresh { v: astr(&v), r: astr(&r), c: astr(&c), m: astr(&m), supplier: astr(&supplier) };
    log.push(json!({"fresh_create": {"vault": f.v, "receipt": f.r, "controller": f.c, "usdc_market": f.m}}));
    let mut next = env.to_ledger_snapshot();
    strip_nonces(&mut next);
    for (k, _) in next.ledger_entries.iter() { let x = k.to_xdr_base64(Limits::none()).unwrap(); CREATED.with(|c| c.borrow_mut().insert(x)); }
    next.sequence_number += 1; next.timestamp += 6;
    (next, f)
}

#[test]
fn fresh_mainnet() {
    let r = std::panic::catch_unwind(fresh_inner);
    let n = write_misses();
    std::println!("UNKNOWN_KEYS_FINAL {n}");
    if let Err(e) = r { std::panic::resume_unwind(e); }
}
fn fresh_inner() {
    let (snap0, st) = base_snapshot();
    CREATED.with(|c| c.borrow_mut().clear()); UNKNOWN.with(|u| u.borrow_mut().clear());
    assert_eq!(snap0.network_id, <[u8; 32]>::from(Sha256::digest(b"Public Global Stellar Network ; September 2015")), "public network id");
    let mut log: StdVec<Value> = StdVec::new();
    let (b0, f) = fresh_create(&snap0, &mut log);
    let (v, r, c, m) = (f.v.clone(), f.r.clone(), f.c.clone(), f.m.clone());
    let cl = |x: &String| x.clone();
    let mut calls: StdVec<Call> = StdVec::new();
    use std::rc::Rc;
    let mut push = |label: &str, target: &String, func: &str, args: std::rc::Rc<dyn Fn(&Env) -> Vec<Val>>| {
        let a2 = args.clone(); calls.push(call(label, target, func, move |e| a2(e), true));
    };
    // ---- vault (pilot steps 5-7)
    push("V1 vault.initialize(admin, pool, 0, CALI)", &v, "initialize", Rc::new(|e| (a(e, O), a(e, POOL), 0u32, a(e, ORACLE)).into_val(e)));
    push("V2 vault.set_primary_reward_token(AQUA)", &v, "set_primary_reward_token", Rc::new(|e| (a(e, O), Some(a(e, AQUA))).into_val(e)));
    push("V3 vault.set_max_deploy(1,000 PYUSD)", &v, "set_max_deploy", Rc::new(|e| (a(e, O), 10_000_000_000u128).into_val(e)));
    // ---- receipt init (pilot step 8)
    push("R1 receipt.initialize(PYUSD, 0, 0, admin)", &r, "initialize", Rc::new(|e| (a(e, PYUSD), 0u128, 0u128, a(e, O)).into_val(e)));
    // ---- group controller
    push("G1 ctrl.initialize(admin)", &c, "initialize", Rc::new(|e| (a(e, O),).into_val(e)));
    push("G2 ctrl.set_oracle(CAFJ)", &c, "set_oracle", Rc::new(|e| (a(e, CAFJ),).into_val(e)));
    push("G3 ctrl.set_oracle_asset_symbol(USDC, Other(USDC))", &c, "set_oracle_asset_symbol", Rc::new(|e| (a(e, USDC), Some(Symbol::new(e, "USDC"))).into_val(e)));
    // ---- USDC group lending market (static 8%, 10% cash reserve)
    push("M1 usdc.initialize(USDC, 0, 0, admin)", &m, "initialize", Rc::new(|e| (a(e, USDC), 0u128, 0u128, a(e, O)).into_val(e)));
    push("M2 usdc.set_min_cash_reserve_bps(1000)", &m, "set_min_cash_reserve_bps", Rc::new(|e| (a(e, O), 1_000u32).into_val(e)));
    push("M2b usdc.set_cash_borrowing_enabled(true)", &m, "set_cash_borrowing_enabled", Rc::new(|e| (a(e, O), true).into_val(e)));
    push("M3 usdc.set_borrow_rate(8%)", &m, "set_borrow_rate", Rc::new(|e| (80_000u128,).into_val(e)));
    push("M4 usdc.enable_static_rates", &m, "enable_static_rates", Rc::new(|e| (a(e, O),).into_val(e)));
    push("M5 usdc.seed_minimum_supply(0.001 USDC)", &m, "seed_minimum_supply", Rc::new(|e| (a(e, O), 10_000u128).into_val(e)));
    { let c2 = cl(&c); push("M6 usdc.set_peridottroller(group ctrl)", &m, "set_peridottroller", Rc::new(move |e| (a(e, &c2),).into_val(e))); }
    { let m2 = cl(&m); push("G4 ctrl.add_market(usdc market)", &c, "add_market", Rc::new(move |e| (a(e, &m2),).into_val(e))); }
    // ---- receipt wiring (pilot steps 9-16, controller = group)
    { let r2 = cl(&r); push("G5 ctrl.add_market(receipt)", &c, "add_market", Rc::new(move |e| (a(e, &r2),).into_val(e))); }
    { let r2 = cl(&r); push("G6 ctrl.set_pause_borrow(receipt, true)", &c, "set_pause_borrow", Rc::new(move |e| (a(e, &r2), true).into_val(e))); }
    { let c2 = cl(&c); push("R2 receipt.set_peridottroller(group ctrl)", &r, "set_peridottroller", Rc::new(move |e| (a(e, &c2),).into_val(e))); }
    push("R3 receipt.set_min_cash_reserve_bps(10000)", &r, "set_min_cash_reserve_bps", Rc::new(|e| (a(e, O), 10_000u32).into_val(e)));
    push("R4 receipt.set_supply_cap(1 PYUSD floor)", &r, "set_supply_cap", Rc::new(|e| (10_000_000u128,).into_val(e)));
    push("R5 receipt.seed_minimum_supply(1 PYUSD)", &r, "seed_minimum_supply", Rc::new(|e| (a(e, O), 10_000_000u128).into_val(e)));
    // ---- custody binding and staged activation (pilot steps 17-20)
    { let r2 = cl(&r); push("V4 vault.stage_bind(admin, receipt)", &v, "stage_bind", Rc::new(move |e| (a(e, O), a(e, &r2)).into_val(e))); }
    push("V5 vault.stage_configure_guardian(admin)", &v, "stage_configure_guardian", Rc::new(|e| (a(e, O), a(e, O)).into_val(e)));
    push("V6 vault.stage_set_invest_keeper(keeper)", &v, "stage_set_invest_keeper", Rc::new(|e| (a(e, O), a(e, KEEPER)).into_val(e)));
    { let v2 = cl(&v); push("R6 receipt.stage_activate(admin, vault, admin, cap 1,000 PYUSD)", &r, "stage_activate", Rc::new(move |e| (a(e, O), a(e, &v2), a(e, O), 10_000_000_000u128).into_val(e))); }
    // ---- StgV2 and group configuration
    { let c2 = cl(&c); push("R7 receipt.stage2_activate(admin, {ctrl, pool, CALI, USDC})", &r, "stage2_activate", Rc::new(move |e| {
        let cfg = Stg2Config { controller: a(e, &c2), pool: a(e, POOL), oracle: a(e, ORACLE), usdc: a(e, USDC) }; (a(e, O), cfg).into_val(e) })); }
    { let r2 = cl(&r); push("G7 ctrl.group_set_sleeve(receipt)", &c, "group_set_sleeve", Rc::new(move |e| (a(e, &r2),).into_val(e))); }
    push("G8 ctrl.group_set_liq_cache_only(true)", &c, "group_set_liq_cache_only", Rc::new(|e| (true,).into_val(e)));
    { let r2 = cl(&r); push("G9 ctrl.set_market_cf(receipt, 50%) first set (immediate)", &c, "set_market_cf", Rc::new(move |e| (a(e, &r2), 500_000u128).into_val(e))); }
    push("G10 ctrl.set_pause_guardian(admin)", &c, "set_pause_guardian", Rc::new(|e| (a(e, O),).into_val(e)));
    drop(push);
    let (cer, s1, ok1) = seq("FRESH ceremony on live Mainnet state (public network id)", &b0, &calls);
    let mut results = vec![cer];
    let mut extra = json!({});
    if ok1 {
        // ---- first command (open -> convert -> mint -> 3 keeper checkpoints -> invest -> finalise)
        let q = read(&s1, { let v2 = cl(&v); move |env| tv(env, &v2, "stage_quote_open", (1_000_000_000u128, 20u32, 5000u32).into_val(env)) });
        log.push(json!({"fresh_open_quote_100_pyusd": q}));
        if q.get("swap_in").is_some() {
            let (swap_in, min_swap, min_shares, reserve) = (u(&q["swap_in"]), u(&q["min_swap"]), u(&q["min_shares"]), u(&q["reserve"]));
            let deadline = s1.timestamp + 3000;
            let mut fl: StdVec<Call> = vec![call("F1 receipt.stage_open(owner, 100 PYUSD)", &r, "stage_open", move |e| (a(e, O), 1_000_000_000u128, swap_in, min_swap, min_shares, reserve, deadline).into_val(e), true),
                call("F2 stage_convert(0)", &r, "stage_convert", |e| (0u64,).into_val(e), true),
                call("F3 stage_mint(0)", &r, "stage_mint", |e| (0u64,).into_val(e), true)];
            for g in ["F4 keeper checkpoint 1", "F5 keeper checkpoint 2", "F6 keeper checkpoint 3"] { fl.push(call(g, &v, "stage_checkpoint_investment", |e| (a(e, KEEPER),).into_val(e), true)); }
            fl.push(call("F7 stage_invest(0)", &r, "stage_invest", |e| (0u64,).into_val(e), true));
            fl.push(call("F8 stage_finalise(0)", &r, "stage_finalise", |e| (0u64,).into_val(e), true));
            let (rows, s2, ok2) = seq("FRESH first command (owner 100 PYUSD) on the fresh stack", &s1, &fl);
            results.push(rows);
            if ok2 {
                let (m2, c2, r2, sup) = (cl(&m), cl(&c), cl(&r), cl(&f.supplier));
                let bl: StdVec<Call> = vec![
                    call("B0 supplier deposits 1,000 USDC (setup)", &m, "deposit", move |e| (a(e, &sup), 10_000_000_000u128).into_val(e), false),
                    call("B1 receipt.checkpoint_nav()", &r, "checkpoint_nav", |e| Vec::new(e), true),
                    call("B2 ctrl.enter_market(owner, receipt)", &c, "enter_market", move |e| (a(e, O), a(e, &r2)).into_val(e), true),
                    call("B2b ctrl.enter_market(owner, usdc market)", &c, "enter_market", { let m3 = cl(&m); move |e| (a(e, O), a(e, &m3)).into_val(e) }, true),
                    call("B3 ctrl.cache_price(USDC) via CAFJ", &c, "cache_price", |e| (a(e, USDC),).into_val(e), true),
                    call("B4 usdc.borrow(owner, 10 USDC)", &m, "borrow", |e| (a(e, O), 100_000_000u128).into_val(e), true),
                    call("B5 ctrl.account_liquidity(owner)", &c2, "account_liquidity", |e| (a(e, O),).into_val(e), true),
                    call("B6 usdc.repay(owner, all)", &m2, "repay", |e| (a(e, O), 100_100_000u128).into_val(e), true),
                ];
                let (rows, _s3, _ok3) = seq("FRESH borrow path on the fresh stack (CAFJ USDC price)", &s2, &bl);
                results.push(rows);
            }
        } else { extra = json!({"open_quote_unavailable": q}); }
    }
    // Fee estimate per measured row (SDK pubnet fee snapshot, conservative rent).
    let out = json!({"schema": "clg-fresh-mainnet-emulation-v1", "state_ledger": st["ledger"], "state_close_time": st["closeTime"], "network": "public",
        "results": results, "log": log, "extra": extra});
    std::fs::write(format!("{}/fresh-result.json", sdir()), serde_json::to_string_pretty(&out).unwrap()).unwrap();
}
