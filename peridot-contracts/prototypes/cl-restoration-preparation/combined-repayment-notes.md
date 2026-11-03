# Combined storage and full-repayment test scope

`combined-repayment-tests.rs` is new, separate from the previously executed
storage-only and repayment-only experiments. It changes no contract or shared
policy. The lead builds the combination of the existing `storage.patch` and
`repayment.patch` with `stellar contract build`; this test module does not build,
sign, broadcast or deploy anything.

Include this module as `combined_repayment_cases` beneath a generated copy of
`prototypes/cl-atomic-qualification/tests/combined.rs`. That lexical parent
provides its private compiled fixture and accounting helpers. Use a unique
native harness package identity to avoid the earlier shared-Cargo-executable
collision. Do not edit or overwrite the original atomic fixture or prior tests.

The tests reject an artifact directory unless its build manifest has:

- `qualification_variant: "combined-storage-and-full-repayment"`;
- `combined_patch_sha256.storage` and `.repayment` equal to the patch bytes
  embedded when this test module compiles;
- `margin_build_feature: "funded-liquidation-validation"` and
  `release_approval: false`;
- matching hashes and lengths for the actual V3, receipt and lending-controller
  WASMs under `CL_ARTIFACTS`, in `CL_ATOMIC_BUILD_DIR/artifacts`.

These identity checks prevent accidental execution against the prior single
patch bundles. Build provenance still requires the lead's pinned overlay and
build logs; a manifest label alone is not proof of how bytecode was built.

## Focused compiled cases

The first two cases reuse the final repayment assertions from the prior
experiment, including typed host diagnostics rather than stripped panic text.
They retain the original controlled-state fixture's 64 owners, eight funded
positions and 407 historical weight checkpoints. Only two owner repayment
lifecycles are executed: enough to exercise exact maximum and surplus/refund,
without repeating the old partial-route regression or all eight exits.

| Test suffix under `combined_repayment_cases::` | Assertions |
| --- | --- |
| `compiled_combined_repayment_aged_full_max_refund_release_and_beneficiary_claims` | Two independent, aged dynamic debts; exact maximum and surplus maximum; exact SAC debit/refund events and net payer/receipt cash; zero debt; repeat returns zero; authorized release and custody transfer while both upstream reward calls fail; preserved booked beneficiary claims; unresolved late accrual remains quarantined; exact scaled conservation across all holders and no custodian entitlement. The other six debt positions remain active. |
| `compiled_combined_repayment_insufficient_max_unauthorized_and_closed_rollback` | Insufficient maximum, zero maximum, absent external authorization, wrong owner, insufficient liquid payer balance, repayment after deletion and repeated release all reject with complete ledger/TTL rollback. Correct full repayment and release remain available after those rejections. |
| `compiled_combined_funded_liquidation_without_borrower_authorization` | One aged position is liquidated with a distinct liquidator's narrowly scoped external authorization only. Actual nested controller/receipt/vault authorization remains enforced. Full debt repayment and permitted seizure are one atomic root; exact maximum/refund events, zero debt and borrower custody exit; prior borrower rewards preserved, no forced waiver or historical credit to the liquidator; unresolved rewards remain quarantined and exact conservation holds. |

The positive full-repayment operation is intentionally owner-authorized. It
must not be described as borrower-independent recovery. The third test supplies
the correct existing funded-liquidation root for restoration work independent
of the borrower. Its TEST-only oracle and funding setup precede the assessed
root; it does not bypass a missing trustworthy production seizure price.

All contract authorization remains real. Generated synthetic external identities
use exact SDK `mock_auths`; no controller, receipt, facade or vault authority is
mocked, and no blanket bypass is used. SDK authorization checks and unsigned
recording-mode transaction evidence remain distinct from real account-signature
verification. Every assessed flow must retain its normal resource rows and
`check_budget.sh` result under the selected finite profile.

The storage overlay replaces broad receipt keepalive with keys consumed by each
operation and preserves fresh interest accrual. The combined cases therefore
specifically exercise the new full-repay ABI through the altered receipt
initialization/interest paths. They do not assume that separate prior passes
qualify their combined bytecode, cold code, or all other lifecycle operations.

## Restoration and interruption evidence still required

The existing exporter restores archived entries as part of a cloned invocation.
That alone does not establish separately paid preparation transactions, their
fees, or interruption/restart behavior. The lead is implementing the RestoreOp
exporter. There are no placeholder restoration tests or passing rows in this
module. Add concrete assertions only after that exporter supplies real evidence.

For each selected financial root, the preparation evidence must establish:

1. Each batch is an actual unsigned `RestoreFootprint` operation with exact
   footprint, serialized envelope, resource accounting, rent and complete fee
   components. Capture every required code, instance and persistent entry;
   counts or WASM byte sizes alone are insufficient.
2. The preparer is distinct from the borrower. Restoring ledger entries requires
   the fee source's transaction authorization, not borrower contract permission.
   An unsigned envelope proves neither a real fee-source signature nor network
   application. State those limits explicitly.
3. Every preparation batch changes only legitimate restoration/TTL metadata.
   Logical code/data, raw debt state, collateral, custody, booked rewards,
   historical weights, replay markers and quarantine must be unchanged. Check
   the snapshot after every prefix, not only after all batches finish.
4. Stop after a nonempty proper prefix, retain that resulting snapshot, and
   continue preparation with another preparer. Retry/replan already-live keys
   without duplicating business effects; report all paid preparation costs.
   Re-running every branch from the original snapshot is not interruption proof.
5. Invoke the actual combined financial root against the resulting prepared
   snapshot. Compare its execution result, logical state, events and contract
   authorization with the unarchived control. Fresh accrual, maximum/refund,
   current valuation and claim-first boundaries remain in that measured root.
   Permissionless restoration does not remove a stale-oracle liquidation blocker.
6. Establish borrower absence on the funded-liquidation root and its preparation
   path. Debt repayment and permitted collateral seizure must remain atomic.
   Owner repayment/release can be tested separately without claiming independence.
7. Treat intervening trades or elapsed time as changed execution inputs. A prior
   footprint or debt quote does not promise the final call still fits or its
   maximum still suffices. A fresh bounded plan may be needed; failed financial
   execution must roll back and preserve ownership/reward history.

Restoration is ledger preparation, not a new business state: it must not move
collateral, originate or cancel debt, waive rewards or change a pending-open
state. No production deadline, cancellation or authorization rule is added by
this test module. A partially completed preparation must not become a new
borrower-controlled prerequisite to the existing funded route.

The module alone establishes none of these preparation claims. Keep missing
preparation, signatures, live-state validation, remaining lifecycle proofs and
release approval failed. Captured-network diagnostic results cannot override the
unchanged internal-policy gate. Do not infer capacity or production readiness
from the focused tests or the earlier separate experiments.
