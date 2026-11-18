//! D3 (cash-borrow upgrade checks, 2026-11-18): flash loans at a 100% fee.
//! Native fixture with real ReceiptVault + SimplePeridottroller, SACs and a
//! per-asset mock oracle. Not a network or footprint proof.
extern crate std;

use crate::{DataKey, ReceiptVault, ReceiptVaultClient};
use simple_peridottroller::{SimplePeridottroller, SimplePeridottrollerClient};
use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::Address as _, token, Address, Bytes, Env,
    IntoVal, InvokeError, Symbol, Val, Vec,
};

const FULL_FEE: u128 = 1_000_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum FxAsset {
    Stellar(Address),
    Other(Symbol),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FxPriceData {
    pub price: i128,
    pub timestamp: u64,
}

#[contracttype]
enum FxOracleKey {
    Price(Address),
}

#[contract]
pub struct FxOracle;

#[contractimpl]
impl FxOracle {
    pub fn set(env: Env, asset: Address, price: i128) {
        env.storage().persistent().set(&FxOracleKey::Price(asset), &price);
    }
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn resolution(_env: Env) -> u32 {
        300
    }
    pub fn lastprice(env: Env, asset: FxAsset) -> Option<FxPriceData> {
        let FxAsset::Stellar(a) = asset else { return None };
        let p: i128 = env.storage().persistent().get(&FxOracleKey::Price(a)).unwrap_or(10_000_000);
        Some(FxPriceData { price: p, timestamp: env.ledger().timestamp() })
    }
}

#[contracttype]
#[derive(Clone)]
enum AtkKey {
    Cfg,
    Results,
}

#[contracttype]
#[derive(Clone)]
pub struct AtkCfg {
    pub vault_a: Address,
    pub token_a: Address,
    pub vault_b: Address,
    pub comp: Address,
    pub victim: Address,
    pub outsider: Address,
}

/// (label, ok, error code: 0 = ok, u32::MAX = abort/host error, else contract error)
pub type Outcome = (Symbol, bool, u32);

#[contract]
pub struct FlashAttacker;

fn record(env: &Env, out: &mut Vec<Outcome>, label: &str, r: Result<(), u32>) {
    out.push_back((Symbol::new(env, label), r.is_ok(), r.err().unwrap_or(0)));
}

fn attempt(env: &Env, target: &Address, func: &str, args: Vec<Val>) -> Result<(), u32> {
    match env.try_invoke_contract::<Val, InvokeError>(target, &Symbol::new(env, func), args) {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(_)) => Err(u32::MAX - 1),
        Err(Ok(InvokeError::Contract(c))) => Err(c),
        Err(Ok(InvokeError::Abort)) => Err(u32::MAX),
        Err(Err(_)) => Err(u32::MAX - 2),
    }
}

#[contractimpl]
impl FlashAttacker {
    pub fn configure(env: Env, cfg: AtkCfg) {
        env.storage().persistent().set(&AtkKey::Cfg, &cfg);
    }
    pub fn results(env: Env) -> Vec<Outcome> {
        env.storage().persistent().get(&AtkKey::Results).unwrap_or(Vec::new(&env))
    }
    /// Runs the same attempts outside a flash loan (positive control).
    pub fn run_attempts(env: Env) -> Vec<Outcome> {
        let out = Self::attempts(&env);
        env.storage().persistent().set(&AtkKey::Results, &out);
        out
    }
    fn attempts(env: &Env) -> Vec<Outcome> {
        let c: AtkCfg = env.storage().persistent().get(&AtkKey::Cfg).unwrap();
        let me = env.current_contract_address();
        let mut out = Vec::new(env);
        let a = &c.vault_a;
        record(env, &mut out, "a_borrow", attempt(env, a, "borrow", (me.clone(), 10u128).into_val(env)));
        record(env, &mut out, "a_withdraw", attempt(env, a, "withdraw", (me.clone(), 10u128).into_val(env)));
        record(env, &mut out, "a_deposit", attempt(env, a, "deposit", (me.clone(), 10u128).into_val(env)));
        record(env, &mut out, "a_repay", attempt(env, a, "repay", (me.clone(), 1u128).into_val(env)));
        record(env, &mut out, "a_repay_max", attempt(env, a, "repay_max", (me.clone(),).into_val(env)));
        record(env, &mut out, "a_transfer", attempt(env, a, "transfer", (me.clone(), c.outsider.clone(), 1i128).into_val(env)));
        // View with no flash guard: fails only if the host forbids re-entry.
        record(env, &mut out, "a_view_rate", attempt(env, a, "get_exchange_rate", ().into_val(env)));
        // Liquidations of the victim (shortfall exists before the flash loan).
        record(env, &mut out, "liq_repay_a", attempt(env, &c.comp, "liquidate",
            (c.victim.clone(), a.clone(), c.vault_b.clone(), 10u128, me.clone()).into_val(env)));
        record(env, &mut out, "liq_coll_a", attempt(env, &c.comp, "liquidate",
            (c.victim.clone(), c.vault_b.clone(), a.clone(), 10u128, me.clone()).into_val(env)));
        // Cross-market on B: attacker has A entered with collateral; outsider has only B.
        record(env, &mut out, "b_borrow_in_a", attempt(env, &c.vault_b, "borrow", (me.clone(), 10u128).into_val(env)));
        record(env, &mut out, "b_withdraw_in_a", attempt(env, &c.vault_b, "withdraw", (me.clone(), 10u128).into_val(env)));
        record(env, &mut out, "b_borrow_outsider", attempt(env, &c.vault_b, "borrow", (c.outsider.clone(), 10u128).into_val(env)));
        record(env, &mut out, "b_withdraw_outsider", attempt(env, &c.vault_b, "withdraw", (c.outsider.clone(), 10u128).into_val(env)));
        out
    }
    pub fn on_flash_loan(env: Env, vault: Address, amount: u128, fee: u128, _data: Bytes) {
        let out = Self::attempts(&env);
        env.storage().persistent().set(&AtkKey::Results, &out);
        let c: AtkCfg = env.storage().persistent().get(&AtkKey::Cfg).unwrap();
        token::Client::new(&env, &c.token_a).transfer(
            &env.current_contract_address(),
            &vault,
            &((amount + fee) as i128),
        );
    }
}

struct Fx {
    env: Env,
    admin: Address,
    comp: Address,
    oracle: Address,
    a: Address,
    ta: Address,
    b: Address,
    tb: Address,
    atk: Address,
    victim: Address,
    outsider: Address,
}

fn cash_market(env: &Env, admin: &Address, comp: &Address) -> (Address, Address) {
    let t = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let v = env.register(ReceiptVault, ());
    let c = ReceiptVaultClient::new(env, &v);
    c.initialize(&t, &0u128, &0u128, admin);
    c.enable_static_rates(admin);
    c.set_min_cash_reserve_bps(admin, &0u32);
    token::StellarAssetClient::new(env, &t).mint(admin, &1_000);
    c.seed_minimum_supply(admin, &1_000u128);
    c.set_peridottroller(comp);
    let cc = SimplePeridottrollerClient::new(env, comp);
    cc.add_market(&v);
    cc.set_market_cf(&v, &500_000u128);
    c.set_cash_borrowing_enabled(admin, &true);
    c.set_flash_loan_fee(&FULL_FEE);
    c.set_flash_loans_enabled(admin, &true);
    (v, t)
}

impl Fx {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        env.cost_estimate().disable_resource_limits();
        env.cost_estimate().budget().reset_unlimited();
        let admin = Address::generate(&env);
        let comp = env.register(SimplePeridottroller, ());
        let cc = SimplePeridottrollerClient::new(&env, &comp);
        cc.initialize(&admin);
        let oracle = env.register(FxOracle, ());
        cc.set_oracle(&oracle);
        let (a, ta) = cash_market(&env, &admin, &comp);
        let (b, tb) = cash_market(&env, &admin, &comp);
        let atk = env.register(FlashAttacker, ());
        let victim = Address::generate(&env);
        let outsider = Address::generate(&env);
        let f = Fx { env, admin, comp, oracle, a, ta, b, tb, atk, victim, outsider };
        let supplier = Address::generate(&f.env);
        f.fund(&supplier, &f.a, &f.ta, 100_000);
        f.fund(&supplier, &f.b, &f.tb, 100_000);
        // Attacker: collateral in A and B, small debt in A (both markets entered).
        f.fund(&f.atk, &f.a, &f.ta, 10_000);
        f.fund(&f.atk, &f.b, &f.tb, 10_000);
        f.cc().enter_market(&f.atk, &f.a);
        f.cc().enter_market(&f.atk, &f.b);
        f.va().borrow(&f.atk, &100u128);
        // Inventory for repayment of amount + 100% fee and for liquidation repay.
        f.mint(&f.ta, &f.atk, 50_000);
        f.mint(&f.tb, &f.atk, 50_000);
        let live = f.env.ledger().sequence() + 100_000;
        token::Client::new(&f.env, &f.ta).approve(&f.atk, &f.a, &50_000, &live);
        token::Client::new(&f.env, &f.tb).approve(&f.atk, &f.b, &50_000, &live);
        // Victim: collateral A and B, debt in A and B; then made liquidatable.
        f.fund(&f.victim, &f.a, &f.ta, 2_000);
        f.fund(&f.victim, &f.b, &f.tb, 100);
        f.cc().enter_market(&f.victim, &f.a);
        f.cc().enter_market(&f.victim, &f.b);
        f.va().borrow(&f.victim, &50u128);
        f.vb().borrow(&f.victim, &900u128);
        // Outsider: only B entered, collateral only.
        f.fund(&f.outsider, &f.b, &f.tb, 1_000);
        f.cc().enter_market(&f.outsider, &f.b);
        FxOracleClient::new(&f.env, &f.oracle).set(&f.ta, &5_000_000); // A token -> $0.50
        // Prices are repriced live; refresh caches so the victim is in shortfall now.
        f.cc().cache_price(&f.ta);
        f.cc().cache_price(&f.tb);
        let (_liq, shortfall) = f.cc().account_liquidity(&f.victim);
        assert!(shortfall > 0, "fixture: victim must be liquidatable");
        FlashAttackerClient::new(&f.env, &f.atk).configure(&AtkCfg {
            vault_a: f.a.clone(),
            token_a: f.ta.clone(),
            vault_b: f.b.clone(),
            comp: f.comp.clone(),
            victim: f.victim.clone(),
            outsider: f.outsider.clone(),
        });
        f
    }
    fn cc(&self) -> SimplePeridottrollerClient<'_> {
        SimplePeridottrollerClient::new(&self.env, &self.comp)
    }
    fn va(&self) -> ReceiptVaultClient<'_> {
        ReceiptVaultClient::new(&self.env, &self.a)
    }
    fn vb(&self) -> ReceiptVaultClient<'_> {
        ReceiptVaultClient::new(&self.env, &self.b)
    }
    fn mint(&self, t: &Address, to: &Address, amt: i128) {
        token::StellarAssetClient::new(&self.env, t).mint(to, &amt);
    }
    fn fund(&self, who: &Address, v: &Address, t: &Address, amt: u128) {
        self.mint(t, who, amt as i128);
        ReceiptVaultClient::new(&self.env, v).deposit(who, &amt);
    }
    fn snapshot(&self) -> std::vec::Vec<u128> {
        let mut s = std::vec::Vec::new();
        let ca = self.va();
        s.push(ca.get_total_borrowed());
        s.push(ca.get_total_ptokens());
        s.push(ca.get_exchange_rate());
        for v in [&self.a, &self.b] {
            let c = ReceiptVaultClient::new(&self.env, v);
            for u in [&self.atk, &self.victim] {
                s.push(c.get_ptoken_balance(u));
                s.push(c.get_user_borrow_balance(u));
            }
        }
        s
    }
}

fn outcome(results: &Vec<Outcome>, env: &Env, label: &str) -> (bool, u32) {
    for (l, ok, code) in results.iter() {
        if l == Symbol::new(env, label) {
            return (ok, code);
        }
    }
    panic!("missing outcome {label}");
}

#[test]
fn flash_full_fee_callback_cannot_reenter_or_liquidate() {
    let f = Fx::new();
    let before = f.snapshot();
    let reserves_before = f.va().get_total_reserves();
    let amount = 20_000u128;
    let atk_cash_before = token::Client::new(&f.env, &f.ta).balance(&f.atk);
    f.va().flash_loan(&f.atk, &f.atk, &amount, &Bytes::new(&f.env));
    let r = FlashAttackerClient::new(&f.env, &f.atk).results();
    std::println!("in-flash outcomes: {:?}", r);
    for label in [
        "a_borrow", "a_withdraw", "a_deposit", "a_repay", "a_repay_max", "a_transfer",
        "a_view_rate", "liq_repay_a", "liq_coll_a", "b_borrow_in_a", "b_withdraw_in_a",
    ] {
        let (ok, code) = outcome(&r, &f.env, label);
        assert!(!ok, "{label} must fail inside the flash callback");
        assert_eq!(code, u32::MAX, "{label}: expected host abort, got {code}");
    }
    // Unrelated account on another market is not blocked (no A frame touched).
    assert!(outcome(&r, &f.env, "b_borrow_outsider").0);
    assert!(outcome(&r, &f.env, "b_withdraw_outsider").0);
    // A and the attacker/victim positions are unchanged; only the 100% fee moved.
    let after = f.snapshot();
    assert_eq!(before, after);
    assert_eq!(f.va().get_total_reserves(), reserves_before + amount);
    assert_eq!(
        token::Client::new(&f.env, &f.ta).balance(&f.atk),
        atk_cash_before - amount as i128
    );
}

#[test]
fn flash_positive_control_same_calls_succeed_outside_flash() {
    let f = Fx::new();
    let r = FlashAttackerClient::new(&f.env, &f.atk).run_attempts();
    std::println!("outside-flash outcomes: {:?}", r);
    for label in ["a_borrow", "a_withdraw", "a_deposit", "a_repay", "a_repay_max", "a_transfer",
        "a_view_rate", "liq_repay_a", "liq_coll_a", "b_borrow_in_a", "b_withdraw_in_a", "b_borrow_outsider",
        "b_withdraw_outsider"] {
        assert!(outcome(&r, &f.env, label).0, "{label} should succeed outside a flash loan");
    }
}

#[test]
fn flash_positive_control_liquidation_seizing_flash_market() {
    let f = Fx::new();
    let before = f.va().get_ptoken_balance(&f.atk);
    f.cc().liquidate(&f.victim, &f.b, &f.a, &10u128, &f.atk);
    assert!(f.va().get_ptoken_balance(&f.atk) > before);
}

#[test]
#[should_panic(expected = "operation blocked during flash loan")]
fn flash_active_flag_blocks_borrow_without_host_reentry() {
    // Defense in depth: with A not on the call stack, the persistent flag alone blocks.
    let f = Fx::new();
    f.env.as_contract(&f.a, || {
        f.env.storage().persistent().set(&DataKey::FlashLoanActive, &true);
    });
    f.va().borrow(&f.atk, &10u128);
}

#[test]
fn flash_active_flag_does_not_guard_seize_but_host_reentry_does() {
    // `seize` has no ensure_not_in_flash_loan: with the flag set and no A frame,
    // a controller liquidation seizing A pTokens still succeeds. Inside a real
    // flash loan it is blocked by the host re-entry rule (see liq_coll_a above).
    let f = Fx::new();
    f.env.as_contract(&f.a, || {
        f.env.storage().persistent().set(&DataKey::FlashLoanActive, &true);
    });
    let before = f.va().get_ptoken_balance(&f.atk);
    f.cc().liquidate(&f.victim, &f.b, &f.a, &10u128, &f.atk);
    assert!(f.va().get_ptoken_balance(&f.atk) > before);
}

#[test]
fn flash_full_fee_charges_exactly_principal_and_keeps_exchange_rate() {
    let f = Fx::new();
    let rate = f.va().get_exchange_rate();
    assert_eq!(f.va().preview_flash_loan_fee(&20_000u128), 20_000u128);
    f.va().flash_loan(&f.atk, &f.atk, &20_000u128, &Bytes::new(&f.env));
    assert_eq!(f.va().get_exchange_rate(), rate);
}

// --- Fee math at u128 extremes ---------------------------------------------

fn deployed_saturating_fee(amount: u128, fee: u128) -> u128 {
    // Exact copy of the deployed (4229b7ef) formula.
    let n = amount.saturating_mul(fee);
    if n == 0 { 0 } else { n.saturating_sub(1) / 1_000_000 + 1 }
}

#[test]
fn flash_fee_deployed_formula_undercharges_above_threshold() {
    let t = u128::MAX / 1_000_000;
    assert_eq!(deployed_saturating_fee(1, FULL_FEE), 1);
    assert_eq!(deployed_saturating_fee(t, FULL_FEE), t);
    assert_eq!(deployed_saturating_fee(t + 1, FULL_FEE), t + 1);
    assert_eq!(deployed_saturating_fee(t + 2, FULL_FEE), t + 1); // first undercharge
    assert!(deployed_saturating_fee(u128::MAX, FULL_FEE) < u128::MAX / 1_000_000 + 2);
    // Unreachable for SAC assets: vault cash is an i128 balance and classic
    // assets are bounded by i64::MAX stroops, far below the threshold.
    assert!((i64::MAX as u128) < t);
}

#[test]
fn flash_fee_patched_formula_exact_at_extremes() {
    let f = Fx::new();
    let t = u128::MAX / 1_000_000;
    for amt in [1u128, 999_999, 1_000_000, t, t + 1, t + 2, u128::MAX] {
        assert_eq!(f.va().preview_flash_loan_fee(&amt), amt, "100% fee must equal principal");
    }
    f.va().set_flash_loan_fee(&3_000u128);
    for amt in [1u128, 333, 334, 1_000_000, 123_456_789, t, u128::MAX] {
        let q = amt / 1_000_000;
        let r = amt % 1_000_000;
        let expect = q * 3_000 + (r * 3_000 + 999_999) / 1_000_000;
        assert_eq!(f.va().preview_flash_loan_fee(&amt), expect);
    }
    f.va().set_flash_loan_fee(&0u128);
    assert_eq!(f.va().preview_flash_loan_fee(&u128::MAX), 0);
}

// --- Separate flash-loan switch ----------------------------------------------

#[test]
fn flash_switch_defaults_off_and_is_independent_of_borrowing() {
    let f = Fx::new();
    f.env.as_contract(&f.a, || {
        f.env.storage().instance().remove(&DataKey::FlashLoansEnabled);
    });
    assert!(!f.va().get_flash_loans_enabled());
    assert!(f.va().get_cash_borrowing_enabled());
    assert!(f.va().try_flash_loan(&f.atk, &f.atk, &100u128, &Bytes::new(&f.env)).is_err());
    // Borrowing still works with flash loans off.
    f.va().borrow(&f.atk, &10u128);
    f.va().set_flash_loans_enabled(&f.admin, &true);
    f.va().flash_loan(&f.atk, &f.atk, &100u128, &Bytes::new(&f.env));
    f.va().set_flash_loans_enabled(&f.admin, &false);
    assert!(f.va().try_flash_loan(&f.atk, &f.atk, &100u128, &Bytes::new(&f.env)).is_err());
    // Turning cash borrowing off does not re-enable or affect the flash switch.
    f.va().set_cash_borrowing_enabled(&f.admin, &false);
    assert!(!f.va().get_flash_loans_enabled());
}

#[test]
#[should_panic(expected = "flash loans disabled")]
fn flash_switch_off_rejects_before_other_checks() {
    let f = Fx::new();
    f.va().set_flash_loans_enabled(&f.admin, &false);
    f.va().flash_loan(&f.atk, &f.atk, &0u128, &Bytes::new(&f.env));
}

#[test]
#[should_panic(expected = "invalid admin")]
fn flash_switch_rejects_non_admin() {
    let f = Fx::new();
    let other = Address::generate(&f.env);
    f.va().set_flash_loans_enabled(&other, &true);
}
