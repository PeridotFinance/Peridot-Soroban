//! Upgrade-candidate tests (cash-borrow upgrade checks, 2026-11-18):
//! B entered-market cap, E absolute price age + fallback switch, F price band.
#![cfg(test)]
extern crate std;

use crate::deployed_price_behaviour_test::{Px, ONE};
use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address};

const DAY: u64 = ADMIN_PARAM_CHANGE_DELAY_SECS;

fn warp(p: &Px, secs: u64) {
    p.env.ledger().with_mut(|l| l.timestamp += secs);
}

fn raise_cap(p: &Px, n: u32) {
    p.cc().set_max_user_markets(&n);
    warp(p, DAY);
    p.cc().set_max_user_markets(&n);
    assert_eq!(p.cc().get_max_user_markets(), n);
}

/// Refresh every observation to "now" (after warps) so prices stay fresh.
fn refresh(p: &Px) {
    for i in 0..p.t.len() {
        p.price(i, ONE, p.now());
    }
}

// ------------------------------- B: cap -------------------------------------

#[test]
fn upg_b_default_cap_is_two_after_upgrade() {
    let p = Px::new(3);
    assert_eq!(p.cc().get_max_user_markets(), DEFAULT_MAX_USER_MARKETS);
    let u = Address::generate(&p.env);
    p.cc().enter_market(&u, &p.v[0]);
    p.cc().enter_market(&u, &p.v[1]);
    p.cc().enter_market(&u, &p.v[1]); // re-entering an entered market is a no-op
    let r = p.cc().try_enter_market(&u, &p.v[2]);
    assert!(r.is_err(), "third market must be rejected");
    assert_eq!(p.cc().get_user_markets(&u).len(), 2);
}

#[test]
#[should_panic(expected = "too many entered markets")]
fn upg_b_track_borrow_market_enforces_cap() {
    let p = Px::new(3);
    let u = Address::generate(&p.env);
    p.cc().track_borrow_market(&u, &p.v[0]);
    p.cc().track_borrow_market(&u, &p.v[1]);
    p.cc().track_borrow_market(&u, &p.v[2]);
}

#[test]
fn upg_b_setter_bounds_and_admin() {
    let p = Px::new(1);
    assert!(p.cc().try_set_max_user_markets(&1u32).is_err());
    assert!(p.cc().try_set_max_user_markets(&(MAX_USER_MARKETS + 1)).is_err());
    p.env.set_auths(&[]);
    assert!(p.cc().try_set_max_user_markets(&3u32).is_err(), "non-admin rejected");
}

#[test]
fn upg_b_raise_is_timelocked_lower_is_immediate() {
    let p = Px::new(1);
    p.cc().set_max_user_markets(&3u32); // staged
    assert_eq!(p.cc().get_max_user_markets(), 2);
    warp(&p, DAY - 1);
    assert!(p.cc().try_set_max_user_markets(&3u32).is_err(), "early execute rejected");
    warp(&p, 1);
    p.cc().set_max_user_markets(&3u32);
    assert_eq!(p.cc().get_max_user_markets(), 3);
    p.cc().set_max_user_markets(&2u32); // tightening: immediate
    assert_eq!(p.cc().get_max_user_markets(), 2);
}

#[test]
fn upg_b_over_cap_legacy_account_can_unwind_but_not_add_risk() {
    let p = Px::new(4);
    raise_cap(&p, 3);
    refresh(&p);
    let u = Address::generate(&p.env);
    p.deposit(&u, 0, 1_000);
    p.deposit(&u, 1, 1_000);
    for i in 0..3 {
        p.cc().enter_market(&u, &p.v[i]);
    }
    p.vc(2).borrow(&u, &500u128);
    p.cc().set_max_user_markets(&2u32); // lowered below this account's 3 markets
    assert_eq!(p.cc().get_user_markets(&u).len(), 3, "markets are kept");
    // No new debt, no new market.
    assert!(p.vc(2).try_borrow(&u, &1u128).is_err());
    assert!(p.vc(0).try_borrow(&u, &1u128).is_err());
    assert!(p.cc().try_enter_market(&u, &p.v[3]).is_err());
    assert!(p.cc().try_hypothetical_liquidity(&u, &p.v[2], &1u128, &p.t[2]).is_err());
    // Unwinding is never gated.
    token::StellarAssetClient::new(&p.env, &p.t[2]).mint(&u, &100);
    p.vc(2).repay(&u, &100u128);
    p.vc(0).withdraw(&u, &10u128);
    let (liq, _short) = p.cc().account_liquidity(&u);
    assert!(liq > 0);
    // Liquidation still works: collateral token 0 and 1 crash.
    p.price(0, ONE / 10, p.now());
    p.price(1, ONE / 10, p.now());
    p.cc().cache_price(&p.t[0]);
    p.cc().cache_price(&p.t[1]);
    let (_l, short) = p.cc().account_liquidity(&u);
    assert!(short > 0);
    let liquidator = Address::generate(&p.env);
    token::StellarAssetClient::new(&p.env, &p.t[2]).mint(&liquidator, &1_000);
    token::Client::new(&p.env, &p.t[2]).approve(&liquidator, &p.v[2], &1_000, &(p.env.ledger().sequence() + 1000));
    p.cc().liquidate(&u, &p.v[2], &p.v[0], &50u128, &liquidator);
    assert!(p.vc(0).get_ptoken_balance(&liquidator) > 0);
}

#[test]
fn upg_b_exit_then_reenter_within_cap() {
    let p = Px::new(3);
    let u = Address::generate(&p.env);
    p.cc().enter_market(&u, &p.v[0]);
    p.cc().enter_market(&u, &p.v[1]);
    p.cc().exit_market(&u, &p.v[1]);
    p.cc().enter_market(&u, &p.v[2]);
    assert_eq!(p.cc().get_user_markets(&u).len(), 2);
    p.deposit(&u, 0, 1_000);
    p.vc(2).borrow(&u, &100u128); // within cap: borrowing works
}

// ------------------------- E: absolute price age ----------------------------

#[test]
fn upg_e_absolute_bound_caps_oracle_reported_resolution() {
    // Deployed: resolution 3600, k=2 => fresh for 7200 s (dep_e_bound_follows_*).
    let p = Px::new(2);
    p.res(3600);
    let u = p.borrower(0, 1, 100);
    assert_eq!(p.cc().get_max_price_age_secs(), DEFAULT_MAX_PRICE_AGE_SECS);
    p.at(900);
    p.vc(1).borrow(&u, &10u128);
    p.at(901);
    assert!(p.vc(1).try_borrow(&u, &10u128).is_err(), "> 900 s must revert borrow");
    assert!(p.vc(0).try_withdraw(&u, &10u128).is_err(), "> 900 s must revert redeem with debt");
    assert!(p.cc().get_price_usd(&p.t[0]).is_none());
}

#[test]
fn upg_e_withdraw_without_debt_unaffected_by_stale_prices() {
    let p = Px::new(2);
    let u = Address::generate(&p.env);
    p.deposit(&u, 0, 1_000);
    p.cc().enter_market(&u, &p.v[0]);
    p.at(5_000);
    p.vc(0).withdraw(&u, &1_000u128);
}

#[test]
fn upg_e_setter_tighten_immediate_loosen_timelocked() {
    let p = Px::new(1);
    assert!(p.cc().try_set_max_price_age_secs(&59u64).is_err());
    assert!(p.cc().try_set_max_price_age_secs(&3_601u64).is_err());
    p.cc().set_max_price_age_secs(&600u64);
    assert_eq!(p.cc().get_max_price_age_secs(), 600);
    p.cc().set_max_price_age_secs(&900u64); // staged
    assert_eq!(p.cc().get_max_price_age_secs(), 600);
    assert!(p.cc().try_set_max_price_age_secs(&900u64).is_err());
    warp(&p, DAY);
    p.cc().set_max_price_age_secs(&900u64);
    assert_eq!(p.cc().get_max_price_age_secs(), 900);
    p.env.set_auths(&[]);
    assert!(p.cc().try_set_max_price_age_secs(&120u64).is_err(), "non-admin rejected");
}

#[test]
fn upg_e_tightened_bound_applies_to_cached_prices() {
    let p = Px::new(2);
    p.cc().set_oracle_max_age_multiplier(&10u64); // k*res = 3000 s
    p.cc().set_max_price_age_secs(&300u64);
    let u = p.borrower(0, 1, 100); // caches written at T0
    p.at(300);
    p.vc(1).borrow(&u, &10u128);
    p.at(301);
    assert!(p.vc(1).try_borrow(&u, &10u128).is_err());
}

#[test]
fn upg_e_fallback_ignored_by_default_and_enable_is_timelocked() {
    let p = Px::new(2);
    let u = p.borrower(0, 1, 100);
    for i in 0..2 {
        p.cc().set_price_fallback(&p.t[i], &Some((10_000_000u128, 10_000_000u128)));
    }
    assert!(!p.cc().get_fallback_prices_enabled());
    p.at(601);
    assert!(p.cc().get_price_usd(&p.t[0]).is_none(), "fallback must be ignored by default");
    assert!(p.vc(1).try_borrow(&u, &10u128).is_err());
    p.cc().set_fallback_prices_enabled(&true); // staged
    assert!(!p.cc().get_fallback_prices_enabled());
    warp(&p, DAY);
    p.cc().set_fallback_prices_enabled(&true);
    assert!(p.cc().get_fallback_prices_enabled());
    // Fallback age is 24 h from set time, which has now passed.
    assert!(p.cc().get_price_usd(&p.t[0]).is_none());
    for i in 0..2 {
        p.cc().set_price_fallback(&p.t[i], &Some((10_000_000u128, 10_000_000u128)));
    }
    assert_eq!(p.cc().get_price_usd(&p.t[0]), Some((10_000_000, 10_000_000)));
    p.cc().set_fallback_prices_enabled(&false); // disabling: immediate
    assert!(p.cc().get_price_usd(&p.t[0]).is_none());
}

// ------------------------------ F: price band -------------------------------

const BAND: (u128, u128) = (980_000, 1_020_000);

#[test]
fn upg_f_band_in_and_out() {
    // Public view is banded; caching (used by health/liquidation) is not.
    let p = Px::new(1);
    p.cc().set_price_band(&p.t[0], &Some(BAND));
    assert_eq!(p.cc().get_price_band(&p.t[0]), Some(BAND));
    for (price, served) in [
        (9_800_000i128, true),
        (10_200_000, true),
        (10_000_000, true),
        (9_790_000, false),
        (10_210_000, false),
    ] {
        p.price(0, price, p.now());
        assert!(p.cc().cache_price(&p.t[0]).is_some(), "live price always cached {price}");
        assert_eq!(p.cc().get_price_usd(&p.t[0]).is_some(), served, "view {price}");
    }
}

fn mint_approve(p: &Px, who: &Address, i: usize, amt: i128) {
    token::StellarAssetClient::new(&p.env, &p.t[i]).mint(who, &amt);
    token::Client::new(&p.env, &p.t[i]).approve(who, &p.v[i], &amt, &(p.env.ledger().sequence() + 1000));
}

/// Out of band (0.97 and 1.03): new borrows and redeems-with-debt revert; repay and debt-free redeem work.
#[test]
fn upg_f_out_of_band_blocks_borrow_and_redeem_but_not_repay() {
    for depeg in [9_700_000i128, 10_300_000] {
        let p = Px::new(2); // market 0 = "USDC"
        p.cc().set_price_band(&p.t[0], &Some(BAND));
        let against = p.borrower(0, 1, 100); // USDC collateral, debt in market 1
        let of = p.borrower(1, 0, 100); // market-1 collateral, USDC debt
        let plain = Address::generate(&p.env);
        p.deposit(&plain, 0, 500);
        p.cc().enter_market(&plain, &p.v[0]);
        p.price(0, depeg, p.now());
        p.cc().cache_price(&p.t[0]); // anyone refreshes: cache now holds the depegged price
        let e = |r: bool, what: &str| assert!(r, "{what} at {depeg}");
        e(p.vc(0).try_borrow(&of, &10u128).is_err(), "borrow of USDC reverts");
        e(p.vc(1).try_borrow(&against, &10u128).is_err(), "borrow against USDC collateral reverts");
        e(p.vc(0).try_withdraw(&against, &10u128).is_err(), "redeem USDC collateral with debt reverts");
        e(p.vc(1).try_withdraw(&of, &10u128).is_err(), "redeem other collateral with USDC debt reverts");
        e(p.vc(0).try_transfer(&against, &plain, &10i128).is_err(), "pToken transfer with debt reverts");
        mint_approve(&p, &of, 0, 50);
        p.vc(0).repay(&of, &50u128); // repay continues
        mint_approve(&p, &against, 1, 50);
        p.vc(1).repay(&against, &50u128);
        p.vc(0).withdraw(&plain, &500u128); // debt-free redeem stays allowed (reads no price)
        // back in band: borrow works again
        p.price(0, ONE, p.now());
        p.cc().cache_price(&p.t[0]);
        p.vc(0).borrow(&of, &10u128);
        // positive controls: the same redeems/transfer succeed in band (so the reverts above are the band)
        p.vc(0).withdraw(&against, &10u128);
        p.vc(1).withdraw(&of, &10u128);
        p.vc(0).transfer(&against, &plain, &10i128);
        p.vc(1).borrow(&against, &10u128);
    }
}

fn liquidator(p: &Px, repay_i: usize) -> Address {
    let l = Address::generate(&p.env);
    token::StellarAssetClient::new(&p.env, &p.t[repay_i]).mint(&l, &10_000);
    token::Client::new(&p.env, &p.t[repay_i]).approve(&l, &p.v[repay_i], &10_000, &(p.env.ledger().sequence() + 1000));
    l
}

/// Liquidation of USDC-debt and USDC-collateral positions continues out of band, at the live price.
#[test]
fn upg_f_out_of_band_liquidation_continues_at_live_price() {
    for depeg in [9_700_000i128, 10_300_000] {
        // USDC debt, market-1 collateral; market-1 price halves -> shortfall.
        let p = Px::new(2);
        p.cc().set_price_band(&p.t[0], &Some(BAND));
        let u = p.borrower(1, 0, 450);
        p.price(1, ONE / 2, p.now());
        p.price(0, depeg, p.now());
        p.cc().cache_price(&p.t[0]); // keeper refresh (permissionless)
        p.cc().cache_price(&p.t[1]);
        let l = liquidator(&p, 0);
        let seized = p.cc().preview_seize_ptokens(&p.v[0], &p.v[1], &50u128);
        p.cc().liquidate(&u, &p.v[0], &p.v[1], &50u128, &l);
        assert_eq!(p.vc(1).get_ptoken_balance(&l), seized, "seize uses the live depegged price {depeg}");

        // USDC collateral, market-1 debt; shortfall from the USDC move (0.97) or a debt-token rise (1.03).
        let p = Px::new(2);
        p.cc().set_price_band(&p.t[0], &Some(BAND));
        let u = p.borrower(0, 1, 490);
        p.price(0, depeg, p.now());
        if depeg > ONE {
            p.price(1, 11_000_000, p.now());
        }
        p.cc().cache_price(&p.t[0]);
        p.cc().cache_price(&p.t[1]);
        let (_, short) = p.cc().account_liquidity(&u);
        assert!(short > 0, "shortfall {depeg}");
        let l = liquidator(&p, 1);
        p.cc().liquidate(&u, &p.v[1], &p.v[0], &50u128, &l);
        assert!(p.vc(0).get_ptoken_balance(&l) > 0, "USDC collateral seized {depeg}");
    }
}

#[test]
fn upg_f_band_applies_to_cached_values_after_band_change() {
    let p = Px::new(2);
    let of = p.borrower(1, 0, 100);
    p.cc().cache_price(&p.t[0]); // $1.00 cached, fresh
    p.cc().set_price_band(&p.t[0], &Some((1_010_000, 1_020_000))); // cached $1.00 now out of band
    assert!(p.cc().get_price_usd(&p.t[0]).is_none());
    assert!(p.vc(0).try_borrow(&of, &10u128).is_err());
}

#[test]
fn upg_f_band_setter_tighten_immediate_widen_and_remove_timelocked() {
    let p = Px::new(1);
    let t = &p.t[0];
    assert!(p.cc().try_set_price_band(t, &Some((0, 1))).is_err());
    assert!(p.cc().try_set_price_band(t, &Some((2, 1))).is_err());
    p.cc().set_price_band(t, &Some(BAND));
    p.cc().set_price_band(t, &Some((990_000, 1_010_000))); // nested: immediate
    assert_eq!(p.cc().get_price_band(t), Some((990_000, 1_010_000)));
    p.cc().set_price_band(t, &Some(BAND)); // widening: staged
    assert_eq!(p.cc().get_price_band(t), Some((990_000, 1_010_000)));
    warp(&p, DAY);
    p.cc().set_price_band(t, &Some(BAND));
    assert_eq!(p.cc().get_price_band(t), Some(BAND));
    p.cc().set_price_band(t, &None); // removal: staged
    assert_eq!(p.cc().get_price_band(t), Some(BAND));
    assert!(p.cc().try_set_price_band(t, &None).is_err());
    warp(&p, DAY);
    p.cc().set_price_band(t, &None);
    assert_eq!(p.cc().get_price_band(t), None);
    p.env.set_auths(&[]);
    assert!(p.cc().try_set_price_band(t, &Some(BAND)).is_err(), "non-admin rejected");
}

// ------------------------- cancel_upgrade (controller) -----------------------

fn pending_upgrade(p: &Px) -> (Option<soroban_sdk::BytesN<32>>, Option<u64>) {
    p.env.as_contract(&p.comp, || {
        (
            p.env.storage().persistent().get(&crate::storage::DataKey::PendingUpgradeHash),
            p.env.storage().persistent().get(&crate::storage::DataKey::PendingUpgradeEta),
        )
    })
}

#[test]
fn upg_cancel_upgrade_clears_pending_then_upgrade_panics() {
    let p = Px::new(1);
    let h = soroban_sdk::BytesN::from_array(&p.env, &[7u8; 32]);
    p.cc().propose_upgrade_wasm(&h);
    assert!(pending_upgrade(&p).0.is_some());
    p.cc().cancel_upgrade();
    assert_eq!(pending_upgrade(&p), (None, None));
    warp(&p, UPGRADE_TIMELOCK_SECS + 1);
    assert!(p.cc().try_upgrade_wasm(&h).is_err(), "upgrade after cancel must fail");
    assert!(p.cc().try_cancel_upgrade().is_err(), "nothing pending");
}

#[test]
fn upg_cancel_upgrade_rejects_non_admin() {
    let p = Px::new(1);
    let h = soroban_sdk::BytesN::from_array(&p.env, &[7u8; 32]);
    p.cc().propose_upgrade_wasm(&h);
    p.env.set_auths(&[]);
    assert!(p.cc().try_cancel_upgrade().is_err());
    assert!(pending_upgrade(&p).0.is_some());
}

#[test]
fn upg_repropose_overwrites_pending_hash_and_eta() {
    let p = Px::new(1);
    let h1 = soroban_sdk::BytesN::from_array(&p.env, &[1u8; 32]);
    let h2 = soroban_sdk::BytesN::from_array(&p.env, &[2u8; 32]);
    p.cc().propose_upgrade_wasm(&h1);
    warp(&p, 3600);
    p.cc().propose_upgrade_wasm(&h2);
    assert_eq!(pending_upgrade(&p), (Some(h2.clone()), Some(p.now() + UPGRADE_TIMELOCK_SECS)));
    warp(&p, UPGRADE_TIMELOCK_SECS - 3600 + 1);
    assert!(p.cc().try_upgrade_wasm(&h1).is_err(), "old hash no longer pending");
    assert!(p.cc().try_upgrade_wasm(&h2).is_err(), "ETA restarted");
}
