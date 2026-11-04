//! ISOLATED SPLIT PROTOTYPE: actual compiled contracts, controlled state.
//! Each stage is a fresh top-level invocation and separately serialized unsigned
//! transaction. Five-second ledger increments below are TEST latency, not RPC timing.
use super::*;
use soroban_sdk::{TryFromVal, BytesN};

#[contracttype]
#[derive(Clone, Debug)]
struct SplitState {
    owner: Address,
    escrow: Address,
    margin_received: u128,
    phase: u32,
    deadline: u64,
}
#[contracttype]
#[derive(Clone, Debug)]
struct Job {
    owner: Address,
    underlying: u128,
    other: u128,
    shares: u128,
    deadline: u64,
    phase: u32,
    fence: Option<BytesN<32>>,
}

struct RestoreProfile(std::string::String);
impl Drop for RestoreProfile { fn drop(&mut self) { std::env::set_var("CL_RESOURCE_PROFILE", &self.0); } }

fn setup() -> (F, Address) {
    let selected = std::env::var("CL_RESOURCE_PROFILE").unwrap();
    let fixture_profile = std::env::var("CL_SPLIT_SETUP_PROFILE").expect("explicit fixture construction profile required");
    let restore_profile = RestoreProfile(selected.clone());
    std::env::set_var("CL_RESOURCE_PROFILE", &fixture_profile);
    std::println!("SPLIT_FIXTURE_CONSTRUCTION selected={selected} fixture={fixture_profile}; pre-existing holder seed is not prototype lifecycle acceptance");
    let f = F::new();
    let e = &f.e;
    MEASURE.with(|m| m.set(false));
    // HARNESS ONLY capacity for a pre-existing1B position plus the new200M job.
    auth::<()>(e, &f.admin, &f.share, "set_max_deploy", (f.admin.clone(), 2_000_000_000u128).into_val(e), &[]);
    let seed = 1_000_000_000i128;
    let subs = [MockAuthInvoke { contract: &f.under, fn_name: "transfer", args: (f.user.clone(), f.share.clone(), seed).into_val(e), sub_invokes: &[] }];
    let shares: i128 = auth(e, &f.user, &f.share, "deposit_underlying", (f.user.clone(), seed, 1i128).into_val(e), &subs);
    assert!(shares > 0);
    let escrow = wasm(e, "cl_split_escrow");
    auth::<()>(e, &f.admin, &escrow, "initialize", (f.admin.clone(), f.margin.clone(), f.share.clone()).into_val(e), &[]);
    auth::<()>(e, &f.admin, &f.share, "register_custodian", (escrow.clone(),).into_val(e), &[]);
    auth::<()>(e, &f.admin, &f.margin, "set_split_escrow", (f.admin.clone(), escrow.clone()).into_val(e), &[]);
    policy(&f, 6000, true);
    let adapter = Address::from_str(e, &crate::resource_profile::contract_address("adapter").unwrap());
    let plane = Address::from_str(e, &crate::resource_profile::contract_address("plane").unwrap());
    crate::resource_profile::register_contracts(e, &[("v3", &f.margin), ("facade", &f.facade), ("vault", &f.share), ("aquarius", &f.pool), ("cash_receipt", &f.cash), ("share_receipt", &f.receipt), ("adapter", &adapter), ("lending_controller", &f.controller), ("oracle", &f.oracle), ("plane", &plane), ("underlying", &f.under), ("paired", &f.paired), ("reward", &f.reward), ("split_escrow", &escrow)]);
    drop(restore_profile);
    crate::cl_resource_limits::configure(e);
    MEASURE.with(|m| m.set(true));
    (f, escrow)
}
fn policy(f: &F, bps: u32, enabled: bool) {
    auth::<()>(&f.e, &f.admin, &f.cash, "set_min_cash_reserve_bps", (f.admin.clone(), bps).into_val(&f.e), &[]);
    auth::<()>(&f.e, &f.admin, &f.cash, "set_cash_borrowing_enabled", (f.admin.clone(), enabled).into_val(&f.e), &[]);
}
fn next(e: &Env) {
    e.ledger().with_mut(|l| { l.timestamp += 5; l.sequence_number += 1; });
}
fn stage<T: soroban_sdk::TryFromVal<Env, Val>>(f: &F, name: &str, args: Vec<Val>) -> T {
    next(&f.e);
    let start = std::time::Instant::now();
    let result = auth(&f.e, &f.user, &f.margin, name, args, &[]);
    std::println!("SPLIT_STAGE_WALL function={name} local_ms={} ledger={} timestamp={}", start.elapsed().as_millis(), f.e.ledger().sequence(), f.e.ledger().timestamp());
    result
}
fn state(f: &F, id: u64) -> SplitState { call(&f.e, &f.margin, "get_split_open", (id,).into_val(&f.e)) }
fn fund(f: &F, id: u64) {
    stage::<()>(f, "split_fund", (f.user.clone(), id, 110_000_000u128, 210_000_000u128).into_val(&f.e));
    assert_eq!(state(f, id).phase, 1);
    assert!(call::<u128>(&f.e, &f.cash, "get_margin_borrow_balance", (id,).into_val(&f.e)) > 0);
}
fn prepare(f: &F, id: u64) {
    let _: BytesN<32> = stage(f, "split_convert_prepare", (f.user.clone(), id, 100_000_000u128, 90_000_000u128).into_val(&f.e));
    assert_eq!(state(f, id).phase, 3);
}
fn mint(f: &F, id: u64) -> u128 {
    let n: u128 = stage(f, "split_mint", (f.user.clone(), id, 194_040_000u128).into_val(&f.e));
    assert!(n >= 194_040_000);
    assert_eq!(state(f, id).phase, 4);
    n
}
fn recover(f: &F, id: u64) {
    next(&f.e);
    let max = 110_000_000u128;
    let transfer = [MockAuthInvoke { contract: &f.under, fn_name: "transfer", args: (f.user.clone(), f.cash.clone(), max as i128).into_val(&f.e), sub_invokes: &[] }];
    let repay = [MockAuthInvoke { contract: &f.cash, fn_name: "repay_full_for_margin", args: (id, f.user.clone(), max).into_val(&f.e), sub_invokes: &transfer }];
    let paid: u128 = auth(&f.e, &f.user, &f.margin, "split_recover", (f.user.clone(), id, max).into_val(&f.e), &repay);
    assert!(paid > 0);
    assert_eq!(call::<u128>(&f.e, &f.cash, "get_margin_borrow_balance", (id,).into_val(&f.e)), 0);
    assert_eq!(state(f, id).phase, 6);
}
fn reject(f: &F, who: &Address, name: &str, args: Vec<Val>, label: &str) {
    next(&f.e);
    f.e.mock_auths(&[MockAuth { address: who, invoke: &MockAuthInvoke { contract: &f.margin, fn_name: name, args: args.clone(), sub_invokes: &[] } }]);
    let before = f.e.to_ledger_snapshot().ledger_entries;
    crate::transaction_evidence::capture_rejection_before(&f.e, &f.margin, name, &args, Some(who), false, label);
    crate::cl_resource_limits::reset(&f.e);
    assert!(f.e.try_invoke_contract::<Val, soroban_sdk::Error>(&f.margin, &Symbol::new(&f.e, name), args).is_err(), "{label}");
    measure(&f.e, label, true);
    assert_eq!(before, f.e.to_ledger_snapshot().ledger_entries, "{label}: full rollback");
}

#[test]
fn compiled_split_open_complete() {
    let (f, escrow) = setup();
    let initial = f.e.ledger().timestamp();
    let id = begin(&f);
    fund(&f, id);
    let converted: u128 = stage(&f, "split_convert", (f.user.clone(), id, 100_000_000u128, 90_000_000u128).into_val(&f.e));
    assert!(converted >= 90_000_000);
    let _: BytesN<32> = stage(&f, "split_prepare", (f.user.clone(), id).into_val(&f.e));
    assert_eq!(state(&f, id).phase, 3);
    let n = mint(&f, id);
    assert_eq!(token::Client::new(&f.e, &f.share).balance(&escrow), n as i128);
    let delivered: u128 = stage(&f, "split_deliver", (f.user.clone(), id).into_val(&f.e));
    assert_eq!(delivered, n);
    assert_eq!(token::Client::new(&f.e, &f.share).balance(&escrow), 0);
    assert_eq!(state(&f, id).phase, 5);
    stage::<()>(&f, "activate_open_position_v3", (f.user.clone(), id).into_val(&f.e));
    std::println!("SPLIT_COMPLETE opening_transactions=7 fixture_margin_funding_transactions=1 test_ledger_latency_seconds={} all_claim_boundaries_retained=true", f.e.ledger().timestamp()-initial);
    stage::<()>(&f,"prepare_close_position_v3",(f.user.clone(),id).into_val(&f.e));
    let held=token::Client::new(&f.e,&f.share).balance(&f.margin);
    let quoted:u128=call(&f.e,&f.facade,"estimate_swap",(1u32,0u32,held as u128).into_val(&f.e));
    stage::<()>(&f,"swap_close_position_v3",(f.user.clone(),id,quoted).into_val(&f.e));
    next(&f.e);
    call::<()>(&f.e,&f.margin,"finish_close_position_v3",(id,).into_val(&f.e));
    measure(&f.e,"finish_close_position_v3",true);
    assert_eq!(call::<u128>(&f.e,&f.cash,"get_margin_borrow_balance",(id,).into_val(&f.e)),0);
    std::println!("SPLIT_LIFECYCLE product_transactions=11 including_margin_transfer=true quote_read_is_separate_unsigned_view=true test_ledger_latency_seconds={}",f.e.ledger().timestamp()-initial);
}

#[test]
fn compiled_split_funded_expiry_pause_recovery() {
    let (f, escrow) = setup();
    let id = begin(&f);
    fund(&f, id);
    let before: Job = call(&f.e, &escrow, "get_job", (id,).into_val(&f.e));
    reject(&f, &f.user, "split_fund", (f.user.clone(), id, 110_000_000u128, 210_000_000u128).into_val(&f.e), "split_duplicate_fund_reject");
    reject(&f, &f.user, "cancel_pending_open_v3", (f.user.clone(), id).into_val(&f.e), "split_funded_cancel_reject");
    policy(&f, 10000, false);
    f.e.ledger().with_mut(|l| {l.timestamp=before.deadline+1;l.sequence_number+=500;});
    reject(&f, &f.user, "split_convert", (f.user.clone(), id, 100_000_000u128, 90_000_000u128).into_val(&f.e), "split_expired_convert_reject");
    let wallet = token::Client::new(&f.e, &f.under).balance(&f.user);
    recover(&f, id);
    assert_eq!(token::Client::new(&f.e, &f.under).balance(&escrow), 0);
    assert!(token::Client::new(&f.e, &f.under).balance(&f.user) > wallet);
    reject(&f, &f.user, "split_recover", (f.user.clone(), id, 110_000_000u128).into_val(&f.e), "split_duplicate_recovery_reject");
}

#[path = "adversarial.rs"]
mod adversarial;

#[path = "fee_differential.rs"]
mod fee_differential;

#[path = "exit_assessment.rs"]
mod exit_assessment;

// These tests isolate complete later transactions from an explicitly constructed
// captured-limit prestate. They are NOT a claim that the entire lifecycle passed
// internal limits. The complete captured lifecycle separately measures every step.
fn prestate(f: &F, action: impl FnOnce()) {
    let selected = std::env::var("CL_RESOURCE_PROFILE").unwrap();
    let restore = RestoreProfile(selected);
    std::env::set_var("CL_RESOURCE_PROFILE", std::env::var("CL_SPLIT_SETUP_PROFILE").unwrap());
    MEASURE.with(|m|m.set(false));
    crate::cl_resource_limits::configure(&f.e);
    action();
    drop(restore);
    crate::cl_resource_limits::configure(&f.e);
    MEASURE.with(|m|m.set(true));
    std::println!("SPLIT_ISOLATED_PRESTATE completed; subsequent top-level transaction enforces selected limits; no lifecycle acceptance inferred");
}
fn id_with_funds(f:&F)->u64 {
    let mut id=0;
    prestate(f,||{id=begin(f);fund(f,id);});
    id
}
#[test]
fn compiled_split_isolated_convert() {
    let (f,_)=setup();let id=id_with_funds(&f);
    let n:u128=stage(&f,"split_convert",(f.user.clone(),id,100_000_000u128,90_000_000u128).into_val(&f.e));assert!(n>=90_000_000);
}
#[test]
fn compiled_split_isolated_prepare() {
    let (f,_)=setup();let id=id_with_funds(&f);
    prestate(&f,||{let _:u128=stage(&f,"split_convert",(f.user.clone(),id,100_000_000u128,90_000_000u128).into_val(&f.e));});
    let _:BytesN<32>=stage(&f,"split_prepare",(f.user.clone(),id).into_val(&f.e));assert_eq!(state(&f,id).phase,3);
}
#[test]
fn compiled_split_isolated_mint() {
    let (f,_)=setup();let id=id_with_funds(&f);prestate(&f,||prepare(&f,id));mint(&f,id);
}
#[test]
fn compiled_split_isolated_delivery() {
    let (f,escrow)=setup();let id=id_with_funds(&f);prestate(&f,||{prepare(&f,id);mint(&f,id);});
    let n:u128=stage(&f,"split_deliver",(f.user.clone(),id).into_val(&f.e));assert!(n>0);assert_eq!(token::Client::new(&f.e,&f.share).balance(&escrow),0);assert_eq!(state(&f,id).phase,5);
}
#[test]
fn compiled_split_isolated_activation() {
    let (f,_)=setup();let id=id_with_funds(&f);prestate(&f,||{prepare(&f,id);mint(&f,id);let _:u128=stage(&f,"split_deliver",(f.user.clone(),id).into_val(&f.e));});
    stage::<()>(&f,"activate_open_position_v3",(f.user.clone(),id).into_val(&f.e));
}

#[path = "late_recovery.rs"]
mod late_recovery;
