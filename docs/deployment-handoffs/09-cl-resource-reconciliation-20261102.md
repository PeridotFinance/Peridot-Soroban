# CL Stage 1: resource reconciliation checkpoint — 2026-11-02

Stage 1 remains incomplete. This continues PR75 from `64093aa3c0f77ab510ebf110138195405bb0d430`; that commit, its machine checkpoint and its evidence are preserved. Shared handoff00 is unchanged. This checkpoint does not grant production approval or change reserve/funding preparation: borrowing stays disabled, production maintenance margin unset, no Upshift binding, no signing, broadcasting, keys or secrets.

## Results

- Protocol29 compiled controlled-state suite: **12 PASS / 0 FAIL**. Actual V3, receipt, share-vault and facade WASM plus pinned Aquarius bytecode execute the full underlying→shares→underlying open/activate/close path. Funded liquidation, repay-release, interrupted-open recovery during claim outage, receipt/margin custody and allowance hooks execute too.
- Internal-policy acceptance suite: **1 PASS / 11 FAIL**. The 100M CPU or100-entry footprint limit stops underlying deposit or swap-open, so dependent lifecycle stages do not execute under that policy.
- Native vault: **44 PASS**. Native funded synthetic-prestate prototype on host25: **9 PASS**, separately classified; this is not target-runtime venue proof. Checker regressions: **6 PASS**.
- Both `check_budget.sh` reports remain **FAIL**. Captured-network report has no exceeded measured ceilings, but required `cancel_after_execute`, `quarantine_reconciliation` and `waiver_redistribution` are missing. Full transaction envelope/return/history/base-fee evidence is also absent. Resource rows indicate invocation outcomes; the complete Cargo results establish that later assertions ran.

## Limits and runtime

The older capture at ledger65264375 already recorded 400M instructions, 40MiB memory, 400 footprint entries and 200 writes. The old checker’s 100M/100/50 were internal policy, not those network limits. Its historical report remains untouched.

The new read-only public capture at ledger65264920 includes all relevant numeric ceilings, CPU/memory coefficients, fee settings and archival settings; SHA256 `a28fb64bb85c74f8f0d1d4bd7a0daf413ada6f603647c64f556a7905af554dcd`. Internal policy remains 100M instructions / 100 footprint / 50 writes. Named `captured-network-diagnostic` uses 400M / 400 / 200. Both retain 41,943,040 memory bytes, 200,000 disk-read bytes, 132,096 write bytes, 16,384 event bytes, 250 key bytes, 65,536 data-entry bytes, 131,072 code-entry bytes and the captured 132,096 transaction-size ceiling. Disk-read entry caps are respectively100 and200. Transaction-envelope size is not supplied by the SDK invocation meter and remains unmeasured, not silently passed.

An isolated native harness pins SDK29.0.1/host29.0.1 (host revision `ff6ae01b3465ac44185d2824876491e62c9a4cc4`); the production workspace remains SDK25.3.1. Both 86-entry cost vectors exactly match the capture; the validator compares every coefficient and rejects a mismatch before definitive execution. It does not reprice host25 execution or merely change a ledger-version field. Official supported-runtime references: [SDK29.0.1](https://github.com/stellar/rs-soroban-sdk/releases/tag/v29.0.1), [RPC29.0.1](https://github.com/stellar/stellar-rpc/releases/tag/v29.0.1).

Archival limits also match: persistent minimum2,073,600, temporary minimum17,280, maximum3,110,400 ledgers. Fixture starts at ledger100/timestamp1000 on a local zero network ID, retaining Mainnet rejection guards. Host Darwin arm64/Rust1.96.0, Stellar CLI26.1.0. Underlying/reward assets are local SAC fixtures; oracle and plane are native synthetic fixtures. Margin is an explicitly labelled **2x diagnostic configuration with harness-only25% maintenance**, while the share vault is unleveraged1x. Actual PYUSD/USDC issuer assets and live pool/oracle state are not imported. Aquarius SHA256 is `12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`.

## Metering reconciliation

`budget().reset_unlimited()` affected the execution budget but did not disable the SDK’s separate invocation resource check. It has been removed from this harness’s measured paths. Acceptance runs explicitly install the selected CPU/memory and invocation ceilings before every call. A separately labelled `profiling-only` mode has finite enlarged execution ceilings and cannot supply acceptance evidence.

Protocol25’s cumulative memory counter is not protocol29’s refunded transient-memory / peak high-water measurement. The prior47,596,008 bytes therefore did not prove protocol29 memory failure. Final baseline swap-open is19,370,578 bytes.

Optional SDK authorization/tree/storage diagnostics use a separate shadow meter. The default shadow allowance and then512MiB allowance were insufficient: they erased late diagnostic descriptors/footprint data and caused an auth snapshot panic after execution returned. Those attempts remain failed evidence (`baseline29-valid.log`). The harness now sets finite20B instructions/4GiB **shadow-only** limits after `enforce_resource_limits`; normal execution caps remain selected and unchanged. Final profiles record consumed shadow resources and reject incomplete descriptors or exhausted diagnostics. Baseline swap uses about625MB shadow memory, separately from19.37MB execution peak. No controller/facade/vault authorization is mocked; only exact synthetic external admin/user auth trees are supplied.

Optional snapshot-file writing at `Env::drop` is disabled: after a genuine policy CPU rejection it otherwise double-panics while reading events through the exhausted execution meter. This suppresses only the redundant disk dump, not execution, authorization, resource checks or the explicitly captured traces/results. Historical failed attempt `policy29.log` is retained. A new-vault build initially omitted the synthetic admin override and failed setup; `network29.log` preserves this ordinary fixture failure. The corrected `stellar contract build` and all later setup/results pass. Neither setup panic counts as an authorization rejection.

## Final resource table

These are protocol29 acceptance quantities under captured-network ceilings and archival settings. Policy comparison columns apply unchanged internal ceilings to those quantities; they are **not** evidence that the full lifecycle executed under internal policy. Separate policy execution stops earlier and fails. Fees are partial local resource estimates at captured rates, including measured rent; they exclude transaction envelope/history/base/return bytes and are not RPC fee quotes. Footprint is memory-read + disk-read + write entries, matching the pinned SDK check.

| Flow | CPU | Peak memory bytes | Footprint | Writes | Partial fee stroops | Internal-policy quantity comparison | Captured ceilings |
|---|---:|---:|---:|---:|---:|---|---|
| deposit_underlying | 102,202,337 | 9,837,972 | 64 | 20 | 17,045,510 | FAIL: instructions | PASS |
| redeem | 137,178,954 | 10,110,763 | 59 | 17 | 556,041 | FAIL: instructions | PASS |
| receipt_custody_ownership_change | 32,521,898 | 5,956,351 | 114 | 21 | 1,697,697 | FAIL: footprint | PASS |
| begin_open | 26,629,248 | 7,815,846 | 99 | 15 | 4,218,672 | within ceilings (not policy lifecycle proof) | PASS |
| swap_open_position_v3 | 165,103,823 | 19,370,578 | 159 | 40 | 20,528,742 | FAIL: instructions,footprint | PASS |
| activate_open_position_v3 | 68,382,638 | 10,252,915 | 169 | 34 | 3,220,617 | FAIL: footprint | PASS |
| prepare_close_position_v3 | 50,911,639 | 9,840,196 | 111 | 25 | 1,605,455 | FAIL: footprint | PASS |
| swap_close | 176,929,826 | 18,308,944 | 162 | 36 | 234,670 | FAIL: instructions,footprint | PASS |
| finish_close | 58,454,748 | 6,782,965 | 97 | 30 | 84,083 | within ceilings (not policy lifecycle proof) | PASS |
| funded_liquidation | 95,327,734 | 13,124,644 | 203 | 52 | 2,706,834 | FAIL: footprint,write_entries | PASS |
| repay_margin_position_v3 | 19,149,768 | 3,425,181 | 79 | 12 | 123,199 | within ceilings (not policy lifecycle proof) | PASS |
| repay_release | 13,643,820 | 3,093,501 | 88 | 18 | 1,050,748 | within ceilings (not policy lifecycle proof) | PASS |
| cancel_after_swap | 37,321,334 | 7,745,582 | 123 | 41 | 1,771,356 | FAIL: footprint | PASS |
| allowance_transfer_from | 9,731,474 | 3,649,929 | 43 | 15 | 853,959 | within ceilings (not policy lifecycle proof) | PASS |
| stress_4_holders_1pct_reserve_trade_swap_open | 229,779,425 | 22,629,185 | 158 | 40 | 4,218,338 | FAIL: instructions,footprint | PASS |
| stress_16_holders_5pct_reserve_trade_swap_open | 165,546,631 | 19,790,712 | 157 | 38 | 4,190,427 | FAIL: instructions,footprint | PASS |

## Call profile and headroom

Detailed trees and cost-type tracker quantities are in `calls-network29-archival.jsonl`; `swap-memory-breakdown.json` reconciles them to the resource rows. Protocol29 per-call memory deltas attribute *growth of the global high-water mark*, not independent per-call peak allocation. CPU self-costs telescope to the root. Configuration/storage serialization costs remain inside their enclosing calls and host cost-type trackers; they are not moved out of measurement.

| Baseline role | Calls | Exclusive CPU | High-water growth bytes |
|---|---:|---:|---:|
| aquarius | 10 | 103,668,639 | 11,566,627 |
| lending_controller | 4 | 3,309,836 | 2,663,623 |
| v3 | 1 | 3,147,898 | 1,404,353 |
| adapter | 1 | 676,091 | 1,264,245 |
| vault | 9 | 19,306,448 | 929,120 |
| cash_receipt | 3 | 28,558,427 | 688,269 |
| reward | 9 | 1,076,469 | 195,239 |
| unlabelled | 6 | 337,306 | 183,148 |
| paired | 10 | 1,087,858 | 172,175 |
| underlying | 25 | 3,034,858 | 168,372 |
| plane | 6 | 128,212 | 135,407 |
| facade | 1 | 771,781 | 0 |

All swap cases retain three actual claims: deposit, share transfer and custody assignment. Baseline claims total about13.9M CPU; repeated vault balance calls are about4.3M. Aquarius dominates exclusive CPU (about62.8%); its liquidity deposit is the largest baseline nested operation. No measured evidence justifies removing ownership checkpoints, valuation checks, authentication, or adding transaction stages.

Stress cases cover4 existing holders plus a real pool trade of1% of initial reserves, and16 holders plus5%. Worst measured swap is229,779,425 instructions and22,629,185 memory bytes:42.56% CPU and46.05% memory headroom under captured ceilings. Both conserve22,732 received rewards in backing. These are bounded controlled states, not a production worst-case guarantee. The4-holder case collects position fees (~53.2M CPU) and makes another liquidity deposit (~63.8M); the16-holder case takes the different path that leaves one-sided funds idle and does not call `deposit_position`. Larger trade/state is not monotonically more expensive. More ticks/ranges, pool conditions, active positions, holders and archived-state cases remain necessary. No speculative production optimization or new stage split is included.

## Rewards and lifecycle

The real venue one-state claim check remains22,732 pending=22,732 claimed; holder payment22,731 plus1 rounding backing. Receipt/margin ownership changes conserve45,464=34,098 old owner+11,365 new owner+1 backing, custodians paid0. Allowance `approve`/`allowance`/`transfer_from` use temporary expiring approvals, exact spender auth, the same claim-first transfer routine, normal transfer/approval events, rollback on claim failure and a prohibition on delegated custodian exits. Tests cover expiration, failed claims and unauthorized spender; arbitrary share burning is not added and full SEP41 certification is not claimed.

Funded compiled settlement uses real nested authorization and fresh synthetic oracle prices; debt repayment and permitted seizure remain one atomic call. Borrower retains pre-liquidation historical rewards (43,487), while liquidator accrues subsequent rewards; exact scaled conservation is asserted. Repay-release preserves booked rewards through killed claims. Interrupted-open recovery first proves underfunded repayment rolls back debt, payer balance, shares and boundary; full repayment then returns shares, preserves43,487 booked rewards and quarantines43,488 late rewards without waiving the borrower. Native9-case funded evidence also includes seizure-failure rollback, but its host25 synthetic state is separately classified and not a substitute for every compiled adversarial case.

The waived-interval test intentionally proves the remaining gap:22,732 later rewards are quarantined, the waived owner gets0, and the original co-owner still gets0. Runtime boundary remains unproven. Current boundary storage does not retain a complete original co-owner weight ledger, including owners who subsequently exit; distributing by current balances would transfer old entitlements incorrectly. Finishing this requires verified interval attribution and immutable original-owner/weight evidence with exact conservation/rounding. The harness does not manufacture a reconciliation/redistribution success row.

`cancel_after_execute` remains missing. The existing `execute_open_position_v3` is an activation finalizer after swap; after activation, the position is live and pending cancellation is not the recovery route. This checkpoint preserves the required gate instead of silently renaming it to close/repay-release. Post-swap pending recovery, pre-swap cancellation and live close/repay-release are individually executed and labelled.

## Reproduce

Run from `peridot-contracts`. This uses the committed captured config; refreshing public settings creates a different capture and must not overwrite this evidence. Obtain the hash-pinned Aquarius bytes with the existing read-only fixture fetcher if needed. The runner refuses an existing output directory and uses only `stellar contract build` for WASM.

```sh
node scripts/fetch_cl_pool_fixture.cjs
CL_CAPTURED_CONFIG="$PWD/../docs/deployment-handoffs/evidence/cl-resource-reconciliation-20261102/network-config.json" \
AQUARIUS_CONCENTRATED_WASM="$PWD/target/cl-share-checkpoint/aquarius-pool.wasm" \
CL_CHECKPOINT_DIR="$PWD/target/cl-resource-reproduction" \
bash scripts/run_cl_share_checkpoint.sh
# Expected exit1: policy and missing-proof gates remain failed.

PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s scripts/tests -p test_cl_resource_profiles.py -v
python3 scripts/profile_cl_calls.py \
  target/cl-resource-reproduction/captured-network-diagnostic-calls.jsonl \
  --flow stress_4_holders_1pct_reserve_trade_swap_open
bash scripts/check_budget.sh \
  --local-report target/cl-resource-reproduction/captured-network-diagnostic-resources.jsonl \
  --profile target/cl-resource-reproduction/captured-network-diagnostic.json
```

For the precise checkpoint runs, `commands.txt` records environment paths and commands; final evidence files use the `*-archival` names. Both profile manifests have creation-machine absolute config paths as provenance. Recreate them using `cl_resource_profiles.py` on another checkout; do not edit their ceilings or hashes.

Machine summary: `peridot-contracts/config/cl-resource-checkpoint.json`. Artifact hashes accompany the evidence; all except the allowance-enabled vault remain unchanged from the preserved synthetic-admin checkpoint artifacts. No production identities change. CI is checked on the published commit separately; local Cargo/checker results are not represented as remote CI approval.
