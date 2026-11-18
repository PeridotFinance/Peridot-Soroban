# D3: proving the normal checkpoint was blocked *continuously* for 10 minutes

Status: implemented in the measurement double (`prototypes/cl-collateral-group/sleeve-d3`) and in one
pure, unit-tested source (`prototypes/cl-collateral-group/d3-chain/src/d3_chain.rs`) that the StgV2
receipt includes verbatim. For the second-opinion review, 18 Nov 2026.

## The gap

The user decision says: the liquidation-only fallback valuation is available only after the normal
checkpoint has been **blocked for 10 minutes**. The first D3 copy stored only the time of the
**first** blocked attempt. That rule admits the fallback after two momentary pool pushes ten
minutes apart with a healthy pool in between, which is exactly what D3 is meant to prevent
(the DESIGN §2.1 griefing window).

## Proposal (implemented): a heartbeat chain

On-chain code cannot see the pool between transactions, so "continuous" is defined over
observations:

| Event | Effect on `Blk { since, last, n }` (receipt instance storage) |
|---|---|
| `checkpoint_nav()` finds a named cause (venue valuation fails, depeg guard, pool out of band or out of range) | Does **not** revert. If `now − last ≤ BLOCK_GAP` (60 s) the chain is extended (`last = now`); otherwise it **restarts** (`since = last = now`). Returns 0. |
| `checkpoint_nav()` succeeds | Chain **deleted**. Permissionless: the borrower, the keeper or anyone can clear it at any healthy moment. |
| `checkpoint_fallback()` | Re-proves a cause **now** (otherwise #4 NavRefreshable). Then: no chain → #11 NotBlocked; `now − last > 60` → #13 ChainBroken; `now − since < 600` → #12 BlockTooRecent; else admitted and the chain is extended. |
| Oracle (CALI) missing or stale | Both calls revert `OracleStale` and record nothing, so the chain lapses after 60 s and the account becomes repay-only (D4). |

**What is proven at admission:** during the last ≥ 600 s, consecutive blocked observations were
never more than 60 s apart, a cause holds at the admission instant, and no healthy NAV checkpoint
landed in between.

**Residual (for the review):** an in-band interval shorter than 60 s that nobody sampled is not
seen. Countermeasure: while a chain is open the protocol keeper (bots/cl-liquidator `watch`)
calls `checkpoint_nav` every ≤ 20 s; any in-band moment it catches deletes the chain. An attacker
must therefore keep the pool out of band against arbitrage at every keeper sample for 10 minutes,
not just twice. A stronger variant (require a minimum observation count, e.g. n ≥ 11) adds no
real protection because the attacker controls when its own observations land; it is not adopted.

`BLOCK_GAP = 60 s` equals the checkpoint maximum age, so the liquidator's normal L1 cadence also
keeps the chain alive. Both constants live in `d3_chain.rs`.

## Tests

* `d3-chain` unit tests (7): 9:59 refused / 10:00 admitted; healthy checkpoint clears and a new
  chain needs its own 10 minutes; a 61 s gap restarts (first-block age alone is not enough);
  lapse 61 s before the fallback is ChainBroken, 60 s is alive; two momentary pushes 10 minutes
  apart are refused; repeated fallbacks extend; clock never runs backwards into a chain.
* Harness rows on the archived Mainnet capture (ledger 65527300, local host 29.0.1,
  `mock_all_auths`, static-rate group stack, D3 sleeve double 27,203 B). Because the capture's
  oracle prices are frozen, the chain is seeded by a measurement-only setter instead of waiting
  10 minutes; causes are emulated with `band_bps = 0` / `depeg_bps = 0` as in the GF rows.

## Measured rows

Entries = memory reads + disk reads + writes; keys = unique ledger keys. Evidence:
`target/clg/evidence/d3-run-result-20261118.json` (outside git), sha256
`a61ba86338ccc1f100a463468d8530af2ff6ce20a2047df6b58454d743f2ec36`.

| Sequence | Stage | ok | CPU | Mem | Entries (m+d+w) | Keys | Writes | Auth | Error |
|---|---|---|---:|---:|---|---:|---:|---:|---|
| D3 GD1 blocked checkpoint_nav starts a chain (returns 0, no revert) | L1a checkpoint_nav (D3: blocked = chain observation, no revert) | yes | 15,745,162 | 5,368,450 | **18** (17+0+1) | 17 | 1 | 0 |  |
| D3 GD1b blocked by depeg starts a chain | L1a checkpoint_nav (D3: blocked = chain observation, no revert) | yes | 14,272,119 | 5,368,450 | **18** (17+0+1) | 17 | 1 | 0 |  |
| D3 GD2 blocked checkpoint_nav 60 s after the last observation extends the chain | L1a checkpoint_nav (D3: blocked = chain observation, no revert) | yes | 15,761,373 | 5,370,075 | **18** (17+0+1) | 17 | 1 | 0 |  |
| D3 GD3 blocked checkpoint_nav 61 s after the last observation restarts the chain | L1a checkpoint_nav (D3: blocked = chain observation, no revert) | yes | 15,761,373 | 5,370,075 | **18** (17+0+1) | 17 | 1 | 0 |  |
| D3 GD4 fallback at 9:59 of continuous block -> revert #12 BlockTooRecent | L1b checkpoint_fallback (D3 chain admission) | no | 4,768,399 | 3,183,305 | **8** (8+0+0) | 8 | 0 | 0 | Error(Contract, #12) |
| D3 GD5 fallback at 10:00 -> ok, then L3 liquidate on fallback, WARM | L1b checkpoint_fallback (D3 chain admission) | yes | 9,027,696 | 3,994,549 | **13** (12+0+1) | 12 | 1 | 0 |  |
| D3 GD5 fallback at 10:00 -> ok, then L3 liquidate on fallback, WARM | L3 liquidate on fallback | yes | 31,295,246 | 4,358,150 | **80** (61+2+17) | 63 | 17 | 1 |  |
| D3 GD5c fallback at 10:00, L2, L3 from COLD | L1b checkpoint_fallback (D3 chain admission) | yes | 9,027,696 | 3,994,549 | **13** (12+0+1) | 12 | 1 | 0 |  |
| D3 GD5c fallback at 10:00, L2, L3 from COLD | L2 cache_price(USDC) | yes | 4,267,930 | 2,004,198 | **17** (16+0+1) | 16 | 1 | 0 |  |
| D3 GD5c fallback at 10:00, L2, L3 from COLD | L3 liquidate on fallback | yes | 31,316,446 | 4,358,150 | **80** (61+2+17) | 63 | 17 | 1 |  |
| D3 GD5d depeg cause: fallback at 10:00, L3, WARM | L1b checkpoint_fallback (D3 chain admission) | yes | 7,437,375 | 3,674,268 | **11** (10+0+1) | 10 | 1 | 0 |  |
| D3 GD5d depeg cause: fallback at 10:00, L3, WARM | L3 liquidate on fallback (depeg) | yes | 31,295,246 | 4,358,150 | **80** (61+2+17) | 63 | 17 | 1 |  |
| D3 GD6 fallback 61 s after the last observation -> revert #13 ChainBroken | L1b checkpoint_fallback (D3 chain admission) | no | 4,768,399 | 3,183,305 | **8** (8+0+0) | 8 | 0 | 0 | Error(Contract, #13) |
| D3 GD7 clear: healthy checkpoint_nav deletes the chain | L1a checkpoint_nav (D3: blocked = chain observation, no revert) | yes | 15,760,248 | 5,370,075 | **18** (17+0+1) | 17 | 1 | 0 |  |
| D3 GD7b after clear: blocked again, fallback -> revert #11 NotBlocked (chain deleted) | L1b checkpoint_fallback (D3 chain admission) | no | 4,744,204 | 3,181,680 | **8** (8+0+0) | 8 | 0 | 0 | Error(Contract, #11) |
| D3 GD7c no chain at all: fallback -> revert #11 NotBlocked | L1b checkpoint_fallback (D3 chain admission) | no | 4,744,204 | 3,181,680 | **8** (8+0+0) | 8 | 0 | 0 | Error(Contract, #11) |
| D3 GD8 cause absent now (NAV refreshable) with a 10-min chain -> revert #4 | L1b checkpoint_fallback (D3 chain admission) | no | 15,649,522 | 5,872,852 | **17** (17+0+0) | 17 | 0 | 0 | Error(Contract, #4) |
| D3 GD9 full D3 sequence from COLD: fallback, L2, L3, L4 claim, L5 sync | L1b checkpoint_fallback (D3 chain admission) | yes | 9,027,696 | 3,994,549 | **13** (12+0+1) | 12 | 1 | 0 |  |
| D3 GD9 full D3 sequence from COLD: fallback, L2, L3, L4 claim, L5 sync | L2 cache_price(USDC) | yes | 4,267,930 | 2,004,198 | **17** (16+0+1) | 16 | 1 | 0 |  |
| D3 GD9 full D3 sequence from COLD: fallback, L2, L3, L4 claim, L5 sync | L3 liquidate on fallback | yes | 31,316,446 | 4,358,150 | **80** (61+2+17) | 63 | 17 | 1 |  |
| D3 GD9 full D3 sequence from COLD: fallback, L2, L3, L4 claim, L5 sync | L4 vault.checkpoint_rewards (real AQUA claim, separate tx) | yes | 9,857,079 | 4,663,119 | **29** (21+1+7) | 22 | 7 | 0 |  |
| D3 GD9 full D3 sequence from COLD: fallback, L2, L3, L4 claim, L5 sync | L5 sync_seized(0) | yes | 3,154,895 | 2,652,869 | **25** (15+0+10) | 15 | 10 | 0 |  |

Reading: the blocked observation costs the same 18 entries / 1 write as a successful L1a (the chain
lives in the instance entry). Admission adds nothing measurable to L1b (13 entries, as GF3). L3 on
the fallback stays at 80 entries. All refusals revert with their distinct codes.
