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

## Next release gates

1. Integrate actual Aquarius all-stream/recycled/outage checkpointing with lending
   share mutations, including deposit, withdraw, delegated transfer and seizure.
   Preserve deferred owner claims without assuming a reward swap can always run.
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
