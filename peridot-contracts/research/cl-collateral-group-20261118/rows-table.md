# CL collateral group rows (18 Nov 2026)

LOCAL host-only, soroban-env-host / SDK 29.0.1, mock_all_auths (the `auth` column counts addresses whose authorization the top-level call required; 0 = permissionless), archived Mainnet capture ledger 65527300 (refreshed 65527301). CPU re-priced to the live cost model. Entries = memory reads + disk reads + writes; keys = unique ledger keys. "cold" = controller PriceCache removed. Sequences prefixed STATIC use the static-rate USDC market (cash-borrowing configuration); the others use the release JRM as the spike did. REVERTED-AS-EXPECTED rows are deliberate negative tests. Produced by `prototypes/cl-collateral-group/summarize.py`; see DESIGN.md.

| # | sequence | row | CPU (live model) | mem bytes | entries (mem+disk+writes) | unique keys | writes | pool | auth | verdict |
|---|---|---|---:|---:|---:|---:|---:|---|---|---|
| 1 | REG S4a ckpt, then borrow, cold | `checkpoint_nav` | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | 0 | FIT |
| 2 | REG S4a ckpt, then borrow, cold | `borrow` | 29,548,993 | 4,966,616 | 85 (72+2+11) | 74 | 11 | n | 1 | FIT |
| 3 | REG S4b ckpt, then borrow, warm | `checkpoint_nav` | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | 0 | FIT |
| 4 | REG S4b ckpt, then borrow, warm | `borrow` | 22,749,301 | 4,674,251 | 71 (60+2+9) | 62 | 9 | n | 1 | FIT |
| 5 | REG S3g ckpt, liquidate units-only, cold | `checkpoint_nav` | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | 0 | FIT |
| 6 | REG S3g ckpt, liquidate units-only, cold | `liquidate` | 40,546,867 | 6,782,208 | 93 (74+2+17) | 76 | 17 | n | 1 | FIT >90 |
| 7 | REG S3h ckpt, liquidate units-only, warm | `checkpoint_nav` | 14,352,511 | 6,666,843 | 23 (22+0+1) | 22 | 1 | Y | 0 | FIT |
| 8 | REG S3h ckpt, liquidate units-only, warm | `liquidate` | 33,835,562 | 5,510,333 | 85 (68+2+15) | 70 | 15 | n | 1 | FIT |
| 9 | REG S3g-t ckpt, liquidate + custody read, cold | `checkpoint_nav` | 14,360,875 | 6,667,420 | 23 (22+0+1) | 22 | 1 | Y | 0 | FIT |
| 10 | REG S3g-t ckpt, liquidate + custody read, cold | `liquidate` | 42,728,520 | 7,686,981 | 97 (78+2+17) | 80 | 17 | n | 1 | FIT >90 |
| 11 | REG S3h-t ckpt, liquidate + custody read, warm | `checkpoint_nav` | 14,360,875 | 6,667,420 | 23 (22+0+1) | 22 | 1 | Y | 0 | FIT |
| 12 | REG S3h-t ckpt, liquidate + custody read, warm | `liquidate` | 36,011,745 | 6,401,240 | 89 (72+2+15) | 74 | 15 | n | 1 | FIT |
| 13 | C1 real staged receipt stage_state (112,328-byte code load) | `receipt.stage_state` | 1,023,741 | 1,339,202 | 2 (2+0+0) | 2 | 0 | n | 0 | FIT |
| 14 | C2 group sleeve get_total_ptokens (24,697-byte double load) | `sleeve.get_total_ptokens` | 599,963 | 1,261,437 | 3 (2+0+1) | 2 | 1 | n | 0 | FIT |
| 15 | C3 real vault total_supply (123,430-byte code load) | `vault.total_supply` | 1,179,331 | 1,378,047 | 2 (2+0+0) | 2 | 0 | n | 0 | FIT |
| 16 | GB0 borrow with no checkpoint ever written -> revert | `borrow (no checkpoint)` | 16,904,728 | 4,720,991 | 66 (59+1+6) | 60 | 6 | n | 0 | REVERTED-AS-EXPECTED — trap in USDC market |
| 17 | GB1 ckpt, borrow, WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,704,231 | 5,364,094 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 18 | GB1 ckpt, borrow, WARM | `B borrow (reads NAV checkpoint)` | 22,879,398 | 4,717,759 | 72 (59+2+11) | 61 | 11 | n | 1 | FIT |
| 19 | GB2 ckpt, borrow, COLD (borrow reads oracle live) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,704,231 | 5,364,094 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 20 | GB2 ckpt, borrow, COLD (borrow reads oracle live) | `B borrow (reads NAV checkpoint)` | 26,475,132 | 4,717,759 | 83 (69+2+12) | 71 | 12 | n | 1 | FIT |
| 21 | GB3 ckpt, cache_price(USDC), borrow, from COLD | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,704,231 | 5,364,094 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 22 | GB3 ckpt, cache_price(USDC), borrow, from COLD | `L2 cache_price(USDC)` | 4,267,930 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 23 | GB3 ckpt, cache_price(USDC), borrow, from COLD | `B borrow (reads NAV checkpoint)` | 22,879,398 | 4,717,759 | 72 (59+2+11) | 61 | 11 | n | 1 | FIT |
| 24 | GB4 ckpt, borrow at checkpoint age 60 s -> ok | `B borrow at age 60` (+54s) | 22,879,398 | 4,717,759 | 72 (59+2+11) | 61 | 11 | n | 1 | FIT |
| 25 | GB5 ckpt, borrow at checkpoint age 61 s -> revert | `B borrow at age 61` (+55s) | 16,930,980 | 4,717,759 | 65 (59+1+5) | 60 | 5 | n | 0 | REVERTED-AS-EXPECTED — trap in USDC market |
| 26 | GB7 repeat borrow, WARM | `B repeat borrow` | 24,122,727 | 4,750,822 | 73 (59+2+12) | 61 | 12 | n | 1 | FIT |
| 27 | GB8 repeat borrow, COLD | `B repeat borrow` | 27,707,338 | 4,750,822 | 84 (69+2+13) | 71 | 13 | n | 1 | FIT |
| 28 | GR1 ckpt, redeem 10% of units with debt, WARM | `D redeem_collateral 10%` | 25,173,053 | 5,533,969 | 68 (58+1+9) | 59 | 9 | n | 1 | FIT |
| 29 | GR2 ckpt, redeem 10% with debt, COLD | `D redeem_collateral 10%` | 28,736,944 | 6,052,248 | 80 (69+1+10) | 70 | 10 | n | 1 | FIT |
| 30 | GR3 redeem at checkpoint age 60 s -> ok | `D redeem at age 60` (+54s) | 25,168,469 | 5,533,969 | 68 (58+1+9) | 59 | 9 | n | 1 | FIT |
| 31 | GR4 redeem at checkpoint age 61 s -> revert | `D redeem at age 61` (+55s) | 1,459,991 | 1,273,556 | 5 (3+1+1) | 4 | 1 | n | 0 | REVERTED-AS-EXPECTED — sleeve error #1 NavStale |
| 32 | GR5 redeem 30% -> insufficient collateral revert | `D redeem 30%` | 24,938,469 | 5,533,969 | 64 (57+1+6) | 58 | 6 | n | 0 | REVERTED-AS-EXPECTED — sleeve error #6 InsufficientCollateral |
| 33 | GL1 L1a ckpt, L3 liquidate (cache-only), WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 34 | GL1 L1a ckpt, L3 liquidate (cache-only), WARM | `L3 liquidate (repay + units mirror + queue)` | 33,116,840 | 5,227,089 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 35 | GL2 L1a ckpt, L3 liquidate (cache-only), COLD -> revert by design | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 36 | GL2 L1a ckpt, L3 liquidate (cache-only), COLD -> revert by design | `L3 liquidate cold, cache-only` | 6,709,167 | 2,885,537 | 36 (34+1+1) | 35 | 1 | n | 0 | REVERTED-AS-EXPECTED — trap in group controller |
| 37 | GL3 L1a, L2 cache_price, L3 liquidate, from COLD | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 38 | GL3 L1a, L2 cache_price, L3 liquidate, from COLD | `L2 cache_price(USDC)` | 4,267,930 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 39 | GL3 L1a, L2 cache_price, L3 liquidate, from COLD | `L3 liquidate (repay + units mirror + queue)` | 33,112,980 | 5,227,089 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 40 | GL4 cache-only OFF: ckpt, liquidate, COLD (live oracle read) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 41 | GL4 cache-only OFF: ckpt, liquidate, COLD (live oracle read) | `L3 liquidate cold, live price` | 36,526,348 | 5,880,708 | 92 (72+2+18) | 74 | 18 | n | 1 | FIT >90 |
| 42 | GL5 cache-only OFF: ckpt, liquidate, WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 43 | GL5 cache-only OFF: ckpt, liquidate, WARM | `L3 liquidate warm, live-capable` | 33,122,524 | 5,227,089 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 44 | GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 45 | GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L2 cache_price(USDC)` | 4,267,930 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 46 | GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L3 liquidate (repay + units mirror + queue)` | 33,112,980 | 5,227,089 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 47 | GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L4 vault.checkpoint_rewards (real AQUA claim, separate tx)` | 9,857,079 | 4,663,119 | 29 (21+1+7) | 22 | 7 | Y | 0 | FIT |
| 48 | GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,095,298 | 2,646,764 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 49 | GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 50 | GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L2 cache_price(USDC)` | 4,285,823 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 51 | GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L3 liquidate (repay + units mirror + queue)` | 33,112,980 | 5,227,089 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 52 | GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L4 vault.checkpoint_rewards (real AQUA claim, separate tx)` | 9,857,079 | 4,663,119 | 29 (21+1+7) | 22 | 7 | Y | 0 | FIT |
| 53 | GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,095,298 | 2,646,764 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 54 | GL7 L5 sync with NO fresh claim (opens reward boundary) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 55 | GL7 L5 sync with NO fresh claim (opens reward boundary) | `L3 liquidate` | 33,116,840 | 5,227,089 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 56 | GL7 L5 sync with NO fresh claim (opens reward boundary) | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,205,131 | 2,645,559 | 27 (16+0+11) | 16 | 11 | n | 0 | FIT |
| 57 | GL8 inline vault sync inside seize (single tx), WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,720,786 | 5,366,182 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 58 | GL8 inline vault sync inside seize (single tx), WARM | `L3+L5 inline liquidate` | 35,995,900 | 5,895,201 | 107 (80+2+25) | 82 | 25 | n | 1 | OVER:ENTRIES |
| 59 | GL9 inline vault sync inside seize (single tx), COLD live price | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,720,786 | 5,366,182 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 60 | GL9 inline vault sync inside seize (single tx), COLD live price | `L3+L5 inline liquidate cold` | 39,423,571 | 6,557,048 | 112 (84+2+26) | 86 | 26 | n | 1 | OVER:ENTRIES |
| 61 | GL10 liquidate at NAV checkpoint age 60 s -> ok | `L3 liquidate at age 60` (+54s) | 33,116,448 | 5,227,089 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 62 | GL11 liquidate at NAV checkpoint age 61 s, no fallback -> revert | `L3 liquidate at age 61` (+55s) | 7,137,542 | 2,890,541 | 36 (34+1+1) | 35 | 1 | n | 0 | REVERTED-AS-EXPECTED — trap in group controller |
| 63 | GL12 repeat seizure (second liquidation; liquidator already holds units) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,734,965 | 5,367,399 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 64 | GL12 repeat seizure (second liquidation; liquidator already holds units) | `L3 liquidate (repeat)` | 33,447,732 | 5,230,295 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 65 | GL12 repeat seizure (second liquidation; liquidator already holds units) | `L5 sync_seized(1) (vault-side settle-only reassign)` | 3,232,481 | 2,646,764 | 26 (16+0+10) | 16 | 10 | n | 0 | FIT |
| 66 | GF0 fallback refused while NAV is refreshable | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` | 15,642,010 | 5,868,381 | 17 (17+0+0) | 17 | 0 | Y | 0 | REVERTED-AS-EXPECTED — sleeve error #4 NavRefreshable |
| 67 | GF1 pool out of band: checkpoint_nav reverts | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,638,890 | 5,365,604 | 17 (17+0+0) | 17 | 0 | Y | 0 | REVERTED-AS-EXPECTED — sleeve error #3 PoolOutOfBand |
| 68 | GF1b PYUSD depeg: checkpoint_nav reverts | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 14,167,039 | 5,365,604 | 17 (17+0+0) | 17 | 0 | Y | 0 | REVERTED-AS-EXPECTED — sleeve error #2 DepegGuard |
| 69 | GF2 no NAV checkpoint, only a fallback checkpoint: borrow reverts (cf. GB1 ok) | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` | 8,933,206 | 3,987,691 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 70 | GF2 no NAV checkpoint, only a fallback checkpoint: borrow reverts (cf. GB1 ok) | `B borrow with only a fallback checkpoint` | 16,895,858 | 4,717,478 | 65 (59+1+5) | 60 | 5 | n | 0 | REVERTED-AS-EXPECTED — trap in USDC market |
| 71 | GF3 pool out of band: L1b fallback, L3 liquidate, WARM | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 8,945,448 | 3,989,326 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 72 | GF3 pool out of band: L1b fallback, L3 liquidate, WARM | `L3 liquidate on fallback` | 33,281,496 | 5,230,825 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 73 | GF4 pool out of band: L1b, L2, L3 from COLD | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 8,945,448 | 3,989,326 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 74 | GF4 pool out of band: L1b, L2, L3 from COLD | `L2 cache_price(USDC)` | 4,267,930 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 75 | GF4 pool out of band: L1b, L2, L3 from COLD | `L3 liquidate on fallback` | 33,285,356 | 5,230,825 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 76 | GF5 depeg cause: L1b fallback, L3 liquidate, WARM | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 7,355,663 | 3,669,045 | 11 (10+0+1) | 10 | 1 | n | 0 | FIT |
| 77 | GF5 depeg cause: L1b fallback, L3 liquidate, WARM | `L3 liquidate on fallback (depeg)` | 33,281,496 | 5,230,825 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 78 | GF6 fallback age 61 s -> liquidate reverts | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 8,945,448 | 3,989,326 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 79 | GF6 fallback age 61 s -> liquidate reverts | `L3 liquidate fallback age 61` (+55s) | 7,182,361 | 2,892,455 | 36 (34+1+1) | 35 | 1 | n | 0 | REVERTED-AS-EXPECTED — trap in group controller |
| 80 | GF7 full fallback sequence from COLD + claim + sync | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 8,945,448 | 3,989,326 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 81 | GF7 full fallback sequence from COLD + claim + sync | `L2 cache_price(USDC)` | 4,267,930 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 82 | GF7 full fallback sequence from COLD + claim + sync | `L3 liquidate on fallback` | 33,285,356 | 5,230,825 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 83 | GF7 full fallback sequence from COLD + claim + sync | `L4 vault.checkpoint_rewards (real AQUA claim, separate tx)` | 9,857,079 | 4,663,119 | 29 (21+1+7) | 22 | 7 | Y | 0 | FIT |
| 84 | GF7 full fallback sequence from COLD + claim + sync | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,111,157 | 2,648,398 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 85 | GF7w full fallback sequence from WARM + claim + sync | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 8,945,448 | 3,989,326 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 86 | GF7w full fallback sequence from WARM + claim + sync | `L2 cache_price(USDC)` | 4,285,823 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 87 | GF7w full fallback sequence from WARM + claim + sync | `L3 liquidate on fallback` | 33,285,356 | 5,230,825 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 88 | GF7w full fallback sequence from WARM + claim + sync | `L4 vault.checkpoint_rewards (real AQUA claim, separate tx)` | 9,857,079 | 4,663,119 | 29 (21+1+7) | 22 | 7 | Y | 0 | FIT |
| 89 | GF7w full fallback sequence from WARM + claim + sync | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,111,157 | 2,648,398 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 90 | X2 borrower holds lending-market pTokens: ckpt, liquidate, WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 91 | X2 borrower holds lending-market pTokens: ckpt, liquidate, WARM | `L3 liquidate` | 33,205,152 | 5,228,956 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 92 | X3 borrower with locked units: ckpt, liquidate, claim-model, sync | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 93 | X3 borrower with locked units: ckpt, liquidate, claim-model, sync | `L3 liquidate` | 33,237,768 | 5,227,089 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 94 | X3 borrower with locked units: ckpt, liquidate, claim-model, sync | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,108,247 | 2,646,764 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 95 | X4 fee on: ckpt, liquidate, claim-model, sync liquidator, sync reserve | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 96 | X4 fee on: ckpt, liquidate, claim-model, sync liquidator, sync reserve | `L3 liquidate (fee on)` | 33,458,533 | 5,228,657 | 91 (70+2+19) | 72 | 19 | n | 1 | FIT >90 |
| 97 | X4 fee on: ckpt, liquidate, claim-model, sync liquidator, sync reserve | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,095,298 | 2,646,764 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 98 | X4 fee on: ckpt, liquidate, claim-model, sync liquidator, sync reserve | `L5 sync_seized(1) (vault-side settle-only reassign)` | 3,113,345 | 2,646,764 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 99 | X5 third market listed; entering it -> revert (cap 2) | `enter_market third` | 2,517,036 | 1,340,971 | 15 (13+1+1) | 14 | 1 | n | 0 | REVERTED-AS-EXPECTED — trap in group controller |
| 100 | X5b twin: an account with 0 entered markets enters the listed extra market -> ok | `enter_market first` | 2,761,360 | 1,350,372 | 17 (13+1+3) | 14 | 3 | n | 1 | FIT |
| 101 | X6 interest over 240 s: ckpt, liquidate, WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` (+240s) | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 102 | X6 interest over 240 s: ckpt, liquidate, WARM | `L3 liquidate after 240 s accrual` | 33,136,864 | 5,227,089 | 87 (68+2+17) | 70 | 17 | n | 1 | FIT |
| 103 | STATIC C1 real staged receipt stage_state (112,328-byte code load) | `receipt.stage_state` | 1,023,741 | 1,339,202 | 2 (2+0+0) | 2 | 0 | n | 0 | FIT |
| 104 | STATIC C2 group sleeve get_total_ptokens (24,697-byte double load) | `sleeve.get_total_ptokens` | 599,963 | 1,261,437 | 3 (2+0+1) | 2 | 1 | n | 0 | FIT |
| 105 | STATIC C3 real vault total_supply (123,430-byte code load) | `vault.total_supply` | 1,179,331 | 1,378,047 | 2 (2+0+0) | 2 | 0 | n | 0 | FIT |
| 106 | STATIC GB0 borrow with no checkpoint ever written -> revert | `borrow (no checkpoint)` | 15,206,139 | 4,527,260 | 56 (50+1+5) | 51 | 5 | n | 0 | REVERTED-AS-EXPECTED — trap in USDC market |
| 107 | STATIC GB1 ckpt, borrow, WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,704,231 | 5,364,094 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 108 | STATIC GB1 ckpt, borrow, WARM | `B borrow (reads NAV checkpoint)` | 21,152,899 | 4,524,865 | 64 (52+2+10) | 54 | 10 | n | 1 | FIT |
| 109 | STATIC GB2 ckpt, borrow, COLD (borrow reads oracle live) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,704,231 | 5,364,094 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 110 | STATIC GB2 ckpt, borrow, COLD (borrow reads oracle live) | `B borrow (reads NAV checkpoint)` | 24,710,697 | 4,524,865 | 75 (62+2+11) | 64 | 11 | n | 1 | FIT |
| 111 | STATIC GB3 ckpt, cache_price(USDC), borrow, from COLD | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,704,231 | 5,364,094 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 112 | STATIC GB3 ckpt, cache_price(USDC), borrow, from COLD | `L2 cache_price(USDC)` | 4,267,930 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 113 | STATIC GB3 ckpt, cache_price(USDC), borrow, from COLD | `B borrow (reads NAV checkpoint)` | 21,152,899 | 4,524,865 | 64 (52+2+10) | 54 | 10 | n | 1 | FIT |
| 114 | STATIC GB4 ckpt, borrow at checkpoint age 60 s -> ok | `B borrow at age 60` (+54s) | 21,152,899 | 4,524,865 | 64 (52+2+10) | 54 | 10 | n | 1 | FIT |
| 115 | STATIC GB5 ckpt, borrow at checkpoint age 61 s -> revert | `B borrow at age 61` (+55s) | 15,234,535 | 4,524,865 | 55 (50+1+4) | 51 | 4 | n | 0 | REVERTED-AS-EXPECTED — trap in USDC market |
| 116 | STATIC GB7 repeat borrow, WARM | `B repeat borrow` | 22,394,849 | 4,559,863 | 66 (52+2+12) | 54 | 12 | n | 1 | FIT |
| 117 | STATIC GB8 repeat borrow, COLD | `B repeat borrow` | 25,952,918 | 4,559,863 | 77 (62+2+13) | 64 | 13 | n | 1 | FIT |
| 118 | STATIC GR1 ckpt, redeem 10% of units with debt, WARM | `D redeem_collateral 10%` | 23,350,348 | 5,158,538 | 59 (49+1+9) | 50 | 9 | n | 1 | FIT |
| 119 | STATIC GR2 ckpt, redeem 10% with debt, COLD | `D redeem_collateral 10%` | 26,896,052 | 5,847,713 | 71 (60+1+10) | 61 | 10 | n | 1 | FIT |
| 120 | STATIC GR3 redeem at checkpoint age 60 s -> ok | `D redeem at age 60` (+54s) | 23,350,740 | 5,158,538 | 59 (49+1+9) | 50 | 9 | n | 1 | FIT |
| 121 | STATIC GR4 redeem at checkpoint age 61 s -> revert | `D redeem at age 61` (+55s) | 1,459,991 | 1,273,556 | 5 (3+1+1) | 4 | 1 | n | 0 | REVERTED-AS-EXPECTED — sleeve error #1 NavStale |
| 122 | STATIC GR5 redeem 30% -> insufficient collateral revert | `D redeem 30%` | 23,123,746 | 5,158,538 | 55 (48+1+6) | 49 | 6 | n | 0 | REVERTED-AS-EXPECTED — sleeve error #6 InsufficientCollateral |
| 123 | STATIC GL1 L1a ckpt, L3 liquidate (cache-only), WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 124 | STATIC GL1 L1a ckpt, L3 liquidate (cache-only), WARM | `L3 liquidate (repay + units mirror + queue)` | 31,246,261 | 4,337,412 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 125 | STATIC GL2 L1a ckpt, L3 liquidate (cache-only), COLD -> revert by design | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 126 | STATIC GL2 L1a ckpt, L3 liquidate (cache-only), COLD -> revert by design | `L3 liquidate cold, cache-only` | 6,709,167 | 2,885,537 | 36 (34+1+1) | 35 | 1 | n | 0 | REVERTED-AS-EXPECTED — trap in group controller |
| 127 | STATIC GL3 L1a, L2 cache_price, L3 liquidate, from COLD | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 128 | STATIC GL3 L1a, L2 cache_price, L3 liquidate, from COLD | `L2 cache_price(USDC)` | 4,267,930 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 129 | STATIC GL3 L1a, L2 cache_price, L3 liquidate, from COLD | `L3 liquidate (repay + units mirror + queue)` | 31,221,985 | 4,337,412 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 130 | STATIC GL4 cache-only OFF: ckpt, liquidate, COLD (live oracle read) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 131 | STATIC GL4 cache-only OFF: ckpt, liquidate, COLD (live oracle read) | `L3 liquidate cold, live price` | 34,603,787 | 4,996,955 | 85 (65+2+18) | 67 | 18 | n | 1 | FIT |
| 132 | STATIC GL5 cache-only OFF: ckpt, liquidate, WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 133 | STATIC GL5 cache-only OFF: ckpt, liquidate, WARM | `L3 liquidate warm, live-capable` | 31,231,529 | 4,337,412 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 134 | STATIC GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 135 | STATIC GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L2 cache_price(USDC)` | 4,267,930 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 136 | STATIC GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L3 liquidate (repay + units mirror + queue)` | 31,221,985 | 4,337,412 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 137 | STATIC GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L4 vault.checkpoint_rewards (real AQUA claim, separate tx)` | 9,857,079 | 4,663,119 | 29 (21+1+7) | 22 | 7 | Y | 0 | FIT |
| 138 | STATIC GL6 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,095,298 | 2,646,764 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 139 | STATIC GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 140 | STATIC GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L2 cache_price(USDC)` | 4,285,823 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 141 | STATIC GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L3 liquidate (repay + units mirror + queue)` | 31,221,985 | 4,337,412 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 142 | STATIC GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L4 vault.checkpoint_rewards (real AQUA claim, separate tx)` | 9,857,079 | 4,663,119 | 29 (21+1+7) | 22 | 7 | Y | 0 | FIT |
| 143 | STATIC GL6w full staged sequence from WARM: L1a, L2, L3, L4 claim, L5 sync (claim fresh) | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,095,298 | 2,646,764 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 144 | STATIC GL7 L5 sync with NO fresh claim (opens reward boundary) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 145 | STATIC GL7 L5 sync with NO fresh claim (opens reward boundary) | `L3 liquidate` | 31,246,261 | 4,337,412 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 146 | STATIC GL7 L5 sync with NO fresh claim (opens reward boundary) | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,205,131 | 2,645,559 | 27 (16+0+11) | 16 | 11 | n | 0 | FIT |
| 147 | STATIC GL8 inline vault sync inside seize (single tx), WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,720,786 | 5,366,182 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 148 | STATIC GL8 inline vault sync inside seize (single tx), WARM | `L3+L5 inline liquidate` | 34,060,067 | 5,697,004 | 100 (73+2+25) | 75 | 25 | n | 1 | FIT >90 |
| 149 | STATIC GL9 inline vault sync inside seize (single tx), COLD live price | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,720,786 | 5,366,182 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 150 | STATIC GL9 inline vault sync inside seize (single tx), COLD live price | `L3+L5 inline liquidate cold` | 37,453,821 | 6,356,075 | 105 (77+2+26) | 79 | 26 | n | 1 | OVER:ENTRIES |
| 151 | STATIC GL10 liquidate at NAV checkpoint age 60 s -> ok | `L3 liquidate at age 60` (+54s) | 31,249,337 | 4,337,412 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 152 | STATIC GL11 liquidate at NAV checkpoint age 61 s, no fallback -> revert | `L3 liquidate at age 61` (+55s) | 7,137,542 | 2,890,541 | 36 (34+1+1) | 35 | 1 | n | 0 | REVERTED-AS-EXPECTED — trap in group controller |
| 153 | STATIC GL12 repeat seizure (second liquidation; liquidator already holds units) | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,734,965 | 5,367,399 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 154 | STATIC GL12 repeat seizure (second liquidation; liquidator already holds units) | `L3 liquidate (repeat)` | 31,511,311 | 4,345,038 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 155 | STATIC GL12 repeat seizure (second liquidation; liquidator already holds units) | `L5 sync_seized(1) (vault-side settle-only reassign)` | 3,232,481 | 2,646,764 | 26 (16+0+10) | 16 | 10 | n | 0 | FIT |
| 156 | STATIC GF0 fallback refused while NAV is refreshable | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` | 15,642,010 | 5,868,381 | 17 (17+0+0) | 17 | 0 | Y | 0 | REVERTED-AS-EXPECTED — sleeve error #4 NavRefreshable |
| 157 | STATIC GF1 pool out of band: checkpoint_nav reverts | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,638,890 | 5,365,604 | 17 (17+0+0) | 17 | 0 | Y | 0 | REVERTED-AS-EXPECTED — sleeve error #3 PoolOutOfBand |
| 158 | STATIC GF1b PYUSD depeg: checkpoint_nav reverts | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 14,167,039 | 5,365,604 | 17 (17+0+0) | 17 | 0 | Y | 0 | REVERTED-AS-EXPECTED — sleeve error #2 DepegGuard |
| 159 | STATIC GF2 no NAV checkpoint, only a fallback checkpoint: borrow reverts (cf. GB1 ok) | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` | 8,933,206 | 3,987,691 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 160 | STATIC GF2 no NAV checkpoint, only a fallback checkpoint: borrow reverts (cf. GB1 ok) | `B borrow with only a fallback checkpoint` | 15,199,413 | 4,524,584 | 55 (50+1+4) | 51 | 4 | n | 0 | REVERTED-AS-EXPECTED — trap in USDC market |
| 161 | STATIC GF3 pool out of band: L1b fallback, L3 liquidate, WARM | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 8,945,448 | 3,989,326 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 162 | STATIC GF3 pool out of band: L1b fallback, L3 liquidate, WARM | `L3 liquidate on fallback` | 31,391,285 | 4,344,328 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 163 | STATIC GF4 pool out of band: L1b, L2, L3 from COLD | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 8,945,448 | 3,989,326 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 164 | STATIC GF4 pool out of band: L1b, L2, L3 from COLD | `L2 cache_price(USDC)` | 4,267,930 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 165 | STATIC GF4 pool out of band: L1b, L2, L3 from COLD | `L3 liquidate on fallback` | 31,370,869 | 4,344,328 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 166 | STATIC GF5 depeg cause: L1b fallback, L3 liquidate, WARM | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 7,355,663 | 3,669,045 | 11 (10+0+1) | 10 | 1 | n | 0 | FIT |
| 167 | STATIC GF5 depeg cause: L1b fallback, L3 liquidate, WARM | `L3 liquidate on fallback (depeg)` | 31,391,285 | 4,344,328 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 168 | STATIC GF6 fallback age 61 s -> liquidate reverts | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 8,945,448 | 3,989,326 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 169 | STATIC GF6 fallback age 61 s -> liquidate reverts | `L3 liquidate fallback age 61` (+55s) | 7,182,361 | 2,892,455 | 36 (34+1+1) | 35 | 1 | n | 0 | REVERTED-AS-EXPECTED — trap in group controller |
| 170 | STATIC GF7 full fallback sequence from COLD + claim + sync | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 8,945,448 | 3,989,326 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 171 | STATIC GF7 full fallback sequence from COLD + claim + sync | `L2 cache_price(USDC)` | 4,267,930 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 172 | STATIC GF7 full fallback sequence from COLD + claim + sync | `L3 liquidate on fallback` | 31,370,869 | 4,344,328 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 173 | STATIC GF7 full fallback sequence from COLD + claim + sync | `L4 vault.checkpoint_rewards (real AQUA claim, separate tx)` | 9,857,079 | 4,663,119 | 29 (21+1+7) | 22 | 7 | Y | 0 | FIT |
| 174 | STATIC GF7 full fallback sequence from COLD + claim + sync | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,111,157 | 2,648,398 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 175 | STATIC GF7w full fallback sequence from WARM + claim + sync | `L1b checkpoint_fallback (liquidation-only, band-edge cheaper asset)` (+61s) | 8,945,448 | 3,989,326 | 13 (12+0+1) | 12 | 1 | Y | 0 | FIT |
| 176 | STATIC GF7w full fallback sequence from WARM + claim + sync | `L2 cache_price(USDC)` | 4,285,823 | 2,004,198 | 17 (16+0+1) | 16 | 1 | n | 0 | FIT |
| 177 | STATIC GF7w full fallback sequence from WARM + claim + sync | `L3 liquidate on fallback` | 31,370,869 | 4,344,328 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 178 | STATIC GF7w full fallback sequence from WARM + claim + sync | `L4 vault.checkpoint_rewards (real AQUA claim, separate tx)` | 9,857,079 | 4,663,119 | 29 (21+1+7) | 22 | 7 | Y | 0 | FIT |
| 179 | STATIC GF7w full fallback sequence from WARM + claim + sync | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,111,157 | 2,648,398 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 180 | STATIC X2 borrower holds lending-market pTokens: ckpt, liquidate, WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 181 | STATIC X2 borrower holds lending-market pTokens: ckpt, liquidate, WARM | `L3 liquidate` | 31,309,513 | 4,339,279 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 182 | STATIC X3 borrower with locked units: ckpt, liquidate, claim-model, sync | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 183 | STATIC X3 borrower with locked units: ckpt, liquidate, claim-model, sync | `L3 liquidate` | 31,276,689 | 4,337,412 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
| 184 | STATIC X3 borrower with locked units: ckpt, liquidate, claim-model, sync | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,108,247 | 2,646,764 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 185 | STATIC X4 fee on: ckpt, liquidate, claim-model, sync liquidator, sync reserve | `L1a checkpoint_nav (vault-wide, band+depeg guards)` | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 186 | STATIC X4 fee on: ckpt, liquidate, claim-model, sync liquidator, sync reserve | `L3 liquidate (fee on)` | 31,569,682 | 4,367,324 | 84 (63+2+19) | 65 | 19 | n | 1 | FIT |
| 187 | STATIC X4 fee on: ckpt, liquidate, claim-model, sync liquidator, sync reserve | `L5 sync_seized(0) (vault-side settle-only reassign)` | 3,095,298 | 2,646,764 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 188 | STATIC X4 fee on: ckpt, liquidate, claim-model, sync liquidator, sync reserve | `L5 sync_seized(1) (vault-side settle-only reassign)` | 3,113,345 | 2,646,764 | 25 (15+0+10) | 15 | 10 | n | 0 | FIT |
| 189 | STATIC X5 third market listed; entering it -> revert (cap 2) | `enter_market third` | 2,517,036 | 1,340,971 | 15 (13+1+1) | 14 | 1 | n | 0 | REVERTED-AS-EXPECTED — trap in group controller |
| 190 | STATIC X5b twin: an account with 0 entered markets enters the listed extra market -> ok | `enter_market first` | 2,761,360 | 1,350,372 | 17 (13+1+3) | 14 | 3 | n | 1 | FIT |
| 191 | STATIC X6 interest over 240 s: ckpt, liquidate, WARM | `L1a checkpoint_nav (vault-wide, band+depeg guards)` (+240s) | 15,714,882 | 5,365,604 | 18 (17+0+1) | 17 | 1 | Y | 0 | FIT |
| 192 | STATIC X6 interest over 240 s: ckpt, liquidate, WARM | `L3 liquidate after 240 s accrual` | 31,246,261 | 4,337,412 | 80 (61+2+17) | 63 | 17 | n | 1 | FIT |
unknown 0
