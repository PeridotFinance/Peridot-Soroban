# Option1 pending-state recovery research

This is an isolated, **unadopted** design and compiled experiment. Stage1 remains
incomplete. Production sources, handoff00, both resource profiles, borrowing
configuration, production maintenance margin and existing financial stages are
unchanged. No signing, broadcasting, secrets or Upshift binding. The patch is
compiled only with `funded-liquidation-validation` and rejects Mainnet.

The state map and chronological measurements below preserve earlier partial
variants. The corrected six-overlay candidate now executes both pending routes,
the six Long Closing/Liquidated recovery shapes, full-repayment regressions and
stressed open/close together: seven captured-profile tests pass. Use
[lifecycle-proof-map.md](lifecycle-proof-map.md) for the complete current map and
the exact joined gate reports for remaining failures. Earlier `UNCOVERED` rows
describe this pending-only variant, not a claim that the new joined test omits
all those shapes. No earlier resource result is relabelled as new bytecode.

## Actual debt-bearing state map

The source of truth is `contracts/margin-controller/src/perps.rs`,
`pending_recovery.rs`, `funded_liquidation.rs` and the stored structures in
`storage.rs`. Position `debt_shares` is not the authoritative accrued debt;
`ReceiptVault.get_margin_borrow_balance(id)` after interest update is.
Failed root invocations roll back; they do not create another persisted stage.

| Persisted state and transition | Held assets and debt | Deadline / existing recovery | Independent funded gap |
| --- | --- | --- | --- |
| `PendingOpen`, no execution: successful `begin_open_position_v3` | Margin pTokens and reserved opening fee remain in controller; receipt initializes **zero debt** | Borrower cancel at any time; permissionless expire strictly after `expires_at` (30 minutes) if no execution/debt | Not debt-bearing. Swapping withdraws margin, borrows, trades and assigns CL beneficial custody atomically; any failed swap restores this zero-debt prestate. |
| `PendingOpen` plus execution, before expiry: successful swap | Controller holds exact `execution.position_amount` of position asset, including original margin plus sale proceeds for Short; Long holds purchased base/CL shares. Borrower retains CL beneficial ownership. Full receipt debt accrues; no position pTokens minted yet. | Borrower activation/execute finalizer or owner-funded recovery. Cancel rejects debt; expire rejects execution. | No independent direct funded route before expiry. Proposed patch deliberately rejects. Existing borrower liveness is assumed until timeout; a requirement for earlier unhealthy takeover needs a separate policy/implementation. |
| Same swapped `PendingOpen`, expired / failed activation / repeated failed activation | Identical custody and growing debt; failure does not partially mint collateral | Permissionless `resolve_pending_open_v3` still performs activation with health/reserve/claim requirements. `liquidate_position_v3` after expiry first activates without open-health enforcement, then requires current liquidation health and closes through venue; failures roll back. Owner-funded recovery fully repays and returns asset without activation. | **Patch target:** independent payer settles without activation, venue conversion or borrower authorization; no bonus, exact residual returned. This closes only this proposed local route, subject to tests and valuation/pause availability. |
| `Open` plus prepared `PendingPerpsClose`, no withdrawal | Existing position pTokens still in controller/receipt, existing debt accrues; pending close amounts zero | Owner cancel or permissionless expire (>5 minutes) clears only preparation. Active funded liquidation remains available when unhealthy. | Existing active funded path is concrete; expiry need not take collateral. Not a new closing custody state. |
| `Closing`, withdrawn but unswapped (`debt_amount==0`, `received_debt_asset==0`) | Controller holds actual `pending.collateral_underlying`; receipt pTokens have been burned though position field can retain historical count. Debt remains live. | Owner cancel / permissionless expired cancel redeposits underlying, which can fail if deposit/claim hooks fail. Staged liquidation may start when unhealthy; subsequent swap/finish require venue. | **UNCOVERED:** direct independent full-repay + proportional in-kind seizure from held underlying. Never use historical pToken count as additional collateral. Healthy expired state has only redeposit route today. |
| `Closing`, swap completed | Controller holds `received_debt_asset`; Short may additionally hold exact `PendingPerpsCloseRemainder` in collateral asset. Actual receipt debt remains until finish. | Permissionless finish updates interest, repays available proceeds and releases assets if debt clears. Cancel/expire refuse already-swapped states. | Finish can commit partial repayment and leave residual. Independent funded completion using held debt-asset proceeds first and external top-up, plus bounded seizure from remaining collateral, remains **UNCOVERED**. Already-held proceeds must never be counted twice or assigned to funder for free. |
| `Closing`, partial finish committed, debt still positive | Remaining debt asset proceeds usually zero; Short remainder can remain. Debt reduced but continues accruing. `pending.debt_amount` marks prior swap, not current debt. | Owner repayment/full-repay overlay plus debt-free release. After timeout staged liquidation allows healthy residual recovery with zero incentive; unhealthy recovery retains configured bonus. Venue and bad-debt settlement remain dependencies. | **UNCOVERED:** complete independent externally funded deficit settlement with exact residual partition; zero collateral cannot guarantee an economically willing payer. Do not label bad-debt absorption full funded repayment. |
| `Liquidated`, `PendingLiquidation::Started` from Open | Locked position pTokens remain; accrued debt persists | Chosen liquidator swap, then finish; another liquidator may take over strictly after 5-minute takeover deadline. | **UNCOVERED:** atomic funded settlement of pending liquidation without first swapping. Preserve takeover ownership before deadline, refresh valuation, and return residual. |
| `Liquidated`, `Started` inherited from withdrawn/partly swapped Closing | `PendingLiquidationCollateralUnderlying` is authoritative held collateral, potentially alongside debt-asset proceeds; pToken count zero | Same takeover/swap logic. Subsequent swap updates proceeds and stage atomically. | **UNCOVERED:** multi-asset partition, external deficit repayment and custody-safe bounded seizure; cannot reuse Open pToken formula blindly. |
| `Liquidated`, `CollateralConverted` | Only held `received_debt_asset` plus any explicitly recorded assets; accrued receipt debt still exists until finish | Chosen liquidator finish or independent taker after deadline. Finish repays proceeds, may absorb remaining bad debt, pays incentive only from surplus and credits owner residual. | Existing independent takeover is a route but **not** a full external-funding proof. Atomic funded top-up instead of loss absorption and precise rights to surplus remain unimplemented here. |
| Position removed / debt zero | No live position authority remains; beneficiary reward ledger/history survives | Normal reward claim/reconciliation and wallet/free-margin exits | Repeat funded recovery must reject with complete rollback; no second seizure. |

`execute_open_position_v3` is an alternate activation finalizer after swap, not
an operation before swap. Shared00's old begin/execute/swap/activate ordering
still needs coordinated reconciliation; this document does not edit policy00.

## Concrete PendingOpen prototype

`pending-recovery.patch` appends a validation-only
`recover_expired_pending_funded(payer, id, max_repay, min_seize_underlying)`.
It requires a swapped pending V3 record, strictly elapsed original deadline,
matching owner/assets/vaults, nonzero current debt and distinct external payer.
The existing owner-funded method remains unchanged.

The receipt updates interest; `refresh_price_usd` invokes `cache_price` and fails
closed on unavailable/zero prices, without the ordinary cached/static-price
fallback. With prices represented as `(numerator, denominator)`:

- `debt_value = floor(current_debt * debt_price.num / debt_price.den)`.
- `seized = min(position_amount, floor(debt_value * collateral_price.den / collateral_price.num))`.
- `returned = position_amount - seized`.

There is **zero liquidation incentive**, including for underwater positions.
Rounding favors the borrower. Payer signs the maximum and minimum seizure;
the existing receipt `repay_full_for_margin` takes the fixed maximum and refunds
surplus atomically. Debt must become exactly zero before either recovery-mode
CL custody transfer. Each transfer retains its claim-first boundary and passes
no waiver. Seized underlying goes only to payer; all other position underlying
returns directly to borrower, and reserved opening fee pTokens are refunded to
borrower's existing margin ledger. Recipient/controller balance deltas are
checked. Any repayment, valuation, custody or transfer failure reverts all state.
No holder enumeration, pool trade, activation or receipt deposit occurs in this
financial root. Restoration preparation remains a separate conditional tool.

Borrowing pause / disabled cash borrowing / reserve headroom do not prevent this
risk-reducing route. The existing explicit liquidation pauses and margin-lock
configuration remain enforced. **No unconditional recovery guarantee exists
while liquidations are paused or fresh trusted valuation is unavailable.**
Neither fallback price acceptance nor a new pause override is invented.

The timeout-triggered unwind of a healthy position at zero bonus is an
**unadopted design choice**. The user authorized local research, not production
policy adoption. It extends the prior stalled-close zero-incentive idea to
PendingOpen; it does not claim that policy already exists. Payer availability,
operating costs, oracle freshness/validity on actual live CL NAV, and short-side
qualification remain open. The synthetic test oracle supplies current timestamps
and controlled prices: even with real Aquarius bytecode/trades this is not a
proof of live economically correct collateral valuation.

## Existing proofs versus new tests

Existing compiled `compiled_atomic_qualification_8_independent_pending_recoveries_repeated_both_call_outages`
proves **owner-funded** recovery of eight swapped pending positions and preserves
historical rewards/quarantine; it does not prove owner absence. Existing
`compiled_combined_funded_liquidation_without_borrower_authorization` proves
independent funding for **Open**, not PendingOpen/Closing/Liquidated. Native
`test_expired_close_residual_has_permissionless_zero_incentive_recovery`,
`test_finish_close_v3_commits_partial_repay_then_owner_can_top_up`, and
`test_split_liquidate_position_v3_can_be_taken_over_after_timeout` establish
separate synthetic behaviors, not the new compiled route.

New source: `pending-recovery-tests.rs`, child of a copy of the existing atomic
fixture. Test name:
`test::pending_qualification::pending_cases::compiled_option1_expired_pending_independent_funding_outage_trade_and_rollback`.
It retains 64 owners / eight swapped pending debt positions / historical weights,
uses actual Aquarius trade bytecode, accrued interest, a distinct funded payer,
explicit narrow external auth and real controller/receipt/vault contract auth.
Assertions cover live/exact-deadline rejection, insufficient max, excessive
minimum seizure, absent and missing nested external authorization, successful
full repayment/refund/residual return, no borrower authorization, unchanged
borrower booked rewards, no payer prior entitlement, no custody remaining for
the settled borrower, replay rollback, late quarantine and scaled conservation.
Host recording cannot prove an omitted SDK mock tree: that case retains a
positive recording-only host companion and a separate actual SDK auth rejection.

**Still required / FAIL until executed:** this new test's results and resource
limits; wrong argument/recipient authorization binding; insufficient payer token
balance; invalid/stale valuation; failed collateral transfer after repayment;
liquidation-pause handling; zero-debt/unswapped/Open/wrong-mode roots; underwater
full-funder-loss case; Short; post-restoration execution and historical state;
all Closing and Liquidated rows above; real signatures and target live state.
No missing test is filled by a placeholder row or inferred from setup success.

## Reproduce

Build exactly the atomic TTL patch, storage patch, full-repayment patch and new
pending patch, in that order. `build-pending.py` rejects any other overlay bytes,
uses a fresh contract cache and only `stellar contract build`, and preserves
baseline artifacts and production sources.

```sh
cat prototypes/cl-atomic-qualification/optimization.patch \
  prototypes/cl-storage-qualification/storage.patch \
  prototypes/cl-storage-qualification/repayment.patch \
  prototypes/cl-option1-qualification/pending-recovery.patch \
  > target/cl-option1-pending-four-overlays.patch
CARGO_BUILD_JOBS=1 python3 prototypes/cl-option1-qualification/build-pending.py \
  --output target/cl-option1-pending-build-v1 \
  --optimization target/cl-option1-pending-four-overlays.patch
python3 prototypes/cl-storage-qualification/identify_build.py \
  --build target/cl-option1-pending-build-v1
CL_ATOMIC_BUILD_DIR="$PWD/target/cl-option1-pending-build-v1" CARGO_BUILD_JOBS=1 \
  bash prototypes/cl-storage-qualification/run_variant.sh \
  captured-network-diagnostic pending_cases target/cl-option1-pending-captured-v1
```

Use fresh paths; preserve every failed attempt. The wrapper invokes
`check_budget.sh` even after a failed test and retains missing global flow/proof
failures. Captured-network and internal-policy runs remain separate. Invocation
success is not lifecycle, full-transaction or release approval.

## Executed expired-only checkpoint (preserved before the early-unhealthy extension)

`target/cl-option1-pending-captured-v1` failed Rust test compilation (two
ambiguous `IntoVal` payment-event assertions); no execution proof. Its logs and
failed budget gate are retained. Explicit `Val` locals fixed the fixture;
compiled contract bytes did not change.

Captured v2 executes the one focused compiled test: **1 PASS**, 52.89 seconds.
Internal v2 executes the same source/artifacts: **0 PASS / 1 FAIL**, 42.02 seconds.
The latter reaches the recovery's return then fails the finite SDK footprint
check at **152 entries >100**. This is a resource failure, not an authorization
rejection. The root's subsequent assertions/quarantine/replay are not reached
under internal policy.

| Measured successful recovery | Captured network | Internal policy diagnostic |
| --- | ---: | ---: |
| SDK instructions |65,949,709|Root returned before SDK entry-limit rejection; no completed SDK resource row claimed|
| SDK memory bytes |13,237,653|Same completed invocation not accepted|
| SDK memory reads + disk reads + writes |102+0+50 =152|152 >100, FAIL|
| Official host instructions |70,452,749|70,452,749|
| Official host memory bytes |13,091,451|13,091,451|
| Unique host footprint |103|103 >100, FAIL|
| Writes / serialized write bytes |50 /11,400|50 /11,400|
| Unsigned envelope / events bytes |9,792 /1,808|9,792 /1,808|
| Fee estimate, stroops |9,621,183|9,621,183, not an acceptable transaction|

Fee components are inclusion100, nonrefundable110,441, refundable4,415 and
rent9,506,227. The recording-only companion for the missing-nested-SDK-auth case
and the actual positive root have matching host resources; they are two host
records, **not two successful SDK test executions**.

Actual settlement: debt25,000,321 (including321 accrued interest), seizure
25,000,321 underlying, borrower residual24,971,725, maximum refund2,499,679.
Borrower booked rewards remain unchanged; payer receives no old entitlement.
Final exact scaled conservation: received5,436,269; paid0; pending100,632;
quarantine5,335,598; fractions38,999,999,999,973,007,512;
rounding26,992,488; scale1e18. Actual vault reward-token balance equals received
minus paid. Seven rejecting roots reach the actual compiled method: live,
exact deadline, insufficient max, excessive min, absent authorization, missing
nested authorization and repeated execution; their complete ledger/TTL state
rolls back. These are not setup panics.

Both `check_budget.sh --gate checkpoint` results stay **FAIL**. Captured has
invocationPASS but lifecycle/transaction/checkpointFAIL,46 missing required
flows,2 lifecycle errors,46 transaction errors. Internal has all four gatesFAIL,
46 missing required flows,8 lifecycle errors,48 transaction errors. The separate
proof map binds the new concrete test; it does not replace global requirements.
`target/cl-option1-pending-expired-only-results/results.json` preserves exact
errors, host records and artifact hashes plus the original patch/test sources.
No CI or interrupted/restored/re-expired execution of this new route is claimed.

Expired-only V3 bytecode:94,841 bytes,
`4da1792767959903d7e800c18ce423ac8a3fe794b49856a87fc4f8380297f823`.
Receipt:88,329 bytes,
`fc1ac7d397428438c58c50ce8930907c5965587aac7cc0f9988025235dcde362`.
Controller:63,411 bytes,
`a55d5740007553d955c26e08efad4289b093c0d85c34a2b0e2af7262eaba0052`.
Pinned execution is host/SDK29.0.1 with captured ledger65270655 coefficients;
contract SDK25.3.1. These are local controlled-state bytes, not live deployed
identities or real account-signature measurements.

## Additive early-unhealthy experiment (results pending)

`pending-unhealthy.patch` applies **after** the expired-only patch and adds
`liquidate_pending_open_funded`. Its early admission condition reuses existing
V3 pair maintenance configuration and fresh current debt/collateral values:
Long notional is collateral value; Short notional is debt value; equity is
saturating collateral minus debt; eligibility is equity<=maintenance. It sets
no maintenance margin and rejects an unavailable/invalid configuration.
Healthy pre-expiry positions remain protected. The timeout-only method retains
its strict deadline. Both routes share full settlement and zero bonus; they
never force activation. This is still an unadopted validation-only ABI, including
its new earlier timing availability, and not a production policy change.

`pending-unhealthy-tests.rs` adds healthy rejection and unhealthy settlement
before expiry, nonzero interest, actual payer cash insufficiency, absent auth,
replay, no waiver and delayed quarantine. `combined-pending-regression-tests.rs`
retains the prior three combined test bodies with two explicit adaptations:
manifest candidate label, and replacement of the restoration-trade hook with an
actual measured Aquarius trade in the active-liquidation fixture. It must not
be described as the unchanged original tests or restoration coverage. The
five-overlay builder copies every generated Rust test and patch into the build
and records source hashes; repeated measurement must use those new V3 bytes.

## Five-overlay combined pending checkpoint

The exact new V3 is95,480 bytes,
`5fd1417563862825430dc3dfa1d6f0a3a2bc1c044095ac3b36e0077e02776ff5`.
`target/cl-option1-pending-captured-v3` completed **5 PASS /0 FAIL** and its
invocation gate passed. This reruns both pending routes plus the three adapted
repayment/active-liquidation regressions on these exact bytes. Runtime681.08s
includes severe concurrent host resource contention; it is not network latency.
The expired-only internal failure above cannot be relabelled as a completed
internal run of this new bytecode. New full-bundle internal and joined
Closing/Liquidated/open-close measurements remain separate required work.

| New V3 completed SDK root | Instructions | Memory bytes | SDK entries | Writes |
| --- | ---: | ---: | ---: | ---: |
| Expired pending independent recovery |65,952,494|13,238,119|152|50|
| Unhealthy pending independent recovery before expiry |64,083,720|13,025,561|152|50|
| Active funded liquidation after actual trade |62,124,315|12,782,608|170|50|
| Aged full repay, maximum of two cases |14,021,663|4,354,322|74|12|
| Debt-free release, maximum of two cases |12,461,976|4,088,229|86|22|

Early unhealthy recovery repays25,000,071, seizes38,461,647 at the controlled
0.65 valuation and returns11,510,399. Healthy pre-expiry state, insufficient
maximum, actual insufficient payer cash, absent authorization and replay reject
with complete rollback. Its final conservation is received1,208,059; paid0;
pending100,632; quarantine1,107,388; the same fraction/rounding terms as above.
No live oracle adequacy follows from the synthetic0.65 price. No bonus is paid.

Post-change resource/transaction proof mapping uses
`pending-combined-proof-map-v2.json`. The first mapping used an incorrect
`closed_position` label instead of the actual `closed_repay` label; its failed
report is preserved, not treated as a failed contract test. Required global
flows/proofs stay missing and release approval remains false. No CI execution
of this new pending bundle is claimed.

All custom-account authorization/reentry behavior, real G-account signatures,
failed collateral delivery after successful repayment, expiry while actual
ledger time passes, and worst-case per-borrower64-position cleanup remain
unqualified. The tested registry has64 owners/eight independent pending debts,
but the settled borrower owns one position. Nonzero current debt, nonzero held
underlying, positive `min_seize`, available fresh primary valuation and
unpaused liquidation are admission requirements; this is not unconditional
recovery or an economically guaranteed payer.

A separate generated restoration harness is prepared by
`prepare-pending-restoration.py`. It wraps only the two new pending ABIs and
extends their post-execution beneficiary checks. Its first run was deliberately
stopped by the lead to relieve resource contention after compilation; logs/cache
and `interruption.json` remain in `target/cl-option1-pending-restoration-v1`.
A fresh v2 attempt is separate. The bounded re-expiry mechanism explicitly sets
the first prepared batch's TTL back to current ledger minus one, serializes and
reloads that snapshot, invokes official native restore simulation again, and
uses the exactly re-prepared state for subsequent roots. This is **synthetic
TTL fault injection with zero time advance**, not elapsed-time or Core expiry.
An extra measured batch/fee is recorded separately, and total scenario fee/count
includes it. The base journey totals alone exclude that deliberately added
retry; no double payment may be hidden. `check-pending-reexpiry.py` verifies
those links and decoded batch costs; default qualification stays failed until
actual required evidence exists. None of this substitutes for the available
backend's actual Core transaction-application test.
