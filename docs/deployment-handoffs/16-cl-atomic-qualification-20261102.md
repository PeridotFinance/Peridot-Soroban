# Focused atomic qualification — 2026-11-02

**Recommendation: require a storage-access/restoration optimization before another Option1 qualification decision.** The unchanged atomic baseline exceeds the proposed240M CPU workload boundary. An isolated same-transaction receipt optimization recovers4.66M instructions, but restored liquidation still uses104 writes:24 over the80-write workload boundary and4 over the proposal's100-write ceiling. Optimized restored swap-open leaves only511,628 instructions before unmeasured real authorization. Do not adopt either the patch or the proposed policy from these results. They do not yet establish that a venue redesign is necessary under Option1.

Stage1 remains incomplete; PR75 remains draft. Existing100M/100 SDK entries/50 writes policy, captured-network diagnostic profile, transaction stages, all three claim-first boundaries, production borrowing disablement and maintenance-margin settings are unchanged. No signing, broadcasting, deployment, secrets, Upshift binding or handoff00 edits. The negative split remains preserved separately.

## Executed scope and identities

This is actual compiled V3, receipts, vault, facade, adapter and lending controller, with **actual Aquarius bytecode and controlled state**, plus compiled synthetic oracle/plane fixtures and SAC assets. It is not live PYUSD/USDC issuer-state or account-signature evidence. The exact atomic binaries were extracted from the local archive preserved at9cf4308; source hashes match all147 baseline pins. The candidate changes only a copied receipt-vault source and is built with `stellar contract build`. No production contract source changed.

SDK/host29.0.1, Rust1.96.0, Stellar CLI26.1.0, Darwin arm64. The existing read-only capture at ledger65,270,655, protocol29, SHA256`4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`, supplies the exact86 CPU/86 memory cost terms and ceilings. It is reused without a new network transaction. Synthetic network ID is retained; validation contracts' Mainnet rejection is intact.

Receipt baseline:87,197 bytes, SHA256`db995c2ee1614eb90660c8ab79b0780358ed91e0e686aa1ec1c95dc56cc11f0e`. Candidate:87,243 bytes, SHA256`707ef0dbba1a3ecbeb6ae130e1d5142507fc7118e5531985dff7d818242f5445`. Aquarius:90,057 bytes, SHA256`12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`. Every other artifact identity, source, generated exporter, lockfile, profile, command and raw result is in the [durable evidence index](evidence/cl-atomic-qualification-20261102/index.json). Linux CI binaries remain separately identified.

Setup uses captured limits, not unlimited meters. Assessed calls enforce the selected existing profile in both the SDK and fresh host. SDK host reconstruction preserves exact ledger-data/TTL/configuration hashes and mock nonce/address generators, discarding accumulated test objects only at completed transaction boundaries. It is a fixture correction, not a contract saving. Narrow external `mock_auths` remain synthetic; V3/facade/vault/receipt nested contract authorization executes. All totals include the measured root's claims, valuation, custody and post-cost reserve checks.

## Targeted questions and proof mapping

All names below are in `test::atomic_qualification::`; the source is `prototypes/cl-atomic-qualification/tests/combined.rs`. Four cases run on baseline/candidate, live/restored variants. The former peak is a separate existing case, not a broad suite rerun.

| Concrete test | Question and asserted result |
| --- | --- |
| `compiled_atomic_qualification_swap_close_64_owners_128_changes_8_unresolved_5pct_trade` | Combines64 owners,128 additional real ownership transfers, eight unresolved reward intervals, real aged debt/dynamic interest, configured reserves, a5% controlled pool trade and actual64-position borrower vector. Measures aged swap-open too. Completes begin/swap/activate/prepare-close/swap-close/finish-close and conserves rewards/quarantine.63 sibling positions are real begin-only locked-margin states, explicitly zero actual debt; they are not63 debt-bearing positions. |
| `compiled_atomic_qualification_8_independent_liquidations_repeated_both_call_outages` | Eight different borrowers, one distinct funded liquidator, accrued debt fully repaid atomically with permitted seizure. Both claim and pending view fail. Borrower authorization is absent; historical rewards survive with no waiver. |
| `compiled_atomic_qualification_8_independent_pending_recoveries_repeated_both_call_outages` | Eight different swapped-pending positions recover with real accrued debt while both upstream reward calls fail. Debt/custody clear; original entitlement and missing-boundary quarantine remain. |
| `compiled_atomic_qualification_8_independent_repay_release_and_exited_entitlements` | Full repayment, debt-free release, margin-to-spot, receipt withdrawal, principal exit and later booked-reward claim for eight exited holders. Full repayment uses **static zero interest**, not a new dynamic repayment proof. Both-call corruption is restored before principal withdrawal; that withdrawal uses Aquarius's actual claim-kill mode. |
| `test::stress_tests::compiled_stress_four_holders_shifted_pool` | Repeats only the known high-CPU baseline branch, with its original1% controlled trade and ownership configuration, to benchmark the same patch. This case is not the configured-reserve proof. |

The common fixture has407 checkpoints before the additional measured histories; it does not cap global holders or delete records. On-chain liquidation never enumerates the64 holders: conservation enumerates them only in test assertions on disposable snapshots. Pool `User` record corruption is explicit controlled fault injection against actual bytecode, not an observed live-pool outage. Test-only reserve is1,000bps with borrowing enabled solely in the fixture; margin25M raw/borrow25M raw and maintenance margin250000 remain harness settings, never deployment policy.

## Measurements against the proposal, not a changed gate

Option1 remains analysis-only: proposed ceiling300M CPU/32MiB/300 SDK entries/300 unique keys/100 writes;80% workload boundaries **240M/26,843,545 bytes/240/240/80**. Other existing checks remain. Independent column maxima need not occur in one transaction. SDK totals are memory-read+disk-read+write counts; unique transaction keys are the union of serialized RO/RW LedgerKeys. Restored counts below include actual auto-restoration in each disposable root, not merely configured expiry.

| Complete unsigned root / state | Baseline CPU | Candidate CPU | Tx writes, both | Unique tx keys, both |
| --- | ---: | ---: | ---: | ---: |
| Combined aged swap-open, live |242,307,129|237,646,433|48|135|
| Combined swap-open, restored |244,149,068|239,488,372|81|135|
| Activation, live / restored |91,214,714 /94,219,172|91,215,575 /94,220,033|40 /98|152|
| Swap-close, live / restored |196,369,375 /198,966,918|196,369,369 /198,966,912|42 /89|145|
| Funded liquidation, live / restored |102,125,283 /104,698,560|102,128,458 /104,701,735|50 /104|156|
| Former peak, live |236,693,999|232,137,949|45|127|

The unchanged baseline misses240M by2,307,129 live and4,149,068 restored. The candidate saves4,660,696 on the new peak, leaving2,353,567 live and **511,628 restored**. That last distance is0.213% of the workload boundary, not measured signed headroom. The optimized live sampled workloads fit the five numeric workload boundaries; the restored workloads do not.

| Remaining baseline root, restored | CPU | Modeled memory bytes | Unique keys | Writes |
| --- | ---: | ---: | ---: | ---: |
| Prepare-close |58,775,854|10,636,388|94|61|
| Finish-close |65,405,417|6,826,248|74|47|
| Pending funded recovery |38,923,545|6,818,186|86|58|
| Full repayment, static-interest fixture |21,895,147|3,307,232|68|40|
| Debt-free release |16,788,669|3,211,338|71|48|
| Margin-to-spot recovery |47,220,214|8,127,871|91|55|
| Receipt withdrawal |38,384,979|7,808,323|90|54|
| Principal emergency exit |135,154,795|10,722,352|51|31|

New combined SDK maxima:baseline237,214,100 CPU/23,021,209 memory/204 SDK entries/50 writes; candidate232,507,385 CPU/22,961,757 memory/204/50. The separate former-peak SDK memory remains24,514,601 baseline and24,456,289 candidate. These fit26,843,545 but do not establish worst-state memory. Earlier whole-baseline maxima213 SDK entries/158 unique keys still apply; the narrower new matrix does not supersede them. The receipt patch cannot change direct vault deposit/redeem bytes: preserved baseline166,914,792/139,966,854 CPU maxima remain relevant and below240M only for their measured states.

All five comparison dimensions are in the assessment; disk reads/bytes, write bytes, entry/code sizes, envelope, return/events and existing fee checks are retained in the raw checker reports. Recorded restored fee maxima include46,030,879 stroops for swap-open,29,249,742 activation,47,170,471 swap-close and22,992,479 liquidation. These are controlled-state unsigned estimates including recorded rent, **not complete signed/network fee budgets**. Handled-failure restoration TTL/rent remains an explicit gap. Do not add fee maxima from different roots. Evidence-record sums include probes and independently re-expired state, so they are not a sequential lifecycle price or latency estimate. No network latency was measured.

## What the isolated optimization establishes

The patch keeps the first full receipt keepalive pass and removes its immediate duplicate inside `withdraw_with_cash_policy` and `borrow_for_margin`. Fresh interest/model inputs are read at the same point. Public `update_interest` retains its original missing-underlying guard and both TTL loops. All claims, fee-inclusive mint valuation, token deltas, authorization, reserve finalization and atomic funded repayment/seizure remain.

Paired comparison: **151/151 live transactions,151/151 restored transactions and17/17 former-peak transactions** match returned values, events, authorization forest, touched before/after data, full logical contract data and every reported per-key TTL change. Only the two independently pinned receipt executable identities are normalized. The old JS union normalization error and its failed comparisons are preserved separately. Exact source review and differential checks are evidence of this sampled behavior, not general patch approval.

The patch saves about2.27M on receipt withdrawal; it does not materially improve liquidation or reduce unique keys/writes. Direct nonzero-elapsed `borrow_for_margin` accrual, deliberately near-expiry first-pass renewal, custom account reentry, public-wrapper edge/failure parity and broader interest/reserve combinations still need their specific compatibility proofs. Existing open accrues same-market interest during the earlier withdrawal and cannot prove the later helper's independent time-crossing branch.

## Required architecture work

The worst restored liquidation has104 RW+52 RO keys;85 RW keys restore. Restored roles are cash receipt25, share receipt25, lending controller13, reward/custody vault13, V3 eight and Aquarius one. Broad receipt core/borrow TTL sweeps and controller initialization reads are concrete targets. Removing a duplicate read/TTL pass does not remove a key already touched by the first pass.

Require **at least24 fewer touched/restored write keys** to reach80 in this measured liquidation, plus space for actual auth/nonce effects. Activation needs18 fewer, swap-close nine and swap-open one. A candidate should separate immutable/configuration reads from dynamic debt/cash/custody state, or use versioned compact configuration with measured compatibility migration. Preserve fresh valuation, reserve checks and every needed debt/reward record. Packing increases write bytes/rent/contention; no numeric savings are assumed. Migration and old-position reads must themselves be bounded and recoverable, including archived legacy records. Do not prune unresolved entitlement or force restoration into an unmeasured prerequisite to make the table fit.

The CPU patch is worth retaining as an isolated candidate, but additional measured CPU headroom is needed before signed qualification. Neither this patch nor selecting a different profile repairs the write failures. Under the unchanged100M/100/50 policy, the earlier venue/accounting ABI redesign dependency remains; this experiment does not offer a wrapper-only path to those limits.

## New-risk bounds and authorization decision

The [detailed workload and authorization plan](../../peridot-contracts/prototypes/cl-atomic-qualification/authorization-and-workloads.md) separates current enforcement, tested samples and missing admission controls. Current enforceable bounds include64 position IDs per borrower, compaction at most eight IDs,32 pair configurations, a direct two-asset route, explicit-index historical settlement and waiver batches1..4. Existing amount/slippage bounds do **not** bound initialized-tick traversal.64 beneficiaries/eight independent borrowers/eight intervals are measurements, not invented global caps.

Before admitting new risk: bind the exact venue/asset/version class; either measure the full allowed registry and path complexity or implement a reviewed bound checked before new-risk mutation. Require venue/accounting work whose bounded worst case remains inside the envelope despite public trades. A frontend-only quote rule cannot enforce that. History bounds must limit work per call, not total retained entitlement: unresolved intervals can accumulate during forced recovery, so an eight-interval admission limit cannot justify refusing a ninth recovery. Indexed settlement, durable exited beneficiaries and quarantine remain available; missing authentic boundary evidence never becomes fabricated attribution.

Qualification initially targets two separate auth classes: single-signature G-account source credentials, and a distinct G transaction source with real G-address actor authorization. Arbitrary multisig/contract wallets are not covered. Do not add a recovery allowlist that traps existing owners; funded liquidation must remain borrower-independent. The current C-address mocks and recording templates prove neither G-account class.

The assessment records exact peak test/root, contract, snapshot, host-function and unsigned-envelope hashes for each required root. The **later, separately authorized** experiment must regenerate the same economic states with synthetic G actors, pair source versus distinct-source address credentials, and execute a protocol29 transaction application layer plus the real host. Measure envelope/signature/threshold/sequence checks, address-auth signatures/nonces/expiry, additional account/trustline/nonce reads and writes, TTL/rent, serialization and all fees. Include replay, wrong caller/amount/recipient/nested tree/network/signature/expiry failures and complete business-state rollback. Source-auth host execution alone omits the transaction layer's signature check. No signatures or signing code were executed here.

## Conservation, recovery and remaining evidence

Every new compiled case passes exact scaled conservation at each checked interval. The final exited-holder case has received765101, paid83048, pending17584, quarantine664430, fractions38999999999973007512 and rounding26992488 at scale10^18. Thus `(paid+pending+quarantine)*scale + fractions + rounding == received*scale`; held reward tokens equal received minus paid. Margin, receipt and facade receive no beneficiary entitlement. Original booked rewards remain claimable after principal exit; unresolved later receipts stay quarantined. Forced liquidation never waives.

Missing evidence remains material: signed auth/transaction application, effective restored TTL for handled failed calls, code/instance archival, full32-pair serialization, dense/adverse tick paths and realistic live state, deeper per-beneficiary history/search bounds, nonzero-interest full repay/release behavior, and the optimization edge cases above. Existing `repay_for_margin` caps the requested amount against pre-accrual debt; the static-interest test does not resolve that gap. The full Stage1 lifecycle and release gates remain failed; no missing flow is replaced by a fabricated passing row.

The exporter correction is isolated: pinned host29 emits zero-size/no-value/no-TTL footprint-only records when a nested failed call is handled. Both old projections mistook those records for deletions after restoration. The repaired projection retains the initial logical value or absence, exports the raw classification proof, and changes no resource, fee, footprint, TTL or restoration-index calculation. The independently decoded proof must also validate this marker. Logical retention does not prove the network-installed restored TTL. Original failures, corrected results and exact gate replays remain distinct in the evidence.

## Reproduction and CI

Use [the prototype README](../../peridot-contracts/prototypes/cl-atomic-qualification/README.md) for exact baseline/candidate preparation, focused tests, restored replay, comparison and assessment commands. Every run calls `check_budget.sh`. Internal profile:zero of four combined tests complete; all four gates fail. Captured profile:four of four combined tests pass in each baseline/candidate live/restored variant, and both one-test former-peak comparisons pass; invocation passes but full lifecycle/transaction/checkpoint gates remain failed because the focused map omits46 required Stage1 labels. Parser/exporter correction failures are preserved, not relabelled as successful auth tests.

Local verification: **42 checker tests pass**, including ten real-XDR recording-projection regressions; Python/Node syntax and diff whitespace checks pass. Exact gate replay compares every field, missing-label list and error string for all seven runs, not just exit1. Live/restored combined captured runs retain two lifecycle errors and46 transaction errors for missing full Stage1 mapping. Internal retains18 lifecycle/62 transaction errors. Restored original reports had142 transaction errors; the audited decoder removes exactly96 placeholder-projection errors, leaving all46 missing-proof failures. All four gate booleans remain unchanged and release approval remains false. The raw pre-repair reports are preserved.

Earlier fixture failures are in a separate [negative-evidence archive](evidence/cl-atomic-qualification-20261102/failures/README.md): accumulated SDK objects, setup nonce replay after generator reset, first-run generated lockfile mutation, and exporter/JS-normalizer mismatches. The nonce panic is not an auth-rejection proof. Final measured tests use fresh exact-ledger hosts with preserved generators and `--locked`; contract meters remain finite. The exporter repair changes only evidence projection, and a paired replay verifies identical execution quantities, envelopes, fees, returns/events and reported TTL changes for all151 rows of each restored build.

Completed baseline CI [41492773441](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41492773441), head90111e2: **FAILURE**. Checker32 pass; checkpoint fails; artifact upload succeeds. Captured all four gates pass, internal all four fail with106 lifecycle/303 transaction errors. Captured compiled36 main+five archival tests pass; internal four pass/32 fail plus five archival failures. Native funded captured9 pass/internal5 pass4 fail; reward53 pass; default margin110 pass/two ignored. Its [separate durable archive](evidence/cl-linux-ci-90111e2-20261102/README.md) preserves artifact13167198650 before November16 expiry, exact errors/tests and ten Linux WASM identities. Full offline replay of that CI archive was not repeated. This baseline workflow executes neither isolated prototype. A workflow triggered by publishing this checkpoint is a separate result, never inferred from the completed run.
