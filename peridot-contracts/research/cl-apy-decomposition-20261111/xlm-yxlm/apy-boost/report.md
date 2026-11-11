# XLM/yXLM concentrated pool: APY and boost inputs

Public captures: 2026-11-11 10:50 UTC, getter ledger65421638; zero-auth reward views ledger65421641. No build, signing, submission or state mutation. Prior accepted three-pool evidence remains unchanged.

## Identity

The fresh Aquarius UI pool listing returned all378listed pools in one page. Exactly one listed pool matches nativeXLM/yXLM and type`concentrated`:

- Pool: `CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F`.
- XLM: native asset (noissuer), SAC`CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA`.
- yXLM: issuer`GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55`, SAC`CBZVSNVB55ANF24QVJL2K5QCLOAB6XITGTGXYEAF6NPTXYKEJUYQOHFC`.
- Both tokens7decimals; fee10bp; tick spacing20; current tick8. No other CL tier appears in this listing. This establishes the listed canonical candidate, not an exhaustive reconstruction of every unindexed on-chain deployment.
- A different XLM/yXLM **stable-swap** pool exists: `CCFGZJTHQZGDZP5PK6WMLKHKJ72ACSVMJGCI2NFR7Q6EAVSKWLJB3ZH3`, also10bp. Its APY/liquidity must not be substituted for this CL pool.
- `get_tokens` independently agrees with the API pair. `get_fee_fraction=10`, `get_protocol_fee_fraction=5000`, hence50%of ordinary swap fees belongs toLPs.

## UI decomposition

| Component | Captured APR |
|---|---:|
| Swap fees (`apy`) |0.025640%|
| Base AQUA, no ICE (`rewards_apy`) |4.451190%|
| Base total |4.476830%|
| Maximum ICE-boosted AQUA |11.127975%|
| Maximum total |11.153615%|
| Additional pool incentives |0|

The same captured deployed UI component as the accepted decomposition, `../../apy-boost/live-chunk-2950.js`, computes **fee +2.5×reward**, not2.5×fees. Its tooltip calls this APR; API field names useAPY. The max panel is hypothetical unless the LPowner has enoughICE. No personalteamICE automatically boosts vault-owned positions.

API TVL is$303,377.676172; UI24hourvolume$501.539137. With the measured50%LPshare, ordinary24hourfees×365/currentTVL produces0.030170609%, not the displayed0.02564%. **Backend fee annualization window remains unverified.** UI24hour volume label does not establish its fee lookback. Lead separately reconstructs completed-week volume and position-weight scenarios. API rates are pool-level fields, not realized income for$10k of new wide liquidity.

## Current reward and lock inputs

| Input | Raw amount / interpretation |
|---|---|
| Raw total liquidity T |9,810,831,244,853,116|
| Active liquidity |9,810,831,244,853,116|
| Existing checkpointed working liquidity W |11,624,878,347,869,489|
| AQUA emission |17,484,101raw7/s =1.7484101AQUA/s|
| Reward expiry |1794413719 =2026-11-11 16:15:19UTC|
| Global ICE denominator I |616,277,490,709,458,557raw7 =61,627,749,070.9458557ICE|
| Boost token |`CARCKZ66U4AI2545NS4RAF47QVEXG3PRRCDA52H4Q3FDRAGSMP4BRU3W` (classicICE SAC)|
| Boost total-supply feed |`CBKCROE56TU2FTT3C5CVN676PYVLTOQUQDHHH57GLWDY5VOKSCZPGOFN`|

`reward-views.json` confirms allgetter auth counts0. Three existing holders' new working weights match `min(L+1.5*T*ownICE/I,2.5*L)` within1–2rawrounding units; all their recorded ranges contain currenttick8. `formula-crosschecks.json` preserves exact inputs and differences. T is raw total, W is the emission denominator, active liquidity is the fee denominator; they must remain separate.

For an entrant's new liquidityL, base weeklyAQUA is`1.7484101*604800*L/(W+L)`. Maximum is`1.7484101*604800*2.5L/(W+2.5L)`. For newly mintedICE x entering its own denominator, `x/(I+x)>=L/(T+L)` gives`x=I*L/T`. If a$10k wide quote givesL10k, allocate totalcapital betweenLP sleeveP andAQUAlock, using`L=L10k*P/10000`; solve`P+price_AQUA*(I*L/T)/m=10000`. m10is day-one3yearlock; m7prefunds enoughAQUA to survive firstyear's10→7ICE/AQUA melt, freezingotherICE/T/W. It does not guarantee maxboost if these denominators or the reward schedule change. The lead's fresh $10,000 wide quote has L=3,544,730,546,961 for range [-2000,2020], at ledger65421656. `boost-math.py/json` records the following capital-inclusive calculation.

A3yearlock's remaining ICE falls daily; future renewal/topup costs cannot be omitted. The accepted official locking/delegation documents establish that voting delegation doesnot transferICE or LPboost. A supported route provisioningICE to this product's vaultcontract remains **unproven**, just as for the other candidatepools. Boosted output must remain an eligible-account hypothetical, with lockedcapital counted and acquisition slippage unmeasured.

The current epoch doesnot fund a week/year. Pairvotes affect eligibility/allocation; allocations are refreshed daily and shared across relatedpools/SDEX. ICE ownership alone doesnot secure future emissions. No futureincome counts as day-one backing.

## Code identity and evidence scope

Current Aquarius instance WASM SHA256`155a17b9929ffb1f9e84bd6ef5c00a4d613c1ab5f4ad4c502d84515250cc2907`, same bytes as the freshly measuredPYUSD/EURC candidates, different from accepted old`12fca5...`fixture. `ConfigStorage=CAUJNVN5DWOBCKLV5KPR3KRQBKRI7L2X7VOUAWF5FDISVSQNXCEYITPJ`; rewardvault`CACKGTHQN3CENUMSSU7ASAAASE4L3W5MQTEFUHMN3TDAFYCSF45S7E5U`. IdenticalpoolWASM doesnot qualify nativeSAC/creditSAC bindings, tickpath, chosen size, dependencies, oracle or complete nine-row costs for this pair. No XLM/yXLM signedtable is claimed.

AQUA yields above exclude any issuer-paid yXLM interest. Whether such payments accrue to the pool/vault, get allocated to LPs, or bypass the strategy is separate evidence; do not add advertisedyXLM holding yield automatically. Pair also has commonXLM/USD market exposure, so its dollarreturn mustnot be described as aUSDstable-pair return. Lead and yield/oracle subtask cover these questions.

Raw API files, RPCXDRs, ledgers, sourceURLs/timestamps/hashes and scripts are inthisfolder. `getters.cjs` and`reward-views.cjs` use public unsigned simulations only; the sourceaddress is the publicAQUAissuer, with no keyloaded. The getter captures instance hash; no deployment performed.

## Numeric wide-range lock calculation

Fresh normalized AQUA acquisition mark is `0.001672833639748945 / 6.775538940000069 = $0.00024689307441998525`; this is not a purchase quote.

| Case | LP capital | Locked AQUA capital | AQUA locked | Weekly AQUA reward |
|---|---:|---:|---:|---:|
| Base, no ICE |$10,000|0|0|322.3424426241|
| Full $10k LP, day-one maximum, lock extra |$10,000|$549.7467058|2,226,659.0793398|not used as the $10k-total-capital result|
| $10k total, day-one minimum 3-year lock |$9,478.9005640|$521.0994360|2,110,628.0003094|763.5441817201|
| $10k total, 3-year lock prefunded for one-year melt |$9,271.8342387|$728.1657613|2,949,316.2699539|746.8763607312|

The annual-prefunded case initially receives29,493,162.6995395ICE, falling to20,645,213.8896776 after one year. This meets the max-boost threshold only under the frozen-denominator assumptions. All rows precede the lead's entry-conversion haircut, measured lifecycle anchor, weekly collection/sale costs, acquisition slippage and price-path effects. The full $10k LP extra-lock row requires22,266,590.7933981ICE. Comparing the boosted yield with only the LP sleeve in the denominator would be incorrect. No issuer-paid yXLM yield is included here.
