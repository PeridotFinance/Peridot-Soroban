# Explicit restoration preparation and remaining execution blockers — 2026-11-03

**Decision: bounded RestoreFootprint preparation removes the measured cold-footprint barrier conditionally under captured limits. It does not make the preserved atomic lifecycle fit the unchanged internal policy.** Open/close still exceed100M CPU; liquidation, activation and pending recovery still exceed100 SDK entries. Keep Stage1 incomplete and PR75 draft. No production policy, financial stage, borrowing, maintenance-margin, oracle, Upshift or handoff00 change is adopted.

The removed blocker is specific: required archived code/instances/data no longer have to be restored inside one financial invocation. The experiment constructs measured native restore batches, projects their TTL-only effects, then freshly simulates and executes the original compiled financial root. Funded repayment and permitted seizure remain atomic. The next implementation decision is venue/accounting redesign if100M/100/50 is mandatory, or further qualification of the separately proposed Option1; no policy choice is made here.

## Scope and identity

Continuation of627e2e2; all earlier checkpoints, atomic baseline9cf43082, prior storage/TTL experiments, negative splitbb154ec8 and both resource profiles remain preserved. The new directory is `peridot-contracts/prototypes/cl-restoration-preparation/`. It combines the exact prior TTL, storage and **separate full-repayment** patches in a fresh build-specific WASM cache. Production source is untouched. WASMs are built only with `stellar contract build`; validation retains its Mainnet restriction. Tests explicitly label their borrowing/reserve parameters as test-only; deployment borrowing stays disabled.

Local host: Darwin arm64, Rust1.96.0, Stellar CLI26.1.0. Contract SDK25.3.1; compiled harness SDK/host/simulation29.0.1, actual protocol29. Numeric ceilings and all86 CPU/86 memory coefficients come from the unchanged ledger65270655 capture, SHA256 `4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`. These are controlled-state measurements using actual pinned Aquarius bytecode, not live-pool qualification.

Exact combined Mac WASMs: V3 `46c186e3bb333b929dabb3a4a4bec4376457081f2e1e404a467c8ada62f078c2` (92,246 bytes), receipt `fc1ac7d397428438c58c50ce8930907c5965587aac7cc0f9988025235dcde362` (88,329), lending controller `a55d5740007553d955c26e08efad4289b093c0d85c34a2b0e2af7262eaba0052` (63,411). All eight compiled artifacts plus Aquarius identity, generated Rust and lockfiles are pinned in the evidence.

**Prior artifact correction:** the older local candidate-v2 metadata names corrected receipt source, but a reused Cargo output retained the88,231-byte receipt (`292801…`). Its measurements prove only those exact bytes. They do not prove the corrected nonzero-cap renewal source. Fresh Linux CI and this independently rebuilt Mac candidate compile source `a215cb99…`; Linux receipt88,329 has separate hash `d36d3620732a2a6b2773cdc27a4e323b2db24d6620f06c086d5b1157a2637974`. No old evidence was overwritten or retroactively relabelled. Use the new fresh-cache builder for further qualification.

## Preparation, independent of execution

The official `soroban-simulation=29.0.1` restore API measures serialized entry bytes, footprint and rent. Each batch is limited to50 unique read-write keys and132,096 write bytes, also checking the selected profile's other applicable ceilings. It restores only the persistent dependency keys discovered by a complete controlled-state preflight. Missing archive bytes or an indivisible oversized entry fail. Every entry's data is unchanged; TTL becomes `ledger + captured minPersistentTtl - 1`, with minimum2,073,600 ledgers.

Both profiles validate **140 batches and30 complete preparation prefixes** (280 batches/60 prefixes total), with no prefix errors. Largest batch:50 keys/writes,121,860 disk/read-write bytes,4,684 unsigned envelope bytes. Disk reads stay below200,000 and write bytes below132,096. Native RestoreFootprint has no SDK invocation entry count. Its declared instruction field is0; native Core CPU/memory are **unmeasured**, not zero.

The pinned helper omits restored-key disk-entry fees that Core v29 counts. Its original quote is retained alongside a separate official-host fee recomputation with the actual RW count and serialized unsigned envelope. Code rent uses decoded module-memory cost plus serialized entry size, not WASM bytes alone. The independent checker recomputes XDR, key sets, fee components and rent; the source audit links the exact upstream code in [restore-notes](../../peridot-contracts/prototypes/cl-restoration-preparation/restore-notes.md).

After each batch, the snapshot is serialized/reloaded. Duplicate simulation returns an empty footprint; the partial-prefix financial attempt fails. Preparation can resume without borrower authorization because it changes TTL only. In the active-liquidation case an independently funded trader changes Aquarius state after preparation; fresh liquidation simulation and execution still completes under captured limits without borrower authorization. That trader's60.0518027 XLM unsigned fee estimate is reported separately, not charged to the operator's preparation journey.

**Application limit:** no Stellar Core binary is available locally. Native restore application, source-account sequence/balance/fee processing and actual installed TTLs are not executed. Financial execution is conditional on explicitly projected, validated TTL effects. SDK/official-host data and events agree after substitution of only the exact fresh synthetic external-authorization nonce pair; raw hashes, exact nonce credentials and both entries remain recorded. All other state is compared exactly. No controller/facade/vault authorization is mocked. Source/account signatures remain untested.

## Complete financial roots after preparation

Both unchanged profiles enforce finite SDK and host limits. `reset_unlimited` is not used for acceptance. The table uses completed captured executions; each column is an independent maximum across its named workload. Internal CPU failures are partial executions and are not substituted for complete costs. SDK entries include its read/write/disk-entry totals; unique keys are the union of actual serialized transaction footprint keys.

| Root | Host CPU | Host memory bytes | SDK entries | Unique keys | Writes | Internal result | Captured result |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| Swap-open | 227,912,081 | 22,133,291 | 172 | 126 | 48 | FAIL CPU + entries | PASS unsigned invocation |
| Activation | 63,352,770 | 11,388,217 | 158 | 119 | 40 | FAIL SDK/unique entries | PASS unsigned invocation |
| Swap-close | 189,698,460 | 18,998,407 | 164 | 123 | 42 | FAIL CPU + entries | PASS unsigned invocation |
| Funded liquidation | 67,310,552 | 12,573,342 | 170 | 122 | 50 | FAIL SDK/unique entries | PASS unsigned invocation |
| Pending funded recovery | 30,190,230 | 6,666,012 | 111 | 74 | 38 | FAIL SDK entries | PASS unsigned invocation |
| Full repayment | 15,487,886 | 4,202,515 | 74 | 63 | 12 | PASS unsigned invocation | PASS unsigned invocation |
| Debt-free release | 14,422,759 | 4,096,876 | 86 | 65 | 22 | PASS unsigned invocation | PASS unsigned invocation |

There are30 completed captured financial roots and8 internal successes (five full-repayment calls, including repeats, and three debt-free releases). All remaining22 internal roots fail selected limits. The maximum SDK CPU/memory are222,174,549 /22,242,145 bytes. No successful final host root needs further automatic restoration; successful SDK calls record zero disk entries. This does not establish Core handled-failure TTL/application parity.

Against the **unadopted** Option1 workload boundaries, independent unsigned maxima leave12,087,919 CPU below240M,4,601,400 memory bytes below26,843,545,68 SDK entries below240,114 unique keys below240 and30 writes below80. This is not qualification: signatures, Core application, elapsed preparation, more expensive price-cache branches and live state are missing. Liquidation is exactly at the unchanged internal50-write ceiling before additional real-account costs.

## Fees and latency

These are unsigned controlled-state estimates, including rent, resource components and100-stroop inclusion per transaction. No fee was paid. Each row is an **independent cold-root probe**, not a sequential lifecycle in which the same code should be restored again. Do not sum rows or nine independently cold liquidation probes as one shared-restoration batch for concurrent borrowers.

| Root | Required archived keys | Preparation transactions | Maximum preparation rent (XLM) | Maximum preparation fee (XLM) | Maximum prefix + financial fee (XLM) | Serial confirmation rounds if waiting each |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Swap-open | 92 | 6 | 568.3041913 | 568.3561142 | 629.8080167 | 7 |
| Activation | 90–95 | 6 | 539.2023504 | 539.2532856 | 599.8694829 | 7 |
| Swap-close | 100 | 6 | 568.5518751 | 568.6057257 | 629.8463604 | 7 |
| Funded liquidation | 83–90 | 6 | 538.5777152 | 538.6273144 | 539.2123172 | 7 |
| Pending funded recovery | 58 | 5 | 454.0667597 | 454.1045680 | 454.5905766 | 6 |
| Full repayment | 46–47 | 2 | 234.3366396 | 234.3598177 | 234.4108715 | 3 |
| Debt-free release | 45–46 | 2 | 234.3111791 | 234.3341392 | 234.4519681 | 3 |

The roughly538–569 XLM preparation estimates for liquidation/open/close are dominated by rent, not merely extra inclusion fees. Exact per-batch components and the helper/Core fee discrepancy are in the raw records. No network confirmation latency was measured; fixture time advances0 during preparation. Debt keeps accruing in reality, deadlines are not extended, and long interruption/re-expiry requires fresh discovery and simulation. No keeper-always-live assumption is used. Dependency discovery here uses the supplied controlled snapshot and its archived bytes; a live cold-RPC discovery/indexing implementation and any extra adaptive discovery rounds remain unproved. Incomplete-prefix simulations and empty duplicate simulations are not submitted transactions or charged fees.

## Remaining CPU decision

The **fresh combined** SDK call trees reconcile every exclusive instruction. Aged open with actual pool deposit uses223,150,467 SDK CPU: Peridot contracts50,558,731 exclusive, Aquarius162,817,228 exclusive, other dependencies9,774,508. Aquarius disjoint inclusive calls total167,831,028. Close uses183,245,406: Peridot36,504,273, Aquarius137,759,524, other dependencies8,981,609; Aquarius inclusive141,545,386. These are SDK attribution trees for the comparison invocation, not a decomposition of the separate post-preparation host totals.

All three open claim-first boundaries remain (deposit, transfer, custody assignment), with17,553,329 inclusive CPU across their claims. Fee collection plus actual position deposit costs118,185,789; close's fee collection plus withdrawal115,815,792. The earlier negative split remains negative. A later157M open without `deposit_position` cannot replace the full-deposit result. No helper provides a fresh budget inside one transaction.

If100M remains mandatory, removing all Peridot overhead still leaves Aquarius-exclusive open/close above it. A specific next dependency is a venue/ABI change providing cheaper fee-inclusive position valuation and liquidity mutation, while retaining each actual reward ownership boundary; exact fee-growth snapshots/settlement and combined venue operations need their own correctness/resource proof. Narrower bounded receipt/controller records must additionally address172/170 SDK entry totals. No saving is assumed and no such change is adopted. Further financial stages should not be pursued until mint, activation, close and borrower-independent recovery can all complete, including intervening trades. This checkpoint justifies explicit restoration preparation research, not another sequence of small TTL edits.

## Combined accounting and recovery evidence

Six compiled tests pass: three combined repayment/adversarial/liquidation tests, the stressed open/close test, eight independent active liquidations, and eight independent pending recoveries. Full names/assertions are in the [proof map](../../peridot-contracts/prototypes/cl-restoration-preparation/proof-map.md). Fifty offline checker/constructor/prefix tests pass. Seven intended repayment/authorization/closed-state negative cases reach their predicates and preserve complete ledger/TTL rollback; a setup panic is never counted as authorization rejection.

The combined full helper clears elapsed-interest debt using the payer-authorized maximum; exact maximum and surplus refund are checked, a repeat returns0, and authorized release succeeds during claim/pending-view outages. The active liquidator is distinct from borrowers and supplies only its own external authorization. After the intervening trade, the borrower's10,382 raw booked reward plus its fraction remains unchanged,132 prior historical records survive and the liquidator receives no earlier rewards. Actual reward-token balance equals received minus paid. Forced exits never waive rewards.

Exact scaled conservation uses `received*10^18 = (paid + pending + quarantine)*10^18 + fractions + rounding`. Final examples:

| Fixture | Received | Paid | Pending | Quarantine | Fractions (scaled) | Rounding (scaled) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Combined repayment/release |3,906,060|20,764|79,868|3,805,389|38,999,999,999,973,007,512|26,992,488|
| Eight liquidations |765,101|0|100,632|664,430|38,999,999,999,973,007,512|26,992,488|
| Eight pending recoveries |765,101|0|100,632|664,430|38,999,999,999,973,007,512|26,992,488|
| Stressed open/close |1,032,539|0|368,079|664,430|29,999,999,999,534,073,420|465,926,580|

Post-execution assertions inspect separate clones; no holder enumeration or settlement is moved out of the measured financial operation. Pending recovery here is owner-funded. Borrower-independent active liquidation is proved; third-party rescue for every debt-bearing pending state is not established by it. Primary freshness/issuer-pause/execution-price checks remain. A stale/unavailable primary price can still block forced liquidation; the secondary oracle remains deferred.

## Subsequent actual Core and completed head CI

[Handoff22](22-cl-option1-qualification-20261104.md) supplies actual controlled
Core29 native restoration/maintenance application and independent pending/stalled
recovery research. It does not retroactively upgrade this checkpoint's TTL
projections, owner-funded pending route or earlier compiled identities.

Completed **9bb51aa** CI is now [durably archived and replayed separately](evidence/cl-ci-9bb51aa-20261103/README.md): baseline41626834491 checker PASS/checkpoint FAIL;
storage41626834469 both profile jobs FAIL; restoration41626834559 six compiled
cases PASS/full checkpoint FAIL. All38 per-profile gate summaries and exact errors
were replayed;829 members and90 WASM files/20 distinct Linux identities are pinned.
Captured baseline's four gates pass while internal's four fail. Restoration's
captured post-preparation invocation passes but global proofs stay missing;
internal open/close journey integrity also fails where CPU rejection omits
complete financial metadata. Aggregate exit1 is not the verification criterion.
This CI precedes the new Option1 recovery ABI and supplies no execution claim for it.

## Gates, CI and reproducibility

Every assessed financial flow runs through unchanged `check_budget.sh`; native preparation has its explicit XDR/rent checker. All16 final per-profile financial/preparation reports replay exactly, not merely as aggregate exit1. Captured invocation and targeted post-preparation execution pass in all four runs. All lifecycle/transaction/checkpoint gates remain failed: each captured run has two lifecycle errors, one missing proof-to-call mapping, and44 or46 missing required labels. Internal invocation fails in every run. Internal open/close complete-journey integrity also fails because three CPU-rejected host calls have no complete fee/footprint evidence. The separate prefix-only verifier proves preparation feasibility without hiding those failures. Core application, signed authentication, live state and release stay false. All four wrappers correctly exit1 despite six functional tests passing.

Earlier compile failures, the zero-selected-test probe, v5 setup-path failure, interrupted v5 nonce-mismatch runs and v6 diagnostic are preserved separately. The nonce discrepancy was decoded, not silently normalized. Finalv7 uses the exact actor/credential-bound comparison and a fresh cache. The constructor rejects an incompatible overlay, feature or cache provenance. Dedicated draft-only CI covers shared dependency paths, runs both profiles, and uploads failed evidence without swallowing exit codes.

Completed **627e2e2** CI is now durably preserved in two separate archives:

- [Baseline run41613837844](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41613837844): checker PASS, checkpoint FAIL; captured four gates PASS, internal four FAIL with106 lifecycle/303 transaction errors. Archive SHA256 `cc53ab0679eaaa4f1cad756647c449412f6a483d90d99965dec9b7444d317bf5`, [index](evidence/cl-linux-ci-627e2e2-20261103/README.md).
- [Storage prototype run41613837952](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41613837952): both profile jobs FAIL. Captured baseline/candidate each4 tests pass,151 pairs match and59 TTL differences remain; missing lifecycle/transaction proofs and cold recovery fail. Internal baseline/candidate each0 pass/4 fail. Archive SHA256 `69d7c40f473caee457bd4f34d8bf0d66a82c4e6358af6a1d9b8da975ebe33849`, [index](evidence/cl-storage-ci-627e2e2-20261103/README.md). Separate Linux WASM identities and exact errors are preserved. Neither run executes this new restoration prototype.

[Commands](../../peridot-contracts/prototypes/cl-restoration-preparation/README.md), [machine assessment](evidence/cl-restoration-preparation-20261103/assessment.json), [durable raw evidence and verifier](evidence/cl-restoration-preparation-20261103/README.md). Use fresh paths. The main commands are the fresh `build.py`, `prepare.py`, `identify_build.py`, then `run.sh` for the four documented filters. `assess.py` reproduces resource/fee tables; `cpu-attribution.py` separately verifies the preserved older trees. Native preparation is checked via `restore-checker.py`; `prefix_summary.py` keeps restoration feasibility independent from financial failure. No signing or broadcast is performed by any command.

Before a later decision, run a supported Core29 application path with restored archive entries, fee-source balance/sequence handling and elapsed time, then measure the exact funded account authorization forms: source=payer and a distinct fee-source/payer with real nested authorization. Include serialized signed envelopes, signature verification, nonce/account footprint and fees under the same limits. Custom-account authorization needs separate bounded qualification; synthetic mock nonces do not establish it. No signing is authorized now. Live liquidity/capacity, stale-price recovery, complete Stage1 proof mappings and all required lifecycle branches remain gates.

## Reserve planning remains conditional

The original-USDC benchmark holds the originally deposited USDC. It differs from holding the post-conversion token basket. For a modeled50/50 full-range $1,000 position with USDC at$1 and PYUSD falling50%, the no-rebalance basket is$750 and the ideal LP is$707.106781: total shortfall$292.893219 consists of$250 underlying-depeg loss plus$42.893219 additional LP loss. At a75% PYUSD decline, total$500 comprises$375 depeg plus$125 additional LP loss. Unwind/recovery costs are added once, not twice. These are conditional mathematical valuations, not realized exit capacity or a principal guarantee.

The [reserve study](20-stellar-reserve-sizing-study-20261103.md) keeps $1k/$5k/$10k gross planning scenarios,1x first and2x separate, unbuffered/25%/50% contingencies and zero initial future-income credit. Full range and hypothetical wide range are not qualified on supplier returns or execution. The new XLM rent estimates inform an operating-cost input but do not establish costs at those exposures or a safe cap. Dedicated available seed assets, supplier withdrawal cash, operating/restore fee funding and liquidator repayment capital remain separate. Allocate each coverage cost once; ordinary deposits, future earnings and quarantined rewards supply no day-one backing.50% collected-income retention remains a replenishment scenario, not an approved production parameter. No production cap or reserve guarantee is approved.
