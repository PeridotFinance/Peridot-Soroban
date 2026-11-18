"""Copy contracts/simple-peridottroller/src (build commit 4c98bdd3) into ./src and apply the
CL-collateral-group patches. Every behavioural patch is gated by `cfg!(feature = "group")`. A build
WITHOUT the feature is NOT byte-identical to the deployed controller: it still exports the two group
setters (they assert at runtime) and is 63,529 bytes. The regression rows therefore use the DEPLOYED
controller bytes 847894a6, not a build of this copy.
usage (from peridot-contracts/): python3 -I prototypes/cl-collateral-group/controller/apply-patches.py
"""
import os, shutil, sys
here = os.path.dirname(os.path.abspath(__file__))
root = os.path.abspath(os.path.join(here, '../../..'))
up = os.path.join(root, 'contracts/simple-peridottroller/src')
src = os.path.join(here, 'src')
for f in ['constants.rs', 'contract.rs', 'events.rs', 'reflector.rs', 'storage.rs']:
    shutil.copy(os.path.join(up, f), os.path.join(src, f))
p = os.path.join(src, 'contract.rs')
s = open(p).read()

def rep(old, new, count=1):
    global s
    n = s.count(old)
    assert n == count, f'expected {count} occurrence(s), found {n}: {old[:90]!r}'
    s = s.replace(old, new)

# G0. Group-only instance keys (instance entry is already in every footprint: no extra entry).
rep('#[contract]\npub struct SimplePeridottroller;',
    '''#[soroban_sdk::contracttype]
#[derive(Clone)]
pub enum GroupKey {
    /// The single CL sleeve market whose liquidation-only valuation is dispatched.
    Sleeve,
    /// Liquidation reads only a fresh cached price (cold oracle path unreachable).
    LiqCacheOnly,
}
#[contract]
pub struct SimplePeridottroller;''')

# G1. Admin setters (prototype: one-time sleeve binding; cache-only switch for measurement).
rep('    pub fn set_market_cf(env: Env, market: Address, cf_scaled: u128) {',
    '''    pub fn group_set_sleeve(env: Env, market: Address) {
        assert!(cfg!(feature = "group"), "group build only");
        require_admin(env.clone());
        Self::require_market_supported(&env, &market);
        assert!(!env.storage().instance().has(&GroupKey::Sleeve), "sleeve fixed");
        env.storage().instance().set(&GroupKey::Sleeve, &market);
    }
    pub fn group_set_liq_cache_only(env: Env, on: bool) {
        assert!(cfg!(feature = "group"), "group build only");
        require_admin(env.clone());
        env.storage().instance().set(&GroupKey::LiqCacheOnly, &on);
    }
    pub fn set_market_cf(env: Env, market: Address, cf_scaled: u128) {''')

# G2. Entered-market cap of 2 in the group (sleeve + USDC lending market): bounded health loop.
rep('            if entered.len() >= MAX_USER_MARKETS {',
    '            if entered.len() >= (if cfg!(feature = "group") { 2 } else { MAX_USER_MARKETS }) {')

# G3. Liquidation-context health sum: the sleeve answers get_liquidation_account_snapshot.
rep('''    fn sum_positions_usd_for_markets(
        env: Env,
        user: Address,
        exclude_market: Option<Address>,
        markets: Vec<Address>,
        refresh_market_state: bool,
    ) -> (u128, u128, bool, bool) {''',
    '''    fn sum_positions_usd_for_markets(
        env: Env,
        user: Address,
        exclude_market: Option<Address>,
        markets: Vec<Address>,
        refresh_market_state: bool,
    ) -> (u128, u128, bool, bool) {
        Self::sum_positions_usd_for_markets_ctx(env, user, exclude_market, markets, refresh_market_state, false)
    }
    fn group_sleeve(env: &Env) -> Option<Address> {
        if cfg!(feature = "group") { env.storage().instance().get(&GroupKey::Sleeve) } else { None }
    }
    fn group_liq_cache_only(env: &Env) -> bool {
        cfg!(feature = "group") && env.storage().instance().get(&GroupKey::LiqCacheOnly).unwrap_or(false)
    }
    fn try_cached_price(env: &Env, token: &Address) -> Option<(u128, u128)> {
        storage::bump_price_cache_ttl(env, token);
        let cached = env.storage().persistent().get::<_, CachedPrice>(&DataKey::PriceCache(token.clone()))?;
        if cached.price > 0 && cached.scale > 0 && Self::cached_price_fresh(env, cached.timestamp, cached.resolution) {
            Some((cached.price, cached.scale))
        } else {
            None
        }
    }
    fn sum_positions_usd_for_markets_ctx(
        env: Env,
        user: Address,
        exclude_market: Option<Address>,
        markets: Vec<Address>,
        refresh_market_state: bool,
        liq: bool,
    ) -> (u128, u128, bool, bool) {
        let liq_sleeve = if liq { Self::group_sleeve(&env) } else { None };
        let liq_cache_only = liq && Self::group_liq_cache_only(&env);''')
rep('''            let accrued_snapshot = cfg!(feature = "lp-zero-peri") && refresh_market_state;
            let snapshot = env.try_invoke_contract::<(u128, u128, u128, Address), InvokeError>(
                &m,
                &Symbol::new(
                    &env,
                    if accrued_snapshot {
                        "get_accrued_account_snapshot"
                    } else {
                        "get_account_snapshot"
                    },
                ),''',
    '''            let accrued_snapshot = cfg!(feature = "lp-zero-peri") && refresh_market_state;
            let snapshot = env.try_invoke_contract::<(u128, u128, u128, Address), InvokeError>(
                &m,
                &Symbol::new(
                    &env,
                    if accrued_snapshot {
                        "get_accrued_account_snapshot"
                    } else if liq_sleeve.as_ref() == Some(&m) {
                        "get_liquidation_account_snapshot"
                    } else {
                        "get_account_snapshot"
                    },
                ),''')
rep('''            // Get price — fail-closed.
            let (price, scale) = match Self::try_require_price(&env, &token) {''',
    '''            // Get price — fail-closed.
            let (price, scale) = match if liq_cache_only { Self::try_cached_price(&env, &token) } else { Self::try_require_price(&env, &token) } {''')

# G4. liquidate_internal: liquidation-context health, liquidation snapshot of the sleeve, cache-only prices.
rep('''            let (known_collateral_usd, known_borrow_usd, indeterminate, collateral_indeterminate) =
                Self::sum_positions_usd(env.clone(), borrower.clone(), None);
            let account_shortfall = known_borrow_usd.saturating_sub(known_collateral_usd);
            let account_liquidity = known_collateral_usd.saturating_sub(known_borrow_usd);
            // Ensure borrower is undercollateralized using known (deterministic) positions.''',
    '''            let (known_collateral_usd, known_borrow_usd, indeterminate, collateral_indeterminate) =
                if cfg!(feature = "group") {
                    let markets = Self::get_user_markets(env.clone(), borrower.clone());
                    Self::sum_positions_usd_for_markets_ctx(env.clone(), borrower.clone(), None, markets, false, true)
                } else {
                    Self::sum_positions_usd(env.clone(), borrower.clone(), None)
                };
            let account_shortfall = known_borrow_usd.saturating_sub(known_collateral_usd);
            let account_liquidity = known_collateral_usd.saturating_sub(known_borrow_usd);
            // Ensure borrower is undercollateralized using known (deterministic) positions.''')
rep('''        let lp_collateral_snapshot: Option<(u128, u128, Address)> =
            if cfg!(feature = "lp-zero-peri") {''',
    '''        let lp_collateral_snapshot: Option<(u128, u128, Address)> =
            if cfg!(feature = "lp-zero-peri") || Self::group_sleeve(&env).as_ref() == Some(&collateral_market) {''')
rep('''        let (pb, sb) = Self::require_price(env.clone(), borrow_token.clone());
        let (pc, sc) = Self::require_price(env.clone(), coll_token.clone());''',
    '''        let cache_only = Self::group_liq_cache_only(&env);
        let (pb, sb) = if cache_only { Self::try_cached_price(&env, &borrow_token).expect("liquidation: fresh cached price required") } else { Self::require_price(env.clone(), borrow_token.clone()) };
        let (pc, sc) = if cache_only { Self::try_cached_price(&env, &coll_token).expect("liquidation: fresh cached price required") } else { Self::require_price(env.clone(), coll_token.clone()) };''')
# advisory redeem preview: the sleeve's seized units are not redeemable cash; skip the extra quote (as lp-zero-peri).
rep('''        let max_redeem_ptokens = if cfg!(feature = "lp-zero-peri") {''',
    '''        let max_redeem_ptokens = if cfg!(feature = "lp-zero-peri") || Self::group_sleeve(&env).as_ref() == Some(&collateral_market) {''')

# G5. Item 2: withdrawing OTHER collateral never reads an invalid sleeve as zero; it reverts.
rep('''        let (collateral_usd, _borrows, indeterminate, _collateral_indeterminate) =
            Self::sum_positions_usd(env, user, Some(exclude_market));
        if indeterminate {
            0u128''',
    '''        let (collateral_usd, _borrows, indeterminate, _collateral_indeterminate) =
            Self::sum_positions_usd(env, user, Some(exclude_market));
        if indeterminate && cfg!(feature = "group") {
            panic!("collateral valuation invalid");
        }
        if indeterminate {
            0u128''')

# ---- 18 Nov 2026 build (feat/cl-collateral-group): structural isolation, condition (c) and D2 ----
# G6. Condition (c): only the bound CL sleeve may carry a collateral factor, so no account can mix CL
#     collateral with cash collateral (the USDC lending market stays CF 0, debt-only).
rep('''    pub fn set_market_cf(env: Env, market: Address, cf_scaled: u128) {
        bump_core_ttl(&env);
        require_admin(env.clone());''',
    '''    pub fn set_market_cf(env: Env, market: Address, cf_scaled: u128) {
        bump_core_ttl(&env);
        require_admin(env.clone());
        if cfg!(feature = "group") {
            assert!(Self::group_sleeve(&env).as_ref() == Some(&market), "group: only the CL sleeve is collateral");
        }''')
# G7. The group lists at most two markets: the CL sleeve and one USDC lending market.
rep('''        markets.set(market.clone(), true);
        env.storage()
            .persistent()
            .set(&DataKey::SupportedMarkets, &markets);''',
    '''        if cfg!(feature = "group") {
            assert!(markets.contains_key(market.clone()) || markets.len() < 2, "group is limited to two markets");
        }
        markets.set(market.clone(), true);
        env.storage()
            .persistent()
            .set(&DataKey::SupportedMarkets, &markets);''')
# G8. D2: no static price fallback in the group; USDC is priced only by the configured feed.
rep('''    pub fn set_price_fallback(env: Env, token: Address, price: Option<(u128, u128)>) {
        bump_core_ttl(&env);
        require_admin(env.clone());''',
    '''    pub fn set_price_fallback(env: Env, token: Address, price: Option<(u128, u128)>) {
        bump_core_ttl(&env);
        require_admin(env.clone());
        assert!(!(cfg!(feature = "group") && price.is_some()), "group: no price fallback");''')
# G9. No margin liquidation controller in the group.
rep('''    pub fn set_margin_liquidation_ctrl(env: Env, controller: Address, allowed: bool) {
        bump_core_ttl(&env);
        require_admin(env.clone());''',
    '''    pub fn set_margin_liquidation_ctrl(env: Env, controller: Address, allowed: bool) {
        bump_core_ttl(&env);
        require_admin(env.clone());
        assert!(!(cfg!(feature = "group") && allowed), "group: no margin liquidation");''')
# G10. Emergency-exit path: an account with NO debt in any entered group market may exit the CL
#      sleeve while holding units (oracle-free: only borrow balances are read). The StgV2 receipt
#      refuses the no-claim emergency exit while the owner is entered (100-entry limit).
rep('''        if pbal > 0 && market_cf > 0 {
            panic!("Cannot exit with collateral in market");
        }''',
    '''        if pbal > 0 && market_cf > 0 && !(cfg!(feature = "group") && Self::group_no_debt(&env, &user, &entered)) {
            panic!("Cannot exit with collateral in market");
        }''')
rep('''    fn group_liq_cache_only(env: &Env) -> bool {''',
    '''    fn group_no_debt(env: &Env, user: &Address, entered: &Vec<Address>) -> bool {
        for m in entered.iter() {
            match env.try_invoke_contract::<u128, soroban_sdk::InvokeError>(&m, &Symbol::new(env, "get_user_borrow_balance"), (user.clone(),).into_val(env)) {
                Ok(Ok(0)) => {}
                _ => return false,
            }
        }
        true
    }
    fn group_liq_cache_only(env: &Env) -> bool {''')
open(p, 'w').write(s)
print('patched', p)
