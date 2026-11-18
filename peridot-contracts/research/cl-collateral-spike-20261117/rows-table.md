| # | sequence / row | CPU | mem bytes | entries (mem+disk+writes) | unique keys | writes | pool | verdict |
|---|---|---:|---:|---:|---:|---:|---|---|
| 1 | V1 view: vault.get_asset_amounts_per_shares(owner units) [live NAV incl. venue quote] | 11,479,208 | 3,845,091 | 15 (15+0+0) | 15 | 0 | Y | FIT |
| 2 | V2 view: receipt.stage_account(owner) [real staged receipt valuation] | 13,779,603 | 5,432,350 | 19 (19+0+0) | 19 | 0 | Y | FIT |
| 3 | V3 view: controller.cache_price(PYUSD) [Reflector read + cache write] | 4,422,617 | 2,001,689 | 17 (16+0+1) | 16 | 1 | n | FIT |
| 4 | V4 view: sleeve.get_account_snapshot(owner) src1 (via real receipt) | 14,331,079 | 6,666,971 | 22 (22+0+0) | 22 | 0 | Y | FIT |
| 5 | V5 view: sleeve snapshot src0 (vault-direct) | 12,038,772 | 5,078,761 | 18 (18+0+0) | 18 | 0 | Y | FIT |
| 6 | S1a borrow LIVE src1 (real receipt) COLD price cache, single tx | 43,378,346 | 10,189,094 | 101 (88+2+11) | 90 | 11 | Y | OVER:ENTRIES |
| 7 | S1b borrow LIVE src1 WARM price cache, single tx | 36,896,578 | 10,189,094 | 89 (78+2+9) | 80 | 9 | Y | FIT |
| 8 | S1c borrow LIVE src0 (vault-direct) COLD cache, single tx | 40,971,177 | 8,583,448 | 97 (84+2+11) | 86 | 11 | Y | FIT |
| 9 | S1d staged price pre-warm: cache_price x2, then borrow LIVE src1 | 4,422,617 | 2,001,689 | 17 (16+0+1) | 16 | 1 | n | FIT |
| 10 | S1d staged price pre-warm: cache_price x2, then borrow LIVE src1 | 4,249,875 | 2,000,841 | 17 (16+0+1) | 16 | 1 | n | FIT |
| 11 | S1d staged price pre-warm: cache_price x2, then borrow LIVE src1 | 36,896,578 | 10,189,094 | 89 (78+2+9) | 80 | 9 | Y | FIT |
| 12 | S1e repeat borrow LIVE src1 (existing debt) WARM | 38,124,191 | 10,225,941 | 92 (78+2+12) | 80 | 12 | Y | FIT |
| 13 | S1f repeat borrow LIVE src1 (existing debt) COLD | 44,621,545 | 10,225,941 | 104 (88+2+14) | 90 | 14 | Y | OVER:ENTRIES |
| 14 | S4a staged checkpoint: checkpoint_nav, then borrow CHECKPOINT COLD | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 15 | S4a staged checkpoint: checkpoint_nav, then borrow CHECKPOINT COLD | 29,548,993 | 4,966,616 | 85 (72+2+11) | 74 | 11 | n | FIT |
| 16 | S4b staged checkpoint: checkpoint_nav, then borrow CHECKPOINT WARM | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 17 | S4b staged checkpoint: checkpoint_nav, then borrow CHECKPOINT WARM | 22,749,301 | 4,674,251 | 71 (60+2+9) | 62 | 9 | n | FIT |
| 18 | S2a borrow LIVE src1 + extra cash collateral, COLD, single tx | 46,284,662 | 10,190,022 | 114 (101+2+11) | 103 | 11 | Y | OVER:ENTRIES+KEYS |
| 19 | S2b borrow LIVE src1 + extra cash collateral, WARM, single tx | 39,718,348 | 10,190,022 | 102 (91+2+9) | 93 | 9 | Y | OVER:ENTRIES |
| 20 | S2c borrow LIVE src0 (vault-direct) + extra cash collateral, WARM | 37,354,994 | 8,584,376 | 98 (87+2+9) | 89 | 9 | Y | FIT |
| 21 | S2d staged: checkpoint_nav then borrow CHECKPOINT + extra cash collateral, COLD | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 22 | S2d staged: checkpoint_nav then borrow CHECKPOINT + extra cash collateral, COLD | 32,367,831 | 5,711,805 | 98 (85+2+11) | 87 | 11 | n | FIT |
| 23 | S2e staged: checkpoint_nav then borrow CHECKPOINT + extra cash collateral, WARM | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 24 | S2e staged: checkpoint_nav then borrow CHECKPOINT + extra cash collateral, WARM | 25,509,372 | 5,001,300 | 84 (73+2+9) | 75 | 9 | n | FIT |
| 25 | S3 setup: borrow 300 USDC, shrink units to 40%, liquidator approve | 36,896,578 | 10,189,094 | 89 (78+2+9) | 80 | 9 | Y | FIT |
| 26 | S3a liquidate LIVE src1, units-only seize, COLD, single tx | 65,161,113 | 10,965,967 | 109 (90+2+17) | 92 | 17 | Y | OVER:ENTRIES |
| 27 | S3b liquidate LIVE src1, units-only seize, WARM, single tx | 58,798,406 | 9,723,377 | 103 (86+2+15) | 88 | 15 | Y | OVER:ENTRIES |
| 28 | S3c liquidate LIVE src0 (vault-direct), units-only, COLD | 62,826,474 | 10,451,017 | 105 (86+2+17) | 88 | 17 | Y | OVER:ENTRIES |
| 29 | S3d liquidate LIVE src0 (vault-direct), units-only, WARM | 56,373,429 | 9,212,291 | 99 (82+2+15) | 84 | 15 | Y | FIT |
| 30 | S3e liquidate LIVE src1 + seize reward-boundary proxy (vault.checkpoint_rewards), COLD | 75,561,884 | 14,796,348 | 133 (106+3+24) | 109 | 24 | Y | OVER:ENTRIES+KEYS |
| 31 | S3f liquidate LIVE src1 + proxy, WARM | 69,155,787 | 13,531,406 | 127 (102+3+22) | 105 | 22 | Y | OVER:ENTRIES+KEYS |
| 32 | S3g staged: checkpoint_nav then liquidate CHECKPOINT units-only, COLD | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 33 | S3g staged: checkpoint_nav then liquidate CHECKPOINT units-only, COLD | 40,546,867 | 6,782,208 | 93 (74+2+17) | 76 | 17 | n | FIT |
| 34 | S3h staged: checkpoint_nav then liquidate CHECKPOINT units-only, WARM | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 35 | S3h staged: checkpoint_nav then liquidate CHECKPOINT units-only, WARM | 33,835,562 | 5,510,333 | 85 (68+2+15) | 70 | 15 | n | FIT |
| 36 | S3i staged: checkpoint_nav then liquidate CHECKPOINT + proxy, COLD | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 37 | S3i staged: checkpoint_nav then liquidate CHECKPOINT + proxy, COLD | 51,157,766 | 10,908,816 | 122 (95+3+24) | 98 | 24 | Y | OVER:ENTRIES |
| 38 | S3j staged: checkpoint_nav then liquidate CHECKPOINT + proxy, WARM | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 39 | S3j staged: checkpoint_nav then liquidate CHECKPOINT + proxy, WARM | 44,361,729 | 9,609,059 | 114 (89+3+22) | 92 | 22 | Y | OVER:ENTRIES |
| 40 | S3g-t staged: checkpoint then liquidate CHECKPOINT, seize READS vault custody of both holders, COLD | 14,360,875 | 6,667,420 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 41 | S3g-t staged: checkpoint then liquidate CHECKPOINT, seize READS vault custody of both holders, COLD | 42,728,520 | 7,686,981 | 97 (78+2+17) | 80 | 17 | n | FIT |
| 42 | S3h-t staged: checkpoint then liquidate CHECKPOINT, seize reads vault custody, WARM | 14,360,875 | 6,667,420 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 43 | S3h-t staged: checkpoint then liquidate CHECKPOINT, seize reads vault custody, WARM | 36,011,745 | 6,401,240 | 89 (72+2+15) | 74 | 15 | n | FIT |
| 44 | S3k-t fully staged + seize reads vault custody | 14,360,875 | 6,667,420 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 45 | S3k-t fully staged + seize reads vault custody | 4,422,617 | 2,001,689 | 17 (16+0+1) | 16 | 1 | n | FIT |
| 46 | S3k-t fully staged + seize reads vault custody | 4,249,875 | 2,000,841 | 17 (16+0+1) | 16 | 1 | n | FIT |
| 47 | S3k-t fully staged + seize reads vault custody | 36,032,161 | 6,401,240 | 89 (72+2+15) | 74 | 15 | n | FIT |
| 48 | S3k fully staged: checkpoint_nav, cache_price x2, then liquidate CHECKPOINT units-only | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 49 | S3k fully staged: checkpoint_nav, cache_price x2, then liquidate CHECKPOINT units-only | 4,422,617 | 2,001,689 | 17 (16+0+1) | 16 | 1 | n | FIT |
| 50 | S3k fully staged: checkpoint_nav, cache_price x2, then liquidate CHECKPOINT units-only | 4,249,875 | 2,000,841 | 17 (16+0+1) | 16 | 1 | n | FIT |
| 51 | S3k fully staged: checkpoint_nav, cache_price x2, then liquidate CHECKPOINT units-only | 33,835,562 | 5,510,333 | 85 (68+2+15) | 70 | 15 | n | FIT |
| 52 | S3x setup on S2 account (extra cash collateral) | 39,718,348 | 10,190,022 | 102 (91+2+9) | 93 | 9 | Y | OVER:ENTRIES |
| 53 | S3x-a liquidate LIVE src1 + extra cash collateral, COLD | 68,062,477 | 11,314,380 | 122 (103+2+17) | 105 | 17 | Y | OVER:ENTRIES+KEYS |
| 54 | S3x-b liquidate LIVE src1 + extra cash collateral, WARM | 61,672,859 | 10,066,026 | 116 (99+2+15) | 101 | 15 | Y | OVER:ENTRIES+KEYS |
| 55 | S3x-d staged: checkpoint_nav then liquidate CHECKPOINT + extra cash collateral, COLD | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 56 | S3x-d staged: checkpoint_nav then liquidate CHECKPOINT + extra cash collateral, COLD | 43,490,967 | 7,117,753 | 106 (87+2+17) | 89 | 17 | n | OVER:ENTRIES |
| 57 | S3x-e staged + extra cash collateral + seize reads vault custody, WARM | 14,360,875 | 6,667,420 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 58 | S3x-e staged + extra cash collateral + seize reads vault custody, WARM | 38,971,144 | 6,758,374 | 102 (85+2+15) | 87 | 15 | n | OVER:ENTRIES |
| 59 | S3x-c staged: checkpoint_nav then liquidate CHECKPOINT + extra cash collateral, WARM | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | FIT |
| 60 | S3x-c staged: checkpoint_nav then liquidate CHECKPOINT + extra cash collateral, WARM | 36,783,672 | 5,837,424 | 98 (81+2+15) | 83 | 15 | n | FIT |
unknown 0
