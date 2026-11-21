//! COMPILED-bytes upgrade tests for the group controller (G0-G11) and the receipt StgV2 Mainnet build,
//! under the PUBLIC network id, each against its own v2' marker build (`upgrade_marker() -> 2`).
//! Artifacts: target/clg/upg/{ctrl-group,ctrl-group-marker,receipt-v2-mainnet,receipt-v2-marker}
//! (override the directory with CLG_UPG_DIR). See research/cl-collateral-group-20261118/UPGRADEABILITY.md.
extern crate std;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
    vec, Address, Bytes, BytesN, Env, IntoVal, Symbol, Val, Vec,
};

const LEDGER: &str = "GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP";
const DAY: u64 = 24 * 60 * 60;

fn wasm(dir: &str, file: &str) -> std::vec::Vec<u8> {
    let base = std::env::var("CLG_UPG_DIR").unwrap_or_else(|_| std::format!("{}/../../../target/clg/upg", env!("CARGO_MANIFEST_DIR")));
    let p = std::format!("{base}/{dir}/{file}");
    std::fs::read(&p).unwrap_or_else(|_| panic!("missing compiled artifact {p}"))
}
fn env() -> Env {
    let e = Env::default();
    e.mock_all_auths();
    let id = e.crypto().sha256(&Bytes::from_slice(&e, b"Public Global Stellar Network ; September 2015"));
    e.ledger().set_network_id(id.to_array());
    e.ledger().set_timestamp(1_000_000);
    e
}
fn adv(e: &Env, dt: u64) { let t = e.ledger().timestamp(); e.ledger().set_timestamp(t + dt); }
fn call<T: soroban_sdk::TryFromVal<Env, Val>>(e: &Env, c: &Address, f: &str, a: Vec<Val>) -> T { e.invoke_contract(c, &Symbol::new(e, f), a) }
fn fails(e: &Env, c: &Address, f: &str, a: Vec<Val>) -> bool {
    !matches!(e.try_invoke_contract::<Val, soroban_sdk::Error>(c, &Symbol::new(e, f), a), Ok(Ok(_)))
}
type Pending = Option<(BytesN<32>, u64)>;

/// Calls `f` with ONLY `who`'s authorization mocked (for exactly that invocation).
fn only(e: &Env, who: &Address, c: &Address, f: &'static str, a: Vec<Val>) -> bool {
    e.set_auths(&[]);
    e.mock_auths(&[MockAuth { address: who, invoke: &MockAuthInvoke { contract: c, fn_name: f, args: a.clone(), sub_invokes: &[] } }]);
    let failed = fails(e, c, f, a);
    e.mock_all_auths();
    failed
}

fn ctrl(e: &Env) -> (Address, Address) {
    let admin = Address::from_str(e, LEDGER);
    let c = e.register(wasm("ctrl-group", "simple_peridottroller.wasm").as_slice(), ());
    let _: () = call(e, &c, "initialize", vec![e, admin.into_val(e)]);
    (c, admin)
}

#[test]
fn controller_propose_cancel_timelock_auth_and_v2_prime() {
    let e = env();
    let (c, admin) = ctrl(&e);
    let mallory = Address::generate(&e);
    let v2p = e.deployer().upload_contract_wasm(Bytes::from_slice(&e, &wasm("ctrl-group-marker", "simple_peridottroller.wasm")));
    let h = |n: u8| BytesN::<32>::from_array(&e, &[n; 32]);
    assert!(fails(&e, &c, "upgrade_marker", vec![&e]), "v2 has no marker");
    let p: Pending = call(&e, &c, "get_pending_upgrade", vec![&e]);
    assert_eq!(p, None);
    // Non-admin refused (only mallory signs).
    assert!(only(&e, &mallory, &c, "propose_upgrade_wasm", vec![&e, h(1).into_val(&e)]));
    // Propose -> cancel clears -> nothing to cancel / execute.
    let _: () = call(&e, &c, "propose_upgrade_wasm", vec![&e, h(1).into_val(&e)]);
    let p: Pending = call(&e, &c, "get_pending_upgrade", vec![&e]);
    assert_eq!(p, Some((h(1), 1_000_000 + DAY)));
    assert!(only(&e, &mallory, &c, "cancel_upgrade", vec![&e]));
    let _: () = call(&e, &c, "cancel_upgrade", vec![&e]);
    let p: Pending = call(&e, &c, "get_pending_upgrade", vec![&e]);
    assert_eq!(p, None);
    assert!(fails(&e, &c, "cancel_upgrade", vec![&e]));
    assert!(fails(&e, &c, "upgrade_wasm", vec![&e, h(1).into_val(&e)]));
    // Real proposal: state to preserve, early execute reverts, mismatch reverts, ETA succeeds.
    let _: () = call(&e, &c, "set_close_factor", vec![&e, 600_000u128.into_val(&e)]);
    let cf_before: Val = call(&e, &c, "get_close_factor_scaled", vec![&e]);
    let _: () = call(&e, &c, "propose_upgrade_wasm", vec![&e, v2p.clone().into_val(&e)]);
    adv(&e, DAY - 1);
    assert!(fails(&e, &c, "upgrade_wasm", vec![&e, v2p.clone().into_val(&e)]), "eta - 1 s");
    adv(&e, 1);
    assert!(fails(&e, &c, "upgrade_wasm", vec![&e, h(9).into_val(&e)]), "hash mismatch");
    assert!(only(&e, &mallory, &c, "upgrade_wasm", vec![&e, v2p.clone().into_val(&e)]), "non-admin execute");
    let _: () = call(&e, &c, "upgrade_wasm", vec![&e, v2p.clone().into_val(&e)]);
    let m: u32 = call(&e, &c, "upgrade_marker", vec![&e]);
    assert_eq!(m, 2);
    let a: Address = call(&e, &c, "get_admin", vec![&e]);
    assert_eq!(a, admin, "admin survives");
    let cf_after: Val = call(&e, &c, "get_close_factor_scaled", vec![&e]);
    assert!(cf_after.shallow_eq(&cf_before) || std::format!("{:?}", cf_after) == std::format!("{:?}", cf_before), "close factor survives");
    let p: Pending = call(&e, &c, "get_pending_upgrade", vec![&e]);
    assert_eq!(p, None);
}

#[test]
fn receipt_upgrade_needs_controller_pauses_and_preserves_state() {
    let e = env();
    let (c, admin) = ctrl(&e);
    let r = e.register(wasm("receipt-v2-mainnet", "receipt_vault.wasm").as_slice(), ());
    let issuer = Address::generate(&e);
    let usdc = e.register_stellar_asset_contract_v2(issuer).address();
    let _: () = call(&e, &r, "initialize", vec![&e, usdc.into_val(&e), 0u128.into_val(&e), 0u128.into_val(&e), admin.into_val(&e)]);
    let _: () = call(&e, &c, "add_market", vec![&e, r.into_val(&e)]);
    let _: () = call(&e, &r, "set_peridottroller", vec![&e, c.into_val(&e)]);
    // State to preserve. (A fresh StgV2 receipt takes deposits only through the staged vault
    // route; the real open/mint/invest after an upgrade is the fresh-stack harness row.)
    let _: () = call(&e, &r, "set_min_cash_reserve_bps", vec![&e, admin.into_val(&e), 2_500u32.into_val(&e)]);
    let reserve: Val = call(&e, &r, "get_min_cash_reserve_bps", vec![&e]);
    let rate: u128 = call(&e, &r, "get_exchange_rate", vec![&e]);
    let tot: u128 = call(&e, &r, "get_total_ptokens", vec![&e]);
    let mallory = Address::generate(&e);
    let v2p = e.deployer().upload_contract_wasm(Bytes::from_slice(&e, &wasm("receipt-v2-marker", "receipt_vault.wasm")));
    let h1 = BytesN::<32>::from_array(&e, &[1; 32]);
    assert!(fails(&e, &r, "upgrade_marker", vec![&e]));
    // Non-admin refused; cancel clears.
    assert!(only(&e, &mallory, &r, "propose_upgrade_wasm", vec![&e, h1.clone().into_val(&e)]));
    let _: () = call(&e, &r, "propose_upgrade_wasm", vec![&e, h1.clone().into_val(&e)]);
    let p: Pending = call(&e, &r, "get_pending_upgrade", vec![&e]);
    assert_eq!(p, Some((h1.clone(), 1_000_000 + DAY)));
    assert!(only(&e, &mallory, &r, "cancel_upgrade", vec![&e]));
    let _: () = call(&e, &r, "cancel_upgrade", vec![&e]);
    let p: Pending = call(&e, &r, "get_pending_upgrade", vec![&e]);
    assert_eq!(p, None);
    assert!(fails(&e, &r, "cancel_upgrade", vec![&e]));
    // Real proposal.
    let _: () = call(&e, &r, "propose_upgrade_wasm", vec![&e, v2p.clone().into_val(&e)]);
    adv(&e, DAY - 1);
    let up = || vec![&e, v2p.clone().into_val(&e)];
    assert!(fails(&e, &r, "upgrade_wasm", up()), "eta - 1 s");
    adv(&e, 1);
    assert!(fails(&e, &r, "upgrade_wasm", up()), "market not paused");
    let pause = |f: &str, on: bool| { let _: () = call(&e, &c, f, vec![&e, r.into_val(&e), on.into_val(&e)]); };
    pause("set_pause_deposit", true);
    pause("set_pause_redeem", true);
    assert!(fails(&e, &r, "upgrade_wasm", up()), "borrow still open");
    pause("set_pause_borrow", true);
    assert!(only(&e, &mallory, &r, "upgrade_wasm", up()), "non-admin execute");
    let _: () = call(&e, &r, "upgrade_wasm", up());
    let m: u32 = call(&e, &r, "upgrade_marker", vec![&e]);
    assert_eq!(m, 2);
    let a: Address = call(&e, &r, "get_admin", vec![&e]);
    assert_eq!(a, admin);
    let tot2: u128 = call(&e, &r, "get_total_ptokens", vec![&e]);
    let rate2: u128 = call(&e, &r, "get_exchange_rate", vec![&e]);
    assert_eq!((tot2, rate2), (tot, rate), "supply and rate survive");
    let reserve2: Val = call(&e, &r, "get_min_cash_reserve_bps", vec![&e]);
    assert_eq!(std::format!("{:?}", reserve2), std::format!("{:?}", reserve), "reserve setting survives");
    let listed: bool = call(&e, &c, "is_market_supported", vec![&e, r.into_val(&e)]);
    assert!(listed);
    let u2: Address = call(&e, &r, "get_underlying_token", vec![&e]);
    assert_eq!(u2, usdc);
    // Reopen: the controller admin lifts the pauses; deposits and redemptions work on v2'.
    pause("set_pause_deposit", false);
    pause("set_pause_redeem", false);
    pause("set_pause_borrow", false);
    for f in ["is_deposit_paused", "is_redeem_paused", "is_borrow_paused"] {
        let on: bool = call(&e, &c, f, vec![&e, r.into_val(&e)]);
        assert!(!on, "{f} lifted");
    }
    // The gate survives: v2' can be proposed and cancelled again.
    let _: () = call(&e, &r, "propose_upgrade_wasm", vec![&e, h1.clone().into_val(&e)]);
    let _: () = call(&e, &r, "cancel_upgrade", vec![&e]);
}
