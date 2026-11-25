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
   (USDC) + `ClaimsTotal(asset)`. Whoever mints/receives or sends/burns pTokens updates it in the same call
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
5. **Pool check (WP3)** per SPEC §4; `close_reserve_usd = 0` → derive `1.01 × max(cf_xlm × maxdebt_long, cf_usdc ×
   maxdebt_short)` with `maxdebt = per_position_equity_usd × (max_leverage − 1)`.
6. **Backstop (WP1)**: CODE-PLAN §3 (oracle-free NAV in USDC pTokens; dead shares; rounding against the backer; exit
   request → 72 h → 48 h window; frozen while `BsDebtShares(XLM) > 0` or `Impaired`).
7. **Events**: single-tuple topics `(Symbol,)`: `margin_open`, `margin_close`, `margin_liquidate`, `margin_bad_debt`,
   `margin_claim`, `bs_deposit`, `bs_request`, `bs_cancel`, `bs_withdraw`, `bs_settle`, `margin_impaired`.
8. Custody-only: never call receipt hook functions; `try_invoke` not needed for our own two receipts.
