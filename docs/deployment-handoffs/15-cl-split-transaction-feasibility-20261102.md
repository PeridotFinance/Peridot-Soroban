# Isolated transaction-split feasibility checkpoint — 2026-11-02

Stage 1 remains incomplete. PR #75 remains draft. Recommendation: **do not adopt this prototype**. Separating conversion from LP-fee collection is useful and both complete calls fit the internal limits in this fixture. The tested atomic share-mint transaction and existing activation/close transactions still fail. This does not prove that every possible split fails: the individual Aquarius deposit call is below100M, and the old147.9M venue sum was not an indivisibility proof.

The comparison baseline is commit `9cf43082bfb57334d57dbc58cfa90ccad4d98078`, with its separate reserve/history evidence archive. Production contract source, transaction stages, limits, borrowing configuration, maintenance margin and shared00 are unchanged. The experiment is a source-hash-bound overlay under `peridot-contracts/prototypes/cl-split-transactions/`, compiled only with `stellar contract build`. The new controller methods require the existing validation feature/network guard; the new vault methods and escrow also reject Mainnet. No signing, broadcasting, deployment, secret access or Upshift binding occurred.

## What was actually executed

Compiled V3, receipt markets, share vault, facade, adapter and escrow run with actual pinned Aquarius bytecode (`12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`). State, oracle and external identities are synthetic. Actual nested contract authorization and `authorize_as_current_contract` execute; narrowly scoped external test authorization is not an account-signature proof. No blanket authorization bypass is used.

Contracts resolve SDK25.3.1; definitive local execution uses SDK/host29.0.1, protocol29 and captured cost-model coefficients, not host25 repricing. Numeric settings and coefficients are bound to config SHA256 `a28fb64bb85c74f8f0d1d4bd7a0daf413ada6f603647c64f556a7905af554dcd` (captured settings ledger65264920). Internal limits remain100M instructions,41943040 memory bytes,100 SDK entry accesses and50 writes; captured diagnostics retain400M/400/200 with the same memory ceiling. All other existing checks remain active. Acceptance metering enforces the selected limits; diagnostic call-trace shadows have separate larger instrumentation budgets and are never acceptance quantities.

The pre-existing1B holder position is explicitly constructed under captured limits. The test-only capacity is2B and the margin scenario contributes100M plus100M borrowed cash; this is a debt-bearing Long stress fixture, not a production1x launch or permission to enable borrowing. Later isolated-stage tests label their captured prestate. Passing one of those tests cannot establish an internal-policy full lifecycle.

Each measured stage is a separate top-level invocation and separately serialized unsigned end-to-end host transaction. SDK entry totals equal read + write + disk-read accesses; unique transaction footprint keys are a separate column. Transaction CPU includes its actual execution/auth template; real signatures, final signed envelopes, surge fees and live-state validation remain unmeasured.

## Smallest useful split and atomic boundaries

The tested combined conversion+fee preparation costs about107.6M CPU, so the useful variant separates them. The current prototype then uses seven opening transactions with a prefunded margin account: begin, fund, convert, prepare fees, mint, deliver, activate. Funding margin adds one. Existing close still adds prepare/swap/finish. These are experimental transactions only.

LP trading-fee collection is distinct from AQUA reward claims. Preparation collects the former after the entrant's conversion (which itself creates incumbent LP fees). It does not replace any reward boundary. Mint retains the actual claim before introducing new share weight; escrow beneficial assignment adds a fourth actual claim. Delivery retains the original claim-before-transfer and V3 claim-before-custody-assignment. Every call is included in measurement.

These parts remain atomic in the candidate:

- Receipt withdrawal, debt draw, exact escrow credit and funded-state record. There is no committed debt without the associated escrow record.
- Each conversion, actual token deltas and its phase transition.
- Final fee/config witness validation, fresh oracle/composition/NAV/supply, claim-first mint, exact pair transfer and Aquarius deposit, minimum share output, beneficial escrow assignment and consumed marker.
- Claim-first share transfer, V3 claim-first assignment and exact canonical execution record.
- Full accrued debt repayment, zero-debt verification, permitted release and cleanup. Existing funded repayment/seizure remains atomic and unchanged; it does not cover the new pre-delivery escrow states.

The final mint uses a conservative witness: pool executable/configuration, fee parameters, both fee-growth globals, slot/active liquidity, range-boundary ticks, full position data and owner range snapshot. It rejects changed/malformed views, nonzero owed fees, unsupported ranges and expired/consumed jobs. It then values current holdings and supply; it does not cache the preparation's NAV. The fence protects only supported configurations, not every possible venue mutation. In particular, skipping the final swap quote would require binding all relevant traversed ticks/bitmaps; current owned-range checks alone do not establish that dependency.

## Committed intermediate states

| State | Assets / rewards / debt | Resume and recovery |
| --- | --- | --- |
| Reserved | V3 reserves margin and fee receipt shares; no new debt or CL rewards | Owner funds before the existing deadline; existing authorized cancel or expiry releases unborrowed reservation. |
| Funded (1) | Escrow holds exact job cash; borrower is economic owner. Receipt interest starts with the actual borrow and continues during delay | Owner may convert or externally fund complete repayment plus atomic cash return. Legacy unswapped cancellation/swap/finalization rejects. |
| Converted (2) | Escrow holds exact job cash and paired asset; donations remain unassigned | Owner prepares fees before deadline, or fully repays and receives both legs in kind. No venue call is needed for that release. |
| Prepared (3) | Same escrow assets; collected incumbent LP fees stay in the vault | Owner may mint against a current witness, reprepare with the **same deadline**, or fully repay/recover. Intervening trades invalidate the witness. No global ownership lock exists. |
| Minted (4) | Escrow holds job shares; borrower already has beneficial reward weight, including across a delivery delay | Owner delivers (including after expiry), or fully repays and recovers shares with forced custody hooks. No voluntary reward waiver is used. |
| Delivered (5) | V3 physically holds exact shares and beneficial assignment; canonical pending execution exists | Existing activation rechecks health/costs/cash reserve. Failure leaves delivered collateral recoverable by owner-funded full repayment. |
| Active / closing | Existing margin/receipt custody and debt state | Existing close or repayment/release applies. Pending cancellation cannot be used. |
| Recovered (6) | Principal allocation and debt are zero; terminal marker retained; historical reward entitlements remain | Duplicate transitions reject. No pruning of unresolved entitlements. |

The new `split_*` progression/recovery methods require the owner. Deadlines are immutable; there is no renewal ABI. Funding, conversion, fee preparation and minting reject expiry. Delivery explicitly remains available after expiry, and canonical activation does not add an expiry check. Existing permissionless `resolve_pending_open_v3` can finalize a delivered execution after expiry under its health/reserve checks. Owner-funded repayment/release remains available. Donation accounting never authorizes a job to sweep the whole escrow wallet.

**Unsafe-to-deploy gap:** before delivery, an absent or malicious borrower cannot be liquidated through the existing V3 route. Debt can continue accruing while escrow collateral changes value. Owner recovery requires external cash and does not spend escrow cash to repay itself. A future candidate must implement an independently valued, bounded, liquidator-funded seizure and owner-surplus return in the same transaction as repayment, or an equally sound no-debt design. Expiry alone must never authorize seizure, debt forgiveness or cancellation. No claim is made that every intermediate state has autonomous recovery.

Pause semantics are also explicit: funding checks controller borrow pause plus configured cash reserve/enabled state. Conversion/preparation/mint check relevant vault/pool conditions and fresh valuation, but do not recheck cash borrowing/reserve flags at every step. Final activation rechecks cash flag/reserve; it does not newly recheck the legacy controller borrow-pause switch. Repayment/release remains available under cash pause/reserve shortage. A complete pause matrix is a prerequisite for adoption.

## Accounting and failure proofs

The actual-bytecode differential branches compare omitted versus repeated zero LP-fee collection at the same time/ledger, then odd-sized trades, partial exit, range exit/re-entry, changed emissions and owner transfer. They assert identical fee-inclusive NAV and later balances, escrow reward weight zero, and exact scaled reward conservation. The venue's `estimate_withdraw_position` includes owed fees: adding a fee view again double-counts them. This is a bounded differential proof, not a universal fee-view theorem.

Recovery through simultaneous claim and pending-view outage preserves210960 already-booked rewards;196099 later rewards remain quarantined. They are not reassigned from a later cumulative reading. The two-owner in-kind exit proves:

`161387 × 10^18 = 161386 × 10^18 + 999999999640209772 + 359790228`, with zero paid/quarantine in that working-claim branch. Raw reward-token balance also matches received minus paid. Existing historical attribution/waiver operating rules remain those of the preserved baseline; no new attribution policy or pruning is introduced.

The corrected post-repayment transfer-failure fixture uses the official SAC balance structure with zero available cash. Actual receipt repayment returns before the SAC release rejects; every ledger entry, debt and allocation rolls back. The earlier malformed-value fixture produced `Value(UnexpectedType)` and was correctly refused as semantic rejection evidence; it is preserved as diagnostic failure. Missing nested external repayment authorization is proven by the enforcing SDK invocation; its recording-mode unsigned companion is positive execution and is not mislabelled as a negative signature proof.

## What the remaining cost requires

The measured balanced-mint call is approximately131.9M CPU in the smaller state and136.0–136.1M with another owner. The actual Aquarius deposit subtree is about63.7M, not147.9M. The complete transaction also needs fresh fee/config witness reads (~17.1M), fresh composition (~4.7M), final swap quote (~8.8M), deposit quote (~4.5M), two real reward claims (~9.8M), vault accounting and escrow/V3 authorization. These are disjoint cost categories only where the preserved call tree establishes that; inclusive ancestor/child figures must not be summed twice.

Removing obvious repeated balance reads saves only about0.13M in the measured tree. Folding beneficial mint into a single verified claim boundary might remove the additional ~4.9M claim, but would still leave the observed transaction well over100M and needs custody/rounding proofs. Batch venue state reads or a different audited venue/accounting ABI could reduce serialization and repeated calls; those changes have not been implemented or benchmarked. Merely placing a quote or claim in an earlier transaction is invalid without proving freshness at the actual ownership change. There is no established resource headroom yet.

Existing activation's181 SDK entries and prepare-close's120 are independent blockers even though their CPU is below100M and unique transaction footprints may be smaller. Narrowing receipt initialization/read sets requires measured implementation work; moving a getter alone does not remove later dependencies. No extra production stage or policy increase is recommended or implemented.

Direct deposit, normal redeem, in-kind redeem and V3 swap-close are also measured through their unchanged entrypoints in the prototype binary. These are not byte-identical baseline A/B runs: the binary contains added methods and the fixtures have pre-existing liquidity. The separate baseline binaries and earlier full baseline archive are preserved for comparison. In-kind direct exit followed by a separately authorized wallet conversion proves the ownership boundary, but the redemption itself still exceeds100M. A V3 close split cannot send debt-backed paired assets to the owner first: debt, valuation, custody and atomic funded seizure must stay enforceable. No staged V3 close has been implemented or claimed complete.

New escrow-job archival/restoration, larger overlapping job histories, every pool/tick/config mutation, a full pause matrix, true account signatures and live state remain missing evidence. Baseline historical restoration measurements are not transplanted as proof for the new escrow. The current experiment is a measured negative feasibility result, not release approval.


## Final resources and gates

| Profile | Completed tests | Invocation | Lifecycle | Transaction | Checkpoint |
| --- | --- | --- | --- | --- | --- |
| internal-policy | 8 pass / 13 fail | FAIL | FAIL | FAIL | FAIL |
| captured-network-diagnostic | 21 pass / 0 fail | PASS | FAIL | FAIL | FAIL |

The captured transaction rows individually validate; its remaining transaction error is `transaction proof-to-call mapping missing`. The isolated suite deliberately does not substitute for the required full Stage1 map. Internal errors include the actual resource failures and incomplete unsigned captures after those failures. Exact error lists and raw invocation errors are preserved and replayed. Neither profile grants release approval.

The table uses complete captured executions, evaluated against unchanged internal ceilings. Internal acceptance independently passes the prefix through preparation, then stops in mint at100000843 instructions; its later isolated mint and activation tests also fail. Failed partial measurements are never substituted for full stage costs.

| Complete transaction | SDK CPU M | Unsigned tx CPU M | SDK memory bytes | SDK entries | Unique tx keys | SDK writes | Total fee stroops | Internal constraints |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `transfer_spot_to_margin` | 27.325 | 28.918 | 5635667 | 78 | 70 | 9 | 1627604 | within measured numeric limits |
| `begin_open_position_v3` | 30.279 | 32.329 | 8501489 | 100 | 86 | 15 | 4227653 | within measured numeric limits (zero entry headroom) |
| `split_fund` | 49.691 | 50.134 | 6837046 | 82 | 67 | 16 | 2845632 | within measured numeric limits |
| `split_convert` | 28.933 | 29.442 | 6829757 | 35 | 27 | 9 | 497773 | within measured numeric limits |
| `split_prepare` | 79.966 | 80.990 | 12233997 | 49 | 41 | 9 | 587765 | within measured numeric limits |
| `split_mint` | 131.864 | 132.305 | 19336187 | 93 | 67 | 27 | 2006368 | CPU fails |
| `split_deliver` | 24.405 | 24.128 | 8557023 | 73 | 51 | 23 | 2429533 | within measured numeric limits |
| `activate_open_position_v3` | 81.461 | 86.421 | 12288035 | 181 | 145 | 37 | 3959016 | SDK entries fail; unique keys fail |
| `prepare_close_position_v3` | 54.335 | 55.772 | 11273224 | 120 | 93 | 28 | 2334916 | SDK entries fail |
| `swap_close_position_v3` | 187.767 | 191.474 | 20335321 | 173 | 135 | 39 | 2388249 | CPU fails; SDK entries fail; unique keys fail |
| `finish_close_position_v3` | 59.308 | 58.807 | 7003398 | 97 | 67 | 30 | 79948 | within measured numeric limits |

All rows fit the41943040-byte memory and50-write ceilings in these controlled states. This is not stressed-state or restoration headroom. Unsigned memory, authorization/envelope bytes, return/events, read/write byte sizes, rent changes and individual fee components are in the full JSON summary and raw evidence.

| Other complete invocation | SDK CPU M | Memory bytes | SDK entries | Writes | Internal result |
| --- | ---: | ---: | ---: | ---: | --- |
| Direct deposit | 161.691 | 13284988 | 64 | 19 | CPU fail |
| Normal redeem | 139.810 | 10658581 | 64 | 19 | CPU fail |
| In-kind redeem | 130.146 | 10719002 | 70 | 23 | CPU fail |
| Subsequent owner conversion | 7.146 | 2072934 | 17 | 6 | measured conversion fits |
| Existing V3 swap-close | 187.686 | 20210213 | 173 | 39 | CPU fail |

### Total fees and latency

| Captured journey | Transactions | Inclusion | Non-refundable | Refundable | Rent | Total stroops |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Open, including margin funding | 8 | 800 | 464558 | 22279 | 17693707 | 18181344 |
| Open and close, including margin funding | 11 | 1100 | 752807 | 40942 | 22189608 | 22984457 |

These are1.8181344 XLM and2.2984457 XLM respectively at the pinned model, dominated by fixture TTL extensions/new state. They are controlled unsigned estimates, not signed live fee quotes. The separate close quote simulation costs4122 stroops if submitted as another transaction; it is excluded from the11 product transactions. Assertion-only getters and setup are not user lifecycle stages and are separately recorded. No complete internal-policy lifecycle fee total exists because execution fails.

The fixture advances the ledger clock by5 seconds between selected roots and records45 synthetic seconds across the11-transaction journey; it does not measure consensus or wallet latency. The added transactions require additional confirmations in a sequential client. Local per-stage wall time includes shadow tracing/unsigned replay and cannot be presented as network latency. New escrow archive/restore fees and real signed-authorization costs remain gates.


## Proof-to-test map

Names below are exact function suffixes under `test::split_tests` (submodules are recorded in the indexed raw logs). Sources are `tests/split.rs`, `tests/adversarial.rs`, `tests/fee_differential.rs` and `tests/exit_assessment.rs`. The index binds their bytes, all21 completed outcomes, each resource row and each unsigned transaction. This experimental map does not replace `config/cl-proof-map.json` or grant a full Stage1 gate.

| Test | Assertions / scope |
| --- | --- |
| `compiled_split_open_complete` | Separate conversion/preparation; mint/beneficial assignment; delivery; activation; prepare/swap/finish-close; debt zero after close. |
| `compiled_split_funded_expiry_pause_recovery` | Duplicate fund/cancel/expired conversion reject; full repayment and exact cash return through pause/reserve shortage; duplicate recovery rejects. |
| `compiled_split_each_phase_legacy_replay_guards_dust_recovery` | Each funded phase rejects incompatible legacy operations and repeats; donated cash/shares are not assigned to the job; owner-funded recovery preserves allocations. |
| `compiled_split_price_move_stale_fence_reprepare_reserve_pause` | Fresh-price divergence and external-swap fence rejection roll back; same-deadline reprepare; paused final activation rejects; funded recovery succeeds. |
| `compiled_split_claim_failure_mint_and_delivery_retry` | Actual claim failures stop mint/delivery without partial transitions; retry after restoring upstream operation. |
| `compiled_split_both_reward_calls_outage_preserves_escrow_beneficiary` | Delayed escrow beneficiary earns rewards; both pending-view and claim outage; recovery preserves booked entitlement and later quarantine. |
| `compiled_split_auth_and_post_repayment_release_failure_atomicity` | Empty auth and missing nested external repayment auth reject; valid SAC cash shortfall rejects release after repayment; complete rollback and later recovery. |
| `compiled_split_expiry_before_fund_and_after_conversion` | No-debt expiry releases reservation; expired funded preparation/legacy resolution reject; paired assets recover after full repayment. |
| `compiled_split_separate_convert_prepare_roots_and_recovery` | Conversion and fee preparation are distinct roots/ledgers; exact intermediate job allocations and funded recovery. |
| `compiled_split_isolated_{convert,prepare,mint,delivery,activation}` | Five separate tests execute each complete later transaction from an explicitly captured-profile prestate. They establish individual limits only. |
| `compiled_split_fee_zero_recollection_future_rounding_partial_exit` | Actual-bytecode differential: recollect versus omit zero fee collection; later rounding, partial exit and owner transfer; scaled conservation. |
| `compiled_split_fee_zero_recollection_range_emission_matrix` | Same differential through range exit/re-entry, emission change and later accrual; exact conservation and escrow reward weight zero. |
| `compiled_exit_assessment_direct_deposit_baseline` | Unchanged deposit entrypoint mints positive shares equal to the physical balance delta. |
| `compiled_exit_assessment_normal_redeem_baseline` | Unchanged redeem entrypoint burns shares and pays actual underlying delta. |
| `compiled_exit_assessment_in_kind_then_owner_conversion` | Two owners, real trading fees/rewards, donated dust; fee-inclusive proportional in-kind payout; later owner conversion; exact rewards. |
| `compiled_exit_assessment_standalone_owner_conversion` | Standalone real Aquarius owner swap with narrowly scoped token auth and exact balances. |
| `compiled_exit_assessment_v3_swap_close_baseline` | Existing compiled swap-close from explicitly captured prestate; measured close and final debt settlement. |

The internal8 passing tests are the two early/funded recovery cases, exact auth/rollback case, separate conversion/preparation/recovery case, isolated conversion/preparation/delivery and standalone owner conversion. The other13 fail on resources; later assertions in those tests are not counted as completed. Setup panics from earlier diagnostic runs are never authorization proofs.


## Reproduction, preserved artifacts and CI

Run the three commands in [the prototype README](../../peridot-contracts/prototypes/cl-split-transactions/README.md) individually; inspect both `gates.json` files, raw Cargo outcomes and errors. The [durable archive](evidence/cl-split-prototype-20261102/README.md) includes separate rebuilt baseline/prototype WASMs, pinned source and full raw evidence. Its offline replay compares per-profile gates/errors and complete test outcomes, not aggregate status.

The initial shared-output Cargo build labelled a prototype vault as baseline. Fresh separate baseline/prototype build directories corrected that ordinary tooling defect. The actual measured prototype identities were unchanged; the invalid baseline label and earlier setup/auth/fault diagnostics remain archived. Setup panic and malformed token state are not counted as semantic authorization/transfer rejections.

The bounded independent-author review corrected the deadline description above. It found no further concrete escape in its limited pass; it is not a security audit or production approval. Remaining gaps are listed explicitly, and missing required Stage1 flows remain failed gates. Baseline resource policy alternatives remain separate in handoff12; historical attribution operations remain in handoff13. Neither policy nor shared00 changed.

Linux baseline commit9cf43082bfb57334d57dbc58cfa90ccad4d98078 completed workflow41473099204 with checker-tests PASS/checkpoint-gates FAIL/upload PASS. Captured all4 gates PASS; internal all4 FAIL with106 lifecycle and303 transaction errors. It ran36 main compiled tests +5 archival tests under captured limits, with native reward53, default margin110 and checker32 tests passing. This CI result covers that earlier baseline, not the isolated prototype. The new prototype is locally executed evidence; a future workflow status for this commit must not be interpreted as executing the prototype unless the workflow actually invokes it.


## Supplemental late recovery limit enforcement

After the main21-case run, two isolated recovery tests were added. The original
21 test functions remain byte-identical; only a module declaration was appended.
Their original source remains in the main archive. The supplementary archive
binds the added source, same WASM identities and its own complete raw reports.
Both new tests pass captured limits and fail the explicitly enforced internal SDK
entry limit after the call returns. Combined executed coverage is23 cases:
23 captured passes;8 internal passes and15 resource failures. These are two
preserved runs, not a claim that the original21-case log contains23 tests.

| Supplemental test | SDK CPU | Memory bytes | SDK entries | Writes | Internal / captured |
| --- | ---: | ---: | ---: | ---: | --- |
| `compiled_split_late_recovery_minted` |36781465|9743060|127|40|FAIL / PASS|
| `compiled_split_late_recovery_delivered` |37682204|8538749|123|39|FAIL / PASS|

Both use an explicitly captured prestate and then enforce the chosen profile for
the entire repayment/release transaction, including real contract authorization,
claims and cleanup. Thus code-level recovery availability does **not** imply it
is executable under internal limits. That limitation applies to the state table
above. The both-reward-calls outage recovery in the main suite also measures122
entries. Recovery must fit policy before any staged design is adopted.

The current reproduction filter runs all23 cases; use filter `late_recovery` to
rerun just this supplement. The [supplemental archive](evidence/cl-split-prototype-20261102/late-recovery/README.md)
keeps its gate errors and resource rows separate. Its checker also retains all
missing Stage1 lifecycle/transaction-map failures; no synthetic passing row was
added.

Both prototype archives were replayed offline: each profile's exact gates/error lists, raw invocation blocks, completed outcomes and WASM/source hashes match. Replay exit1 correctly preserves the failed checkpoint; it is not treated as CI approval. The32 existing checker tests also pass after registering the separate9cf Linux capture.


The completed [9cf Linux archive](evidence/cl-linux-ci-9cf4308-20261102/README.md)
is preserved separately from both earlier Linux captures: all75 original members
plus22 pinned replay sources, ten separate Linux WASM identities, exact named
outcomes and both profiles' complete gate/error lists. Both profiles replayed
exactly, including invocation resource/error blocks. The original artifact13158618482
expires2026-11-16T16:18:07Z; durable preservation no longer depends on that expiry.

For clean Git whitespace checking of patch-as-data, the published overlay uses
zero-context diff encoding. `patch-encoding-verification.json` records that its
nine resulting source files are byte-identical to the executed build (with the
same documented OpenZeppelin path substitution). The archives retain the exact
patch bytes that were executed. No source or WASM behavior changed for this
formatting correction.
