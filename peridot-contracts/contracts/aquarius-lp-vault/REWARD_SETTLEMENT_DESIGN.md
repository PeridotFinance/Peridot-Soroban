# Hybrid reward settlement proposal

Date: 2026-09-13. Status: product direction recorded; NOT implemented, audited,
or deployed. This is an accounting specification, not an upgrade-ready algorithm.

## Intended behavior

Retain threshold-gated automatic compounding. On withdrawal, also settle the
withdrawing holder's eligible, not-yet-compounded reward portion into the market's
settlement asset: XLM, PYUSD, or USDC respectively. This does not introduce arbitrary
output-token selection or merge the two stablecoin settlement strategies.

Only that holder's eligible reward portion should be swapped for the withdrawal;
do not call a whole-vault harvest and describe it as user-only settlement.
Already-compounded rewards are included in share backing and must not be paid again.
NAV/cache maintenance and concentrated-position rebalancing remain independent.

If a reward route is blocked or the amount is dust, the intended fallback is to
retain an attributable, later-claimable reward balance rather than confiscate it.
Principal exit should not require a successful reward swap. This does not bypass
existing principal redemption, oracle, liquidity, or minimum-output checks.
The implementation must prove that fallback preserves accounting atomically;
silently skipping a failed ownership checkpoint is not a safe fallback.

## Integration points confirmed in the current source

- ReceiptVault owns the user pToken balances. The bound Aquarius strategy sees
  ReceiptVault as its holder, not the individual withdrawing user.
- ReceiptVault `withdraw` may use idle cash without calling the strategy. Reward
  settlement must therefore begin at the receipt layer even for an idle-only exit.
- ReceiptVault `deposit` prices shares before receiving the new deposit. A reward
  ownership checkpoint must respect this boundary and exclude incoming capital
  from the historical accrual period.
- Both `transfer` and `transfer_from` pass through `transfer_internal`. There are
  also direct pToken moves in `seize`, including the fee recipient. Every balance
  mutation needs an explicit ownership rule; adding hooks only to deposit and
  withdrawal is insufficient.
- Existing `accrue_user_rewards` calls concern controller incentives, not AQUA.
  Keep those ledgers and behavior separate.
- Strategy `harvest` currently claims and converts pooled rewards, then deploys
  available settlement cash. There is no per-user reward liability ledger.
- Unconverted AQUA is excluded from current strategy NAV. A late depositor must
  not acquire historical AQUA merely by buying shares before the next harvest.
- Final-holder exit burns all pTokens and unwinds the strategy. Outstanding reward
  claims must remain valid after supply reaches zero and after later deposits.

## Accounting requirements before contract implementation

1. Define separate ownership for invested backing, unconverted rewards, reserved
   exit claims, and converted proceeds. Each token unit can back at most one claim.
   Reserved amounts must be excluded from deployment, ordinary NAV, and future
   harvest consumption unless the corresponding liability is settled atomically.
2. Specify a bounded checkpoint/index mechanism, including how rewards still
   unclaimed in Aquarius are measured at balance changes. Indexing only the tokens
   already held by the strategy does not prevent late-depositor capture.
3. Specify transfer semantics explicitly. Recommended: crystallized historical
   reward claims remain with the earning account; transferred pTokens participate
   in future accrual. Reconcile this with pooled compounding before adopting it.
4. Prove how an account's unconverted entitlement becomes compounded backing
   without being redistributed to later holders or counted twice. An ordinary
   reward-per-share index alone does not establish this. Specify normalization or
   another bounded mechanism with worked multi-user examples before coding.
5. Define partial-withdrawal allocation, repeated-withdrawal rounding, self-transfers,
   zero-supply epochs, donation handling, and deterministic remainder ownership.
   Use checked arithmetic and demonstrate conservation across all transitions.
6. Retain old token claims when the primary reward token, route, or gauge changes.
   Bound active reward-token work per call; governance must not erase liabilities.
7. Authenticate receipt/strategy settlement calls and user claims. Apply existing
   route floors and slippage protections, a user-specified settlement minimum where
   appropriate, and actual input/output balance-delta verification.
8. Make the feature explicitly opt-in for these LP receipt markets. Existing
   lending/DeFindex markets, account-health snapshots, and MarginController must
   not acquire new reward-contract dependencies or storage-footprint requirements.

## Verification and release gates

Before production implementation is considered complete:

- Demonstrate two-holder timelines spanning accrued-but-unclaimed rewards, a new
  deposit, transfers, keeper compounding, partial exit, and final exit. Reconcile
  assets and liabilities at every boundary, including rounding and dust.
- Test idle-only and strategy-funded withdrawals; blocked/missing routes; partial
  multi-token conversion; failed checkpoints; full exit with deferred claims;
  re-entry after zero supply; token rotation; and unauthorized settlement calls.
- Test that a keeper harvest cannot consume reserved claims, and that withdrawing
  after a harvest cannot receive the same reward twice.
- Exercise all pToken mutation paths and preserve legacy multi-market health and
  borrowing behavior. Do not modify unrelated Margin work for this feature.
- Run exact deployed-WASM migration tests and realistic transaction simulations
  for auth trees, storage footprints, execution budgets, and atomic rollback.
  If the combined path does not fit, design an explicit resumable claim flow;
  do not silently reduce reward settlement guarantees.
- Review and scan the final contract/keeper range, then obtain scoped upgrade
  approval, respect timelocks, and verify deployed hashes and invariants.

No production reward mode, keeper release, contract, or governance setting changes
as a result of recording this proposal. The separately reviewed keeper freshness
candidate remains a separate release task.
