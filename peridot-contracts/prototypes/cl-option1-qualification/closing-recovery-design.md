# Isolated funded settlement for Closing and staged Liquidated positions

This is an implementable local prototype plan, not production policy or a claim
of completed coverage. Stage1 remains incomplete. Keep the validation feature
and Mainnet rejection, borrowing disabled, production maintenance margin unset,
all claim-first boundaries, current limits and shared00 unchanged. No signing,
broadcasting, secrets or production edits are needed for this plan.

## Review conclusion

`pending-recovery.md` correctly separates owner-funded recovery from independent
funding and correctly identifies the remaining Closing/Liquidated gaps. Its new
expired-PendingOpen root does not cover those states. Existing staged liquidation
is not equivalent to externally funded full repayment: `finish_liquidation_v3`
calls `absorb_margin_bad_debt` when proceeds do not clear debt. That extinguishes
accounting debt through loss recognition; it does not pay the missing assets.

The narrow next patch should add **one optional atomic settlement root**, with
state-specific asset normalization and eligibility checks. It should neither
swap collateral nor create another persisted financial stage. Its permitted
seizure and owner residual transfers occur only after full current debt has been
paid in the same invocation. A failed transfer, claim hook, valuation, payment
or state check must roll the whole invocation back, including earlier use of
held proceeds. No governance reward attribution or borrower signature is needed.

## Exact state and per-position asset ledger

Read actual current debt from the debt receipt after `update_interest`, not
`Position.debt_shares`, a pending quoted debt or a pre-accrual cap. Let D be that
debt, P held debt-asset proceeds, U held collateral underlying, and T still-held
position receipt pTokens. Quantities come from the following records; a global
controller token balance is only a sufficiency check, never the attribution
source. Controller balances also contain other users' cash, fee backing and dust.

| Persisted state | P | U | T | Necessary consistency checks |
|---|---:|---:|---:|---|
| `Closing`, withdrawn/unconverted |0|`PendingPerpsClose.collateral_underlying`|0|`debt_amount==0`, `received_debt_asset==0`, remainder absent/zero; historical `Position.collateral_ptokens` was burned and is not another asset. |
| `Closing`, converted |`PendingPerpsClose.received_debt_asset`|`PendingPerpsCloseRemainder(id)`|0|`debt_amount>0`; do not reuse original `collateral_underlying`. Long ordinarily has U=0; Short can retain U>0. |
| `Closing`, partial finish committed |Remaining `received_debt_asset`, often0|Exact remainder|0|Stored `debt_amount>0` is a conversion marker, not current D. Debt may have accrued after the partial payment. |
| `Liquidated/Started`, inherited from Open |`PendingLiquidation.received_debt_asset` (normally0)|0|`Position.collateral_ptokens`|No nonzero `PendingLiquidationCollateralUnderlying`; T must still be backed by controller receipt custody. |
| `Liquidated/Started`, inherited from Closing |Carried `received_debt_asset`|`PendingLiquidationCollateralUnderlying`|0|`begin_liquidation_v3` explicitly zeros position pTokens when inheriting Closing. Reject ambiguous simultaneous U>0/T>0. |
| `Liquidated/CollateralConverted` |`PendingLiquidation.received_debt_asset`|0|0|Assert held collateral key absent/zero and position pTokens0, rather than silently ignoring inconsistent residual backing. |

Validate mode `PerpsV3`, owner equality across pending/position records, side,
position/debt/collateral vault bindings and all quantities before payment. A
missing archived required record must be restored/read successfully or fail;
do not substitute zero. The legacy enum `PendingLiquidationStage::Repaid` is not
produced by this V3 state machine; reject it explicitly and record it as a
separate compatibility/migration dependency if any live V3 instance contains it.

If collateral asset equals debt asset, normalize attributable U into P exactly
once and set U=0. Never pay a funder to acquire debt currency that can already
repay its owner's debt. Keep same-asset residuals separate from unrelated cash.
For a T-backed state, use fresh exchange rate and the existing pToken transfer
path; do not redeem T merely to manufacture P or force a venue interaction.

## Minimal additive ABI and eligibility

Proposed validation-only ABI:

```text
settle_stalled_funded_v3(
    payer: Address,
    id: u64,
    max_top_up: u128,
    min_seize_ptokens: u128,
    min_seize_underlying: u128,
) -> (paid_from_proceeds, paid_by_payer, seized_ptokens, seized_underlying)
```

The root fixes recipient=payer and residual recipient=stored owner; no arbitrary
recipient is accepted. Require a distinct non-owner payer and bind all bounds
to its actual narrow authorization tree. Permit zero max/min bounds for a
proceeds-covered completion or explicitly altruistic payer; reject a minimum
in the wrong collateral unit. No funder entitlement is created by merely calling
the root. Exact maximum transfer/refund is retained if a top-up is required.

Eligibility is explicit, not just `status != Open`:

- **Unhealthy Closing:** allow independent settlement at any time after fresh
  health validation against the normalized P/U/T ledger. This reuses the current
  ability to begin liquidation of an unhealthy Closing position.
- **Expired converted Closing with a deficit:** require `now > expires_at` and
  current proceeds insufficient for D. The existing stalled-close path already
  permits healthy residual recovery with zero incentive. Keep exact-deadline
  rejection for this timeout branch.
- **Healthy expired withdrawn/unconverted Closing:** this is a real missing
  policy decision. The current timeout method redeposits; it does not authorize
  involuntary in-kind termination. The local candidate may explicitly choose
  timeout-based zero-bonus full settlement to remove the redeposit dependency,
  but it must label that choice unadopted and test it. Without that choice this
  row remains uncovered; do not call the gap solved by the converted branch.
- **Liquidated Started/CollateralConverted:** respect the selected liquidator's
  existing exclusivity through `takeover_after`; the selected actor can settle
  now, another non-owner can settle strictly after the deadline. Use the existing
  legacy deadline fallback only where that compatibility behavior is already
  required. The recorded liquidation was admitted by `begin_liquidation_v3`;
  price recovery does not invent a requirement to reconstruct that prior state.
  Current prices still bound seizure. Do not restart/extend the takeover deadline
  on a failed attempt or require the absent borrower to hand over ownership.
- **Proceeds already cover D:** completing repayment and returning all remainder
  without payer seizure is permissionless subject to existing settlement locks;
  the current converted Closing `finish_close` already has this property. For
  Liquidated positions preserve the liquidator/takeover restriction above.

The optional new root should use **zero bonus** in every branch for the first
bounded prototype. A payer opts into this bound; existing incentive-bearing
routes remain separate. This avoids silently giving a new funder a reward on
the original owner's proceeds or transferring a prior actor's exclusivity.
It does not guarantee a willing payer, especially for zero/insufficient residual
collateral. A future bonus would need explicit rules for payer contribution,
previous liquidator work, health at settlement and residual fee priority.

## Atomic repayment and asset partition

1. Apply validation-network, identity, state, takeover, lock and pause checks.
   Accrue current debt; refresh required prices/receipt exchange rate. Establish
   eligibility and the partition before making token transfers. A debt-zero
   position pays/seizes nothing; if this root supports cleanup, return only
   attributed remaining assets under the same takeover/lock conditions.
2. Compute `C=min(P,D)`. Repay C from the controller **attributable proceeds** via
   an exact contract-authorized receipt subcall. The existing
   `repay_margin_from_controller` is reusable only after this root has accrued
   interest and with measured exact controller/debt-vault balance deltas. Assert
   paid=C and resulting debt `E=D-C`; same-ledger accrual must not silently leave
   a residual. This first payment must not persist if the rest of the root fails.
3. If E>0, require `max_top_up>=E`, then invoke existing
   `repay_full_for_margin(id,payer,max_top_up)`. The debt receipt transfers the
   fixed signed maximum and refunds its excess to payer. Require returned
   paid=E and authoritative final debt exactly0. Assert payer net spend and debt
   vault cash delta reconcile. If E=0, make no payer transfer and grant no seizure.
4. Compute payer's maximum permitted collateral from **E only**, not D or
   max_top_up. With current rational prices `q_debt`, `q_collateral`, zero bonus:
   `V=floor(E*q_debt)`; `S_U=min(U,floor(V/q_collateral))`, or
   `S_T=min(T,floor(floor(V/q_collateral)*SCALE/exchange_rate))`. Use checked
   multiplication and borrower-favoring floors as the existing funded route
   does. Require the chosen minimum. Never seize both T and U from the same
   attributed position; state normalization has already selected one form.
5. Transfer permitted S to payer with the exact existing receipt/CL recovery
   hooks, then return U−S_U / T−S_T and P−C to their original owner. P−C belongs
   entirely to owner (subject only to the existing applicable executed-close
   fee policy), not to the payer. If E exceeds collateral value, the payer
   knowingly accepts the bounded in-kind amount; no reserve, another user's
   balance or bad-debt absorption silently funds the shortfall.
6. Reconcile per-position assets and all physical deltas; settle or explicitly
   preserve applicable fee accounting; only then clear exact position/pending
   keys and remove the user's position index. Historical reward/waiver/quarantine
   records are not position cleanup. Publish actual C, E, refunded maximum,
   seizure, owner residual, fee treatment and prior stage. Repeated execution
   must fail with no payment, release or second reward attribution.

Example: D=100, P=70, remaining collateral value50 gives C=70, E=30. A payer
signing max40 spends30 net/refund10 and receives at most30 of collateral;
the owner's20 residual remains theirs. Seizing100 or40 would overcompensate
the funder. If P=110, C=100/E=0 and owner receives10 plus all remaining
collateral; no payer seizure or liquidation bounty is created by this root.

## Custody, rewards, fee entitlements and pauses

For T-backed collateral, reuse `cl_receipt_reassign(..., recovery=true)`, exact
margin-withdraw authorization and receipt `transfer` from the active funded
path. Return residual T as owner free margin with `accrue_user_fee` **before**
increasing that owner's free pToken weight, or transfer to the owner through the
same custody-safe path. Choose one disposition explicitly and test its history.

For U-backed CL shares, use `cl_prepare(..., recovery=true)` followed by the
exact physical transfer for **each** ownership boundary, including the owner's
residual release. Never use bare transfer as a substitute for beneficiary
accounting. Recovery may quarantine unavailable claim/pending-view evidence;
it never waives the borrower's existing entitlement. Payer must not receive
earlier rewards; exited borrower rights survive. Existing upstream claim-first
boundaries remain inside measurement. No holder enumeration is introduced.

Avoid `credit_margin_underlying` for the residual U/P recovery path: it deposits
again, can be paused or claim-dependent, and would reintroduce the very recovery
dependency being removed. A direct wallet return is safe only with the CL hooks
above and exact token deltas. Existing reserved fee pTokens/underlying remain
controller-backed and must not be swept as position residual or donated dust.

Fee distinction matters. `set_close_execution_fee` records a computed term;
`reserve_close_fee` later allocates entitlement from surplus **after debt zero**.
Opening fees already reserved in Open/Closing remain owed and are not refunded
by this path. For an unswapped Closing, do not invent a close fee. For an already
swapped Closing, the implemented local candidate explicitly does **not charge
the contingent, still-unreserved close fee** on forced recovery. It records that
amount in the settlement event and clears only the position fee terms. This is
an unadopted research policy, not a finding that an earned reserved claim can
be erased. Existing global reserved fee entitlements and backing are untouched.
The normal voluntary close remains unchanged. Already-Liquidated settlement
also preserves the existing no-new-close-fee behavior. A prior `begin_liquidation_v3` clears
Closing records and can discard an unreserved fee term on cleanup; do not claim
equivalence to the Closing fee path without deciding this distinction.

Borrowing/new-open pauses and insufficient cash reserves must not prevent debt
repayment or this collateral release. Keep existing explicit liquidation pauses,
margin lock/controller bindings and contract reentry restrictions. A pause
override is a separate unapproved policy dependency, not an implicit rescue
exception. Likewise, no cached/static fallback may substitute for fresh trusted
seizure valuation. Pure no-seizure proceeds repayment can avoid inventing an
oracle dependency where amounts alone suffice; any unhealthy admission or
value-based seizure still requires the trusted fresh inputs. Primary oracle
outage, stale CL NAV, a configured liquidation pause or an asset that refuses
transfer can still block completion. No unconditional liveness claim is honest.

## Small patch boundary and concrete proofs

The isolated overlay needs a new `funded_stalled_settlement.rs` module, one
validation-gated public entrypoint in `contract.rs`, a module declaration in
`lib.rs`, and narrowly exposed helpers if their current `perps.rs` visibility
prevents reuse. Do not change stored structs, create a new stage or alter normal
begin/swap/activate sequencing. Reuse `repay_full_for_margin` (it already exists),
the exact CL custody APIs and current receipt transfers; do not invent setters
or valuation methods. Add only a purpose-built `RecoveryAssets` in-memory struct
and normalized-state enum, not new persistent aggregate custody amounts.

Implement and meter a bounded table of actual compiled transitions:

| Case | Required entry path and result |
|---|---|
| Withdrawn Closing | Real prepare+withdraw; expired healthy rescue and unhealthy-before-expiry branches; pTokens burned once; no redeposit/swap; owner absent. |
| Converted Closing | Real Long swap and separate Short partial swap; P used before external top-up; exact refund/residual; nonzero close fee handling. |
| Partial close finish | Let interest outrun earlier proceeds; execute `finish_close` to commit its partial payment; recover current remaining debt with U remainder and no double-use of original proceeds. |
| Liquidated Started from Open | Real `begin_liquidation_v3`; pre-timeout selected actor succeeds, another actor rejects; post-timeout independent payer uses actual receipt T without withdrawal. |
| Liquidated Started from Closing | Real inheritance of U and P; no stale pToken amount seized; Short remainder and prior proceeds conserve. |
| CollateralConverted | Real liquidation swap; P≥D zero-top-up release and P<D voluntary deficit funding; no `absorb_margin_bad_debt` call. |

For each asset form include insufficient max/token balance, wrong owner/caller,
missing nested payer authorization, wrong amount/minimum, stale/zero valuation,
failed post-repayment collateral transfer, liquidation pause, exact deadline,
repeat execution, both upstream reward calls failing, unrelated positions/dust,
nonzero accrued interest, fee entitlements and scaled reward conservation.
Assert rollback of controller/receipt balances, authoritative debt/principal,
all custody/history and TTL when failure follows the initial C repayment.
Record actual reached stages; setup panics are not authorization rejections.

Measure prepared and cold dependencies, all required reward hooks, fresh-value
branches and serialized unsigned evidence under both existing profiles and any
separately authorized Option1 diagnostic. `check_budget.sh` must retain missing
global proofs. The final joined restoration+recovery path, enforcing account
signatures, Core application and live economic valuation remain separate gates.

## Decisions still needed before a production-ready claim

1. Healthy expired unswapped termination authority and before-expiry unhealthy
   takeover of swapped PendingOpen. The pending patch covers only expiry; an
   all-debt-states claim cannot hide that pre-expiry branch or assume a borrower.
2. Executed-close fee priority and the difference between settling Closing
   directly versus first converting it into a Liquidated record. Preserve
   existing earned/reserved claims whatever policy is chosen.
3. Whether liquidation pauses may ever be bypassed, and trusted fresh valuation
   during oracle/venue outages. Current design preserves both restrictions.
4. Economics and available capital for zero-bonus or underwater recovery. A
   permissionless method does not create a willing or funded liquidator.
5. Actual deployed ABI/version, custody hooks, archive availability and any
   unexpected legacy pending-stage records. Do not treat a guessed migration,
   bad-debt absorption or an unmeasured prerequisite as successful repayment.

This plan gives the lead a concrete minimal implementation target. It does not
erase the still-unexecuted rows, decide production policy or claim every
debt-bearing state is presently safe without its owner's cooperation.


## Implemented research checkpoint and exact limits

The isolated `closing-recovery.patch` now implements the normalizer and atomic
settlement under `funded-liquidation-validation`, with the existing Mainnet
rejection retained. It adds no persisted financial stage. The current patch is
layered after both pending recovery overlays; original executed four-state bytes
are preserved separately in `target/cl-option1-closing-build-v2`.

Captured-protocol controlled-state execution v2 passed one test covering four
actual Long transitions: Closing withdrawn; Closing converted; Liquidated
Started with receipt custody; and Liquidated CollateralConverted. External
contributions were respectively 25,000,023;0;25,000,040;0 raw debt units. Existing
proceeds paid 25,000,040 in each converted case. Settlement had no borrower
signature, borrowing was disabled, both upstream reward calls failed, debt
became zero, booked rewards survived and exact reward conservation held after
late checkpoint. The impossible minimum rejected with Contract9101 and exact
ledger/TTL rollback. This is controlled compiled behavior, not signed Core
transaction application or live-state qualification.

| Long settlement | SDK CPU | SDK memory bytes | SDK entry total | Writes |
|---|---:|---:|---:|---:|
| Closing withdrawn |70,314,342|14,801,815|153|50|
| Closing converted |29,961,074|6,544,251|85|26|
| Liquidated Started |70,713,356|14,760,612|176|52|
| Liquidated converted |31,020,985|6,636,152|89|28|

Entry totals are SDK read+write accounting, not unique transaction footprint.
The checker gate remained failed: the separately reported deliberate rejection
is an unrecognized `ok:false` row under the unchanged generic checker. It must
not be relabelled a resource overrun or hidden as a success. Required lifecycle
and transaction proof omissions remain failures. These numbers describe the
five-overlay v2 binary and must not substitute for the final joined six-overlay
binary measurements.

The current test source additionally requests a fifth Long case: real converted
close, then a deliberately artificial 30-year timestamp advance (only 20 ledger
ticks) so actual accrual outruns proceeds; `finish_close` commits a residual
payment and leaves zero proceeds/positive debt. Independent zero-seizure full
repayment must then retire the position with no phantom collateral. This horizon
isolates accounting semantics; it is not an archival, latency or economic model.
That added case and final joined binary are not yet measured at this writing.

CL Short is **unsupported by current guards**, not simply an omitted arbitrary
fixture. `perps.rs::begin_open_position_v3_impl` requires cash-funded Long when
either asset is CL; receipt `borrow_for_margin` rejects custody borrowing. A
Short CL-debt test would require changing those guards and designing the CL-debt
borrow/repay/refund beneficiary hooks. No such change is in this prototype.
The generic normalizer includes Short shapes for design clarity, but this
checkpoint makes no reachable CL Short execution claim. Converted partial
receipt-to-underlying remnants, Liquidated Repaid, stale/missing price and paused
liquidation liveness remain separately disclosed dependencies, not bad-debt
absorption substitutes for repayment.


### Joined six-overlay five-state execution

`target/cl-option1-joined-closing-captured-v1` passed the expanded single test
(249.38 seconds). The partial-finish case left 21,921,918 raw debt units after
using all own proceeds. The independent payer repaid that residual, received
zero collateral and required no borrower authorization. This establishes a
voluntary loss-cover route; it does not establish that a profit-seeking
liquidator would fund a zero-asset deficit. Authorized loss-cover capital remains
a separate dependency. CL Short guards remain unchanged.

The joined V3 is 100,373 bytes, SHA256
`923c66f71ab9d74aa2edde8e1a25a4b0e0f1cc1852b7cb9aef76024d75e5b969`.
The receipt is 88,329 bytes,
`fc1ac7d397428438c58c50ce8930907c5965587aac7cc0f9988025235dcde362`;
the lending controller is 63,411 bytes,
`a55d5740007553d955c26e08efad4289b093c0d85c34a2b0e2af7262eaba0052`.
The final seven-case suite is separate and pending at this writing.

| Joined Long operation | SDK CPU | SDK memory bytes | SDK entry total | Writes |
|---|---:|---:|---:|---:|
| Closing withdrawn |70,314,927|14,802,280|153|50|
| Closing converted |29,963,435|6,544,716|85|26|
| Liquidated Started |70,719,701|14,761,077|176|52|
| Liquidated converted |31,023,586|6,636,617|89|28|
| Actual partial finish |23,084,560|5,404,429|55|12|
| Zero-asset residual funded repayment |36,085,157|8,005,932|96|27|

The deliberate minimum rejection now records a passing *proof outcome* only
after asserting Contract9101 and complete ledger/TTL rollback. Its raw unsigned
transaction remains `expected_rejection:true`, `success:false`,
`error:Contract(9101)`, `post_state_unchanged:true`, `ledger_changes_empty:true`.
The original conservative failed proof row remains in v2 evidence. The joined
single-test invocation gate passes; missing lifecycle, transaction and checkpoint
proofs still fail. No checker/profile limit changed.

Reproduction requires concatenating the six exact overlays, in order:
`cl-atomic-qualification/optimization.patch`,
`cl-storage-qualification/storage.patch`,
`cl-storage-qualification/repayment.patch`,
`cl-option1-qualification/pending-recovery.patch`,
`cl-option1-qualification/pending-unhealthy.patch`, and
`cl-option1-qualification/closing-recovery.patch` (all below `prototypes/`). Then:

```sh
CARGO_BUILD_JOBS=1 python3 prototypes/cl-option1-qualification/build-closing.py \
  --output target/FRESH-JOINED-BUILD --optimization target/SIX-OVERLAYS.patch
python3 prototypes/cl-storage-qualification/identify_build.py \
  --build target/FRESH-JOINED-BUILD
python3 prototypes/cl-option1-qualification/run-closing.py \
  --profile captured-network-diagnostic --build target/FRESH-JOINED-BUILD \
  --run target/FRESH-JOINED-CAPTURED
python3 prototypes/cl-option1-qualification/run-closing.py \
  --profile internal-policy --build target/FRESH-JOINED-BUILD \
  --run target/FRESH-JOINED-INTERNAL
```

The thin runner chooses exactly seven cases, preserving original open/activation/
close, three repayment regressions, expired/unhealthy pending recovery and the
five-state Closing/Liquidated test. It excludes three unrelated inherited atomic
matrix tests. It preserves initial runner reports, then binds actual generated
source hashes and exact test names to a separate joined proof manifest and runs
`check_budget.sh --gate checkpoint`. Missing Stage1 flow labels remain failures.
Signed authorization/application, live state, additional failure/reentry cases,
archival behavior of the new module, nonzero earned-fee execution and governance
adoption of the forced-expiry/contingent-fee policy remain unqualified.


### Review correction: live converted close fee bypass

The earlier v1 joined binary is now **known unsafe and diagnostic only**.
Source review found that `needed == 0` bypassed Closing expiry/unhealthy
admission. A third party could settle a healthy converted close before its
deadline and bypass the contingent close fee. Ordinary permissionless finish
being available did not justify that fee-policy change.

The exact seven-case run `target/cl-option1-joined-final-captured-v1` was
interrupted by SIGTERM of its identified local test process. Its raw rows,
source/build pins, original gates, joined partial gates and `INTERRUPTED.txt`
remain preserved. Four/five-state earlier successes do not clear this defect.
Copies of the pre-fix patch/test/runner/map are retained under
`target/cl-option1-closing-work/pre-live-close-fix`.

The corrected isolated candidate requires an expired Closing deadline when
`needed == 0`, with typed Contract9102 rejection. The existing `needed > 0`
expired-or-unhealthy admission remains unchanged, as do ordinary fee-preserving
finish and all production guards. The regression creates one additional real
active position for an existing owner with100bps contingent close fees; it
rejects live converted settlement and verifies exact ledger/TTL, debt, proceeds,
reward and fee-term rollback before allowing expired settlement. The original
eight positions remain intact. This is nine setup positions across the original
64 owners, not a claim of only eight positions or changed production limits.
A fresh test host preserves exact ledger state after this added setup.

The corrected six-overlay build and exact seven-case reruns are separate fresh
outputs. Until those complete, previous numbers are diagnostic measurements of
the earlier binary, not qualification of the correction.
