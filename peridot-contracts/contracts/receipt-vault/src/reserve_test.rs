//! Native synthetic accounting fixtures, not network account-signature proofs.
extern crate std;
use crate::{DataKey, ReceiptVault, ReceiptVaultClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
    token, Address, Env, IntoVal, Val, Vec,
};
use stellar_tokens::fungible::Base as TokenBase;

// Synthetic forwarding contract exercises actual invoker authorization; no
// controller auth is mocked. This is not the compiled V3 integration proof.
#[soroban_sdk::contract]
struct ReserveBorrower;
#[soroban_sdk::contractimpl]
impl ReserveBorrower {
    pub fn locked_ptokens_in_market(_env: Env, _user: Address, _market: Address) -> u128 {
        0
    }
    pub fn prepare(env: Env, receipt: Address, owner: Address, amount: u128) {
        ReceiptVaultClient::new(&env, &receipt).cl_prepare_margin_rewards(&owner, &amount, &true);
    }
    pub fn reassign(env: Env, receipt: Address, owner: Address, recipient: Address, amount: u128) {
        ReceiptVaultClient::new(&env, &receipt)
            .cl_reassign_margin_rewards(&owner, &recipient, &amount, &true);
    }
    pub fn draw(env: Env, receipt: Address, amount: u128) {
        ReceiptVaultClient::new(&env, &receipt).borrow_for_margin(
            &1,
            &env.current_contract_address(),
            &amount,
        );
    }
}

fn measure(env: &Env, flow: &str, ok: bool) {
    let r = env.cost_estimate().resources();
    let line = std::format!(
        concat!(
            "{{\"flow\":\"{}\",\"ok\":{},\"instructions\":{},\"memory_bytes\":{},",
            "\"read_entries\":{},\"write_entries\":{},\"read_bytes\":{},\"write_bytes\":{},",
            "\"disk_read_entries\":{},\"contract_events_size_bytes\":{},",
            "\"persistent_rent_ledger_bytes\":{},\"persistent_entry_rent_bumps\":{},",
            "\"temporary_rent_ledger_bytes\":{},\"temporary_entry_rent_bumps\":{},",
            "\"scope\":\"native-synthetic-reserve-fixture\"}}\n"
        ),
        flow,
        ok,
        r.instructions,
        r.mem_bytes,
        r.memory_read_entries,
        r.write_entries,
        r.disk_read_bytes,
        r.write_bytes,
        r.disk_read_entries,
        r.contract_events_size_bytes,
        r.persistent_rent_ledger_bytes,
        r.persistent_entry_rent_bumps,
        r.temporary_rent_ledger_bytes,
        r.temporary_entry_rent_bumps
    );
    std::println!("{line}");
    if let Ok(path) = std::env::var("RESERVE_RESOURCE_REPORT") {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap()
            .write_all(line.as_bytes())
            .unwrap();
    }
}

struct F {
    env: Env,
    admin: Address,
    user: Address,
    vault: Address,
    asset: Address,
}
impl F {
    fn new() -> Self {
        let env = Env::default();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let asset = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let vault = env.register(ReceiptVault, ());
        let f = Self {
            env,
            admin,
            user,
            vault,
            asset,
        };
        f.auth(
            &f.admin,
            &f.vault,
            "initialize",
            (&f.asset, 0u128, 0u128, &f.admin).into_val(&f.env),
        );
        f.c().initialize(&f.asset, &0, &0, &f.admin);
        f.auth(
            &f.admin,
            &f.vault,
            "enable_static_rates",
            (&f.admin,).into_val(&f.env),
        );
        f.c().enable_static_rates(&f.admin);
        f
    }
    fn c(&self) -> ReceiptVaultClient<'_> {
        ReceiptVaultClient::new(&self.env, &self.vault)
    }
    fn auth(&self, who: &Address, target: &Address, method: &str, args: Vec<Val>) {
        self.env.mock_auths(&[MockAuth {
            address: who,
            invoke: &MockAuthInvoke {
                contract: target,
                fn_name: method,
                args,
                sub_invokes: &[],
            },
        }]);
    }
    fn policy(&self, bps: u32) {
        self.auth(
            &self.admin,
            &self.vault,
            "set_min_cash_reserve_bps",
            (&self.admin, bps).into_val(&self.env),
        );
        self.c().set_min_cash_reserve_bps(&self.admin, &bps);
    }
    fn enable(&self, enabled: bool) {
        self.auth(
            &self.admin,
            &self.vault,
            "set_cash_borrowing_enabled",
            (&self.admin, enabled).into_val(&self.env),
        );
        self.c().set_cash_borrowing_enabled(&self.admin, &enabled);
    }
    fn mint(&self, to: &Address, amount: i128) {
        self.auth(
            &self.admin,
            &self.asset,
            "mint",
            (to, amount).into_val(&self.env),
        );
        token::StellarAssetClient::new(&self.env, &self.asset).mint(to, &amount);
    }
    // Synthetic ledger setup is explicit: no real strategy or economic integration claim.
    fn accounting(&self, cash: u128, strategy: u128, debt: u128, claims: u128) {
        self.mint(&self.vault, cash as i128);
        self.env.as_contract(&self.vault, || {
            let p = self.env.storage().persistent();
            p.set(&DataKey::ManagedCash, &cash);
            p.set(&DataKey::HasBorrowed(self.user.clone()), &false);
            p.set(&DataKey::MarginHasBorrowed(1), &false);
            p.set(&DataKey::TotalBorrowed, &debt);
            p.set(&DataKey::TotalDeposited, &(cash + strategy + debt - claims));
            p.set(&DataKey::TotalReserves, &claims);
            if strategy > 0 {
                p.set(&DataKey::BoostedVault, &Address::generate(&self.env));
                p.set(&DataKey::BoostedUnderlyingCached, &strategy);
                p.set(&DataKey::BoostedUnderlyingUpdatedAt, &0u64);
            }
            TokenBase::mint(
                &self.env,
                &self.user,
                (cash + strategy + debt - claims) as i128,
            );
        });
    }
    fn borrow(&self, amount: u128) -> bool {
        self.auth(
            &self.user,
            &self.vault,
            "borrow",
            (&self.user, amount).into_val(&self.env),
        );
        self.env.cost_estimate().budget().reset_unlimited();
        let ok = self.c().try_borrow(&self.user, &amount).is_ok();
        if ok {
            measure(&self.env, "reserve_native_borrow", true);
        }
        ok
    }
}
#[test]
fn reserve_optional_bounded_and_authorized() {
    let f = F::new();
    assert_eq!(f.c().get_min_cash_reserve_bps(), None);
    assert!(f.c().try_set_min_cash_reserve_bps(&f.admin, &1000).is_err());
    f.policy(1000);
    assert_eq!(f.c().get_min_cash_reserve_bps(), Some(1000));
    assert!(!f.c().get_cash_borrowing_enabled());
    f.auth(
        &f.admin,
        &f.vault,
        "set_min_cash_reserve_bps",
        (&f.admin, 10_001u32).into_val(&f.env),
    );
    assert!(f
        .c()
        .try_set_min_cash_reserve_bps(&f.admin, &10_001)
        .is_err());
    f.policy(0);
    assert_eq!(f.c().get_min_cash_reserve_bps(), Some(0));
}
#[test]
fn reserve_cash_donations_and_protocol_claims_are_not_lendable() {
    let f = F::new();
    f.accounting(40_000, 60_000, 0, 0);
    f.policy(1000);
    assert_eq!(f.c().get_borrowable_cash(), 30_000);
    f.mint(&f.vault, 99_000);
    assert_eq!(f.c().get_borrowable_cash(), 30_000);
    let g = F::new();
    g.accounting(20, 0, 80, 10);
    g.policy(1000);
    assert_eq!(g.c().get_borrowable_cash(), 1); // NAV90, local unencumbered10, floor9
}
#[test]
fn reserve_debt_examples_and_rounding() {
    let f = F::new();
    f.accounting(10, 10, 80, 0);
    f.policy(1000);
    assert_eq!(f.c().get_borrowable_cash(), 0);
    let g = F::new();
    g.accounting(14, 6, 80, 0);
    g.policy(1000);
    assert_eq!(g.c().get_borrowable_cash(), 4);
    let h = F::new();
    h.accounting(11, 0, 0, 0);
    h.policy(1000);
    assert_eq!(h.c().get_borrowable_cash(), 9); // ceil(1.1)=2
}
#[test]
fn reserve_ordinary_borrow_and_durable_pause() {
    let f = F::new();
    f.accounting(100, 0, 0, 0);
    f.policy(6000);
    assert!(!f.borrow(1)); // configured market starts disabled
    f.enable(true);
    assert!(!f.borrow(41));
    assert!(f.borrow(40));
    assert_eq!(token::Client::new(&f.env, &f.asset).balance(&f.user), 40);
    assert_eq!(f.c().get_borrowable_cash(), 0);
    f.c().assert_borrowing_reserve();
    assert!(!f.borrow(1));
}
#[test]
fn reserve_withdrawal_can_cross_floor_then_complete_open_check_fails() {
    let f = F::new();
    f.accounting(100, 0, 40, 0);
    f.policy(6000);
    f.enable(true);
    f.c().assert_borrowing_reserve();
    f.auth(
        &f.user,
        &f.vault,
        "withdraw",
        (&f.user, 50u128).into_val(&f.env),
    );
    f.c().withdraw(&f.user, &50); // exits may take cash below reserve
    assert_eq!(token::Client::new(&f.env, &f.asset).balance(&f.vault), 50);
    assert_eq!(f.c().get_borrowable_cash(), 0);
    assert!(f.c().try_assert_borrowing_reserve().is_err());
    assert!(!f.borrow(1));
}
#[test]
fn reserve_margin_uses_same_floor_and_controller_auth() {
    let f = F::new();
    f.accounting(100, 0, 0, 0);
    f.policy(6000);
    f.enable(true);
    let controller = f.env.register(ReserveBorrower, ());
    // Native fixture storage models controller binding, not controller signatures.
    f.env.as_contract(&f.vault, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::MarginController, &controller)
    });
    assert!(f.c().try_borrow_for_margin(&1, &controller, &1).is_err());
    let borrower = ReserveBorrowerClient::new(&f.env, &controller);
    assert!(borrower.try_draw(&f.vault, &41).is_err());
    f.env.cost_estimate().budget().reset_unlimited();
    borrower.draw(&f.vault, &40);
    measure(&f.env, "reserve_native_margin_borrow", true);
    assert_eq!(f.c().get_borrowable_cash(), 0);
}

fn strategy_fixture() -> (F, Address, Address) {
    let f = F::new();
    let strategy = f.env.register(crate::test::MockBoostedVault, ());
    crate::test::MockBoostedVaultClient::new(&f.env, &strategy).initialize(&f.asset);
    f.auth(
        &f.admin,
        &f.vault,
        "set_boosted_vault",
        (&f.admin, &strategy).into_val(&f.env),
    );
    f.c().set_boosted_vault(&f.admin, &strategy);
    f.auth(
        &f.admin,
        &f.vault,
        "set_idle_cash_buffer_bps",
        (&f.admin, 5000u32).into_val(&f.env),
    );
    f.c().set_idle_cash_buffer_bps(&f.admin, &5000);
    f.policy(1000);
    f.enable(true);
    f.mint(&f.user, 100_000_000);
    f.env.mock_auths(&[MockAuth {
        address: &f.user,
        invoke: &MockAuthInvoke {
            contract: &f.vault,
            fn_name: "deposit",
            args: (&f.user, 100_000_000u128).into_val(&f.env),
            sub_invokes: &[MockAuthInvoke {
                contract: &f.asset,
                fn_name: "transfer",
                args: (&f.user, &f.vault, 100_000_000i128).into_val(&f.env),
                sub_invokes: &[],
            }],
        },
    }]);
    f.c().deposit(&f.user, &100_000_000);
    let controller = f.env.register(ReserveBorrower, ());
    f.env.as_contract(&f.vault, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::MarginController, &controller);
        f.env
            .storage()
            .persistent()
            .set(&DataKey::MarginHasBorrowed(1), &false);
    });
    ReserveBorrowerClient::new(&f.env, &controller).draw(&f.vault, &40_000_000);
    (f, strategy, controller)
}

#[test]
fn reserve_cash_first_withdrawal_worked_example_and_shortfall_rollback() {
    for haircut in [0i128, 3_000_000] {
        let (f, strategy, _controller) = strategy_fixture();
        assert_eq!(
            token::Client::new(&f.env, &f.asset).balance(&f.vault),
            10_000_000
        );
        assert_eq!(
            token::Client::new(&f.env, &f.asset).balance(&strategy),
            50_000_000
        );
        crate::test::MockBoostedVaultClient::new(&f.env, &strategy).set_withdraw_haircut(&haircut);
        f.auth(
            &f.user,
            &f.vault,
            "withdraw",
            (&f.user, 25_000_000u128).into_val(&f.env),
        );
        f.env.cost_estimate().budget().reset_unlimited();
        let outcome = f.c().try_withdraw(&f.user, &25_000_000);
        measure(
            &f.env,
            if haircut == 0 {
                "reserve_native_withdraw"
            } else {
                "reserve_native_withdraw_shortfall_expected_rejection"
            },
            outcome.is_ok(),
        );
        if haircut == 0 {
            assert!(outcome.is_ok());
            assert_eq!(token::Client::new(&f.env, &f.asset).balance(&f.vault), 1); // existing bounded redeem rounds strategy shares up one raw unit
            assert_eq!(
                token::Client::new(&f.env, &f.asset).balance(&strategy),
                34_999_999
            );
            assert_eq!(f.c().get_total_underlying(), 75_000_000);
            assert_eq!(f.c().get_borrowable_cash(), 0);
        } else {
            assert!(outcome.is_err());
            assert_eq!(
                token::Client::new(&f.env, &f.asset).balance(&f.vault),
                10_000_000
            );
            assert_eq!(
                token::Client::new(&f.env, &f.asset).balance(&strategy),
                50_000_000
            );
            assert_eq!(f.c().get_ptoken_balance(&f.user), 100_000_000);
        }
    }
}

#[test]
fn reserve_new_borrow_never_unwinds_strategy_and_deployment_obeys_floor() {
    let (f, strategy, controller) = strategy_fixture();
    assert!(ReserveBorrowerClient::new(&f.env, &controller)
        .try_draw(&f.vault, &1)
        .is_err());
    assert_eq!(
        token::Client::new(&f.env, &f.asset).balance(&strategy),
        50_000_000
    );
    f.auth(
        &f.admin,
        &f.vault,
        "rebalance_idle_cash",
        (&f.admin,).into_val(&f.env),
    );
    f.env.cost_estimate().budget().reset_unlimited();
    f.c().rebalance_idle_cash(&f.admin);
    measure(&f.env, "reserve_native_rebalance", true);
    assert_eq!(
        token::Client::new(&f.env, &f.asset).balance(&f.vault),
        10_000_000
    );
    assert_eq!(
        token::Client::new(&f.env, &f.asset).balance(&strategy),
        50_000_000
    );
}

// Direct fixture setup isolates receipt configuration/context behavior. The
// controller is an actual native forwarding contract; its auth is never mocked.
fn custody_context_fixture() -> (F, Address) {
    let f = F::new();
    let controller = f.env.register(ReserveBorrower, ());
    f.env.as_contract(&f.vault, || {
        f.env.storage().instance().set(&DataKey::ClCustody, &true);
        f.env
            .storage()
            .persistent()
            .set(&DataKey::MarginController, &controller);
    });
    f.env.mock_auths(&[]);
    (f, controller)
}

#[test]
fn cl_custody_controller_is_immutable_even_for_admin() {
    let (f, controller) = custody_context_fixture();
    assert!(f
        .c()
        .try_set_margin_controller(&f.admin, &Some(controller.clone()))
        .is_err());
    for proposed in [
        Some(controller.clone()),
        None,
        Some(Address::generate(&f.env)),
    ] {
        f.auth(
            &f.admin,
            &f.vault,
            "set_margin_controller",
            (&f.admin, &proposed).into_val(&f.env),
        );
        let result = f.c().try_set_margin_controller(&f.admin, &proposed);
        assert_eq!(result.is_ok(), proposed == Some(controller.clone()));
        assert_eq!(f.c().get_margin_controller(), Some(controller.clone()));
    }
}

#[test]
fn cl_custody_orphan_context_replace_requires_controller_and_later_ledger() {
    let (f, controller) = custody_context_fixture();
    let c = ReserveBorrowerClient::new(&f.env, &controller);
    let recipient = Address::generate(&f.env);
    c.prepare(&f.vault, &f.user, &7);
    assert!(c.try_reassign(&f.vault, &f.user, &recipient, &8).is_err());
    assert!(c.try_prepare(&f.vault, &f.user, &8).is_err());
    f.env.ledger().with_mut(|ledger| ledger.sequence_number += 1);
    assert!(f
        .c()
        .try_cl_prepare_margin_rewards(&f.user, &9, &true)
        .is_err());
    // Authorization failure leaves the original orphan untouched.
    f.env.as_contract(&f.vault, || {
        let old: crate::storage::ClCustodyContext = f
            .env
            .storage()
            .instance()
            .get(&DataKey::ClCustodyContext)
            .unwrap();
        assert_eq!(old.ptokens, 7);
        assert!(old.withdrawal);
    });
    c.reassign(&f.vault, &f.user, &recipient, &8);
    f.env.as_contract(&f.vault, || {
        let new: crate::storage::ClCustodyContext = f
            .env
            .storage()
            .instance()
            .get(&DataKey::ClCustodyContext)
            .unwrap();
        assert_eq!(new.ptokens, 8);
        assert_eq!(new.recipient, recipient);
        assert_eq!(new.ledger, f.env.ledger().sequence());
        assert!(!new.withdrawal);
    });
    assert!(c.try_prepare(&f.vault, &f.user, &9).is_err());
    f.env.ledger().with_mut(|ledger| ledger.sequence_number += 1);
    c.prepare(&f.vault, &f.user, &9);
}

// Actual native receipt + SimplePeridottroller; SAC and callback contracts execute
// their own authorization. Generated external admin/user auth is narrowly mocked.
// This proves native behavior, not compiled resource or real-signature acceptance.
fn flash_guard_fixture() -> (F, Address, Address) {
    use simple_peridottroller::{SimplePeridottroller, SimplePeridottrollerClient};
    let f = F::new();
    let controller = f.env.register(SimplePeridottroller, ());
    let c = SimplePeridottrollerClient::new(&f.env, &controller);
    f.auth(&f.admin, &controller, "initialize", (&f.admin,).into_val(&f.env));
    c.initialize(&f.admin);
    f.auth(&f.admin, &controller, "add_market", (&f.vault,).into_val(&f.env));
    c.add_market(&f.vault);
    f.auth(&f.admin, &f.vault, "set_peridottroller", (&controller,).into_val(&f.env));
    f.c().set_peridottroller(&controller);
    let receiver = f.env.register(crate::test::FlashLoanRepayer, ());
    crate::test::FlashLoanRepayerClient::new(&f.env, &receiver).configure(&f.asset);
    flash_guard_supply(&f, 1_000_000);
    (f, controller, receiver)
}

fn flash_guard_supply(f: &F, amount: u128) {
    f.mint(&f.user, amount as i128);
    f.env.mock_auths(&[MockAuth {
        address: &f.user,
        invoke: &MockAuthInvoke {
            contract: &f.vault,
            fn_name: "deposit",
            args: (&f.user, amount).into_val(&f.env),
            sub_invokes: &[MockAuthInvoke {
                contract: &f.asset,
                fn_name: "transfer",
                args: (&f.user, &f.vault, amount as i128).into_val(&f.env),
                sub_invokes: &[],
            }],
        },
    }]);
    f.c().deposit(&f.user, &amount);
}

fn flash_guard_pause(f: &F, controller: &Address, paused: bool) {
    f.auth(&f.admin, controller, "set_pause_borrow", (&f.vault, paused).into_val(&f.env));
    simple_peridottroller::SimplePeridottrollerClient::new(&f.env, controller)
        .set_pause_borrow(&f.vault, &paused);
}

fn flash_guard_attempt(f: &F, receiver: &Address, amount: u128, expected: bool) {
    let data = soroban_sdk::Bytes::new(&f.env);
    f.auth(&f.user, &f.vault, "flash_loan", (&f.user, receiver, amount, &data).into_val(&f.env));
    let cash = token::Client::new(&f.env, &f.asset).balance(&f.vault);
    let receiver_cash = token::Client::new(&f.env, &f.asset).balance(receiver);
    let shares = f.c().get_ptoken_balance(&f.user);
    let reserves = f.c().get_total_reserves();
    let before = f.env.to_snapshot().ledger;
    let outcome = f.c().try_flash_loan(&f.user, receiver, &amount, &data);
    assert_eq!(outcome.is_ok(), expected, "flash outcome: {outcome:?}");
    if !expected {
        // Includes managed cash, flash-active marker, interest and controller state.
        assert_eq!(f.env.to_snapshot().ledger, before, "rejection must roll back all ledger state");
    }
    assert_eq!(token::Client::new(&f.env, &f.asset).balance(&f.vault), cash);
    assert_eq!(token::Client::new(&f.env, &f.asset).balance(receiver), receiver_cash);
    assert_eq!(f.c().get_ptoken_balance(&f.user), shares);
    assert_eq!(f.c().get_total_reserves(), reserves);
}

#[test]
fn reserve_flash_durable_disabled_survives_controller_pause_expiry_and_lift() {
    let (f, controller, receiver) = flash_guard_fixture();
    f.policy(1000);
    f.enable(false);
    flash_guard_pause(&f, &controller, true);
    let c = simple_peridottroller::SimplePeridottrollerClient::new(&f.env, &controller);
    for phase in 0..3 {
        match phase {
            0 => assert!(c.is_borrow_paused(&f.vault)),
            1 => {
                f.env.ledger().with_mut(|l| l.timestamp += simple_peridottroller::MAX_PAUSE_DURATION_SECS + 1);
                assert!(!c.is_borrow_paused(&f.vault));
            }
            _ => {
                flash_guard_pause(&f, &controller, true);
                flash_guard_pause(&f, &controller, false);
                assert!(!c.is_borrow_paused(&f.vault));
            }
        }
        assert!(!f.c().get_cash_borrowing_enabled());
        flash_guard_attempt(&f, &receiver, 100, false);
        // Supply and principal redemption remain available under borrowing-only pause.
        let shares = f.c().get_ptoken_balance(&f.user);
        flash_guard_supply(&f, 200);
        assert_eq!(f.c().get_ptoken_balance(&f.user), shares + 200);
        f.auth(&f.user, &f.vault, "withdraw", (&f.user, 200u128).into_val(&f.env));
        let cash = token::Client::new(&f.env, &f.asset).balance(&f.user);
        f.c().withdraw(&f.user, &200);
        assert_eq!(f.c().get_ptoken_balance(&f.user), shares);
        assert_eq!(token::Client::new(&f.env, &f.asset).balance(&f.user), cash + 200);
    }
    // Same callback/auth succeeds once the actual durable switch is enabled.
    f.enable(true);
    flash_guard_attempt(&f, &receiver, 100, true);
}

#[test]
fn reserve_flash_enabled_obeys_floor_and_controller_pause() {
    let (f, controller, receiver) = flash_guard_fixture();
    f.policy(6000);
    f.enable(true);
    flash_guard_attempt(&f, &receiver, 400_001, false);
    flash_guard_attempt(&f, &receiver, 400_000, true);
    flash_guard_pause(&f, &controller, true);
    flash_guard_attempt(&f, &receiver, 1, false);
    flash_guard_pause(&f, &controller, false);
    flash_guard_attempt(&f, &receiver, 1, true);
}

#[test]
fn reserve_flash_unconfigured_legacy_repayment_still_succeeds() {
    let (f, _controller, receiver) = flash_guard_fixture();
    assert_eq!(f.c().get_min_cash_reserve_bps(), None);
    assert!(!f.c().get_cash_borrowing_enabled());
    flash_guard_attempt(&f, &receiver, 900_000, true);
}
