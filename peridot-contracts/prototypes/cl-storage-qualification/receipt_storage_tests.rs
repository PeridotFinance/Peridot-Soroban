//! Synthetic native compatibility tests for the isolated storage overlay.
//! Raw fixture state is deliberately TEST ONLY; this is not initialization,
//! compiled authorization, resource qualification or live venue evidence.
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::testutils::storage::Persistent as _;

fn fixture() -> (Env, Address) {
    let env = Env::default();
    env.ledger().with_mut(|info| {
        info.sequence_number = 100;
        info.timestamp = 0;
        info.min_persistent_entry_ttl = 10;
        info.max_entry_ttl = 6_000_000;
    });
    let id = env.register(ReceiptVault, ());
    let asset = Address::generate(&env);
    let admin = Address::generate(&env);
    env.as_contract(&id, || {
        let p = env.storage().persistent();
        p.set(&DataKey::UnderlyingToken, &asset);
        p.set(&DataKey::Initialized, &true);
        p.set(&DataKey::Admin, &admin);
        p.set(&DataKey::LastUpdateTime, &0u64);
        p.set(&DataKey::TotalBorrowed, &1_000_000_000u128);
        p.set(&DataKey::BorrowIndex, &1_000_000_000_000_000_000u128);
        p.set(&DataKey::BorrowYearlyRateScaled, &100_000u128);
        p.set(&DataKey::TotalReserves, &0u128);
        p.set(&DataKey::TotalAdminFees, &0u128);
        p.set(&DataKey::ReserveFactorScaled, &100_000u128);
        p.set(&DataKey::AdminFeeScaled, &50_000u128);
        p.set(&DataKey::YearlyRateScaled, &25_000u128);
        p.set(&DataKey::AccumulatedInterest, &123u128);
        p.set(&DataKey::SupplyCap, &5_000_000_000u128);
    });
    env.ledger().with_mut(|info| { info.sequence_number = 102; info.timestamp = 31_536_000; });
    (env, id)
}

fn totals(env: &Env, id: &Address) -> (u128,u128,u128,u128,u64) {
    env.as_contract(id, || {
        let p = env.storage().persistent();
        (p.get(&DataKey::TotalBorrowed).unwrap(),
         p.get(&DataKey::BorrowIndex).unwrap(),
         p.get(&DataKey::TotalReserves).unwrap(),
         p.get(&DataKey::TotalAdminFees).unwrap(),
         p.get(&DataKey::LastUpdateTime).unwrap())
    })
}

#[test]
fn storage_scoped_nonzero_public_helper_parity_and_repeat() {
    let (public_env, public_id) = fixture();
    let (direct_env, direct_id) = fixture();
    ReceiptVaultClient::new(&public_env, &public_id).update_interest();
    direct_env.as_contract(&direct_id, || ReceiptVault::accrue_interest_after_keepalive(direct_env.clone()));
    let expected = (1_100_000_000, 1_100_000_000_000_000_000, 10_000_000, 5_000_000, 31_536_000);
    assert_eq!(totals(&public_env, &public_id), expected);
    assert_eq!(totals(&direct_env, &direct_id), expected);
    ReceiptVaultClient::new(&public_env, &public_id).update_interest();
    direct_env.as_contract(&direct_id, || ReceiptVault::accrue_interest_after_keepalive(direct_env.clone()));
    assert_eq!(totals(&public_env, &public_id), expected);
    assert_eq!(totals(&direct_env, &direct_id), expected);
}

#[test]
fn storage_scoped_near_expiry_renews_consumed_keys_not_governance() {
    let (env, id) = fixture();
    let before = env.as_contract(&id, || env.storage().persistent().get_ttl(&DataKey::Admin));
    assert!(before < 10);
    ReceiptVaultClient::new(&env, &id).update_interest();
    env.as_contract(&id, || {
        let p = env.storage().persistent();
        for key in [DataKey::BorrowIndex, DataKey::LastUpdateTime, DataKey::ReserveFactorScaled, DataKey::AdminFeeScaled] {
            assert!(p.get_ttl(&key) >= 999_999);
        }
        for key in [DataKey::Admin, DataKey::SupplyCap, DataKey::YearlyRateScaled, DataKey::AccumulatedInterest] {
            assert_eq!(p.get_ttl(&key), before);
        }
        assert_eq!(p.get::<_,u128>(&DataKey::AccumulatedInterest), Some(123));
    });
}

#[test]
fn storage_scoped_missing_optional_factors_keep_original_defaults() {
    let (env, id) = fixture();
    env.as_contract(&id, || {
        env.storage().persistent().remove(&DataKey::ReserveFactorScaled);
        env.storage().persistent().remove(&DataKey::AdminFeeScaled);
    });
    ReceiptVaultClient::new(&env, &id).update_interest();
    assert_eq!(totals(&env, &id), (1_100_000_000, 1_100_000_000_000_000_000, 0, 0, 31_536_000));
    env.as_contract(&id, || {
        assert!(!env.storage().persistent().has(&DataKey::InterestModel));
        assert!(!env.storage().persistent().has(&DataKey::ReserveFactorScaled));
        assert!(!env.storage().persistent().has(&DataKey::AdminFeeScaled));
    });
}

#[test]
fn storage_scoped_failed_accrual_rolls_back_data_and_ttl() {
    let (env, id) = fixture();
    // Invalid factors fail after initialization/interest TTL reads, before commit.
    env.as_contract(&id, || env.storage().persistent().set(&DataKey::ReserveFactorScaled, &1_000_001u128));
    let before = env.to_ledger_snapshot();
    assert!(ReceiptVaultClient::new(&env, &id).try_update_interest().is_err());
    assert_eq!(env.to_ledger_snapshot().ledger_entries, before.ledger_entries);
}

#[test]
fn storage_scoped_flash_reentry_guard_still_rejects_with_rollback() {
    let (env, id) = fixture();
    let user = Address::generate(&env);
    env.as_contract(&id, || env.storage().persistent().set(&DataKey::FlashLoanActive, &true));
    let before = env.to_ledger_snapshot();
    // No external auth is supplied: the existing flash-loan guard must reject
    // before user authorization or token movement, with no accounting changes.
    assert!(ReceiptVaultClient::new(&env, &id).try_withdraw(&user, &1u128).is_err());
    assert_eq!(env.to_ledger_snapshot().ledger_entries, before.ledger_entries);
}

#[test]
fn storage_scoped_missing_underlying_update_remains_noop() {
    let (env, id) = fixture();
    env.as_contract(&id, || env.storage().persistent().remove(&DataKey::UnderlyingToken));
    let before = env.to_ledger_snapshot();
    ReceiptVaultClient::new(&env, &id).update_interest();
    assert_eq!(env.to_ledger_snapshot().ledger_entries, before.ledger_entries);
}

#[soroban_sdk::contract]
struct StorageRepayer;
#[soroban_sdk::contractimpl]
impl StorageRepayer {
    pub fn repay(env: Env, vault: Address, payer: Address, amount: u128) -> u128 {
        // A real native contract call supplies the configured controller's own
        // authorization. Only the external payer is narrowly mocked below.
        ReceiptVaultClient::new(&env, &vault).repay_full_for_margin(&7, &payer, &amount)
    }
}

#[test]
fn storage_scoped_nonzero_cap_full_repay_renews_principal_near_expiry() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    let (env, id) = fixture();
    let admin = Address::generate(&env);
    let payer = Address::generate(&env);
    let asset = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let controller = env.register(StorageRepayer, ());
    let amount = 1_100_000_000u128;
    env.mock_auths(&[MockAuth { address: &admin, invoke: &MockAuthInvoke {
        contract: &asset, fn_name: "mint", args: (payer.clone(), amount as i128).into_val(&env), sub_invokes: &[],
    }}]);
    token::StellarAssetClient::new(&env, &asset).mint(&payer, &(amount as i128));
    env.as_contract(&id, || {
        let p = env.storage().persistent();
        p.set(&DataKey::UnderlyingToken, &asset);
        p.set(&DataKey::MarginController, &controller);
        p.set(&DataKey::BorrowCap, &5_000_000_000u128);
        p.set(&DataKey::ManagedCash, &0u128);
        p.set(&DataKey::TotalBorrowPrincipal, &1_000_000_000u128);
        p.set(&DataKey::MarginHasBorrowed(7), &true);
        p.set(&DataKey::MarginBorrowPrincipal(7), &1_000_000_000u128);
        p.set(&DataKey::MarginBorrowSnapshots(7), &BorrowSnapshot {
            principal: 1_000_000_000, interest_index: 1_000_000_000_000_000_000,
        });
        env.storage().instance().set(&DataKey::MinCashReserveBps, &10_000u32);
        env.storage().instance().set(&DataKey::CashBorrowingEnabled, &false);
        assert!(p.get_ttl(&DataKey::TotalBorrowPrincipal) < 10);
    });
    env.mock_auths(&[MockAuth { address: &payer, invoke: &MockAuthInvoke {
        contract: &id, fn_name: "repay_full_for_margin",
        args: (7u64, payer.clone(), amount).into_val(&env),
        sub_invokes: &[MockAuthInvoke {
            contract: &asset, fn_name: "transfer",
            args: (payer.clone(), id.clone(), amount as i128).into_val(&env), sub_invokes: &[],
        }],
    }}]);
    assert_eq!(StorageRepayerClient::new(&env, &controller).repay(&id, &payer, &amount), amount);
    env.as_contract(&id, || {
        let p = env.storage().persistent();
        assert_eq!(p.get::<_,u128>(&DataKey::TotalBorrowPrincipal), Some(0));
        assert!(p.get_ttl(&DataKey::TotalBorrowPrincipal) >= 999_999);
        assert_eq!(p.get::<_,u128>(&DataKey::TotalBorrowed), Some(0));
        assert_eq!(p.get::<_,u128>(&DataKey::ManagedCash), Some(amount));
    });
    assert_eq!(token::Client::new(&env, &asset).balance(&payer), 0);
    assert_eq!(ReceiptVaultClient::new(&env, &id).get_margin_borrow_balance(&7), 0);
}

fn funded_cash(env: &Env, id: &Address, amount: i128) -> Address {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    let admin = Address::generate(env);
    let asset = env.register_stellar_asset_contract_v2(admin.clone()).address();
    env.mock_auths(&[MockAuth { address: &admin, invoke: &MockAuthInvoke {
        contract: &asset, fn_name: "mint", args: (id.clone(), amount).into_val(env), sub_invokes: &[],
    }}]);
    token::StellarAssetClient::new(env, &asset).mint(id, &amount);
    env.as_contract(id, || {
        let p = env.storage().persistent();
        p.set(&DataKey::UnderlyingToken, &asset);
        p.set(&DataKey::ManagedCash, &(amount as u128));
        p.set(&DataKey::TotalDeposited, &(amount as u128));
    });
    asset
}

#[soroban_sdk::contractimpl]
impl StorageRepayer {
    pub fn borrow(env: Env, vault: Address, receiver: Address, amount: u128) {
        ReceiptVaultClient::new(&env, &vault).borrow_for_margin(&8, &receiver, &amount);
    }
}

#[test]
fn storage_scoped_elapsed_withdraw_actual_callsite() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    let (env, id) = fixture();
    let user = Address::generate(&env);
    let asset = funded_cash(&env, &id, 2_000_000_000);
    env.as_contract(&id, || {
        TokenBase::mint(&env, &user, 3_000_000_000);
        env.storage().persistent().set(&DataKey::HasBorrowed(user.clone()), &false);
    });
    env.mock_auths(&[MockAuth { address: &user, invoke: &MockAuthInvoke {
        contract: &id, fn_name: "withdraw", args: (user.clone(),100_000_000u128).into_val(&env), sub_invokes: &[],
    }}]);
    ReceiptVaultClient::new(&env, &id).withdraw(&user, &100_000_000);
    assert_eq!(totals(&env, &id), (1_100_000_000,1_100_000_000_000_000_000,10_000_000,5_000_000,31_536_000));
    // (2B cash + 1.1B debt - 15M protocol claims) / 3B supply.
    assert_eq!(token::Client::new(&env, &asset).balance(&user), 102_833_300);
    assert_eq!(ReceiptVaultClient::new(&env, &id).get_ptoken_balance(&user), 2_900_000_000);
}

#[test]
fn storage_scoped_elapsed_margin_borrow_actual_callsite() {
    let (env, id) = fixture();
    let asset = funded_cash(&env, &id, 2_000_000_000);
    let controller = env.register(StorageRepayer, ());
    let holder = Address::generate(&env);
    env.as_contract(&id, || {
        let p = env.storage().persistent();
        p.set(&DataKey::MarginController, &controller);
        p.set(&DataKey::RatesReady, &true);
        p.set(&DataKey::MarginHasBorrowed(8), &false);
        TokenBase::mint(&env, &holder, 3_000_000_000);
        env.storage().instance().set(&DataKey::MinCashReserveBps, &1_000u32);
        // Explicit TEST ONLY configuration; production borrowing remains off.
        env.storage().instance().set(&DataKey::CashBorrowingEnabled, &true);
    });
    env.set_auths(&[]);
    // The actual native controller caller provides its own authorization.
    StorageRepayerClient::new(&env, &controller).borrow(&id, &controller, &100_000_000);
    assert_eq!(totals(&env, &id), (1_200_000_000,1_100_000_000_000_000_000,10_000_000,5_000_000,31_536_000));
    assert_eq!(token::Client::new(&env, &asset).balance(&controller), 100_000_000);
    assert_eq!(ReceiptVaultClient::new(&env, &id).get_margin_borrow_balance(&8), 100_000_000);
}

#[test]
fn storage_scoped_failure_after_accrual_writes_rolls_back() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    let (env, id) = fixture();
    let user = Address::generate(&env);
    funded_cash(&env, &id, 2_000_000_000);
    // No pTokens: withdraw accrues successfully, then rejects insufficient
    // pTokens after its rewards checkpoint. Exact auth cannot be the failure.
    let numerical_before = totals(&env, &id);
    env.mock_auths(&[MockAuth { address: &user, invoke: &MockAuthInvoke {
        contract: &id, fn_name: "withdraw", args: (user.clone(),1u128).into_val(&env), sub_invokes: &[],
    }}]);
    let before = env.to_ledger_snapshot();
    assert!(ReceiptVaultClient::new(&env, &id).try_withdraw(&user, &1).is_err());
    // Capture diagnostics before a subsequent inspection contract context resets
    // the host's last-invocation event buffer.
    let events = env.host().get_events().unwrap();
    assert_eq!(totals(&env, &id), numerical_before);
    assert_eq!(env.to_ledger_snapshot().ledger_entries, before.ledger_entries);
    // The host event buffer retains the emitted failed-call accrual event even though
    // its numerical writes and event are rolled back. This establishes that
    // the intended failure was reached AFTER a real nonzero accrual.
    let accrued_then_rolled_back = events.0.iter().any(|event| {
        use soroban_sdk::xdr::{ContractEventBody, ContractEventType, ScVal};
        if !event.failed_call || event.event.type_ != ContractEventType::Contract { return false; }
        let ContractEventBody::V0(body) = &event.event.body;
        if !body.topics.iter().any(|topic| matches!(topic, ScVal::Symbol(s) if s.to_utf8_string().unwrap() == "accrue_interest")) { return false; }
        let ScVal::Map(Some(fields)) = &body.data else { return false; };
        fields.iter().any(|field| matches!((&field.key,&field.val),
            (ScVal::Symbol(name),ScVal::U128(amount)) if name.to_utf8_string().unwrap() == "interest_accumulated" && amount.hi == 0 && amount.lo == 100_000_000))
    });
    assert!(accrued_then_rolled_back, "nonzero failed-call accrual event missing: {:?}", events);
}

extern crate std;
std::thread_local! {
    // Native observation only: no contract storage, no authorization bypass and
    // no contribution to measured execution evidence or production behavior.
    static REENTRY_ERROR: std::cell::RefCell<Option<soroban_sdk::Error>> = const { std::cell::RefCell::new(None) };
}
#[soroban_sdk::contracterror]
#[derive(Copy,Clone,Debug,Eq,PartialEq)]
#[repr(u32)]
enum StorageAccountError { RefusedReentry = 71 }
#[soroban_sdk::contract]
struct StorageReentryAccount;
#[soroban_sdk::contractimpl]
impl soroban_sdk::auth::CustomAccountInterface for StorageReentryAccount {
    type Signature = Val;
    type Error = StorageAccountError;
    fn __check_auth(env: Env, _payload: soroban_sdk::crypto::Hash<32>, _signature: Val,
                    contexts: Vec<soroban_sdk::auth::Context>) -> Result<(), StorageAccountError> {
        let soroban_sdk::auth::Context::Contract(context) = contexts.get(0).unwrap() else { panic!("contract context required") };
        let nested = env.try_invoke_contract::<(),soroban_sdk::Error>(
            &context.contract, &Symbol::new(&env,"update_interest"), Vec::new(&env));
        if let Err(Ok(error)) = nested {
            REENTRY_ERROR.with(|observed| *observed.borrow_mut() = Some(error));
        }
        Err(StorageAccountError::RefusedReentry)
    }
}

#[test]
fn storage_scoped_custom_account_check_auth_reentry_is_host_rejected() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    let (env, id) = fixture();
    let account = env.register(StorageReentryAccount, ());
    // Build one unsigned auth entry without mock_auths: set_auths invokes this
    // real custom account's __check_auth. The callback intentionally rejects;
    // this is not an account-signature acceptance proof.
    let entry: soroban_sdk::xdr::SorobanAuthorizationEntry = MockAuth {
        address: &account, invoke: &MockAuthInvoke {
            contract: &id, fn_name: "withdraw", args: (account.clone(),1u128).into_val(&env), sub_invokes: &[],
        },
    }.into();
    env.set_auths(&[entry]);
    REENTRY_ERROR.with(|observed| *observed.borrow_mut() = None);
    let before = env.to_ledger_snapshot();
    assert!(ReceiptVaultClient::new(&env, &id).try_withdraw(&account, &1).is_err());
    REENTRY_ERROR.with(|observed| assert_eq!(*observed.borrow(), Some(soroban_sdk::Error::from_type_and_code(
        soroban_sdk::xdr::ScErrorType::Context, soroban_sdk::xdr::ScErrorCode::InvalidAction))));
    assert_eq!(env.to_ledger_snapshot().ledger_entries, before.ledger_entries);
}
