#!/usr/bin/env python3
from pathlib import Path
import json
H=Path(__file__).resolve().parent;P=H.parent
pools=json.loads((P/'pool-data/calculations.json').read_text())['pools'];nets=json.loads((H/'net-sensitivity.json').read_text())['rows'];density=json.loads((P/'pool-data/fee-density.json').read_text());weeks=json.loads((P/'pool-data/weekly-history.json').read_text());base='../../peridot-contracts/research/cl-pool-selection-20261111/'
name=lambda p:'/'.join(x.split(':')[0]for x in p['pair'])
lines=['''# Stage 1 five-pool selection — Day 1 evidence and team-only rollout decision

11 November 2026. Accepted reusable ABI checkpoint `b7b433c4885db5878f7fa6f25f3d1fad2b5a07b3` remains unchanged. Package2 is paused until the operator chooses the pair. This is one bounded read-only study, with data/history and cash-pilot scoping agents in parallel; no contract build, signature, deployment or public financial mutation.

**Recommendation:** PYUSD/USDC is the first technical candidate for the team-only rehearsal/launch, because it preserves the closest asset/execution shape and avoids sUSD's unsupported primary price and EUR/USD exposure. **USDC/EURC is the second candidate**, if the operator wants explicit currency exposure and the current pool/oracle discrepancy is resolved. Neither is established as a profitable wide-range product above lending. The strongest conditional wide-range economics is EURC, not PYUSD. No pool has a proven realized-net ranking from the available evidence.

## Pool table first

The supplied CAPIO address is independently confirmed by fresh on-chain token getters as **PYUSD/USDC concentrated liquidity**. The intended PYUSD/sUSD CL pool is CC7… below. Separate PYUSD/USDC stable-swap pool CDMH535JSD224YXPET3B4SJOLXTQQ24GRSCWACGYBKSH2DKFJYWI7SUW is excluded. The earlier report's full/wide figures were for CAPIO CL, not the stable-swap pool; they were not returns for a tight concentrated position. This study adds the actual sUSD pool and compares explicit ranges rather than treating pool-wide APY as position APY.

Current indexed TVL; completed-week volume November4–11UTC. Fee estimates use actual indexed executed volume×fee×verified50% LP share. They are aggregate ordinary fees, not exact position collections. All values USD-normalized to USDC=$1 for this historical indexer comparison.

|#|Pair|TVL|7-day volume|Fee tier / LP share|7-day aggregate ordinary LP fees|Realized net at$10k|
|---|---|---:|---:|---|---:|---|''']
for i,p in enumerate(pools,1):lines.append(f'|{i}|{name(p)}|${p["tvl_usd"]:,.2f}|${p["week_volume_historical_indexed_usd"]:,.2f}|{p["fee_fraction"]*10000:.0f}bps /50%|${p["week_lp_fee_estimate_historical_usd"]:,.5f}|UNPROVEN|')
lines+=['\n|#|Exact concentrated pool address|\n|---|---|']
for i,p in enumerate(pools,1):lines.append(f'|{i}|`{p["address"]}`|')
lines+=['''
EURC is a native pool leg in pool4, not pool3. The current receipt prototype accepts one configured underlying asset; a second-leg economics benchmark is not a promise that both tokens can be supplied through that receipt ABI. Selecting EURC as the receipt underlying, or entering pool3 from EURC, requires explicitly specified binding/order or conversion work and remeasurement; no generic extra zap is included here.

Exact issuers/SACs, fresh token getters, decimal7 checks, pool types and WASMs are in the [identity report]('''+base+'''pool-data/report.md). sUSD is **synt.tech's exact-issued asset, not Synthetix sUSD**. Its issuer says it is algorithmically market-made, unbacked by the tracked asset and nonredeemable. That is a different risk from Circle/Paxos fiat redemption or USDT0's bridge/backing/attestation risk. See the [asset/price report]('''+base+'''price-history/report.md) and [issuer statement](https://synt.tech/).

## What is measured, and why pool APY is insufficient

There is no unique “$1,000 in-range position” until a range, composition, interval and reinvestment/recentering policy are specified. Full-range, simple-wide (approximately±2,000ticks centered on the observed tick), and one-containing-tick-band tight diagnostics were quoted separately at$1,000 and$10,000. Tight is20ticks except EURC60; exact bounds/amounts are pinned. No narrower production range or active management is adopted.

All four complete historical weeks are retained. Aggregate ordinary LP fee estimates by week:

|Pool|Oct14–21|Oct21–28|Oct28–Nov4|Nov4–11|
|---|---:|---:|---:|---:|''']
for p,w in zip(pools,weeks):lines.append('|'+name(p)+'|'+'|'.join(f'${x["ordinary_lp_fee_estimate_usd"]:.5f}'for x in w['weeks'])+'|')
lines+=['''
There were39/580/4,375/1,531/0 swaps in the latest complete week respectively. Zero pool5 swaps is supported by complete event pagination; its last indexed swap wasOctober29. Old statistics outside an active week are not forward-filled as fees.

Raw transaction metadata independently supplies global fee growth, actual trade fees, slot/active-liquidity state and touched tick records. First complete-week boundary transactions are outside the public RPC retention window; bounded first-retained/last-week captures provide these **partial historical fee-density observations**:

|Pool|Observed days|Fee-density×reference$1k full-range liquidity, actual interval|7-day normalized sensitivity|
|---|---:|---:|---:|''']
for d in density:
 if 'reference_full_range'in d:
  q=d['reference_full_range'][0];lines.append(f'|{d["pool"]}|{d["elapsed_days"]:.5f}|${q["interval_fee_usd_at_current_marks"]:.9f}|${q["seven_day_normalized_fee_usd_sensitivity"]:.9f}|')
lines+=['''
These are authenticated fee-density differences, marked at current prices and multiplied by current reference liquidity. They are **not an executed finite$1,000 deposit**, and adding liquidity changes the swap path. Endpoint WASMs match; all intervening upgrades/resets have not been audited. No partial window is called a full week. Full/wide/tight finite-position realized fees and time in range require ordered tick/feeGrowthOutside and active-liquidity reconstruction. The available metadata provides a concrete route to that reconstruction, but this one-day study did not crawl/replay thousands of full transactions. Pool3 historical density is about5× the current-weight fee estimate: this directly demonstrates why current liquidity weights cannot be labeled realized history.

The requested realized arbitrage loss per week is **unmeasured**, not zero. Daily prices cannot value each ordered swap against an independent synchronized mark or construct handoff00's delta-matched benchmark. Available Reflector price history retains24h, not90days. No terminal IL or naive volatility formula is substituted for realized arbitrage loss. This is the exact evidence gap preventing a truthful realized-net ranking.

### UI reconciliation

|Pool|Captured API fee APY|AQUA APY|Total APY|
|---|---:|---:|---:|''']
for p in pools:lines.append(f'|{name(p)}|{p["ui_fee_apy_percent"]:.5f}%|{p["ui_aqua_apy_percent"]:.5f}%|{p["ui_total_apy_percent"]:.5f}%|')
lines+=['''
Pool-wide0.014% does **not** determine a tight position's return. Conversely, the captured roughly8% total for CAPIO currently consists primarily of AQUA, while actual PYUSD/sUSD currently shows8.14% fee plus42.68% AQUA. The position estimator may display a different number for a selected range. Fixed completed-week volume, varying TVL, range concentration, time in range, fees versus reward components and averaging/compounding conventions explain why these quantities differ. The precise backend averaging implementation is not reproduced; an exact numeric UI reconciliation is not claimed. No UI value is used as measured realized revenue.

## Conditional weekly income per$1,000 — range and dilution explicit

The following is a **current-weight sensitivity**, not historical position earnings: completed-week aggregate fees×L/(currentActiveL+L); current AQUA rate×604800×L/(currentWorkingSupply+L). Full amounts consumed and residual cash are retained. Maximum boost is recomputed as2.5L/(currentWorkingSupply+2.5L), not simply multiplying final income2.5×. No ICE capital/boost is assumed owned.

|Pool|Full fees / AQUA USD weekly|Wide fees / AQUA USD weekly|Tight fees / AQUA USD weekly|Tight AQUA tokens / max-boost USD weekly|
|---|---:|---:|---:|---:|''']
for p in pools:
 qs={q['range']:q for q in p['positions']if q['capital_usd']==1000};t=qs['tight'];v=[]
 for ran in ['full','wide','tight']:
  q=qs[ran];v.append(f'${q["conditional_week_fees_using_current_weight_usd"]:.7f} /${q["conditional_week_aqua_usd"]:.6f}')
 lines.append('|'+name(p)+'|'+'|'.join(v)+f'|{t["conditional_week_aqua_tokens"]:,.2f} AQUA /${t["conditional_week_aqua_maxboost_usd"]:.6f}|')
lines+=['''
Full/wide boost and AQUA-unit rows at both sizes are in `pool-data/calculations.json`. AQUA/s in pool order:2.5651983,0.3308042,3.1785207,0.5097735,0.4155747. **Every observed schedule ends2026-11-11 16:15:19UTC.** Weekly and annual continuations require renewed allocations; [daily reward cycles](https://docs.aqua.network/voting-and-rewards/reward-cycle), [votes/eligibility](https://docs.aqua.network/voting-and-rewards/aquarius-voting) and competition can reduce them. Reward range weight is checkpointed on interactions in current CL documentation; stale out-of-range weight is not a durable-yield assumption or a reason to skip claim-first boundaries. Unclaimed/quarantined/non-distributable rewards count zero to the supplier.

## Conditional annual net — not a realized ranking

Same one-ledger framework as the accepted economics study: current marked starting capital, actual unsigned range-specific entry quote from each deposited leg, idle residual retained, one measured warm in-kind lifecycle anchor0.5744714XLM (about$0.08387), fee/reward income, final constant-price value versus holding the original leg. Do not sum independent cold restorations into a normal lifecycle. The old fee anchor is not a current selected-pool signed fee quote.

Sixty entry quotes succeeded (rate-limit failures and paced retries preserved). Liquidity is conservatively limited by both post-quote token amounts at the original independent deposit quote; no assumption of half-capital conversion for asymmetric tight ranges. These are still **independent rolling prestates**, not a sequential swap→investment execution. Initial/future conversion, newWASM gas and capacity need selected-fixture measurement. Thirty-one optional-exit quotes remain rate-limited/unavailable; the table uses accepted in-kind exit, not a promised conversion back to the deposited asset.

Annual sensitivities assume current fee week repeats, current competition, uninterrupted range participation, renewed rewards, no price-path loss and zero additional recurring OPEX. Each holding benchmark has constant captured prices and no unverified issuer distributions. In particular sUSD's advertised issuer distribution is not credited to either side; actual received distributions would need consistent inclusion. Negative marked entry costs (especially USDC→PYUSD) reflect an index/pool valuation basis, **not realized arbitrage profit or recurring yield**.

Each cell below shows net dollars **from holding/depositing leg0 / leg1**, in the pair order shown. This distinguishes both requested hold benchmarks. Full versus wide choices are separate.

|Pool|$1k full|$1k wide|$10k full|$10k wide|
|---|---:|---:|---:|---:|''']
for p in pools:
 vals=[]
 for cap,ran in [(1000,'full'),(1000,'wide'),(10000,'full'),(10000,'wide')]:
  rs=sorted([r for r in nets if r['pool']==p['address']and r['capital_usd']==cap and r['range']==ran],key=lambda r:r['deposited_leg'])
  vals.append(' / '.join(f'${r["annual_net_constant_marks_renewed_rewards_usd"]:+,.2f}'for r in rs))
 lines.append('|'+name(p)+'|'+'|'.join(vals)+'|')
lines+=['''
Blend3.07% fixed comparison is$30.70/$307 annually before its own entry/exit costs, changing utilization and credit risk—not a guaranteed future rate or refreshed lending recommendation. **Every full/wide base case is below that comparison.** Fifty-percent earnings retention, zero-distributable-AQUA and max-boost sensitivities are retained separately in `lead/net-sensitivity.json`, not approved production parameters. Operating XLM, supplier cash, loss reserve and liquidation funding remain separate.

A provisional **conditional wide/token0** order is4(EURC),2(PYUSD/sUSD),3(USDC/sUSD),5(USDT0/USDC),1(PYUSD/USDC). This is explicitly **not the requested realized-net ranking**, which remains unavailable. Entry from the other leg can reverse that ordering. In particular pool2's$10k sUSD-funded wide quote loses about$4,614 of marked value before lifecycle costs; its$10k PYUSD-funded wide quote loses about$50. Its TVL is only about$6k. High displayed APY does not establish scalable entry or exit.

Tight-range$10k annual sensitivities are stored but are not launch forecasts: roughly$702/$742(PYUSD/USDC),-$1,611/+$2,957(PYUSD/sUSD),$10,115/$10,105(USDC/sUSD),$5,761/$5,702(USDC/EURC),$1,721/$1,728(USDT0/USDC), from leg0/leg1. They assume remaining in a20/60-tick interval for a year, omit recentering/arbitrage loss and can dominate current active liquidity. Those assumptions are not measured or credible production bounds. They illustrate why a position display can differ radically from pool APY, not grounds to adopt a narrow strategy now.

## Ninety-day depeg/FX evidence and guard

Exact-issuer classic-market daily buckets coverAugust13–November10:90days for older pairs and only70days for USDT0. These are not Aquarius historical ticks or executable$10k quotes. Worst downside daily VWAP observations: sUSD/PYUSD1.38791%, sUSD/USDC1.15613%, PYUSD/USDC0.11655%, USDT0/USDC0.79526%. Raw intraday extremes are retained separately and can be dust: the USDT010000print was0.0002696tokens. It is not a usable depeg price. Full90-day USDT0 evidence does not exist in this response.

EURC's64 same-calendar-day ECB comparisons show up to2.45126% discount, with unsynchronized daily timing and USDC-basis limitations. EUR/USD movement is separate: observed ECB start/end1.1534→1.1214 would cost a$1k full/wide LP about$13.97/$14.90 versus holding USDC even with a perfect euro peg, while outperforming holding EURC by about$13.77/$12.85.

At the downside daily marks, analytic$1k full/wide losses versus holding the counter leg are$6.96/$7.19 for sUSD/PYUSD,$5.80/$5.96 for sUSD/USDC,$0.58/$0.58 for PYUSD/USDC,$3.98/$4.06 for USDT0/USDC, and$12.33/$13.06 for EURC's relative ECB discount. Versus holding the depegging leg, the mixture can outperform instead. Exact both-leg and initial-mixture decomposition is in the [price-history report]('''+base+'''price-history/report.md). These are terminal analytic sensitivities, not realized arbitrage loss, a replay or safe-cap extrapolation. Do not subtract them again from a ledger already marking that price move.

The [depeg-guard specification]('''+base+'''lead/depeg-guard.md) proposes, without configuring production,50bps to pause new deposits/mint/investment/share-backed borrowing and200bps or stale/missing prices to invalidate priced valuation, retaining permitted in-kind exit/recovery and repayment. Both each leg's primary peg deviation and pool/oracle relative deviation are checked. EURC is checked against EUR/USD, not$1. **This guard limits new exposure; it does not recover existing-position losses.** It cannot create a fair price-based seizure during a stale oracle; that remains a later2× liquidation limitation.

The configured primary lacks PYUSD/sUSD/USDT0; same-provider exact-asset DEX primary supplies PYUSD/USDT0 with USDC-base conversion, but not sUSD. A new feed mapping must be explicitly bound/reviewed, not silently substituted. Proposed-mapping snapshot tests give pool/oracle deviations about0.17bps(PYUSD),1.28bps(USDT0),74.11bps(EURC). EURC would pause fresh risk under the proposed50bps candidate; do not widen it merely to admit today's quote. Missing sUSD makes both sUSD valuations invalid under the requested guard. Primary coverage is not solved by ticker matching or quoting the same pool.

Resource attribution reuses accepted traces: two synthetic price reads plus two resolution reads already cost about1.49MCPU and one slot read1.07M, included in old roots. Fresh isolated real-primary simulations declare0.807–1.292M instructions per relevant price getter, with three-key unions for required two-feed combinations; they do not supply full-root memory/SDK totals. No sum of independent declarations is an exact incremental cost. The new guard's complete transaction cost remains unmeasured and must be inside the chosen live-WASM nine-row budget, not an unmeasured keeper prerequisite. Signed baseline warm investment has4,855,188CPU headroom; cold4,794,234. Those numbers are a measurement constraint, not proof the extra guard fits.

## Verdicts and choice

|Pool|Verdict for the current launch choice|Why / later product|
|---|---|---|
|PYUSD/USDC|**Preferred first1× technical candidate**|Closest tested asset shape, exact primary mapping possible, good relative price alignment; wide economics below lending and near-zero fee flow. Team launch is a product/execution pilot, not a demonstrated yield advantage.|
|USDC/EURC|**Preferred second1× candidate, conditional**|Best modeled wide return among requested pools; real fee activity and primary/FX coverage. Requires accepted EUR/USD exposure and resolution of current pool/oracle basis. No2× recommendation from this study.|
|USDC/sUSD|**Reject for this week's launch; later1×/2× research candidate only**|Most weekly dollar fee activity, but no supported sUSD primary, unbacked/nonredeemable asset, and missing realized loss/exit-capacity evidence. Handoff00 permits later2× only if these returns/pricing/exits become adequate.|
|USDT0/USDC|**Reject as launch/second now; retain family interface**|No swaps/fees in completed week; reward-dependent and fewer price-history days, bridge/issuer/local-basis risks. Exact-primary mapping possible does not supply organic yield.|
|PYUSD/sUSD|**Reject at requested$10k research size**|Thin~$6k pool, severe directional entry shortfall, absent sUSD primary and high dependence on renewable rewards. Do not infer safe smaller cap from a quote.|

This is a technical/risk recommendation, deliberately distinct from the conditional numerical order above. **No pool can presently be ranked by proven realized net at$10k.** The missing items are exact finite-position historical fee/range reconstruction, synchronized independent swap-time prices/markouts, actual distributed rewards and path-consistent costs—not UI APY precision. No additional broad study is launched automatically.

Shared00§2 disposition: “At research sizes$1k/$10k, no pool yet establishes profitable executable capacity or a realized net-return range. Under static current-weight, renewed-reward, zero-extra-OPEX assumptions, the simple-wide$10k first-leg results range about-$15.51 to+$208.02 annually, below the$307 lending comparison; tighter diagnostics are unqualified. Pair choice therefore selects a team-capital product prototype, not an approved safe cap or principal guarantee.”

## Current rollout instructions and remeasurement

One vault per chosen stable pair, same accepted ABI. The ABI is reusable; the old prototype's token order/underlying choice, fixed range math, guard source and code identities are not automatically portable. Do not reinterpret EURC asUSD or silently reverse token0. This report makes no ABI or contract change.

Operator's revised order: Day1 this table and pair choice; Day2 remeasure all nine signed transactions against chosen live Aquarius code/state; Days3–4 prepare the exact receipt/vault-only deployment manifest and Ledger ceremony, team allowlist/cap named by operator, borrowing disabled, keeper investment/finalization, forked-live close rehearsal before signatures, then owner-approved first live open. No facade. G1–G4 are explicitly gates for **lifting the allowlist/public deposits**, not team-only deployment gates under the new instruction. Packages2–9 continue in parallel toward public access after the team prototype, as instructed. No public access is enabled by this study.

All five current pools share `155a17b9929ffb1f9e84bd6ef5c00a4d613c1ab5f4ad4c502d84515250cc2907`; accepted fixture is `12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`. The first package3 task remeasures O1prepare,O2fund,O3convert,O4fee-checkpoint/cash-mint,O5keeper-invest,C1prepare-close,C2ain-kind,C2bwithdraw-for-conversion,C3optional-convert, plus the complete cold journey. Pin actual asset/range/oracle bindings and final allowlist/cap artifact; a table for earlier bytes is not evidence for modified deployed bytes. Include ordinary/source and detached private-fixture authorization, restoration, all fee/byte/event/memory checks and claim-first accounting. Network boundary remains100M/41,943,040/100SDK/100unique/50writes; an over-limit row stops deployment. No rule is loosened or claim boundary removed to fit.

The Day3–4 runbook must enumerate every owner Ledger signature, exact unsigned payload, read-back and abort/retry state; operator names public team addresses and capital. The close rehearsal and the requested depeg guard remain part of the stated artifact/behavior, not silently omitted because G1–G4 are deferred. Current validation-only Mainnet guards are preserved in this research; any release-artifact distinction must be explicit in that reviewed ceremony. No Mainnet signing/broadcast authority is inferred for the agent. No further signing occurred here, including private fixtures.

## Parallel cash-only USDC/XLM pilot scope

Item(b) is complete as a document, after the pool table: [cash pilot scope](../../peridot-contracts/research/cl-cash-pilot-scope-20261111/report.md). Five fresh instances, no old compromised deployment, no Upshift/DeFindex/margin binding. Reserve ABI exists in default source; planner is unsigned. Complete compiled cash supply/borrow/repay/liquidate resource rows are missing, so none receives an inferred pass. Native synthetic borrow51SDK is not a complete cash transaction qualification. Ordinary elapsed-interest repay_max auth and stale-primary liquidation remain explicit gaps. Estimate13–21 engineer-days plus3–5 contingency, borrowing disabled. No cash-lending build was performed.

## Reproducibility and honest gates

From `peridot-contracts`:

```sh
python3 research/cl-pool-selection-20261111/pool-data/calculate.py
python3 research/cl-pool-selection-20261111/pool-data/fee-density.py
python3 research/cl-pool-selection-20261111/pool-data/verify.py
python3 research/cl-pool-selection-20261111/price-history/analyze.py
python3 research/cl-pool-selection-20261111/price-history/verify.py
python3 research/cl-pool-selection-20261111/lead/calculate_net.py
python3 research/cl-pool-selection-20261111/lead/calculate_guard.py
python3 research/cl-pool-selection-20261111/lead/resource_attribution.py
python3 research/cl-pool-selection-20261111/verify.py
python3 research/cl-cash-pilot-scope-20261111/verify.py
```

Replay checks file identities/arithmetic, not financial release. All raw failed requests are retained; source/manifest verification is offline. No new financial tests, builds or check_budget flows were run, no old missing-flow gate changed, and no Option1 work resumed. Primary-only research; no paid provider/fallback integration. Handoff00, production settings, borrowing/caps/MM, accepted ABI and prior artifacts remain unchanged. The independent review found no critical net-math/unit error, and required explicit labels for proposed feed mappings and negative marked-entry costs; both are retained above.
''']
Path('/tmp/cl-five-pool-report.md').write_text('\n'.join(lines)+'\n')
print('wrote /tmp/cl-five-pool-report.md')
