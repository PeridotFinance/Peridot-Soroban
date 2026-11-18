# CL collateral group: build report (18 Nov 2026, branch feat/cl-collateral-group)

This report covers the build started today. It does not cover Mainnet: nothing was signed, sent, deployed
or upgraded on Mainnet. It extends DESIGN.md and D3-CONTINUITY.md for tomorrow's second-opinion review.

## 1. Artefacts

| Component | Source | WASM (B, of 131,072) | Tests |
|---|---|---:|---|
| Vault v2 | `prototypes/cl-collateral-group/vault-v2` (verbatim base, then v2 commits) | 124,762 (Mainnet admin constant), sha256 72286249… | 60/60 (`seize_tests` 7) |
| Receipt StgV2 | `receipt-v2/{staged_v2.rs, apply-stgv2.py}` over run-5 StgV1 | 124,916, sha256 47b4a147… | compiled-bytes rows below |
| D3 chain | `d3-chain/src/d3_chain.rs` (shared) | — | 7/7 |
| Group controller | `controller/apply-patches.py` G0–G10 | 64,267, sha256 debe1a36… | rows below |
| Liquidator bot | `bots/cl-liquidator` | — | 21/21 |
| Capacity tool | `prototypes/cl-collateral-group/capacity` | — | 5/5 |
| Testnet fixtures | `testnet-oracle`, `testnet-pool` (Testnet-only) | 5,138 / 18,425 | Testnet run |

New controller patches (in addition to G0–G5):
- **G6:** only the sleeve may hold a CF, which enforces condition (c) structurally.
- **G7:** at most 2 listed markets.
- **G8:** no static price fallback.
- **G9:** no margin liquidation controller.
- **G10:** with zero group debt, an account may exit the sleeve while holding units. This read does not use the oracle.

## 2. Local measurements on the production candidates

Method:
- Harness: `harness/src/real.rs`, local host 29.0.1, archived Mainnet capture at ledger 65527300, `mock_all_auths`.
- The live vault and receipt instances keep their storage. Their code is swapped in the local snapshot only.
- A fresh G0–G10 controller and a static 8% USDC market were deployed.
- USDC is priced on CALI. CAFJ adds 0 entries [rpc].
- Entries = memory reads + disk reads + writes. Keys are counted separately.

Evidence (outside git, `target/clg/evidence/`):
- `real-run-result.json`, sha256 c8df049e…5aca85
- `realm-run-result.json`, sha256 3ae68052…fb4d

**Summary (largest stage = L3, 80 entries, 63 keys, 17 writes, 32.4M CPU; every stage ≤ 85 entries except as noted):**

| Stage | Cold | Warm |
|---|---|---|
| L1a `checkpoint_nav` | 20 (19+0+1) / 19 keys / 15.3M | same |
| L1b `checkpoint_fallback` (after a 10-min chain) | 15 (14+0+1) / 14 / 7.9M | same |
| L2 `cache_price(USDC)` | 17 / 16 / 4.3M | 17 |
| L3 `liquidate` (cache-only) | reverts by design at 36 without L2 | **80** (61+2+17) / 63 / 17 w / 32.4M |
| L3 fee on | — | 84 / 65 / 19 w |
| L4 `vault.checkpoint_rewards` | 29 (21+1+7) / 22 / 9.9M | 28–29 |
| L5 `sync_seized` | 29 (19+0+10) / 19 / 4.9M; 31 when it opens the D6 boundary | same |
| B borrow | 75 (62+2+11) / 64 / 25.2M | 64 / 54 / 21.6M |
| B repeat borrow | 77 | 66 |
| D prepare_close with debt | 80 (67+1+12) / 68 / 29.3M | 68 / 57 / 25.7M |
| D prepare_close without debt | — | 59 |
| Emergency exit (not entered) | — | 89 (StgV1 baseline 86) |
| exit_market without debt (G10) | — | 58 |

Edge rows reproduced on the real bytes:
- **Checkpoint age:** at 60 s, borrow succeeds. At 61 s, borrow and liquidate revert, and close reverts with #2101.
- **Insufficient collateral:** close at 90% reverts with #2106.
- **Entered owner:** emergency exit reverts with #2116 while entered.
- **Liquidator before L5:** closing seized units before L5 reverts on vault availability.
- **Repeat seizure:** works, and closing after L5 succeeds.
- **Isolation:** CF on the USDC market (G6) and listing a third market (G7) both revert.
- **D3 on a real pool push** (2,000,000 PYUSD sold into the live pool):
  - L1a records an observation (returns 0) instead of reverting.
  - Fallback refusals: #2111 with no chain, #2112 at 9:59, #2113 after a 61 s gap.
  - With the pool healthy, the fallback reverts #2104.
  - A blocked NAV with no fallback makes liquidation revert (health indeterminate).

Full table:

| Sequence | Stage | ok | CPU | Mem | Entries (m+d+w) | Keys | Writes | Auth | Error |
|---|---|---|---:|---:|---|---:|---:|---:|---|
| REAL RD0 baseline: live StgV1 receipt + live vault, emergency exit 10% (no StgV2) | D stage_emergency_exit 10% (StgV1 baseline) | yes | 61,219,511 | 9,094,514 | **86** (55+1+30) | 56 | 30 | 1 |  |
| REAL RC1 L1a checkpoint_nav (real receipt + vault v2) | L1a receipt.checkpoint_nav | yes | 15,243,483 | 5,401,646 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RB0 borrow with no checkpoint -> revert | borrow (no checkpoint) | no | 15,638,305 | 4,615,364 | **55** (50+1+4) | 51 | 4 | 0 |  |
| REAL RB1 L1a, borrow, WARM | L1a receipt.checkpoint_nav | yes | 15,243,483 | 5,401,646 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RB1 L1a, borrow, WARM | B borrow | yes | 21,574,855 | 4,616,762 | **64** (52+2+10) | 54 | 10 | 1 |  |
| REAL RB2 L1a, borrow, COLD | L1a receipt.checkpoint_nav | yes | 15,243,483 | 5,401,646 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RB2 L1a, borrow, COLD | B borrow | yes | 25,181,618 | 4,616,762 | **75** (62+2+11) | 64 | 11 | 1 |  |
| REAL RB3 borrow at checkpoint age 61 s -> revert | B borrow at age 61 | no | 15,655,530 | 4,616,762 | **55** (50+1+4) | 51 | 4 | 0 |  |
| REAL RB3b borrow at checkpoint age 60 s -> ok | B borrow at age 60 | yes | 21,574,855 | 4,616,762 | **64** (52+2+10) | 54 | 10 | 1 |  |
| REAL RB5 repeat borrow, WARM | B repeat borrow | yes | 22,823,767 | 4,651,760 | **66** (52+2+12) | 54 | 12 | 1 |  |
| REAL RB6 repeat borrow, COLD | B repeat borrow | yes | 26,411,412 | 4,651,760 | **77** (62+2+13) | 64 | 13 | 1 |  |
| REAL RD1 prepare_close 10% with debt, WARM | D stage_prepare_close 10% | yes | 25,694,966 | 5,250,766 | **68** (56+1+11) | 57 | 11 | 1 |  |
| REAL RD2 prepare_close 10% with debt, COLD | D stage_prepare_close 10% | yes | 29,288,372 | 5,939,941 | **80** (67+1+12) | 68 | 12 | 1 |  |
| REAL RD3 prepare_close at checkpoint age 61 s -> revert #2101 | D close at age 61 | no | 18,145,632 | 4,810,977 | **51** (44+1+6) | 45 | 6 | 0 | Error(Contract, #2101) |
| REAL RD4 prepare_close 90% -> revert #2106 insufficient collateral | D close 90% | no | 23,623,988 | 5,250,766 | **55** (48+1+6) | 49 | 6 | 0 | Error(Contract, #2106) |
| REAL RD5 emergency exit while entered (with debt) -> revert #2116 | D stage_emergency_exit 10% entered | no | 2,830,681 | 2,666,335 | **9** (7+1+1) | 8 | 1 | 0 | Error(Contract, #2116) |
| REAL RD7 prepare_close 10%, no debt (no health dependency) | D stage_prepare_close 10% no debt | yes | 8,596,709 | 4,092,317 | **59** (51+1+7) | 52 | 7 | 1 |  |
| REAL RD8 emergency exit while entered, no debt -> revert #2116 | D stage_emergency_exit 10% entered | no | 2,816,688 | 2,664,969 | **9** (7+1+1) | 8 | 1 | 0 | Error(Contract, #2116) |
| REAL RD9 exit_market (no debt), then emergency exit 10% (not entered) | exit_market(receipt) | yes | 9,841,895 | 3,509,388 | **58** (53+1+4) | 54 | 4 | 1 |  |
| REAL RD9 exit_market (no debt), then emergency exit 10% (not entered) | D stage_emergency_exit 10% not entered | yes | 62,382,473 | 9,320,306 | **89** (58+1+30) | 59 | 30 | 1 |  |
| REAL RD10 exit_market with debt -> revert | exit_market(receipt) with debt | no | 8,922,993 | 3,287,275 | **54** (52+1+1) | 53 | 1 | 0 |  |
| REAL RL1 L1a, L3 liquidate (cache-only), WARM | L1a receipt.checkpoint_nav | yes | 15,261,371 | 5,402,988 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RL1 L1a, L3 liquidate (cache-only), WARM | L3 liquidate (repay + unit move + queue) | yes | 32,465,772 | 4,613,348 | **80** (61+2+17) | 63 | 17 | 1 |  |
| REAL RL2 L1a, L3 from COLD -> revert by design (cache-only) | L1a receipt.checkpoint_nav | yes | 15,261,371 | 5,402,988 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RL2 L1a, L3 from COLD -> revert by design (cache-only) | L3 liquidate cold | no | 7,096,892 | 2,975,799 | **36** (34+1+1) | 35 | 1 | 0 |  |
| REAL RL3 L1a, L2, L3 from COLD | L1a receipt.checkpoint_nav | yes | 15,261,371 | 5,402,988 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RL3 L1a, L2, L3 from COLD | L2 cache_price(USDC) | yes | 4,274,357 | 2,004,886 | **17** (16+0+1) | 16 | 1 | 0 |  |
| REAL RL3 L1a, L2, L3 from COLD | L3 liquidate (repay + unit move + queue) | yes | 32,441,496 | 4,613,348 | **80** (61+2+17) | 63 | 17 | 1 |  |
| REAL RL4 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (fresh claim) | L1a receipt.checkpoint_nav | yes | 15,261,371 | 5,402,988 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RL4 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (fresh claim) | L2 cache_price(USDC) | yes | 4,274,357 | 2,004,886 | **17** (16+0+1) | 16 | 1 | 0 |  |
| REAL RL4 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (fresh claim) | L3 liquidate (repay + unit move + queue) | yes | 32,441,496 | 4,613,348 | **80** (61+2+17) | 63 | 17 | 1 |  |
| REAL RL4 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (fresh claim) | L4 vault.checkpoint_rewards (real AQUA claim) | yes | 9,896,281 | 4,664,377 | **29** (21+1+7) | 22 | 7 | 0 |  |
| REAL RL4 full staged sequence from COLD: L1a, L2, L3, L4 claim, L5 sync (fresh claim) | L5 receipt.sync_seized(0) -> vault.stage_seize_reassign | yes | 4,930,435 | 2,898,345 | **29** (19+0+10) | 19 | 10 | 0 |  |
| REAL RL4w full staged sequence from WARM | L1a receipt.checkpoint_nav | yes | 15,261,371 | 5,402,988 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RL4w full staged sequence from WARM | L2 cache_price(USDC) | yes | 4,292,250 | 2,004,886 | **17** (16+0+1) | 16 | 1 | 0 |  |
| REAL RL4w full staged sequence from WARM | L3 liquidate | yes | 32,441,496 | 4,613,348 | **80** (61+2+17) | 63 | 17 | 1 |  |
| REAL RL4w full staged sequence from WARM | L4 vault.checkpoint_rewards (real AQUA claim) | yes | 9,896,281 | 4,664,377 | **29** (21+1+7) | 22 | 7 | 0 |  |
| REAL RL4w full staged sequence from WARM | L5 receipt.sync_seized(0) -> vault.stage_seize_reassign | yes | 4,930,435 | 2,898,345 | **29** (19+0+10) | 19 | 10 | 0 |  |
| REAL RL5 L5 sync 61 s after the last claim (opens D6 boundary) | L1a receipt.checkpoint_nav | yes | 15,261,371 | 5,402,988 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RL5 L5 sync 61 s after the last claim (opens D6 boundary) | L3 liquidate | yes | 32,465,772 | 4,613,348 | **80** (61+2+17) | 63 | 17 | 1 |  |
| REAL RL5 L5 sync 61 s after the last claim (opens D6 boundary) | L5 receipt.sync_seized(0) -> vault.stage_seize_reassign | yes | 5,205,954 | 2,910,367 | **31** (20+0+11) | 20 | 11 | 0 |  |
| REAL RL6 liquidate at NAV checkpoint age 61 s, no fallback -> revert | L3 liquidate at age 61 | no | 16,463,720 | 3,770,997 | **58** (56+1+1) | 57 | 1 | 0 |  |
| REAL RL7 L5 before L3 (nothing queued) -> revert #2109 | L5 receipt.sync_seized(0) -> vault.stage_seize_reassign | no | 1,075,982 | 1,353,093 | **3** (3+0+0) | 3 | 0 | 0 | Error(Contract, #2109) |
| REAL RL8 repeat seizure (liquidator already holds units), L3 + L4 + L5 | L1a receipt.checkpoint_nav | yes | 15,266,221 | 5,403,720 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RL8 repeat seizure (liquidator already holds units), L3 + L4 + L5 | L3 liquidate (repeat) | yes | 32,723,771 | 4,615,412 | **80** (61+2+17) | 63 | 17 | 1 |  |
| REAL RL8 repeat seizure (liquidator already holds units), L3 + L4 + L5 | L4 vault.checkpoint_rewards (real AQUA claim) | yes | 9,920,028 | 4,666,022 | **28** (21+0+7) | 21 | 7 | 0 |  |
| REAL RL8 repeat seizure (liquidator already holds units), L3 + L4 + L5 | L5 receipt.sync_seized(1) -> vault.stage_seize_reassign | yes | 5,184,063 | 2,913,917 | **30** (20+0+10) | 20 | 10 | 0 |  |
| REAL RL9 liquidator closes seized units after L5 -> ok | liquidator stage_prepare_close | yes | 4,605,483 | 2,970,025 | **22** (15+1+6) | 16 | 6 | 1 |  |
| REAL RL10 liquidator closes seized units BEFORE L5 -> revert (vault availability) | L3 liquidate (setup) | yes | 32,465,772 | 4,613,348 | **80** (61+2+17) | 63 | 17 | 1 |  |
| REAL RL10 liquidator closes seized units BEFORE L5 -> revert (vault availability) | liquidator stage_prepare_close before sync | no | 4,408,236 | 2,962,535 | **20** (15+1+4) | 16 | 4 | 0 |  |
| REAL RX4 fee on: L1a, L3, L4, L5 liquidator, L5 reserve | L1a receipt.checkpoint_nav | yes | 15,261,371 | 5,402,988 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RX4 fee on: L1a, L3, L4, L5 liquidator, L5 reserve | L2 cache_price(USDC) | yes | 4,334,018 | 2,005,462 | **17** (16+0+1) | 16 | 1 | 0 |  |
| REAL RX4 fee on: L1a, L3, L4, L5 liquidator, L5 reserve | L3 liquidate (fee on) | yes | 32,734,980 | 4,643,914 | **84** (63+2+19) | 65 | 19 | 1 |  |
| REAL RX4 fee on: L1a, L3, L4, L5 liquidator, L5 reserve | L4 vault.checkpoint_rewards (real AQUA claim) | yes | 9,896,281 | 4,664,377 | **29** (21+1+7) | 22 | 7 | 0 |  |
| REAL RX4 fee on: L1a, L3, L4, L5 liquidator, L5 reserve | L5 receipt.sync_seized(0) -> vault.stage_seize_reassign | yes | 4,930,435 | 2,898,345 | **29** (19+0+10) | 19 | 10 | 0 |  |
| REAL RX4 fee on: L1a, L3, L4, L5 liquidator, L5 reserve | L5 receipt.sync_seized(1) -> vault.stage_seize_reassign | yes | 4,922,424 | 2,898,553 | **29** (19+0+10) | 19 | 10 | 0 |  |
| REAL RX6 CF on the USDC market -> revert (condition c, G6) | set_market_cf(usdc market) | no | 3,114,676 | 1,346,511 | **14** (12+1+1) | 13 | 1 | 0 |  |
| REAL RX7 third market listing -> revert (G7) | add_market(vault as third market) | no | 2,418,101 | 1,336,810 | **14** (12+1+1) | 13 | 1 | 0 |  |
| REAL RF0 pool pushed out of band by a real swap: L1a records a D3 observation (returns 0) | L1a receipt.checkpoint_nav | yes | 14,813,347 | 5,397,829 | **20** (19+0+1) | 19 | 1 | 0 |  |
| REAL RF1 pool pushed, chain just started: fallback reverts #2112 | L1b receipt.checkpoint_fallback | no | 5,030,555 | 3,275,901 | **10** (10+0+0) | 10 | 0 | 0 | Error(Contract, #2112) |
| REAL RF1b pool pushed, no chain: fallback reverts #2111 | L1b receipt.checkpoint_fallback | no | 5,007,198 | 3,274,275 | **10** (10+0+0) | 10 | 0 | 0 | Error(Contract, #2111) |
| REAL RF6 pool healthy: fallback reverts #2104 NavRefreshable | L1b receipt.checkpoint_fallback | no | 16,151,681 | 5,967,138 | **19** (19+0+0) | 19 | 0 | 0 | Error(Contract, #2104) |
| REAL RF7 pool pushed, NAV blocked, no fallback: liquidation reverts (indeterminate) | L3 liquidate while blocked, NAV 61+ s old, no fallback | no | 16,481,084 | 3,773,913 | **58** (56+1+1) | 57 | 1 | 0 |  |

| REALM RF3 pool pushed out of band, chain 9:59 -> fallback reverts #2112 | L1b receipt.checkpoint_fallback | no | 5,033,501 | 3,276,390 | **10** (10+0+0) | 10 | 0 | 0 | Error(Contract, #2112) |
| REALM RF2 pool pushed, chain 10:00: L1b fallback, L3 WARM | L1b receipt.checkpoint_fallback | yes | 7,872,333 | 3,818,937 | **15** (14+0+1) | 14 | 1 | 0 |  |
| REALM RF2 pool pushed, chain 10:00: L1b fallback, L3 WARM | L3 liquidate on fallback (WARM) | yes | 32,514,389 | 4,625,859 | **80** (61+2+17) | 63 | 17 | 1 |  |
| REALM RF4 pool pushed, chain 10:00: full L1b, L2, L3, L4, L5 from COLD | L1b receipt.checkpoint_fallback | yes | 7,872,333 | 3,818,937 | **15** (14+0+1) | 14 | 1 | 0 |  |
| REALM RF4 pool pushed, chain 10:00: full L1b, L2, L3, L4, L5 from COLD | L2 cache_price(USDC) | yes | 4,274,357 | 2,004,886 | **17** (16+0+1) | 16 | 1 | 0 |  |
| REALM RF4 pool pushed, chain 10:00: full L1b, L2, L3, L4, L5 from COLD | L3 liquidate on fallback | yes | 32,520,149 | 4,625,859 | **80** (61+2+17) | 63 | 17 | 1 |  |
| REALM RF4 pool pushed, chain 10:00: full L1b, L2, L3, L4, L5 from COLD | L4 vault.checkpoint_rewards (real AQUA claim) | yes | 9,698,302 | 4,658,046 | **29** (21+1+7) | 22 | 7 | 0 |  |
| REALM RF4 pool pushed, chain 10:00: full L1b, L2, L3, L4, L5 from COLD | L5 receipt.sync_seized(0) -> vault.stage_seize_reassign | yes | 4,950,373 | 2,901,801 | **29** (19+0+10) | 19 | 10 | 0 |  |
| REALM RF5 pool pushed, chain lapsed 61 s -> fallback reverts #2113 | L1b receipt.checkpoint_fallback | no | 5,033,501 | 3,276,390 | **10** (10+0+0) | 10 | 0 | 0 | Error(Contract, #2113) |

## 3. Testnet rehearsal (18 Nov 2026, Testnet only)

**Setup:**
- Keys are friendbot-funded `stellar keys generate` identities: `clg-admin` GCOKES3D…UAAL, `clg-liq` GBLWAC6A…QABJ and `clg-sup` GDCFKW4P…YICO. Only identity names were used; no key material was read.
- Scripts: `prototypes/cl-collateral-group/testnet/0{1..4}-*.sh`. They are resumable.
- Append-only journal of every transaction: `target/clg/testnet/journal.jsonl` (outside git).
- Candidates are the reviewed sources built with the Testnet admin as their init-admin constant. Only that constant differs from the measured builds.

**Fixtures, which make this a rehearsal rather than a qualification of the venue:**
- mock-token tPYUSD, tUSDC, tAQUA and tGAUGE.
- A Reflector-ABI oracle (`testnet-oracle`).
- A pool fixture (`testnet-pool`) with a settable slot0.
- The vault's investment step needs keeper observations on consecutive ledgers, which the CLI cannot hit. The minted command was therefore cancelled at MINTED, which leaves **cash-backed units** (no pool position). The position path is covered only by the local rows on the real pool bytes (§2).

| Contract | Testnet ID |
|---|---|
| Vault v2 | CAZBVTWVCR62OTDDCE6UTJWHGPSMYATXTYII7KV4UOLBFYANB5HP3G7F |
| Receipt StgV2 | CDAGPUU6VNCZ3TI65ZVIMJ7F5STWECFC2LAOIHOJTYON2UXUJXISJ2TH |
| Group controller | CD552QGXXFNBKOKTF63TDZVX3AWETL3HSBJZ2TO7BUKE5RT32O6Z2LJL |
| USDC lending market | CDHJBI53SZF5ATPGZV3RTQ2DHYGEYKCXCKETZ7CA3RVRO6RAXGVZ4DS4 |
| Oracle / pool fixtures | CAURK7PQJC2D6DC2DGYJWV6LTMDWJW5P2BADKN7DMZWCUVGINY5VQSC6 / CDUBGFQGKOF5DEMBFDSQVXPTXWGDRC4POJIUMO723APK7TG22FTB52GX |
| tPYUSD / tUSDC / tAQUA | CBCDTWPI…X5CZ / CC4GVSZ6…XUSD / CCVPY2T7…OSP2 |

**Flow 1: NAV-path liquidation**
1. StgV2 activated (fda181b2…aaa3).
2. 800 tPYUSD were staged, giving 7,920,430,357 units.
3. L1a returned rate 999,799 (e2e074aa…7af3).
4. A 300 tUSDC borrow succeeded (c3018bca…fa78).
5. The shortfall was created with `emergency_disable_collateral` (abe670b4…c323).
6. L1a (6abf2328…079), then L2 (93f50557…6f124).
7. L3 liquidate 100 tUSDC (bff17525…cbca8): 1,080,217,123 units moved and were queued as pending 0.
8. L4, a real tAQUA claim (8c86206a…4b41).
9. L5 sync (f8f674d2…dffe): the vault beneficiary moved and no boundary opened.
10. The liquidator closed in kind (f65f43f0…9959) and withdrew 54.55 tPYUSD + 53.46 tUSDC (4079c35a…f507f73).

**Flow 2: real-time D3 fallback**
1. The pool was moved out of range (8b07643d…c53cb).
2. Fallback with no chain was simulated: #2111.
3. 22 heartbeat `checkpoint_nav` calls were sent about every 30 s, each returning 0. Chain since 1795010134, last 1795010759.
4. At t+591 s the simulated fallback refused with #2112.
5. At t+620 s, L1b succeeded (678f96ce…71b0) with rate **988,811 = 98.90% of NAV**.
6. L2 (0fe15049…f6124), then **L3 on the fallback**, 50 tUSDC (a1454517…a9c5).
7. L4 (f3d6360d…79fd), then L5 (097c79c0…8870): 546,110,429 units moved.

**Bot:** `bots/cl-liquidator --once` (dry run) read the live Testnet state correctly. During the chain it planned "blocked, fallback admissible in about 570 s" and sent nothing.

## 4. Open issues for the review

1. **Emergency exit while entered.** The StgV1 extraction is 86 entries, and any health read inside it exceeds 100 (126–135 measured). StgV2 therefore refuses it while the owner is entered (#2116). G10 lets a debt-free owner exit the market first without the oracle (58 + 89 entries). An owner with debt must repay or use prepare_close, which needs a fresh NAV.
2. **NAV survives a pool push.** A NAV checkpoint taken before a pool push stays valid for up to 60 s (by design, rows RF7/RL6).
3. **D3 residual.** The chain is sampled, not continuous; see D3-CONTINUITY.md. Protection against unsampled healthy windows relies on a live keeper heartbeat of 20 s or less. The bot implements this, but the bot itself has not run live yet. Anyone can start a chain.
4. **D6 quarantine is coarse.** The boundary quarantines the whole next claim, including other holders' share; release is governed per boundary. L4 should run within 60 s before L5.
5. **The CF cut is timelocked.** Shortfall in the rows and on Testnet used `emergency_disable_collateral`; natural price-driven shortfalls were not exercised.
6. **Testnet gaps.** There is no concentrated position, and the oracle and pool are fixtures. CAFJ for USDC was not exercised (0-entry delta from [rpc]). The bot ran in dry run only.
7. **Native tests.** There is no native receipt test suite for StgV2. Coverage comes from compiled-bytes rows plus Testnet. The pre-existing receipt suite fails 134/201 even on the control tree (run-5), so it was not used.
8. **Reproducibility.** The receipt hash depends on the path of its out-of-workspace dependency (§README), so 140dcaab is only reproducible at the run-5 path.
9. **Commit size.** Commit 65fc44b exceeded 3,000 lines because it included three generated Cargo.lock files of about 2,000 lines each. It was pushed and not rewritten (no force push).
10. **Bot review points** from its builder:
    - L1b timing: the bot uses `last − since ≥ 600`, the contract uses `now − since ≥ 600`.
    - `account_liquidity` is indeterminate without a fresh NAV.
    - L2 cannot help on a late oracle round.
    - The L1b → L3 window is tight.
