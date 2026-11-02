# Resource decision memo — refreshed decision, 2026-11-02

**Recommendation: preserve the existing atomic baseline as the reference architecture and evaluate it against a separately proposed bounded internal policy. Do not adopt the split prototype or add stages.** This is an architecture recommendation, not permission to change limits. Stage 1 remains incomplete; PR #75 remains draft; borrowing stays disabled. If 100M/100 SDK entries/50 writes remains mandatory, the next feasibility dependency is a measured venue/accounting ABI redesign plus narrower receipt/controller key access, not more transaction splitting.

“Atomic baseline” means the existing V3 transaction stages with atomic conversion, fee-inclusive share mint and custody changes inside swap-open, and atomic funded repayment/seizure. It does not mean the entire open/close lifecycle is a single transaction. Its preserved source is `9cf43082bfb57334d57dbc58cfa90ccad4d98078`. The isolated prototype at `bb154ec8f03889aabfdb8dd5ea120f22c46a47f5` remains a **negative feasibility result**. All historical artifacts and checkpoints remain intact.

No active profile, ceiling, production contract, stage, claim-first boundary, maintenance-margin setting or shared handoff 00 changes in this update. No signing, broadcasting, deployment, secret access or Upshift binding. This memo reuses measured executions; it does not expand the contract test suite or substitute a different profile to make a release gate green.

## Refreshed network settings — read-only

Public RPC `https://rpc.lightsail.network/` returned Mainnet protocol 29 settings at ledger **65,270,655**. Capture SHA256: `4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`. The [new capture and analysis](evidence/cl-resource-decision-refresh-20261102/README.md) are separate from the execution capture at 65,264,920 (`a28fb64…4dcd`). All 13 setting XDR records were independently decoded and compared with their stored values. This read accessed network configuration only, not accounts or transaction artifacts.

| Dimension | Existing internal ceiling | Refreshed network ceiling | Interpretation |
| --- | ---: | ---: | --- |
| Instructions |100,000,000|400,000,000|Both SDK invocations and complete transaction execution are measured.|
| Modeled memory bytes |41,943,040|41,943,040|Host metering, not OS resident memory.|
| SDK entry total |100|No separate network SDK-total field|Supplemental SDK guard; never substitute unique keys.|
| Unique transaction footprint keys |100|400|Set union of serialized read-only/read-write LedgerKeys.|
| Write entries |50|200|Check invocation and serialized transaction counts separately.|
| Disk-read entries / bytes |100 /200,000|200 /200,000|Restored reads remain included.|
| Write bytes |132,096|132,096|Unchanged.|
| Events / envelope bytes |16,384 /132,096|16,384 /132,096|Unsigned templates are not signed-envelope proof.|
| Key / data / code bytes |250 /65,536 /131,072|250 /65,536 /131,072|Unchanged entry-size checks.|

All execution ceilings and all **86 CPU plus 86 memory cost-model terms** are unchanged. The only changed setting is the 30-sample live Soroban state-size window: mean 1,740,511,394→1,757,819,311 bytes; its last-modified ledger advances 65,264,896→65,270,592. The derived rent price remains at its floor of **1,000 stroops per rent KiB**. All other derived fee/rent inputs match. Recomputing the preserved successful transactions' fee estimates with these fresh price inputs gives identical results.

This permits comparison of the existing protocol 29 controlled-state quantities without repricing host 25 or claiming a new execution. It does not refresh venue liquidity, histories, account signatures, TTL state or live execution costs. The original execution hashes/gates remain bound to their original capture. If protocol or cost coefficients change, this analysis refuses reuse and requires matching-runtime measurements; it never relabels old runs as fresh network simulations.

## Evidence and origin of the existing limits


The first CL local checker introduced 100,000,000 instructions, 100 entry total and 50 writes at `2d92318d9af7d60e8dde7986edfa4b8a07f233d4`, `peridot-contracts/scripts/check_local_budget.py:18–21`. It also checked 40 MiB memory, 200,000 read bytes and 132,096 write bytes. Its comment required missing coverage to fail. The commit and checker do not document a quantitative risk, fee or headroom rationale for those three numbers. The prior `check_budget.sh` was an unsigned RPC reporting helper, not this numeric local gate. No numeric 100M/100/50 requirement was found in authoritative shared 00; it requires complete resource proofs and claim-first accounting.

`64093aa3c0f77ab510ebf110138195405bb0d430` added disk reads to the entry total while retaining the caps. `14ef4d813b476a0c06bd9a9f9792834b29b3f03a` named the existing numbers `internal-policy`, added `captured-network-diagnostic`, and applied `min(policy, captured network)` to every policy field. See `scripts/cl_resource_profiles.py:7–25,27–47` and `docs/deployment-handoffs/09-cl-resource-reconciliation-20261102.md:14–16` at that commit. Their documented continuing purpose is a conservative, separately enforced gate while reconciling runtime/network settings. That is a defensible description of current intent, not evidence that the original author derived a specific safety margin.

The numerical resemblance is to a **mixture of defaults**, not a single verified network snapshot:

| Source inspected | Instructions | SDK entry total | Writes | Meaning |
| --- | ---: | ---: | ---: | --- |
| host 25.0.1 `src/budget/limits.rs:43–46` | 100,000,000 | — | — | Host budget default; source explicitly directs embedders to customize to network configuration |
| SDK 25.3.1 `src/testutils/cost_estimate.rs:133–150` | 600,000,000 | 100 | 50 | Hardcoded invocation limits; source says these are not dynamically fetched |
| SDK 29.0.1 same file, lines 138–158 | 400,000,000 | 400 | 200 | Source describes a hardcoded snapshot dated 2026-07-10 |
| Captured ledger 65,264,920, protocol 29 | 400,000,000 | 400 unique transaction keys | 200 | Hash-pinned chain settings used by the current evidence |

Thus 100M is **not** SDK 25.3.1's invocation CPU ceiling. It matches the host default, while 100/50 match that SDK's entry/write defaults. Copying these defaults is a plausible inference, not documented provenance. SDK comments alone do not independently establish historical network state. The earlier capture at ledger 65,264,375 had the same four headline network ceilings; this analysis uses the newer complete capture and its full cost model, SHA256 `a28fb64bb85c74f8f0d1d4bd7a0daf413ada6f603647c64f556a7905af554dcd`.

Primary SDK/host files were inspected locally and byte-compared with their cached crate archives. Crate SHA256 identities: SDK 25.3.1 `4502f2e018f238a4c5d3212d7d20ea6abcdc6e58babd63b642b693739db30fd1`; SDK 29.0.1 `6a3b97a76c88f413859bb670903b63af54d7ddb03303a5bd585fee8b9f2a3d8f`; host 25.0.1 `43af5d53c57bc2f546e122adc0b1cca6f93942c718977379aa19ddd04f06fcec`. These can be reproduced by reading `src/testutils/cost_estimate.rs` or `src/budget/limits.rs` inside the respective crates, without fetching current documentation or treating it as historical evidence.

`budget().reset_unlimited()` removes one host execution ceiling; it does not disable SDK invocation-result checks. Preserved acceptance evidence uses explicit selected limits and protocol 29 host execution with the captured coefficients. Profiling/shadow quantities are separately labelled and do not replace acceptance checks. Repricing host 25 counters with protocol 29 fees or setting a ledger-version field would not establish protocol 29 execution.

## Preserved atomic baseline: current comparison population

This refresh verifies the complete hashes/member sets of the [local 9cf baseline](evidence/cl-decisions-reserve-20261102/README.md) and its [separate completed Linux capture](evidence/cl-linux-ci-9cf4308-20261102/README.md), then reads their raw measurements. It supersedes the earlier ac63255 comparison for this decision without deleting that historical analysis or its archives. Separate WASM identities remain in each index; Linux and Mac binaries are not interchangeable.

Each 9cf archive contains 1,652 SDK rows and 2,741 unsigned transaction records:2,655 successes,86 expected rejections and 726 successful restored variants. Both captured profiles pass their four existing checkpoint gates; both internal profiles fail all four, with 106 lifecycle and 303 transaction errors. The captured checkpoint is still not release approval: signed authorization and live-state evidence are absent.

Table entries are independent maxima across the two preserved builds. SDK proof-label and unsigned ABI-entrypoint populations are separate; column maxima need not co-occur. In particular, the `transfer` transaction population contains more than the SDK custody proof alone. Funded transaction maxima include adversarial/restoration variations.

| Flow | SDK CPU | SDK memory bytes | SDK entries | SDK writes | Unsigned CPU | Unique tx keys | Tx writes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| deposit_underlying | 166,914,792 | 14,418,955 | 70 | 22 | 166,680,464 | 49 | 22 |
| redeem | 139,752,097 | 10,640,393 | 65 | 19 | 139,966,854 | 47 | 19 |
| receipt_custody_ownership_change | 34,389,169 | 6,595,879 | 124 | 25 | 37,442,916 | 100 | 25 |
| swap_open_position_v3 | 235,456,689 | 24,514,601 | 171 | 45 | 236,694,863 | 128 | 46 |
| activate_open_position_v3 | 81,229,338 | 13,326,755 | 182 | 40 | 85,934,085 | 144 | 40 |
| prepare_close_position_v3 | 53,765,162 | 11,093,755 | 120 | 28 | 55,499,702 | 93 | 28 |
| swap_close | 184,220,661 | 19,901,018 | 175 | 41 | 188,846,736 | 135 | 41 |
| finish_close | 58,855,997 | 6,824,744 | 97 | 30 | 58,696,467 | 67 | 30 |
| funded_liquidation | 100,066,565 | 14,591,435 | 213 | 56 | 112,983,374 | 158 | 57 |
| repay_release | 13,741,496 | 3,099,376 | 88 | 18 | 15,286,226 | 71 | 18 |
| history_scale_64_owner_exit | 138,682,384 | 14,125,779 | 75 | 24 | 132,860,630 | 53 | 28 |
| history_scale_64_owner_settle | 5,037,430 | 2,210,162 | 16 | 6 | 4,286,577 | 12 | 11 |
| history_scale_8_funded_fault_exit | 98,041,525 | 15,080,366 | 197 | 50 | 112,983,374 | 158 | 57 |

SDK totals are memory-read + disk-read + write entries; a key can appear in more than one category. Unique transaction keys are the union of the serialized footprint. Neither repeated getter counts nor SDK totals are unique keys. Activation's 182 SDK entries versus 144 transaction keys and prepare-close's 120 versus 93 show why both checks must remain.

Restored-success maxima include 171,514,133 CPU,18,758,943 memory bytes,157 unique keys,57 writes,12 disk reads and 2,580 disk-read bytes. Across all successful transactions, maxima include 15,092 unsigned envelope bytes,532 authorization-template bytes and 19,236 write bytes. The maximum total fee is 22,845,643 stroops; rent's independent maximum is 22,686,852. Other independent fee maxima are 162,160 non-refundable,15,147 refundable and 100 inclusion. Do not add component maxima; they need not belong to one transaction and rent is already in total. These are pinned controlled-state TTL assumptions, not a production funding budget.

## Negative split result: what it establishes

[Handoff 15](15-cl-split-transaction-feasibility-20261102.md) and its two archives remain the authoritative experiment. The main 21 tests and supplemental 2 recovery tests are separate runs: all 23 pass captured limits; internal 8 pass/15 fail on resources. Its full Stage 1 lifecycle/transaction-map gates remain failed. Three original claim-first boundaries remain, with an additional actual escrow beneficial-assignment claim. No failure is reclassified as approval here.

| Complete prototype transaction | SDK CPU | SDK entries | Internal obstacle |
| --- | ---: | ---: | --- |
| Conversion |28,933,011|35|Fits measured numeric limits.|
| Fee preparation |79,965,545|49|Fits measured numeric limits.|
| Mint |131,864,126|93|CPU;136.0–136.1M with another owner.|
| Activation |81,460,987|181|SDK entries;145 unique transaction keys also exceed 100.|
| Prepare close |54,334,925|120|SDK entries despite 93 unique transaction keys.|
| Swap close |187,766,920|173|CPU/entries; unsigned 191,473,540 CPU and 135 unique keys.|
| Recovery from minted shares |36,781,465|127|Actual enforced SDK rejection, not just comparison.|
| Recovery after delivery |37,682,204|123|Actual enforced SDK rejection, not just comparison.|

The actual Aquarius deposit subtree is 63.694M; the 147.936M atomic-baseline venue figure is a sum of different disjoint calls. Neither is proof that every split fails. What fails is this complete candidate after adding fee/state validation, fresh valuation, accounting, real authorization and required claims. Its late recovery is not usable under internal policy even though source-level repayment/release exists. Its pre-delivery debt phases also lack borrower-independent liquidation, and its exact fee/config fence can be invalidated repeatedly by public pool trades. Owner-funded recovery and repeated re-preparation do not repair those safety/liveness gaps. If intervening trades require fee collection again at final mint, its measured approximately 53.23M collection subtree and additional accounting must be included again. The current fence avoids that repeated cost by rejecting; it does not establish completion under active trading. A nonzero-fee final valuation/implicit-settlement alternative needs a new proof against actual venue behavior.

Its 11-transaction captured open/close journey costs 22,984,457 stroops including rent. The 45-second fixture clock is not network latency. Its deposit/redeem assessment uses unchanged entrypoints inside the larger prototype binary, not byte-identical baseline A/B costs. New escrow restoration and worst-state combinations remain unmeasured. No further stage is adopted or proposed for implementation from this result.

## Option 1: atomic baseline with a separately proposed bounded policy

Retain the previously discussed **analysis-only** candidate:300M instructions,300 SDK entries, a separate 300 unique transaction keys,100 writes and 32 MiB memory. This creates no runnable profile. It deliberately tightens memory relative to the existing 40 MiB while proposing higher CPU/key/write caps. Keep every other current limit and check, including disk reads, bytes, return/events, entry/code sizes, rent/fees, lifecycle proofs and missing-evidence failures. Any future approved policy must be clamped to then-current verified network settings.

The candidate's rationale is two reserves: below current network capacity, and a further 20% reserve inside the candidate for the supported workload. Its 80% workload boundary must include real authorization, restoration, stress and reverting paths—not just unsigned normal examples. That 20% is a proposed risk allowance, not an empirically established guarantee or a changed gate.

| Dimension | Proposed ceiling | 80% workload boundary | Observed atomic maximum | Remaining to boundary | Reserve below refreshed network |
| --- | ---: | ---: | ---: | ---: | --- |
| CPU | 300,000,000 | 240,000,000 | 236,694,863 | 3,305,137 | 25% below 400M |
| Modeled memory bytes | 33,554,432 | 26,843,545 | 24,514,601 | 2,328,944 | 20% below 40 MiB |
| SDK entries | 300 | 240 | 213 | 27 | Separate SDK guard; no network SDK-total field |
| Unique transaction keys | 300 | 240 | 158 | 82 | 25% below 400 |
| Writes | 100 | 80 | 57 | 23 | 50% below 200 |

The critical distance is **3,305,137 CPU**, only about 1.38% of the 240M workload boundary. Memory has 2,328,944 bytes left. A claim that the baseline “fits 300M” is insufficient: signature verification, account/contract-wallet authorization, source/nonce/trustline entries, signed serialization and combined stresses are unmeasured. No positive headroom for those costs is established. Even a small verified atomic-path optimization may matter here, but no saving is credited in advance and no increase beyond this proposal is implied.

This option has the smaller architectural change: it preserves existing custody/debt transitions and avoids the prototype's new unliquidatable escrow states. It does not remove the need for complete reserve, reward, auth, liquidation or recovery proofs, and it does not promote validation-only CL/funded code into a production build.

**Missing evidence / decision gate:** define enforceable admitted workloads and supported authorization forms; measure signed end-to-end costs in an explicitly authorized environment; combine the highest relevant history/tick/range/borrower/restoration stresses; establish a funded rent/fee envelope and unavailable live-state validation; then decide whether a bounded policy is acceptable. Current no-signing restrictions remain, so signed execution is an outstanding gate rather than work performed now. If the observed combined workload exceeds the proposed 80% boundary, optimize or narrow a defensible workload; do not silently consume the reserve or select captured-network diagnostics as release policy.

Existing domains include 4/16-holder and 1%/5%-reserve-trade paths;64 owners with 383 checkpoints,128 added ownership changes,8 outages and 8 exited owners;8 positions belonging to one borrower with 16 missing-boundary faults; and emission/range/rounding/partial-exit cases. They do not establish the Cartesian product or many independent borrowers. State-size monotonicity is not proved. Admission must constrain new risk without preventing recovery of existing debt or pruning unresolved historical entitlements.

## Option 2: retain 100M /100 SDK entries /50 writes

This option preserves current policy but requires a credible implementation dependency before another product split. The following full-stack CPU reductions merely reach 100M, with no extra headroom. “Required CPU” takes the worse of SDK and unsigned execution; conditional venue sums apply only if the existing call set remains in one transaction.

| Atomic path | Required CPU maximum | Reduction to 100M | Conditional unchanged venue sum |
| --- | ---: | ---: | ---: |
| deposit_underlying |166,914,792|66,914,792 (40.1%)|134,354,879|
| redeem |139,966,854|39,966,854 (28.6%)|126,765,769|
| swap_open_position_v3 |236,694,863|136,694,863 (57.8%)|147,935,537|
| swap_close |188,846,736|88,846,736 (47.0%)|130,983,173|

A 1x launch still needs deposit and redemption, so excluding margin borrowing does not solve their CPU failures. Wrapper cleanup is not a credible explanation for reductions of this size. The old hypothetical removal of every repeated getter saved only about 7.03M on atomic swap-open and some of those reads straddle mutations. The split's removable repeated balance reads are about 0.13M; omitting its additional escrow assignment claim could save roughly 4.9M only with a newly proven atomic beneficial-mint design, still far below its mint deficit. None of the original three ownership-boundary claims may be removed or replaced by an earlier claim.

| Specific dependency | Required behavior | Evidence still needed |
| --- | --- | --- |
| Venue fee/position primitive | A reviewed fused fee-realization + position-mutation path avoids repeated fee-growth/tick/position work. It must separately identify incumbent fees, entrant contribution/exit principal, actual receipts/spends and liquidity changes so the vault can preserve fee-inclusive pricing in the same transaction. | Actual compiled implementation/ABI and end-to-end benchmark. Current claim-all-fees (~52.9M) plus withdrawal (~62.2M) already exceed 100M before surrounding work. No savings assumed. If pinned Aquarius cannot expose the needed operation, new reviewed venue ABI/bytecode is an explicit external dependency. |
| Vault accounting and fresh valuation | Price incumbent entitlement before new weight/burn; reconcile actual balances and partial-owner fee allocation; preserve fractional carry, range crossings and slippage. Batch views or invocation-local snapshots only where dependencies remain valid. | Exact scaled conservation, quote/claim equivalence at actual boundaries, newcomer/exited-owner/outage/waiver cases, adversarial valuation and mutation tests. The current withdrawal estimate includes all owed fees; adding the fee view again double-counts. |
| Receipt/controller/custody read sets | Reduce genuinely touched keys, not merely repeated getter calls. Versioned compact immutable configuration or narrower state loading are candidate mechanisms. | Entry-size/write-amplification/rent/concurrency and migration/restoration measurements; actual auth/valuation and post-cost reserve checks retained. Funded settlement reaches 213 SDK entries/158 unique keys/57 tx writes; activation 182/144; prepare-close120/93; receipt transfer 124/100. CPU-only venue work cannot solve these. |
| Historical accounting and forced recovery | Bounded per-beneficiary processing, durable exited entitlements and quarantine; liquidation must never enumerate all holders or waive rewards. | Complete recovery/forced-settlement fit under 100M/100/50 after changes, including outages and archives. Keep full funded repayment and permitted seizure atomic; no pruning or governance-dependent forced exit. |

Getter batching alone cannot be credited with reducing unique footprints. Packing records can reduce keys while increasing write bytes, rent and contention. All such costs remain measured inside the complete transaction. A combined fee/position return is not trusted merely because it reports favorable numbers: actual token/liquidity deltas and authenticated venue behavior must support the accounting.

**Missing evidence / decision gate:** actual feasibility of the reviewed venue ABI plus receipt key redesign, dependency control/upgrade/migration compatibility, and complete lifecycle/recovery measurements under unchanged caps with documented headroom. These changes are research dependencies, not an implementation plan authorized by this memo. If that work is unavailable, this option remains blocked; further splitting is not a substitute for proving it.

## Admission criteria before any further staged candidate

The present split stays a negative result. Before another staged implementation is considered, first provide a credible, measured design for **mint, activation, close and every recovery route**, alongside deposit/redeem and the full prefix, within policy. Passing conversion/preparation or constructing later phases under captured limits is not that demonstration.

1. **Complete resource path:** each required root—including retries, failures, restoration and supported authorization—must fit. Address the measured 131.9–136.1M mint,181-entry activation,120-entry prepare-close,187.8M swap-close and 127/123-entry recovery obstacles explicitly. Begin at 100 entries has zero observed entry headroom. Do not move required claims/valuation outside measurement.
2. **Borrower-independent liquidation in every debt-bearing intermediate state:** a third party must be able to fund debt reduction and obtain only freshly valued permitted collateral, returning surplus, atomically with repayment. Specify interest, custody, beneficiary, deadline, nonce/replay and pause semantics for every state. Neither borrower signature/external cash nor governance attribution can be the sole safety route. Healthy expiry alone does not authorize seizure or debt forgiveness.
3. **Completion despite intervening trades:** the exact fence is a safe rejection mechanism, not a progress guarantee. Reprepare-with-the-same-deadline can be invalidated repeatedly and charges more fees. A candidate needs fresh fee-inclusive final settlement that tolerates admissible intervening trades, or another proven bounded fallback that completes safely within policy. It cannot require a quiet public pool, lock other users' trades, weaken pricing, retry indefinitely or drop a later claim-first boundary. Pathological price/slippage moves may still require safe cancellation/liquidation; they must not strand debt.
4. **Ownership and operating proofs:** preserve beneficial ownership at mint/delivery/margin custody, all three actual claim-first boundaries, exited beneficiaries and unresolved quarantine. Specify phase-specific pause/expiry/recovery behavior and funded archive retention. No pruning to obtain bounded costs. Baseline restoration evidence does not cover new escrow state.

A no-debt-until-final-transition design might remove some intermediate liquidation obligations, but it has no measured solution here and still needs final mint/activation/close/recovery to fit. It is an architectural alternative to analyze only after these admission requirements, not another stage authorized now.

## Concrete next decision

Prefer the atomic baseline for the next resource decision because it already executes the required controlled-state lifecycle with the exact target runtime and avoids new escrow liveness/liquidation obligations. The next focused work, if separately authorized, is to quantify the missing signed/stressed envelope and determine whether measured same-transaction optimization can cover those missing costs while preserving proposed headroom. No broad rerun is warranted merely because this memo changed.

If 100M/100/50 is a firm product constraint, prioritize a venue ABI feasibility specification and receipt key-access plan with measurable targets from the table above. Do not promise a wrapper-only fix or start another staged candidate first. Neither option is presently release-ready; the choice is whether to fund constrained atomic-path work for a separately considered policy, or accept the larger venue/accounting dependency needed to keep today's caps.

## CI is separate from the prototype and the policy decision

Completed Linux baseline run [41473099204](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41473099204) at 9cf4308: checker-tests PASS 32; checkpoint-gates FAIL; upload PASS. Captured all four gates PASS, internal all four FAIL with 106 lifecycle/303 transaction errors. Captured 36 main plus 5 archival cases pass; internal 4 main pass/32 fail plus 5 archival failures. Native reward 53 and default margin 110 pass;2 default ignores remain. Its ten Linux WASM identities and exact gates/errors were durably preserved and replayed separately. It did not execute the isolated prototype.

Completed baseline run [41484260698](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41484260698) at `bb154ec8f03889aabfdb8dd5ea120f22c46a47f5` also concluded **FAILURE**: checker tests **32 PASS**, checkpoint job **FAIL**, artifact upload **PASS**. Its [fresh durable archive](evidence/cl-linux-ci-bb154ec-20261102/README.md) is separate from every previous Linux capture. Captured invocation/lifecycle/transaction/checkpoint gates all **PASS**; internal gates all **FAIL**, with 31 invocation error rows, 106 lifecycle errors and 303 transaction errors. Captured main tests pass 36/36 and archival reruns 5/5; internal main tests pass 4 and fail 32, with five archival failures. Captured native funded tests pass 9/9; internal pass 5 and fail 4. Shared native reward tests pass 53; default margin tests pass 110 with two ignored.

Artifact `13163838118` was preserved before its November 16, 17:25:37 UTC expiry. All 75 original members plus 22 source members, exact profile gates/errors, named test outcomes and ten independently hashed Linux WASMs are retained. All ten WASM identities match the previous Linux baseline; local Mac and prototype identities remain separate. ZIP digest/CRC, member hashes, source and log bindings, per-profile gates/errors and completed test counts were verified. **Full offline checker/XDR replay was not performed for this newest capture**; the earlier capture's replay is separate evidence. Its exact identity is registered for later reproducible replay.

This completed CI executed the baseline runner, not `prototypes/cl-split-transactions/run.sh`. It supplies no CI execution claim for the isolated prototype and does not approve either resource policy. Publishing this documentation checkpoint may start another baseline run; it does not change the completed result above.

## Reproduction without contract reruns

From `peridot-contracts`, use a fresh directory and the pinned Node dependencies:

```sh
mkdir -p target/resource-decision-refresh-fresh
node scripts/capture_cl_network_config.cjs target/resource-decision-refresh-fresh/network-config.json
python3 scripts/refresh_cl_resource_decision.py \
  --capture target/resource-decision-refresh-fresh/network-config.json \
  --output target/resource-decision-refresh-fresh/analysis.json
```

The capture performs only `getNetwork` and `getLedgerEntries` against a fixed public RPC; no account/key/transaction path is used. The analysis verifies all members of both 9cf archives and both negative-prototype archives, decodes raw setting XDR, compares every ceiling and cost coefficient, checks the saved per-profile gates/errors, requires measured flows, reconciles call trees and recomputes old-state fees using refreshed inputs. It fails on missing or mismatched evidence; it emits no active profile and does not alter the existing release gate. If runtime/cost/pricing changes invalidate reuse, preserve that result and request the specifically needed measurement rather than silently repricing or widening limits.

For exact reproduction of this memo's snapshot, replace `--capture` with the committed refresh `network-config.json`. Existing full `check_budget.sh` replays remain linked from their separate evidence READMEs. The earlier `analyze_cl_decision.py` and ac63255 data remain available as historical analysis; they are not silently overwritten with 9cf measurements.
