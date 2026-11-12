# Storage and repayment qualification evidence — 2026-11-03

Stage 1 remains **incomplete** and PR #75 remains draft. This is local,
controlled-state, unsigned prototype evidence, including failed experiments.
It does not approve a policy, deployment, contract upgrade or production release.

- Payload: `checkpoint-payload.tar.xz`, 11,389,220 bytes.
- SHA256: `e25df882bfb0e9c1b9e4f7556980431258fdf54dcdfb0ad8def0319a5159183a`.
- Inventory: 795 exact members, 934,262,945 uncompressed bytes.
- Format: sorted deterministic tar, uid/gid/mtime zero, mode 0644, XZ preset 3.
- Every member's name, size and SHA256 was independently checked after packing;
  each original source was checked again for mutation.

`index.json` contains exact member hashes, all 24 run configurations, named test
outcomes and Cargo totals, complete per-run gates/errors, recorded source pins,
and eight build manifests' independently checked WASM identities. The pinned
Aquarius bytecode is included separately as an exact dependency. Its SHA256 is
`12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`.

Run test counts must not be summed: live/data views copy their source run's
original logs, and liquidation views select transaction rows from a four-test
source. The index preserves the filter and extraction provenance. Missing
completion remains missing; the initial stale repayment run records zero tests,
and the next two compilation failures have no successful Cargo test result.

The baseline data/code runs and initial candidate each complete four fixture
tests; focused candidate roots, pending and exit retries each complete one.
All checkpoint gates remain false. Final live/data projections retain 46 missing
transaction-proof errors; cold code/instance evidence also retains genuine host
and resource failures. A fixture's passing assertions are not transaction-limit
or full Stage 1 acceptance. All four final repayment runs complete one test
each while their incomplete checkpoint gates remain false. Historical failures,
old exporter versions, the native seven-test source and final eleven-test local
compatibility logs are retained. See `target/cl-storage-checkpoint-report.md`,
`target/cl-storage-operating-costs.md`, the assessment, four differential reports,
and per-key CSV inside the payload for detailed measured findings and limits.

The payload includes exactly 147 hash-validated baseline files, generated
harnesses and WASM artifacts, new prototype/workflow source, analysis, independent
audit logs, and scoped native/debug histories. It excludes compiler caches,
whole build workspaces, prior archives, unrelated target artifacts and secrets.
The prior Linux CI artifact is preserved separately in
`../cl-linux-ci-1198078-20261103/`; that baseline CI did not run this prototype.

Verify without extracting or executing contracts:

```sh
python3 docs/deployment-handoffs/evidence/cl-storage-qualification-20261103/verify.py
```

This verifies preservation, not a fresh execution or full gate replay. No
signatures, broadcasts, Mainnet transactions, Upshift bindings, production
maintenance-margin changes or borrowing enablement were performed. Existing
resource profiles and production transaction stages remain unchanged.
