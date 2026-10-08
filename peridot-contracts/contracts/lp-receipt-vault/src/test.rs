extern crate std;
use crate::{DataKey, ReceiptVault};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{contract, contractimpl, token, Address, Env, IntoVal};

#[contract]
struct Harness;
#[contractimpl]
impl Harness {
    pub fn init(env: Env, asset: Address, admin: Address) {
        ReceiptVault::initialize(env, asset, 0, 0, admin);
    }
    pub fn deposit(env: Env, user: Address, amount: u128) {
        ReceiptVault::deposit(env, user, amount);
    }
    pub fn withdraw(env: Env, user: Address, amount: u128) {
        ReceiptVault::withdraw(env, user, amount);
    }
    pub fn balance(env: Env, user: Address) -> i128 {
        ReceiptVault::balance(env, user)
    }
    pub fn nav(env: Env) -> u128 {
        ReceiptVault::get_total_underlying(env)
    }
    pub fn cap(env: Env, admin: Address, amount: u128) {
        ReceiptVault::set_supply_cap(env, admin, amount);
    }
    pub fn pause(env: Env, admin: Address, value: bool) {
        ReceiptVault::set_deposit_paused(env, admin, value);
    }
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        ReceiptVault::transfer(env, from, to.into(), amount);
    }
}

struct Fixture {
    env: Env,
    receipt: Address,
    asset: Address,
    admin: Address,
    user: Address,
}
impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let asset = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        token::StellarAssetClient::new(&env, &asset).mint(&user, &1_000_000);
        let receipt = env.register(Harness, ());
        HarnessClient::new(&env, &receipt).init(&asset, &admin);
        Self {
            env,
            receipt,
            asset,
            admin,
            user,
        }
    }
    fn r(&self) -> HarnessClient<'_> {
        HarnessClient::new(&self.env, &self.receipt)
    }
}

#[test]
fn lending_state_is_not_created_or_required_by_supply_paths() {
    let f = Fixture::new();
    f.r().deposit(&f.user, &100_000);
    f.r().withdraw(&f.user, &100_000);
    assert_eq!(f.r().nav(), 0);
    f.env.as_contract(&f.receipt, || {
        for key in [
            DataKey::BorrowIndex,
            DataKey::TotalBorrowed,
            DataKey::InterestModel,
            DataKey::Peridottroller,
            DataKey::MarginController,
            DataKey::TotalReserves,
            DataKey::HasBorrowed(f.user.clone()),
            DataKey::LastUpdateTime,
        ] {
            assert!(!f.env.storage().persistent().has(&key));
        }
    });
}

#[test]
fn donations_are_not_minted_priced_or_paid_to_final_holder() {
    let f = Fixture::new();
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&f.receipt, &50_000);
    f.r().deposit(&f.user, &100_000);
    assert_eq!(f.r().nav(), 100_000);
    f.r().withdraw(&f.user, &100_000);
    assert_eq!(f.r().nav(), 0);
    assert_eq!(
        token::Client::new(&f.env, &f.asset).balance(&f.receipt),
        50_000
    );
}

#[test]
fn pause_blocks_deposit_but_keeps_principal_exit_available() {
    let f = Fixture::new();
    f.r().deposit(&f.user, &100_000);
    f.r().pause(&f.admin, &true);
    assert!(f.r().try_deposit(&f.user, &1).is_err());
    f.r().withdraw(&f.user, &100_000);
    assert_eq!(f.r().balance(&f.user), 0);
}

#[test]
fn cap_failure_preserves_user_cash_and_supply() {
    let f = Fixture::new();
    f.r().cap(&f.admin, &100);
    assert!(f.r().try_deposit(&f.user, &101).is_err());
    assert_eq!(f.r().balance(&f.user), 0);
    assert_eq!(
        token::Client::new(&f.env, &f.asset).balance(&f.user),
        1_000_000
    );
}

#[test]
fn exact_owner_authorization_is_required_for_deposit_and_withdraw() {
    let f = Fixture::new();
    f.env.mock_auths(&[]);
    assert!(f.r().try_deposit(&f.user, &100).is_err());
    f.r()
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "deposit",
                args: (f.user.clone(), 100u128).into_val(&f.env),
                sub_invokes: &[MockAuthInvoke {
                    contract: &f.asset,
                    fn_name: "transfer",
                    args: (f.user.clone(), f.receipt.clone(), 100i128).into_val(&f.env),
                    sub_invokes: &[],
                }],
            },
        }])
        .deposit(&f.user, &100);
    f.env.mock_auths(&[]);
    assert!(f.r().try_withdraw(&f.user, &100).is_err());
    assert_eq!(f.r().balance(&f.user), 100);
    f.r()
        .mock_auths(&[MockAuth {
            address: &f.user,
            invoke: &MockAuthInvoke {
                contract: &f.receipt,
                fn_name: "withdraw",
                args: (f.user.clone(), 100u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .withdraw(&f.user, &100);
}

#[test]
fn direct_escrow_transfers_are_rejected() {
    let f = Fixture::new();
    f.r().deposit(&f.user, &100);
    assert!(f.r().try_transfer(&f.user, &f.receipt, &1).is_err());
    assert_eq!(f.r().balance(&f.user), 100);
}

#[test]
fn legacy_initialized_state_is_not_silently_activated_or_reinitialized() {
    let f = Fixture::new();
    assert!(f.r().try_init(&f.asset, &f.admin).is_err());
    f.env.as_contract(&f.receipt, || {
        f.env
            .storage()
            .instance()
            .remove(&crate::storage::LpKey::LpReceiptVersion);
    });
    assert!(f.r().try_deposit(&f.user, &100).is_err());
    assert!(f.r().try_init(&f.asset, &f.admin).is_err());
}

#[test]
fn administrative_changes_require_the_configured_admin() {
    let f = Fixture::new();
    assert!(f.r().try_pause(&f.user, &true).is_err());
    assert!(f.r().try_cap(&f.user, &1).is_err());
    f.env.mock_auths(&[]);
    assert!(f.r().try_cap(&f.admin, &1).is_err());
    assert!(f.r().try_pause(&f.admin, &true).is_err());
}

#[test]
fn minting_uses_exact_nav_ratio_not_a_truncated_exchange_rate() {
    let f = Fixture::new();
    f.r().deposit(&f.user, &999_999);
    // Model already-recognized managed yield (not an untracked donation).
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&f.receipt, &2);
    f.env.as_contract(&f.receipt, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::ManagedCash, &1_000_001u128);
    });
    let late = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&late, &1_000_001);
    f.r().deposit(&late, &1_000_001);
    assert_eq!(f.r().balance(&late), 999_999);
}
