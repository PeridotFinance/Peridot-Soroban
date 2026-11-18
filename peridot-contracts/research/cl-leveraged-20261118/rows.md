| # | Seq | Row | CPU | Mem | Entries (m+d+w) | Keys | Writes | W bytes | Depends on |
|---|---|---|---:|---:|---|---:|---:|---:|---|
| 1 | A | open_v1 (first position) | 3123815 | 3891818 | **32** (21+2+9) | 23 | 9 | 3672 | market borrow (X4), controller pause (G3) |
| 2 | A | fund_v1 (pair open + fund, nested transfers) | 3190092 | 2835836 | **21** (13+0+8) | 13 | 8 | 3676 | receipt prepare_open_pair (X1) + fund_open |
| 3 | A | R convert_open (MOCK) | 1182793 | 2457445 | **7** (6+0+1) | 6 | 1 | 980 | MOCK: cite doc27 O3 / pilot stage_convert |
| 4 | A | R mint_cash (MOCK) | 1363308 | 2453847 | **11** (7+0+4) | 7 | 4 | 1764 | MOCK: cite doc27 O4 / pilot stage_mint |
| 5 | A | R invest_cash (MOCK) | 1022510 | 1288854 | **11** (7+0+4) | 7 | 4 | 1616 | MOCK: cite doc27 O5 / pilot stage_invest+finalise |
| 6 | A | sync_open_v1 (activate, assign beneficiary) | 2237301 | 2632538 | **14** (10+0+4) | 10 | 4 | 1212 | receipt job result (X2), beneficiary (X3) |
| 7 | A | R checkpoint_nav (MOCK, L1a) | 1002799 | 2448203 | **5** (4+0+1) | 4 | 1 | 644 | MOCK: cite group L1a 18 entries |
| 8 | A | set_exit_v1 (equity TP, fresh stored NAV) | 2133600 | 2704521 | **14** (12+0+2) | 12 | 2 | 732 | NAV view (G1) |
| 9 | A | arm_exit_v1 (stale NAV -> inline checkpoint_nav) | 2753431 | 3835471 | **12** (10+0+2) | 10 | 2 | 1368 | inline group L1a (G1) |
| 10 | A | trigger_exit_v1 TP (inline refresh) + prepare_close | 5225654 | 3939151 | **20** (16+0+4) | 16 | 4 | 2292 | inline L1a (G1) + receipt prepare_close (doc27 C1) |
| 11 | A | R withdraw_close (MOCK) | 1500543 | 1333432 | **19** (11+0+8) | 11 | 8 | 2796 | MOCK: cite doc27 C2a 78 entries |
| 12 | A | settle_v1 TP, USDC leg covers debt (no swap) | 3948872 | 2875620 | **33** (21+1+11) | 22 | 11 | 2608 | market repay (X4), receipt job/beneficiary (X2/X3) |
| 13 | B | trigger_exit_v1 TP (fresh stored NAV) + prepare_close | 4219787 | 3191651 | **18** (14+0+4) | 14 | 4 | 2292 | NAV view (G1) + receipt prepare_close |
| 14 | B | arm_exit_v1 (fresh stored NAV) | 1752843 | 2603815 | **9** (8+0+1) | 8 | 1 | 724 | NAV view (G1) |
| 15 | B | health_v1 view | 1958869 | 2695104 | **11** (11+0+0) | 11 | 0 | 0 | NAV view (G1), market debt (X4) |
| 16 | B | open_v1 (second position, same owner) | 3183478 | 3892959 | **30** (21+0+9) | 21 | 9 | 3672 | market borrow (X4), controller pause (G3) |
| 17 | B | add_v1 (fresh NAV, borrow more) | 4171839 | 4026354 | **31** (23+0+8) | 23 | 8 | 2388 | market borrow (X4), NAV view (G1) |
| 18 | B | repay_v1 (external payer) | 1427974 | 2515294 | **15** (11+0+4) | 11 | 4 | 688 | market repay (X4) |
| 19 | B | close_v1 (owner, voluntary) + prepare_close | 2070191 | 2646176 | **15** (10+0+5) | 10 | 5 | 2212 | receipt prepare_close (doc27 C1) |
| 20 | H | trigger_exit_v1 Health (inline refresh) + prepare_close | 4283842 | 3935180 | **20** (16+0+4) | 16 | 4 | 2256 | inline L1a (G1), CF (G3), market debt (X4) |
| 21 | H | settle_v1 Health, deficit swap PYUSD->USDC | 5123282 | 3138387 | **41** (27+1+13) | 28 | 13 | 3020 | market repay (X4), receipt job/beneficiary (X2/X3) |
| 22 | H | trigger_exit_v1 Health (fresh stored NAV) + prepare_close | 3314514 | 2969264 | **18** (14+0+4) | 14 | 4 | 2256 | NAV view (G1) |
| 23 | F | trigger_exit_v1 Health on fallback (NAV try fails, inline fallback) | 5645659 | 4275974 | **20** (16+0+4) | 16 | 4 | 2292 | inline L1a try + L1b (G2) |
| 24 | F | R checkpoint_fallback (MOCK, L1b) | 1029757 | 2448782 | **5** (4+0+1) | 4 | 1 | 680 | MOCK: cite group L1b 13 entries |
| 25 | F | trigger_exit_v1 Health on fallback (stored fallback, NAV try fails) | 4678611 | 3939150 | **20** (16+0+4) | 16 | 4 | 2292 | inline L1a try (G1) + fallback view (G2) |
| 26 | R | cancel_v1 PendingFund by owner (inline refund + swap) | 3713098 | 2897486 | **31** (21+0+10) | 21 | 10 | 2432 | market repay (X4) |
| 27 | R | cancel_v1 Opening after expiry (receipt cancel + sync) | 3318034 | 2829736 | **18** (12+0+6) | 12 | 6 | 2532 | receipt cancel/job (doc27 cancel, X2) |
| 28 | R | settle_v1 refund (swap for fee+interest deficit) | 3514866 | 2870910 | **31** (21+0+10) | 21 | 10 | 2432 | market repay (X4), receipt job/beneficiary (X2/X3) |
