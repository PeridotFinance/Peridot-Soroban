//! Unit tests for the reward ledger, with a controlled upstream token/pool.
//! Principal/venue integration is measured separately by the compiled harness.
use super::*;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
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
        custody::received(&e, &a, n);
        rewards::reset_debt(&e, &a);
    }
    pub fn move_shares(e: Env, a: Address, b: Address, n: u128) {
        a.require_auth();
        let owner = custody::outgoing(&e, &a, &b, n).unwrap_or_else(|| {
            rewards::normal(&e, &a);
            rewards::settle(&e, &b);
            a.clone()
        });
        storage::set_shares(&e, &a, storage::shares_of(&e, &a) - n);
        storage::set_shares(&e, &b, storage::shares_of(&e, &b) + n);
        custody::received(&e, &b, n);
        rewards::reset_debt(&e, &owner);
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
    pub fn register(e: Env, c: Address) {
        custody::register(&e, &c);
    }
    pub fn assign(e: Env, c: Address, owner: Address, n: u128) {
        custody::assign(&e, &c, &owner, n, false);
    }
    pub fn reassign(e: Env, c: Address, a: Address, b: Address, n: u128, recovery: bool) {
        custody::reassign(&e, &c, &a, &b, n, recovery);
    }
    pub fn prepare(e: Env, c: Address, owner: Address, to: Address, n: u128, recovery: bool) {
        custody::prepare(&e, &c, &owner, &to, n, recovery);
    }
    pub fn allocation(e: Env, c: Address, a: Address) -> u128 {
        custody::allocation(&e, &c, &a)
    }
    pub fn propose(e: Env, id: u64, n: u128, evidence: soroban_sdk::BytesN<32>) {
        rewards::propose_attribute(&e, id, n, evidence);
    }
    pub fn attribute(e: Env, id: u64, n: u128, evidence: soroban_sdk::BytesN<32>) {
        rewards::attribute(&e, id, n, evidence);
    }
    pub fn checkpoint(e: Env) -> bool {
        rewards::checkpoint(&e)
    }
    pub fn claim(e: Env, a: Address) -> u128 {
        rewards::claim(&e, &a)
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
    admin: Address,
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
        Self {
            e,
            p,
            l,
            a,
            b,
            admin,
        }
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

// Native controlled custodian fixture: no mock authorization for its own calls.
// A production receipt/controller must additionally validate its lifecycle.
#[contract]
struct Custodian;
#[contractimpl]
impl Custodian {
    pub fn assign(e: Env, ledger: Address, owner: Address, n: u128) {
        LedgerClient::new(&e, &ledger).assign(&e.current_contract_address(), &owner, &n);
    }
    pub fn reassign(e: Env, ledger: Address, a: Address, b: Address, n: u128, recovery: bool) {
        LedgerClient::new(&e, &ledger).reassign(
            &e.current_contract_address(),
            &a,
            &b,
            &n,
            &recovery,
        );
    }
    pub fn release(e: Env, ledger: Address, owner: Address, to: Address, n: u128, recovery: bool) {
        let c = LedgerClient::new(&e, &ledger);
        c.prepare(&e.current_contract_address(), &owner, &to, &n, &recovery);
        c.move_shares(&e.current_contract_address(), &to, &n);
    }
    pub fn bad_release(e: Env, ledger: Address, owner: Address, to: Address, n: u128, kind: u32) {
        let c = LedgerClient::new(&e, &ledger);
        if kind != 0 {
            c.prepare(&e.current_contract_address(), &owner, &to, &n, &true);
        }
        let amount = if kind == 1 { n - 100 } else { n };
        let recipient = if kind == 2 { owner } else { to };
        c.move_shares(&e.current_contract_address(), &recipient, &amount);
    }
}
fn auth(f: &F, who: &Address, name: &str, args: soroban_sdk::Vec<soroban_sdk::Val>) {
    f.e.mock_auths(&[MockAuth {
        address: who,
        invoke: &MockAuthInvoke {
            contract: &f.l,
            fn_name: name,
            args,
            sub_invokes: &[],
        },
    }]);
}
fn custody_fixture() -> (F, Address) {
    let f = F::new();
    let c = f.e.register(Custodian, ());
    auth(&f, &f.admin, "register", (c.clone(),).into_val(&f.e));
    f.c().register(&c);
    f.e.mock_auths(&[]);
    f.c().mint(&f.a, &2000);
    auth(
        &f,
        &f.a,
        "move_shares",
        (f.a.clone(), c.clone(), 1000u128).into_val(&f.e),
    );
    f.c().move_shares(&f.a, &c, &1000);
    f.e.mock_auths(&[]);
    CustodianClient::new(&f.e, &c).assign(&f.l, &f.a, &1000);
    (f, c)
}
#[test]
fn custody_rewards_follow_beneficial_owner_and_forced_seizure_preserves_history() {
    let (f, c) = custody_fixture();
    let cc = CustodianClient::new(&f.e, &c);
    f.earn(100);
    assert!(f.c().checkpoint());
    assert_eq!(f.c().pending(&f.a), 100);
    assert_eq!(f.c().pending(&c), 0);
    f.earn(50);
    PoolClient::new(&f.e, &f.p).fail(&true, &true);
    cc.reassign(&f.l, &f.a, &f.b, &500, &true);
    assert_eq!(f.c().allocation(&c, &f.a), 500);
    assert_eq!(f.c().allocation(&c, &f.b), 500);
    assert_eq!(f.c().pending(&f.a), 100);
    assert!(!f.c().boundary(&0).unwrap().waived);
    assert_eq!(f.c().boundary(&1).unwrap().shares, 0);
    f.e.as_contract(&f.l, || {
        let segment=crate::reward_history::segment(&f.e,0).unwrap();
        let count=crate::reward_history::count(&f.e,&f.a);
        let mut historical=0;
        for n in 0..count {
            let w=crate::reward_history::weight(&f.e,&f.a,n).unwrap();
            if w.revision<=segment.revision {historical=w.weight;}
        }
        assert_eq!(historical,2000); // direct1000 + custody1000 before seizure
        assert_eq!(crate::reward_history::count(&f.e,&c),0); // custodian never beneficiary
        assert_eq!(crate::reward_history::weight(&f.e,&f.b,0).unwrap().weight,500);
    });
    let evidence = soroban_sdk::BytesN::from_array(&f.e, &[8; 32]);
    auth(
        &f,
        &f.admin,
        "propose",
        (1u64, 50u128, evidence.clone()).into_val(&f.e),
    );
    assert!(f.c().try_propose(&1, &50, &evidence).is_err());
    f.e.mock_auths(&[]);
    cc.release(&f.l, &f.a, &f.a, &500, &true);
    PoolClient::new(&f.e, &f.p).fail(&false, &false);
    assert!(f.c().checkpoint());
    assert_eq!(f.c().totals().quarantine, 50);
    f.earn(100);
    assert!(f.c().checkpoint());
    assert_eq!(f.c().pending(&f.a), 175);
    assert_eq!(f.c().pending(&f.b), 25);
    assert_eq!(f.c().totals().received, 250);
    std::println!("CUSTODY_CONSERVATION: received=250 original_owner=175 new_owner=25 quarantine=50; forced failed-claim transfer preserves historical rewards; custodian=0");
}
#[test]
fn custody_exact_context_and_actual_invoker_authorization_are_required() {
    let (f, c) = custody_fixture();
    let cc = CustodianClient::new(&f.e, &c);
    assert!(f.c().try_prepare(&c, &f.a, &f.b, &500, &true).is_err());
    assert!(f.c().try_assign(&c, &f.a, &100).is_err());
    assert!(f.c().try_reassign(&c, &f.a, &f.b, &500, &true).is_err());
    for kind in [0, 1, 2] {
        assert!(cc.try_bad_release(&f.l, &f.a, &f.b, &500, &kind).is_err());
    }
    assert_eq!(f.c().allocation(&c, &f.a), 1000);
    cc.release(&f.l, &f.a, &f.b, &500, &false);
    assert_eq!(f.c().allocation(&c, &f.a), 500);
    std::println!("CUSTODY_AUTH: unauthorized prepare/assign/reassign rejected; missing context, wrong amount, wrong recipient rejected AFTER setup; real contract invoker positive succeeds");
}
#[test]
fn unassigned_donation_rewards_are_quarantined_without_blocking_other_holders() {
    let (f, c) = custody_fixture();
    auth(
        &f,
        &f.a,
        "move_shares",
        (f.a.clone(), c.clone(), 100u128).into_val(&f.e),
    );
    f.c().move_shares(&f.a, &c, &100);
    f.e.mock_auths(&[]);
    f.earn(200);
    assert!(f.c().checkpoint());
    assert_eq!(f.c().pending(&f.a), 190);
    assert_eq!(f.c().pending(&c), 0);
    assert_eq!(f.c().totals().quarantine, 10);
    assert_eq!(f.c().totals().received, 200);
}
#[test]
fn quarantine_attribution_requires_delayed_exact_governance_evidence() {
    let f = F::new();
    f.c().mint(&f.a, &1000);
    f.earn(100);
    PoolClient::new(&f.e, &f.p).fail(&true, &true);
    assert!(f.exit(true, false));
    PoolClient::new(&f.e, &f.p).fail(&false, &false);
    assert!(f.c().checkpoint());
    let evidence = soroban_sdk::BytesN::from_array(&f.e, &[7; 32]);
    let args = (0u64, 100u128, evidence.clone()).into_val(&f.e);
    auth(&f, &f.admin, "attribute", args);
    assert!(f.c().try_attribute(&0, &100, &evidence).is_err());
    auth(
        &f,
        &f.admin,
        "propose",
        (0u64, 100u128, evidence.clone()).into_val(&f.e),
    );
    f.c().propose(&0, &100, &evidence);
    auth(
        &f,
        &f.admin,
        "attribute",
        (0u64, 100u128, evidence.clone()).into_val(&f.e),
    );
    assert!(f.c().try_attribute(&0, &100, &evidence).is_err());
    f.e.ledger().with_mut(|l| l.timestamp += 86_400);
    auth(
        &f,
        &f.admin,
        "attribute",
        (0u64, 99u128, evidence.clone()).into_val(&f.e),
    );
    assert!(f.c().try_attribute(&0, &99, &evidence).is_err());
    auth(
        &f,
        &f.admin,
        "attribute",
        (0u64, 100u128, evidence.clone()).into_val(&f.e),
    );
    f.c().attribute(&0, &100, &evidence);
    assert_eq!(f.c().pending(&f.a), 100);
    assert_eq!(f.c().totals().quarantine, 0);
    auth(
        &f,
        &f.admin,
        "attribute",
        (0u64, 100u128, evidence.clone()).into_val(&f.e),
    );
    assert!(f.c().try_attribute(&0, &100, &evidence).is_err());
}

#[test]
fn personal_claims_pay_only_owner_and_booked_rewards_survive_upstream_failure() {
    let f = F::new();
    f.c().mint(&f.a, &1000);
    f.earn(10);
    f.c().mint(&f.b, &1000);
    f.earn(90);
    auth(&f, &f.a, "claim", (f.a.clone(),).into_val(&f.e));
    assert_eq!(f.c().claim(&f.a), 55);
    PoolClient::new(&f.e, &f.p).fail(&true, &true);
    auth(&f, &f.b, "claim", (f.b.clone(),).into_val(&f.e));
    assert_eq!(f.c().claim(&f.b), 45);
    auth(&f, &f.a, "claim", (f.a.clone(),).into_val(&f.e));
    assert_eq!(f.c().claim(&f.a), 0);
    assert_eq!(f.c().totals().received, 100);
    assert_eq!(f.c().totals().paid, 100);
    assert_eq!(f.c().totals().quarantine, 0);
    std::println!("PERSONAL_CLAIMS: received=100 paid=100 Alice55 Bob45; existing booked rewards payable during upstream claim/view failure; repeat pays0");
}

#[test]
fn custody_registration_requires_deployed_wasm_not_account_or_asset() {
    let f = F::new();
    // Public G-address parsing only: never register a mock auth contract there.
    let account = Address::from_str(&f.e, crate::contract::DEFAULT_INIT_ADMIN);
    let undeployed = Address::generate(&f.e);
    let asset =
        f.e.register_stellar_asset_contract_v2(f.admin.clone())
            .address();
    for address in [&account, &undeployed, &asset] {
        auth(&f, &f.admin, "register", (address.clone(),).into_val(&f.e));
        assert!(f.c().try_register(address).is_err());
        f.e.mock_auths(&[]);
    }
    let contract = f.e.register(Custodian, ());
    auth(&f, &f.admin, "register", (contract.clone(),).into_val(&f.e));
    f.c().register(&contract);
    std::println!("CUSTODY_REGISTRATION: G-account, undeployed C-address and SAC rejected; deployed native fixture accepted; G-address never mocked");
}
