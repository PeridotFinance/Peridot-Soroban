// ---------------------------------------------------------------------------------------------
// CL collateral GROUP stages (18 Nov 2026). Included into lib.rs after common.rs.
// Topology: FRESH local instances of the PATCHED group controller (ctrl-group.wasm, source
// prototypes/cl-collateral-group/controller, every patch behind `--features group`), a fresh
// USDC lending market (deployed market bytes 4229b7ef), the release JRM 4f57459d and the group
// sleeve double (group-sleeve.wasm). Real live: CL vault, pool, Reflector CALI, SACs.
// Regression rows re-run six item-S rows on the DEPLOYED controller bytes 847894a6 + spike sleeve.
// ---------------------------------------------------------------------------------------------

#[soroban_sdk::contracttype]
#[derive(Clone)]
pub struct Cfg { pub ctrl: Address, pub vault: Address, pub pool: Address, pub oracle: Address, pub pyusd: Address, pub usdc: Address }
#[soroban_sdk::contracttype]
#[derive(Clone)]
pub struct Prm { pub max_age: u64, pub oracle_max_age: u64, pub depeg_bps: u32, pub band_bps: u32, pub fb_haircut_bps: u32, pub claim_fresh: u64 }
fn p0() -> Prm { Prm { max_age: 60, oracle_max_age: 600, depeg_bps: 100, band_bps: 50, fb_haircut_bps: 200, claim_fresh: 60 } }

struct G { ctrl: String, usdc_m: String, extra_m: String, gs: String, liquidator: String, reserve: String }

fn deploy_group(snap: &LedgerSnapshot, log: &mut StdVec<Value>, static_rates: bool) -> (LedgerSnapshot, G) {
    let t0: u8 = if static_rates { 0xd0 } else { 0xc0 };
    let s = step_env(snap); let env = &s.env;
    let o = a(env, O);
    let usdc = soroban_sdk::token::StellarAssetClient::new(env, &a(env, USDC));
    let pyusd = soroban_sdk::token::StellarAssetClient::new(env, &a(env, PYUSD));
    usdc.mint(&o, &1_000_000_000i128); pyusd.mint(&o, &1_000_000_000i128);
    let supplier = Address::generate(env); usdc.mint(&supplier, &50_000_000_000i128);
    // Distinct fixed contract ids (env.register's counter would collide with the spike stack).
    let caddr = |tag: u8| -> Address {
        let sc = ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(soroban_sdk::xdr::ContractId(soroban_sdk::xdr::Hash([tag; 32]))));
        Address::try_from_val(env, &sc).unwrap()
    };
    let ctrl = env.register_at(&caddr(t0 + 1), wasm("ctrl-group.wasm").as_slice(), ());
    let usdc_m = env.register_at(&caddr(t0 + 2), wasm("market.wasm").as_slice(), ());
    let extra_m = env.register_at(&caddr(t0 + 3), wasm("market.wasm").as_slice(), ());
    let jrm = env.register_at(&caddr(t0 + 4), wasm("jrm.wasm").as_slice(), ());
    let gs = env.register_at(&caddr(t0 + 5), wasm("group-sleeve.wasm").as_slice(), ());
    let inv = |c: &Address, f: &str, args: Vec<Val>| { let r = env.try_invoke_contract::<Val, soroban_sdk::Error>(c, &Symbol::new(env, f), args); assert!(matches!(r, Ok(Ok(_))), "group setup {f} failed: {r:?}"); };
    inv(&ctrl, "initialize", (o.clone(),).into_val(env));
    // Same Reflector feed object the vault uses. USDC is CALI's base (price 1e14). See DESIGN.md §1
    // for the CAFJ external-feed alternative and its measured footprint delta.
    inv(&ctrl, "set_oracle", (a(env, ORACLE),).into_val(env));
    if !static_rates { inv(&jrm, "initialize", (20_000u128, 180_000u128, 4_000_000u128, 800_000u128, o.clone()).into_val(env)); }
    inv(&usdc_m, "initialize", (a(env, USDC), 0u128, 0u128, o.clone()).into_val(env));
    inv(&usdc_m, "set_min_cash_reserve_bps", (o.clone(), 1_000u32).into_val(env));
    inv(&usdc_m, "set_cash_borrowing_enabled", (o.clone(), true).into_val(env));
    if static_rates {
        // cash-borrowing release choice: static 8% APR, no JRM (research/cash-borrowing-upgrade-20261118 §C)
        inv(&usdc_m, "set_borrow_rate", (80_000u128,).into_val(env));
        inv(&usdc_m, "enable_static_rates", (o.clone(),).into_val(env));
    } else {
        inv(&usdc_m, "set_interest_model", (jrm.clone(),).into_val(env));
    }
    inv(&usdc_m, "seed_minimum_supply", (o.clone(), 10_000u128).into_val(env));
    inv(&ctrl, "add_market", (usdc_m.clone(),).into_val(env));
    inv(&usdc_m, "set_peridottroller", (ctrl.clone(),).into_val(env));
    inv(&extra_m, "initialize", (a(env, PYUSD), 0u128, 0u128, o.clone()).into_val(env));
    inv(&extra_m, "seed_minimum_supply", (o.clone(), 10_000u128).into_val(env));
    let c = Cfg { ctrl: ctrl.clone(), vault: a(env, V), pool: a(env, POOL), oracle: a(env, ORACLE), pyusd: a(env, PYUSD), usdc: a(env, USDC) };
    inv(&gs, "init", (c, p0()).into_val(env));
    inv(&ctrl, "add_market", (gs.clone(),).into_val(env));
    inv(&ctrl, "set_market_cf", (gs.clone(), 500_000u128).into_val(env));
    inv(&ctrl, "group_set_sleeve", (gs.clone(),).into_val(env));
    inv(&ctrl, "group_set_liq_cache_only", (true,).into_val(env));
    inv(&usdc_m, "deposit", (supplier.clone(), 50_000_000_000u128).into_val(env));
    let g = G { ctrl: astr(&ctrl), usdc_m: astr(&usdc_m), extra_m: astr(&extra_m), gs: astr(&gs), liquidator: TREASURY.into(), reserve: KEEPER.into() };
    log.push(json!({"deploy_group": {"static_rates": static_rates, "ctrl": g.ctrl, "usdc_market": g.usdc_m, "extra_market_unlisted": g.extra_m, "group_sleeve": g.gs, "jrm": astr(&jrm), "liquidator": g.liquidator}}));
    let mut next = env.to_ledger_snapshot();
    strip_nonces(&mut next);
    for (k, _) in next.ledger_entries.iter() { let x = k.to_xdr_base64(Limits::none()).unwrap(); CREATED.with(|c| c.borrow_mut().insert(x)); }
    next.sequence_number += 1; next.timestamp += 6;
    (next, g)
}

fn gviews(snap: &LedgerSnapshot, g: &G) -> Value {
    let (gs, um, ct, lq) = (g.gs.clone(), g.usdc_m.clone(), g.ctrl.clone(), g.liquidator.clone());
    read(snap, move |env| json!({
        "nav_ck": tv(env, &gs, "nav_checkpoint", Vec::new(env)),
        "fb_ck": tv(env, &gs, "fallback_checkpoint", Vec::new(env)),
        "borrower_units": tv(env, &gs, "get_ptoken_balance", (a(env, O),).into_val(env)),
        "liquidator_units": tv(env, &gs, "get_ptoken_balance", (a(env, &lq),).into_val(env)),
        "borrower_vault_side_beneficial": tv(env, &gs, "emu_beneficial", (a(env, O),).into_val(env)),
        "liquidator_vault_side_beneficial": tv(env, &gs, "emu_beneficial", (a(env, &lq),).into_val(env)),
        "pending_seizures": tv(env, &gs, "pending_seizures", Vec::new(env)),
        "borrower_debt": tv(env, &um, "get_user_borrow_balance", (a(env, O),).into_val(env)),
        "account_liquidity": tv(env, &ct, "account_liquidity", (a(env, O),).into_val(env)),
        "real_vault_liquidity": tv(env, V, "get_position_liquidity", Vec::new(env)),
        "real_vault_total_supply": tv(env, V, "total_supply", Vec::new(env)),
    }))
}

#[test]
fn group() {
    let r = std::panic::catch_unwind(group_inner);
    let n = write_misses();
    std::println!("UNKNOWN_KEYS_FINAL {n}");
    if let Err(e) = r { std::panic::resume_unwind(e); }
}
fn group_inner() {
    let (snap0, st) = base_snapshot();
    CREATED.with(|c| c.borrow_mut().clear()); UNKNOWN.with(|u| u.borrow_mut().clear());
    let mut log: StdVec<Value> = StdVec::new();
    let out_path = format!("{}/run-result.json", sdir());
    let finish = |log: &StdVec<Value>, extra: Value| {
        let n = write_misses();
        std::fs::write(&out_path, serde_json::to_string_pretty(&json!({"state_ledger": st["ledger"], "state_close_time": st["closeTime"], "unknown_keys": n, "extra": extra, "log": log})).unwrap()).unwrap();
        std::println!("UNKNOWN_KEYS {n}");
    };
    let (snap1, ad) = deploy(&snap0, &mut log);
    let snap2 = match invest(&snap1, &mut log) { Some(s) => s, None => { finish(&log, json!({})); panic!("invest flow failed; see log"); } };
    let avail = u(&read(&snap2, |env| tv(env, V, "stage_available", (a(env, O),).into_val(env))));
    log.push(json!({"owner_available_units_after_invest": avail.to_string()}));
    let (sl, um, ct, liq) = (ad.sleeve.clone(), ad.usdc_m.clone(), ad.ctrl.clone(), ad.liquidator.clone());
    let (bind, base, ok) = {
        let (sl3, um2, ct2, ct3) = (sl.clone(), um.clone(), ct.clone(), ct.clone());
        seq("bind (setup, spike stack)", &snap2, &[
            call("set_units", &sl, "spike_set_units", move |e| (a(e, O), avail).into_val(e), false),
            call("enter_sleeve", &ct2, "enter_market", move |e| (a(e, O), a(e, &sl3)).into_val(e), false),
            call("enter_usdc", &ct3, "enter_market", move |e| (a(e, O), a(e, &um2)).into_val(e), false),
        ])
    };
    log.push(bind); assert!(ok);
    let mut results: StdVec<Value> = StdVec::new();
    let borrow_amt: u128 = 3_000_000_000;

    // ================= R: regression of six item-S rows on the deployed controller bytes =================
    {
        let mode = |m: u32, src: u32, proxy: bool| { let sl = sl.clone(); call(&format!("set_mode_{m}_{src}_{proxy}"), &sl, "spike_set_mode", move |e| (m, src, proxy).into_val(e), false) };
        let borrow = |label: &str, amt: u128| { let um = um.clone(); call(label, &um, "borrow", move |e| (a(e, O), amt).into_val(e), true) };
        let ckpt = |label: &str| { let sl = sl.clone(); call(label, &sl, "checkpoint_nav", |e| (a(e, O),).into_val(e), true) };
        let liquidate = |label: &str| { let (ct, um, sl, lq) = (ct.clone(), um.clone(), sl.clone(), liq.clone());
            call(label, &ct, "liquidate", move |e| (a(e, O), a(e, &um), a(e, &sl), 1_000_000_000u128, a(e, &lq)).into_val(e), true) };
        let touch = |t: bool| { let sl = sl.clone(); call(&format!("set_touch_{t}"), &sl, "spike_set_touch", move |e| (t,).into_val(e), false) };
        let cold = |s: &LedgerSnapshot| drop_price_cache(s, &ct);
        let bc = cold(&base);
        results.push(seq("REG S4a ckpt, then borrow, cold", &bc, &[mode(2, 1, false), ckpt("checkpoint_nav"), borrow("borrow", borrow_amt)]).0);
        results.push(seq("REG S4b ckpt, then borrow, warm", &base, &[mode(2, 1, false), ckpt("checkpoint_nav"), borrow("borrow", borrow_amt)]).0);
        let (sl2, lq2, um2) = (sl.clone(), liq.clone(), um.clone()); let n = avail * 2 / 5;
        let (r, lb, okl) = seq("REG S3 setup", &base, &[borrow("borrow (setup)", borrow_amt),
            call("shrink units", &sl2, "spike_set_units", move |e| (a(e, O), n).into_val(e), false),
            call("liquidator approves", USDC, "approve", move |e| (a(e, &lq2), a(e, &um2), 20_000_000_000i128, e.ledger().sequence() + 100_000).into_val(e), false)]);
        log.push(r); assert!(okl);
        let lc = cold(&lb);
        results.push(seq("REG S3g ckpt, liquidate units-only, cold", &lc, &[mode(2, 1, false), ckpt("checkpoint_nav"), liquidate("liquidate")]).0);
        results.push(seq("REG S3h ckpt, liquidate units-only, warm", &lb, &[mode(2, 1, false), ckpt("checkpoint_nav"), liquidate("liquidate")]).0);
        results.push(seq("REG S3g-t ckpt, liquidate + custody read, cold", &lc, &[mode(2, 1, false), touch(true), ckpt("checkpoint_nav"), liquidate("liquidate")]).0);
        results.push(seq("REG S3h-t ckpt, liquidate + custody read, warm", &lb, &[mode(2, 1, false), touch(true), ckpt("checkpoint_nav"), liquidate("liquidate")]).0);
    }

    // Group stack with the release JRM (as the spike), then with static rates (cash-borrowing choice).
    group_suite(&base, avail, false, &mut results, &mut log);
    group_suite(&base, avail, true, &mut results, &mut log);
    finish(&log, json!({"results": results}));
}

fn group_suite(base: &LedgerSnapshot, avail: u128, static_rates: bool, results: &mut StdVec<Value>, log: &mut StdVec<Value>) {
    let borrow_amt: u128 = 3_000_000_000;
    // ================= group deploy + bind =================
    let (gb0, g) = deploy_group(base, log, static_rates);
    let pre = if static_rates { "STATIC " } else { "" };
    let (gsx, gct, gum, gliqa) = (g.gs.clone(), g.ctrl.clone(), g.usdc_m.clone(), g.liquidator.clone());
    let (r, gbase, ok) = {
        let (s1, s2, c1, c2, m1) = (gsx.clone(), gsx.clone(), gct.clone(), gct.clone(), gum.clone());
        seq("group bind (setup)", &gb0, &[
            call("set_units", &s1, "spike_set_units", move |e| (a(e, O), avail).into_val(e), false),
            call("enter_sleeve", &c1, "enter_market", move |e| (a(e, O), a(e, &s2)).into_val(e), false),
            call("enter_usdc", &c2, "enter_market", move |e| (a(e, O), a(e, &m1)).into_val(e), false),
        ])
    };
    log.push(r); assert!(ok, "group bind failed");
    log.push(json!({"group_views_base": gviews(&gbase, &g)}));

    // ---- group call builders ----
    let gckpt = |m: bool| { let s = gsx.clone(); call("L1a checkpoint_nav (vault-wide, band+depeg guards)", &s, "checkpoint_nav", |e| Vec::new(e), m) };
    let gfb = || { let s = gsx.clone(); call("L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)", &s, "checkpoint_fallback", |e| Vec::new(e), true) };
    let gwarm = || { let c = gct.clone(); call("L2 cache_price(USDC)", &c, "cache_price", |e| (a(e, USDC),).into_val(e), true) };
    let gborrow = |label: &str, amt: u128, m: bool| { let x = gum.clone(); call(label, &x, "borrow", move |e| (a(e, O), amt).into_val(e), m) };
    let gliq = |label: &str| { let (c, m, s, l) = (gct.clone(), gum.clone(), gsx.clone(), gliqa.clone());
        call(label, &c, "liquidate", move |e| (a(e, O), a(e, &m), a(e, &s), 1_000_000_000u128, a(e, &l)).into_val(e), true) };
    let gclaim = || call("L4 vault.checkpoint_rewards (real AQUA claim, separate tx)", V, "checkpoint_rewards", |e| Vec::new(e), true);
    let emu = |d: u128| { let s = gsx.clone(); call(&format!("emu_claim({d}) [model of the claim's index update; unmeasured]"), &s, "emu_claim", move |e| (d,).into_val(e), false) };
    let gsync = |id: u64| { let s = gsx.clone(); call(&format!("L5 sync_seized({id}) (vault-side settle-only reassign)"), &s, "sync_seized", move |e| (id,).into_val(e), true) };
    let gparams = |p: Prm| { let s = gsx.clone(); call("set params (unmeasured)", &s, "spike_set_params", move |e| (p.clone(),).into_val(e), false) };
    let inline = |b: bool| { let s = gsx.clone(); call(&format!("set inline sync {b} (unmeasured)"), &s, "spike_set_inline", move |e| (b,).into_val(e), false) };
    let cacheonly = |b: bool| { let c = gct.clone(); call(&format!("group_set_liq_cache_only({b}) (unmeasured)"), &c, "group_set_liq_cache_only", move |e| (b,).into_val(e), false) };
    let redeem = |label: &str, n: u128| { let s = gsx.clone(); call(label, &s, "redeem_collateral", move |e| (a(e, O), n).into_val(e), true) };
    let gcold = |s: &LedgerSnapshot| drop_price_cache(s, &gct);
    let mut push = |name: &str, b: &LedgerSnapshot, calls: &[Call]| { let r = seq(&format!("{pre}{name}"), b, calls); results.push(r.0); (r.1, r.2) };

    // ================= calibration (code-size: real receipt vs double) =================
    push("C1 real staged receipt stage_state (112,328-byte code load)", &gbase, &[call("receipt.stage_state", R, "stage_state", |e| Vec::new(e), true)]);
    push("C2 group sleeve get_total_ptokens (24,697-byte double load)", &gbase, &[call("sleeve.get_total_ptokens", &gsx, "get_total_ptokens", |e| Vec::new(e), true)]);
    push("C3 real vault total_supply (123,430-byte code load)", &gbase, &[call("vault.total_supply", V, "total_supply", |e| Vec::new(e), true)]);

    // ================= B: borrow =================
    let gbc = gcold(&gbase);
    push("GB0 borrow with no checkpoint ever written -> revert", &gbase, &[gborrow("borrow (no checkpoint)", borrow_amt, true).fails()]);
    push("GB1 ckpt, borrow, WARM", &gbase, &[gckpt(true), gborrow("B borrow (reads NAV checkpoint)", borrow_amt, true)]);
    push("GB2 ckpt, borrow, COLD (borrow reads oracle live)", &gbc, &[gckpt(true), gborrow("B borrow (reads NAV checkpoint)", borrow_amt, true)]);
    push("GB3 ckpt, cache_price(USDC), borrow, from COLD", &gbc, &[gckpt(true), gwarm(), gborrow("B borrow (reads NAV checkpoint)", borrow_amt, true)]);
    push("GB4 ckpt, borrow at checkpoint age 60 s -> ok", &gbase, &[gckpt(false), gborrow("B borrow at age 60", borrow_amt, true).after(54)]);
    push("GB5 ckpt, borrow at checkpoint age 61 s -> revert", &gbase, &[gckpt(false), gborrow("B borrow at age 61", borrow_amt, true).after(55).fails()]);
    let (gbr, okb) = { let (s, ok) = push("GB6 first borrow (base for repeat/redeem)", &gbase, &[gckpt(false), gborrow("borrow 1st (setup)", borrow_amt, false)]); (s, ok) };
    if okb {
        push("GB7 repeat borrow, WARM", &gbr, &[gckpt(false), gborrow("B repeat borrow", 500_000_000, true)]);
        push("GB8 repeat borrow, COLD", &gcold(&gbr), &[gckpt(false), gborrow("B repeat borrow", 500_000_000, true)]);
        // ================= D: redeem (remove units from collateral; stands for prepare-close) =================
        let n_ok = avail / 10; let n_bad = avail * 3 / 10;
        push("GR1 ckpt, redeem 10% of units with debt, WARM", &gbr, &[gckpt(false), redeem("D redeem_collateral 10%", n_ok)]);
        push("GR2 ckpt, redeem 10% with debt, COLD", &gcold(&gbr), &[gckpt(false), redeem("D redeem_collateral 10%", n_ok)]);
        push("GR3 redeem at checkpoint age 60 s -> ok", &gbr, &[gckpt(false), redeem("D redeem at age 60", n_ok).after(54)]);
        push("GR4 redeem at checkpoint age 61 s -> revert", &gbr, &[gckpt(false), redeem("D redeem at age 61", n_ok).after(55).fails()]);
        push("GR5 redeem 30% -> insufficient collateral revert", &gbr, &[gckpt(false), redeem("D redeem 30%", n_bad).fails()]);
    }

    // ================= L: liquidation =================
    let shrink = |n: u128| { let s = gsx.clone(); call("shrink units to 40% (setup)", &s, "spike_set_units", move |e| (a(e, O), n).into_val(e), false) };
    let approve = || { let (l, m) = (gliqa.clone(), gum.clone()); call("liquidator approves USDC market (setup)", USDC, "approve", move |e| (a(e, &l), a(e, &m), 20_000_000_000i128, e.ledger().sequence() + 100_000).into_val(e), false) };
    let (r, gl, okl) = seq("GL setup: ckpt, borrow 300 USDC, shrink units to 40%, approve", &gbase, &[gckpt(false), gborrow("borrow (setup)", borrow_amt, false), shrink(avail * 2 / 5), approve()]);
    log.push(r); assert!(okl, "group liquidation setup failed");
    log.push(json!({"group_views_liq_base": gviews(&gl, &g)}));
    let glc = gcold(&gl);
    push("GL1 L1a ckpt, L3 liquidate (cache-only), WARM", &gl, &[gckpt(true), gliq("L3 liquidate (repay + units mirror + queue)")]);
    push("GL2 L1a ckpt, L3 liquidate (cache-only), COLD -> revert by design", &glc, &[gckpt(true), gliq("L3 liquidate cold, cache-only").fails()]);
    push("GL3 L1a, L2 cache_price, L3 liquidate, from COLD", &glc, &[gckpt(true), gwarm(), gliq("L3 liquidate (repay + units mirror + queue)")]);
    push("GL4 cache-only OFF: ckpt, liquidate, COLD (live oracle read)", &glc, &[cacheonly(false), gckpt(true), gliq("L3 liquidate cold, live price")]);
    push("GL5 cache-only OFF: ckpt, liquidate, WARM", &gl, &[cacheonly(false), gckpt(true), gliq("L3 liquidate warm, live-capable")]);
    let (full, okf) = push("GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh)", &glc,
        &[gckpt(true), gwarm(), gliq("L3 liquidate (repay + units mirror + queue)"), gclaim(), emu(0), gsync(0)]);
    if okf { log.push(json!({"group_views_after_GL6": gviews(&full, &g)})); }
    push("GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh)", &gl,
        &[gckpt(true), gwarm(), gliq("L3 liquidate (repay + units mirror + queue)"), gclaim(), emu(0), gsync(0)]);
    push("GL7 L5 sync with NO fresh claim (opens reward boundary)", &gl, &[gckpt(true), gliq("L3 liquidate"), gsync(0)]);
    push("GL8 inline vault sync inside seize (single tx), WARM", &gl, &[inline(true), gckpt(true), gliq("L3+L5 inline liquidate")]);
    push("GL9 inline vault sync inside seize (single tx), COLD live price", &glc, &[inline(true), cacheonly(false), gckpt(true), gliq("L3+L5 inline liquidate cold")]);
    push("GL10 liquidate at NAV checkpoint age 60 s -> ok", &gl, &[gckpt(false), gliq("L3 liquidate at age 60").after(54)]);
    push("GL11 liquidate at NAV checkpoint age 61 s, no fallback -> revert", &gl, &[gckpt(false), gliq("L3 liquidate at age 61").after(55).fails()]);
    if okf {
        let gl2 = full.clone();
        push("GL12 repeat seizure (second liquidation; liquidator already holds units)", &gl2, &[gckpt(true), gliq("L3 liquidate (repeat)"), emu(0), gsync(1)]);
    }

    // ================= F: fallback valuation (liquidation only) =================
    let mut pb = p0(); pb.band_bps = 0; // emulates "pool pushed out of band"
    let mut pd = p0(); pd.depeg_bps = 0; // emulates "PYUSD depeg beyond guard"
    push("GF0 fallback refused while NAV is refreshable", &gl, &[gfb().fails()]);
    push("GF1 pool out of band: checkpoint_nav reverts", &gl, &[gparams(pb.clone()), gckpt(true).fails()]);
    push("GF1b PYUSD depeg: checkpoint_nav reverts", &gl, &[gparams(pd.clone()), gckpt(true).fails()]);
    push("GF2 no NAV checkpoint, only a fallback checkpoint: borrow reverts (cf. GB1 ok)", &gbase, &[gparams(pb.clone()), gfb(), gborrow("B borrow with only a fallback checkpoint", borrow_amt, true).fails()]);
    let (fbs, okfb) = push("GF3 pool out of band: L1b fallback, L3 liquidate, WARM", &gl, &[gparams(pb.clone()), gfb().after(61), gliq("L3 liquidate on fallback")]);
    if okfb { log.push(json!({"group_views_after_GF3": gviews(&fbs, &g)})); }
    push("GF4 pool out of band: L1b, L2, L3 from COLD", &glc, &[gparams(pb.clone()), gfb().after(61), gwarm(), gliq("L3 liquidate on fallback")]);
    push("GF5 depeg cause: L1b fallback, L3 liquidate, WARM", &gl, &[gparams(pd.clone()), gfb().after(61), gliq("L3 liquidate on fallback (depeg)")]);
    push("GF6 fallback age 61 s -> liquidate reverts", &gl, &[gparams(pb.clone()), gfb().after(61), gliq("L3 liquidate fallback age 61").after(55).fails()]);
    push("GF7 full fallback sequence from COLD + claim + sync", &glc, &[gparams(pb.clone()), gfb().after(61), gwarm(), gliq("L3 liquidate on fallback"), gclaim(), emu(0), gsync(0)]);
    push("GF7w full fallback sequence from WARM + claim + sync", &gl, &[gparams(pb.clone()), gfb().after(61), gwarm(), gliq("L3 liquidate on fallback"), gclaim(), emu(0), gsync(0)]);
    // value comparison: NAV checkpoint and fallback written in the same state
    {
        let (_, s1, ok1) = seq("value-compare (setup)", &gl, &[gckpt(false)]);
        let (_, s2, ok2) = seq("value-compare fb (setup)", &s1, &[gparams(pb.clone()), gfb()]);
        // both written within 12 s on the same state: nav_ck.rate vs fb_ck.rate is the fallback discount
        if ok1 && ok2 { log.push(json!({"value_compare": gviews(&s2, &g)})); }
    }

    // ================= X: footprint variation rows =================
    {
        // X2 borrower also holds USDC-market pTokens (debt market supply, CF 0)
        let m = gum.clone();
        let (x2, ok) = { let r = seq("X2 setup: borrower deposits 50 USDC into the lending market", &gl, &[call("deposit usdc (setup)", &m, "deposit", |e| (a(e, O), 500_000_000u128).into_val(e), false)]); log.push(r.0); (r.1, r.2) };
        if ok { push("X2 borrower holds lending-market pTokens: ckpt, liquidate, WARM", &x2, &[gckpt(true), gliq("L3 liquidate")]); }
        // X3 borrower with locked units (redeemed 10% into a close command before the shortfall)
        let (r, x3, ok) = seq("X3 setup: ckpt, borrow, redeem 10% (locks), shrink, approve", &gbase, &[gckpt(false), gborrow("borrow (setup)", borrow_amt, false), gckpt(false), redeem("redeem 10% (setup)", avail / 10).fails_if(false), shrink(avail * 2 / 5), approve()]);
        log.push(r);
        if ok { push("X3 borrower with locked units: ckpt, liquidate, claim-model, sync", &x3, &[gckpt(true), gliq("L3 liquidate"), emu(0), gsync(0)]); }
        // X4 liquidation fee on: fee units go to a reserve recipient (second queued sync)
        let (c1, c2, rs) = (gct.clone(), gct.clone(), g.reserve.clone());
        let (r, x4, ok) = seq("X4 setup: reserve recipient + 5% liquidation fee", &gl, &[
            call("set_reserve_recipient (setup)", &c1, "set_reserve_recipient", move |e| (a(e, &rs),).into_val(e), false),
            call("set_liquidation_fee 5% (setup)", &c2, "set_liquidation_fee", |e| (50_000u128,).into_val(e), false)]);
        log.push(r);
        if ok { push("X4 fee on: ckpt, liquidate, claim-model, sync liquidator, sync reserve", &x4, &[gckpt(true), gliq("L3 liquidate (fee on)"), emu(0), gsync(0), gsync(1)]); }
        // X5 entered-market cap of 2
        let (c3, c4, em, em2) = (gct.clone(), gct.clone(), g.extra_m.clone(), g.extra_m.clone());
        push("X5 third market listed; entering it -> revert (cap 2)", &gbase, &[
            call("add_market extra (setup)", &c3, "add_market", move |e| (a(e, &em),).into_val(e), false),
            call("enter_market third", &c4, "enter_market", move |e| (a(e, O), a(e, &em2)).into_val(e), true).fails()]);
        let (c5, c6, em3, em4, lq) = (gct.clone(), gct.clone(), g.extra_m.clone(), g.extra_m.clone(), gliqa.clone());
        push("X5b twin: an account with 0 entered markets enters the listed extra market -> ok", &gbase, &[
            call("add_market extra (setup)", &c5, "add_market", move |e| (a(e, &em3),).into_val(e), false),
            call("enter_market first", &c6, "enter_market", move |e| (a(e, &lq), a(e, &em4)).into_val(e), true)]);
        // X6 interest accrued over 240 s before liquidation
        push("X6 interest over 240 s: ckpt, liquidate, WARM", &gl, &[gckpt(true).after(240), gliq("L3 liquidate after 240 s accrual")]);
    }
}

impl Call { fn fails_if(self, f: bool) -> Self { if f { self.fails() } else { self } } }

// ---------------------------------------------------------------------------------------------
// Q3 model test: who accrues AQUA on units held as collateral; seizure settles without a claim.
// The sleeve double's emulated vault-side ledger uses the same arithmetic as
// contracts/cl-share-vault/src/rewards.rs settle/reset_debt and custody.rs effective/Beneficial.
// ---------------------------------------------------------------------------------------------
#[soroban_sdk::contract]
pub struct StubVault;
#[soroban_sdk::contractimpl]
impl StubVault { pub fn stage_receipt(e: Env) -> Address { e.current_contract_address() } }

#[test]
fn reward_semantics_model() {
    use soroban_sdk::testutils::Ledger;
    let wdir = std::env::var("SPIKE_WASM_DIR").expect("SPIKE_WASM_DIR");
    let mut env = Env::default(); env.set_config(EnvTestConfig { capture_snapshot_at_drop: false }); env.mock_all_auths();
    let vault = env.register(StubVault, ());
    let gs = env.register(std::fs::read(format!("{wdir}/group-sleeve.wasm")).unwrap().as_slice(), ());
    let (ctrl, alice, bob, liq) = (Address::generate(&env), Address::generate(&env), Address::generate(&env), Address::generate(&env));
    let c = Cfg { ctrl: ctrl.clone(), vault: vault.clone(), pool: vault.clone(), oracle: vault.clone(), pyusd: vault.clone(), usdc: vault.clone() };
    let inv = |f: &str, args: Vec<Val>| -> Val { env.invoke_contract::<Val>(&gs, &Symbol::new(&env, f), args) };
    let pending = |x: &Address| -> u128 { u128::try_from_val(&env, &inv("emu_pending", (x.clone(),).into_val(&env))).unwrap() };
    inv("init", (c, p0()).into_val(&env));
    // Alice (borrower) and Bob each own 1000 units; Alice's units are her collateral. Entering a
    // market and borrowing are controller-side flags: no unit leaves custody, the beneficiary of
    // all 1000 units stays Alice, so Alice accrues on them while they are collateral.
    inv("spike_set_units", (alice.clone(), 1000u128).into_val(&env));
    inv("spike_set_units", (bob.clone(), 1000u128).into_val(&env));
    env.ledger().set_timestamp(1_000);
    inv("emu_claim", (100u128,).into_val(&env)); // claim #1 (index advances ONLY on a claim)
    assert_eq!((pending(&alice), pending(&bob)), (50, 50));
    // L3: seizure of 400 units moves the receipt-side mirror only; vault-side beneficiary unchanged.
    inv("seize", (alice.clone(), liq.clone(), 400u128, Option::<Val>::None).into_val(&env));
    // L4: claim between L3 and L5: still credited at the old vault-side balances (Alice 1000).
    inv("emu_claim", (20u128,).into_val(&env));
    assert_eq!((pending(&alice), pending(&bob), pending(&liq)), (60, 60, 0));
    // L5 with a fresh claim (same timestamp): settle both at the stored index, no claim, move.
    inv("sync_seized", (0u64,).into_val(&env));
    assert_eq!((pending(&alice), pending(&liq)), (60, 0), "borrower keeps everything indexed before the move");
    inv("emu_claim", (30u128,).into_val(&env)); // after: Alice 600, Bob 1000, liquidator 400 of 2000
    assert_eq!((pending(&alice), pending(&bob), pending(&liq)), (69, 75, 6));
    assert_eq!(69 + 75 + 6, 150u128, "conserved");
    // Stale-claim case: L5 runs > claim_fresh (60 s) after the last claim -> boundary opens and the
    // next claim (which contains accrual from before the move) is quarantined, not given to the liquidator.
    inv("seize", (alice.clone(), liq.clone(), 100u128, Option::<Val>::None).into_val(&env));
    env.ledger().set_timestamp(1_061);
    inv("sync_seized", (1u64,).into_val(&env));
    inv("emu_claim", (40u128,).into_val(&env));
    assert_eq!(u128::try_from_val(&env, &inv("emu_quarantine", Vec::new(&env))).unwrap(), 40);
    assert_eq!((pending(&alice), pending(&bob), pending(&liq)), (69, 75, 6), "quarantined interval paid to no one automatically");
    std::println!("REWARD_MODEL: claim1 100 -> A50 B50; seize 400 queued; claim2 20 at old balances -> A60 B60 L0; sync (fresh) -> A60 L0; claim3 30 -> A69 B75 L6 (sum 150); stale sync -> claim4 40 quarantined");
}

// Q4 model test: checkpoint age boundaries on the views alone (the harness rows show the full-stack reverts).
#[test]
fn no_checkpoint_views_model() {
    let wdir = std::env::var("SPIKE_WASM_DIR").expect("SPIKE_WASM_DIR");
    let mut env = Env::default(); env.set_config(EnvTestConfig { capture_snapshot_at_drop: false }); env.mock_all_auths();
    let gs = env.register(std::fs::read(format!("{wdir}/group-sleeve.wasm")).unwrap().as_slice(), ());
    let x = Address::generate(&env);
    let c = Cfg { ctrl: x.clone(), vault: x.clone(), pool: x.clone(), oracle: x.clone(), pyusd: x.clone(), usdc: x.clone() };
    env.invoke_contract::<Val>(&gs, &Symbol::new(&env, "init"), (c, p0()).into_val(&env));
    // no checkpoint: borrow-view rate 0 with a balance (controller reverts the borrow), liquidation view 0
    env.invoke_contract::<Val>(&gs, &Symbol::new(&env, "spike_set_units"), (x.clone(), 10u128).into_val(&env));
    let snap: (u128, u128, u128, Address) = env.invoke_contract(&gs, &Symbol::new(&env, "get_account_snapshot"), (x.clone(),).into_val(&env));
    assert_eq!(snap.2, 0);
    let l: (u128, u128, Address) = env.invoke_contract(&gs, &Symbol::new(&env, "get_liquidation_snapshot"), (x.clone(),).into_val(&env));
    assert_eq!(l.1, 0);
}
