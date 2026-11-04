# Atomic Option1 qualification checkpoint — 2026-11-04

**Continue bounded Option1 qualification; do not adopt it or enable borrowing.**
The joined candidate fits the proposed workload boundaries for the measured
roots. Economic viability, supported account authorization, complete Core
financial application and live operating bounds remain unproved. Stage 1 stays
incomplete and PR #75 stays draft. Stop incremental wrapper/TTL tuning toward
100M: the current venue's measured work already exceeds that boundary.

This checkpoint changes isolated research only. Production source, financial
stages, both resource profiles, caps, maintenance margin and shared handoff00
remain unchanged. Borrowing preparation remains disabled. No signing,
broadcasting, deployment, real wallet/secret access or Upshift work is included.

## Identity, results and failed gates

The exact six-overlay candidate combines atomic TTL, operation-specific storage,
full repayment, independent expired/unhealthy pending recovery and stalled
settlement. Final local V3 is **100,419 bytes**, SHA256
`ad9fbb7669ff126ff54e8d9b021d610b9313cb5f3dc595df6e9d7dabc3d7d482`,
from `target/cl-option1-joined-build-v3`. Test-source and build manifests are pinned.
Earlier pending-only builds and the unsafe live-close fee-bypass diagnostic
remain separate. The corrected implementation rejects live converted zero-top-up
settlement before any proceeds application or contingent-fee waiver.

| Exact seven-case run | Compiled tests | Invocation | Lifecycle / transaction / checkpoint | Reviewed errors: lifecycle / transaction | Missing required flows |
|---|---|---|---|---:|---:|
| Captured network | 7 PASS | PASS | FAIL / FAIL / FAIL | 2 / 46 | 46 |
| Internal policy | 2 PASS, 5 FAIL | FAIL | FAIL / FAIL / FAIL | 34 / 58 | 46 |

These are the reviewed reports in `target/cl-option1-joined-assessment-v3.json`;
original execution gates remain preserved separately. Missing global mappings
stay failed even where a selected new test exercises a related transition.
Lower internal values from rejected or unreached calls are not completed-flow
savings. Aggregate exit1 alone does not establish exact test/gate outcomes.

## Measured financial roots

The table takes per-dimension maxima across completed captured SDK/official-host
root observations. SDK resource entries and unique serialized host keys are
different quantities; maxima need not belong to one invocation. These are
unsigned controlled-state roots, not every route after complete cold restoration.

| Root | Instructions | Memory bytes | SDK entries | Unique keys | Writes |
|---|---:|---:|---:|---:|---:|
| Swap-open | 227,685,319 | 22,793,790 | 172 | 126 | 48 |
| Activation | 63,256,252 | 11,848,088 | 157 | 119 | 40 |
| Prepare-close | 45,068,223 | 10,695,350 | 111 | 81 | 31 |
| Swap-close | 189,438,037 | 19,188,362 | 164 | 123 | 42 |
| Finish-close | 38,655,115 | 7,220,826 | 96 | 66 | 30 |
| Active funded liquidation | 67,249,744 | 12,784,837 | 170 | 121 | 50 |
| Full repayment | 15,504,090 | 4,356,551 | 74 | 63 | 12 |
| Debt-free release | 14,411,567 | 4,100,539 | 86 | 65 | 22 |
| Expired PendingOpen | 70,469,573 | 13,240,348 | 152 | 103 | 50 |
| Unhealthy PendingOpen before expiry | 69,208,840 | 13,077,840 | 152 | 103 | 50 |
| Six Long stalled settlement shapes | 74,180,636 | 14,802,394 | 176 | 125 | 52 |

Proposed workload boundaries remain **240M instructions / 26,843,545 memory bytes /
240 SDK entries / 240 unique keys / 80 writes**, inside proposed outer ceilings
300M / 32 MiB / 300 / 300 / 100. The observed maxima leave 12,314,681 instructions
(about 5.13% of the workload boundary), 4,049,755 memory bytes, 64 SDK entries,
114 unique keys and 28 writes. Unmeasured supported-auth and stress costs must fit
there; the outer 20% reserve is not available workload. No acceptance profile is adopted.

The unchanged internal100M/100SDK/100unique/50write policy still fails. Preserved
Aquarius-exclusive open162.817M and close137.760M, and fee collection plus position
deposit118.186M/withdrawal115.816M, require a venue/accounting ABI redesign if100M
is mandatory. Supplier deposit/redeem remain separate preserved populations
(166,914,792 /139,966,854 instructions), not additional joined test successes.
See [decision-joined.md](../../peridot-contracts/prototypes/cl-option1-qualification/decision-joined.md); no assumed savings or extra financial split is credited.

## What the seven proofs establish

The [machine map](../../peridot-contracts/prototypes/cl-option1-qualification/joined-proof-map.json) binds exact names; the
[lifecycle map](../../peridot-contracts/prototypes/cl-option1-qualification/lifecycle-proof-map.md) maps every public financial entrypoint,
alias, custody state, deadline and recovery condition, distinguishing baseline
evidence from new joined execution and untested branches.

| Joined proof | Concrete scope |
|---|---|
| J1 expired pending | Distinct payer, strict expiry, actual intervening trade, borrowing disabled, full debt/max/refund, bounded zero-bonus underlying seizure, both reward outages, auth/bounds/replay rollback. |
| J2 early unhealthy pending | Existing maintenance test permits unhealthy pending recovery before expiry; healthy/cash/max/auth rejections and historical entitlement preservation. |
| J3 full repayment/release | Aged debt cleared exactly, excess refunded, zero-debt repeat, backed free-margin release, beneficiary claims and receipt exit. |
| J4 repayment negatives | Seven intended rejection conditions, complete ledger/TTL rollback, followed by repayment/release. |
| J5 active funded liquidation | No borrower authorization; actual preceding Aquarius trade; current full repayment and permitted receipt seizure preserve history. |
| J6 stressed open/close | Real claim-first mint/transfer/custody boundaries, 64 owners,128 ownership changes,eight unresolved intervals and staged close. Finite workload only. |
| J7 six Long stalled shapes | Withdrawn/converted Closing; receipt/converted Liquidated; actual committed partial-finish residual; inherited underlying with distinct successor after exclusivity. |

J7 retains typed minimum rollback and nonzero contingent-fee live-close rejection.
Recorded position proceeds pay debt before any external contribution; seizure is
bounded by that contribution and debt must be zero before delivery. The residual
case actually pays **24,450,210 raw** with **zero seizure**: voluntary permanent
loss cover, not profitable liquidation. Its artificial30-year timestamp jump
with few ledger ticks is not realistic latency or elapsed archival. CL Short is
prohibited by the existing cash-funded-Long guard and remains unqualified.

Exact reward conservation uses `received*S = (paid + pending + quarantine)*S +
fractions + rounding`, with `S = 10^18`; forced settlement adds no waiver:

| Completed example | Received | Paid | Pending | Quarantine | Fractions | Rounding |
|---|---:|---:|---:|---:|---:|---:|
| Six Long stalled shapes | 7,520,813 | 0 | 7,192,235 | 328,567 | 10,999,999,999,573,601,590 | 426,398,410 |
| Aged repayment/release beneficiaries | 3,906,060 | 20,764 | 79,868 | 3,805,389 | 38,999,999,999,973,007,512 | 26,992,488 |
| Shared eight-borrower sequence | 100,671 | 0 | 100,632 | 0 | 38,999,999,999,973,007,512 | 26,992,488 |

Shared physical quarantine zero does not establish zero unobserved reward debt;
that continuous-outage scenario does not deliver the later reward receipt.

## Operating cost and funding remain conditional

The separate shared experiment carries one joint prestate through eight actual
sequential pending recoveries:214 original keys,54 common to all borrowers and160
specific to one, versus592 summed memberships. Captured completes8/8; internal
applies0/8 after first-root rejection. Full gates still fail. This is not eight
independent cold probes merged or summed after the fact.

Eight restore batches project **606.2864327 XLM**, followed by eight financial
roots **7.2397101 XLM**, total **613.5261428 XLM**. Gross includes60.6771448 XLM of
synthetic-authorizer rent; removing it is not a real-account fee credit. Warm
discovery totals6.9356427 XLM and is separate from post-restoration execution.
An analysis-only25% fee contingency gives766.9076785 XLM, excluding account reserve,
signatures, retries, re-expiry and liquidator principal. No wallet is presumed funded.

The proposed proactive30-day code-rent component is134.4263898 XLM at an assumed
five-second ledger cadence; complete maintenance of that seven-code set and instance/data
costs remain unmeasured. A separate actual Core code extension is quantified below. Compare actual collected income with measured workload,
maintenance, failures and keeper compensation before claiming affordability.
Keep supplier withdrawal cash, loss reserve, XLM operating funds and debt-token
working capital separate. Future income/unclaimed rewards/quarantine give no
day-one backing; zero-bonus recovery creates no funded willing liquidator.
See [operating-plan.md](../../peridot-contracts/prototypes/cl-option1-qualification/operating-plan.md) and [shared-results.md](../../peridot-contracts/prototypes/cl-option1-qualification/shared-results.md).

## Actual Core application, companion proof and remaining gates

Actual local Core29 now passes **2,581 assertions** across four modes:12 native
operations (10 Restore,2 Extend) and four normal wrong-preauthorization rejections.
Runtime SHA256 `dcbf2fb09e28d777d819b4c2785d7f1758c7e6c8c54649f89bec66df78ca0a47`.
Actual receipt code and a23-key code/instance/data footprint restore from both
expired-live and hot-archive fixtures. Installed TTL is2,073,879; complete entry
data, source balance/sequence, fee-pool credit, consumed signers and replay
rejection are checked through ordinary transaction validation and LedgerManager
application. Code-only fee is1,026,250,252stroops;23-key fee1,031,215,394stroops.

The actual code Restore→already-live retry→Extend sequence charges
1,026,250,252 +60,092 +513,117,036 =**1,539,427,380stroops**, ending at sequence8
and TTL3,110,681. Retry adds no rent. A separate seeded mixed-live/archived case
charges1,026,300,772stroops and preserves22 already-live entries; it is **not** an
interrupted multi-transaction journey. Neither chain is a complete financial
Core recovery or a normal lifecycle cost to add to the shared projection.

There are zero envelope signatures and no signing/broadcasting. Exact `preAuthTx`
permissions are seeded at controlled genesis; ordinary wrong-hash authorization
returns `txBAD_AUTH` without application/debit. The diagnostic fixture omits
network-overlay construction through its test seam because stock PeerAuth would
sign an unused certificate; a mandatory signing guard blocked the initial setup
attempt. Transaction authorization, fees, archive application and invariants
remain stock. Zero SDK last-modified metadata is explicitly imported as genesis
ledger279 while preserving all LedgerEntryData. Raw inputs and failed attempts
remain intact. Official Core29 source/hosts, diagnostic native compilation and
its native-performance limitation are pinned separately; this is not live-state
or real-signature evidence.

The separate **mixed-funding companion passes1/1 under each profile** on the same
V3 bytes. One root applies49,962,213 own proceeds plus33,966,075 external funding,
refunds5,000,000 and clears83,928,288 debt. Insufficient maximum fully rolls back;
500,000 previously reserved fee shares/backing survive and subsequently pay their
beneficiary exactly once. SDK46,980,924 CPU /9,091,183 memory /97 entries /28writes;
unique host keys70, unsigned fee165,610stroops. This is voluntary loss cover with
no collateral, using artificial elapsed-interest stress. Its invocation gates
pass but full gates fail:46 missing flows,2 lifecycle and46 transaction errors
under both profiles. See [the exact companion evidence](../../peridot-contracts/prototypes/cl-option1-qualification/mixed-results.md) for scope and identities.

- Supported real account signatures, signed envelopes/nonces, full financial
  Core lifecycles, live archive discovery, tick/registry/history capacity,
  actual elapsed re-expiry, complete pause/delivery/reentry branches and funding
  remain unqualified. Synthetic TTL reset is conditional restoration evidence,
  not elapsed Core expiry. Stale primary prices and liquidation pauses remain
  blockers, including fresh-price dependency for zero-collateral top-up.
- The companion closes reserved-fee and mixed-funding gaps for its converted Long
  Closing path; other mixed asset shapes and transfer/reentry failures remain.
  Healthy-timeout zero-bonus settlement and contingent-fee waiver remain unadopted policy.

## Completed earlier CI and reproducibility

The [9bb51aa archive](evidence/cl-ci-9bb51aa-20261103/README.md)
preserves829 original files,90 WASM files/20 identities and all38 exactly replayed
gate reports. Baseline41626834491: checker PASS/checkpoint FAIL; captured four
gates PASS, internal four FAIL with106 lifecycle/303 transaction errors. Storage
41626834469: both jobs FAIL. Restoration41626834559: six compiled tests PASS,
checkpoint FAIL. These Linux runs precede and do not execute the new joined recovery ABI.
Payload SHA256 is `96d9fb8e5d51a55f3a63698a3cccb264006749854b920fb3f3370eb65f94fc50`;
Linux and local WASM identities remain separate. New prototype CI is not inferred.

[README](../../peridot-contracts/prototypes/cl-option1-qualification/README.md) provides fresh-directory reproduction and both unchanged
profiles; [qualification-plan.md](../../peridot-contracts/prototypes/cl-option1-qualification/qualification-plan.md) defines missing evidence
levels. Preserve failed attempts and exact gate/error arrays. The evidence gives no release
approval: technical headroom justifies bounded qualification,
while economic viability and every required release gate remain open.


## Durable evidence and exact reproduction

The [new local evidence archive](evidence/cl-option1-qualification-20261104/README.md)
is separate from every earlier Linux and local checkpoint. It includes actual
unsigned transaction artifacts, per-profile errors, generated test/source/WASM
identities, shared state lineage, native Core metadata and failed attempts. The
native executable/compiler caches are excluded; its exact runtime hash, source,
flags and build commands are retained. Six reporting refusal tests pass, covering
profile swapping, omitted roots, widened proposed boundaries and over-bound
recommendations; seven existing operating-cost arithmetic tests pass.

From `peridot-contracts`, use fresh output paths:

```sh
python3 prototypes/cl-option1-qualification/ci.py --output target/cl-option1-reproduce
python3 prototypes/cl-option1-qualification/summarize-joined.py --captured target/cl-option1-joined-final-captured-v3 --internal target/cl-option1-joined-final-internal-v3 --output target/cl-option1-assessment-replay.json
python3 prototypes/cl-option1-qualification/decision-joined.py --assessment target/cl-option1-assessment-replay.json
python3 prototypes/cl-option1-qualification/core/verify-staged-inputs.py target/cl-option1-core
```

Run commands individually. The first deliberately retains failed full checkpoint
status; inspect each profile's actual gates and completed test outcomes. It builds
contracts only with `stellar contract build`, then runs seven joined cases and
one separate mixed companion per profile. Native Core is a separate path:

```sh
bash target/cl-option1-core/run-restore-test.sh code
bash target/cl-option1-core/run-restore-test.sh state
bash target/cl-option1-core/run-restore-test.sh chain
bash target/cl-option1-core/run-restore-test.sh partial
```

These commands require the pinned local native build/staged inputs described in
[the Core instructions](../../peridot-contracts/prototypes/cl-option1-qualification/core/README.md).
They create no signatures or broadcasts. The dedicated new Linux prototype job
runs the financial tests with independently bound platform WASM identities; it
does not run Core, real account signatures or live-state validation. Its status
must be reported separately from completed9bb51aa CI and local results.

The blocker removed here is absence of actual native Core restoration/fee/TTL
application evidence, plus the tested independent pending/stalled and mixed-funding
recovery gaps. Complete signed financial Core application, all real restoration
keys, interrupted/elapsed-re-expiry journeys, stale-price liquidation, bounded
live workloads and funded economics remain blockers. A separately authorized
local synthetic-signing experiment for source=payer and distinct source/actor is
specified in the qualification plan; no signing is performed or authorized now.
