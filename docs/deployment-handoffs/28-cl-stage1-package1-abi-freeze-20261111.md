# Stage 1 package 1 — ABI/invariant freeze and economics checkpoint

11 November 2026. Approved plan: `3b962e371d74c32e7f6076296a58bbfcecaa7ff9`.
Accepted signed controlled-state baseline: `7f66fb056d0289fcfcd7526399608284521c79e3`.

**Package 1 is complete. Stage 1 remains incomplete; PR75 remains draft. Package 2
has not started.** This checkpoint implements the approved wire/invariant freeze,
not receipt/vault financial behavior. The one bounded economics subagent also completed
its report: simple full/wide Aquarius supply is below the active lending comparison.
This finding is reported before package 2 as requested.

## Freeze delivered

`peridot-contracts/interfaces/cl-stage1-interface/` is an SDK25.3.1 rlib, not a
contract/WASM. Production contracts do not depend on it yet. Cargo.lock only adds the
new local library package; no dependency version changes.

- 29 generated client methods across receipt and vault; facade has no new ABI or
  hot-path hop. Typed commands, phases, results, per-id liabilities, category totals,
  locks, fee allocation, binding, risk revision, waiver consent and event payloads.
- Canonical ScVal-XDR/SHA256 domains bind network, contract pair, command id, owner,
  immutable binding, every open/close term and requested close quantity. No mutable
  share field or JSON encoding substitutes for the owner's commitment.
- Exact caller/phase matrix, custom funding authorization arguments and nested
  contract/token calls; terminal replay rejection; strictly-after-deadline expiry.
- Set-once mutual binding and staged-mode upgrade freeze, including queued upgrades;
  config revision changes stop fresh risk without disabling in-kind release/recovery.
- B4 cash-first ownership retained: cash-backed mint precedes investment, shares are
  locked, post-mint cancellation releases shares and execution reserve, never refunds
  common backing twice. No claim that temporary participation has zero economic carry.
- Typed, boundary/ledger-scoped owner waiver; direct-holder emergency consent has a
  separate domain. Neither absent owners nor permissionless executors can waive.
- Ten namespaced new keys, fixed durability and constant-sized command/liability
  records. No migration or packing/pruning of beneficiary histories. Source-derived
  inventory checks 115 existing key variants across 11 receipt/vault/facade enums.
- Fifteen named invariants, including
  `INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN`, exact reward conservation,
  historical entitlement preservation, lock enforcement and category backing identity.
- Stable error IDs1700–1718, native type/key/spec wire vectors, exact fixture/runtime/
  resource-profile pins, and a Stage1 proof map with all nine rows and G1–G4.

The generated key audit compares representative native XDR encodings for every listed
variant against new namespaces, regardless of durability. This is not an audit of all
transitive library bytecode storage. Existing same-contract variant names were also
checked; no duplicates found. `freeze.py` pins whole source files as baseline evidence:
a later source implementation change requires explicit review of that pin, not an
automatic assertion that every body change is a wire-schema change.

`SEMANTICS.md` is the normative auth/state/storage/invariant companion. `src/lib.rs`
is the field/type/signature definition. Golden creation refuses overwrite; normal
verification compares existing artifacts. Intentional changes require a reviewed
freeze revision, not silent regeneration to make tests pass. Full implementation
must consume the shared types and preserve the snapshot meanings.

## Verification and gate status

|Check|Result and scope|
|---|---|
|Native library compilation|PASS; no deployable contract built|
|Source/API freeze|PASS:29 signatures,115 existing key variants|
|Native XDR/key/hash tests|PASS:5 tests; all committed binding/open/close fields mutation-tested|
|Baseline pins/proof-map integrity|PASS; all financial/new-build gates explicitly unrun|
|Economics offline replay|PASS:40 public response hashes,53 evidence members, outcome matrix|
|New financial/resource rows O1–O5,C1–C3|NOT IMPLEMENTED / NOT RUN; accepted baseline passes remain separate|
|G1 outage mixture recovery|NOT RUN; required family `compiled_stage1_outage_mixture_recovery`|
|G2 stressed histories|NOT RUN; required family `compiled_stage1_stressed_histories`|
|G3 interrupted/re-expired preparation|NOT RUN; required family `private_core_stage1_interrupted_restore_resume_reexpire`|
|G4 Core accounting parity|NOT RUN; required family `private_core_stage1_financial_parity`|
|Release|NOT APPROVED|

The first wire-test compile failed because SDK25 Bytes lacks `to_alloc_vec` without
its allocation feature. Replaced it with the supported iterator collection; preserved
that failed log and final passing log. No accounting or authorization test was weakened.
These five tests establish wire stability, not enforcement of financial/auth rules.
No new financial root exists to run through check_budget.sh yet. Every later implemented
root must be measured with it, including added liability/consent overhead. Existing
profiles and failing combined lifecycle/transaction gates remain unchanged.

This checkpoint adds no dedicated financial CI and claims no CI qualification. Accepted
signed-checkpoint CI42835303812 completed checkerPASS/checkpointFAIL; those baseline
jobs do not execute the signed prototype. Any push-triggered baseline CI for this
checkpoint is separate from the five local interface tests.

## Economics — below lending before package 2

Full report, arithmetic, exact public responses and hashes:
[`research/cl-stage1-economics-20261111/report.md`](../../peridot-contracts/research/cl-stage1-economics-20261111/report.md).
No compromised legacy Peridot market is used as a benchmark. External active Blend
Fixed v2.1 USDC supply APR is3.0666% at the observed ledger; no lending safety or
withdrawal-capacity qualification is inferred.

Observations around08:05–08:20UTC:2.5651983 AQUA/s,0.10% pool fee with50% LP share,
24h volume about$125.37. Current emission schedule ends11 November16:15:19UTC.
Pool-wide roughly8% marketing/API yield is not the return to a new full/wide entrant:
existing liquidity is much more narrowly concentrated. Actual unsigned venue liquidity
quotes plus active/boosted denominators produce these conditional results:

|Research capital|Range|Gross annual run rate|365-day net versus holding PYUSD, in kind|
|---|---|---:|---:|
|$1,000|Full[-887260,887260]|0.003809%|-$0.40449|
|$1,000|Wide[-2000,2000]|0.040041%|-$0.04217|
|$10,000|Full[-887260,887260]|0.003809%|-$3.40405|
|$10,000|Wide[-2000,2000]|0.040034%|+$0.21843|

Annual figures assume repeated emission renewal, unchanged prices/competition, no
strategy price-path loss and zero additional recurring OPEX. They deduct quoted entry
cost and accepted continuous warm in-kind lifecycle cost0.5744714XLM, about$0.08314
at the observed XLM mark. Optional converted lifecycle and30-day results are in the
full report. These historical unsigned continuous fee measurements are not represented
as a current signed/live fee quote. Current PYUSD mark is applied consistently to the
vault and hold benchmark; at alternative$1/$1 parity all four annual outcomes are
negative. Undistributed/quarantined reward value0 also makes all four negative.
The tiny best-case surplus is not profitability or capacity evidence.

**Shared00§2 form (without editing00):** This strategy has not established positive
profitable capacity. At research sizes$1,000/$10,000, simple full/wide1× configurations
model approximately-0.04045% to+0.00218% annual net versus holding PYUSD under the
conditional current-mark scenario above. Stress execution, collected/distributable
rewards and ongoing operating costs remain unproven; no safe cap follows. Both gross
run rates are already below the active lending supply APR. A narrower strategy or
new economics study is not started by this checkpoint.

Current Aquarius pool at ledger65419687 has WASM
`155a17b9929ffb1f9e84bd6ef5c00a4d613c1ab5f4ad4c502d84515250cc2907`, unlike the
accepted fixture `12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`.
This is a concrete compatibility/qualification dependency. Preserve the accepted
fixture; do not substitute the new live code or transfer old CPU/reward claims to it.
No new live-code execution study was run.

## Next milestone and reproducibility

Package2's receipt implementation may consume this freeze under the approved plan;
the economic underperformance is now visible before it starts. Package3 closes G1,
followed by targeted G2 and persisted Core G3/G4 per the approved ordering. Report
each gate with measured changed roots, exact failures and conservation assertions.
The new per-id liability and scoped waiver costs are unmeasured until implementation.
No earlier claim substitutes for any of the three required ownership boundaries.

From `peridot-contracts`:

```sh
python3 interfaces/cl-stage1-interface/freeze.py
python3 interfaces/cl-stage1-interface/verify.py
CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 cargo test --locked -p cl-stage1-interface
python3 research/cl-stage1-economics-20261111/calculate.py
python3 research/cl-stage1-economics-20261111/verify.py
python3 research/cl-stage1-abi-freeze-20261111/verify.py
```

No signing occurred in package1. No production contract/limit/cap/maintenance-margin
change, borrowing activation, deployment, Upshift binding or handoff00 edit. Aquarius
remains the venue; Option1 remains closed. Previous checkpoints, transaction artifacts,
negative split and unrelated local changes are preserved. Fable's recorded prior
non-reapproval is not blocking per the operator; no new Fable review is claimed here.
