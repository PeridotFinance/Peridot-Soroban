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
                    call("B6 usdc.repay_max(owner)", &m2, "repay_max", |e| (a(e, O),).into_val(e), true),
                ];
                let (rows, s3, ok3) = seq("FRESH borrow path on the fresh stack (CAFJ USDC price)", &s2, &bl);
                results.push(rows);
                if ok3 {
                    let only_close = std::env::var("FRESH_ONLY_CLOSE").map(|v| v == "1").unwrap_or(false);
                    if !only_close { fresh_exit_rows(&s2, &s3, &f, &mut results); fresh_liq_rows(&s2, &f, &mut results, &mut log); }
                    fresh_close_rows(&s2, &s3, &f, &mut results, &mut log);
                }
                if ok3 && std::path::Path::new(&format!("{}/vault-v2-marker.wasm", wdir())).exists() {
                    fresh_upgrade_rows(&s3, &f, &mut results, &mut log);
                }
            }
        } else { extra = json!({"open_quote_unavailable": q}); }
    }
    let out = json!({"schema": "clg-fresh-mainnet-emulation-v1", "state_ledger": st["ledger"], "state_close_time": st["closeTime"], "network": "public",
        "results": results, "log": log, "extra": extra});
    std::fs::write(format!("{}/fresh-result.json", sdir()), serde_json::to_string_pretty(&out).unwrap()).unwrap();
}

/// Upload a code blob in a setup step (no measurement). Returns (next snapshot, wasm hash).
fn fresh_upload(snap: &LedgerSnapshot, code: &[u8]) -> (LedgerSnapshot, [u8; 32]) {
    let s = step_env(snap); let env = &s.env;
    let h = env.deployer().upload_contract_wasm(soroban_sdk::Bytes::from_slice(env, code)).to_array();
    let mut next = env.to_ledger_snapshot();
    strip_nonces(&mut next);
    for (k, _) in next.ledger_entries.iter() { let x = k.to_xdr_base64(Limits::none()).unwrap(); CREATED.with(|c| c.borrow_mut().insert(x)); }
    next.sequence_number += 1; next.timestamp += 6;
    (next, h)
}

/// Vault v2 upgrade gate on the fresh stack, public network id: propose -> timelock -> pause ->
/// upgrade_wasm(v2' marker) -> unpause -> a real second command; state preserved. Also cancel.
/// The proposal is placed 24 h + 60 s in the PAST (snapshot clock moved back for that step only)
/// so the post-upgrade command still sees fresh live oracle and pool state; the propose step reads
/// no time-dependent external state.
fn fresh_upgrade_rows(s3: &LedgerSnapshot, f: &Fresh, results: &mut StdVec<Value>, log: &mut StdVec<Value>) {
    let (s4, h) = fresh_upload(s3, &wasm("vault-v2-marker.wasm"));
    log.push(json!({"marker_hash": hex::encode(h)}));
    let v = f.v.clone(); let r = f.r.clone();
    let hb = move |e: &Env| soroban_sdk::BytesN::<32>::from_array(e, &h);
    let state = |snap: &LedgerSnapshot| { let (v, r) = (v.clone(), r.clone()); read(snap, move |env| json!({
        "vault_total_supply": tv(env, &v, "total_supply", Vec::new(env)),
        "vault_liquidity": tv(env, &v, "get_position_liquidity", Vec::new(env)),
        "vault_beneficial_owner": tv(env, &v, "stage_beneficial", (a(env, O),).into_val(env)),
        "vault_ticks": tv(env, &v, "get_ticks", Vec::new(env)),
        "vault_receipt": tv(env, &v, "stage_receipt", Vec::new(env)),
        "vault_reward_totals": tv(env, &v, "reward_totals", Vec::new(env)),
        "receipt_units_owner": tv(env, &r, "stage_units", (a(env, O),).into_val(env)),
        "receipt_stage_state": tv(env, &r, "stage_state", Vec::new(env)),
        "receipt_stage2_config": tv(env, &r, "stage2_config", Vec::new(env)),
    })) };
    let before = state(&s4);
    // Control: the same second command on the PRE-upgrade vault (attributes the second-mint footprint).
    {
        let q = read(&s4, { let v2 = v.clone(); move |env| tv(env, &v2, "stage_quote_open", (1_000_000_000u128, 20u32, 5000u32).into_val(env)) });
        if q.get("swap_in").is_some() {
            let (si, ms, mh, rs) = (u(&q["swap_in"]), u(&q["min_swap"]), u(&q["min_shares"]), u(&q["reserve"]));
            let dl = s4.timestamp + 3000;
            let (rows, _, _) = seq("FRESH CONTROL second command on the PRE-upgrade vault (P1-P3)", &s4, &[
                call("K1 receipt.stage_open(owner, 100 PYUSD) pre-upgrade", &r, "stage_open", move |e| (a(e, O), 1_000_000_000u128, si, ms, mh, rs, dl).into_val(e), true),
                call("K2 stage_convert(1)", &r, "stage_convert", |e| (1u64,).into_val(e), true),
                call("K3 stage_mint(1)", &r, "stage_mint", |e| (1u64,).into_val(e), true)]);
            results.push(rows);
        }
    }
    // ---- cancel branch
    {
        let mut sp = s4.clone(); sp.timestamp -= 86_460;
        let (h1, h2) = (hb.clone(), hb.clone());
        let (rows, _, _) = seq("FRESH UPG-C propose then cancel_upgrade; upgrade_wasm then fails", &sp, &[
            call("UC1 vault.propose_upgrade_wasm(admin, v2')", &v, "propose_upgrade_wasm", move |e| (a(e, O), h1(e)).into_val(e), true),
            call("UC2 vault.cancel_upgrade(admin)", &v, "cancel_upgrade", |e| (a(e, O),).into_val(e), true),
            call("UC3 vault.get_pending_upgrade() = None", &v, "get_pending_upgrade", |e| Vec::new(e), true),
            call("UC4 vault.set_paused(admin, true)", &v, "set_paused", |e| (a(e, O), true).into_val(e), true),
            call("UC5 vault.upgrade_wasm after cancel -> revert", &v, "upgrade_wasm", move |e| (a(e, O), h2(e)).into_val(e), true).after(86_460).fails(),
        ]);
        results.push(rows);
    }
    // ---- propose (24 h + 60 s ago), early execute refused
    let mut sp = s4.clone(); let t0 = sp.timestamp; sp.timestamp = t0 - 86_460;
    let (h1, h2) = (hb.clone(), hb.clone());
    let (rows, mut sq, ok) = seq("FRESH UPG-1 propose (T-24h-60s), pause; execute before ETA refused", &sp, &[
        call("U1 vault.propose_upgrade_wasm(admin, v2')", &v, "propose_upgrade_wasm", move |e| (a(e, O), h1(e)).into_val(e), true),
        call("U1b vault.get_pending_upgrade()", &v, "get_pending_upgrade", |e| Vec::new(e), true),
        call("U1c vault.set_paused(admin, true)", &v, "set_paused", |e| (a(e, O), true).into_val(e), true),
        call("U2 vault.upgrade_wasm before ETA (paused) -> revert timelocked", &v, "upgrade_wasm", move |e| (a(e, O), h2(e)).into_val(e), true).fails(),
    ]);
    results.push(rows);
    if !ok { return; }
    sq.timestamp = t0 + 6;
    let (h3, h4, h5) = (hb.clone(), hb.clone(), hb.clone());
    let (rows, sq2, ok) = seq("FRESH UPG-2 after ETA: unpause; unpaused execute refused", &sq, &[
        call("U3a vault.set_paused(admin, false)", &v, "set_paused", |e| (a(e, O), false).into_val(e), true),
        call("U3 vault.upgrade_wasm unpaused -> revert", &v, "upgrade_wasm", move |e| (a(e, O), h3(e)).into_val(e), true).fails()]);
    results.push(rows);
    if !ok { return; }
    let (rows, s5, ok) = seq("FRESH UPG-3 pause; wrong hash refused", &sq2, &[
        call("U5 vault.set_paused(admin, true)", &v, "set_paused", |e| (a(e, O), true).into_val(e), true),
        call("U4 vault.upgrade_wasm wrong hash -> revert", &v, "upgrade_wasm", |e| (a(e, O), soroban_sdk::BytesN::<32>::from_array(e, &[7u8; 32])).into_val(e), true).fails(),
    ]);
    results.push(rows);
    if !ok { return; }
    let (rows, s5, ok) = seq("FRESH UPG-4 upgrade_wasm(v2'), marker, unpause", &s5, &[
        call("U6 vault.upgrade_wasm(admin, v2')", &v, "upgrade_wasm", move |e| (a(e, O), h4(e)).into_val(e), true),
        call("U7 vault.upgrade_marker() = 2", &v, "upgrade_marker", |e| Vec::new(e), true),
        call("U8 vault.get_pending_upgrade() = None", &v, "get_pending_upgrade", |e| Vec::new(e), true),
        call("U9 vault.set_paused(admin, false)", &v, "set_paused", |e| (a(e, O), false).into_val(e), true),
    ]);
    let _ = h5;
    results.push(rows);
    if !ok { return; }
    let after = state(&s5);
    log.push(json!({"upgrade_state_before": before, "upgrade_state_after": after, "state_preserved": before == after}));
    // ---- a real second command after the upgrade (id 1), plus a NAV checkpoint
    let q = read(&s5, { let v2 = v.clone(); move |env| tv(env, &v2, "stage_quote_open", (1_000_000_000u128, 20u32, 5000u32).into_val(env)) });
    if q.get("swap_in").is_none() { log.push(json!({"post_upgrade_quote_unavailable": q})); return; }
    let (swap_in, min_swap, min_shares, reserve) = (u(&q["swap_in"]), u(&q["min_swap"]), u(&q["min_shares"]), u(&q["reserve"]));
    let deadline = s5.timestamp + 3000;
    let mut fl: StdVec<Call> = vec![call("P1 receipt.stage_open(owner, 100 PYUSD) after upgrade", &r, "stage_open", move |e| (a(e, O), 1_000_000_000u128, swap_in, min_swap, min_shares, reserve, deadline).into_val(e), true),
        call("P2 stage_convert(1)", &r, "stage_convert", |e| (1u64,).into_val(e), true),
        call("P3 stage_mint(1)", &r, "stage_mint", |e| (1u64,).into_val(e), true)];
    for g in ["P4 keeper checkpoint 1", "P5 keeper checkpoint 2", "P6 keeper checkpoint 3"] { fl.push(call(g, &v, "stage_checkpoint_investment", |e| (a(e, KEEPER),).into_val(e), true)); }
    fl.push(call("P7 stage_invest(1)", &r, "stage_invest", |e| (1u64,).into_val(e), true));
    fl.push(call("P8 stage_finalise(1)", &r, "stage_finalise", |e| (1u64,).into_val(e), true));
    fl.push(call("P9 receipt.checkpoint_nav() after upgrade", &r, "checkpoint_nav", |e| Vec::new(e), true));
    let (rows, s6, _) = seq("FRESH UPG-5 real second command on the upgraded vault", &s5, &fl);
    results.push(rows);
    log.push(json!({"post_second_command_state": state(&s6)}));
}

/// USER-REQUIRED emergency-exit rows plus F1/F2, on the COMPILED release bytes, public network id.
/// s2 = owner holds units, not entered, no debt. s3 = owner entered (receipt + USDC market), debt repaid.
/// "Trap" = the contract's code swapped in the local snapshot to the cash-market WASM, so every call to
/// it fails with a recoverable VM error (a dead or badly upgraded dependency).
fn fresh_exit_rows(s2: &LedgerSnapshot, s3: &LedgerSnapshot, f: &Fresh, results: &mut StdVec<Value>) {
    let (v, r, c, m) = (f.v.clone(), f.r.clone(), f.c.clone(), f.m.clone());
    let trap = wasm("market.wasm");
    let units = |snap: &LedgerSnapshot| { let r = r.clone(); u(&read(snap, move |env| tv(env, &r, "stage_units", (a(env, O),).into_val(env)))) };
    let next_id = |snap: &LedgerSnapshot| { let r = r.clone(); u(&read(snap, move |env| tv(env, &r, "stage_state", Vec::new(env)))["next_id"]) as u64 };
    let pauses = |c: &String, mk: &[&String], deposit: bool| -> StdVec<Call> {
        let mut out = StdVec::new();
        for mm in mk { for (fnm, on) in [("set_pause_redeem", true), ("set_pause_deposit", deposit), ("set_pause_borrow", true), ("set_pause_liquidation", true)] {
            if !on { continue; }
            let m2 = (*mm).clone(); out.push(call(&format!("setup {fnm}({})", &m2[..6]), c, fnm, move |e| (a(e, &m2), true).into_val(e), false));
        } }
        out
    };
    let ee = |label: &str, n: u128, waive: bool| call(label, &r, "stage_emergency_exit", move |e| (a(e, O), n, waive).into_val(e), true);
    let pay = |label: &str, id: u64| call(label, &r, "stage_emergency_pay", move |e| (id,).into_val(e), true);
    let r_c = r.clone();
    let exit_m = move |label: &str| { let rr = r_c.clone(); call(label, &c, "exit_market", move |e| (a(e, O), a(e, &rr)).into_val(e), true) };
    let r_c2 = r.clone(); let c2 = f.c.clone();
    let enter_m = move |label: &str| { let rr = r_c2.clone(); call(label, &c2, "enter_market", move |e| (a(e, O), a(e, &rr)).into_val(e), true) };
    let r_c3 = r.clone(); let c3 = f.c.clone();
    let disable = move |label: &str| { let rr = r_c3.clone(); call(label, &c3, "emergency_disable_collateral", move |e| (a(e, &rr), true).into_val(e), true) };
    // borrower snapshot sB: s2 + supplier liquidity, NAV, enter both, cache, borrow 10 USDC
    let (m1, c1, r1, sup) = (m.clone(), f.c.clone(), r.clone(), f.supplier.clone());
    let (m1b, r1b) = (m.clone(), r.clone());
    let (rows, sb, okb) = seq("FRESH EX setup: borrower with debt (enter both, borrow 10 USDC)", s2, &[
        call("setup supplier deposit", &m, "deposit", move |e| (a(e, &sup), 10_000_000_000u128).into_val(e), false),
        call("setup checkpoint_nav", &r, "checkpoint_nav", |e| Vec::new(e), false),
        call("setup enter receipt", &c1, "enter_market", move |e| (a(e, O), a(e, &r1)).into_val(e), false),
        call("setup enter usdc", &f.c, "enter_market", move |e| (a(e, O), a(e, &m1)).into_val(e), false),
        call("setup cache_price(USDC)", &f.c, "cache_price", |e| (a(e, USDC),).into_val(e), false),
        call("setup borrow 10 USDC", &m1b, "borrow", |e| (a(e, O), 100_000_000u128).into_val(e), false)]);
    results.push(rows);
    let _ = r1b;
    let n2 = units(s2); let n3 = units(s3);
    // X1: entered debt-free owner: refused while entered; exit_market (oracle-free, 3,600 s later) then exit + pay
    { let id = next_id(s3);
      let (rows, x, ok) = seq("FRESH EX1a entered debt-free emergency exit refused (#2116 expected)", s3, &[ee("EX1a stage_emergency_exit while entered -> #2116", n3, true).fails()]);
      results.push(rows);
      if ok { let mut x = x; x.timestamp += 3_600;
        let (rows, _, _) = seq("FRESH EX1b exit_market (oracle stale) then emergency exit and pay", &x, &[exit_m("EX1b ctrl.exit_market(owner, receipt) 3,600 s later", ), ee("EX1c stage_emergency_exit(all)", n3, true), pay("EX1d stage_emergency_pay (permissionless)", id)]);
        results.push(rows); } }
    // X2: entered owner WITH debt: emergency exit refused, exit_market refused
    if okb {
        let (rows, _, _) = seq("FRESH EX2a USER-REQUIRED entered with debt: emergency exit refused (#2116)", &sb, &[ee("EX2a stage_emergency_exit with debt -> #2116", n2 / 10, true).fails()]);
        results.push(rows);
        let (rows, _, _) = seq("FRESH EX2b entered with debt: exit_market refused", &sb, &[exit_m("EX2b ctrl.exit_market with debt -> revert").fails()]);
        results.push(rows);
    }
    // X3: NOT-entered holder; StgV1 deposit latch, every controller pause, CF 0, oracle and pool trapped, +10,000 s
    { let mut calls = vec![call("setup receipt.stage_pause_deposits(guardian)", &r, "stage_pause_deposits", |e| (a(e, O),).into_val(e), false)];
      calls.extend(pauses(&f.c, &[&r, &m], true));
      calls.push(disable("setup ctrl.emergency_disable_collateral(receipt)"));
      let (rows, x, ok) = seq("FRESH EX3 setup: all pauses + CF 0 (not-entered holder)", s2, &calls);
      results.push(rows);
      if ok { let x = swap_code(&x, ORACLE, &trap); let mut x = x; x.timestamp += 10_000; let id = next_id(&x);
        let (rows, _, _) = seq("FRESH EX3 USER-REQUIRED not-entered holder exits while paused, oracle trapped and stale, CF 0 (a fully dead POOL makes any extraction impossible: inherent)", &x, &[
            ee("EX3a stage_emergency_exit(half)", n2 / 2, true), pay("EX3b stage_emergency_pay", id), ee("EX3c stage_emergency_exit(rest, no waiver)", n2 - n2 / 2, false)]);
        results.push(rows); } }
    // X4: ENTERED debt-free holder under pauses + broken oracle, then again at CF 0
    { let mut calls = vec![call("setup receipt.stage_pause_deposits(guardian)", &r, "stage_pause_deposits", |e| (a(e, O),).into_val(e), false)];
      calls.extend(pauses(&f.c, &[&r, &m], false));
      let (rows, x, ok) = seq("FRESH EX4 setup: pauses (entered debt-free holder)", s3, &calls);
      results.push(rows);
      if ok { let x = swap_code(&x, ORACLE, &trap); let mut x = x; x.timestamp += 10_000;
        let (rows, _, _) = seq("FRESH EX4 USER-REQUIRED entered debt-free exits while paused, oracle trapped, and again at CF 0", &x, &[
            exit_m("EX4a ctrl.exit_market"), ee("EX4b stage_emergency_exit(half)", n3 / 2, true), enter_m("EX4c ctrl.enter_market again"),
            disable("EX4d ctrl.emergency_disable_collateral (CF 0)"), exit_m("EX4e ctrl.exit_market at CF 0"), ee("EX4f stage_emergency_exit(rest)", n3 - n3 / 2, true)]);
        results.push(rows); } }
    // F1a: controller trapped: an entered borrower WITH debt can emergency-exit (documented trade-off)
    if okb { let x = swap_code(&sb, &f.c, &trap);
        let (rows, _, _) = seq("FRESH F1a controller TRAPPED: entered borrower with debt emergency-exits (fail-open trade-off)", &x, &[ee("F1a stage_emergency_exit(all) with dead controller", n2, true)]);
        results.push(rows); }
    // F1b: controller answering but fully paused: entered owner refused #2116
    { let calls = pauses(&f.c, &[&r, &m], true);
      let (rows, x, ok) = seq("FRESH F1b setup: every controller pause on (answering controller)", s3, &calls);
      results.push(rows);
      if ok { let (rows, _, _) = seq("FRESH F1b paused but answering controller refuses an entered owner (#2116)", &x, &[ee("F1b stage_emergency_exit -> #2116", n3, true).fails()]); results.push(rows); } }
    // F2: CF 0 does not let an indebted borrower leave the sleeve
    if okb {
        let (rows, x, ok) = seq("FRESH F2 setup: emergency_disable_collateral (CF 0), checkpoint_nav", &sb, &[disable("F2 setup CF 0"), call("F2 setup checkpoint_nav", &r, "checkpoint_nav", |e| Vec::new(e), false)]);
        results.push(rows);
        if ok {
            let dl = x.timestamp + 3000;
            let (rows, _, _) = seq("FRESH F2a CF 0 + debt: prepare_close refused (#2106)", &x, &[call("F2a stage_prepare_close(all) -> #2106", &r, "stage_prepare_close", move |e| (a(e, O), n2, 0u128, dl).into_val(e), true).fails()]);
            results.push(rows);
            let (rows, _, _) = seq("FRESH F2b CF 0 + debt: exit_market refused", &x, &[exit_m("F2b ctrl.exit_market -> revert").fails()]);
            results.push(rows);
            let (rows, _, _) = seq("FRESH F2c CF 0 + debt: emergency exit refused (#2116)", &x, &[ee("F2c stage_emergency_exit -> #2116", n2, true).fails()]);
            results.push(rows);
            let (rows, _, _) = seq("FRESH F2d after full repay: exit_market and emergency exit succeed", &x, &[
                call("F2d usdc.repay_max(owner)", &m, "repay_max", |e| (a(e, O),).into_val(e), true), exit_m("F2e ctrl.exit_market"), ee("F2f stage_emergency_exit(all)", n2, true)]);
            results.push(rows);
        }
    }
    let _ = v;
}

/// EMULATION-ONLY oracle roll: the captured Reflector instance (CALI / CAFJ) re-timestamps its latest
/// round to `t` (same prices). Models a flat feed that keeps publishing (as CALI does); needed because
/// a frozen capture otherwise goes stale 600 s after its last round. Nothing else in the instance changes.
fn roll_oracle(snap: &LedgerSnapshot, contract: &str, t: u64) -> LedgerSnapshot {
    use soroban_sdk::xdr::{ScMapEntry, ScVec};
    let mut next = snap.clone(); let mut done = false;
    let ms = t * 1000;
    for (k, (e, _)) in next.ledger_entries.iter_mut() {
        let LedgerKey::ContractData(d) = &**k else { continue };
        if d.contract.to_string() != contract || !matches!(d.key, ScVal::LedgerKeyContractInstance) { continue; }
        let LedgerEntryData::ContractData(cd) = &mut e.data else { continue };
        let ScVal::ContractInstance(inst) = &mut cd.val else { continue };
        let Some(map) = inst.storage.as_mut() else { continue };
        let name = |k: &ScVal| match k { ScVal::Symbol(s) => s.0.to_utf8_string_lossy(), ScVal::String(s) => s.0.to_utf8_string_lossy(), _ => String::new() };
        let mut entries: StdVec<ScMapEntry> = map.0.to_vec();
        let last = entries.iter().find(|x| name(&x.key) == "last_timestamp").and_then(|x| if let ScVal::U64(v) = x.val { Some(v) } else { None }).expect("last_timestamp");
        for x in entries.iter_mut() {
            if name(&x.key) == "last_timestamp" { x.val = ScVal::U64(ms); }
            if name(&x.key) == "cache" {
                if let ScVal::Vec(Some(v)) = &x.val {
                    let mut items: StdVec<ScVal> = v.0.to_vec();
                    for it in items.iter_mut() {
                        if let ScVal::Vec(Some(pair)) = it { let mut p: StdVec<ScVal> = pair.0.to_vec();
                            if p.first() == Some(&ScVal::U64(last)) { p[0] = ScVal::U64(ms); *it = ScVal::Vec(Some(ScVec(p.try_into().unwrap()))); } }
                    }
                    x.val = ScVal::Vec(Some(ScVec(items.try_into().unwrap())));
                }
            }
        }
        map.0 = entries.try_into().unwrap(); done = true;
    }
    assert!(done, "oracle instance {contract} not found");
    next
}
fn at_time(snap: &LedgerSnapshot, t: u64, roll_cali: bool, roll_cafj: bool) -> LedgerSnapshot {
    let mut s = snap.clone(); s.timestamp = t;
    if roll_cali { s = roll_oracle(&s, ORACLE, t); }
    if roll_cafj { s = roll_oracle(&s, CAFJ, t); }
    s
}

/// Liquidation L1-L5 (NAV path) and the D3 fallback path on the COMPILED release bytes, public network
/// id, live state. Shortfall via the admin's immediate emergency_disable_collateral (a CF cut is
/// timelocked), as in BUILD-REPORT/Testnet. Liquidator = TREASURY (G account with a live USDC trustline).
fn fresh_liq_rows(s2: &LedgerSnapshot, f: &Fresh, results: &mut StdVec<Value>, log: &mut StdVec<Value>) {
    let (v, r, c, m) = (f.v.clone(), f.r.clone(), f.c.clone(), f.m.clone());
    let (sup, m1, m2, r1, r2, m3) = (f.supplier.clone(), m.clone(), m.clone(), r.clone(), r.clone(), m.clone());
    let units_of = |snap: &LedgerSnapshot, who: &'static str| { let r = r.clone(); u(&read(snap, move |env| tv(env, &r, "stage_units", (a(env, who),).into_val(env)))) };
    let ck = |label: &str| call(label, &r, "checkpoint_nav", |e| Vec::new(e), true);
    let fb = |label: &str| call(label, &r, "checkpoint_fallback", |e| Vec::new(e), true);
    let l2 = || call("L2 ctrl.cache_price(USDC) via CAFJ", &c, "cache_price", |e| (a(e, USDC),).into_val(e), true);
    let l3 = |label: &str| { let (mm, rr) = (m.clone(), r.clone()); call(label, &c, "liquidate", move |e| (a(e, O), a(e, &mm), a(e, &rr), 100_000_000u128, a(e, TREASURY)).into_val(e), true) };
    let l4 = |label: &str| call(label, &v, "checkpoint_rewards", |e| Vec::new(e), true);
    let l5 = |id: u64| call(&format!("L5 receipt.sync_seized({id})"), &r, "sync_seized", move |e| (id,).into_val(e), true);
    let ee_liq = |label: &str, n: u128| call(label, &r, "stage_emergency_exit", move |e| (a(e, TREASURY), n, true).into_val(e), true);
    let ee_own = |label: &str, n: u128| call(label, &r, "stage_emergency_exit", move |e| (a(e, O), n, true).into_val(e), true);
    // setup: lender liquidity, NAV, enter both, price, borrow 40 USDC, liquidator funds + approval, CF 0
    let (rows, sl, ok) = seq("FRESH LIQ setup: borrow 40 USDC, liquidator funded and approved, emergency CF 0", s2, &[
        call("setup supplier deposit 1,000 USDC", &m, "deposit", move |e| (a(e, &sup), 10_000_000_000u128).into_val(e), false),
        call("setup checkpoint_nav", &r, "checkpoint_nav", |e| Vec::new(e), false),
        call("setup enter receipt", &c, "enter_market", move |e| (a(e, O), a(e, &r1)).into_val(e), false),
        call("setup enter usdc", &c, "enter_market", move |e| (a(e, O), a(e, &m1)).into_val(e), false),
        call("setup cache_price(USDC)", &c, "cache_price", |e| (a(e, USDC),).into_val(e), false),
        call("setup borrow 40 USDC", &m, "borrow", |e| (a(e, O), 400_000_000u128).into_val(e), false),
        call("setup mint 100 USDC to liquidator", USDC, "mint", |e| (a(e, TREASURY), 1_000_000_000i128).into_val(e), false),
        call("setup liquidator approves USDC market", USDC, "approve", move |e| (a(e, TREASURY), a(e, &m2), 1_000_000_000i128, e.ledger().sequence() + 100_000).into_val(e), false),
        call("setup ctrl.emergency_disable_collateral(receipt)", &c, "emergency_disable_collateral", move |e| (a(e, &r2), true).into_val(e), false)]);
    results.push(rows);
    if !ok { return; }
    let n0 = units_of(&sl, O);
    // ---- NAV path: L1a -> L2 -> L3 -> (liquidator exit before L5 refused) -> L4 -> L5 -> exits
    let (rows, sn, ok) = seq("FRESH LIQ-N NAV path: L1a, L2, L3 liquidate 10 USDC", &sl, &[ck("L1a receipt.checkpoint_nav()"), l2(), l3("L3 ctrl.liquidate(owner, usdc, receipt, 10 USDC, liquidator)")]);
    results.push(rows);
    if ok {
        let seized = units_of(&sn, TREASURY);
        log.push(json!({"nav_path": {"borrower_units_before": n0.to_string(), "borrower_units_after": units_of(&sn, O).to_string(), "liquidator_units": seized.to_string()}}));
        let (rows, _, _) = seq("FRESH LIQ-N liquidator emergency exit BEFORE L5 refused (vault beneficiary not moved)", &sn, &[ee_liq("EXL0 liquidator stage_emergency_exit before L5 -> revert", seized).fails()]);
        results.push(rows);
        let (rows, s5, ok5) = seq("FRESH LIQ-N L4 claim then L5 sync within 60 s (no boundary)", &sn, &[l4("L4 vault.checkpoint_rewards()"), l5(0)]);
        results.push(rows);
        if ok5 {
            let id = { let r = r.clone(); u(&read(&s5, move |env| tv(env, &r, "stage_state", Vec::new(env)))["next_id"]) as u64 };
            let left = units_of(&s5, O);
            let (rows, _, _) = seq("FRESH LIQ-N after L5: liquidator (not entered) emergency-exits seized units; pay", &s5, &[ee_liq("EXL1 liquidator stage_emergency_exit(seized)", seized), call("EXL2 stage_emergency_pay", &r, "stage_emergency_pay", move |e| (id,).into_val(e), true)]);
            results.push(rows);
            let (rows, _, _) = seq("FRESH LIQ-N after L5: entered borrower with debt refused (#2116)", &s5, &[ee_own("EXL3 borrower stage_emergency_exit -> #2116", left).fails()]);
            results.push(rows);
            log.push(json!({"nav_path_after_l5": read(&s5, { let (v, r) = (v.clone(), r.clone()); move |env| json!({
                "vault_beneficial_owner": tv(env, &v, "stage_beneficial", (a(env, O),).into_val(env)), "vault_beneficial_liq": tv(env, &v, "stage_beneficial", (a(env, TREASURY),).into_val(env)),
                "receipt_units_owner": tv(env, &r, "stage_units", (a(env, O),).into_val(env)), "receipt_units_liq": tv(env, &r, "stage_units", (a(env, TREASURY),).into_val(env)),
                "pending_0": tv(env, &r, "stage2_pending", (0u64,).into_val(env)), "last_claim": tv(env, &v, "stage_last_claim", Vec::new(env)) }) })}));
        }
    }
    // ---- D3 path: real pool push, heartbeat chain on the real clock, 9:59 refused, 10:00 admitted, L2, L3, L4, L5, route B
    let trader = { let s = step_env(&sl); let x = Address::generate(&s.env); astr(&x) };
    let (t1, t2) = (trader.clone(), trader.clone());
    let t0 = sl.timestamp + 6;
    let (rows, sp, ok) = seq("FRESH LIQ-D setup: real swap of 2,000,000 PYUSD into the live pool", &at_time(&sl, t0, true, true), &[
        call("setup mint 2,000,000 PYUSD to trader", PYUSD, "mint", move |e| (a(e, &t1), 20_000_000_000_000i128).into_val(e), false),
        call("setup trader swaps PYUSD->USDC", POOL, "swap", move |e| {
            let toks: Vec<Address> = e.invoke_contract(&a(e, POOL), &Symbol::new(e, "get_tokens"), Vec::new(e));
            let i_in: u32 = if toks.get(0).unwrap() == a(e, PYUSD) { 0 } else { 1 };
            (a(e, &t2), i_in, 1 - i_in, 20_000_000_000_000u128, 0u128).into_val(e) }, false)]);
    results.push(rows);
    if !ok { return; }
    let since = sp.timestamp; // first observation lands here
    let oracle_t = since;     // both feeds re-timestamped to the chain start (CALI age reaches exactly 600 s at admission)
    let mut cur = roll_oracle(&roll_oracle(&sp, ORACLE, oracle_t), CAFJ, oracle_t);
    let mut obs = StdVec::new();
    for k in 0..12u64 { // observations at since + 0, 50, ..., 550
        let mut x = cur.clone(); x.timestamp = since + 50 * k;
        let (rows, nx, ok) = seq(&format!("FRESH LIQ-D heartbeat checkpoint_nav at chain t+{}", 50 * k), &x, &[ck(&format!("D3 heartbeat {k}: checkpoint_nav (blocked -> 0)"))]);
        obs.push(rows["rows"][0]["result"].clone());
        if k == 0 || k == 11 { results.push(rows); }
        if !ok { results.push(json!({"sequence": "FRESH LIQ-D heartbeat failed", "ok": false, "k": k})); return; }
        cur = nx;
    }
    log.push(json!({"d3_heartbeat_results": obs, "d3_status": read(&cur, { let r = r.clone(); move |env| tv(env, &r, "stage2_status", Vec::new(env)) })}));
    // chain-gap branch: a 61 s gap after the last observation -> #2113
    { let x = at_time(&cur, since + 550 + 61, true, false); // CALI re-timestamped so the gap, not oracle age, decides
      let (rows, _, _) = seq("FRESH LIQ-D fallback 61 s after the last observation -> #2113 ChainBroken", &x, &[fb("L1b checkpoint_fallback after a 61 s gap -> #2113").fails()]); results.push(rows); }
    // 9:59 refused
    { let mut x = cur.clone(); x.timestamp = since + 599;
      let (rows, _, _) = seq("FRESH LIQ-D fallback at 9:59 -> #2112 BlockTooRecent", &x, &[fb("L1b checkpoint_fallback at 599 s -> #2112").fails()]); results.push(rows); }
    // 10:00 admitted, then L2 (CAFJ re-timestamped to the admission instant), L3
    let mut x = cur.clone(); x.timestamp = since + 600;
    let (rows, sf, ok) = seq("FRESH LIQ-D fallback at 10:00 admitted", &x, &[fb("L1b checkpoint_fallback at 600 s")]);
    results.push(rows);
    if !ok { return; }
    let sf = roll_oracle(&sf, CAFJ, sf.timestamp);
    let (rows, sq, ok) = seq("FRESH LIQ-D L2, L3 liquidate on the fallback", &sf, &[l2(), l3("L3 ctrl.liquidate on the fallback (10 USDC)")]);
    results.push(rows);
    if !ok { return; }
    let seized = units_of(&sq, TREASURY);
    // L4 claim, then L5 MORE than 60 s later -> D6 boundary opens
    let (rows, s4, ok) = seq("FRESH LIQ-D L4 claim, then L5 71 s later (opens the D6 boundary)", &sq, &[l4("L4 vault.checkpoint_rewards()"), l5(0).after(65)]);
    results.push(rows);
    if !ok { return; }
    let opened = rows_last_result(results);
    log.push(json!({"d3_l5_opened_boundary": opened, "d3_seized_units": seized.to_string(), "fallback_status": read(&s4, { let r = r.clone(); move |env| tv(env, &r, "stage2_status", Vec::new(env)) })}));
    // liquidator emergency exit after L5 on the D3 path
    { let id = { let r = r.clone(); u(&read(&s4, move |env| tv(env, &r, "stage_state", Vec::new(env)))["next_id"]) as u64 };
      let (rows, _, _) = seq("FRESH LIQ-D after L5: liquidator emergency-exits seized units; pay", &s4, &[ee_liq("EXL4 liquidator stage_emergency_exit(seized)", seized), call("EXL5 stage_emergency_pay", &r, "stage_emergency_pay", move |e| (id,).into_val(e), true)]);
      results.push(rows); }
    // route B: a claim one hour later seals the quarantined interval; propose -> 24 h -> attribute -> prepare -> settle each holder
    let mut x = s4.clone(); x.timestamp += 3_600; x.sequence_number += 720; // venue rewards accrue per ledger
    let (rows, sc, ok) = seq("FRESH LIQ-D route B: claim 1 h later seals quarantine", &x, &[l4("L4' vault.checkpoint_rewards() 1 h later")]);
    results.push(rows);
    if !ok { return; }
    let info = read(&sc, { let v = v.clone(); move |env| {
        let cnt: u64 = env.invoke_contract::<Option<u64>>(&a(env, &v), &Symbol::new(env, "stage_last_claim"), Vec::new(env)).unwrap_or(0);
        json!({"totals": tv(env, &v, "reward_totals", Vec::new(env)), "boundary0": tv(env, &v, "reward_boundary", (0u64,).into_val(env)), "last_claim": cnt.to_string()}) } });
    log.push(json!({"route_b_before": info}));
    let interval = info["boundary0"]["interval"].as_str().map(|x| x.parse::<u64>().unwrap()).or_else(|| info["boundary0"]["interval"].as_u64());
    let Some(interval) = interval else { log.push(json!({"route_b": "no boundary 0"})); return; };
    let iv = read(&sc, { let v = v.clone(); move |env| tv(env, &v, "reward_interval", (interval,).into_val(env)) });
    log.push(json!({"route_b_interval": iv}));
    let (Some(seg), Some(recv)) = (iv["first_segment"].as_str().map(|x| x.parse::<u64>().unwrap()), iv["received"].as_str().map(|x| x.parse::<u128>().unwrap())) else { return; };
    if recv == 0 { log.push(json!({"route_b": "quarantine empty: nothing to release"})); }
    let ev = [5u8; 32];
    let (rows, sr, ok) = seq("FRESH LIQ-D route B propose", &sc, &[call("RB1 vault.propose_reward_segment(seg, received, evidence)", &v, "propose_reward_segment", move |e| (seg, recv, soroban_sdk::BytesN::<32>::from_array(e, &ev)).into_val(e), true),
        call("RB2 attribute before 24 h -> revert", &v, "attribute_reward_segment", move |e| (seg, recv, soroban_sdk::BytesN::<32>::from_array(e, &ev)).into_val(e), true).fails()]);
    results.push(rows);
    if !ok { return; }
    let mut x = sr.clone(); x.timestamp += 86_400;
    let cnt = |snap: &LedgerSnapshot, who: &'static str| { let v = v.clone(); u(&read(snap, move |env| tv(env, &v, "reward_history_count", (a(env, who),).into_val(env)))) as u32 };
    let (co, cl) = (cnt(&x, O).saturating_sub(1), cnt(&x, TREASURY).saturating_sub(1));
    let (rows, sz, _) = seq("FRESH LIQ-D route B attribute (24 h), prepare, settle every holder", &x, &[
        call("RB3 vault.attribute_reward_segment", &v, "attribute_reward_segment", move |e| (seg, recv, soroban_sdk::BytesN::<32>::from_array(e, &ev)).into_val(e), true),
        call("RB4 vault.prepare_reward_segment(seg, 4)", &v, "prepare_reward_segment", move |e| (seg, 4u32).into_val(e), true),
        call("RB5 settle_reward_segment(borrower)", &v, "settle_reward_segment", move |e| (a(e, O), seg, co).into_val(e), true),
        call("RB6 settle_reward_segment(liquidator)", &v, "settle_reward_segment", move |e| (a(e, TREASURY), seg, cl).into_val(e), true)]);
    results.push(rows);
    log.push(json!({"route_b_after": read(&sz, { let v = v.clone(); move |env| json!({"totals": tv(env, &v, "reward_totals", Vec::new(env)),
        "pending_owner": tv(env, &v, "reward_account", (a(env, O),).into_val(env)), "pending_liq": tv(env, &v, "reward_account", (a(env, TREASURY),).into_val(env))}) })}));
    let _ = m3;
}
fn rows_last_result(results: &StdVec<Value>) -> Value {
    results.last().and_then(|s| s["rows"].as_array()).and_then(|r| r.last()).map(|r| r["result"].clone()).unwrap_or(Value::Null)
}

/// In-kind exit footprint on the COMPILED release bytes, public network id, live pool state:
/// stage_prepare_close + stage_withdraw (min_swap 0 = in kind) at 10 / 50 / 100 % of the owner's units,
/// and the emergency extract/pay pair, for (a) not entered, no debt; (b) entered, no debt; (c) entered with
/// 10 USDC debt (fresh NAV first). Repeated after real swaps move the live pool across initialised ticks.
fn fresh_close_rows(s2: &LedgerSnapshot, s3: &LedgerSnapshot, f: &Fresh, results: &mut StdVec<Value>, log: &mut StdVec<Value>) {
    let (r, c, m) = (f.r.clone(), f.c.clone(), f.m.clone());
    let units = |snap: &LedgerSnapshot| { let r = r.clone(); u(&read(snap, move |env| tv(env, &r, "stage_units", (a(env, O),).into_val(env)))) };
    let next_id = |snap: &LedgerSnapshot| { let r = r.clone(); u(&read(snap, move |env| tv(env, &r, "stage_state", Vec::new(env)))["next_id"]) as u64 };
    let slot = |snap: &LedgerSnapshot| read(snap, |env| tv(env, POOL, "get_slot0", Vec::new(env)));
    // debt branch base: s2 + supplier liquidity + enter both + price + borrow 10 USDC
    let (sup, r1, m1) = (f.supplier.clone(), r.clone(), m.clone());
    let (rows, sd, okd) = seq("FRESH CLOSE setup: borrower with 10 USDC debt", s2, &[
        call("setup supplier deposit", &m, "deposit", move |e| (a(e, &sup), 10_000_000_000u128).into_val(e), false),
        call("setup checkpoint_nav", &r, "checkpoint_nav", |e| Vec::new(e), false),
        call("setup enter receipt", &c, "enter_market", move |e| (a(e, O), a(e, &r1)).into_val(e), false),
        call("setup enter usdc", &c, "enter_market", move |e| (a(e, O), a(e, &m1)).into_val(e), false),
        call("setup cache_price(USDC)", &c, "cache_price", |e| (a(e, USDC),).into_val(e), false),
        call("setup borrow 10 USDC", &m, "borrow", |e| (a(e, O), 100_000_000u128).into_val(e), false)]);
    results.push(rows);
    // pool states: base, then real swaps by a fresh trader
    let push = |snap: &LedgerSnapshot, pyusd_in: bool, amount: u128| -> Option<LedgerSnapshot> {
        let trader = { let s = step_env(snap); let x = Address::generate(&s.env); astr(&x) };
        let (t1, t2) = (trader.clone(), trader.clone());
        let tok = if pyusd_in { PYUSD } else { USDC };
        let (rows, x, ok) = seq(&format!("FRESH CLOSE pool push: swap {} {}", amount, if pyusd_in { "PYUSD->USDC" } else { "USDC->PYUSD" }), snap, &[
            call("setup mint to trader", tok, "mint", move |e| (a(e, &t1), amount as i128).into_val(e), false),
            call("setup trader swap", POOL, "swap", move |e| {
                let toks: Vec<Address> = e.invoke_contract(&a(e, POOL), &Symbol::new(e, "get_tokens"), Vec::new(e));
                let pyusd_idx: u32 = if toks.get(0).unwrap() == a(e, PYUSD) { 0 } else { 1 };
                let i_in = if pyusd_in { pyusd_idx } else { 1 - pyusd_idx };
                (a(e, &t2), i_in, 1 - i_in, amount, 0u128).into_val(e) }, false)]);
        let _ = rows; if ok { Some(x) } else { None }
    };
    let states: StdVec<(&str, bool, u128)> = vec![("base (live pool)", true, 0), ("after 200,000 PYUSD->USDC", true, 2_000_000_000_000), ("after 2,000,000 PYUSD->USDC", true, 20_000_000_000_000), ("after 2,000,000 USDC->PYUSD", false, 20_000_000_000_000)];
    for (name, dir, amt) in states {
        let pick = |b: &LedgerSnapshot| -> Option<LedgerSnapshot> { if amt == 0 { Some(b.clone()) } else { push(b, dir, amt) } };
        let (Some(p2), Some(p3)) = (pick(s2), pick(s3)) else { log.push(json!({"close_push_failed": name})); continue };
        let pd = if okd { pick(&sd) } else { None };
        log.push(json!({"close_pool_state": name, "slot0": slot(&p2)}));
        for (scen, base, debt) in [("not entered, no debt", Some(p2.clone()), false), ("entered, no debt", Some(p3.clone()), false), ("entered, 10 USDC debt", pd.clone(), true)] {
            let Some(base) = base else { continue };
            let n = units(&base);
            for pct in [10u128, 50, 100] {
                let sh = n * pct / 100; let id = next_id(&base); let dl = base.timestamp + 3000;
                let mut calls = StdVec::new();
                if debt { calls.push(call("setup checkpoint_nav (fresh NAV for the health check)", &r, "checkpoint_nav", |e| Vec::new(e), false)); }
                calls.push(call(&format!("prepare_close {pct}% [{scen}; {name}]"), &r, "stage_prepare_close", move |e| (a(e, O), sh, 0u128, dl).into_val(e), true));
                calls.push(call(&format!("withdraw in kind {pct}% [{scen}; {name}]"), &r, "stage_withdraw", move |e| (id,).into_val(e), true));
                let (rows, _, _) = seq(&format!("FRESH CLOSE {name} | {scen} | {pct}%"), &base, &calls);
                results.push(rows);
            }
            if !debt && scen.starts_with("not entered") {
                for pct in [10u128, 50, 100] {
                    let sh = n * pct / 100; let id = next_id(&base);
                    let (rows, _, _) = seq(&format!("FRESH EMERG {name} | {scen} | {pct}%"), &base, &[
                        call(&format!("emergency extract {pct}% [{name}]"), &r, "stage_emergency_exit", move |e| (a(e, O), sh, true).into_val(e), true),
                        call(&format!("emergency pay {pct}% [{name}]"), &r, "stage_emergency_pay", move |e| (id,).into_val(e), true)]);
                    results.push(rows);
                }
            }
        }
    }
}
