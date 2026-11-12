# XLM/yXLM: issuer yield, redemption, oracle and reward-sale valuation

Bounded public read-only addition, 2026-11-11. This does not replace or overwrite the accepted three-pool decomposition. No transaction creation at the anchor, authentication, signing, submission, build or deployment was performed. No new historical study was run. The Stellar data skill was used for public RPC and Horizon reads.

## Exact asset and pool

CL pool `CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F`: token0 native XLM SAC `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA`, token1 yXLM SAC `CBZVSNVB55ANF24QVJL2K5QCLOAB6XITGTGXYEAF6NPTXYKEJUYQOHFC`. Issuer `GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55`, issuer home domain `ultracapital.xyz`, organization Ultra Capital LLC per its stellar.toml. Fresh getters confirm fee10bps, protocol fee fraction50% of that fee, version220. This is the CL pool, not the older stable pool `CCFGZ...JB3ZH3`.

Issuer account flags currently report auth-required/revocable/clawback allfalse and auth-immutable false. That is a current flag reading, not an assurance about issuer backing or future availability. XLM/yXLM is a correlated **XLM-risk** pair, not a USD-stable pair: both legs retain XLM/USD market exposure; yXLM adds issuer/redemption risk.

## Yield is a separate payout, not price accretion

The [current asset page](https://ultracapital.xyz/assets/yxlm) advertises **1.6% APY**, distributed daily based on account-balance snapshots. Extra yXLM compounds if retained. The [issuer TOML](https://ultracapital.xyz/.well-known/stellar.toml) promises nominal1:1 yXLM/XLM redemption. Thus this is a daily coupon mechanism; there is no documented steadily increasing redemption exchange rate to add to NAV. Advertised rates may change. This bounded check does not independently reconstruct actual historic payout APY or audit reserves.

**CL coupon entitlement is unproven.** The publisher describes wallet balances, trustlines and daily account payouts. It does not establish recognition of Soroban contract balances, CL position beneficial ownership, or the receipt/vault's beneficiaries. Even if a pool receives a direct token payment, that alone does not establish that its internal reserves, position accounting and withdrawal entitlement attribute it to this LP. No1.6% issuer yield is included in the CL base case without this evidence. Coupon receipts, if supported later, need explicit attribution and conservation; they cannot be treated as unexplained donated inventory or counted again as price accretion. Borrower/custody reward rules remain relevant when the later margin milestone begins.

## Redemption terms and practical boundary

Public [deposit form](https://ultracapital.xyz/direct-deposit-yXLM/) says no fee,1XLM→1yXLM, minimum20XLM and maximum500,000XLM. Public [withdrawal form](https://ultracapital.xyz/direct-withdraw-yXLM/) says a **fixed0.5yXLM fee**, minimum20 and maximum100,000yXLM. Unauthenticated SEP24 `/info` and `/fee?operation=...&asset_code=yXLM&amount=10000` independently confirm availability/limits and fee0.5 withdrawal/0 deposit. Additional Stellar transaction fees are separate. The fixed charge is about$0.07424408 at the captured primary XLM/USD price, assuming nominalpar.

Both direct forms ask for a **G-address** destination. SEP24 interactive redemption is not a proved synchronous, permissionless Soroban C-address redemption ABI. SEP6 info does not list yXLM. Do not count issuer redemption as an automatic vault exit, or promise immediate XLM conversion. No form was submitted and no capacity, settlement time, KYC decision, actual redemption or backing was tested. An in-kind exit can deliver the asset mixture; the holder still faces yXLM conversion/redemption conditions.

## Fresh oracle and pool ratio

RPC reads at ledgers65421640–65421642; oracle lastprice timestamp1794394200 (2026-11-11 10:50:00UTC), resolution300seconds, decimals14.

| Reading | Result |
|---|---|
| CL sqrt price |`79259974659293628166848868508`, tick8|
| Pool yXLM per XLM |1.000803212663698645|
| Pool XLM per yXLM |0.999197431969107247|
| Discount to issuer's nominal1:1 promise |−8.025680309bp|
| Existing external primary `Other(XLM)` |0.14848816077125 USD/XLM|
| External primary `Other(USDC)` |1.00077597650360 USD/USDC|
| Stellar DEX primary `Stellar(XLM SAC)` |0.14851761814408 USDC/XLM|
| External primary `Other(yXLM)` |None|
| Stellar DEX primary `Stellar(yXLM SAC)` |None|

External primary is `CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN`, baseUSD. Stellar DEX primary is `CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M`, baseCircleUSDC. Both bases read directly. Current Reflector config also omits yXLM. There is **no verified yXLM independent primary quote** in this existing configuration. The8bp nominal peg reading cannot establish reserve backing or a depeg-safe valuation feed.

For the requested guard design, XLM's primary price alone cannot validate yXLM's independent redemption/credit value. Multiplying the pool-implied yXLM/XLM ratio by the XLM oracle reuses the market being checked, so it is not an independent depeg check. No new publisher/fallback integration is proposed here. Until a qualified primary coverage/valuation decision exists, independent yXLM valuation remains a blocker to claiming this candidate meets the specified guard. Retain freshness, issuer checks and execution-price limits; invalid/unavailable valuation must leave in-kind exit/recovery available. Guards constrain new exposure, not restore an existing loss. Complete signed-root cost of any added reads is unmeasured; successful getter declarations are not financial-root resource proofs.

## Comparable holding benchmarks

Let `P(t)` be USD/XLM and `q(t)` be XLM/yXLM. Holding XLM tracks `P(t)` and has no issuer coupon. Holding yXLM tracks `P(t)×q(t)` plus **actually received** coupons; nominalpar is a promise, not a market-price guarantee. A CL portfolio tracks its changing mixture plus fees and realized rewards, less losses/costs. Compare all strategies on the same initial capital, same terminalXLM price and same actual conversion assumptions.

| Conditional benchmark, constant XLM/USD and no depeg |$1,000 capital|$10,000 capital|
|---|---:|---:|
| Hold XLM, coupon income |0|0|
| Hold yXLM, publisher1.6% annual continuation |$16|$160|
| Illustrative50% yXLM LP weight, **only if** coupon eligibility/allocation proved |$8|$80|
| CL issuer coupon admitted without that proof |0|0|

The50% row is an explanatory sensitivity, not the measured wide-position composition. Actual holdings and daily snapshots determine the eligible amount. Coupon APY must not be applied to the entire XLM/yXLM LP; underlying coupon cannot also be counted as redemption-price accretion. Subtract entry/exit, sale, redemption and operating costs once. These are conditional benchmarks, not measured realized returns; neither the annual rate nor the year's payouts are guaranteed. Starting/ending market discounts and XLM/USD movement can dominate these amounts.

## Reproducibility and remaining evidence

`reads.json` preserves getter arguments, returnXDRs, timestamps, ledgers and resource declarations. `sources.json` pins exact public URLs and bodies including the legacy Ultra Stellar404. `calculated.json` reproduces normalized ratios and benchmark arithmetic. `quote.cjs` records only unsigned `estimate_swap` when supplied an amount file; it never signs or submits. Public daily payout history, reserve audit, smart-contract payout eligibility, actual redemption capacity, independent yXLM primary valuation and complete recurring claim/sale costs are not proved by this bounded study. No90-day study or additional infrastructure was started.

## AQUA proceeds: current quotes and preserved realized reference

The lead supplied base weekly322.3424426241AQUA; the boost calculation supplied746.8763607AQUA for the annual-prefunded, three-year-lock sensitivity, or763.5441817AQUA for the day-one minimum lock. These are current-weight model amounts, not measured historic position earnings. Unsigned `estimate_swap` in the same direct Aquarius AQUA/USDC CP pool `CA6GAFOJCW4MGQQBUCQUSA3CLIH25G4SNKB2JHYKZCVWZTNW5VXMSC4O` gives:

| Case | AQUA/week, truncated7decimals | Net USDC quote/week | Conditional annual USDC,365/7 |
|---|---:|---:|---:|
| Base, noICE |322.3424426|0.0795011|4.14541450|
| Boost, ICEcapital within$10k, yearprefunded |746.8763607|0.1842063|9.60504279|
| Boost, ICEcapital within$10k, dayoneonly |763.5441817|0.1883172|9.81939686|

These are **quotes, not realized sales**; captures ledgers65421671 and65421673, newer than the prior three-pool sale snapshots. Current effective quoted price is about0.0002466356USDC/AQUA. The earlier verified actual third-party venue leg `d056736c75967356e897cd24b6215bbff29676041d51eb5c656291d8b3ec81db`, Nov11 08:56:52UTC, exchanged534.4348539AQUA for0.1296981USDC: realized venue-leg price0.0002426827125. Applying that **historical-price sensitivity** to the three model reward quantities gives0.07822694/0.18125398/0.18529897USDC per week, respectively. Those hypothetical amounts were not executed. The successful Horizon transfer evidence is reused and hash-pinned locally, not re-pulled as a new study.

Quoted outputs include venue swap pricing/fee but exclude claim/sale transaction and keeper fees. Getter resource declarations are not a signed sale measurement. The annual ICE-budget case allocates$9,271.8342387 to LP and$728.1657613 at an indicative mark to2,949,316.2699539 lockedAQUA; it assumes unchanged otherICE/pool/emission inputs and eligibility not yet proved. AQUA acquisition slippage, locking cost and coupon-sale operational overhead remain unmeasured. A three-year lock retains capital/risk beyond the one-year projection. Annual continuation is not funded by the current reward epoch, and current AQUA/USDC sale depth is not a guarantee for later weekly sales. USDC outputs are quantities, not USD cash received; any USD presentation must state its USDC valuation convention.
