# Conditional XLM operating-cost study

Stage1 remains incomplete. This offline study changes no resource policy, contract, cap or production configuration. It reuses finalv7 controlled-state evidence, preserves failed qualification gates and adds no execution or signature proof. All figures are XLM converted from exact raw stroops; no fiat price is assumed.

## Warm roots versus one cold preparation

Warm is the original checkpoint invocation with live state. Post-preparation is a different invocation after projecting minimum restored TTL. It may pay additional rent to extend that TTL again. Pairing is checked by exact host-function hash and operation order. State and any controlled intervening trade still differ: the fee delta is not claimed to isolate restoration alone. These columns must not be conflated.

| Fixture / operation | Samples | Warm fee range XLM | One-cold preparation range XLM | Immediate post-preparation range XLM | One-cold total range XLM |
|---|---:|---:|---:|---:|---:|
| liquidations / liquidate_funded_v3 | 8 | 0.5243332–0.7163784 | 538.3687178–538.6273144 | 0.5850009–0.7657148 | 539.1344326–539.2123172 |
| pending / recover_pending_open_funded | 8 | 0.4479964–0.4626568 | 454.0498801–454.1045680 | 0.4859923–0.5006582 | 454.5505383–454.5905766 |
| open-close / swap_open_position_v3 | 2 | 0.3587815–0.3611633 | 568.3561142–568.3561142 | 61.4492656–61.4519025 | 629.8053798–629.8080167 |
| open-close / activate_open_position_v3 | 2 | 0.2393522–0.3946449 | 539.1081231–539.2532856 | 60.6161973–60.7601580 | 599.8682811–599.8694829 |
| open-close / swap_close_position_v3 | 1 | 0.1327674–0.1327674 | 568.6057257–568.6057257 | 61.2406347–61.2406347 | 629.8463604–629.8463604 |
| combined / liquidate_funded_v3 | 1 | 0.7164194–0.7164194 | 538.3687178–538.3687178 | 0.7657535–0.7657535 | 539.1344713–539.1344713 |
| combined / repay_full_margin_position_v3 | 5 | 0.0117314–0.0130561 | 234.3221153–234.3598177 | 0.0501132–0.0510538 | 234.3722285–234.4108715 |
| combined / release_debt_free_position_v3 | 3 | 0.0794469–0.1055704 | 234.3077720–234.3341392 | 0.1178289–0.1439525 | 234.4517245–234.4519681 |

One-cold rows are independent root probes, not successive operations in a single restored lifecycle. Summing every cold open/activation/close or every borrower cold probe would repeatedly charge shared code and mutable dependencies. The actual joined lifecycle cost is not measured. Preparation simulation does not apply Core transactions; its projected TTL and unsigned fees are conditional evidence only.

## Eight borrowers: overlap is not permission to reuse stale mutable state

| Fixture | Key occurrences | Unique code / instance / data | Shared code / instance / data | Shared differing instance / data |
|---|---:|---|---|---|
| liquidations | 713 | 8 / 11 / 189 | 8 / 11 / 54 | 2 / 16 |
| pending | 464 | 13 / 15 / 170 | 5 / 7 / 26 | 2 / 8 |

The per-key JSON retains exact key XDR, entry hashes, observed versions, modeled rent sizes and borrower-probe ordinals. All repeated code entries have identical bytes and a content-addressed code key. Their restoration can be reused only while that same code remains live and required. Instance and data keys are mutable: even identical observed bytes do not establish a safe joint snapshot, and differing bytes explicitly disprove it. This study grants no mutable-state savings. Do not construct a merged snapshot by picking an arbitrary version of a shared key.

Synthetic-account caveat: native authorizer markers may occur in every old financial fixture prefix, including open/close; this is not limited to the two eight-borrower groups analyzed below. The SDK snapshot contains native external-authorizer markers encoded as86-byte ASCII `test_contract_for_contract_id_<address>`, not deployable WASM. Their hashes and declared external-authorizer addresses are independently matched here. Each such code entry contributes7.5670556XLM of modeled minimum-TTL payload rent. Liquidation reuses one marker; pending recovery has eight different borrower markers. Measured controlled-state totals retain these costs, but a production G-account would not use these markers. Real account/sequence/signature costs are unmeasured, so this study does not silently subtract the markers and advertise a production price. Maintenance estimates below exclude native markers and still include other compiled controlled-state fixtures.

liquidations: the 8 unique code entries have 431,456 serialized bytes and 9,635,585 modeled rent-size bytes. One minimum-TTL code payload rent is 535.3102781XLM, comprising 527.7432225XLM for 7 non-native code entries and 7.5670556XLM for 1 native authorizer markers. Avoiding repeated non-native identical-code restoration across these eight probes could avoid 3694.2025575XLM of code payload rent under the stated live-code assumption. This is unexecuted arithmetic, not a measured eight-borrower recovery saving; TTL writes, envelope fees and mutable costs are not deducted.
pending: the 13 unique code entries have 365,656 serialized bytes and 9,080,642 modeled rent-size bytes. One minimum-TTL code payload rent is 504.4801117XLM, comprising 443.9436669XLM for 5 non-native code entries and 60.5364448XLM for 8 native authorizer markers. Avoiding repeated non-native identical-code restoration across these eight probes could avoid 3107.6056683XLM of code payload rent under the stated live-code assumption. This is unexecuted arithmetic, not a measured eight-borrower recovery saving; TTL writes, envelope fees and mutable costs are not deducted.

## Proactive maintenance estimate, not a measured operation

The pinned host29 fee formula uses observed rent-size bytes, captured rent rate1000 stroops/KB, persistent denominator1215 and the code discount factor3. TTL metadata writes are1250 stroops/entry plus437 stroops/KB for48 bytes/entry. Code rent size includes the host's modeled module size; raw WASM bytes are not substituted. Estimate only the incremental ledger extension of unchanged code already live, with no size growth or re-upload.

| Dependency set | Added ledgers | Rent + TTL-write component XLM | Complete operation fee |
|---|---:|---:|---|
| liquidations code | 17,280 | 4.3987501 | UNMEASURED |
| liquidations code | 518,400 | 131.9366953 | UNMEASURED |
| liquidations code | 2,073,600 | 527.7441119 | UNMEASURED |
| pending code | 17,280 | 3.7001662 | UNMEASURED |
| pending code | 518,400 | 110.9865522 | UNMEASURED |
| pending code | 2,073,600 | 443.9443022 | UNMEASURED |

At an assumed5-second ledger cadence only,17,280/518,400/2,073,600 ledgers correspond to1/30/120 days; no actual monthly schedule or latency is measured. Extending too frequently repeatedly pays transaction/TTL overhead. Maintenance does not make long-duration rent free: it may avoid emergency restoration, code parsing and latency, but still pays for additional byte-ledgers. A real unsigned ExtendFootprintTTL simulation, exact envelope/batches, source-account reserve and signatures are needed before choosing a cadence or monthly allowance.

## Conditional operating-wallet scenarios

These are fee-planning arithmetic, not safe operating caps. One-cold-plus-seven-warm assumes the prepared dependencies stay live and subsequent calls have costs no worse than the recorded warm maximum. That combined sequence is unexecuted, mutable dependencies and new owners may require extra restoration, and account minimum balance, fee escalation, retries and real signatures are excluded.

| Eight-borrower scenario | Liquidations XLM | Pending recoveries XLM |
|---|---:|---:|
| Observed eight warm root fees | 4.3867279 | 3.5987106 |
| Eight times observed warm maximum | 5.7310272 | 3.7012544 |
| One cold + seven warm maxima (unexecuted) | 544.2269660 | 457.8291742 |
| Same +25% fee contingency (unexecuted) | 680.2837075 | 572.2864678 |

Economic recommendation: investigate shared-code TTL maintenance and a measured sequential eight-borrower recovery before funding every position as independently cold. Keep a separately earmarked emergency XLM allowance for one actual cold incident; the above scenario is a starting estimate with explicit exclusions, not an approved required balance. Refresh fees and snapshot immediately before any eventual submission. Do not substitute speculative shared-state savings for complete recovery capability.

XLM operating/restore funds are distinct from supplier withdrawal cash, dedicated loss reserves and the liquidator's underlying debt-repayment capital. No dollar conversion, loss-seed credit, reward/quarantine appropriation or subscription is included. A cheaper operation does not cure price/authorization/resource failures or establish that a full-range/wide pool is profitable. Failed included transactions also spend fees; their rates/frequency and a sustainable keeper cadence remain unqualified.

Reproduce: `python3 prototypes/cl-option1-qualification/cost-study.py`; focused checks: `python3 prototypes/cl-option1-qualification/cost-study.py --self-test`. The script fails on changed source hashes, unmatched calls, duplicated keys or unreconciled recorded fees. It rewrites only its sibling JSON/Markdown. Fee formulas trace to pinned `soroban-env-host29.0.1/src/fees.rs`, already used by `scripts/cl_transaction_evidence.py`; no current-network fee claim is made.
