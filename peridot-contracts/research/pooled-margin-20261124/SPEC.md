# Pooled margin v2 — spec (24 Nov 2026; code starts Thu 26 Nov)

Custody-only (receipt MarginController hook stays **unset**; lenders never call into margin). Builds on the custody-only
rewrite in `contracts/margin-controller/src/contract.rs` (branch `assess/margin-20261124`). Contract-level detail:
`CODE-PLAN.md` next to this file.

## 1. Roles

| Role | Deposits | Earns | Carries |
|---|---|---|---|
| **Backstop backer** (opt-in) | USDC pTokens only → backstop shares | open/close fees, backstop cut of liquidation bonus, USDC lending APY on its pTokens | shortfall (bad debt) of any position; exits after cooldown |
| **Trader** (isolated position) | collateral pTokens (XLM for long, USDC for short) | P&L of own position | only own losses; **never** another trader's |

- Bad debt of one position moves to the backstop's own debt bucket → backstop share price drops; no other trader's debt
  or holdings change. If the backstop is exhausted the contract flags `Impaired`, pauses opens, and needs recapitalisation.
- Backstop is USDC-only so its value never depends on the XLM price (XLM pTokens would lose value exactly when longs blow up).
- Treasury seeds the backstop at launch (§5).

## 2. Inputs used for sizing

**XLM moves** (Binance XLMUSDT; hourly 2018–2026, 5-min 2023–2026; worst adverse move from window open):

| Window | p99 | p99.9 | max |
|---|---|---|---|
| 5 min | 1.0 % | 2.2 % | 36.8 % |
| 15 min | 1.7 % | 3.7–4.1 % | 53.5 % (10 Oct 2025) |
| 1 h | 3.5–4.3 % | 7.7–9.9 % | 53–63 % |
| 4 h | 8.3–8.9 % | 18–20 % | 56 % / +103 % |

≥ 15 % within 1 h happens ~2.5×/year each direction; ≥ 20 % within 4 h ~2–4×/year.

**Aquarius depth** (Mainnet simulation, ledger ~65649470; best pool = concentrated CBBMQ…BUCV, 0.10 %):

| Direction | ≤ 1 % impact | ≤ 2 % | ≤ 5 % |
|---|---|---|---|
| XLM→USDC (liquidating longs; thinner side) | $15.8k | $38.8k | $110k |
| USDC→XLM (liquidating shorts) | $24.8k | $47.6k | $146k |

Base offset vs oracle is ~15–25 bps even at $1. Concentrated-pool depth can vanish if LPs move; re-measure before each
cap raise. No usable multi-hop route.

**Network limits** (live Mainnet config, ledger 65649551): 400 footprint entries, 200 disk reads, 200 writes,
400 M instructions per tx. Our 100-key gate is internal only.

## 3. Parameters (first release)

| Parameter | Value | Why |
|---|---|---|
| Max leverage | **3×** long and short XLM (open LTV ≤ 0.667) | owner decision |
| Maintenance / liquidation LTV | **0.85** | 3× long liquidated after XLM −21.6 % from entry, short after +27.5 %: a 15–20 % move does **not** liquidate a fresh 3× position |
| Liquidation bonus | **5 %**: 4 % liquidator, 1 % backstop | |
| Solvency bound (enforced in `set_risk`) | liq LTV × (1 + bonus) × (1 + slippage 1 %) < 1 → 0.85 × 1.05 × 1.01 = **0.901** | |
| Gap tolerated before bad debt | **~9.9 %** past the liquidation point (long; ~10.4 % short) | ≈ 1 h p99.9; ≥ 2× the 15-min p99.9. Keeper reaction ≈ oracle (Reflector 5 min) + 1 cycle |
| Open fee / close fee | **0.10 % / 0.10 %** of notional → backstop | |
| Funding | cash-market borrow APR passed through (USDC 8 %, XLM 6 %); optional +2 % APR spread to backstop (off at launch) | |
| Pool headroom check in `open` | pooled account HF ≥ **1.25** at spot **and** ≥ **1.10** at XLM −20 % and +20 %, **and** free liquidity ≥ `close_reserve_usd` | health is monotonic in the XLM price, so checking the two ends covers ±20 % |
| `close_reserve_usd` | ≥ CF_held × max position debt × 1.02 | so the largest position can always be closed by swap |
| Backstop exit | request → **7-day cooldown** → withdraw at NAV **at execution** (backers keep bearing losses during cooldown); refused if it breaks the headroom check | |
| Circuit breakers | opens paused automatically when: oracle stale, `Impaired`, backstop below its required size (§5), or guardian pause. Close, `close_with_repay`, liquidate, claim never pause | |

Tail case (10 Oct 2025, −54 % in 15 min): a 3× long ends with debt 2C against collateral worth ~1.37C → bad debt
≈ **0.63 × that trader's equity**, charged to the backstop. Sized for in §5.

## 4. Caps

| Cap | Pilot | Tier 1 ("useful") | Tier 2 |
|---|---|---|---|
| Per position (trader equity C) | $50 | $500 (notional $1.5k) | $2,000 (notional $6k) |
| Open interest per side (notional) | $300 | $6k | $30k |
| Total trader collateral | $200 | $4k | $20k |
| Largest single liquidation vs depth | ≪ 1 % impact | ≪ 1 % | ~0.4 % ($6k); full long-book cascade $30k ≈ 1.7 % |

## 5. Backstop seed (treasury, USDC pTokens)

Requirement per §3 (from the code plan, at 3×):
- steady state at stress (XLM −20 %, pooled HF 1.10): **B ≥ 1.2 × long trader collateral**
  (shorts need ~0.2×; longs and shorts partly hedge);
- transient, because the live controller health-checks the pooled account mid-open (at the borrow, before the swap
  proceeds are deposited) and mid-close: **+ ~2 × the largest single position**;
- tail: covers the −54 % event's bad debt (0.63 × long equity) several times over.

| | Pilot | Tier 1 | Tier 2 |
|---|---|---|---|
| Trader collateral (long/short) | $100 / $100 | $2k / $2k | $10k / $10k |
| **Backstop seed needed** | **$500** | **$5k** | **$20k** |
| Worst-tail loss absorbed (−54 % long) | ~$63 | ~$1.3k (≈ 25 % of seed) | ~$6.3k (≈ 32 %) |

**Cash-market limits are the real constraint above the pilot** (no margin change fixes these):
- Longs borrow USDC = 2 × equity. Live USDC **borrow cap 900** and the **70 % cash reserve** (only 30 % of supply
  borrowable). Supply today ≈ $2.9k → ≈ $880 borrowable for **all** borrowers.
  - The backstop seed is itself USDC supply: a $5k seed lifts borrowable USDC by ~$1.5k.
  - Tier 1 needs ~$4k of USDC borrow for longs: raise the USDC borrow cap to ≥ $5k **and** either more USDC
    supply (~$13k) or a lower cash reserve (e.g. 50 %).
- Shorts borrow XLM = 2 × equity. Live **XLM borrow cap 4,500 XLM** (~$560). Tier 1 needs ~32k XLM: raise the cap
  and check XLM supply first.
- The pilot fits inside today's caps.

## 6. Keeper

- Margin pass inside the existing `bots/cash-liquidator` process (single signer, inventory, simulate-then-send, journal).
- Every cycle (≤ 60 s): list open positions (counter + events), simulate `liquidate` for every position past liq LTV,
  send the worst first; then `settle_backstop_debt` for any bad debt.
- The cash liquidator must exclude the margin contract's pooled account from its own liquidation (alert only).
- Liquidator inventory: USDC ≥ largest long debt, XLM ≥ largest short debt (Tier 1: ~$1k each).

## 7. Open decisions for the owner

1. **Footprint gate:** raise the internal 100-key / 100 M gate to **130 keys / 200 M** (network allows 400 / 400 M).
   Projected flows: 108–112 keys, ~130 M instructions.
2. **Almanax** is inactive: scan the margin release before the pilot, or waive it as for the CL launch?
3. **Tier 1 cash-market changes** (USDC borrow cap, cash reserve, XLM borrow cap): separate admin transactions on the
   live controller/receipts; propose after the pilot.
4. Funding spread to the backstop (+2 % APR) on or off.
5. Backstop cooldown 7 days (or 72 h).

## 8. Estimate (parallel subagents; start Thu 26 Nov)

| WP | Work | Days | Owner |
|---|---|---|---|
| 0 | Freeze spec (keys, structs, errors, signatures, params) | 0.5 | main, Thu am |
| 1 | Backstop module (shares, NAV, cooldown, dead shares) | 1.5 | subagent A |
| 2 | Positions v2 (3× risk, USDC fees, caps/OI, liquidation split, claims, bad-debt move, counter view) | 2.0 | main (critical) |
| 3 | Headroom + stress via `controller.portfolio`, `reconcile` / `Impaired` | 1.0 | subagent B |
| 4a | Fork with the real Aquarius pool + verify swap auth | 1.0 | subagent C |
| 4 | Unit + property tests | 2.5 | subagent D + main |
| 5 | Fork scenarios + resource table | 1.5 | main (critical) |
| 6 | Footprint buffer, if over the gate | 0–1.5 | main |
| 7 | Keeper margin pass + tests + dry run | 2.0 | subagent E |
| 8 | Ceremony + read-back scripts | 1.0 | subagent F |
| 9 | Scan + fixes (if Almanax available) | 1.5 | main |
| 10 | Sign-off, Ledger ceremony, $5 canary, backstop seed, keeper live | 1.0 | owner + main |

- **Effort:** ~15–17 person-days; **elapsed ~7 working days**.
- **Code complete and fork-measured:** Wed 2 Dec.
- **Earliest supervised pilot:** **Mon 7 Dec** with a scan; **Thu 3 – Fri 4 Dec** if the scan is waived.
  Slips to Wed 9 – Thu 10 Dec if the footprint needs a redesign or a review finds High/Medium issues.
