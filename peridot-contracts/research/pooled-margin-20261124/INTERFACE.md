# Margin v2 — WP0 interface freeze (25 Nov 2026)

Branch `feat/margin-v2` (from `assess/margin-20261124` 5ab616e). SDK: workspace soroban-sdk 25.3.x (lock 25.3.2).
Spec: `SPEC.md` rev 2 (wins over `CODE-PLAN.md` where they differ: 5×, per-position maintenance, 4 % bonus 3/1, funding
+2 % APR, 72 h cooldown). Code: `contracts/margin-controller/src/`.

| File | Owner | Content |
|---|---|---|
| `types.rs` | frozen (integrator) | errors, `Key`, structs, constants |
| `common.rs` | integrator | live ABIs (+ `portfolio`, `account_liquidity`), helpers moved from v0.2, `ledger_*`, `usdc()`, `xlm()`, `fees()`, `is_impaired()`; **WP2 may edit** `position_debt`, `ltv`, `load`, `store`, `finish` |
| `contract.rs` | WP2 | constructor/admin (done), `set_risk/set_caps/set_fees/set_max_slippage`, `open/close/close_with_repay/liquidate/claim`, views |
| `backstop.rs` | WP1 | `credit_pt`, `assign_bad_debt`, `nav_pt`, `pending_pt`, `value_usd`; backstop entrypoints + views |
| `headroom.rs` | WP3 | `pool_check(Check)`, `required_backstop_usd`, `require_not_impaired`; `set_headroom`, `pooled_health`, `reconcile`, `admin_resolve_impairment` |

Each WP replaces only its own `unimplemented!` bodies. A needed signature change → ask the integrator; don't edit
another WP's file. Units: amounts raw 7-decimal; fractions 1e6; USD values "usd7" (`usd()` on 7-dp tokens).

## Rules every WP relies on

1. **Ledger.** `Ledger(asset)` = pTokens this contract accounts for = Σ open positions' `held_ptokens` + `Bs.ptokens`
   (USDC) + `BsXlmPt` (XLM, WP6) + `ClaimsTotal(asset)`. Whoever mints/receives or sends/burns pTokens updates it in the same call
   (`ledger_add/sub`). Donations are never counted. `reconcile` compares it to `get_ptoken_balance(me)`.
2. **Debt shares.** `DebtShares(asset)` = position shares only; `BsDebtShares(asset)` = backstop bad-debt shares.
   Debt per share = `pooled_debt / (DebtShares + BsDebtShares)`. **WP2 must change `position_debt` to divide by the sum**,
   so moving shares into the backstop bucket leaves every other position's debt unchanged.
3. **Backstop holds USDC pTokens only.** All fees, funding and the backstop bonus arrive as USDC: deposit USDC into the
   USDC receipt (`deposit_self`) or move already-held USDC pTokens internally, then `ledger_add` (if newly minted) and
   `backstop::credit_pt`.
4. **Position economics (WP2).**
   - Open: `equity_usd` = collateral value; `notional_usd` = held value after the trade; leverage = notional/equity
     ≤ `RiskParams.max_leverage`; post-trade LTV = debt/held; `liq_ltv = (1e6 + LTV_open) / 2` stored in the position.
   - Open fee = `open_fee × notional` in USDC (long: taken from the borrowed USDC before the swap; short: taken from
     the XLM→USDC sale proceeds before the deposit). `MaxFee` if above `max_fee`.
   - Funding = `funding_apr × debt_usd × (now − funding_ts) / YEAR_SECS`, in USDC, charged at close/close_with_repay/
     liquidate; counted as debt in `get_position_ltv`.
   - Close fee = `close_fee × notional_usd` in USDC (long: from USDC proceeds; short: from held USDC pTokens).
   - Liquidate at `LTV ≥ liq_ltv`: liquidator pays `D` (+ for long `D × bonus_backstop` in USDC); gets held pTokens worth
     `D × (1 + bonus_liquidator)`; backstop gets `D × bonus_backstop` (short: from held USDC pTokens) + funding; owner
     remainder → `Claim(owner, held)` (never transferred in `liquidate`). Underwater: CODE-PLAN §3 (residual shares →
     `assign_bad_debt`).
   - Caps per side (held asset): equity per position (`min_equity_usd..per_position_equity_usd`), side equity, side OI
     (`SideTotals`, decreased by exactly what the position added).
   - Open order: `require_not_impaired` → paused check → caps → trade → `headroom::pool_check(Check::Open)` last.
   - Close / close_with_repay / liquidate / claim never check the pause or `Impaired`.
5. **Pool check (WP3)** per SPEC §4; `close_reserve_usd = 0` → derive `1.015 × max(cf_xlm × maxdebt_long, cf_usdc ×
   maxdebt_short)` with `maxdebt = per_position_equity_usd × (max_leverage − 1)` (1.01 before WP6).
6. **Backstop (WP1)**: CODE-PLAN §3 (oracle-free NAV in USDC pTokens; dead shares; rounding against the backer; exit
   request → 72 h → 48 h window; frozen while `BsDebtShares(XLM) > 0` or `Impaired`).
7. **Events**: single-tuple topics `(Symbol,)`: `margin_open`, `margin_close`, `margin_liquidate`, `margin_bad_debt`,
   `margin_claim`, `bs_deposit`, `bs_request`, `bs_cancel`, `bs_withdraw`, `bs_settle`, `margin_impaired`.
8. Custody-only: never call receipt hook functions; `try_invoke` not needed for our own two receipts.

## Integration changes (25 Nov, WP4)

Shims removed: `contract.rs` calls `headroom::require_not_impaired` / `pool_check(Check::Open)` and
`backstop::credit_pt` / `assign_bad_debt` directly; `headroom.rs` uses `backstop::value_usd` / `pending_pt`;
`backstop::exit_check` always runs `pool_check(Check::BackstopExit)`. Tests run the real modules
(`src/test_integration.rs` for cross-module flows). Behaviour changes:

1. **Exhaustion → Impaired.** After an underwater `liquidate` assigns bad debt and `finish`es the position,
   `backstop::impair_if_exhausted` checks `nav_pt(false) < 0` (XLM bad debt at the oracle); if so it sets
   `Impaired`, pauses opens and emits `margin_impaired` (same event struct as `reconcile`, `caller` = liquidator).
   The check runs after `finish` because until then the divisor still holds the burned shares and the backstop
   debt reads low. Shared helper `headroom::mark_impaired` emits only when the flag is newly set.
2. **`reconcile` return value:** `true` only when that call newly sets `Impaired`. A deficit found while already
   impaired returns `false` and emits nothing; `is_impaired()` is the state view. Keeper: simulate each cycle,
   send only on `true`.
3. **New `admin_recapitalize(admin, ptokens) -> u128`** (Bs.ptokens after): USDC pTokens admin → contract,
   added to `Bs.ptokens` and `Ledger(USDC)`, **no shares minted**. Works at NAV ≤ 0, while `Impaired` and while
   XLM bad debt is outstanding (no NAV or oracle read). Needs an existing backstop (`BackstopEmpty` before the
   seed). Event `bs_recap (admin, ptokens, bs_ptokens)`.
4. **`admin_resolve_impairment(admin, usdc_pt_charge, xlm_pt_restore)`:** the XLM argument no longer writes
   `Ledger(XLM)` down. It **pulls `xlm_pt_restore` XLM pTokens from the admin** and leaves the Ledger unchanged,
   so the last XLM closers/claims can still exit in full (a Ledger-only write-down left `Ledger(XLM)` below held +
   claims, and the pTokens were physically missing). The USDC charge is unchanged (backstop loss: `Bs.ptokens`
   and `Ledger(USDC)` both drop). `Impaired` clears only if both ledgers ≤ balances **and** `nav_pt(false) ≥ 0`
   (so recapitalise first). Event `margin_resolved` field renamed `xlm_pt_charge` → `xlm_pt_restored`.
   **Owner decision open:** backstop-funded XLM restoration (swap backstop USDC → XLM pTokens) is not implemented;
   today the XLM restore comes from the operator/treasury.
5. **New `backstop_clear_expired(caller, backer)`:** permissionless (caller auth only); returns an expired
   request's shares to the backer (`now > expires_at`; live → `BadParams`, none → `NoRequest`), so stale
   requests stop counting in `pending_pt` for opens. Event `bs_cancel`.
6. **`MaxSlippage` default 15_000 (1.5 %)** (`types::DEFAULT_MAX_SLIPPAGE`; constructor and every fallback).
   The real-pool fork saw a close fill 1.02 % below Reflector. 5× solvency: 0.9 × 1.04 × 1.015 = 0.950 < 1
   (tested; a 9.5 % total bonus is now refused at the default, accepted at 1 %).
7. **`credit_pt` never panics:** saturating; an empty backstop (no shares) accumulates the fee pTokens, which the
   later seed's shares own pro rata with the dead shares.

Known gaps carried to WP5 (superseded by "WP6 changes" below): once bad debt exceeds the backstop, the pooled controller HF can fall below 1 and the
live controller then blocks close (withdraw), liquidate/claim (pToken transfer) and settle (withdraw before
repay); the integration test keeps HF ≥ 1 with short-side USDC. Large `settle_backstop_debt` amounts need
chunking via `max_ptokens` (the withdraw is health-checked before the repay). `harness/margin-fork` still drives
the v0.2 ABI by name and must be ported to v2.

## WP6 changes (25 Nov)

Resource fix for `open` and the two gaps the WP5 fork found. Build sha256 `b1cb1cb2…766f` (67,488 B); results in
`WP5-FORK-RESULTS.md` ("WP6 changes").

1. **Opens refused while backstop bad debt is unsettled.** `open` fails `BackstopDebtOutstanding` (#25, reused; its
   meaning now also covers this) while `BsDebtShares(USDC) > 0` or `BsDebtShares(XLM) > 0`, checked right after the
   Impaired/pause checks. This mirrors the backstop-deposit freeze for XLM bad debt. Close, close_with_repay, liquidate,
   claim and `settle_backstop_debt` are not gated. The keeper must settle to **exactly zero shares** before opens
   resume; the XLM settle cannot leave a dust tail (item 4).
2. **One read per value in `open` → `pool_check`.** The open path no longer repeats any of these reads:
   - Prices are read once and passed to `oracle_min_out_p` (new in `common.rs`).
   - The USDC exchange rate is read once before the trade, and the held rate once after it.
   - The position debt comes from the borrow's own `get_user_borrow_balance` via `share_debt` (new), with no second
     `update_interest` + balance pair.
   - The held receipt's pToken balance is read before the first and after the last held deposit (`deposit_raw`, new),
     not around every deposit.
   - `pool_check_with(env, Check::Open, Some(OpenPre { usdc_rate, usdc_price }))` skips both `update_interest` calls,
     because both receipts already accrued in the same invocation. `Check::BackstopExit` still accrues both receipts.
   - The backstop NAV is read once per pool check (`pending_pt_at` / `value_usd_at`). The pre-trade USDC rate can only
     understate the backstop value, which is the safe side.
   - Short open: the open fee's pTokens are the fee's pro-rata share of the single collateral + proceeds mint.
3. **Self-sizing settle, and a keeper view.** `settle_backstop_debt` caps its USDC withdrawal at the pooled free
   liquidity, because the live controller health-checks the withdrawal before the repay:
   - The cap is `account_liquidity(me)`, after accruing the receipt the settle has not yet accrued, converted at the USDC
     CF, price and exchange rate, less a 1 % haircut.
   - With no free liquidity the settle fails with the **new error `PooledShortfall` (#35)**. Recapitalise first, or let
     pay-in-first exits restore liquidity.
   - Every settle frees liquidity, so the keeper repeats it until `get_bs_debt_shares` is 0.
   - **New view `settle_max_ptokens(debt_asset) -> u128`** returns the USDC pTokens a `max_ptokens = MAX` settle would
     withdraw now. 0 means nothing to settle, the backstop's XLM pTokens cover the debt, or a shortfall.
4. **XLM settle sizing and surplus.**
   - The swap input is the pool's `estimate_swap_strict_receive(USDC→XLM, want)` + 1, with `want = debt + 0.05 % + 10
     stroops`, at least 0.1 XLM. The quote is untrusted and called with `try_invoke`. It is capped at the old bound
     (oracle value of `want` + max slippage); without a quote that bound is used.
   - The output floor is unchanged: oracle less max slippage, and the caller's `min_out`.
   - XLM left after the repay is deposited to the XLM receipt and tracked as **backstop XLM pTokens (new key `BsXlmPt`,
     appended to `Key`; in `Ledger(XLM)`)**. They are not counted in the NAV, which is conservative.
   - The next XLM settle first repays from `BsXlmPt` (withdraw + repay, no swap), then swaps for the rest.
   - Only sub-pToken dust can stay idle. The `bs_settle` event's last field is now the XLM surplus that was deposited.
   - **New view `get_bs_xlm_ptokens() -> u128`.**
5. **Close reserve multiplier 1.015** (`headroom::RESERVE_MARGIN`), matching the 1.5 % default slippage (SPEC §4).
6. **Pooled controller HF < 1, traced on the fork.** The live controller's check, inside the receipt, rejects every
   pToken removal from the pooled account that would leave it short:
   - The `withdraw` / `transfer` traps `UnreachableCodeReached` after `get_collateral_excl_usd`, `get_borrows_excl`, the
     XLM price and the CF.
   - Long `close_with_repay` already repays first. It fails on the **XLM receipt `transfer` of the held pTokens to the
     owner**, because the pooled account's other debt needs that collateral.
   - `claim` fails on the same `transfer`.
   - Swap-`close` fails on the **XLM receipt `withdraw`** of the pTokens it sells, before any swap or repay. It cannot
     repay first without the proceeds.
   - So no reordering helps, and the contract does not bypass the lender protection. Pay-in-first flows still work:
     `liquidate` and the short's `close_with_repay`.
   - The recovery order is: liquidate, then self-sized settles (repeat), then the blocked exits go through. Exercised on
     the fork.
   - Open owner decision, not implemented: parking a failed exit's collateral as a `Claim`.

