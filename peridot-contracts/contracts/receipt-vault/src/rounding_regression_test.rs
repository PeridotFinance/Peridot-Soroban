//! Medium2 native economic regressions. External test actors have narrow auth;
//! no blanket auth, synthetic NAV injection, real-account or resource-bound claim.
extern crate std;
use crate::{ReceiptVault, ReceiptVaultClient};
use soroban_sdk::{testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke}, token, Address, Env, IntoVal, Val, Vec};

struct R { e: Env, admin: Address, incumbent: Address, entrant: Address, asset: Address, vault: Address }
impl R {
    fn new() -> Self {
        Self::with_rate(1_000_000)
    }
    fn with_rate(borrow_rate: u128) -> Self {
        let e = Env::default();
        let admin = Address::generate(&e);
        let incumbent = Address::generate(&e);
        let entrant = Address::generate(&e);
        let asset = e.register_stellar_asset_contract_v2(admin.clone()).address();
        let vault = e.register(ReceiptVault, ());
        let f = Self { e, admin, incumbent, entrant, asset, vault };
        f.auth(&f.admin, &f.vault, "initialize", (&f.asset, 0u128, borrow_rate, &f.admin).into_val(&f.e));
        f.c().initialize(&f.asset, &0, &borrow_rate, &f.admin);
        if borrow_rate > 0 {
            f.auth(&f.admin, &f.vault, "enable_static_rates", (&f.admin,).into_val(&f.e));
            f.c().enable_static_rates(&f.admin);
        }
        f
    }
    fn seed(&self, amount: u128) -> u128 {
        self.mint(&self.admin, amount as i128);
        self.e.mock_auths(&[MockAuth { address: &self.admin, invoke: &MockAuthInvoke {
            contract: &self.vault, fn_name: "seed_minimum_supply", args: (&self.admin, amount).into_val(&self.e),
            sub_invokes: &[MockAuthInvoke { contract: &self.asset, fn_name: "transfer", args: (&self.admin, &self.vault, amount as i128).into_val(&self.e), sub_invokes: &[] }],
        } }]);
        self.c().seed_minimum_supply(&self.admin, &amount)
    }
    fn c(&self) -> ReceiptVaultClient<'_> { ReceiptVaultClient::new(&self.e, &self.vault) }
    fn auth(&self, who: &Address, target: &Address, name: &str, args: Vec<Val>) {
        self.e.mock_auths(&[MockAuth { address: who, invoke: &MockAuthInvoke { contract: target, fn_name: name, args, sub_invokes: &[] } }]);
    }
    fn mint(&self, to: &Address, amount: i128) {
        self.auth(&self.admin, &self.asset, "mint", (to, amount).into_val(&self.e));
        token::StellarAssetClient::new(&self.e, &self.asset).mint(to, &amount);
    }
    fn fund_call(&self, who: &Address, name: &str, amount: u128) {
        self.e.mock_auths(&[MockAuth { address: who, invoke: &MockAuthInvoke {
            contract: &self.vault, fn_name: name, args: (who, amount).into_val(&self.e),
            sub_invokes: &[MockAuthInvoke { contract: &self.asset, fn_name: "transfer", args: (who, &self.vault, amount as i128).into_val(&self.e), sub_invokes: &[] }],
        } }]);
        match name { "deposit" => self.c().deposit(who, &amount), "repay" => self.c().repay(who, &amount), _ => panic!("unsupported fixture call") }
    }
    fn deposit(&self, who: &Address, amount: u128) { self.mint(who, amount as i128); self.fund_call(who, "deposit", amount); }
    fn withdraw(&self, who: &Address, amount: u128) {
        self.auth(who, &self.vault, "withdraw", (who, amount).into_val(&self.e));
        self.c().withdraw(who, &amount);
    }
    fn balance(&self, who: &Address) -> i128 { token::Client::new(&self.e, &self.asset).balance(who) }
}

#[test]
fn medium2_direct_donation_minimum_supply_last_exit_next_minter_no_capture() {
    for incumbent_first in [true, false] {
        let f = R::new();
        f.seed(1000);
        f.deposit(&f.incumbent, 1);
        f.mint(&f.incumbent, 999);
        f.auth(&f.incumbent, &f.asset, "transfer", (&f.incumbent, &f.vault, 999i128).into_val(&f.e));
        token::Client::new(&f.e, &f.asset).transfer(&f.incumbent, &f.vault, &999);
        assert_eq!(f.c().get_total_underlying(), 1001);
        assert_eq!(f.c().get_exchange_rate(), 1_000_000);
        f.deposit(&f.entrant, 1500);
        assert_eq!(f.c().get_ptoken_balance(&f.entrant), 1500);
        if incumbent_first { f.withdraw(&f.incumbent, 1); f.withdraw(&f.entrant, 1500); }
        else { f.withdraw(&f.entrant, 1500); f.withdraw(&f.incumbent, 1); }
        assert_eq!(f.balance(&f.incumbent), 1);
        assert_eq!(f.balance(&f.entrant), 1500);
        assert_eq!(f.c().get_total_underlying(), 1000);
        assert_eq!(f.c().get_total_ptokens(), 1000);
        assert_eq!(f.balance(&f.vault), 1999);
        // An entrant front-running the next legitimate deposit cannot capture donations.
        f.deposit(&f.incumbent, 1);
        f.deposit(&f.entrant, 7);
        f.withdraw(&f.incumbent, 1);
        f.withdraw(&f.entrant, 7);
        assert_eq!(f.balance(&f.incumbent), 2);
        assert_eq!(f.balance(&f.entrant), 1507);
        assert_eq!(f.balance(&f.vault), 1999);
    }
}

#[test]
fn medium2_seeded_public_interest_uses_exact_ratio() {
    let f = R::new();
    f.seed(1000);
    // Nonzero debt exists only in this explicitly enabled test configuration.
    f.auth(&f.admin, &f.vault, "set_cash_borrowing_enabled", (&f.admin, true).into_val(&f.e));
    f.c().set_cash_borrowing_enabled(&f.admin, &true);
    f.deposit(&f.incumbent, 3);
    f.auth(&f.incumbent, &f.vault, "borrow", (&f.incumbent, 1u128).into_val(&f.e));
    f.c().borrow(&f.incumbent, &1);
    f.e.ledger().with_mut(|l| l.timestamp += 365 * 24 * 60 * 60);
    f.c().update_interest();
    assert_eq!(f.c().get_user_borrow_balance(&f.incumbent), 2);
    f.mint(&f.incumbent, 1);
    f.fund_call(&f.incumbent, "repay", 2);
    assert_eq!(f.c().get_total_borrowed(), 0);
    assert_eq!(f.c().get_total_underlying(), 1004);
    assert_eq!(f.c().get_total_ptokens(), 1003);
    let amount = 100_000_000u128;
    let exact_mint = amount * 1003 / 1004;
    let rounded_quote_mint = amount * 1_000_000 / f.c().get_exchange_rate();
    assert!(rounded_quote_mint > exact_mint);
    f.deposit(&f.entrant, amount);
    assert_eq!(f.c().get_ptoken_balance(&f.entrant), exact_mint);
    let expected_incumbent = 3 * f.c().get_total_underlying() / f.c().get_total_ptokens();
    f.withdraw(&f.incumbent, 3);
    assert_eq!(f.balance(&f.incumbent), expected_incumbent as i128);
    let expected_entrant = exact_mint * f.c().get_total_underlying() / f.c().get_total_ptokens();
    f.withdraw(&f.entrant, exact_mint);
    assert_eq!(f.balance(&f.entrant), expected_entrant as i128);
    assert!(expected_entrant <= amount);
    assert_eq!(f.c().get_total_ptokens(), 1000);
    assert_eq!(f.c().get_ptoken_balance(&f.vault), 1000);
}

#[test]
fn medium2_fresh_cash_requires_explicit_funded_immutable_seed() {
    let f = R::new();
    assert_eq!(f.c().get_min_cash_reserve_bps(), Some(0));
    assert_eq!(f.c().get_minimum_supply(), 0);
    f.mint(&f.incumbent, 10);
    f.e.mock_auths(&[MockAuth { address: &f.incumbent, invoke: &MockAuthInvoke {
        contract: &f.vault, fn_name: "deposit", args: (&f.incumbent, 10u128).into_val(&f.e),
        sub_invokes: &[MockAuthInvoke { contract: &f.asset, fn_name: "transfer",
            args: (&f.incumbent, &f.vault, 10i128).into_val(&f.e), sub_invokes: &[] }],
    } }]);
    let before_unseeded_deposit = f.e.to_snapshot().ledger;
    assert!(f.c().try_deposit(&f.incumbent, &10).is_err());
    assert_eq!(f.e.to_snapshot().ledger, before_unseeded_deposit);
    assert!(f.c().try_seed_minimum_supply(&f.admin, &1000).is_err());
    f.auth(&f.entrant, &f.vault, "seed_minimum_supply", (&f.entrant, 1000u128).into_val(&f.e));
    assert!(f.c().try_seed_minimum_supply(&f.entrant, &1000).is_err());
    f.auth(&f.admin, &f.vault, "seed_minimum_supply", (&f.admin, 999u128).into_val(&f.e));
    assert!(f.c().try_seed_minimum_supply(&f.admin, &999).is_err());
    assert_eq!(f.seed(1000), 1000);
    assert_eq!(f.balance(&f.admin), 0);
    assert_eq!(f.c().get_ptoken_balance(&f.admin), 0);
    assert_eq!(f.c().get_ptoken_balance(&f.vault), 1000);
    assert_eq!(f.c().get_minimum_supply(), 1000);
    f.auth(&f.admin, &f.vault, "seed_minimum_supply", (&f.admin, 2000u128).into_val(&f.e));
    assert!(f.c().try_seed_minimum_supply(&f.admin, &2000).is_err());
    // Ordinary deposits are separately funded and entirely owned by their depositor.
    f.deposit(&f.incumbent, 1234);
    assert_eq!(f.c().get_ptoken_balance(&f.incumbent), 1234);
    f.withdraw(&f.incumbent, 1234);
    assert_eq!(f.balance(&f.incumbent), 1244);
    assert_eq!(f.c().get_total_ptokens(), 1000);
    assert_eq!(f.c().get_total_underlying(), 1000);
    assert_eq!(f.balance(&f.vault), 1000);
    f.deposit(&f.entrant, 7);
    f.withdraw(&f.entrant, 7);
    assert_eq!(f.c().get_total_ptokens(), 1000);
    assert_eq!(f.c().get_minimum_supply(), 1000);
    std::println!("MEDIUM2_SEED locked_assets=1000 locked_raw_ptokens=1000 depositor_confiscated=0");
}

// Synthetic seizure-context issuer, with actual contract-invoker auth. This
// isolates the immutable-share protection, not controller liquidation pricing.
#[soroban_sdk::contract]
struct SeedSeizer;
#[soroban_sdk::contractimpl]
impl SeedSeizer {
    pub fn take(e: Env, receipt: Address, owner: Address, recipient: Address, amount: u128) {
        ReceiptVaultClient::new(&e, &receipt).seize(&owner, &recipient, &amount, &Some(crate::SeizeContext {
            liquidity: 0, shortfall: 1, max_redeem_ptokens: amount, seize_ptokens: amount,
            fee_recipient: None, fee_ptokens: 0, expires_at: e.ledger().timestamp() + 10,
        }));
    }
}

#[test]
fn medium2_locked_seed_cannot_be_seized_and_cap_is_enforced() {
    let f = R::new();
    f.auth(&f.admin, &f.vault, "set_supply_cap", (999u128,).into_val(&f.e));
    f.c().set_supply_cap(&999);
    f.auth(&f.admin, &f.vault, "seed_minimum_supply", (&f.admin, 1000u128).into_val(&f.e));
    assert!(f.c().try_seed_minimum_supply(&f.admin, &1000).is_err());
    assert_eq!(f.c().get_minimum_supply(), 0);
    f.auth(&f.admin, &f.vault, "set_supply_cap", (10_000u128,).into_val(&f.e));
    f.c().set_supply_cap(&10_000);
    // An untracked donation before seed remains excluded from both seed pricing and NAV.
    f.mint(&f.vault, 99);
    assert_eq!(f.seed(1000), 1000);
    assert_eq!(f.c().get_total_underlying(), 1000);
    assert_eq!(f.balance(&f.vault), 1099);
    f.deposit(&f.incumbent, 10);
    let controller = f.e.register(SeedSeizer, ());
    // Explicit synthetic controller binding: the context issuer is deliberately
    // not a price/controller implementation. Its invoker authorization is real.
    f.e.as_contract(&f.vault, || f.e.storage().persistent().set(&crate::DataKey::Peridottroller, &controller));
    f.e.mock_auths(&[]);
    let seizer = SeedSeizerClient::new(&f.e, &controller);
    let before = f.e.to_snapshot().ledger;
    assert!(seizer.try_take(&f.vault, &f.vault, &f.entrant, &1).is_err());
    assert_eq!(f.e.to_snapshot().ledger, before);
    assert_eq!(f.c().get_ptoken_balance(&f.vault), 1000);
    // The same real nested authorization can seize an ordinary holder's shares.
    seizer.take(&f.vault, &f.incumbent, &f.entrant, &1);
    assert_eq!(f.c().get_ptoken_balance(&f.entrant), 1);
    assert_eq!(f.c().get_ptoken_balance(&f.incumbent), 9);
    assert_eq!(f.c().get_ptoken_balance(&f.vault), 1000);
}

#[test]
fn medium1_missing_caps_fail_closed_without_blocking_supply_only_exit() {
    use simple_peridottroller::{SimplePeridottroller, SimplePeridottrollerClient};
    let f = R::with_rate(0);
    let controller = f.e.register(SimplePeridottroller, ());
    let c = SimplePeridottrollerClient::new(&f.e, &controller);
    f.auth(&f.admin, &controller, "initialize", (&f.admin,).into_val(&f.e));
    c.initialize(&f.admin);
    f.auth(&f.admin, &controller, "add_market", (&f.vault,).into_val(&f.e));
    c.add_market(&f.vault);
    f.auth(&f.admin, &f.vault, "set_peridottroller", (&controller,).into_val(&f.e));
    f.c().set_peridottroller(&controller);
    f.auth(&f.admin, &f.vault, "set_min_cash_reserve_bps", (&f.admin, 10000u32).into_val(&f.e));
    f.c().set_min_cash_reserve_bps(&f.admin, &10000);
    assert!(!f.c().get_cash_borrowing_enabled());
    assert_eq!(f.seed(1000), 1000);
    f.deposit(&f.incumbent, 100);
    // Construct truly absent configuration, NOT a simulation of archival.
    // Host29 TTL-only restoration is tested separately in the compiled fixture.
    f.e.as_contract(&f.vault, || {
        f.e.storage().persistent().remove(&crate::DataKey::SupplyCap);
        f.e.storage().persistent().remove(&crate::DataKey::BorrowCap);
    });
    f.mint(&f.entrant, 10);
    f.auth(&f.entrant, &f.vault, "deposit", (&f.entrant, 10u128).into_val(&f.e));
    let before = f.e.to_snapshot().ledger;
    assert!(f.c().try_deposit(&f.entrant, &10).is_err());
    assert_eq!(f.e.to_snapshot().ledger, before);
    f.auth(&f.incumbent, &f.vault, "borrow", (&f.incumbent, 1u128).into_val(&f.e));
    let before = f.e.to_snapshot().ledger;
    assert!(f.c().try_borrow(&f.incumbent, &1).is_err());
    assert_eq!(f.e.to_snapshot().ledger, before);
    f.e.ledger().with_mut(|l| l.timestamp += 4 * 24 * 60 * 60);
    f.withdraw(&f.incumbent, 100);
    assert_eq!(f.balance(&f.incumbent), 100);
    assert_eq!(f.c().get_total_borrowed(), 0);
    assert_eq!(f.c().get_total_underlying(), 1000);
    assert_eq!(f.c().get_total_ptokens(), 1000);
    assert_eq!(f.c().get_ptoken_balance(&f.vault), 1000);
    assert!(!f.c().get_cash_borrowing_enabled());
}
