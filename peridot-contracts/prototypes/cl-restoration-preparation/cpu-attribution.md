# Preserved CPU attribution and reserve benchmark decomposition

Stage 1 remains incomplete; no policy, production code, stages or caps changed. This is an offline analysis of pinned existing evidence, not a new execution or qualification pass.

## Attribution method and scope

The SDK29 trees use the captured protocol29 cost model. For every node, this script independently checks exclusive CPU = inclusive CPU minus direct-child inclusive CPU, and verifies the sum of all exclusive nodes equals the root. Role totals below are exclusive. Aquarius inclusive totals include its token/plane descendants; selected Aquarius calls are proven disjoint, so their inclusive totals may be summed. Never add the two accounting views together. Outside-Aquarius costs include Peridot, tokens, oracle, interest-model and synthetic external-auth work; they are not all removable Peridot overhead.

The baseline file name contains data-archive, but its call trees describe the original SDK invocation, not its separate archived companion. SDK diagnostics/shadow work and cumulative cost-type trackers are not added to these trees. Memory counter contributions are not allocation or peak-memory attribution. Recording-only unsigned replay does not prove real signatures or full transaction application.

## Matched SDK roots

| Case | Baseline CPU | Candidate CPU | Saved | Candidate Aquarius inclusive | Candidate Aquarius exclusive | Outside Aquarius subtrees |
|---|---:|---:|---:|---:|---:|---:|
| aged_open_with_actual_pool_deposit | 237,214,100 | 223,144,751 | 14,069,349 | 167,831,028 | 162,817,228 | 55,313,723 |
| later_open_without_pool_deposit | 170,930,685 | 157,056,081 | 13,874,604 | 103,955,308 | 100,785,452 | 53,100,773 |
| swap_close | 188,506,613 | 183,241,286 | 5,265,327 | 141,545,386 | 137,759,524 | 41,695,900 |

The later open reaches estimate_deposit_position but does not call deposit_position; it is not evidence for the full venue-deposit path. The existing deposit_balances code has explicit unchanged/idle returns after an unavailable, invalid or zero-liquidity estimate. The call trace alone does not establish which return predicate fired. Use the aged open with actual deposit when assessing full deposit feasibility.

## Candidate exclusive CPU by contract role

| Role | Aged open | Later open | Close |
|---|---:|---:|---:|
| adapter | 670,012 | 694,828 | 676,784 |
| aquarius | 162,817,228 | 100,785,452 | 137,759,524 |
| cash_receipt | 17,892,058 | 17,913,337 | 5,382,799 |
| facade | 756,054 | 786,341 | 1,138,440 |
| lending_controller | 3,254,215 | 3,296,116 | 3,090,076 |
| oracle | 0 | 0 | 2,212,573 |
| paired | 1,076,592 | 771,746 | 802,049 |
| plane | 2,602,594 | 1,500,630 | 2,188,042 |
| reward | 1,193,967 | 1,097,791 | 776,782 |
| underlying | 3,132,421 | 2,697,046 | 2,047,429 |
| unlabelled | 1,768,934 | 1,720,385 | 954,734 |
| v3 | 3,059,996 | 3,066,610 | 4,650,668 |
| vault | 24,920,680 | 22,725,799 | 21,561,386 |

These are call-boundary ownership labels, not source-line profiles. Unlabelled nodes include the interest model, synthetic external __check_auth and token balance calls. V3/facade/vault inclusive values overlap their descendants; the following view must not be summed with the role table or Aquarius totals.

## Peridot call boundaries: inclusive versus exclusive

| Case | Path / role / function | Inclusive CPU | Exclusive CPU |
|---|---|---:|---:|
| aged_open_with_actual_pool_deposit | 0 / v3 / swap_open_position_v3 | 223,144,751 | 3,059,996 |
| aged_open_with_actual_pool_deposit | 0.4 / cash_receipt / withdraw | 11,851,593 | 8,176,978 |
| aged_open_with_actual_pool_deposit | 0.6 / cash_receipt / borrow_for_margin_to_controller | 10,518,864 | 8,533,676 |
| aged_open_with_actual_pool_deposit | 0.8 / facade / swap | 184,337,342 | 756,054 |
| aged_open_with_actual_pool_deposit | 0.8.0 / vault / quote | 5,625,032 | 3,322,084 |
| aged_open_with_actual_pool_deposit | 0.8.5 / vault / deposit_underlying | 165,098,817 | 9,419,034 |
| aged_open_with_actual_pool_deposit | 0.8.7 / vault / transfer | 9,682,957 | 3,875,841 |
| aged_open_with_actual_pool_deposit | 0.10 / vault / custody_assign | 9,534,260 | 3,749,887 |
| swap_close | 0 / v3 / swap_close_position_v3 | 183,241,286 | 4,650,668 |
| swap_close | 0.4.0 / vault / quote | 4,201,652 | 2,978,860 |
| swap_close | 0.7 / vault / custody_prepare_transfer | 1,075,780 | 1,075,780 |
| swap_close | 0.9 / facade / swap | 160,521,578 | 655,402 |
| swap_close | 0.9.0 / vault / quote | 4,158,152 | 2,966,564 |
| swap_close | 0.9.3 / vault / transfer | 11,060,186 | 4,967,516 |
| swap_close | 0.9.5 / vault / redeem | 141,474,594 | 6,870,891 |

## aged_open_with_actual_pool_deposit: individual Aquarius calls

| Call path | Function | Baseline inclusive | Candidate inclusive | Candidate exclusive |
|---|---|---:|---:|---:|
| 0.8.0.0 | get_is_killed_swap | 1,090,582 | 1,087,231 | 1,087,231 |
| 0.8.0.1 | get_is_killed_deposit | 1,064,238 | 1,063,738 | 1,063,738 |
| 0.8.5.3 | claim | 6,297,073 | 6,196,410 | 5,363,417 |
| 0.8.5.5 | claim_all_position_fees | 54,098,707 | 53,952,286 | 53,275,588 |
| 0.8.5.6 | estimate_withdraw_position | 4,764,058 | 4,770,986 | 4,770,986 |
| 0.8.5.11 | get_is_killed_deposit | 1,067,579 | 1,067,269 | 1,067,269 |
| 0.8.5.12 | estimate_swap | 8,783,512 | 8,785,882 | 8,785,882 |
| 0.8.5.14 | swap | 10,789,172 | 10,778,299 | 10,220,469 |
| 0.8.5.19 | estimate_deposit_position | 4,538,341 | 4,538,505 | 4,538,505 |
| 0.8.5.20 | deposit_position | 64,246,763 | 64,233,503 | 62,549,781 |
| 0.8.7.1 | claim | 5,687,962 | 5,684,682 | 5,050,928 |
| 0.10.1 | claim | 5,712,274 | 5,672,237 | 5,043,434 |

## later_open_without_pool_deposit: individual Aquarius calls

| Call path | Function | Baseline inclusive | Candidate inclusive | Candidate exclusive |
|---|---|---:|---:|---:|
| 0.8.0.0 | get_is_killed_swap | 1,201,004 | 1,194,688 | 1,194,688 |
| 0.8.0.1 | get_is_killed_deposit | 1,062,736 | 1,062,468 | 1,062,468 |
| 0.8.5.3 | claim | 6,603,481 | 6,552,699 | 5,769,158 |
| 0.8.5.5 | claim_all_position_fees | 53,974,870 | 53,919,030 | 53,263,516 |
| 0.8.5.6 | estimate_withdraw_position | 4,566,639 | 4,570,289 | 4,570,289 |
| 0.8.5.11 | get_is_killed_deposit | 1,066,327 | 1,066,119 | 1,066,119 |
| 0.8.5.12 | estimate_swap | 9,169,656 | 9,167,680 | 9,167,680 |
| 0.8.5.14 | swap | 11,084,982 | 11,100,517 | 10,588,471 |
| 0.8.5.19 | estimate_deposit_position | 4,114,912 | 4,114,540 | 4,114,540 |
| 0.8.7.1 | claim | 5,628,542 | 5,596,172 | 4,983,794 |
| 0.10.1 | claim | 5,593,685 | 5,611,106 | 5,004,729 |

## swap_close: individual Aquarius calls

| Call path | Function | Baseline inclusive | Candidate inclusive | Candidate exclusive |
|---|---|---:|---:|---:|
| 0.4.0.0 | get_is_killed_swap | 1,087,356 | 1,081,735 | 1,081,735 |
| 0.9.0.0 | get_is_killed_swap | 1,070,236 | 1,069,738 | 1,069,738 |
| 0.9.3.1 | claim | 6,024,025 | 5,946,047 | 5,225,045 |
| 0.9.5.3 | claim | 5,753,145 | 5,720,808 | 5,076,474 |
| 0.9.5.5 | claim_all_position_fees | 53,358,723 | 53,254,436 | 52,890,781 |
| 0.9.5.8 | withdraw_position | 62,691,711 | 62,561,356 | 61,064,024 |
| 0.9.5.12 | estimate_swap | 4,971,133 | 4,957,303 | 4,957,303 |
| 0.9.5.14 | swap | 6,996,928 | 6,953,963 | 6,394,424 |

## Distinct fresh unsigned host measurements

| State | Aged open CPU | Later open CPU | Close CPU |
|---|---:|---:|---:|
| live | 227,663,611 | 162,770,975 | 189,418,885 |
| data | 229,258,928 | 164,528,869 | 191,235,141 |

These are separate complete host invocations from disposable snapshots, including their selected restoration work, not a sum of SDK subtree quantities. No fresh-host call-level tree was captured. The SDK breakdown therefore identifies measured dependencies without asserting an exact per-contract allocation of the unsigned transaction total or its difference from SDK costs.

## What remains necessary for the unchanged 100M limit

The aged open's Aquarius-only exclusive CPU is already 162,817,228; close is 137,759,524. Even an impossible zero-cost Peridot wrapper would not fit those unchanged venue paths under 100M. Inclusive venue subtrees total 167,831,028 and 141,545,386 respectively. A helper or compact Peridot records alone cannot establish compliance.

Aged open retains claim-first deposit, share transfer and custody assignment (three claims: 6,196,410 + 5,684,682 + 5,672,237 = 17,553,329 inclusive), current principal valuation via estimate_withdraw_position (4,770,986), guarded conversion and exact spend authorization. The vault's oracle-derived NAV root is age-checked cached state on this path; the trace has no direct oracle call. This is not a measurement of the expired-cache/fresh-oracle branch. The fee checkpoint precedes principal refresh; estimate_swap (8,785,882) and estimate_deposit_position (4,538,505) remain inside the measured invocation. No valuation work is dropped from this attribution.

Its fee collection and actual pool deposit cost 53,952,286 and 64,233,503 inclusive; together 118,185,789 before claims, quotes, swap or Peridot accounting. Close retains claim-first transfer and redemption, fee collection and actual withdrawal; fee collection plus withdrawal totals 115,815,792. Those call pairs alone exceed 100M. The third open ownership boundary has not been suppressed merely because close has a different two-claim call tree.

Reaching 100M requires a measured change to the necessary venue work: fee-collection/position mutation implementation or ABI that reduces duplicate work while preserving fee-inclusive valuation, the three open claim boundaries, exact spend checks and atomic funded repayment/seizure. This is a dependency, not a claim that a new ABI exists or saves enough. Removing valuation or deferring mandatory claims is not an optimization. Different transaction stages would need a complete custody/debt/recovery design and are not introduced here. Existing 100-entry/50-write limits, restoration/code-byte ceiling, real signature overhead and missing lifecycle proofs remain separate failures.

Most measured aged-open SDK savings are cash-receipt exclusive CPU (31,314,820 to 17,892,058, saving 13,422,762). Aquarius source is unchanged; smaller observed venue interval differences are not evidence of an Aquarius algorithm optimization. Exact source hashes and full call nodes are in the JSON.

## Original-USDC reserve benchmark decomposition

At one boundary with initial gross G, relative PYUSD/USDC price r and costless initial 50/50 conversion: basket = G(1+r)/2. Signed loss against original USDC decomposes exactly as G − LP = (G − basket) + (basket − LP). Coverage clips the sum at zero, not each summand separately. Basket loss may be negative when PYUSD gains relative to USDC. This is a boundary diagnostic; path-peak reserve still includes already-paid claims and cannot be reconstructed by adding independent maxima.

Existing model numbers for G=1,000 USDC at the first shock (no fee/reward credits, costs still unknown):

| Strategy / path | Signed 50/50 hold loss | Additional LP loss | Original-USDC shortfall |
|---|---:|---:|---:|
| full_range / pyusd_down50_persistent | 250.000000 | 42.893219 | 292.893219 |
| full_range / pyusd_down75_persistent | 375.000000 | 125.000000 | 500.000000 |
| full_range / both_down25_persistent | 0.000000 | 0.000000 | 0.000000 |
| hypothetical_wide_0.5_2 / pyusd_down50_persistent | 250.000000 | 146.446609 | 396.446609 |
| hypothetical_wide_0.5_2 / pyusd_down75_persistent | 375.000000 | 323.223305 | 698.223305 |
| hypothetical_wide_0.5_2 / both_down25_persistent | 0.000000 | 0.000000 | 0.000000 |

For full range at PYUSD0.50/USDC1, 292.893219 USDC total loss = 250 underlying-basket depeg loss + 42.893219 additional LP loss. At PYUSD0.25/USDC1, 500 = 375 + 125. Calling the additional LP term the entire product loss would underfund the original-USDC benchmark. Equal depegs leave the relative price unchanged and can produce zero USDC shortfall while losing dollar purchasing power. Actual zap/unwind/restoration costs, reserve-asset impairment, timing and capacity remain unproved; no production seed or cap is approved.

Reproduce without contract execution: `python3 prototypes/cl-restoration-preparation/cpu-attribution.py`. It fails on changed input hashes or non-reconciling trees and rewrites only its JSON/Markdown reports. Required preserved inputs and exact hashes are listed in JSON; it does not download, sign or broadcast anything.
