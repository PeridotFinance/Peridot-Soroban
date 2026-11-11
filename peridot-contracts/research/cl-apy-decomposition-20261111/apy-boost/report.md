# APY and ICE decomposition — bounded public read-only capture

Captured 2026-11-11 10:13–10:18 UTC. No transaction signed, submitted or executed. Public getter simulations required zero authorization entries. Old failed URL/DNS attempts retained. This report does not establish a realized position return or a year of funded rewards.

## UI decomposition (verified deployed UI source)

`live-chunk-2950.js` renders API `apy` as **Swap Fees**, `rewards_apy` as **AQUA Rewards (Base)** and `incentive_apy` as pool incentives. Unconnected boosted display computes `apy + 2.5*(rewards_apy + incentive_apy)`; connected accounts use the returned user boost instead of2.5. Its detailed tooltip calls the totals APR even though API fields are named APY. Fees are not multiplied by ICE. Fresh API rows:

| Pool | Swap fee display | Base AQUA display | Max boosted AQUA | Base total | Max total | Extra incentives |
|---|---:|---:|---:|---:|---:|---:|
| PYUSD/USDC CL |0.013960%|7.972300%|19.930750%|7.986250%|19.944710%|0|
| USDC/EURC CL |4.682430%|4.143570%|10.358925%|8.826000%|15.041355%|0|
| USDT0/USDC CL |0|4.920680%|12.301700%|4.920680%|12.301700%|0|

API rounds components independently, hencePYUSD total differs0.000010 percentage point from component sum. ~8% displayed PYUSD/USDC is **not8% fees**. These numbers are pool-wide API fields; they are not the return to a new$10k wide position. Concentration and existing boosted working weights explain the huge difference.

**Fee annualization window remains unverified.** The UI labels volume24H, but imports `apy` from the backend and does not expose its lookback/window/formula in the captured pool components. Fresh volume24H×fee×50%LPshare×365/currentTVL gives0.00930569%PYUSD and3.68812517%EURC, not displayed0.01396%/4.68243%. The prior completed7day window also does not reproduce it exactly. Do not claim the UI annualizes lastweek; do not infer a position return from it. APIrewardAPY is consistent with annual current emission value/currentTVL, but exactbackendmark/timestamp/rounding is not certified. The source-backed statement is decomposition; the unknown window remains explicit.

## Live boost denominator and formula

Pool instances bind RewardBoostToken `CARCKZ66U4AI2545NS4RAF47QVEXG3PRRCDA52H4Q3FDRAGSMP4BRU3W` to classic `ICE:GAXSGZ2JM3LNWOO4WRGADISNMWO4HQLG4QBGUZRKH5ZHL3EQBGX73ICE`. RewardBoostFeed `CBKCROE56TU2FTT3C5CVN676PYVLTOQUQDHHH57GLWDY5VOKSCZPGOFN` implements `total_supply`; WASM SHA256`3f605ed5a9e5e0ed3aaeb879d46376a9bcbba9a8e4023eb33e00022b2181ac40`. Ledger65421224 instance and subsequent zero-auth getter agree:616277490709458557raw7 = **61,627,749,070.9458557ICE**. This is not the voting total, nor the50b example in docs.

Published boost formula is equivalent to working weight`min(L + 1.5*T*ownICE/totalICE,2.5*L)` for in-range liquidity, with noICEweightL. **T is raw total shares/liquidity, not current active liquidity or working supply.** Live `get_rewards_info` returns `supply`, `boost_supply`, `boost_balance`, `new_working_balance`, `working_supply`. Public existing-holder crosschecks in`formula-crosschecks.json` reproduce the expression within0–2rawunits for8/9holders (integer/per-range rounding). The ninth holder has two ranges, one outofrange, so using all rawL as in-range is intentionally wrong and recorded as such. This is a public-getter crosscheck of published mathematics, not a recompiled-source proof. GitHub main/master requests returned404; their files are failedfetches, not sources.

For a fresh entrant with new liquidityL and newly mintedICEx, baseline totalrawliquidityT, priortotalICEI:

`x/(I+x) >= L/(T+L)` implies `x*T >= I*L`, therefore **x=I*L/T** at fullboost. Omitting the ICE added to the denominator understates the lock. Emission share is `2.5L/(W+2.5L)`, not2.5times the new unboosted share; W is current existing checkpointed working liquidity. Future claims update other users' working weights (and prices/ranges may change), so fixedW is a conditional continuation only. Inputs and arithmetic are in`boost-math.py/json`.

## Lock capital and one-year melt

Official lock formula:`ICE=AQUAlocked*(1+9*min(daysuntilunlock/1095,1))`. A3yearlock starts10×, melts daily to7× after1year,4× after2years,1× atunlock. Lockmultiplier is distinct from2.5×rewardboost. OriginalAQUA remains locked; no assumption of weekly unlocking/selling it. A one-year maxboost continuation must either renew/topup (new capital/actions) or prefund sufficientAQUA to satisfy the endpoint7×requirement. Tables below use the latter for annual sensitivity, freezingotherICE/T/W/price and keeping the positioninrange. That is not guaranteed bycurrentepoch.

Using prior captureAQUA mark$0.00024442824, same acceptedwide quote L per$10k, and$10k total capitalincludinglock:

| Pool | Day1 minimum3yearlock AQUA | Day1 LP$ / lock$ | Annual-prefunded3yearlock AQUA | Annual LP$ / lock$ | Weekly AQUA atprefundedmaxboost |
|---|---:|---:|---:|---:|---:|
| PYUSD/USDC |1,461,477.934812|9,642.773521 /357.226479|2,056,343.604603|9,497.371552 /502.628448|746.4932590|
| USDC/EURC |33,216,097.178907|1,881.047827 /8,118.952173|35,202,632.224297|1,395.482562 /8,604.517438|2,396.1647205|
| USDT0/USDC |19,852,442.936430|5,147.502313 /4,852.497687|23,478,041.055869|4,261.303746 /5,738.696254|1,640.9439218|

Annualprefunded PYUSD startICE20,563,436.046034/end14,394,405.232224;EURC352,026,322.242973/end246,418,425.570081;USDT0234,780,410.558687/end164,346,287.391081. The final table excludes acquisition slippage/fees, lockoperationfees/keepercosts, opportunitycost, AQUApricechange and votingincentiveincome. Sale agent separately quotes earnedAQUA; acquisitionquote is an additionalconditional check. A lock does not destroy capital, but it is unavailable and exposedtoAQUApricerisk, so must remain in denominator. Comparing returnonlyonLPcash would inflate yield.

If instead retaining a full$10kLP position, day1required3yearlock is1,515,619.890575AQUA($370.460302),176,582,948.631396($43,161.859348),38,567,137.473547($9,426.897534), respectively; totalcapital exceeds$10k. These are not the budgetinclusive rows.

**Vault boost eligibility is unproven.** The classic locker grants nontransferableICE to the locking wallet via a claimablebalance process. Docs' delegation grants dICE/gdICE for voting; it does not transferICE ownership or LPboost. The LPowner for the product is a contractaddress. No captured supported distributor/locker path mintsICE to thatvault, and no delegatebeneficiary mapping was found in feedABI (only globaltotalsupply and administration). PersonalteamICE must not be assumed to boost vaultowned liquidity. Therefore boostedproductrows are a hypothetical eligible-account sensitivity, not a launchyield. BaseAQUA doesnotrequireICE.

## Emission horizon and votes

All three fresh public `get_rewards_info` views return expiry1794413719 = **2026-11-11 16:15:19UTC**; tps2.5651983/0.5097735/0.4155747AQUA/s. No renewed schedule/week/year was evidenced. Weekly and annual continuation assumes renewalatcurrentrate and price; actual fundedhorizon is shorter. Pairvoting affects rewardzone/allocation, then market allocation is shared across relatedpools andSDEX. Existing docs say rewards resyncdaily, whitelisting and0.5%voteeligibility apply,10%marketcap. A personalICE lock doesnot guaranteefuturepoolvotes/allocation. Delegatevotingincentives are separate income and not included.

## Evidence and reproduction

Run`python3 research/cl-apy-decomposition-20261111/apy-boost/boost-math.py` for deterministic arithmetic. `reward-views.cjs` runs onlypublic zero-authgetter simulations, usingpublicAQUAissuer as source and no key. `boost-getters.cjs` contains an initial spec-parsererror retained in`boost-contracts.json`; successful offlineSDK`Spec.fromWasm` output is`boost-feed-spec.json`. No codeexecutionqualification inferred. `sources.json` recordsURL,time,status,hash; `manifest.json` hashesallownedfiles exceptitself. Old404/DNS errors retained, neverlabelledsuccessfulsources.
