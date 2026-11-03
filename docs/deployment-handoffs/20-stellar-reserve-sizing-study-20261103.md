# Conditional Stellar loss-reserve sizing — 2026-11-03

**Coverage remains UNPROVEN.** This supplies reproducible loss-only planning
numbers for the requested gross CL exposures, not safe production caps or an
all-in seed guarantee. Stage1 remains incomplete; PR75 stays draft. Borrowing,
production parameters, resource limits, transaction stages and00 are unchanged.
No Mainnet access, signing, broadcasting, deployment, secrets or secondary-oracle
integration was performed. [Handoff18](18-stellar-product-decisions-20261103.md)
still governs the product decisions and four distinct capital purposes.

## Exact scope and benchmark

Gross exposureG is1,000/5,000/10,000 USDC per pool, worth those dollar amounts
**only at the assumed initial USDCUSD=1**. Borrowed principal is included once.
The initial single-underlying USDC deposit is modeled as a costless50/50 zap;
the actual zap cost remains an unknown addback. The coverage benchmark is
**holding the original deposited USDC**, not holding a50/50 asset basket.

The study covers100% of the modeled aggregate LP shortfall against that benchmark
plus necessary unwind/recovery costs once. It does not guarantee100% of dollar
principal or protect against every conceivable path. A common25% depeg of both
assets produces zero LP shortfall in USDC units while losing25% of initial USD
value; that distinct risk is not silently insured by this benchmark.

Two analytic strategies are compared: fee-free full range and a hypothetical
fixed wide relative-price band `[0.5,2]`, with no recentering. Continuous inventory
equations use the [primary v3 math specification](https://app.uniswap.org/whitepaper-v3.pdf)
and [official amount formulas](https://raw.githubusercontent.com/Uniswap/v3-periphery/main/contracts/libraries/LiquidityAmounts.sol).
This validates the analytical formulas' provenance, **not Aquarius equivalence**,
production tick bounds or realized returns. Actual venue fees/rewards are not
credited, and no APR is assumed. Fees already owed to suppliers remain theirs.

Each path imposes50% third-party liquidity removal at hour1 and either24h or72h
of oracle/venue outage. Initial price1 is followed by the specified shock and
three prices for largest-first50/30/20% position exits at reopening/+1h/+2h:
the first conditional exit is at hour25 or73, a full24h/72h after the shock.

| Path | Shock PYUSD/USD; USDC/USD | Exit PYUSD/USD path; USDC/USD unless stated |
| --- | --- | --- |
| Persistent PYUSD declines |0.90,0.75,0.50 or0.25;1 | Same shocked price for all three exits |
| PYUSD recovery |0.50;1 |0.50 →0.75 →1.00 |
| Sequential deterioration |0.90;1 |0.75 →0.50 →0.25 |
| USDC decline |1;0.90 | PYUSD1, USDC0.90 throughout |
| Common decline |0.75;0.75 | Both0.75 throughout |
| Both depeg, PYUSD worse |0.25;0.50 | PYUSD0.25, USDC0.50 throughout |

No actual TVL, tick depth, fee tier, slippage or fillability is supplied. The
liquidity removal/outage assumptions are recorded but cannot be assigned invented
execution costs. Marks during an outage do not authorize stale-oracle seizure.
In-kind exits after reopening are **conditional**, not observed fills. The same
analytical price sequence yields equal24h/72h loss floors; this says nothing
about equal real costs, interest, capacity or risk at those durations.

## Conditional loss-only seed planning

At each boundary, required reserve is **shortfall already paid + marked shortfall
of remaining positions**. The maximum across the entire path is used; later
recovery does not refund earlier paid claims. All modeled positions are concurrent
from the start; no newly admitted exposure replaces exiting positions.

The worst named scenario is a persistent75% PYUSD decline with USDC at1. Full
range then marks at50% ofG; the hypothetical wide band holds only PYUSD below its
lower bound and marks at about30.1777% ofG. These are scenario-specific loss
floors, not estimated probabilities or universal maximum losses.

| Initial gross CL exposure | Strategy | Unbuffered loss-only seed, USDC | With25% contingency | With50% contingency |
| --- | --- | ---: | ---: | ---: |
|1,000|Full range|500.00|625.00|750.00|
|1,000|Hypothetical wide|698.23|872.78|1,047.34|
|5,000|Full range|2,500.00|3,125.00|3,750.00|
|5,000|Hypothetical wide|3,491.12|4,363.90|5,236.68|
|10,000|Full range|5,000.00|6,250.00|7,500.00|
|10,000|Hypothetical wide|6,982.24|8,727.80|10,473.35|

Values round upward to a USDC cent. A contingency can push planned seed above
initial gross exposure; it does not promise that a less severe loss is expected.
For necessary total costsC, the full expression is `(path peak shortfall+C) *
(1+contingency)`. C includes actual entry zap, unwind/exit slippage, recovery,
resource and restoration costs exactly once. It is unknown, **not assumed zero**.
Default results therefore leave guaranteed all-in seed `null`. Dedicated seed
assets have not been verified; no existing balance is credited. This does not
claim that actual treasury balances are zero.

For contrast, at a50% PYUSD decline the full-range loss against original USDC
is29.2893% ofG; the50/50 basket's impermanent-loss diagnostic is only4.2893%.
Using the basket diagnostic for this single-underlying product would materially
understate reserve planning. A price recovery can reduce remaining losses but
does not erase reserve amounts already paid to the first exiting position.

The full JSON includes per-exit **PYUSD and USDC quantities** and their marked
values. In-kind value is not a guarantee of an immediate USDC sale. For example,
the hypothetical wide strategy below its lower band exits entirely in PYUSD;
funded liquidators must be willing and able to hold/recover that exposure.

##1x first;2x is a separate, unapproved funding scenario

For fixed grossG, modeled LP assets and aggregate shortfall are the same at1x
and2x. At1x owner strategy equity isG and debt is zero. At2x equityG/2 plus
borrowed principalG/2 equalsG. The owner's original-USDC benchmark and lender
principal partition that same aggregate loss; **do not add principal collateral
deficit on top of it**. This deficit is measured before recovery or reserve
coverage. A successful funded full repayment pays the lender in full; negative
collateral economics then belong to the payer/liquidator unless covered. Actual
lender bad debt is therefore not inferred from collateral deficit alone. JSON
separates full-initial-exposure counterfactual marks from actual per-exit,
cumulative and unexited principal-deficit/owner-shortfall allocations. Their
sum equals required coverage at every boundary, even after partial exits.
Actual interest remains unquantified; it raises required payer funding but is an
internal transfer across borrower/lender accounts, not new external LP loss.

Without recycling collateral,2x funded-liquidator capital must at least cover
principalG/2 before accrued interest and its transaction funding. Largest-first
principal funding tranches are0.25G/0.15G/0.10G: forG10,000,2,500/1,500/1,000
USDC, totaling5,000. Seized in-kind collateral does not make those funds available
for the next liquidation immediately. This budget is **separate** from the loss
seed, supplier withdrawal cash and operating budget. Availability of funding
also does not overcome a missing trustworthy seizure price or failed execution.

The75%/90% setting is supplier-capital deployment, not retained-income allocation.
For supplier capitalS, deployment fractiona and leverageell, `G=ell*a*S`; the
conditional idle supplier cash is `(1-a)*S` if no other uses/debts/claims consume
it. ForG1,000 at1x, S is1,333.33…/1,111.11… and idle cash333.33…/111.11…;
at2x, S is666.66…/555.55… and idle cash166.66…/55.55…. JSON preserves precision.
The2x identities assume **distinct lender cash outside S**. If the same
supplier pool funds borrowed principal as well, that amount also reduces idle
cash. At75%/90% deployment the residual cannot fund the additional G/2; JSON
reports the resulting funding gap. Neither interpretation establishes sufficient
withdrawal liquidity. Apply the withdrawal-demand and other-commitment tests
from18 separately.

Day1 reserves receive **zero future-income credit**. Future50% retention applies
only to actually collected eligible income under an approved prospective policy;
previously earned rewards, supplier liabilities and quarantine are excluded.
Neither an invented yield nor a forecast replenishment date reduces these seeds.
Full versus wide *net supplier return* still needs actual fees, loss paths, costs
and prospective retention; this no-income stress model is not that return study.

## Reproduction and next operator decision

From `peridot-contracts`:

```sh
python3 research/cl-reserve-planning/study.py
python3 -m unittest discover -s research/cl-reserve-planning -p 'test_study.py' -v
```

The [local study](../../peridot-contracts/research/cl-reserve-planning/README.md)
contains source, deterministic results and11 passing model tests.108 cases cover
3 gross exposures ×2 strategies ×9 paths ×2 outage lengths;12 additional rows
separate leverage and capital allocation. These are mathematical tests only.
No contract execution/resource gate is represented as passed by this study.

Use these numbers to request dedicated available seed capital and compare the
explicit stress choices—not to activate a cap. Before choosing a funded amount:

1. Confirm coverage units/exclusions and the chosen range/stress set; consider
   deeper depegs and reserve-asset impairment rather than treating75% as a maximum.
2. Supply actual venue tick/liquidity data and the shape of the50% removal,
   authenticated prices, executable in-kind exits and concurrent liquidation
   capacity under24h/72h outages. Keep the primary stale-price blocker explicit.
3. Obtain necessary all-in cost bounds and enter them as C once. Identify who
   pays each cost and whether another operating budget already funded it; avoid
   counting one cost or one dedicated token twice. Interest and lender accounting
   require their own reconciliation against the same gross asset loss.
4. Verify independently earmarked liquid seed assets, withdrawal cash, operations
   runway and liquidator cash. Apply18's exhaustion rules: immediate fair loss
   recognition, no stale-NAV early-exit priority, new-risk pause, preserved
   authorized recovery and no entitlement pruning.
5. Keep existing code/restoration, authorization and complete lifecycle gates.
   A model with finite reserves cannot make an unexecutable repayment/seizure
   transaction safe or establish borrower-independent recovery for a new stage.

No live capacity, safe cap, all-in protected amount or production approval has
been established. Secondary-oracle implementation remains deferred.
