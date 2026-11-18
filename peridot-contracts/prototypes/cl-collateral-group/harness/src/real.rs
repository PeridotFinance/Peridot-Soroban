// ---------------------------------------------------------------------------------------------
// REAL-stack rows (18 Nov 2026, feat/cl-collateral-group). Included into lib.rs after group.rs.
// Production candidates, no doubles: the LIVE vault CD6N and staged receipt CCGK instances (live
// storage from the capture) with their code SWAPPED IN THE LOCAL SNAPSHOT ONLY to vault v2
// (vault-v2.wasm) and the StgV2 receipt (receipt-v2.wasm, default build); a fresh group controller
// (ctrl-group-v2.wasm, patches G0-G9) and a fresh static-rate USDC market (deployed market bytes).
// The snapshot network id is set to Testnet so the default (non-Mainnet) StgV2 build activates;
// no network-dependent value enters any measured row. Real Aquarius pool, Reflector CALI, SACs.
// `REAL_MEASURE=1` runs only the D3 fallback rows against the stg2-measure build (chain seeding).
// ---------------------------------------------------------------------------------------------

#[soroban_sdk::contracttype]
#[derive(Clone)]
pub struct Stg2Config { pub controller: Address, pub pool: Address, pub oracle: Address, pub usdc: Address }
#[soroban_sdk::contracttype]
#[derive(Clone)]
pub struct Stg2Blk { pub since: u64, pub last: u64, pub n: u32 }

/// Replace a live contract's executable in the LOCAL snapshot (instance storage untouched).
fn swap_code(snap: &LedgerSnapshot, contract: &str, code: &[u8]) -> LedgerSnapshot {
    use soroban_sdk::xdr::{ContractExecutable, Hash};
    let s = step_env(snap); let env = &s.env;
    let h = env.deployer().upload_contract_wasm(soroban_sdk::Bytes::from_slice(env, code));
    let mut next = env.to_ledger_snapshot();
    strip_nonces(&mut next);
    let mut swapped = 0;
    for (k, (e, _)) in next.ledger_entries.iter_mut() {
        if let LedgerKey::ContractData(d) = &**k {
            if d.contract.to_string() == contract && matches!(d.key, ScVal::LedgerKeyContractInstance) {
                if let LedgerEntryData::ContractData(cd) = &mut e.data {
                    if let ScVal::ContractInstance(inst) = &mut cd.val { inst.executable = ContractExecutable::Wasm(Hash(h.to_array())); swapped += 1; }
                }
            }
        }
    }
    assert_eq!(swapped, 1, "instance of {contract} not found");
    for (k, _) in next.ledger_entries.iter() { let x = k.to_xdr_base64(Limits::none()).unwrap(); CREATED.with(|c| c.borrow_mut().insert(x)); }
    next.sequence_number += 1; next.timestamp += 6;
    next
}

struct RealG { ctrl: String, usdc_m: String, trader: String }

fn deploy_real(snap: &LedgerSnapshot, log: &mut StdVec<Value>) -> (LedgerSnapshot, RealG) {
    let s = step_env(snap); let env = &s.env;
    let o = a(env, O);
    let usdc = soroban_sdk::token::StellarAssetClient::new(env, &a(env, USDC));
    usdc.mint(&o, &1_000_000_000i128);
    let supplier = Address::generate(env); usdc.mint(&supplier, &50_000_000_000i128);
    let caddr = |tag: u8| -> Address {
        let sc = ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(soroban_sdk::xdr::ContractId(soroban_sdk::xdr::Hash([tag; 32]))));
        Address::try_from_val(env, &sc).unwrap()
    };
    let ctrl = env.register_at(&caddr(0xf1), wasm("ctrl-group-v2.wasm").as_slice(), ());
    let usdc_m = env.register_at(&caddr(0xf2), wasm("market.wasm").as_slice(), ());
    let trader = caddr(0xf9);
    let inv = |c: &Address, f: &str, args: Vec<Val>| { let r = env.try_invoke_contract::<Val, soroban_sdk::Error>(c, &Symbol::new(env, f), args); assert!(matches!(r, Ok(Ok(_))), "real setup {f} failed: {r:?}"); };
    let r = a(env, R);
    let admin: Address = env.invoke_contract(&r, &Symbol::new(env, "get_admin"), Vec::new(env));
    inv(&ctrl, "initialize", (o.clone(),).into_val(env));
    // Local rows price USDC on CALI (its base, 1.0). Proposed Testnet/Mainnet config: CAFJ Other(USDC)
    // (same 2-key footprint per lastprice; DESIGN.md §1.2 [rpc]).
    inv(&ctrl, "set_oracle", (a(env, ORACLE),).into_val(env));
    inv(&usdc_m, "initialize", (a(env, USDC), 0u128, 0u128, o.clone()).into_val(env));
    inv(&usdc_m, "set_min_cash_reserve_bps", (o.clone(), 1_000u32).into_val(env));
    inv(&usdc_m, "set_cash_borrowing_enabled", (o.clone(), true).into_val(env));
    inv(&usdc_m, "set_borrow_rate", (80_000u128,).into_val(env));
    inv(&usdc_m, "enable_static_rates", (o.clone(),).into_val(env));
    inv(&usdc_m, "seed_minimum_supply", (o.clone(), 10_000u128).into_val(env));
    inv(&ctrl, "add_market", (usdc_m.clone(),).into_val(env));
    inv(&usdc_m, "set_peridottroller", (ctrl.clone(),).into_val(env));
    inv(&r, "set_peridottroller", (ctrl.clone(),).into_val(env));
    let cfg = Stg2Config { controller: ctrl.clone(), pool: a(env, POOL), oracle: a(env, ORACLE), usdc: a(env, USDC) };
    inv(&r, "stage2_activate", (admin.clone(), cfg).into_val(env));
    inv(&ctrl, "add_market", (r.clone(),).into_val(env));
    inv(&ctrl, "group_set_sleeve", (r.clone(),).into_val(env));
    inv(&ctrl, "set_market_cf", (r.clone(), 500_000u128).into_val(env));
    inv(&ctrl, "group_set_liq_cache_only", (true,).into_val(env));
    inv(&usdc_m, "deposit", (supplier.clone(), 50_000_000_000u128).into_val(env));
    let g = RealG { ctrl: astr(&ctrl), usdc_m: astr(&usdc_m), trader: astr(&trader) };
    log.push(json!({"deploy_real": {"ctrl": g.ctrl, "usdc_market": g.usdc_m, "receipt": R, "vault": V, "receipt_admin": astr(&admin)}}));
    let mut next = env.to_ledger_snapshot();
    strip_nonces(&mut next);
    for (k, _) in next.ledger_entries.iter() { let x = k.to_xdr_base64(Limits::none()).unwrap(); CREATED.with(|c| c.borrow_mut().insert(x)); }
    next.sequence_number += 1; next.timestamp += 6;
    (next, g)
}

fn real_views(snap: &LedgerSnapshot, g: &RealG) -> Value {
    let (um, ct) = (g.usdc_m.clone(), g.ctrl.clone());
    read(snap, move |env| json!({
        "stage2_status": tv(env, R, "stage2_status", Vec::new(env)),
        "borrower_units": tv(env, R, "stage_units", (a(env, O),).into_val(env)),
        "liquidator_units": tv(env, R, "stage_units", (a(env, TREASURY),).into_val(env)),
        "vault_beneficial_borrower": tv(env, V, "stage_beneficial", (a(env, O),).into_val(env)),
        "vault_beneficial_liquidator": tv(env, V, "stage_beneficial", (a(env, TREASURY),).into_val(env)),
        "pending_0": tv(env, R, "stage2_pending", (0u64,).into_val(env)),
        "borrow_balance": tv(env, &um, "get_user_borrow_balance", (a(env, O),).into_val(env)),
        "account_liquidity": tv(env, &ct, "account_liquidity", (a(env, O),).into_val(env)),
        "vault_last_claim": tv(env, V, "stage_last_claim", Vec::new(env)),
        "slot0": tv(env, POOL, "get_slot0", Vec::new(env)),
    }))
}

fn real_suite(base: &LedgerSnapshot, results: &mut StdVec<Value>, log: &mut StdVec<Value>) {
    let measure = std::env::var("REAL_MEASURE").map(|v| v == "1").unwrap_or(false);
    let pre = if measure { "REALM " } else { "REAL " };
    let mut b = base.clone();
    b.network_id = Sha256::digest(b"Test SDF Network ; September 2015").into();
    let b = swap_code(&b, V, &wasm("vault-v2.wasm"));
    let b = swap_code(&b, R, &wasm(if measure { "receipt-v2-measure.wasm" } else { "receipt-v2.wasm" }));
    if !measure {
        // Baseline: the frozen StgV1 receipt + deployed vault (live code), no StgV2, same state.
        let r = seq("REAL RD0 baseline: live StgV1 receipt + live vault, emergency exit 10% (no StgV2)", base,
            &[call("D stage_emergency_exit 10% (StgV1 baseline)", R, "stage_emergency_exit", |e| {
                let n: u128 = e.invoke_contract(&a(e, R), &Symbol::new(e, "stage_units"), (a(e, O),).into_val(e)); (a(e, O), n / 10, false).into_val(e) }, true)]);
        results.push(r.0);
    }
    let (b0, g) = deploy_real(&b, log);
    let (gct, gum, gtr) = (g.ctrl.clone(), g.usdc_m.clone(), g.trader.clone());
    let units = u(&read(&b0, |env| tv(env, R, "stage_units", (a(env, O),).into_val(env))));
    log.push(json!({"real_borrower_units": units.to_string()}));
    let (r, rbase, ok) = {
        let (c1, c2, m1) = (gct.clone(), gct.clone(), gum.clone());
        seq(&format!("{pre}bind (setup)"), &b0, &[
            call("enter receipt", &c1, "enter_market", move |e| (a(e, O), a(e, R)).into_val(e), false),
            call("enter usdc", &c2, "enter_market", move |e| (a(e, O), a(e, &m1)).into_val(e), false),
        ])
    };
    log.push(r); assert!(ok, "real bind failed");
    log.push(json!({"real_views_base": real_views(&rbase, &g)}));

    let ck = |m: bool| call("L1a receipt.checkpoint_nav", R, "checkpoint_nav", |e| Vec::new(e), m);
    let fb = || call("L1b receipt.checkpoint_fallback", R, "checkpoint_fallback", |e| Vec::new(e), true);
    let warm = || { let c = gct.clone(); call("L2 cache_price(USDC)", &c, "cache_price", |e| (a(e, USDC),).into_val(e), true) };
    let borrow = |label: &str, amt: u128, m: bool| { let x = gum.clone(); call(label, &x, "borrow", move |e| (a(e, O), amt).into_val(e), m) };
    let close = |label: &str, n: u128| call(label, R, "stage_prepare_close", move |e| (a(e, O), n, 0u128, e.ledger().timestamp() + 3000).into_val(e), true);
    let emerg = |label: &str, n: u128| call(label, R, "stage_emergency_exit", move |e| (a(e, O), n, false).into_val(e), true);
    let liq = |label: &str| { let (c, m) = (gct.clone(), gum.clone());
        call(label, &c, "liquidate", move |e| (a(e, O), a(e, &m), a(e, R), 1_000_000_000u128, a(e, TREASURY)).into_val(e), true) };
    let claim = || call("L4 vault.checkpoint_rewards (real AQUA claim)", V, "checkpoint_rewards", |e| Vec::new(e), true);
    let sync = |id: u64| call(&format!("L5 receipt.sync_seized({id}) -> vault.stage_seize_reassign"), R, "sync_seized", move |e| (id,).into_val(e), true);
    let setcf = |x: u128| { let c = gct.clone(); call(&format!("set_market_cf(receipt, {x}) (setup)"), &c, "set_market_cf", move |e| (a(e, R), x).into_val(e), false) };
    let approve = || { let m = gum.clone(); call("liquidator approves USDC market (setup)", USDC, "approve", move |e| (a(e, TREASURY), a(e, &m), 20_000_000_000i128, e.ledger().sequence() + 100_000).into_val(e), false) };
    let cold = |s: &LedgerSnapshot| drop_price_cache(s, &gct);
    // Real pool push: a trader sells 2,000,000 PYUSD into the live concentrated pool (unmeasured setup).
    let push_pool = || -> StdVec<Call> {
        let (t1, t2) = (gtr.clone(), gtr.clone());
        StdVec::from([
            call("mint 2,000,000 PYUSD to trader (setup)", PYUSD, "mint", move |e| (a(e, &t1), 20_000_000_000_000i128).into_val(e), false),
            call("trader swaps PYUSD->USDC in the live pool (setup)", POOL, "swap", move |e| {
                let toks: Vec<Address> = e.invoke_contract(&a(e, POOL), &Symbol::new(e, "get_tokens"), Vec::new(e));
                let i_in: u32 = if toks.get(0).unwrap() == a(e, PYUSD) { 0 } else { 1 };
                (a(e, &t2), i_in, 1 - i_in, 20_000_000_000_000u128, 0u128).into_val(e) }, false),
        ])
    };
    let mut push = |name: &str, b: &LedgerSnapshot, calls: &[Call]| { let r = seq(&format!("{pre}{name}"), b, calls); results.push(r.0); (r.1, r.2) };
    let borrow_amt: u128 = 3_000_000_000;

    // ---- liquidation base (shared by both modes) ----
    // Shortfall: a CF cut is timelocked in the controller, so the rows use the admin's immediate
    // emergency_disable_collateral (CF 0). Seize sizing does not read the CF.
    let disable = || { let c = gct.clone(); call("emergency_disable_collateral(receipt) (setup)", &c, "emergency_disable_collateral", |e| (a(e, R), true).into_val(e), false) };
    let _ = setcf;
    let (r, rl, okl) = seq(&format!("{pre}RL setup: ckpt, borrow 300 USDC, emergency CF 0, approve"), &rbase,
        &[ck(false), borrow("borrow (setup)", borrow_amt, false), disable(), approve()]);
    if !okl { std::eprintln!("REAL_SETUP_FAIL {}", serde_json::to_string(&r).unwrap().chars().take(6000).collect::<String>()); }
    log.push(r); assert!(okl, "real liquidation setup failed");
    log.push(json!({"real_views_liq_base": real_views(&rl, &g)}));
    let rlc = cold(&rl);

    if measure {
        let seed = |since_ago: u64, last_ago: u64, n: u32| call(&format!("seed chain -{since_ago}s/-{last_ago}s (measure build, unmeasured)"), R, "stage2_measure_set_chain", move |e| {
            let t = e.ledger().timestamp() + 6; (Some(Stg2Blk { since: t - since_ago, last: t - last_ago, n }),).into_val(e) }, false);
        let mut calls = push_pool(); calls.extend([seed(599, 59, 10), fb().fails()]);
        push("RF3 pool pushed out of band, chain 9:59 -> fallback reverts #2112", &rl, &calls);
        let mut calls = push_pool(); calls.extend([seed(600, 60, 10), fb(), liq("L3 liquidate on fallback (WARM)")]);
        let (s, ok) = push("RF2 pool pushed, chain 10:00: L1b fallback, L3 WARM", &rl, &calls);
        if ok { log.push(json!({"real_views_after_RF2": real_views(&s, &g)})); }
        let mut calls = push_pool(); calls.extend([seed(600, 60, 10), fb(), warm(), liq("L3 liquidate on fallback"), claim(), sync(0)]);
        push("RF4 pool pushed, chain 10:00: full L1b, L2, L3, L4, L5 from COLD", &rlc, &calls);
        let mut calls = push_pool(); calls.extend([seed(900, 61, 15), fb().fails()]);
        push("RF5 pool pushed, chain lapsed 61 s -> fallback reverts #2113", &rl, &calls);
        return;
    }

    // ---- L1a alone, B: borrow ----
    push("RC1 L1a checkpoint_nav (real receipt + vault v2)", &rbase, &[ck(true)]);
    push("RB0 borrow with no checkpoint -> revert", &rbase, &[borrow("borrow (no checkpoint)", borrow_amt, true).fails()]);
    push("RB1 L1a, borrow, WARM", &rbase, &[ck(true), borrow("B borrow", borrow_amt, true)]);
    push("RB2 L1a, borrow, COLD", &cold(&rbase), &[ck(true), borrow("B borrow", borrow_amt, true)]);
    push("RB3 borrow at checkpoint age 61 s -> revert", &rbase, &[ck(false), borrow("B borrow at age 61", borrow_amt, true).after(55).fails()]);
    push("RB3b borrow at checkpoint age 60 s -> ok", &rbase, &[ck(false), borrow("B borrow at age 60", borrow_amt, true).after(54)]);
    let (rb, okb) = push("RB4 first borrow (base for repeat/exit)", &rbase, &[ck(false), borrow("borrow 1st (setup)", borrow_amt, false)]);
    if okb {
        push("RB5 repeat borrow, WARM", &rb, &[ck(false), borrow("B repeat borrow", 500_000_000, true)]);
        push("RB6 repeat borrow, COLD", &cold(&rb), &[ck(false), borrow("B repeat borrow", 500_000_000, true)]);
        // ---- D: exits of an owner with debt ----
        let (n_ok, n_bad) = (units / 10, units * 9 / 10);
        push("RD1 prepare_close 10% with debt, WARM", &rb, &[ck(false), close("D stage_prepare_close 10%", n_ok)]);
        push("RD2 prepare_close 10% with debt, COLD", &cold(&rb), &[ck(false), close("D stage_prepare_close 10%", n_ok)]);
        push("RD3 prepare_close at checkpoint age 61 s -> revert #2101", &rb, &[ck(false), close("D close at age 61", n_ok).after(55).fails()]);
        push("RD4 prepare_close 90% -> revert #2106 insufficient collateral", &rb, &[ck(false), close("D close 90%", n_bad).fails()]);
        push("RD5 emergency exit while entered (with debt) -> revert #2116", &rb, &[ck(false), emerg("D stage_emergency_exit 10% entered", n_ok).fails()]);
        let _ = n_bad;
    }
    push("RD7 prepare_close 10%, no debt (no health dependency)", &rbase, &[close("D stage_prepare_close 10% no debt", units / 10)]);
    push("RD8 emergency exit while entered, no debt -> revert #2116", &rbase, &[emerg("D stage_emergency_exit 10% entered", units / 10).fails()]);
    { let c = gct.clone();
      push("RD9 exit_market (no debt), then emergency exit 10% (not entered)", &rbase, &[
        call("exit_market(receipt)", &c, "exit_market", |e| (a(e, O), a(e, R)).into_val(e), true),
        emerg("D stage_emergency_exit 10% not entered", units / 10)]);
      let c2 = gct.clone();
      if okb { push("RD10 exit_market with debt -> revert", &rb, &[call("exit_market(receipt) with debt", &c2, "exit_market", |e| (a(e, O), a(e, R)).into_val(e), true).fails()]); } }

    // ---- L: liquidation ----
    push("RL1 L1a, L3 liquidate (cache-only), WARM", &rl, &[ck(true), liq("L3 liquidate (repay + unit move + queue)")]);
    push("RL2 L1a, L3 from COLD -> revert by design (cache-only)", &rlc, &[ck(true), liq("L3 liquidate cold").fails()]);
    push("RL3 L1a, L2, L3 from COLD", &rlc, &[ck(true), warm(), liq("L3 liquidate (repay + unit move + queue)")]);
    let (full, okf) = push("RL4 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (fresh claim)", &rlc,
        &[ck(true), warm(), liq("L3 liquidate (repay + unit move + queue)"), claim(), sync(0)]);
    if okf { log.push(json!({"real_views_after_RL4": real_views(&full, &g)})); }
    push("RL4w full staged sequence from WARM", &rl, &[ck(true), warm(), liq("L3 liquidate"), claim(), sync(0)]);
    push("RL5 L5 sync 61 s after the last claim (opens D6 boundary)", &rl, &[ck(true), liq("L3 liquidate"), sync(0).after(61)]);
    push("RL6 liquidate at NAV checkpoint age 61 s, no fallback -> revert", &rl, &[ck(false), liq("L3 liquidate at age 61").after(55).fails()]);
    push("RL7 L5 before L3 (nothing queued) -> revert #2109", &rl, &[sync(0).fails()]);
    if okf {
        push("RL8 repeat seizure (liquidator already holds units), L3 + L4 + L5", &full, &[ck(true), liq("L3 liquidate (repeat)"), claim(), sync(1)]);
        // Liquidator cannot exit seized units before L5; after L5 it can.
        push("RL9 liquidator closes seized units after L5 -> ok", &full, &[call("liquidator stage_prepare_close", R, "stage_prepare_close", |e| (a(e, TREASURY), 1_000_000u128, 0u128, e.ledger().timestamp() + 3000).into_val(e), true)]);
    }
    push("RL10 liquidator closes seized units BEFORE L5 -> revert (vault availability)", &rl, &[ck(false), liq("L3 liquidate (setup)").fails_if(false),
        call("liquidator stage_prepare_close before sync", R, "stage_prepare_close", |e| (a(e, TREASURY), 1_000_000u128, 0u128, e.ledger().timestamp() + 3000).into_val(e), true).fails()]);
    {
        let (c1, c2) = (gct.clone(), gct.clone());
        let (r, x4, ok) = seq(&format!("{pre}RX4 setup: reserve recipient + 5% liquidation fee"), &rl, &[
            call("set_reserve_recipient (setup)", &c1, "set_reserve_recipient", |e| (a(e, KEEPER),).into_val(e), false),
            call("set_liquidation_fee 5% (setup)", &c2, "set_liquidation_fee", |e| (50_000u128,).into_val(e), false)]);
        log.push(r);
        if ok { push("RX4 fee on: L1a, L3, L4, L5 liquidator, L5 reserve", &x4, &[ck(true), warm(), liq("L3 liquidate (fee on)"), claim(), sync(0), sync(1)]); }
    }
    // ---- isolation (patches G6/G7) ----
    {
        let (c1, m1) = (gct.clone(), gum.clone());
        push("RX6 CF on the USDC market -> revert (condition c, G6)", &rbase, &[call("set_market_cf(usdc market)", &c1, "set_market_cf", move |e| (a(e, &m1), 500_000u128).into_val(e), true).fails()]);
        let c2 = gct.clone();
        push("RX7 third market listing -> revert (G7)", &rbase, &[call("add_market(vault as third market)", &c2, "add_market", |e| (a(e, V),).into_val(e), true).fails()]);
    }
    // ---- D3 on the real receipt: real pool push (no chain seeding in the default build) ----
    let mut calls = push_pool(); calls.push(ck(true));
    let (s, ok) = push("RF0 pool pushed out of band by a real swap: L1a records a D3 observation (returns 0)", &rl, &calls);
    if ok { log.push(json!({"real_views_after_RF0": real_views(&s, &g)})); }
    let mut calls = push_pool(); calls.extend([ck(false), fb().fails()]);
    push("RF1 pool pushed, chain just started: fallback reverts #2112", &rl, &calls);
    let mut calls = push_pool(); calls.push(fb().fails());
    push("RF1b pool pushed, no chain: fallback reverts #2111", &rl, &calls);
    push("RF6 pool healthy: fallback reverts #2104 NavRefreshable", &rl, &[fb().fails()]);
    let mut calls = push_pool(); calls.extend([ck(false), liq("L3 liquidate while blocked, NAV 61+ s old, no fallback").after(61).fails()]);
    push("RF7 pool pushed, NAV blocked, no fallback: liquidation reverts (indeterminate)", &rl, &calls);
}
