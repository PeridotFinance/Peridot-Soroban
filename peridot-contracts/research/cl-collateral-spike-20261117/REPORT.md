# CL collateral measurement spike (item S): 17 November 2026

Spec: `prototypes/cl-staged-market-integration/COLLATERAL-BORROWING-SPEC.md` §2 and §8, D1. Nothing was signed or sent. The only network use was read-only public RPC: `getLatestLedger`, `getLedgerEntries` and `simulateTransaction`. Written by the spike subagent; saved by the lead (the subagent could not write this file).

## Verdict: FIT-ONLY-STAGED

Every row is inside the network limits. Only the internal limits of 100 entries (memory reads + disk reads + writes) and 100 unique keys bind. CPU peaks at 75.6M and no row is ≥ 98M. Memory peaks at 14.8 MB.

**Answer.** One borrow plus one units-only liquidation, with the CL vault valued live inside the health check, do not both fit as single transactions. Both fit when staged:

- **Borrow:** a permissionless `checkpoint_nav` tx (23 entries), then a borrow that reads it: 71 entries with a warm price cache, 85 cold.
  - A single-tx live borrow fits only with a warm controller price cache: 89 entries.
  - With a cold cache it is 101.
- **Units-only liquidation:** checkpoint, then liquidate: 85 warm / 93 cold.
  - With the minimum vault touch a real reassign needs (reading both holders' custody records: +4 entries, plus 2 holder writes), it is about 91 warm / about 99 cold.
  - So the robust sequence is checkpoint, then price pre-warm, then liquidate.
- **Conditions:**
  1. You approve the bounded-age checkpoint policy (item 1, option b; handoff 00 §6).
  2. The vault-side reward claim stays out of the seize transaction. A claim proxy inside seize is 114–122 entries even when staged.
  3. Accounts that also hold a cash-collateral market do not fit for liquidation even when staged: 106 cold, 102 warm with the custody read.

**D1.** Shape 1 with a live read (89–101 entries) exceeds the ~85-entry kill line. It is not a stop only because shape 4 fits. Collateral is executable only with the checkpoint policy.

## Live network limits (ledger 65527300, protocol 29, fresh `getLedgerEntries` of the config settings)

| Limit | Value |
|---|---:|
| `txMaxInstructions` | 400,000,000 |
| `txMemoryLimit` | 41,943,040 |
| `txMaxDiskReadEntries` | 200 |
| `txMaxFootprintEntries` | 400 |
| `txMaxWriteLedgerEntries` | 200 |
| `txMaxDiskReadBytes` | 200,000 |
| `txMaxWriteBytes` | 132,096 |

Last-modified ledgers: compute, ledger cost and ledger cost ext 65080525; memory cost params 62447231; CPU cost params **65305114**, which is after ledger 65264920, the last ledger at which host 29.0.1 was verified to match.

The harness test `cost_model_vs_live` found **9 CPU coefficients that differ** from host 29.0.1; those that matter here are `Int256Mul`, `Int256Shift`, `VerifyEd25519Sig` and `ComputeEd25519PubKey`. Every CPU figure below is re-priced with the live coefficients (tracker iterations × coefficient delta): +0.2M to +0.6M per row. All memory coefficients match.

## Topology: T2, real vs mocked

| Component | What was used |
|---|---|
| CL vault `CD6N…AWL63` | Real `e6638030…` bytes and the live instance and storage |
| Staged receipt `CCGK…XBCI` | Real `140dcaab…`, live |
| Aquarius pool `CAPI…IDQYX` | Real, live |
| Reflector `CALI…PLE6M` | Real, live; the controller's oracle, not a double |
| PYUSD and USDC SACs | Real, live |
| Controller | Fresh local instance of deployed `847894a6…` (generic build, Reflector `lastprice`). Admin is the Ledger account. First `set_oracle` and first CF set are immediate. CF 50% for the sleeve and 50% for the PYUSD cash market |
| USDC lending market and PYUSD cash market | Fresh local instances of deployed `4229b7ef…` |
| USDC market settings | Reserve 1000 bps, cash borrowing enabled, JRM `4f57459d…` (release artifact; 2% / 18% / 400% / kink 80%) |
| PYUSD market settings | 100% reserve, collateral only |
| Sleeve | **Spike-only double** (9,968 bytes, sha `dc1ea9e8…`): a units ledger plus the controller-facing views |

Sleeve valuation sources: `src1` = the real receipt's `stage_account(owner)`, which calls the vault's live NAV (upper bound); `src0` = the vault's `get_asset_amounts_per_shares` directly (lower bound); `mode2` = bounded-age checkpoint with 60 s max age, expired returns rate 0. Seize variants: default moves units only; `touch` also reads the vault's `stage_beneficial` for both holders; `proxy` also calls the vault's `checkpoint_rewards`.

**Invested state.** The live vault was uninvested at capture; open command 2 was funded at phase 1. The harness continued that command locally through the real code (`stage_convert`, `stage_mint`, three `stage_checkpoint_investment`, `stage_invest`, `stage_finalise`) to a real pool position: liquidity 3,995,332,283,154; 7,996,432,200 units; NAV rate 1.000002. Only receipt-owner and keeper authorizations were mocked; no position state was written by hand.

**Test capital** (mint auth host-mocked): 3,000 PYUSD and 100 USDC to the owner; 5,000 USDC to a supplier contract; 2,000 USDC to the treasury G account, used as liquidator (live trustline; approves the market for `repay_on_behalf`'s `transfer_from`).

**Shortfall:** a spike setter shrinks the borrower's units to 40%. Time is not advanced (Reflector would go stale); `emergency_disable_collateral` not used. **Cold cache:** controller `PriceCache` entries removed from the snapshot; "warm" = fresh under 600 s. **Auth:** `mock_all_auths`; nonce entries created and counted, one Ed25519 verification metered.

**Evidence class.** Local host only (SDK / env-host 29.0.1). Not Core, not signed, no fees or transaction size. Each row is its own Env, chained snapshot to snapshot, advancing 1 ledger and 6 s per step.

## Rows

CPU uses the live cost model. Entries are mem+disk+writes. **ckpt** = staged `checkpoint_nav` tx first (14.35M CPU, 6.67 MB, 23 (22+0+1), 22 keys). **prewarm** = staged `cache_price` txs first: PYUSD 4.42M / 17 entries / 16 keys; USDC 4.25M / 17 / 16.

### Views

| Row | CPU | Memory | Entries | Keys |
|---|---:|---:|---|---:|
| V1 vault `get_asset_amounts_per_shares` (live NAV incl. `estimate_withdraw_position`) | 11,479,208 | 3,845,091 | 15 (15+0+0) | 15 |
| V2 real receipt `stage_account` | 13,779,603 | 5,432,350 | 19 (19+0+0) | 19 |
| V3 `cache_price(PYUSD)` | 4,422,617 | 2,001,689 | 17 (16+0+1) | 16 |
| V4 sleeve snapshot, src1 | 14,331,079 | 6,666,971 | 22 (22+0+0) | 22 |
| V5 sleeve snapshot, src0 | 12,038,772 | 5,078,761 | 18 (18+0+0) | 18 |

For comparison, the uninvested live valuation (RPC simulation) was 6.37M CPU and 10 keys, with no pool read.

### Shape 1: borrow 300 USDC, sleeve the only collateral

| Row | CPU | Memory | Entries | Keys | Verdict |
|---|---:|---:|---|---:|---|
| S1a live src1, cold | 43,378,346 | 10,189,094 | **101** (88+2+11) | 90 | OVER |
| S1b live src1, warm | 36,896,578 | 10,189,094 | 89 (78+2+9) | 80 | FIT |
| S1c live src0, cold | 40,971,177 | 8,583,448 | 97 (84+2+11) | 86 | FIT (lower bound) |
| S1d prewarm, then live src1 | 36,896,578 | 10,189,094 | 89 (78+2+9) | 80 | FIT |
| S1e repeat borrow, warm | 38,124,191 | 10,225,941 | 92 (78+2+12) | 80 | FIT |
| S1f repeat borrow, cold | 44,621,545 | 10,225,941 | **104** (88+2+14) | 90 | OVER |

### Shape 4: checkpoint borrow

| Row | CPU | Memory | Entries | Keys | Verdict |
|---|---:|---:|---|---:|---|
| S4a ckpt, then borrow, cold | 29,548,993 | 4,966,616 | 85 (72+2+11) | 74 | FIT |
| S4b ckpt, then borrow, warm | 22,749,301 | 4,674,251 | 71 (60+2+9) | 62 | FIT |

### Shape 2: borrow with an extra PYUSD cash-collateral market

| Row | CPU | Memory | Entries | Keys | Verdict |
|---|---:|---:|---|---:|---|
| S2a live src1, cold | 46,284,662 | 10,190,022 | **114** (101+2+11) | **103** | OVER |
| S2b live src1, warm | 39,718,348 | 10,190,022 | **102** (91+2+9) | 93 | OVER |
| S2c live src0, warm | 37,354,994 | 8,584,376 | 98 (87+2+9) | 89 | FIT (lower bound) |
| S2d ckpt, then borrow, cold | 32,367,831 | 5,711,805 | 98 (85+2+11) | 87 | FIT |
| S2e ckpt, then borrow, warm | 25,509,372 | 5,001,300 | 84 (73+2+9) | 75 | FIT |

### Shape 3: liquidate, repay 100 USDC, units-only seize, no vault burn

| Row | CPU | Memory | Entries | Keys | Verdict |
|---|---:|---:|---|---:|---|
| S3a live src1, cold | 65,161,113 | 10,965,967 | **109** (90+2+17) | 92 | OVER |
| S3b live src1, warm | 58,798,406 | 9,723,377 | **103** (86+2+15) | 88 | OVER |
| S3c live src0, cold | 62,826,474 | 10,451,017 | **105** (86+2+17) | 88 | OVER |
| S3d live src0, warm | 56,373,429 | 9,212,291 | 99 (82+2+15) | 84 | FIT (lower bound) |
| S3e live + proxy, cold | 75,561,884 | 14,796,348 | **133** (106+3+24) | **109** | OVER |
| S3f live + proxy, warm | 69,155,787 | 13,531,406 | **127** (102+3+22) | **105** | OVER |
| S3g ckpt, cold | 40,546,867 | 6,782,208 | 93 (74+2+17) | 76 | FIT |
| S3h ckpt, warm | 33,835,562 | 5,510,333 | 85 (68+2+15) | 70 | FIT |
| S3g-t ckpt + custody read, cold | 42,728,520 | 7,686,981 | 97 (78+2+17) | 80 | FIT (+2 writes ≈ 99) |
| S3h-t ckpt + custody read, warm | 36,011,745 | 6,401,240 | 89 (72+2+15) | 74 | FIT (+2 ≈ 91) |
| S3k-t ckpt + prewarm + custody read | 36,032,161 | 6,401,240 | 89 (72+2+15) | 74 | FIT |
| S3i ckpt + proxy, cold | 51,157,766 | 10,908,816 | **122** (95+3+24) | 98 | OVER |
| S3j ckpt + proxy, warm | 44,361,729 | 9,609,059 | **114** (89+3+22) | 92 | OVER |

### Shape 3 on the shape-2 account

| Row | CPU | Memory | Entries | Keys | Verdict |
|---|---:|---:|---|---:|---|
| S3x-a live, cold | 68,062,477 | 11,314,380 | **122** (103+2+17) | **105** | OVER |
| S3x-b live, warm | 61,672,859 | 10,066,026 | **116** (99+2+15) | **101** | OVER |
| S3x-c ckpt, warm | 36,783,672 | 5,837,424 | 98 (81+2+15) | 83 | FIT (2 spare) |
| S3x-d ckpt, cold | 43,490,967 | 7,117,753 | **106** (87+2+17) | 89 | OVER |
| S3x-e ckpt + custody read, warm | 38,971,144 | 6,758,374 | **102** (85+2+15) | 87 | OVER |

### Setup rows on the pilot bytes (all fit)

| Call | CPU | Entries | Keys | Writes |
|---|---:|---:|---:|---:|
| `stage_convert` | 36.1M | 28 | 19 | 9 |
| `stage_mint` | 18.4M | 44 | 32 | 12 |
| `stage_checkpoint_investment` | 3.7M | 8 | 6 | 2 |
| `stage_invest` | 57.7M | 56 | 39 | 17 |
| `stage_finalise` | 11.5M | 26 | 18 | 8 |

## What dominated

Keys by owner in S1a (cold, 90 keys); [S1b warm, 80] where different: USDC lending market 36; controller 19 [9]; contract code 8; JRM 6; pool 4; vault 4; USDC SAC 3; sleeve 2; PYUSD SAC 2; staged receipt 2; Reflector 1; nonce 1; account 1; trustline 1.

- The live valuation adds about 18 entries and about 14M CPU (S1b − S4b; same in S3b − S3h).
- The second valuation in a liquidation adds CPU only, no entries.
- A cold price cache adds about 10–12 entries.
- The reward-claim proxy is the heaviest add-on: +24 entries live and +29 staged (7 more pool keys, 3 reward contracts).

**Calibration.** `src1` also loads the 9.9 KB double next to the 112 KB receipt; a merged production receipt would save about 2 entries. Production should fall between `src0` and `src1`: about 99 for S1a and about 107 for S3a.

## What was not measured

- The vault-side units reassign and entitlement writes (the frozen vault traps `custody_*`); only a lower bound (custody read) and an upper bound (claim proxy).
- Signing, fees, transaction size and Core application.
- The item-2 controller patches ("revert instead of zero") and an LP-style liquidation snapshot (logic, not entry counts).
- Other position shapes: only 800 PYUSD in range at tick −2.

## What would change the verdict

- Raising the internal entry/key limit to about 135 makes every measured row fit as a single tx (open question 0).
- Slimming the lending market's 36-key borrow/repay path is the biggest code lever.
- A keeper-held warm price cache saves about 10 entries in every row.
- **NO-FIT if any holds:** the checkpoint policy is rejected; the reassign needs more than about 1 entry (cold) or 9 (warm) beyond the custody-read floor; the reward claim must sit inside seize; accounts mix sleeve and cash collateral.

## Rerun

From `peridot-contracts/`: `bash prototypes/cl-collateral-spike/run.sh` (captures fresh state, builds the sleeve with `stellar contract build`, iterates and fetches missing keys, refreshes at one ledger, compares the cost model, summarizes).

Against the archived capture:

```
SPIKE_STATE_DIR=$PWD/.scratch/cl-collateral-spike/state SPIKE_WASM_DIR=$PWD/.scratch/cl-collateral-spike/wasm CARGO_TARGET_DIR=$PWD/.scratch/spike-target bash prototypes/cl-collateral-spike/loop.sh
python3 -I prototypes/cl-collateral-spike/summarize.py <state> <out>
```

The invest step depends on live state: it continues the owner's latest unfinished open command, skips if the vault is already invested, and fails if the 8,000,000,000 backing cap is full with no command open.

## Evidence

`results.json` and `rows-table.md` in this folder. Capture ledger 65527300, refreshed at 65527301; the last fill pass fetched 2 absent keys later.

Raw files in `.scratch/cl-collateral-spike/` (sha256):

| File | Bytes | sha256 |
|---|---:|---|
| `state/state.json` | 917,279 | `70cdf2517a578729c789ddb0292cfb638b85fc4f52136b7b685e9695e6d85a0b` |
| `state/run-result.json` | 221,130 | `4e7a257a4f26642e03c952b588e597a75268f493c20269110edf20f01f9e2eb9` |
| `wasm/controller.wasm` | 63,285 | `847894a6…` |
| `wasm/market.wasm` | 89,033 | `4229b7ef…` |
| `wasm/jrm.wasm` | 7,644 | `4f57459d…` |
| `wasm/sleeve.wasm` | 9,968 | `dc1ea9e84c9f5a7ae0883b7221adab4764e41ba30aa08c3398b0c836b32d1ec3` |

Vault code (`e6638030…`) and receipt code (`140dcaab…`) were loaded from the captured contract-code entries.

Pilot state during the session: 65526929 units available, uninvested; 65527069 close command 1 locked; 65527208 close command 1 terminal, open command 2 funded (phase 1); 65527300 open command 2 still at phase 1.
