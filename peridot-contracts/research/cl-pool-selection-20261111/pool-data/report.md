# Pool identity, executed-volume history and range weights

Read-only data pull on 2026-11-11. No wallet, signatures, transaction submission or financial execution. `getters.json` independently checks the public contract instances and unsigned getters at ledgers65420650–51; `sources.json` timestamps and hashes API/document responses. These are rolling observations, not an atomic historical snapshot.

## Identity correction

The supplied CAPIO address is **concentrated PYUSD/USDC**, not stable-swap and not sUSD/PYUSD. Its on-chain `get_tokens` agrees with exact-issuer metadata. The actual concentrated PYUSD/sUSD pool is CC7… below. All five economic candidates use current WASM`155a17b9929ffb1f9e84bd6ef5c00a4d613c1ab5f4ad4c502d84515250cc2907`, distinct from accepted historical fixture12fca5…. The discovery list contains378 pools; exact pair/type matching found these candidates and distinct stable counterparts. No ticker-only match was used.

|#|Pair|Concentrated pool|Fee tier / LP fee share|Current TVL indexed USD|
|---|---|---|---|---:|
|1|PYUSD/USDC|CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX|10bps /50%|247,599.97|
|2|PYUSD/sUSD|CC7IGNJ4CDAKZPOZJCTTWGAVKTQXKYQGUSVJ5CDUPES3QULCQW6UNAH5|10bps /50%|5,965.59|
|3|USDC/sUSD|CBV6LZABOAJRXHLIEKILTWBPC2AXLYVR4CGGPKZQ3PWJ5QFTPIWSWGWQ|10bps /50%|121,067.15|
|4|USDC/EURC|CDTSE6RLRI7ZO25JSER6E4SQR4PHJJNONEGS5HDJ3Y6LAKECRZKYN5CA|30bps /50%|94,833.21|
|5|USDT0/USDC|CCJHUCHZ2DC6MP7LMGQJYSENNTGIFWU4J4TCGFYYDLAC5LTD4YMF6HSQ|10bps /50%|64,988.60|

Issuer and SAC identities:

|Asset|Issuer|SAC|
|---|---|---|
|PYUSD|GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5|CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2|
|USDC|GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN|CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75|
|sUSD|GCHW7CWI7GMIYQYFXMFJNJX5645XGWIINIAEQK3SABQO6CAYL5T7JYIH|CDFZUVS5YNLXU7VENKOUDEOHCJGKQNVUBWD7KMN6E7ZROKPYPFLRUJFG|
|EURC|GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2|CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV|
|USDT0|GATISXX6BZ6NC7IKQBY37CJD4SOZL3CYZJWXEDG6JVIY4WBS6KXJHN6Q|CBSJZEIO5C7KC2SF3MKSNXXJSW5G3VTNBX4ATMKUI3B2MR4JKM4R26YF|

All token decimals7, independently getter-confirmed. EURC is directly a leg only in pool4. A EURC depositor using pool3 must convert EURC into USD-denominated assets and then bears USDC/sUSD, not continuing euro exposure. For pool4, an initially EURC-only depositor bears a changing EURC/USDC mixture and EUR/USD FX exposure; a pool price change caused by EUR/USD is not by itself a euro-token depeg.

## Completed week: executed volume and ordinary LP fee estimate

Window is **November4 00:00 through November11 00:00 UTC**, not a rolling incomplete day. Complete indexer event pagination retains104/606/4495/1576/3 events, with39/580/4375/1531/0 swaps. Five pool histories extend roughlyOctober12–November11; pool5's last swap/statistic isOctober29. Weekly historical statistics for four completed weeks are retained in `weekly-history.json`.

|Pool|Executed indexed volume USD|Ordinary LP fees, historical USD estimate|UI fee APY|UI AQUA APY|UI total APY|
|---|---:|---:|---:|---:|---:|
|1|1,404.59635|0.70230|0.01396%|7.97485%|7.98880%|
|2|25,404.63795|12.70232|8.14257%|42.68168%|50.82425%|
|3|416,125.12471|208.06256|7.61411%|20.23483%|27.84894%|
|4|66,652.24087|99.97836|4.68000%|4.14209%|8.82209%|
|5|0|0|0|4.92225%|4.92225%|

`ordinary LP fee estimate = indexed executed swap volume × fee tier ×50%`. Actual input token totals are separately retained, along with fees estimated in native token quantities and their current marks. This is an executed-volume-derived estimate; it is **not** a claim of exact fees collected by an individual position. Integer fee rounding and extra unswapped-input remainder distributions when active liquidity is exhausted can differ. `claim_fees` cash payouts also cannot substitute for interval accrual: they may collect older balances.

The displayed8% for pool1 is presently almost entirely AQUA, not its API fee component. Pool2's displayed50% presently combines8.14% fee and42.68% AQUA components. These fields are captured, not inferred from the rounded UI. Differences between the API fee indicator and this fixed completed-week annualization also reflect interval/TVL differences; the exact backend APY averaging/compounding implementation has not been independently reproduced, so no exact reconciliation is claimed where unsupported.

**Pool-wide APY is not CL position APY.** A $1,000 position has no defined fee return until its range, deployed composition, active liquidity competition and time in range are specified. The indexer events expose amounts but do not provide complete swap-step active-liquidity and fee-growth-inside history. A bounded raw-RPC sample confirmed that transaction metadata does expose actual trade fees, pre/post global fee growth, slot/active liquidity and touched TickChunks. That supports a specific historical reconstruction path; it is not fundamentally unavailable. Complete realized finite-position fee income per$1,000, historical time in range and realized arbitrage/LVR loss remain **unproven**, not zero. Current range weights must not be applied retrospectively and labeled realized.

## Actual unsigned range quotes and conditional weekly income

`quotes.json` retains30 unsigned `estimate_deposit_position` returns: each pool ×$1,000/$10,000 ×full/wide/tight. Full uses spacing-aligned maximum bounds. Wide centers approximately±2,000 ticks on the current tick; endpoints round outward to spacing. Tight uses only the current containing tick band,20ticks except EURC60ticks. Tight is a diagnostic, not approved production range. Desired quantities use the range's actual token composition and indexed marks normalized toUSDC=$1; they are not assumed50/50. All raw quote returns, bounds, marks, ledgers and residual cash are available.

Conditional fee share is `newL/(currentActiveL+newL)`. Base AQUA share is `newL/(currentWorkingSupply+newL)`. Maximum boost sensitivity is `2.5newL/(currentWorkingSupply+2.5newL)` and requires sufficient externally funded ICE; no boost is assumed available. These calculations include entrant dilution. At$1,000, tight pool2 would already own57.12% of current active liquidity; at$10,000 simple linear extrapolation is especially invalid.

|Pool|Full: weekly fees / AQUA USD|Wide: weekly fees / AQUA USD|Tight: weekly fees / AQUA USD|Tight maximum-boost AQUA USD|
|---|---:|---:|---:|---:|
|1|0.00000168 /0.000735|0.0000175 /0.007688|0.003343 /1.466987|3.646308|
|2|0.008437 /0.027281|0.087692 /0.283838|7.255667 /25.821698|36.022962|
|3|0.004847 /0.008797|0.050686 /0.091988|9.266076 /16.964287|40.231956|
|4|0.029373 /0.016403|0.302092 /0.168818|16.392430 /9.559642|20.078562|
|5|0 /0.003623|0 /0.037874|0 /6.483889|13.994259|

These are **conditional current-weight weekly scenarios**, not realized returns. Fee scenarios combine the completed week's aggregate ordinary fee estimate with today's diluted range weight. AQUA scenarios extend today's rate over seven days. Historical swaps may traverse other ticks, trigger different active liquidity or take the position out of range. Rebalancing, arbitrage loss and ongoing costs are not included. $10,000 rows and AQUA token quantities are in `calculations.json`; no$1,000-to$10,000 extrapolation.

Current schedule rates in pool order:2.5651983,0.3308042,3.1785207,0.5097735,0.4155747 AQUA/s. **All expire2026-11-11 16:15:19UTC**, long before seven days. [Reward-cycle docs](https://docs.aqua.network/voting-and-rewards/reward-cycle) say allocations resync daily from votes; [voting docs](https://docs.aqua.network/voting-and-rewards/aquarius-voting) require market eligibility/whitelisting and describe vote-weight allocation/caps. Therefore future weekly/annual AQUA is conditional on voting and renewal, not already funded backing. ICEboost capital and liquidity-voting incentives are separate from LP income and cannot be double-counted.

[Current CL documentation](https://docs.aqua.network/developers/concentrated-liquidity) additionally warns that in-range reward weight is checkpointed on deposit/withdraw/claim and may remain stale after trades until the next interaction. A keeper cannot treat stale out-of-range accrual as a durable return assumption, nor omit required claims to exploit it. This behavior must be tested against the exact newWASM.

`conversion-quotes.json` holds120 independent unsigned entry/optional-exit attempts from either original leg, for every range/size. The first59 succeeded, then61 hit HTTP429. `entry-retries.json` preserves30 paced successful entry retries, giving60/60 successful entry estimates in the merged result;31 optional-exit estimates remain unavailable. Original failures are retained. Entry input is initial deposited-leg quantity minus the range-specific quantity retained; it is **not automatically half**. Independent current-state swap and deposit estimates do not establish a sequential post-conversion execution or future exit guarantee. Main report must retain those limitations when using them for conditional net sensitivities.

## Required execution requalification

All five same current poolWASM identities differ from the signed table's accepted venue fixture. Sharing a hash across candidates does not make their tick/storage/capacity states identical. Before the signed table applies: pin the selected pool code and complete required state; run all nine staged roots including actual candidate token ordering/decimals, current range/tick paths, accumulated fee/reward boundaries and extra depeg/oracle reads; repeat exact auth, return/event/accounting comparisons and cold restoration journeys; include changed initializer/key/serialized bytes and Core fee parity. This task does not execute that qualification or authorize production rollout.

Reproduce offline: `python3 research/cl-pool-selection-20261111/pool-data/calculate.py`; verify retained files with `verify.py`. API responses are primary indexer evidence, not independent full-chain historical reconstruction. No mathematically defensible realized-net ranking can be obtained from the current-weight scenarios alone.

## Bounded historical fee-density evidence

Four recent public swap transaction samples succeeded; pool5's October29 swap is
outside the RPC retention window (`NOT_FOUND`). The first swap of the completed
November4–11 week is likewise `NOT_FOUND` for each active pool. We then fetched one
first-retained and one last-week transaction per pool. Both endpoint instances have
the same currentWASM and positive global fee-growth deltas. Raw metadata, XDR,
transaction hashes, times and decoded states are retained in `fee-boundaries.json`
and individual `pN-*-boundary.json`; `fee-density.py` computes exactQ128 differences.

|Pool|Observed endpoint interval UTC|Days|Full-range reference$1k fee USD at current marks|Seven-day normalized sensitivity|
|---|---|---:|---:|---:|
|1|Nov4 12:17:14–Nov10 16:33:17|6.17781|0.00000154465|0.00000175022|
|2|Nov4 10:34:14–Nov10 19:12:12|6.35970|0.01234189|0.01358449|
|3|Nov4 09:47:19–Nov10 23:42:52|6.58024|0.02346207|0.02495873|
|4|Nov4 10:33:49–Nov10 23:42:57|6.54801|0.02418669|0.02585623|

This is historical fee-density times the current-quoted reference full-rangeL. It is
not an actual$1k funded position's P&L or a finite-size counterfactual simulation:
adding that position changes active liquidity and the swap path. The$10k reference
is in `fee-density.json` but must not be used as a self-diluted production projection.
Endpoint identity alone does not audit all intervening resets/upgrades. No partial
window is called a complete week. Wide/tight reconstruction additionally requires
boundary tick feeGrowthOutside and full path/time-in-range evidence. The difference
between historical density and current-weight sensitivities is material—pool3's
historical reference density is about5× its current-weight fee scenario.

No further history crawl or financial study was run. The data supports a precise
next historical-reconstruction task if needed, rather than inventing realized nets
from current pool APY. All source/manifest verification is offline and no release
qualification is claimed.
