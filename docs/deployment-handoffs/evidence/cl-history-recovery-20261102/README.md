# Evidence package

Controlled-state protocol29 evidence; no production approval. Original checkpoints and captures are retained in their earlier directories. This package combines the completed main compiled suite with explicit supplemental executions; it never substitutes a resource row for a completed test assertion.

- `*-resources.jsonl`, `*-calls.jsonl`, CSV tables: SDK invocation accounting and nested call profiles.
- `*-transactions.jsonl.xz`: exact JSONL bytes compressed with standard LZMA; complete unsigned envelopes, authorization trees, entry/code bytes, rent inputs and fees. No signatures or secrets. The checker supports plain JSONL, gzip and xz.
- `*-proof-manifest.json`: source/map/log/report hash bindings and actual process exit codes.
- `*-summary.json` and `logs/*-budget.log`: independent invocation/lifecycle/transaction/checkpoint gates. Captured-network passes the controlled-state checks; conservative policy fails. Neither grants release approval.
- `*-governed/*.json`: published controlled attribution evidence; deliberately distinct from automatic boundary observations.
- `artifacts.json`: compiled WASM hashes and sizes. Actual bytecode is also present in serialized transaction entry evidence. Sizes alone are not a resource gate.
- `v3-state-machine.md`: all public V3 routes mapped by source review; the concrete proof map defines the tested subset.

The main captured suite completed23 tests, then archival reruns completed3, the added unresolved-interval test completed1 and the supplemental allowance transaction test completed1. This is24 distinct compiled tests, with completed repeat executions. Internal policy completed2 and failed21 main tests, failed3 archival reruns, and failed the added unresolved test at resource limits. The prototype host25 suite is separately9 PASS under the captured ceilings and5 PASS/4 FAIL under internal ceilings; it cannot satisfy protocol29 gates. Native vault53 PASS; checker13 PASS.

Saved manifests retain the original runner's absolute paths as provenance. Downloading these artifacts does not make those paths portable. Reproduce fresh execution with the command in the main handoff; it regenerates validated profiles and hash-bound proof manifests for the checkout. To replay only the saved evidence, use `scripts/replay_cl_evidence.py`: it first checks original source/map/log/report hashes, then writes relocated manifests in a NEW directory and invokes the unchanged checker. It does not execute contracts, modify source evidence, authorize release, or reinterpret failed tests as passed.

```sh
python3 scripts/replay_cl_evidence.py \
  ../docs/deployment-handoffs/evidence/cl-history-recovery-20261102 \
  --output target/cl-history-evidence-replay
```

Expected aggregate exit1 because the conservative policy remains failed. The new output directory must not exist.

The initial local wrapper was edited while active when redundant four-pass checker decoding was reduced to one pass. Its saved `logs/initial-wrapper-failure.log` records a shell read-position parsing failure after the main/archival suites. It is not claimed as a completed final runner. Final source passes `bash -n`; a frozen full runner is rerun separately. Published aggregate test logs and independent checker outputs completed successfully within the captured profile. The supplemental native funded command separately completed9 tests.
