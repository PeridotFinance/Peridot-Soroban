# Stellar storage/restoration decision — 2026-11-03

Later evidence: [handoff21](21-cl-restoration-preparation-20261103.md) measures explicit restoration preparation with the combined full-repayment candidate. It also identifies that the older local candidate-v2 retained the88,231-byte receipt despite corrected source intent. The figures below remain evidence for their exact preserved WASM; they do not retrospectively prove the corrected receipt source. Fresh Linux and Mac88,329-byte builds are separately identified in handoff21. Both627e2e2 CI runs have completed with failed qualification gates; their final artifacts are now separately archived.


Stage 1 remains incomplete and PR #75 remains draft. **Do not adopt this candidate or Option 1.** Narrowing storage access produces measured savings, but restored liquidation still exceeds the proposed write workload, and restoring the required code exceeds the captured network disk-read ceiling. Further qualification requires an architecture decision, not more small TTL changes.

The atomic baseline `9cf43082`, prior TTL candidate, negative split `bb154ec8`, checkpoints `c80a1da3` and `11980783`, both resource profiles, transaction artifacts and shared handoff 00 are preserved. Production contract sources, limits, stages, caps and configuration are unchanged. Borrowing remains disabled; production maintenance margin remains unset. No signing, broadcasting, deployment, secrets or Upshift binding occurred.

## What this checkpoint removes—and does not remove

The storage prototype removes entire unrelated key accesses from the measured call trees: 34 unique keys /22 restored writes in liquidation, 33/21 in activation, 22/14 in swap-close and9/5 in swap-open. Accounting, events and external authorization forests match the baseline in **151/151 independently compared complete data-restored records**. Fifty-nine records have intentional maintenance/TTL differences; these are reported separately, not hidden in accounting normalization.

A separate behavioral prototype removes the demonstrated post-accrual repayment residual: the exact original V3 leaves219 raw units of debt after a maximum27,500,000 payment request against25,000,219 post-accrual debt. Its actual payment is25,000,000; debt-free release correctly rejects. The additive prototype routes an explicit full-repayment request through the already-existing receipt `repay_full_for_margin`, pays current debt, refunds excess and permits the existing separately authorized debt-free release. This is a new isolated ABI, not a production correction already adopted.

The resource, signed-authorization, live-state, economics and stale-price liquidation blockers remain. These results do not authorize deployment or borrowing activation.

## Measurements and unchanged boundaries

Contracts resolve SDK25.3.1 and are built only with `stellar contract build`; controlled compiled execution uses SDK/host29.0.1, protocol29, and the exact86 CPU/86 memory cost terms captured at ledger65270655. Network-config SHA256: `4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`.

Internal policy remains100M instructions,41,943,040 memory bytes,100 SDK entries/unique keys and50 writes, with every other check preserved. The captured diagnostic profile remains400M/41,943,040/400/200. Acceptance meters are finite. The unadopted Option1 workload comparison is240M/26,843,545/240 SDK entries/240 unique keys/80 writes; its proposed ceilings are not installed as a profile.

The matrix retains64 holders,128 additional ownership changes,407 initial history checkpoints, eight independent borrowers, repeated claim-plus-pending-view outages, exited beneficiaries, dynamic interest where applicable, a5% pool trade and a64-position vector. These are controlled test configurations, not approved caps or production maintenance margin. Range and economics qualification under the newer full/wide product direction remain separate.

Completed unsigned host invocations and serialized transaction evidence with archived **persistent data**, code/instances live; full transaction application remains unproven. Each column is an independent maximum, not one joint worst transaction:

| Root | Baseline CPU | Prior TTL CPU | Storage CPU | CPU saved vs baseline | Baseline → candidate writes | Distance to80 writes |
|---|---:|---:|---:|---:|---:|---:|
| Funded liquidation |104,698,560|104,701,735|67,731,074|36,967,486|104 →82|**2 over**|
| Activation |94,219,172|94,220,033|64,880,718|29,338,454|98 →77|3 spare|
| Swap-close |198,966,918|198,966,912|191,235,141|7,731,777|89 →75|5 spare|
| Swap-open |244,149,068|239,488,372|229,258,928|14,890,140|81 →76|4 spare|

Swap-open has10,741,072 instructions (4.48%) of unsigned headroom below240M, versus511,628 in the prior TTL candidate. That is useful evidence, **not qualification**. Liquidation still misses the write workload before additional authorization costs. The original100M policy still fails: open/close CPU exceeds100M and SDK entry totals exceed100. No new internal-policy acceptance pass is claimed; the dedicated CI runs both unchanged profiles.

| Root | Transaction memory | Unique keys | Restored data keys | SDK entry total (live invocation) | SDK memory | Unsigned envelope bytes | Auth template bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| Funded liquidation |12,702,531|122|63|170|14,365,993|11,936|532|
| Activation |11,895,934|119|66|157|11,820,475|11,192|192|
| Swap-close |19,271,651|123|57|164|19,178,107|11,664|208|
| Swap-open |22,328,679|126|50|172|22,780,496|11,736|188|

SDK entry totals are the SDK invocation sum of memory reads, disk reads and writes. They are not the deduplicated transaction footprint. Memory/unique-key distances to the proposed workload are respectively: liquidation14,141,014/118; activation14,947,611/121; close7,571,894/117; open4,514,866/114. SDK-entry headroom is70/83/76/68. Existing prior stress measurements, unmeasured configurations, real signatures and live state are not superseded by these selected maxima.

| Root | Disk-read bytes | Write bytes | Return+event bytes | Max rent stroops | Max total fee stroops |
|---|---:|---:|---:|---:|---:|
| Funded liquidation |9,768|14,396|2,068|18,268,784|18,480,840|
| Activation |13,032|19,076|1,800|24,700,526|24,903,980|
| Swap-close |19,860|27,760|4,016|43,955,763|44,203,498|
| Swap-open |18,484|28,884|6,204|44,787,507|45,047,928|

Raw records retain inclusion, non-refundable, refundable and rent components, exact XDR, source state, authorization templates, code identities and per-entry checks. Do not sum these independent maxima into a lifecycle fee or infer a keeper cadence from fixture execution time. Signed fees and network latency remain unmeasured.

## Exactly what stopped being touched

The accompanying per-key JSON and CSV enumerate contract/address, complete key, purpose, RO/RW status, restoration and reason for every key in the four maximum-write roots. Same-prestate differentials identify the eliminated keys; merely deleting repeated TTL calls would not produce these reductions.

In the worst liquidation, the share receipt avoids ten restored sweep-only entries: `AccumulatedInterest`, `Admin`, `AdminFeeScaled`, `BorrowCap`, `CollateralFactorScaled`, `InitialExchangeRate`, `ReserveFactorScaled`, `SupplyCap`, `TotalDeposited`, `YearlyRateScaled`. The cash receipt avoids seven: `AccumulatedInterest`, `Admin`, `CollateralFactorScaled`, `InitialExchangeRate`, `Peridottroller`, `SupplyCap`, `YearlyRateScaled`. The lending controller avoids five: `Admin`, `CloseFactorScaled`, `LiquidationFeeScaled`, `LiquidationIncentiveScaled`, `SupportedMarkets`. Twelve additional absent/optional RO probes disappear. These are conclusions for this exact call tree, not permission to remove those fields from other paths.

Initialization/underlying identity, applicable controller authorization, debt/index/principal, live interest, reserve and cap enforcement, fresh valuation, custody and reward checks remain. Conditional debt/factor/strategy reads are retained where required. Review found and corrected a missing near-expiry renewal for nonzero-cap `TotalBorrowPrincipal` before measurement. No schema migration, packed configuration or beneficiary-history packing was introduced. Existing fields may now archive sooner when unrelated operations no longer renew them; required on-path renewal and this intentional maintenance difference are tested and documented.

## Code and instance archival is a separate hard failure

The selected archival set is vault, both receipts, V3, Aquarius and lending controller. No keeper pre-restores it. Code/instances are expired in the disposable prestate before the official host invocation. Other dependencies remaining live do not make this a global worst-case cold-state claim.

All four target roots fail complete cold-code execution. Baseline failed attempts include liquidation224.15M CPU/41.75MB and254,004 disk bytes; swap-open224.94M/41.94MB and440,472 bytes; swap-close201.09M/41.70MB and437,944 bytes. These are **partial failed-call quantities**, not completion costs. Activation can fail inside the host without a resource record; that gate remains failed. Some other cold roots also cannot encode the computed fee in the envelope'su32 fee field. Missing quantities/fees are preserved as failures, not replaced by estimates.

The completed data-restored traces independently establish a stronger lower bound: all four roots require the same five serialized code entries—Aquarius90,160 bytes, receipt88,332, share vault85,852, lending controller63,512 and V391,760—**419,616 bytes**, before instances, data, accounts or signatures. Both receipts share one code entry. This exceeds the captured200,000-byte disk-read ceiling by219,616. This is an actual required transaction-key set plus measured cold failures, not a WASM-size-only qualification gate.

Host29 explicitly charges parsing of archived modules; live snapshot code follows its supported cache-hit simulation path. Removing unrelated data reads cannot remove these necessary code dependencies. Effective installed restoration TTL/rent for recording-mode handled-failure placeholders still requires independent enforcing/full-transaction evidence; retaining their logical values does not prove their effective TTL.

## Accounting, recovery and compatibility evidence

The four storage fixture families cover activation, complete close, eight independent funded liquidations, eight pending recoveries, static-interest repayment/release, receipt exits, in-kind principal exits and exited-owner claims. All151 paired data-restored records match returns, events, authorization and complete normalized logical contract state. Only reviewed executable identities are normalized; maintenance TTL differences remain explicit. Required swap-open traces retain **three Aquarius reward claims** plus fee collection.

Eleven native compatibility tests pass: direct nonzero accrual through withdrawal and margin borrowing, public/helper parity, near-expiry first-pass renewal, missing optional state, actual post-accrual rollback, nonzero-cap full repayment while borrowing is disabled, and custom-account `__check_auth` reentry rejected with `Context/InvalidAction`. The post-accrual rollback proof observes a failed-call100,000,000 interest event before checking exact state/TTL rollback. These are SDK25 native fixtures; compiled protocol29 tests and real signatures are separate.

The separate repayment candidate has three independently completed compiled tests; the exact original baseline has one independently completed regression test. Each invocation budget passes. Eight aged debts25,000,225..25,000,252 clear, with exact maxima/refunds, repeat payment0, both upstream reward calls failing, authorized release and beneficiary claims preserved. Seven adversarial conditions prove the intended rejection stages and full ledger/TTL rollback. SDK wrappers are distinguished from the underlying diagnostic error; a setup or unrelated authorization failure is not credited as the intended rejection.

Exact final dynamic reward conservation at scale10^18:

`4,389,282 = 83,048 paid + 17,584 pending + 4,288,611 quarantine + 38.999999999973007512 fractional + 0.000000000026992488 rounding`.

No forced waiver, custodian reward capture, newcomer redistribution or entitlement pruning is introduced. The old static-interest case remains labelled as such. The additive full-repayment API is not adopted, and combined storage+repayment, Closing-state qualification, signed authorization and live-state measurements remain outstanding.

## Verification failures retained and CI

All flows run `check_budget.sh`. Targeted data runs pass invocation checks but retain missing global Stage1 mappings and failed lifecycle/transaction/checkpoint gates. Raw cold runs retain resource/capture failures. The dedicated prototype workflow propagates every missing/failed gate, validates per-profile summaries/errors and completed test counts, pins separate WASM identities, and always uploads failed evidence. Seven helper checks pass; local syntax checks pass. It does not turn an incomplete checkpoint into CI approval.

Preserved fixture/evidence failures include native observation-order/auth-context mistakes, repayment SDK wrapper/panic-string assertions, a stale same-name Cargo executable with zero selected tests, and cold-host errors initially suppressing otherwise completed companion records. Content-specific harness identities fix the Cargo issue. Independent live/data/cold invocations now retain failures separately. Three focused fixture reruns repair the missing host captures. The extractor adds only authentic live-baseline archive metadata; resources, fees, events, authorization and raw original records remain unchanged. A 14-row audit removes exactly14 missing-baseline-hash errors while retaining the other46 transaction/11 lifecycle errors and cold failure. No synthetic passing row was introduced.

Completed **prior head1198078 CI** is separately preserved in [its durable index](evidence/cl-linux-ci-1198078-20261103/README.md): checker42PASS; baseline checkpointFAIL. Captured profile four gatesPASS/zero errors, internal profile fourFAIL/106 lifecycle and303 transaction errors. Captured compiled36+5 archival tests pass; internal4PASS32FAIL+5 archival failures. Ten Linux WASM identities are preserved separately. This CI did not execute the storage or repayment prototype. The new dedicated prototype workflow is additional coverage, not a replacement for that historical result.

## Decision and next architecture gate

Stop this storage experiment here. It establishes useful CPU/data-key savings but **does not establish a credible complete recovery envelope**. Two more liquidation writes are the minimum data-only reduction before authorization, other configured branches and code/instance restoration. Do not raise limits or silently assume permanent code keepalive.

Under an eventual separately approved Option1 policy, bounded versioned receipt/configuration and V3 position records are a concrete next design to assess. Any migration must measure archived legacy first access, interruption/retry and mixed-version recovery; it cannot prune unresolved claims or defer mandatory work to an unmeasured prerequisite. Packing alone does not solve the419,616-byte code dependency.

With the existing100M/100/50 policy, venue/accounting execution or a new completely specified transaction design is still required. A restoration-only split would not solve the remaining229.26M live/data swap-open CPU. A helper wrapper earns no assumed saving: benchmark its complete lifecycle, including repeated required claims, valuation, authorization and restoration. If code restoration is separated, its full costs, funding, permissionless retries and recovery must be explicit and measured; it is not implemented or approved here. If single-call cold recovery is retained, the required executable set must shrink below the network byte ceiling. Neither route permits splitting funded debt repayment from collateral seizure.

Before another staged candidate, establish a credible path through mint, activation, close and recovery under policy, including borrower-independent liquidation in every debt-bearing state and completion despite intervening trades. Preserve the negative split result. Existing-position recovery cannot be rejected because history grew, and exited/unresolved entitlements cannot be pruned to meet a budget.

The latest product direction is recorded in [handoff18](18-stellar-product-decisions-20261103.md): defer secondary-oracle work, keep the primary checks and stale-price liquidation blocker explicit, compare full/simple-wide supplier outcomes, separately account for seeded loss reserves, accept qualified in-kind exits and separate all four funding purposes. Range width and reserve capital do not cure the resource/valuation failures. The new reserve study uses research sizes only; no production cap is approved.

## Reproduction and remaining proof inventory

See [prototype README](../../peridot-contracts/prototypes/cl-storage-qualification/README.md), [storage review](../../peridot-contracts/prototypes/cl-storage-qualification/storage-review.md), [repayment review](../../peridot-contracts/prototypes/cl-storage-qualification/repayment-review.md) and the [durable evidence index](evidence/cl-storage-qualification-20261103/README.md). Main resource inputs are `cl-storage-assessment.json`, per-keyCSV/JSON, four differential reports, two cold-code lower-bound reports and `cl-repayment-results.json`.

Build the isolated baseline and layered old-TTL+storage variants, run `add_companions.py`, assign `identify_build.py` identities, then use `run_variant.sh` with the unchanged selected profile. Extract live/data records using `extract.py`; selected-source filters retain exact provenance and never claim excluded transaction coverage. `compare.cjs` optionally takes the exact test-family filter. Native tests use only the isolated workspace. The separate repayment variant adds only `repayment.patch` with the validation feature and its unique harness identity.

Remaining failed/unproven gates include complete cold execution, the80-write proposed liquidation workload, the active100M/100/50 policy, signed authorization/envelope costs, target live state and adverse venue branches beyond the controlled matrix, nonzero-cap compiled resource combinations, effective handled-failure restoration TTL/rent, combined behavioral/storage qualification, supported full/wide economics and capacity, trustworthy primary pricing during liquidation, funded-liquidator capital and reserve/cap decisions. The next checkpoint must remove a named blocker, not substitute a new profile for a failed gate.
