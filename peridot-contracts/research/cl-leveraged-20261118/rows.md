| # | Seq | Row | CPU | Mem | Entries (m+d+w) | Keys | Writes | W bytes | Depends on |
|---|---|---|---:|---:|---|---:|---:|---:|---|
| 1 | A | open_v1 (first position) | 3128900 | 3892525 | **32** (21+2+9) | 23 | 9 | 3672 | market borrow (X4), controller pause (G3) |
| 2 | A | fund_v1 (pair open + fund, nested transfers) | 3194889 | 2836543 | **21** (13+0+8) | 13 | 8 | 3676 | receipt prepare_open_pair (X1) + fund_open |
| 3 | A | R convert_open (MOCK) | 1182793 | 2457445 | **7** (6+0+1) | 6 | 1 | 980 | MOCK: cite doc27 O3 / pilot stage_convert |
| 4 | A | R mint_cash (MOCK) | 1363308 | 2453847 | **11** (7+0+4) | 7 | 4 | 1764 | MOCK: cite doc27 O4 / pilot stage_mint |
| 5 | A | R invest_cash (MOCK) | 1022510 | 1288854 | **11** (7+0+4) | 7 | 4 | 1616 | MOCK: cite doc27 O5 / pilot stage_invest+finalise |
| 6 | A | sync_open_v1 (activate, assign beneficiary) | 2242098 | 2633245 | **14** (10+0+4) | 10 | 4 | 1212 | receipt job result (X2), beneficiary (X3) |
| 7 | A | R checkpoint_nav (MOCK, L1a) | 1002799 | 2448203 | **5** (4+0+1) | 4 | 1 | 644 | MOCK: cite group L1a 18 entries |
| 8 | A | set_exit_v1 (equity TP, fresh stored NAV) | 2138397 | 2705228 | **14** (12+0+2) | 12 | 2 | 732 | NAV view (G1) |
| 9 | A | arm_exit_v1 (stale NAV -> inline checkpoint_nav) | 2758228 | 3836178 | **12** (10+0+2) | 10 | 2 | 1368 | inline group L1a (G1) |
| 10 | A | trigger_exit_v1 TP (inline refresh) + prepare_close | 5228349 | 4032058 | **20** (16+0+4) | 16 | 4 | 2292 | inline L1a (G1) + receipt prepare_close (doc27 C1) |
| 11 | A | R withdraw_close (MOCK) | 1500543 | 1333432 | **19** (11+0+8) | 11 | 8 | 2796 | MOCK: cite doc27 C2a 78 entries |
| 12 | A | collect_v1 (read paid exit, release beneficiary) | 2026175 | 2635191 | **9** (7+0+2) | 7 | 2 | 800 | receipt job result (X2), beneficiary (X3) |
| 13 | A | settle_v1 TP, USDC leg covers debt (repay, fee, incentive, payout) | 2684825 | 2613558 | **28** (17+1+10) | 18 | 10 | 2468 | market repay (X4) |
| 14 | B | trigger_exit_v1 TP (fresh stored NAV) + prepare_close | 4221997 | 3192333 | **18** (14+0+4) | 14 | 4 | 2292 | NAV view (G1) + receipt prepare_close |
| 15 | B | arm_exit_v1 (fresh stored NAV) | 1757640 | 2604522 | **9** (8+0+1) | 8 | 1 | 724 | NAV view (G1) |
| 16 | B | health_v1 view | 1963642 | 2695811 | **11** (11+0+0) | 11 | 0 | 0 | NAV view (G1), market debt (X4) |
| 17 | B | open_v1 (second position, same owner) | 3188563 | 3893666 | **30** (21+0+9) | 21 | 9 | 3672 | market borrow (X4), controller pause (G3) |
| 18 | B | add_v1 (fresh NAV, borrow more) | 4176924 | 4027061 | **31** (23+0+8) | 23 | 8 | 2388 | market borrow (X4), NAV view (G1) |
| 19 | B | repay_v1 (external payer) | 1432771 | 2516001 | **15** (11+0+4) | 11 | 4 | 688 | market repay (X4) |
| 20 | B | close_v1 (owner, voluntary) + prepare_close | 2074988 | 2646883 | **15** (10+0+5) | 10 | 5 | 2212 | receipt prepare_close (doc27 C1) |
| 21 | H | trigger_exit_v1 Health (inline refresh) + prepare_close | 4286841 | 4028087 | **20** (16+0+4) | 16 | 4 | 2256 | inline L1a (G1), CF (G3), market debt (X4) |
| 22 | H | swap_deficit_v1 PYUSD->USDC (oracle-bounded min) | 2449023 | 2736572 | **21** (16+0+5) | 16 | 5 | 1520 | swap adapter + Aquarius pool (real swap not measured) |
| 23 | H | settle_v1 Health after swap (repay, fee, incentive, payout) | 2679126 | 2613309 | **28** (17+1+10) | 18 | 10 | 2432 | market repay (X4) |
| 24 | H | trigger_exit_v1 Health (fresh stored NAV) + prepare_close | 3317028 | 2969946 | **18** (14+0+4) | 14 | 4 | 2256 | NAV view (G1) |
| 25 | F | trigger_exit_v1 Health on fallback (NAV try fails, inline fallback) | 5649574 | 4369493 | **20** (16+0+4) | 16 | 4 | 2292 | inline L1a try + L1b (G2) |
| 26 | F | R checkpoint_fallback (MOCK, L1b) | 1029757 | 2448782 | **5** (4+0+1) | 4 | 1 | 680 | MOCK: cite group L1b 13 entries |
| 27 | F | trigger_exit_v1 Health on fallback (stored fallback, NAV try fails) | 4680828 | 4032057 | **20** (16+0+4) | 16 | 4 | 2292 | inline L1a try (G1) + fallback view (G2) |
| 28 | R | cancel_v1 PendingFund by owner (reserve back, -> Settling) | 1043395 | 1308023 | **12** (8+0+4) | 8 | 4 | 1144 |  |
| 29 | R | cancel_v1 Opening after expiry (receipt cancel + sync) | 3296916 | 2830163 | **18** (12+0+6) | 12 | 6 | 2532 | receipt cancel/job (doc27 cancel, X2) |
| 30 | R | swap_deficit_v1 (open fee + interest deficit) | 2441519 | 2736572 | **21** (16+0+5) | 16 | 5 | 1520 | swap adapter + Aquarius pool |
| 31 | R | settle_v1 refund (repay, payout) | 2298380 | 2613309 | **23** (15+0+8) | 15 | 8 | 1984 | market repay (X4) |
