# Measured resource feasibility — preserved Linux checkpoint

Stage 1 remains incomplete. This is offline analysis of existing protocol 29 execution, not a new acceptance run or a policy change. No transaction stages or claim-first boundaries changed.

Artifact SHA256: `9b01db8227a07f0b8a4bc13dfbf9b043002df79f0c6de068d126e3dd74d87d03`. Captured settings SHA256: `a28fb64bb85c74f8f0d1d4bd7a0daf413ada6f603647c64f556a7905af554dcd`. Source CI run 41434868065 / commit da4e1a598b7c652c79745836417d9943e561c6f2. Linux WASM hashes are separately included in the machine report; they must not be replaced by local hashes.

## Per-flow requirements

These are maxima across successful captured-network invocations; columns can come from different samples. Internal limits remain 100M CPU / 41,943,040 memory bytes / 100 SDK entries / 50 writes. SDK entries is read+write+disk-read, not serialized unique footprint. The original internal-profile executions fail; this table quantifies the deficit using complete captured-network executions, not truncated 100M runs.

| Flow | CPU | Memory bytes | SDK entries | Writes | CPU reduction needed | Entry reduction needed | Write reduction needed |
|---|---:|---:|---:|---:|---:|---:|---:|
| deposit_underlying | 166,914,792 | 14,418,955 | 70 | 22 | 66,914,792 | 0 | 0 |
| redeem | 139,752,097 | 10,640,393 | 65 | 19 | 39,752,097 | 0 | 0 |
| receipt_custody_ownership_change | 34,389,776 | 6,595,716 | 124 | 25 | 0 | 24 | 0 |
| swap_open_position_v3 | 235,455,712 | 24,514,438 | 171 | 45 | 135,455,712 | 71 | 0 |
| activate_open_position_v3 | 78,076,392 | 13,326,592 | 182 | 40 | 0 | 82 | 0 |
| prepare_close_position_v3 | 53,764,953 | 11,093,592 | 120 | 28 | 0 | 20 | 0 |
| swap_close | 184,218,244 | 19,900,855 | 175 | 41 | 84,218,244 | 75 | 0 |
| finish_close | 58,855,788 | 6,824,581 | 97 | 30 | 0 | 0 | 0 |
| funded_liquidation | 100,065,588 | 14,591,272 | 213 | 56 | 65,588 | 113 | 6 |
| both_reward_calls_failed_funded_liquidation | 95,508,559 | 13,603,697 | 205 | 53 | 0 | 105 | 3 |
| historical_unresolved_intervals | 94,903,802 | 13,992,113 | 197 | 50 | 0 | 97 | 0 |
| repay_release | 13,741,095 | 3,099,213 | 88 | 18 | 0 | 0 | 0 |
| cancel_after_swap | 44,999,508 | 9,896,063 | 132 | 44 | 0 | 32 | 0 |

## Feasibility conclusions

- `deposit_underlying`: infeasible for wrapper-only optimization with these venue subcalls preserved. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 134,354,879 instructions.
- `redeem`: infeasible for wrapper-only optimization with these venue subcalls preserved. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 126,765,769 instructions.
- `receipt_custody_ownership_change`: currently fails internal limits; feasibility after optimization unknown. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 6,467,527 instructions.
- `swap_open_position_v3`: infeasible for wrapper-only optimization with these venue subcalls preserved. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 147,933,713 instructions.
- `activate_open_position_v3`: currently fails internal limits; feasibility after optimization unknown. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 12,620,023 instructions.
- `prepare_close_position_v3`: currently fails internal limits; feasibility after optimization unknown. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 9,953,507 instructions.
- `swap_close`: infeasible for wrapper-only optimization with these venue subcalls preserved. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 130,981,925 instructions.
- `finish_close`: observed invocation fits these four limits; stress/release sufficiency unknown. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 0 instructions.
- `funded_liquidation`: currently fails internal limits; feasibility after optimization unknown. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 12,629,062 instructions.
- `both_reward_calls_failed_funded_liquidation`: currently fails internal limits; feasibility after optimization unknown. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 2,474,125 instructions.
- `historical_unresolved_intervals`: currently fails internal limits; feasibility after optimization unknown. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 2,473,605 instructions.
- `repay_release`: observed invocation fits these four limits; stress/release sufficiency unknown. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 0 instructions.
- `cancel_after_swap`: currently fails internal limits; feasibility after optimization unknown. Preserved venue mutation/claim subtree floor at its CPU-worst sample: 2,160,121 instructions.

## Swap-open attribution and bounded optimization assessment

Exclusive CPU by role sums exactly to the root. Inclusive subtrees include children and must not be summed unless disjoint. Memory is modeled global high-water growth with refunds, not process RSS; attributed growth is not each contract's independent peak or removable allocation. Signed ledger deltas are not additive footprint counts.

| Role | Exclusive CPU | Attributed memory counter growth bytes |
|---|---:|---:|
| aquarius | 163,797,778 | 8,231,801 |
| cash_receipt | 28,738,480 | 714,925 |
| vault | 25,794,262 | 1,027,528 |
| underlying | 3,620,620 | 190,629 |
| lending_controller | 3,327,873 | 2,739,783 |
| v3 | 3,199,719 | 1,415,777 |
| plane | 2,601,962 | 8,654,547 |
| reward | 1,361,280 | 0 |
| paired | 1,244,132 | 99,920 |
| facade | 789,722 | 0 |
| adapter | 680,799 | 1,268,053 |
| unlabelled | 299,085 | 171,475 |

Required current venue mutation/claim calls (disjoint inclusive subtrees):

| Path | Aquarius function | CPU |
|---|---|---:|
| 0.8.5.3 | claim | 6,598,979 |
| 0.8.5.5 | claim_all_position_fees | 53,570,569 |
| 0.8.5.14 | swap | 10,955,817 |
| 0.8.5.20 | deposit_position | 64,865,976 |
| 0.8.7.1 | claim | 5,973,639 |
| 0.10.1 | claim | 5,968,733 |

At this measured state, fee collection plus position deposit alone exceed 100M. Keeping the present pinned venue path means making every non-Aquarius component free still cannot meet 100M. This is a conditional lower bound for this path/state, not a theorem about all conceivable equivalent venue algorithms. Fees must remain in NAV before ownership changes; simply omitting collection/valuation is not an acceptable saving.

The three Aquarius `claim` calls are deposit, transfer and custody assignment. All three stay required, including zero-reward claims. Their CPU is not an optimization allowance.

An intentionally unrealistic deletion of every repeated `balance`/`get_*` call after its first occurrence would save at most 7,033,065 observed instructions and still leave 228,422,647. This is NOT a safe or achievable saving: arguments can differ, state changes between reads, balance checks protect donated-dust and custody invariants, and getters include guards. The detailed candidates are in JSON. A correctness-preserving cache may only reuse an identical read while all dependencies are unchanged; its achievable saving remains unmeasured.

Other bounded investigation targets: pass an immutable config snapshot inside one contract invocation instead of deserializing it repeatedly; consolidate legacy receipt/controller configuration into bounded records; avoid redundant local NAV serialization while retaining fresh quote, slippage and donation checks. Current call instrumentation cannot isolate these inline costs. No positive savings claim is made until a before/after target-runtime replay proves all boundaries, rollback, valuation and reward conservation unchanged. Packing unrelated mutable records can increase write bytes, restoration rent, contention and data-entry size, so fewer keys alone is insufficient.

Measured worst swap memory headroom is 17,428,602 bytes (41.55%). CPU and entry cardinality bind before memory in these samples. Four/16-holder stressed pool traces do not establish arbitrary historical-state headroom. New larger-history and archive measurements must be reported separately, never inferred from this spare memory.

## Serialized transaction footprint

| Actual entrypoint | Max CPU | Max unique footprint | Max writes |
|---|---:|---:|---:|
| swap_open_position_v3 | 236,694,758 | 128 | 46 |
| activate_open_position_v3 | 80,777,680 | 143 | 40 |
| liquidate_funded_v3 | 112,983,269 | 158 | 57 |
| swap_close_position_v3 | 188,845,767 | 135 | 41 |

These actual ledger keys are already lower bounds on what the unchanged state layout touches; deleting repeated reads cannot delete the first access to each key. They are not immutable lower bounds after a schema/architecture redesign. Machine JSON includes read-only/read-write key counts grouped by observed contract role for each CPU-worst transaction. CPU-worst and footprint-worst transactions may differ. No subtraction of contract footprints assumes that their guards or balances are dispensable.

## Architectural alternatives (unimplemented; limits and stages unchanged)

1. Compact receipt/controller configuration and narrow request-local caching. Most promising for activation and custody flows whose CPU already fits but entry counts do not. Measure changed key counts and restore rent; keep all reserve/auth/valuation checks. This cannot solve the measured swap venue CPU floor by itself.
2. A CL accounting/venue path that establishes exact pre-mint fee-inclusive NAV more cheaply, or a revised venue implementation that computes equivalent fee/position updates with less work. Requires a new reviewed algorithm and pinned-bytecode evidence, partial-exit/donation/tick/rounding equivalence and all three reward claims. It is an architectural dependency, not a safe deletion of today's `claim_all_position_fees` call. No such replacement is measured here.
3. Redesign storage ownership between V3, receipt and vault to reduce duplicate bookkeeping while retaining transferable shares and beneficiary history. Requires explicit migration, historical reward, auth and reserve proofs; writes may move rather than disappear. Combining wrappers alone does not remove the venue cost floor.

No additional transaction stages are proposed or implemented by this assessment. Funded repayment and permitted seizure remain atomic. Any future staged proposal would need a complete intermediate-state recovery design and separate approval.

## Policy alternatives (not implemented or approved)

Retaining the current architecture has measured demands above internal CPU/entries/writes but below the captured-network ceilings. Raising internal budgets is a distinct risk-policy decision; captured-network diagnostic success does not authorize it. A proposal would need justified stress bounds, safety headroom, network-config freshness, complete transaction fees/restoration/auth evidence and review. No numeric policy change is recommended as an automatic fix.

## Existing archival baseline

All 297 restoration observations target vault entries in cloned controlled state across three historical test reruns. Max archived invocation CPU is 171,514,028; max disk reads are 2,580 bytes. Fee component maxima (stroops, potentially different rows): {"inclusion": 100, "non_refundable": 151343, "refundable": 13663, "rent": 22686852, "total": 22845643}. Total-fee maximum is inclusive of rent; do not add rent again. These unsigned estimates exclude real signature verification and do not prove an arbitrarily long archived history can fit. The machine report lists the exact test names.

## Evidence limits and reproduction

Analyzed 590 SDK rows, 1459 unsigned transaction rows and 297 restoration observations from the preserved Linux artifact. The protocol 29 host uses captured cost coefficients; these are actual controlled-state target-runtime measurements, not protocol 25 quantities repriced with 29 fees. Exact live-state, real account-signature and production release validation remain unavailable. Internal gate summary remains all four FAIL; captured summary remains all four PASS; release approval remains false. Detailed gate errors are preserved in JSON.

```sh
python3 scripts/analyze_cl_feasibility.py --durable-evidence target/cl-next-linux-v2 --output-dir target/cl-next-feasibility
```

The script verifies the artifact/config/profile pins, requires each reported flow and reconciles every exclusive CPU tree. It fails on missing evidence. It is an analysis tool, not a replacement for `check_budget.sh`, lifecycle proofs, transaction gates or release approval.
