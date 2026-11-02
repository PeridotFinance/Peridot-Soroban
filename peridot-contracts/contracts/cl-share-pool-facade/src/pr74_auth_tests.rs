//! Reviewed PR74 authorization fixtures, adapted to the integrated ABI.
//! Synthetic native controller/vault contracts; NOT a real V3 or venue proof.
use super::*;
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    token, vec, Address, Bytes, Env, IntoVal, Symbol, Vec,
};

#[contract]
struct VaultFixture;
#[contracttype]
enum VaultKey {
    Asset,
    Balance(Address),
    Bonus,
    ClaimFails,
    NavValid,
}
#[contractimpl]
impl VaultFixture {
    pub fn __constructor(env: Env, asset: Address) {
        env.storage().instance().set(&VaultKey::Asset, &asset);
        env.storage().instance().set(&VaultKey::NavValid, &true);
    }
    pub fn setup(env: Env, bonus: i128, claim_fails: bool, valid: bool) {
        env.storage().instance().set(&VaultKey::Bonus, &bonus);
        env.storage()
            .instance()
            .set(&VaultKey::ClaimFails, &claim_fails);
        env.storage().instance().set(&VaultKey::NavValid, &valid);
    }
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&VaultKey::Balance(id))
            .unwrap_or(0)
    }
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        assert!(amount >= 0);
        assert!(
            !env.storage()
                .instance()
                .get::<_, bool>(&VaultKey::ClaimFails)
                .unwrap_or(false),
            "claim failed"
        );
        let old = Self::balance(env.clone(), from.clone());
        assert!(old >= amount);
        let next = Self::balance(env.clone(), to.clone())
            .checked_add(amount)
            .unwrap();
        env.storage()
            .persistent()
            .set(&VaultKey::Balance(from), &(old - amount));
        env.storage()
            .persistent()
            .set(&VaultKey::Balance(to), &next);
    }
    pub fn get_underlying(env: Env) -> Address {env.storage().instance().get(&VaultKey::Asset).unwrap()}
    pub fn quote(env: Env, deposit: bool, amount: u128) -> u128 {
        if deposit {Self::estimate_deposit(env,amount)} else {Self::estimate_redeem(env,amount)}
    }
    pub fn estimate_deposit(env: Env, amount: u128) -> u128 {
        if env
            .storage()
            .instance()
            .get::<_, bool>(&VaultKey::NavValid)
            .unwrap_or(false)
        {
            amount
        } else {
            0
        }
    }
    pub fn estimate_redeem(env: Env, amount: u128) -> u128 {
        Self::estimate_deposit(env, amount)
    }
    pub fn deposit_underlying(env: Env, from: Address, amount: i128, min_shares: i128) -> i128 {
        from.require_auth();
        assert!(
            !env.storage()
                .instance()
                .get::<_, bool>(&VaultKey::ClaimFails)
                .unwrap_or(false),
            "claim failed"
        );
        let asset: Address = env.storage().instance().get(&VaultKey::Asset).unwrap();
        token::TokenClient::new(&env, &asset).transfer(
            &from,
            &env.current_contract_address(),
            &amount,
        );
        let produced = amount
            + env
                .storage()
                .instance()
                .get::<_, i128>(&VaultKey::Bonus)
                .unwrap_or(0);
        // Deliberately omit the vault's own minimum guard: the facade must
        // independently reject under-delivery, even with donated output dust.
        let _ = min_shares;
        let value = Self::balance(env.clone(), from.clone()) + produced;
        env.storage()
            .persistent()
            .set(&VaultKey::Balance(from), &value);
        // Lie about the return value to prove facade settlement uses balances.
        1
    }
    pub fn redeem(env: Env, from: Address, shares: i128, min_underlying_out: i128) -> i128 {
        from.require_auth();
        assert!(
            !env.storage()
                .instance()
                .get::<_, bool>(&VaultKey::ClaimFails)
                .unwrap_or(false),
            "claim failed"
        );
        let old = Self::balance(env.clone(), from.clone());
        assert!(old >= shares);
        env.storage()
            .persistent()
            .set(&VaultKey::Balance(from.clone()), &(old - shares));
        let produced = shares
            + env
                .storage()
                .instance()
                .get::<_, i128>(&VaultKey::Bonus)
                .unwrap_or(0);
        let _ = min_underlying_out;
        let asset: Address = env.storage().instance().get(&VaultKey::Asset).unwrap();
        token::TokenClient::new(&env, &asset).transfer(
            &env.current_contract_address(),
            &from,
            &produced,
        );
        1
    }
}

/// Copies V3's exact authorize_controller_pool_swap invocation shape from
/// perps.rs at 47858c1. It does NOT emulate V3 positions, risk or user auth.
#[contract]
struct ControllerAuthFixture;
#[contractimpl]
impl ControllerAuthFixture {
    pub fn trade(
        env: Env,
        facade: Address,
        input: Address,
        reverse: bool,
        amount: u128,
        minimum: u128,
        auth_mode: u32,
    ) -> u128 {
        let me = env.current_contract_address();
        let (in_idx, out_idx) = if reverse { (1u32, 0u32) } else { (0u32, 1u32) };
        let approved = if auth_mode == 2 { amount + 1 } else { amount };
        let recipient = if auth_mode == 3 { input.clone() } else { facade.clone() };
        let transfer = SubContractInvocation {
            context: ContractContext {
                contract: input,
                fn_name: Symbol::new(&env, "transfer"),
                args: (me.clone(), recipient, approved as i128).into_val(&env),
            },
            sub_invocations: Vec::new(&env),
        };
        if auth_mode != 1 {
            env.authorize_as_current_contract(vec![
                &env,
                InvokerContractAuthEntry::Contract(transfer.clone()),
                InvokerContractAuthEntry::Contract(SubContractInvocation {
                    context: ContractContext {
                        contract: facade.clone(),
                        fn_name: Symbol::new(&env, "swap"),
                        args: (me.clone(), in_idx, out_idx, amount, minimum).into_val(&env),
                    },
                    sub_invocations: vec![&env, InvokerContractAuthEntry::Contract(transfer)],
                }),
            ]);
        }
        ClSharePoolFacadeClient::new(&env, &facade).swap(&me, &in_idx, &out_idx, &amount, &minimum)
    }
}

struct Fixture {
    env: Env,
    admin: Address,
    treasury: Address,
    asset: Address,
    vault: Address,
    controller: Address,
    facade: Address,
}
impl Fixture {
    fn new() -> Self {
        Self::with_wasm(None)
    }
    fn with_wasm(wasm: Option<&[u8]>) -> Self {
        let env = Env::default();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let asset = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let vault = env.register(VaultFixture, (asset.clone(),));
        let controller = env.register(ControllerAuthFixture, ());
        let facade = match wasm {
            Some(code) => env.register(code, ()),
            None => env.register(ClSharePoolFacade, ()),
        };
        ClSharePoolFacadeClient::new(&env,&facade).mock_auths(&[MockAuth{address:&admin,invoke:&MockAuthInvoke{
            contract:&facade,fn_name:"initialize",args:(admin.clone(),controller.clone(),asset.clone(),vault.clone(),treasury.clone()).into_val(&env),sub_invokes:&[]
        }}]).initialize(&admin,&controller,&asset,&vault,&treasury);
        let f = Self {
            env,
            admin,
            treasury,
            asset,
            vault,
            controller,
            facade,
        };
        f.mint(&f.controller, 10_000);
        f.mint(&f.vault, 10_000);
        f
    }
    fn mint(&self, to: &Address, amount: i128) {
        token::StellarAssetClient::new(&self.env, &self.asset)
            .mock_auths(&[MockAuth {
                address: &self.admin,
                invoke: &MockAuthInvoke {
                    contract: &self.asset,
                    fn_name: "mint",
                    args: (to.clone(), amount).into_val(&self.env),
                    sub_invokes: &[],
                },
            }])
            .mint(to, &amount);
    }
    fn pauses(&self, deposit: bool, full: bool) {
        ClSharePoolFacadeClient::new(&self.env, &self.facade)
            .mock_auths(&[MockAuth {
                address: &self.admin,
                invoke: &MockAuthInvoke {
                    contract: &self.facade,
                    fn_name: "set_paused",
                    args: (deposit, full).into_val(&self.env),
                    sub_invokes: &[],
                },
            }])
            .set_paused(&deposit, &full);
    }
    fn trade(&self, reverse: bool, amount: u128, min: u128) -> u128 {
        ControllerAuthFixtureClient::new(&self.env, &self.controller).trade(
            &self.facade,
            if reverse { &self.vault } else { &self.asset },
            &reverse,
            &amount,
            &min,
            &0,
        )
    }
}

#[test]
fn real_contract_auth_roundtrip_preserves_donations_and_pays_full_output() {
    let f = Fixture::new();
    f.mint(&f.facade, 7);
    VaultFixtureClient::new(&f.env, &f.vault).setup(&3, &false, &true);
    assert_eq!(
        ClSharePoolFacadeClient::new(&f.env, &f.facade).estimate_swap(&0, &1, &100),
        100
    );
    assert_eq!(f.trade(false, 100, 100), 103);
    assert_eq!(
        VaultFixtureClient::new(&f.env, &f.vault).balance(&f.controller),
        103
    );
    assert_eq!(f.trade(true, 103, 103), 106);
    assert_eq!(
        token::TokenClient::new(&f.env, &f.asset).balance(&f.facade),
        7
    );
    assert_eq!(
        token::TokenClient::new(&f.env, &f.asset).balance(&f.controller),
        10_006
    );
    assert_eq!(
        VaultFixtureClient::new(&f.env, &f.vault).balance(&f.facade),
        0
    );
}

#[test]
fn missing_or_wrong_nested_authorization_fails_and_rolls_back() {
    for mode in [1, 2, 3] {
        let f = Fixture::new();
        assert!(ControllerAuthFixtureClient::new(&f.env, &f.controller)
            .try_trade(&f.facade, &f.asset, &false, &100, &100, &mode)
            .is_err());
        assert_eq!(
            token::TokenClient::new(&f.env, &f.asset).balance(&f.controller),
            10_000
        );
        assert_eq!(
            token::TokenClient::new(&f.env, &f.asset).balance(&f.facade),
            0
        );
    }
}

#[test]
fn controller_address_without_real_controller_authorization_fails() {
    let f = Fixture::new();
    assert!(ClSharePoolFacadeClient::new(&f.env, &f.facade)
        .try_swap(&f.controller, &0, &1, &100, &100)
        .is_err());
    let outsider = Address::generate(&f.env);
    assert!(ClSharePoolFacadeClient::new(&f.env, &f.facade)
        .try_swap(&outsider, &0, &1, &100, &100)
        .is_err());
}

#[test]
fn deposit_pause_preserves_exit_quotes_and_execution() {
    let f = Fixture::new();
    f.trade(false, 100, 100);
    f.pauses(true, false);
    let facade = ClSharePoolFacadeClient::new(&f.env, &f.facade);
    assert_eq!(facade.estimate_swap(&0, &1, &100), 0);
    assert_eq!(facade.estimate_swap(&1, &0, &100), 100);
    assert_eq!(f.trade(true, 100, 100), 100);
    f.pauses(false, true);
    assert_eq!(facade.estimate_swap(&0, &1, &100), 0);
    assert_eq!(facade.estimate_swap(&1, &0, &100), 0);
}

#[test]
fn failed_claim_cannot_consume_input_or_donations() {
    let f = Fixture::new();
    f.mint(&f.facade, 99);
    VaultFixtureClient::new(&f.env, &f.vault).setup(&0, &true, &true);
    assert!(ControllerAuthFixtureClient::new(&f.env, &f.controller)
        .try_trade(&f.facade, &f.asset, &false, &100, &100, &0)
        .is_err());
    assert_eq!(
        token::TokenClient::new(&f.env, &f.asset).balance(&f.controller),
        10_000
    );
    assert_eq!(
        token::TokenClient::new(&f.env, &f.asset).balance(&f.facade),
        99
    );
    // Deliberately no claim that recovery works: the fixture has no recovery
    // owner context or entitlement ledger. That remains a blocked gate.
}

#[test]
fn stale_nav_quotes_zero_and_insufficient_output_cannot_use_donations() {
    let f = Fixture::new();
    VaultFixtureClient::new(&f.env, &f.vault).setup(&0, &false, &false);
    let facade = ClSharePoolFacadeClient::new(&f.env, &f.facade);
    assert_eq!(facade.estimate_swap(&0, &1, &100), 0);
    assert!(ControllerAuthFixtureClient::new(&f.env, &f.controller)
        .try_trade(&f.facade, &f.asset, &false, &100, &100, &0)
        .is_err());
    VaultFixtureClient::new(&f.env, &f.vault).setup(&0, &false, &true);
    f.trade(false, 100, 100);
    f.mint(&f.facade, 1_000);
    assert!(ControllerAuthFixtureClient::new(&f.env, &f.controller)
        .try_trade(&f.facade, &f.vault, &true, &100, &101, &0)
        .is_err());
    assert_eq!(
        token::TokenClient::new(&f.env, &f.asset).balance(&f.facade),
        1_000
    );
}

#[test]
#[ignore = "requires locally compiled FACADE_VALIDATION_WASM; other contracts remain explicit fixtures"]
fn compiled_facade_roundtrip_with_native_auth_and_vault_fixtures() {
    let path = std::env::var("FACADE_VALIDATION_WASM").expect("FACADE_VALIDATION_WASM required");
    let wasm = std::fs::read(path).expect("read compiled facade");
    let f = Fixture::with_wasm(Some(&wasm));
    f.mint(&f.facade, 7);
    assert_eq!(f.trade(false, 100, 100), 100);
    crate::test::measure_scope(&f.env,"facade_fixture_open",true,"compiled-facade-native-synthetic-fixtures");
    let opening = f.env.cost_estimate().resources();
    std::println!("compiled facade / native fixtures opening: {opening:?}");
    assert_eq!(f.trade(true, 100, 100), 100);
    crate::test::measure_scope(&f.env,"facade_fixture_close",true,"compiled-facade-native-synthetic-fixtures");
    let closing = f.env.cost_estimate().resources();
    std::println!("compiled facade / native fixtures closing: {closing:?}");
    assert_eq!(
        token::TokenClient::new(&f.env, &f.asset).balance(&f.facade),
        7
    );
    for mode in [1, 2, 3] {
        assert!(ControllerAuthFixtureClient::new(&f.env, &f.controller)
            .try_trade(&f.facade, &f.asset, &false, &100, &100, &mode)
            .is_err());
        crate::test::measure_scope(&f.env,match mode {1=>"missing_nested_auth",2=>"wrong_amount_auth",_=>"wrong_recipient_auth"},true,"compiled-facade-native-synthetic-fixtures");
    }
    assert!(ClSharePoolFacadeClient::new(&f.env, &f.facade)
        .try_swap(&f.controller, &0, &1, &100, &100)
        .is_err());
    VaultFixtureClient::new(&f.env, &f.vault).setup(&0, &true, &true);
    assert!(ControllerAuthFixtureClient::new(&f.env, &f.controller)
        .try_trade(&f.facade, &f.asset, &false, &100, &100, &0)
        .is_err());
    assert_eq!(
        token::TokenClient::new(&f.env, &f.asset).balance(&f.facade),
        7
    );
}

#[test]
fn donation_sweep_requires_admin_and_has_fixed_recipient() {
    let f = Fixture::new();
    f.mint(&f.facade, 77);
    let facade = ClSharePoolFacadeClient::new(&f.env, &f.facade);
    assert!(facade.try_sweep(&f.asset).is_err());
    facade.mock_auths(&[MockAuth{address:&f.admin,invoke:&MockAuthInvoke{
        contract:&f.facade,fn_name:"sweep",args:(f.asset.clone(),).into_val(&f.env),sub_invokes:&[]
    }}]).sweep(&f.asset);
    assert_eq!(
        token::TokenClient::new(&f.env, &f.asset).balance(&f.treasury),
        77
    );
}

#[test]
#[should_panic(expected = "CL validation: Mainnet disabled")]
fn mainnet_initialization_is_disabled() {
    let env=Env::default();
    let hash=env.crypto().sha256(&Bytes::from_slice(&env,b"Public Global Stellar Network ; September 2015"));
    env.ledger().with_mut(|l|l.network_id=hash.to_array());
    let a=Address::generate(&env);let id=env.register(ClSharePoolFacade,());
    ClSharePoolFacadeClient::new(&env,&id).initialize(&a,&a,&a,&a,&a);
}
