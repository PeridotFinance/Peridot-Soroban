# Exact-issuer price history, primary-oracle coverage and asset-risk evidence

Read-only bounded study, 11 November 2026. Owned contribution to the pool-selection
checkpoint; no contract changes, signing, deployment, subscriptions or new oracle
integration. Raw public responses, request timestamps and hashes are in `sources.json`.
Failed DNS, HTTP402 and ECB timeout attempts are preserved separately from successes.

## Identity and direct exposure

Current Aquarius API identities were independently resolved by the pool-data task:
CAPIO… is **PYUSD/USDC concentrated**, not stable-swap and not sUSD/PYUSD. Actual
sUSD/PYUSD CL is **CC7IGNJ4CDAKZPOZJCTTWGAVKTQXKYQGUSVJ5CDUPES3QULCQW6UNAH5**.
Pool3 CBV6… is USDC/sUSD; pool4 CDTSE… is USDC/EURC; added pool5 CCJHU… is
USDT0/USDC. Use full addresses in the parent identity table, not ticker-only matching.

- sUSD issuer `GCHW7CWI7GMIYQYFXMFJNJX5645XGWIINIAEQK3SABQO6CAYL5T7JYIH`,
  SAC `CDFZUVS5YNLXU7VENKOUDEOHCJGKQNVUBWD7KMN6E7ZROKPYPFLRUJFG`.
  [Issuer](https://synt.tech/) and its [stellar.toml](https://synt.tech/.well-known/stellar.toml)
  identify synthetic USD managed by centralized algorithmic market making. The issuer
  expressly says its tokens are **not backed by the tracked assets and not redeemable**.
  This is not Synthetix sUSD; importing Synthetix depeg charts would be an identity error.
  No redemption fee/depth can establish parity because there is no promised redemption.
  The page advertises7.35% weekly-distributed annual yield funded by EVM strategies and
  market making; this is a separate issuer programme, not Aquarius swap fees or AQUA.
  It is not credited as verified contract-enforceable vault income. A hold-sUSD
  benchmark must include any actually received issuer distribution consistently.
- EURC issuer `GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2`,
  SAC `CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV`.
  [Circle](https://www.circle.com/eurc) targets one euro, **not one US dollar**.
  EURC can be deposited directly as one leg of pool4. A balanced pool4 position owns
  EURC and USDC, creating EUR/USD exposure: relative to holding EURC it sells roughly
  half the initial euro exposure; relative to holding USDC it buys euro exposure.
  Pool3 has no EURC leg. Entry from EURC requires a separate measured conversion into
  USDC/sUSD and gives dollar/synthetic-dollar exposure, not retained euro principal.
  This route is not implemented or execution/capacity-qualified by the study.
- USDT0 issuer `GATISXX6BZ6NC7IKQBY37CJD4SOZL3CYZJWXEDG6JVIY4WBS6KXJHN6Q`,
  SAC `CBSJZEIO5C7KC2SF3MKSNXXJSW5G3VTNBX4ATMKUI3B2MR4JKM4R26YF`.
  [Stellar's deployment documentation](https://developers.stellar.org/launch/usdt0)
  and [USDT0's security explanation](https://usdt0.to/ecosystem/stellar) bind this exact
  asset to Ethereum USDT backing, OFT mint/burn and LayerZero/USDT0 attestation.
  Inference: local USDT0 can trade away from USDT under transport, backing, attestation,
  liquidity or availability stress; a USDT symbol feed cannot detect that local basis.
  Cross-chain redemption is not the local in-kind exit. No bridge route/minimum/fee
  or redemption capacity was qualified. There is no native USDT0 holding yield.

## Ninety-day observation window

Requested window **2026-08-13T00:00Z through 2026-11-11T00:00Z**. Exact-issuer Horizon
`trade_aggregations` return90 daily buckets for each of the four older pairs and70 for
USDT0/USDC, starting September2. These are **classic Stellar market prints**, not the
Aquarius CL pool's historical ticks or executable quotations. They are useful public
reference observations, not a complete all-venue depeg feed. Current/partial days are
excluded. Earlier89-bucket queries are preserved; `90complete` files are authoritative.

|Base/counter|Daily buckets|Daily VWAP minimum / maximum|Worst downward daily VWAP deviation from parity|Raw trade low / high|
|---|---:|---:|---:|---:|
|sUSD/PYUSD|90|0.9861209 /0.9999755|-1.38791%|0.9708739 /1.0031085|
|sUSD/USDC|90|0.9884387 /0.9956176|-1.15613%|0.9000000 /1.0028996|
|PYUSD/USDC|90|0.9988345 /1.0008428|-0.11655%|0.9333333 /1.0020000|
|EURC/USDC|90|1.0981780 /1.1667415|Not a dollar peg|1.0000000 /1.5000000|
|USDT0/USDC|70|0.9920474 /1.5998183|-0.79526% observed VWAP; no90-day coverage|0.8000000 /10000.0000000|

Primary summary uses **daily volume-weighted observations** with explicit coverage.
Raw extrema remain visible, but are not asserted as sustained depegs or prices at which
$1,000/$10,000 could execute. Daily VWAP also is not a depth quote and can be distorted
when trading volume is tiny.

USDT0's launch-day September2 bucket contained only34.7024113 tokens across72 trades.
Its10000 price print is traceable to Horizon trade`275918968361865217-5` at17:27:22UTC:
**0.0002696 USDT0 for2.6965946 USDC**, an orderbook print. Several0.8 trades were also
fractions of a token. The first200 exact-pair trades are retained. The resulting launch
VWAP1.5998183 is therefore not evidence that meaningful capital could exit at a60%
premium. No fabricated filtering turns this into a verified90-day worst depeg. The20
missing pre-launch days and market-depth uncertainty remain explicit.

### EURC depeg separated from FX

The [ECB90-day reference series](https://www.ecb.europa.eu/stats/eurofxref/eurofxref-hist-90d.xml)
provides64 business days. Match EURC/USDC daily VWAP to the same calendar day's ECB
USD-per-EUR rate, assuming USDC=$1: observed relative marks range **0.97548745–0.99825288**,
so the largest daily-mark discount is **2.45126% on August17**. This is a *different-time
same-day comparison*, not a synchronized oracle deviation: ECB's reference fixation,
DEX trades over24h, and USDC basis differ. Weekends have no matched ECB observation;
there is no forward fill disguised as a fresh oracle. Exact pair values and dates are
in `analysis.json`. These limitations prevent calling2.45126% a proven economic
90-day depeg maximum.

Ordinary FX movement is separate: ECB EUR/USD started1.1534 and ended1.1214, with
window min1.1130/max1.1699. A EURC/USDC LP can lose dollar value while EURC remains
perfectly redeemable for one euro. A guard that insists EURC=$1 would be incorrect.

## What these observations would do to a position

`analyze.py` evaluates a terminal-price **analytic sensitivity**, starting with$1,000
of50/50 marked assets at relative price1. Full-range constant-product value is
`1000*sqrt(r)`. Simple-wide is relative ticks[-2000,2000], prices
[0.8187389399,1.2213905450], using the standard concentrated-liquidity token quantities.
The wide range is centered on initial fair price; EUR pairs must normalize to FX,
not literal parity. Full-range is the analytical infinite-range approximation;
actual tick-spacing, rounding, rebalances and venue execution are not simulated.

|Downside daily mark|Full:vs holding counter|Wide:vs holding counter|Full:vs holding base|Wide:vs holding base|
|---|---:|---:|---:|---:|
|sUSD/PYUSD0.9861209|-$6.96380|-$7.19436|+$6.91530|+$6.68474|
|sUSD/USDC0.9884387|-$5.79746|-$5.95725|+$5.76384|+$5.60405|
|PYUSD/USDC0.9988345|-$0.58292|-$0.58454|+$0.58258|+$0.58096|
|USDT0/USDC0.9920474|-$3.98424|-$4.05971|+$3.96836|+$3.89289|
|EURC relative to same-day ECB0.97548745|-$12.33232|-$13.05540|+$12.18023|+$11.45715|

Multiply by10 for$10,000 only as arithmetic sensitivity, **not a capacity claim**.
At sUSD/PYUSD's worst daily mark, the full/wide shortfall relative to holding the
initial50/50 mixture is only$0.02425/$0.25481; most loss versus holding PYUSD comes
from owning sUSD at all. This distinction avoids calling all depeg exposure “LP loss.”
No fee, reward, lifecycle cost or strategy loss is double-counted here. These values
are not added to a ledger that already marks the same terminal token amounts.

For perfectly pegged EURC following the observed start/end FX change alone, full/wide
$1,000 positions end$986.03039/$985.10257: -$13.96961/-$14.89743 versus holding USDC,
but +$13.77446/+$12.84663 versus holding EURC. This is FX allocation plus LP divergence,
not a EURC redemption failure.

**Realized arbitrage loss per week remains UNMEASURED.** Daily OHLCV, current pool
liquidity and last-price oracle reads cannot reconstruct swap-time external marks,
active liquidity, tick crossings and historical owner weights. Required evidence is
ordered Aquarius swaps and fee splits, pre/post tick/liquidity state, and independent
synchronized reference prices for each leg. The00§2 delta-matched rebalancing benchmark
must be replayed, with no second LVR deduction from marked portfolio P&L. A raw swap's
in/out amounts or terminal IL are not substitutes. A paid StellarExpert candles
endpoint returned402; no API subscription, key or alternate provider integration was
started. Current Reflector history retention is24h, insufficient to reconstruct90d.

## Primary-oracle coverage and read cost

Public [Reflector configuration](https://orchestrator.reflector.network/config),
[documentation](https://reflector.network/docs) and actual unsigned RPC getter results
are retained. `oracles.json` includes ledger numbers, canonical return and resource XDR,
exact footprints and absence results. No public financial mutation was simulated.

|Feed|Base/coverage relevant here|Actual current result|
|---|---|---|
|Configured primary CAFJZQ…|External symbols/USD; USDC,EURC present|USDC/EURC valid; PYUSD,sUSD,USDT0 returnNone|
|Same-provider DEX CALI2…|Exact Stellar SACs/CircleUSDC; PYUSD,USDT0,EURC present|Those three valid; exact sUSD returnsNone|
|Same-provider FX CBKGPW…|Fiat/USD; EUR present|EUR valid|

All three return resolution300s and history_retention_period86400s. This is discovery,
not approval to change the configured feed. sUSD's absent primary price is a concrete
guard/admission blocker. PYUSD/USDT0 require explicitly reviewed exact-asset feed/base
mapping; mapping them to USD or USDT masks their own depeg. EURC needs its actual token
price as well as EUR/USD peg reference. Same-provider DEX pricing is not independent
of Stellar liquidity; upstream venue overlap/manipulation resistance remains to qualify.

|Isolated public getter|Declared instructions|Unique Soroban keys|Declared writes|
|---|---:|---:|---:|
|Primary lastprice USDC|807,059|2|0|
|Primary lastprice EURC|825,303|2|0|
|DEX lastprice PYUSD|1,289,348|2|0|
|DEX lastprice USDT0|1,291,822|2|0|
|DEX lastprice EURC|1,187,914|2|0|
|FX lastprice EUR|863,926|2|0|
|Primary /DEX /FX resolution|697,407 /1,063,889 /767,094|2 each|0|

These are **RPC resource declarations**, not independently metered execution CPU;
RPC omitted memory/SDK totals. Actual getter footprint union primaryUSDC+primaryEURC+
FXEUR is3 RO keys (shared code and two instances); primaryUSDC+DEXPYUSD or DEXUSDT0
also3 RO. Do not sum isolated envelopes as incremental financial-root cost or equate
unique keys with SDK entry counts. The accepted financial fixture uses synthetic
oracle bytecode. Every affected prepared/funded/converted/mint/invest/priced-close
root needs the selected real-feed guard inside the unchanged full transaction budget,
including arithmetic, freshness/error paths and restored dependencies. This study
cannot claim resource qualification from isolated getters.

A depeg guard must independently compare pool ratio to the fair leg-price ratio and
stablecoin price to its peg reference, pause new deposits/share-backed borrowing at
its admission tolerance, invalidate priced valuation at its wider bound or stale/
missing feed, and preserve permitted in-kind exit/recovery without treating missing
prices as parity. Numeric tolerances are proposed by the lead, not production settings
in this evidence. **The guard limits new exposure; it does not recover an existing
position's loss.** It cannot turn an unbacked/nonredeemable token into collateral with
a principal guarantee.

## Reproduction

```sh
python3 research/cl-pool-selection-20261111/price-history/analyze.py
python3 research/cl-pool-selection-20261111/price-history/verify.py
```

`pull-history.py` and `read-oracles.cjs` are optional live read-only sampling, not
reproducible pinned replay; preserve a new checkpoint instead of replacing this one.
No Stage1 or lending build occurred. All current policy/production guards remain intact.
