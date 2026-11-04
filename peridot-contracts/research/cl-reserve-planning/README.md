# Conditional Stellar reserve planning

This executable study is **not venue execution, a safe cap, a yield estimate or
insurance approval**. `coverage_status=UNPROVEN` and all-in seed is `null` in the
default result. It changes no contract/configuration. No signing, deployment,
Mainnet access or secondary-oracle integration is involved.

```sh
python3 research/cl-reserve-planning/study.py
python3 -m unittest discover -s research/cl-reserve-planning -p 'test_study.py' -v
```

`results.json` is deterministic, uses60-digit Decimal arithmetic and binds its
source hash. Loss floors and contingencies round **up** to a USDC cent. Optional
`--cost-usdc 100 --output /tmp/cl-reserve-cost-sensitivity.json` explicitly models
100 USDC of necessary total zap/unwind/recovery costs across all exits. That is
a user-chosen sensitivity, never an actual fee quote. Omitting the argument
means unknown cost, not zero. No Soroban root is executed, so this study provides
no new `check_budget.sh` flow or resource proof; existing failed gates remain.

## Model and provenance

Gross CL exposure `G=1000,5000,10000` is original USDC deposited at initial
USDCUSD=PYUSDUSD=1. It includes borrowed principal once. The deposit is assumed
to zap to50/50 pool assets at par; actual zap friction is unknown and belongs in
`C`, not silently forgiven. Coverage compares the LP with **holding the original
USDC**, not holding the post-zap basket. Basket impermanent loss is diagnostic
only. At2x, original owner equityG/2 and lender principalG/2 partition the same
gross benchmark. Principal collateral deficit before recovery is not another aggregate loss to add, nor realized lender bad debt after a successfully funded full repayment. That repayment pays the lender; negative collateral economics belong to the payer/liquidator unless covered.

Let `p=PYUSD_USD/USDC_USD`. The fee-free full-range model has
`x=G/(2 sqrt(p))`, `y=G sqrt(p)/2`, valued as `x*p+y` USDC. The hypothetical
fixed wide range uses `[a,b]=[0.5,2]`, `L=G/[2(1-sqrt(a))]`,
`z=clamp(sqrt(p),sqrt(a),sqrt(b))`, `x=L(1/z-1/sqrt(b))`, `y=L(z-sqrt(a))`.
These continuous AMM equations are grounded in the
[Uniswap v3 whitepaper, section2](https://app.uniswap.org/whitepaper-v3.pdf) and
[official LiquidityAmounts implementation](https://raw.githubusercontent.com/Uniswap/v3-periphery/main/contracts/libraries/LiquidityAmounts.sol).
They are **not** a demonstration of exact Aquarius integer ticks, rounding,
fee growth, arbitrage timing or actual exit capacity. No source code is copied.
The band is a transparent planning example, not a recommended production range.

Nine paths specify initial prices, an hour1 shock, and three exit-boundary prices:
PYUSD10/25/50/75% persistent declines;50% decline then recovery; sequentially
worsening PYUSD; USDC10% decline; both assets25% down; and PYUSD75%/USDC50% down.
Each is evaluated with24h/72h oracle/venue outages, starting at hour1 and
conditionally reopening at hour25/73. Both assume50% of third-party
liquidity removed at the shock. No TVL, tick depth, slippage or fill capacity is
invented: these conditions cannot change the analytical loss without additional
inputs, and their execution/cost consequences remain unmodeled. Hence equal
24h/72h loss numbers are not equal real-world risk.

Concurrent positions own50/30/20% ofG. They exit largest-first at hypothetical
reopening, one hour later and two hours later, receiving both actual model token
legs. Marking those legs is not immediate conversion. During the outage marks
are planning values, not permission to bypass stale-oracle liquidation guards.
The cumulative required reserve is **claims already paid + current shortfall on
unexited positions**; the maximum over the full path is used. Paid claims survive
a later price recovery. No new contributions or replacement positions are added.

CostsC are necessary aggregate zap/unwind/recovery/slippage/resource/restoration
costs counted once across all positions. An upfront reserve for the fullC plus
the loss peak is a conservative cost-timing assumption. The0/25/50% contingency
multiplies `(peak loss+C)`. ActualC is missing, so the default only computes
conditional loss-only floors. Larger unmodeled depegs or unavailable exits can
exceed them. Income/reward credits are zero for day1; no income/APR is forecast.

## Separate capital and leverage

For strategy deployment fraction `d=75% or90%` of supplier capitalS and leverage
`ell=1 or2`, `G=ell*d*S`, strategy equity=`G/ell`, debt=`G-G/ell`, and conditional
supplier withdrawal cash=`(1-d)*S`. Other commitments or losses reduce actual
cash. The 2x calculation explicitly assumes distinct lender cash outside S. If the same supplier pool also funds the debt, subtract that cash from the residual: at75%/90% deployment it is insufficient to supply the additional G/2. The JSON reports this funding gap separately. These are allocation scenarios, **not**75–90% retention of income.

Loss reserve, supplier withdrawal cash, operating expenditure and liquidator
funding are four separate purposes. Without recycling seized collateral,
principal-only funded-liquidator cash at2x isG/2, with largest-first tranches
0.25G/0.15G/0.10G. Actual accrued interest, transaction fees, lending losses,
oracle permissions and recoverability must still be assessed. Borrow interest
is an internal aggregate transfer, while it increases required payer cash; do
not call it new external strategy yield or double-count it as LP shortfall.

No existing dedicated reserve balance is verified. Zero seed credit means no
verified assets credited, not an assertion about actual treasury balances. Fund
the required seed with independently available assets before risk. Future50%
retention of eligible actually collected income can replenish it only under a
prospective approved ownership policy. Existing rewards/quarantine are excluded.

## Output and remaining evidence

There are108 strategy/path/outage/gross cases and12 allocation/leverage capital
rows. Each timeline records in-kind holdings, conditional losses, paid/unpaid
coverage, and the principal-only2x loss partition. Full-initial-G diagnostics are explicitly counterfactual; actual exit, cumulative paid and still-unexited partitions sum to required coverage at every boundary. Eleven unit tests cover the
single-underlying benchmark, wide-boundary continuity, cumulative paid claims,
linear scaling, partial-exit principal-deficit allocation, cost-once/contingency arithmetic, same-pool funding gaps and gross debt
counting, exact outage duration and common-numeraire depeg behavior. These are model tests, not contract
or empirical market validation.

Use handoff20 for the planning table and operator actions. Actual token/issuer
behavior, fee-tier/ticks/liquidity history, removed-liquidity shape, oracle outage
paths, concurrent liquidation execution, all-in cost quotes and committed seed
assets remain missing. Existing code/restoration, authorization and lifecycle
gates remain. Report UNPROVEN even when a conditional row has a numeric floor.
