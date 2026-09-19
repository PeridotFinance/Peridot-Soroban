# Isolated LP lending — implementation boundary

September 18, 2026. This supersedes the **supply-only final target**, not existing
Mainnet safety controls. Local validation artifacts now exist; they are NOT
Mainnet release candidates. Fresh Mainnet initialization is explicitly blocked.

## Latest compiled validation and migration scope

The separate `hybrid-validation` strategy feature now permits compiled local or
Testnet tests, with public-network fences in initialization, hybrid activation and
all bridge calls. Ordinary `hybrid-rewards` WASM compilation remains blocked.
Hybrid activation excludes both legacy harvest and sweep, not just primary-token
rotation. Compiled receipt/controller/strategy plus exact deployed pool WASM pass
simultaneous loans from both stable markets against XLM, exact borrower auth,
aggregate over-limit rejection, actual primary rewards, conversion, payout, repay
and exit. Latest peak240entries/197.13M CPU with live-price controller in the
bounded250-entry/200M fixture; live
read-only network limit400M CPU. Mock oracle/reward routes/plane/boost remain.
These tests are not a Mainnet simulation, real gauge test or live canary.

User approved the fresh small-canary migration described in
`FRESH_STACK_MIGRATION.md`; no funds moved. Existing pooled rewards must settle
before withdrawing to deployer and redepositing via NEW Peridot receipts.
Native Reflector now provides PYUSD/USDC; the new `CrossQuoted` router source
converts through upstream USDC/USD, checks currency/precision/freshness and does
not cap debt valuation at parity. yXLM has no observed Reflector feed. The proposed
keeper-published observation source is now user-approved and implemented locally:
window-end freshness, scoped reporter, two-way pool checks and required-observation
dependencies; LP controller risk checks bypass neither live invalidation nor
expiry via caches/fallbacks. Read-only live collector halted on sparse SDEX volume.
See `../../bots/aquarius-keeper/PRICING.md` for candidate policy and limitations. Neither
the router change nor migration approval removes the remaining release gates.

## Explicit LP lending interface — September 18

`contracts/lp-lending-vault` now exports a separate contract interface over the
shared lending engine. Engine methods compile as ordinary Rust, not inherited
contract exports. Deposit, withdraw, borrow, transfer, transfer_from and seizure
use the complete reward coordinator; repayment stays independent of reward
availability. Conversion, recycling, reserved claims and owner payouts use the
same managed-cash and historical-ownership accounting. Admin handoff and upgrades
retain the core authorization/timelock checks. Margin, flash loans, recovery,
bootstrap and uncheckpointed token mutations are not receipt exports.

`contracts/lp-peridottroller` is a separate zero-PERI artifact. Its fresh-instance
policy marker cannot be reconstructed by upgrading an old controller. Nonzero
supply/borrow emission rates are rejected even for the admin; the PERI token may
still be configured. It admits at most three LP-versioned receipts, not generic
core markets. The receipt seals controller/strategy bindings after activation.
This is a code-enforced configuration policy, not a promise that a future
governance-approved code upgrade cannot change it.

User-approved target collateral factors are **XLM 50%, PYUSD 80%, USDC 80%**
(1e6-scaled: 500000/800000/800000). Fresh activation checks the controller's factor
against the receipt's target. `config/lp-lending-mainnet.json` records those targets
and existing pilot addresses, NOT applied Mainnet settings. Existing live factors
remain zero; temporary parity-alias pricing is not sufficient borrowing validation.

The native Aquarius bridge now has a one-way fresh-strategy hybrid activation
marker. It fences legacy harvest and raw primary-token changes, and receipt
activation verifies it. The ordinary Aquarius hybrid-WASM compile gate remains;
there is no reviewed legacy-state migration or deployable full hybrid stack yet.
Fresh deployment constructors reject the public-network ID. Do not remove these
release guards or install validation artifacts on existing Mainnet contracts.

Actual-interface tests use three receipt instances, the zero-PERI controller,
real SACs, native Aquarius/JRM and mock pools/oracle. Both stable settlement
strategies use one pool with opposite settlement indices. Tests cover cross-market
loans, aggregate limits, reward ownership, liquidation, authorization, outage
repayment, sealed setup and Mainnet-init rejection. A separate explicit test loads
the compiled receipt AND controller; it passes borrowing, conversion, payout,
repayment, transfer and exit with the strategy still native. Measured compiled
steps: loan 220 entries/80.59M CPU, conversion 122/65.79M, payout119/96.08M.
The bounded250-entry research envelope and other SDK resource limits stay on.
The payout is close to this fixture's100M CPU ceiling. This is NOT full compiled
strategy/pool/oracle/gauge resource evidence or a Mainnet simulation.

Historical sections below describe the stages leading to this interface. Their
old "native-only/no exports" statements are superseded only for the new explicit
validation package; generic ReceiptVault remains gated against hybrid WASM.

## Confirmed product

September 18 scope clarification: **PERI emissions are zero for the initial
Mainnet release and foreseeable future.** Retain the token reference if needed
for deployment, explicitly configure supply/borrow emission speeds to zero, and
verify that constraint before activation. AQUA conversion and payouts are NOT
disabled. Escrow PERI attribution/payment and incentive-specific liquidation
checkpoints can be deferred for a proven zero-emission, no-legacy-liability
release; they must be completed/reviewed before enabling PERI later. Zero current
speeds alone cannot erase historical accrued obligations. An explicit launch
guard is implemented by the fresh zero-PERI controller; legacy migration and any
subsequent nonzero-emission release still need a separate reviewed policy.

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

## Controller incentive share boundaries — September 18

Controller incentives are separate from Aquarius primary/gauge emissions. The
older lending fixtures left the controller reward token unset, which makes its
accrual entrypoint return early. Those tests did not validate PERI distribution.

New funded-asset/native-controller fixtures enable supply and borrow emissions.
Two regressions failed before this patch: converting rewards after an elapsed
interval reduced the original supplier's 10,000 raw entitlement to 9,065; redeeming
backing instead credited 9,999 to the ordinary supplier rather than 9,065 because
Core saw temporarily credited escrow shares. These figures are controlled test
units, not a Mainnet loss or APY estimate.

Shared backing now invokes an internal engine hook before escrow mint and before
escrow-to-owner temporary credit. Lending constructs authenticated complete hints
from actual old supply, old holder balances and debt. It advances the global
denominator before mint, preserves the escrow account's accrued incentives and
settles the payout owner's ordinary position before any temporary credit. Later
Core withdrawal at the same timestamp cannot retroactively earn on those shares.
Failure of authorization, controller accrual, payout minimum or withdrawal reverts
the entire invocation, including controller state and earlier token conversions.
The lean engine rejects a controller link instead of silently skipping incentives.

Eight new regressions cover both stable settlement indices, repeated minting,
all-escrow supply after principal exit, separate borrower emissions, exact payout
auth, controller failure, auth/minimum rollback and unfunded/funded controller
claims. Actual SAC cash/native controller/strategy/JRM; MOCK pool/oracle. Existing
250-entry research envelope and other SDK limits remain enabled. No compiled
stack or Mainnet claim follows from this coverage.

**This is not yet end-to-end escrow PERI ownership.** With positive emissions,
the controller retains a
liability to the receipt account, not historical backing-unit owners. After all
units exit, that liability still exists. Permissionless `claim_all` can send real
PERI to the receipt outside its own coordinator; current reward cash snapshots
alone cannot safely attribute it. Never reset this liability, assign it to new
unit holders, count it in principal NAV, or classify it as an admin donation.
An ownership-preserving pending-incentive ledger and reconciliation for unsolicited
payments must precede PERI-enabled activation. The later user-approved zero-PERI
release can defer that functionality only with verified zero emissions and no
unresolved legacy incentive liabilities. Underfunded controller accrual is not cash.
Missing/archived incentive indices and late controller attachment also require a
reviewed restoration/migration policy, not reconstruction from current weights.

### Production release boundary (new validation interface implemented above)

| Operation | Required boundary before export |
|---|---|
| Deposit, withdraw, transfer, transfer_from | Complete pool checkpoint plus lending engine, including all managed-cash/delta and historical-ownership checks |
| Borrow | Complete pool checkpoint, shared LP controller/JRM, managed-only liquidity and collateral checks |
| Repay | Keep risk-reducing cash-for-debt path independent of pool reward availability |
| Seize, repay_on_behalf | Controller must settle pre-mutation supply/borrow incentives for every affected account; receipt calling back into the active controller would reenter it |
| Backing mint/payout | New internal checkpoints plus still-pending historical escrow PERI attribution and payment reconciliation |
| Compound, recycle, reserved payout | Complete retained-stream coordinator and actual cash gates; no externally selected callback or accounting hints |
| Init, migration, admin, upgrades | Atomic mode/binding checks, old debt/incentive preservation, strategy binding and legacy harvest exclusion |
| Generic margin, flash-loan or token mutation helpers | Do not inherit exports accidentally; explicitly support with complete accounting or exclude |

Current controller liquidation invokes `repay_on_behalf`/`seize` without those
pre-mutation incentive checkpoints. Existing no-retroactive-liquidator test uses
a fresh recipient index and does not establish conservation for already-indexed
borrower/liquidator/fee accounts. Resolve and test this in the controller call
frame, including repayment-only liquidation, before enabling the LP public ABI.
This patch does not change liquidation or generic production entrypoints. This
incentive-specific work is deferred under the later zero-PERI release constraint;
actual loan liquidation, collateral health and Aquarius reward ownership remain
required. Existing core settings and deployed LP addresses are in addresses.md;
they are reference values, not automatic LP risk-parameter approvals.

## Next release gates

1. Complete production coverage of the native all-stream/recycled/outage lending
   integration, including historical escrow controller-incentive ownership,
   unsolicited payment reconciliation and liquidation accrual IF PERI is enabled.
   For the approved zero-PERI release, instead enforce zero emissions and reconcile
   legacy liabilities; prevent an unreviewed nonzero-emission configuration.
   Native share mint/burn ordering is fixed; full incentive ownership is not. Preserve
   deferred owner claims without assuming a reward swap can always run.
   Inability to observe rewards must not silently reassign rights; liquidation
   liveness under a total reward-data outage remains an unresolved design gate.
2. Validate real strategy-funded borrowing, repayment/redeployment, loan-backed
   withdrawal and post-unwind health. Exercise simultaneous users, bad debt,
   LP losses, fees, zero-cash states and liquidation price/quote outages.
3. Validate the new explicit receipt ABI and native legacy-harvest fence against
   the full compiled strategy/pool stack. No generic lending selector or standalone
   helper may bypass the complete coordinator. Preserve the release guards.
4. Replace temporary parity-alias collateral valuation with reviewed depeg-aware
   pricing/freshness behavior. Collateral factors are now approved at50% XLM and
   80% PYUSD/USDC; apply them only with the remaining activation safeguards.
   Debt caps, cash buffers, rate curves and liquidation policy still need final
   configuration review; fixture numbers are NOT production approvals.
5. Rework the migration from the existing LP pilot controller into the NEW
   controller, preserving balances, strategy binding, old debt and incentive
   obligations. A new controller does not make old obligations disappear. The
   old supply-only migration draft is research, not the approved migration plan.
6. Reproducible compiled candidates, exact-WASM/auth/resource and restoration
   tests, final Almanax review, governed timelocks and a bounded Mainnet canary
   precede borrowing activation. No changes to the other market group or treasury
   signature policy are part of this integration.
