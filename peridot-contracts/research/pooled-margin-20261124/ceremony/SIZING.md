# Pooled margin v2 — pilot parameters, backstop and cash-market sizing (WP8, 25 Nov 2026)

Inputs: `SPEC.md` rev 2 §2–§5, `INTERFACE.md` (WP6), `WP5-FORK-RESULTS.md`, the slippage study of 25 Nov
(`MARGIN-SLIPPAGE-20261125.md`, session scratchpad) and live Mainnet reads (`node margin-ceremony.cjs headroom`, ledger
65664577: XLM $0.121708, USDC $1.001008).

**Verdict.**
- The **$750 seed covers the pilot book.** The contract's own formula needs **$625** at full pilot caps, so there is
  $125 (17 %) of slack.
- **Both cash markets fit the pilot**, shared with all other borrowers:
  - USDC: **482.70 USDC** of borrowable room is left after the full long book.
  - XLM: **1,213.45 XLM** is left after the full short book.
- Nothing in the cash markets needs to change for the pilot.

## 1. Parameters

| Parameter | Value (on chain) | Justification |
|---|---|---|
| Max leverage | 5× both sides (`set_risk` 5,000,000); frontend default 3× | Owner 24 Nov. At 5× the liquidation move from entry is −11.1 % (long) / +12.5 % (short). The gap tolerated before bad debt is ~5.5 %, above the 15-min p99.9 XLM move (3.7–4.1 %), so the keeper must act within one Reflector round |
| Maintenance | half the starting margin, per position: liq LTV = (1 + LTV_open)/2 | Owner. Fork: 5× → 0.900, 3× → 0.833 (canary 0.833) |
| Solvency bound | liq LTV × (1 + bonuses) × (1 + slippage) < 1 → 0.90 × 1.04 × 1.015 = 0.950 | Enforced by `set_risk` / `set_fees` / `set_max_slippage` |
| Open / close fee | 0.10 % / 0.10 % of notional → backstop | Owner. About 1.5 USDC per $1.5k of notional; the canary paid 0.0149 pT per open |
| Funding | +2 % APR on debt → backstop | Owner. On top of the cash borrow rate (USDC 8 %, XLM 6 %, static) |
| Liquidation bonus | 3 % liquidator + 1 % backstop | Owner |
| Max slippage | 1.5 % vs Reflector | Slippage study: fee 10 bps per leg, impact < 0.6 bps at pilot size. 1.5 % leaves ~139 bps for oracle lag. In 20 h of real data it rejected 0 minutes; the 7-day proxy rejects ~9 min/week, only in fast moves. `close_with_repay` (no swap) is always available. The 1 % floor was already exceeded at the fork |
| Backstop exit | 72 h cooldown, 48 h window, pool-checked | Owner. Backers cannot run ahead of a known loss |
| Pool check | HF ≥ 1.25 spot, ≥ 1.10 at XLM ±20 %, free liquidity ≥ close reserve 1.015 × max(0.55 × largest long debt, 0.75 × largest short debt) | SPEC §4. The live controller health-checks the pooled account mid-flow |
| Caps | $25 per position (5× = $125 notional), $100 side equity, $500 side OI, $1 minimum | SPEC §5 pilot |
| Seed | 750 USDC pTokens from the admin's existing USDC pTokens | Approved 24 Nov |

## 2. Required backstop vs the seed

Formula (SPEC §4, `headroom::required_backstop_usd`, evaluated at every open on the **current** book):

    B >= 3.0 x long equity + 2.1 x short equity + 4.6 x largest allowed position

At full pilot caps: 3.0 × $100 + 2.1 × $100 + 4.6 × $25 = **$300 + $210 + $115 = $625**. The tool asserts this
(`requiredBackstop` test). The largest-position term uses the per-position *cap* ($25), so it is $115 even with an empty
book. The fork read `required_backstop` = $115.00 before the first open.

| | Value |
|---|---|
| Seed | 750.0000000 USDC pT × exchange rate 1.000000 × $1.0010 = **$750.76** |
| Required, empty book | $115 |
| Required, full pilot book | **$625** |
| Slack at full book | **$125.76** (16.8 % of the seed) |
| Worst 10-Oct-2025 path, 5× long book $100, worst print (SPEC §3) | ≈ $197 backstop loss |
| Same, liquidated one round late (21:15 close) | ≈ $142 |
| Same at the frontend default 3× | $43–77 |

**What happens after a loss.** A loss above the $125.76 slack, with the book full, puts NAV below $625. Every new open
then fails with `BackstopTooSmall` (#23) until the book shrinks or the backstop is recapitalised (K4). Closes,
close_with_repay, liquidations and claims are unaffected. This failure mode is safe. A loss larger than the whole
backstop sets `Impaired` and pauses opens; recovery uses the K-steps in CEREMONY.md §5. At the worst observed path the
seed would keep 74 % of its value. There are about 6 events per year in which a 5× position near its liquidation point
could produce bad debt within one 5-min round. Each one costs the backstop at most the position's gap beyond ~5.5 %.

Fees accrue to the backstop. Each open and close adds 0.10 % of notional, so a full pilot book opened and closed once adds
about $2 ($500 × 2 sides × 0.2 %), plus funding at 2 % APR on up to $800 of debt (≈ $0.04/day). They do not
meaningfully grow the backstop at pilot size.

## 3. Cash-market headroom (live, 25 Nov, ledger 65664577)

Headroom = min(borrow cap − total borrowed, borrowable cash) − pilot maximum debt. Borrowable cash is the share above the
70 % cash reserve. The pilot maximum debt per side is min(side equity × (L − 1), side OI − side equity / L) = **$400**.

| | USDC market (long debt) | XLM market (short debt) |
|---|---|---|
| Supply | 2,941.0006 USDC | 25,745.65 XLM |
| Cash reserve | 70 % (borrowable 30 %) | 70 % |
| Borrowable cash | 882.3002 USDC | 7,723.69 XLM |
| Borrow cap | 900 USDC | 4,500 XLM |
| Borrowed today (all borrowers) | 0.0000006 USDC | 0.0000007 XLM |
| Effective limit | **882.30 USDC** (the reserve binds, not the cap) | **4,500 XLM** (the cap binds) |
| Pilot max debt ($400) | 399.60 USDC | 3,286.55 XLM at $0.121708 |
| **Headroom left for everyone else** | **482.70 USDC** | **1,213.45 XLM** |

Notes:
- **The seed does not add supply.** SPEC §5 says "the seed adds supply". That is not so, because the seed moves the
  admin's existing pTokens into the margin contract. Supply and borrowable cash are unchanged. A seed minted from new
  USDC (FB1) would add supply.
- **The XLM cap is price-dependent.** Debt is booked in XLM at open. New shorts of $400 hit the 4,500 XLM cap once XLM
  is below **$0.0889 (−27 %)**. After that, new short opens fail at the borrow; existing positions are unaffected.
- **The admin is nearly the only lender**: 2,940 of 2,941 USDC and 25,658 of 25,746 XLM. An admin withdrawal shrinks
  borrowable cash directly. To keep the full pilot book borrowable, USDC supply must stay ≥ 1,332 USDC (400 / 0.30) and
  XLM supply ≥ 10,955 XLM (3,287 / 0.30). The 750 seed pTokens count in the USDC supply and are locked behind the 72 h
  exit.
- When cap or cash is short, the cash market refuses the margin `borrow`, so the open fails. Lenders are never forced and
  no hook is involved. Re-read before every cap raise:

```sh
cd /Users/joshua/Peridot/Peridot-Soroban/.wt-margin-assess/peridot-contracts/research/pooled-margin-20261124/ceremony && node margin-ceremony.cjs headroom
```

## 4. Expected costs

**Ceremony (admin XLM).** M1 upload: 93.20 XLM simulated, max 111.85 XLM; the unused part of the padding is refunded.
Each setter costs ≈ 0.001 XLM, and the seed ≈ 0.07 XLM including rent. Each canary open or close costs ≈ 0.01 XLM plus
rent for the new position entry. Estimated total: **≈ 94–95 XLM**; the cap is ~113 XLM. The admin holds 210.59 XLM.
The long canary locks ~41 XLM until C2.

**Per trade (slippage study, 25 Nov; swaps are about the debt size, fees on gross notional).** Round-trip cost as a
share of equity:

| Equity / leverage | Hold 1 day | Hold 7 days | Per-leg spread vs Reflector, p50 / p99 |
|---|---|---|---|
| $25, 3× (frontend default) | 1.05–1.06 % | 1.32–1.39 % | ±9 / ±63 bps of the debt |
| $25, 5× | 1.92–1.95 % | 2.45–2.60 % | same |
| $500, 3× (Tier 1) | 1.22–1.23 % | 1.49–1.56 % | same |
| $500, 5× (Tier 1) | 2.59–2.61 % | 3.11–3.27 % | same |

- Components: 2 × 10 bps pool fee plus impact (< 0.6 bps at pilot size), 0.10 % + 0.10 % fees, +2 % APR funding, and the
  cash borrow rate (USDC 8 %, XLM 6 %).
- The Reflector-lag offset cancels in expectation over a round trip. Per leg it is a ±1.3 % (3×) / ±2.5 % (5×) of equity
  risk at p99.
- A $5 canary at 3× costs about 1 % of $5, i.e. ~$0.05 per side.

## 5. Gates

### Before the supervised team pilot (Almanax does NOT gate this)

1. CEREMONY.md approved. Guardian chosen and written into `margin-mainnet.json`.
2. Code pinned to the deployed hash (`b1cb1cb2…766f`, or a newer final build: then re-pin, re-freeze, re-run CEREMONY.md
   §1). Fork suites pass: WP5 5/5 plus `wp8_ceremony_sequence_on_fork`.
3. WP7 keeper:
   - its margin pass runs **live on the Droplet** after a clean dry stage;
   - the alerts are tested: liquidatable > 10 min, Impaired, keeper down;
   - the cash liquidator excludes the margin pooled account (per-borrower; `neverTouch` cannot do this);
   - the operator has written the keeper gate file.
4. Keeper inventory: USDC ≥ the largest long debt ($100), XLM ≥ the largest short debt + 20 XLM. The 25 Nov balances
   already meet this: 502 USDC, 2,608 XLM.
5. The 30-s cycle, and a reaction within one Reflector round, are confirmed in the dry stage.
6. The margin contract instance and open positions are added to the TTL keeper inventory (`bump_ttl`).
7. G0 preflight, M1–M16, then the canary gate, all clean.
8. Known residuals accepted for the pilot:
   - pooled HF < 1 blocks some exits until settles run (WP6 §6);
   - no claim-parking;
   - the XLM restore comes from the treasury.
9. The contract is permissionless once unpaused. Anyone with the id can open within the pilot caps, and the pilot caps
   are what bound the risk. Keep the id unpublished until the canary is clean.

### Before public opening (in addition)

1. A **clean Almanax scan of the exact deployed hash**. Any code change goes through the 24 h upgrade with its own
   ceremony list.
2. A pilot review: every liquidation was reacted to within one round, every alert fired correctly, Ledger == pToken
   balance throughout, and no unexplained events.
3. Re-measure the Aquarius depth and re-read the cash-market headroom (§3).
4. Tier 1 (SPEC §9) is separate. Seed $12.5k first, then the cash-market cap and reserve changes, each simulated live with
   a 72 h announced delay, then the margin caps.
5. The frontend shows the liquidation price, the default 3× and the close-with-repay fallback.
6. Owner decisions are closed: claim-parking on a failed exit, and backstop-funded XLM restoration.
