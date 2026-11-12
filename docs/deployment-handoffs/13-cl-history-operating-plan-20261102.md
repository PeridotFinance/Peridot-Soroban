# Historical reward retention and attribution operating plan

Design checkpoint, 2026-11-02. Stage 1 remains incomplete; PR #75 remains draft.
This plan changes no contract, limit, transaction stage, or shared handoff 00.
It authorizes no signing, broadcasting, deployment, key access, Upshift binding,
production maintenance-margin configuration, or borrowing activation.

The contract review below applies to the historical-accounting implementation at
`ac6325589a0095084c2df641d9b743860b4795f8`. Resource numbers are explicitly the
preserved **local controlled-state** evidence in
`evidence/cl-feasibility-history-reserve-20261102/execution-summary.json.xz`,
relative to this handoff directory. They are not substitutes for the separately
identified Linux CI archive or live-state evidence. The Stellar Soroban skill
informed the storage, authorization, and archival review; repository code and
captured settings determine the actual behavior.

## 1. Operating invariant and present capabilities

Every historical beneficiary survives an exit. A seized receipt or share changes
future beneficial ownership; it does not transfer or waive the borrower's already
booked or unresolved rewards. Voluntary waiver, original entitlement, subsequent
accrual, verified-but-unsettled backing, and unresolved quarantine remain separate
logical categories even where the implementation holds their token backing in
the same physical bucket. Rewards remain outside principal NAV.

All three existing claim-first ownership boundaries remain. An outage that blocks
a normal ownership change must not turn into an administrative requirement for a
forced exit. Funded repayment and permitted seizure remain atomic. Historical
attribution runs separately and must never become a precondition for liquidation,
full repayment/release, or the contract's permitted pending recovery.

| Capability | Implemented behavior | Operating limitation / work still needed |
| --- | --- | --- |
| Beneficiary weights | `reward_history::record_weight` appends a revision and effective weight, including zero on exit. Effective weight includes assigned receipt/margin custody, excludes the custodian's physical balance and unassigned donated dust. | An off-chain inventory must retain all addresses ever seen, not just current share holders. |
| Bounded settlement | `settle_reward_segment(holder,id,checkpoint)` verifies a supplied historical checkpoint and its next bracket; `prepare_reward_segment(id,limit)` processes 1–4 waiver records per call. | The index must supply the correct checkpoint. No liquidation should enumerate this inventory. Preparation/settlement workload can grow across many transactions. |
| Missing readings | Segments retain revision, supply, owned supply, boundary readings, completeness, verification class, and evidence digest. A missing reading invalidates the adjacent attribution boundary. | Later cumulative rewards cannot reconstruct an earlier missing boundary. No elapsed-time estimate may fill it. |
| Attribution | Segment proposal/execution requires the configured administrator, matching amount/digest, and at least 86,400 seconds. Governed attribution remains `governed`, not `observed_verified`. | The digest is only a commitment. Contracts do not fetch or validate the evidence document, reviewer independence, or its economic correctness. |
| Replay protection | Per-segment/per-holder settled markers prevent duplicate credit. Booked reward accounts remain claimable after exit. | Retain markers and beneficiary discovery records after payment; a zero current share balance is not permission to erase history. |
| Events | Share transfer and other business events exist; segment governance emits `reward_attribution_proposed` and `reward_attribution_governed`. | There is no complete reward/weight/custody event stream. Legacy direct-attribution calls emit neither of those segment events. Event-only indexing is insufficient. |
| Views | Weight, interval, segment, boundary, totals, and holder-count entrypoints exist. | Global revision/segment/boundary counters are private instance keys. Historical getters extend TTL; `reward_account` settles bookkeeping. A submitted getter is not necessarily a read-only operation. |
| Archival | History and entitlements are persistent, with TTL renewal on access. No unresolved-entitlement pruning is implemented. | A durable key inventory, restore planner, funding monitor, backups, and gap-aware indexer are still to be implemented and rehearsed. |

Relevant implementations are `contracts/cl-share-vault/src/reward_history.rs`,
`rewards.rs`, `custody.rs`, `storage.rs`, `events.rs`, and the public ABI in
`contract.rs:1844–1877` / `2006–2045`. The enablement of the pinned-pool verified
view is validation-network guarded. This document does not make that validation
mechanism a production-approved venue attestation.

## 2. Evidence captured at each actual ownership boundary

The collector must start before the first deposit and retain the deployment,
initial state, upgrades, and custody registration history. For every committed
ownership change, persist a boundary bundle containing:

1. **Chain identity:** network-passphrase hash, ledger sequence/hash/previous hash,
   close timestamp, transaction hash and application order, operation index,
   contract IDs, WASM hashes for V3, receipt, facade, vault and Aquarius, target
   protocol, and the exact relevant network/cost-model configuration. Store a
   content hash and provenance for every source artifact.
2. **Ownership identity:** caller and authorization tree classification, original
   owner, actual beneficial owner, custodian/receipt market, position identifier,
   recipient, amount, before/after custody allocation, physical share balances,
   total supply, unassigned custody, effective weight, history revision, and the
   predecessor/successor weight checkpoint indices. Preserve mappings when a
   position closes or its current allocation becomes zero.
3. **Accrual evidence:** call order and boundary ordinal within the transaction;
   pending-view result or exact failure classification; upstream claim result;
   reward-token balances before/after claim and actual received delta; reward
   index and holder account before/after; interval/segment/boundary IDs and their
   full serialized records; pool emission/range/tick inputs needed for the
   boundary calculation. `None`, failure, and zero are distinct values.
4. **Disposition:** forced or voluntary exit, explicit waiver authorization where
   applicable, the waiver's interval and through-segment scope, original versus
   redistribution credit, unresolved remainder, and actual reward payouts. A
   forced exit always records `waived=false` irrespective of operator preference.
5. **Outcome:** committed success or rejection. Failed/rolled-back calls are
   diagnostic evidence, not new owners, successful claims, or settled markers.
   Retain the rejection and demonstrate unchanged effective contract state.

Receipt deposit/withdrawal, receipt transfer, margin custody assignment/reassignment,
funded seizure, repayment release, emergency redemption, and ordinary direct
share transfer all need their actual nested boundaries, not one approximate
timestamp for the outer transaction. Approvals/allowance changes do not themselves
move ownership; consumption through `transfer_from`/other applicable hooks must
link to the same ownership evidence as a direct move.

Use committed transaction results/state-change metadata and a complete pre-state
to construct the journal. Public business events are discovery aids. When normal
claim-first calls do not persist each intermediate reading, final ledger state
alone may not expose all intratransaction reads. A version-pinned deterministic
replay of the **actual committed invocation with its actual pre-state** is needed
to recover that call ordering. Validate its return, changes, and events against
the chain, and archive the replay runtime/configuration. Do not mistake a
controlled-state test, later simulation, or client-reported pending value for an
authenticated historical boundary. If inputs or runtime cannot be authenticated,
mark the record incomplete; do not assert an automatic verification.

Observational collection uses raw ledger data and disposable unsigned simulation
where appropriate. It must not submit a getter simply to observe state. It must
not ingest local key files, secrets, or pending signed envelopes. Retain public
confirmed metadata and unsigned invocation descriptions needed for replay. Any
future account-signature execution proof remains a separate release gate.

## 3. Durable index, replay, and failure gaps

Implement the following schema in an off-chain index. These are proposed index
keys, not new contract setters or a claim that a service already exists.

| Index | Primary identity and required retained links |
| --- | --- |
| Ledger / transaction | Network + ledger sequence + hash; network + transaction hash, application order, operation/call/event ordinal, committed result, raw source digest. |
| Holder history | Vault + beneficiary + checkpoint index; revision, weight, raw `HistoryWeight` key/value, predecessor/successor, custody/position origin. |
| Interval / segment / boundary | Vault + their respective monotonically assigned IDs; full raw records, revision/supply/readings, trust class, raw evidence references, interval membership. |
| Waiver | Vault + interval + beneficiary; immutable journal of authorized through-segment updates and their source calls; corresponding `HistoryWaiver` / `HistoryWaiverIndex` keys. |
| Custody | Vault + custodian + beneficiary + revision, with receipt and position IDs; before/after changes, including deleted zero-current-allocation records. |
| Settlement | Vault + segment + beneficiary; checkpoint, original credit, redistribution credit, scaled residue, `HistorySettled` marker and committed transaction. |
| Governance | Vault + segment/boundary + proposal transaction + digest; publication locations, evidence version, amount, execute-after, supersession, execution. |
| Ledger-key inventory | Network + serialized LedgerKey hash; exact serialized key, owner/interval/segment links, durability, current state version, live-until ledger, last observed/extended/restored ledger. |

Inventory includes instance keys/counters and all persistent `HistoryCount`,
`HistoryWeight`, `HistorySegment`, `HistoryInterval`, `HistoryWaiver`,
`HistoryWaiverIndex`, `HistorySettled`, `HistoryProposal`, `Holder`, `Boundary`,
`Bucket`, `Evidence`, `Proposal`, shares, and relevant custody keys. Discover keys
from complete transaction/state history, not by guessing active holders. Exited
beneficiaries remain addressable even if their current custody keys were deleted.
Never recreate an archived record from an application default.

Store raw artifacts content-addressed by SHA-256 in two independently administered
durable copies, with a manifest linking all chunks and code/configuration hashes.
The database is a rebuildable projection; raw evidence plus reproducible decoders
is the source for rebuilding it. Backups need corruption checks and an actual
restore/rebuild rehearsal. A GitHub Actions artifact's expiry is not a retention
policy. Do not configure a new paid storage service or credentials as part of
this checkpoint.

Maintain separate contiguous watermarks for fetched ledger data, verified ledger
hash chain, decoded transactions, reconciled ownership/reward state, and complete
key inventory/TTL observations. An attribution review can use only the lowest
watermark that covers all of its dependencies. Commit derived rows and their
watermark atomically; unique keys make replay idempotent. Record gaps explicitly
with first/last affected ledger and dependent intervals. A timeout, truncated
page, decoder failure, missing metadata, or provider retention gap must not
advance the complete watermark.

RPC transaction/event history has a configured retention window; it is not an
indefinite archive. The current upstream configuration describes its default as
roughly seven days. Collect before that window and verify the actual provider's
retention instead of relying on that default. [Stellar RPC configuration](https://github.com/stellar/stellar-rpc/blob/main/cmd/stellar-rpc/internal/config/options.go)

On restart, replay from the last fully verified ledger and overlap the preceding
page to test deduplication. Compare an independent provider or archival source at
checkpoints. A conflicting closed-ledger hash is an incident: retain both source
responses, freeze attribution from the disputed range, and investigate canonical
history. Do not silently overwrite prior evidence or treat conflicting histories
as routine rollback. Rebuild a quarantined projection after resolution and prove
that no settlement was credited twice.

## 4. Attribution workflow and evidence standard

Automatically verified segments use authentic boundary deltas and historically
complete weights. Successful later claim seals the interval with actual reward
tokens received. It does not establish missing intermediate deltas. The current
segment settlement requires every segment in the interval to be resolved before
settlement: an unresolved segment can therefore keep that interval's otherwise
verified entitlements reserved too. This is a liveness limitation to display,
not permission to invent a delta or remove an entitlement.

For a missing boundary, the proposed governance workflow is:

1. Publish a content-addressed evidence bundle before proposal. It identifies the
   exact interval/segment and code versions, authenticated state at both relevant
   boundaries, all emission/range/tick changes, actual receipts, historical
   beneficiary weights including exited/custody owners, waiver scope, calculation
   code and input hashes, rounding/shortfall/excess treatment, and every unresolved
   assumption. Include a reproducible replay and complete allocation table.
2. Two independent reviewers reproduce the evidence and exact scaled conservation
   before the configured administrator proposes it. This reviewer rule is an
   operating requirement to implement; the contract currently enforces admin
   authorization and delay, not reviewer signatures or an evidence oracle.
3. Select the segment attribution route for interval-wide historical allocation:
   `propose_reward_segment(id,amount,evidence_digest)`. Publish the proposal
   transaction, digest, amount, timestamp, execute-after, and all evidence mirror
   locations. The on-chain evidence hash must commit to the exact canonical bundle
   bytes, including its manifest and calculation version.
4. Wait at least 86,400 seconds and recheck eligibility, digest, amount, sealed
   state, interval status, and unchanged claim backing before
   `attribute_reward_segment`. A replacement proposal resets the delay; there is
   no dedicated cancellation entrypoint. Document supersession publicly and do
   not execute a superseded proposal. Already attributed segment amounts cannot
   simply be edited later: stop and escalate a discovered error rather than
   applying another attribution as an informal correction.
5. Advance `prepare_reward_segment(id,limit)` in bounded batches (1–4 waiver
   records), and settle each historical beneficiary with
   `settle_reward_segment(holder,id,checkpoint)`. Preparation and settlement do
   not need to occur inside liquidation. Settling is permissionless; eventual
   `claim_rewards(holder)` requires that holder's authorization. Replays must
   return no additional credit.
6. Reconcile actual token backing against paid amounts, booked holder amounts and
   fractional credits, original and redistribution totals, reserved historical
   credit, unresolved quarantine, and explicit scaled rounding. Use the exact
   integer equations from the tests; do not round away a discrepancy. Publish
   both the result and remaining unsettled beneficiaries/intervals.

The implementation allocates a received shortfall proportionally against
resolved expected accrual; only verified attribution can identify excess. Unknown
excess remains quarantined. Waiver redistribution follows historical eligibility
and weight for the waived segment. A newcomer is not eligible merely because
they own shares when payment arrives, and an original eligible owner remains
eligible after their exit. Forced liquidation never adds a waiver.

The legacy `propose_reward_attribution` / `attribute_rewards` direct-beneficiary
route has its own evidence and timelock records, but no corresponding segment
proposal/execution events. Its interval `direct_governed` mode cannot be mixed
with segment settlement, and it is not a substitute for a missing multi-owner
allocation proof. The operator must explicitly select and document one route per
interval; prefer the historical segment route. No route makes governance
attribution equivalent to automatically verified accrual.

If authentic boundary evidence **never** becomes available, the unresolved amount
stays quarantined indefinitely. Age, inactivity, a successful later claim, a
governance vote without evidence, or a beneficiary's exit does not make it
distributable. There is no timeout to treasury, current holders, supplier NAV, or
liquidators. Historical records and exited beneficiaries stay indexed. Alerting
SLAs concern investigation, not forfeiture. Any eventual shutdown/migration must
preserve these obligations and raw evidence; it cannot prune them as a cleanup.

## 5. Archival, availability, and fee funding

Persistent history is recoverable after archival; temporary entries have different
semantics and are not a historical entitlement store. Restoration makes archived
persistent state live; a local backup is evidence, not authority to rewrite
on-chain contract state. [Stellar persistent-state archival specification](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-12.md)

Inventory every entitlement/history key and its TTL, plus the instance/code keys
and dependencies needed by recovery. In the implementation, history/reward reads
and writes renew persistent TTL at threshold 500,000 ledgers toward 1,000,000;
share storage has a different configured extension. Captured network settings
must determine effective minima/maxima: do not assume a requested extension
overrides the network maximum. Schedule by ledger distance and fresh settings,
not an assumed constant number of wall-clock days. Cold exited-owner records
still need inventory and restoration plans.

Separate hot-path renewal from cold-history restoration. A proposed maintainer
can simulate TTL extension/restoration batches using exact inventoried keys,
check limits and complete fee components, then publish unsigned plans. Submission
and funding would require a separately authorized operator; neither is performed
here. A missing raw lookup is not proof that a holder never existed. Distinguish
archived, unavailable, absent, and unknown; fail closed on uncertain entitlement
state and recover the correct archived entry.

The preserved local controlled-state tests contain 436 restored history variants
(733 restored variants across the full captured profile). Each compared its
return and effective contract changes with its unarchived baseline. They restored
the touched keys in each variant, not every historical record in one invocation.
They are not proof that a live archive provider or funded maintenance service
exists.

| Larger-history restored flow | Measured variants | Maximum total fee, stroops | Maximum added total vs paired unarchived baseline, stroops |
| --- | ---: | ---: | ---: |
| `emergency_redeem` | 8 | 5,166,963 | 995,154 |
| `checkpoint_rewards` | 16 | 2,720,745 | 942,186 |
| `prepare_reward_segment` | 26 | 2,136,336 | 2,132,565 |
| `settle_reward_segment` | 303 | 3,954,502 | 3,685,579 |
| `liquidate_funded_v3` | 8 | 9,209,408 | 3,589,307 |
| `propose_reward_segment` | 3 | 2,539,896 | 2,047,425 |
| `attribute_reward_segment` | 3 | 2,232,694 | 2,045,236 |

Rows are maxima across controlled-state variants; maxima in different columns
need not come from the same transaction. The remaining 69 variants are
`reward_boundary`, `reward_interval`, and `reward_segment` views, illustrating
that submitted historical getters can incur restoration/rent too. Larger-history
maxima were 128,416,150 instructions, 12,281,537 memory bytes, 149 unique
transaction footprint keys, 52 writes, 12 disk-read entries / 2,200 disk-read
bytes, and 14,012 serialized unsigned envelope bytes. Captured settings were
protocol 29, official host 29.0.1, ledger 65,264,920, configuration SHA-256
`a28fb64bb85c74f8f0d1d4bd7a0daf413ada6f603647c64f556a7905af554dcd`.

Across those larger-history rows the individual fee maxima were inclusion 100,
nonrefundable 140,485, refundable 5,049, rent 9,068,453, and total 9,209,408
stroops. **Rent is already included in total.** Do not sum independently observed
component maxima and call the sum a measured transaction. The whole captured
suite has a higher maximum total fee, 22,845,642 stroops; the historical-flow
table is not an overall service budget.

Budget maintenance from exact bounded workload counts and simulations:

`planned fee reserve = sum(simulated complete fee for each planned operation)
                     + separately approved contingency
                     + measured signed-envelope/authentication increment`.

For scale only, eight operations at the largest history-flow total would consume
73,675,264 stroops (7.3675264 XLM) in this unsigned controlled scenario, before
contingency or missing real-signature costs. This is not a proposed transfer,
production fee quote, or sufficient wallet balance. Measure dedicated renewal
batches, real workload frequency, live state, and signed overhead before choosing
a funding target. Retain each simulation's parameters, fee components, envelope,
key inventory, and check-budget result. Neither WASM size nor average fee suffices.

Keep maintenance fees in a separately accounted operational fund; never consume
unresolved reward backing, waive a borrower entitlement, or silently deduct it
from supplier cash/NAV. This fee fund is distinct from liquidator debt-repayment
assets and the market's cash reserve. Replenishment rules and alert thresholds are
to be approved after measurements; no wallet or funding transaction is created by
this plan.

The internal 100M/100-entry/50-write policy still rejects relevant measured flows;
the captured-network diagnostic does not override it. Every new operational flow
must run the existing `check_budget.sh` checks, with missing measurements failing.
Signed transaction/authentication costs remain unmeasured. These are release
blockers even where controlled unsigned restoration fits captured network limits.

Forced recovery must not depend on a keeper being online or on governance
attribution. A liquidator or repaying owner must be able to supply exact needed
restoration keys and pay network fees independently of the maintenance service.
This is not a promise of free recovery: unavailable required state, insufficient
transaction fee funding, or a resource-limit failure can still prevent execution.
Expose those failures honestly; do not introduce an attribution approval or a
global keeper-funded flag into the principal-recovery authorization path.

## 6. Operational stop rules and remaining implementation

| Condition | Required response | Recovery invariant |
| --- | --- | --- |
| Missing boundary or history gap | Freeze attribution of the affected interval; retain/quarantine its backing, preserve raw evidence, investigate. | Do not require attribution to release principal or perform permitted funded seizure. |
| Claim or pending-view outage | Normal ownership changes follow existing claim-first rejection. Forced path journals its actual unknown boundary; voluntary waiver requires explicit holder consent. | No forced waiver, no current-holder windfall, no keeper approval. |
| Pool code/configuration changes | Stop treating old verification assumptions as current; retain both code versions and revalidate evidence. | No synthetic attestation or production guard removal. |
| Corrupted index / disagreeing provider | Stop attribution and new operational risk-increasing preparation; rebuild from verified raw history with gap markers. | Index availability is not a new on-chain permission check for recovery. |
| Low TTL / fee fund / maintenance outage | Alert and publish exact unsigned restoration/renewal plans with costs; prioritize hot recovery keys, retain all cold keys. | No pruning or fee extraction from unresolved entitlements; caller-funded recovery remains available subject to actual execution prerequisites. |
| Bad conservation or duplicate credit | Stop attribution/settlement automation and preserve logs; do not invent offsetting credit. | Preserve the existing independent authorized principal routes and atomic rollback. |
| Failed resource or signed-transaction gate | Keep Stage 1 incomplete and borrowing disabled; report the individual failure. | No policy/stage change hidden in an operations workaround. |

Before production readiness, implement and test the collector/indexer and raw-key
inventory, two-copy evidence archive and rebuild, contiguous watermark/gap
handling, exact intratransaction replay, beneficiary lookup after exit, governance
bundle validation, and bounded unsigned restore/renewal planner. Rehearse a
provider retention gap, conflicting data, archive corruption, all maintainers
offline, multiple missing intervals, paused borrowing, exited original waiver
beneficiaries, newcomer exclusion, and repeated settlement. Demonstrate that
forced exits still preserve rewards without waiting for any attribution job.

Keep the existing 64-owner / 128-additional-change / eight-outage and eight-funded-
exit proofs, but do not treat those finite cases as unbounded-history coverage.
Define maximum supported per-operation workload from actual measurements; retain
all older history outside the immediate operation. Extend archival tests to cold
exited beneficiaries, proposal/settled replay markers, and the entire dependency
closure of each recovery route. Publish unsigned and eventual real-authorization
evidence separately. No part of this operating plan constitutes release approval.
