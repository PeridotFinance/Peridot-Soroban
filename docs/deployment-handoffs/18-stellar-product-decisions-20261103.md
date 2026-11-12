# Stellar product decisions and reserve scenarios — 2026-11-03

Stage 1 remains incomplete; PR #75 remains draft. This records the current
product direction without changing shared handoff 00, contracts, resource
profiles, caps or production configuration. Borrowing stays disabled and
production maintenance margin remains unset. No Mainnet calls, signatures,
broadcasts, secrets, deployments or Upshift binding were used for this document.
The Stellar Soroban skill informs the accounting, custody and recovery review;
the repository and preserved execution evidence determine actual capabilities.

## Decisions and immediate priorities

Defer the secondary oracle. Do not start another oracle integration, API
publisher or paid data service. Preserve existing research for a later decision.
Deferral does **not** resolve the primary-oracle stale-price liquidation blocker
or authorize stale-price seizure, weaker valuation checks or borrowing activation.

Compare a full-range strategy with a simple, explicitly specified wide range
using actual supplier net returns, losses and costs. Choose neither from quoted
fee APR or liquidity utilization alone. Optional in-kind exits are accepted as
a product direction; their existing compiled evidence is reusable, subject to
the operational and resource limits below.

A segregated loss reserve should be seeded before exposing the intended risk.
Deploying75–90% of supplier capital into the strategy and retaining50% of eligible
future collected strategy income for the loss reserve are **separate scenarios**.
The75–90% concerns capital deployment, not a second split of retained income.
Neither is an implemented fee, approved allocation, forecast return or protection
promise. Existing supplier and historical reward entitlements cannot finance
that reserve retrospectively.

## Four capital purposes, with separate ledgers

| Purpose | What it funds | What must not be counted as the same available capital |
| --- | --- | --- |
| Supplier withdrawal cash | Actual local token liquidity available to redeem supplier claims, subject to the existing market accounting. | Strategy NAV, outstanding loans, loss-reserve assets, reward liabilities or an assumed immediate sale of seized collateral. |
| Seeded loss reserve | Independently funded assets under a prospective, explicit loss-coverage policy, with eligibility, payout authority and limits. | The market's minimum cash reserve, supplier principal, unpaid AQUA, quarantined rewards or money simultaneously promised for withdrawals. |
| Operating expenditure | Gas, resource fees, archival restoration/retention, indexing and other approved operating obligations. | A guaranteed future income stream or loss capital already committed to claims. Infrastructure pricing is outside this handoff. |
| Funded-liquidator capital | External, spendable debt-asset cash that can pay accrued debt before receiving permitted collateral in the atomic funded route. | Loan receivables, borrower collateral before seizure, market withdrawal cash or the loss reserve unless an explicit reviewed policy dedicates separate funds. |

Supplier strategy principal remains an asset allocation of the supplier market;
outstanding loan principal plus accrued interest remains a receivable. Neither
creates additional cash. If the same treasury holds multiple purposes, its
ledger must earmark balances and commitments so one token is counted once.
Native `min_cash_reserve_bps` enforcement protects borrowing liquidity; it does
not create loss-absorbing equity or guarantee withdrawals under all conditions.

Funded liquidation can exchange liquid cash for illiquid receipt/share claims.
Its capital budget must include the interval until those claims can actually be
converted, including simultaneous liquidations and a broken exit venue. Receiving
collateral does not immediately replenish the liquidator's cash budget.

## What the repository establishes about caps

These are local source facts, **not current on-chain configuration readbacks**:

| Source | Values / status | Permitted use in this decision |
| --- | --- | --- |
| [Legacy PYUSD/USDC deployment script](../../peridot-contracts/scripts/deploy_aquarius_pyusd_usdc_mainnet.sh) | Defaults `PYUSD_SUPPLY_CAP`, `PYUSD_MAX_DEPLOY`, `USDC_SUPPLY_CAP`, `USDC_MAX_DEPLOY` each to125,000,000,000 raw units:12,500 tokens at7 decimals. Environment overrides exist. | Historical script defaults only. They do not prove that these limits were deployed, remain active, or apply to the new CL share vault. Do not add the two token quantities and silently label the sum dollars. |
| [LP lending target configuration](../../peridot-contracts/config/lp-lending-mainnet.json) | `approved-targets-not-applied`; XLM/PYUSD/USDC supply/borrow caps are raw0. Its text explicitly says0 disables caps and is not borrowing disablement. | An unapplied, older target for a different configuration. It does not supply a finite CL exposure bound or authorize this launch. |
| [Generic LP deployment template](../../peridot-contracts/scripts/deploy_aquarius_lp_market_mainnet.sh) |100,000-token default supply/deploy limits. | Another template, not evidence of this product's exposure. |
| Receipt/vault implementation | Supply/borrow cap0 means no cap; `max_deploy=0` disables its deploy cap. A positive deploy cap limits pool deployment, not all supplier liabilities or pool traversal cost. | Code semantics only. Neither a finite deployed cap nor a stress-loss bound is inferred. |

Harness values—including1B/2B capacity,1000bps cash reserve,25M/100M raw
positions and test maintenance margin—are excluded from economic sizing.

**Numeric result: minimum seed and affordable strategy allocation are blocked.**
There is no verified finite cap set for the proposed CL product in the inspected
configuration, no approved loss-coverage definition, and no measured stress loss
or liquidator funding envelope. No finite seed can be justified against an
uncapped exposure from the percentage scenarios alone. This does not assert
that live deployments are uncapped; no Mainnet readback was performed here.

Subsequent explicit planning inputs are evaluated in
[handoff20's executable reserve study](20-stellar-reserve-sizing-study-20261103.md):
1,000/5,000/10,000 gross CL exposure, original-USDC deposit benchmark, named stress
paths and loss-only seed floors. These conditional numbers do not resolve the
missing actual-capacity/cost evidence or establish a guaranteed all-in seed.

Required inputs are: exact asset identities/decimals and contract/version scope;
approved and enforced supplier, deployment, debt and concentration caps; initial
debt-free1x versus any future borrowed exposure; stress price/depeg paths, range
liquidity and exit costs; correlated withdrawal demand; oracle-outage duration;
concurrent accrued debts to liquidate and cash recycling delay; independently
available seed assets with haircuts; permitted reserve coverage/exclusions;
the prospective income-ownership waterfall; and operating/retention obligations.

## Bounded sizing formulas, not a funded recommendation

Use a common valuation date and stated settlement units. Let `S` be supplier
capital and `a` the fraction deployed into the strategy. For a finite admitted
exposure vector `C(a,S)` and reviewed stress set `T`, let `L(C(a,S),s)` be the eligible loss
that the reserve is intended to absorb after supplier/borrower allocation and
actually recoverable collateral, excluding operating and liquidator capital
already budgeted separately. Do not subtract uncertain future income or count
illiquid nominal claims at face value.

`required_loss_capacity(a,S) = max(s in T) L(C(a,S),s) + explicit_model_margin`

For independently earmarked existing loss capital `R0` and conservative
realizable haircut factor `h` for newly seeded assets (`0 < h <= 1`):

`additional_seed >= max(0, required_loss_capacity - R0) / h`

The stress set and margin must be approved and measured; this formula does not
choose them. If exposure is unbounded, coverage exclusions undefined, or `h`
unsupported, the result stays **unquantified**, not zero. A reserve intended to
cover only specified losses must disclose that scope instead of implying full
principal insurance.

Budget withdrawal cash from simultaneous cash demands and realizable inflows
under the same stress horizon, keeping the enforced native reserve floor as a
separate constraint. Budget funded-liquidator cash from the peak concurrently
funded accrued debts before collateral recycling; include authorization/fee
funding and price/slippage constraints. Neither budget is interchangeable with
`required_loss_capacity`.

For income, define `I(a,S,s)` as **actually collected strategy income eligible
for a prospectively approved ownership split** over a specified period and
scenario. It excludes previously owned reward liabilities and quarantine. Its
cost basis must be explicit: if collection or execution costs have already been
deducted in `I`, they cannot be charged again. Until that prospective ownership
policy exists, this scenario is not executable. With retained fraction `r=0.50`,
loss-reserve additions are `0.50 I(a,S,s)`; the other `0.50 I(a,S,s)` is supplier
income before any separately specified, non-duplicated supplier costs. Supplier
net performance also includes principal gains/losses and actual flows; half the
collected income is not a promised net yield.

| Capital-allocation scenario | Strategy deployment | Residual supplier withdrawal cash, only with no other asset uses/debt | Prospective loss-reserve income addition |
| --- | --- | --- | --- |
| `a=0.75` |`0.75 S`|`0.25 S`|`0.50 I(0.75,S,s)`|
| `a=0.90` |`0.90 S`|`0.10 S`|`0.50 I(0.90,S,s)`|

These cash figures are conditional balance identities, not liquidity guarantees.
If `U` is other supplier assets already committed outside local cash—such as
outstanding loans, counted once—and `W` the required withdrawal cash, the simple
admission condition is `a S + U + W <= S`, hence
`a <= 1 - (U + W)/S` for `S>0`. Use reconciled actual local cash and liabilities,
not NAV alone, to apply it. A10% residual is sufficient only if the measured
withdrawal requirement and other commitments fit it; no such input is supplied.

An affordable `a` must **also** satisfy finite deployment/debt/concentration caps,
the seeded stress-loss capacity condition above, available funded-liquidator
capital, primary-oracle/recovery and execution gates. No single percentage
replaces those constraints. The loss seed, operating budget and liquidator cash
are separately funded/earmarked; they are not additional claims on the same
residual supplier cash. If an illustrative per-deployed-capital income rate `y`
is used, `I=a S y` would give reserve contributions `0.375 S y` or `0.45 S y`.
Those are normalized algebra only: `y` is not a measured yield, and37.5%/45%
are not alternative retention rates on already collected income.

Zero eligible income produces zero reserve replenishment. Negative performance
does not create a contribution; the seed and separately funded operations must
stand on their own. Replenishment timelines require realized observations, not
invented APR. Future income cannot replace the seed needed before exposure.

## Reserve exhaustion and fair loss recognition

A finite reserve can be exhausted. Apply only available, independently earmarked
reserve assets under the published coverage rules; never mint fictitious backing,
borrow historical reward liabilities or assume a future top-up. Recognize a
verified strategy/credit loss in the affected market's NAV promptly, including
any residual loss after eligible reserve coverage, before finalizing payouts or
share burns. The policy must identify which market and holders bear that residual
loss. Unrelated markets or historical reward beneficiaries cannot silently fund it.

Do not let early redeemers escape an already incurred loss at a stale exchange
rate, leaving the deficit to later holders. Apply the same loss-adjusted valuation
and coverage rules to affected holders; a discretionary first-come reserve payout
is not an acceptable substitute. If loss valuation is unresolved, do not process
ordinary payouts against a known misleading NAV. Any available proportional
in-kind recovery must preserve fair allocation, ownership and debt constraints.

Pause new risk when reserve coverage or its required replenishment is breached.
Keep authorized repayment, funded liquidation and permitted recovery routes
available subject to their actual valuation/authorization/resource conditions;
reserve exhaustion must not add a governance-attribution or keeper-availability
prerequisite. Record exhausted capacity and the uncovered loss explicitly.
Preserve all booked and unresolved entitlements and exited beneficiaries; no
pruning, forced waiver or newcomer-funded redistribution repairs a deficit.
Resume new risk only after a reviewed recapitalization or lower finite exposure
restores the stated coverage, liquidity and recovery conditions.

## Compare full range and a simple wide range on supplier outcomes

Fix exact token identities, fee tier, deployed bytecode, initial capital, price
path and common calendar period. Specify the wide range's actual lower/upper
ticks, out-of-range behavior and any recenter rule/cooldown; “wide” alone is not
a strategy. Compare equal economic exposure and record capital additions and
withdrawals. Preserve the existing range research; do not import an unreviewed
range rule into deployment preparation.

Maintain one portfolio ledger per strategy as required by00: ending supplier
equity plus withdrawals/distributions minus starting equity and contributions.
Account for actual swaps, collected LP fees, realized strategy losses, inventory
mark-to-market, execution slippage, resource fees, restoration, prospective
protocol retention and, where relevant, borrow interest. Use actual transferable
or distributed reward value under the existing entitlement policy; show unsold
reward quantities separately. Do not count the same retained income as both
supplier return and reserve growth, or count internal borrower interest as new
system yield.

Report dollar/token P&L, time-weighted strategy return, depositor money-weighted
return for explicit flows, maximum drawdown, worst exit realization, time out of
range, failed/delayed exits, claim/oracle outage behavior, transaction count and
complete paid execution/restoration costs. Compare at each proposed finite cap
and a sharply reduced/zero-AQUA scenario. Attribute LP fees and reward income
separately; do not subtract impermanent loss or LVR a second time after portfolio
losses have already entered P&L. No current measurement establishes that either
range produces a superior sustainable supplier net return.

## Primary oracle and accepted in-kind exits

The funded route in [funded_liquidation.rs](../../peridot-contracts/contracts/margin-controller/src/funded_liquidation.rs)
uses fresh debt/collateral prices through `refresh_price_usd`; unavailable pricing
reverts before funded repayment/seizure. It does not fall back to a cached
seizure price. Consequently a stale/unavailable primary feed can prevent forced
liquidation even with liquidator cash. Preserve that blocker while the secondary
oracle is deferred. Operators need freshness/status monitoring and an incident
procedure that blocks new risk, preserves historical claims, and identifies
which authorized repayment/release routes remain usable. Do not declare a
stablecoin permanently worth one unit or use a spot pool quote as a replacement
independent valuation.

Existing [emergency_redeem](../../peridot-contracts/contracts/cl-share-vault/src/contract.rs)
returns both pool-token legs to the authorized **unencumbered direct holder**.
It records the recovery reward boundary, collects LP trading fees before the
proportional burn and can preserve rewards without voluntary waiver. The token
mix may differ from the deposit asset. User acceptance of in-kind exits is not
blanket consent to waive rewards, accept a forced haircut or release encumbered
assets before debt settlement.

Reusable compiled evidence:

- [Handoff15](15-cl-split-transaction-feasibility-20261102.md),
  `compiled_exit_assessment_in_kind_then_owner_conversion`: actual Aquarius,
  two owners, real trading fees/rewards, donation exclusion, proportional in-kind
  exit with waiverfalse, separate owner-authorized conversion and exact scaled
  reward conservation. The exit itself costs about130.146M CPU and fails the
  unchanged100M policy. Subsequent conversion is a separate user action, not an
  atomic promise to redeem in the deposit token.
- [Handoff16](16-cl-atomic-qualification-20261102.md),
  `compiled_atomic_qualification_8_independent_repay_release_and_exited_entitlements`:
  receipt/margin release through both reward-call failures, later receipt
  withdrawal and physical in-kind exits with claim killed, retaining and paying
  booked exited-owner rewards. That historical repayment branch uses static
  interest. The latest separate prototype checkpoint, to be indexed in handoff17,
  reports three compiled dynamic-repayment tests passing, including actual nonzero
  accrued debt. Preserve that newer evidence separately; it is not retroactive
  dynamic coverage of handoff16 or production approval. Physical pool withdrawal first
  restores the controlled malformed pool record, so this is not proof of exit
  through arbitrary venue storage failure.

In-kind exit still depends on pool fee collection/position withdrawal and token
transfer availability. An oracle-free direct exit is not proof of price-free
funded seizure. Receipt/share delivery is not guaranteed immediate underlying
liquidity. Frozen/clawed-back assets, venue withdrawal failure, code/restoration
limits and signed authorization remain separate operational gates.

## Income ownership and operator actions

Do not skim previously earned AQUA or booked supplier fees into a new reserve.
Reward liabilities remain outside principal NAV; unresolved quarantine is not
income available to the protocol. Preserve original entitlement, prospective
accrual, voluntary waiver redistribution and unresolved amounts separately.
Forced liquidation never waives. Missing authentic boundary evidence stays
quarantined indefinitely; a later cumulative reading does not reconstruct it.
Follow [handoff13](13-cl-history-operating-plan-20261102.md) for evidence capture,
exited-beneficiary indexing, governance attribution and archival retention.

Before selecting a reserve scenario, document a prospective fee/ownership policy
and obtain coordinated reconciliation if it conflicts with00. This handoff does
not amend00 or charge a new fee. A loss-reserve claim policy also needs triggers,
asset eligibility, payout priority, authority, coverage limits and accounting
that cannot seize historical reward backing.

Concrete next operator actions, without activation or transactions:

1. Assemble an approved finite-cap worksheet with contract/asset/version scope,
   distinguishing proposed values, legacy defaults and later verified readbacks.
   Keep borrowing disabled while the worksheet and safety gates are incomplete.
2. Supply the stress-loss, withdrawal-demand, liquidator concurrency/recycling,
   seed-asset and operating-obligation inputs above. Produce four earmarked capital
   budgets; evaluate75% and90% supplier-capital deployment with50% prospective
   eligible-income retention, cash-demand constraints and reserve-exhaustion loss
   allocation. Do not apply75%/90% as another income-retention split.
3. Prepare equal-cap full/wide portfolio comparisons and an in-kind exit product
   specification: assets received, user-controlled subsequent conversion, reward
   claim survival, debt/custody restrictions and explicit failure states.
4. Retain primary-oracle freshness/incident monitoring; defer secondary-oracle
   implementation. Identify the remaining stale-price recovery blocker in every
   activation checklist without weakening it.
5. Carry forward the resource checkpoint: the lead's current storage/code study
   identifies419,616 required serialized code bytes versus the captured200,000
   disk-read-byte ceiling. This is a distinct obstruction from CPU/write policy,
   and in-kind acceptance does not remove it. Any necessary next split must first
   define and prove the complete lifecycle, custody/debt, deadlines, retries and
   borrower-independent recovery for every debt-bearing intermediate state.
   Keep funded repayment and permitted seizure atomic. Preserve the prior
   negative split and all failed evidence; do not announce a deployable design
   from a reduced single-operation cost.

No yield, loss protection, fixed seed amount or production readiness is promised
by these decisions. Infrastructure cost comparisons remain separate work.
