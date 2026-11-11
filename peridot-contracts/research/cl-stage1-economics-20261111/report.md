# Stage 1 economics checkpoint — 11 November 2026

**Report before package 2: the simple full-range and wide-range candidates are below current lending supply rates.** Their conditional gross run rates are approximately **0.00381% and 0.04004% annually**, respectively, before entry/exit costs. Current public ledger data gives **3.0666% supply APR for Blend Fixed v2.1 USDC**. This is a comparison, not a recommendation to deposit in Blend. The Aquarius pool's displayed roughly 8% reward-plus-fee figure cannot be applied to a new wide/full-range entrant: existing liquidity is concentrated much more narrowly.

This is one bounded read-only economics study, not a backtest, strategy implementation, deployment, resource rerun, or proof of realized returns. Neither $1,000 nor $10,000 is an approved exposure cap. No signing, submission, account mutation, secret access or legacy Peridot operation occurred.

## Evidence and identities

Public observations were collected on 2026-11-11 around 08:05–08:20 UTC. `sources.json` records exact HTTP URLs, timestamps, response hashes and status, including failures. `chain-read.json`, `chain-read-initial.json` and `marked-quotes.json` retain ledger numbers, raw return/ledger-entry XDR and decoded observations. These are rolling observations, not an atomic historical snapshot.

- Pool: `CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX`.
- PYUSD: issuer `GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5`; SAC `CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2`.
- USDC: issuer `GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN`; SAC `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75`.

Identity checks agree across issuer documentation ([Paxos issuer](https://docs.paxos.com/api-reference/endpoints/crypto-withdrawals/create-crypto-withdrawal), [Paxos SAC](https://docs.paxos.com/guides/stablecoin/pyusd/mainnet), [Circle issuer](https://developers.circle.com/stablecoins/usdc-contract-addresses)), deterministic SDK asset-contract derivation for the public network, Aquarius's API and live `get_tokens` results. Duplicate ticker records in the API were excluded by exact issuer/address.

At ledger 65419687 the current pool WASM was `155a17b9929ffb1f9e84bd6ef5c00a4d613c1ab5f4ad4c502d84515250cc2907`, API version 220. This differs from the historical compiled fixture hash `12fca5a7…`. **Current economics is not execution qualification of this newer bytecode.** Accepted resource evidence remains preserved with its original code identity.

## Current pool and range weights

| Observation | Captured value |
|---|---:|
| Pool TVL, indexed USD | $245,871.09 |
| Last-24h volume, indexed USD | $125.37 |
| Pool swap fee / LP share | 0.10% / 50% |
| Derived entire-pool LP fee income at that volume | $0.062685/day |
| AQUA emissions | 2.5651983 AQUA/second |
| AQUA indexed price, assuming USDC=$1 | $0.0002423937 |
| Entire-pool emissions at that mark | $53.72248/day |
| XLM indexed price, same assumption | $0.14472467 |
| PYUSD indexed mark, same assumption | $0.9995638173 |
| Current tick / raw active liquidity | -2 / 2,091,675,066,835,145 |
| Current active boosted reward working supply | 2,575,853,465,725,930 |
| Raw total liquidity (not TVL or active reward denominator) | 2,123,566,702,904,480 |
| Current reward schedule expiration | **2026-11-11 16:15:19 UTC** |

The schedule had about 8.1 hours remaining at the captured ledger. No 30-day or annual emission renewal is established. API `rewards_apy=0.0798324` and `apy=0.0001396` are pool-level indicators, not our expected return. Fee-only API annualization also differs from the last-24h calculation above; the model uses the observed volume and verified on-chain fee split, without silently equating those measures. Seven-day indexed volume averages about $194.29/day depending on the rolling timestamp; this does not rescue the comparison because modeled fee income is tiny relative to emissions. API currency values use 10^7 scaling, checked against token reserves and prices.

Two configurations are compared: **full range [-887260,887260]**, the tick-spacing-compatible bounds; and **simple wide [-2000,2000]**, approximately 0.81874–1.22139 USDC per PYUSD. The latter is the existing test example, not an approved production range. No narrower strategy or ICE purchase is proposed.

The unsigned `estimate_deposit_position` calls produce actual venue liquidity estimates. Conditional entrant reward share is `L/(active_boosted_working_supply+L)` with no ICE boost; conditional fee share is `L/(raw_active_liquidity+L)` while trades stay in the present active band. Both denominators include our added liquidity. The documented boost/in-range mechanism is assumed for these quotes; newly deposited current-bytecode reward settlement is not executed. See [Aquarius CL model](https://docs.aqua.network/developers/concentrated-liquidity) and [ICE weighting](https://docs.aqua.network/aqua-and-ice/ice-boosts-how-to-maximize-lp-rewards).

| Capital | Range | Entrant reward weight, after dilution | AQUA/year, USD mark | LP fees/year, USD | Conditional gross APR |
|---|---|---:|---:|---:|---:|
| $1,000 | Full | 0.000193985% | $0.038038 | $0.000055 | 0.003809% |
| $1,000 | Wide | 0.002039088% | $0.399839 | $0.000575 | 0.040041% |
| $10,000 | Full | 0.001939777% | $0.380365 | $0.000547 | 0.003809% |
| $10,000 | Wide | 0.020387138% | $3.997654 | $0.005744 | 0.040034% |

Those annualized figures hold current prices, competition, volume, reward schedule renewals and availability fixed. They are **not earned or guaranteed yield**. They do not divide our capital by pool TVL. Even an unearned maximum 2.5× boost sensitivity would leave wide-range gross income roughly 0.10%, far below the observed lending supply rate; purchasing/locking ICE would itself need separate capital and cost accounting.

## Against holding PYUSD, after warm costs

Benchmark: buy and hold the initial quantity of self-custodied Stellar PYUSD, with unchanged valuation over the stated horizon. No native yield is verified for that holding. PayPal's rewards require eligible PYUSD held at PayPal; they are not a coupon attached to PYUSD in this vault. [PayPal eligibility](https://www.paypal.com/us/cshelp/article/what-are-pyusd-rewards-help1247).

The primary calculation uses the **same current indexed PYUSD/USD mark on both sides** and USDC=$1. $1,000/$10,000 is initial marked capital; half is converted at independent current unsigned Aquarius quotes. Estimated entry shortfalls are $0.35944/$3.70182. Equal-value desired deposits retain unused cash; unspent amounts are not treated as losses. Conversion estimates and deposit estimates are independently evaluated against the current prestate: they are **not** a sequential post-conversion execution proof. Future terminal conversion is another independent quote and therefore a cost scenario, not an exit guarantee.

Warm costs use the accepted continuous **unsigned** journeys in report 25: **0.5744714 XLM** in kind, **0.5394184 XLM** converted, approximately **$0.08314/$0.07807** at the captured mark. These include the original measured resource/rent costs; they are not refreshed current-network signed fees. Report 26's independently imported signed prestates are not summed into a lifecycle. No cold restoration probe is added to a normal warm lifecycle.

| Capital | Range | 30-day net, in kind | 365-day net, in kind | 30-day net, converted | 365-day net, converted |
|---|---|---:|---:|---:|---:|
| $1,000 | Full | -$0.43945 | -$0.40449 | -$1.07682 | -$1.04186 |
| $1,000 | Wide | -$0.40967 | -$0.04217 | -$1.04704 | -$0.67954 |
| $10,000 | Full | -$3.75366 | -$3.40405 | -$10.28004 | -$9.93044 |
| $10,000 | Wide | -$3.45592 | **+$0.21843** | -$9.98230 | -$6.30795 |

These are single-contribution modeled horizon P&L differences, not historical TWR/IRR. The $10,000 wide annual surplus is only **0.002184%** before continuing operating costs. It cannot establish an economic floor. At an alternative strict $1/$1 parity mark, the same broad calculation is negative for all four configurations: annual in-kind results -$0.62240/-$0.26028 for $1,000 and -$5.58307/-$1.96256 for $10,000 full/wide. This mark sensitivity is why a tiny apparent positive result cannot be presented as profitability.

Missing recurring costs are not silently declared zero: operator hosting, RPC/monitoring allocation, periodic reward realization, reward-sale slippage, proactive TTL funding, outages, restoration and recovery are **additional unmeasured costs**. The tables set them to zero as an optimistic sensitivity. They also set strategy price-path loss to zero, assume no fee charged by Peridot and no idle delay before investment. Cash-first idle contributions cannot be advertised as generating external pool revenue before investment; any common-vault entitlement during that interval is an internal allocation, not new yield.

If AQUA is unclaimed, quarantined or lacks enforceable distribution, credited AQUA value is **zero**. Under that rule the 365-day in-kind net is -$0.44253/-$0.44201 for $1,000 full/wide and -$3.78442/-$3.77922 for $10,000. There is no unconditional loss floor: underlying depegs and execution outages can make results much worse. Nor is the full annual AQUA schedule funded by the remaining eight hours of the observed emission epoch.

## Lending comparison and reserve scenario

USDC rates below are calculated offline from live `ResConfig`, `ResData` and instance `Config` at ledger 65419712 using the official [Blend Reserve.setRates formula](https://github.com/blend-capital/blend-sdk-js/blob/main/src/pool/reserve.ts). Exact integer rounding and source responses are retained. These are marginal snapshot supply rates before our added deposit, not borrow rates, incentive APRs, backstop yields or promises of realized returns.

| External market | Current status | Supply APR | SDK estimated supply APY | Annual interest per $1,000 / $10,000 if rate stayed fixed |
|---|---|---:|---:|---:|
| Blend Fixed v2.1 USDC `CBAK…QWAO` | 0, admin active; reserve enabled | 3.06660% | 3.11317% | $30.666 / $306.660 |
| Blend Fixed USDC `CAJJ…BXBD` | 3, on ice; reserve enabled | 5.34069% | 5.48299% | $53.4069 / $534.069 |

Fixed v2.1 is the more relevant active comparison. The on-ice legacy external Blend pool is separately labeled; on-ice is not evidence of operational safety. Neither pool's withdraw capacity, bad debt, backstop quality, new-size utilization effects or future interest has been qualified here. No lending balance is used or deposited. All compromised legacy **Peridot** markets remain excluded. Lending additionally changes PYUSD exposure to USDC and has its own conversion, fees and credit risk. Nevertheless, even before any such adjustments, the tested Aquarius full/wide run rates are roughly 80–800 times below the active supply APR. No claim of CL outperformance is supported.

The unapproved **50% collected-income retention** scenario makes the annual in-kind outcomes -$0.42353/-$0.24237 for $1,000 full/wide and -$3.59451/-$1.78327 for $10,000. It would replenish the reserve by only about $0.019/$0.200 or $0.190/$2.002 per year at these conditional run rates. Future income, unclaimed AQUA and quarantine establish **zero day-one reserve backing**. Dedicated seed reserve, supplier withdrawal cash, operating XLM and liquidator capital must remain separate. No reserve size or loss guarantee is approved by this study.

## Strategy loss and capacity limitations

An analytic price-path illustration makes the benchmark explicit. Starting from parity with a $1,000 balanced investment, a 1% **USDC** depeg while PYUSD stays at $1 produces approximately -$5.01256 full-range or -$5.13202 wide-range relative to holding PYUSD, before revenue and costs. The initial token-mixture loss is $5; additional LP shortfall is $0.01256/$0.13202. For a 1% **PYUSD** depeg with USDC unchanged, the same LP dollar loss would instead outperform holding PYUSD by about $4.98744/$4.86798. These are elementary terminal-value examples, not execution stress tests, forecasts, measured LVR or proof of the vault's exit capacity. No second IL/LVR loss is subtracted from the portfolio result.

Fee allocation changes as ticks are crossed. Current weights, static prices, independent quotes and API aggregates do not prove a path through stressed concurrent exits or liquidity removal. No live strategy supplier-return history exists for the new implementation. New current-code rewards/fee attribution, continuous conversion→deposit→exit marks, collected reward distribution, stressed history/outage recovery and operating cost remain necessary before a profitable-capacity assertion.

**Handoff 00 section 2 disposition:** “This strategy has not established a positive profitable capacity: at research sizes of $1,000 and $10,000, the full/wide 1× configurations model roughly -0.04045% to +0.00218% annual net versus holding PYUSD for in-kind exit under constant current marks/emissions/weights and zero additional OPEX; at parity marks all are negative, and stressed executable capacity and enforceable realized rewards remain UNPROVEN. No safe or production cap follows.”

Recommendation for the lead: report the lending underperformance before package 2. Preserve the accounting/ABI work, but do not describe the currently simple range as an economically qualified deployment. A narrower strategy, higher genuine fee flow or lower costs would be a separate product/research decision; none is proposed for implementation here.

## Reproduction

From `peridot-contracts`:

```sh
python3 research/cl-stage1-economics-20261111/calculate.py
python3 research/cl-stage1-economics-20261111/verify.py
```

The first command recalculates all numbers from captured responses. The second verifies retained response hashes, identity evidence and evidence-file inventory. Optional fresh read-only sampling (changes observations, not a deterministic replay): `node research/cl-stage1-economics-20261111/read-chain.cjs` then `node research/cl-stage1-economics-20261111/read-marked-quotes.cjs`. Neither script can sign or submit; they use a public issuer account solely as a getter-simulation source. `calculate.py` does not invoke a network. A fresh sampling should be stored as a new checkpoint, not substituted for these pinned observations.
