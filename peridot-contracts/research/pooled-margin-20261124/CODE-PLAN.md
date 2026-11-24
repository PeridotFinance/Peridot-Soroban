# Margin v2 plan: pooled backstop + isolated positions (read-only design review, 24 Nov 2026)

Base: `.wt-margin-assess/peridot-contracts/contracts/margin-controller/src/contract.rs` (custody-only, 690 lines).
Live: controller `CBMRS7…JRK` (f41e5260), receipts 5630a701, XLM CF 0.55, USDC CF 0.75, max 2 entered markets, receipt `MarginController` hook stays UNSET.
Every resource figure for the new design below is a **projection**, not a measurement.

---

## 0. The binding constraint: transient headroom, not steady state

The live controller health-checks the pooled account on every `borrow`, `withdraw` and pToken `transfer` *mid-flow*, at cash CFs. Per unit of trader collateral C (USD), 3x, 1 % slippage:

| Step | 3x long XLM (hold XLM, owe USDC) | 3x short XLM (hold USDC, owe XLM) |
|---|---|---|
| open: free liquidity needed **at the borrow** (before swap proceeds are deposited) | 2C − 0.55C = **1.45C** | 2C − 0.75C = **1.25C** |
| open: steady-state liquidity change after deposit of proceeds | 0.55·2.98C − 2C = **−0.36C** | 0.75·2.98C − 2C = **+0.235C** |
| stress XLM −20 % (long) / +20 % (short), steady | 0.55·2.98·0.8C − 2C = −0.69C | 0.75·2.98C − 2.4C = −0.165C |
| swap-close: transient drop at `withdraw` (before `repay`) | 0.55 · 2.02C = **1.11C** | 0.75 · 2.02C = **1.52C** |
| liquidate (repay first, owner remainder kept as an internal claim) | +D − CF·bonus·D > 0 always | same |

Consequences:
- The backstop's job is mostly **transient** liquidity for opens and swap-closes. The check must therefore include a free-liquidity floor in USD (`close_reserve_usd`), not only a HF ratio.
- Per-position cap must satisfy `CF_held · max_debt · 1.02 ≤ close_reserve_usd`, otherwise a large position can be un-closable by swap (only `close_with_repay` works).
- Backstop sizing for a long-only book at HF target 1.25: `0.75B ≥ 1.25·2C − 1.639C` → **B ≥ 1.15 × long trader collateral** (≈ 0.38 × long notional). Stress −20 % at HF 1.10 needs B ≥ 1.19C. Shorts need much less (≈ 0.22C at stress HF 1.0). Longs and shorts partly hedge at the pooled level.
- **Liquidation must not transfer the owner's remainder out.** Keep it as an internal `Claim(owner)` so liquidation's net liquidity change is `D(1 − CF·(1+bonus))` > 0, so it can never be blocked by the controller's health check, even when the pooled account is near shortfall.

## 1. Data model

### Instance (all aggregates; zero new footprint keys because the instance entry is already read/written)
| Key | Type | Meaning |
|---|---|---|
| `Bs` | `Backstop { shares: u128, ptokens: u128, pending_shares: u128, pending_ptokens_snapshot: u128 }` | backstop totals in **USDC pTokens**, tracked (never `balanceOf`) |
| `BsDebtShares(asset)` | u128 | debt shares reassigned from underwater positions (residual bad debt owned by the backstop) |
| `BsParams` | `{ cooldown_s: u64, window_s: u64, min_deposit_pt: u128 }` | e.g. 3 d cooldown, 2 d window |
| `Headroom` | `{ hf_target: u128, hf_stress: u128, stress_bps: u128, close_reserve_usd: u128 }` | 1e6 scale; e.g. 1_250_000 / 1_050_000 / 200_000 / R |
| `Fees` | `{ open_bps, close_bps, liq_bonus_liquidator, liq_bonus_backstop }` | 1e6 scale |
| `Caps(asset)` | `CapsV2 { per_position_debt, per_position_collateral, global_debt, global_collateral }` | replaces `Caps` |
| `Oi` | `{ long_held_xlm_pt: u128, short_debt_xlm_shares: u128 }` | OI per side, capped by `OiCaps { long, short }` |
| `CollTotal(asset)` | u128 | sum of trader collateral posted (raw), for `global_collateral` |
| `Ledger(asset)` | u128 | Σ position `held_ptokens` + backstop pt + claims, per market, for reconciliation |
| `ClaimsTotal(asset)` | u128 | owner remainders awaiting `claim` |
| `Impaired` | bool | set by reconcile/insolvent backstop; blocks opens and backstop exits |
| `RiskParams` | add `max_leverage` | new invariant (below) |

`Counter` already exists; add an exported `get_position_counter()` view (keeper bootstrap needs it; the rewrite lacks it).

### Persistent
| Key | Value |
|---|---|
| `Position(u64)` | add `collateral: u128` (raw, for caps/OI unwind), `side: u32` |
| `Backer(Address)` | `{ shares: u128 }` |
| `WithdrawReq(Address)` | `{ shares: u128, ready_at: u64, expires_at: u64 }` (one per backer) |
| `Claim(Address, Address)` | owner, asset → pTokens claimable |

## 2. Entrypoints

Changed:
- `open(user, held, debt, collateral: u128, borrow: u128, min_out: u128, max_fee: u128) -> u64`
  adds: OI/collateral caps, USDC fee, `update_interest` on both receipts, post-trade headroom + stress check, `Impaired` gate.
- `close(user, id, min_out)` — sells for `debt + close_fee`; fee in USDC (long: from proceeds; short: from held USDC pTokens, internal move). No headroom target, only the controller's hard HF ≥ 1.
- `close_with_repay(user, id)` — long: user pays `debt + fee` in USDC; short: fee from held USDC pTokens. No price needed except for fee on long (fee is % of debt → oracle-free). Keep it oracle-free.
- `liquidate(liquidator, id, max_pay: u128)` — at `ltv ≥ liq_ltv` (maintenance), see §3.
- `set_risk(admin, held, open_ltv, liq_ltv, max_leverage)` — **drop `liq_ltv < CF`**. New: `open_ltv < liq_ltv`, `liq_ltv · (1e6 + bonus_liq + bonus_bs) · (1e6 + max_slippage) < 1e18` (solvency at liquidation), `open_ltv ≤ 1 − 1/max_leverage`, `max_leverage ≤ 3e6`.
- `set_caps(admin, asset, CapsV2)`, `set_oi_caps(admin, long, short)`, `set_fees(admin, Fees)`, `set_headroom(admin, Headroom)`, `set_backstop_params(admin, BsParams)`.

New:
- `backstop_deposit(backer, ptokens: u128, min_shares: u128) -> u128` — `receipt_usdc.transfer(backer → me)` (backer auth; controller checks backer's own HF). Mint `ptokens · shares / nav_pt` rounded down. Reverts if `Impaired` or `BsDebtShares(XLM) > 0`.
- `backstop_request_withdraw(backer, shares)`; `backstop_cancel_withdraw(backer)`.
- `backstop_withdraw(backer, min_ptokens: u128) -> u128` — after `ready_at`, before `expires_at`; pays at **withdraw-time** NAV (requested shares keep bearing losses and earning fees); then headroom + stress check on the post-transfer state; reverts `BackstopHeadroom` if it fails. Allows partial (`shares` argument optional variant `backstop_withdraw_partial`).
- `settle_backstop_debt(caller, debt_asset, max_ptokens, min_out)` — permissionless; withdraws backstop USDC pTokens, (XLM only) swaps, repays, burns `BsDebtShares`.
- `reconcile(caller) -> bool` — permissionless; compares `receipt.get_ptoken_balance(me)` with `Ledger(asset)` per market; on deficit sets `Impaired`, pauses opens, emits `margin_impaired`.
- `admin_resolve_impairment(admin, usdc_pt_charge, xlm_pt_charge)` — explicit book entry against backstop; clears `Impaired` only if ledger ≤ balance afterwards.
- `claim(owner, asset)` — transfers `Claim(owner, asset)` pTokens (controller health-checked, but by then the debt is gone).
- Views: `get_position_counter`, `get_backstop() -> Backstop`, `backstop_nav() -> (ptokens_net, shares)`, `pooled_health() -> (hf, hf_down, hf_up, free_usd)`, `get_withdraw_req`, `get_claim`, `is_impaired`.

## 3. Accounting

### Backstop share price (oracle-free, USDC pTokens)
```
nav_pt = Bs.ptokens − bsdebt_usdc_pt
bsdebt_usdc_pt = ceil( debtFromShares(BsDebtShares(USDC)) · 1e6 / rate_usdc )
share_price = nav_pt / Bs.shares
```
- `BsDebtShares(XLM) > 0` would need an oracle → instead deposits/withdrawals **revert until settled** (keeper settles within a cycle).
- First-depositor/donation inflation: tracked `Bs.ptokens` (donated pTokens are ignored), plus 1e6 **dead shares** minted to the contract at the first deposit (admin seeds), `min_deposit_pt`.
- Rounding: shares minted down, pTokens redeemed down, debt rounded up → all dust stays with the pool.

### Fees, all into the backstop as USDC pTokens
- Long open: borrow `B`, deposit `fee = B·open_bps` as USDC pTokens (USDC receipt is already in the footprint), swap `B − fee`. Short open: fee from USDC sale proceeds before deposit. → `Bs.ptokens += fee_pt`.
- Close: long from USDC proceeds; short from held USDC pTokens (internal move, no call).
- Liquidation: liquidator pays `D · (1 + bonus_bs)` in the **debt asset**:
  - long (debt USDC): `D` repays, `D·bonus_bs` deposited to USDC receipt → backstop pTokens;
  - short (debt XLM): backstop cut is taken from held **USDC** pTokens internally (held asset is already USDC).
  - liquidator receives held pTokens worth `D · (1 + bonus_liq)`.
  So the backstop only ever holds USDC pTokens; NAV never needs an oracle.

### Shortfall socialisation (charged to backstop, never to traders)
Position debt is `shares · pooled / total_shares`. Other positions' cost per share must not change.
1. In `liquidate`, if held value `H` < `D·(1+bonus_liq)`: liquidator pays `R = H/(1+bonus_liq)` (debt units, oracle), gets all held pTokens. The remaining `D − R` is expressed as shares `s_res = shares_pos · (D−R)/D` and **moved** from the position to `BsDebtShares(debt_asset)`. `total_shares` is unchanged, so every other position's debt is unchanged. The repaid part burns `shares_pos − s_res`.
2. Backstop NAV drops immediately by the USDC value of `s_res` (oracle-free for USDC debt).
3. `settle_backstop_debt` (keeper, next block): USDC → withdraw backstop pTokens + repay (no swap); XLM → withdraw USDC pTokens, swap, repay.
4. If `nav_pt < 0` (backstop exhausted): `Impaired = true`, opens paused, backstop exits frozen. The uncovered hole is then an **operator** recapitalisation; isolation beyond backstop capacity is not guaranteed (later closers can hit the controller's HF ≥ 1 check, or the pooled account can be liquidated). Caps must keep stress loss ≤ backstop NAV.

### Interest
- Pooled debt per asset grows at the receipt's borrow index; shares spread it pro-rata, so a position pays exactly its own interest (`update_interest` before every read, as the fork found).
- `BsDebtShares` also accrue interest → settle fast.
- Backstop pTokens earn USDC supply APY on top of fees.

## 4. Headroom + stress check (on-chain)

One call: `controller.portfolio(me)` returns per-market rows `(market, pbal, debt, coll_usd_cf, debt_usd)` plus totals, i.e. exactly the controller's own math (`pbal·rate·cf·price`). Precede it with `update_interest()` on both receipts (portfolio uses the lagging view).
```
C_x, D_x = XLM row coll_usd_cf, debt_usd;   C_u, D_u = USDC row
C_u_eff = C_u · (pbal_u − pending_pt) / pbal_u          // opens only: queued backstop exits don't count
base:     (C_x + C_u_eff)        · 1e6 ≥ hf_target · (D_x + D_u)
          (C_x + C_u_eff) − (D_x + D_u) ≥ close_reserve_usd
down:     (C_x·(1e6−s)/1e6 + C_u_eff) · 1e6 ≥ hf_stress · (D_x·(1e6−s)/1e6 + D_u)
up:       (C_x·(1e6+s)/1e6 + C_u_eff) · 1e6 ≥ hf_stress · (D_x·(1e6+s)/1e6 + D_u)
```
HF is linear-fractional in the XLM price, so checking the ±s endpoints covers the interval. Run it after the final deposit in `open`, and after the transfer in `backstop_withdraw` (with `pending_pt` excluding the withdrawing request).

**Footprint cost (projection).** The controller's `borrow` health check in the same transaction already loads both receipts' instances, margin's pToken/borrow balances in both receipts, the CF entries, the prices/PriceCache and the oracle. Footprint is a set, so `portfolio` + 2× `update_interest` should add **≈ 0–2 new keys** (possibly `get_underlying_token` instance-only; maybe the XLM receipt's rate-model entry if `update_interest` on the non-borrowed market touches it). CPU: ≈ +8–15M instructions. All new aggregates are instance keys → 0 new keys. The fee deposit in `open` reuses the USDC receipt/SAC entries → ≈ 0–1 new keys. **Projected open ≈ 108–112 keys, ≈ 125–135M instructions with the real pool.** To be measured in WP5.

### The 100-key gate
It is already exceeded **without** these changes: `close` measured 102 keys with the mock pool, and the real pool adds ~10 per flow (≈108). The stress check is not what breaks it, and caching aggregates cannot fix it. Network limits are 400M instructions, 200 disk reads, 200 writes. Owner decision needed. Options:
1. Raise the internal gate to e.g. **≤ 130 keys / ≤ 200M instructions** (≥ 35 % margin to the network limit), measured on a p30 fork with the real pool. Recommended.
2. Reduce keys: accept trader collateral as already-supplied pTokens (saves the user's SAC balance + one `deposit`, maybe −2…−4); drop the separate pre-borrow `get_user_borrow_balance` in favour of `portfolio` rows; reuse one `get_exchange_rate` per market.
3. Fallback if `portfolio` proves expensive: compute HF from cached `Ledger(asset)` and the receipt reads already made, with CF cached at `set_risk` and refreshed by a permissionless `sync_cf()` (risk: a controller CF cut not yet synced → cached CF must be `min(cached, …)`; only acceptable because the same admin controls both).

## 5. Risks and edge cases

- **Backstop withdrawal blocked by headroom.** Expected when traders use the capacity. Exit is FIFO-less: whoever withdraws first. Mitigate: `pending_shares` excluded from open headroom (new opens can't consume capacity already queued for exit); expiring windows; a view showing withdrawable now.
- **Backer exits before an underwater position is liquidated**: exits at a NAV that doesn't yet show the loss. Bounded by keeper latency; mitigations: withdraw also requires the stress check to pass; a cooldown ≥ keeper SLA; optional exit haircut (e.g. 0.1 %) to the remaining backers.
- **Cash-controller liquidation of the pooled account.** Possible if pooled HF < 1 (gap moves, keeper outage, stale oracle). It seizes pooled pTokens of the liquidator's choosing and repays pooled debt, so the ledger exceeds the balance and debt per share drops for everyone. v1: `reconcile` detects the deficit → `Impaired` → opens and backstop exits frozen → admin books the charge against the backstop. Prevention: HF target 1.25, stress check, keeper liquidating positions at maintenance well above controller liquidation.
- **Our own cash-liquidator will see the margin contract** via `borrow_event` and may plan to liquidate the pooled account. Add the margin contract address to a `neverLiquidateBorrowers` list (alert-only).
- **Oracle staleness.** `get_price_usd` returns None → open, swap-close, liquidate, backstop withdraw and reconcile-with-price revert. `close_with_repay` and `claim` stay oracle-free and always work. Liquidations are blocked while stale, so the keeper must keep `cache_price` fresh (cash-liquidator already does).
- **Rounding.** Debt up, shares on open up, backstop shares down; last debt-share holder pays the full pooled balance, and that might now be `BsDebtShares` → settle handles it.
- **Griefing.** Dust positions (`min_collateral`), storage spam (opener pays persistent rent), withdraw-request squatting (one request per backer, expiry), pToken donations (ignored by tracked accounting), front-running liquidations (harmless), sandwiching the pool swap (oracle floor + 1 % slippage).
- **Transient liquidity** (§0): a fully used backstop blocks swap-closes; `close_with_repay` remains.
- **No funding rate**: an imbalanced book (all long) concentrates directional risk on backers; OI caps per side are the only control in v1.
- **Utilisation**: margin borrowing raises cash-market utilisation and lender rates; the receipts' 70 % cash reserve and borrow caps still bound it.
- **Live-receipt idle-interest finding** (new borrowers charged pre-borrow interest): margin works around it with `update_interest`; it is still an open receipt issue.

## 6. Keeper

Base it on **cash-liquidator** (single signer, key-file handling, fee budget, `cache_price` refresh, simulate→journal→send, USDC/XLM inventory). A second process with the same key would collide on sequence numbers, so add a margin pass to the same process.
- Port from **margin-liquidator**: discovery by `get_position_counter` bootstrap + `margin_open/close/liquidate` events (`state.mjs`).
- Each cycle: `is_liquidatable(id)` per open position → simulate `liquidate(keeper, id, max_pay)` → profit check (bonus − fees) → send. Then `settle_backstop_debt` whenever `BsDebtShares > 0`. Then `reconcile` (simulate only; send if it returns true). Alert when `pooled_health().hf < hf_target` or `hf_down < 1.10`.
- Inventory: liquidations need `D·(1+bonus_bs)` in the debt asset, same as cash liquidations; size the USDC/XLM inventory to the margin per-position caps.
- Backstop withdrawals stay user-driven; the keeper doesn't service them.
- Phase 2 (not in this estimate): delegated take-profit/stop closes.

## 7. Test plan

- **Unit** (native, in-crate; there are zero today): compiled live receipt + controller WASM (`.wt-cash-upgrade/.../artifacts`, hashes 5630a701 / f41e5260) + mock pool.
  - backstop: mint/redeem rounding, dead shares, donations ignored, cooldown/window/expiry, partial exit, exit blocked by headroom, deposit frozen while XLM bad debt;
  - positions: 3x long/short open at the boundary, `max_leverage`/`set_risk` invariants, caps (per position, global collateral, OI per side), fees in USDC on all four paths;
  - liquidation: at maintenance, profitable split, underwater → `BsDebtShares` reassignment, **other positions' debt and held pTokens unchanged** (property test over random open/close/price/interest sequences: Σ position debt + backstop debt = pooled debt ± rounding; no trader's `held_ptokens` or debt per share changes from another's loss);
  - headroom/stress formula against an independent off-chain computation; `Impaired` paths; `close_with_repay` and `claim` with a stale oracle.
- **Mainnet fork** (`harness/margin-fork`): protocol-30 host; real Aquarius pool contract entries in the snapshot (verify the swap auth shape); long + short open concurrently; backstop deposit/withdraw; XLM −20 %/+20 %/−40 % gaps; bad debt settle (USDC and XLM); simulate a controller liquidation of the pooled account → `reconcile`; strict (non-mock) auth trees.
- **Resource measurement**: same harness table (instructions, mem, disk/mem reads, writes) for open long, open short, close (both), close_with_repay, liquidate (solvent + underwater), backstop_withdraw, settle_backstop_debt, reconcile; then a read-only Mainnet `simulateTransaction` once deployed (opens paused → simulate with the admin unpaused only on the fork).
- **Keeper**: unit tests for plan/profit/alert logic; dry run against the fork via a local RPC or Testnet deployment.
- **Security**: Almanax scan, zero High/Medium.

## 8. Work packages and estimate (start Thu 26 Nov 2026)

| WP | Work | Days | Parallel? | Dates |
|---|---|---|---|---|
| WP0 | Freeze spec: storage keys, structs, errors, signatures, parameters | 0.5 | main | Thu 26 am |
| WP1 | Backstop module (shares, NAV, request/withdraw, dead shares) | 1.5 | subagent A | Thu 26 pm – Fri 27 |
| WP2 | Positions v2: leverage/set_risk, USDC fees, caps/OI, liquidation split, claims, bad-debt reassignment, settle, counter view | 2.0 | main (critical) | Thu 26 pm – Mon 30 |
| WP3 | Headroom/stress via `portfolio`, reconcile/Impaired | 1.0 | subagent B | Thu 26 pm – Fri 27 |
| WP4a | Prereq: real Aquarius pool entries in the fork + p30 host + verify swap auth on the *existing* contract | 1.0 | subagent C | Thu 26 – Fri 27 |
| WP4 | Native unit + property tests | 2.5 | subagent D from Fri, main integrates | Fri 27 – Tue 1 |
| WP5 | Fork scenarios + resource table | 1.5 | main (critical) | Tue 1 – Wed 2 |
| WP6 | Footprint remediation buffer (if > agreed gate) | 0–1.5 | main | Wed 2 – Thu 3 |
| WP7 | Keeper margin pass in cash-liquidator + tests + dry run | 2.0 | subagent E | Mon 30 – Wed 2 |
| WP8 | Ceremony + read-back scripts, parameter/backstop sizing note | 1.0 | subagent F | Mon 30 – Tue 1 |
| WP9 | Almanax scan + fixes + rescan | 1.5 | main | Thu 3 – Fri 4 |
| WP10 | Owner sign-off, Ledger ceremony, $5 canary, backstop seed, keeper live | 1.0 | owner + main | Mon 7 Dec |

- **Effort:** about 15–17 person-days. **Elapsed:** about 7 working days.
- **Critical path:** WP0 → WP2 → WP4 (integration) → WP5 → (WP6) → WP9 → WP10.
- **Dates:** code-complete and fork-measured Wed 2 Dec; scan Thu 3 – Fri 4 Dec; earliest supervised pilot **Mon 7 Dec**. If the footprint needs a redesign or the scan returns High/Medium findings: Wed 9 – Thu 10 Dec.
- **Assumptions:**
  - The owner decides the footprint gate by Wed 2 Dec.
  - Almanax turns a scan around in ≤ 1 day.
  - The Ledger signer is available on Mon 7 Dec.
  - The parameters are signed off.
  - No receipt or controller change is needed: custody-only, hook unset.
  - Pilot caps of roughly $200 trader collateral, and a backstop seed of ≥ 1.2× trader collateral from a team wallet.
