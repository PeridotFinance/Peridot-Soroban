//! Native fee-paying pool fixture exercising the actual vault emergency entrypoint.
//! It deliberately has no reward methods, proving recovery under both failures.
//! This is a controlled ABI fixture, not pinned Aquarius venue evidence.
use crate::{accounting, storage, ClShareVault, ClShareVaultClient};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, IntoVal, Vec};
#[contracttype]
#[derive(Clone)]
enum Key {
    T0,
    T1,
    Fees0,
    Fees1,
}
#[contract]
struct FeePool;
#[contractimpl]
impl FeePool {
    pub fn init(e: Env, t0: Address, t1: Address) {
        e.storage().instance().set(&Key::T0, &t0);
        e.storage().instance().set(&Key::T1, &t1);
        e.storage().instance().set(&Key::Fees0, &100u128);
        e.storage().instance().set(&Key::Fees1, &200u128);
    }
    pub fn send(e: Env, who: Address, a: u128, b: u128) {
        for (key, n) in [(Key::T0, a), (Key::T1, b)] {
            let t: Address = e.storage().instance().get(&key).unwrap();
            token::Client::new(&e, &t).transfer(
                &e.current_contract_address(),
                &who,
                &i128::try_from(n).unwrap(),
            );
        }
    }
    pub fn claim_all_position_fees(e: Env, user: Address) -> Vec<u128> {
        user.require_auth();
        let a = e.storage().instance().get(&Key::Fees0).unwrap_or(0);
        let b = e.storage().instance().get(&Key::Fees1).unwrap_or(0);
        e.storage().instance().set(&Key::Fees0, &0u128);
        e.storage().instance().set(&Key::Fees1, &0u128);
        Self::send(e.clone(), user, a, b);
        soroban_sdk::vec![&e, a, b]
    }
    pub fn withdraw_position(
        e: Env,
        user: Address,
        _lo: i32,
        _hi: i32,
        burn: u128,
        _mins: Vec<u128>,
    ) -> Vec<u128> {
        user.require_auth();
        // Aquarius semantics: partial principal withdrawal also pays ALL fees.
        let fees = Self::claim_all_position_fees(e.clone(), user.clone());
        Self::send(e.clone(), user, burn, burn * 2);
        soroban_sdk::vec![
            &e,
            burn + fees.get(0).unwrap(),
            burn * 2 + fees.get(1).unwrap()
        ]
    }
}
#[test]
fn actual_emergency_entrypoint_excludes_donations_and_splits_all_fees() {
    let e = Env::default();
    let admin = Address::generate(&e);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    let t0 = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let t1 = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let reward = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let pool = e.register(FeePool, ());
    let vault = e.register(ClShareVault, ());
    FeePoolClient::new(&e, &pool).init(&t0, &t1);
    for t in [&t0, &t1] {
        e.mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: t,
                fn_name: "mint",
                args: (pool.clone(), 10_000i128).into_val(&e),
                sub_invokes: &[],
            },
        }]);
        token::StellarAssetClient::new(&e, t).mint(&pool, &10_000);
    }
    e.mock_auths(&[]);
    // Explicit synthetic fixture state; no production admin identity or oracle is changed.
    e.as_contract(&vault, || {
        storage::set_config(
            &e,
            &storage::Config {
                pool: pool.clone(),
                token0: t0.clone(),
                token1: t1.clone(),
                dec0: 7,
                dec1: 7,
                underlying_index: 0,
                tick_lower: -40,
                tick_upper: 40,
                oracle: reward.clone(),
            },
        );
        storage::set_primary_reward(&e, &Some(reward.clone()));
        let mut state = storage::state(&e);
        state.total_shares = 2000;
        state.position_liquidity = 1000;
        storage::set_state(&e, &state);
        storage::set_shares(&e, &a, 1000);
        storage::set_shares(&e, &b, 1000);
        accounting::begin(&e);
        FeePoolClient::new(&e, &pool).send(&vault, &300, &400);
        accounting::end(&e);
    });
    FeePoolClient::new(&e, &pool).send(&vault, &50, &60); // unrelated donation excluded
    let client = ClShareVaultClient::new(&e, &vault);
    for (holder, expected0, expected1) in [(&a, 700u128, 1300u128), (&b, 700, 1300)] {
        e.mock_auths(&[MockAuth {
            address: holder,
            invoke: &MockAuthInvoke {
                contract: &vault,
                fn_name: "emergency_redeem",
                args: (holder.clone(), 1000u128, true).into_val(&e),
                sub_invokes: &[],
            },
        }]);
        let out = client.emergency_redeem(holder, &1000, &true);
        assert_eq!(out, soroban_sdk::vec![&e, expected0, expected1]);
        assert_eq!(
            token::Client::new(&e, &t0).balance(holder),
            expected0 as i128
        );
        assert_eq!(
            token::Client::new(&e, &t1).balance(holder),
            expected1 as i128
        );
    }
    assert_eq!(client.total_supply(), 0);
    assert_eq!(token::Client::new(&e, &t0).balance(&vault), 50);
    assert_eq!(token::Client::new(&e, &t1).balance(&vault), 60);
    std::println!("PRINCIPAL_CONSERVATION: both reward calls unavailable; A=700/1300 B=700/1300 fees=100/200 split equally; excluded donations=50/60; total_supply=0");
}
