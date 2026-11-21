//! NATIVE tests for the group-controller rules (G1, G2, G6-G10, cache-only liquidation, timelocks) on
//! the same stack as `stgv2_test.rs`. The controller is compiled as a DEPENDENCY of this test crate,
//! so `admin_param_change_delay_secs()` is the real 24 h (it is 0 only under the controller's own
//! `cfg!(test)`).
extern crate std;
use crate::stgv2_test::{fails, TrapController, G, CF, P1};
use simple_peridottroller::{SimplePeridottroller, SimplePeridottrollerClient};
use soroban_sdk::{testutils::Address as _, Address};

const DAY: u64 = 86_400;

#[test]
fn g6_only_the_sleeve_carries_a_cf_so_cash_collateral_never_mixes() {
    let t = G::new();
    assert!(fails(t.ctl().try_set_market_cf(&t.usdc_m, &100_000)), "USDC market cannot become collateral");
    assert_eq!(t.ctl().get_market_cf(&t.usdc_m), 0);
    // Supplying USDC adds no borrowing power: only sleeve units do.
    t.ctl().enter_market(&t.bob, &t.usdc_m);
    t.m().deposit(&t.bob, &1_000_000_000);
    assert!(fails(t.m().try_borrow(&t.bob, &1_000)), "cash supply is not collateral");
    let n = t.stake(&t.bob);
    t.ctl().enter_market(&t.bob, &t.receipt);
    let r = t.c().checkpoint_nav();
    let (liq, short) = t.ctl().account_liquidity(&t.bob);
    assert_eq!(short, 0);
    assert_eq!(liq, n * r / 1_000_000 * CF / 1_000_000, "collateral = sleeve units only");
    // With no sleeve bound, no market can take a CF at all (fresh controller).
    let c2 = t.env.register(SimplePeridottroller, ());
    let ctl2 = SimplePeridottrollerClient::new(&t.env, &c2);
    ctl2.initialize(&t.admin);
    ctl2.add_market(&t.usdc_m);
    assert!(fails(ctl2.try_set_market_cf(&t.usdc_m, &100_000)), "no sleeve bound: no CF anywhere");
}

#[test]
fn g1_sleeve_binding_is_one_time_and_needs_a_listed_market() {
    let t = G::new();
    assert!(fails(t.ctl().try_group_set_sleeve(&t.usdc_m)), "sleeve fixed");
    let c2 = t.env.register(SimplePeridottroller, ());
    let ctl2 = SimplePeridottrollerClient::new(&t.env, &c2);
    ctl2.initialize(&t.admin);
    let stranger = Address::generate(&t.env);
    assert!(fails(ctl2.try_group_set_sleeve(&stranger)), "unlisted market");
    t.env.set_auths(&[]);
    assert!(fails(t.ctl().try_group_set_liq_cache_only(&false)), "admin only");
    assert!(fails(ctl2.try_group_set_sleeve(&t.usdc_m)), "admin only");
}

#[test]
fn g7_at_most_two_markets_and_g2_at_most_two_entered() {
    let t = G::new();
    let third = t.env.register(crate::ReceiptVault, ());
    crate::ReceiptVaultClient::new(&t.env, &third).initialize(&t.usdc, &0, &0, &t.admin);
    assert!(fails(t.ctl().try_add_market(&third)), "third market refused");
    t.ctl().add_market(&t.usdc_m); // re-listing an existing market is allowed
    t.ctl().enter_market(&t.bob, &t.receipt);
    t.ctl().enter_market(&t.bob, &t.usdc_m);
    assert!(fails(t.ctl().try_enter_market(&t.bob, &third)));
    assert_eq!(t.ctl().get_user_markets(&t.bob).len(), 2);
}

#[test]
fn g8_no_static_price_fallback_and_borrow_fails_closed_when_the_usdc_price_is_stale() {
    let t = G::new();
    assert!(fails(t.ctl().try_set_price_fallback(&t.usdc, &Some((1_000_000u128, 1_000_000u128)))));
    t.ctl().set_price_fallback(&t.usdc, &None);
    t.stake(&t.alice);
    t.ctl().enter_market(&t.alice, &t.receipt);
    t.ctl().enter_market(&t.alice, &t.usdc_m);
    t.c().checkpoint_nav();
    t.o().set(&t.usdc, &P1, &(t.now() - 601), &false);
    assert!(fails(t.m().try_borrow(&t.alice, &1_000)), "no USDC price: no borrow");
    t.o().set(&t.usdc, &P1, &0, &true);
    t.m().borrow(&t.alice, &1_000);
}

#[test]
fn g9_no_margin_liquidation_controller() {
    let t = G::new();
    let m = Address::generate(&t.env);
    assert!(fails(t.ctl().try_set_margin_liquidation_ctrl(&m, &true)));
    t.ctl().set_margin_liquidation_ctrl(&m, &false);
}

#[test]
fn g10_debt_free_account_can_always_exit_and_an_indebted_or_unreadable_one_cannot() {
    let t = G::new();
    t.borrower(100_000_000);
    assert!(fails(t.ctl().try_exit_market(&t.alice, &t.receipt)), "debt: refused (CF > 0)");
    // A debt-free account exits with units, oracle trapping, no checkpoint.
    t.stake(&t.bob);
    t.ctl().enter_market(&t.bob, &t.receipt);
    t.ctl().enter_market(&t.bob, &t.usdc_m);
    t.o().trap(&true);
    t.adv(10_000);
    t.ctl().exit_market(&t.bob, &t.receipt);
    // Unreadable debt fails CLOSED: the USDC market traps, so the sleeve exit is refused.
    let u = G::new();
    u.stake(&u.bob);
    u.ctl().enter_market(&u.bob, &u.receipt);
    u.ctl().enter_market(&u.bob, &u.usdc_m);
    u.env.register_at(&u.usdc_m, TrapController, ());
    assert!(fails(u.ctl().try_exit_market(&u.bob, &u.receipt)), "debt unreadable: fail closed");
}

#[test]
fn cf_changes_are_timelocked_both_ways_and_only_emergency_disable_is_immediate() {
    let t = G::new();
    // Loosening 50 % -> 60 %: pending; executes only after 24 h.
    t.ctl().set_market_cf(&t.receipt, &600_000);
    assert_eq!(t.ctl().get_market_cf(&t.receipt), CF, "pending");
    t.adv(DAY - 1);
    assert!(fails(t.ctl().try_set_market_cf(&t.receipt, &600_000)), "timelocked");
    t.adv(1);
    t.ctl().set_market_cf(&t.receipt, &600_000);
    assert_eq!(t.ctl().get_market_cf(&t.receipt), 600_000);
    // Tightening 60 % -> 40 % is ALSO timelocked (BUILD-REPORT open issue 5).
    t.ctl().set_market_cf(&t.receipt, &400_000);
    assert_eq!(t.ctl().get_market_cf(&t.receipt), 600_000, "a CF cut waits 24 h too");
    // A different proposal replaces the pending one and restarts the clock.
    t.adv(DAY - 10);
    t.ctl().set_market_cf(&t.receipt, &450_000);
    t.adv(10);
    assert!(fails(t.ctl().try_set_market_cf(&t.receipt, &450_000)), "clock restarted");
    // Immediate tightening: emergency_disable_collateral (CF 0), which also clears the pending change.
    assert!(fails(t.ctl().try_emergency_disable_collateral(&t.receipt, &false)), "ack required");
    t.ctl().emergency_disable_collateral(&t.receipt, &true);
    assert_eq!(t.ctl().get_market_cf(&t.receipt), 0);
    // Out of range.
    assert!(fails(t.ctl().try_set_market_cf(&t.receipt, &1_000_001)));
    assert!(fails(t.ctl().try_set_market_cf(&t.receipt, &9_999)));
}

#[test]
fn incentive_and_close_factor_are_timelocked_guardian_pauses_immediately_but_cannot_unpause() {
    let t = G::new();
    let li = t.ctl().get_liquidation_incentive_scaled();
    t.ctl().set_liquidation_incentive(&1_100_000);
    assert_eq!(t.ctl().get_liquidation_incentive_scaled(), li, "pending");
    t.adv(DAY);
    t.ctl().set_liquidation_incentive(&1_100_000);
    assert_eq!(t.ctl().get_liquidation_incentive_scaled(), 1_100_000);
    let cf0 = t.ctl().get_close_factor_scaled();
    t.ctl().set_close_factor(&600_000);
    assert_eq!(t.ctl().get_close_factor_scaled(), cf0, "pending");
    t.ctl().pause_borrow_g(&t.guardian, &t.usdc_m, &true);
    assert!(t.ctl().is_borrow_paused(&t.usdc_m), "guardian pause is immediate");
    assert!(fails(t.ctl().try_pause_borrow_g(&t.guardian, &t.usdc_m, &false)), "guardian cannot unpause");
    t.ctl().set_pause_borrow(&t.usdc_m, &false);
    assert!(!t.ctl().is_borrow_paused(&t.usdc_m));
}

#[test]
fn liquidation_requires_a_shortfall_respects_the_close_factor_and_the_pause() {
    let t = G::new();
    t.borrower(300_000_000);
    t.ctl().cache_price(&t.usdc);
    assert!(fails(t.ctl().try_liquidate(&t.alice, &t.usdc_m, &t.receipt, &10_000_000, &t.liq)), "healthy: refused");
    t.v().mock_lose(&5_000);
    t.c().checkpoint_nav();
    let debt = t.m().get_user_borrow_balance(&t.alice);
    // A request above the 50 % close factor is CLAMPED to it (not refused).
    let b0 = t.bal(&t.usdc, &t.liq);
    t.ctl().liquidate(&t.alice, &t.usdc_m, &t.receipt, &(debt * 9 / 10), &t.liq);
    let paid = b0 - t.bal(&t.usdc, &t.liq);
    assert!(paid <= debt / 2 + 1_000 && paid + 1_000 >= debt / 2, "clamped to the close factor: paid {paid} of {debt}");
    t.ctl().set_pause_liquidation(&t.receipt, &true);
    t.c().checkpoint_nav();
    assert!(fails(t.ctl().try_liquidate(&t.alice, &t.usdc_m, &t.receipt, &1_000_000, &t.liq)), "paused");
}
