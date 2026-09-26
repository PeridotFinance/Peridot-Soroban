//! LOCAL storage-shadow rehearsals, not a contract upgrade or authorization test.
//! Start with the actual generic receipt's native methods. The test-only marker
//! insertion below is NOT a migration entrypoint and MUST NOT become one.
extern crate std;

use crate::{migration::*, reward_ledger as ledger, storage::LpKey, DataKey, ReceiptVault};
use receipt_vault::ReceiptVault as Legacy;
use soroban_sdk::{contract, contractimpl, testutils::Address as _, token, Address, Env};
use stellar_tokens::fungible::Base as TokenBase;

#[contract]
struct LegacyShadow;
#[contractimpl]
impl LegacyShadow {
    pub fn init(env: Env, asset: Address, admin: Address) {
        Legacy::initialize(env, asset, 0, 0, admin);
    }
    pub fn enable(env: Env, admin: Address) {
        Legacy::enable_static_rates(env, admin);
    }
    pub fn deposit(env: Env, owner: Address, amount: u128) {
        Legacy::deposit(env, owner, amount);
    }
    pub fn approve(env: Env, owner: Address, spender: Address) {
        Legacy::approve(env, owner, spender, 123, 1000);
    }
    pub fn probe(env: Env) {
        let _ = snapshot(&env);
    }
    pub fn lean_deposit(env: Env, owner: Address, amount: u128) {
        ReceiptVault::deposit(env, owner, amount);
    }
    pub fn lean_withdraw(env: Env, owner: Address, amount: u128) {
        ReceiptVault::withdraw(env, owner, amount);
    }
}

struct Fixture {
    env: Env,
    receipt: Address,
    asset: Address,
    alice: Address,
    bob: Address,
}
impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let asset = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let receipt = env.register(LegacyShadow, ());
        let client = LegacyShadowClient::new(&env, &receipt);
        client.init(&asset, &admin);
        client.enable(&admin);
        let sac = token::StellarAssetClient::new(&env, &asset);
        sac.mint(&alice, &1_000_000);
        sac.mint(&bob, &1_000_000);
        client.deposit(&alice, &100_000);
        client.deposit(&bob, &300_000);
        client.approve(&alice, &bob);
        // Explicit synthetic canonical mirrors. Generic initialization doesn't
        // create all these keys. A real missing mirror is UNKNOWN, not zero.
        env.as_contract(&receipt, || {
            let p = env.storage().persistent();
            p.set(&DataKey::TotalBorrowPrincipal, &0u128);
            p.set(&DataKey::TotalBadDebt, &0u128);
            p.set(&DataKey::IdleCashBufferBps, &10_000u32);
        });
        Self {
            env,
            receipt,
            asset,
            alice,
            bob,
        }
    }
    fn snapshot(&self) -> LocalSnapshot {
        self.env.as_contract(&self.receipt, || snapshot(&self.env))
    }
    fn shadow_only(&self) {
        // Only this deliberately controller-free, strategy-free local fixture
        // may model storage compatibility. No exported method can set a marker.
        let before = self.snapshot();
        assert_eq!(check_local(&before), Ok(()));
        assert_eq!(before.strategy, None);
        self.env.as_contract(&self.receipt, || {
            assert!(!self.env.storage().instance().has(&LpKey::LpReceiptVersion));
            self.env
                .storage()
                .instance()
                .set(&LpKey::LpReceiptVersion, &1u32);
            self.env
                .storage()
                .instance()
                .set(&LpKey::LpDepositPaused, &true);
        });
        assert_local_preserved(&before, &self.snapshot());
    }
}

#[test]
fn migration_shadow_preserves_raw_shares_allowances_metadata_cash_and_donations() {
    let f = Fixture::new();
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&f.receipt, &57);
    let before = f.snapshot();
    let owners = f.env.as_contract(&f.receipt, || {
        (
            TokenBase::balance(&f.env, &f.alice),
            TokenBase::balance(&f.env, &f.bob),
            TokenBase::allowance_data(&f.env, &f.alice, &f.bob),
        )
    });
    f.shadow_only();
    f.env.as_contract(&f.receipt, || {
        assert_eq!(
            ReceiptVault::balance(f.env.clone(), f.alice.clone()),
            owners.0
        );
        assert_eq!(
            ReceiptVault::balance(f.env.clone(), f.bob.clone()),
            owners.1
        );
        let allowance = TokenBase::allowance_data(&f.env, &f.alice, &f.bob);
        assert_eq!(allowance.amount, owners.2.amount);
        assert_eq!(allowance.live_until_ledger, owners.2.live_until_ledger);
        assert_eq!(
            ReceiptVault::get_total_underlying(f.env.clone()),
            before.managed_cash
        );
    });
    assert_eq!(before.actual_cash as u128 - before.managed_cash, 57);
    let c = LegacyShadowClient::new(&f.env, &f.receipt);
    assert!(c.try_lean_deposit(&f.alice, &1).is_err());
    c.lean_withdraw(&f.alice, &100_000);
    c.lean_withdraw(&f.bob, &300_000);
    assert_eq!(token::Client::new(&f.env, &f.asset).balance(&f.receipt), 57);
    assert_eq!(f.snapshot().supply, 0);
}

#[test]
fn migration_diagnostic_never_activates_legacy_state() {
    let f = Fixture::new();
    let before = f.snapshot();
    assert_eq!(check_local(&before), Ok(()));
    let c = LegacyShadowClient::new(&f.env, &f.receipt);
    assert!(c.try_lean_deposit(&f.alice, &1).is_err());
    assert_eq!(before, f.snapshot());
}

#[test]
fn migration_preserves_non_par_share_value_without_minting_replacement_shares() {
    let f = Fixture::new();
    token::StellarAssetClient::new(&f.env, &f.asset).mint(&f.receipt, &40_000);
    f.env.as_contract(&f.receipt, || {
        // Already recognized legacy yield, not an untracked donation.
        f.env
            .storage()
            .persistent()
            .set(&DataKey::ManagedCash, &440_000u128);
        assert_eq!(Legacy::get_total_underlying(f.env.clone()), 440_000);
    });
    f.shadow_only();
    assert_eq!(f.snapshot().supply, 400_000);
    let c = LegacyShadowClient::new(&f.env, &f.receipt);
    let sac = token::Client::new(&f.env, &f.asset);
    let before = sac.balance(&f.alice);
    c.lean_withdraw(&f.alice, &100_000);
    assert_eq!(sac.balance(&f.alice) - before, 110_000);
    let before = sac.balance(&f.bob);
    c.lean_withdraw(&f.bob, &300_000);
    assert_eq!(sac.balance(&f.bob) - before, 330_000);
    assert_eq!(sac.balance(&f.receipt), 0);
}

#[test]
fn migration_missing_mirrors_are_not_assumed_zero() {
    for key in [
        DataKey::TotalBorrowPrincipal,
        DataKey::TotalBadDebt,
        DataKey::TotalReserves,
        DataKey::TotalAdminFees,
        DataKey::IdleCashBufferBps,
    ] {
        let f = Fixture::new();
        f.env
            .as_contract(&f.receipt, || f.env.storage().persistent().remove(&key));
        assert_eq!(
            check_local(&f.snapshot()),
            Err(LocalBlocker::UnknownAccounting)
        );
    }
}

#[test]
fn migration_missing_required_cash_does_not_fall_back_to_actual_donations() {
    let f = Fixture::new();
    f.env.as_contract(&f.receipt, || {
        f.env.storage().persistent().remove(&DataKey::ManagedCash)
    });
    assert!(LegacyShadowClient::new(&f.env, &f.receipt)
        .try_probe()
        .is_err());
}

#[test]
fn migration_missing_supply_or_metadata_is_not_reinitialized() {
    use stellar_tokens::fungible::FungibleStorageKey;
    for key in [FungibleStorageKey::TotalSupply, FungibleStorageKey::Meta] {
        let f = Fixture::new();
        f.env
            .as_contract(&f.receipt, || f.env.storage().instance().remove(&key));
        assert!(LegacyShadowClient::new(&f.env, &f.receipt)
            .try_probe()
            .is_err());
    }
}

#[test]
fn migration_any_debt_fees_or_bad_debt_rejects_even_one_raw_unit() {
    for key in [
        DataKey::TotalBorrowed,
        DataKey::TotalBorrowPrincipal,
        DataKey::TotalBadDebt,
        DataKey::TotalReserves,
        DataKey::TotalAdminFees,
    ] {
        let f = Fixture::new();
        f.env.as_contract(&f.receipt, || {
            f.env.storage().persistent().set(&key, &1u128)
        });
        assert_eq!(check_local(&f.snapshot()), Err(LocalBlocker::DebtOrFees));
    }
}

#[test]
fn migration_controller_links_cannot_be_cleared_by_local_zero_totals() {
    for key in [DataKey::Peridottroller, DataKey::MarginController] {
        let f = Fixture::new();
        f.env.as_contract(&f.receipt, || {
            f.env.storage().persistent().set(&key, &f.bob)
        });
        assert_eq!(
            check_local(&f.snapshot()),
            Err(LocalBlocker::ExternalController)
        );
    }
}

#[test]
fn migration_zero_totals_do_not_prove_absence_of_individual_liabilities() {
    let f = Fixture::new();
    f.env.as_contract(&f.receipt, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::BorrowPrincipal(f.alice.clone()), &77u128);
        f.env
            .storage()
            .persistent()
            .set(&DataKey::MarginBorrowPrincipal(1), &11u128);
    });
    // Deliberately demonstrates the boundary, NOT approval. No owner iteration
    // or archive discovery exists in this local diagnostic. Production activation
    // remains absent and rejects these legacy shares despite local Ok.
    assert_eq!(check_local(&f.snapshot()), Ok(()));
    assert!(LegacyShadowClient::new(&f.env, &f.receipt)
        .try_lean_withdraw(&f.alice, &1)
        .is_err());
}

#[test]
fn migration_escrow_and_underbacking_require_separate_resolution() {
    let f = Fixture::new();
    f.env.as_contract(&f.receipt, || {
        TokenBase::transfer(&f.env, &f.alice, &f.receipt.clone().into(), 1)
    });
    assert_eq!(
        check_local(&f.snapshot()),
        Err(LocalBlocker::ExistingEscrow)
    );
    let f = Fixture::new();
    f.env.as_contract(&f.receipt, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::ManagedCash, &400_001u128)
    });
    assert_eq!(
        check_local(&f.snapshot()),
        Err(LocalBlocker::InvalidBacking)
    );
}

#[test]
fn migration_pending_admin_flash_and_repeat_activation_rejected() {
    let f = Fixture::new();
    f.env.as_contract(&f.receipt, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &f.bob)
    });
    assert_eq!(
        check_local(&f.snapshot()),
        Err(LocalBlocker::AdministrativeTransition)
    );
    f.env.as_contract(&f.receipt, || {
        f.env.storage().persistent().remove(&DataKey::PendingAdmin);
        f.env
            .storage()
            .persistent()
            .set(&DataKey::FlashLoanActive, &true);
    });
    assert_eq!(
        check_local(&f.snapshot()),
        Err(LocalBlocker::AdministrativeTransition)
    );
    let f = Fixture::new();
    f.shadow_only();
    assert_eq!(
        check_local(&f.snapshot()),
        Err(LocalBlocker::AlreadyActivated)
    );
}

#[test]
#[should_panic(expected = "local cutover changed accounting")]
fn migration_comparison_rejects_balance_normalization() {
    let f = Fixture::new();
    let before = f.snapshot();
    let mut after = before.clone();
    after.lp_version = Some(1);
    after.supply *= 10; // Changing display decimals never permits changing raw shares.
    assert_local_preserved(&before, &after);
}

#[test]
fn migration_new_stream_starts_at_zero_and_cold_legacy_holder_keeps_new_emissions() {
    let f = Fixture::new();
    f.shadow_only();
    let reward = f
        .env
        .register_stellar_asset_contract_v2(f.alice.clone())
        .address();
    let sac = token::StellarAssetClient::new(&f.env, &reward);
    sac.mint(&f.receipt, &999); // Unclassified OLD inventory, never a new receipt.
    f.env.as_contract(&f.receipt, || {
        ledger::register(&f.env, &reward, 400_000);
        assert_eq!(ledger::stream(&f.env, &reward).raw, 0);
    });
    let receive = f
        .env
        .as_contract(&f.receipt, || ledger::begin_receive(&f.env, &reward));
    sac.mint(&f.receipt, &400);
    f.env
        .as_contract(&f.receipt, || ledger::finish_receive(&f.env, receive, 400));
    f.env.as_contract(&f.receipt, || {
        // Neither owner needed a per-account migration record at zero epoch.
        let a = ledger::checkpoint(&f.env, &reward, &f.alice, 100_000);
        let b = ledger::checkpoint(&f.env, &reward, &f.bob, 300_000);
        assert_eq!(a.raw_scaled / ledger::SCALE, 100);
        assert_eq!(b.raw_scaled / ledger::SCALE, 300);
        assert_eq!(ledger::stream(&f.env, &reward).raw, 400);
    });
    assert_eq!(
        token::Client::new(&f.env, &reward).balance(&f.receipt),
        1399
    );
    // Old 999 remain UNASSIGNED, not payable, and are a real activation blocker.
}
