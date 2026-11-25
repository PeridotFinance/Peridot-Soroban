# Pooled margin v2 — spec (rev 2, 24 Nov 2026: 5x, per-position maintenance; code starts Wed 25 Nov)

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

Per-tx limits (stellar network settings, protocol 30, CLI 26 read; CLI 30 re-check pending): 400 M instructions,
41.9 MB memory, 200 disk-read entries / 200 kB, 200 write entries / 132 kB, 400 footprint entries, 132 kB tx size.
**Owner rule:** internal limit 100 entries / 100 M except the margin release (**130 keys / 200 M approved**);
liquidation flows ≤ 85 % of every live limit (340 footprint, 170 disk reads, 170 writes, 340 M instructions).

## 3. Parameters (owner decisions 24 Nov, rev 2)

| Parameter | Value | Why |
|---|---|---|
| Max leverage | **5×** long and short XLM; **frontend default 3×** | owner |
| Maintenance margin | **half the starting margin, per position** (stored at open): liq LTV = 1 − 1/(2L) | Robinhood/Avalanche style |
| → liquidation move from entry (long ≈ 1/(2L−1)) | 2×: −33 % · 3×: −20 % · **5×: −11.1 %** (short: +50 % / +25 % / +12.5 %) | |
| Liquidation bonus | **4 %**: 3 % liquidator, 1 % backstop | owner |
| Solvency bound (`set_risk`) | liq LTV × 1.04 × (1 + 1 % slippage) < 1 → 5×: 0.90 × 1.0504 = **0.945** | |
| Gap tolerated past the liquidation point before bad debt | **5×: ~5.5 %** · 3×: ~12.5 % · 2×: ~21 % | 5×: > 5-min p99.9 (2.2 %) and > 15-min p99.9 (3.7–4.1 %), < 1-h p99.9 (8–10 %) → keeper must act within one oracle round |
| Open / close fee | 0.10 % / 0.10 % of notional → backstop | |
| Funding spread | **on: +2 % APR** over the cash-market borrow APR → backstop | owner |
| Backstop exit | request → **72 h cooldown** → withdraw at NAV at execution; refused if it breaks the pool check | owner |
| Circuit breakers | opens paused when: oracle stale, `Impaired`, backstop below required size, guardian pause. Close / `close_with_repay` / liquidate / claim never pause | |

**XLM 5-min moves, last 12 months** (Binance 5-min candles, 24 Nov 2025 – 24 Nov 2026, 105,121 candles):
- open→low > 6 %: **5 candles (0.0048 %)**; open→high > 6 %: **1 (0.0010 %)**; any > 6 % range: 7 (0.0067 %).
- i.e. about **6 events per year** where a 5× position near its liquidation point could produce bad debt within one 5-min round.
- Largest: 15 Jun 2026 −25.5 %, 10 Dec 2025 17.1 %, 22 Aug 2026 8.7 %, 19 Jan 2026 7.0 %, 20 Dec 2025 6.6 %.
- 10 Oct 2025 is just outside this window (it is in the 3-year set).

**10 Oct 2025 crash at 5×** (5-min path: 20:45 open 0.3623 → 21:10 low 0.305 → 21:15 low 0.200 → 21:20 low 0.160 → 21:45 0.285).
A 5× long opened at 0.3623 has liquidation price 0.322 (−11.1 %), crossed inside the 21:10 candle. Backstop loss per $1 of
long trader equity (debt 4, collateral 5 × price ratio, 1 % slippage, 4 % bonus owed):

| Liquidation executes at | XLM | Backstop loss / $1 equity (5×) | same event at 3× |
|---|---|---|---|
| 21:15 open (next round) | 0.316 (−12.7 %) | **$0** (owner keeps ~$0.15) | $0 |
| 21:15 close (oracle one round late) | 0.201 (−44.6 %) | **$1.42** | $0.43 |
| 21:20 low (worst print) | 0.160 (−55.8 %) | **$1.97** | $0.77 |

Pilot long book $100 at 5× → worst case ≈ **$197** backstop loss; Tier 1 ($2k) → ≈ $3.9k. Shorts were not hurt
(XLM fell). Binance wicks overstate what the Reflector median and Aquarius would print, but liquidity also vanishes.

## 4. Pool check in `open` (re-sized for 5×)

Live controller health-checks the pooled account mid-flow (at the borrow before the proceeds are deposited, and at the
withdraw before repay in a swap-close). Per $1 of trader equity C at 5× (1 % slippage; the debt/collateral row and the close reserve use the 1.5 % default):

| | 5× long (hold XLM, owe USDC) | 5× short (hold USDC, owe XLM) |
|---|---|---|
| debt / collateral after open | 4C / 4.94C | 4C / 4.94C |
| transient need at the borrow (weighted USD) | 4 − 0.55 = **3.45C** → 4.6C of backstop USDC | 4 − 0.75 = **3.25C** → 4.33C |
| transient at swap-close withdraw | 0.55 × 4.04 = 2.22C → 2.96C | 0.75 × 4.04 = 3.03C → 4.04C |
| steady, pooled HF 1.25 at spot | B ≥ 3.03C | B ≥ 1.71C |
| stress, pooled HF 1.10 at XLM −20 % / +20 % | B ≥ 2.96C | B ≥ 2.08C |

Checks in `open` (all must pass after the trade):
1. pooled HF ≥ **1.25** at spot;
2. pooled HF ≥ **1.10** at XLM −20 % and at +20 % (monotonic, so both ends cover the range);
3. free liquidity ≥ `close_reserve_usd` = 1.015 × max(0.55 × largest long debt, 0.75 × largest short debt), so the largest
   position can always be swap-closed and liquidated;
4. position leverage ≤ 5×; per-position, per-side OI and total-collateral caps (§5).

**Required backstop:** `B ≥ 3.0 × long equity + 2.1 × short equity + 4.6 × largest single position` (no hedge credit).

## 5. Caps and treasury seed (5×)

| | Pilot (team, supervised) | Tier 1 | Tier 2 |
|---|---|---|---|
| Per position (equity) | $25 (5× = $125 notional) | $500 ($2.5k) | $2,000 ($10k) |
| Long / short equity | $100 / $100 | $2k / $2k | $10k / $10k |
| OI per side (notional) | $500 | $10k | $50k |
| **Backstop seed (USDC pTokens)** | **$750** (formula $625) | **$12.5k** | **$60k** |
| Worst 10-Oct-2025 loss (5× long book, worst print) | ~$197 (26 % of seed) | ~$3.9k (32 %) | ~$19.7k (33 %) |
| Largest liquidation vs Aquarius depth | ≪ 1 % | ~0.2 % | ~0.6 %; long-book cascade $50k ≈ 2.3 % |

Pilot fits today's cash caps: long USDC debt 4 × $100 = $400 (≈ $880 borrowable, plus the seed adds supply);
short XLM debt ≈ $400 ≈ 3.2k XLM (cap 4,500, shared with all borrowers).

## 6. Keeper

- Margin pass in the existing `bots/cash-liquidator` process (single signer, inventory, simulate-then-send, journal).
- Every cycle (≤ 30 s at 5×): list positions (counter + events), simulate `liquidate` for every position past its own
  liq LTV, worst first; then `settle_backstop_debt`. Refresh/act on every new Reflector round.
- Cash liquidator excludes the margin pooled account from its own liquidation (alert only).
- **Reaction target: one Reflector round.** Alert when any position has been liquidatable for > 10 min (owner, 24 Nov).
- Inventory: USDC ≥ largest long debt, XLM ≥ largest short debt.
- CAP-88: repeated ledger timestamps must not stall or double-count (audit 24 Nov).

## 7. Decisions recorded / still open

Recorded 24 Nov: 5× max (frontend default 3×); maintenance = half starting margin; bonus 4 % (3/1); footprint
130 keys / 200 M for margin, liquidation ≤ 85 % of live limits; funding spread +2 % APR on; cooldown 72 h; Almanax on
the margin release in parallel, gating **public opening only**, not the supervised team pilot; pilot target **Thu 3 Dec**.

Recorded 24 Nov (later): **pilot backstop seed $750 approved**, from the treasury, in USDC pTokens; Tier 1 / Tier 2 stay
as drafted until the pilot has run; **keeper reaction target = one Reflector round**, with an **alert if a liquidatable
position waits > 10 min**; margin release builds on the **workspace SDK 25.3.x** (SDK 30 later, whole workspace);
Almanax renewal in progress — start the margin scan the day it is active.

Open: Tier 1 values (§9) after the pilot.

## 8. Estimate (parallel subagents; contract code starts Wed 25 Nov during the CL go-live)

Work packages as in `CODE-PLAN.md` §8 (WP0 spec freeze → WP2 positions critical path; WP1 backstop, WP3 pool check,
WP4a real-pool fork, WP4 tests, WP7 keeper, WP8 ceremony in parallel). Starting one day earlier than the plan:
- code complete and fork-measured: **Tue 1 Dec**;
- Almanax (if the subscription is active) Wed 2 – Thu 3 Dec, in parallel; does not gate the team pilot;
- **supervised team pilot: Thu 3 Dec** (pilot caps above, $750 seed, keeper live, 30-s cycle);
- public opening: after a clean scan and the pilot review.

## 9. Tier 1 cash-market changes — proposal list (after the pilot; not before)

Separate, announced ahead and executed only after the pilot review. Each item: simulate on live Mainnet first, Ledger
sign, read back. If a setter has no on-chain timelock, apply an operational **72 h announced delay** before executing.

| # | Change | From → to (Tier 1 at 5×) | Why | Check first |
|---|---|---|---|---|
| T1-1 | USDC market borrow cap | 900 → **10,000** USDC | long debt 4 × $2k = $8k + existing borrowers | live USDC supply and utilisation |
| T1-2 | USDC cash reserve | 70 % → **50 %** | borrowable share 30 % → 50 % | rate model / lender exit liquidity at 50 % |
| T1-3 | XLM market borrow cap | 4,500 → **70,000** XLM | short debt ≈ $8k ≈ 64k XLM at $0.125 | live XLM supply (must be ≥ ~140k XLM at a 50 % reserve) |
| T1-4 | XLM cash reserve | 70 % → **50 %** (only if supply allows) | as T1-2 | as T1-3 |
| T1-5 | Margin caps | pilot → Tier 1 (§5) | | backstop seed $12.5k deposited first |

Order: seed the backstop (adds USDC supply) → T1-1/T1-2 → T1-3/T1-4 → T1-5. Values are provisional until the live
supply reads are done; nothing here is executed before the pilot.
