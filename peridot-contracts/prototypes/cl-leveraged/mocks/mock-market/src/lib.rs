#![no_std]
//! MOCK group USDC lending market, margin namespace only (dependency X4). Mirrors the
//! receipt-vault entrypoint names and their authorization shape: the configured margin
//! controller must authorize borrow; the payer authorizes repayment, whose USDC transfer is
//! invoked by this market (so a contract payer must pre-authorize that nested transfer).
//! Interest: simple linear accrual at a fixture per-second rate (1e18-scaled). Not the real
//! market's footprint: its rows are marked as group-dependent in the measurements.
use soroban_sdk::{contract, contracttype, contractimpl, token, Address, Env};

#[contracttype]
#[derive(Clone)]
struct Debt {
    principal: u128,
    ts: u64,
}
#[contracttype]
enum K {
    Usdc,
    Controller,
    Margin,
    Rate,
    Debt(u64),
}

#[contract]
pub struct MockMarket;

fn accrued(e: &Env, d: &Debt) -> u128 {
    let rate: u128 = e.storage().instance().get(&K::Rate).unwrap_or(0);
    let dt = e.ledger().timestamp().saturating_sub(d.ts) as u128;
    let interest = d.principal.checked_mul(rate).unwrap().checked_mul(dt).unwrap() / 1_000_000_000_000_000_000u128;
    // Round up, as a lender would.
    d.principal + interest + if rate > 0 && dt > 0 && d.principal > 0 { 1 } else { 0 }
}

#[contractimpl]
impl MockMarket {
    pub fn mock_init(e: Env, usdc: Address, controller: Address, margin: Address, rate_per_sec_1e18: u128) {
        let s = e.storage().instance();
        s.set(&K::Usdc, &usdc);
        s.set(&K::Controller, &controller);
        s.set(&K::Margin, &margin);
        s.set(&K::Rate, &rate_per_sec_1e18);
    }
    pub fn borrow_for_margin_to_controller(e: Env, position_id: u64, amount: u128) {
        let s = e.storage().instance();
        let margin: Address = s.get(&K::Margin).unwrap();
        margin.require_auth();
        assert!(amount > 0, "bad amount");
        let ctrl: Address = s.get(&K::Controller).unwrap();
        let paused: bool = e.invoke_contract(&ctrl, &soroban_sdk::Symbol::new(&e, "is_borrow_paused"), soroban_sdk::vec![&e, soroban_sdk::IntoVal::into_val(&e.current_contract_address(), &e)]);
        assert!(!paused, "borrow paused");
        let key = K::Debt(position_id);
        let cur = e.storage().persistent().get::<_, Debt>(&key).map(|d| accrued(&e, &d)).unwrap_or(0);
        e.storage().persistent().set(&key, &Debt { principal: cur + amount, ts: e.ledger().timestamp() });
        let usdc: Address = s.get(&K::Usdc).unwrap();
        token::Client::new(&e, &usdc).transfer(&e.current_contract_address(), &margin, &(amount as i128));
    }
    pub fn repay_for_margin(e: Env, position_id: u64, payer: Address, amount: u128) {
        let s = e.storage().instance();
        let _margin: Address = s.get(&K::Margin).unwrap();
        payer.require_auth();
        let key = K::Debt(position_id);
        let d: Debt = e.storage().persistent().get(&key).expect("no margin debt");
        let cur = accrued(&e, &d);
        assert!(amount > 0 && amount <= cur, "bad repay");
        let usdc: Address = s.get(&K::Usdc).unwrap();
        token::Client::new(&e, &usdc).transfer(&payer, &e.current_contract_address(), &(amount as i128));
        e.storage().persistent().set(&key, &Debt { principal: cur - amount, ts: e.ledger().timestamp() });
    }
    pub fn get_margin_borrow_balance(e: Env, position_id: u64) -> u128 {
        e.storage().persistent().get::<_, Debt>(&K::Debt(position_id)).map(|d| accrued(&e, &d)).unwrap_or(0)
    }
}
