# Sixth pool: XLM/yXLM CL, and consolidated decomposition

XLM/yXLM is included as pool6: **`CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F`**. Public discovery lists one concentrated tier for this exact pair,10bp,50% ordinary LP fee share. It is distinct from the stable-swap pool. Token0 is nativeXLM SAC `CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA`; token1 yXLM SAC `CBZVSNVB55ANF24QVJL2K5QCLOAB6XITGTGXYEAF6NPTXYKEJUYQOHFC`, issuer `GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55`. This pair retains XLM/USD exposure; it is not a dollar-stable vault.

This extends preserved checkpoint4c76b104 and report31. sUSD pools2/3 remain rejected; USDT0 pool5 remains deferred. No sixth-pool build, signature, deployment or cap setting occurred. Data for pool6 are captured around10:50–10:53UTC onNovember11; rows1/4/5 preserve the earlier10:13–10:19UTC captures rather than pretending every quote shares one prestate.

## One decomposition table

UI components are verified from the deployed UI: “Swap Fees” + “AQUA Rewards (Base)”; the maximum multiplies rewards2.5×, never fees. Percentages are API APY fields rendered as APR in its tooltip. These are pool-wide displays, not returns to a new wide position.

|Pool|UI fee / base AQUA / base total|UI boosted AQUA / max total|Completed7day volume → ordinary LP fees → annual fee/current TVL¹|Day1 full boost for a full$10k LP:3year AQUA lock → initialICE; extra marked capital²|$10k wide base AQUA/week → current netUSDC sale quote³|Page annualization window|
|---|---:|---:|---:|---|---:|---|
|1 PYUSD/USDC CL|0.01396% /7.97230% /7.98625%|19.93075% /19.94471%|$1,404.60 →$0.70230 →0.01474%|1.51562mAQUA →15.15620mICE; $370.46|314.4875 →0.0765732|Fee backend window unverified; volume label24h|
|4 USDC/EURC CL|4.68243% /4.14357% /8.82600%|10.358925% /15.041355%|$66,652.24 →$99.97836 →5.48633%|176.58295mAQUA →1,765.82949mICE; $43,161.86|6,770.1435 →1.6484204|Same unresolved backend window|
|5 USDT0/USDC CL|0 /4.92068% /4.92068%|12.30170% /12.30170%|$0 →$0 →0%|38.56714mAQUA →385.67137mICE; $9,426.90|1,540.9379 →0.3751957|Same; no completed-week trades|
|6 XLM/yXLM CL|0.02564% /4.45119% /4.47683%|11.127975% /11.153615%|$2,812.10 →$1.40605 →0.02417%|2.22666mAQUA →22.26659mICE; $549.75|322.3424 →0.0795011|Same;24h calculation gives0.03017%, not display|

¹ CompletedNovember4–11 window. Pool6 has235swaps among291fully paginated events and86nonempty statistical buckets. Current TVL$303,377.68; current24h volume$501.54. Indexed realized pool volume times fee times50%LPshare estimates aggregate ordinary LP fees; finite-position historical fee allocation remains unmeasured. Normalization uses currentTVL, not a time-weighted historical denominator. Prior pool observations and partial authenticated fee-density evidence remain in report29. Do not label current-liquidity allocation a realized position backtest.

² This column answers the lock for a **full$10k LP plus additional lock capital**; it is not the denominator used below. Formula for newly created ICE includes its dilution of total supply: requiredICE=existingICE×newL/existingRawT. The3year lock starts10ICE/AQUA, falling to7afterone year. For annual rows below, the endpoint requirement is prefunded and lock capital is **inside$10k**. Future other holders' weights, ICE totals and range participation are frozen only as a conditional sensitivity. AQUA acquisition is valued at an indexed mark, not a proved executable purchase; acquisition slippage/fees remain missing. Personal ICE is not proven to boost vault-owned liquidity.

³ These unsigned AQUA/USDC estimates include the venue30bp swap fee and size-dependent impact, but not claim/sale transaction fees. They are **not realized sales by this position**. The annual table values earned AQUA using independently verified historical executed third-party sale-leg prices, as in report31. Pool6 uses the same534.4348539AQUA→0.1296981USDC observed leg (0.0002426827125USDC/AQUA); its current quote is higher, not silently substituted. Exact amount sale execution remains unperformed. No Mainnet sale is authorized here.

**All four captured reward schedules endNovember11,2026 at16:15:19UTC.** Pool6 rate1.7484101AQUA/s. Weekly/annual extrapolations require vote-dependent daily renewal; none is a funded year or guaranteed allocation. ICE boost and reward-zone voting are distinct. The exact fee lookback remains unavailable from the public backend/UI evidence; we report that gap rather than inventing a last-week formula. See [reward-cycle documentation](https://docs.aqua.network/voting-and-rewards/reward-cycle).

## Conditional annual net at$10,000 total capital

Same wide-range method as report29/31: current weights including entrant dilution, completed fee week repeated, current rewards renewed all year, constant leg/AQUA marks, range stays active, prior or current independent entry quotes, idle cash retained, one warm lifecycle cost deducted. Price-path/arbitrage loss, repeated keeper/restoration cost, weekly claim/sale fees and ICE acquisition costs remain unmeasured. These omissions are not zero-cost execution proofs.

|Pool|Case|LP / lock capital|Net vs holding first leg|Net vs holding second leg|
|---|---|---:|---:|---:|
|PYUSD/USDC|Fees only|$10,000 /$0|−$19.50|+$9.08|
|PYUSD/USDC|Fees + base AQUA|$10,000 /$0|−$15.54|+$13.06|
|PYUSD/USDC|Fees + hypothetical boost|$9,497.37 /$502.63|−$9.12|+$18.07|
|USDC/EURC|Fees only|$10,000 /$0|+$122.24|+$123.72|
|USDC/EURC|Fees + base AQUA|$10,000 /$0|+$207.50|+$209.00|
|USDC/EURC|Fees + hypothetical boost|$1,395.48 /$8,604.52|+$47.71|+$47.92|
|USDT0/USDC|Fees only|$10,000 /$0|−$7.27|−$8.86|
|USDT0/USDC|Fees + base AQUA|$10,000 /$0|+$12.10|+$10.51|
|USDT0/USDC|Fees + hypothetical boost|$4,261.30 /$5,738.70|+$17.48|+$16.80|
|XLM/yXLM|Fees only|$10,000 /$0|−$5.22|−$165.24*|
|XLM/yXLM|Fees + base AQUA|$10,000 /$0|−$1.15|−$161.16*|
|XLM/yXLM|Fees + hypothetical boost|$9,271.83 /$728.17|+$4.59|−$155.42*|

*The yXLM holding benchmark conditionally includes the issuer's advertised1.6%APY, $160/year on$10k if actually paid throughout the year. No issuer coupon is credited to the CL vault without entitlement/receipt evidence. Excluding issuer distributions on **both** sides, the yXLM-funded values are−$5.24/−$1.16/+$4.58. This separation avoids comparing a nonpaying CL position against an artificially yield-free yXLM holding. It does not certify future coupon payments or issuer backing. Every row remains below the requested$307/year Blend3.07% comparison, which itself carries changing-rate/credit risk and excludes its own transaction costs.

Pool6 wide range[-2000,2020], capturedtick8, liquidity3,544,730,546,961, quoted33,909.2603492XLM+33,873.2975707yXLM. Independent directional entry quotes retain both leg benchmarks and limit minted liquidity to the available token mixture; this is not sequential execution or safe-cap proof. The annual boost sensitivity locks2,949,316.269954AQUA for3years:29,493,162.70ICE initially,20,645,213.89 afterone year, leaving$9,271.83LP capital. Its earned746.8763607AQUA/week currently quotes0.1842063USDC, before transaction fees. Acquisition slippage could require more capital or lessLP; eligibility for a contract-owned vault remains unproven.

Pool6's tiny wide fee result follows its share of active liquidity (about0.0361% for$10k), not its fraction of poolTVL (~3.3%). Existing concentrated positions dominate liquidity weights. Its gross wide fee sensitivity is only about$0.000508/week. Thus the UI4.48% total does not imply$448/year for this wide entrant.

Warm fees: PYUSD0.5642323XLM andEURC0.5642217 from the newly signed current-code roots; USDT0 andXLM/yXLM use the preserved0.5744714XLM comparison anchor. **XLM/yXLM has no signed-row measurement**, so this is not its transaction-fee qualification. Prior pair measurements and old values remain preserved.

## yXLM coupon, redemption and primary-price gap

The current issuer page advertises1.6%APY with daily additional tokens, not an exchange-rate-accreting/rebasing token. It promises1yXLM redeemable for1XLM. Do not count both a rising redemption exchange rate and a daily token coupon. No captured evidence demonstrates that the Aquarius Soroban pool, its LP position or our vault receives/attributes this coupon. A half-yXLM mixture might notionally earn$80/year at$10k **only if** the relevant entitlement and payment path are proved; it is not included above.

Public issuer forms/fee endpoints show free1:1deposit,20–500,000XLM; withdrawal20–100,000yXLM with fixed0.5yXLM fee. These forms accept G-address destinations and are not evidence of a permissionless Soroban redemption ABI. The redemption promise is not audited backing, immediate contract liquidity or an independent oracle. Captured pool price is0.999197431969XLM/yXLM (−8.02568bp versus nominalpar). That apparent alignment alone cannot qualify valuation.

Existing primary `CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN` returns XLM/USD0.14848816077125, but `Other(yXLM)` isNone. Issuer-specific Stellar DEX primary `CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M` also returnsNone for the exactyXLM SAC. **Independent yXLM valuation remains missing.** Do not hard-peg it toXLM or use the same pool as its own independent oracle. Under the requested guard, fresh priced risk is unqualified; permitted in-kind exit/recovery remains the intended fallback. A guard limits new exposure; it does not repay an existing issuer/depeg loss. No secondary-oracle integration was started.

## Recommendation and resource scope

The sixth pool **does not displace PYUSD/USDC as the first technical pilot candidate**. It has XLM/USD exposure, unproved issuer-coupon attribution, no supported yXLM primary price, negligible wide-position fee income and no native-XLM signed fixture. EURC remains the second candidate conditional on its unresolved~71.64bp primary basis and accepted FX exposure; USDT0 remains deferred. This is not a realized-profit ranking.

Pool6 shares current Aquarius WASM155a17… with the two measured candidates, but **the signed table does not automatically transfer**. NativeXLM uses source/account balances and minimum-balance semantics rather than only credit trustlines; keeper/owner fee debits must be separated from token0 flows and refund accounting. A chosen native pair would require all nine signed roots, actual primary/guard bindings and cold restoration against that state. No new harness or financial experiment was launched for pool6. The completed18 private applications for PYUSD/EURC remain unchanged; final real-primary/cap/live-state/cold qualification remains missing.

Latest operator plan remains1,000USDC-equivalent aggregate cap,target500perleg, unrestricted on-chain deposits with frontend password, borrowing disabled. The password does not enforce team-only access; G1–G4 remain unresolved and cannot gate public access without an on-chain restriction. Keeper550XLM is a **planning buffer**, pending new-code cold repricing, separate from deposit capital and loss reserves. Pair selection still awaits the operator. No Mainnet signing/broadcast/deployment, secrets, Upshift or handoff00 edits. After pilot live only: narrow risk/recovery ABI then2× vault-share margin and conventional V3-close take-profit/common registry; current activation/recovery limits remain unresolved.

Reproduce: `python3 research/cl-apy-decomposition-20261111/xlm-yxlm/apy-boost/boost-math.py`; `python3 research/cl-apy-decomposition-20261111/xlm-yxlm/lead/calculate.py`; main research verifier checks file identities and preserved private application evidence. Source captures, failed request, getters, exact quote XDRs and issuer terms are retained under this directory. No UI number is promoted to a realized return.

Supporting detail: [issuer/oracle/weekly-sale evidence](../../peridot-contracts/research/cl-apy-decomposition-20261111/xlm-yxlm/yield-oracle/report.md), [UI/ICE proof](../../peridot-contracts/research/cl-apy-decomposition-20261111/xlm-yxlm/apy-boost/report.md), [preserved current-code signed table](../../peridot-contracts/research/cl-apy-decomposition-20261111/lead/signed-table.md).
