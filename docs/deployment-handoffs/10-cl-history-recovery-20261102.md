# CL Stage 1 historical accounting and recovery checkpoint — 2026-11-02

**WIP / not production-approved.** Continues PR75 from `14ef4d813b476a0c06bd9a9f9792834b29b3f03a`; both that checkpoint and `64093aa3c0f77ab510ebf110138195405bb0d430`, their machine manifests, and their two resource profiles remain intact. This report does not modify shared00. Borrowing preparation remains disabled, production maintenance margin unset, and no signing, broadcasting, key/secret access, Mainnet transaction or Upshift binding occurred.

## Accounting model and limits

Each beneficiary has immutable sparse weight checkpoints. Effective weight includes beneficial receipt/margin custody and survives full exit. Ownership changes append a checkpoint and, during recovery, an accrual segment at the ownership boundary. A tracked-weight/supply completeness guard rejects settlement if an older holding has no history. Liquidation and emergency withdrawal append bounded state; neither enumerates holders or old intervals.

Settlement is incremental: at most four waiver records per preparation call, each using a binary search bounded by 32 historical reads, then one beneficiary/segment per settlement with an explicit adjacent-checkpoint proof. Settlements are idempotent. Original scaled entitlement, waiver redistribution, later segments and unresolved backing have separate counters. There is no automatic cleanup of immutable history; storage growth and archival restoration are operational costs, not free garbage collection.

Automatic attribution is validation-only, opt-in, and pins the Aquarius executable hash on every boundary. Missing/decreasing pending evidence or incomplete ownership history leaves the interval unresolved. A later cumulative reading cannot repair a missing earlier boundary. Automatic and governed attribution remain separate: governed allocation requires a nonzero published evidence digest, a 24-hour delay and explicit governed events. It does not become an automatically verified observation. No voluntary waiver discards a verified original entitlement or another owner's entitlement; forced liquidation never waives the borrower.

Allocation is proportional to `min(received, expected)` across segments and then historical weights. Attributable later accrual has its own segment; unexplained excess and exact division remainders remain reserved/quarantined. Every conservation result below includes scaled pending/fractions/rounding/backing rather than treating integer truncation as lost rewards.

## Evidence classifications

- Native SDK25 tests exercise arithmetic and adversarial state fixtures. Their injected observations are not venue evidence.
- The compiled SDK/host29.0.1 suite executes the real V3, receipt/cash markets, share vault, facade, adapter and pinned Aquarius WASM. Oracle/plane are compiled synthetic fixtures; SAC assets and initialized pool state are controlled. Admins use `Address::generate` with narrowly scoped external authorization. Contract authorization for V3/facade/vault remains real, with no blanket mock.
- The automatic boundary matrix uses actual Aquarius pending/claim reads at ownership changes, emission updates, out-of-range/return tick paths, partial exit and rounding. The compiled shortfall case deliberately supplies a governed synthetic expected table twice the actual receipt; it proves proportional allocation, **not an observed automatic Aquarius shortfall**. An underfunded Aquarius claim traps atomically; it is not a partial receipt.
- The simultaneous claim/pending-view outage test corrupts the controlled pool's per-vault user entry while retaining the exact bytecode, restores it afterward, and verifies late quarantine. This is explicit dependency-fault injection, not a claimed naturally occurring live state.
- Unsigned transaction evidence uses official protocol29 host embedder execution on a cloned ledger, captured coefficients and archival configuration. It records external authorization only; unexpected recorded contract authorization fails. Unauthorized negative cases use enforcing-empty auth. Actual serialized unsigned envelopes, authorization trees, footprint, return/events, entry sizes, restoration, rent and complete fee components are independently decoded/recomputed. No signatures are created or verified; signed-envelope overhead and actual account-signature cost remain unmeasured. Inclusion fee is explicitly a controlled 100-stroop setting, not a live-network congestion quote.
- Archival evidence expires actual persistent vault keys in a cloned controlled snapshot and executes the same entrypoint through host auto-restoration. It compares return values and effective post-state contract data/code against the unarchived baseline and records restored key names, read/write indices and rent. It is not a live archival simulation or a production TTL-retention guarantee.

## State machine and recovery

Actual ordering is **begin → swap → activate**, with `execute_open_position_v3` an alternate activation finalizer after swap. Shared00's begin/execute/swap/activate description is outdated and needs coordinated reconciliation. This checkpoint flags the conflict without independently changing the shared policy. The source-reviewed full public V3 map is [v3-state-machine.md](evidence/cl-history-recovery-20261102/v3-state-machine.md); routes outside the concrete test map remain source-reviewed only.

`cancel_after_execute` is a composite rejection/release proof: execute explicitly activates, pending cancellation and repeated execute reject without ledger changes, then owner full repayment and release succeed. It is not a new cancellation route. Unswapped cancellation/expiry, swapped expiry, failed activation, permissionless resolution and funded pending recovery are separate tests. Funded liquidation adverse cases prove entire-ledger rollback for missing auth, insufficient external funding, invalid valuation, collateral transfer failure after debt repayment returns, and repeat execution. Funded repayment and permitted seizure remain atomic.

## Reproduction and gate semantics

From `peridot-contracts`, with pinned CLI26.1.0 and Rust1.96.0:

```sh
npm ci --prefix bots/margin-liquidator --ignore-scripts
node scripts/fetch_cl_pool_fixture.cjs
CL_CAPTURED_CONFIG="$PWD/../docs/deployment-handoffs/evidence/cl-resource-reconciliation-20261102/network-config.json" \
AQUARIUS_CONCENTRATED_WASM="$PWD/target/cl-share-checkpoint/aquarius-pool.wasm" \
CL_CHECKPOINT_DIR="$PWD/target/cl-history-reproduction" \
bash scripts/run_cl_share_checkpoint.sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -p 'test_cl*py' -v
```

Use a new output directory; the runner refuses overwriting a checkpoint. All WASM builds use `stellar contract build`. The runner executes both profiles, complete tests and archival reruns, binds proof mappings to source/log/report hashes, and runs every `check_budget.sh` gate. Expected overall failure is preserved if any policy, required proof or transaction check fails. Native funded host25 results remain separately labelled and cannot satisfy protocol29 gates.

Invocation limits, completed lifecycle assertions, full unsigned transaction evidence and production release approval are independent. A numerical row before a later test assertion fails is not lifecycle proof. Negative transaction rows bind exact per-case identifiers and unchanged-state hashes; one rejection cannot stand in for all adverse cases. Missing required flows/evidence still fail; release approval is never inferred from a captured-network profile passing.

## Remaining production proofs

No live PYUSD/USDC state, production oracle/depeg/capacity conditions, signed account authorization, production reserve enforcement, deployment/configuration readbacks, funding deployment budgets or final security audit are established. The receipt reserve ABI implemented in checkpoint08 remains present (`set_min_cash_reserve_bps`, `get_min_cash_reserve_bps`, `get_borrowable_cash`, `assert_borrowing_reserve`, `set_cash_borrowing_enabled` and its getter). Complete-open reserve integration, deployment/readbacks and funding lifecycle budgets remain separate dependencies; no setter is invented and borrowing stays paused. The tested holder/interval/tick sizes are explicit finite samples, not an arbitrary-state headroom guarantee. Both missing boundary quarantine and governed evidence remain operationally relevant. Automatic shortfall under a legitimate partial Aquarius receipt is not claimed. Production approval remains absent even if all controlled-state captured-network gates pass.

## Execution results and gate status

| Evidence | Result |
|---|---|
| Native vault/accounting | 53 PASS |
| Captured-network compiled | 24 distinct tests PASS (main23 + supplemental1); archive3 and allowance1 reruns PASS |
| Internal-policy compiled | 2 PASS /22 FAIL; archive0 PASS /3 FAIL |
| Native funded host25 | Captured ceilings9 PASS; internal5 PASS /4 FAIL; not protocol29 proof |
| Checker regressions | 13 PASS |
| Captured invocation / lifecycle / unsigned transaction / checkpoint | PASS /PASS /PASS /PASS |
| Internal invocation / lifecycle / unsigned transaction / checkpoint | FAIL /FAIL /FAIL /FAIL |
| Production release | NOT GRANTED |

The source runner now evaluates all four gates in one `check_budget.sh --gate checkpoint` pass; it retains nonzero status for any failure. Earlier local batches predated the final supplemental tests; their commands and failures remain in the working checkpoint. The published manifests bind every complete supplemental log and exact exit code. The full reproduction script runs all24 together.

Reached stages include underlying→shares→underlying, real nested authorization, receipt/margin reward custody, swap/activation/close, explicit execute-finalization, repayment/release, funded in-kind liquidation and interrupted-open recovery. Negative cases are contract executions after successful setup, not setup panics. The34-label machine proof map is [cl-proof-map.json](../../peridot-contracts/config/cl-proof-map.json).

## Reward conservation

All amounts are raw reward units; scale S=10^18. Conservation assertions include pending, fractions, rounding, paid and quarantined backing.

| Compiled scenario | Exact result |
|---|---|
| Seven actual Aquarius ownership boundaries, changed emissions, range exit/return, partial exit | 123264*S =123262*S +1999999999866792967 fractional +133207033 rounding; quarantine0 |
| Automatically observed history, exited owner | 25042*S =25041*S +(S−1) fractional +1 rounding; newcomer old entitlement0 |
| Governed waiver; original coowner exits all receipt/margin/direct custody before arrival | 85017*S =85016*S +(S−1) fractional +1 rounding; original coowner63420*S−1, newcomer later21597*S, newcomer original interval0 |
| Three sealed unsettled intervals | 52723*S =52722*S +(S−4) fractional +4 rounding; exited owner retained; newcomer original intervals0 |
| Governed synthetic shortfall | Actual65232 vs explicitly synthetic expected130464; exited owner43488*S and newcomer later21744*S; exact conservation, replay0; not automatically observed shortfall |
| Three missing-boundary intervals | Booked115776 preserved after full borrower exit; late347331 quarantined; six missing boundaries remain unresolved after later reads; settlement rejects; scaled conservation passes |
| Delegated allowance transfer | 45464*S =45463*S +999999999936292250 fractional +63707750 rounding; spender0; failed-claim allowance/balance rollback |

Unexplained excess/shortfall arithmetic and missing-boundary cases also have native runtime tests. Automatic attribution is an empirical, pinned-bytecode validation assumption, not a formal guarantee across every possible venue state.

## CI

There were no tracked CI workflows/triggers at the previous checkpoint. Added `.github/workflows/cl-stage1.yml` for pull requests and manual dispatch, with read-only permissions, pinned action/CLI/toolchain versions, public hash-pinned code retrieval, no secrets and always-uploaded evidence. The checkpoint job must remain failed while internal policy fails; no `continue-on-error` or swallowed exit converts this checkpoint into approval. Remote status belongs to the published commit and is reported on PR75.

## Post-change resource measurements

Both profiles use captured ledger65264920 and all86 CPU/memory coefficients with SDK/host29.0.1. The original ledger65264375 capture remains preserved. Limits are 100M/100entries/50writes for internal policy and400M/400entries/200writes for captured network; both retain41,943,040 memory bytes and every other configured cap. The SDK entry total below is its `read + disk + write` accounting; the unsigned transaction table uses the distinct serialized footprint.

| Profile | Max CPU | Peak memory bytes | Max SDK entry total | Max writes | Failed invocation rows |
|---|---:|---:|---:|---:|---:|
| internal-policy | 100,004,098 | 16,797,614 | 136 | 30 | 25 / 68 |
| captured-network-diagnostic | 235,455,376 | 24,514,438 | 213 | 56 | 0 / 602 |

Internal measurements include halted calls: their lower memory is not a completed-flow improvement. All raw rows and both complete CSV tables are retained. Missing/unreached policy flows remain failures.

| Captured invocation (maximum per column) | CPU | Memory bytes | SDK entries | Writes | Internal invocation |
|---|---:|---:|---:|---:|---|
| `deposit_underlying` | 166,916,088 | 14,418,955 | 70 | 22 | FAIL |
| `redeem` | 139,752,817 | 10,640,393 | 65 | 19 | not reached |
| `receipt_custody_ownership_change` | 34,387,568 | 6,595,716 | 124 | 25 | not reached |
| `swap_open_position_v3` | 235,455,376 | 24,514,438 | 171 | 45 | FAIL |
| `activate_open_position_v3` | 78,078,792 | 13,326,592 | 182 | 40 | not reached |
| `prepare_close_position_v3` | 53,765,481 | 11,093,592 | 120 | 28 | not reached |
| `swap_close` | 184,219,876 | 19,900,855 | 175 | 41 | not reached |
| `finish_close` | 58,856,796 | 6,824,581 | 97 | 30 | not reached |
| `funded_liquidation` | 100,062,084 | 14,591,272 | 213 | 56 | not reached |
| `both_reward_calls_failed_funded_liquidation` | 95,505,055 | 13,603,697 | 205 | 53 | not reached |
| `historical_unresolved_intervals` | 94,904,906 | 13,992,113 | 197 | 50 | not reached |
| `repay_release` | 13,741,575 | 3,099,213 | 88 | 18 | not reached |
| `quarantine_reconciliation` | 3,242,943 | 1,626,533 | 16 | 6 | not reached |
| `waiver_redistribution` | 4,118,320 | 1,701,006 | 18 | 6 | not reached |
| `governed_proportional_shortfall` | 3,433,100 | 1,684,066 | 17 | 6 | not reached |
| `historical_prepare_segment` | 1,322,105 | 1,408,604 | 6 | 1 | not reached |
| `historical_repeat_idempotent` | 931,264 | 1,361,445 | 3 | 0 | not reached |
| `allowance_transfer_from` | 11,321,733 | 4,371,218 | 53 | 19 | not reached |

Worst measured SDK call has 41.14% CPU and 41.55% memory headroom. These are finite controlled workloads (4/16 holders, shifted pools, actual range crossing, 3 settled and 3 unresolved intervals, exited beneficiaries), not production bounds. No claim-first boundary or transaction stage was removed.

### Call breakdown at the most expensive measured swap-open

Exclusive CPU adds across roles. Memory is maximum observed global high-water growth within calls of each role, **not independent per-contract memory and not additive**. The exact call paths and cost trackers are in the call-profile JSONL.

| Role | Exclusive CPU | Largest high-water growth bytes | Calls |
|---|---:|---:|---:|
| aquarius | 163,797,778 | 4,744,847 | 12 |
| cash_receipt | 28,738,768 | 2,195,567 | 3 |
| vault | 25,794,310 | 12,521,294 | 9 |
| underlying | 3,620,620 | 99,007 | 26 |
| lending_controller | 3,327,681 | 1,326,757 | 4 |
| v3 | 3,200,247 | 24,514,438 | 1 |
| plane | 2,601,866 | 1,239,895 | 7 |
| reward | 1,361,280 | 0 | 9 |
| paired | 1,244,132 | 99,920 | 10 |
| facade | 789,626 | 16,970,804 | 1 |
| adapter | 679,983 | 1,268,053 | 1 |
| unlabelled | 299,085 | 35,155 | 6 |

The trace retains three Aquarius reward claims at deposit, share transfer and custody assignment. Repeated balance/configuration/valuation reads remain measured. This checkpoint adds historical correctness; it does not claim further duplication optimization.

### Full unsigned transaction examples

Rows below select the highest-total-fee ordinary controlled-state sample for each function (no archive injection). Fees are stroops and include CPU/disk/write/history/bandwidth, events plus return, rent and the explicit controlled100-stroop inclusion setting. This is distinct from the partial invocation fee model. The CSV contains every sample, including rejections and restorations.

| Function | CPU | Memory bytes | Footprint / writes | Envelope / auth bytes | Events / return bytes | Rent | Total fee |
|---|---:|---:|---:|---:|---:|---:|---:|
| `deposit_underlying` | 103,378,129 | 9,899,008 | 48 / 22 | 4640 / 372 | 3076 / 20 | 17,719,061 | 17,803,803 |
| `swap_open_position_v3` | 171,467,632 | 18,748,146 | 127 / 45 | 11440 / 188 | 5592 / 4 | 22,458,914 | 22,615,564 |
| `activate_open_position_v3` | 80,776,816 | 11,300,443 | 143 / 40 | 12968 / 192 | 1796 / 4 | 3,842,705 | 3,956,120 |
| `prepare_close_position_v3` | 55,499,933 | 10,250,945 | 93 / 28 | 8704 / 192 | 1588 / 4 | 2,252,229 | 2,332,291 |
| `swap_close_position_v3` | 188,844,615 | 18,525,718 | 135 / 41 | 12368 / 208 | 4248 / 4 | 1,175,082 | 1,328,289 |
| `finish_close_position_v3` | 58,696,362 | 6,229,662 | 67 / 30 | 6128 / 4 | 1796 / 4 | 0 | 74,787 |
| `liquidate_funded_v3` | 99,982,052 | 12,211,199 | 153 / 53 | 14528 / 532 | 2016 / 52 | 7,963,370 | 8,101,770 |
| `recover_pending_open_funded` | 45,749,154 | 8,891,629 | 89 / 44 | 8488 / 520 | 1032 / 20 | 4,945,560 | 5,041,615 |
| `settle_reward_segment` | 4,229,712 | 1,504,746 | 11 / 6 | 1372 / 4 | 0 / 20 | 254,605 | 269,123 |
| `transfer_from` | 11,645,484 | 4,209,663 | 35 / 19 | 3912 / 268 | 588 / 4 | 1,764,290 | 1,806,603 |

Unsigned target-host execution: 1478 rows, 1449 successes and 29 expected rejections; maximum CPU236,694,470, memory21,678,403, unique footprint158. 297 rows actually restore archived keys; all archived return/data-state comparisons match the ordinary baseline. SDK and unsigned-host numbers differ because of their different external-auth/embedding paths; both are independently checked, not mixed or repriced from host25.


## Proof-to-test mapping

Every required label maps to completed assertions, measured resource flows, and exact unsigned functions/outcomes in the machine map. Rejected transactions bind case identifiers; wrong-recipient auth additionally binds changed argument index2.

| Required label | Concrete compiled test(s) |
|---|---|
| `deposit` | `compiled_vault_core_flows` |
| `redeem` | `compiled_vault_core_flows` |
| `transfer` | `compiled_vault_core_flows` |
| `emergency_redeem` | `compiled_vault_core_flows` |
| `claim_rewards` | `compiled_vault_core_flows` |
| `begin_open` | `compiled_v3_checkpoint` |
| `swap_open_position_v3` | `compiled_v3_checkpoint` |
| `activate_open_position_v3` | `compiled_v3_checkpoint` |
| `prepare_close_position_v3` | `compiled_v3_checkpoint` |
| `swap_close` | `compiled_v3_checkpoint` |
| `finish_close` | `compiled_v3_checkpoint` |
| `funded_liquidation` | `funded_tests::compiled_funded_liquidation` |
| `repay_release` | `funded_tests::compiled_repay_release` |
| `claim_failure_recovery` | `compiled_venue_rewards_and_claim_failure` |
| `cancel_before_execute` | `compiled_authorization_rejections_after_setup` |
| `cancel_after_execute` | `recovery_state_tests::compiled_cancel_after_execute_is_rejection_then_repay_release` |
| `cancel_after_swap` | `funded_tests::compiled_interrupted_open_recovery_claim_outage` |
| `receipt_custody_ownership_change` | `compiled_receipt_margin_reward_custody` |
| `allowance_transfer_from` | `lifecycle_tests::compiled_allowance_reward_boundary_and_claim_rollback` |
| `quarantine_reconciliation` | `historical_tests::compiled_automatic_history_exited_owner_and_later_accrual` |
| `waiver_redistribution` | `historical_tests::compiled_governed_waiver_original_custody_coowner_exits_before_receipt` |
| `pending_claim_boundary_matrix` | `venue_boundary_tests::compiled_pending_claim_boundary_matrix` |
| `failed_activation_recovery` | `recovery_state_tests::compiled_failed_activation_preserves_pending_recovery` |
| `expired_pending_before_swap` | `recovery_state_tests::compiled_pending_before_swap_expiry_and_abandonment` |
| `expired_pending_resolution` | `recovery_state_tests::compiled_expired_swapped_permissionless_resolution` |
| `both_reward_calls_failed_funded_liquidation` | `adversarial_tests::compiled_both_reward_calls_fail_forced_settlement_preserves_history` |
| `funded_insufficient_liquidator_rollback` | `adversarial_tests::compiled_funded_adversarial_atomicity` |
| `funded_invalid_valuation_rollback` | `adversarial_tests::compiled_funded_adversarial_atomicity` |
| `funded_failed_collateral_transfer_rollback` | `adversarial_tests::compiled_funded_adversarial_atomicity` |
| `funded_unauthorized_rollback` | `adversarial_tests::compiled_funded_adversarial_atomicity` |
| `funded_repeated_execution_rollback` | `adversarial_tests::compiled_funded_adversarial_atomicity` |
| `historical_multiple_intervals` | `historical_tests::compiled_multiple_unsettled_intervals_exited_beneficiary` |
| `historical_shortfall` | `historical_shortfall_tests::compiled_governed_shortfall_proportional_exited_custody_owner` |
| `historical_unresolved_intervals` | `unresolved_stress_tests::compiled_multiple_unresolved_intervals_after_funded_exits` |


The initial local wrapper was edited while active when redundant four-pass checker decoding was reduced to one pass. Its saved `logs/initial-wrapper-failure.log` records a shell read-position parsing failure after the main/archival suites. It is not claimed as a completed final runner. Final source passes `bash -n`; a frozen full runner is rerun separately. Published aggregate test logs and independent checker outputs completed successfully within the captured profile. The supplemental native funded command separately completed9 tests.

CI run41434002559 for8f3e5fd: checker-tests PASS; checkpoint-gates failed before contract execution because Ubuntu lacked rg in the binary-discovery step. Rust/CLI download and pinned checksum succeeded. The follow-up replaces that optional utility with Python standard-library discovery and retains bootstrap logs as artifacts. This initial CI failure is distinct from the expected conservative-policy failure. Full job log: logs/ci-initial-job.log; https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41434002559 . Artifact relocation/replay also completed: captured four gates PASS, internal four FAIL, aggregate exit1.
