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
