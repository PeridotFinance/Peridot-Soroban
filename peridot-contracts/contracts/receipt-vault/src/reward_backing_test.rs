//! Real SAC custody and ReceiptVault integration through a TEST-ONLY interface.
//! Allocation/pending setters stand in for the still-missing epoch coordinator.
use crate::reward_backing::{self as backing, BackingState};
use crate::test::{
    MockBoostedVault, MockBoostedVaultClient, UnderDeliverToken, UnderDeliverTokenClient,
};
use crate::{DataKey, ReceiptVault};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
    token, Address, Env, IntoVal,
};

#[contract]
struct BackingHarness;

fn admin_auth(env: &Env) {
    let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
    admin.require_auth();
}

#[contractimpl]
impl BackingHarness {
    pub fn initialize(env: Env, asset: Address, admin: Address) {
        ReceiptVault::initialize(env, asset, 0, 0, admin);
    }
    pub fn static_rates(env: Env, admin: Address) {
        ReceiptVault::enable_static_rates(env, admin);
    }
    pub fn init_backing(env: Env) {
        backing::initialize(&env);
    }
    pub fn deposit(env: Env, owner: Address, amount: u128) {
        ReceiptVault::deposit(env, owner, amount);
    }
    pub fn withdraw(env: Env, owner: Address, shares: u128) {
        ReceiptVault::withdraw(env, owner, shares);
    }
    pub fn boosted(env: Env, strategy: Address) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        ReceiptVault::set_boosted_vault(env, admin, strategy);
    }
    pub fn cap(env: Env, cap: u128) {
        ReceiptVault::set_supply_cap(env, cap);
    }
    pub fn fund(env: Env, source: Address, amount: u128, recycled: u128, min_units: u128) -> u128 {
        backing::fund(&env, &source, amount, recycled, min_units)
    }
    pub fn recycle(env: Env, source: Address, amount: u128, recycled: u128) -> u128 {
        source.require_auth();
        let snapshot = backing::begin_recycled(&env);
        let asset = ReceiptVault::get_underlying_token(env.clone());
        token::Client::new(&env, &asset).transfer(
            &source,
            &env.current_contract_address(),
            &(amount as i128),
        );
        backing::finish_idle(&env, snapshot, amount, recycled, 0)
    }
    pub fn fund_with_yield(env: Env, source: Address, strategy: Address, amount: u128) -> u128 {
        source.require_auth();
        let snapshot = backing::begin(&env);
        let asset: Address = env
            .storage()
            .persistent()
            .get(&DataKey::UnderlyingToken)
            .unwrap();
        let token = token::Client::new(&env, &asset);
        token.transfer(&source, &env.current_contract_address(), &(amount as i128));
        token.transfer(&source, &strategy, &(amount as i128));
        backing::finish(&env, snapshot, amount, 0, 0)
    }
    pub fn allocate(env: Env, owner: Address, units: u128) {
        admin_auth(&env);
        backing::allocate(&env, &owner, units);
    }
    pub fn pending(env: Env, count: u32) {
        admin_auth(&env);
        backing::set_pending_rewards(&env, count);
    }
    pub fn redeem(env: Env, owner: Address, units: u128, minimum: u128) -> u128 {
        backing::redeem(&env, &owner, units, minimum)
    }
    pub fn state(env: Env) -> BackingState {
        backing::state(&env)
    }
    pub fn units(env: Env, owner: Address) -> u128 {
        backing::owner_units(&env, &owner)
    }
    pub fn balance(env: Env, owner: Address) -> i128 {
        ReceiptVault::balance(env, owner)
    }
    pub fn supply(env: Env) -> u128 {
        ReceiptVault::get_total_ptokens(env)
    }
    pub fn nav(env: Env) -> u128 {
        ReceiptVault::get_total_underlying(env)
    }
}

struct Fixture {
    env: Env,
    vault: Address,
    asset: Address,
    depositor: Address,
    source: Address,
    alice: Address,
    bob: Address,
}

impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let depositor = Address::generate(&env);
        let source = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let asset = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let vault = env.register(BackingHarness, ());
        BackingHarnessClient::new(&env, &vault).initialize(&asset, &admin);
        BackingHarnessClient::new(&env, &vault).static_rates(&admin);
        BackingHarnessClient::new(&env, &vault).init_backing();
        let mint = token::StellarAssetClient::new(&env, &asset);
        mint.mint(&depositor, &10_000_000);
        mint.mint(&source, &10_000_000);
        Self {
            env,
            vault,
            asset,
            depositor,
            source,
            alice,
            bob,
        }
    }
    fn client(&self) -> BackingHarnessClient<'_> {
        BackingHarnessClient::new(&self.env, &self.vault)
    }
    fn token(&self) -> token::Client<'_> {
        token::Client::new(&self.env, &self.asset)
    }
    fn with_strategy(&self) -> Address {
        let strategy = self.env.register(MockBoostedVault, ());
        MockBoostedVaultClient::new(&self.env, &strategy).initialize(&self.asset);
        self.client().boosted(&strategy);
        strategy
    }
}

#[test]
fn real_receipts_back_reward_claims_without_diluting_principal() {
    let f = Fixture::new();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    assert_eq!(c.fund(&f.source, &200_000, &0, &200_000), 200_000);
    assert_eq!(c.balance(&f.vault), 200_000);
    assert_eq!(c.supply(), 1_200_000);
    assert_eq!(c.nav(), 1_200_000);
    c.allocate(&f.alice, &120_000);
    c.allocate(&f.bob, &80_000);
    assert!(c.try_allocate(&f.bob, &1).is_err());
    assert_eq!(c.redeem(&f.alice, &120_000, &120_000), 120_000);
    assert!(c.try_redeem(&f.alice, &1, &0).is_err());
    assert_eq!(c.redeem(&f.bob, &80_000, &80_000), 80_000);
    assert_eq!(c.balance(&f.vault), 0);
    assert_eq!(c.state().units, 0);
    c.withdraw(&f.depositor, &1_000_000);
    assert_eq!(f.token().balance(&f.depositor), 10_000_000);
    assert_eq!(f.token().balance(&f.vault), 0);
    assert_eq!(c.supply(), 0);
}

#[test]
fn reward_backing_reinvests_and_redeems_real_strategy_yield() {
    let f = Fixture::new();
    let strategy = f.with_strategy();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    c.fund(&f.source, &200_000, &0, &200_000);
    c.allocate(&f.alice, &200_000);
    assert_eq!(f.token().balance(&f.vault), 0);
    assert_eq!(f.token().balance(&strategy), 1_200_000);
    // Real extra underlying backing, not an arbitrary valuation multiplier.
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&strategy, &120_000);
    assert_eq!(c.nav(), 1_320_000);
    assert_eq!(c.redeem(&f.alice, &200_000, &220_000), 220_000);
    c.withdraw(&f.depositor, &1_000_000);
    assert_eq!(f.token().balance(&f.depositor), 10_100_000);
    assert_eq!(f.token().balance(&strategy), 0);
    assert_eq!(c.supply(), 0);
}

#[test]
fn funding_at_increased_nav_mints_fewer_real_receipts() {
    let f = Fixture::new();
    let strategy = f.with_strategy();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&strategy, &1_000_000);
    assert_eq!(c.fund(&f.source, &200_000, &0, &100_000), 100_000);
    c.allocate(&f.alice, &100_000);
    assert_eq!(c.supply(), 1_100_000);
    assert_eq!(c.nav(), 2_200_000);
    assert_eq!(c.redeem(&f.alice, &100_000, &200_000), 200_000);
    c.withdraw(&f.depositor, &1_000_000);
    assert_eq!(f.token().balance(&f.depositor), 11_000_000);
}

#[test]
fn recycled_yield_belongs_to_old_units_before_new_units_are_issued() {
    let f = Fixture::new();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    c.fund(&f.source, &100_000, &0, &100_000);
    c.allocate(&f.alice, &100_000);
    // 100k for existing backing plus 100k newly earned by Bob.
    assert_eq!(c.fund(&f.source, &200_000, &100_000, &50_000), 50_000);
    c.allocate(&f.bob, &50_000);
    assert_eq!(c.state().ptokens, 300_000);
    assert_eq!(c.state().units, 150_000);
    assert_eq!(c.redeem(&f.alice, &100_000, &200_000), 200_000);
    assert_eq!(c.redeem(&f.bob, &50_000, &100_000), 100_000);
    assert_eq!(c.nav(), 1_000_000);
}

#[test]
fn pure_recycled_yield_mints_no_new_claim_units() {
    let f = Fixture::new();
    let c = f.client();
    assert!(c.try_fund(&f.source, &10_000, &10_000, &0).is_err());
    c.fund(&f.source, &100_000, &0, &100_000);
    c.allocate(&f.alice, &100_000);
    assert_eq!(c.fund(&f.source, &100_000, &100_000, &0), 0);
    assert_eq!(c.state().units, 100_000);
    assert_eq!(c.redeem(&f.alice, &100_000, &200_000), 200_000);
    assert_eq!(c.supply(), 0);
}

#[test]
fn pending_recycle_snapshot_cannot_issue_units_or_clear_other_pending_streams() {
    let f = Fixture::new();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    c.fund(&f.source, &100_000, &0, &100_000);
    c.allocate(&f.alice, &100_000);
    assert!(c.try_recycle(&f.source, &50_000, &50_000).is_err()); // no pending rewards
    c.pending(&2);
    let before = c.state();
    let source = f.token().balance(&f.source);
    assert!(c.try_recycle(&f.source, &100_000, &50_000).is_err());
    assert_eq!(c.state(), before);
    assert_eq!(f.token().balance(&f.source), source);
    assert_eq!(c.recycle(&f.source, &100_000, &100_000), 0);
    assert_eq!(c.state().units, before.units);
    assert_eq!(c.state().ptokens, before.ptokens + 100_000);
    assert_eq!(c.state().pending_rewards, 2);
    assert!(c.try_redeem(&f.alice, &100_000, &1).is_err());
    assert!(c.try_fund(&f.source, &100_000, &0, &0).is_err());
}

#[test]
fn failed_minimum_rolls_back_tokens_claims_and_strategy_unwind() {
    let f = Fixture::new();
    let strategy = f.with_strategy();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    c.fund(&f.source, &100_000, &0, &100_000);
    c.allocate(&f.alice, &100_000);
    let before = c.state();
    assert!(c.try_redeem(&f.alice, &100_000, &100_001).is_err());
    assert_eq!(c.state(), before);
    assert_eq!(c.units(&f.alice), 100_000);
    assert_eq!(c.balance(&f.alice), 0);
    assert_eq!(c.balance(&f.vault), 100_000);
    assert_eq!(f.token().balance(&f.alice), 0);
    assert_eq!(f.token().balance(&strategy), 1_100_000);
    assert_eq!(c.redeem(&f.alice, &100_000, &100_000), 100_000);
}

#[test]
fn failed_funding_rolls_back_source_cash_and_supply() {
    let f = Fixture::new();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    let before = c.state();
    assert!(c.try_fund(&f.source, &100_000, &0, &100_001).is_err());
    assert_eq!(c.state(), before);
    assert_eq!(f.token().balance(&f.source), 10_000_000);
    assert_eq!(c.supply(), 1_000_000);
    c.cap(&1_050_000);
    assert!(c.try_fund(&f.source, &100_000, &0, &0).is_err());
    assert_eq!(c.state(), before);
    assert_eq!(f.token().balance(&f.source), 10_000_000);
}

#[test]
fn pending_backing_rewards_gate_claim_changes_but_not_principal_exit() {
    let f = Fixture::new();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    c.fund(&f.source, &100_000, &0, &100_000);
    c.allocate(&f.alice, &100_000);
    c.pending(&1);
    assert!(c.try_fund(&f.source, &100_000, &0, &0).is_err());
    assert!(c.try_redeem(&f.alice, &100_000, &0).is_err());
    c.withdraw(&f.depositor, &1_000_000);
    assert_eq!(f.token().balance(&f.depositor), 10_000_000);
    assert_eq!(c.units(&f.alice), 100_000);
    c.pending(&0);
    assert_eq!(c.redeem(&f.alice, &100_000, &100_000), 100_000);
}

#[test]
fn donation_is_not_minted_or_deployed_as_new_reward_backing() {
    let f = Fixture::new();
    let strategy = f.with_strategy();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    f.token().transfer(&f.source, &f.vault, &50_000);
    c.fund(&f.source, &100_000, &0, &100_000);
    assert_eq!(c.nav(), 1_100_000);
    assert_eq!(c.supply(), 1_100_000);
    assert_eq!(f.token().balance(&strategy), 1_100_000);
    assert_eq!(f.token().balance(&f.vault), 50_000);
}

#[test]
fn invested_strategy_cannot_be_used_as_a_new_funding_source() {
    let f = Fixture::new();
    let strategy = f.with_strategy();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    assert!(c.try_fund(&strategy, &100_000, &0, &0).is_err());
    assert!(c.try_fund(&f.vault, &100_000, &0, &0).is_err());
    assert_eq!(f.token().balance(&strategy), 1_000_000);
    assert_eq!(c.state().units, 0);
}

#[test]
fn authentication_is_required_for_funding_claims_and_coordinator_hooks() {
    let f = Fixture::new();
    let c = f.client();
    c.fund(&f.source, &100_000, &0, &100_000);
    c.allocate(&f.alice, &50_000);
    f.env.mock_auths(&[]);
    assert!(c.try_fund(&f.source, &100_000, &0, &0).is_err());
    assert!(c.try_redeem(&f.alice, &50_000, &0).is_err());
    assert!(c.try_allocate(&f.bob, &50_000).is_err());
    assert!(c.try_pending(&1).is_err());
    assert!(c.try_init_backing().is_err());
    assert_eq!(c.units(&f.alice), 50_000);
    assert_eq!(c.state().unallocated_units, 50_000);
}

#[test]
fn short_token_transfer_cannot_create_unbacked_units() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let source = Address::generate(&env);
    let asset = env.register(UnderDeliverToken, ());
    let token = UnderDeliverTokenClient::new(&env, &asset);
    token.initialize();
    token.mint(&source, &100_000);
    token.set_transfer_haircut(&1);
    let vault = env.register(BackingHarness, ());
    let c = BackingHarnessClient::new(&env, &vault);
    c.initialize(&asset, &admin);
    c.static_rates(&admin);
    c.init_backing();
    assert!(c.try_fund(&source, &100_000, &0, &0).is_err());
    assert_eq!(token.balance(&source), 100_000);
    assert_eq!(token.balance(&vault), 0);
    assert_eq!(c.supply(), 0);
    assert_eq!(c.state().units, 0);
}

#[test]
fn exact_source_and_owner_auth_trees_succeed_without_blanket_auth() {
    let f = Fixture::new();
    let c = f.client();
    f.env.mock_auths(&[]);
    c.mock_auths(&[MockAuth {
        address: &f.source,
        invoke: &MockAuthInvoke {
            contract: &f.vault,
            fn_name: "fund",
            args: (f.source.clone(), 100_000u128, 0u128, 100_000u128).into_val(&f.env),
            sub_invokes: &[MockAuthInvoke {
                contract: &f.asset,
                fn_name: "transfer",
                args: (f.source.clone(), f.vault.clone(), 100_000i128).into_val(&f.env),
                sub_invokes: &[],
            }],
        },
    }])
    .fund(&f.source, &100_000, &0, &100_000);
    // Test-only admin allocation stands in for the absent epoch coordinator.
    f.env.mock_all_auths();
    c.allocate(&f.alice, &100_000);
    f.env.mock_auths(&[]);
    let paid = c
        .mock_auths(&[MockAuth {
            address: &f.alice,
            invoke: &MockAuthInvoke {
                contract: &f.vault,
                fn_name: "redeem",
                args: (f.alice.clone(), 100_000u128, 100_000u128).into_val(&f.env),
                sub_invokes: &[],
            },
        }])
        .redeem(&f.alice, &100_000, &100_000);
    assert_eq!(paid, 100_000);
    assert_eq!(f.token().balance(&f.alice), 100_000);
}

#[test]
fn fractional_units_leave_dust_with_old_backing_and_last_claim_collects_it() {
    let f = Fixture::new();
    let c = f.client();
    c.fund(&f.source, &3, &0, &3);
    c.allocate(&f.alice, &3);
    // Old backing gains 1, new funding adds 3 => floor(3*3/4)=2 new units.
    assert_eq!(c.fund(&f.source, &4, &1, &2), 2);
    c.allocate(&f.bob, &2);
    assert_eq!(c.state().ptokens, 7);
    assert_eq!(c.state().units, 5);
    assert_eq!(c.redeem(&f.bob, &2, &2), 2);
    assert_eq!(c.redeem(&f.alice, &3, &5), 5);
    assert_eq!(c.supply(), 0);
    assert_eq!(f.token().balance(&f.vault), 0);
}

#[test]
fn principal_exit_then_new_deposit_cannot_capture_existing_reward_backing() {
    let f = Fixture::new();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    c.fund(&f.source, &100_000, &0, &100_000);
    c.allocate(&f.alice, &100_000);
    c.withdraw(&f.depositor, &1_000_000);
    assert_eq!(c.supply(), 100_000);
    assert_eq!(c.balance(&f.vault), 100_000);
    c.deposit(&f.depositor, &500_000);
    c.withdraw(&f.depositor, &500_000);
    assert_eq!(f.token().balance(&f.depositor), 10_000_000);
    assert_eq!(c.redeem(&f.alice, &100_000, &100_000), 100_000);
    assert_eq!(c.supply(), 0);
    assert!(c.try_init_backing().is_err());
}

#[test]
fn failed_strategy_quote_with_expired_cache_preserves_every_claim_and_token() {
    let f = Fixture::new();
    let strategy = f.with_strategy();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    c.fund(&f.source, &100_000, &0, &100_000);
    c.allocate(&f.alice, &100_000);
    let state = c.state();
    let boosted = MockBoostedVaultClient::new(&f.env, &strategy);
    boosted.set_fail_quote(&true);
    // Ordinary ReceiptVault can fall back to accounting. Reward pricing may not.
    f.env.ledger().with_mut(|info| info.timestamp += 3_601);
    assert!(c.try_redeem(&f.alice, &100_000, &0).is_err());
    assert!(c.try_fund(&f.source, &100_000, &0, &0).is_err());
    assert_eq!(c.state(), state);
    assert_eq!(c.units(&f.alice), 100_000);
    assert_eq!(c.balance(&f.vault), 100_000);
    assert_eq!(f.token().balance(&strategy), 1_100_000);
    assert_eq!(f.token().balance(&f.alice), 0);
    assert_eq!(f.token().balance(&f.source), 9_900_000);
}

#[test]
fn funding_rejects_strategy_value_changes_during_conversion() {
    let f = Fixture::new();
    let strategy = f.with_strategy();
    let c = f.client();
    c.deposit(&f.depositor, &1_000_000);
    assert!(c
        .try_fund_with_yield(&f.source, &strategy, &100_000)
        .is_err());
    assert_eq!(c.state().units, 0);
    assert_eq!(c.supply(), 1_000_000);
    assert_eq!(f.token().balance(&f.source), 10_000_000);
    assert_eq!(f.token().balance(&strategy), 1_000_000);
    assert_eq!(f.token().balance(&f.vault), 0);
}
