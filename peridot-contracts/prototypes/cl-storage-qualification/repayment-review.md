# Isolated dynamic full-repayment correction

This is an additive behavioral candidate, separate from the storage optimization.
`repayment.patch` applies only to a disposable source overlay; production source,
the prior atomic prototype, stages, profiles and shared handoff 00 are unchanged.
The candidate adds one V3 ABI method. It is not adopted or production-approved.

The existing public `repay_margin_position_v3(user, position_id, amount)` calls
receipt `repay_for_margin`. That helper caps `amount` at the stored debt **before**
interest accrual. It then accrues interest and transfers the capped amount. A
maximum covering the newer debt therefore still leaves an interest residual.
The existing debt-free release correctly rejects that residual. Additionally,
authorizing a token debit equal to the uncapped maximum cannot fix this behavior:
the actual debit has different authorization arguments. The existing API remains
a partial-payment route; this candidate does not change its semantics.

The proposed `repay_full_margin_position_v3(user, position_id, max_amount) -> u128`
requires external owner authorization and the same owner, Open/Closing state and
V3 mode checks as the existing repayment method. It delegates to the existing
receipt `repay_full_for_margin`, already used by funded liquidation and pending
recovery. That helper accrues interest, rejects a maximum below current debt,
transfers the exact authorized maximum and refunds surplus in the same root,
clears the position debt and updates aggregate principal/debt accounting. Its
return is actual debt repaid, not the temporary maximum debit. A zero-debt repeat
returns zero before another token debit; a zero maximum is rejected by V3.

The external authorization tree is precise:

```
owner: V3.repay_full_margin_position_v3(owner, id, maximum)
  receipt.repay_full_for_margin(id, owner, maximum)
    underlying.transfer(owner, receipt, maximum)
```

V3's controller authorization and the receipt's refund authorization are actual
contract authorization. No controller, facade or vault authorization is mocked.
Generated synthetic contract-address owners use narrow SDK mock_auths; these do
not prove real account signatures. No callback to the active V3 controller is
introduced. Existing receipt flash-loan/reentry restrictions remain unchanged.
The isolated V3 WASM must build with `funded-liquidation-validation` to retain
all CL custody hooks and its Public-network initialization guard.

Repayment and collateral release remain separate existing lifecycle operations.
The full repayment leaves collateral intact and owned by its prior beneficiary;
release again requires the owner and verifies zero accrued debt. A borrower can
stop after repayment without creating unbacked debt or transferring collateral.
No liquidation or seizure path changes, and funded repayment plus permitted
seizure remains atomic. No new staging is introduced by this candidate.

The new tests are included as a child module of a generated copy of the existing
64-owner, eight-borrower atomic fixture. They use compiled V3, receipt markets,
share vault and actual pinned Aquarius with controlled pool state, a compiled
dynamic rate model, TEST-only reserve/risk configuration and bounded resource
profiles. Setup is separate and uses the existing captured-network limits;
assessed roots retain SDK invocation checks and unsigned host29 transaction
measurement. Test assertion reads use cloned ledger snapshots to avoid warming
state before the measured operation. Existing test/source files are not edited.

| Test | Required assertions |
| --- | --- |
| `compiled_repayment_baseline_aged_partial_route_leaves_residual_and_release_rejects` | Old route offers a sufficient maximum but pays the pre-accrual amount, leaves positive residual, preserves beneficiary rewards and rejects release with full ledger rollback. This is regression reproduction, not full-repayment qualification. |
| `compiled_repayment_candidate_aged_full_max_refund_release_and_beneficiary_claims` | Eight aged debt positions; exact maximum and surplus maximum; actual debit/refund token events; payer and receipt net cash equal returned repayment; zero debt; repeated call returns zero; release and custody transfer through both upstream reward calls failing; booked beneficiary claims preserved and payable; later unresolved accrual quarantined; exact scaled aggregate conservation and no custodian entitlement. |
| `compiled_repayment_candidate_insufficient_max_unauthorized_and_closed_rollback` | Below-debt maximum, zero maximum, unauthenticated caller, wrong authorized owner, insufficient funds for maximum, calls after position deletion and repeated release reject; whole ledger snapshot/TTLs equal before and after each rejection; successful full repayment and release remain available afterward. |

The test harness emits detailed proof lines and the normal resource/transaction
records. Results, current artifact hashes, resource numbers, check_budget results
and CI status belong in the execution evidence produced by the lead; authored
test assertions alone are not a pass claim. Captured-network diagnostic success
cannot override internal-policy failures or missing lifecycle/release proofs.
Remaining qualification includes Closing-state full repayment, real signatures,
live-state conditions and deployment compatibility. No production approval is
implied by additive ABI availability in a local WASM.

The first focused execution (`target/cl-repayment-captured-v1`) ran **zero** tests
and correctly failed its budget gate for missing evidence. Its dependency file
showed the shared, identically named atomic harness executable omitted the new
repayment modules. This is a root-crate cache/identity collision, not repayment
qualification. The log, dependency file and original generated package metadata
are preserved. The disposable repayment harness package and lockfile were renamed
`cl-repayment-qualification-harness`, with updated manifest hashes, to isolate its
executable while reusing unchanged dependencies. No WASM was rebuilt or replaced
by this harness-only correction. Reproducible preparation must likewise assign an
isolated package identity; zero-test runs must remain missing-evidence failures.

The v4 execution completed the eight-borrower dynamic lifecycle but failed the two
negative suites because optimized WASM omits Rust panic strings. The old route
actually left 219 units of debt and release trapped after reading that debt; the
insufficient-maximum path likewise reached receipt repayment and trapped. The
corrected tests check exact host error types, explicit precondition reads from
cloned state, actual `fn_call` diagnostic events and complete ledger rollback.
They distinguish those observed conditions from an unavailable panic message and
reject unexpected authorization failures. SAC's explicit balance diagnostic and
the Auth error type remain strict assertions. The failed logs and original test
source are preserved; their successful invocation-budget result is not a passed
lifecycle suite.

A second negative-test diagnostic distinction was confirmed against SDK29: its
`try_call` reports `Context/InvalidAction` for both uncaught VM traps and missing
authorization, while typed diagnostic events retain the underlying `WasmVm` or
`Auth` error. The corrected helper checks both layers, precise reached calls and
the precondition predicate. A separate small compiled fixture passed all eight
rejection/rollback cases and a successful full repayment/release afterward
(`target/cl-repayment-debug-v1`). That debug run does not replace the final
64-owner historical stress cases. The prior v5 failure and source are preserved.

The isolated repayment V3 artifact is SHA256
`46c186e3bb333b929dabb3a4a4bec4376457081f2e1e404a467c8ada62f078c2`
(92,246 WASM bytes), versus pinned baseline
`f59f592b1013d4e1ec49975f0cb2bb0ff81e9ab0f88f92bd76c3b7a1d6de7a36`
(91,657 bytes). These are identities, not a resource gate. The extra ABI/code can
change instantiation cost for other V3 operations even when their source bodies
are unchanged. This separate experiment does not qualify the combined storage
optimization plus repayment patch, nor replace full lifecycle remeasurement if
that combined implementation is considered later.

## Final execution evidence

Three candidate tests completed successfully in **three separate filtered Cargo
runs**, plus one independent reproduction using the exact baseline artifact.
There is no invented combined Cargo summary. Every run's invocation gate passes;
its lifecycle, full-transaction and checkpoint gates remain failed for missing
Stage 1 evidence. All runs use the captured-network diagnostic profile, official
host29 cost coefficients at ledger 65270655, and controlled state. No internal
policy acceptance run is claimed for this isolated repayment experiment.

- `target/cl-repayment-final-positive`: eight-borrower full repayment/refund,
  zero-debt repetition, release, both-call outages, custody exit, beneficiary
  claims and exact conservation; 1 test passed, 72 unsigned records.
- `target/cl-repayment-final-adversarial`: seven negative cases with exact
  diagnostic classification and complete ledger/TTL rollback, followed by full
  repayment and release; 1 test passed, 9 unsigned records.
- `target/cl-repayment-final-old-route`: unchanged partial route inside the
  additive candidate; 1 test passed, 2 unsigned records.
- `target/cl-repayment-final-exact-baseline`: same regression on original V3
  bytecode; 1 test passed, 2 unsigned records.

Both old-route runs offered 27,500,000 against post-accrual debt of 25,000,219,
paid 25,000,000, left 219 and rejected debt-free release without changing the
ledger. The expected-negative regression pass is **not** full-repayment approval.
The new route settled debts of 25,000,225 through 25,000,252. Its first maximum
was exact; seven 27,500,000 maxima refunded 2,499,770 through 2,499,748. All eight
positions reached zero debt, repetitions returned zero, and booked beneficiary
claims totaled 83,048 through both upstream failures.

Final reward conservation is exact at scale 1e18:

```
received = 4,389,282
paid = 83,048
pending = 17,584
quarantine = 4,288,611
holder fractions = 38,999,999,999,973,007,512
rounding_scaled = 26,992,488
received * scale = (paid + pending + quarantine) * scale + fractions + rounding
```

No forced waiver occurred; custody contract entitlements are zero. The paid
amounts were observed in beneficiaries' reward-token balances.

| Assessed SDK root | Max instructions | Max memory bytes | SDK entries | Writes |
| --- | ---: | ---: | ---: | ---: |
| Full post-accrual repayment | 21,264,173 | 4,898,409 | 74 | 12 |
| Repeated zero-debt repayment | 16,378,731 | 4,317,642 | 73 | 7 |
| Debt-free release | 17,179,502 | 4,204,990 | 79 | 22 |
| Margin custody exit, both reward calls failing | 46,704,580 | 9,280,076 | 90 | 20 |
| Booked beneficiary reward claim | 5,258,792 | 3,271,925 | 14 | 5 |
| Receipt withdrawal after dependency recovery | 36,117,842 | 8,244,947 | 89 | 22 |

The unsigned transaction measurements are separate: full repayment peaks at
20,751,132 instructions, 75 unique footprint entries, 6,776 serialized envelope
bytes and 524 authorization bytes. Its highest total fee estimate is 134,271
stroops: inclusion 100, non-refundable 37,282, refundable 1,739 and rent 95,150.
Margin custody exit peaks at 46,162,365 instructions, 7,963,668 memory bytes,
91 unique footprint entries and 8,492 envelope bytes. Its highest fee is
5,192,885 stroops, dominated by controlled-state rent (5,130,659). Fee-component
maxima are taken from the same highest-fee transaction, not independently summed.
SDK entry counts and unique transaction footprints are not interchangeable.

Re-run one filtered case with the corresponding prepared artifact bundle and a
**fresh** output directory, for example:

```sh
CL_ATOMIC_BUILD_DIR="$PWD/target/cl-repayment-build" CARGO_BUILD_JOBS=1 \
  bash prototypes/cl-storage-qualification/run_variant.sh \
  captured-network-diagnostic \
  repayment_cases::compiled_repayment_candidate_aged_full_max_refund_release_and_beneficiary_claims \
  target/cl-repayment-positive-reproduction
```

For the exact old regression use `target/cl-repayment-baseline-build` and filter
`repayment_cases::compiled_repayment_baseline_aged_partial_route_leaves_residual_and_release_rejects`.
The wrapper runs `check_budget.sh` and retains missing-proof failures in its gate
JSON even when the selected invocation gate passes. Source, artifact and input
hashes plus per-flow resources and complete fee components are summarized in
`target/cl-repayment-results.json`; its generator is
`target/cl-repayment-work/summarize.py`. Raw logs and unsigned records remain the
execution evidence. No additional archival run, live-state validation, signature
measurement, combined storage/repayment adoption or CI approval is claimed here.
