# AQUA sale valuation and EURC basis — 2026-11-11

Scope: public read-only captures using the Stellar data skill. No sale was signed or submitted. **An unsigned estimate is not a realized sale price.** Weekly future proceeds cannot be called realized: reward amounts, market prices, depth, claim costs and routing costs can change. The current execution estimates below are suitable only for explicitly conditional continuation scenarios. `calculate.py` reproduces arithmetic offline; `sources.json` records HTTP URL/time/status/hash, `reads.json` records RPC/ledger/arguments/return XDR, and `manifest.json` pins every file. Three initial DNS failures are retained as empty response files with original error metadata.

## Executable venue estimate

Direct Aquarius constant-product AQUA/USDC pool: `CA6GAFOJCW4MGQQBUCQUSA3CLIH25G4SNKB2JHYKZCVWZTNW5VXMSC4O`. AQUA issuer `GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA`; Circle USDC issuer `GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN`. Read-only getters confirm reserve balances 851,157,161.6455658 AQUA / 207,868.2508388 USDC. Reserve spot is 0.00024421841253961 USDC/AQUA. Fee is 30/10,000 = 0.30%; protocol fee fraction 5,000/10,000 partitions that fee, not an extra user fee. `estimate_swap(0,1,input)` exact results match integer input-fee constant-product math from these reserves for all nine initial probes.

Captures 2026-11-11 10:13:49–10:14 UTC, ledgers 65421197–65421201. Rolling readings, not one atomic state snapshot. Inputs are the accepted prior $10,000-wide current-weight scenarios, rounded down to 7 decimals. They are neither historic position earnings nor a perpetual funded schedule.

| LP-only case | Weekly AQUA input | Net quoted USDC | Effective USDC/AQUA | Discount from reserve spot, including fee | Annual continuation USDC, 365/7 |
|---|---:|---:|---:|---:|---:|
| PYUSD/USDC base |314.4875276|0.0765732|0.000243485650|30.0044 bp|3.9927454|
| PYUSD/USDC maximum boost, lock outside budget |785.9798327|0.1913747|0.000243485509|30.0101 bp|9.9788236|
| USDC/EURC base |6770.1434811|1.6484204|0.000243483820|30.0793 bp|85.9533494|
| USDC/EURC maximum boost, lock outside budget |16385.6447703|3.9895945|0.000243481081|30.1915 bp|208.0288561|
| USDT0/USDC base |1540.9379183|0.3751957|0.000243485280|30.0196 bp|19.5637758|
| USDT0/USDC maximum boost, lock outside budget |3817.2401176|0.9294394|0.000243484657|30.0451 bp|48.4636259|

The spot discount is dominated by the 30bp fee; worst measured additional impact/rounding here is ~0.1915bp. Gross pre-fee output, input fee and exact arithmetic are in `calculated.json`. None of these figures deducts claim/sale transaction fees or keeper costs. `estimate_swap` is a getter: its resource declaration is **not** a metered sale or claim root, and must not be used as the sale gas bill. The separate annual ICE-inclusive estimates are in `budget-reads.json`/`calculated.json`; parent calculations specify their lock amounts, dilution, melt, eligibility and capital assumptions. ICE purchase slippage is not measured here and must not be silently treated as zero.

For the boosted scenario with **ICE capital inside the $10,000 budget**, the separate reward agent solved the LP/lock budget using an indicative AQUA mark of $0.00024442824. Its annual sensitivity starts a three-year lock and prefunds enough ICE to remain at the modeled maximum after one year of melt (10→7 ICE/AQUA), conditional on frozen other ICE, pool weights and emission, and unproven vault ICE eligibility. Fresh direct-CP estimates for those amounts:

| Pool | LP budget at indicative mark | AQUA locked | Weekly AQUA sold | Net quoted USDC/week | Conditional annual USDC from rewards |
|---|---:|---:|---:|---:|---:|
| PYUSD/USDC |$9,497.37155|2,056,343.6046034|746.4932590|0.1817603|9.47750136|
| USDC/EURC |$1,395.48256|35,202,632.2242973|2396.1647205|0.5834303|30.42172279|
| USDT0/USDC |$4,261.30375|23,478,041.0558687|1640.9439218|0.3995457|20.83345436|

For the minimum lock that reaches maximum **only on day one**, weekly quote outputs are 0.1845416 / 0.7843161 / 0.4819822 USDC respectively. Annualizing those without accounting for ICE melt would overstate the maintained boost. These are illustrative lock-capital budgets, **not executable AQUA acquisition quotes**: large lock purchases could have material price impact and leave less capital for LP shares. No purchase-price study was run. Ongoing claim, sale, locking and operational fees are unmeasured and must be subtracted when known; reward-sale quote includes the swap fee only. Neither lock principal nor its return is strategy income, and AQUA price risk remains part of total-capital performance.

Classic Horizon strict-send estimates offer slightly better routes at nearby observation times. For base amounts the best captured outputs are 0.0768224 / 1.6537991 / 0.3764176 USDC via AQUA→XLM→PYUSD→USDC; simpler AQUA→XLM→USDC routes quote 0.0768030 / 1.6533221 / 0.3763223. Direct classic routes quote 0.0766691 / 1.6503280 / 0.3756591. These are alternative snapshots, not a synchronized best-execution guarantee. Multi-hop routes add different venue/asset execution dependencies. No extra explicit router fee appears in Horizon's returned amounts; transaction fee is excluded. Classical pathfinding does not include the Soroban Aquarius CP pool. Raw orderbook depth and paths are preserved; use full path outputs rather than assuming top-of-book applies to arbitrary size. Historical-orderbook capacities and future weekly execution remain unproven.

## Historical realized price references, not our sales

The latest 100 indexed CP swaps span Nov10 15:42:37–Nov11 09:54:07 UTC; 42 are AQUA→USDC. Closest available small/medium/large examples were independently cross-checked against successful Horizon operation asset transfers into/out of this exact pool. These are **venue legs of other users' routed transactions**, not standalone sale transactions or this vault's reward realization. Their transaction-level fees cannot be assigned entirely to this one leg.

| Time UTC | AQUA into CP | USDC out | Realized venue-leg price | Transaction |
|---|---:|---:|---:|---|
| Nov11 08:56:52 |534.4348539|0.1296981|0.0002426827125|`d056736c75967356e897cd24b6215bbff29676041d51eb5c656291d8b3ec81db`|
| Nov10 23:05:12 |2764.6825935|0.6675974|0.0002414734341|`c683e94201519e9c17de45ab6a2a3514fd7036e38fab2c502226ab66ed4da298`|
| Nov11 06:30:27 |26808.1604665|6.5124573|0.0002429281676|`d89eba2ec03aa5fc6c6086da152a3e79a6fe2e94b75d4446d71ef63ba1331474`|

The samples bracket some of the requested sizes but are not exact same-size sales. Future realized price and collection/sale overhead remain unavailable without executing the future operation. Annual results should therefore say “conditional, current executable quote,” not “realized annual yield.”

## EURC: the ~74bp gap persists

Pool `CDTSE6RLRI7ZO25JSER6E4SQR4PHJJNONEGS5HDJ3Y6LAKECRZKYN5CA` is token0 Circle USDC, token1 EURC SAC `CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV` (issuer `GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2`). `sqrt_price_x96=75336759151749414060382207238`, tick −1008, both7 decimals. Square ratio is **EURC per USDC**, so invert for USDC per EURC.

| Reading | Value / base | Identity |
|---|---:|---|
| Pool marginal EURC price |1.1059749744440877 USDC/EURC|actual CL slot0|
| External USDC |1.00083307783972 USD/USDC|`CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN`, `Other(USDC)`|
| External EURC |1.11482658760255 USD/EURC|same contract, `Other(EURC)`|
| Correct external cross |1.1138986233437477 USDC/EURC|external EURC divided by external USDC|
| Stellar DEX issuer-specific EURC |1.10599720911274 USDC/EURC|`CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M`, `Stellar(CDTKP...)`|
| Fiat EUR |1.11864410559291 USD/EUR|`CBKGPWGKSKZF52CFHMTRR23TBWTPMRDIYZ4O2P5VS65BMHYH4DXMCJZC`, `Other(EUR)`|

All oracle values14 decimals, resolution300 seconds, lastprice timestamp1794391800 (2026-11-11 10:10:00UTC); captures10:13–10:14UTC. `base()` is read directly: external and fiat base USD; Stellar DEX base Circle USDC. EURC is a euro claim, not a USD peg. Reflector public config labels dataSources `exchanges`, `pubnet`, `forex`; captured website describes respectively CEX/DEX aggregation, Stellar Classic DEX/liquidity pools, and central-bank/commercial FX sources. The exact active exchange list/weights and upstream FX provider list are **not disclosed by this captured config**. Do not invent venue names or claim the FX feed itself prices Stellar redemption basis. The config publishes oracle code hash `25fc306065cbb9b05461db78ec259802cb916053ba5b3ab911b0a72dd1a81439`; this is configuration evidence, not an independently downloaded on-chain WASM pin.

Pool-versus-primary deviation is **+71.6440162bp** in the previous EURC-per-USDC orientation, or **−71.1343809bp** in USDC-per-EURC orientation. It was ~74bp earlier; it is still above a proposed50bp deposit-pause threshold. Pool is only −0.2010373bp from the Stellar DEX feed, while that feed is −70.9347697bp from the external cross; external EURC is itself −34.1262960bp from fiat EUR. This is evidence of a local Stellar EURC basis, not a stale decimal conversion that fixes itself by using FX. Pool-implied checks against the same market's oracle also have correlated manipulation risk.

**Resolution needed before claiming the gap closed:** either observe genuine pool/local-market convergence inside an adopted fresh independent guard, or explicitly qualify and approve the correct issuer-specific primary-feed and valuation policy, including independently justified local basis, redemption/execution capacity, feed venue/concentration and manipulation analysis. Merely replacing the external feed with the nearly matching DEX quote, hard-pegging EURC to EUR, or widening a threshold until the current reading passes does not resolve the risk. No such policy change is made here. Under the existing external-primary comparison the gap remains an unresolved launch condition for EURC; this read-only study does not remove it.

Depeg/execution guards limit new exposure; they do not repair losses already incurred. Any extra oracle reads still need full signed-root resource measurement, with freshness, issuer pause and execution-price checks retained.

## Reproduction and limits

Offline: `python3 research/cl-apy-decomposition-20261111/sale-fx/calculate.py`; then `python3 research/cl-apy-decomposition-20261111/sale-fx/verify.py`. Public online recorder `read.cjs` refuses evidence overwrite; supply a new output filename. `fetch.py filename URL` archives public responses and hashes. No credentials or private account information are needed. Raw logs may require explicit Git add because `.log` files are ignored.

Primary URLs: [Aquarius sale-pool index](https://amm-api.aqua.network/pools/CA6GAFOJCW4MGQQBUCQUSA3CLIH25G4SNKB2JHYKZCVWZTNW5VXMSC4O/), [indexed swaps](https://amm-api.aqua.network/events/pool/CA6GAFOJCW4MGQQBUCQUSA3CLIH25G4SNKB2JHYKZCVWZTNW5VXMSC4O/?event_type=swap&size=100), [Reflector config](https://orchestrator.reflector.network/config), [Reflector docs](https://reflector.network/docs), [Stellar order-book schema](https://developers.stellar.org/docs/data/apis/horizon/api-reference/aggregations/order-books/object). Exact Horizon quote and historical-operation URLs are in `sources.json`.
