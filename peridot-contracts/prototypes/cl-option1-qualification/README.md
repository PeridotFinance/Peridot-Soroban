# Atomic Option1 qualification research

Stage 1 is incomplete; PR75 remains draft. Option1 is an **unadopted proposal**.
Neither resource profile, production code, cap, maintenance margin, borrowing
setting nor handoff00 is changed. All patches apply only to fresh isolated build
trees. The new recovery ABI requires the validation feature and rejects Mainnet.
No signing, broadcasting, deployment, secret access or Upshift binding occurs.

The [published checkpoint](../../../docs/deployment-handoffs/22-cl-option1-qualification-20261104.md)
and [durable evidence](../../../docs/deployment-handoffs/evidence/cl-option1-qualification-20261104/README.md)
retain the exact compiled identities, failed attempts and per-profile gates.

The experiment combines the preserved same-transaction TTL/storage optimization,
full-repayment correction and new independent funded recovery routes. Every
repayment/seizure remains atomic. Claims at mint, transfer and custody ownership
boundaries remain; history and quarantine are never pruned to obtain a fit.

## Reproduce the joined candidate

From `peridot-contracts`, with the repository's pinned native Rust/Node/venue
dependencies installed, use a **fresh** output path:

```sh
python3 prototypes/cl-option1-qualification/ci.py \
  --output target/cl-option1-reproduce
```

This creates the exact six-overlay candidate using `stellar contract build`,
records all compiled identities and generated test sources, and executes seven
joined tests plus the separate mixed-funding companion under each unchanged
profile. It invokes `check_budget.sh` and
retains its exact per-profile gates/errors. Missing global proofs keep the
command nonzero; do not swallow that status or treat it as CI approval. Native
test compilation uses Cargo; WASM compilation uses only the Stellar CLI.

For already identified build trees, run the two profiles separately:

```sh
python3 prototypes/cl-option1-qualification/run-closing.py \
  --profile captured-network-diagnostic \
  --build target/cl-option1-joined-build-v3 \
  --run target/cl-option1-joined-replay-captured
python3 prototypes/cl-option1-qualification/run-closing.py \
  --profile internal-policy \
  --build target/cl-option1-joined-build-v3 \
  --run target/cl-option1-joined-replay-internal
```

Run each command individually; each checkpoint result can fail despite completed
functional tests. `run-closing.py` pins the test, patch, manifest and runner bytes
through execution and requires all seven actual outcomes. Baseline, negative
split, initial pending-only and rejected earlier closing variants remain separate.
The corrected V3 is100,419 bytes, SHA256
`ad9fbb7669ff126ff54e8d9b021d610b9313cb5f3dc595df6e9d7dabc3d7d482`.
Local Mac bytes and Linux CI bytes must be independently identified.

## Evidence and interpretation

- [Lifecycle map](lifecycle-proof-map.md): actual entrypoints, six Long stalled
  custody shapes, deadlines, exact proof tests and outstanding branches.
- [Qualification plan](qualification-plan.md): new-risk bounds, recovery
  obligations and exact future supported-account signature experiment. No
  signing is authorized now.
- [Current decision table](decision-joined.md): internal policy, proposed boundaries and
  captured network ceilings remain distinct. SDK entries are not unique keys.
  `decision-table.md` preserves the earlier9bb51aa comparison population.
- [Shared recovery](shared-results.md) and [operating plan](operating-plan.md):
  one joint eight-borrower state, shared code versus per-position rent, warm and
  cold costs, distinct operating funds/reserves/liquidator capital.
- [Mixed funding](mixed-results.md): both profiles pass the exact own-proceeds
  plus external-deficit and previously reserved-fee regression on unchanged
  contract bytes. Its full checkpoint remains failed.
- [Core restoration research](core-research.md): normal unsigned preauthorized
  local application scope and limitations. Host simulations alone do not prove
  Core source balances, sequences, fees or installed archive TTLs.

Summarize completed joined runs without changing a profile:

```sh
python3 prototypes/cl-option1-qualification/summarize-joined.py \
  --captured target/cl-option1-joined-final-captured-v3 \
  --internal target/cl-option1-joined-final-internal-v3 \
  --output target/cl-option1-joined-assessment-replay.json
```

The latest archived assessment is `cl-option1-joined-assessment-v3.json`.
It binds the actual profile names/configuration and compiled manifest identities;
`decision-joined.py` refuses missing roots, widened proposal inputs or an observed
workload above the proposal. Six reporting regressions test these refusal paths.
The earlier assessment versions and original typo-bearing gate reports remain
preserved; reviewed maps change only `closed_position` to the actually executed
`closed_repay` rejection label. They do not rerun or manufacture a proof.

The companion shared test uses `prepare-shared.py`, `run-shared.sh` and
`shared-verify.py`; exact commands and results are in `shared-results.md`.
The native preparation/re-expiry companion uses `prepare-pending-restoration.py`,
`run-pending-restoration.sh` and `check-pending-reexpiry.py`. Its forced TTL reset
does **not** advance time or prove actual Core re-expiry. The older fixture and
current combined candidate are never silently relabelled as identical evidence.

Completed9bb51aa Linux baseline/storage/restoration CI is preserved separately
under `docs/deployment-handoffs/evidence/cl-ci-9bb51aa-20261103/` at repository
root. Its38 exact gate reports were replayed, including errors and completed
outcomes, rather than accepting aggregate exit1. That CI does not execute this
new joined prototype. The dedicated Option1 workflow retains its own failed
gates and artifacts, including the mixed companion; it performs no Core or
signature qualification. CI's explicit platform rebind pins only the generated
test's expected hash to its freshly built six-overlay WASM, records both input
and generated source hashes, and does not relabel Mac evidence as Linux evidence.

`package.py` accepts only an explicitly reviewed research file list. `verify.py`
checks every archive member and reports exact gate records, test outcomes and
separate WASM hashes; hash verification is distinct from full checker replay.
Preserve original failed attempts and source bindings when creating new evidence.

Fresh primary prices and explicit liquidation permissions remain prerequisites
for seizure. CL Short is prohibited by existing route guards. Zero-collateral
full repayment requires an actual loss-cover payer, not an assumption of a
profitable liquidator. No resource fit establishes live valuation, payer
availability, a production safe cap, affordable income or release approval.
