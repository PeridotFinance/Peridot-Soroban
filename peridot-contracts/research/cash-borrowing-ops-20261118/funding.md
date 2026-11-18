# Liquidator and pause-key funding (PROPOSED; ~$1.2k approved in principle)

Prices are from the live Reflector external feed at Mainnet ledger 65541367 (2026-11-18 09:08 UTC, `evidence/oracle-bounds-20261118.json`):
XLM $0.14165, USDC $1.00094, EURC $1.09733. All amounts are whole units. Raw amounts use 7 decimals.

## 1. Caps against inventory

Rule: `borrow cap × close factor × 1.10 ≤ liquidator inventory`, per debt asset (`ceremony.cjs caps`, enforced by `prepare`).

- The close factor is 50% and the incentive is 1.08×. Both are live values and stay unchanged.
- The 1.10 buffer covers interest accrued above the principal cap: about one year at 8% APR, plus rounding.

| Debt asset | Old cap | **New cap** | Cap × 50% | × 1.10 | **Inventory** | Headroom |
|---|---|---|---|---|---|---|
| USDC | 1,000 | **900** | 450 | 495 | **500** | 5 USDC |
| XLM | 5,000 | **4,500** | 2,250 | 2,475 | **2,500** | 25 XLM |
| EURC | 500 | **450** | 225 | 247.5 | **250** | 2.5 EURC |

With the new caps, the most the protocol can lend is about $901 + $637 + $494 = **$2.03k** (it was $2.25k).

**Assumptions** (to be signed off, because they change the accepted parameter table):

1. **Worst single position.** The per-market borrow cap bounds the market's total principal. The worst single position is therefore one borrower holding the whole cap. Repaying it at the close factor needs cap × 50%.
2. **Correlated move.**
   - Every borrower in the market crosses into shortfall in the same move.
   - The total is still bounded by the cap, so the first liquidation round needs the same cap × 50%.
   - In an XLM crash, the USDC and EURC debts (XLM-collateralised) liquidate together: 495 USDC and 247.5 EURC, from separate inventories.
   - XLM debt becomes liquidatable on an XLM *rise*, so it does not coincide with the crash. The inventory covers all three at once anyway.
3. **One round per position.** One close-factor round restores health whenever collateral/debt stays above the 1.08 incentive.
   - A further fall needs a second round, which this inventory does not cover.
   - Response: the operator tops up within 24 h. The keeper alerts on low inventory.
4. **No recycling within an incident.** Seized collateral is a different asset, which the controller requires. The bot withdraws it but does not swap it back, so repay inventory is not replenished during an incident.
5. **Units, not USD.** Inventory is held in the debt asset, so a price move does not change how much debt it can repay.

## 2. What to fund, and when

| When | Account | Asset | Amount | ≈ USD | Why |
|---|---|---|---|---|---|
| **T0**: right after `bootstrap-liquidator.sh` prints the key, before the dry-run soak | liquidator | XLM | **25** | 3.54 | Covers a 2 XLM account reserve (1 base + 2 trustlines × 0.5) and the trustline transaction, and keeps the balance above the 20 XLM fee-floor alert. A dry run needs an existing account (`prepare` reads it). |
| T0 + trustlines | liquidator | — | — | — | `src/trustlines.mjs` (dry run, then `CONFIRM_TRUSTLINES=yes`): USDC and EURC trustlines. |
| **P0**: right after `bootstrap-pause-watcher.sh` prints the key | pause key | XLM | **10** | 1.42 | Covers a 1 XLM reserve and fees for at least 3 pauses per incident (cap 0.5 XLM each, about 0.01 actual). |
| **T1**: after the 48–72 h dry-run soak passes and CEREMONY §A–D read back, **before** flipping `DRY_RUN=false` | liquidator | USDC | **500** | 500.47 | Repay inventory |
| T1 | liquidator | EURC | **250** | 274.33 | Repay inventory |
| T1 | liquidator | XLM | **2,600** | 368.29 | 2,500 inventory plus 100 more for fees. The total fee balance of about 122 XLM covers the price refresher for about 2–3 months at the §3 estimate. |
| T1 → live | — | — | — | — | Flip `DRY_RUN=false`. The bot approves the USDC, XLM and EURC repay allowances on start. Then run `gate gate.liquidator-ready` (G2), which needs balance ≥ inventory (XLM: 2,500 + 20 + 2) and allowance ≥ inventory. |
| **Total** | | USDC 500 · EURC 250 · XLM 2,635 | | **≈ $1,148** | Within about $1.2k |

The canary account for CEREMONY step C (about 110 XLM, its own key) is separate from this budget.

**Why T1 is late.** G2 needs the allowances, and the bot only approves in live mode. Live mode should start only after the soak. Funding the hot key just before that keeps the holding window short. The ceremony is blocked at G2 until T1 is done.

## 3. Running costs

- **`cache_price` refresher.** It runs on every Reflector round (300 s): 3 tokens × 288 = 864 transactions/day.
  - **Measured on Mainnet today** (`evidence/pause-and-refresh-sim-20261118.json`, read-only): with the oracle still unset, `cache_price` takes the no-write `None` path at 1.96M CPU, with a padded max fee of 0.00214 XLM including the 0.0001 bid. That is about 0.00087 XLM of real resource fee.
  - **Once the oracle is set,** each refresh also calls Reflector (about 2.2M CPU) and writes `PriceCache`. Estimate: **0.0015–0.002 XLM per refresh, about 1.3–1.7 XLM/day, 40–52 XLM/month.** Testnet measured 0.0007 XLM, but Mainnet resource prices are higher. Read the first live refresh's `feeCharged` in the log and adjust.
  - **Hard limits:** 0.01 XLM max fee per refresh (alert if the simulation exceeds it), a 30 s validity and 35 s wait, and a persisted **3 XLM/UTC-day** budget charged at the actual `feeCharged`, or at the max fee when the result is unknown. It idles until the controller oracle is set (ceremony step 24).
- **Pause watcher.** The Mainnet simulation of `pause_borrow_g` costs a max fee of 0.0125 XLM including the 0.01 bid (cap 0.5 XLM). Three markets per incident come to about 0.04 XLM.
- **Liquidations.** 0.01 XLM bid, 2 XLM max fee each, plus a withdrawal of the seized collateral.
- **Alerts.** XLM below the 20 XLM fee floor, or net inventory below the §1 minimums (live mode). Top up within 24 h.
- **Droplets.** Liquidator Basic 1 vCPU / 1 GiB plus a 10 GiB volume (about $7/month). Pause watcher: a second Basic 512 MiB (about $4/month) is recommended. These are approximate list prices; check current DigitalOcean pricing.
