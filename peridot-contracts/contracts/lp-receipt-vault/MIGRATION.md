# LP receipt migration specification — native draft

Status: September 17, 2026. **Not an approved migration or deployable release.**
Applies only to the three isolated Aquarius settlement receipts, not core
DeFindex/lending receipts or Margin. No new public ABI or activation method exists.

## Decision and invariants

Prefer an **in-place receipt upgrade**, subject to all gates below. Aquarius
`set_receipt_vault` is one-time and strategy shares are non-transferable. Merely
deploying a new receipt cannot take custody of the existing strategy. An alternative
is a separately approved, user-authorized exit/redeposit into a new strategy; that
is not automatic migration and may realize slippage/fees.

Preserve the receipt address, settlement asset, strategy address and reciprocal
binding, admin, raw pToken balances, supply, allowances **including expiration**,
metadata, caps and tracked cash. Do not burn/re-mint or scale shares to fix display
decimals. Unreadable metadata is a blocker, not permission to choose decimals.
Do not reset `TotalDeposited` to supply or count cash donations as managed capital.
Strategy share custody, pool liquidity, NAV methodology, route floors and price
guards must also be reconciled. An accounting transition itself must transfer no
value between holders. Execution costs/slippage of preparatory unwinds are separate
and must be disclosed in the approved plan.

Do not create escrow shares or reward units at activation. A genuinely new reward
ledger starts at epoch/index/raw/reserved/units zero with ordinary weight equal to
existing raw pToken supply, provided escrow is empty. Existing holders can then
checkpoint lazily; new deposits/transfers must checkpoint their zero/old weight
before changing balances. This only works for a truly new history. Never reset
an existing or archived registry, stream, account, closed epoch, backing or exit
intent; use restoration or a separately reviewed versioned schema migration.

## Gates which local zero totals do not establish

1. **External lending obligations.** Reconcile both aggregate and per-account /
   per-position debt, principal mirrors, bad debt, reserves and admin fees using
   authoritative state and archive restoration. An address list supplied by an
   admin is not proof of completeness. Nonzero rounding dust is not zero. Missing
   entries are unknown unless absence is proven against the exact legacy schema.
2. **Controller retirement.** Discover external market registries, entered-user
   membership, collateral use, pause expiries, boosted-strategy ownership and
   Margin references, even if the receipt's link is absent. CF=0 and paused
   borrowing are insufficient. Stop supply/borrow incentive speeds with the old
   accounting intact, preserve final indices and settle each affected supplier's
   rights (or retain a bounded, reviewed legacy claim adapter). Do not delete
   controller state or reset accrual indices to obtain a clean-looking snapshot.
3. **Nonempty-market delisting.** Current `remove_market` requires zero pToken
   supply and no entered users. It therefore cannot retire a funded receipt in
   place. `force_remove_market` is an emergency governance action, not an automated
   migration shortcut; it does not prove incentive settlement. Select and review
   an explicit nonempty retirement/compatibility protocol before activation.
4. **Legacy yield.** Enumerate actual primary/gauge inventory, unclaimed pool
   rewards, pair fees, settlement cash and routes. No live historical per-user
   reward ledger exists to import. Do not retroactively assign accumulated AQUA
   to new individual claims or label it a donation. Suggested policy, **requiring
   approval**: finish legacy compounding under existing pooled-share economics
   before establishing the new boundary. Otherwise specify a separately audited
   entitlement allocation; neither policy is silently selected by this draft.
5. **New interface and restoration.** Every share mutation, including
   `transfer_from`, must use the all-stream coordinator. Required archived state
   must be restored before inspection, not replaced with zero. Old lending,
   seizure and Margin bypass selectors must never remain callable after the
   transition. Preserve timelock/admin security with a reviewed upgrade interface.

`migration::snapshot` reads only local state; `check_local` reports necessary
conditions, **never readiness or an activation permit**. It deliberately rejects
missing canonical mirrors, fees/debt, cash deficits, escrow and linked controllers.
It cannot discover absent archived records, external membership, individual debt,
reward history or strategy liabilities. No operator boolean clears these gates.

## Proposed staged cutover (not implemented)

| Stage | Required evidence and permitted behavior |
|---|---|
| Observe | Pin network, contract IDs, legacy/candidate code hashes, schema and a consistent ledger. Inventory obligations without changing live state. |
| Prepare | Reviewed governance installs a resumable migration fence. Pin the receipt/strategy pair, target hashes, nonce and state version. Keep old accounting authoritative. |
| Drain old yield | Stop new LP reward accrual by unwinding only as approved; settle every old claim/fee stream and reconcile actual cash. Receipt share weights must remain frozen for this boundary. A failed route, unknown gauge or residual dust blocks completion, not silently forfeits claims. |
| Reconcile | Verify reciprocal custody, all backing, legacy reward disposition, external retirement/incentives, metadata, restored records and zero legacy liabilities. Revalidate after every state-changing operation; a prepared snapshot is not reusable proof. |
| Activate | One atomic, governed transition checks nonce/version/hash and all on-chain fence conditions, preserves existing shares, initializes genuinely new streams at zero and sets the marker last. Replays and failures leave the old version intact. |
| Canary | Deposits remain paused. Validate old-holder reads, authorized tiny exit, retained allowances, reward ownership and actual payout before approving reopening. |

A controller deposit/redeem pause alone is **not** the fence: transfers, delegated
transfers, admin liquidity actions, harvest/sweep, strategy deposits/withdrawals,
reward configuration and other callbacks may still mutate the boundary. Existing
breakers also expire. The fence must enforce the permitted operations in code,
not depend on a stopped keeper, a human-maintained list or indefinite pauses.
NAV/cache maintenance may continue only where proven not to alter entitlement.
No fence is implemented by this work; do not use this table as a runbook.

Before activation, safely abort through the old accounting after revalidating
state; never erase earned yield. After activation, a blind downgrade is unsafe:
the old binary cannot protect hybrid escrow/reservations. Use a reviewed forward
repair; establish rollback feasibility before the first hybrid operation.

## Production-interface boundary

The public surface remains a design, not generated exports:

- Read balance/supply/metadata/allowance and principal/reward/exit status separately.
  A reward receivable is not spendable settlement cash or principal NAV.
- Deposit, transfer and delegated transfer must checkpoint **all** retained
  streams, then apply actual share changes. Preserve old allowance amount/expiry
  semantics; allowance spending does not transfer a holder's already-earned rewards.
- Principal exit keeps the proportional actual-custody path and user minimum;
  deferred reward settlement/payout is separate and authenticated to the earner.
  Exit requests are retryable signed intents, not transferable claims or locks.
- Keeper claim/convert/recycle methods must have bounded scopes and actual-delta
  checks. No keeper-chosen beneficiary, weights, arbitrary swap minimum or right
  to spend user shares. Keep economic thresholds separate from mandatory freshness.
- Admin rotation retains all historical streams and staged custody checks. No
  legacy permissionless harvest/sweep or direct principal primitive may bypass
  the coordinator. Retired gauge enumeration, fifth-token policy, dust and pair
  emissions must be completed before this interface is exposed.
- Decide whether controller discovery/getter compatibility remains temporarily
  necessary. Do not simulate healthy lending behavior with dummy zero getters.

## Executable evidence in this change

`cargo test -p lp-receipt-vault --locked migration_` uses actual native generic
ReceiptVault initialization/deposits and real SAC transfers. A **test-only**
storage marker insertion in a controller-free, strategy-free scratch fixture
models an in-place accounting read, not a deployable migration. It preserves raw
balances, allowance amount/expiry, metadata and tracked cash; full exits leave
donations untouched and deposits stay paused. Canonical zero mirrors are explicitly
seeded fixture state, not inferred from generic initialization.

Negative tests cover one-raw-unit liabilities, missing mirrors/managed cash,
controller links, escrow, deficits, pending admin/flash state, replay and attempted
share normalization. Another test intentionally puts individual debt behind zero
aggregates: the local diagnostic cannot detect it, and activation remains absent.
The zero-epoch reward test proves cold legacy holders receive only **newly
received** emissions; preexisting raw rewards stay unassigned and are a production
cutover blocker, not waived liabilities.

These tests use mock authorization and idle-only custody. They do **not** prove
compiled upgrade compatibility, old production storage layout, controller
retirement, actual gauge behavior, strategy-funded migration, complete archive
restoration, transaction fit or exact auth trees. The original WASM compile gates
remain, no keeper changes are made, and no Mainnet transaction is signed.

Next implementation gate: a reviewed nonempty controller-retirement/incentive
mechanism and old-yield policy, then the fenced versioned transition with exact
legacy/candidate WASM tests, realistic resources, complete auth and rollback
tests, security scan, governance approval/timelock and a bounded canary.
