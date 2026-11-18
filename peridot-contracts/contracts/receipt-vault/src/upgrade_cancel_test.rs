//! cancel_upgrade (cash-borrow upgrade checks, 2026-11-18).
extern crate std;

use crate::{DataKey, ReceiptVault, ReceiptVaultClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, BytesN, Env};

const DAY: u64 = 24 * 60 * 60;

fn setup() -> (Env, Address, ReceiptVaultClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let admin = Address::generate(&env);
    let tok = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let id = env.register(ReceiptVault, ());
    let c = ReceiptVaultClient::new(&env, &id);
    c.initialize(&tok, &0u128, &0u128, &admin);
    let _ = token::Client::new(&env, &tok);
    (env, id, c)
}

fn pending(env: &Env, id: &Address) -> (Option<BytesN<32>>, Option<u64>) {
    env.as_contract(id, || {
        (env.storage().persistent().get(&DataKey::PendingUpgradeHash), env.storage().persistent().get(&DataKey::PendingUpgradeEta))
    })
}

#[test]
fn receipt_cancel_upgrade_clears_pending_then_upgrade_panics() {
    let (env, id, c) = setup();
    let h = BytesN::from_array(&env, &[7u8; 32]);
    c.propose_upgrade_wasm(&h);
    assert!(pending(&env, &id).0.is_some());
    c.cancel_upgrade();
    assert_eq!(pending(&env, &id), (None, None));
    env.ledger().set_timestamp(1_000_000 + DAY + 1);
    assert!(c.try_upgrade_wasm(&h).is_err(), "upgrade after cancel must fail");
    assert!(c.try_cancel_upgrade().is_err(), "cancel with nothing pending fails");
}

#[test]
fn receipt_cancel_upgrade_rejects_non_admin() {
    let (env, id, c) = setup();
    let h = BytesN::from_array(&env, &[7u8; 32]);
    c.propose_upgrade_wasm(&h);
    env.set_auths(&[]);
    assert!(c.try_cancel_upgrade().is_err());
    assert!(pending(&env, &id).0.is_some(), "pending unchanged");
}

#[test]
fn receipt_repropose_overwrites_pending_hash_and_eta() {
    let (env, id, c) = setup();
    let h1 = BytesN::from_array(&env, &[1u8; 32]);
    let h2 = BytesN::from_array(&env, &[2u8; 32]);
    c.propose_upgrade_wasm(&h1);
    env.ledger().set_timestamp(1_000_000 + 3600);
    c.propose_upgrade_wasm(&h2);
    assert_eq!(pending(&env, &id), (Some(h2.clone()), Some(1_000_000 + 3600 + DAY)));
    env.ledger().set_timestamp(1_000_000 + DAY + 1);
    assert!(c.try_upgrade_wasm(&h1).is_err(), "old hash no longer pending");
    assert!(c.try_upgrade_wasm(&h2).is_err(), "new ETA restarts the timelock");
}
