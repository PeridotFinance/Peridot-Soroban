# Report21 individual Aquarius call measurement

Offline re-analysis of the original captured protocol29 trees; no new harness or contract execution. Complete call cost is inclusive of token/plane descendants. Exclusive totals allocate CPU by executing contract and must not be added to inclusive totals.

## open_with_actual_deposit

| Path | Aquarius function | Inclusive CPU | Exclusive CPU | Under100M |
|---|---|---:|---:|---|
| 0.8.0.0 | get_is_killed_swap | 1,087,231 | 1,087,231 | YES |
| 0.8.0.1 | get_is_killed_deposit | 1,063,738 | 1,063,738 | YES |
| 0.8.5.3 | claim | 6,196,410 | 5,363,417 | YES |
| 0.8.5.5 | claim_all_position_fees | 53,952,286 | 53,275,588 | YES |
| 0.8.5.6 | estimate_withdraw_position | 4,770,986 | 4,770,986 | YES |
| 0.8.5.11 | get_is_killed_deposit | 1,067,269 | 1,067,269 | YES |
| 0.8.5.12 | estimate_swap | 8,785,882 | 8,785,882 | YES |
| 0.8.5.14 | swap | 10,778,299 | 10,220,469 | YES |
| 0.8.5.19 | estimate_deposit_position | 4,538,505 | 4,538,505 | YES |
| 0.8.5.20 | deposit_position | 64,233,503 | 62,549,781 | YES |
| 0.8.7.1 | claim | 5,684,682 | 5,050,928 | YES |
| 0.10.1 | claim | 5,672,237 | 5,043,434 | YES |

Exclusive venue total **162,817,228**, disjoint inclusive total **167,831,028**, complete SDK root **223,150,467**.

## later_open_without_deposit

| Path | Aquarius function | Inclusive CPU | Exclusive CPU | Under100M |
|---|---|---:|---:|---|
| 0.8.0.0 | get_is_killed_swap | 1,194,688 | 1,194,688 | YES |
| 0.8.0.1 | get_is_killed_deposit | 1,062,468 | 1,062,468 | YES |
| 0.8.5.3 | claim | 6,552,699 | 5,769,158 | YES |
| 0.8.5.5 | claim_all_position_fees | 53,919,030 | 53,263,516 | YES |
| 0.8.5.6 | estimate_withdraw_position | 4,570,289 | 4,570,289 | YES |
| 0.8.5.11 | get_is_killed_deposit | 1,066,119 | 1,066,119 | YES |
| 0.8.5.12 | estimate_swap | 9,167,680 | 9,167,680 | YES |
| 0.8.5.14 | swap | 11,100,517 | 10,588,471 | YES |
| 0.8.5.19 | estimate_deposit_position | 4,114,540 | 4,114,540 | YES |
| 0.8.7.1 | claim | 5,596,172 | 4,983,794 | YES |
| 0.10.1 | claim | 5,611,106 | 5,004,729 | YES |

Exclusive venue total **100,785,452**, disjoint inclusive total **103,955,308**, complete SDK root **157,058,149**.

## close

| Path | Aquarius function | Inclusive CPU | Exclusive CPU | Under100M |
|---|---|---:|---:|---|
| 0.4.0.0 | get_is_killed_swap | 1,081,735 | 1,081,735 | YES |
| 0.9.0.0 | get_is_killed_swap | 1,069,738 | 1,069,738 | YES |
| 0.9.3.1 | claim | 5,946,047 | 5,225,045 | YES |
| 0.9.5.3 | claim | 5,720,808 | 5,076,474 | YES |
| 0.9.5.5 | claim_all_position_fees | 53,254,436 | 52,890,781 | YES |
| 0.9.5.8 | withdraw_position | 62,561,356 | 61,064,024 | YES |
| 0.9.5.12 | estimate_swap | 4,957,303 | 4,957,303 | YES |
| 0.9.5.14 | swap | 6,953,963 | 6,394,424 | YES |

Exclusive venue total **137,759,524**, disjoint inclusive total **141,545,386**, complete SDK root **183,245,406**.

Every captured node reconciles inclusive minus direct children to exclusive, and all exclusive nodes to its root. Input hashes match the durable report21 archive. The later open never calls deposit_position and is not a full deposit feasibility proof.

Largest observed invocation across this captured lifecycle: **deposit_position 64233503 CPU**. This is the largest observed indivisible external ABI call, not a universal tick/history bound. Separate fee collection and position mutation cannot simply be assumed safe across transactions: fresh fee ownership, valuation, all reward boundaries, debt/custody and recovery still require a design. No statement about source-line or instruction-level indivisibility is made.
