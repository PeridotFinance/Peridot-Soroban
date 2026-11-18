//! Actual vault token entrypoints over a native controlled reward source.
//! Seeded share state is explicit; this is not a venue or principal-deposit proof.
use crate::{storage, ClShareVault, ClShareVaultClient};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger, MockAuth, MockAuthInvoke};
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, IntoVal, Symbol, Val, Vec,
};

#[contracttype]
#[derive(Clone)]
enum Key {
    Token,
    Pending,
    Fail,
}
#[contract]
struct RewardSource;
#[contractimpl]
impl RewardSource {
    pub fn init(e: Env, token: Address) {
        e.storage().instance().set(&Key::Token, &token);
    }
    pub fn accrue(e: Env, amount: u128) {
        e.storage().instance().set(&Key::Pending, &amount);
    }
    pub fn fail(e: Env, failed: bool) {
        e.storage().instance().set(&Key::Fail, &failed);
    }
    pub fn claim(e: Env, who: Address) -> u128 {
        who.require_auth();
        assert!(
            !e.storage().instance().get(&Key::Fail).unwrap_or(false),
            "claim outage"
        );
        let amount: u128 = e.storage().instance().get(&Key::Pending).unwrap_or(0);
        e.storage().instance().set(&Key::Pending, &0u128);
        let t: Address = e.storage().instance().get(&Key::Token).unwrap();
        if amount > 0 {
            token::Client::new(&e, &t).transfer(
                &e.current_contract_address(),
                &who,
                &(amount as i128),
            );
        }
        amount
    }
}
#[contract]
struct Spender;
#[contractimpl]
impl Spender {
    pub fn twice(e: Env, vault: Address, from: Address, to: Address) {
        let c = ClShareVaultClient::new(&e, &vault);
        c.transfer_from(&e.current_contract_address(), &from, &to, &200);
        c.transfer_from(&e.current_contract_address(), &from, &to, &300);
    }
    pub fn approve(e: Env, vault: Address, spender: Address) {
        ClShareVaultClient::new(&e, &vault).approve(
            &e.current_contract_address(),
            &spender,
            &100,
            &200,
        );
    }
}
struct F {
    e: Env,
    v: Address,
    p: Address,
    a: Address,
    b: Address,
    s: Address,
    admin: Address,
}
impl F {
    fn new() -> Self {
        let e = Env::default();
        e.ledger().with_mut(|l| l.sequence_number = 100);
        let admin = Address::generate(&e);
        let a = Address::generate(&e);
        let b = Address::generate(&e);
        let s = Address::generate(&e);
        let t = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let p = e.register(RewardSource, ());
        let v = e.register(ClShareVault, ());
        RewardSourceClient::new(&e, &p).init(&t);
        e.mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &t,
                fn_name: "mint",
                args: (p.clone(), 1000i128).into_val(&e),
                sub_invokes: &[],
            },
        }]);
        token::StellarAssetClient::new(&e, &t).mint(&p, &1000);
        e.mock_auths(&[]);
        e.as_contract(&v, || {
            storage::set_config(
                &e,
                &storage::Config {
                    pool: p.clone(),
                    token0: t.clone(),
                    token1: t.clone(),
                    dec0: 7,
                    dec1: 7,
                    underlying_index: 0,
                    tick_lower: -20,
                    tick_upper: 20,
                    oracle: t.clone(),
                },
            );
            storage::set_primary_reward(&e, &Some(t));
            e.storage().instance().set(&storage::DataKey::Admin, &admin);
            let mut state = storage::state(&e);
            state.total_shares = 1000;
            storage::set_state(&e, &state);
            storage::set_shares(&e, &a, 1000);
        });
        Self {
            e,
            v,
            p,
            a,
            b,
            s,
            admin,
        }
    }
    fn c(&self) -> ClShareVaultClient<'_> {
        ClShareVaultClient::new(&self.e, &self.v)
    }
    fn auth(&self, who: &Address, name: &str, args: Vec<Val>) {
        self.e.mock_auths(&[MockAuth {
            address: who,
            invoke: &MockAuthInvoke {
                contract: &self.v,
                fn_name: name,
                args,
                sub_invokes: &[],
            },
        }]);
    }
    fn approve(&self, spender: &Address, amount: i128, expiration: u32) {
        self.auth(
            &self.a,
            "approve",
            (self.a.clone(), spender.clone(), amount, expiration).into_val(&self.e),
        );
        self.c().approve(&self.a, spender, &amount, &expiration);
    }
    fn move_by_spender(&self, amount: i128) {
        self.auth(
            &self.s,
            "transfer_from",
            (self.s.clone(), self.a.clone(), self.b.clone(), amount).into_val(&self.e),
        );
        self.c().transfer_from(&self.s, &self.a, &self.b, &amount);
    }
}
#[test]
fn allowance_authorization_expiration_revocation_and_events() {
    let f = F::new();
    assert!(f.c().try_approve(&f.a, &f.s, &500, &101).is_err());
    for (amount, expiration) in [(-1, 101), (1, 99), (1, 6_312_200)] {
        f.auth(
            &f.a,
            "approve",
            (f.a.clone(), f.s.clone(), amount, expiration).into_val(&f.e),
        );
        assert!(f.c().try_approve(&f.a, &f.s, &amount, &expiration).is_err());
    }
    f.approve(&f.s, 500, 101);
    let approval = (
        f.v.clone(),
        (Symbol::new(&f.e, "approve"), f.a.clone(), f.s.clone()).into_val(&f.e),
        (500i128, 101u32).into_val(&f.e),
    );
    assert_eq!(
        f.e.events().all().filter_by_contract(&f.v),
        soroban_sdk::vec![&f.e, approval.clone()]
    );
    assert_eq!(f.c().allowance(&f.a, &f.s), 500);
    f.e.ledger().with_mut(|l| l.sequence_number = 101);
    f.move_by_spender(100);
    let transfer = (
        f.v.clone(),
        (Symbol::new(&f.e, "transfer"), f.a.clone(), f.b.clone()).into_val(&f.e),
        100i128.into_val(&f.e),
    );
    assert_eq!(
        f.e.events().all().filter_by_contract(&f.v),
        soroban_sdk::vec![&f.e, transfer]
    );
    assert_eq!(f.c().allowance(&f.a, &f.s), 400);
    f.e.ledger().with_mut(|l| l.sequence_number = 102);
    assert_eq!(f.c().allowance(&f.a, &f.s), 0);
    f.auth(
        &f.s,
        "transfer_from",
        (f.s.clone(), f.a.clone(), f.b.clone(), 100i128).into_val(&f.e),
    );
    assert!(f.c().try_transfer_from(&f.s, &f.a, &f.b, &100).is_err());
    f.approve(&f.s, 300, 105);
    f.approve(&f.s, 0, 0);
    assert_eq!(f.c().allowance(&f.a, &f.s), 0);
}
#[test]
fn allowance_transfer_claims_at_old_balances_and_spender_does_not_earn() {
    let f = F::new();
    let p = RewardSourceClient::new(&f.e, &f.p);
    p.accrue(&10);
    f.approve(&f.s, 500, 200);
    assert_eq!(
        f.c().reward_totals().received,
        0,
        "approval cannot checkpoint ownership"
    );
    f.move_by_spender(500);
    assert_eq!(f.c().allowance(&f.a, &f.s), 0);
    p.accrue(&90);
    assert!(f.c().checkpoint_rewards());
    assert_eq!(f.c().reward_account(&f.a).pending, 55);
    assert_eq!(f.c().reward_account(&f.b).pending, 45);
    assert_eq!(f.c().reward_account(&f.s).pending, 0);
    assert_eq!(f.c().reward_totals().received, 100);
}
#[test]
fn allowance_claim_failure_and_wrong_authorization_revert_all_changes() {
    let f = F::new();
    f.approve(&f.s, 500, 200);
    f.e.mock_auths(&[]);
    assert!(f.c().try_transfer_from(&f.s, &f.a, &f.b, &500).is_err());
    f.auth(
        &f.s,
        "transfer_from",
        (f.s.clone(), f.a.clone(), f.b.clone(), 400i128).into_val(&f.e),
    );
    assert!(f.c().try_transfer_from(&f.s, &f.a, &f.b, &500).is_err());
    let p = RewardSourceClient::new(&f.e, &f.p);
    p.accrue(&10);
    p.fail(&true);
    f.auth(
        &f.s,
        "transfer_from",
        (f.s.clone(), f.a.clone(), f.b.clone(), 500i128).into_val(&f.e),
    );
    assert_eq!(
        f.c().try_transfer_from(&f.s, &f.a, &f.b, &500),
        Err(Ok(crate::rewards::RewardError::ClaimUnavailable.into()))
    );
    assert_eq!(f.c().allowance(&f.a, &f.s), 500);
    assert_eq!(f.c().balance(&f.a), 1000);
    assert_eq!(f.c().balance(&f.b), 0);
    assert_eq!(f.c().reward_totals().received, 0);
    p.fail(&false);
    f.move_by_spender(500);
    assert_eq!(f.c().reward_account(&f.a).pending, 10);
    assert_eq!(f.c().reward_account(&f.b).pending, 0);
}
#[test]
fn real_contract_spender_multiple_moves_zero_claims_and_no_custody_bypass() {
    let f = F::new();
    let spender = f.e.register(Spender, ());
    f.approve(&spender, 500, 200);
    RewardSourceClient::new(&f.e, &f.p).accrue(&10);
    f.e.mock_auths(&[]);
    SpenderClient::new(&f.e, &spender).twice(&f.v, &f.a, &f.b);
    assert_eq!(f.c().allowance(&f.a, &spender), 0);
    assert_eq!(f.c().balance(&f.b), 500);
    assert_eq!(f.c().reward_account(&f.a).pending, 10);
    assert_eq!(f.c().reward_account(&f.b).pending, 0);
    assert_eq!(f.c().reward_totals().received, 10);
    f.auth(
        &f.admin,
        "register_custodian",
        (spender.clone(),).into_val(&f.e),
    );
    f.c().register_custodian(&spender);
    f.e.mock_auths(&[]);
    assert!(SpenderClient::new(&f.e, &spender)
        .try_approve(&f.v, &f.s)
        .is_err());
    f.auth(
        &f.s,
        "transfer_from",
        (f.s.clone(), spender.clone(), f.b.clone(), 0i128).into_val(&f.e),
    );
    assert!(f.c().try_transfer_from(&f.s, &spender, &f.b, &0).is_err());
}
#[test]
fn zero_and_self_delegated_transfers_preserve_balances_and_reward_owner() {
    let f = F::new();
    f.approve(&f.s, 200, 200);
    f.move_by_spender(0);
    assert_eq!(f.c().allowance(&f.a, &f.s), 200);
    assert_eq!(f.c().balance(&f.a), 1000);
    RewardSourceClient::new(&f.e, &f.p).accrue(&10);
    f.auth(
        &f.s,
        "transfer_from",
        (f.s.clone(), f.a.clone(), f.a.clone(), 200i128).into_val(&f.e),
    );
    f.c().transfer_from(&f.s, &f.a, &f.a, &200);
    assert_eq!(f.c().allowance(&f.a, &f.s), 0);
    assert_eq!(f.c().balance(&f.a), 1000);
    assert_eq!(f.c().reward_account(&f.a).pending, 10);
}
