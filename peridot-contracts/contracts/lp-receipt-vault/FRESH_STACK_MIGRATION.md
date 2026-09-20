# Fresh LP lending stack — approved migration scope

September 18, 2026. The user approved moving the small canary positions, not
executing an unvalidated release. This supersedes the old supply-only migration
proposal in `MIGRATION.md` for this rollout. **No migration has been executed.**

## Scope and destination

Only the three pilot receipts listed in `config/lp-lending-mainnet.json` are in
scope. Withdraw to deployer
`GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL`, settle their existing
pooled reward economics, and deposit the actual proceeds through new Peridot LP
lending receipts. The receipts, not the deployer, fund their bound strategies.
New receipt/strategy/controller addresses are expected; bindings are immutable.
No core/DeFindex position, treasury balance, Margin configuration, signature policy
or repeated September16 withdrawal is authorized by this migration.

One fresh LP-only controller permits cross-collateral borrowing inside the group.
Target collateral factors: XLM50%, PYUSD80%, USDC80%; PERI emission speeds zero.
PYUSD and USDC use separate settlement strategies on the same concentrated pool.
Existing half-width40 range policy remains the target. Borrowing activation is a
separate final step after validated deployment, funding and canary checks.

## Preconditions before spending or moving funds

1. Reproducible production-admin artifacts, explicit export review, compiled
   lending/liquidation/reward/outage tests, exact pool and oracle/route/gauge tests,
   restoration checks and final scoped Almanax review. Current validation builds
   reject Mainnet initialization and must not be installed on Mainnet.
2. Resolve yXLM pricing. Native Reflector now quotes PYUSD in USDC; the new
   `CrossQuoted` source converts this via external USDC/USD. No equivalent yXLM
   feed was observed. The user approved a keeper-published observation source;
   local implementation and candidate thresholds are in the pricing handoff.
   Actual SDEX liquidity/liveness and exposure review still apply; a same-pool spot
   price or parity alias is not a substitute. Set required-observation dependencies
   on all three settlement assets and do not retain yXLM parity symbol aliases.
3. Record the approved uncapped supply/borrow policy, rate curve, cash buffer, liquidation
   parameters, reward routes/floors, keeper cadence and funded fee budget. The
   collateral-factor approval does not approve arbitrary exposure or upload fees.
4. Verify admin authority and any applicable timelocks. No governance bypass.
5. Freshly verify all three reciprocal bindings, asset IDs, total supply equals
   deployer shares, every debt/reserve/fee/incentive obligation, and controller
   membership. Current supply equality alone cannot exclude historical claims,
   missing archived records or liabilities. Restore relevant entries first;
   stop if another owner's rights or unknown liabilities appear.

## Ordered cutover

1. Save a public-address/WASM/configuration manifest and balances. Pause new legacy
   deposits/borrowing and coordinate its keeper to prevent concurrent harvest or
   rebalance during settlement. Keep legitimate exits and debt repayment possible.
2. Inventory primary, gauge, pair-fee and already-held reward cash per strategy.
   Settle under legacy pooled rules before withdrawing. Legacy `harvest` has soft
   claim failures and normally reinvests its proceeds: a successful call or zero
   return does **not** prove complete reward settlement. Reconcile actual transfers,
   pool claimable balances, strategy shares/NAV and residual cash. Never reassign
   uncollected rewards to future depositors or silently discard dust/claims.
3. Freshly simulate each full canary withdrawal to the deployer, check asset,
   recipient, share burn, fee and resource limits, then submit once and confirm
   before the next step. Old receipt withdrawal does not expose the new receipt's
   user minimum-output selector: simulation is not an on-chain minimum guarantee.
   Concrete enforceable slippage/price protections must be checked before signing.
4. Reconcile wallet deltas and old supply/debt/strategy liabilities. If any leg
   fails, leave its proceeds in the deployer wallet; do not retry an unknown hash
   or use unrelated wallet/core funds to conceal a shortfall. Remove old markets
   only through normal zero-liability/membership checks, never a force shortcut.
5. Deploy and validate fresh controller/JRM/router, receipts and strategies;
   register only these three markets, zero PERI emissions, approved collateral
   factors and the user-approved zero caps (unlimited supply and borrowing).
   Small canary transactions do not bound total public exposure. Bind each receipt and strategy in both directions
   and enable the reviewed reward mode before receipt activation. Publish verified
   addresses with explicit group labels, never replace core addresses accidentally.
6. Deposit only reconciled migration proceeds via each new receipt as deployer,
   with checked minimum shares/output and fee bounds. Verify minted pTokens,
   managed cash, strategy shares, actual concentrated liquidity and reward ownership.
7. Configure the reviewed keeper for the new addresses; prove funded continuous
   maintenance and deferred-reward retry behavior. Tiny authorized borrow/repay and
   withdrawal canaries precede public borrowing/frontend enablement. Preserve old
   evidence and address history rather than deleting it.

## Read-only evidence and restart rules

`node scripts/preflight_lp_lending_mainnet.mjs --migration-preview` only queries
RPC and simulates unsigned transactions. It cannot sign or submit. Its harvest
and withdrawal previews are independent current-state simulations, not an atomic
harvest-then-withdraw rehearsal. An error, restoration preamble, unexpected signer,
recipient or asset blocks execution. There is intentionally no execution flag.

A future executor must journal public transaction hashes and confirmed deltas,
explicit network/source/target IDs and bounds, never seeds or signed envelopes.
Resume by querying any previously submitted hash before rebuilding a transaction.
Preflight evidence expires as ledger state changes and must be refreshed at cutover.

## September20 diagnosis and preliminary upload estimate

Read-only preflight at16:44UTC (ledgers64527376–77) verifies all three strategy
hashes remain `00a1e9097339cbd1ee194a7a7f938d8d72918a7c95f89520c23c5ea2d4b8162e`
and their admin remains the deployer. XLM's range is[20,100]; its oracle is
CAFJZQ...34DLN, max-pool divergence200bps, execution slippage100bps. The failed
withdrawal trace reads the same Other(XLM) feed for both tokens (legacy parity
alias). The 242708102raw yXLM liquidation quote returns236759670raw XLM,
below the parity guard's floor237853939. The trace stops immediately after that
quote; this agrees with the source's oracle-divergence rejection. Production
WASM strips the panic text, so the trace itself only reports a VM trap.
This is not evidence of missing assets or permission to widen the guard. A
validated non-parity price/recovery plan is required before migration. Stable
withdrawal previews still return4.8427816PYUSD and4.8449612USDC; nothing submitted.

`node scripts/estimate_lp_upload_fees_mainnet.mjs` verifies the four validation
WASM hashes and only simulates unsigned uploads. At ledger64527360 these total
328.8256722XLM including nominal100-stroop base fees: receipt120.3278453,
controller81.7296050, strategy87.5321065, router39.2361154. These are NOT final
production bytes or a complete budget. Excludes contract creation/initialization,
JRM, configuration, migration, canary, inclusion surge and safety margin. No
upload occurred. A400XLM total ceiling was proposed to the user, not yet approved.
