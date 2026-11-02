//! Unit tests for the reward ledger, with a controlled upstream token/pool.
//! Principal/venue integration is measured separately by the compiled harness.
use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    token, Address, Env, IntoVal,
};
#[contracttype]
#[derive(Clone)]
enum K {
    Token,
    Pending,
    Fail,
    ViewFail,
}
#[contract]
struct Pool;
#[contractimpl]
impl Pool {
    pub fn initialize(e: Env, t: Address) {
        e.storage().instance().set(&K::Token, &t);
    }
    pub fn accrue(e: Env, n: u128) {
        e.storage().instance().set(&K::Pending, &n);
    }
    pub fn fail(e: Env, claim: bool, view: bool) {
        e.storage().instance().set(&K::Fail, &claim);
        e.storage().instance().set(&K::ViewFail, &view);
    }
    pub fn get_user_reward(e: Env, _user: Address) -> u128 {
        assert!(
            !e.storage().instance().get(&K::ViewFail).unwrap_or(false),
            "view outage"
        );
        e.storage().instance().get(&K::Pending).unwrap_or(0)
    }
    pub fn claim(e: Env, user: Address) -> u128 {
        assert!(
            !e.storage().instance().get(&K::Fail).unwrap_or(false),
            "claim outage"
        );
        let n: u128 = e.storage().instance().get(&K::Pending).unwrap_or(0);
        e.storage().instance().set(&K::Pending, &0u128);
        let t: Address = e.storage().instance().get(&K::Token).unwrap();
        if n > 0 {
            token::Client::new(&e, &t).transfer(
                &e.current_contract_address(),
                &user,
                &i128::try_from(n).unwrap(),
            );
        }
        n
    }
}
#[contract]
struct Ledger;
#[contractimpl]
impl Ledger {
    pub fn init(e: Env, pool: Address, t: Address, admin: Address) {
        storage::set_config(
            &e,
            &storage::Config {
                pool,
                token0: t.clone(),
                token1: t.clone(),
                dec0: 7,
                dec1: 7,
                underlying_index: 0,
                tick_lower: -40,
                tick_upper: 40,
                oracle: t.clone(),
            },
        );
        storage::set_primary_reward(&e, &Some(t));
        e.storage().instance().set(&storage::DataKey::Admin, &admin);
    }
    pub fn mint(e: Env, a: Address, n: u128) {
        rewards::normal(&e, &a);
        let mut s = storage::state(&e);
        s.total_shares += n;
        storage::set_state(&e, &s);
        storage::set_shares(&e, &a, storage::shares_of(&e, &a) + n);
        rewards::reset_debt(&e, &a);
    }
    pub fn move_shares(e: Env, a: Address, b: Address, n: u128) {
        a.require_auth();
        rewards::normal(&e, &a);
        rewards::settle(&e, &b);
        storage::set_shares(&e, &a, storage::shares_of(&e, &a) - n);
        storage::set_shares(&e, &b, storage::shares_of(&e, &b) + n);
        rewards::reset_debt(&e, &a);
        rewards::reset_debt(&e, &b);
    }
    pub fn exit(e: Env, a: Address, forced: bool, waiver: bool) {
        a.require_auth();
        rewards::recovery(&e, &a, forced, waiver);
        let mut s = storage::state(&e);
        s.total_shares -= storage::shares_of(&e, &a);
        storage::set_state(&e, &s);
        storage::set_shares(&e, &a, 0);
        rewards::reset_debt(&e, &a);
    }
    pub fn checkpoint(e: Env) -> bool {
        rewards::checkpoint(&e)
    }
    pub fn pending(e: Env, a: Address) -> u128 {
        rewards::settle(&e, &a);
        rewards::holder(&e, &a).pending
    }
    pub fn totals(e: Env) -> rewards::Totals {
        rewards::totals(&e)
    }
    pub fn boundary(e: Env, id: u64) -> Option<rewards::Boundary> {
        rewards::boundary(&e, id)
    }
}
struct F {
    e: Env,
    p: Address,
    l: Address,
    a: Address,
    b: Address,
}
impl F {
    fn new() -> Self {
        let e = Env::default();
        let admin = Address::generate(&e);
        let a = Address::generate(&e);
        let b = Address::generate(&e);
        let t = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let p = e.register(Pool, ());
        let l = e.register(Ledger, ());
        e.mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &t,
                fn_name: "mint",
                args: (p.clone(), 1_000_000i128).into_val(&e),
                sub_invokes: &[],
            },
        }]);
        token::StellarAssetClient::new(&e, &t).mint(&p, &1_000_000);
        e.mock_auths(&[]);
        PoolClient::new(&e, &p).initialize(&t);
        LedgerClient::new(&e, &l).init(&p, &t, &admin);
        Self { e, p, l, a, b }
    }
    fn c(&self) -> LedgerClient<'_> {
        LedgerClient::new(&self.e, &self.l)
    }
    fn earn(&self, n: u128) {
        PoolClient::new(&self.e, &self.p).accrue(&n);
    }
    fn exit(&self, forced: bool, waiver: bool) -> bool {
        self.e.mock_auths(&[MockAuth {
            address: &self.a,
            invoke: &MockAuthInvoke {
                contract: &self.l,
                fn_name: "exit",
                args: (self.a.clone(), forced, waiver).into_val(&self.e),
                sub_invokes: &[],
            },
        }]);
        self.c().try_exit(&self.a, &forced, &waiver).is_ok()
    }
}
#[test]
fn alice_bob_unclaimed_and_emission_change_conserve() {
    let f = F::new();
    f.c().mint(&f.a, &1000);
    f.earn(10);
    f.c().mint(&f.b, &1000);
    assert_eq!(f.c().pending(&f.a), 10);
    assert_eq!(f.c().pending(&f.b), 0);
    let f = F::new();
    f.c().mint(&f.a, &1000);
    f.c().mint(&f.b, &1000);
    f.earn(10);
    assert!(f.exit(false, false));
    f.earn(90);
    assert!(f.c().checkpoint());
    assert_eq!(f.c().pending(&f.a), 5);
    assert_eq!(f.c().pending(&f.b), 95);
    assert_eq!(f.c().totals().received, 100);
    std::println!(
        "REWARD_CONSERVATION: upstream=100 Alice=5 Bob=95; Alice-only pre-deposit=10 Bob=0"
    );
}
#[test]
fn outage_waiver_and_missing_boundary_stay_quarantined() {
    let f = F::new();
    f.c().mint(&f.a, &1000);
    f.c().mint(&f.b, &1000);
    f.earn(10);
    PoolClient::new(&f.e, &f.p).fail(&true, &true);
    assert!(f.c().try_mint(&f.b, &1000).is_err());
    assert!(!f.exit(false, false));
    assert!(f.exit(true, false));
    let b = f.c().boundary(&0).unwrap();
    assert!(!b.proven);
    assert!(!b.waived);
    assert_eq!(b.reading, None);
    assert_eq!(b.holder, f.a);
    f.earn(100);
    PoolClient::new(&f.e, &f.p).fail(&false, &false);
    assert!(f.c().checkpoint());
    assert_eq!(f.c().totals().quarantine, 100);
    assert_eq!(f.c().pending(&f.b), 0);
    assert!(!f.c().boundary(&0).unwrap().proven);
    let f = F::new();
    f.c().mint(&f.a, &1000);
    f.earn(10);
    PoolClient::new(&f.e, &f.p).fail(&true, &false);
    assert!(!f.exit(false, false));
    assert!(f.exit(false, true));
    assert!(f.c().boundary(&0).unwrap().waived);
    assert!(!f.c().boundary(&0).unwrap().proven);
    std::println!("REWARD_CONSERVATION: unresolved received=100 quarantine=100 distributed=0; waiver false rejects, true records waiver; late reading remains unproven");
}
#[test]
fn transfers_checkpoint_at_old_balances_zero_claims_do_not_duplicate() {
    let f = F::new();
    f.c().mint(&f.a, &1000);
    f.earn(10);
    f.e.mock_auths(&[MockAuth {
        address: &f.a,
        invoke: &MockAuthInvoke {
            contract: &f.l,
            fn_name: "move_shares",
            args: (f.a.clone(), f.b.clone(), 500u128).into_val(&f.e),
            sub_invokes: &[],
        },
    }]);
    f.c().move_shares(&f.a, &f.b, &500);
    assert!(f.c().checkpoint());
    assert!(f.c().checkpoint());
    f.earn(90);
    assert!(f.c().checkpoint());
    assert_eq!(f.c().pending(&f.a), 55);
    assert_eq!(f.c().pending(&f.b), 45);
    assert_eq!(f.c().totals().received, 100);
}
