# Isolated LP lending — implementation boundary

September 17, 2026. This supersedes the **supply-only final target**, not existing
Mainnet safety controls. Native research only; no deployment candidate exists.

## Confirmed product

One NEW LP-only Peridottroller manages three lending-capable receipts (XLM, PYUSD,
USDC). A user supplies XLM collateral to this group and can borrow PYUSD or USDC
from this same group. The controller aggregates the user's LP-group debts and
eligible collateral. Core/DeFindex collateral and debt must not enter that health
calculation, and LP collateral must not authorize core-group loans. Individual LP
markets are **not** isolated from one another.

Retain one settlement-denominated strategy per receipt. PYUSD and USDC strategies
may use the same concentrated pool but not a single shared receipt/strategy share
ledger. The keeper remains operational infrastructure, not the lending risk manager.

## Accounting to preserve

Use the existing lending engine as the integration baseline, rather than adding
borrow entrypoints to the lean supply-only principal engine. Final extraction of
shared lending code/public exports is still required: linking a generic contract
must not accidentally expose uncheckpointed legacy entrypoints in the new receipt.

Supplier NAV remains:

`managed settlement cash + strategy value + outstanding loans - reserves - admin fees`

- Borrowing moves cash/strategy value into a loan receivable; it does not burn
  supplier pTokens or give the borrower supplier reward weight in that debt market.
- Repayment replaces the receivable with actual settlement cash. Interest changes
  NAV and debt, not share count or historical reward ownership. LP emissions can
  fall as borrowing pulls capital out of the pool; lending interest is separate.
- Uncollected reward receivables and unconverted raw rewards are not collateral
  NAV. Converted escrow pTokens already belong to total supply; their separate
  owner claims must not create a second collateral or principal valuation.
- The lean proportional cash/strategy-only exit cannot be reused for lending:
  it omits loans. Do not burn a lender's entire claim in exchange for only their
  fraction of currently liquid assets, leaving the loan value to other holders.
  Preserve lending withdrawal liquidity and collateral-health checks.
- Borrow/repay may change actual LP custody without changing receipt weights.
  Integrate fresh primary/gauge claims and fees around strategy operations without
  double counting or attributing old emissions to later receipt mutations.
- Liquidation transfers collateral shares, not the borrower's previously earned
  raw/converted rewards. Checkpoint borrower, liquidator and any fee recipient
  before moving shares. Future emissions follow the new balances. Existing
  controller incentive indices also require separate validation/checkpointing.

## Implemented native building block

`receipt-vault/src/reward_share_hooks.rs` provides an opaque same-invocation
checkpoint around **supply-neutral** share movement. It handles up to three
distinct affected owners and all four retained reward streams, coalesces duplicate
fee/liquidator addresses, and debits weights before crediting them. It checks
supply, escrow/backing, registry, stream stability and conservation of affected
balances. Missing histories fail closed through the shared ledger; escrow cannot
be an ordinary collateral recipient. Failure must revert the whole operation.

The helper does not authorize a transfer, discover pool emissions, prove registry
completeness or replace the all-stream coordinator. The caller must first perform
the complete fresh reward checkpoint, invoke the authenticated lending operation,
then finish within the same atomic invocation. There are no production call sites.
The helper is behind `test`/`hybrid-rewards`; the latter still rejects WASM builds.
Do not deploy the test wrapper or use `begin`/`finish` as user-callable methods.

## Current tests and resource boundary

Run `cargo test -p receipt-vault --locked lp_lending_ -- --nocapture`.

The native fixture uses actual ReceiptVault lending methods, two separately
registered Peridottrollers, JumpRateModel and real SAC transfers. Three test assets
play XLM/PYUSD/USDC roles at synthetic prices; they are not live Mainnet assets.
Principal stays idle. Rewards are controlled funded token deposits, **not Aquarius
claims**. Initial receipt deposits precede reward registration; the wrapper blocks
uncheckpointed deposits afterward. One isolated-group fixture seeds a known owner
through native core deposit and explicit weight synchronization before emissions.

Regressions cover both stablecoin loans against LP XLM, shared debt limits,
same-token collateral excluded in both directions across groups, repayment and
interest NAV, borrower/supplier reward separation, delegated/self transfers,
unsafe-transfer rollback including allowances, liquidation with separate or
duplicate fee recipients, four retained streams, omitted-recipient rollback,
missing stream/escrow rejection and exact borrower/liquidator authorization.

SDK25's bundled 100-entry fixture initially failed at 121 entries for the second
loan and 149 for an unsafe delegated transfer. These tests deliberately use a
**200-entry research envelope**, retaining the SDK's CPU/memory/write limits.
They never disable transaction resource enforcement. This is not a claim about
current network limits or compiled transaction fit. The direct host dev-dependency
only names the same SDK-pinned host to configure that bounded test envelope; no
production dependency version changed. Final measured values are in Agents.md.

## Aquarius-backed native follow-up

`receipt-vault/src/reward_claims.rs` now contains the existing all-stream claim,
receivable, registration and custody-check code. Both principal engines compile
that same source. Retained keys and accounting rules are unchanged. This does not
port the lean proportional exit or retirement policy into the lending engine.

`reward_lending.rs` calls this checkpoint around lending deposit, withdrawal,
borrow, transfer, delegated transfer and seizure. Share movements use the existing
opaque hooks. Deposit/withdrawal reconcile actual owner/supply deltas, retain
escrow, and update every retained stream. Withdrawal reserves earned raw claims,
then calls Core's debt-aware withdrawal and enforces the user's actual payout
minimum. Failed health, liquidity, auth or minimum checks roll everything back.
Core authenticates once in the same frame; a second wrapper auth conflicted with
Core's authorization arguments and was removed, not bypassed.

Repayment deliberately remains Core's settlement-cash-for-debt operation: it does
not change shares or redeploy liquidity, so reward-data failure must not block it.
Separate native admin reinvestment changes custody only, not reward weights.
Lending-specific recycled backing conversion/payout, fee attribution and complete
production selector coverage still require integration; these tests do not prove
all of those work together with debt. The native test wrapper has no production
activation, and default contract entrypoints remain unchanged.

Run `cargo test -p aquarius-lp-vault --locked aquarius_lending_ -- --nocapture`.
Nine new regressions use actual native Aquarius strategy/bridge, lending engine,
controller/JRM, real SACs, and controlled **mock** concentrated pools and oracle.
The two stable settlement strategies use opposite indices of one shared pool.
Both stable loans unwind actual strategy shares; wallet/debt/share and reward
cash are checked. Tests also cover repayment/reinvestment, partial lender exit
with an outstanding loan, refusal to erase all lender shares while debt exists,
unsafe collateral withdrawal, minimum rollback, late-deposit/delegated-transfer
reward ownership, exact borrower/liquidator/full-exit auth, failed swap rollback,
and observable versus unobservable reward outages. Primary and gauge use distinct
real tokens. Rewards do not need a conversion route for principal exit.

Initial integrated exit/minimum rollback exceeded the earlier200-entry research
envelope at205. This fixture explicitly uses250 entries, retaining SDK CPU,
memory and write limits; limits are never disabled. Current native maxima include
206 entries for an LP-funded collateral exit,196 for the second stable borrow,
191 for liquidation with claims and42 writes. This is NOT full compiled-stack,
actual deployed-pool-WASM, live oracle/gauge or current network-limit evidence.

**Newly exposed legacy cash-policy gap:** donations are excluded from initial
NAV, but Core's `ensure_liquid_cash` counts actual cash (including donations).
An exit can therefore consume untracked settlement cash, unwind less LP, and
leave additional strategy value for remaining holders. The new regression pins
that behavior explicitly; it is NOT proof of strict donation segregation. Raw
reward donations stay unallocated. Resolve settlement cash policy/segregation
before the LP production ABI; no generic core or Mainnet behavior was changed.

## Managed cash and lending-backed reward settlement — September 18

The legacy donation issue above is now fixed in the **native LP path**. Core's
borrow/withdraw/rebalance bodies share an internal cash-policy parameter. Existing
public entrypoints select the legacy policy; native LP helpers select managed
cash and are outside `contractimpl`, behind test/hybrid gates. There is no mutable
mode flag, production activation or caller-selected policy. Source was refactored,
so do not claim byte-identical deployed WASM; the live contracts are untouched.

LP payouts and LP redeployment now use only tracked cash, require actual custody
to cover it, and pull missing liquidity from the strategy. Donations remain intact
after deposit, borrow, repay, reinvestment, partial/final withdrawal and reward
payout. Missing records are checked before legacy helpers can reconstruct them.
The new regression replaces the prior expectation of donation consumption. This
does not retroactively change existing core/DeFindex donation policy.

`reward_settlement.rs` extracts the existing conversion/recycling/reserved payout
rules once for both research engines. All retained recycled streams must settle
before new reward units or owner payouts; a blocked route preserves pending
inventory and returns Deferred. Successful staged conversions enrich old units
without issuing new units. Each later operation still claims fresh emissions.
Lending uses the same real escrow pTokens and loan-inclusive NAV. Its internal
backing redemption selects managed-only Core withdrawal via a Rust function
pointer, never a user-supplied contract callback/selector. Ordinary collateral,
liquidity, authorization and last-lender debt checks still apply. In particular,
an already-underwater borrower cannot use reward payout to bypass Core health
checks; that liveness policy remains explicit rather than silently relaxed.

Nine additional native regressions cover converted payouts in all three asset
roles, both stable settlement indices, LP-funded payout while debt remains,
new escrow pricing against outstanding debt, staged recycling failure/retry,
principal/repayment during reward-route failure, converted/reserved claims after
full exit and later deposit, exact payout auth/minimum rollback, donations and
missing custody, and converted rights retained by the borrower after liquidation.
Tests use real SAC transfers and native strategy/controller/JRM with **mock**
concentrated pools/oracle. The bounded250-entry fixture and other SDK limits stay
enabled; no full compiled lending-stack, current-network or production-liveness
claim follows from these tests. See Agents.md for final measurements/checks/scan.

## Next release gates

1. Complete production coverage of the native all-stream/recycled/outage lending
   integration, including controller incentive accounting during escrow share
   mint/burn and temporary owner credit before backing withdrawal. Preserve
   deferred owner claims without assuming a reward swap can always run.
   Inability to observe rewards must not silently reassign rights; liquidation
   liveness under a total reward-data outage remains an unresolved design gate.
2. Validate real strategy-funded borrowing, repayment/redeployment, loan-backed
   withdrawal and post-unwind health. Exercise simultaneous users, bad debt,
   LP losses, fees, zero-cash states and liquidation price/quote outages.
3. Implement explicit receipt mode/ABI and legacy harvest exclusion. No generic
   lending selector or standalone helper may bypass the complete coordinator.
4. Replace temporary parity-alias collateral valuation with reviewed depeg-aware
   pricing/freshness behavior. Choose collateral factors, debt caps, cash buffers,
   rate curves and liquidation parameters explicitly; fixture numbers are NOT
   production defaults or approvals.
5. Rework the migration from the existing LP pilot controller into the NEW
   controller, preserving balances, strategy binding, old debt and incentive
   obligations. A new controller does not make old obligations disappear. The
   old supply-only migration draft is research, not the approved migration plan.
6. Reproducible compiled candidates, exact-WASM/auth/resource and restoration
   tests, final Almanax review, governed timelocks and a bounded Mainnet canary
   precede borrowing activation. No changes to the other market group or treasury
   signature policy are part of this integration.
