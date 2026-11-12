# Atomic Option 1 qualification: duplicate receipt keepalive removal

This source review predates execution. The measured follow-up in handoff16
supersedes its "unmeasured" statements: the isolated patch saves4,660,696
instructions on the combined-history unsigned swap-open. It leaves restoration
write counts unchanged and is not adopted. The additional compatibility cases
listed below remain outstanding unless explicitly mapped by that report.

Candidate only. `optimization.patch` is frozen and must be applied to an isolated copy, not production sources. No build, execution measurement or policy/stage change was performed by this review. Stage 1 remains incomplete. The patch preserves all three actual claim-first ownership boundaries, fee-inclusive mint pricing, fresh valuation, reserve finalization and atomic funded repayment/seizure.

## Exact baseline and patch identity

- Baseline commit: `90111e2dc1a3cc9ffd941eb3661459e6441e1280`.
- Changed file in copied workspace: `contracts/receipt-vault/src/contract.rs`.
- Baseline file SHA256: `2ee32aa7ef77933d23a7895ec12e7f01f73c31be224771558b2e879c37374358`.
- Candidate file SHA256 after patch: `2a1d144359d7d9b7e0e686363e8b9a923901b3c7ecd6cbf1520358c5804ae053`.
- Patch SHA256: `bffb4dcf73404f5e98f7f57c90ae5f896dbcd289f6d7734f4687b16121cdd20e`.
- `git apply --check` passed against the exact baseline, without modifying it.

The patch changes two internal callers and extracts one private helper. No new public selector, policy flag, instance cache, ledger cache, transaction stage or external call is introduced.

## Opportunity and measured context

The preserved ac63255 Linux atomic swap-open trace is235,455,777CPU; the associated complete unsigned transaction peak is236,694,861CPU. At an analysis-only240M workload boundary, that unsigned result leaves3,305,139 instructions before still-unmeasured authentication/signature and stress costs. Saving some CPU is useful; this patch has **no measured saving yet**, and no claim that it covers those remaining costs.

In the peak trace:

| Subtree | Inclusive CPU | Exclusive CPU |
| --- | ---: | ---: |
| Cash receipt `withdraw` | 16,949,918 | 13,886,927 |
| Cash receipt `borrow_for_margin_to_controller` | 12,639,123 | 11,353,839 |
| Cash receipt `begin_margin_withdraw` | 3,497,714 | 3,497,714 |

Source inspection identifies repeated work inside the first two subtrees. Both `withdraw_with_cash_policy` and `borrow_for_margin` call `ensure_initialized` before `update_interest`. `ensure_initialized` already runs `bump_core_ttl` and `bump_borrow_state_ttl`. The original `update_interest` repeats those same two loops immediately before reading fresh interest inputs. Together the loops examine30 core/global borrow keys (22 core and8 global borrow keys), renewing present entries. The patch removes two duplicate passes through these30 keys per atomic open, without deleting either caller's first pass. Trace granularity does not isolate their CPU; the exclusive subtree totals are ceilings on possible local savings, not estimates of actual savings.

The full atomic gate remains the benchmark. The relevant source is receipt `contract.rs:1779–1795,2933–2953,4009–4014` and `storage.rs:128–142,203–288,363–393` at the baseline. Primary trace source: preserved new Linux CI artifact13146343183, ZIP SHA256 `33dcb99170894384466ebe6ac9d0e3d2fc95dfeaec599fc2066eb682f6a56701`, successful peak `swap_open_position_v3` in its call JSONL. Later baseline reserve changes must be measured separately; old quantities cannot substitute for the fresh baseline/candidate comparison.

## Precisely what changes

`update_interest` remains the public wrapper. Its original optional UnderlyingToken lookup and early return remain first; its complete core and global-borrow keepalive loops remain next, in their original order. It then invokes `accrue_interest_after_keepalive`, a private Rust helper containing the original accrual body verbatim, starting with the LastUpdateTime read. All public callers other than the two named below continue using this wrapper.

Only these paths use the private helper directly:

1. `withdraw_with_cash_policy` retains full `ensure_initialized`, flash-loan exclusion, user authorization and user borrow-flag maintenance, then calls the helper at the same point where it previously called `update_interest`.
2. `borrow_for_margin` retains full `ensure_initialized` and flash-loan exclusion, then calls the helper at the same point. Margin-controller authentication, receiver authorization, pause/rates readiness, borrow caps, reserve/liquidity checks, actual transfer deltas and debt writes retain their original order.

The helper has no public export and no caller-controlled boolean. It must never be called on the assumption that another transaction previously renewed the keys. No interest, rate, reserve, cash, oracle, valuation or balance value is cached or passed as a stale hint. Dynamic interest-model calls and all subsequent storage reads/writes/events remain in the original body.

## Same-invocation proof and external effects

The first initialization check runs inside the same receipt invocation and touches exactly the keys that the duplicate loops would touch. Ledger sequence/timestamp cannot advance within that invocation; repeating extend_ttl with the same threshold/target therefore cannot extend these keys farther. Removing the duplicate pass saves host work but does not reduce the final TTL target, restoration requirement, unique footprint or first-pass rent. This is not moving maintenance into a previous transaction or declaring setup costs to be acceptance costs.

For `borrow_for_margin`, the only operation between first initialization and the accrual helper is the flash-loan flag read. It neither creates nor deletes any core/global borrow key.

For `withdraw_with_cash_policy`, the intervening operations are that read, `user.require_auth`, and `ensure_user_borrow_flag`. The latter reads/writes only per-user borrow flag/snapshot/principal information and renews their separate user TTLs; it does not alter the30 core/global keys. Those user writes and renewals remain unchanged. Custom account authorization may execute account code. An attempt by that code to reenter the active receipt and mutate its storage is rejected by the target host's contract reentry rules; another contract cannot directly write the receipt's storage. Primary host29 source: `src/host/frame.rs:112–127` sets external/default calls to `ContractReentryMode::Prohibited`, and lines1150–1177 enforce it. A failed/reentrant authorization still fails before interest accrual, with rollback. Account code may affect unrelated external state; **fresh** subsequent model reads remain present, so this optimization does not cache away that effect.

The old internal update wrapper's repeated UnderlyingToken presence lookup is also redundant here: both callers already obtained that exact initialized token address and no intervening supported path can remove it. The public wrapper's early-return behavior is unchanged, including the unusual case of an uninitialized contract with no underlying token. Missing required interest data still fails at its original accrual read. Body ordering and arithmetic are unchanged.

Reentrancy rejection is a runtime assumption that should be exercised in the compiled compatibility tests, not replaced by a synthetic account-authority success claim. Existing narrow authorization trees stay intact.

## What this does not optimize

It does not remove an Aquarius quote, fee collection, position-composition refresh or any actual reward claim. Reusing a quote across mutations was rejected: stale quotes and unknown tick changes can invalidate valuation. It does not remove token balance deltas, custody authorization, or reserve checks. It does not apply a broad keepalive skip to other receipt calls or use a same-ledger persistent flag that could make later transactions look artificially cheap.

Expected footprint and real rent should remain unchanged because the same first pass remains. CPU/memory savings are unmeasured until actual compiled baseline and candidate runs. Direct vault deposit/redeem contains no changed receipt path and should not be represented as improved by this patch. The isolated split-transaction prototype is unrelated and remains separately archived.

## Focused compatibility qualification

The static-zero-interest compiled CL matrix is necessary for ownership/authentication/resource regression, but insufficient to prove nonzero accrual. Benchmark both compiled baseline and candidate under the same pinned host29 cost model and selected profile, with matching fixture inputs, contract addresses, ledger times, identity configuration and saved pre-state. Compare business-state bytes, events, token deltas and every relevant live-until value; normalize only code/instance WASM identity differences, never accounting values.

Required additional compiled scenarios:

1. **Dynamic interest through direct withdrawal.** Establish a real outstanding margin debt using the compiled V3 and receipt, configure a controlled nonzero dynamic rate model and nonzero reserve/admin factors, then advance timestamp by a day (large enough to produce nonzero integer interest). Invoke a direct authorized cash-receipt withdrawal with sufficient unlocked pTokens. Compare baseline/candidate BorrowIndex, LastUpdateTime, TotalBorrowed, TotalBorrowPrincipal, TotalReserves, TotalAdminFees, AccumulatedInterest, ManagedCash, receipt supply, payout and accrual/withdraw events. Also assert interest actually increased and the factor splits are nonzero. A getter that calls public update_interest before the tested withdrawal would defeat the test; observe pre-state directly in the controlled fixture instead.
2. **Dynamic interest through margin borrowing.** With an existing live debt and elapsed nonzero time, begin a distinct pending V3 open and execute the actual compiled swap-open so its new `borrow_for_margin_to_controller` reaches the changed helper through real authorization. The earlier cash withdrawal may already accrue the shared cash/debt vault, so a dedicated supported configuration with distinct margin and debt receipts, or a direct real-controller test entrypoint in an explicitly synthetic fixture, is needed to isolate nonzero accrual inside `borrow_for_margin` itself. Do not claim the second helper's time-crossing path was tested merely because the earlier withdrawal accrued interest. Keep native accounting tests separate from actual V3 authorization evidence.
3. **Near-TTL and restored state.** For each changed caller, set the30 existing core/global entry live-until values just above the test ledger but below TTL_THRESHOLD (controlled snapshot only). Execute the call as the first receipt operation, without an intervening getter/keepalive. Compare all post-call live-until values and first-pass rent with baseline; assert present entries renew and absent optional entries remain absent. Repeat through the existing unsigned restored-snapshot path; include contract instance/code and touched debt/user state. Restore fees and bytes stay in evidence.
4. **Failures after accrual.** Withdraw more shares than available and attempt a margin borrow blocked by pause/reserve/borrow-cap or transfer failure after interest has been computed. Compare full pre/post ledger data and TTL rollback; no partial interest/maintenance/debt mutation may survive rejection. Keep controller/receiver/amount/recipient authorization negative cases and account-auth reentry rejection.
5. **Public-wrapper parity.** Call public update_interest on missing-underlying state, same-ledger state, nonzero elapsed time, zero rounding accrual, dynamic-rate change/failure fallback, combined-factor violation and borrow-index overflow. Compare results/errors, order, footprint, events and final state. Verify the private helper is absent from exported WASM ABI.

Existing unit tests can supply **native accounting regression only**, without editing them: `test_margin_borrow_repay_happy_path`, `test_withdraw_with_ptokens`, `test_borrow_for_margin_respects_peridottroller_pause`, `test_borrow_for_margin_requires_init_margin_borrow_state`, `test_interest_model_borrow_accrual_and_reserves`, `test_borrow_interest_accrues_and_index_updates`, `test_update_interest_same_ledger_prepares_full_accrual_footprint`, `test_update_interest_zero_rate_prepares_positive_rate_writes`, `test_update_interest_does_not_advance_time_when_rounds_to_zero`, `test_update_interest_reverts_on_borrow_index_overflow`, `test_update_interest_rejects_legacy_combined_fee_over_cap`, and the three local-only withdrawal collateral tests. Several existing native fixtures use blanket mocks; they must not be counted as proof of this task's real authorization tree and should not replace the compiled narrow-auth checks. The existing margin happy-path test is zero-rate and does not fill the dynamic-margin gap. The existing core-TTL test exercises permissionless keepalive, not either changed caller, so it alone does not fill the near-TTL gap.

No new test is claimed as passed in this review. The lead owns benchmark/test implementation and publishes honest missing proofs. Any measured reduction is evidence about the new build, not release approval, and does not adopt the proposed240M workload boundary or alter the existing100M policy gate.
