//! WP4 cross-module tests: positions (WP2) + backstop (WP1) + pool check (WP3), no shims.
//! State is driven only through public entrypoints and the mocks' test controls (prices,
//! interest, exchange rates, pool fill, an external seizure). Storage is read only for
//! assertions where no view exists (Ledger, DebtShares, ClaimsTotal).
//! Mocks and fixture: test_positions.rs (shared; the mock controller's `portfolio` and its
//! borrow/withdraw/transfer health check use one row function, like the live controller).
extern crate std;
use crate::test_positions::{caps, ce, check_debt_invariant, err, Rng, T, PS, U};
use crate::{Error, Headroom};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::Address;
use std::collections::BTreeMap;
use std::vec::Vec as SVec;

const COOLDOWN: u64 = 259_200; // 72 h

fn trader(t: &T) -> Address {
    let a = Address::generate(&t.env);
    for tok in [&t.usdc, &t.xlm] {
        StellarAssetClient::new(&t.env, tok).mint(&a, &(100_000_000 * U as i128));
    }
    a
}
fn long(t: &T, who: &Address, coll_xlm: u128, borrow_usdc: u128) -> u64 {
    t.m.open(who, &t.xlm, &t.usdc, &coll_xlm, &borrow_usdc, &0, &u128::MAX)
}
fn short(t: &T, who: &Address, coll_usdc: u128, borrow_xlm: u128) -> u64 {
    t.m.open(who, &t.usdc, &t.xlm, &coll_usdc, &borrow_xlm, &0, &u128::MAX)
}
fn nav(t: &T) -> i128 {
    t.m.backstop_nav().0
}
/// Fee in USDC raw (= pTokens at rate 1e6) for `usd7` notional at 0.1 %, rounded up.
fn fee_of(usd7: u128) -> u128 {
    (usd7 * 1_000).div_ceil(1_000_000)
}
fn set_headroom(t: &T, coef_long: u128, coef_short: u128, coef_largest: u128) {
    t.m.set_headroom(
        &t.admin,
        &Headroom {
            hf_target: 1_250_000,
            hf_stress: 1_100_000,
            stress: 200_000,
            close_reserve_usd: 0,
            coef_long,
            coef_short,
            coef_largest,
        },
    );
}
/// Admin-held USDC pTokens (supplied through the receipt).
fn admin_usdc_pt(t: &T, amount: u128) {
    StellarAssetClient::new(&t.env, &t.usdc).mint(&t.admin, &(amount as i128));
    t.ru.deposit(&t.admin, &amount);
}
/// Keeper settle of USDC bad debt in chunks: each withdrawal of backstop pTokens happens before
/// its repay, so the controller's health check bounds a chunk by the pooled free liquidity.
fn settle_usdc_chunked(t: &T, keeper: &Address) -> u32 {
    let mut n = 0;
    while t.bs_shares(&t.usdc) > 0 {
        let free = t.m.pooled_health().3; // usd7 at CF; USDC $1, CF 0.75
        let rate = t.ru.get_exchange_rate();
        let chunk = (free * 4 / 3 * 9 / 10) * 1_000_000 / rate;
        t.m.settle_backstop_debt(keeper, &t.usdc, &chunk.max(1), &0);
        n += 1;
        assert!(n < 50, "settle does not converge");
    }
    n
}
fn impaired_events(t: &T) -> usize {
    use soroban_sdk::testutils::Events as _;
    let want: soroban_sdk::xdr::ScVal =
        (&soroban_sdk::Symbol::new(&t.env, "margin_impaired")).into();
    t.env
        .events()
        .all()
        .filter_by_contract(&t.m.address)
        .events()
        .iter()
        .filter(|e| {
            let soroban_sdk::xdr::ContractEventBody::V0(b) = &e.body;
            b.topics.first() == Some(&want)
        })
        .count()
}

// ------------------------------------------------------------------ 1. fees reach NAV
#[test]
fn seed_then_long_and_short_at_3x_and_5x_fees_reach_backstop_nav() {
    let t = T::new(); // admin seed of 750 USDC pTokens through backstop_deposit
    let b = t.m.get_backstop();
    assert_eq!((b.ptokens, b.shares), (750 * U, 750 * U + crate::BS_DEAD_SHARES));
    assert_eq!(nav(&t), (750 * U) as i128);
    let (a, c) = (trader(&t), trader(&t));
    let mut ids = SVec::new();
    // (who, long?, collateral, borrow, gross notional usd7)
    let opens: [(&Address, bool, u128, u128, u128); 4] = [
        (&a, true, 100 * U, 50 * U, 75 * U),    // 3x long: $25 + $50
        (&a, true, 100 * U, 100 * U, 125 * U),  // 5x long
        (&c, false, 25 * U, 200 * U, 75 * U),   // 3x short: $25 + 200 XLM ($50)
        (&c, false, 25 * U, 400 * U, 125 * U),  // 5x short
    ];
    for (who, is_long, coll, borrow, gross) in opens {
        let n0 = nav(&t);
        let id = if is_long { long(&t, who, coll, borrow) } else { short(&t, who, coll, borrow) };
        assert_eq!(nav(&t) - n0, fee_of(gross) as i128, "open fee {}", id);
        ids.push(id);
    }
    assert_eq!(nav(&t), (750 * U + 4 * U / 10) as i128); // 0.075+0.125+0.075+0.125
    t.assert_ledgers();
    // Close fees (same timestamp: no funding) reach the NAV too: 0.1 % of each notional.
    for id in ids {
        let p = t.pos(id);
        let n0 = nav(&t);
        t.m.close(&p.owner, &id, &0);
        assert_eq!(nav(&t) - n0, fee_of(p.notional_usd) as i128, "close fee {}", id);
    }
    assert_eq!(t.pooled(&t.usdc), 0);
    assert_eq!(t.pooled(&t.xlm), 0);
    assert_eq!(t.ledger(&t.usdc), t.bs_pt());
    assert_eq!(t.ledger(&t.xlm), 0);
    t.assert_ledgers();
}

// ------------------------------------------------------------------ 2. exit vs headroom
#[test]
fn backstop_exit_blocked_by_headroom_then_allowed_after_close() {
    let t = T::new();
    let traders: SVec<Address> = (0..4).map(|_| trader(&t)).collect();
    let ids: SVec<u64> = traders.iter().map(|w| long(&t, w, 100 * U, 100 * U)).collect();
    // Book: XLM $500 x 0.55 = $275, USDC debt $400; backstop ~$750.5.
    t.m.backstop_request_withdraw(&t.admin, &(500 * U));
    t.skip(COOLDOWN);
    // Leaving ~$250 of pTokens: HF (275 + 188) / 400 = 1.16 < 1.25 (still >= 1, so the
    // controller's own transfer check passes and the margin pool check is what refuses).
    assert_eq!(err(t.m.try_backstop_withdraw(&t.admin, &0)), ce(Error::BackstopHeadroom));
    let b = t.m.get_backstop();
    assert_eq!(b.pending_shares, 500 * U);
    // Two closes later the same exit passes (HF (137.5 + 188) / 200).
    t.m.close(&traders[0], &ids[0], &0);
    t.m.close(&traders[1], &ids[1], &0);
    let before = t.ru.get_ptoken_balance(&t.admin);
    let pt_before = t.bs_pt(); // includes the two closes' fees
    assert!(pt_before > b.ptokens);
    let out = t.m.backstop_withdraw(&t.admin, &0);
    assert_eq!(t.ru.get_ptoken_balance(&t.admin) - before, out);
    let b2 = t.m.get_backstop();
    assert_eq!(b2.ptokens, pt_before - out);
    assert_eq!(b2.pending_shares, 0);
    assert_eq!(b2.shares, b.shares - 500 * U);
    // Paid at execution-time NAV (incl. the closes' fees), rounded down.
    assert_eq!(out, 500 * U * pt_before / b.shares);
    t.assert_ledgers();
}

// ------------------------------------------------------------------ 3. underwater + settle
#[test]
fn underwater_long_nav_drops_by_exactly_the_bad_debt_then_settle() {
    let t = T::new();
    t.fees(0, 0, 0);
    let (a, b) = (trader(&t), trader(&t));
    let la = long(&t, &a, 100 * U, 100 * U); // 5x, liq 0.9
    let lb = long(&t, &b, 100 * U, 25 * U); // 2x, liq 0.75
    let pb = t.pos(lb);
    t.set_xlm(1_800_000); // A: $90 < 103 -> underwater; B: LTV 0.69
    let db = t.m.get_position_debt(&lb);
    let (n0, pt0) = (nav(&t), t.bs_pt());
    t.m.liquidate(&t.liq, &la, &u128::MAX);
    // Independent: residual shares x pooled / all shares, after the call.
    let (s, ps, pooled) = (t.bs_shares(&t.usdc), t.shares(&t.usdc), t.pooled(&t.usdc));
    let bad = (s * pooled).div_ceil(s + ps);
    let bad_pt = (bad * 1_000_000).div_ceil(t.ru.get_exchange_rate());
    assert_eq!(n0 - nav(&t), bad_pt as i128);
    assert_eq!(t.bs_pt(), pt0); // the underwater branch pays the backstop nothing
    assert_eq!(t.pos(lb), pb);
    assert_eq!(t.m.get_position_debt(&lb), db);
    assert!(!t.m.is_impaired());
    // Keeper settles: exactly the bad debt's pTokens leave the backstop; NAV unchanged.
    let n1 = nav(&t);
    let k = Address::generate(&t.env);
    let repaid = t.m.settle_backstop_debt(&k, &t.usdc, &u128::MAX, &0);
    assert_eq!(repaid, bad);
    assert_eq!(t.bs_shares(&t.usdc), 0);
    assert_eq!(t.bs_pt(), pt0 - bad_pt);
    assert_eq!(nav(&t), n1);
    assert_eq!(nav(&t), t.bs_pt() as i128);
    assert_eq!(t.m.get_position_debt(&lb), db);
    assert_eq!(t.pooled(&t.usdc), db);
    t.assert_ledgers();
}

#[test]
fn underwater_short_freezes_backstop_until_xlm_settle() {
    let t = T::new();
    t.fees(0, 0, 0);
    let (a, b, backer) = (trader(&t), trader(&t), trader(&t));
    let sa = short(&t, &a, 25 * U, 400 * U); // 5x
    let sb = short(&t, &b, 25 * U, 100 * U); // 2x, liq 0.75
    let pb = t.pos(sb);
    let px = 3_500_000; // XLM $0.35: A owes $140 > 125 / 1.03; B LTV 0.70
    t.set_xlm(px);
    let db = t.m.get_position_debt(&sb);
    let (n0, pt0) = (nav(&t), t.bs_pt());
    t.m.liquidate(&t.liq, &sa, &u128::MAX);
    let (s, ps, pooled) = (t.bs_shares(&t.xlm), t.shares(&t.xlm), t.pooled(&t.xlm));
    assert!(s > 0);
    let bad_xlm = (s * pooled).div_ceil(s + ps);
    let usdc_up = (bad_xlm * px).div_ceil(PS); // USDC at $1, both prices on scale PS
    let bad_pt = (usdc_up * 1_000_000).div_ceil(t.ru.get_exchange_rate());
    assert_eq!(n0 - nav(&t), bad_pt as i128); // non-strict NAV values XLM debt at the oracle
    assert_eq!(t.bs_pt(), pt0);
    assert_eq!(t.pos(sb), pb);
    // Frozen: deposits and exits wait for the settle.
    t.ru.deposit(&backer, &(10 * U));
    assert_eq!(
        err(t.m.try_backstop_deposit(&backer, &(10 * U), &0)),
        ce(Error::BackstopDebtOutstanding)
    );
    t.m.backstop_request_withdraw(&t.admin, &U); // queueing is not frozen
    t.skip(COOLDOWN);
    assert_eq!(
        err(t.m.try_backstop_withdraw(&t.admin, &0)),
        ce(Error::BackstopDebtOutstanding)
    );
    // Settle: this mock pool has no strict-receive quote, so the backstop pays the oracle value
    // of the debt + 0.05 % buffer plus at most the 1.5 % slippage buffer (+1 pT rounding); the
    // XLM left after the repay is kept as tracked backstop XLM pTokens (WP6).
    let k = Address::generate(&t.env);
    let repaid = t.m.settle_backstop_debt(&k, &t.xlm, &u128::MAX, &0);
    assert_eq!(repaid, bad_xlm);
    assert_eq!(t.bs_shares(&t.xlm), 0);
    let want = (bad_xlm + bad_xlm / 2_000 + 10).max(1_000_000);
    let usdc_w = (want * px).div_ceil(PS);
    let need_pt = (usdc_w * (1_000_000 + crate::DEFAULT_MAX_SLIPPAGE)).div_ceil(1_000_000) + 1;
    assert_eq!(pt0 - t.bs_pt(), need_pt);
    assert!(t.m.get_bs_xlm_ptokens() > 0);
    assert_eq!(TokenClient::new(&t.env, &t.xlm).balance(&t.m.address), 0);
    assert_eq!(nav(&t), t.bs_pt() as i128);
    assert_eq!(t.m.get_position_debt(&sb), db);
    // Freeze lifted.
    assert!(t.m.backstop_deposit(&backer, &(10 * U), &0) > 0);
    assert!(t.m.backstop_withdraw(&t.admin, &0) > 0);
    t.assert_ledgers();
}

// ------------------------------------------------------------------ 4. exhaustion
#[test]
fn exhaustion_impairs_exits_still_work_then_recap_and_resolve() {
    // Small seed; the backstop requirement reduced to the largest-position term so that six
    // 5x longs fit, and twelve 2x shorts so the pooled account keeps controller HF >= 1 after
    // the backstop is gone (otherwise the controller itself would block every exit).
    let t = T::new_unseeded();
    set_headroom(&t, 0, 0, 1_000_000);
    t.m.set_caps(&t.admin, &t.xlm, &caps(25, 150, 1_000));
    t.m.set_caps(&t.admin, &t.usdc, &caps(25, 300, 1_000));
    t.seed_backstop(350 * U);
    let shorts: SVec<(Address, u64)> = (0..12)
        .map(|_| {
            let w = trader(&t);
            let id = short(&t, &w, 25 * U, 100 * U);
            (w, id)
        })
        .collect();
    let longs: SVec<(Address, u64)> = (0..6)
        .map(|_| {
            let w = trader(&t);
            let id = long(&t, &w, 100 * U, 100 * U);
            (w, id)
        })
        .collect();
    // L0: solvent liquidation at -11.2 % leaves its owner a claim.
    t.set_xlm(2_220_000);
    t.m.liquidate(&t.liq, &longs[0].1, &u128::MAX);
    let claim = t.m.get_claim(&longs[0].0, &t.xlm);
    assert!(claim > 0);
    // Crash to $0.005: every remaining long is underwater (~$97.6 bad debt each).
    t.set_xlm(50_000);
    let mut impaired_at = None;
    let mut liq_after = 0;
    for (k, (_, id)) in longs.iter().enumerate().skip(1) {
        t.m.liquidate(&t.liq, id, &u128::MAX);
        let ev = impaired_events(&t);
        if impaired_at.is_none() {
            if t.m.is_impaired() {
                assert_eq!(ev, 1);
                impaired_at = Some(k);
                assert!(nav(&t) < 0);
            } else {
                assert!(nav(&t) >= 0);
            }
        } else {
            assert_eq!(ev, 0, "margin_impaired emitted once");
            liq_after += 1; // liquidate still works while Impaired
        }
    }
    assert!(impaired_at.is_some() && liq_after >= 1, "{:?} {}", impaired_at, liq_after);
    assert!(t.m.opens_paused());
    // Opens refused (Impaired before the pause, even after an unpause).
    t.m.set_opens_paused(&t.admin, &false);
    let w = trader(&t);
    assert_eq!(
        err(t.m.try_open(&w, &t.usdc, &t.xlm, &(25 * U), &(100 * U), &0, &u128::MAX)),
        ce(Error::Impaired)
    );
    // Exits keep working: close, close_with_repay, claim.
    t.m.close(&shorts[0].0, &shorts[0].1, &0);
    t.m.close_with_repay(&shorts[1].0, &shorts[1].1);
    assert_eq!(t.m.claim(&longs[0].0, &t.xlm), claim);
    t.assert_ledgers();
    // Deposits refused while Impaired; resolve cannot clear while NAV < 0.
    assert_eq!(
        err(t.m.try_backstop_deposit(&t.admin, &U, &0)),
        ce(Error::Impaired)
    );
    t.m.admin_resolve_impairment(&t.admin, &0, &0);
    assert!(t.m.is_impaired());
    // Recapitalise (no shares), then resolve clears.
    let shares = t.m.get_backstop().shares;
    let hole = (-nav(&t)) as u128;
    admin_usdc_pt(&t, hole + 200 * U);
    t.m.admin_recapitalize(&t.admin, &(hole + 200 * U));
    assert_eq!(t.m.get_backstop().shares, shares);
    assert_eq!(nav(&t), (200 * U) as i128);
    t.m.admin_resolve_impairment(&t.admin, &0, &0);
    assert!(!t.m.is_impaired());
    // Keeper settles the USDC bad debt (chunked by the pooled free liquidity).
    let k = Address::generate(&t.env);
    let chunks = settle_usdc_chunked(&t, &k);
    std::println!("settle chunks {}", chunks);
    assert!(nav(&t) > 0);
    // Opens work again after the admin unpauses (already unpaused above).
    t.set_xlm(2_500_000);
    let id = short(&t, &w, 25 * U, 100 * U);
    assert!(t.pos(id).open);
    t.assert_ledgers();
}

// ------------------------------------------------------------------ 5. reconcile
#[test]
fn reconcile_after_external_seizure_then_restore_and_full_exit() {
    let t = T::new();
    let (a, b, c) = (trader(&t), trader(&t), trader(&t));
    let la = long(&t, &a, 100 * U, 100 * U);
    let lb = long(&t, &b, 100 * U, 50 * U);
    let sc = short(&t, &c, 25 * U, 200 * U);
    let k = Address::generate(&t.env);
    assert!(!t.m.reconcile(&k));
    // The cash controller liquidates the pooled account: repays 10 USDC of its debt and seizes
    // $10.8 of XLM pTokens (8 % bonus).
    let seized = 432_000_000u128; // 43.2 XLM
    t.ru.sub_debt(&t.m.address, &(10 * U));
    t.rx.seize(&t.m.address, &t.liq, &seized);
    assert!(t.m.reconcile(&k)); // newly impaired
    assert!(t.m.is_impaired() && t.m.opens_paused());
    assert!(!t.m.reconcile(&k)); // already impaired: keeper does not resend
    let w = trader(&t);
    assert_eq!(err(t.m.try_open(&w, &t.xlm, &t.usdc, &(100 * U), &(50 * U), &0, &u128::MAX)), ce(Error::Impaired));
    // Backstop exits are frozen while Impaired.
    t.m.backstop_request_withdraw(&t.admin, &U);
    t.skip(COOLDOWN);
    assert_eq!(err(t.m.try_backstop_withdraw(&t.admin, &0)), ce(Error::Impaired));
    // Resolve: the admin restores the XLM pTokens; Ledger(XLM) is unchanged.
    let lx = t.ledger(&t.xlm);
    StellarAssetClient::new(&t.env, &t.xlm).mint(&t.admin, &(seized as i128));
    t.rx.deposit(&t.admin, &seized);
    t.m.admin_resolve_impairment(&t.admin, &0, &seized);
    assert!(!t.m.is_impaired());
    assert_eq!(t.ledger(&t.xlm), lx);
    t.assert_ledgers();
    // Every XLM holder exits in full: Ledger(XLM) reaches exactly 0 without underflow.
    t.m.close(&a, &la, &0);
    t.m.close_with_repay(&b, &lb);
    t.m.close(&c, &sc, &0);
    assert_eq!(t.ledger(&t.xlm), 0);
    assert_eq!(t.rx.get_ptoken_balance(&t.m.address), 0);
    assert_eq!(t.pooled(&t.usdc), 0);
    assert_eq!(t.pooled(&t.xlm), 0);
    assert!(t.m.backstop_withdraw(&t.admin, &0) > 0);
    t.assert_ledgers();
}

// ------------------------------------------------------------------ 6. invariant loop
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snap {
    held: u128,
    shares: u128,
    debt: u128,
}
fn bump(c: &mut BTreeMap<&'static str, u32>, k: &'static str) {
    *c.entry(k).or_insert(0) += 1;
}
fn snaps(t: &T, open: &[u64]) -> BTreeMap<u64, Snap> {
    open.iter()
        .map(|id| {
            let p = t.pos(*id);
            (*id, Snap { held: p.held_ptokens, shares: p.debt_shares, debt: t.m.get_position_debt(id) })
        })
        .collect()
}
/// No position other than `acted` changed its held pTokens or debt shares; its debt did not
/// rise (unless interest accrued) and fell by at most rounding.
fn assert_isolated(t: &T, before: &BTreeMap<u64, Snap>, acted: Option<u64>, interest: bool, step: usize) {
    for (id, s) in before {
        if Some(*id) == acted || !t.pos(*id).open {
            continue;
        }
        let p = t.pos(*id);
        assert_eq!((p.held_ptokens, p.debt_shares), (s.held, s.shares), "step {} pos {}", step, id);
        let d = t.m.get_position_debt(id);
        if !interest {
            assert!(d <= s.debt && s.debt - d <= 2, "step {} pos {} debt {} -> {}", step, id, s.debt, d);
        }
    }
}

#[test]
fn invariant_loop_positions_backstop_and_isolation() {
    let t = T::new_no_snapshot();
    t.m.set_caps(&t.admin, &t.xlm, &caps(200, 1_000_000, 10_000_000));
    t.m.set_caps(&t.admin, &t.usdc, &caps(200, 1_000_000, 10_000_000));
    t.m.set_backstop_params(
        &t.admin,
        &crate::BackstopParams { cooldown_s: COOLDOWN, window_s: 172_800, min_deposit_pt: 1_000_000 },
    );
    // Lower backstop coefficients than the pilot defaults, so opens fill the book up to the
    // pooled HF limit and backstop exits do meet the headroom check.
    set_headroom(&t, 1_000_000, 500_000, 2_000_000);
    t.ru.set_rate(&1_020_000);
    t.rx.set_rate(&1_010_000);
    t.pool.set_fill(&997_000);
    let traders: SVec<Address> = (0..3).map(|_| trader(&t)).collect();
    let backers: SVec<Address> = (0..3).map(|_| trader(&t)).collect();
    // backers[0] is a whale: the required-backstop formula keeps the backstop several times the
    // book, so only an exit of most of it can hit the headroom check.
    for (n, bk) in backers.iter().enumerate() {
        t.backer_deposit(bk, if n == 0 { 2_500 * U } else { 200 * U });
    }
    let keeper = Address::generate(&t.env);
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let mut open: SVec<u64> = SVec::new();
    let mut xp: u128 = 2_500_000;
    let mut c: BTreeMap<&'static str, u32> = BTreeMap::new();
    const STEPS: usize = 240;
    for step in 0..STEPS {
        let before = snaps(&t, &open);
        let mut acted: Option<u64> = None;
        let mut interest = false;
        match rng.next() % 14 {
            0..=2 => {
                // Long-biased book: longs (CF 0.55 held vs USDC debt) use the pool's headroom;
                // shorts (USDC held at CF 0.75) add to it.
                let who = &traders[(rng.next() % 3) as usize];
                let coll = rng.range(8, 700) * U;
                let lev = rng.range(1_100_000, 4_990_000);
                let b = coll * xp / PS * (lev - 1_000_000) / 1_000_000;
                if let Ok(Ok(id)) = t.m.try_open(who, &t.xlm, &t.usdc, &coll, &b, &0, &u128::MAX) {
                    open.push(id);
                    acted = Some(id);
                    bump(&mut c, "open");
                }
            }
            3 => {
                let who = &traders[(rng.next() % 3) as usize];
                let coll = rng.range(2, 190) * U;
                let lev = rng.range(1_100_000, 4_990_000);
                let b = coll * (lev - 1_000_000) / 1_000_000 * PS / xp;
                if let Ok(Ok(id)) = t.m.try_open(who, &t.usdc, &t.xlm, &coll, &b, &0, &u128::MAX) {
                    open.push(id);
                    acted = Some(id);
                    bump(&mut c, "open");
                }
            }
            4 if !open.is_empty() => {
                let k = (rng.next() as usize) % open.len();
                let id = open[k];
                let owner = t.pos(id).owner;
                let ok = if rng.next().is_multiple_of(2) {
                    t.m.try_close(&owner, &id, &0).is_ok()
                } else {
                    t.m.try_close_with_repay(&owner, &id).is_ok()
                };
                if ok {
                    open.remove(k);
                    acted = Some(id);
                    bump(&mut c, "close");
                }
            }
            5 => {
                let pct = if rng.next().is_multiple_of(4) { rng.range(650, 1_350) } else { rng.range(950, 1_050) };
                xp = (xp * pct / 1_000).clamp(500_000, 10_000_000);
                t.set_xlm(xp);
            }
            6 => {
                interest = true;
                for a in [t.usdc.clone(), t.xlm.clone()] {
                    let pooled = t.pooled(&a);
                    if pooled > 0 {
                        t.rec(&a).add_debt(&t.m.address, &(pooled * rng.range(1, 15) / 1_000));
                    }
                }
                t.ru.set_rate(&(t.ru.get_exchange_rate() + rng.range(0, 1_500)));
                t.rx.set_rate(&(t.rx.get_exchange_rate() + rng.range(0, 1_500)));
            }
            7 => {
                let s = if rng.next().is_multiple_of(3) { COOLDOWN + rng.range(0, 200_000) as u64 } else { rng.range(0, 86_400) as u64 };
                t.skip(s);
            }
            8 => {
                let bk = &backers[1 + (rng.next() % 2) as usize]; // the whale only seeds once
                let amt = rng.range(10, 150) * U;
                StellarAssetClient::new(&t.env, &t.usdc).mint(bk, &(amt as i128));
                t.ru.deposit(bk, &amt);
                let pt = t.ru.get_ptoken_balance(bk);
                if t.m.try_backstop_deposit(bk, &pt, &0).is_ok() {
                    bump(&mut c, "bs_deposit");
                }
            }
            9 => {
                let n = 1 + (rng.next() % 2) as usize; // the whale (0) exits only in its cycle
                let bk = &backers[n];
                let have = t.m.get_backer(bk);
                if have > 0 {
                    let sh = (have * rng.range(20, 100) / 100).max(1);
                    if t.m.try_backstop_request_withdraw(bk, &sh).is_ok() {
                        bump(&mut c, "bs_request");
                    }
                }
            }
            10 | 11 => {
                let n = (rng.next() % 3) as usize;
                let bk = &backers[n];
                if let Some(r) = t.m.get_withdraw_req(bk) {
                    let mut now = t.env.ledger().timestamp();
                    if now < r.ready_at && rng.next().is_multiple_of(2) {
                        t.skip(r.ready_at - now); // the backer waits out its own cooldown
                        now = r.ready_at;
                    }
                    if now > r.expires_at {
                        t.m.backstop_clear_expired(&keeper, bk);
                        bump(&mut c, "bs_clear_expired");
                    } else if now >= r.ready_at {
                        match t.m.try_backstop_withdraw(bk, &0) {
                            Ok(Ok(_)) => bump(&mut c, "bs_withdraw"),
                            Err(Ok(e)) if e == ce(Error::BackstopHeadroom) => bump(&mut c, "bs_headroom_blocked"),
                            Err(Ok(e)) if e == ce(Error::BackstopDebtOutstanding) => bump(&mut c, "bs_frozen"),
                            Err(Err(_)) => bump(&mut c, "bs_ctrl_blocked"), // controller HF < 1 at transfer
                            other => panic!("step {} withdraw {:?}", step, other),
                        }
                    } else if rng.next().is_multiple_of(4) {
                        t.m.backstop_cancel_withdraw(bk);
                        bump(&mut c, "bs_cancel");
                    }
                }
            }
            12 => {
                for a in [t.usdc.clone(), t.xlm.clone()] {
                    if t.bs_shares(&a) > 0 && t.m.try_settle_backstop_debt(&keeper, &a, &(500 * U), &0).is_ok() {
                        bump(&mut c, "settle");
                    }
                }
            }
            _ => {
                if !t.m.is_impaired() {
                    // Whale cycle: only once the pool is heavily used (spot HF < 1.8) does the
                    // whale queue a full exit, wait out the cooldown and try it. (Queued earlier,
                    // the open check would exclude it and the exit would simply pass.)
                    let w = &backers[0];
                    match t.m.get_withdraw_req(w) {
                        None if t.m.get_backer(w) > 0 && t.m.pooled_health().0 < 1_800_000 => {
                            t.m.backstop_request_withdraw(w, &t.m.get_backer(w));
                            bump(&mut c, "bs_request");
                        }
                        None => {}
                        Some(r) => {
                            let now = t.env.ledger().timestamp();
                            if now > r.expires_at {
                                t.m.backstop_clear_expired(&keeper, w);
                                bump(&mut c, "bs_clear_expired");
                            } else {
                                t.skip(r.ready_at.saturating_sub(now));
                                match t.m.try_backstop_withdraw(w, &0) {
                                    Ok(Ok(_)) => bump(&mut c, "bs_withdraw"),
                                    Err(Ok(e)) if e == ce(Error::BackstopHeadroom) => bump(&mut c, "bs_headroom_blocked"),
                                    Err(Ok(e)) if e == ce(Error::BackstopDebtOutstanding) => bump(&mut c, "bs_frozen"),
                                    Err(Err(_)) => bump(&mut c, "bs_ctrl_blocked"),
                                    other => panic!("step {} whale withdraw {:?}", step, other),
                                }
                            }
                        }
                    }
                } else {
                    // Operator recovery: recapitalise, resolve, unpause.
                    let hole = (-t.m.backstop_nav().0).max(0) as u128;
                    let amt = hole + 2_000 * U;
                    StellarAssetClient::new(&t.env, &t.usdc).mint(&t.admin, &(amt as i128));
                    let p0 = t.ru.get_ptoken_balance(&t.admin);
                    t.ru.deposit(&t.admin, &amt);
                    t.m.admin_recapitalize(&t.admin, &(t.ru.get_ptoken_balance(&t.admin) - p0));
                    t.m.admin_resolve_impairment(&t.admin, &0, &0);
                    if !t.m.is_impaired() {
                        t.m.set_opens_paused(&t.admin, &false);
                        bump(&mut c, "recap");
                    }
                }
            }
        }
        assert_isolated(&t, &before, acted, interest, step);
        // Keeper pass: liquidate everything past its own liq LTV; each touches only itself.
        let mut k = 0;
        while k < open.len() {
            let id = open[k];
            if t.m.is_liquidatable(&id) {
                let before = snaps(&t, &open);
                let bs0 = t.bs_shares(&t.usdc) + t.bs_shares(&t.xlm);
                t.m.liquidate(&t.liq, &id, &u128::MAX);
                if t.bs_shares(&t.usdc) + t.bs_shares(&t.xlm) > bs0 {
                    bump(&mut c, "bad_debt");
                }
                if t.m.is_impaired() {
                    bump(&mut c, "impaired_seen");
                }
                bump(&mut c, "liquidate");
                open.remove(k);
                assert_isolated(&t, &before, Some(id), false, step);
            } else {
                k += 1;
            }
        }
        check_debt_invariant(&t, &open);
    }
    std::println!("{} steps: {:?} open {}", STEPS, c, open.len());
    let g = |k: &str| c.get(k).copied().unwrap_or(0);
    assert!(g("open") > 25 && g("close") > 5 && g("liquidate") > 5, "{:?}", c);
    assert!(g("bad_debt") > 0 && g("settle") > 0, "{:?}", c);
    assert!(g("bs_deposit") > 3 && g("bs_request") > 3 && g("bs_withdraw") > 0, "{:?}", c);
    assert!(g("bs_headroom_blocked") + g("bs_ctrl_blocked") > 0, "{:?}", c);
}

// ------------------------------------------------------------------ WP6: bad debt gates opens
fn try_open(t: &T, who: &Address, long_side: bool) -> Result<u64, soroban_sdk::Error> {
    let r = if long_side {
        t.m.try_open(who, &t.xlm, &t.usdc, &(100 * U), &(50 * U), &0, &u128::MAX)
    } else {
        t.m.try_open(who, &t.usdc, &t.xlm, &(25 * U), &(200 * U), &0, &u128::MAX)
    };
    match r {
        Ok(Ok(id)) => Ok(id),
        Err(Ok(e)) => Err(e),
        other => panic!("unexpected open result {other:?}"),
    }
}

/// WP6 1a: while BsDebtShares(USDC) or BsDebtShares(XLM) > 0 every open fails
/// BackstopDebtOutstanding; close, close_with_repay, liquidate, claim and settle keep working,
/// and opens resume once the keeper has settled.
#[test]
fn opens_refused_while_backstop_bad_debt_unsettled() {
    let t = T::new();
    t.fees(0, 0, 0);
    let a = trader(&t);
    // ---- USDC bad debt (underwater 5x long)
    let l5 = long(&t, &a, 100 * U, 100 * U); // 5x at $0.25
    let l2 = long(&t, &a, 100 * U, 25 * U); // 2x
    let l2b = long(&t, &a, 100 * U, 25 * U); // 2x, liquidated solvent below -> claim
    let s2 = short(&t, &a, 25 * U, 100 * U); // 2x
    t.set_xlm(1_900_000); // $0.19: L5 underwater
    t.m.liquidate(&t.liq, &l5, &u128::MAX);
    assert!(t.bs_shares(&t.usdc) > 0);
    for side in [true, false] {
        assert_eq!(try_open(&t, &a, side), Err(ce(Error::BackstopDebtOutstanding)));
    }
    // exits still work: close_with_repay, swap-close, solvent liquidation + claim
    t.m.close_with_repay(&a, &l2);
    t.m.close(&a, &s2, &0);
    t.set_xlm(1_500_000); // $0.15: L2b (liq LTV 0.75) is liquidatable but solvent
    assert!(t.m.is_liquidatable(&l2b));
    t.m.liquidate(&t.liq, &l2b, &u128::MAX);
    let c = t.m.get_claim(&a, &t.xlm);
    assert!(c > 0);
    assert_eq!(t.m.claim(&a, &t.xlm), c);
    t.set_xlm(2_500_000);
    let k = Address::generate(&t.env);
    t.m.settle_backstop_debt(&k, &t.usdc, &u128::MAX, &0);
    assert_eq!(t.bs_shares(&t.usdc), 0);
    let s5 = try_open(&t, &a, false).expect("open after the USDC settle");
    t.assert_ledgers();

    // ---- XLM bad debt (underwater 5x short)
    let s5b = short(&t, &a, 25 * U, 400 * U);
    t.set_xlm(3_500_000); // $0.35: S5b underwater
    t.m.liquidate(&t.liq, &s5b, &u128::MAX);
    assert!(t.bs_shares(&t.xlm) > 0 && t.bs_shares(&t.usdc) == 0);
    for side in [true, false] {
        assert_eq!(try_open(&t, &a, side), Err(ce(Error::BackstopDebtOutstanding)));
    }
    t.m.close_with_repay(&a, &s5);
    t.m.settle_backstop_debt(&k, &t.xlm, &u128::MAX, &0);
    assert_eq!(t.bs_shares(&t.xlm), 0);
    t.set_xlm(2_500_000);
    try_open(&t, &a, true).expect("open after the XLM settle");
    t.assert_ledgers();
}

// ------------------------------------------------------------------ WP6: settle under shortfall
/// WP6 2: the settle's USDC withdrawal is health-checked before its repay. Under a pooled
/// controller shortfall it fails PooledShortfall (view 0); with a little free liquidity it sizes
/// itself to what the (health-checking) mock receipt accepts, and repeating it reaches zero.
#[test]
fn settle_self_sizes_to_pooled_liquidity_and_converges() {
    let t = T::new();
    t.fees(0, 0, 0);
    let (a, b) = (trader(&t), trader(&t));
    let sa = short(&t, &a, 25 * U, 400 * U); // 5x
    let _sb = short(&t, &b, 25 * U, 100 * U); // 2x
    t.set_xlm(3_500_000);
    t.m.liquidate(&t.liq, &sa, &u128::MAX);
    assert!(t.bs_shares(&t.xlm) > 0);
    // An external seizure of USDC pTokens (cash-market liquidation of the pooled account)
    // leaves the account short: 60 USDC pT x 0.75 = $45 against about $54 of XLM debt.
    let ext = Address::generate(&t.env);
    let left = 60 * U;
    let bal = t.ru.get_ptoken_balance(&t.m.address);
    t.ru.seize(&t.m.address, &ext, &(bal - left));
    let (_, short_) = t.ctrl.account_liquidity(&t.m.address);
    assert!(short_ > 0);
    let k = Address::generate(&t.env);
    assert_eq!(t.m.settle_max_ptokens(&t.xlm), 0);
    assert_eq!(
        err(t.m.try_settle_backstop_debt(&k, &t.xlm, &u128::MAX, &0)),
        ce(Error::PooledShortfall)
    );
    // Operator recapitalises 25 pT: a little free liquidity, far less than the full settle.
    admin_usdc_pt(&t, 25 * U);
    t.m.admin_recapitalize(&t.admin, &(25 * U));
    let (liq, short_) = t.ctrl.account_liquidity(&t.m.address);
    assert!(liq > 0 && short_ == 0);
    let mut n = 0;
    let mut chunks = SVec::new();
    while t.bs_shares(&t.xlm) > 0 {
        let view = t.m.settle_max_ptokens(&t.xlm);
        assert!(view > 0);
        let pt0 = t.bs_pt();
        t.m.settle_backstop_debt(&k, &t.xlm, &u128::MAX, &0); // never traps the health check
        assert_eq!(pt0 - t.bs_pt(), view, "view == what the settle withdrew");
        chunks.push(view);
        n += 1;
        assert!(n < 30, "settle does not converge: {chunks:?}");
    }
    assert!(n >= 2, "the first settle must have been capped: {chunks:?}");
    assert_eq!(t.bs_shares(&t.xlm), 0);
}
